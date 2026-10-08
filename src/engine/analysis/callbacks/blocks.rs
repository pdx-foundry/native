//! Registry field blocks: the scope that each direct evaluation of a stored trigger or effect
//! block receives, for `this`, `root`, the `from` chain and the `prev` chain.
//!
//! A registry item evaluates its own blocks in its methods: the method passes `this` plus the
//! block's storage offset and a scope object to a trigger evaluator or an effect executor. The
//! binding lists every direct call to an evaluator in a method of a registry owner. The method
//! then works in two steps.
//!
//! - The **name pass** names the block: the call is attributed to `(owner, offset)` only when
//!   `x0` holds `this` plus one offset on every path. It also finds where the scope comes from.
//!   When the scope is the method's own parameter, the method is a *wrapper*, and the same test
//!   runs at each direct call to it, up to [`climb::CALLER_DEPTH`] callers. A call whose scope is
//!   not a parameter is an *entry*.
//! - The **context pass** runs each entry's function to the entry call, then through it. Every
//!   wrapper on the way runs inline, with its caller's arguments and memory, so the context of
//!   each evaluation that the entry reaches is read from the scope that its caller built.
//!
//! Evaluators forward through virtual calls that the pass cannot follow. The pass assumes that an
//! evaluation, and other trigger and effect code that receives a scope, changes no scope object's
//! type or links and keeps no pointer to one; this lets a scope that one evaluation received stay
//! known for the next. Virtual calls to an evaluator,
//! evaluations outside the owner's methods and nested blocks are outside the method.
use std::collections::{BTreeMap, BTreeSet};

use super::climb::{self, CallSite, Decoded, Entry, Wrapper};
use super::contexts::{
    CallReads, EVALUATED_SCOPE, EntryCalls, Read, Runner, ScopeFunctions, Selected, Subject,
};
use super::instances;
use super::names::{self, Fact, StringFunctions};
use super::{CallbackLayout, Context, Findings};
use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::evaluate::ReadOnlyData;
use crate::engine::analysis::stop::Unresolved;

/// How many calls away from the decoded functions the binding decodes functions that receive a
/// scope.
pub const RECEIVER_DEPTH: usize = 3;

/// The register in which an evaluator receives its block.
const EVALUATED_BLOCK: usize = 0;

/// The method's input.
#[derive(Debug, Clone)]
pub struct BlockInput {
    /// Direct calls to an evaluator in a method of a registry owner.
    pub sites: Vec<EvaluationSite>,
    /// Trigger evaluators and effect executors. Each takes its block in `x0` and its scope in
    /// `x1`.
    pub evaluators: BTreeSet<u64>,
    /// Other trigger and effect code that receives a scope, such as a tooltip builder.
    pub scope_users: BTreeSet<u64>,
    /// Decoded functions: each function that holds a site, and its direct callers up to
    /// [`climb::CALLER_DEPTH`] calls away.
    pub functions: BTreeMap<u64, Vec<Instruction>>,
    /// The direct calls to each function that holds a site, and to its callers short of
    /// [`climb::CALLER_DEPTH`].
    pub callers: BTreeMap<u64, Vec<CallSite>>,
    pub scope_code: Vec<Instruction>,
    pub scope_functions: ScopeFunctions,
    pub strings: StringFunctions,
    pub data: ReadOnlyData,
    pub layout: CallbackLayout,
    /// The engine's scope names by type bit, when the table could be read.
    pub scope_names: Option<Vec<String>>,
    /// How many argument registers, from `x0`, each known function reads.
    pub arguments: BTreeMap<u64, usize>,
    /// How many argument registers a call through a pointer reads, by the call instruction.
    pub call_arguments: BTreeMap<u64, usize>,
    /// Functions that ignore the `x8` that they receive: no path reads it before writing it. A
    /// call to one does not receive the address of a returned object.
    pub ignores_x8: BTreeSet<u64>,
    /// The vtable address point of the object that each proven instance pointer holds
    /// ([`instances`]). `data` holds the pointer slots that name these pointers and the slots of
    /// their vtables; the method places the objects, so a virtual call on one has a known target.
    pub instances: BTreeMap<u64, u64>,
    /// Decoded functions, other than wrappers and evaluators, that receive a scope.
    pub receivers: BTreeSet<u64>,
    /// Functions that never return.
    pub never_return: BTreeSet<u64>,
}

