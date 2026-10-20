//! Connect a constructed persistent object to its read and collection insertion on every path.
//! A factory that returns the object through `x8` is entered, so its allocation and constructor
//! call are evaluated. A moving insertion counts only when the binding proved that it clears its
//! source; the model then clears the moved-from pointer. With no insertion, the one owner word
//! that holds the object owns it; the root may first release the object that an earlier read
//! stored there through its deleting destructor. Paths join at loop heads, and a call that changes
//! no script state, such as a duplicate check's diagnostic or `_memcmp`, returns.
//!
//! The walk reads constant pointer slots and initialized zero-fill words, so a constructor
//! argument that the code loads through either is known. It labels the category mask of a
//! modifier container's construction, which gives the collection's container mask.
use super::containers::{self, PathMasks};
use super::{CollectionField, FieldInput, Owner, RootField, tokens};
use crate::engine::analysis::discovery::SymbolIndex;
use crate::engine::analysis::evaluate::{Call, Code, Exit, Machine, Path, ReadOnlyData};
use crate::engine::analysis::readers;
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
/// The labels of an object that a resizing constructed at an array's end: the addresses of the
/// array's pointer and count words, which the resizing writes, and the pointer that it left.
const EMPLACED_POINTER: u64 = 4;
const EMPLACED_COUNT: u64 = 5;
const EMPLACED_ELEMENTS: u64 = 6;
/// The labels of a resizing that a run reached with its array unknown: the array's owner offset
/// and the class of its objects.
const RESIZED_OFFSET: u64 = 7;
const RESIZED_CLASS: u64 = 8;
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
    // The walk runs the factories and the `shared_ptr` destructors of the objects.
    let factories: Vec<_> = input
        .functions
        .iter()
        .filter(|function| {
            objects.iter().any(|object| {
                object.factories.contains(&function.address)
                    || object.shared_releases.contains(&function.address)
            })
        })
        .collect();
    // Dispatch goes on in the inherited readers and value readers, so the walk enters them.
    let inherited: Vec<_> = input
        .functions
        .iter()
        .filter(|function| {
            function.address != root.address
                && (readers::is_inheritable_member(&function.name)
                    || readers::is_member_value_reader(&function.name))
        })
        .collect();
    let mut entered: BTreeSet<u64> = factories
        .iter()
        .chain(&inherited)
        .map(|function| function.address)
        .collect();
    // A call can reach an inherited reader through an import stub that the image binds to its
    // body. The walk runs each such stub as a branch to the body.
    let stub_branches: Vec<(u64, [u8; 4])> = input
        .stubs
        .iter()
        .filter(|(_, body)| entered.contains(body))
        .filter_map(|(&stub, &body)| Some((stub, branch_to(stub, body)?)))
        .collect();
    entered.extend(stub_branches.iter().map(|(stub, _)| stub));
    let ranges: Vec<_> = std::iter::once(root)
        .chain(factories)
        .chain(inherited)
        .map(|function| (function.address, function.code.as_slice()))
        .chain(
            stub_branches
                .iter()
                .map(|(stub, branch)| (*stub, branch.as_slice())),
        )
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
    let compares: Vec<_> = input
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "_memcmp")
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
        let run = |start: Start| {
            let mut machine = Machine::new(&code, image.constant_data());
            let owner = machine.reserve(SPAN);
            let reader = machine.reserve(SPAN);
            // A store through an unknown pointer invalidates this sentinel along with any
            // unprotected memory. Such a store could reset the collection between reads.
            let unchanged = machine.reserve(8);
            machine.write(unchanged, 8, 1);
            let mut release = None;
            let mut resized = None;
            match start {
                Start::Unknown => {}
                Start::Owned(offset, previous) => {
                    let value = match previous {
                        Previous::Null => 0,
                        Previous::Object => {
                            let (object, destructor) = stand_in(&mut machine);
                            release = Some((object, destructor));
                            object
                        }
                    };
                    machine.write(owner + offset, 8, value);
                }
                Start::Resized(array) => {
                    resized = one_element(&mut machine, &objects[array.class], owner, array)
                        .map(|elements| (array, elements));
                }
            }
            machine.set_register(0, owner);
            machine.set_register(1, reader);
            machine.set_register(2, field.token as u64);
            if let Some(offset) = input.key_readers.reader_token {
                machine.write(reader + offset, 4, field.token as u64);
            }
            let paths = machine.run_paths_joining(root.address, &mut |target, machine| {
                let target = target.ok_or_else(|| Unresolved::new("nested-indirect-call"))?;
                if release.is_some_and(|(object, destructor)| {
                    target == destructor && machine.register(0) == Some(object)
                }) {
                    return Ok(Call::Return(None));
                }
                if allocate.contains(&target) {
                    let at = machine.reserve(SPAN);
                    machine.label(ALLOCATED | at, machine.pc() + 4);
                    return Ok(Call::Return(Some(at)));
                }
                if entered.contains(&target) {
                    return Ok(Call::Enter);
                }
                for (index, object) in objects.iter().enumerate() {
                    if object.constructors.contains(&target) {
                        let at = machine.known_register(0, "constructed-object")?;
                        if machine.labelled(ALLOCATED | at).is_none()
                            && !emplaced(machine, object, at)
                        {
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
                    if object.array_emplace.contains(&target) {
                        return emplace(machine, object, index, owner, resized);
                    }
                    if Some(target) == input.key_readers.persistent_read
                        && machine.register(0) == Some(reader)
                    {
                        let at = machine.known_register(1, "read-object")?;
                        if machine.labelled(OBJECT | at) == Some(index as u64)
                            && machine.labelled(EMPLACED_POINTER).is_some()
                        {
                            machine.label(READ | at, index as u64);
                            return Ok(Call::Return(None));
                        }
                    }
                    if target == object.read {
                        let at = machine.known_register(0, "read-object")?;
                        let inline = constructed_inline(machine, object, at);
                        if machine.register(1) == Some(reader)
                            && (machine.labelled(OBJECT | at) == Some(index as u64) || inline)
                        {
                            if let Some(start) = machine.labelled(ALLOCATED | at).filter(|_| inline)
                            {
                                machine.label(INLINE_START | at, start);
                                machine.label(INLINE_READ | at, machine.pc());
                            }
                            machine.label(OBJECT | at, index as u64);
                            machine.label(READ | at, index as u64);
                            return Ok(Call::Return(None));
                        }
                    }
                    let moved = object.moving_insert.get(&target).copied();
                    if moved.is_some() || object.insert.contains(&target) {
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
                            for word in (0..moved.unwrap_or(0)).step_by(8) {
                                machine.write(pointer + word, 8, 0);
                            }
                            machine.label(INSERTED_OBJECT, at);
                            machine.label(INSERTED_CLASS, index as u64);
                            machine.label(INSERTED_OFFSET, receiver - owner);
                            return Ok(Call::Return(None));
                        }
                    }
                }
                if let Some(initialized) = initialized_members(machine, objects, &zero_fill, target)
                {
                    machine
                        .forget_known_bytes(initialized.start, initialized.end - initialized.start);
                    return Ok(Call::Return(None));
                }
                if compares.contains(&target)
                    || symbol_index
                        .name(&input.symbols, target)
                        .is_some_and(|name| {
                            readers::changes_no_script_state(name)
                                || names_log_id_outside(machine, name, owner)
                        })
                {
                    return Ok(Call::Return(None));
                }
                Err(Unresolved::new("nested-unmodelled-call"))
            });
            FieldRun {
                owner,
                unchanged,
                paths,
            }
        };
        let first_run = run(Start::Unknown);
        let joined = match (first_run.join(), first_run.resized_array()) {
            (Some(joined), _) => joined,
            // A resizing must find its array holding one element and grow it to two.
            (None, Some(array)) => {
                let Some(joined) = run(Start::Resized(array)).join() else {
                    continue;
                };
                joined
            }
            (None, None) => {
                // A root that keeps one owned object may first release the object that an
                // earlier read stored. Both starting states of the word must join.
                let Some(word) = first_run.owned_word() else {
                    continue;
                };
                let joined = [Previous::Null, Previous::Object]
                    .into_iter()
                    .map(|previous| run(Start::Owned(word, previous)).join())
                    .collect::<Option<Vec<_>>>();
                let Some(joined) = joined else {
                    continue;
                };
                joined.into_iter().flatten().collect()
            }
        };
        let first = joined[0].site;
        if joined.iter().any(|insertion| insertion.site != first) {
            continue;
        }
        let container_mask = joined
            .iter()
            .any(|insertion| insertion.container_mask.is_some())
            .then(|| {
                let mut masks = PathMasks::default();
                for insertion in &joined {
                    masks.add(insertion.container_mask);
                }

                masks.agreed()
            });
        let construction_scopes =
            construction_scopes(joined.iter().map(|insertion| insertion.scope));
        let object = &objects[first.class as usize];
        let inline_constructions = joined
            .iter()
            .filter_map(|insertion| insertion.inline_construction);
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
                    serializer: None,
                    base: None,
                    objects: &object.objects,
                    persistent,
                },
                names,
            ),
        };
        let (data_offset, count_offset) = match first.owned {
            true => (None, None),
            false => (object.data_offset, object.count_offset),
        };
        collections.push(CollectionField {
            token: field.token,
            offset: first.offset,
            owned_pointer: first.owned,
            data_offset,
            count_offset,
            class: object.class.clone(),
            reader,
            fields: Box::new(fields),
            container_mask,
            construction_scopes,
        });
    }
    collections
}

