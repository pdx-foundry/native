//! The lookups over an executable's symbols that each registry's readers repeat, so that a lookup
//! does not scan every symbol. The catalog keeps its symbols in address order; the index adds
//! their name order once.
use std::collections::BTreeSet;
use std::ops::Range;

use crate::engine::analysis::{discovery::Symbol, evaluate::ReadOnlyData};

use super::families::{VtableGroup, vtable_group_between};
use super::receivers::constructor_class;

/// The name order of an executable's address-ordered symbols, and its constructor entries.
pub(in crate::binding) struct SymbolIndex {
    /// Positions in the symbol list, by name. Equal names keep the list order, so a lookup gives
    /// its symbols in list order, as a scan would.
    by_name: Vec<usize>,
    /// The address of every symbol that [`constructor_class`] reads as a constructor.
    constructors: BTreeSet<u64>,
}

impl SymbolIndex {
    /// Index `symbols`, which are in address order, as the catalog keeps them. Look them up
    /// through [`SymbolIndex::over`] with the same list.
    pub fn new(symbols: &[Symbol]) -> Self {
        let mut by_name: Vec<usize> = (0..symbols.len()).collect();

        by_name.sort_by(|&left, &right| symbols[left].name.cmp(&symbols[right].name));
        Self {
            by_name,
            constructors: symbols
                .iter()
                .filter(|symbol| constructor_class(&symbol.name).is_some())
                .map(|symbol| symbol.address)
                .collect(),
        }
    }

    /// The lookups over `symbols`, the address-ordered list that this index was built from.
    pub fn over<'a>(&'a self, symbols: &'a [Symbol]) -> IndexedSymbols<'a> {
        debug_assert_eq!(self.by_name.len(), symbols.len());

        IndexedSymbols {
            symbols,
            index: self,
        }
    }
}

/// An executable's address-ordered symbols with their [`SymbolIndex`].
#[derive(Clone, Copy)]
pub(in crate::binding) struct IndexedSymbols<'a> {
    pub(super) symbols: &'a [Symbol],
    index: &'a SymbolIndex,
}

impl<'a> IndexedSymbols<'a> {
    /// The address of every constructor, whatever its class.
    pub(super) fn constructors(self) -> &'a BTreeSet<u64> {
        &self.index.constructors
    }

    /// The symbols named `name`, in list order.
    pub(super) fn named(self, name: &str) -> impl Iterator<Item = &'a Symbol> + use<'a, '_> {
        self.in_name_order_from(name)
            .map(move |position| &self.symbols[position])
            .take_while(move |symbol| symbol.name == name)
    }

    /// The constructors of `class`, whose [`constructor_class`] is `class`, in list order.
    pub(super) fn constructors_of(self, class: &str) -> Vec<&'a Symbol> {
        let base = class.split_once('<').map_or(class, |(base, _)| base);
        let prefix = format!("{class}::{base}(");
        let positions: BTreeSet<usize> = self
            .in_name_order_from(&prefix)
            .take_while(|&position| self.symbols[position].name.starts_with(&prefix))
            .filter(|&position| constructor_class(&self.symbols[position].name) == Some(class))
            .collect();

        positions
            .into_iter()
            .map(|position| &self.symbols[position])
            .collect()
    }

    /// The symbols at any of `addresses`, in list order.
    pub(super) fn at(
        self,
        addresses: &BTreeSet<u64>,
    ) -> impl Iterator<Item = &'a Symbol> + use<'a, '_> {
        addresses.iter().flat_map(move |&address| {
            self.symbols[self.position(address)..]
                .iter()
                .take_while(move |symbol| symbol.address == address)
        })
    }

    /// The symbols with an address in `range`, in list order.
    pub(super) fn between(self, range: Range<u64>) -> impl Iterator<Item = &'a Symbol> {
        let start = self.position(range.start);

        self.symbols[start..]
            .iter()
            .take_while(move |symbol| symbol.address < range.end)
    }

    /// The least symbol address above `address`.
    pub(super) fn next_address(self, address: u64) -> Option<u64> {
        let after = self.position(address.checked_add(1)?);

        self.symbols.get(after).map(|symbol| symbol.address)
    }

    /// [`super::families::vtable_group`] through the index.
    pub(super) fn vtable_group(self, data: &ReadOnlyData, class: &str) -> Option<VtableGroup> {
        let start = self.unique(&format!("vtable for {class}"))?;
        let typeinfo = self.unique(&format!("typeinfo for {class}"))?;
        let end = self.next_address(start)?;

        vtable_group_between(data, start, end, typeinfo)
    }

    /// The one address of the symbols named `name`.
    fn unique(self, name: &str) -> Option<u64> {
        let addresses: BTreeSet<_> = self.named(name).map(|symbol| symbol.address).collect();

        (addresses.len() == 1).then(|| *addresses.first().unwrap())
    }

    /// The positions from the first symbol whose name is not below `name`, in name order.
    fn in_name_order_from(self, name: &str) -> impl Iterator<Item = usize> + use<'a> {
        let symbols = self.symbols;
        let start = self
            .index
            .by_name
            .partition_point(|&position| symbols[position].name.as_str() < name);

        self.index.by_name[start..].iter().copied()
    }

    /// The position of the first symbol whose address is not below `address`.
    fn position(self, address: u64) -> usize {
        self.symbols
            .partition_point(|symbol| symbol.address < address)
    }
}

