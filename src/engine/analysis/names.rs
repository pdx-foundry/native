//! Derived names: the names that a registry's own code composes from an item key or a string
//! field and then checks or looks up, with what the engine does when such a name is missing.
//!
//! A root is a direct `const` member of the registry's item class (the engine uses the item) or
//! its post-read initialization. A root counts when it reaches a lookup or check function (a
//! sink) directly, or through members of the item class that it calls. The run enters those
//! members and the composers of the string model, follows each composed text with the model, and
//! reads the name at each sink call with the chain of calls that the path is inside.
//!
//! Two kinds of run have different jobs. A search run gives the item its key and leaves every
//! other byte of the item unknown, so each branch on content forks: only search runs establish
//! names of the key alone, their conditions and their miss behavior. A template run also plants
//! real text in each string field of the item and of one installed element of each collection,
//! so the model can name a field part. Planted text prunes branches, so a condition from a template
//! run always keeps an unresolved term.
//!
//! Each run fixes its inputs: the key's form (long or short), each tested flag (a Boolean field
//! that a storage selection of the root tests) written 0 or 1, and the outcome of each check, by
//! the checked name's text. A first run lets every check find its name and collects the names
//! that paths check; the root then runs for each assignment of outcomes to those names, until no
//! new name appears. A search run has no element memory, so it writes only the item's own flags.
//! A root that checks more than [`CHECK_BOUND`] names or tests more than [`FLAG_BOUND`] flags is a
//! gap.
//!
//! A search run joins its paths at loop heads, since a loop over a collection of unknown length
//! would fork without end. A template run's collections hold one element, so it follows every
//! path without joining, and the installed element keeps its identity through a selection loop.
//! A check of a name with an unresolved part may go either way, except after the path assumed
//! text, where it finds nothing. A lookup that returns its text through `x8` gives the empty text.
//!
//! A name's miss behavior comes from pairs of runs that differ only in the name's outcome: a check
//! or lookup site that uses the name when it is found and another derived name when it is missing
//! gives a fallback; a diagnostic that only the missing paths write and that receives the name's
//! text gives a diagnostic; missing paths that do nothing more with the name are silent. A name
//! that no path checks before its lookup gets the lookup function's stated rule.
//!
//! The model writes unresolved text as the empty text, so that the code runs on. A path's events
//! after that point establish no condition and no miss behavior, and a search path that goes on
//! after it is incomplete.
//!
//! Outside the method: names that other code composes, such as interface code; the run-time
//! choice behind a selection (which collection element, its validity); field states that template
//! runs do not explore; the meaning of fields that the engine sets itself.
use std::collections::{BTreeMap, BTreeSet};

use super::evaluate::{Call, Code, Machine, Path};
use super::families::{
    ASSUMED_TEXT, Arena, Effect, Ending, FamilyInput, ITEM_KEY, ITEM_SPAN, KeyForm, Loading, Model,
    Node, NotEstablished, Part, Sources, StringFunctions, StringLayout, ending,
    every_returned_path, key_storage, loading, write_source,
};
use super::fields::{RegistryFieldResult, RootField, StorageSelection, storage_offset};
use super::stop::Unresolved;

#[cfg(test)]
mod tests;

/// Name and revision of the derived-name method.
pub const METHOD: &str = "derived-names/v1";

/// The most names whose outcome one root's runs enumerate.
const CHECK_BOUND: usize = 4;

/// The most tested flags that one root's runs enumerate.
const FLAG_BOUND: usize = 3;

/// Bytes of the zeroed object that a root returns its result through (`x8`).
const RESULT_SPAN: u64 = 0x100;

/// The label of the number of events that a path has recorded. The label of each event's index
/// follows it. Every scratch and stack address is below it.
const EVENT_COUNT: u64 = 1 << 62;

/// The lookup and check functions, and the functions that write a diagnostic.
#[derive(Debug, Clone, Default)]
pub struct NameInput {
    pub sinks: BTreeMap<u64, Sink>,
    pub logs: BTreeMap<u64, LogArguments>,
}

/// A function that checks or looks up a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sink {
    pub target: Target,
    pub role: Role,
    pub argument: NameArgument,
    /// What a name gives that this lookup does not find, when no check of the name came first.
    pub unchecked_miss: Option<Miss>,
    /// Where a lookup returns its text, when it returns a view through `x8`.
    pub result: Option<ResultView>,
}

/// A text that a lookup returns through `x8`, in an object of `size` bytes whose first word points
/// at the text. A run gives each lookup the empty text, all other bytes zero, so that the code
/// after the lookup takes the branches of one known text instead of forking on unknown ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultView {
    pub size: u64,
}

/// What a sink searches for a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Localization,
    Sprite,
    File,
}

/// Whether a sink only tests that a name exists, or looks it up for use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Returns whether the name exists in `w0`.
    Check,
    Lookup,
}

/// Where a sink receives the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameArgument {
    /// A `CPdxStringView`: the text in one register and its length in another.
    View { text: usize, length: usize },
    /// A string object.
    Object(usize),
    /// A text.
    Text(usize),
}

/// Where a diagnostic function receives its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogArguments {
    /// A format in `x1`, with one eight-byte stack argument for each directive.
    Formatted,
    Text(usize),
    Object(usize),
    /// The function receives no text, such as one that only starts a message.
    NoText,
}