/// A direct call to an evaluator in a method of a registry owner.
#[derive(Debug, Clone)]
pub struct EvaluationSite {
    /// The call instruction.
    pub address: u64,
    /// The start of the method that holds it.
    pub function: u64,
    /// The owner type of the registry whose method holds it.
    pub owner: String,
}

/// A stored block: the owner type and the offset of the block in the owner object.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Block {
    /// The owner type of the registry whose item stores the block.
    pub owner: String,
    /// The block's storage offset from the item's `this`.
    pub offset: i64,
}

/// What the method established.
#[derive(Debug, Clone, Default)]
pub struct BlockEntries {
    /// The contexts of every attributed block, with the reasons that some could not be read.
    pub blocks: BTreeMap<Block, Findings>,
    /// By owner, the evaluator calls in its methods whose block the method could not name.
    pub unattributed: BTreeMap<String, usize>,
    /// What the context pass found from each entry, for Native's developers.
    pub runs: Vec<EntryRun>,
}

/// What the context pass found from one entry call.
#[derive(Debug, Clone)]
pub struct EntryRun {
    /// The start of the function that holds the entry call.
    pub function: u64,
    /// The entry call.
    pub site: u64,
    /// The wrapper that the entry calls, or `None` when it calls an evaluator.
    pub wrapper: Option<u64>,
    /// The blocks that the entry reaches, which its reasons are charged to.
    pub blocks: BTreeSet<Block>,
    /// Each attributed evaluation that a path reached: the evaluator call, its block and the
    /// context that it receives. Paths that agree give one entry.
    pub reached: BTreeSet<(u64, Block, Context)>,
    /// Why some path could not be followed, other than a bound of the search, each once with
    /// where it stopped.
    pub unresolved: Vec<Unresolved>,
    /// The bounds of the search that some path reached, each once.
    pub bounded: Vec<Unresolved>,
    /// Whether the bounds were charged to the blocks: the run reached no evaluation, or one with
    /// an unreadable context.
    pub contradicted: bool,
}

/// Run the method over every site.
pub fn analyze_blocks(input: &BlockInput) -> BlockEntries {
    let decoded = Decoded {
        functions: &input.functions,
        callers: &input.callers,
        scope_code: &input.scope_code,
        strings: &input.strings,
    };
    let mut result = BlockEntries::default();
    let Attribution {
        evaluations,
        mut entries,
        wrappers,
    } = attribute_sites(input, &mut result);
    let climb = climb::climb(&decoded, wrappers);
    for (block, reason) in climb.charges {
        charge(&mut result, &BTreeSet::from([block]), reason);
    }
    entries.extend(climb.entries);
    collect_contexts(
        input,
        &decoded,
        &evaluations,
        entries,
        &climb.wrappers,
        &mut result,
    );
    result
}

/// What the name pass establishes at the sites.
struct Attribution {
    /// The block that each attributed site evaluates, by the site's address.
    evaluations: BTreeMap<u64, Block>,
    /// The sites whose scope is not a parameter.
    entries: Vec<Entry<Block>>,
    /// The functions whose sites take their scope from a parameter, with the blocks they reach.
    wrappers: BTreeMap<Wrapper, BTreeSet<Block>>,
}

/// Attribute each site to the block that it evaluates, and sort the attributed sites into entries
/// and wrappers. A site whose block is not `this` plus one offset is counted as unattributed.
fn attribute_sites(input: &BlockInput, result: &mut BlockEntries) -> Attribution {
    let states = climb::states_at(
        &input.functions,
        &input.strings,
        input.sites.iter().map(|site| (site.function, site.address)),
    );
    let mut attribution = Attribution {
        evaluations: BTreeMap::new(),
        entries: Vec::new(),
        wrappers: BTreeMap::new(),
    };

    for site in &input.sites {
        let offset = states.get(&site.address).and_then(|state| {
            match names::sole_fact(state.register(EVALUATED_BLOCK)) {
                Some(Fact::Argument(0, offset)) => Some((state, offset)),
                _ => None,
            }
        });
        let Some((state, offset)) = offset else {
            *result.unattributed.entry(site.owner.clone()).or_default() += 1;
            continue;
        };

        let block = Block {
            owner: site.owner.clone(),
            offset,
        };
        result.blocks.entry(block.clone()).or_default();
        attribution.evaluations.insert(site.address, block.clone());
        match climb::parameter(state, EVALUATED_SCOPE) {
            Some(parameter) => {
                let wrapper = Wrapper {
                    function: site.function,
                    parameter,
                };
                attribution
                    .wrappers
                    .entry(wrapper)
                    .or_default()
                    .insert(block);
            }
            None => attribution.entries.push(Entry {
                function: site.function,
                site: site.address,
                selected: Selected::Evaluator,
                reaches: BTreeSet::from([block]),
            }),
        }
    }
    attribution
}