#[cfg(test)]
mod tests {
    use super::super::families::Names;
    use super::*;

    fn symbol(name: &str, address: u64) -> Symbol {
        Symbol {
            name: name.into(),
            address,
        }
    }

    fn described<'a>(symbols: impl IntoIterator<Item = &'a Symbol>) -> Vec<String> {
        symbols
            .into_iter()
            .map(|symbol| format!("{} {:#x}", symbol.name, symbol.address))
            .collect()
    }

    /// The catalog's order: by address, then by name.
    #[test]
    fn indexed_lookups_find_what_a_scan_of_the_list_finds_in_list_order() {
        let symbols = vec![
            symbol("A<B>::A()", 0x10),
            symbol("A::A()", 0x18),
            symbol("A::A()", 0x20),
            symbol("B::B()", 0x20),
            symbol("other", 0x20),
            symbol("A<B<C>>::A(int)", 0x28),
            symbol("A<B>::A(int)", 0x30),
            symbol("A<x::Y>::A()", 0x38),
            symbol("A::B::B()", 0x40),
            symbol("A::A() [clone .cold.1]", 0x50),
            symbol("vtable for A<B>", 0x100),
            symbol("vtable for A<B>", 0x100),
            symbol("typeinfo for A<B>", 0x200),
        ];
        let index = SymbolIndex::new(&symbols);
        let indexed = index.over(&symbols);
        let scan = |keep: &dyn Fn(&Symbol) -> bool| described(symbols.iter().filter(|s| keep(s)));
        let addresses = BTreeSet::from([0x18, 0x20, 0x100]);

        assert_eq!(
            described(indexed.named("A::A()")),
            scan(&|symbol| symbol.name == "A::A()")
        );
        for class in ["A", "A<B>", "A<B<C>>", "A<x::Y>", "A::B", "B"] {
            assert_eq!(
                described(indexed.constructors_of(class)),
                scan(&|symbol| constructor_class(&symbol.name) == Some(class))
            );
        }
        assert_eq!(
            described(indexed.at(&addresses)),
            scan(&|symbol| addresses.contains(&symbol.address))
        );
        assert_eq!(
            described(indexed.between(0x18..0x30)),
            scan(&|symbol| (0x18..0x30).contains(&symbol.address))
        );
        assert_eq!(
            indexed.constructors().iter().copied().collect::<Vec<_>>(),
            [0x10, 0x18, 0x20, 0x28, 0x30, 0x38, 0x50]
        );
        assert_eq!(indexed.next_address(0x20), Some(0x28));
        assert_eq!(indexed.next_address(0x200), None);
        assert_eq!(indexed.unique("vtable for A<B>"), Some(0x100));
        assert_eq!(indexed.unique("A::A()"), None);
        assert_eq!(
            Names::new(&symbols).of(0x20).collect::<Vec<_>>(),
            ["A::A()", "B::B()", "other"]
        );
    }
}
