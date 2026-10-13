//! Modifier families: the modifier names that a registry's code registers for each of its items.
//!
//! The method runs each root of the registry (`joins`) for one item whose memory is unknown
//! except its key, and follows each composed text with the string model (`strings.rs`). A root
//! that loops over the database gets a database that holds that one item; an item root gets the
//! item. The run enters the functions between a root and a registration call, and the composers
//! that only call string functions, so a name that a helper composes from its caller's arguments
//! is followed through the helper. At each registration call it reads the composed name and the
//! category mask argument, with the calls that the path is inside.
//!
//! The item key's place in the item is where the item's constructor stores its copy of the key
//! argument. The method runs the constructor with a labelled long key and finds the labelled
//! buffer pointer in the item. The place is the same for a short key, whose text the object holds
//! in place.
//!
//! A function on a joined context that takes an item of another named registry (an input,
//! `joins`) runs on its own, as an entry beside the roots: it receives the item and a new item of
//! that registry whose key is a second labelled source, at the key place of that registry's item
//! constructor. Its names have both keys, and are never `Always`, since the root may skip the
//! call. A root run does not enter an input. The function is taken to be a member function, so
//! the other item is in the register after `this` for its parameter.
//!
//! Each root runs twice: with a long key and with a short key, since the engine branches on the
//! key's form. An input runs four times, once for each pair of the two keys' forms. A family is
//! one name that one chain of calls registers. Every run must give it the same names at that
//! chain, and every path must give it one mask. A family is `Always` generated only for a root
//! that loops over every item, when, in its condition runs, no path fails, some path returns, and
//! every returned path registers it. A branch on an unknown item field makes a path that skips the
//! call, so such a family is never `Always`. Paths that end in a function that never returns, or
//! in a trap, are ignored. An item root counts as a root that loops over every item only when
//! `loading` establishes that the engine calls it for every item that content defines; otherwise
//! its families are never `Always`.
//!
//! A root's condition runs follow the same code as its name runs, but join states at loop heads,
//! so a loop of unknown length before the registrations does not spend the path bound, and stop a
//! path once it has registered every name that the name run with its key form registered, at the
//! same chain of calls: later code cannot take back a registration, so such a path counts as
//! returned. Names come only from the name runs: a loop that registers on each pass does not
//! join, and its later names would be lost.
//!
//! A cache field is an item word where a returned path of the name runs stores a modifier type
//! that its own registration created. The engine keeps the item's type there and registers only
//! while the field holds the value that the item's constructor leaves. The condition runs start
//! with each cache field at that value, when every returned path of every constructor body, with
//! the key in either form, leaves the same one.
//!
//! A helper can read the category mask of a declared modifier type from the engine's definition
//! table (`CModifierGeneratorBase::GenerateFrom`). The run holds a table with the mask of each
//! type that a direct definition declares; its other fields are unknown.
//!
//! The model writes unresolved text as the empty text, so that the code runs on; the text stays
//! unresolved in every name. After that, a path takes branches that the real text may not take,
//! so only its registrations before that point count for the condition.
//!
//! An item's key does not change after its constructor. A store to an unknown address or a call
//! that the model does not follow therefore leaves the key object and its text as they were. No
//! content reader writes a cache field.
//!
//! Outside the method: database state other than the item array reads as zero, so a gate on
//! database state is not detected; memory that content fills reads as unknown; where a modifier
//! takes effect; the engine's handling of a name that is registered again.
use std::collections::{BTreeMap, BTreeSet};

use super::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};
use super::stop::Unresolved;
use super::strings::{
    ASSUMED_TEXT, Arena, Effect, ITEM_KEY, Model, Node, Part, Sources, StringFunctions,
    StringLayout,
};

pub mod joins;
pub mod loading;

pub use joins::Receiver;
pub use loading::{Loading, NotEstablished};

/// Name and revision of the modifier-family method.
pub const METHOD: &str = "modifier-families/v4";

/// The long key: longer than the longest short string, so the engine stores it in a buffer.
const LONG_KEY: &str = "generatedmodifierfamilyitemkey01";

/// The short key: the engine stores its text in the string object.
const SHORT_KEY: &str = "itemkey1";

/// The long key of an item of another registry, as long as [`LONG_KEY`].
const LONG_REGISTRY_KEY: &str = "generatedmodifierfamilyinputkey1";

/// The short key of an item of another registry.
const SHORT_REGISTRY_KEY: &str = "inputky1";

/// Bytes of item memory that the method gives the constructor and the roots.
pub(super) const ITEM_SPAN: u64 = 0x4000;

/// Bytes of zeroed database memory that a database root reads.
const DATABASE_SPAN: u64 = 0x400;

/// The most paths that one condition run follows. A loop of unknown length joins at its head, but
/// each pass can still fork: `CEthic::InitPostRead` compares each ethic category's key before its
/// registration, and its condition runs return on 210 paths on M452.
const CONDITION_PATH_LIMIT: usize = 256;

/// Labels at or above this value are the method's own facts about a path, not text labels. Every
/// scratch and stack address is below it.
const METHOD_LABELS: u64 = 1 << 62;

/// The label of the number of registrations that a path has made. The label of each
/// registration's record follows it.
const RECORD_COUNT: u64 = METHOD_LABELS;

/// The label of the key's offset that the constructor run found.
const FOUND_KEY: u64 = u64::MAX;

/// The most registrations of one run that get a definition of their own. The engine writes the
/// new modifier type to the registration's first argument, and code after the call reads that
/// type's definition.
const DYNAMIC_TYPES: u64 = 256;

/// Executable-derived input that every registry shares.
pub struct FamilyInput {
    /// The function that registers a generated modifier: the name in `x1`, the category mask on
    /// the stack.
    pub registration: u64,
    /// Offset of the category mask from the stack pointer at the registration call.
    pub category_offset: u64,
    pub database: DatabaseLayout,
    /// The engine's definition table, when its layout is established.
    pub definitions: Option<Definitions>,
    pub strings: StringFunctions,
    pub layout: StringLayout,
    pub data: ReadOnlyData,
}

/// The code of one registry that registers generated modifiers.
pub struct RegistryInput {
    pub roots: Vec<Root>,
    /// Functions that a run enters: the functions between a root and a registration call, and
    /// the composers that they call.
    pub entered: BTreeSet<u64>,
    /// Every body of the item constructor that takes the key.
    pub constructors: Vec<KeyConstructor>,
    /// The code that makes the registry's items, when it has an item root.
    pub loading: Option<Loading>,
    /// The entered functions that take an item of another named registry.
    pub inputs: Vec<SecondInput>,
    /// Every root, entered function and constructor body (also of the inputs' registries), and
    /// the loading code.
    pub code: Code,
}

/// An entered function that takes an item of another named registry.
pub struct SecondInput {
    pub function: u64,
    /// The argument register that holds the item.
    pub register: usize,
    /// The other registry's content directory.
    pub registry: String,
    /// The generation calls that it reaches by direct calls.
    pub sites: BTreeSet<u64>,
    /// Every body of the other registry's item constructor that takes the key.
    pub constructors: Vec<KeyConstructor>,
}

/// A function that the engine runs for a registry's items.
pub struct Root {
    pub function: u64,
    pub receiver: Receiver,
    /// The generation calls that the root reaches by direct calls.
    pub sites: BTreeSet<u64>,
}

/// The engine's modifier definition table, and the category mask of each declared type.
pub struct Definitions {
    /// Address of the table's array header.
    pub table: u64,
    pub data_offset: u64,
    pub stride: u64,
    pub mask_offset: u64,
    pub masks: BTreeMap<u64, u64>,
}

/// An item constructor that takes the key and one int, and the argument register (`x1` or `x2`)
/// that holds the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyConstructor {
    pub address: u64,
    pub key_register: usize,
}

/// Constructor code and string behavior needed to locate an item's key independently of
/// modifier generation.
pub struct KeyStorageInput {
    pub constructors: Vec<KeyConstructor>,
    pub strings: StringFunctions,
    pub layout: StringLayout,
    pub code: Code,
    pub data: ReadOnlyData,
}

/// The key's offset in an item. Every constructor body must establish the same place.
pub fn item_key_offset(input: &KeyStorageInput) -> Result<u64, Unresolved> {
    key_storage(
        &input.code,
        &input.data,
        &input.strings,
        input.layout,
        &input.constructors,
    )
}

pub(super) fn key_storage(
    code: &Code,
    data: &ReadOnlyData,
    strings: &StringFunctions,
    layout: StringLayout,
    constructors: &[KeyConstructor],
) -> Result<u64, Unresolved> {
    if constructors.is_empty() {
        return Err(Unresolved::new("constructor"));
    }
    let offsets: BTreeSet<_> = constructors
        .iter()
        .map(|&constructor| key_offset(code, data, strings, layout, constructor))
        .collect::<Result<_, _>>()?;
    (offsets.len() == 1)
        .then(|| *offsets.first().expect("one offset"))
        .ok_or(Unresolved::new("key-storage-ambiguous"))
}

/// Where a database holds its items: a pointer array and its count.
#[derive(Debug, Clone, Copy)]
pub struct DatabaseLayout {
    pub items_offset: u64,
    pub count_offset: u64,
}

/// What the method established for one registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyResult {
    /// The key's offset in the item, or why the constructor run did not establish it. Without it
    /// no root is run.
    pub key_offset: Result<u64, Unresolved>,
    /// Each family, once, in template order.
    pub families: Vec<Family>,
    /// How many registered names, or generation calls that a root reaches, could not be
    /// followed, by reason.
    pub failures: BTreeMap<&'static str, usize>,
}

/// The names that one chain of calls registers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Family {
    /// The parts of each name. At least one is the item key; none is unresolved.
    pub parts: Vec<Part>,
    /// The longest name in bytes that the engine's formatter keeps.
    pub limit: Option<u64>,
    /// The category mask, when every path gives one known value.
    pub mask: Option<u64>,
    pub condition: Condition,
}

/// Whether every item generates the family, from the most to the least established.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Condition {
    Always,
    /// Every path registers the family, but some only after the model assumed the text of an
    /// earlier name part that it does not follow; such a path may take branches that the engine
    /// does not.
    AfterAssumedText,
    /// A path skips the registration, or a path could not be followed.
    Unresolved,
    /// Only an item root registers it, and that the engine runs that root for every item is not
    /// established, for this reason.
    ItemRoot(NotEstablished),
}

