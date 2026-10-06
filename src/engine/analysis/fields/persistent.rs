//! Join generic persistent member destinations to constructor-installed virtual methods.
use super::containers::{self, PathMasks};
use super::{ConcreteReader, FieldGap, FieldGapKind, FieldInput, ReaderJoin, RootField};
use crate::engine::analysis::{
    evaluate::{Call, Code, Exit, ReadOnlyData},
    readers,
    receivers::{ConstructorImage, Constructors, accept_entered_path, install_vtables},
    stop::{CauseKind, Unresolved},
};
use std::collections::{BTreeMap, BTreeSet};

const SPAN: u64 = 0x10000;

#[derive(Default)]
pub(super) struct PersistentFields {
    pub readers: BTreeMap<i64, ConcreteReader>,
    pub points: BTreeMap<i64, u64>,
    pub scoped: BTreeMap<i64, u64>,
    /// Requested words that every constructor's entered run establishes with one value.
    pub words: BTreeMap<(i64, u64), u64>,
    pub containers: BTreeMap<i64, Result<u64, Unresolved>>,
    pub gaps: Vec<FieldGap>,
}

pub(super) fn discover(input: &FieldInput, fields: &[RootField]) -> PersistentFields {
    let offsets: BTreeSet<_> = fields
        .iter()
        .flat_map(|field| &field.readers)
        .filter_map(|join| {
            let ReaderJoin::Joined { callee, .. } = join else {
                return None;
            };
            matches!(
                callee.as_str(),
                "CReader::Read(CPersistent&)"
                    | "CVariableValue::Read(CReader&, EScopeType)"
                    | "CVariableValue::Assign(CToken const&, EScopeType, CString const&)"
            )
            .then(|| readers::destination(join))
            .flatten()
        })
        .filter(|offset| (0..SPAN as i64 - 8).contains(offset))
        .collect();
    if offsets.is_empty() {
        return PersistentFields::default();
    }
    let Some(binding) = &input.persistent else {
        return PersistentFields::default();
    };
    let ConstructorPoints {
        points,
        words,
        containers,
        gaps,
    } = constructor_points(binding, &input.read_only_data, &offsets);
    let readers = points
        .clone()
        .into_iter()
        .filter_map(|(offset, point)| {
            binding
                .readers
                .get(&point)
                .cloned()
                .map(|reader| (offset, reader))
        })
        .collect();
    let scoped = points
        .clone()
        .into_iter()
        .filter(|(offset, _)| {
            fields.iter().flat_map(|field| &field.readers).any(|join| {
                matches!(join, ReaderJoin::Joined { callee, .. }
                    if matches!(callee.as_str(),
                        "CVariableValue::Read(CReader&, EScopeType)"
                            | "CVariableValue::Assign(CToken const&, EScopeType, CString const&)"
                    ) && readers::destination(join) == Some(*offset))
            })
        })
        .collect();
    PersistentFields {
        readers,
        points,
        scoped,
        words,
        containers,
        gaps,
    }
}

/// The vtable address points that every owner constructor installs at the requested offsets.
pub(crate) struct ConstructorPoints {
    pub points: BTreeMap<i64, u64>,
    /// Requested words that every constructor's entered run establishes with one value.
    pub words: BTreeMap<(i64, u64), u64>,
    /// The category mask of each modifier container at a kept point, from every path of every
    /// run that returned.
    pub containers: BTreeMap<i64, Result<u64, Unresolved>>,
    pub gaps: Vec<FieldGap>,
}

