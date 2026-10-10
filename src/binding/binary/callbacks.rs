//! Read the input of the callback method: every direct call that fires an on_action or evaluates
//! a game rule, the functions that hold those calls, and the scope functions that the method
//! runs, with the callers of each function whose site fires or evaluates a scope parameter. Read
//! the input of the block method the same way: every direct call to a trigger evaluator, an
//! effect executor, a weight evaluator or a tooltip builder and every call through a register, in
//! a function whose registers lead to a registry owner ([`super::type_pointers`]), with the vtable
//! slots that evaluate a block or build its tooltip, the offset getters that the decoded functions
//! call, and the callers of each function whose call the block method attributes.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::BlockFamily;
use crate::engine::analysis::{
    callbacks::{
        CallbacksInput, Forwarder, ForwarderKind, Pulse, RuleFamily, RuleTables, ScopeFunctions,
        Site, SiteCall, SiteScope, StringFunctions,
        blocks::{
            BlockInput, Evaluation, EvaluationCall, EvaluationSite, NamedFunctions, RECEIVER_DEPTH,
            TypePointers, named_functions,
        },
        climb::{CALLER_DEPTH, CallSite},
        climbing_functions,
    },
    decode::{
        Instruction, decode_arm64, general_register, reads_before_writing, written_registers,
    },
    discovery::Symbol,
    evaluate::ReadOnlyData,
};

use super::super::targets::DeclarationRecipe;
use super::declarations::{Text, addresses, read_only_data, unique};
use super::families::{Names, never_return, vtable_group};
use super::instances::instances;
use super::references::Image;
use super::type_pointers::TypeOwners;
use crate::engine::analysis::declarations::number;

const EXTRA: &str = "CPdxUnorderedMap<EEffectUserDataKey, unsigned long long, SPdxHash<EEffectUserDataKey, void>, std::__1::equal_to<EEffectUserDataKey>, false>";

/// The engine functions that the method starts from, with the registers of their arguments.
fn anchors() -> Vec<(String, SiteCall)> {
    vec![
        (
            format!(
                "COnActionDatabase::PerformEvent(CString const&, CEventScope&, EEventExecutionMode, {EXTRA} const*) const"
            ),
            SiteCall::Fire {
                name: 1,
                scope: SiteScope::Register(2),
            },
        ),
        (
            format!(
                "COnActionCommand::COnActionCommand(CString const&, CEventScope const&, EEventExecutionMode, {EXTRA}&&)"
            ),
            SiteCall::Fire {
                name: 1,
                scope: SiteScope::Register(2),
            },
        ),
        (
            format!(
                "COnActionCommand::COnActionCommand(TPdxRef<CCountry> const&, CString const&, EEventExecutionMode, {EXTRA}&&)"
            ),
            SiteCall::Fire {
                name: 2,
                scope: SiteScope::BuiltByCallee,
            },
        ),
        (
            "COnActionDatabase::GetOnActionList(CString const&) const".into(),
            SiteCall::Lookup { name: 1 },
        ),
        (
            format!(
                "COnActionDatabase::PerformEvent(COnActionList const*, CEventScope&, EEventExecutionMode, {EXTRA} const*) const"
            ),
            SiteCall::FireList { list: 1, scope: 2 },
        ),
        (
            "CScriptedRule::Evaluate(CEventScope&, CString*, CScriptedRule::EShowTooltip, bool) const"
                .into(),
            SiteCall::Rule {
                family: RuleFamily::Scripted,
                rule: 0,
                scope: 1,
            },
        ),
        (
            "CWeightedRule::Evaluate(CEventScope&, CString*) const".into(),
            SiteCall::Rule {
                family: RuleFamily::Weighted,
                rule: 0,
                scope: 1,
            },
        ),
    ]
}

/// Functions that take a name or rule from their caller. Each is checked by the method.
const FORWARDERS: &[(&str, ForwarderKind)] = &[
    (
        "NScriptUtil::FireFleetOnAction(CString const&, CEventScope&)",
        ForwarderKind::Fire {
            name: 0,
            scope: Some(1),
        },
    ),
    (
        "PerformEvent(CString const&, CCountry*, CCountry*)",
        ForwarderKind::Fire {
            name: 0,
            scope: None,
        },
    ),
    (
        "CGameRules::EvaluateWithCountryAsThis(NGameRules::EGameRule, CCountry const*, CString*) const",
        ForwarderKind::Rule {
            family: RuleFamily::Scripted,
            enumeration: 1,
        },
    ),
    (
        "CGameRules::EvaluateIsFounderSpeciesUsing(NGameRules::EGameRule, CCountry const&, CString*) const",
        ForwarderKind::Rule {
            family: RuleFamily::Scripted,
            enumeration: 1,
        },
    ),
    (
        "CGameRules::CanBuildStationTypeAroundInternal(NGameRules::EGameRule, CCountry const*, CDepositHolder const*, CString*) const",
        ForwarderKind::Rule {
            family: RuleFamily::Scripted,
            enumeration: 1,
        },
    ),
];

/// Functions whose own calls are not sites: the dispatch inside the database, and the command
/// that fires what its constructor was given.
const DISPATCH: &[&str] = &["COnActionCommand::Execute()"];

/// The effect with which script fires an on_action that it names.
const SCRIPT_FIRED: &str = "CFireOnActionEffect::";