/// The code of one registry that composes names.
pub struct RegistryInput {
    pub roots: Vec<Root>,
    /// The functions that a run enters: the item class's members between a root and a sink, and
    /// the composers that they call.
    pub entered: BTreeSet<u64>,
    /// Every body of the item constructor that takes the key.
    pub constructors: Vec<u64>,
    /// The code that makes the registry's items, when a root is post-read initialization.
    pub loading: Option<Loading>,
    /// Every root, entered function and constructor body.
    pub code: Code,
}

/// A function of the item class that composes names.
pub struct Root {
    pub function: u64,
    /// The demangled name, which joins the root to its storage selections.
    pub name: String,
    pub stage: Stage,
    /// The sink calls that the root reaches by direct calls.
    pub sites: BTreeSet<u64>,
}

/// When a root runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// Each time the engine uses an item.
    WhenUsed,
    /// When the engine initializes an item after reading it.
    OwnerInitialization,
}

/// The stored fields of the registry that runs write: string fields, Boolean flags, and the
/// collections whose one element a template run installs.
#[derive(Debug, Clone, Default)]
pub struct Storage {
    pub strings: Vec<StoredField>,
    pub flags: Vec<StoredField>,
    pub collections: Vec<Collection>,
    pub selections: Vec<StorageSelection>,
}

/// A field at an offset of the item, or of the element of a collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredField {
    /// The path from the registry's definition.
    pub path: Vec<String>,
    pub offset: u64,
    /// The index of the collection whose element holds the field.
    pub element: Option<usize>,
}

/// A collection of element pointers at an offset of the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Collection {
    pub offset: u64,
    /// The buffer pointer's offset in the collection.
    pub data_offset: u64,
    /// The element count's offset in the collection.
    pub count_offset: u64,
}

/// What the method established for one registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameResult {
    /// The key's offset in the item, or why the constructor run did not establish it. Without it
    /// no root runs.
    pub key_offset: Result<u64, Unresolved>,
    pub names: Vec<Name>,
    /// How many paths, sites or roots could not be followed, by reason.
    pub failures: BTreeMap<&'static str, usize>,
    /// Why the engine's call of post-read initialization for every item is not established, when
    /// a root is post-read initialization.
    pub not_established: Option<NotEstablished>,
}

/// One derived name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Name {
    /// At least one part is the item key or a field; none is unresolved.
    pub parts: Vec<Part>,
    pub target: Target,
    pub stage: Stage,
    pub on_missing: Miss,
    pub condition: Condition,
}

/// What a missing name gives.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Miss {
    /// The text is the name itself, and no diagnostic is written.
    ShowsKey,
    Silent,
    Diagnostic,
    /// The same site uses this other derived name.
    Fallback(Vec<Part>),
    Unresolved,
}

/// The stored field values under which a root uses a name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Condition {
    Always,
    /// Every term holds; the first is always `Unresolved`.
    All(Vec<Term>),
    Unresolved,
}

/// One term of a condition.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Term {
    /// A run-time choice or a field state that the runs do not explore.
    Unresolved,
    /// The field is zero (a string field: empty) or not.
    FieldZero { path: Vec<String>, zero: bool },
}

/// The string and Boolean fields of the registry's item and of the elements of its collections,
/// at their storage offsets, with the storage selections of its methods.
pub fn storage(result: &RegistryFieldResult) -> Storage {
    let mut storage = Storage {
        selections: result.uses.clone(),
        ..Storage::default()
    };
    collect_fields(&mut storage, &result.fields, &[], None);

    for collection in &result.collections {
        let (Some(data_offset), Some(count_offset)) =
            (collection.data_offset, collection.count_offset)
        else {
            continue;
        };
        let Some(parent) = result
            .fields
            .iter()
            .find(|field| field.token == collection.token)
        else {
            continue;
        };
        let element = storage.collections.len();
        storage.collections.push(Collection {
            offset: collection.offset,
            data_offset,
            count_offset,
        });
        let prefix = [parent.name.clone()];
        collect_fields(
            &mut storage,
            &collection.fields.fields,
            &prefix,
            Some(element),
        );
    }

    storage
}

fn collect_fields(
    storage: &mut Storage,
    fields: &[RootField],
    prefix: &[String],
    element: Option<usize>,
) {
    for field in fields {
        let Some(offset) = storage_offset(field).and_then(|offset| u64::try_from(offset).ok())
        else {
            continue;
        };
        let stored = StoredField {
            path: prefix.iter().chain([&field.name]).cloned().collect(),
            offset,
            element,
        };
        match super::readers::classify(&field.readers).kind {
            crate::ReaderKind::String => storage.strings.push(stored),
            crate::ReaderKind::Boolean => storage.flags.push(stored),
            _ => {}
        }
    }
}

/// Follow every root of the registry.
pub fn analyze(
    shared: &FamilyInput,
    input: &NameInput,
    registry: &RegistryInput,
    storage: &Storage,
) -> NameResult {
    let key_offset = key_storage(
        &registry.code,
        &shared.data,
        &shared.strings,
        shared.layout,
        &registry.constructors,
    );
    let mut result = NameResult {
        key_offset: key_offset.clone(),
        names: Vec::new(),
        failures: BTreeMap::new(),
        not_established: None,
    };
    let Ok(key_offset) = key_offset else {
        return result;
    };

    let initialization: BTreeSet<u64> = registry
        .roots
        .iter()
        .filter(|root| root.stage == Stage::OwnerInitialization)
        .map(|root| root.function)
        .collect();
    result.not_established = match &registry.loading {
        _ if initialization.is_empty() => None,
        Some(loading) => loading::every_item(shared, loading, &initialization).err(),
        None => Some(NotEstablished::NoLoader),
    };

    let analysis = Analysis {
        shared,
        input,
        registry,
        storage,
        key_offset,
    };
    for root in &registry.roots {
        let every_item = root.stage == Stage::WhenUsed || result.not_established.is_none();
        let (names, failures) = analysis.root_names(root, every_item);
        result.names.extend(names);
        for reason in failures {
            *result.failures.entry(reason).or_default() += 1;
        }
    }
    result.names = merged(result.names);
    result
}

