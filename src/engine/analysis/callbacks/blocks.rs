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
//!   runs at each direct call to it, up to [`CALLER_DEPTH`] callers. A call whose scope is not a
//!   parameter is an *entry*.
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

use super::contexts::{BlockCalls, EVALUATED_SCOPE, Evaluations, Runner, ScopeFunctions, Selected};
use super::instances;
use super::names::{self, Fact, State, StringFunctions};
use super::{CallbackLayout, Context, Findings};
use crate::engine::analysis::declarations::number;
use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::evaluate::{Code, ReadOnlyData};
use crate::engine::analysis::stop::Unresolved;

/// How many callers up the method follows a scope that wrappers pass on.
pub const CALLER_DEPTH: usize = 2;

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
    /// [`CALLER_DEPTH`] calls away.
    pub functions: BTreeMap<u64, Vec<Instruction>>,
    /// The direct calls to each function that holds a site, and to its callers short of
    /// [`CALLER_DEPTH`].
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

/// A direct call to a function.
#[derive(Debug, Clone, Copy)]
pub struct CallSite {
    /// The call instruction.
    pub address: u64,
    /// The start of the function that holds it.
    pub function: u64,
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

/// A call from which the context pass reads the evaluations that it reaches.
struct Entry {
    function: u64,
    site: u64,
    selected: Selected,
    /// The blocks that the entry reaches, which an unresolved run is charged to.
    blocks: BTreeSet<Block>,
}

/// A function that passes its scope parameter `parameter` on to an evaluation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Wrapper {
    function: u64,
    parameter: usize,
}

/// Run the method over every site.
pub fn analyze_blocks(input: &BlockInput) -> BlockEntries {
    let mut result = BlockEntries::default();
    let Attribution {
        evaluations,
        mut entries,
        wrappers,
    } = attribute_sites(input, &mut result);
    let wrappers = climb_wrappers(input, wrappers, &mut entries, &mut result);
    collect_contexts(input, &evaluations, entries, &wrappers, &mut result);
    result
}

/// What the name pass establishes at the sites.
struct Attribution {
    /// The block that each attributed site evaluates, by the site's address.
    evaluations: BTreeMap<u64, Block>,
    /// The sites whose scope is not a parameter.
    entries: Vec<Entry>,
    /// The functions whose sites take their scope from a parameter, with the blocks they reach.
    wrappers: BTreeMap<Wrapper, BTreeSet<Block>>,
}

/// Attribute each site to the block that it evaluates, and sort the attributed sites into entries
/// and wrappers. A site whose block is not `this` plus one offset is counted as unattributed.
fn attribute_sites(input: &BlockInput, result: &mut BlockEntries) -> Attribution {
    let states = states_at(
        input,
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
        match parameter(state, EVALUATED_SCOPE) {
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
                blocks: BTreeSet::from([block]),
            }),
        }
    }
    attribution
}

/// Follow each wrapper's scope parameter up its direct callers, to [`CALLER_DEPTH`] callers, and
/// add each call that does not pass a parameter on to `entries`. Give every wrapper found.
fn climb_wrappers(
    input: &BlockInput,
    wrappers: BTreeMap<Wrapper, BTreeSet<Block>>,
    entries: &mut Vec<Entry>,
    result: &mut BlockEntries,
) -> BTreeSet<u64> {
    let mut found = BTreeSet::new();
    let mut level = wrappers;
    for depth in 1..=CALLER_DEPTH + 1 {
        found.extend(level.keys().map(|wrapper| wrapper.function));
        if depth > CALLER_DEPTH {
            for blocks in level.values() {
                charge(result, blocks, "caller-depth");
            }
            break;
        }

        let callers =
            |wrapper: &Wrapper| input.callers.get(&wrapper.function).into_iter().flatten();
        for (wrapper, blocks) in &level {
            if callers(wrapper).next().is_none() {
                charge(result, blocks, "no-caller");
            }
        }
        let calls: Vec<(&Wrapper, &BTreeSet<Block>, CallSite)> = level
            .iter()
            .flat_map(|(wrapper, blocks)| {
                callers(wrapper).map(move |call| (wrapper, blocks, *call))
            })
            .collect();
        let states = states_at(
            input,
            calls
                .iter()
                .map(|(_, _, call)| (call.function, call.address)),
        );

        let mut next: BTreeMap<Wrapper, BTreeSet<Block>> = BTreeMap::new();
        for (wrapper, blocks, call) in calls {
            let Some(state) = states.get(&call.address) else {
                charge(result, blocks, "caller-not-decoded");
                continue;
            };
            match parameter(state, wrapper.parameter) {
                Some(parameter) => {
                    let caller = Wrapper {
                        function: call.function,
                        parameter,
                    };
                    next.entry(caller)
                        .or_default()
                        .extend(blocks.iter().cloned());
                }
                None => entries.push(Entry {
                    function: call.function,
                    site: call.address,
                    selected: Selected::Wrapper(wrapper.function),
                    blocks: blocks.clone(),
                }),
            }
        }
        level = next;
    }
    found
}

