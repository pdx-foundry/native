//! Read which registry owner each register of a function leads to at entry: the type pointers of
//! the block method (`engine::analysis::callbacks::blocks`).
//!
//! A register leads to the owner type `O` when it holds:
//!
//! - the receiver of a member function of `O`. The class is the function's exact qualifier, so a
//!   member of a nested class such as `CAnomalyType::COutcomeEffect` is not a member of
//!   `CAnomalyType`;
//! - a parameter of type `O const*`, `O*`, `O const&` or `O&`;
//! - an object of a class `C` whose word at offset `k` leads to `O`: the constructors of `C`, and
//!   the members of `C` that load `TPdxNullObject<O>::_pInstance`, store there an item of `O` (a
//!   parameter of one of those types, or that null object) and no item of another owner or other
//!   named value, and every constructor of `C` decodes. A stored null, and a value that the name
//!   pass does not name, agree with any owner.
//!
//! A buffer of pointers to items of `O` is the word at [`ARRAY_BUFFER`] in a `CPdxArray<O const*,
//! int>`: a parameter of that type by pointer or reference; a member array of a class `C` at `k`,
//! when every constructor of `C` decodes and every store that they make at `k` is that array
//! class's vtable; or such a member array of a database `D`, in a function that loads
//! `TGameDatabase<D>::_pInstance` or `D::_pInstance` through its pointer slot.
//!
//! A parameter's register is known up to the first parameter that one general register may not
//! pass: each pointer, reference, integer, `bool` and `TPdxRef<…>` takes the next register,
//! after the receiver of a member function. A by-value class, a floating-point number or an
//! enumeration, which a demangled name does not tell from a class, ends the known registers. A
//! function whose qualifier is not a known class or `(anonymous namespace)` has no known registers:
//! a class has a vtable, a type info, a constructor, a destructor or a `const` member function
//! symbol. A
//! demangled name does not say whether a member function is static; a function of a class is read
//! as taking its receiver in `x0`.
//!
//! Stated assumption: every function that writes a member word that leads to `O` keeps the
//! registry owner's layout: it stores an `O`, a null object of `O`, or null. The reader checks the
//! constructors and the members that load an owner's null object; a writer that does neither is
//! taken on the assumption. Checked by hand on M452 for `CMission` (its constructors, and
//! `ReadMember`, which stores a lookup result or `TPdxNullObject<CMissionType>::_pInstance` at
//! `+0x18`), `CResolution` (its constructor from a `CResolutionType const&`, its default
//! constructor, which stores `TPdxNullObject<CResolutionType>::_pInstance`, and `ReadMember`,
//! which stores a lookup result), `CCosmicStorm` (`+0x328`, which `ReadMember` fills with a lookup
//! result or `TPdxNullObject<CCosmicStormType>::_pInstance`), `CTechnology` (`+0x600`, which
//! `ReadMember` fills with a tier database element or `TPdxNullObject<CTechnologyTier>::_pInstance`)
//! and `CGalacticCommunity` (`+0x2c0`, which `DailyUpdate` fills with the chosen focus or
//! `TPdxNullObject<CGalacticFocus>::_pInstance`). Remove the assumption with a writer check that
//! covers every function that stores the word.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::callbacks::blocks::{TypePointers, receiver_stores};
use crate::engine::analysis::callbacks::{Buffer, StoredWord};
use crate::engine::analysis::discovery::Symbol;

use super::callbacks::{INTEGERS, decoded, signature, top_level};
use super::declarations::Text;

/// The offset of the buffer pointer in a `CPdxArray`: vtable `+0`, buffer `+8`, capacity `+0x10`,
/// size `+0x14`. Checked on M452 in `TradeActions`, `CAgreementManager::CreateAgreement` and
/// `CScriptableValue::GetValue`.
const ARRAY_BUFFER: i64 = 8;

/// The offset of a vtable's address point, which an object's vtable word holds, from the
/// `vtable for` symbol: the offset to top and the type info come first.
const VTABLE_ADDRESS_POINT: u64 = 0x10;

/// The qualifier of a free function in a translation unit's own namespace.
const ANONYMOUS_NAMESPACE: &str = "(anonymous namespace)";

/// The registry owners and the classes whose members lead to one.
pub(super) struct TypeOwners<'a> {
    owners: &'a BTreeSet<&'a str>,
    /// The classes that the symbols name.
    classes: BTreeSet<&'a str>,
    /// The owner that each member word of a class leads to, by class and offset.
    members: BTreeMap<&'a str, BTreeMap<i64, &'a str>>,
    /// The element owner of each member array of a class, by class and the offset of the array's
    /// buffer word.
    arrays: BTreeMap<&'a str, BTreeMap<i64, &'a str>>,
    /// The element owner of each buffer of a global instance's arrays, by each function that loads
    /// the instance.
    instance_loads: BTreeMap<u64, BTreeMap<Buffer, &'a str>>,
}