/// Read the callback method's input.
pub(in crate::binding) fn callbacks(
    image: &Image<'_>,
    bound_slots: &BTreeSet<u64>,
    recipe: &DeclarationRecipe,
) -> Result<CallbacksInput, AnalysisError> {
    let Image {
        bytes,
        symbols,
        strings,
        imports,
        ..
    } = *image;
    let text = Text::read(bytes, symbols)?;
    let CallbackSites {
        sites,
        forwarders,
        script_fired_sites,
    } = callback_sites(&text, symbols)?;

    let pulse = Pulse {
        init: unique(symbols, "COnActionDatabase::Init()")?,
        // An import stub keeps its raw name when it does not demangle.
        string_compare: addresses(symbols, "_strcmp"),
        instance: unique(symbols, "COnActionDatabase::_pInstance")?,
    };

    let mut functions = BTreeMap::new();
    for function in sites
        .iter()
        .map(|site| site.function)
        .chain([pulse.init])
        .collect::<BTreeSet<_>>()
    {
        if let Some(rows) = decoded(&text, function) {
            functions.insert(function, rows);
        }
    }
    let string_functions = string_functions(symbols, recipe);
    let callers = direct_callers(
        &text,
        climbing_functions(&sites, &forwarders, &functions, &string_functions),
    );
    for call in callers.values().flatten() {
        if let std::collections::btree_map::Entry::Vacant(entry) = functions.entry(call.function)
            && let Some(rows) = decoded(&text, call.function)
        {
            entry.insert(rows);
        }
    }

    let mut scope_functions = scope_functions(symbols);
    scope_functions.factories =
        scope_factories(&text, functions.values().flatten(), &scope_functions);
    let scope_code = scope_code(&text, &scope_functions);

    let initializer = unique(symbols, "__GLOBAL__sub_I_game_rules.cpp")?;
    let finders = vec![
        (
            RuleFamily::Scripted,
            unique(symbols, "FindRuleDeclarationByEnum(NGameRules::EGameRule)")?,
        ),
        (
            RuleFamily::Weighted,
            unique(
                symbols,
                "FindWeightedRuleDeclarationByEnum(NGameRules::EWeightedGameRule)",
            )?,
        ),
    ];
    let table_functions =
        std::iter::once(initializer).chain(finders.iter().map(|&(_, finder)| finder));
    let mut table_code = Vec::new();
    for start in table_functions {
        let rows = decoded(&text, start).ok_or(AnalysisError::InvalidRange)?;
        table_code.extend(rows);
    }

    let call_arguments = import_call_arguments(&functions, imports);
    let ignores_x8 = callees_ignoring_x8(
        &text,
        symbols,
        functions.values().flatten().chain(&scope_code),
    );
    let instances = instances(&text, image, bound_slots, &functions)?;
    Ok(CallbacksInput {
        functions,
        callers,
        never_return: never_return(symbols),
        scope_code,
        scope_functions,
        strings: string_functions,
        lookups: addresses(
            symbols,
            "COnActionDatabase::GetOnActionList(CString const&) const",
        ),
        sites,
        forwarders,
        rule_owners: symbols
            .iter()
            .filter(|symbol| symbol.name.starts_with("CGameRules::"))
            .map(|symbol| symbol.address)
            .collect(),
        script_fired_sites,
        pulse: Some(pulse),
        rule_tables: RuleTables {
            code: table_code,
            initializer,
            finders,
        },
        tokens: text.token_names(symbols, strings)?,
        scope_names: text.scope_names(symbols, strings).map(|table| table.names),
        data: read_only_data(bytes)?.with_words(&instances.words),
        layout: recipe.callbacks,
        arguments: argument_registers(symbols),
        call_arguments,
        ignores_x8,
        instances: instances.vtables,
    })
}

/// The functions that evaluate a stored trigger or weight block or run a stored effect block,
/// with the family of the block. Each takes the block in `x0` and the scope in `x1`.
fn evaluator_names() -> Vec<(String, BlockFamily)> {
    use BlockFamily::{Effect, Trigger, Weight};
    vec![
        ("CTrigger::Evaluate(CEventScope&) const".into(), Trigger),
        (
            "CTrigger::Evaluate(CEventScope const&) const".into(),
            Trigger,
        ),
        (
            format!("CTrigger::EvaluateExtended(CEventScope&, {EXTRA} const&) const"),
            Trigger,
        ),
        (
            format!("CTrigger::EvaluateExtended(CEventScope const&, {EXTRA} const&) const"),
            Trigger,
        ),
        ("CEffect::Execute(CEventScope&) const".into(), Effect),
        (
            format!("CEffect::ExecuteExtended(CEventScope&, {EXTRA} const&) const"),
            Effect,
        ),
        (
            format!("CEffect::ExecuteExtended(CEventScope&, {EXTRA}&&) const"),
            Effect,
        ),
        ("CRootEffect::Execute(CEventScope&) const".into(), Effect),
        (
            "CAndTrigger::ActualEvaluate(CEventScope&) const".into(),
            Trigger,
        ),
        (
            "SafeExecuteEffect(CEffect const&, CEventScope&)".into(),
            Effect,
        ),
        (
            format!("SafeExecuteEffectExtended(CEffect const&, CEventScope&, {EXTRA}&&)"),
            Effect,
        ),
        (
            "CMeanTimeToHappen::GetRawFactor(CEventScope&) const".into(),
            Weight,
        ),
        (
            "CMeanTimeToHappen::GetRawFactorNoScopeCopy(CEventScope const&) const".into(),
            Weight,
        ),
        (
            "CMeanTimeToHappen::GetChance(CEventScope&, int) const".into(),
            Weight,
        ),
        (
            "CMeanTimeToHappen::GetDailyChance(CEventScope&) const".into(),
            Weight,
        ),
        (
            "CAIMTTHChance::GetChance(CEventScope&) const".into(),
            Weight,
        ),
    ]
}