/// Drop exact duplicates, and the conditional duplicates of a name that some root always uses
/// with the same lookup, stage and miss behavior.
fn merged(names: Vec<Name>) -> Vec<Name> {
    let always: BTreeSet<_> = names
        .iter()
        .filter(|name| name.condition == Condition::Always)
        .map(|name| (&name.parts, name.target, name.stage, &name.on_missing))
        .collect();
    let kept: BTreeSet<Name> = names
        .iter()
        .filter(|name| {
            name.condition == Condition::Always
                || !always.contains(&(&name.parts, name.target, name.stage, &name.on_missing))
        })
        .cloned()
        .collect();
    kept.into_iter().collect()
}

/// The two kinds of run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Search,
    Template,
}

/// A name and what it is checked or looked up in, by the name's text in one run.
type NameKey = (Target, String);

/// A derived name's parts and lookup target, independent of the run.
type NameId = (Vec<Part>, Target);

/// The fixed inputs of one run.
struct Plan<'s> {
    kind: Kind,
    form: KeyForm,
    flags: Vec<(&'s StoredField, bool)>,
    misses: BTreeSet<NameKey>,
}

/// One check, lookup, diagnostic or unfollowed call that receives a derived name.
#[derive(Debug, Clone)]
struct Event {
    kind: EventKind,
    /// The calls that the path was inside, then the call itself.
    chain: Vec<u64>,
    /// The path had assumed text before the call.
    assumed: bool,
}

#[derive(Debug, Clone)]
enum EventKind {
    Use {
        role: Role,
        target: Target,
        name: Node,
        unchecked_miss: Option<Miss>,
    },
    /// A diagnostic that receives these derived texts.
    Log(Vec<String>),
    /// A call that the run does not follow, and that receives these derived texts.
    Unfollowed(Vec<String>),
}

/// How one path ended, and the index of each event it recorded, in order.
struct PathRecord {
    ending: Ending,
    /// Why a failed path could not be followed.
    failure: Option<&'static str>,
    events: Vec<usize>,
    /// The path assumed text at some point.
    assumed: bool,
}

/// What one run recorded.
struct RunRecord<'s> {
    plan: Plan<'s>,
    /// The text of each planted field.
    fields: BTreeMap<Vec<String>, String>,
    events: Vec<Event>,
    paths: Vec<PathRecord>,
    /// The resolved names that paths checked.
    checked: BTreeSet<NameKey>,
    /// The chain of every sink call that a path made.
    sites: BTreeSet<Vec<u64>>,
    /// The chain of every sink call whose name has an unresolved part.
    unresolved: BTreeSet<Vec<u64>>,
    /// The chain of every sink call whose name a formatter's buffer bounds.
    bounded: BTreeSet<Vec<u64>>,
}

impl RunRecord<'_> {
    fn sources(&self) -> Sources<'_> {
        Sources {
            key: self.plan.form.text(),
            fields: &self.fields,
        }
    }

    /// The name's text in this run.
    fn text(&self, name: &NameId) -> Option<String> {
        Node {
            parts: name.0.clone(),
            limit: None,
        }
        .text(&self.sources())
    }

    fn flag(&self, flag: &StoredField) -> Option<bool> {
        self.plan
            .flags
            .iter()
            .find(|(planned, _)| *planned == flag)
            .map(|(_, value)| *value)
    }

    /// The path's events, in order.
    fn events<'a>(&'a self, path: &'a PathRecord) -> impl Iterator<Item = &'a Event> + 'a {
        path.events.iter().map(|&index| &self.events[index])
    }
}

/// The registry's code and storage, and the shared inputs, for its root runs.
struct Analysis<'a> {
    shared: &'a FamilyInput,
    input: &'a NameInput,
    registry: &'a RegistryInput,
    storage: &'a Storage,
    key_offset: u64,
}

