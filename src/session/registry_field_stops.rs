//! The registry field method's own result, for Native's developers: every token path, where each
//! one stopped, and every gap before normalization, with the public answer derived from it and
//! from the build's reader export.
//! Addresses appear here and in no public answer. Not a consumer API.
//!
//! ```no_run
//! use pdx_native::Native;
//! use pdx_native::internals::registry_field_stops;
//!
//! let native = Native::open("/path/to/Stellaris")?;
//! let run = registry_field_stops::run(&native, "common/megastructures")?;
//! println!("{:?}", run.answer.completeness);
//! for gap in run.result.gaps.iter().filter(|gap| gap.stop.is_some()) {
//!     println!("{:?} {}: {}", gap.kind, gap.reason, gap.stop.unwrap());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use std::collections::{BTreeMap, BTreeSet};

use super::Native;
use super::field_entries;
use super::questions::{RegistryFieldRun, error};
use crate::binding::BlockFacts;
use crate::engine::analysis::fields::FieldInput;
use crate::{Answer, Error, Field, Gap, Operation};

pub use crate::engine::analysis::callbacks::blocks::{Block, EntryRun};
pub use crate::engine::analysis::callbacks::{Context, Slot};

pub use crate::engine::analysis::fields::{
    Condition, FieldGap, FieldGapKind, PathOutcome, ReaderJoin, RegistryFieldResult, RootField,
    TokenPath, Value,
};
pub use crate::engine::analysis::stop::{
    Bound, CAUSE_LIMIT, Cause, CauseKind, Obstacle, Stop, Trace, Unknown, Unresolved,
};

/// One run of the registry field method for one registry.
#[derive(Debug, Clone)]
pub struct Run {
    /// The public answer, as `Native::registry_fields` gives it on this installation.
    pub answer: Answer<Vec<Field>>,
    /// The method's own result, from which `answer` is derived.
    pub result: RegistryFieldResult,
    /// What the block entry context method found for the registry's root trigger, effect and
    /// weight blocks.
    pub entry_contexts: EntryContexts,
}

/// The block entry context method's findings for one registry.
#[derive(Debug, Clone)]
pub struct EntryContexts {
    /// The owner type of the registry's items, which owns its blocks.
    pub owner: String,
    /// The gaps that entry contexts add to the answer.
    pub gaps: Vec<Gap>,
    /// The blocks of each root trigger, effect and weight field, by field name: the owner, each
    /// storage offset and the field's reader family. A field whose storage is not established has
    /// none.
    pub field_blocks: BTreeMap<String, BTreeSet<Block>>,
    /// The context pass from each entry call that reaches one of the owner's blocks.
    pub runs: Vec<EntryRun>,
    /// The direct evaluator calls in the owner's methods whose block the method cannot name.
    pub unnamed_evaluations: usize,
    /// The engine's scope names by type bit, when the table could be read.
    pub scope_names: Option<Vec<String>>,
}

/// Run the registry field method once for `registry` on an opened installation. A `Native` over
/// recorded answers has no method result: the error is `Error::Unsupported`. The answer is not
/// written to a recorder.
pub fn run(native: &Native, registry: &str) -> Result<Run, Error> {
    native.method_result(Operation::RegistryFields, || {
        let run = native.registry_field_run(registry)?;
        let entry_contexts = entry_contexts(&run, native.block_facts()?);
        let layout = super::questions::root_layout(&run.result);
        Ok(Run {
            answer: native.with_reader_export(registry, layout, run.answer)?,
            result: std::sync::Arc::unwrap_or_clone(run.result),
            entry_contexts,
        })
    })
}

/// One reader of a registry's root keys. The Ghidra export walks the registry from these readers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RootReader {
    /// The root key that the loader reserves for `class`; empty for the registry's owner.
    pub key: String,
    /// The class of the items that the reader reads.
    pub class: String,
    /// The reader's demangled signature.
    pub reader: String,
    /// The reader's entry address, when the symbol table names the reader.
    pub address: Option<u64>,
}

/// The readers of `registry`'s root keys: the root reader of the registry's owner, then the member
/// reader of each class that the loader reserves a root key for. The owner's root reader is its
/// member reader, its serializer's, or its base class's, as the registry field method selects. A
/// registry with no one owner is an error, as for `Native::registry_fields`.
pub fn root_readers(native: &Native, registry: &str) -> Result<Vec<RootReader>, Error> {
    let operation = Operation::RegistryFields;
    native.method_result(operation, || {
        let name = registry.trim_end_matches('/');
        let analysis = &native.bound().analysis;
        let verified = analysis.verified().map_err(|e| error(operation, e))?;
        let Some(Ok(owner)) = verified.registry_owner(name) else {
            return Err(native.unanswered_registry(operation, registry));
        };
        let keyed = verified
            .database_schedule()
            .map(|schedule| schedule.keyed_classes(name))
            .unwrap_or_default();
        let classes = std::iter::once(("", owner)).chain(keyed);

        classes
            .map(|(key, class)| {
                let input = analysis
                    .registry_field_input(&verified, name, class)
                    .map_err(|e| error(operation, e))?;

                Ok(root_reader(key, &input))
            })
            .collect()
    })
}

/// The reader that the registry field method starts its walk of `input`'s owner from.
fn root_reader(key: &str, input: &FieldInput) -> RootReader {
    let reader = match (&input.serializer, &input.base_root) {
        (Some(serializer), _) => serializer.reader.clone(),
        (None, Some(base)) => base.reader.clone(),
        (None, None) => format!("{}::ReadMember(CReader&, int)", input.owner),
    };
    let address = input
        .symbols
        .iter()
        .find(|symbol| symbol.name == reader)
        .map(|symbol| symbol.address);

    RootReader {
        key: key.to_owned(),
        class: input.owner.clone(),
        reader,
        address,
    }
}

/// The block method's findings for the owner of `run`'s registry.
fn entry_contexts(run: &RegistryFieldRun, facts: &BlockFacts) -> EntryContexts {
    let field_blocks = super::loader_kinds::owner_fields(&run.answer.value, &run.result)
        .iter()
        .zip(&run.result.fields)
        .filter(|(field, _)| field_entries::takes_entry_contexts(field.reader.family))
        .map(|(field, root)| {
            let destinations = field_entries::destinations(root);
            let blocks =
                field_entries::field_blocks(&run.owner, field.reader.family, &destinations);
            (field.name.clone(), blocks)
        })
        .collect();
    let runs = facts
        .entries
        .runs
        .iter()
        .filter(|entry| entry.blocks.iter().any(|block| block.owner == run.owner))
        .cloned()
        .collect();

    EntryContexts {
        owner: run.owner.clone(),
        gaps: run.entry_gaps.clone(),
        field_blocks,
        runs,
        unnamed_evaluations: facts
            .entries
            .unattributed
            .get(&run.owner)
            .copied()
            .unwrap_or_default(),
        scope_names: facts.scope_names.clone(),
    }
}
