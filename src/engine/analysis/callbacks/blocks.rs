//! Registry field blocks: the scope that each evaluation of a stored trigger, effect or weight
//! block receives, for `this`, `root`, the `from` chain and the `prev` chain.
//!
//! An evaluation passes a *type pointer* plus the block's storage offset and a scope object to a
//! trigger evaluator, an effect executor or a weight evaluator, directly or through the block
//! object's own vtable. A type pointer is a register that points at a registry owner's item: the
//! receiver of the owner's methods, a parameter of an owner pointer or reference type, or a member
//! that the constructors of the parameter's class fill only with such a parameter. The binding
//! decides which registers of a function lead to which owner, and which vtable slots evaluate a
//! block of which family. The method then works in two steps.
//!
//! - The **name pass** names the block: a call is attributed to `(owner, offset, family)` only
//!   when `x0` holds one type pointer plus one offset on every path, and, for a call through a
//!   register, the called register holds an evaluation slot of the vtable at that same address.
//!   A call to an *offset getter*, whose whole body is `add x0, x0, #k; ret`, returns its `x0`
//!   plus `k`. The name pass also finds where the scope comes from. When the scope is the
//!   function's own parameter, the function is a *wrapper*, and the same test runs at each direct
//!   call to it, up to [`climb::CALLER_DEPTH`] callers. A call whose scope is not a parameter is
//!   an *entry*.
//! - A function whose evaluation's block is its own parameter is a *helper*. At each direct call
//!   to it, the name pass names the block in that parameter's register, as at a site; the call is
//!   then an entry that runs through the helper, and the helper's evaluation gives its context to
//!   that caller's block only. The method does not go further up: a caller that passes a
//!   parameter on, and a helper whose scope is a parameter too, leave the block without a context.
//! - The **context pass** runs each entry's function to the entry call, then through it. Every
//!   wrapper on the way runs inline, with its caller's arguments and memory, so the context of
//!   each evaluation that the entry reaches is read from the scope that its caller built.
//!
//! A tooltip call, to a tooltip builder or through a tooltip slot of a block's vtable, names that
//! block but evaluates nothing; the method records the block, so that a block that only tooltip
//! calls name has a narrower gap.
//!
//! Evaluators forward through virtual calls that the pass cannot follow. The pass assumes that an
//! evaluation, and other trigger and effect code that receives a scope, changes no scope object's
//! type or links and keeps no pointer to one; this lets a scope that one evaluation received stay
//! known for the next. A call through a vtable slot is read on arrival and then treated as a call
//! that the pass does not follow: the pass does not know the block's family, and the same slot of
//! another family's block is not an evaluation. Other virtual calls and nested blocks are outside
//! the method.
use std::collections::{BTreeMap, BTreeSet};

use super::climb::{self, CallSite, Decoded, Entry, Wrapper};
use super::contexts::{
    CallReads, EVALUATED_SCOPE, EntryCalls, Read, Runner, ScopeFunctions, Selected, Subject,
};
use super::instances;
use super::names::{self, Fact, State, StringFunctions};
use super::{CallbackLayout, Context, Findings};
use crate::BlockFamily;
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
    /// The calls that may evaluate a block or build its tooltip, in the functions of
    /// `type_pointers`.
    pub sites: Vec<EvaluationSite>,
    /// The block family of each trigger evaluator, effect executor and weight evaluator. Each
    /// takes its block in `x0` and its scope in `x1`.
    pub evaluators: BTreeMap<u64, BlockFamily>,
    /// The block family of each vtable slot that evaluates a block, by the slot's displacement
    /// from its vtable's address point.
    pub evaluation_slots: BTreeMap<i64, BlockFamily>,
    /// The block family of each function that builds a block's tooltip and evaluates nothing.
    /// Each takes its block in `x0`.
    pub tooltip_builders: BTreeMap<u64, BlockFamily>,
    /// The block family of each vtable slot that builds a block's tooltip, by its displacement.
    pub tooltip_slots: BTreeMap<i64, BlockFamily>,
    /// The offset `k` of each function whose whole body is `add x0, x0, #k; ret`, by its address.
    pub getters: BTreeMap<u64, i64>,
    /// The registry owners that the registers of each function that holds a site, and of each
    /// decoded caller, lead to.
    pub type_pointers: BTreeMap<u64, TypePointers>,
    /// Other trigger and effect code that receives a scope, such as a tooltip builder.
    pub scope_users: BTreeSet<u64>,
    /// Decoded functions: each function that holds a site, and the direct callers of each one
    /// that holds an attributed site or a helper's evaluation, up to [`climb::CALLER_DEPTH`] calls
    /// away.
    pub functions: BTreeMap<u64, Vec<Instruction>>,
    /// The direct calls to each function that holds an attributed site or a helper's evaluation,
    /// and to its callers short of [`climb::CALLER_DEPTH`].
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