impl Analysis<'_> {
    /// The names of one root, and the reason of each path, site or bound that could not be
    /// followed. `every_item` says whether the engine runs the root for every item.
    fn root_names(&self, root: &Root, every_item: bool) -> (Vec<Name>, Vec<&'static str>) {
        let flags = self.tested_flags(root);
        if flags.len() > FLAG_BOUND {
            return (Vec::new(), vec!["flag-bound"]);
        }

        let mut runs = Vec::new();
        for kind in self.kinds() {
            let kind_flags: Vec<&StoredField> = flags
                .iter()
                .copied()
                .filter(|flag| kind == Kind::Template || flag.element.is_none())
                .collect();
            for form in KeyForm::BOTH {
                match self.enumerated_runs(root, kind, form, &kind_flags) {
                    Some(enumerated) => runs.extend(enumerated),
                    None => return (Vec::new(), vec!["check-bound"]),
                }
            }
        }

        let failures = failures(root, &runs);
        let names = self.names(root, every_item, &flags, &runs);
        (names, failures)
    }

    /// Search runs, and template runs when the registry has a field to plant.
    fn kinds(&self) -> Vec<Kind> {
        if self.storage.strings.is_empty() {
            vec![Kind::Search]
        } else {
            vec![Kind::Search, Kind::Template]
        }
    }

    /// The Boolean fields that the root's storage selections test.
    fn tested_flags(&self, root: &Root) -> Vec<&StoredField> {
        let tested: BTreeSet<&Vec<String>> = self
            .storage
            .selections
            .iter()
            .filter(|selection| selection.method == root.name)
            .map(|selection| &selection.tested)
            .collect();
        self.storage
            .flags
            .iter()
            .filter(|flag| tested.contains(&flag.path))
            .collect()
    }

    /// The runs of one kind and key form for each flag value and each outcome of the checked
    /// names, after the checked names stop growing. `None` over the check bound.
    fn enumerated_runs<'s>(
        &'s self,
        root: &Root,
        kind: Kind,
        form: KeyForm,
        flags: &[&'s StoredField],
    ) -> Option<Vec<RunRecord<'s>>> {
        let mut checked: BTreeSet<NameKey> = BTreeSet::new();
        loop {
            let runs: Vec<RunRecord> = assignments(flags.len())
                .flat_map(|values| subsets(&checked).map(move |misses| (values.clone(), misses)))
                .map(|(values, misses)| {
                    let flags = flags.iter().copied().zip(values).collect();
                    self.run(
                        root,
                        Plan {
                            kind,
                            form,
                            flags,
                            misses,
                        },
                    )
                })
                .collect();
            let found: BTreeSet<NameKey> = runs
                .iter()
                .flat_map(|run| run.checked.iter().cloned())
                .collect();
            if found.is_subset(&checked) {
                return Some(runs);
            }
            checked.extend(found);
            if checked.len() > CHECK_BOUND {
                return None;
            }
        }
    }

    /// Run the root once with `plan`'s inputs.
    fn run<'s>(&self, root: &Root, plan: Plan<'s>) -> RunRecord<'s> {
        let mut machine = Machine::new(&self.registry.code, &self.shared.data);
        let mut arena = Arena::default();
        let fields = self.prepare(&mut machine, &mut arena, &plan);

        let model = Model {
            functions: &self.shared.strings,
            layout: self.shared.layout,
            data: &self.shared.data,
            sources: Sources {
                key: plan.form.text(),
                fields: &fields,
            },
        };
        let mut recorder = Recorder {
            input: self.input,
            model: &model,
            misses: &plan.misses,
            arena,
            events: Vec::new(),
            checked: BTreeSet::new(),
            sites: BTreeSet::new(),
            unresolved: BTreeSet::new(),
            bounded: BTreeSet::new(),
        };
        let entered = &self.registry.entered;
        let mut calls = |target, machine: &mut Machine| recorder.call(target, entered, machine);
        // A template run's collections hold one element, so its loops end, and joining at a loop
        // head would merge the installed element with the other values a path holds there.
        let paths = match plan.kind {
            Kind::Search => machine.run_paths_joining(root.function, &mut calls),
            Kind::Template => machine.run_paths(root.function, &mut calls),
        };
        let paths = paths
            .iter()
            .map(|path| path_record(&self.shared.strings, path))
            .collect();

        let Recorder {
            events,
            checked,
            sites,
            unresolved,
            bounded,
            ..
        } = recorder;
        RunRecord {
            plan,
            fields,
            events,
            paths,
            checked,
            sites,
            unresolved,
            bounded,
        }
    }

    /// Give the machine the item and its inputs: the key in the plan's form, the installed
    /// elements and the planted fields of a template run, each tested flag, and a result object
    /// in `x8`. The text of each planted field.
    fn prepare(
        &self,
        machine: &mut Machine,
        arena: &mut Arena,
        plan: &Plan,
    ) -> BTreeMap<Vec<String>, String> {
        let layout = self.shared.layout;
        let item = machine.reserve(ITEM_SPAN);
        let key = item + self.key_offset;
        write_source(machine, layout, key, plan.form.text(), plan.form, ITEM_KEY);

        let (elements, fields) = match plan.kind {
            Kind::Search => (Vec::new(), BTreeMap::new()),
            Kind::Template => {
                let elements = install_elements(machine, item, &self.storage.collections);
                let fields = plant_fields(
                    machine,
                    arena,
                    layout,
                    plan.form,
                    (item, &elements),
                    &self.storage.strings,
                );
                (elements, fields)
            }
        };
        for (flag, value) in &plan.flags {
            if let Some(base) = holder(item, &elements, flag) {
                machine.write(base + flag.offset, 1, u64::from(*value));
                machine.protect(base + flag.offset, 1);
            }
        }

        machine.set_register(0, item);
        let result = machine.allocate(RESULT_SPAN);
        machine.set_register(8, result);
        fields
    }

    /// The derived names that the root's runs record, with their miss behavior and condition.
    fn names(
        &self,
        root: &Root,
        every_item: bool,
        flags: &[&StoredField],
        runs: &[RunRecord],
    ) -> Vec<Name> {
        let mut names = Vec::new();
        for kind in [Kind::Search, Kind::Template] {
            let kind_runs: Vec<&RunRecord> =
                runs.iter().filter(|run| run.plan.kind == kind).collect();
            for name in recorded_names(&kind_runs, kind) {
                let condition = match kind {
                    Kind::Search => key_condition(&name, &kind_runs, flags, every_item),
                    Kind::Template => field_condition(&name, &kind_runs, flags),
                };
                names.push(Name {
                    on_missing: miss_behavior(&name, &kind_runs),
                    parts: name.0,
                    target: name.1,
                    stage: root.stage,
                    condition,
                });
            }
        }
        names
    }
}