/// The encoding of `b body` at `from`, when the distance fits the branch's 26-bit word offset.
fn branch_to(from: u64, body: u64) -> Option<[u8; 4]> {
    const B: u32 = 0x1400_0000;
    let words = (body as i64).checked_sub(from as i64)? / 4;

    (-(1 << 25)..1 << 25)
        .contains(&words)
        .then(|| (B | (words as u32 & 0x03ff_ffff)).to_le_bytes())
}

/// How one run of a field starts: with the owner unknown, with one owned word given, or with
/// the array that a resizing grows holding one element.
#[derive(Clone, Copy)]
enum Start {
    Unknown,
    Owned(u64, Previous),
    Resized(ResizedArray),
}

/// An owner array of objects that a resizing grows: its owner offset and its objects' class.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ResizedArray {
    offset: u64,
    class: usize,
}

/// Give `array` of `owner` one element at a fresh address, and return that address. `None` when
/// the class's array layout is not proven.
fn one_element(
    machine: &mut Machine<'_>,
    object: &super::ObjectReader,
    owner: u64,
    array: ResizedArray,
) -> Option<u64> {
    let (data, count) = (object.data_offset?, object.count_offset?);
    let elements = machine.reserve(SPAN);
    machine.write(owner + array.offset + data, 8, elements);
    machine.write(owner + array.offset + count, 4, 1);

    Some(elements)
}