/// A call that may evaluate a block.
#[derive(Debug, Clone)]
pub struct EvaluationSite {
    /// The call instruction.
    pub address: u64,
    /// The start of the function that holds it.
    pub function: u64,
    /// What it calls.
    pub call: EvaluationCall,
}

impl EvaluationSite {
    /// Whether the site calls one of `evaluators` directly. Such a call in an owner's method whose
    /// block the name pass does not name is counted as unattributed.
    pub fn calls_evaluator(&self, evaluators: &BTreeMap<u64, BlockFamily>) -> bool {
        matches!(self.call, EvaluationCall::Direct(target) if evaluators.contains_key(&target))
    }
}

/// What an evaluation site calls.
#[derive(Debug, Clone, Copy)]
pub enum EvaluationCall {
    /// An evaluator or a tooltip builder, by its address.
    Direct(u64),
    /// The function in this register: an evaluation when it is an evaluation slot of the vtable of
    /// the block in `x0`.
    Register(usize),
}

/// The registry owners that one function's registers lead to at entry. Each register is an
/// argument register; a member function's receiver is `x0`. Each owner is a class with a vtable,
/// so the word at offset zero of its item is the item's vtable pointer and holds no block.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypePointers {
    /// The owner whose method the function is. A direct evaluator call in it whose block the
    /// method cannot name is counted for this owner.
    pub method_of: Option<String>,
    /// The owner type that each register points at.
    pub registers: BTreeMap<usize, String>,
    /// The owner type that the word at each register plus an offset points at.
    pub members: BTreeMap<(usize, i64), String>,
}

/// A stored block: the owner type, the offset of the block in the owner object, and the family of
/// the evaluation that the method found.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Block {
    /// The owner type of the registry whose item stores the block.
    pub owner: String,
    /// The block's storage offset from the item's `this`.
    pub offset: i64,
    /// The family of the evaluator or the vtable slot that evaluates the block. A field joins only
    /// a block of its own reader family.
    pub family: BlockFamily,
}

/// What the method established.
#[derive(Debug, Clone, Default)]
pub struct BlockEntries {
    /// The contexts of every attributed block, with the reasons that some could not be read.
    pub blocks: BTreeMap<Block, Findings>,
    /// By owner, the direct evaluator calls in its methods whose block the method could not name.
    pub unattributed: BTreeMap<String, usize>,
    /// The blocks that a tooltip call names: a direct call to a tooltip builder, or a call
    /// through a tooltip slot.
    pub tooltips: BTreeSet<Block>,
    /// Each helper by its start, with how often a call to it named no block for one of its
    /// evaluations, by reason, for Native's developers.
    pub helpers: BTreeMap<u64, BTreeMap<&'static str, usize>>,
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
        getters: &input.getters,
    };
    let mut result = BlockEntries::default();
    let Attribution {
        evaluations,
        mut entries,
        wrappers,
        helpers,
    } = attribute_sites(input, &mut result);
    let climb = climb::climb(&decoded, wrappers);
    for (block, reason) in climb.charges {
        charge(&mut result, &BTreeSet::from([block]), reason);
    }
    entries.extend(climb.entries);
    let helper_calls = helper_calls(&decoded, &input.type_pointers, &helpers, &mut result);
    collect_contexts(
        input,
        &decoded,
        &evaluations,
        entries,
        helper_calls,
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
    /// Each helper's evaluations of the block that it receives, by the helper's start.
    helpers: BTreeMap<u64, Vec<PassedBlock>>,
}