/// Run every owner constructor of `binding` and keep the address points at `offsets` that all
/// of them agree on.
pub(crate) fn constructor_points(
    binding: &super::PersistentInput,
    read_only_data: &[super::DataSection],
    offsets: &BTreeSet<i64>,
) -> ConstructorPoints {
    let sections = ReadOnlyData::new(
        read_only_data
            .iter()
            .map(|section| (section.address, section.bytes.clone()))
            .collect(),
    )
    .with_32_bit_words(&binding.initialized_words);
    let image = ConstructorImage::new(&sections, &binding.pointers, &binding.writable_slots)
        .with_calls(binding.constructor_calls.clone());
    let mut agreement: Option<BTreeMap<i64, u64>> = None;
    let mut word_agreement: Option<BTreeMap<(i64, u64), u64>> = None;
    let mut container_paths = Vec::new();
    let mut gaps = Vec::new();
    for constructor in &binding.constructors {
        type Run = (
            BTreeMap<i64, u64>,
            BTreeMap<(i64, u64), u64>,
            Vec<BTreeMap<i64, BTreeSet<u64>>>,
        );
        let run = |enter_constructors: bool| -> Result<Run, Unresolved> {
            let bodies: Vec<_> = binding
                .constructors
                .iter()
                .map(|body| (body.address, body.code.as_slice()))
                .chain(
                    binding
                        .constructor_bodies
                        .values()
                        .filter(|_| enter_constructors)
                        .map(|body| (body.address, body.code.as_slice())),
                )
                .collect();
            let mut bodies = bodies;
            if enter_constructors {
                bodies.extend(
                    binding
                        .constructor_calls
                        .helpers
                        .iter()
                        .map(|(&at, bytes)| (at, bytes.as_slice())),
                );
            }
            let code = Code::decode(&bodies)
                .map_err(|_| Unresolved::new("persistent-constructor-code"))?;
            let mut machine = if enter_constructors {
                image.entered(&code)
            } else {
                image.baseline(&code)
            };
            machine.intercept_tail_calls(binding.summaries.keys().copied().collect());
            let owner = machine.reserve(SPAN);
            machine.set_register(0, owner);
            if enter_constructors {
                machine.track_owner(owner, owner + SPAN);
            }
            let constructors = Constructors {
                code: &code,
                image: &image,
                summaries: &binding.summaries,
                owner,
                end: owner + SPAN,
            };
            let paths = machine.run_paths(constructor.address, &mut |target, machine| {
                if target.is_some_and(|target| binding.never_return.contains(&target)) {
                    return Ok(Call::Stop);
                }
                let container = target.and_then(|target| binding.containers.mask(target, machine));
                let receiver = machine
                    .register(0)
                    .filter(|receiver| (owner..owner + SPAN).contains(receiver));
                if let (Some(mask), Some(receiver)) = (container, receiver) {
                    machine.label(containers::LABEL | (receiver - owner), mask);
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
                if let Some((target, vtables)) =
                    target.and_then(|target| Some((target, binding.summaries.get(&target)?)))
                {
                    let receiver = machine.known_register(0, "persistent-constructor-receiver")?;
                    if !(owner..owner + SPAN).contains(&receiver) {
                        return Err(Unresolved::new("persistent-constructor-owner"));
                    }
                    let vtable_bound = || Unresolved::new("persistent-vtable-bound");
                    if !enter_constructors {
                        install_vtables(
                            machine,
                            receiver,
                            owner + SPAN,
                            vtables,
                            CauseKind::Invalidated,
                        )
                        .ok_or_else(vtable_bound)?;
                        return Ok(Call::Return(None));
                    }
                    let body_available = binding.constructor_bodies.contains_key(&target);
                    return constructors
                        .call(machine, target, receiver, body_available)
                        .ok_or_else(vtable_bound);
                }
                if enter_constructors {
                    return Ok(machine.opaque_call());
                }
                machine.forget(owner, SPAN);
                Ok(Call::Return(None))
            });
            let mut established: Option<BTreeMap<i64, u64>> = None;
            let mut established_words: Option<BTreeMap<(i64, u64), u64>> = None;
            let mut path_containers = Vec::new();
            for path in paths {
                match path.end? {
                    Exit::Stopped(target) if binding.never_return.contains(&target) => continue,
                    Exit::Returned => {}
                    _ => return Err(Unresolved::new("persistent-constructor-terminal")),
                }
                accept_entered_path(&path.machine)?;
                let points: BTreeMap<_, _> = offsets
                    .iter()
                    .filter_map(|&offset| {
                        path.machine
                            .read(owner + offset as u64, 8)
                            .map(|point| (offset, point))
                    })
                    .collect();
                let words = requested_words(&path.machine, owner, &points, binding);
                intersect(&mut established, points);
                intersect(&mut established_words, words);
                path_containers.push(path_containers_of(&path.machine, owner, offsets, binding));
            }
            let points = established.ok_or(Unresolved::new("persistent-constructor-return"))?;
            Ok((
                points,
                established_words.unwrap_or_default(),
                path_containers,
            ))
        };
        let baseline = run(false).map(|(points, _, paths)| {
            container_paths.extend(paths);
            points
        });
        let entered = run(true);
        // A constructor body that the baseline does not enter may write any word, so only an
        // entered run establishes words.
        let words = match &entered {
            Ok((_, words, _)) => words.clone(),
            Err(_) => BTreeMap::new(),
        };
        intersect(&mut word_agreement, words);
        let entered = entered.map(|(points, _, paths)| {
            container_paths.extend(paths);
            points
        });
        let result = match (baseline, entered) {
            (Ok(baseline), Ok(mut entered)) => {
                entered.extend(baseline);
                Ok(entered)
            }
            (Ok(baseline), _) => Ok(baseline),
            (Err(baseline), Err(_)) => Err(baseline),
            (Err(_), entered) => entered,
        };
        match result {
            Ok(points) => intersect(&mut agreement, points),
            Err(stop) => {
                intersect(&mut agreement, BTreeMap::new());
                gaps.push(FieldGap::unresolved(FieldGapKind::ReaderJoin, stop));
            }
        }
    }
    let points = agreement.unwrap_or_default();
    let words = word_agreement
        .unwrap_or_default()
        .into_iter()
        .filter(|((offset, _), _)| points.contains_key(offset))
        .collect();
    let containers = container_masks(&container_paths, &points);
    ConstructorPoints {
        points,
        words,
        containers,
        gaps,
    }
}

/// The masks that one path gives each container, by owner offset: the mask of each construction
/// that it labels, and the mask word that a requested modifier destination holds at its end.
fn path_containers_of(
    machine: &crate::engine::analysis::evaluate::Machine<'_>,
    owner: u64,
    offsets: &BTreeSet<i64>,
    binding: &super::PersistentInput,
) -> BTreeMap<i64, BTreeSet<u64>> {
    let mut masks = BTreeMap::<i64, BTreeSet<u64>>::new();

    for (&key, &mask) in machine
        .labels()
        .range(containers::LABEL..containers::LABEL + SPAN)
    {
        let offset = (key - containers::LABEL) as i64;
        masks.entry(offset).or_default().insert(mask);
    }

    for &offset in offsets {
        let destination = owner + offset as u64;
        let modifier = machine
            .read(destination, 8)
            .and_then(|point| binding.readers.get(&point))
            .is_some_and(|reader| reader.family == crate::BlockFamily::Modifier);
        let word = machine.read(destination + binding.containers.mask_offset, 4);

        if let (true, Some(word)) = (modifier, word) {
            masks.entry(offset).or_default().insert(word);
        }
    }

    masks
}

/// The agreed mask of each container at a kept point, over the masks of every path.
fn container_masks(
    paths: &[BTreeMap<i64, BTreeSet<u64>>],
    points: &BTreeMap<i64, u64>,
) -> BTreeMap<i64, Result<u64, Unresolved>> {
    let offsets: BTreeSet<i64> = paths.iter().flat_map(|path| path.keys().copied()).collect();

    offsets
        .into_iter()
        .filter(|offset| points.contains_key(offset))
        .map(|offset| {
            let mut masks = PathMasks::default();
            for path in paths {
                masks.add(path.get(&offset).into_iter().flatten().copied());
            }
            (offset, masks.agreed())
        })
        .collect()
}

/// The requested words inside each persistent destination at the end of one constructor path.
fn requested_words(
    machine: &crate::engine::analysis::evaluate::Machine<'_>,
    owner: u64,
    points: &BTreeMap<i64, u64>,
    binding: &super::PersistentInput,
) -> BTreeMap<(i64, u64), u64> {
    points
        .iter()
        .filter_map(|(&offset, point)| Some((offset, binding.requested_words.get(point)?)))
        .flat_map(|(offset, words)| words.iter().map(move |&word| (offset, word)))
        .filter_map(|(offset, word)| {
            let value = machine.read(owner + offset as u64 + word, 8)?;
            Some(((offset, word), value))
        })
        .collect()
}

fn intersect<K: Ord, V: PartialEq>(agreement: &mut Option<BTreeMap<K, V>>, points: BTreeMap<K, V>) {
    match agreement {
        Some(known) => known.retain(|offset, point| points.get(offset) == Some(point)),
        None => *agreement = Some(points),
    }
}