impl<'a> TypeOwners<'a> {
    /// Read the classes, the member words and the member arrays that lead to one of `owners`.
    /// `pointers` gives the target of each pointer slot, such as a GOT slot.
    pub(super) fn read(
        text: &Text,
        symbols: &'a [Symbol],
        pointers: &BTreeMap<u64, u64>,
        owners: &'a BTreeSet<&'a str>,
    ) -> Self {
        let (classes, constructors) = classes_and_constructors(symbols);
        let mut evidence = Evidence::read(symbols, pointers, owners);
        let forming = text.functions_forming(&evidence.formed_addresses());
        let arrays = member_arrays(text, &constructors, &forming, owners, &evidence);
        let instances = instance_slots(symbols, pointers, &arrays);
        evidence.elements = instance_buffers(&instances, &arrays).collect();
        let writers = null_object_writers(symbols, &constructors, &forming, &evidence);
        let members = member_words_of_classes(text, &constructors, &writers, owners, &evidence);
        let instance_loads = text
            .functions_forming(&instances.keys().copied().collect())
            .into_iter()
            .map(|(function, slots)| {
                let loaded = slots.into_iter().map(|slot| (slot, instances[&slot]));
                (
                    function,
                    instance_buffers(&loaded.collect(), &arrays).collect(),
                )
            })
            .collect();

        Self {
            owners,
            classes,
            members,
            arrays,
            instance_loads,
        }
    }

    /// The type pointers of `function`, whose names are `names`: those of its signature, when
    /// every name gives the same, and the buffers of the global instances that it loads.
    pub(super) fn of<'n>(
        &self,
        function: u64,
        mut names: impl Iterator<Item = &'n str>,
    ) -> Option<TypePointers> {
        let signed = names.next().and_then(|first| {
            let first = self.function(first)?;
            names
                .all(|name| self.function(name).as_ref() == Some(&first))
                .then_some(first)
        });
        let Some(loaded) = self.instance_loads.get(&function) else {
            return signed;
        };

        let mut pointers = signed.unwrap_or_default();
        for (&buffer, owner) in loaded {
            pointers.elements.insert(buffer, (*owner).to_owned());
        }
        Some(pointers)
    }

    /// The owner whose member function the function that `names` name is, when every name gives
    /// the same owner.
    pub(super) fn method_owner<'n>(
        &self,
        mut names: impl Iterator<Item = &'n str>,
    ) -> Option<&'a str> {
        let owner = |name: &str| {
            let (qualified, _) = signature(name)?;
            self.owners.get(qualifier(qualified)?).copied()
        };
        let first = owner(names.next()?)?;
        names
            .all(|name| owner(name) == Some(first))
            .then_some(first)
    }

    /// The type pointers of the function `name`, when it has one.
    fn function(&self, name: &str) -> Option<TypePointers> {
        if name.contains(".cold.") {
            return None;
        }
        let (qualified, parameters) = signature(name)?;
        let class = qualifier(qualified).filter(|class| *class != ANONYMOUS_NAMESPACE);
        let member = class.is_some_and(|class| self.classes.contains(class));
        if class.is_some() && !member {
            return None;
        }

        let mut pointers = TypePointers::default();
        if let Some(class) = class {
            self.lead(&mut pointers, 0, class);
            if self.owners.contains(class) {
                pointers.method_of = Some(class.to_owned());
            }
        }
        let registers = parameter_registers(parameters, member);
        for &(register, parameter) in &registers {
            let Some(class) = pointee(parameter) else {
                continue;
            };
            self.lead(&mut pointers, register, class);
            if let Some(owner) = element_class(class).and_then(|class| self.owners.get(class)) {
                pointers.elements.insert(
                    Buffer::Argument(register, ARRAY_BUFFER),
                    (*owner).to_owned(),
                );
            }
        }
        if pointers == TypePointers::default() {
            return None;
        }

        pointers.scope_parameters = registers
            .iter()
            .filter(|(_, parameter)| pointee(parameter) == Some("CEventScope"))
            .map(|&(register, _)| register)
            .collect();
        Some(pointers)
    }

    /// Record the owners that a register holding an object of `class` leads to.
    fn lead(&self, pointers: &mut TypePointers, register: usize, class: &str) {
        if let Some(owner) = self.owners.get(class) {
            pointers.registers.insert(register, (*owner).to_owned());
        }
        for (&offset, owner) in self.members.get(class).into_iter().flatten() {
            pointers
                .members
                .insert((register, offset), (*owner).to_owned());
        }
        for (&buffer, owner) in self.arrays.get(class).into_iter().flatten() {
            pointers
                .elements
                .insert(Buffer::Argument(register, buffer), (*owner).to_owned());
        }
    }
}