/// How the path ended, and the events that it recorded in its labels.
fn path_record(strings: &StringFunctions, path: &Path) -> PathRecord {
    let ending = ending(strings, &path.end);
    let reason = match &path.end {
        Err(unresolved) => unresolved.reason,
        Ok(_) => "stopped",
    };
    let failure = (ending == Ending::Failed).then_some(reason);
    let count = path.machine.labelled(EVENT_COUNT).unwrap_or(0);
    PathRecord {
        ending,
        failure,
        events: (0..count)
            .filter_map(|index| path.machine.labelled(EVENT_COUNT + 1 + index))
            .map(|index| index as usize)
            .collect(),
        assumed: path.machine.labelled(ASSUMED_TEXT).is_some(),
    }
}

/// The element of each collection that a template run installs: the collection holds one
/// pointer, to element memory that is unknown except for what the run plants. A store to an
/// unknown address is taken not to change the collection.
fn install_elements(machine: &mut Machine, item: u64, collections: &[Collection]) -> Vec<u64> {
    collections
        .iter()
        .map(|collection| {
            let element = machine.reserve(ITEM_SPAN);
            let buffer = machine.allocate(8);
            machine.write(buffer, 8, element);
            let header = item + collection.offset;
            machine.write(header + collection.data_offset, 8, buffer);
            machine.write(header + collection.count_offset, 4, 1);
            machine.protect(buffer, 8);
            machine.protect(header + collection.data_offset, 8);
            machine.protect(header + collection.count_offset, 4);
            element
        })
        .collect()
}

/// Plant a distinct text in `form` in each string field, labelled as that field's source, and
/// give each field's text.
fn plant_fields(
    machine: &mut Machine,
    arena: &mut Arena,
    layout: StringLayout,
    form: KeyForm,
    (item, elements): (u64, &[u64]),
    strings: &[StoredField],
) -> BTreeMap<Vec<String>, String> {
    let mut texts = BTreeMap::new();
    for (index, field) in strings.iter().enumerate() {
        let Some(base) = holder(item, elements, field) else {
            continue;
        };
        let text = field_text(form, index);
        let node = arena.add(Node {
            parts: vec![Part::Field(field.path.clone())],
            limit: None,
        });
        write_source(machine, layout, base + field.offset, &text, form, node);
        texts.insert(field.path.clone(), text);
    }
    texts
}

/// The planted text of field number `index`: longer than the longest short string in the long
/// form, and in place in the short form. No planted text holds another, or the key's.
fn field_text(form: KeyForm, index: usize) -> String {
    match form {
        KeyForm::Long => format!("derivednamefieldsource{index:03}"),
        KeyForm::Short => format!("field{index:03}src"),
    }
}

/// The object that holds `field`: the item, or the installed element of its collection.
fn holder(item: u64, elements: &[u64], field: &StoredField) -> Option<u64> {
    match field.element {
        None => Some(item),
        Some(index) => elements.get(index).copied(),
    }
}

/// Every assignment of 0 or 1 to `count` flags.
fn assignments(count: usize) -> impl Iterator<Item = Vec<bool>> {
    (0..1u32 << count).map(move |bits| (0..count).map(|index| bits >> index & 1 == 1).collect())
}

/// Every subset of `names`.
fn subsets(names: &BTreeSet<NameKey>) -> impl Iterator<Item = BTreeSet<NameKey>> + '_ {
    (0..1u32 << names.len()).map(move |bits| {
        names
            .iter()
            .enumerate()
            .filter(|(index, _)| bits >> index & 1 == 1)
            .map(|(_, name)| name.clone())
            .collect()
    })
}

/// What one run's calls do, and what the run records at them.
struct Recorder<'r> {
    input: &'r NameInput,
    model: &'r Model<'r>,
    misses: &'r BTreeSet<NameKey>,
    arena: Arena,
    events: Vec<Event>,
    checked: BTreeSet<NameKey>,
    sites: BTreeSet<Vec<u64>>,
    unresolved: BTreeSet<Vec<u64>>,
    bounded: BTreeSet<Vec<u64>>,
}

