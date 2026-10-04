//! Read the input of the callback method: every direct call that fires an on_action or evaluates
//! a game rule, the functions that hold those calls, and the scope functions that the method
//! runs. Read the input of the block method the same way: every direct call to a trigger
//! evaluator or an effect executor in a method of a registry owner, with the callers of those
//! methods.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::engine::analysis::{
    callbacks::{
        CallbacksInput, Forwarder, ForwarderKind, Pulse, RuleFamily, RuleTables, ScopeFunctions,
        Site, SiteCall, SiteScope, StringFunctions,
        blocks::{BlockInput, CALLER_DEPTH, CallSite, EvaluationSite, RECEIVER_DEPTH},
    },
    decode::{Instruction, decode_arm64},
    discovery::Symbol,
};

use super::super::targets::DeclarationRecipe;
use super::declarations::{Text, addresses, read_only_data, unique};
use super::families::never_return;
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
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
    imports: &BTreeMap<u64, String>,
    recipe: &DeclarationRecipe,
) -> Result<CallbacksInput, AnalysisError> {
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

    let scope_functions = scope_functions(symbols);
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
    Ok(CallbacksInput {
        functions,
        scope_code,
        scope_functions,
        strings: string_functions(symbols, recipe),
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
        data: read_only_data(bytes)?,
        layout: recipe.callbacks,
        arguments: argument_registers(symbols),
        call_arguments,
    })
}

/// The functions that evaluate a stored trigger block or run a stored effect block. Each takes
/// the block in `x0` and the scope in `x1`.
fn evaluator_names() -> Vec<String> {
    vec![
        "CTrigger::Evaluate(CEventScope&) const".into(),
        "CTrigger::Evaluate(CEventScope const&) const".into(),
        format!("CTrigger::EvaluateExtended(CEventScope&, {EXTRA} const&) const"),
        format!("CTrigger::EvaluateExtended(CEventScope const&, {EXTRA} const&) const"),
        "CEffect::Execute(CEventScope&) const".into(),
        format!("CEffect::ExecuteExtended(CEventScope&, {EXTRA} const&) const"),
        format!("CEffect::ExecuteExtended(CEventScope&, {EXTRA}&&) const"),
        "CRootEffect::Execute(CEventScope&) const".into(),
    ]
}

/// Read the block method's input for the registry owner types `owners`.
pub(in crate::binding) fn block_evaluations(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
    imports: &BTreeMap<u64, String>,
    owners: &BTreeSet<&str>,
    recipe: &DeclarationRecipe,
) -> Result<BlockInput, AnalysisError> {
    let text = Text::read(bytes, symbols)?;
    let function_of = |address: u64| text.starts.range(..=address).next_back().copied();

    let mut evaluators = BTreeSet::new();
    for name in evaluator_names() {
        let found = addresses(symbols, &name);
        if found.is_empty() {
            return Err(AnalysisError::InvalidRange);
        }
        evaluators.extend(found);
    }

    let owner_of: BTreeMap<u64, &str> = symbols
        .iter()
        .filter(|symbol| !symbol.name.contains(".cold."))
        .filter_map(|symbol| {
            let (class, _) = symbol.name.split_once("::")?;
            owners.get(class).map(|owner| (symbol.address, *owner))
        })
        .collect();
    let sites: Vec<EvaluationSite> = text
        .calls_into(&evaluators)
        .into_iter()
        .filter_map(|(address, _)| {
            let function = function_of(address)?;
            let owner = owner_of.get(&function)?;
            Some(EvaluationSite {
                address,
                function,
                owner: (*owner).to_string(),
            })
        })
        .collect();

    let mut level: BTreeSet<u64> = sites.iter().map(|site| site.function).collect();
    let mut held = level.clone();
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
        held.extend(&next);
        level = next;
    }

    let mut functions: BTreeMap<u64, Vec<Instruction>> = held
        .into_iter()
        .filter_map(|start| Some((start, decoded(&text, start)?)))
        .collect();
    let scope_functions = scope_functions(symbols);
    let scope_users = scope_users(symbols, &evaluators);
    let not_followed: BTreeSet<u64> = evaluators
        .iter()
        .chain(&scope_users)
        .chain(&scope_functions.fresh_constructors)
        .chain(&scope_functions.setters)
        .chain(&scope_functions.copies)
        .chain(&scope_functions.destructors)
        .chain(&scope_functions.readers)
        .copied()
        .collect();
    let receivers = decode_receivers(&text, symbols, &not_followed, &mut functions);

    Ok(BlockInput {
        sites,
        scope_users,
        receivers,
        never_return: never_return(symbols),
        call_arguments: import_call_arguments(&functions, imports),
        evaluators,
        functions,
        callers,
        scope_code: scope_code(&text, &scope_functions),
        scope_functions,
        strings: string_functions(symbols, recipe),
        data: read_only_data(bytes)?,
        layout: recipe.callbacks,
        scope_names: text.scope_names(symbols, strings).map(|table| table.names),
        arguments: argument_registers(symbols),
    })
}