/// Follow every root and input of the registry. `None` when it has neither.
pub fn analyze(input: &FamilyInput, registry: &RegistryInput) -> Option<FamilyResult> {
    if registry.roots.is_empty() && registry.inputs.is_empty() {
        return None;
    }

    let key_offset = key_storage(
        &registry.code,
        &input.data,
        &input.strings,
        input.layout,
        &registry.constructors,
    );
    let Ok(offset) = key_offset else {
        return Some(FamilyResult {
            key_offset,
            families: Vec::new(),
            failures: BTreeMap::new(),
        });
    };

    let item_roots: BTreeSet<u64> = registry
        .roots
        .iter()
        .filter(|root| root.receiver == Receiver::Item)
        .map(|root| root.function)
        .collect();
    // Why the engine's call of the item roots for every item is not established, when the
    // registry has an item root.
    let not_established = match &registry.loading {
        _ if item_roots.is_empty() => None,
        Some(loading) => loading::every_item(input, loading, &item_roots).err(),
        None => Some(NotEstablished::NoLoader),
    };

    let mut failures = BTreeMap::new();
    let mut entries: Vec<Entry> = registry.roots.iter().map(Entry::Root).collect();
    for second in &registry.inputs {
        let place = key_storage(
            &registry.code,
            &input.data,
            &input.strings,
            input.layout,
            &second.constructors,
        );
        match place {
            Ok(place) => entries.push(Entry::Input { second, place }),
            Err(unresolved) => *failures.entry(unresolved.reason).or_default() += 1,
        }
    }

    let mut families: BTreeMap<Template, Condition> = BTreeMap::new();
    for entry in &entries {
        let runs: Vec<Vec<PathSummary>> = entry
            .key_forms()
            .into_iter()
            .map(|forms| entry_paths(input, registry, entry, offset, forms, &Pass::Names))
            .collect();

        let (entry_families, entry_failures) = match entry {
            Entry::Root(root) => {
                let conditions = condition_runs(input, registry, entry, offset, &runs);
                run_families(&root.sites, &runs, |calls, name| {
                    condition(root, not_established.as_ref(), &conditions, calls, name)
                })
            }
            Entry::Input { second, .. } => {
                run_families(&second.sites, &runs, |_, _| Condition::Unresolved)
            }
        };
        for family in entry_families {
            let condition = families
                .entry((family.parts, family.limit, family.mask))
                .or_insert_with(|| family.condition.clone());
            if family.condition < *condition {
                *condition = family.condition;
            }
        }
        for reason in entry_failures {
            *failures.entry(reason).or_default() += 1;
        }
    }

    let families = families
        .into_iter()
        .map(|((parts, limit, mask), condition)| Family {
            parts,
            limit,
            mask,
            condition,
        })
        .collect();
    Some(FamilyResult {
        key_offset,
        families,
        failures,
    })
}

/// The form of a string object: a long string holds its text in a buffer, a short string in
/// place. The engine branches on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum KeyForm {
    Long,
    Short,
}

impl KeyForm {
    pub(super) const BOTH: [Self; 2] = [Self::Long, Self::Short];

    /// The item key's text in this form.
    pub(super) fn text(self) -> &'static str {
        match self {
            Self::Long => LONG_KEY,
            Self::Short => SHORT_KEY,
        }
    }

    /// The key text of an item of another registry in this form.
    fn registry_key_text(self) -> &'static str {
        match self {
            Self::Long => LONG_REGISTRY_KEY,
            Self::Short => SHORT_REGISTRY_KEY,
        }
    }
}

/// The key forms of one run: of the item key, and of the key of each item of another registry.
#[derive(Debug, Clone, Copy)]
struct KeyForms {
    item: KeyForm,
    registry_key: KeyForm,
}

/// Where a run starts.
enum Entry<'a> {
    Root(&'a Root),
    /// An entered function that takes an item of another registry, with the key place of that
    /// registry's item. It runs on its own for the item and one item of the other registry.
    Input {
        second: &'a SecondInput,
        place: u64,
    },
}

impl Entry<'_> {
    fn function(&self) -> u64 {
        match self {
            Self::Root(root) => root.function,
            Self::Input { second, .. } => second.function,
        }
    }

    /// The key forms of each run: one run for each form of the item key, and for an input each
    /// pair of forms, since the engine branches on each key's form.
    fn key_forms(&self) -> Vec<KeyForms> {
        let registry_keys = |item| match self {
            Self::Root(_) => vec![item],
            Self::Input { .. } => KeyForm::BOTH.to_vec(),
        };

        KeyForm::BOTH
            .into_iter()
            .flat_map(|item| {
                registry_keys(item)
                    .into_iter()
                    .map(move |registry_key| KeyForms { item, registry_key })
            })
            .collect()
    }
}

/// Write the item key into the string object at `object`, label its text, and keep the object
/// and its text known through a store to an unknown address.
fn write_key(machine: &mut Machine, layout: StringLayout, object: u64, form: KeyForm) {
    write_source(machine, layout, object, form.text(), form, ITEM_KEY);
}

/// Write `text` into the string object at `object` in `form`, label its text with the source
/// `node`, and keep the object and its text known through a store to an unknown address. A short
/// form's text must fit in the object.
pub(super) fn write_source(
    machine: &mut Machine,
    layout: StringLayout,
    object: u64,
    text: &str,
    form: KeyForm,
    node: u64,
) {
    let length = text.len() as u64;

    let (address, flag) = match form {
        KeyForm::Long => {
            let buffer = machine.allocate(length + 1);
            let capacity = (length + 1).next_multiple_of(16);
            machine.write(object, 8, buffer);
            machine.write(object + 8, 8, length);
            machine.write(object + layout.capacity_offset(), 8, capacity | 1 << 63);
            (buffer, None)
        }
        KeyForm::Short => (object, Some(length)),
    };

    for (offset, byte) in text.bytes().enumerate() {
        machine.write(address + offset as u64, 1, u64::from(byte));
    }
    machine.write(address + length, 1, 0);
    if let Some(length) = flag {
        machine.write(object + layout.flag_byte, 1, length);
    }
    machine.label(address, node);
    machine.protect(object, layout.flag_byte + 1);
    machine.protect(address, length + 1);
}

/// Run the item constructor with a long key, and find where it stores the key's buffer pointer.
fn key_offset(
    code: &Code,
    data: &ReadOnlyData,
    strings: &StringFunctions,
    layout: StringLayout,
    constructor: KeyConstructor,
) -> Result<u64, Unresolved> {
    let mut machine = Machine::new(code, data);
    let item = machine.reserve(ITEM_SPAN);
    let key = machine.allocate(layout.flag_byte + 1);
    write_key(&mut machine, layout, key, KeyForm::Long);
    machine.set_register(0, item);
    machine.set_register(1, 0);
    machine.set_register(2, 0);
    machine.set_register(constructor.key_register, key);

    let model = Model {
        functions: strings,
        layout,
        data,
        sources: Sources::key_only(LONG_KEY),
    };
    let mut arena = Arena::default();
    let paths = machine.run_paths(constructor.address, &mut |target, machine| {
        if let Some(offset) = stored_key(machine, item) {
            machine.label(FOUND_KEY, offset);
            return Ok(Call::Stop);
        }
        follow(&model, target, machine, &mut arena)
    });

    let mut offsets = BTreeSet::new();
    for path in paths {
        let found = path
            .machine
            .labelled(FOUND_KEY)
            .or_else(|| stored_key(&path.machine, item));
        match (found, ending(strings, &path.end)) {
            (Some(offset), _) => {
                offsets.insert(offset);
            }
            (None, Ending::Ignored) => {}
            (None, Ending::Returned | Ending::Failed) => {
                return Err(Unresolved::new("key-storage"));
            }
        }
    }

    match offsets.len() {
        1 => Ok(offsets.pop_first().expect("one offset")),
        0 => Err(Unresolved::new("key-storage")),
        _ => Err(Unresolved::new("key-storage-ambiguous")),
    }
}

/// The offset in the item of a word that points at the labelled key text. Only words below
/// `METHOD_LABELS` can be text addresses.
fn stored_key(machine: &Machine, item: u64) -> Option<u64> {
    machine
        .known_words()
        .into_iter()
        .filter(|(address, value)| {
            (item..item + ITEM_SPAN).contains(address)
                && *value < METHOD_LABELS
                && machine.labelled(*value) == Some(ITEM_KEY)
        })
        .map(|(address, _)| address - item)
        .next()
}

/// The value that every returned path of every body of the item constructor, with the key in
/// either form, leaves in each of the 4-byte item words at `fields`. A word has none when a path
/// fails, no path returns, or the paths disagree. The constructor's int argument stays unknown.
fn constructor_defaults(
    input: &FamilyInput,
    registry: &RegistryInput,
    fields: &BTreeSet<u64>,
) -> BTreeMap<u64, u64> {
    let mut defaults: Option<BTreeMap<u64, Option<u64>>> = None;
    for &constructor in &registry.constructors {
        for form in KeyForm::BOTH {
            let mut machine = Machine::new(&registry.code, &input.data);
            let item = machine.reserve(ITEM_SPAN);
            let key = machine.allocate(input.layout.flag_byte + 1);
            write_key(&mut machine, input.layout, key, form);
            machine.set_register(0, item);
            machine.set_register(constructor.key_register, key);

            let model = Model {
                functions: &input.strings,
                layout: input.layout,
                data: &input.data,
                sources: Sources::key_only(form.text()),
            };
            let mut arena = Arena::default();
            let paths = machine.run_paths(constructor.address, &mut |target, machine| {
                follow(&model, target, machine, &mut arena)
            });

            let mut returned = false;
            for path in &paths {
                match ending(&input.strings, &path.end) {
                    Ending::Returned => returned = true,
                    Ending::Ignored => continue,
                    Ending::Failed => return BTreeMap::new(),
                }
                let left: BTreeMap<u64, Option<u64>> = fields
                    .iter()
                    .map(|&field| (field, path.machine.read(item + field, 4)))
                    .collect();
                let agreed = defaults.get_or_insert_with(|| left.clone());
                for (field, value) in agreed.iter_mut() {
                    if *value != left[field] {
                        *value = None;
                    }
                }
            }
            if !returned {
                return BTreeMap::new();
            }
        }
    }

    defaults
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(field, value)| Some((field, value?)))
        .collect()
}

fn model<'a>(
    input: &'a FamilyInput,
    form: KeyForm,
    registry_keys: &'a BTreeMap<String, String>,
) -> Model<'a> {
    Model {
        functions: &input.strings,
        layout: input.layout,
        data: &input.data,
        sources: Sources {
            registry_keys,
            ..Sources::key_only(form.text())
        },
    }
}

/// Let the string model follow a call. A call that it does not follow returns an unknown value.
fn follow(
    model: &Model,
    target: Option<u64>,
    machine: &mut Machine,
    arena: &mut Arena,
) -> Result<Call, Unresolved> {
    Ok(match model.call(target, machine, arena)? {
        Effect::Followed(call) => call,
        Effect::Other => Call::Return(None),
    })
}

/// How one path ended, for the generation condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ending {
    Returned,
    /// A trap or a function that never returns: the path does not finish the loop. A path that
    /// arrived at a loop head in a state that another path covers is also ignored: the covering
    /// path goes on for it.
    Ignored,
    Failed,
}

pub(super) fn ending(strings: &StringFunctions, end: &Result<Exit, Unresolved>) -> Ending {
    match end {
        Ok(Exit::Returned) => Ending::Returned,
        Ok(Exit::Trapped | Exit::Looped) => Ending::Ignored,
        Ok(Exit::Stopped(target)) if strings.never_return.contains(target) => Ending::Ignored,
        Ok(Exit::Stopped(_) | Exit::Reached) | Err(_) => Ending::Failed,
    }
}