impl Recorder<'_> {
    fn call(
        &mut self,
        target: Option<u64>,
        entered: &BTreeSet<u64>,
        machine: &mut Machine,
    ) -> Result<Call, Unresolved> {
        if let Some(target) = target {
            if let Some(sink) = self.input.sinks.get(&target) {
                return Ok(self.sink(sink, machine));
            }
            if let Some(log) = self.input.logs.get(&target) {
                self.log(*log, machine);
                return Ok(Call::Return(None));
            }
            if entered.contains(&target) {
                return Ok(Call::Enter);
            }
        }

        let functions = self.model.functions;
        let modelled = target.is_some_and(|target| {
            functions.follows(target) || functions.never_return.contains(&target)
        });
        if !modelled {
            self.unfollowed(machine);
        }
        Ok(match self.model.call(target, machine, &mut self.arena)? {
            Effect::Followed(call) => call,
            Effect::Other => Call::Return(None),
        })
    }

    /// Record the name at a sink call, and give a check the run's outcome for it.
    fn sink(&mut self, sink: &Sink, machine: &mut Machine) -> Call {
        if let (Some(view), Some(result)) = (sink.result, machine.register(8)) {
            let empty = machine.allocate(1);
            for offset in (0..view.size).step_by(8) {
                machine.write(result + offset, 8, 0);
            }
            machine.write(result, 8, empty);
        }
        let chain = chain(machine);
        self.sites.insert(chain.clone());
        let node = self.name(sink.argument, machine);
        let node = self.arena.node(node).clone();
        if node.limit.is_some() {
            self.bounded.insert(chain);
            return Call::Return(None);
        }
        let Some(text) = node.text(&self.model.sources) else {
            self.unresolved.insert(chain);
            return Call::Return(unresolved_outcome(sink, machine));
        };

        let key = (sink.target, text);
        let outcome = (sink.role == Role::Check).then(|| u64::from(!self.misses.contains(&key)));
        if sink.role == Role::Check {
            self.checked.insert(key);
        }
        if has_source(&node) {
            let kind = EventKind::Use {
                role: sink.role,
                target: sink.target,
                name: node,
                unchecked_miss: sink.unchecked_miss.clone(),
            };
            self.record(machine, kind, chain);
        }
        Call::Return(outcome)
    }

    fn name(&mut self, argument: NameArgument, machine: &Machine) -> u64 {
        let model = self.model;
        match argument {
            NameArgument::View { text, length } => model.view(
                machine,
                machine.register(text),
                machine.register(length),
                &mut self.arena,
            ),
            NameArgument::Object(register) => {
                model.object_node(machine, machine.register(register), &mut self.arena)
            }
            NameArgument::Text(register) => {
                model.text_node(machine, machine.register(register), &mut self.arena)
            }
        }
    }

    /// Record a diagnostic that receives a derived text.
    fn log(&mut self, arguments: LogArguments, machine: &mut Machine) {
        let nodes = match arguments {
            LogArguments::Formatted => self.formatted_arguments(machine),
            LogArguments::Text(register) => {
                vec![
                    self.model
                        .text_node(machine, machine.register(register), &mut self.arena),
                ]
            }
            LogArguments::Object(register) => {
                vec![
                    self.model
                        .object_node(machine, machine.register(register), &mut self.arena),
                ]
            }
            LogArguments::NoText => Vec::new(),
        };
        let texts = self.derived_texts(&nodes);
        if !texts.is_empty() {
            self.record(machine, EventKind::Log(texts), chain(machine));
        }
    }

    /// The node of each stack argument of the literal format in `x1`, one per directive.
    fn formatted_arguments(&mut self, machine: &Machine) -> Vec<u64> {
        let Some(format) = machine
            .register(1)
            .and_then(|format| self.model.data.string(format))
        else {
            return Vec::new();
        };
        let directives = format.replace("%%", "").matches('%').count();
        (0..directives as u64)
            .map(|index| {
                let text = machine.read(machine.stack_pointer() + index * 8, 8);
                self.model.text_node(machine, text, &mut self.arena)
            })
            .collect()
    }

    /// Record a call that the run does not follow when an argument register holds a derived
    /// text or a string object with one.
    fn unfollowed(&mut self, machine: &mut Machine) {
        let mut nodes = Vec::new();
        for register in 0..8 {
            let Some(address) = machine.register(register) else {
                continue;
            };
            let text = machine.labelled(address);
            let object = machine
                .read(address, 8)
                .and_then(|buffer| machine.labelled(buffer));
            nodes.extend(text.into_iter().chain(object));
        }
        let texts = self.derived_texts(&nodes);
        if !texts.is_empty() {
            self.record(machine, EventKind::Unfollowed(texts), chain(machine));
        }
    }

    /// The real text of each resolved node that has a source part.
    fn derived_texts(&self, nodes: &[u64]) -> Vec<String> {
        nodes
            .iter()
            .map(|&node| self.arena.node(node))
            .filter(|node| has_source(node))
            .filter_map(|node| node.text(&self.model.sources))
            .collect()
    }

    /// Add an event to the run and to this path's events.
    fn record(&mut self, machine: &mut Machine, kind: EventKind, chain: Vec<u64>) {
        self.events.push(Event {
            kind,
            chain,
            assumed: machine.labelled(ASSUMED_TEXT).is_some(),
        });
        let count = machine.labelled(EVENT_COUNT).unwrap_or(0);
        machine.label(EVENT_COUNT + 1 + count, self.events.len() as u64 - 1);
        machine.label(EVENT_COUNT, count + 1);
    }
}

/// What a sink gives for a name with an unresolved part: an unknown value, so that both outcomes of
/// a check stay possible. After the path assumed text, a check finds nothing: the model wrote the
/// empty text, and the path's later events establish nothing, so it need not fork.
fn unresolved_outcome(sink: &Sink, machine: &Machine) -> Option<u64> {
    let assumed = machine.labelled(ASSUMED_TEXT).is_some();
    (sink.role == Role::Check && assumed).then_some(0)
}

/// The calls that the path is inside, then the present call.
fn chain(machine: &Machine) -> Vec<u64> {
    machine.entered_calls().chain([machine.pc()]).collect()
}

fn has_source(node: &Node) -> bool {
    node.parts
        .iter()
        .any(|part| matches!(part, Part::ItemKey | Part::Field(_)))
}