/// The member functions of a class, by class.
type ClassFunctions<'a> = BTreeMap<&'a str, Vec<(u64, &'a str)>>;

/// The classes that `symbols` name, and the constructors of each.
fn classes_and_constructors(symbols: &[Symbol]) -> (BTreeSet<&str>, ClassFunctions<'_>) {
    let functions = || {
        symbols
            .iter()
            .filter(|symbol| !symbol.name.contains(".cold."))
    };
    let mut classes: BTreeSet<&str> = symbols
        .iter()
        .filter_map(|symbol| {
            let name = symbol.name.as_str();
            name.strip_prefix("vtable for ")
                .or_else(|| name.strip_prefix("typeinfo for "))
        })
        .collect();
    classes.extend(functions().filter_map(|symbol| const_member_class(&symbol.name)));
    let mut constructors: ClassFunctions = BTreeMap::new();
    for symbol in functions() {
        let Some((class, destructor)) = constructed_class(&symbol.name) else {
            continue;
        };
        classes.insert(class);
        if !destructor {
            constructors
                .entry(class)
                .or_default()
                .push((symbol.address, &symbol.name));
        }
    }
    (classes, constructors)
}

/// The member arrays of owner pointers of each class whose constructors form an array vtable, by
/// the offset of each array's buffer word.
fn member_arrays<'a>(
    text: &Text,
    constructors: &ClassFunctions<'a>,
    forming: &BTreeMap<u64, BTreeSet<u64>>,
    owners: &BTreeSet<&'a str>,
    evidence: &Evidence<'a>,
) -> BTreeMap<&'a str, BTreeMap<i64, &'a str>> {
    let mut arrays = BTreeMap::new();
    for (&class, constructors) in constructors {
        if !constructors
            .iter()
            .any(|(address, _)| forming.contains_key(address))
        {
            continue;
        }
        let Some(stores) = stores_of(text, constructors, owners, &BTreeSet::new()) else {
            continue;
        };
        let buffers = array_buffers(&stores, evidence);
        if !buffers.is_empty() {
            arrays.insert(class, buffers);
        }
    }
    arrays
}

/// The element owner of each buffer of the member arrays of the global instances `instances`.
fn instance_buffers<'s, 'a: 's>(
    instances: &'s BTreeMap<u64, &'a str>,
    arrays: &'s BTreeMap<&'a str, BTreeMap<i64, &'a str>>,
) -> impl Iterator<Item = (Buffer, &'a str)> + 's {
    instances.iter().flat_map(move |(&slot, class)| {
        arrays[class]
            .iter()
            .map(move |(&buffer, &owner)| (Buffer::Instance(slot, buffer), owner))
    })
}

/// The member functions, other than constructors and destructors, of each class with a
/// constructor that load an owner's null object.
fn null_object_writers<'a>(
    symbols: &'a [Symbol],
    constructors: &ClassFunctions<'a>,
    forming: &BTreeMap<u64, BTreeSet<u64>>,
    evidence: &Evidence<'a>,
) -> ClassFunctions<'a> {
    let loads_null_object = |address: &u64| {
        forming
            .get(address)
            .is_some_and(|formed| formed.iter().any(|slot| evidence.is_null_object(*slot)))
    };
    let mut writers: ClassFunctions = BTreeMap::new();
    for symbol in symbols
        .iter()
        .filter(|symbol| loads_null_object(&symbol.address))
    {
        if symbol.name.contains(".cold.") || constructed_class(&symbol.name).is_some() {
            continue;
        }
        let Some((qualified, _)) = signature(&symbol.name) else {
            continue;
        };
        if let Some((&class, _)) =
            qualifier(qualified).and_then(|class| constructors.get_key_value(class))
        {
            writers
                .entry(class)
                .or_default()
                .push((symbol.address, &symbol.name));
        }
    }
    writers
}