/// Read the block method's input for the registry owner types `owners`.
pub(in crate::binding) fn block_evaluations(
    image: &Image<'_>,
    bound_slots: &BTreeSet<u64>,
    owners: &BTreeSet<&str>,
    recipe: &DeclarationRecipe,
) -> Result<BlockInput, AnalysisError> {
    let Image {
        bytes,
        symbols,
        strings,
        imports,
        pointers,
    } = *image;
    let text = Text::read(bytes, symbols)?;
    let names = Names::new(symbols);
    let evaluators = with_families(symbols, evaluator_names())?;
    let constants = super::language::constant_data(bytes, pointers, bound_slots)?;
    let evaluation_slots = holding_slots(symbols, &constants, &names, &evaluators);
    let tooltip_builders = tooltip_builders(symbols);
    let tooltip_slots = holding_slots(
        symbols,
        &constants,
        &names,
        &base_tooltip_builders(symbols)?,
    );
    let polymorphic: BTreeSet<&str> = owners
        .iter()
        .copied()
        .filter(|owner| unique(symbols, &format!("vtable for {owner}")).is_ok())
        .collect();
    let type_owners = TypeOwners::read(&text, symbols, &polymorphic);
    let string_functions = string_functions(symbols, recipe);
    let evaluation = Evaluation {
        evaluators: &evaluators,
        slots: &evaluation_slots,
        tooltip_builders: &tooltip_builders,
        tooltip_slots: &tooltip_slots,
    };
    let Candidates {
        sites,
        mut type_pointers,
        mut functions,
        evaluating,
    } = candidates(&text, &names, &type_owners, &evaluation, &string_functions);

    let callers = direct_callers(&text, evaluating.clone());
    let calling: BTreeSet<u64> = callers
        .values()
        .flatten()
        .map(|call| call.function)
        .collect();
    for &caller in &calling {
        if let std::collections::btree_map::Entry::Vacant(entry) = type_pointers.entry(caller)
            && let Some(pointers) = type_owners.of(names.of(caller))
        {
            entry.insert(pointers);
        }
    }
    let held: BTreeSet<u64> = evaluating.into_iter().chain(calling).collect();
    for &start in &held {
        if let std::collections::btree_map::Entry::Vacant(entry) = functions.entry(start)
            && let Some(rows) = decoded(&text, start)
        {
            entry.insert(rows);
        }
    }
    let evaluator_addresses: BTreeSet<u64> = evaluators.keys().copied().collect();
    let mut scope_functions = scope_functions(symbols);
    let scope_users = scope_users(symbols, &evaluator_addresses);
    let not_followed: BTreeSet<u64> = evaluator_addresses
        .iter()
        .chain(&scope_users)
        .chain(&scope_functions.fresh_constructors)
        .chain(&scope_functions.setters)
        .chain(&scope_functions.copy_constructors)
        .chain(&scope_functions.internal_copies)
        .chain(&scope_functions.copies)
        .chain(&scope_functions.destructors)
        .chain(&scope_functions.readers)
        .copied()
        .collect();
    let receivers = decode_receivers(&text, symbols, &not_followed, &held, &mut functions);
    scope_functions.factories =
        scope_factories(&text, functions.values().flatten(), &scope_functions);
    let scope_code = scope_code(&text, &scope_functions);
    let ignores_x8 = callees_ignoring_x8(
        &text,
        symbols,
        functions.values().flatten().chain(&scope_code),
    );
    let instances = instances(&text, image, bound_slots, &functions)?;

    Ok(BlockInput {
        sites,
        evaluators,
        evaluation_slots,
        tooltip_builders,
        tooltip_slots,
        offset_getters: offset_getters(&text, &functions),
        type_pointers,
        scope_users,
        receivers,
        never_return: never_return(symbols),
        call_arguments: import_call_arguments(&functions, imports),
        functions,
        callers,
        scope_code,
        scope_functions,
        strings: string_functions,
        data: read_only_data(bytes)?.with_words(&instances.words),
        layout: recipe.callbacks,
        scope_names: text.scope_names(symbols, strings).map(|table| table.names),
        arguments: argument_registers(symbols),
        ignores_x8,
        instances: instances.vtables,
    })
}

/// The member that builds the tooltip of a trigger block. It evaluates nothing.
const BUILD_TOOLTIP: &str = "BuildToolTip(CEventScope&, bool, int, CSimpleBitMask<NTriggerTooltip::EOptions, NTriggerTooltip::EOptions>) const";

/// Every class's [`BUILD_TOOLTIP`], with the trigger family.
fn tooltip_builders(symbols: &[Symbol]) -> BTreeMap<u64, BlockFamily> {
    let member = format!("::{BUILD_TOOLTIP}");
    symbols
        .iter()
        .filter(|symbol| symbol.name.ends_with(&member))
        .map(|symbol| (symbol.address, BlockFamily::Trigger))
        .collect()
}

/// The [`BUILD_TOOLTIP`] of the trigger base classes, whose vtables give the tooltip slot. A
/// builder that the build lacks is an error.
fn base_tooltip_builders(symbols: &[Symbol]) -> Result<BTreeMap<u64, BlockFamily>, AnalysisError> {
    let names = ["CTrigger", "CAndTrigger"]
        .map(|class| (format!("{class}::{BUILD_TOOLTIP}"), BlockFamily::Trigger));
    with_families(symbols, names.into())
}

/// The address of each function of `named`, with its block family. A function that the build
/// lacks is an error.
fn with_families(
    symbols: &[Symbol],
    named: Vec<(String, BlockFamily)>,
) -> Result<BTreeMap<u64, BlockFamily>, AnalysisError> {
    let mut functions = BTreeMap::new();
    for (name, family) in named {
        let found = addresses(symbols, &name);
        if found.is_empty() {
            return Err(AnalysisError::InvalidRange);
        }
        functions.extend(found.into_iter().map(|address| (address, family)));
    }
    Ok(functions)
}