/// Whether, in every run, no path failed, some path returned, and every returned path `holds`.
pub(super) fn every_returned_path<P>(
    runs: &[Vec<P>],
    ending: impl Fn(&P) -> Ending,
    holds: impl Fn(&P) -> bool,
) -> bool {
    runs.iter().all(|paths| {
        let mut returned = paths
            .iter()
            .filter(|path| ending(path) == Ending::Returned)
            .peekable();
        !paths.iter().any(|path| ending(path) == Ending::Failed)
            && returned.peek().is_some()
            && returned.all(&holds)
    })
}

/// One registration: the calls that the path was inside and the call itself, the name, and
/// the category mask.
type Record = (Vec<u64>, Node, Option<u64>);

/// The masks that one run's paths give each name registered at one chain of calls.
type MasksByName<'a> = BTreeMap<&'a Node, BTreeSet<Option<u64>>>;

/// A family's parts, name bound and mask.
type Template = (Vec<Part>, Option<u64>, Option<u64>);

/// One path of a root run: how it ended, and each registration that it made.
struct PathSummary {
    ending: Ending,
    records: BTreeSet<Record>,
    /// The chain and name of each registration made before the path first assumed text: only
    /// these show that the path registers them.
    established: BTreeSet<(Vec<u64>, Node)>,
    /// The offset of each item word where the path stored a modifier type that one of its own
    /// registrations created.
    cache_fields: BTreeSet<u64>,
}

/// What a run of an entry is for.
enum Pass<'a> {
    /// The names, masks and limits: every path, without joins.
    Names,
    /// The condition of a root's families: states join at loop heads, a path ends once it has
    /// registered each of `names` at its chain of calls, and each cache field starts at its
    /// default.
    Condition {
        names: BTreeSet<(Vec<u64>, Node)>,
        defaults: &'a BTreeMap<u64, u64>,
    },
}

/// The condition runs of a root `entry`, one for each key form, from its name runs `runs`.
fn condition_runs(
    input: &FamilyInput,
    registry: &RegistryInput,
    entry: &Entry,
    key_offset: u64,
    runs: &[Vec<PathSummary>],
) -> Vec<Vec<PathSummary>> {
    let fields: BTreeSet<u64> = runs
        .iter()
        .flatten()
        .flat_map(|path| path.cache_fields.iter().copied())
        .collect();
    let defaults = if fields.is_empty() {
        BTreeMap::new()
    } else {
        constructor_defaults(input, registry, &fields)
    };
    entry
        .key_forms()
        .into_iter()
        .zip(runs)
        .map(|(forms, paths)| {
            let names = paths
                .iter()
                .flat_map(|path| &path.records)
                .map(|(calls, name, _)| (calls.clone(), name.clone()))
                .collect();
            let pass = Pass::Condition {
                names,
                defaults: &defaults,
            };
            entry_paths(input, registry, entry, key_offset, forms, &pass)
        })
        .collect()
}

/// One registration call of a root run, as the run records it.
struct Registration {
    /// The calls that the path was inside, then the registration call itself.
    calls: Vec<u64>,
    /// The name's node in the run's arena.
    name: u64,
    /// The category mask.
    mask: Option<u64>,
    /// The path had assumed text before the call.
    assumed_text: bool,
}

/// Run `entry` over one item whose key has the item form of `forms`, and summarize each path. An
/// input entry also receives one item of the other registry, whose key has the other form. A root
/// run does not enter an input: the input runs on its own.
fn entry_paths(
    input: &FamilyInput,
    registry: &RegistryInput,
    entry: &Entry,
    key_offset: u64,
    forms: KeyForms,
    pass: &Pass,
) -> Vec<PathSummary> {
    let form = forms.item;
    let mut machine = Machine::new(&registry.code, &input.data);
    let table = input
        .definitions
        .as_ref()
        .map(|definitions| DefinitionTable::install(&mut machine, definitions));
    let item = machine.reserve(ITEM_SPAN);
    write_key(&mut machine, input.layout, item + key_offset, form);
    if let Pass::Condition { defaults, .. } = pass {
        for (&field, &value) in *defaults {
            machine.write(item + field, 4, value);
        }
    }

    let mut arena = Arena::default();
    let mut registry_keys = BTreeMap::new();
    let receiver = match entry {
        Entry::Root(root) => match root.receiver {
            Receiver::Item => item,
            Receiver::Database => {
                let database = machine.allocate(DATABASE_SPAN);
                let items = machine.allocate(8);
                machine.write(database + input.database.items_offset, 8, items);
                machine.write(database + input.database.count_offset, 4, 1);
                machine.write(items, 8, item);
                database
            }
        },
        Entry::Input { second, place } => {
            let node = arena.add(Node {
                parts: vec![Part::RegistryKey(second.registry.clone())],
                limit: None,
            });
            let other = machine.reserve(ITEM_SPAN);
            let text = forms.registry_key.registry_key_text();
            write_source(
                &mut machine,
                input.layout,
                other + place,
                text,
                forms.registry_key,
                node,
            );
            machine.set_register(second.register, other);
            registry_keys.insert(second.registry.clone(), text.to_owned());
            item
        }
    };
    machine.set_register(0, receiver);

    let model = model(input, form, &registry_keys);
    let mut records: Vec<Registration> = Vec::new();
    let is_input = |target| {
        registry
            .inputs
            .iter()
            .any(|second| second.function == target)
    };
    let mut calls = |target: Option<u64>, machine: &mut Machine| match target {
        Some(target) if target == input.registration => {
            let site = machine.known_register(30, "call-site")? - 4;
            let mut calls: Vec<u64> = machine.entered_calls().collect();
            calls.push(site);
            let name = model.object_node(machine, machine.register(1), &mut arena);
            let mask = machine.read(machine.stack_pointer() + input.category_offset, 4);
            let assumed_text = machine.labelled(ASSUMED_TEXT).is_some();
            records.push(Registration {
                calls,
                name,
                mask,
                assumed_text,
            });

            // The path's own registrations so far, so forked paths never share a new type.
            let count = machine.labelled(RECORD_COUNT).unwrap_or(0);
            if let (Some(table), Some(argument)) = (&table, machine.register(0)) {
                table.register(machine, argument, count, mask);
            }
            machine.label(RECORD_COUNT + 1 + count, records.len() as u64 - 1);
            machine.label(RECORD_COUNT, count + 1);

            if let Pass::Condition { names, .. } = pass {
                let made: BTreeSet<(Vec<u64>, Node)> = (0..=count)
                    .filter_map(|index| machine.labelled(RECORD_COUNT + 1 + index))
                    .map(|record| &records[record as usize])
                    .map(|made| (made.calls.clone(), arena.node(made.name).clone()))
                    .collect();
                if names.is_subset(&made) {
                    return Ok(Call::Stop);
                }
            }
            Ok(Call::Return(Some(1)))
        }
        Some(target) if registry.entered.contains(&target) && !is_input(target) => Ok(Call::Enter),
        _ => follow(&model, target, machine, &mut arena),
    };
    let paths = match pass {
        Pass::Names => machine.run_paths(entry.function(), &mut calls),
        Pass::Condition { .. } => {
            machine.set_path_limit(CONDITION_PATH_LIMIT);
            machine.run_paths_joining(entry.function(), &mut calls)
        }
    };
    let first_dynamic = table.as_ref().map(|table| table.first_dynamic);

    paths
        .into_iter()
        .map(|path| {
            let count = path.machine.labelled(RECORD_COUNT).unwrap_or(0);
            let made: Vec<_> = (0..count)
                .filter_map(|index| path.machine.labelled(RECORD_COUNT + 1 + index))
                .map(|record| &records[record as usize])
                .collect();
            let ending = match &path.end {
                Ok(Exit::Stopped(target)) if *target == input.registration => Ending::Returned,
                end => ending(&input.strings, end),
            };
            let cache_fields = match (first_dynamic, ending) {
                (Some(first), Ending::Returned) => {
                    stored_types(&path.machine, item, &(first..first + count))
                }
                _ => BTreeSet::new(),
            };
            PathSummary {
                ending,
                cache_fields,
                records: made
                    .iter()
                    .map(|registration| {
                        let name = arena.node(registration.name).clone();
                        (registration.calls.clone(), name, registration.mask)
                    })
                    .collect(),
                established: made
                    .iter()
                    .filter(|registration| !registration.assumed_text)
                    .map(|registration| {
                        let name = arena.node(registration.name).clone();
                        (registration.calls.clone(), name)
                    })
                    .collect(),
            }
        })
        .collect()
}

/// The offset of each 4-byte word of the item at `item` that holds one of `types`.
fn stored_types(machine: &Machine, item: u64, types: &std::ops::Range<u64>) -> BTreeSet<u64> {
    (0..ITEM_SPAN)
        .step_by(4)
        .filter(|&offset| {
            machine
                .read(item + offset, 4)
                .is_some_and(|value| types.contains(&value))
        })
        .collect()
}

/// The definition array that a run holds for the engine's table.
struct DefinitionTable<'a> {
    definitions: &'a Definitions,
    array: u64,
    /// The type that a path's first registration creates.
    first_dynamic: u64,
}

impl<'a> DefinitionTable<'a> {
    /// Install a definition array in `machine`: write its address into the table's header, and
    /// each declared type's category mask into its definition. The array has room for
    /// [`DYNAMIC_TYPES`] more types. Every other field is unknown. A store to an unknown address
    /// is taken not to change a definition's category mask.
    fn install(machine: &mut Machine, definitions: &'a Definitions) -> Self {
        let first_dynamic = definitions
            .masks
            .keys()
            .next_back()
            .map_or(0, |last| last + 1);
        let last_type = first_dynamic + DYNAMIC_TYPES - 1;
        let length = (last_type + 1) * definitions.stride;
        let array = machine.reserve(length);
        let header = definitions.table + definitions.data_offset;
        machine.write(header, 8, array);
        machine.protect(header, 8);
        machine.protect(array, length);

        let table = Self {
            definitions,
            array,
            first_dynamic,
        };
        for (&modifier_type, &mask) in &definitions.masks {
            table.write_mask(machine, modifier_type, Some(mask));
        }
        table
    }

    /// The path's registration number `index` creates a new type: write it to the type argument
    /// at `argument`, and its mask into its definition.
    fn register(&self, machine: &mut Machine, argument: u64, index: u64, mask: Option<u64>) {
        if index >= DYNAMIC_TYPES {
            machine.forget(argument, 4);
            return;
        }
        let modifier_type = self.first_dynamic + index;
        machine.write(argument, 4, modifier_type);
        self.write_mask(machine, modifier_type, mask);
    }

    fn write_mask(&self, machine: &mut Machine, modifier_type: u64, mask: Option<u64>) {
        let field =
            self.array + modifier_type * self.definitions.stride + self.definitions.mask_offset;
        match mask {
            Some(mask) => machine.write(field, 4, mask),
            None => machine.forget(field, 4),
        }
    }
}

