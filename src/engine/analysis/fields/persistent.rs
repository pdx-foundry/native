//! Join generic persistent member destinations to constructor-installed virtual methods.
use super::containers::{self, PathMasks};
use super::{
    ConcreteReader, EmbeddedField, FieldGap, FieldGapKind, FieldInput, Owner, PathOutcome,
    ReaderJoin, Receiver, RootField, TokenPath, tokens,
};
use crate::engine::analysis::{
    discovery::SymbolIndex,
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

pub(super) fn discover(
    binding: Option<&super::PersistentInput>,
    read_only_data: &[super::DataSection],
    fields: &[RootField],
) -> PersistentFields {
    let offsets: BTreeSet<_> = fields
        .iter()
        .flat_map(|field| &field.readers)
        .filter(|join| readers::reads_embedded_object(join))
        .filter_map(readers::destination)
        .filter(|offset| (0..SPAN as i64 - 8).contains(offset))
        .collect();
    if offsets.is_empty() {
        return PersistentFields::default();
    }
    let Some(binding) = binding else {
        return PersistentFields::default();
    };
    let ConstructorPoints {
        points,
        words,
        containers,
        gaps,
    } = constructor_points(binding, read_only_data, &offsets);
    // An offset that the constructors do not establish takes the point that the loader's inline
    // build installs there.
    let mut points = points;
    for (&offset, &point) in &binding.loader_points {
        if offsets.contains(&offset) {
            points.entry(offset).or_insert(point);
        }
    }
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
                readers::reads_scoped_numeric(join) && readers::destination(join) == Some(*offset)
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

/// The keys of each embedded object that a field of `owner` reads on every path, when the
/// object's member reader has no block family. A block family's own method gives the grammar of
/// the others. The object is in the owner's allocation, so its analysis keeps the owner's
/// constructor evidence, with its receiver at the field's destination.
pub(super) fn embedded(
    input: &FieldInput,
    symbol_index: &SymbolIndex,
    owner: &Owner<'_>,
    fields: &[RootField],
    paths: &[TokenPath],
    readers: &BTreeMap<i64, ConcreteReader>,
    names: &BTreeMap<i64, tokens::Token>,
) -> Vec<EmbeddedField> {
    let mut embedded = Vec::new();
    let serializer_slot = |callee: &str| {
        input
            .stack_serializers
            .values()
            .find(|serializer| serializer.reader == callee)
            .map(|serializer| serializer.owner_slot)
    };
    for field in fields {
        // Every path that holds the token, including a wider one, must read the object.
        let joins: Option<Vec<_>> = paths
            .iter()
            .filter(|path| path.domain[0] <= field.token && field.token <= path.domain[1])
            .filter(|path| path.outcome != PathOutcome::Rejected)
            .map(|path| match &path.outcome {
                PathOutcome::Reader(join) => Some(join),
                _ => None,
            })
            .collect();
        let Some(joins) = joins else {
            continue;
        };
        let Some((read, member, receiver)) = embedded_reader(&joins, readers, &serializer_slot)
        else {
            continue;
        };
        let objects = input
            .embedded_objects
            .get(&member)
            .map_or(&[][..], Vec::as_slice);
        let child = owner.holding(Owner {
            reader: member.clone(),
            receiver,
            objects,
            persistent: owner.persistent,
            enclosing: Vec::new(),
        });
        let result = super::owner_result(input, symbol_index, &child, names);
        // With no named key and no reader of any key, the field keeps its unresolved members
        // and their gap.
        if result.fields.is_empty() && result.any_key.is_none() {
            continue;
        }
        embedded.push(EmbeddedField {
            token: field.token,
            read,
            member,
            fields: Box::new(result),
        });
    }
    embedded
}

/// The read, member reader and receiver of the one embedded object that every join reads: a
/// stack serializer around the owner member at its destination, or an object at one destination
/// whose constructor-installed reader has no block family.
fn embedded_reader(
    joins: &[&ReaderJoin],
    readers: &BTreeMap<i64, ConcreteReader>,
    serializer_slot: &dyn Fn(&str) -> Option<i64>,
) -> Option<(String, String, Receiver)> {
    // Every path reads the field through one stack serializer: its member reader gives the
    // keys, with the wrapped member at its owner slot.
    let serializers: Vec<_> = joins
        .iter()
        .map(|join| match join {
            ReaderJoin::Stored {
                callee,
                kind: crate::ReaderKind::Block,
                destination: Some(destination),
                ..
            } => serializer_slot(callee).map(|slot| (callee.clone(), slot, *destination)),
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if let [Some((member, slot, destination))] = &serializers[..] {
        let receiver = Receiver::Serializer {
            slot: *slot,
            owner_offset: *destination,
        };

        return Some((readers::PERSISTENT_READ.into(), member.clone(), receiver));
    }

    if !joins
        .iter()
        .all(|join| readers::reads_embedded_object(join))
    {
        return None;
    }

    let offsets: BTreeSet<_> = joins
        .iter()
        .map(|join| readers::destination(join))
        .collect();
    let [Some(offset)] = offsets.into_iter().collect::<Vec<_>>()[..] else {
        return None;
    };
    let reader = readers.get(&offset)?;
    let keys = reader.family == crate::BlockFamily::Unknown
        && reader.member.ends_with("::ReadMember(CReader&, int)");

    keys.then(|| {
        (
            reader.read.clone(),
            reader.member.clone(),
            Receiver::Owner(offset),
        )
    })
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
                    // An object outside the owner, such as the stack string that a delegating
                    // constructor passes on, is built by an ordinary call for the owner.
                    if (owner..owner + SPAN).contains(&receiver) {
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
                }
                if enter_constructors {
                    return Ok(image.unbound_call(target, machine));
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
            speak_for_established(&mut path_containers, &points);
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

/// Limit the paths of one run to the containers whose address points the run establishes, with an
/// entry for each, so that an empty entry is a path that does not construct the container. A run
/// that does not establish a point, such as a baseline that forgets the owner at an ordinary call
/// after an inline construction, says nothing about that container.
fn speak_for_established(paths: &mut [BTreeMap<i64, BTreeSet<u64>>], points: &BTreeMap<i64, u64>) {
    for path in paths {
        path.retain(|offset, _| points.contains_key(offset));

        for &offset in points.keys() {
            path.entry(offset).or_default();
        }
    }
}

/// The agreed mask of each container at a kept point that a path gives a mask, over the masks of
/// every path that has an entry for it.
fn container_masks(
    paths: &[BTreeMap<i64, BTreeSet<u64>>],
    points: &BTreeMap<i64, u64>,
) -> BTreeMap<i64, Result<u64, Unresolved>> {
    let offsets: BTreeSet<i64> = paths
        .iter()
        .flat_map(|path| path.iter())
        .filter(|(_, masks)| !masks.is_empty())
        .map(|(&offset, _)| offset)
        .collect();

    offsets
        .into_iter()
        .filter(|offset| points.contains_key(offset))
        .map(|offset| {
            let mut masks = PathMasks::default();

            for path_masks in paths.iter().filter_map(|path| path.get(&offset)) {
                masks.add(path_masks.iter().copied());
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