/// The block family of each vtable slot that holds one of `functions`: each slot of a function's
/// class's primary vtable that holds one, by its displacement from the address point. A
/// displacement that functions of two families hold is left out.
fn holding_slots(
    symbols: &[Symbol],
    data: &ReadOnlyData,
    names: &Names<'_>,
    functions: &BTreeMap<u64, BlockFamily>,
) -> BTreeMap<i64, BlockFamily> {
    let classes: BTreeSet<&str> = functions
        .keys()
        .flat_map(|&function| names.of(function))
        .filter_map(|name| Some(name.split_once("::")?.0))
        .collect();
    let mut families: BTreeMap<i64, BTreeSet<BlockFamily>> = BTreeMap::new();
    for class in classes {
        let Some(group) = vtable_group(symbols, data, class) else {
            continue;
        };
        for (&displacement, slot) in &group.primary {
            if let Some(&family) = functions.get(slot) {
                families
                    .entry(displacement as i64)
                    .or_default()
                    .insert(family);
            }
        }
    }

    families
        .into_iter()
        .filter_map(
            |(displacement, families)| match families.into_iter().collect::<Vec<_>>()[..] {
                [family] => Some((displacement, family)),
                _ => None,
            },
        )
        .collect()
}

/// The sites that the block method reads, with the functions that hold them.
struct Candidates {
    /// The direct evaluator calls and calls through a register in the kept functions.
    sites: Vec<EvaluationSite>,
    /// The type pointers of each kept function.
    type_pointers: BTreeMap<u64, TypePointers>,
    /// Each kept function, decoded.
    functions: BTreeMap<u64, Vec<Instruction>>,
    /// The kept functions that hold an evaluation that the block method attributes or a helper's
    /// evaluation.
    evaluating: BTreeSet<u64>,
}

/// The direct evaluator calls and calls through a register in the functions whose registers lead
/// to an owner. Of those functions, keep each that holds a call that the block method names a
/// block at, and each owner's method with a direct evaluator call, which the block method counts
/// when it names no block.
fn candidates(
    text: &Text,
    names: &Names<'_>,
    type_owners: &TypeOwners<'_>,
    evaluation: &Evaluation<'_>,
    strings: &StringFunctions,
) -> Candidates {
    let direct: BTreeSet<u64> = evaluation
        .evaluators
        .keys()
        .chain(evaluation.tooltip_builders.keys())
        .copied()
        .collect();
    let calls = text
        .calls_into(&direct)
        .into_iter()
        .map(|(address, target)| (address, EvaluationCall::Direct(target)))
        .chain(
            text.register_calls()
                .into_iter()
                .map(|(address, register)| (address, EvaluationCall::Register(register))),
        );
    let mut found: BTreeMap<u64, Option<TypePointers>> = BTreeMap::new();
    let mut sites = Vec::new();
    for (address, call) in calls {
        let Some(&function) = text.starts.range(..=address).next_back() else {
            continue;
        };
        let pointers = found
            .entry(function)
            .or_insert_with(|| type_owners.of(names.of(function)));
        if pointers.is_some() {
            sites.push(EvaluationSite {
                address,
                function,
                call,
            });
        }
    }
    let mut type_pointers: BTreeMap<u64, TypePointers> = found
        .into_iter()
        .filter_map(|(function, pointers)| Some((function, pointers?)))
        .collect();

    let mut functions: BTreeMap<u64, Vec<Instruction>> = type_pointers
        .keys()
        .filter_map(|&start| Some((start, decoded(text, start)?)))
        .collect();
    let getters = offset_getters(text, &functions);
    let NamedFunctions {
        evaluating,
        tooltips,
    } = named_functions(
        &sites,
        &functions,
        &type_pointers,
        evaluation,
        strings,
        &getters,
    );
    let counted: BTreeSet<u64> = sites
        .iter()
        .filter(|site| site.calls_evaluator(evaluation.evaluators))
        .filter(|site| type_pointers[&site.function].method_of.is_some())
        .map(|site| site.function)
        .collect();
    let kept = |function: &u64| {
        evaluating.contains(function) || tooltips.contains(function) || counted.contains(function)
    };
    sites.retain(|site| kept(&site.function));
    functions.retain(|function, _| kept(function));
    type_pointers.retain(|function, _| kept(function));

    Candidates {
        sites,
        type_pointers,
        functions,
        evaluating,
    }
}

/// The offset getters that `functions` call directly: each function whose whole body is
/// `add x0, x0, #k; ret`, with its `k`.
fn offset_getters(text: &Text, functions: &BTreeMap<u64, Vec<Instruction>>) -> BTreeMap<u64, i64> {
    let called: BTreeSet<u64> = functions
        .values()
        .flatten()
        .filter(|row| row.operation == "bl")
        .filter_map(|row| number(&row.operands))
        .collect();
    called
        .into_iter()
        .filter_map(|target| {
            let code = text.bytes(target, 8).ok()?;
            let rows = decode_arm64(code, target).ok()?;
            Some((target, getter_offset(&rows)?))
        })
        .collect()
}

/// The `k` of a function whose first two instructions, `rows`, are `add x0, x0, #k; ret`.
fn getter_offset(rows: &[Instruction]) -> Option<i64> {
    let [add, ret, ..] = rows else {
        return None;
    };
    if add.operation != "add" || ret.operation != "ret" || !ret.operands.is_empty() {
        return None;
    }
    let offset = add.operands.strip_prefix("x0,x0,")?;
    number(offset).map(|offset| offset as i64)
}

/// The direct calls to each of `functions` and to its callers short of [`CALLER_DEPTH`], other
/// than a function's calls to itself.
fn direct_callers(text: &Text, functions: BTreeSet<u64>) -> BTreeMap<u64, Vec<CallSite>> {
    let function_of = |address: u64| text.starts.range(..=address).next_back().copied();
    let mut level = functions;
    let mut callers: BTreeMap<u64, Vec<CallSite>> = BTreeMap::new();
    for _ in 0..CALLER_DEPTH {
        let mut next = BTreeSet::new();
        for (address, target) in text.calls_into(&level) {
            let Some(function) = function_of(address).filter(|function| *function != target) else {
                continue;
            };
            callers
                .entry(target)
                .or_default()
                .push(CallSite { address, function });
            next.insert(function);
        }
        level = next;
    }
    callers
}