/// The families of one entry from its runs with each key form, and the reason of each name or
/// generation call that could not be followed. Each of `sites` must be reached. `condition` gives
/// a family's condition from its chain of calls and its name.
fn run_families(
    sites: &BTreeSet<u64>,
    runs: &[Vec<PathSummary>],
    condition: impl Fn(&[u64], &Node) -> Condition,
) -> (Vec<Family>, Vec<&'static str>) {
    let mut failures = Vec::new();

    // For each chain of calls, the names that each key form registers there, with every mask.
    let mut chains: BTreeMap<&[u64], Vec<MasksByName>> = BTreeMap::new();
    for (form, paths) in runs.iter().enumerate() {
        for (calls, name, mask) in paths.iter().flat_map(|path| &path.records) {
            let names = chains
                .entry(calls)
                .or_insert_with(|| vec![BTreeMap::new(); runs.len()]);
            names[form].entry(name).or_default().insert(*mask);
        }
    }

    for &site in sites {
        if !chains.keys().any(|calls| calls.contains(&site)) {
            failures.push("unreached");
        }
    }

    let mut families = Vec::new();
    for (calls, forms) in &chains {
        let first: BTreeSet<_> = forms[0].keys().collect();
        if forms
            .iter()
            .any(|names| names.keys().collect::<BTreeSet<_>>() != first)
        {
            failures.push("key-forms-disagree");
            continue;
        }

        for name in first {
            let masks: BTreeSet<_> = forms.iter().flat_map(|names| &names[name]).collect();
            match family(name, &masks) {
                Ok((parts, limit, mask)) => families.push(Family {
                    parts,
                    limit,
                    mask,
                    condition: condition(calls, name),
                }),
                Err(Unresolved { reason, .. }) => failures.push(reason),
            }
        }
    }

    (families, failures)
}

/// `Always` for a database root, or an item root that the engine runs for every item, when, in
/// every condition run, no path failed, some path returned, and every returned path registered
/// `name` at `calls` before it assumed any text.
fn condition(
    root: &Root,
    not_established: Option<&NotEstablished>,
    runs: &[Vec<PathSummary>],
    calls: &[u64],
    name: &Node,
) -> Condition {
    if root.receiver == Receiver::Item
        && let Some(reason) = not_established
    {
        return Condition::ItemRoot(reason.clone());
    }

    let registers = |established: bool| {
        every_returned_path(
            runs,
            |path| path.ending,
            |path| {
                if established {
                    path.established
                        .iter()
                        .any(|(at, registered)| at == calls && registered == name)
                } else {
                    path.records
                        .iter()
                        .any(|(at, registered, _)| at == calls && registered == name)
                }
            },
        )
    };

    if registers(true) {
        Condition::Always
    } else if registers(false) {
        Condition::AfterAssumedText
    } else {
        Condition::Unresolved
    }
}

