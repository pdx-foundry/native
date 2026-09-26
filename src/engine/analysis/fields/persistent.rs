//! Join generic persistent member destinations to constructor-installed virtual methods.
use super::{ConcreteReader, FieldGap, FieldGapKind, FieldInput, ReaderJoin, RootField};
use crate::engine::analysis::{
    evaluate::{Call, Code, Exit, Machine, ReadOnlyData},
    readers,
    stop::Unresolved,
};
use std::collections::{BTreeMap, BTreeSet};

const SPAN: u64 = 0x10000;

pub(super) fn discover(
    input: &FieldInput,
    fields: &[RootField],
) -> (BTreeMap<i64, ConcreteReader>, Vec<FieldGap>) {
    let offsets: BTreeSet<_> = fields
        .iter()
        .flat_map(|field| &field.readers)
        .filter_map(|join| {
            let ReaderJoin::Joined { callee, .. } = join else {
                return None;
            };
            (callee == "CReader::Read(CPersistent&)")
                .then(|| readers::destination(join))
                .flatten()
        })
        .filter(|offset| (0..SPAN as i64 - 8).contains(offset))
        .collect();
    if offsets.is_empty() {
        return (BTreeMap::new(), vec![]);
    }
    let Some(binding) = &input.persistent else {
        return (BTreeMap::new(), vec![]);
    };
    let data = ReadOnlyData::new(
        input
            .read_only_data
            .iter()
            .map(|section| (section.address, section.bytes.clone()))
            .collect(),
    );
    let mut agreement: Option<BTreeMap<i64, u64>> = None;
    let mut gaps = Vec::new();
    for constructor in &binding.constructors {
        let run = || -> Result<BTreeMap<i64, u64>, Unresolved> {
            let bodies: Vec<_> = binding
                .constructors
                .iter()
                .map(|body| (body.address, body.code.as_slice()))
                .collect();
            let code = Code::decode(&bodies)
                .map_err(|_| Unresolved::new("persistent-constructor-code"))?;
            let mut machine = Machine::new(&code, &data);
            for (&slot, &pointer) in &binding.pointers {
                machine.write(slot, 8, pointer);
            }
            let owner = machine.reserve(SPAN);
            machine.set_register(0, owner);
            let paths = machine.run_paths(constructor.address, &mut |target, machine| {
                if target.is_some_and(|target| binding.never_return.contains(&target)) {
                    return Ok(Call::Stop);
                }
                if target.is_some_and(|target| {
                    binding
                        .constructors
                        .iter()
                        .any(|body| body.address == target)
                }) {
                    if machine.register(0) != Some(owner) {
                        return Err(Unresolved::new("persistent-owner-delegate"));
                    }
                    return Ok(Call::Enter);
                }
                if let Some(vtables) = target.and_then(|target| binding.summaries.get(&target)) {
                    let receiver = machine.known_register(0, "persistent-constructor-receiver")?;
                    if !(owner..owner + SPAN).contains(&receiver) {
                        return Err(Unresolved::new("persistent-constructor-owner"));
                    }
                    crate::engine::analysis::receivers::install_vtables(
                        machine,
                        receiver,
                        owner + SPAN,
                        vtables,
                    )
                    .ok_or(Unresolved::new("persistent-vtable-bound"))?;
                    return Ok(Call::Return(None));
                }
                machine.forget(owner, SPAN);
                Ok(Call::Return(None))
            });
            let mut established: Option<BTreeMap<i64, u64>> = None;
            for path in paths {
                match path.end? {
                    Exit::Stopped(target) if binding.never_return.contains(&target) => continue,
                    Exit::Returned => {}
                    _ => return Err(Unresolved::new("persistent-constructor-terminal")),
                }
                let points: BTreeMap<_, _> = offsets
                    .iter()
                    .filter_map(|&offset| {
                        path.machine
                            .read(owner + offset as u64, 8)
                            .map(|point| (offset, point))
                    })
                    .collect();
                intersect(&mut established, points);
            }
            established.ok_or(Unresolved::new("persistent-constructor-return"))
        };
        match run() {
            Ok(points) => intersect(&mut agreement, points),
            Err(stop) => {
                intersect(&mut agreement, BTreeMap::new());
                gaps.push(FieldGap::unresolved(FieldGapKind::ReaderJoin, stop));
            }
        }
    }
    let readers = agreement
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(offset, point)| {
            binding
                .readers
                .get(&point)
                .cloned()
                .map(|reader| (offset, reader))
        })
        .collect();
    (readers, gaps)
}

fn intersect(agreement: &mut Option<BTreeMap<i64, u64>>, points: BTreeMap<i64, u64>) {
    match agreement {
        Some(known) => known.retain(|offset, point| points.get(offset) == Some(point)),
        None => *agreement = Some(points),
    }
}