/// Decode, into `functions`, the functions outside `not_followed` that receive a scope and that
/// the functions of `from` call directly, up to [`RECEIVER_DEPTH`] calls away, and give them.
fn decode_receivers(
    text: &Text,
    symbols: &[Symbol],
    not_followed: &BTreeSet<u64>,
    from: &BTreeSet<u64>,
    functions: &mut BTreeMap<u64, Vec<Instruction>>,
) -> BTreeSet<u64> {
    let receives_scope: BTreeSet<u64> = symbols
        .iter()
        .filter(|symbol| {
            !symbol.name.contains(".cold.")
                && symbol
                    .name
                    .split_once('(')
                    .is_some_and(|(_, parameters)| parameters.contains("CEventScope"))
        })
        .map(|symbol| symbol.address)
        .filter(|address| !not_followed.contains(address))
        .collect();

    let mut receivers = BTreeSet::new();
    let mut level: Vec<u64> = from.iter().copied().collect();
    for _ in 0..RECEIVER_DEPTH {
        let called: BTreeSet<u64> = level
            .iter()
            .filter_map(|function| functions.get(function))
            .flatten()
            .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
            .filter_map(|row| number(&row.operands))
            .filter(|target| receives_scope.contains(target) && !receivers.contains(target))
            .collect();
        level = Vec::new();
        for start in called {
            if let std::collections::btree_map::Entry::Vacant(entry) = functions.entry(start) {
                let Some(rows) = decoded(text, start) else {
                    continue;
                };
                entry.insert(rows);
            }
            receivers.insert(start);
            level.push(start);
        }
    }
    receivers
}

/// How many argument registers the C library functions that the decoded code calls read, by
/// their raw names, which have no parameter list. The C and POSIX standards fix these
/// signatures. `_fmodf` takes its arguments in floating-point registers, and the stack probe
/// takes its size in `x15`.
const LIBRARY_ARGUMENTS: &[(&str, usize)] = &[
    ("___chkstk_darwin", 0),
    ("_bzero", 2),
    ("_fmodf", 0),
    ("_memcmp", 3),
    ("_memcpy", 3),
    ("_memmove", 3),
    ("_memset", 3),
    ("_strcmp", 2),
    ("_strlen", 1),
];

/// How many argument registers the C library function `name` reads, from [`LIBRARY_ARGUMENTS`].
fn library_arguments(name: &str) -> Option<usize> {
    LIBRARY_ARGUMENTS
        .iter()
        .find(|(function, _)| *function == name)
        .map(|(_, count)| *count)
}

/// How many argument registers each call through an import pointer reads, for the imports in
/// [`LIBRARY_ARGUMENTS`], by the call instruction. The call loads the pointer with `adrp` and
/// `ldr` into the register that it calls through.
fn import_call_arguments(
    functions: &BTreeMap<u64, Vec<Instruction>>,
    imports: &BTreeMap<u64, String>,
) -> BTreeMap<u64, usize> {
    let mut calls = BTreeMap::new();
    for rows in functions.values() {
        for window in rows.windows(3) {
            let [page, load, call] = window else {
                continue;
            };
            if page.operation != "adrp" || load.operation != "ldr" || call.operation != "blr" {
                continue;
            }

            let register = call.operands.as_str();
            let Some(page_target) = page
                .operands
                .strip_prefix(&format!("{register},"))
                .and_then(number)
            else {
                continue;
            };
            let Some(offset) = load
                .operands
                .strip_prefix(&format!("{register},[{register},"))
                .and_then(|rest| rest.strip_suffix(']'))
                .and_then(number)
            else {
                continue;
            };
            let Some(name) = imports.get(&(page_target + offset)) else {
                continue;
            };
            if let Some(count) = library_arguments(name) {
                calls.insert(call.address, count);
            }
        }
    }
    calls
}

/// The `const` members of trigger and effect classes, other than `evaluators`, that receive a
/// scope, such as `CAndTrigger::BuildToolTip(CEventScope&, …) const`.
fn scope_users(symbols: &[Symbol], evaluators: &BTreeSet<u64>) -> BTreeSet<u64> {
    symbols
        .iter()
        .filter(|symbol| !evaluators.contains(&symbol.address))
        .filter(|symbol| {
            let name = symbol.name.as_str();
            let Some((class, member)) = name.split_once("::") else {
                return false;
            };
            (class.ends_with("Trigger") || class.ends_with("Effect"))
                && member.contains("CEventScope")
                && name.ends_with(" const")
        })
        .map(|symbol| symbol.address)
        .collect()
}

/// The decoded code of the scope functions that the context pass runs.
fn scope_code(text: &Text, scope_functions: &ScopeFunctions) -> Vec<Instruction> {
    scope_functions
        .fresh_constructors
        .iter()
        .chain(&scope_functions.setters)
        .chain(&scope_functions.factories)
        .filter_map(|&start| decoded(text, start))
        .flatten()
        .collect()
}

/// The targets of the direct calls in `rows` that build a scope in the object that `x8`
/// addresses: they pass the `x8` that they receive to a scope constructor as its object.
fn scope_factories<'r>(
    text: &Text,
    rows: impl Iterator<Item = &'r Instruction>,
    scope_functions: &ScopeFunctions,
) -> BTreeSet<u64> {
    let constructors: BTreeSet<u64> = scope_functions
        .fresh_constructors
        .union(&scope_functions.copy_constructors)
        .copied()
        .collect();
    let targets: BTreeSet<u64> = rows
        .filter(|row| row.operation == "bl")
        .filter_map(|row| number(&row.operands))
        .collect();

    targets
        .into_iter()
        .filter(|&target| {
            decoded(text, target).is_some_and(|callee| constructs_at_x8(&callee, &constructors))
        })
        .collect()
}