/// The member words that lead to an owner, by class, from the stores of each class's
/// constructors and null-object `writers`. A class whose constructors take no owner parameter and
/// that has no such writer has none.
fn member_words_of_classes<'a>(
    text: &Text,
    constructors: &ClassFunctions<'a>,
    writers: &ClassFunctions<'a>,
    owners: &BTreeSet<&'a str>,
    evidence: &Evidence<'a>,
) -> BTreeMap<&'a str, BTreeMap<i64, &'a str>> {
    let buffers: BTreeSet<Buffer> = evidence.elements.keys().copied().collect();
    let mut members = BTreeMap::new();
    for (&class, constructors) in constructors {
        let typed_parameters = constructors
            .iter()
            .any(|(_, name)| !parameter_owners(name, owners).is_empty());
        let others = writers.get(class).map_or(&[][..], Vec::as_slice);
        if !typed_parameters && others.is_empty() {
            continue;
        }
        let Some(constructors) = stores_of(text, constructors, owners, &buffers) else {
            continue;
        };
        let others: Vec<Stores> = others
            .iter()
            .filter_map(|&writer| stores_of(text, &[writer], owners, &buffers)?.pop())
            .collect();

        let words = member_words(constructors.iter().chain(&others), evidence);
        if !words.is_empty() {
            members.insert(class, words);
        }
    }
    members
}

/// What the stored words of a class's writers say about the owners of its members.
struct Evidence<'a> {
    /// The owner of each null object, by the pointer slot that holds the null object's address.
    null_objects: BTreeMap<u64, &'a str>,
    /// The element owner of each array of owner pointers, by its vtable's address point.
    arrays: BTreeMap<u64, &'a str>,
    /// The element owner of each buffer of a global instance's arrays.
    elements: BTreeMap<Buffer, &'a str>,
}

impl<'a> Evidence<'a> {
    /// Read the slots of the null objects `TPdxNullObject<O>::_pInstance` and the vtables of the
    /// arrays `CPdxArray<O const*, int>` and `CPdxArray<O*, int>` of each of `owners`.
    fn read(
        symbols: &'a [Symbol],
        pointers: &BTreeMap<u64, u64>,
        owners: &BTreeSet<&'a str>,
    ) -> Self {
        let mut null_object_addresses: BTreeMap<u64, &str> = BTreeMap::new();
        let mut arrays = BTreeMap::new();
        for symbol in symbols {
            let name = symbol.name.as_str();
            let null_object = name
                .strip_prefix("TPdxNullObject<")
                .and_then(|name| name.strip_suffix(">::_pInstance"));
            if let Some(owner) = null_object.and_then(|class| owners.get(class)) {
                null_object_addresses.insert(symbol.address, owner);
            }
            let array = name.strip_prefix("vtable for ").and_then(element_class);
            if let Some(owner) = array.and_then(|class| owners.get(class)) {
                arrays.insert(symbol.address + VTABLE_ADDRESS_POINT, *owner);
            }
        }

        let null_objects = pointers
            .iter()
            .filter_map(|(&slot, target)| Some((slot, *null_object_addresses.get(target)?)))
            .collect();
        Self {
            null_objects,
            arrays,
            elements: BTreeMap::new(),
        }
    }

    /// The addresses whose forming marks a function as a possible writer: each null object's
    /// slot and each array vtable's symbol.
    fn formed_addresses(&self) -> BTreeSet<u64> {
        self.null_objects
            .keys()
            .copied()
            .chain(self.arrays.keys().map(|point| point - VTABLE_ADDRESS_POINT))
            .collect()
    }

    fn is_null_object(&self, slot: u64) -> bool {
        self.null_objects.contains_key(&slot)
    }

    /// What a word that a writer with `parameters` stores says: the owner of its item, a null
    /// pointer, an array of an owner's pointers, or another value.
    fn word(&self, stored: StoredWord, parameters: &BTreeMap<usize, &'a str>) -> Word<'a> {
        match stored {
            StoredWord::Parameter(register) => parameters
                .get(&register)
                .map_or(Word::Other, |owner| Word::Item(owner)),
            StoredWord::Loaded(slot) => self
                .null_objects
                .get(&slot)
                .map_or(Word::Other, |owner| Word::Item(owner)),
            StoredWord::Constant(0) => Word::Null,
            StoredWord::Constant(point) => self
                .arrays
                .get(&point)
                .map_or(Word::Other, |owner| Word::Array(owner)),
            StoredWord::Element(buffer) => self
                .elements
                .get(&buffer)
                .map_or(Word::Unknown, |owner| Word::Item(owner)),
            StoredWord::Unknown => Word::Unknown,
        }
    }
}

/// What one stored word says about the member that holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Word<'a> {
    /// An item of this owner: a parameter that points at one, or the owner's null object.
    Item(&'a str),
    Null,
    /// The vtable of an array of this owner's pointers.
    Array(&'a str),
    /// A value that the name pass does not name.
    Unknown,
    Other,
}

/// What one function stores in its receiver.
struct Stores<'a> {
    /// The words that it stores, by offset.
    words: BTreeMap<i64, BTreeSet<StoredWord>>,
    /// The owner that each of its parameter registers points at.
    parameters: BTreeMap<usize, &'a str>,
}

/// The stores of each of a class's member functions `functions`, which name the elements of
/// `buffers`. `None` when one does not decode.
fn stores_of<'a>(
    text: &Text,
    functions: &[(u64, &str)],
    owners: &BTreeSet<&'a str>,
    buffers: &BTreeSet<Buffer>,
) -> Option<Vec<Stores<'a>>> {
    functions
        .iter()
        .map(|&(address, name)| {
            Some(Stores {
                words: receiver_stores(&decoded(text, address)?, buffers),
                parameters: parameter_owners(name, owners),
            })
        })
        .collect()
}

