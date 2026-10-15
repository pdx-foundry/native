//! Connect a constructed persistent object to its read and collection insertion on every path.
//! A factory that returns the object through `x8` is entered, so its allocation and constructor
//! call are evaluated. A moving insertion counts only when the binding proved that it clears its
//! source; the model then clears the moved-from pointer.
//!
//! The walk reads constant pointer slots and initialized zero-fill words, so a constructor
//! argument that the code loads through either is known. It labels the category mask of a
//! modifier container's construction, which gives the collection's container mask.
use super::containers::{self, PathMasks};
use super::{CollectionField, FieldInput, Owner, RootField, tokens};
use crate::engine::analysis::discovery::SymbolIndex;
use crate::engine::analysis::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};
use crate::engine::analysis::receivers::ConstructorImage;
use crate::engine::analysis::stop::Unresolved;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

const OBJECT: u64 = 1 << 62;
/// The label of an `operator new` allocation, at its address: the allocation call's return.
const ALLOCATED: u64 = 1 << 60;
const READ: u64 = 1 << 61;
const INSERTED_CLASS: u64 = 1;
const INSERTED_OFFSET: u64 = 2;
const INSERTED_OBJECT: u64 = 3;
const SPAN: u64 = 0x10000;
/// The label of the scope that an object's constructor receives, at the object's address.
const SCOPE: u64 = 1 << 58;
/// The labels of an object that the root constructed inline, at its address: its allocation's
/// return, where the code that constructs it starts, and the call that first reads it.
const INLINE_START: u64 = 1 << 57;
const INLINE_READ: u64 = 1 << 56;
/// A constructor scope argument that is not known on the path.
const UNKNOWN_SCOPE: u64 = u64::MAX;