/// Whether `rows`, one whole function, calls one of `constructors` with the `x8` that it
/// received in `x0`. The scan follows the rows in address order, not the branches: a factory
/// only selects code that the pass runs on the path, so a false match costs time and a missed
/// one keeps the scope unfollowed.
fn constructs_at_x8(rows: &[Instruction], constructors: &BTreeSet<u64>) -> bool {
    let mut holders = BTreeSet::from([8]);
    for row in rows {
        let operation = row.operation.as_str();
        let target = number(&row.operands);
        if matches!(operation, "bl" | "b")
            && target.is_some_and(|target| constructors.contains(&target))
            && holders.contains(&0)
        {
            return true;
        }

        let mut copy_holder = None;
        if operation == "mov"
            && let Some((destination, source)) = row.operands.split_once(',')
            && general_register(source).is_some_and(|source| holders.contains(&source))
        {
            copy_holder = general_register(destination);
        }

        for register in written_registers(operation, &row.operands) {
            holders.remove(&register);
        }
        holders.extend(copy_holder);
    }
    false
}

/// The targets of the direct calls in `rows` that ignore the `x8` that they receive: no path
/// reads it before writing it, or the target is the stub of a function in [`LIBRARY_ARGUMENTS`],
/// which returns no object in memory. A demangled name does not state whether a function returns
/// an object in memory, so its code decides whether a call to it receives `x8`.
fn callees_ignoring_x8<'r>(
    text: &Text,
    symbols: &[Symbol],
    rows: impl Iterator<Item = &'r Instruction>,
) -> BTreeSet<u64> {
    let stubs: BTreeSet<u64> = symbols
        .iter()
        .filter(|symbol| library_arguments(&symbol.name).is_some())
        .map(|symbol| symbol.address)
        .collect();
    let targets: BTreeSet<u64> = rows
        .filter(|row| row.operation == "bl")
        .filter_map(|row| number(&row.operands))
        .collect();

    targets
        .into_iter()
        .filter(|&target| {
            stubs.contains(&target)
                || decoded(text, target).is_some_and(|callee| !reads_before_writing(&callee, 8))
        })
        .collect()
}

/// How many argument registers, from `x0`, each function reads whose signature the symbol states
/// or whose stub [`LIBRARY_ARGUMENTS`] lists. A function whose count is not known is left out.
fn argument_registers(symbols: &[Symbol]) -> BTreeMap<u64, usize> {
    symbols
        .iter()
        .filter_map(|symbol| {
            let count = registers_read(&symbol.name).or_else(|| library_arguments(&symbol.name))?;
            Some((symbol.address, count))
        })
        .collect()
}

/// The most argument registers that a function with this demangled name reads, counting
/// generously: a qualified name may be a member and take `this`, and an argument passed by
/// value that is not a plain number may take two registers. `None` for a name with no
/// parameter list, or one that takes a variable number of arguments.
fn registers_read(name: &str) -> Option<usize> {
    let (qualified, parameters) = signature(name)?;
    let receiver = usize::from(top_level(qualified).any(|part| part.contains("::")));
    let mut count = receiver;
    for parameter in top_level(parameters).map(str::trim) {
        count += match parameter {
            "" | "void" => 0,
            "..." => return None,
            parameter if is_passed_in_one_register(parameter) => 1,
            _ => 2,
        };
    }
    Some(count)
}

/// The qualified name and the parameter list of the demangled function name `name`:
/// `C::F(A, B) const` gives `C::F` and `A, B`. `None` for a name with no parameter list.
pub(super) fn signature(name: &str) -> Option<(&str, &str)> {
    let name = name.split(" [clone").next()?;
    let mut end = name.trim_end();
    for qualifier in [" const", " volatile", " &&", " &"] {
        end = end.strip_suffix(qualifier).unwrap_or(end);
    }
    let close = end.strip_suffix(')')?;

    let mut depth = 0usize;
    let mut open = None;
    for (index, character) in close.char_indices().rev() {
        match character {
            ')' | '>' => depth += 1,
            '(' | '<' if depth > 0 => depth -= 1,
            '(' => {
                open = Some(index);
                break;
            }
            _ => {}
        }
    }
    let open = open?;
    Some((&close[..open], &close[open + 1..]))
}

