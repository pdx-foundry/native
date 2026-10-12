use std::hash::{DefaultHasher, Hash, Hasher};

use super::Symbol;

/// Lookups by address and by name over one symbol list. Build it once per list: it holds
/// positions in that list, so every lookup takes the same list, unchanged.
#[derive(Debug, Default)]
pub struct SymbolIndex {
    /// Each symbol's position, in address order and then list order.
    by_address: Vec<(u64, usize)>,
    /// Each symbol's position, in name-hash order and then list order. A hash sorts much faster
    /// than the long demangled names; a lookup compares the names to drop collisions.
    by_name: Vec<(u64, usize)>,
}

impl SymbolIndex {
    /// The index of `symbols`.
    pub fn new(symbols: &[Symbol]) -> Self {
        let mut by_address: Vec<_> = symbols
            .iter()
            .enumerate()
            .map(|(position, symbol)| (symbol.address, position))
            .collect();
        let mut by_name: Vec<_> = symbols
            .iter()
            .enumerate()
            .map(|(position, symbol)| (name_hash(&symbol.name), position))
            .collect();

        by_address.sort_unstable();
        by_name.sort_unstable();

        Self {
            by_address,
            by_name,
        }
    }

    /// The one name of the symbols at `address`. `None` when no symbol is there or their names
    /// disagree.
    pub fn name<'a>(&self, symbols: &'a [Symbol], address: u64) -> Option<&'a str> {
        let mut names = positions(&self.by_address, symbols, address)
            .map(|position| symbols[position].name.as_str());
        let name = names.next()?;

        names.all(|other| other == name).then_some(name)
    }

    /// The symbols named `name`, in list order.
    pub fn named<'a>(
        &self,
        symbols: &'a [Symbol],
        name: &str,
    ) -> impl Iterator<Item = &'a Symbol> + Clone {
        positions(&self.by_name, symbols, name_hash(name))
            .map(|position| &symbols[position])
            .filter(move |symbol| symbol.name == name)
    }
}

/// The positions under `key` in `index`, which was built from `symbols`.
fn positions<'a>(
    index: &'a [(u64, usize)],
    symbols: &[Symbol],
    key: u64,
) -> impl Iterator<Item = usize> + Clone + 'a {
    debug_assert_eq!(index.len(), symbols.len(), "an index of another list");
    let start = index.partition_point(|&(entry, _)| entry < key);

    index[start..]
        .iter()
        .take_while(move |&&(entry, _)| entry == key)
        .map(|&(_, position)| position)
}

fn name_hash(name: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);

    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups_keep_list_order_and_refuse_disagreeing_names() {
        let symbol = |name: &str, address| Symbol {
            name: name.into(),
            address,
        };
        let symbols = [
            symbol("late", 0x30),
            symbol("alias", 0x10),
            symbol("constructor", 0x20),
            symbol("other", 0x10),
            symbol("constructor", 0x20),
            symbol("late", 0x08),
        ];
        let index = SymbolIndex::new(&symbols);
        let addresses = |name| {
            index
                .named(&symbols, name)
                .map(|symbol| symbol.address)
                .collect::<Vec<_>>()
        };

        assert_eq!(index.name(&symbols, 0x20), Some("constructor"));
        assert_eq!(index.name(&symbols, 0x10), None, "disagreeing names");
        assert_eq!(index.name(&symbols, 0x40), None, "no symbol");
        assert_eq!(addresses("late"), [0x30, 0x08]);
        assert_eq!(addresses("constructor"), [0x20, 0x20]);
        assert!(addresses("missing").is_empty());
    }
}