/// The parts, bound and mask of one registered name, from every mask that the paths gave it.
fn family(name: &Node, masks: &BTreeSet<&Option<u64>>) -> Result<Template, Unresolved> {
    if !name.is_resolved() {
        return Err(Unresolved::new("name"));
    }
    if !name.parts.contains(&Part::ItemKey) {
        return Err(Unresolved::new("no-item-key"));
    }
    let (Some(mask), None) = (masks.first(), masks.iter().nth(1)) else {
        return Err(Unresolved::new("paths-disagree"));
    };

    Ok((name.parts.clone(), name.limit, **mask))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::decode::Instruction;

    const FROM_TEXT: u64 = 0x900;
    const APPEND_STRING: u64 = 0x904;
    const APPEND_TEXT: u64 = 0x908;
    const FORMAT: u64 = 0x90c;
    const ALLOCATE: u64 = 0x910;
    const LENGTH: u64 = 0x91c;
    const COPY: u64 = 0x920;
    const NEVER_RETURNS: u64 = 0x924;
    const APPEND_VIEW: u64 = 0x928;
    const APPEND_CHARACTER: u64 = 0x92c;
    const RESERVE: u64 = 0x934;
    const REGISTER: u64 = 0x930;
    const OTHER: u64 = 0x940;

    const CONSTRUCTOR: u64 = 0x100;
    const THUNK: u64 = 0x300;
    const GENERATOR: u64 = 0x400;
    const ITEM_ROOT: u64 = 0x600;
    const METHOD: u64 = 0x700;
    const HELPER: u64 = 0x800;
    const SIZE: u64 = 0xb00;
    /// The engine's definition table header in the authored runs.
    const TABLE: u64 = 0x6000;

    type Rows = Vec<(u64, String, String)>;

    fn rows(start: u64, lines: &[(&str, &str)]) -> Rows {
        lines
            .iter()
            .enumerate()
            .map(|(index, (operation, operands))| {
                (
                    start + index as u64 * 4,
                    (*operation).into(),
                    (*operands).into(),
                )
            })
            .collect()
    }

    /// Replace the instruction at `address`.
    fn patch(rows: &mut Rows, address: u64, operation: &str, operands: &str) {
        let row = rows.iter_mut().find(|row| row.0 == address).unwrap();
        *row = (address, operation.into(), operands.into());
    }

    /// Copies the key argument inline into a stack temporary, then into the item at `offset`
    /// through vector registers. The temporary's last eight bytes are never written, so the copy
    /// of its capacity and flag byte is unknown.
    fn constructor(offset: u64) -> Rows {
        constructor_with_key_in(offset, "x2")
    }

    /// A constructor that copies the key from the argument `key` into the item at `offset`.
    fn constructor_with_key_in(offset: u64, key: &str) -> Rows {
        constructor_at(CONSTRUCTOR, offset, key)
    }

    /// A constructor at `address` that copies the key from the argument `key` into the item at
    /// `offset`.
    fn constructor_at(address: u64, offset: u64, key: &str) -> Rows {
        rows(
            address,
            &[
                ("sub", "sp,sp,#0x40"),
                ("mov", "x20,x0"),
                ("ldrsb", &format!("w8,[{key},#0x17]")),
                ("ldr", &format!("x9,[{key}]")),
                ("cmp", "w8,#0"),
                ("csel", &format!("x22,x9,{key},lt")),
                ("mov", "x0,x22"),
                ("bl", &format!("#{LENGTH:#x}")),
                ("mov", "x23,x0"),
                ("add", "x1,x23,#1"),
                ("bl", &format!("#{ALLOCATE:#x}")),
                ("mov", "x24,x0"),
                ("str", "x24,[sp,#8]"),
                ("str", "x23,[sp,#0x10]"),
                ("str", "x23,[sp,#0x18]"),
                ("mov", "x0,x24"),
                ("mov", "x1,x22"),
                ("mov", "x2,x23"),
                ("bl", &format!("#{COPY:#x}")),
                ("ldur", "q0,[sp,#0x18]"),
                ("ldur", "q1,[sp,#8]"),
                ("stur", &format!("q0,[x20,#{:#x}]", offset + 0x10)),
                ("stur", &format!("q1,[x20,#{offset:#x}]")),
                ("bl", &format!("#{OTHER:#x}")),
                ("add", "sp,sp,#0x40"),
                ("ret", ""),
            ],
        )
    }

    /// `planet_{key}_build_speed_mult`, with the key object at item `+0x10`: build, append the
    /// key, move the object through vector registers, append the suffix, register.
    fn concatenating_generator() -> Rows {
        rows(
            GENERATOR,
            &[
                ("sub", "sp,sp,#0x80"),
                ("ldrsw", "x8,[x0,#0x54]"),
                ("cbz", "w8,#0x47c"),
                ("mov", "x21,x0"),
                ("ldr", "x27,[x0,#0x48]"),
                ("add", "x28,x27,x8,lsl#3"),
                // 0x418: the loop.
                ("ldr", "x8,[x27]"),
                ("add", "x26,x8,#0x10"),
                ("add", "x0,sp,#0x28"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#0"),
                ("bl", &format!("#{FROM_TEXT:#x}")),
                ("add", "x0,sp,#0x28"),
                ("mov", "x1,x26"),
                ("bl", &format!("#{APPEND_STRING:#x}")),
                ("ldur", "q0,[sp,#0x28]"),
                ("ldur", "q1,[sp,#0x38]"),
                ("stp", "q0,q1,[sp,#0x50]"),
                ("add", "x0,sp,#0x50"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#0x10"),
                ("bl", &format!("#{APPEND_TEXT:#x}")),
                ("mov", "w8,#0x40000000"),
                ("str", "w8,[sp]"),
                ("add", "x1,sp,#0x50"),
                // 0x464: the registration.
                ("bl", &format!("#{REGISTER:#x}")),
                ("add", "x27,x27,#8"),
                ("cmp", "x27,x28"),
                ("b.ne", "#0x418"),
                ("nop", ""),
                // 0x47c
                ("add", "sp,sp,#0x80"),
                ("ret", ""),
            ],
        )
    }

    /// `%s_empire_windup_mult` through the formatter, with the key object at item `+0x18`. The
    /// formatter takes the key's text: the object for a short key, its buffer for a long one.
    fn formatting_generator(format: u64) -> Rows {
        rows(
            GENERATOR,
            &[
                ("sub", "sp,sp,#0xc0"),
                ("ldr", "x24,[x0,#0x48]"),
                ("ldr", "x28,[x24]"),
                ("add", "x27,x28,#0x18"),
                ("ldrsb", "w9,[x28,#0x2f]"),
                ("mov", "x8,x27"),
                ("tbz", "w9,#0x1f,#0x420"),
                ("ldr", "x8,[x27]"),
                // 0x420
                ("str", "x8,[sp]"),
                ("add", "x0,sp,#0x40"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#{format}"),
                ("bl", &format!("#{FORMAT:#x}")),
                ("add", "x0,sp,#0x18"),
                ("add", "x1,sp,#0x40"),
                ("bl", &format!("#{FROM_TEXT:#x}")),
                ("mov", "w8,#0x100"),
                ("str", "w8,[sp]"),
                ("add", "x1,sp,#0x18"),
                // 0x44c: the registration.
                ("bl", &format!("#{REGISTER:#x}")),
                ("add", "sp,sp,#0xc0"),
                ("ret", ""),
            ],
        )
        .into_iter()
        .map(|(address, operation, operands)| {
            let operands = operands.replace("{format}", &format!("{format:#x}"));
            (address, operation, operands)
        })
        .collect()
    }

    /// `{key}_build_speed_mult`, with the key object at item `+0x10` copied inline into a stack
    /// string first: a short key into the object itself, a long key into a new buffer.
    fn copying_generator() -> Rows {
        rows(
            GENERATOR,
            &[
                ("sub", "sp,sp,#0x80"),
                ("ldr", "x8,[x0,#0x48]"),
                ("ldr", "x19,[x8]"),
                ("ldr", "x8,[x19,#0x10]!"),
                ("ldrsb", "w9,[x19,#0x17]"),
                ("cmp", "w9,#0"),
                ("csel", "x28,x8,x19,lt"),
                ("mov", "x0,x28"),
                ("bl", &format!("#{LENGTH:#x}")),
                ("mov", "x21,x0"),
                ("cmp", "x0,#0x16"),
                ("b.hi", "#0x43c"),
                ("strb", "w21,[sp,#0x57]"),
                ("add", "x24,sp,#0x40"),
                ("b", "#0x454"),
                // 0x43c: a long key gets a buffer.
                ("add", "x1,x21,#1"),
                ("bl", &format!("#{ALLOCATE:#x}")),
                ("mov", "x24,x0"),
                ("str", "x0,[sp,#0x40]"),
                ("str", "x21,[sp,#0x48]"),
                ("nop", ""),
                // 0x454
                ("mov", "x0,x24"),
                ("mov", "x1,x28"),
                ("mov", "x2,x21"),
                ("bl", &format!("#{COPY:#x}")),
                ("strb", "wzr,[x24,x21]"),
                ("add", "x0,sp,#0x40"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#0x10"),
                ("bl", &format!("#{APPEND_TEXT:#x}")),
                ("mov", "w8,#0x100"),
                ("str", "w8,[sp]"),
                ("add", "x1,sp,#0x40"),
                // 0x484: the registration.
                ("bl", &format!("#{REGISTER:#x}")),
                ("add", "sp,sp,#0x80"),
                ("ret", ""),
            ],
        )
    }

    fn code(parts: &[Rows]) -> Code {
        Code::from_rows(
            parts
                .iter()
                .flatten()
                .map(|(address, operation, operands)| Instruction {
                    address: *address,
                    bytes: [0; 4],
                    operation: operation.clone(),
                    operands: operands.clone(),
                }),
        )
    }

    fn strings() -> StringFunctions {
        StringFunctions {
            from_text: [FROM_TEXT].into(),
            append_string: [APPEND_STRING].into(),
            append_text: [APPEND_TEXT].into(),
            append_view: [APPEND_VIEW].into(),
            append_character: [APPEND_CHARACTER].into(),
            reserves: [RESERVE].into(),
            formatters: [(FORMAT, 128)].into(),
            allocators: [ALLOCATE].into(),
            lengths: [LENGTH].into(),
            copies: [COPY].into(),
            never_return: [NEVER_RETURNS].into(),
            ..StringFunctions::default()
        }
    }

    fn shared_input() -> FamilyInput {
        let data = ReadOnlyData::new(vec![(
            0x5000,
            b"planet_\0\0\0\0\0\0\0\0\0_build_speed_mult\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0%s_empire_windup_mult\0\0\0\0\0\0\0\0\0\0\0%d_bad\0\0\0\0\0\0\0\0\0\0pop\0\0\0\0\0\0\0\0\0\0\0\0\0max_add\0"
                .to_vec(),
        )]);
        FamilyInput {
            registration: REGISTER,
            category_offset: 0,
            database: DatabaseLayout {
                items_offset: 0x48,
                count_offset: 0x54,
            },
            definitions: None,
            strings: strings(),
            layout: StringLayout { flag_byte: 0x17 },
            data,
        }
    }

    /// One registry's code and the input that every registry shares.
    struct Fixture {
        input: FamilyInput,
        registry: RegistryInput,
    }

    impl Fixture {
        fn analyze(&self) -> Option<FamilyResult> {
            analyze(&self.input, &self.registry)
        }

        /// The one family, or the one reason why no family was followed.
        fn only_family(&self) -> Result<Family, Unresolved> {
            let result = self.analyze().unwrap();
            match (result.families.as_slice(), result.failures.len()) {
                ([family], 0) => Ok(family.clone()),
                ([], 1) => Err(Unresolved::new(result.failures.keys().next().unwrap())),
                _ => panic!("more than one outcome: {result:?}"),
            }
        }
    }

    /// The generator at `GENERATOR` as the registry's one database root.
    fn family_input(parts: &[Rows], sites: &[u64], constructor: Option<u64>) -> Fixture {
        fixture(
            parts,
            vec![Root {
                function: GENERATOR,
                receiver: Receiver::Database,
                sites: sites.iter().copied().collect(),
            }],
            &[],
            constructor,
        )
    }

    fn fixture(
        parts: &[Rows],
        roots: Vec<Root>,
        entered: &[u64],
        constructor: Option<u64>,
    ) -> Fixture {
        Fixture {
            input: shared_input(),
            registry: RegistryInput {
                roots,
                entered: entered.iter().copied().collect(),
                constructors: constructor.into_iter().map(template_constructor).collect(),
                loading: None,
                inputs: Vec::new(),
                code: code(parts),
            },
        }
    }

    fn template_constructor(address: u64) -> KeyConstructor {
        KeyConstructor {
            address,
            key_register: 2,
        }
    }

    fn literal(text: &str) -> Part {
        Part::Literal(text.into())
    }

    #[test]
    fn a_concatenated_name_is_a_template_for_every_item() {
        let input = family_input(
            &[constructor(0x10), concatenating_generator()],
            &[0x464],
            Some(CONSTRUCTOR),
        );
        let result = input.analyze().unwrap();
        assert_eq!(result.key_offset, Ok(0x10));
        assert_eq!(
            result.families,
            [Family {
                parts: vec![
                    literal("planet_"),
                    Part::ItemKey,
                    literal("_build_speed_mult")
                ],
                limit: None,
                mask: Some(0x4000_0000),
                condition: Condition::Always,
            }]
        );
        assert!(result.failures.is_empty());
    }

    #[test]
    fn a_formatted_name_keeps_the_formatter_bound() {
        let input = family_input(
            &[constructor(0x18), formatting_generator(0x30)],
            &[0x44c],
            Some(CONSTRUCTOR),
        );
        assert_eq!(
            Fixture::only_family(&input),
            Ok(Family {
                parts: vec![Part::ItemKey, literal("_empire_windup_mult")],
                limit: Some(127),
                mask: Some(0x100),
                condition: Condition::Always,
            })
        );

        let other_directive = family_input(
            &[constructor(0x18), formatting_generator(0x50)],
            &[0x44c],
            Some(CONSTRUCTOR),
        );
        assert_eq!(
            Fixture::only_family(&other_directive),
            Err(Unresolved::new("name"))
        );
    }

    /// Post-freeze revision 1: a short key copied into a stack string and then appended to must
    /// not keep the short string's label, or the two key forms disagree.
    #[test]
    fn an_inline_copy_of_either_key_form_is_the_item_key() {
        let input = family_input(
            &[constructor(0x10), copying_generator()],
            &[0x484],
            Some(CONSTRUCTOR),
        );
        assert_eq!(
            Fixture::only_family(&input),
            Ok(Family {
                parts: vec![Part::ItemKey, literal("_build_speed_mult")],
                limit: None,
                mask: Some(0x100),
                condition: Condition::Always,
            })
        );
    }

    #[test]
    fn a_branch_on_an_item_field_leaves_the_condition_unresolved() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x41c, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("add", "x26,x8,#0x10"),
                ("ldrb", "w9,[x8,#0x508]"),
                ("cbz", "w9,#0x468"),
                ("b", "#0x420"),
            ],
        ));
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        let family = Fixture::only_family(&input).unwrap();
        assert_eq!(family.condition, Condition::Unresolved);
        assert_eq!(family.mask, Some(0x4000_0000));
    }

    #[test]
    fn a_path_into_a_function_that_never_returns_is_ignored() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x478, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("ldr", "x9,[x21,#0x48]"),
                ("ldr", "x9,[x9]"),
                ("ldr", "x9,[x9,#0x200]"),
                ("cbz", "x9,#0x4b4"),
                ("bl", &format!("#{NEVER_RETURNS:#x}")),
                // 0x4b4
                ("add", "sp,sp,#0x80"),
                ("ret", ""),
            ],
        ));
        patch(&mut generator, 0x47c, "b", "#0x4a0");
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        assert_eq!(
            Fixture::only_family(&input).unwrap().condition,
            Condition::Always
        );
    }

    #[test]
    fn paths_that_disagree_on_the_mask_leave_the_call_unresolved() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x458, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("ldr", "x9,[x27]"),
                ("ldrb", "w9,[x9,#0x100]"),
                ("mov", "w8,#1"),
                ("cbz", "w9,#0x4b4"),
                ("mov", "w8,#2"),
                // 0x4b4
                ("b", "#0x45c"),
            ],
        ));
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        assert_eq!(
            Fixture::only_family(&input),
            Err(Unresolved::new("paths-disagree"))
        );
    }

    #[test]
    fn a_call_outside_the_model_leaves_the_name_unresolved() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x458, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("add", "x0,sp,#0x50"),
                ("bl", &format!("#{OTHER:#x}")),
                ("mov", "w8,#0x40000000"),
                ("b", "#0x45c"),
            ],
        ));
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        assert_eq!(Fixture::only_family(&input), Err(Unresolved::new("name")));
    }

    /// Negative control: unresolved text that the code builds before the registration is
    /// written as the empty text, so that path cannot establish that every item generates the
    /// family; the condition says that the path registers it after assumed text.
    #[test]
    fn a_path_on_assumed_text_never_establishes_always() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x458, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("add", "x0,sp,#0x60"),
                ("ldr", "x1,[x21,#0x100]"),
                ("bl", &format!("#{FROM_TEXT:#x}")),
                ("mov", "w8,#0x40000000"),
                ("b", "#0x45c"),
            ],
        ));
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        let family = Fixture::only_family(&input).unwrap();
        assert_eq!(
            family.parts,
            [
                literal("planet_"),
                Part::ItemKey,
                literal("_build_speed_mult")
            ]
        );
        assert_eq!(family.condition, Condition::AfterAssumedText);

        // The same text after the registration does not change that the item registers it.
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x468, "b", "#0x4c0");
        generator.extend(rows(
            0x4c0,
            &[
                ("add", "x0,sp,#0x60"),
                ("ldr", "x1,[x21,#0x100]"),
                ("bl", &format!("#{FROM_TEXT:#x}")),
                ("add", "x27,x27,#8"),
                ("b", "#0x46c"),
            ],
        ));
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        assert_eq!(
            Fixture::only_family(&input).unwrap().condition,
            Condition::Always
        );
    }

    /// Post-freeze revision 2: a copy of unknown length into a string may change it.
    #[test]
    fn a_copy_of_unknown_length_leaves_the_name_unresolved() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x458, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("ldr", "x0,[sp,#0x50]"),
                ("ldr", "x2,[x27]"),
                ("ldr", "x2,[x2,#0x300]"),
                ("bl", &format!("#{COPY:#x}")),
                ("mov", "w8,#0x40000000"),
                ("b", "#0x45c"),
            ],
        ));
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        assert_eq!(Fixture::only_family(&input), Err(Unresolved::new("name")));
    }

    #[test]
    fn a_name_without_the_item_key_is_not_a_family() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x434, "mov", "x1,x0");
        patch(&mut generator, 0x438, "nop", "");
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        assert_eq!(
            Fixture::only_family(&input),
            Err(Unresolved::new("no-item-key"))
        );

        let unreached = family_input(
            &[constructor(0x10), concatenating_generator()],
            &[0x464, 0x468],
            Some(CONSTRUCTOR),
        );
        let result = unreached.analyze().unwrap();
        assert_eq!(result.families.len(), 1);
        assert_eq!(result.failures, BTreeMap::from([("unreached", 1)]));
    }

    #[test]
    fn the_key_storage_is_found_through_thunks_and_failed_paths() {
        let thunk = rows(THUNK, &[("b", &format!("#{CONSTRUCTOR:#x}"))]);
        let through_thunk = family_input(
            &[constructor(0x18), thunk, concatenating_generator()],
            &[0x464],
            Some(THUNK),
        );
        assert_eq!(through_thunk.analyze().unwrap().key_offset, Ok(0x18));

        let mut failing = constructor(0x18);
        patch(&mut failing, 0x15c, "fmla", "s0,s1,s2");
        let stored_then_failed = family_input(
            &[failing, concatenating_generator()],
            &[0x464],
            Some(CONSTRUCTOR),
        );
        assert_eq!(stored_then_failed.analyze().unwrap().key_offset, Ok(0x18));

        let no_store = rows(CONSTRUCTOR, &[("ret", "")]);
        let missing = family_input(
            &[no_store, concatenating_generator()],
            &[0x464],
            Some(CONSTRUCTOR),
        );
        let result = missing.analyze().unwrap();
        assert_eq!(result.key_offset, Err(Unresolved::new("key-storage")));
        assert!(result.families.is_empty());

        let no_constructor = family_input(&[concatenating_generator()], &[0x464], None);
        assert_eq!(
            no_constructor.analyze().unwrap().key_offset,
            Err(Unresolved::new("constructor"))
        );
    }

    /// An item root keeps the item in a generator object at `+0xb8` and calls a generator
    /// method with it.
    fn item_root() -> Rows {
        rows(
            ITEM_ROOT,
            &[
                ("sub", "sp,sp,#0xc0"),
                ("str", "x0,[sp,#0xb8]"),
                ("mov", "x0,sp"),
                ("bl", &format!("#{METHOD:#x}")),
                ("add", "sp,sp,#0xc0"),
                ("ret", ""),
            ],
        )
    }

    /// Reads the key object of the generator's item, its length through a leaf, and calls the
    /// helper with base type `0x19` and the views `pop`, the key and `max_add`.
    fn generator_method() -> Rows {
        rows(
            METHOD,
            &[
                ("sub", "sp,sp,#0x20"),
                ("ldr", "x8,[x0,#0xb8]"),
                ("add", "x9,x8,#0x10"),
                ("str", "x9,[sp,#0x10]"),
                ("mov", "x0,x9"),
                ("bl", &format!("#{SIZE:#x}")),
                ("mov", "x4,x0"),
                ("ldr", "x9,[sp,#0x10]"),
                ("ldrsb", "w10,[x9,#0x17]"),
                ("ldr", "x11,[x9]"),
                ("cmp", "w10,#0"),
                ("csel", "x3,x11,x9,lt"),
                ("mov", "w0,#0x19"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#0x60"),
                ("mov", "x2,#3"),
                ("adrp", "x5,#0x5000"),
                ("add", "x5,x5,#0x70"),
                ("mov", "x6,#7"),
                ("bl", &format!("#{HELPER:#x}")),
                ("add", "sp,sp,#0x20"),
                ("ret", ""),
            ],
        )
    }

    /// `CString::GetSize`: a leaf.
    fn size() -> Rows {
        rows(
            SIZE,
            &[
                ("ldrsb", "w8,[x0,#0x17]"),
                ("tbnz", "w8,#0x1f,#0xb10"),
                ("and", "x0,x8,#0xff"),
                ("ret", ""),
                ("ldr", "x0,[x0,#8]"),
                ("ret", ""),
            ],
        )
    }

    /// Like `CModifierGeneratorBase::GenerateFrom`: the mask of the base type from the
    /// definition table, the name `prefix_key_suffix` from three views, the registration, and
    /// a call that the model does not follow with the new type's definition.
    fn helper() -> Rows {
        rows(
            HELPER,
            &[
                ("sub", "sp,sp,#0x80"),
                ("stp", "x3,x4,[sp,#0x50]"),
                ("stp", "x5,x6,[sp,#0x60]"),
                ("stp", "x1,x2,[sp,#0x70]"),
                ("adrp", "x8,#0x6000"),
                ("ldr", "x8,[x8,#8]"),
                ("mov", "x9,#0x98"),
                ("madd", "x8,x0,x9,x8"),
                ("ldr", "w10,[x8,#0x84]"),
                ("str", "w10,[sp]"),
                ("stp", "xzr,xzr,[sp,#0x28]"),
                ("stp", "xzr,xzr,[sp,#0x38]"),
                ("add", "x0,sp,#0x28"),
                ("ldp", "x1,x2,[sp,#0x70]"),
                ("bl", &format!("#{APPEND_VIEW:#x}")),
                ("add", "x0,sp,#0x28"),
                ("mov", "w1,#0x5f"),
                ("bl", &format!("#{APPEND_CHARACTER:#x}")),
                ("add", "x0,sp,#0x28"),
                ("ldp", "x1,x2,[sp,#0x50]"),
                ("bl", &format!("#{APPEND_VIEW:#x}")),
                ("add", "x0,sp,#0x28"),
                ("mov", "w1,#0x5f"),
                ("bl", &format!("#{APPEND_CHARACTER:#x}")),
                ("add", "x0,sp,#0x28"),
                ("ldp", "x1,x2,[sp,#0x60]"),
                ("bl", &format!("#{APPEND_VIEW:#x}")),
                ("add", "x0,sp,#0x14"),
                ("mov", "w8,#0x23b"),
                ("str", "w8,[sp,#0x14]"),
                ("add", "x1,sp,#0x28"),
                ("bl", &format!("#{REGISTER:#x}")),
                ("ldrsw", "x8,[sp,#0x14]"),
                ("adrp", "x9,#0x6000"),
                ("ldr", "x9,[x9,#8]"),
                ("mov", "x10,#0x98"),
                ("madd", "x0,x8,x10,x9"),
                ("add", "x0,x0,#0x28"),
                ("ldr", "x1,[sp,#0x50]"),
                ("bl", &format!("#{OTHER:#x}")),
                ("add", "sp,sp,#0x80"),
                ("ret", ""),
            ],
        )
    }

    fn definitions() -> Definitions {
        Definitions {
            table: TABLE,
            data_offset: 8,
            stride: 0x98,
            mask_offset: 0x84,
            masks: BTreeMap::from([(0x19, 2)]),
        }
    }

    fn item_fixture(method: Rows, entered: &[u64]) -> Fixture {
        let mut fixture = fixture(
            &[constructor(0x10), item_root(), method, size(), helper()],
            vec![Root {
                function: ITEM_ROOT,
                receiver: Receiver::Item,
                sites: BTreeSet::from([0x87c]),
            }],
            entered,
            Some(CONSTRUCTOR),
        );
        fixture.input.definitions = Some(definitions());
        fixture
    }

    #[test]
    fn an_item_root_follows_a_helper_through_views_and_a_base_type() {
        let fixture = item_fixture(generator_method(), &[METHOD, HELPER, SIZE]);
        assert_eq!(
            fixture.only_family(),
            Ok(Family {
                parts: vec![literal("pop_"), Part::ItemKey, literal("_max_add")],
                limit: None,
                mask: Some(2),
                condition: Condition::ItemRoot(NotEstablished::NoLoader),
            })
        );

        let mut no_table = item_fixture(generator_method(), &[METHOD, HELPER, SIZE]);
        no_table.input.definitions = None;
        assert_eq!(no_table.only_family().unwrap().mask, None);

        let not_entered = item_fixture(generator_method(), &[METHOD, SIZE]);
        assert_eq!(
            not_entered.analyze().unwrap().failures,
            BTreeMap::from([("unreached", 1)])
        );
    }

    /// Negative control: a view whose length is not the length of its text is not that text.
    #[test]
    fn a_view_shorter_than_its_text_leaves_the_name_unresolved() {
        let mut method = generator_method();
        patch(&mut method, 0x73c, "mov", "x2,#2");
        let fixture = item_fixture(method, &[METHOD, HELPER, SIZE]);
        assert_eq!(fixture.only_family(), Err(Unresolved::new("name")));
    }

    /// Negative control: a name part from an object other than the item, such as the key of a
    /// second content object, never becomes a one-key family.
    #[test]
    fn a_part_from_another_object_is_not_the_item_key() {
        let mut method = generator_method();
        patch(&mut method, 0x708, "ldr", "x9,[x8,#0x200]");
        let fixture = item_fixture(method, &[METHOD, HELPER, SIZE]);
        assert_eq!(fixture.only_family(), Err(Unresolved::new("name")));
    }

    /// A call that the model does not follow receives the key object, and a store goes through
    /// an unknown pointer, before the method reads the key. The key is still the item key.
    #[test]
    fn the_item_key_survives_unknown_stores_and_calls() {
        let method = rows(
            METHOD,
            &[
                ("sub", "sp,sp,#0x20"),
                ("ldr", "x19,[x0,#0xb8]"),
                ("add", "x0,x19,#0x10"),
                ("bl", &format!("#{OTHER:#x}")),
                ("ldr", "x12,[x19,#0x300]"),
                ("str", "x12,[x12]"),
                ("add", "x0,x19,#0x10"),
                ("bl", &format!("#{SIZE:#x}")),
                ("mov", "x4,x0"),
                ("add", "x9,x19,#0x10"),
                ("ldrsb", "w10,[x9,#0x17]"),
                ("ldr", "x11,[x9]"),
                ("cmp", "w10,#0"),
                ("csel", "x3,x11,x9,lt"),
                ("mov", "w0,#0x19"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#0x60"),
                ("mov", "x2,#3"),
                ("adrp", "x5,#0x5000"),
                ("add", "x5,x5,#0x70"),
                ("mov", "x6,#7"),
                ("bl", &format!("#{HELPER:#x}")),
                ("add", "sp,sp,#0x20"),
                ("ret", ""),
            ],
        );
        let fixture = item_fixture(method, &[METHOD, HELPER, SIZE]);
        assert_eq!(
            fixture.only_family(),
            Ok(Family {
                parts: vec![literal("pop_"), Part::ItemKey, literal("_max_add")],
                limit: None,
                mask: Some(2),
                condition: Condition::ItemRoot(NotEstablished::NoLoader),
            })
        );
    }

    /// Each path creates its own modifier types: two sibling paths that register once both get
    /// the first new type, so both register the second family.
    #[test]
    fn forked_paths_create_their_own_modifier_types() {
        let generator = rows(
            GENERATOR,
            &[
                ("sub", "sp,sp,#0x80"),
                ("ldr", "x8,[x0,#0x48]"),
                ("ldr", "x19,[x8]"),
                ("ldrb", "w9,[x19,#0x500]"),
                ("cbz", "w9,#0x418"),
                ("nop", ""),
                // 0x418
                ("add", "x0,sp,#0x20"),
                ("add", "x1,x19,#0x10"),
                ("str", "wzr,[sp]"),
                ("bl", &format!("#{REGISTER:#x}")),
                ("ldr", "w8,[sp,#0x20]"),
                ("cbnz", "w8,#0x45c"),
                ("add", "x0,sp,#0x40"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#0"),
                ("bl", &format!("#{FROM_TEXT:#x}")),
                ("add", "x0,sp,#0x40"),
                ("add", "x1,x19,#0x10"),
                ("bl", &format!("#{APPEND_STRING:#x}")),
                ("add", "x0,sp,#0x20"),
                ("add", "x1,sp,#0x40"),
                ("str", "wzr,[sp]"),
                // 0x458
                ("bl", &format!("#{REGISTER:#x}")),
                // 0x45c
                ("add", "sp,sp,#0x80"),
                ("ret", ""),
            ],
        );
        let mut fixture = family_input(
            &[constructor(0x10), generator],
            &[0x424, 0x458],
            Some(CONSTRUCTOR),
        );
        fixture.input.definitions = Some(Definitions {
            masks: BTreeMap::new(),
            ..definitions()
        });
        let families: Vec<_> = fixture
            .analyze()
            .unwrap()
            .families
            .into_iter()
            .map(|family| (family.parts, family.condition))
            .collect();
        assert_eq!(
            families,
            [
                (vec![literal("planet_"), Part::ItemKey], Condition::Always),
                (vec![Part::ItemKey], Condition::Always),
            ]
        );
    }

    /// One registration call in a loop over a static table registers one family per entry.
    #[test]
    fn a_loop_over_a_static_table_gives_a_family_per_entry() {
        let mut generator = concatenating_generator();
        // Replace the suffix with each of two literals: the loop at 0x4a0 runs twice.
        patch(&mut generator, 0x44c, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("mov", "x22,#0"),
                // 0x4a4
                ("add", "x0,sp,#0x50"),
                ("stp", "xzr,xzr,[x0]"),
                ("stp", "xzr,xzr,[x0,#0x10]"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#0x60"),
                ("cbz", "x22,#0x4c0"),
                ("add", "x1,x1,#0x10"),
                // 0x4c0
                ("bl", &format!("#{APPEND_TEXT:#x}")),
                ("add", "x0,sp,#0x50"),
                ("mov", "x1,x26"),
                ("bl", &format!("#{APPEND_STRING:#x}")),
                ("mov", "w8,#0x40000000"),
                ("str", "w8,[sp]"),
                ("add", "x1,sp,#0x50"),
                // 0x4dc: the registration.
                ("bl", &format!("#{REGISTER:#x}")),
                ("add", "x22,x22,#1"),
                ("cmp", "x22,#2"),
                ("b.ne", "#0x4a4"),
                ("b", "#0x468"),
            ],
        ));
        let input = family_input(&[constructor(0x10), generator], &[0x4dc], Some(CONSTRUCTOR));
        let result = input.analyze().unwrap();
        let templates: Vec<_> = result
            .families
            .iter()
            .map(|family| (family.parts.clone(), family.condition.clone()))
            .collect();
        assert_eq!(
            templates,
            [
                (vec![literal("max_add"), Part::ItemKey], Condition::Always),
                (vec![literal("pop"), Part::ItemKey], Condition::Always),
            ]
        );
        assert!(result.failures.is_empty());
    }

    /// Loading code that constructs one item and calls the item root with it.
    fn loading() -> Loading {
        const LOADER: u64 = 0x1000;
        const DATABASE_CONSTRUCTOR: u64 = 0x1100;
        const ALLOCATE_ITEM: u64 = 0x950;
        let loader = rows(
            LOADER,
            &[
                ("stp", "x19,x30,[sp,#-0x10]!"),
                ("bl", &format!("#{ALLOCATE_ITEM:#x}")),
                ("mov", "x19,x0"),
                ("bl", &format!("#{CONSTRUCTOR:#x}")),
                ("mov", "x0,x19"),
                ("bl", &format!("#{ITEM_ROOT:#x}")),
                ("ldp", "x19,x30,[sp],#0x10"),
                ("ret", ""),
            ],
        );
        let database_constructor = rows(
            DATABASE_CONSTRUCTOR,
            &[("str", "wzr,[x0,#0x70]"), ("ret", "")],
        );
        let function = |address, name: &str| loading::Function {
            address,
            name: name.into(),
            pointers: Vec::new(),
        };
        Loading {
            loaders: vec![function(LOADER, "CDb::Load()")],
            database_constructors: vec![function(DATABASE_CONSTRUCTOR, "CDb::CDb()")],
            database_loops: Vec::new(),
            elsewhere: Vec::new(),
            inline_constructions: Vec::new(),
            constructors: [CONSTRUCTOR].into(),
            vtables: [(0, 0x5000)].into(),
            thunks: BTreeMap::new(),
            dispatch: BTreeSet::new(),
            allocations: [ALLOCATE_ITEM].into(),
            code: code(&[loader, database_constructor]),
        }
    }

    /// With the call of the item root established for every item, its families follow the
    /// rule of a database root: `Always` when every path registers them, `Unresolved` when a
    /// branch on an item field skips the registration.
    #[test]
    fn an_item_root_that_runs_for_every_item_can_be_always() {
        let mut every_item = item_fixture(generator_method(), &[METHOD, HELPER, SIZE]);
        every_item.registry.loading = Some(loading());
        let result = every_item.analyze().unwrap();
        assert_eq!(result.families[0].condition, Condition::Always);

        let gated_root = rows(
            ITEM_ROOT,
            &[
                ("sub", "sp,sp,#0xc0"),
                ("ldr", "w8,[x0,#0x200]"),
                ("cbz", "w8,#0x618"),
                ("str", "x0,[sp,#0xb8]"),
                ("mov", "x0,sp"),
                ("bl", &format!("#{METHOD:#x}")),
                ("add", "sp,sp,#0xc0"),
                ("ret", ""),
            ],
        );
        let mut gated = fixture(
            &[
                constructor(0x10),
                gated_root,
                generator_method(),
                size(),
                helper(),
            ],
            vec![Root {
                function: ITEM_ROOT,
                receiver: Receiver::Item,
                sites: BTreeSet::from([0x87c]),
            }],
            &[METHOD, HELPER, SIZE],
            Some(CONSTRUCTOR),
        );
        gated.input.definitions = Some(definitions());
        gated.registry.loading = Some(loading());
        let result = gated.analyze().unwrap();
        assert_eq!(result.families[0].condition, Condition::Unresolved);

        let mut no_loading = item_fixture(generator_method(), &[METHOD, HELPER, SIZE]);
        no_loading.registry.loading = None;
        let result = no_loading.analyze().unwrap();
        assert_eq!(
            result.families[0].condition,
            Condition::ItemRoot(NotEstablished::NoLoader)
        );
    }

    /// A template that a database root establishes for every item stays `Always` when an item
    /// root registers it too; the item root alone never gives `Always`.
    #[test]
    fn conditions_are_established_per_root() {
        let mut both = family_input(
            &[constructor(0x10), concatenating_generator()],
            &[0x464],
            Some(CONSTRUCTOR),
        );
        both.registry.roots.push(Root {
            function: GENERATOR + 0x18,
            receiver: Receiver::Item,
            sites: BTreeSet::from([0x464]),
        });
        let conditions: Vec<_> = both
            .analyze()
            .unwrap()
            .families
            .iter()
            .map(|family| family.condition.clone())
            .collect();
        assert_eq!(conditions, [Condition::Always]);

        both.registry.roots.remove(0);
        let item_only = both.analyze().unwrap();
        assert!(
            item_only
                .families
                .iter()
                .all(|family| family.condition == Condition::ItemRoot(NotEstablished::NoLoader))
        );
    }

    #[test]
    fn live_key_storage_uses_each_item_constructors_offset() {
        let fixture = family_input(&[constructor(0x18)], &[], None);
        let key = KeyStorageInput {
            constructors: vec![template_constructor(CONSTRUCTOR)],
            strings: fixture.input.strings,
            layout: fixture.input.layout,
            code: fixture.registry.code,
            data: fixture.input.data,
        };
        assert_eq!(item_key_offset(&key), Ok(0x18));

        let missing = KeyStorageInput {
            constructors: Vec::new(),
            ..key
        };
        assert_eq!(
            item_key_offset(&missing),
            Err(Unresolved::new("constructor"))
        );
    }

    #[test]
    fn a_constructor_that_takes_the_key_first_reads_it_from_x1() {
        let fixture = family_input(&[constructor_with_key_in(0x30, "x1")], &[], None);
        let key = |key_register| KeyStorageInput {
            constructors: vec![KeyConstructor {
                address: CONSTRUCTOR,
                key_register,
            }],
            strings: fixture.input.strings.clone(),
            layout: fixture.input.layout,
            code: fixture.registry.code.clone(),
            data: fixture.input.data.clone(),
        };

        assert_eq!(item_key_offset(&key(1)), Ok(0x30));
        assert!(item_key_offset(&key(2)).is_err());
    }

    /// The constructor of the other registry's item, with the key first and stored at `+0x30`.
    const OTHER_CONSTRUCTOR: u64 = 0xd00;
    /// A function of the item's class that takes an item of the other registry in `x1`.
    const MATRIX: u64 = 0xc00;
    const OTHER_REGISTRY: &str = "common/other";

    /// An item root that calls [`MATRIX`] with an item of the other registry that it does not
    /// know.
    fn matrix_root() -> Rows {
        rows(
            ITEM_ROOT,
            &[
                ("sub", "sp,sp,#0x10"),
                ("bl", &format!("#{MATRIX:#x}")),
                ("add", "sp,sp,#0x10"),
                ("ret", ""),
            ],
        )
    }

    /// `{key}_{other key}_build_speed_mult`, like `CEconomicCategory::FillResourceModifierMatrix`,
    /// and the address of its generation call. With `mixed`, the name ends in `max_add` instead
    /// when exactly one of the two keys is short.
    fn matrix(mixed: bool) -> (Rows, u64) {
        let mut lines: Vec<(&str, String)> = vec![
            ("sub", "sp,sp,#0x80".into()),
            ("mov", "x19,x0".into()),
            ("mov", "x20,x1".into()),
            ("stp", "xzr,xzr,[sp,#0x28]".into()),
            ("stp", "xzr,xzr,[sp,#0x38]".into()),
            ("add", "x0,sp,#0x28".into()),
            ("add", "x1,x19,#0x10".into()),
            ("bl", format!("#{APPEND_STRING:#x}")),
            ("add", "x0,sp,#0x28".into()),
            ("mov", "w1,#0x5f".into()),
            ("bl", format!("#{APPEND_CHARACTER:#x}")),
            ("add", "x0,sp,#0x28".into()),
            ("add", "x1,x20,#0x30".into()),
            ("bl", format!("#{APPEND_STRING:#x}")),
            ("adrp", "x1,#0x5000".into()),
            ("add", "x1,x1,#0x10".into()),
        ];
        if mixed {
            let after = MATRIX + 4 * (lines.len() as u64 + 5);
            lines.extend([
                ("ldrb", "w8,[x19,#0x27]".into()),
                ("ldrb", "w9,[x20,#0x47]".into()),
                ("eor", "w8,w8,w9".into()),
                ("tbz", format!("w8,#7,#{after:#x}")),
                // `max_add` at 0x5070.
                ("add", "x1,x1,#0x60".into()),
            ]);
        }
        lines.extend([
            ("add", "x0,sp,#0x28".into()),
            ("bl", format!("#{APPEND_TEXT:#x}")),
            ("mov", "w8,#0x100".into()),
            ("str", "w8,[sp]".into()),
            ("add", "x1,sp,#0x28".into()),
        ]);
        let site = MATRIX + 4 * lines.len() as u64;
        lines.extend([
            ("bl", format!("#{REGISTER:#x}")),
            ("add", "sp,sp,#0x80".into()),
            ("ret", String::new()),
        ]);

        let lines: Vec<(&str, &str)> = lines
            .iter()
            .map(|(operation, operands)| (*operation, operands.as_str()))
            .collect();
        (rows(MATRIX, &lines), site)
    }

    /// A registry whose item root reaches [`MATRIX`], which takes an item of [`OTHER_REGISTRY`].
    fn matrix_fixture(mixed: bool, other_constructor: bool) -> Fixture {
        let (matrix, site) = matrix(mixed);
        let mut fixture = fixture(
            &[
                constructor(0x10),
                constructor_at(OTHER_CONSTRUCTOR, 0x30, "x1"),
                matrix_root(),
                matrix,
            ],
            vec![Root {
                function: ITEM_ROOT,
                receiver: Receiver::Item,
                sites: BTreeSet::new(),
            }],
            &[MATRIX],
            Some(CONSTRUCTOR),
        );
        let constructors = other_constructor.then_some(KeyConstructor {
            address: OTHER_CONSTRUCTOR,
            key_register: 1,
        });
        fixture.registry.inputs = vec![SecondInput {
            function: MATRIX,
            register: 1,
            registry: OTHER_REGISTRY.into(),
            sites: BTreeSet::from([site]),
            constructors: constructors.into_iter().collect(),
        }];
        fixture
    }

    #[test]
    fn a_function_that_takes_another_registrys_item_names_both_keys() {
        let mut without_roots = matrix_fixture(false, true);
        without_roots.registry.roots.clear();
        for fixture in [matrix_fixture(false, true), without_roots] {
            assert_two_key_family(&fixture);
        }
    }

    fn assert_two_key_family(fixture: &Fixture) {
        assert_eq!(
            fixture.only_family(),
            Ok(Family {
                parts: vec![
                    Part::ItemKey,
                    literal("_"),
                    Part::RegistryKey(OTHER_REGISTRY.into()),
                    literal("_build_speed_mult"),
                ],
                limit: None,
                mask: Some(0x100),
                condition: Condition::Unresolved,
            })
        );
    }

    /// Negative control: without the other registry's key place, the input does not run, and
    /// the root run, which does not enter it, gives no one-key name.
    #[test]
    fn an_input_without_the_other_key_place_gives_no_name() {
        assert_eq!(
            matrix_fixture(false, false).only_family(),
            Err(Unresolved::new("constructor"))
        );
    }

    /// Negative control: the two keys' forms vary on their own, so a name that only a long key
    /// with a short key changes disagrees.
    #[test]
    fn an_input_runs_each_pair_of_key_forms() {
        assert_eq!(
            matrix_fixture(true, true).only_family(),
            Err(Unresolved::new("key-forms-disagree"))
        );
    }

    /// An item root at `ITEM_ROOT` whose call the engine makes for every item, with the generator
    /// method, its helper, and the item constructor `constructor`.
    fn every_item_root(root: &[(&str, &str)], constructor: Rows, helper: Rows) -> Fixture {
        let mut fixture = fixture(
            &[
                constructor,
                rows(ITEM_ROOT, root),
                generator_method(),
                size(),
                helper,
            ],
            vec![Root {
                function: ITEM_ROOT,
                receiver: Receiver::Item,
                sites: BTreeSet::from([0x87c]),
            }],
            &[METHOD, HELPER, SIZE],
            Some(CONSTRUCTOR),
        );
        fixture.input.definitions = Some(definitions());
        fixture.registry.loading = Some(loading());
        fixture
    }

    /// The condition of the root's one family, `pop_{key}_max_add`.
    fn root_condition(root: &[(&str, &str)], constructor: Rows, helper: Rows) -> Condition {
        let family = every_item_root(root, constructor, helper)
            .analyze()
            .unwrap()
            .families
            .into_iter()
            .find(|family| family.parts == [literal("pop_"), Part::ItemKey, literal("_max_add")])
            .expect("the root's family");
        family.condition
    }

    /// Counts down the item's word at `+0x200`, of unknown value, then calls the generator method.
    /// `exit` is two instructions after the loop that may branch to `0x628`, past the call.
    fn counting_root(exit: [(&'static str, &'static str); 2]) -> Vec<(&'static str, &'static str)> {
        let [first, second] = exit;
        vec![
            ("sub", "sp,sp,#0xc0"),
            ("str", "x0,[sp,#0xb8]"),
            ("ldr", "w9,[x0,#0x200]"),
            // 0x60c: the loop head.
            ("cbz", "w9,#0x618"),
            ("sub", "w9,w9,#1"),
            ("b", "#0x60c"),
            first,
            second,
            ("mov", "x0,sp"),
            ("bl", "#0x700"),
            // 0x628
            ("add", "sp,sp,#0xc0"),
            ("ret", ""),
        ]
    }

    /// A loop of unknown length before the registration forks until the path bound in an unjoined
    /// run; the condition runs join at its head.
    #[test]
    fn a_loop_of_unknown_length_before_the_registration_can_be_always() {
        let counting = counting_root([("nop", ""), ("nop", "")]);
        assert_eq!(
            root_condition(&counting, constructor(0x10), helper()),
            Condition::Always
        );

        let gated = counting_root([("ldr", "w10,[x0,#0x208]"), ("cbnz", "w10,#0x628")]);
        assert_eq!(
            root_condition(&gated, constructor(0x10), helper()),
            Condition::Unresolved
        );
    }

    /// Calls the generator method, then branches through `x15`, which the run cannot follow.
    /// `before` is three instructions before the call.
    fn stopping_root<'a>(before: [(&'a str, &'a str); 3]) -> Vec<(&'a str, &'a str)> {
        let mut root = vec![
            ("sub", "sp,sp,#0xc0"),
            ("str", "x0,[sp,#0xb8]"),
            ("mov", "x19,x0"),
        ];
        root.extend(before);
        root.extend([("mov", "x0,sp"), ("bl", "#0x700"), ("br", "x15")]);
        root
    }

    /// A path that has registered every name of the name runs ends there: later code cannot take
    /// back a registration.
    #[test]
    fn a_path_ends_once_it_has_registered_every_name() {
        let after_registration = stopping_root([("nop", ""), ("nop", ""), ("nop", "")]);
        assert_eq!(
            root_condition(&after_registration, constructor(0x10), helper()),
            Condition::Always
        );

        let skipping = stopping_root([
            ("ldr", "w8,[x19,#0x200]"),
            ("cbz", "w8,#0x624"),
            ("nop", ""),
        ]);
        assert_eq!(
            root_condition(&skipping, constructor(0x10), helper()),
            Condition::Unresolved
        );

        // The item's unknown string at `+0x300`, appended to a stack string.
        let append = format!("#{APPEND_STRING:#x}");
        let assumed = stopping_root([
            ("add", "x0,sp,#0x40"),
            ("add", "x1,x19,#0x300"),
            ("bl", &append),
        ]);
        assert_eq!(
            root_condition(&assumed, constructor(0x10), helper()),
            Condition::AfterAssumedText
        );
    }

    /// The helper, returning the new modifier type in `w0`.
    fn returning_helper() -> Rows {
        let mut lines = helper();
        let tail = lines.split_off(lines.len() - 2);
        let at = tail[0].0;
        lines.push((at, "ldr".into(), "w0,[sp,#0x14]".into()));
        lines.extend(
            tail.into_iter()
                .map(|(address, operation, operands)| (address + 4, operation, operands)),
        );
        lines
    }

    /// Registers only while the item's word at `+0x300` is zero, and keeps the new type there.
    const CACHE_ROOT: [(&str, &str); 10] = [
        ("sub", "sp,sp,#0xc0"),
        ("mov", "x19,x0"),
        ("str", "x0,[sp,#0xb8]"),
        ("ldr", "w8,[x19,#0x300]"),
        ("cbnz", "w8,#0x620"),
        ("mov", "x0,sp"),
        ("bl", "#0x700"),
        ("str", "w0,[x19,#0x300]"),
        // 0x620
        ("add", "sp,sp,#0xc0"),
        ("ret", ""),
    ];

    /// The item constructor, with `extra` after it stores the key.
    fn constructor_then(extra: &[(&str, &str)]) -> Rows {
        let mut lines = constructor(0x10);
        let tail = lines.split_off(lines.len() - 2);
        lines.extend(
            extra
                .iter()
                .map(|(operation, operands)| (0, (*operation).into(), (*operands).into())),
        );
        lines.extend(tail);
        lines
            .into_iter()
            .enumerate()
            .map(|(index, (_, operation, operands))| {
                (CONSTRUCTOR + index as u64 * 4, operation, operands)
            })
            .collect()
    }

    /// A cache field starts the condition runs at the value that every constructor path leaves.
    #[test]
    fn a_cache_field_at_its_constructor_default_registers_on_every_path() {
        let empty = constructor_then(&[("str", "wzr,[x20,#0x300]")]);
        assert_eq!(
            root_condition(&CACHE_ROOT, empty, returning_helper()),
            Condition::Always
        );
    }

    #[test]
    fn a_cache_field_without_one_known_default_is_not() {
        let unresolved = |extra: &[(&str, &str)]| {
            root_condition(&CACHE_ROOT, constructor_then(extra), returning_helper())
        };

        let filled = [("mov", "w9,#1"), ("str", "w9,[x20,#0x300]")];
        assert_eq!(unresolved(&filled), Condition::Unresolved);

        // `x1` is the constructor's int argument.
        let by_index = [("str", "w1,[x20,#0x300]")];
        assert_eq!(unresolved(&by_index), Condition::Unresolved);

        let overwritten = [
            ("str", "wzr,[x20,#0x300]"),
            ("ldr", "w9,[x20,#0x400]"),
            ("str", "w9,[x20,#0x300]"),
        ];
        assert_eq!(unresolved(&overwritten), Condition::Unresolved);

        // `x22` holds the key's text, whose first byte is `g` in the long form and `i` in the short
        // form: the word is zero only for a long key.
        let by_key_form = [
            ("ldrb", "w9,[x22]"),
            ("sub", "w9,w9,#0x67"),
            ("str", "w9,[x20,#0x300]"),
        ];
        assert_eq!(unresolved(&by_key_form), Condition::Unresolved);

        let failing = [("str", "wzr,[x20,#0x300]"), ("br", "x15")];
        assert_eq!(unresolved(&failing), Condition::Unresolved);

        // Without the cache field's default the root was never `Always`.
        assert_eq!(
            root_condition(&CACHE_ROOT, constructor(0x10), returning_helper()),
            Condition::Unresolved
        );
    }
}