/// Run the context pass from each entry, and record the contexts that the attributed evaluations
/// receive. A run's bounds are charged to its blocks only when a contradiction appears: an
/// unreadable context, or no evaluation reached at all.
fn collect_contexts(
    input: &BlockInput,
    evaluations: &BTreeMap<u64, Block>,
    entries: Vec<Entry>,
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
    };
    let calls = BlockCalls {
        evaluators: &input.evaluators,
        scope_users: &input.scope_users,
        wrappers,
        receivers: &input.receivers,
        never_return: &input.never_return,
    };
    let entered: BTreeSet<u64> = wrappers.union(&input.receivers).copied().collect();

    for entry in entries {
        let run = match entry_code(input, &entered, entry.function) {
            Some(code) => {
                let found =
                    runner.evaluations(&code, entry.function, entry.site, entry.selected, &calls);
                entry_run(entry, found, evaluations)
            }
            None => EntryRun {
                unresolved: vec![Unresolved::new("site-not-decoded")],
                ..entry_run(entry, Evaluations::default(), evaluations)
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
fn entry_run(entry: Entry, found: Evaluations, evaluations: &BTreeMap<u64, Block>) -> EntryRun {
    let reached: BTreeSet<(u64, Block, Context)> = found
        .reached
        .into_iter()
        .filter_map(|(address, context)| {
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
        blocks: entry.blocks,
        reached,
        unresolved: found.unresolved,
        bounded: found.bounded,
        contradicted,
    }
}

/// The name-pass state before each `(function, address)`, for the functions that are decoded.
fn states_at(input: &BlockInput, sites: impl Iterator<Item = (u64, u64)>) -> BTreeMap<u64, State> {
    let mut by_function: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    for (function, address) in sites {
        by_function.entry(function).or_default().insert(address);
    }

    let mut states = BTreeMap::new();
    for (function, addresses) in by_function {
        let Some(rows) = input.functions.get(&function) else {
            continue;
        };
        names::each_state(rows, &input.strings, |row, state| {
            if addresses.contains(&row.address) {
                states.insert(row.address, state.clone());
            }
        });
    }
    states
}

/// The parameter that register `register` holds on every path, unchanged.
fn parameter(state: &State, register: usize) -> Option<usize> {
    match names::sole_fact(state.register(register)) {
        Some(Fact::Argument(parameter, 0)) => Some(parameter),
        _ => None,
    }
}

/// The code of `function`, with every function in `entered` that it reaches by direct calls and
/// the scope functions, or `None` when `function` is not decoded.
fn entry_code(input: &BlockInput, entered: &BTreeSet<u64>, function: u64) -> Option<Code> {
    let mut included = BTreeSet::from([function]);
    let mut pending = vec![function];
    while let Some(next) = pending.pop() {
        let Some(rows) = input.functions.get(&next) else {
            if next == function {
                return None;
            }
            continue;
        };
        let called = rows
            .iter()
            .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
            .filter_map(|row| number(&row.operands))
            .filter(|target| entered.contains(target));
        for callee in called {
            if included.insert(callee) {
                pending.push(callee);
            }
        }
    }

    let rows = included
        .iter()
        .filter_map(|function| input.functions.get(function))
        .flatten()
        .chain(&input.scope_code)
        .cloned();
    Some(Code::from_rows(rows))
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