/// Construct an object of `objects[index]` at the end of the owner array that the resizing's
/// receiver names, as `CPdxArray<Class, int>::SetSizeAndEmplace<>` does. The run must have given
/// that array one element (`resized`), which the root has not changed, and the root must ask for
/// two. The array then holds two objects and points one element size before the new one, so the
/// root's address of its last element is the new object only when the root's element size is the
/// class's. A run that gave the array nothing labels it and stops, so the next run can.
fn emplace(
    machine: &mut Machine<'_>,
    object: &super::ObjectReader,
    index: usize,
    owner: u64,
    resized: Option<(ResizedArray, u64)>,
) -> Result<Call, Unresolved> {
    let (Some(data), Some(count), Some(size)) = (
        object.data_offset,
        object.count_offset,
        object.array_element_size,
    ) else {
        return Err(Unresolved::new("emplace-layout"));
    };
    let array = machine.known_register(0, "emplace-collection")?;

    if !(owner..owner + SPAN).contains(&array) || machine.labelled(INSERTED_CLASS).is_some() {
        return Err(Unresolved::new("emplace-collection"));
    }

    let site = ResizedArray {
        offset: array - owner,
        class: index,
    };
    let Some((_, elements)) = resized.filter(|(given, _)| *given == site) else {
        machine.label(RESIZED_OFFSET, site.offset);
        machine.label(RESIZED_CLASS, index as u64);
        return Err(Unresolved::new("emplace-count"));
    };
    let unchanged = machine.read(array + data, 8) == Some(elements)
        && machine.read(array + count, 4) == Some(1);

    if !unchanged {
        return Err(Unresolved::new("emplace-storage-changed"));
    }

    if machine.register(1) != Some(2) {
        return Err(Unresolved::new("emplace-growth"));
    }

    let at = machine.reserve(SPAN);
    machine.write(array + data, 8, at - size);
    machine.write(array + count, 4, 2);
    for (&offset, &point) in &object.vtables {
        machine.write(at + offset, 8, point);
    }
    for (&slot, &pointer) in &object.pointers {
        machine.write(slot, 8, pointer);
    }
    machine.unlabel(containers::LABEL | at);
    machine.unlabel(SCOPE | at);
    machine.label(OBJECT | at, index as u64);
    machine.label(EMPLACED_POINTER, array + data);
    machine.label(EMPLACED_COUNT, array + count);
    machine.label(EMPLACED_ELEMENTS, at - size);
    machine.label(INSERTED_OBJECT, at);
    machine.label(INSERTED_CLASS, index as u64);
    machine.label(INSERTED_OFFSET, array - owner);

    Ok(Call::Return(None))
}