/// The reason of each obstruction of the root's runs: each sink call that no run reaches, each
/// site with an unresolved name in a search run or a bounded name, the search paths that went on
/// after assumed text, and each distinct reason of a failed path.
fn failures(root: &Root, runs: &[RunRecord]) -> Vec<&'static str> {
    let mut failures = Vec::new();
    let reached: BTreeSet<u64> = runs
        .iter()
        .flat_map(|run| &run.sites)
        .filter_map(|chain| chain.last().copied())
        .collect();
    failures.extend(
        root.sites
            .iter()
            .filter(|site| !reached.contains(site))
            .map(|_| "unreached"),
    );

    let search: Vec<&RunRecord> = runs
        .iter()
        .filter(|run| run.plan.kind == Kind::Search)
        .collect();
    let unresolved: BTreeSet<&Vec<u64>> = search.iter().flat_map(|run| &run.unresolved).collect();
    let bounded: BTreeSet<&Vec<u64>> = runs.iter().flat_map(|run| &run.bounded).collect();
    failures.extend(unresolved.iter().map(|_| "unresolved-name"));
    failures.extend(bounded.iter().map(|_| "name-limit"));
    if search
        .iter()
        .any(|run| run.paths.iter().any(|path| path.assumed))
    {
        failures.push("assumed-text");
    }
    let reasons: BTreeSet<&'static str> = runs
        .iter()
        .flat_map(|run| &run.paths)
        .filter_map(|path| path.failure)
        .collect();
    failures.extend(reasons);
    failures
}

/// The derived names that a kind of run checks or looks up: names of the key alone from search
/// runs, names with a field part from template runs.
fn recorded_names(runs: &[&RunRecord], kind: Kind) -> BTreeSet<NameId> {
    runs.iter()
        .flat_map(|run| &run.events)
        .filter_map(|event| match &event.kind {
            EventKind::Use { target, name, .. } => Some((name.parts.clone(), *target)),
            EventKind::Log(_) | EventKind::Unfollowed(_) => None,
        })
        .filter(|(parts, _)| {
            let field = parts.iter().any(|part| matches!(part, Part::Field(_)));
            field == (kind == Kind::Template)
        })
        .collect()
}

/// Whether the event checks or looks up `name`, and with which role.
fn use_of(event: &Event, name: &NameId) -> Option<Role> {
    match &event.kind {
        EventKind::Use {
            role,
            target,
            name: node,
            ..
        } if node.parts == name.0 && *target == name.1 => Some(*role),
        _ => None,
    }
}

/// `Always` for a root that the engine runs for every item, when every search run's returned
/// paths use the name before they assume text. Otherwise the flags whose one value every run that
/// uses the name has, under an unresolved term; otherwise unresolved. A name that paths use only
/// after they assume text is unresolved.
fn key_condition(
    name: &NameId,
    runs: &[&RunRecord],
    flags: &[&StoredField],
    every_item: bool,
) -> Condition {
    if !established(name, runs) {
        return Condition::Unresolved;
    }

    let paths: Vec<Vec<(&RunRecord, &PathRecord)>> = runs
        .iter()
        .map(|run| run.paths.iter().map(|path| (*run, path)).collect())
        .collect();
    let always = every_item
        && every_returned_path(
            &paths,
            |(_, path)| path.ending,
            |(run, path)| {
                run.events(path)
                    .any(|event| !event.assumed && use_of(event, name).is_some())
            },
        );
    if always {
        return Condition::Always;
    }

    match flag_terms(name, runs, flags) {
        terms if terms.is_empty() => Condition::Unresolved,
        terms => Condition::All([Term::Unresolved].into_iter().chain(terms).collect()),
    }
}

/// An unresolved term, each field part nonempty, and the flags whose one value every run that
/// uses the name has.
fn field_condition(name: &NameId, runs: &[&RunRecord], flags: &[&StoredField]) -> Condition {
    let mut fields: Vec<&Vec<String>> = Vec::new();
    for part in &name.0 {
        if let Part::Field(path) = part
            && !fields.contains(&path)
        {
            fields.push(path);
        }
    }
    let nonempty = fields.into_iter().map(|path| Term::FieldZero {
        path: path.clone(),
        zero: false,
    });
    let terms = [Term::Unresolved]
        .into_iter()
        .chain(nonempty)
        .chain(flag_terms(name, runs, flags))
        .collect();
    Condition::All(terms)
}

/// For each flag, `FieldZero` when every run in which a path uses the name wrote the flag with
/// one value.
fn flag_terms(name: &NameId, runs: &[&RunRecord], flags: &[&StoredField]) -> Vec<Term> {
    let using: Vec<&RunRecord> = runs
        .iter()
        .copied()
        .filter(|run| run.events.iter().any(|event| use_of(event, name).is_some()))
        .collect();
    flags
        .iter()
        .filter_map(|flag| {
            let values: BTreeSet<Option<bool>> = using.iter().map(|run| run.flag(flag)).collect();
            match values.into_iter().collect::<Vec<_>>().as_slice() {
                [Some(value)] => Some(Term::FieldZero {
                    path: flag.path.clone(),
                    zero: !value,
                }),
                _ => None,
            }
        })
        .collect()
}

/// What a missing name gives. A name that no path checks gets its lookup's stated rule; a checked
/// name gets the one result of every pair of runs that differ only in its outcome. A name that
/// paths use only after they assume text is unresolved.
fn miss_behavior(name: &NameId, runs: &[&RunRecord]) -> Miss {
    if !established(name, runs) {
        return Miss::Unresolved;
    }

    let checked = runs.iter().any(|run| {
        run.events
            .iter()
            .any(|event| use_of(event, name) == Some(Role::Check))
    });
    if !checked {
        return stated_rule(name, runs);
    }
    if runs.iter().any(|run| looks_up_unchecked(run, name)) {
        return Miss::Unresolved;
    }

    let results: BTreeSet<Miss> = runs
        .iter()
        .filter_map(|hit| {
            let key = (name.1, hit.text(name)?);
            if hit.plan.misses.contains(&key) {
                return None;
            }
            let miss = paired_run(runs, hit, &key)?;
            pair_result(name, hit, miss)
        })
        .collect();
    match results.into_iter().collect::<Vec<_>>().as_slice() {
        [result] => result.clone(),
        _ => Miss::Unresolved,
    }
}