/// Run the context pass from each entry, and record the contexts that the attributed evaluations
/// receive. A run's bounds are charged to its blocks only when a contradiction appears: an
/// unreadable context, or no evaluation reached at all.
fn collect_contexts(
    input: &BlockInput,
    decoded: &Decoded<'_>,
    evaluations: &BTreeMap<u64, Block>,
    entries: Vec<Entry<Block>>,
    wrappers: &BTreeSet<u64>,
    result: &mut BlockEntries,
) {
    let data = instances::with_objects(&input.data, &input.instances);
    let runner = Runner {
        scope_code: &input.scope_code,
        scopes: &input.scope_functions,
        strings: &input.strings,
        lookups: &BTreeSet::new(),
        data: &data,
        layout: input.layout,
        arguments: &input.arguments,
        call_arguments: &input.call_arguments,
        ignores_x8: &input.ignores_x8,
    };
    let calls = EntryCalls {
        evaluators: &input.evaluators,
        scope_users: &input.scope_users,
        wrappers,
        receivers: &input.receivers,
        never_return: &input.never_return,
    };
    let entered: BTreeSet<u64> = wrappers.union(&input.receivers).copied().collect();
    let reads: BTreeMap<u64, Read> = evaluations
        .keys()
        .map(|&site| {
            let read = Read {
                scope: EVALUATED_SCOPE,
                subject: Subject::None,
            };
            (site, read)
        })
        .collect();

    for entry in entries {
        let run = match climb::entry_code(decoded, &entered, entry.function) {
            Some(code) => {
                let found = runner.read_calls(
                    &code,
                    entry.function,
                    entry.site,
                    entry.selected,
                    &calls,
                    &reads,
                );
                entry_run(entry, found, evaluations)
            }
            None => EntryRun {
                unresolved: vec![Unresolved::new("site-not-decoded")],
                ..entry_run(entry, CallReads::default(), evaluations)
            },
        };

        for (_, block, context) in &run.reached {
            let findings = result.blocks.entry(block.clone()).or_default();
            findings.contexts.insert(context.clone());
        }
        for reason in &run.unresolved {
            charge(result, &run.blocks, reason.reason);
        }
        if run.contradicted {
            for reason in &run.bounded {
                charge(result, &run.blocks, reason.reason);
            }
        }
        result.runs.push(run);
    }
}

/// The run of `entry`, from what the context pass `found`: the evaluations that it reached and
/// that the name pass attributed, and why some path stopped.
fn entry_run(
    entry: Entry<Block>,
    found: CallReads,
    evaluations: &BTreeMap<u64, Block>,
) -> EntryRun {
    let reached: BTreeSet<(u64, Block, Context)> = found
        .reached
        .into_iter()
        .filter_map(|(address, _, context)| {
            Some((address, evaluations.get(&address)?.clone(), context))
        })
        .collect();
    let contradicted = reached.is_empty()
        || reached
            .iter()
            .any(|(_, _, context)| !context.is_established());
    EntryRun {
        function: entry.function,
        site: entry.site,
        wrapper: match entry.selected {
            Selected::Evaluator => None,
            Selected::Wrapper(wrapper) => Some(wrapper),
        },
        blocks: entry.reaches,
        reached,
        unresolved: found.unresolved,
        bounded: found.bounded,
        contradicted,
    }
}

/// Record that the contexts of `blocks` are incomplete, for `reason`.
fn charge(result: &mut BlockEntries, blocks: &BTreeSet<Block>, reason: &'static str) {
    for block in blocks {
        result
            .blocks
            .entry(block.clone())
            .or_default()
            .unresolved
            .insert(reason);
    }
}

#[cfg(test)]
mod tests;