/// The parts of `text` between its top-level commas.
pub(super) fn top_level(text: &str) -> impl Iterator<Item = &str> {
    let mut depth = 0usize;
    let mut start = 0;
    let mut parts = Vec::new();
    for (index, character) in text.char_indices() {
        match character {
            '(' | '<' | '[' => depth += 1,
            ')' | '>' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts.into_iter()
}

/// The integer types, with `bool`, that one general register passes.
pub(super) const INTEGERS: &[&str] = &[
    "bool",
    "char",
    "signed char",
    "unsigned char",
    "wchar_t",
    "short",
    "unsigned short",
    "int",
    "unsigned int",
    "long",
    "unsigned long",
    "long long",
    "unsigned long long",
];

/// A pointer, a reference or a plain number, which one register passes.
fn is_passed_in_one_register(parameter: &str) -> bool {
    parameter.ends_with('*')
        || parameter.ends_with('&')
        || parameter.ends_with("* const")
        || parameter.contains("(*)")
        || INTEGERS.contains(&parameter)
        || matches!(parameter, "float" | "double")
}

/// The string functions that the name pass follows.
fn string_functions(symbols: &[Symbol], recipe: &DeclarationRecipe) -> StringFunctions {
    StringFunctions {
        from_literal: addresses(symbols, "CString::CString(char const*)"),
        copy: addresses(symbols, "CString::CString(CString const&)"),
        destructors: addresses(symbols, "CString::~CString()"),
        object_size: recipe.string_object_size as i64,
    }
}

/// The direct calls that fire an on_action or evaluate a game rule, and what they reach.
struct CallbackSites {
    sites: Vec<Site>,
    /// The forwarders found in this build; a forwarded site holds its index here.
    forwarders: Vec<Forwarder>,
    /// Calls inside the effect that script uses to fire an on_action it names.
    script_fired_sites: usize,
}

/// Find every direct call to an anchor or a forwarder, except calls inside the database's own
/// dispatch and inside the script effect. An anchor that the build lacks is an error; a
/// forwarder without exactly one address is skipped.
fn callback_sites(text: &Text, symbols: &[Symbol]) -> Result<CallbackSites, AnalysisError> {
    let names: BTreeMap<u64, &str> = symbols
        .iter()
        .map(|symbol| (symbol.address, symbol.name.as_str()))
        .collect();
    let function_of = |address: u64| text.starts.range(..=address).next_back().copied();

    let mut targets: Vec<(u64, SiteCall)> = Vec::new();
    for (name, call) in anchors() {
        let found = addresses(symbols, &name);
        if found.is_empty() {
            return Err(AnalysisError::InvalidRange);
        }
        targets.extend(found.into_iter().map(|address| (address, call)));
    }
    let anchor_bodies: BTreeSet<u64> = targets.iter().map(|(address, _)| *address).collect();

    let mut forwarders = Vec::new();
    for (name, kind) in FORWARDERS {
        if let Ok(function) = unique(symbols, name) {
            targets.push((
                function,
                SiteCall::Forwarded {
                    forwarder: forwarders.len(),
                },
            ));
            forwarders.push(Forwarder {
                function,
                kind: *kind,
            });
        }
    }

    let dispatch: BTreeSet<u64> = DISPATCH
        .iter()
        .flat_map(|name| addresses(symbols, name))
        .chain(anchor_bodies.iter().copied())
        .collect();

    let mut sites = Vec::new();
    let mut script_fired_sites = 0;
    for (target, call) in targets {
        for address in text.branches_to(target) {
            let Some(function) = function_of(address) else {
                continue;
            };
            if dispatch.contains(&function) {
                continue;
            }
            if names
                .get(&function)
                .is_some_and(|name| name.starts_with(SCRIPT_FIRED))
            {
                script_fired_sites += 1;
                continue;
            }
            sites.push(Site {
                address,
                function,
                call,
            });
        }
    }

    Ok(CallbackSites {
        sites,
        forwarders,
        script_fired_sites,
    })
}

/// The scope functions, found by their class and signature.
fn scope_functions(symbols: &[Symbol]) -> ScopeFunctions {
    let named = |names: &[&str]| -> BTreeSet<u64> {
        names
            .iter()
            .flat_map(|name| addresses(symbols, name))
            .collect()
    };
    let matching = |select: &dyn Fn(&str) -> bool| -> BTreeSet<u64> {
        symbols
            .iter()
            .filter(|symbol| !symbol.name.contains(".cold.") && select(&symbol.name))
            .map(|symbol| symbol.address)
            .collect()
    };
    let is_scope_member = |name: &str| {
        name.starts_with("CEventScope::") || name.starts_with("CScopeObjectReference::")
    };

    let mut setters = matching(&|name| name.starts_with("CScopeObjectReference::Set"));
    setters.extend(named(&["CEventScope::ClearRootFromPrev()"]));

    // Firing an on_action runs its event in place and keeps only copies of the scope, a game
    // rule only evaluates its trigger or weight, and `AccessVariables` writes only the variables
    // container: none changes a type or a link.
    let reading: Vec<String> = anchors()
        .into_iter()
        .filter(|(_, call)| {
            matches!(
                call,
                SiteCall::Fire {
                    scope: SiteScope::Register(_),
                    ..
                } | SiteCall::FireList { .. }
                    | SiteCall::Rule { .. }
            )
        })
        .map(|(name, _)| name)
        .collect();
    let mut readers = matching(&|name| is_scope_member(name) && name.ends_with(" const"));
    readers.extend(named(&["CEventScope::AccessVariables()"]));
    readers.extend(reading.iter().flat_map(|name| addresses(symbols, name)));

    ScopeFunctions {
        fresh_constructors: named(&[
            "CEventScope::CEventScope()",
            "CEventScope::CEventScope(int)",
            "CEventScope::CEventScope(CCrudeRandom const&)",
        ]),
        setters,
        copy_constructors: named(&["CEventScope::CEventScope(CEventScope const&)"]),
        internal_copies: named(&["CEventScope::CopyInternalScopes(CEventScope const&)"]),
        copies: named(&[
            "CEventScope::CEventScope(CEventScope&&)",
            "CEventScope::operator=(CEventScope const&)",
            "CEventScope::operator=(CEventScope&&)",
            "CEventScope::Copy(CEventScope const&)",
            "CScopeObjectReference::CScopeObjectReference(CScopeObjectReference const&)",
            "CScopeObjectReference::operator=(CScopeObjectReference const&)",
        ]),
        factories: BTreeSet::new(),
        destructors: named(&["CEventScope::~CEventScope()"]),
        readers,
    }
}

/// Decode one whole function; `None` when a part of it does not decode.
pub(super) fn decoded(text: &Text, start: u64) -> Option<Vec<Instruction>> {
    let (address, code) = text.function(start).ok()?;
    decode_arm64(code, address).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{
        argument_registers, constructs_at_x8, getter_offset, import_call_arguments, registers_read,
    };
    use crate::engine::analysis::assembler::arm64;
    use crate::engine::analysis::decode::{Instruction, decode_arm64};
    use crate::engine::analysis::discovery::Symbol;

    #[test]
    fn selected_virtual_evaluations_keep_their_functions_for_context_decoding() {
        use crate::BlockFamily;
        use crate::engine::analysis::callbacks::StringFunctions;
        use crate::engine::analysis::callbacks::blocks::{
            Evaluation, EvaluationCall, EvaluationSite, TypePointers, named_functions,
        };

        let bytes = arm64!(at 0x1000;
            add x8, x0, #0x40;
            ldr x9, [x0, #0x80];
            cmp x2, #0;
            csel x0, x8, x9, eq;
            ldr x8, [x0];
            ldr x8, [x8, #0x48];
            blr x8;
            ret
        );
        let rows = decode_arm64(&bytes, 0x1000).unwrap();
        let site = EvaluationSite {
            address: 0x1018,
            function: 0x1000,
            call: EvaluationCall::Register(8),
        };
        let pointers = TypePointers {
            method_of: Some("COwner".into()),
            registers: BTreeMap::from([(0, "COwner".into())]),
            members: BTreeMap::new(),
        };
        let named = named_functions(
            &[site],
            &BTreeMap::from([(0x1000, rows)]),
            &BTreeMap::from([(0x1000, pointers)]),
            &Evaluation {
                evaluators: &BTreeMap::new(),
                slots: &BTreeMap::from([(0x48, BlockFamily::Effect)]),
                tooltip_builders: &BTreeMap::new(),
                tooltip_slots: &BTreeMap::new(),
            },
            &StringFunctions::default(),
            &BTreeMap::new(),
        );
        assert_eq!(named.evaluating, BTreeSet::from([0x1000]));
        assert!(named.tooltips.is_empty());
    }

    fn row(address: u64, operation: &str, operands: &str) -> Instruction {
        Instruction {
            address,
            bytes: [0; 4],
            operation: operation.into(),
            operands: operands.into(),
        }
    }

    #[test]
    fn a_call_through_the_stack_probe_import_reads_no_argument_register() {
        let rows = vec![
            row(0x1000, "adrp", "x16,#0x5000"),
            row(0x1004, "ldr", "x16,[x16,#0xdc8]"),
            row(0x1008, "blr", "x16"),
            row(0x100c, "adrp", "x16,#0x5000"),
            row(0x1010, "ldr", "x16,[x16,#0x10]"),
            row(0x1014, "blr", "x16"),
            row(0x1018, "ldr", "x8,[x8,#0xdc8]"),
            row(0x101c, "blr", "x8"),
        ];
        let imports = BTreeMap::from([
            (0x5dc8, "___chkstk_darwin".to_string()),
            (0x5010, "_PMurHash32".to_string()),
        ]);

        assert_eq!(
            import_call_arguments(&BTreeMap::from([(0x1000, rows)]), &imports),
            BTreeMap::from([(0x1008, 0)])
        );
    }

    #[test]
    fn a_factory_passes_the_x8_that_it_received_to_a_scope_constructor() {
        let constructors = BTreeSet::from([0x8000]);
        let constructs = |copied_from: &str| {
            let rows = vec![
                row(0x1000, "mov", copied_from),
                row(0x1004, "bl", "#0x9900"),
                row(0x1008, "mov", "x0,x19"),
                row(0x100c, "bl", "#0x8000"),
                row(0x1010, "ret", ""),
            ];
            constructs_at_x8(&rows, &constructors)
        };

        assert!(constructs("x19,x8"));
        assert!(!constructs("x19,x1"));
        assert!(!constructs("x9,x8"));
    }

    #[test]
    fn a_signature_gives_the_argument_registers_that_it_reads() {
        assert_eq!(registers_read("AccessCheatManager()"), Some(0));
        assert_eq!(registers_read("CString::GetSize() const"), Some(1));
        assert_eq!(
            registers_read("CString::operator+=(CString const&)"),
            Some(2)
        );
        assert_eq!(
            registers_read(
                "CAndTrigger::BuildToolTip(CEventScope&, bool, int, CSimpleBitMask<NTriggerTooltip::EOptions, NTriggerTooltip::EOptions>) const"
            ),
            Some(6)
        );
        assert_eq!(
            registers_read("CTraditionType::IsPotential(CCountry const*) const [clone .cold.1]"),
            Some(2)
        );
        assert_eq!(
            registers_read(
                "(anonymous namespace)::ExecuteTradition(CTraditionType const&, CCountry&)"
            ),
            Some(3)
        );
    }

    #[test]
    fn a_c_library_stub_reads_the_registers_that_its_standard_signature_uses() {
        let symbols = [
            ("_strlen", 0x100),
            ("_memmove", 0x10c),
            ("_PMurHash32", 0x118),
        ]
        .map(|(name, address)| Symbol {
            name: name.into(),
            address,
        });

        assert_eq!(
            argument_registers(&symbols),
            BTreeMap::from([(0x100, 1), (0x10c, 3)])
        );
    }

    #[test]
    fn a_name_without_a_fixed_parameter_list_gives_no_count() {
        assert_eq!(registers_read("_strlen"), None);
        assert_eq!(
            registers_read("CRandomLog::Log(char const*, unsigned int, char const*, ...)"),
            None
        );
    }

    #[test]
    fn only_a_function_whose_whole_body_is_add_x0_and_ret_is_an_offset_getter() {
        let offset = |code: Vec<u8>| getter_offset(&decode_arm64(&code, 0x1000).unwrap());

        assert_eq!(offset(arm64!(at 0x1000; add x0, x0, #160; ret)), Some(0xa0));
        assert_eq!(
            offset(arm64!(at 0x1000; add x0, x0, #160; ldr x0, [x0]; ret)),
            None
        );
        assert_eq!(
            offset(arm64!(at 0x1000; mov x8, x1; add x0, x0, #160; ret)),
            None
        );
        assert_eq!(offset(arm64!(at 0x1000; add x0, x1, #160; ret)), None);
    }
}