/// A helper's evaluation of the block that it receives as a parameter.
#[derive(Debug, Clone, Copy)]
struct PassedBlock {
    /// The evaluation call.
    site: u64,
    /// The parameter that holds the block.
    parameter: usize,
    family: BlockFamily,
    /// Whether the evaluation's scope is a parameter of the helper too.
    scope_is_parameter: bool,
}

/// The functions whose sites the name pass names a block at.
#[derive(Debug, Clone, Default)]
pub struct NamedFunctions {
    /// The functions that hold an evaluation of a named block or a helper's evaluation. The
    /// binding gives these functions' callers and the functions that they pass a scope to.
    pub evaluating: BTreeSet<u64>,
    /// The functions that hold a tooltip call on a named block.
    pub tooltips: BTreeSet<u64>,
}

/// The functions that hold a site that the name pass names a block at.
pub fn named_functions(
    sites: &[EvaluationSite],
    functions: &BTreeMap<u64, Vec<Instruction>>,
    type_pointers: &BTreeMap<u64, TypePointers>,
    evaluation: &Evaluation<'_>,
    strings: &StringFunctions,
    getters: &BTreeMap<u64, i64>,
) -> NamedFunctions {
    let states = climb::states_at(
        functions,
        strings,
        getters,
        sites.iter().map(|site| (site.function, site.address)),
    );
    let mut named = NamedFunctions::default();
    for site in sites {
        let pointers = type_pointers.get(&site.function);
        let state = states.get(&site.address);
        let Some((pointers, state)) = pointers.zip(state) else {
            continue;
        };
        match evaluation.name(site, state, pointers) {
            Named::Evaluation(_) | Named::Parameter(..) => {
                named.evaluating.insert(site.function);
            }
            Named::Tooltip(_) => {
                named.tooltips.insert(site.function);
            }
            Named::Nothing => {}
        }
    }
    named
}

/// The argument registers whose values at entry `rows`, one whole function, stores unchanged into
/// the object that `x0` addresses at entry, by the offset of each 64-bit store in that object. The
/// binding reads a constructor's stores of its parameters with it.
pub fn receiver_stores(rows: &[Instruction]) -> BTreeMap<i64, BTreeSet<usize>> {
    names::receiver_stores(rows, &StringFunctions::default())
}

/// What makes a call an evaluation of a block of some family, or a tooltip call on one.
pub struct Evaluation<'a> {
    /// The block family of each evaluator.
    pub evaluators: &'a BTreeMap<u64, BlockFamily>,
    /// The block family of each evaluation slot, by its displacement.
    pub slots: &'a BTreeMap<i64, BlockFamily>,
    /// The block family of each tooltip builder.
    pub tooltip_builders: &'a BTreeMap<u64, BlockFamily>,
    /// The block family of each tooltip slot, by its displacement.
    pub tooltip_slots: &'a BTreeMap<i64, BlockFamily>,
}

/// What the name pass finds that a site calls.
enum Named {
    /// An evaluation of a stored block.
    Evaluation(Block),
    /// A tooltip call on a stored block, which evaluates nothing.
    Tooltip(Block),
    /// An evaluation of the block that the function receives in this parameter.
    Parameter(usize, BlockFamily),
    /// Nothing that the method names.
    Nothing,
}