/// The pointer slot of each global database instance, `TGameDatabase<D>::_pInstance` or
/// `D::_pInstance`, whose class `D` has a member array of owner pointers, with that class.
fn instance_slots<'a>(
    symbols: &[Symbol],
    pointers: &BTreeMap<u64, u64>,
    arrays: &BTreeMap<&'a str, BTreeMap<i64, &'a str>>,
) -> BTreeMap<u64, &'a str> {
    let instances: BTreeMap<u64, &str> = symbols
        .iter()
        .filter_map(|symbol| {
            let class = symbol.name.strip_suffix("::_pInstance")?;
            let class = class
                .strip_prefix("TGameDatabase<")
                .and_then(|class| class.strip_suffix('>'))
                .unwrap_or(class);
            let (&class, _) = arrays.get_key_value(class)?;
            Some((symbol.address, class))
        })
        .collect();
    pointers
        .iter()
        .filter_map(|(&slot, target)| Some((slot, *instances.get(target)?)))
        .collect()
}

/// What `writers` store at each offset of their receiver.
fn words_by_offset<'w, 'a: 'w>(
    writers: impl IntoIterator<Item = &'w Stores<'a>>,
    evidence: &Evidence<'a>,
) -> BTreeMap<i64, BTreeSet<Word<'a>>> {
    let mut stored: BTreeMap<i64, BTreeSet<Word<'a>>> = BTreeMap::new();
    for writer in writers {
        for (&offset, words) in &writer.words {
            let words = words
                .iter()
                .map(|&word| evidence.word(word, &writer.parameters));
            stored.entry(offset).or_default().extend(words);
        }
    }
    stored
}

/// By offset, the owner of each member word that some writer fills with an item of that owner and
/// no writer fills with an item of another owner or another named value. A null pointer and a
/// value that the name pass does not name agree with any owner: the stated assumption at the top
/// of this module covers them.
fn member_words<'w, 'a: 'w>(
    writers: impl IntoIterator<Item = &'w Stores<'a>>,
    evidence: &Evidence<'a>,
) -> BTreeMap<i64, &'a str> {
    words_by_offset(writers, evidence)
        .into_iter()
        .filter_map(|(offset, words)| {
            let mut named = words
                .into_iter()
                .filter(|word| !matches!(word, Word::Null | Word::Unknown));
            match (named.next(), named.next()) {
                (Some(Word::Item(owner)), None) => Some((offset, owner)),
                _ => None,
            }
        })
        .collect()
}

/// By the offset of its buffer word, the element owner of each member array whose vtable word
/// every store of `constructors` fills with the vtable of one owner's array.
fn array_buffers<'a>(
    constructors: &[Stores<'a>],
    evidence: &Evidence<'a>,
) -> BTreeMap<i64, &'a str> {
    words_by_offset(constructors, evidence)
        .into_iter()
        .filter_map(
            |(offset, words)| match words.into_iter().collect::<Vec<_>>()[..] {
                [Word::Array(owner)] => Some((offset + ARRAY_BUFFER, owner)),
                _ => None,
            },
        )
        .collect()
}

/// The owner that each parameter register of the member function `name` points at. Its receiver
/// takes `x0`.
fn parameter_owners<'a>(name: &str, owners: &BTreeSet<&'a str>) -> BTreeMap<usize, &'a str> {
    let Some((_, parameters)) = signature(name) else {
        return BTreeMap::new();
    };
    parameter_registers(parameters, true)
        .into_iter()
        .filter_map(|(register, parameter)| Some((register, *owners.get(pointee(parameter)?)?)))
        .collect()
}

/// The class of the `const` member function `name`: only a class has `const` members.
fn const_member_class(name: &str) -> Option<&str> {
    let (qualified, _) = signature(name)?;
    is_const(name).then(|| qualifier(qualified)).flatten()
}