/// What the owner word of one owned object holds before the read: nothing, or the object that an
/// earlier read of the field stored there.
#[derive(Clone, Copy)]
enum Previous {
    Null,
    Object,
}

/// A stand-in for an owned object that an earlier read stored, and the address of its deleting
/// destructor. Its vtable's second slot, where the Itanium C++ ABI puts the deleting destructor,
/// holds that address; its other slots are unknown, so a call through any of them stops the path.
fn stand_in(machine: &mut Machine<'_>) -> (u64, u64) {
    let object = machine.reserve(8);
    let vtable = machine.reserve(SPAN);
    let destructor = machine.reserve(4);
    machine.write(object, 8, vtable);
    machine.write(vtable + 8, 8, destructor);

    (object, destructor)
}

/// The paths of one field's run of the collection pass, with the run's owner and the sentinel
/// that an unknown store invalidates.
struct FieldRun<'a> {
    owner: u64,
    unchanged: u64,
    paths: Vec<Path<'a>>,
}

/// Where a joined path keeps the object that it read: an array insertion, or the one owner word
/// that owns it.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Site {
    class: u64,
    offset: u64,
    owned: bool,
}

/// What one joined path established about the object that it read and kept.
struct Insertion {
    site: Site,
    container_mask: Option<u64>,
    scope: Option<u64>,
    inline_construction: Option<(u64, u64)>,
}