impl Evaluation<'_> {
    /// What `site` calls, from the name-pass `state` at the call. A direct call calls an evaluator
    /// or a tooltip builder; a call through a register calls a slot of the vtable of the object in
    /// `x0`, whose family the evaluation or tooltip slots give. The object is a stored block when
    /// it is one type pointer plus one offset other than zero: the word at offset zero is the
    /// item's own vtable pointer, so a call with the item itself in `x0` is a call on the item,
    /// not on a block. A parameter that leads to an owner is that object, not a block.
    fn name(&self, site: &EvaluationSite, state: &State, pointers: &TypePointers) -> Named {
        let Some(object) = names::sole_fact(state.register(EVALUATED_BLOCK)) else {
            return Named::Nothing;
        };
        let (evaluated, tooltip) = match site.call {
            EvaluationCall::Direct(target) => (
                self.evaluators.get(&target).copied(),
                self.tooltip_builders.get(&target).copied(),
            ),
            EvaluationCall::Register(register) => (
                slot_family(state, register, object, self.slots),
                slot_family(state, register, object, self.tooltip_slots),
            ),
        };
        let block = |(owner, offset): (&String, i64), family| Block {
            owner: owner.clone(),
            offset,
            family,
        };

        match (stored_block(object, pointers), evaluated, tooltip) {
            (Some(stored), Some(family), _) => Named::Evaluation(block(stored, family)),
            (Some(stored), None, Some(family)) => Named::Tooltip(block(stored, family)),
            (None, Some(family), _) => match object {
                Fact::Argument(parameter, 0) if !pointers.leads(parameter) => {
                    Named::Parameter(parameter, family)
                }
                _ => Named::Nothing,
            },
            _ => Named::Nothing,
        }
    }
}

impl TypePointers {
    /// Whether `register` leads to an owner, as the owner's item or through a member word.
    fn leads(&self, register: usize) -> bool {
        self.registers.contains_key(&register)
            || self.members.keys().any(|(holder, _)| *holder == register)
    }
}

/// The owner and the offset of the block at `object`: one type pointer plus one offset other than
/// zero.
fn stored_block(object: Fact, pointers: &TypePointers) -> Option<(&String, i64)> {
    let (owner, offset) = match object {
        Fact::Argument(register, offset) => (pointers.registers.get(&register)?, offset),
        Fact::Member(register, member, offset) => {
            (pointers.members.get(&(register, member))?, offset)
        }
        _ => return None,
    };
    (offset != 0).then_some((owner, offset))
}

/// The family in `slots` of the slot of `object`'s vtable that `register` holds.
fn slot_family(
    state: &State,
    register: usize,
    object: Fact,
    slots: &BTreeMap<i64, BlockFamily>,
) -> Option<BlockFamily> {
    let target = names::sole_fact(state.register(register))?;
    slots.get(&names::vtable_slot(object, target)?).copied()
}

/// Attribute each site to the block that it evaluates, and sort the attributed sites into entries
/// and wrappers. Record the blocks that tooltip calls name and the helpers' evaluations. A direct
/// evaluator call in an owner's method whose block the name pass does not name is counted as
/// unattributed.
fn attribute_sites(input: &BlockInput, result: &mut BlockEntries) -> Attribution {
    let states = climb::states_at(
        &input.functions,
        &input.strings,
        &input.getters,
        input.sites.iter().map(|site| (site.function, site.address)),
    );
    let evaluation = Evaluation {
        evaluators: &input.evaluators,
        slots: &input.evaluation_slots,
        tooltip_builders: &input.tooltip_builders,
        tooltip_slots: &input.tooltip_slots,
    };
    let mut attribution = Attribution {
        evaluations: BTreeMap::new(),
        entries: Vec::new(),
        wrappers: BTreeMap::new(),
        helpers: BTreeMap::new(),
    };

    for site in &input.sites {
        let Some(pointers) = input.type_pointers.get(&site.function) else {
            continue;
        };
        let state = states.get(&site.address);
        let named = state.map_or(Named::Nothing, |state| {
            evaluation.name(site, state, pointers)
        });
        let scope_parameter = state.and_then(|state| climb::parameter(state, EVALUATED_SCOPE));

        match named {
            Named::Evaluation(block) => {
                result.blocks.entry(block.clone()).or_default();
                attribution.evaluations.insert(site.address, block.clone());
                attribution.evaluation(site, block, scope_parameter);
            }
            Named::Tooltip(block) => {
                result.tooltips.insert(block);
            }
            Named::Parameter(parameter, family) => {
                let passed = PassedBlock {
                    site: site.address,
                    parameter,
                    family,
                    scope_is_parameter: scope_parameter.is_some(),
                };
                attribution
                    .helpers
                    .entry(site.function)
                    .or_default()
                    .push(passed);
            }
            Named::Nothing => {
                if site.calls_evaluator(&input.evaluators)
                    && let Some(owner) = &pointers.method_of
                {
                    *result.unattributed.entry(owner.clone()).or_default() += 1;
                }
            }
        }
    }
    attribution
}