pub(super) fn discover(
    input: &FieldInput,
    symbol_index: &SymbolIndex,
    owner: &Owner<'_>,
    fields: &[RootField],
    names: &BTreeMap<i64, tokens::Token>,
) -> Vec<CollectionField> {
    let objects = owner.objects;
    if objects.is_empty() {
        return Vec::new();
    }
    let root_name = format!("{}::ReadMember(CReader&, int)", owner.class);
    let Some(root) = input
        .functions
        .iter()
        .find(|function| function.name == root_name)
    else {
        return Vec::new();
    };
    let factories: Vec<_> = input
        .functions
        .iter()
        .filter(|function| {
            objects
                .iter()
                .any(|object| object.factories.contains(&function.address))
        })
        .collect();
    let ranges: Vec<_> = std::iter::once(root)
        .chain(factories)
        .map(|function| (function.address, function.code.as_slice()))
        .collect();
    let Ok(code) = Code::decode(&ranges) else {
        return Vec::new();
    };
    let data = ReadOnlyData::new(
        input
            .read_only_data
            .iter()
            .map(|section| (section.address, section.bytes.clone()))
            .collect(),
    );
    let image = match &input.persistent {
        Some(binding) => ConstructorImage::new(
            &data.with_32_bit_words(&binding.initialized_words),
            &binding.pointers,
            &binding.writable_slots,
        ),
        None => ConstructorImage::new(&data, &BTreeMap::new(), &BTreeSet::new()),
    };
    let containers = input.persistent.as_ref().map(|binding| &binding.containers);
    let allocate: Vec<_> = input
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "operator new(unsigned long)")
        .map(|symbol| symbol.address)
        .collect();
    let zero_fill: Vec<_> = input
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "_bzero")
        .map(|symbol| symbol.address)
        .collect();
    let scope_registers = scope_registers(&input.symbols, objects);
    let mut collections = Vec::new();
    for field in fields.iter().filter(|field| {
        field
            .readers
            .iter()
            .any(|join| matches!(join, super::ReaderJoin::Missing(_)))
    }) {
        let mut machine = Machine::new(&code, image.constant_data());
        let owner = machine.reserve(SPAN);
        let reader = machine.reserve(SPAN);
        // A store through an unknown pointer invalidates this sentinel along with any
        // unprotected memory. Such a store could reset the collection between reads.
        let unchanged = machine.reserve(8);
        machine.write(unchanged, 8, 1);
        machine.set_register(0, owner);
        machine.set_register(1, reader);
        machine.set_register(2, field.token as u64);
        let paths = machine.run_paths(root.address, &mut |target, machine| {
            let target = target.ok_or_else(|| Unresolved::new("nested-indirect-call"))?;
            if allocate.contains(&target) {
                let at = machine.reserve(SPAN);
                machine.label(ALLOCATED | at, machine.pc() + 4);
                return Ok(Call::Return(Some(at)));
            }
            if objects
                .iter()
                .any(|object| object.factories.contains(&target))
            {
                return Ok(Call::Enter);
            }
            for (index, object) in objects.iter().enumerate() {
                if object.constructors.contains(&target) {
                    let at = machine.known_register(0, "constructed-object")?;
                    if machine.labelled(ALLOCATED | at).is_none() {
                        // A member of an object that the root constructs inline.
                        if initialized_members(machine, objects, &zero_fill, target).is_some() {
                            break;
                        }
                        return Err(Unresolved::new("object-not-allocated"));
                    }
                    if machine.labelled(INSERTED_OBJECT) == Some(at) {
                        return Err(Unresolved::new("inserted-object-reconstructed"));
                    }
                    match containers.and_then(|known| known.mask(target, machine)) {
                        Some(mask) => machine.label(containers::LABEL | at, mask),
                        None => machine.unlabel(containers::LABEL | at),
                    }
                    match scope_registers.get(&target) {
                        Some(&register) => machine.label(
                            SCOPE | at,
                            machine.register(register).unwrap_or(UNKNOWN_SCOPE),
                        ),
                        None => machine.unlabel(SCOPE | at),
                    }
                    for (&offset, &point) in &object.vtables {
                        machine.write(at + offset, 8, point);
                    }
                    for (&slot, &pointer) in &object.pointers {
                        machine.write(slot, 8, pointer);
                    }
                    machine.unlabel(READ | at);
                    machine.label(OBJECT | at, index as u64);
                    return Ok(Call::Return(Some(at)));
                }
                if target == object.read {
                    let at = machine.known_register(0, "read-object")?;
                    let inline = constructed_inline(machine, object, at);
                    if machine.register(1) == Some(reader)
                        && (machine.labelled(OBJECT | at) == Some(index as u64) || inline)
                    {
                        if let Some(start) = machine.labelled(ALLOCATED | at).filter(|_| inline) {
                            machine.label(INLINE_START | at, start);
                            machine.label(INLINE_READ | at, machine.pc());
                        }
                        machine.label(OBJECT | at, index as u64);
                        machine.label(READ | at, index as u64);
                        return Ok(Call::Return(None));
                    }
                }
                let moves = object.moving_insert.contains(&target);
                if moves || object.insert.contains(&target) {
                    let receiver = machine.known_register(0, "insert-collection")?;
                    let pointer = machine.known_register(2, "insert-value")?;
                    let at = machine
                        .read(pointer, 8)
                        .ok_or_else(|| Unresolved::new("insert-object"))?;
                    if (owner..owner + SPAN).contains(&receiver)
                        && machine.labelled(READ | at) == Some(index as u64)
                        && machine.labelled(OBJECT | at) == Some(index as u64)
                        && machine.labelled(INSERTED_CLASS).is_none()
                    {
                        if moves {
                            machine.write(pointer, 8, 0);
                        }
                        machine.label(INSERTED_OBJECT, at);
                        machine.label(INSERTED_CLASS, index as u64);
                        machine.label(INSERTED_OFFSET, receiver - owner);
                        return Ok(Call::Return(None));
                    }
                }
            }
            if let Some(initialized) = initialized_members(machine, objects, &zero_fill, target) {
                machine.forget_known_bytes(initialized.start, initialized.end - initialized.start);
                return Ok(Call::Return(None));
            }
            Err(Unresolved::new("nested-unmodelled-call"))
        });
        let joined: Option<Vec<_>> = paths
            .iter()
            .map(|path| {
                (path.end == Ok(Exit::Returned)
                    && path.machine.read(unchanged, 8) == Some(1)
                    && !path.machine.has_written(owner, SPAN))
                .then_some(())?;
                let inserted = (
                    path.machine.labelled(INSERTED_CLASS)?,
                    path.machine.labelled(INSERTED_OFFSET)?,
                );
                let object = path.machine.labelled(INSERTED_OBJECT)?;

                Some((
                    inserted,
                    path.machine.labelled(containers::LABEL | object),
                    path.machine.labelled(SCOPE | object),
                    path.machine
                        .labelled(INLINE_START | object)
                        .zip(path.machine.labelled(INLINE_READ | object)),
                ))
            })
            .collect();
        let Some(joined) = joined.filter(|joined| !joined.is_empty()) else {
            continue;
        };
        let first = joined[0].0;
        if joined.iter().any(|(other, ..)| *other != first) {
            continue;
        }
        let container_mask = joined.iter().any(|(_, mask, ..)| mask.is_some()).then(|| {
            let mut masks = PathMasks::default();
            for (_, mask, ..) in &joined {
                masks.add(*mask);
            }

            masks.agreed()
        });
        let construction_scopes = construction_scopes(joined.iter().map(|(_, _, scope, _)| *scope));
        let object = &objects[first.0 as usize];
        let inline_constructions = joined.iter().filter_map(|(.., construction)| *construction);
        let persistent = object
            .persistent
            .as_ref()
            .filter(|evidence| constructs_every_path(evidence, inline_constructions));
        let reader = object
            .reader
            .clone()
            .filter(|bound| bound.reader.family != crate::BlockFamily::Unknown);
        let fields = match reader {
            Some(_) => Default::default(),
            None => super::owner_result(
                input,
                symbol_index,
                &Owner {
                    class: &object.class,
                    objects: &object.objects,
                    persistent,
                },
                names,
            ),
        };
        collections.push(CollectionField {
            token: field.token,
            offset: first.1,
            data_offset: object.data_offset,
            count_offset: object.count_offset,
            class: object.class.clone(),
            reader,
            fields: Box::new(fields),
            container_mask,
            construction_scopes,
        });
    }
    collections
}