/// The class of the constructor or destructor `name`, and whether it is the destructor.
pub(super) fn constructed_class(name: &str) -> Option<(&str, bool)> {
    let (qualified, _) = signature(name)?;
    let class = qualifier(qualified)?;
    let method = &qualified[class.len() + 2..];
    let base = last_component(class);
    let base = base.split_once('<').map_or(base, |(base, _)| base);
    match method.strip_prefix('~') {
        Some(destructor) => (destructor == base).then_some((class, true)),
        None => (method == base).then_some((class, false)),
    }
}

/// The argument register of each parameter in `parameters`, up to the first that one general
/// register may not pass. A member function's receiver takes `x0`.
fn parameter_registers(parameters: &str, member: bool) -> Vec<(usize, &str)> {
    let mut register = usize::from(member);
    let mut found = Vec::new();
    for parameter in top_level(parameters).map(str::trim) {
        if matches!(parameter, "" | "void") {
            continue;
        }
        if register > 7 || !is_one_register(parameter) {
            break;
        }

        found.push((register, parameter));
        register += 1;
    }
    found
}

/// A pointer, a reference, an integer, `bool` or a `TPdxRef<…>`, which one general register
/// passes.
fn is_one_register(parameter: &str) -> bool {
    parameter.ends_with('*')
        || parameter.ends_with('&')
        || parameter.ends_with("* const")
        || parameter.starts_with("TPdxRef<")
        || INTEGERS.contains(&parameter)
}

/// The class that a parameter of type `O const*`, `O*`, `O const&` or `O&` points at.
fn pointee(parameter: &str) -> Option<&str> {
    let class = parameter
        .strip_suffix('*')
        .or_else(|| parameter.strip_suffix('&'))?;
    let class = class.strip_suffix(" const").unwrap_or(class);
    (!class.ends_with(['*', '&'])).then_some(class)
}

/// The element class `O` of the array class `CPdxArray<O const*, int>` or `CPdxArray<O*, int>`.
fn element_class(class: &str) -> Option<&str> {
    let element = class.strip_prefix("CPdxArray<")?.strip_suffix(", int>")?;
    let element = element.strip_suffix('*')?;
    let element = element.strip_suffix(" const").unwrap_or(element);
    (!element.ends_with(['*', '&'])).then_some(element)
}

/// The qualifier of a qualified name, before its last top-level `::`: `A::B::F` gives `A::B`.
fn qualifier(qualified: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut last = None;
    for (at, character) in qualified.char_indices() {
        match character {
            '<' | '(' => depth += 1,
            '>' | ')' => depth = depth.saturating_sub(1),
            ':' if depth == 0 && qualified[at..].starts_with("::") => last = Some(at),
            _ => {}
        }
    }
    last.map(|at| &qualified[..at])
}

/// The last component of a qualifier: `A::B<C::D>` gives `B<C::D>`.
fn last_component(qualifier: &str) -> &str {
    match self::qualifier(qualifier) {
        Some(outer) => &qualifier[outer.len() + 2..],
        None => qualifier,
    }
}