impl Attribution {
    /// Sort the evaluation of `block` at `site` into the entries, or into the wrappers when its
    /// scope is the function's parameter `scope_parameter`.
    fn evaluation(&mut self, site: &EvaluationSite, block: Block, scope_parameter: Option<usize>) {
        match scope_parameter {
            Some(parameter) => {
                let wrapper = Wrapper {
                    function: site.function,
                    parameter,
                };
                self.wrappers.entry(wrapper).or_default().insert(block);
            }
            None => self.entries.push(Entry {
                function: site.function,
                site: site.address,
                selected: Selected::Evaluator,
                reaches: BTreeSet::from([block]),
            }),
        }
    }
}

/// A call that passes a block to a helper: an entry whose run gives what each of the helper's
/// evaluations receives to the block that this call passes, and to no other.
struct HelperCall {
    call: CallSite,
    helper: u64,
    /// The block that the call passes to each of the helper's evaluations, by the evaluation call.
    evaluations: BTreeMap<u64, Block>,
}

/// Name the block that each direct call to a helper passes, one caller up. A call that names a
/// block for an evaluation whose scope the helper builds becomes a [`HelperCall`]; a block whose
/// evaluation's scope is a parameter of the helper too is charged `block-and-scope-from-caller`.
/// Each call that names no block for an evaluation is counted for the helper: `no-caller` when
/// the helper has no caller, `caller-not-decoded`, `block-caller-depth` when the caller passes its
/// own parameter on, and `unattributed` when the caller's register holds no block that the
/// method names.
fn helper_calls(
    decoded: &Decoded<'_>,
    type_pointers: &BTreeMap<u64, TypePointers>,
    helpers: &BTreeMap<u64, Vec<PassedBlock>>,
    result: &mut BlockEntries,
) -> Vec<HelperCall> {
    let callers = |helper: &u64| decoded.callers.get(helper).into_iter().flatten();
    let states = climb::states_at(
        decoded.functions,
        decoded.strings,
        decoded.getters,
        helpers
            .keys()
            .flat_map(callers)
            .map(|call| (call.function, call.address)),
    );

    let mut found = Vec::new();
    for (&helper, passed) in helpers {
        let counts = result.helpers.entry(helper).or_default();
        if callers(&helper).next().is_none() {
            counts.insert("no-caller", 1);
        }

        for call in callers(&helper) {
            let state = states.get(&call.address);
            let pointers = type_pointers.get(&call.function);
            let mut evaluations = BTreeMap::new();
            for evaluation in passed {
                match passed_block(state, pointers, evaluation) {
                    Ok(block) if evaluation.scope_is_parameter => {
                        let block = BTreeSet::from([block]);
                        charge(result, &block, "block-and-scope-from-caller");
                    }
                    Ok(block) => {
                        result.blocks.entry(block.clone()).or_default();
                        evaluations.insert(evaluation.site, block);
                    }
                    Err(reason) => {
                        let counts = result.helpers.entry(helper).or_default();
                        *counts.entry(reason).or_default() += 1;
                    }
                }
            }

            if !evaluations.is_empty() {
                found.push(HelperCall {
                    call: *call,
                    helper,
                    evaluations,
                });
            }
        }
    }
    found
}

