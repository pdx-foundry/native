//! Connect a constructed persistent object to its read and collection insertion on every path.
use super::{
    CollectionField, FieldInput, RegistryFieldResult, RootField, dispatch, inventory, tokens,
};
use crate::engine::analysis::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};
use crate::engine::analysis::stop::Unresolved;
use std::collections::BTreeMap;

const OBJECT: u64 = 1 << 62;
const ALLOCATED: u64 = 1 << 60;
const READ: u64 = 1 << 61;
const INSERTED_CLASS: u64 = 1;
const INSERTED_OFFSET: u64 = 2;
const INSERTED_OBJECT: u64 = 3;
const SPAN: u64 = 0x10000;

pub(super) fn discover(
    input: &FieldInput,
    fields: &[RootField],
    names: &BTreeMap<i64, tokens::Token>,
) -> Vec<CollectionField> {
    if input.objects.is_empty() {
        return Vec::new();
    }
    let root_name = format!(
        "{}::ReadMember(CReader&, int)",
        input.selection.owner_candidate
    );
    let Some(root) = input
        .functions
        .iter()
        .find(|function| function.name == root_name)
    else {
        return Vec::new();
    };
    let Ok(code) = Code::decode(&[(root.address, &root.code)]) else {
        return Vec::new();
    };
    let data = ReadOnlyData::new(
        input
            .read_only_data
            .iter()
            .map(|section| (section.address, section.bytes.clone()))
            .collect(),
    );
    let allocate: Vec<_> = input
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "operator new(unsigned long)")
        .map(|symbol| symbol.address)
        .collect();
    let mut collections = Vec::new();
    for field in fields.iter().filter(|field| {
        field
            .readers
            .iter()
            .any(|join| matches!(join, super::ReaderJoin::Missing(_)))
    }) {
        let mut machine = Machine::new(&code, &data);
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
                machine.label(ALLOCATED | at, 1);
                return Ok(Call::Return(Some(at)));
            }
            for (index, object) in input.objects.iter().enumerate() {
                if object.constructors.contains(&target) {
                    let at = machine.known_register(0, "constructed-object")?;
                    if machine.labelled(ALLOCATED | at) != Some(1) {
                        return Err(Unresolved::new("object-not-allocated"));
                    }
                    if machine.labelled(INSERTED_OBJECT) == Some(at) {
                        return Err(Unresolved::new("inserted-object-reconstructed"));
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
                    if machine.labelled(OBJECT | at) == Some(index as u64)
                        && machine.register(1) == Some(reader)
                    {
                        machine.label(READ | at, index as u64);
                        return Ok(Call::Return(None));
                    }
                }
                if object.insert.contains(&target) {
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
                        machine.label(INSERTED_OBJECT, at);
                        machine.label(INSERTED_CLASS, index as u64);
                        machine.label(INSERTED_OFFSET, receiver - owner);
                        return Ok(Call::Return(None));
                    }
                }
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
                Some((
                    path.machine.labelled(INSERTED_CLASS)?,
                    path.machine.labelled(INSERTED_OFFSET)?,
                ))
            })
            .collect();
        let Some(joined) = joined.filter(|joined| !joined.is_empty()) else {
            continue;
        };
        let first = joined[0];
        if joined.iter().any(|other| *other != first) {
            continue;
        }
        let object = &input.objects[first.0 as usize];
        let (paths, mut gaps) = dispatch::explore_owner(input, &object.class);
        let (fields, field_gaps) = inventory::fields_and_gaps(&paths, names);
        gaps.extend(field_gaps);
        let partition_accounted = inventory::partition_accounted(&paths);
        collections.push(CollectionField {
            token: field.token,
            offset: first.1,
            data_offset: object.data_offset,
            class: object.class.clone(),
            fields: Box::new(RegistryFieldResult {
                uses: Vec::new(),
                fields,
                paths,
                gaps,
                partition_accounted,
                collections: Vec::new(),
            }),
        });
    }
    collections
}