/// Whether the demangled function `name` is a `const` member.
fn is_const(name: &str) -> bool {
    name.split(" [clone")
        .next()
        .is_some_and(|name| name.trim_end().ends_with(" const"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parameter_takes_the_next_register_until_one_that_a_register_may_not_pass() {
        let (_, parameters) = signature(
            "CMission::CMission(TPdxRef<CCountry>, CMissionType const*, CEventScope const&, CMetaRef<CMissionType>)",
        )
        .unwrap();
        assert_eq!(
            parameter_registers(parameters, true),
            [
                (1, "TPdxRef<CCountry>"),
                (2, "CMissionType const*"),
                (3, "CEventScope const&"),
            ]
        );

        let (_, parameters) =
            signature("PerformEvent(CString const&, CCountry*, CCountry*)").unwrap();
        assert_eq!(
            parameter_registers(parameters, false),
            [(0, "CString const&"), (1, "CCountry*"), (2, "CCountry*")]
        );

        let (_, parameters) = signature("CCountry::AddEdict(CEdict const*)").unwrap();
        assert_eq!(
            parameter_registers(parameters, true),
            [(1, "CEdict const*")]
        );

        let (_, parameters) =
            signature("CShip::SetSpeed(CFixedPoint, CEdict const*, float, int)").unwrap();
        assert!(parameter_registers(parameters, true).is_empty());
    }

    #[test]
    fn a_pointer_or_reference_parameter_points_at_its_class() {
        assert_eq!(pointee("CEdict const*"), Some("CEdict"));
        assert_eq!(pointee("CResolution&"), Some("CResolution"));
        assert_eq!(pointee("CEdict**"), None);
        assert_eq!(pointee("CEventScope&&"), None);
        assert_eq!(pointee("int"), None);
    }

    #[test]
    fn only_a_class_has_a_const_member_function() {
        assert_eq!(
            const_member_class("CSubjectSpecialization::IsActive() const"),
            Some("CSubjectSpecialization")
        );
        assert_eq!(
            const_member_class("CSubjectSpecialization::DailyUpdate(CAgreement const&)"),
            None
        );
        assert_eq!(
            const_member_class("PerformEvent(CString const&, CCountry*)"),
            None
        );
    }

    #[test]
    fn a_member_of_a_nested_class_is_not_a_member_of_the_enclosing_class() {
        let owners = BTreeSet::from(["CAnomalyType", "CEdict"]);
        let reader = TypeOwners {
            owners: &owners,
            classes: BTreeSet::from(["CAnomalyType", "CAnomalyType::COutcomeEffect", "CCountry"]),
            members: BTreeMap::new(),
            arrays: BTreeMap::new(),
            instance_loads: BTreeMap::new(),
        };

        assert_eq!(
            reader.function("CAnomalyType::COutcomeEffect::Execute(CEventScope&) const"),
            None
        );
        assert_eq!(
            reader.function("CAnomalyType::OnSuccess(CCountry*, CEventScope&) const"),
            Some(TypePointers {
                method_of: Some("CAnomalyType".into()),
                registers: BTreeMap::from([(0, "CAnomalyType".into())]),
                members: BTreeMap::new(),
                elements: BTreeMap::new(),
                scope_parameters: BTreeSet::from([2]),
            })
        );
        assert_eq!(
            reader.function("CCountry::AddEdict(CEdict const*)"),
            Some(TypePointers {
                method_of: None,
                registers: BTreeMap::from([(1, "CEdict".into())]),
                members: BTreeMap::new(),
                elements: BTreeMap::new(),
                scope_parameters: BTreeSet::new(),
            })
        );
        assert_eq!(reader.function("NUnknown::Run(CEdict const*)"), None);
        assert_eq!(reader.function("CCountry::Fire(CEventScope&)"), None);
        assert_eq!(
            reader.function("Run(CEdict const*)"),
            Some(TypePointers {
                method_of: None,
                registers: BTreeMap::from([(0, "CEdict".into())]),
                members: BTreeMap::new(),
                elements: BTreeMap::new(),
                scope_parameters: BTreeSet::new(),
            })
        );
    }

    #[test]
    fn an_array_parameter_of_owner_pointers_leads_its_buffer_to_the_owner() {
        let owners = BTreeSet::from(["CTradableAction"]);
        let reader = TypeOwners {
            owners: &owners,
            classes: BTreeSet::new(),
            members: BTreeMap::new(),
            arrays: BTreeMap::new(),
            instance_loads: BTreeMap::new(),
        };
        let elements = |name: &str| reader.function(name).map(|pointers| pointers.elements);

        assert_eq!(
            elements(
                "(anonymous namespace)::TradeActions(CPdxArray<CTradableAction const*, int> const&, CCountry const&, CCountry const&)"
            ),
            Some(BTreeMap::from([(
                Buffer::Argument(0, 8),
                "CTradableAction".into()
            )]))
        );
        assert_eq!(
            elements("Run(CCountry const&, CPdxArray<CTradableAction*, int>*)"),
            Some(BTreeMap::from([(
                Buffer::Argument(1, 8),
                "CTradableAction".into()
            )]))
        );
        assert_eq!(elements("Run(CPdxArray<int, int> const&)"), None);
        assert_eq!(
            elements("Run(CPdxArray<CTradableAction const**, int> const&)"),
            None
        );
        assert_eq!(
            elements("NUnknown::Run(CPdxArray<CTradableAction const*, int> const&)"),
            None
        );
    }

    #[test]
    fn a_member_word_and_a_member_array_lead_through_the_register_of_their_class() {
        let owners = BTreeSet::from(["CMissionType"]);
        let reader = TypeOwners {
            owners: &owners,
            classes: BTreeSet::from(["CMission"]),
            members: BTreeMap::from([("CMission", BTreeMap::from([(0x18, "CMissionType")]))]),
            arrays: BTreeMap::from([("CMission", BTreeMap::from([(0xa8, "CEdict")]))]),
            instance_loads: BTreeMap::new(),
        };

        assert_eq!(
            reader.function("CMission::Start()"),
            Some(TypePointers {
                method_of: None,
                registers: BTreeMap::new(),
                members: BTreeMap::from([((0, 0x18), "CMissionType".into())]),
                elements: BTreeMap::from([(Buffer::Argument(0, 0xa8), "CEdict".into())]),
                scope_parameters: BTreeSet::new(),
            })
        );
        assert_eq!(
            reader.function("CGalacticCommunity::Pass(CMission&, bool)"),
            None,
            "CGalacticCommunity is not a known class"
        );
    }

    /// The null object of `CMissionType` is at slot `0x5000`, of `CEdict` at `0x5008`; the array
    /// of `CEdict` pointers has its address point at `0x6010`.
    fn evidence() -> Evidence<'static> {
        Evidence {
            null_objects: BTreeMap::from([(0x5000, "CMissionType"), (0x5008, "CEdict")]),
            arrays: BTreeMap::from([(0x6010, "CEdict")]),
            elements: BTreeMap::new(),
        }
    }

    /// A writer that stores `words` and whose parameter registers point at `parameters`.
    fn writer(
        words: &[(i64, StoredWord)],
        parameters: &[(usize, &'static str)],
    ) -> Stores<'static> {
        let mut stored: BTreeMap<i64, BTreeSet<StoredWord>> = BTreeMap::new();
        for &(offset, word) in words {
            stored.entry(offset).or_default().insert(word);
        }
        Stores {
            words: stored,
            parameters: parameters.iter().copied().collect(),
        }
    }

    #[test]
    fn a_member_word_leads_to_the_one_owner_that_its_writers_store() {
        use StoredWord::{Constant, Loaded, Parameter, Unknown};
        let mission = || BTreeMap::from([(0x18, "CMissionType")]);

        let constructors = [
            writer(&[(0x18, Parameter(2))], &[(2, "CMissionType")]),
            writer(&[(0x18, Parameter(3))], &[(3, "CMissionType")]),
        ];
        assert_eq!(member_words(&constructors, &evidence()), mission());

        let null_or_unknown = [
            writer(&[(0x18, Loaded(0x5000))], &[]),
            writer(&[(0x18, Constant(0)), (0x18, Unknown)], &[]),
        ];
        assert_eq!(member_words(&null_or_unknown, &evidence()), mission());

        let two_owners = [
            writer(&[(0x18, Parameter(1))], &[(1, "CMissionType")]),
            writer(&[(0x18, Parameter(1))], &[(1, "CEdict")]),
        ];
        assert!(member_words(&two_owners, &evidence()).is_empty());

        let another_null_object = [
            writer(&[(0x18, Parameter(1))], &[(1, "CMissionType")]),
            writer(&[(0x18, Loaded(0x5008))], &[]),
        ];
        assert!(member_words(&another_null_object, &evidence()).is_empty());

        let another_type = [
            writer(&[(0x18, Parameter(1))], &[(1, "CMissionType")]),
            writer(&[(0x18, Parameter(2))], &[]),
        ];
        assert!(member_words(&another_type, &evidence()).is_empty());
    }

    #[test]
    fn a_member_array_leads_to_the_owner_whose_array_vtable_every_constructor_stores() {
        use StoredWord::{Constant, Unknown};

        let constructors = [
            writer(&[(0xa0, Constant(0x6010)), (0xa8, Constant(0))], &[]),
            writer(&[(0xa0, Constant(0x6010))], &[]),
        ];
        assert_eq!(
            array_buffers(&constructors, &evidence()),
            BTreeMap::from([(0xa8, "CEdict")])
        );

        let overwritten = [
            writer(&[(0xa0, Constant(0x6010))], &[]),
            writer(&[(0xa0, Unknown)], &[]),
        ];
        assert!(array_buffers(&overwritten, &evidence()).is_empty());

        let another_vtable = [writer(
            &[(0xa0, Constant(0x6010)), (0xa0, Constant(0x7010))],
            &[],
        )];
        assert!(array_buffers(&another_vtable, &evidence()).is_empty());
    }

    #[test]
    fn a_constructor_or_destructor_names_its_class() {
        assert_eq!(
            constructed_class("CMission::CMission(TPdxRef<CCountry>)"),
            Some(("CMission", false))
        );
        assert_eq!(
            constructed_class("CAnomalyType::COutcomeEffect::~COutcomeEffect()"),
            Some(("CAnomalyType::COutcomeEffect", true))
        );
        assert_eq!(
            constructed_class("TPdxRef<CCountry>::TPdxRef(int)"),
            Some(("TPdxRef<CCountry>", false))
        );
        assert_eq!(constructed_class("CMission::Start()"), None);
    }
}