/// The block that a call with the name-pass `state` passes to a helper's `evaluation`, in a caller
/// whose registers lead to owners as `pointers` states; or why it names none.
fn passed_block(
    state: Option<&State>,
    pointers: Option<&TypePointers>,
    evaluation: &PassedBlock,
) -> Result<Block, &'static str> {
    let state = state.ok_or("caller-not-decoded")?;
    let object = names::sole_fact(state.register(evaluation.parameter));
    let stored = object
        .zip(pointers)
        .and_then(|(object, pointers)| stored_block(object, pointers));
    if let Some((owner, offset)) = stored {
        return Ok(Block {
            owner: owner.clone(),
            offset,
            family: evaluation.family,
        });
    }

    let leads = |parameter| pointers.is_some_and(|pointers| pointers.leads(parameter));
    match object {
        Some(Fact::Argument(parameter, 0)) if !leads(parameter) => Err("block-caller-depth"),
        _ => Err("unattributed"),
    }
}

/// Run the context pass from each entry and each call to a helper, and record the contexts that
/// the evaluations that they reach receive. A run's bounds are charged to its blocks only when a
/// contradiction appears: an unreadable context, or no evaluation reached at all.
fn collect_contexts(
    input: &BlockInput,
    decoded: &Decoded<'_>,
    evaluations: &BTreeMap<u64, Block>,
    entries: Vec<Entry<Block>>,
    helper_calls: Vec<HelperCall>,
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
    let evaluators: BTreeSet<u64> = input.evaluators.keys().copied().collect();
    let calls = EntryCalls {
        evaluators: &evaluators,
        scope_users: &input.scope_users,
        wrappers,
        receivers: &input.receivers,
        never_return: &input.never_return,
    };
    let entered: BTreeSet<u64> = wrappers.union(&input.receivers).copied().collect();
    let run = |entry: Entry<Block>,
               entered: &BTreeSet<u64>,
               evaluations: &BTreeMap<u64, Block>,
               reads: &BTreeMap<u64, (Block, Read)>| {
        match climb::entry_code(decoded, entered, entry.function) {
            Some(code) => {
                let found = runner.read_calls(
                    &code,
                    entry.function,
                    entry.site,
                    entry.selected,
                    &calls,
                    &entry.reads(reads),
                );
                entry_run(entry, found, evaluations)
            }
            None => EntryRun {
                unresolved: vec![Unresolved::new("site-not-decoded")],
                ..entry_run(entry, CallReads::default(), evaluations)
            },
        }
    };

    let shared_reads = reads(evaluations);
    for entry in entries {
        record(result, run(entry, &entered, evaluations, &shared_reads));
    }
    // A helper runs only as the selected call of a call that passes it a block, so it is not a
    // wrapper that other runs enter. Its evaluations read as the blocks that this call passes.
    for HelperCall {
        call,
        helper,
        evaluations,
    } in helper_calls
    {
        let entry = Entry {
            function: call.function,
            site: call.address,
            selected: Selected::Wrapper(helper),
            reaches: evaluations.values().cloned().collect(),
        };
        let entered: BTreeSet<u64> = entered.iter().copied().chain([helper]).collect();
        record(
            result,
            run(entry, &entered, &evaluations, &reads(&evaluations)),
        );
    }
}

/// The read of each evaluation call, with the block that it evaluates: the scope in
/// [`EVALUATED_SCOPE`], with no subject.
fn reads(evaluations: &BTreeMap<u64, Block>) -> BTreeMap<u64, (Block, Read)> {
    evaluations
        .iter()
        .map(|(&site, block)| {
            let read = Read {
                scope: EVALUATED_SCOPE,
                subject: Subject::None,
            };
            (site, (block.clone(), read))
        })
        .collect()
}

/// Record the contexts that `run` reached, and charge its reasons to the blocks that it reaches.
fn record(result: &mut BlockEntries, run: EntryRun) {
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