/// Whether the constructor evidence of a nested owner holds the code of every inline
/// construction that a joined path ran, given as its allocation's return and its first read. Each
/// body runs straight from the one to a return in place of the other, so a path that allocates
/// and reads there runs exactly that body.
fn constructs_every_path(
    evidence: &super::PersistentInput,
    mut constructions: impl Iterator<Item = (u64, u64)>,
) -> bool {
    constructions.all(|(start, read)| {
        evidence
            .constructors
            .iter()
            .any(|body| body.address == start && body.address + body.code.len() as u64 - 4 == read)
    })
}

/// Whether the allocation at `at` holds `object`, which the root constructed inline: no
/// constructor of the method's built it, and its primary address point is in place.
fn constructed_inline(machine: &Machine<'_>, object: &super::ObjectReader, at: u64) -> bool {
    machine.labelled(ALLOCATED | at).is_some()
        && machine.labelled(OBJECT | at).is_none()
        && object
            .vtables
            .get(&0)
            .is_some_and(|&point| machine.read(at, 8) == Some(point))
}

/// The bytes that `target` initializes in an allocation that no object constructor has built,
/// while the root constructs an object there inline. A member constructor in an object's
/// `initializers` takes no address but `x0`, so it writes at most from `x0` to the allocation's
/// end. `_bzero` writes the `x1` bytes at `x0`, which must lie in the allocation.
fn initialized_members(
    machine: &Machine<'_>,
    objects: &[super::ObjectReader],
    zero_fill: &[u64],
    target: u64,
) -> Option<Range<u64>> {
    let start = machine.register(0)?;
    let allocation = allocation_containing(machine, start)?;
    if machine.labelled(OBJECT | allocation).is_some() {
        return None;
    }
    let end = allocation + SPAN;

    if zero_fill.contains(&target) {
        let length = machine.register(1)?;
        return (length <= end - start).then_some(start..start + length);
    }
    objects
        .iter()
        .any(|object| object.initializers.contains(&target))
        .then_some(start..end)
}

/// The start of the `operator new` allocation that holds `address`.
fn allocation_containing(machine: &Machine<'_>, address: u64) -> Option<u64> {
    machine
        .labels()
        .range(ALLOCATED..=ALLOCATED | address)
        .next_back()
        .map(|(&key, _)| key & !ALLOCATED)
        .filter(|&start| (start..start + SPAN).contains(&address))
}

/// The register of each object constructor's scope parameter, for constructors whose last
/// parameter is an `EScopeType`.
fn scope_registers(
    symbols: &[crate::engine::analysis::discovery::Symbol],
    objects: &[super::ObjectReader],
) -> BTreeMap<u64, usize> {
    symbols
        .iter()
        .filter(|symbol| {
            objects
                .iter()
                .any(|object| object.constructors.contains(&symbol.address))
        })
        .filter_map(|symbol| Some((symbol.address, scope_register(&symbol.name)?)))
        .collect()
}

/// The argument register of a demangled function's last parameter when it is an `EScopeType`;
/// `x0` is the receiver.
fn scope_register(name: &str) -> Option<usize> {
    let parameters = name.strip_suffix("EScopeType)")?;
    let mut depth = 0;
    let mut commas = 0;
    for character in parameters.chars().rev() {
        match character {
            ')' | '>' => depth += 1,
            '<' => depth -= 1,
            '(' if depth == 0 => return Some(commas + 1),
            '(' => depth -= 1,
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }

    None
}

/// The distinct scopes that the joined paths pass to the object's constructor, with `None` for a
/// path whose scope is not known. Empty when no path's constructor takes a scope.
fn construction_scopes(paths: impl Iterator<Item = Option<u64>>) -> Vec<Option<i64>> {
    let paths: Vec<_> = paths.collect();
    if paths.iter().all(Option::is_none) {
        return Vec::new();
    }
    let mut scopes = Vec::new();
    for scope in paths {
        let scope = scope
            .filter(|&scope| scope != UNKNOWN_SCOPE)
            .map(|scope| scope as i64);
        if !scopes.contains(&scope) {
            scopes.push(scope);
        }
    }

    scopes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_parameter_register_counts_top_level_parameters() {
        assert_eq!(
            scope_register("C::C(int, ModifierCategory, EScopeType)"),
            Some(3)
        );
        assert_eq!(
            scope_register("C<A, B>::C(CPair<int, int>, EScopeType)"),
            Some(2)
        );
        assert_eq!(scope_register("C::C(EScopeType)"), Some(1));
        assert_eq!(scope_register("C::C(int)"), None);
    }

    #[test]
    fn a_path_with_an_unknown_construction_scope_stays_beside_the_known_ones() {
        let scopes = |paths: &[Option<u64>]| construction_scopes(paths.iter().copied());

        assert_eq!(
            scopes(&[Some(4), Some(UNKNOWN_SCOPE), Some(4)]),
            [Some(4), None]
        );
        assert_eq!(scopes(&[Some(4), None]), [Some(4), None]);
        assert_eq!(scopes(&[None, None]), []);
    }
}