impl FieldRun<'_> {
    /// The insertion of every path that returned, when each of them kept the sentinel and one
    /// read object. A looping path arrived where an earlier state covers its own; that
    /// comparison holds known values only, so it must still keep the sentinel and write no owner
    /// byte outside the joined site.
    fn join(&self) -> Option<Vec<Insertion>> {
        let (looped, ended): (Vec<_>, Vec<_>) = self
            .paths
            .iter()
            .partition(|path| path.end == Ok(Exit::Looped));
        let joined: Vec<_> = ended
            .iter()
            .map(|path| {
                (path.end == Ok(Exit::Returned) && path.machine.read(self.unchanged, 8) == Some(1))
                    .then(|| self.insertion(&path.machine))?
            })
            .collect::<Option<_>>()?;
        let site = joined.first()?.site;
        let looped_keep_storage = looped.iter().all(|path| {
            path.machine.read(self.unchanged, 8) == Some(1)
                && self.writes_only_its_site(&path.machine, site)
        });

        looped_keep_storage.then_some(joined)
    }

    /// Whether every owner word that the path wrote is the site's own pointer: none for an
    /// array insertion.
    fn writes_only_its_site(&self, machine: &Machine<'_>, site: Site) -> bool {
        let word = self.owner + site.offset;

        machine
            .written_words(self.owner, self.owner + SPAN)
            .iter()
            .all(|&written| site.owned && (written == word || written == word + 4))
    }

    /// Where the path kept its object: the array that it inserted the object into, with no owner
    /// byte written, or else the one owner word that holds the object.
    fn insertion(&self, machine: &Machine<'_>) -> Option<Insertion> {
        let (site, object) = match machine.labelled(INSERTED_OBJECT) {
            Some(object) if self.writes_only_emplaced_array(machine) => {
                let site = Site {
                    class: machine.labelled(INSERTED_CLASS)?,
                    offset: machine.labelled(INSERTED_OFFSET)?,
                    owned: false,
                };
                (machine.labelled(READ | object) == Some(site.class)).then_some(())?;
                (site, object)
            }
            Some(_) => return None,
            None => self.owned_object(machine)?,
        };

        Some(Insertion {
            site,
            container_mask: machine.labelled(containers::LABEL | object),
            scope: machine.labelled(SCOPE | object),
            inline_construction: machine
                .labelled(INLINE_START | object)
                .zip(machine.labelled(INLINE_READ | object)),
        })
    }

    /// Whether the path wrote no owner word but the pointer and count of the array that a
    /// resizing constructed its object in, and those still hold what the resizing left.
    fn writes_only_emplaced_array(&self, machine: &Machine<'_>) -> bool {
        let written = machine.written_words(self.owner, self.owner + SPAN);
        let (Some(pointer), Some(count)) = (
            machine.labelled(EMPLACED_POINTER),
            machine.labelled(EMPLACED_COUNT),
        ) else {
            return written.is_empty();
        };
        let kept = machine.read(pointer, 8) == machine.labelled(EMPLACED_ELEMENTS)
            && machine.read(count, 4) == Some(2);

        kept && written.is_subset(&BTreeSet::from([pointer, pointer + 4, count]))
    }

    /// The one owner word that the path wrote, when it holds an object that the path
    /// constructed and read.
    fn owned_object(&self, machine: &Machine<'_>) -> Option<(Site, u64)> {
        let words = machine.written_words(self.owner, self.owner + SPAN);
        let word = *words.first()?;
        if !word.is_multiple_of(8) || words != BTreeSet::from([word, word + 4]) {
            return None;
        }
        let object = machine.read(word, 8)?;
        let class = machine.labelled(READ | object)?;
        (machine.labelled(OBJECT | object) == Some(class)).then_some(())?;
        let site = Site {
            class,
            offset: word - self.owner,
            owned: true,
        };

        Some((site, object))
    }

    /// The one array that the run's paths reached a resizing of with the array unknown, which
    /// the next run gives one element.
    fn resized_array(&self) -> Option<ResizedArray> {
        let arrays: BTreeSet<_> = self
            .paths
            .iter()
            .filter_map(|path| {
                Some((
                    path.machine.labelled(RESIZED_OFFSET)?,
                    path.machine.labelled(RESIZED_CLASS)?,
                ))
            })
            .collect();
        let [(offset, class)] = arrays.into_iter().collect::<Vec<_>>()[..] else {
            return None;
        };

        Some(ResizedArray {
            offset,
            class: class as usize,
        })
    }

    /// The owner word of a returned path that keeps its object there, which the next run gives
    /// a stand-in for the object that an earlier read stored.
    fn owned_word(&self) -> Option<u64> {
        self.paths
            .iter()
            .filter(|path| path.end == Ok(Exit::Returned))
            .find_map(|path| self.owned_object(&path.machine))
            .map(|(site, _)| site.offset)
    }
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

/// The string concatenations that build a log ID from its owner's.
const LOG_ID_CONCATENATIONS: [&str; 2] = [
    "operator+(CString const&, char const*)",
    "operator+(CString&&, CString&&)",
];

/// Whether `callee` builds or sets an object's log ID, a name for the engine's log that no read
/// or insertion uses, and writes nothing in the owner at `owner`. `SetLogID` writes only its
/// receiver's log-ID string, so the receiver must be outside the owner. The concatenations and
/// `GetLogID` write only the `CString` that they return through `x8`, so `x8` must name the stack.
fn names_log_id_outside(machine: &Machine<'_>, callee: &str, owner: u64) -> bool {
    if callee.ends_with("::SetLogID(CString const&)") {
        return machine
            .register(0)
            .is_some_and(|receiver| !(owner..owner + SPAN).contains(&receiver));
    }

    let returns_log_id =
        LOG_ID_CONCATENATIONS.contains(&callee) || callee.ends_with("::GetLogID() const");

    returns_log_id
        && machine
            .register(8)
            .is_some_and(|result| machine.is_stack(result))
}

/// Where `std::make_shared` puts the object in its control block: past the control block's
/// vtable pointer and its shared and weak counts (libc++ `__shared_ptr_emplace`).
const EMPLACED_OBJECT: u64 = 0x18;

/// Whether `at` is the object inside a `make_shared` control block of `object`'s class: an
/// allocation whose primary address point is that control block's, with the object at
/// [`EMPLACED_OBJECT`].
fn emplaced(machine: &Machine<'_>, object: &super::ObjectReader, at: u64) -> bool {
    let Some(block) = at.checked_sub(EMPLACED_OBJECT) else {
        return false;
    };

    machine.labelled(ALLOCATED | block).is_some()
        && object
            .shared_emplace_point
            .is_some_and(|point| machine.read(block, 8) == Some(point))
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
