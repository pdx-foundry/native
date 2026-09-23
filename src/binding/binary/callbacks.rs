//! Read the input of the callback method: every direct call that fires an on_action or evaluates
//! a game rule, the functions that hold those calls, and the scope functions that the method
//! runs.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::engine::analysis::{
    callbacks::{
        CallbacksInput, Forwarder, ForwarderKind, Pulse, RuleFamily, RuleTables, ScopeFunctions,
        Site, SiteCall, SiteScope, StringFunctions,
    },
    decode::{Instruction, decode_arm64},
    discovery::Symbol,
};

use super::super::targets::DeclarationRecipe;
use super::declarations::{Text, addresses, read_only_data, unique};

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
    recipe: &DeclarationRecipe,
) -> Result<CallbacksInput, AnalysisError> {
    let text = Text::read(bytes, symbols)?;
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
    let scope_code = scope_functions
        .fresh_constructors
        .iter()
        .chain(&scope_functions.setters)
        .filter_map(|&start| decoded(&text, start))
        .flatten()
        .collect();

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
    let table_code = [initializer]
        .into_iter()
        .chain(finders.iter().map(|(_, finder)| *finder))
        .map(|start| decoded(&text, start).ok_or(AnalysisError::InvalidRange))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();

    Ok(CallbacksInput {
        functions,
        scope_code,
        scope_functions,
        strings: StringFunctions {
            from_literal: addresses(symbols, "CString::CString(char const*)"),
            copy: addresses(symbols, "CString::CString(CString const&)"),
            destructors: addresses(symbols, "CString::~CString()"),
            object_size: recipe.string_object_size as i64,
        },
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
        scope_names: text.scope_names(symbols, strings),
        data: read_only_data(bytes)?,
        layout: recipe.callbacks,
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
    let mut rows = Vec::new();
    for (index, chunk) in code.chunks(4096).enumerate() {
        rows.extend(decode_arm64(chunk, address + (index * 4096) as u64).ok()?);
    }
    Some(rows)
}