/// Whether a path uses the name before it assumes text.
fn established(name: &NameId, runs: &[&RunRecord]) -> bool {
    runs.iter()
        .flat_map(|run| &run.events)
        .any(|event| !event.assumed && use_of(event, name).is_some())
}

/// The stated rule of every lookup function that receives the name before a path assumes text,
/// when they share one.
fn stated_rule(name: &NameId, runs: &[&RunRecord]) -> Miss {
    let rules: BTreeSet<Option<&Miss>> = runs
        .iter()
        .flat_map(|run| &run.events)
        .filter(|event| !event.assumed)
        .filter_map(|event| match &event.kind {
            EventKind::Use { unchecked_miss, .. } if use_of(event, name).is_some() => {
                Some(unchecked_miss.as_ref())
            }
            _ => None,
        })
        .collect();
    match rules.into_iter().collect::<Vec<_>>().as_slice() {
        [Some(rule)] => (*rule).clone(),
        _ => Miss::Unresolved,
    }
}

/// Whether a path of the run looks the name up with no check of it before.
fn looks_up_unchecked(run: &RunRecord, name: &NameId) -> bool {
    run.paths.iter().any(|path| {
        let mut checked = false;
        for event in run.events(path) {
            match use_of(event, name) {
                Some(Role::Check) => checked = true,
                Some(Role::Lookup) if !checked => return true,
                _ => {}
            }
        }
        false
    })
}

/// The run that differs from `hit` only in that `key` is missing.
fn paired_run<'a>(
    runs: &[&'a RunRecord<'a>],
    hit: &RunRecord,
    key: &NameKey,
) -> Option<&'a RunRecord<'a>> {
    let mut misses = hit.plan.misses.clone();
    misses.insert(key.clone());
    runs.iter().copied().find(|run| {
        run.plan.form == hit.plan.form
            && run.plan.misses == misses
            && run.plan.flags.iter().map(|(_, value)| value).eq(hit
                .plan
                .flags
                .iter()
                .map(|(_, value)| value))
    })
}

/// What a missing name gives in one pair of runs, or `None` when no path of the missing run
/// checks the name before it assumes text.
fn pair_result(name: &NameId, hit: &RunRecord, miss: &RunRecord) -> Option<Miss> {
    let text = hit.text(name)?;
    let missing_paths = checking_paths(miss, name);
    if missing_paths.is_empty() {
        return None;
    }

    let replacements = replacements(name, hit, miss);
    match replacements.len() {
        0 => {}
        1 => return replacements.into_iter().next().map(Miss::Fallback),
        _ => return Some(Miss::Unresolved),
    }

    let logs = |run: &RunRecord, path: &PathRecord, established: bool| {
        after_check(run, path, name).any(|event| {
            (!established || !event.assumed)
                && matches!(&event.kind, EventKind::Log(texts) if receives(texts, &text))
        })
    };
    let diagnostic = missing_paths.iter().all(|path| logs(miss, path, true))
        && !checking_paths(hit, name)
            .iter()
            .any(|path| logs(hit, path, false));
    if diagnostic {
        return Some(Miss::Diagnostic);
    }

    let silent = missing_paths.iter().all(|path| {
        path.ending != Ending::Failed
            && !path.assumed
            && !after_check(miss, path, name).any(|event| match &event.kind {
                EventKind::Use { .. } => use_of(event, name) == Some(Role::Lookup),
                EventKind::Log(texts) | EventKind::Unfollowed(texts) => receives(texts, &text),
            })
    });
    Some(if silent {
        Miss::Silent
    } else {
        Miss::Unresolved
    })
}

/// The paths of the run that check the name before they assume text.
fn checking_paths<'a>(run: &'a RunRecord, name: &NameId) -> Vec<&'a PathRecord> {
    run.paths
        .iter()
        .filter(|path| {
            run.events(path)
                .any(|event| !event.assumed && use_of(event, name) == Some(Role::Check))
        })
        .collect()
}

/// The path's events after its first check of the name.
fn after_check<'a>(
    run: &'a RunRecord,
    path: &'a PathRecord,
    name: &'a NameId,
) -> impl Iterator<Item = &'a Event> + 'a {
    run.events(path)
        .skip_while(move |event| use_of(event, name) != Some(Role::Check))
        .skip(1)
}

/// Whether one of the derived texts is the name's text, or a message that holds it.
fn receives(texts: &[String], text: &str) -> bool {
    texts.iter().any(|received| received.contains(text))
}

/// The other derived names, with the name's lookup target, that the missing run's paths use
/// after they check the name, at the sites where the found run's paths use the name after they
/// check it.
fn replacements(name: &NameId, hit: &RunRecord, miss: &RunRecord) -> BTreeSet<Vec<Part>> {
    let name_sites: BTreeSet<&Vec<u64>> = checking_paths(hit, name)
        .into_iter()
        .flat_map(|path| after_check(hit, path, name))
        .filter(|event| !event.assumed && use_of(event, name).is_some())
        .map(|event| &event.chain)
        .collect();
    checking_paths(miss, name)
        .into_iter()
        .flat_map(|path| after_check(miss, path, name))
        .filter(|event| !event.assumed && name_sites.contains(&event.chain))
        .filter_map(|event| match &event.kind {
            EventKind::Use {
                target,
                name: other,
                ..
            } if *target == name.1 && other.parts != name.0 => Some(other.parts.clone()),
            _ => None,
        })
        .collect()
}