/// Decode, into `functions`, the functions outside `not_followed` that receive a scope and that
/// the decoded functions call directly, up to [`RECEIVER_DEPTH`] calls away, and give them.
fn decode_receivers(
    text: &Text,
    symbols: &[Symbol],
    not_followed: &BTreeSet<u64>,
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
    let mut level: Vec<u64> = functions.keys().copied().collect();
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

/// What the stack probe reads: it takes its size in `x15` and no argument register.
const IMPORT_ARGUMENTS: &[(&str, usize)] = &[("___chkstk_darwin", 0)];

/// How many argument registers each call through an import pointer reads, for the imports in
/// [`IMPORT_ARGUMENTS`], by the call instruction. The call loads the pointer with `adrp` and
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
            if let Some((_, count)) = IMPORT_ARGUMENTS.iter().find(|(import, _)| import == name) {
                calls.insert(call.address, *count);
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
        .filter_map(|&start| decoded(text, start))
        .flatten()
        .collect()
}

/// How many argument registers, from `x0`, each function whose signature the symbol states
/// reads. A function whose count is not known is left out.
fn argument_registers(symbols: &[Symbol]) -> BTreeMap<u64, usize> {
    symbols
        .iter()
        .filter_map(|symbol| Some((symbol.address, registers_read(&symbol.name)?)))
        .collect()
}

/// The most argument registers that a function with this demangled name reads, counting
/// generously: a qualified name may be a member and take `this`, and an argument passed by
/// value that is not a plain number may take two registers. `None` for a name with no
/// parameter list, or one that takes a variable number of arguments.
fn registers_read(name: &str) -> Option<usize> {
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
    let (qualified, parameters) = (&close[..open], &close[open + 1..]);

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

/// The parts of `text` between its top-level commas.
fn top_level(text: &str) -> impl Iterator<Item = &str> {
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

/// A pointer, a reference or a plain number, which one register passes.
fn is_passed_in_one_register(parameter: &str) -> bool {
    const NUMBERS: &[&str] = &[
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
        "float",
        "double",
    ];
    parameter.ends_with('*')
        || parameter.ends_with('&')
        || parameter.ends_with("* const")
        || parameter.contains("(*)")
        || NUMBERS.contains(&parameter)
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

    ScopeFunctions {
        fresh_constructors: named(&[
            "CEventScope::CEventScope()",
            "CEventScope::CEventScope(int)",
            "CEventScope::CEventScope(CCrudeRandom const&)",
        ]),
        setters,
        copies: named(&[
            "CEventScope::CEventScope(CEventScope const&)",
            "CEventScope::CEventScope(CEventScope&&)",
            "CEventScope::operator=(CEventScope const&)",
            "CEventScope::operator=(CEventScope&&)",
            "CEventScope::Copy(CEventScope const&)",
            "CEventScope::CopyInternalScopes(CEventScope const&)",
            "CScopeObjectReference::CScopeObjectReference(CScopeObjectReference const&)",
            "CScopeObjectReference::operator=(CScopeObjectReference const&)",
        ]),
        destructors: named(&["CEventScope::~CEventScope()"]),
        readers: matching(&|name| is_scope_member(name) && name.ends_with(" const")),
    }
}

/// Decode one whole function; `None` when a part of it does not decode.
fn decoded(text: &Text, start: u64) -> Option<Vec<Instruction>> {
    let (address, code) = text.function(start).ok()?;
    decode_arm64(code, address).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{import_call_arguments, registers_read};
    use crate::engine::analysis::decode::Instruction;

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
            (0x5010, "_memcpy".to_string()),
        ]);

        assert_eq!(
            import_call_arguments(&BTreeMap::from([(0x1000, rows)]), &imports),
            BTreeMap::from([(0x1008, 0)])
        );
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
    fn a_name_without_a_fixed_parameter_list_gives_no_count() {
        assert_eq!(registers_read("_strlen"), None);
        assert_eq!(
            registers_read("CRandomLog::Log(char const*, unsigned int, char const*, ...)"),
            None
        );
    }
}
