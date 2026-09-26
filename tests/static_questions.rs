//! Parity of the static questions with tracked expected output for the M45 build.
//! Needs the real executable: set `STELLARIS_PATH` and run with `--ignored`. No game starts.
use pdx_native::internals::{command_grammar_stops, registry_field_stops, trace_causes};
use pdx_native::{
    Answer, Basis, Completeness, ContextScopes, Declaration, DeclarationKind, DeclaredScopes,
    DeclaredTags, Define, EntryContext, EntryScope, Error, Field, GameRule, GapKind, LinkData,
    LocalizationContextReference, LocalizationDeclarations, LocalizationOutput,
    ModifierDeclaration, ModifierFamily, NamePart, Native, OnAction, Operation, OutputScope,
    ReaderKind, RuleKind, ScopeId, ScopeInventory, ScopeLink, ScopeReference,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn defines_match_the_recorded_m45_boundary() {
    #[derive(serde::Deserialize)]
    struct Expected {
        count: usize,
        gaps: BTreeMap<String, usize>,
        types: BTreeMap<String, usize>,
        samples: Vec<Define>,
    }
    let expected: Expected = expected("defines.json");
    let native = native();
    assert_eq!(
        native.supports(Operation::Defines),
        pdx_native::Support::Supported
    );
    let answer = native.defines().unwrap();
    assert_eq!(answer.source.basis, Basis::StaticAnalysis);
    assert_eq!(answer.source.method, "defines/v1");
    assert_eq!(answer.completeness, Completeness::Partial);
    assert_eq!(answer.value.len(), expected.count);
    assert_eq!(gap_counts(&answer), expected.gaps);
    let mut types = BTreeMap::new();
    for define in &answer.value {
        let kind = format!("{:?}", define.value_type);
        *types.entry(kind.clone()).or_insert(0usize) += 1;
    }
    assert_eq!(types, expected.types);
    for sample in expected.samples {
        assert!(
            answer.value.contains(&sample),
            "missing {}.{}",
            sample.namespace,
            sample.name
        );
    }
    assert!(answer.gaps.iter().any(|gap| {
        gap.subject.as_ref().map(|subject| subject.name()) == Some("NGraphics.ORBIT_HSV")
            && gap.detail == "reader path exceeds the table-search limit"
    }));
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn declarations_match_the_recorded_m45_inventory() {
    let native = native();
    let gap_counts: serde_json::Value = expected("declaration-gaps.json");
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        let subject = match kind {
            DeclarationKind::Effect => "effect",
            DeclarationKind::Trigger => "trigger",
            _ => unreachable!("the test covers the M45 effect and trigger kinds"),
        };
        let answer = native.declarations(kind).unwrap();
        assert_eq!(answer.source.basis, Basis::Declared);
        let names: std::collections::BTreeSet<_> =
            answer.value.iter().map(|item| item.name.clone()).collect();
        let omitted: std::collections::BTreeSet<_> = answer
            .gaps
            .iter()
            .filter(|gap| {
                gap.kind == GapKind::UnresolvedPath
                    && !names.contains(
                        gap.subject
                            .as_ref()
                            .map(|subject| subject.name())
                            .unwrap_or(""),
                    )
            })
            .filter_map(|gap| {
                gap.subject
                    .as_ref()
                    .map(|subject| subject.name().to_owned())
            })
            .collect();
        let accounted: Vec<_> = names.union(&omitted).cloned().collect();
        assert_eq!(
            omitted.into_iter().collect::<Vec<_>>(),
            gap_counts[subject]["unreadable"]
                .as_array()
                .unwrap()
                .iter()
                .map(|name| name.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            accounted,
            expected::<Vec<String>>(&format!("declarations-{subject}.json"))
        );
        assert_eq!(
            answer
                .gaps
                .iter()
                .filter(|gap| gap.kind == GapKind::UnnamedDeclaration)
                .count(),
            gap_counts[subject]["unnamed_registrations"]
                .as_u64()
                .unwrap() as usize
        );
        let recovered: Value = expected("declaration-recovered.json");
        assert_eq!(
            answer.value.len() as u64,
            recovered[subject]["sdk_488_live_inventory"]
                .as_u64()
                .unwrap()
        );
        for entry in recovered[subject]["recovered"].as_array().unwrap() {
            let sample: Declaration = serde_json::from_value(entry["declaration"].clone()).unwrap();
            assert_eq!(
                answer.value.iter().find(|item| item.name == sample.name),
                Some(&sample),
                "{} ({})",
                sample.name,
                entry["mechanism"]
            );
        }
        assert_eq!(
            answer.completeness,
            if answer
                .gaps
                .iter()
                .all(|gap| gap.kind == GapKind::OutsideMethod)
            {
                Completeness::Complete
            } else {
                Completeness::Partial
            }
        );
        let global_scope_gap = answer.gaps.iter().any(|gap| {
            gap.kind == GapKind::UnreadableInput && gap.detail == "scope name table not found"
        });
        for item in &answer.value {
            assert!(!item.description.contains("Supported Scopes:"));
            assert!(!item.usage.contains("Supported Scopes:"));
            if !global_scope_gap {
                assert_eq!(
                    item.scopes == DeclaredScopes::Unresolved,
                    answer
                        .gaps
                        .iter()
                        .any(|gap| gap.kind == GapKind::UnresolvedPath
                            && gap.subject.as_ref().map(|subject| subject.name())
                                == Some(&item.name)
                            && gap.detail.starts_with("scope declaration not followed at ")),
                    "{}",
                    item.name
                );
            }
        }
        let unresolved: Vec<_> = answer
            .value
            .iter()
            .filter(|item| item.scopes == DeclaredScopes::Unresolved)
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(
            serde_json::to_value(unresolved).unwrap(),
            gap_counts[subject]["unresolved_scopes"]
        );
        assert!(answer.gaps.iter().all(|gap| gap.subject.is_some()
            || gap.kind == GapKind::OutsideMethod
            || gap.kind == GapKind::UnreadableInput));
        let samples: Vec<Declaration> = expected(&format!("declaration-samples-{subject}.json"));
        assert_eq!(samples.len(), 10);
        for sample in samples {
            assert_eq!(
                answer.value.iter().find(|item| item.name == sample.name),
                Some(&sample)
            );
        }
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn declarations_give_the_scopes_of_known_m45_commands() {
    let native = native();
    let effects = native.declarations(DeclarationKind::Effect).unwrap();
    let win = find(&effects.value, "win", |item| &item.name);
    assert_eq!(listed_names(&win.scopes), ["country"]);
    let add_blocker = find(&effects.value, "add_blocker", |item| &item.name);
    assert!(listed_names(&add_blocker.scopes).contains(&"colony"));
    let triggers = native.declarations(DeclarationKind::Trigger).unwrap();
    assert_eq!(
        triggers
            .value
            .iter()
            .find(|item| item.name == "if")
            .unwrap()
            .scopes,
        DeclaredScopes::Any
    );
    // A command vtable loaded through the global offset table, and one stored through a copy of
    // the object register.
    assert_eq!(
        listed_names(&find(&effects.value, "set_citizenship_type", |item| &item.name).scopes),
        ["pop", "pop_group", "leader", "species"]
    );
    assert_eq!(
        find(&triggers.value, "exists", |item| &item.name).scopes,
        DeclaredScopes::Any
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn modifier_declarations_match_the_recorded_m45_boundary() {
    #[derive(serde::Deserialize)]
    struct Expected {
        count: usize,
        gaps: BTreeMap<String, usize>,
        samples: Vec<ModifierDeclaration>,
    }
    let expected: Expected = expected("modifier-declarations.json");
    let answer = native().modifiers().unwrap();
    assert_declared(&answer);

    assert_eq!(answer.value.len(), expected.count);
    assert_eq!(gap_counts(&answer), expected.gaps);
    assert_eq!(expected.samples.len(), 10);
    for sample in &expected.samples {
        assert_eq!(find(&answer.value, &sample.name, |item| &item.name), sample);
    }
    for modifier in &answer.value {
        assert_ne!(modifier.category_tags, DeclaredTags::Unresolved);
    }
}

/// Templates checked by hand against the M45-release disassembly of each database generator,
/// item post-read function and shared helper. The five of buildings, districts and bypass also
/// matched every registration of two SDK-498 live runs with renamed private content; the
/// SDK-566 live run matched the others against the loaded table
/// (`docs/native/modifier-families.md`).
#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn modifier_families_match_the_recorded_m45_generators() {
    let native = native();
    let expected: BTreeMap<String, Value> = expected("modifier-families.json");
    assert_eq!(expected.len(), 22);
    for (registry, expected) in &expected {
        let answer = native.modifier_families(registry).unwrap();
        assert_eq!(answer.source.basis, Basis::StaticAnalysis);
        assert_eq!(answer.completeness, Completeness::Partial);
        assert_eq!(&compact_families(&answer), expected, "{registry}");
    }

    let bypass = native.modifier_families("common/bypass").unwrap();
    let names: Vec<_> = bypass
        .value
        .iter()
        .map(|family| family.name_for("lgate"))
        .collect();
    assert_eq!(
        names,
        [
            Some("lgate_empire_windup_mult".into()),
            Some("lgate_megastructure_bypass_windup_mult".into()),
            Some("lgate_ship_windup_mult".into()),
        ]
    );
    let jobs = native.modifier_families("common/pop_jobs").unwrap();
    let names: Vec<_> = jobs
        .value
        .iter()
        .filter_map(|family| family.name_for("miner"))
        .collect();
    assert!(names.contains(&"job_miner_add".to_owned()));
    assert!(names.contains(&"pop_miner_workforce_mult".to_owned()));

    assert!(matches!(
        native.modifier_families("common/not_a_registry"),
        Err(Error::UnknownRegistry { .. })
    ));
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn modifier_categories_are_the_names_of_the_category_switch() {
    let answer = native().modifier_categories().unwrap();
    assert_declared(&answer);
    assert_eq!(answer.completeness, Completeness::Complete);
    let names: Vec<_> = answer.value.iter().map(|item| item.name.clone()).collect();
    assert_eq!(names, expected::<Vec<String>>("modifier-categories.json"));
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn scopes_group_keywords_by_the_engine_map_only() {
    let answer = native().scopes().unwrap();
    assert_eq!(answer.source.basis, Basis::Declared);
    assert_eq!(answer.completeness, Completeness::Complete);
    assert!(
        answer
            .gaps
            .iter()
            .all(|gap| gap.kind == GapKind::OutsideMethod)
    );
    assert_eq!(
        answer.value,
        expected::<ScopeInventory>("scope-inventory.json")
    );
    let federation = find(&answer.value.types, "federation", |item| &item.name);
    assert_eq!(federation.keywords, ["alliance", "federation"]);
    let groups: Vec<_> = answer
        .value
        .groups
        .iter()
        .map(|group| (group.keyword.as_str(), reference_names(&group.scopes)))
        .collect();
    assert_eq!(groups, [("carrier", vec!["planet", "ship"])]);
    let countries: Vec<_> = answer
        .value
        .types
        .iter()
        .filter(|scope| scope.name == "country")
        .collect();
    assert_eq!(countries.len(), 2);
    assert_ne!(countries[0].id, countries[1].id);
    assert!(
        answer
            .value
            .types
            .iter()
            .all(|scope| !scope.keywords.contains(&"carrier".into()))
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn scope_links_match_the_recorded_m45_boundary() {
    #[derive(serde::Deserialize)]
    struct Expected {
        names: Vec<String>,
        samples: Vec<ScopeLink>,
    }
    let expected: Expected = expected("scope-links.json");
    let answer = native().scope_links().unwrap();
    assert_declared(&answer);

    let names: Vec<_> = answer.value.iter().map(|item| item.name.clone()).collect();
    assert_eq!(names, expected.names);
    for sample in &expected.samples {
        assert_eq!(find(&answer.value, &sample.name, |item| &item.name), sample);
    }

    let documented = answer
        .value
        .iter()
        .filter(|link| link.data == LinkData::None)
        .count();
    assert_eq!(documented, 99);
    let capital = find(&answer.value, "capital_scope", |item| &item.name);
    assert_eq!(listed_names(&capital.input_scopes), ["country"]);
    let OutputScope::Listed(output) = &capital.output_scope else {
        panic!("capital_scope declares its output");
    };
    assert_eq!(reference_names(output), ["colony"]);
    let this = find(&answer.value, "this", |item| &item.name);
    assert_eq!(
        (&this.input_scopes, &this.output_scope),
        (&DeclaredScopes::Any, &OutputScope::Various)
    );
    for name in ["event_target", "parameter"] {
        let link = find(&answer.value, name, |item| &item.name);
        assert_eq!(link.data, LinkData::Prefix(format!("{name}:")));
        assert_eq!(
            (&link.input_scopes, &link.output_scope),
            (&DeclaredScopes::Any, &OutputScope::Various)
        );
    }
    assert!(
        answer
            .gaps
            .iter()
            .all(|gap| gap.kind == GapKind::OutsideMethod)
    );
    assert_eq!(answer.completeness, Completeness::Complete);
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn every_scope_reference_joins_to_one_declared_scope_type() {
    let native = native();
    let scopes = native.scopes().unwrap().value;
    let types: BTreeMap<&ScopeId, &String> = scopes
        .types
        .iter()
        .map(|scope| (&scope.id, &scope.name))
        .collect();

    let mut references = Vec::new();
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        for declaration in native.declarations(kind).unwrap().value {
            if let DeclaredScopes::Listed(scopes) = declaration.scopes {
                references.extend(scopes);
            }
        }
    }
    for link in native.scope_links().unwrap().value {
        if let DeclaredScopes::Listed(scopes) = link.input_scopes {
            references.extend(scopes);
        }
        if let OutputScope::Listed(scopes) = link.output_scope {
            references.extend(scopes);
        }
    }
    for group in &scopes.groups {
        references.extend(group.scopes.iter().cloned());
    }

    assert!(references.len() > 1000);
    for reference in &references {
        assert_eq!(types.get(&reference.id), Some(&&reference.name));
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn localization_declarations_match_the_recorded_m45_inventory() {
    let native = native();
    let answer = native.localization_declarations().unwrap();
    assert_eq!(answer.source.basis, Basis::Declared);
    assert_eq!(
        answer.completeness == Completeness::Complete,
        answer
            .gaps
            .iter()
            .all(|gap| gap.kind == GapKind::OutsideMethod),
        "completeness follows the gaps"
    );

    let localization = &answer.value;
    let contexts: BTreeMap<_, _> = localization
        .contexts
        .iter()
        .map(|context| (context.name.as_str(), context))
        .collect();
    assert_eq!(
        contexts.len(),
        localization.contexts.len(),
        "context names are unique on this build, so the expected output can name them"
    );
    assert_references_join(localization);

    let scopes = native.scopes().unwrap().value;
    for context in &localization.contexts {
        if let ContextScopes::Joined(references) = &context.scopes {
            for reference in references {
                let scope = scopes
                    .types
                    .iter()
                    .find(|scope| scope.id == reference.id)
                    .expect("a joined scope is a scope type of Native::scopes");
                assert_eq!(scope.name, reference.name);
            }
        }
    }

    assert_eq!(
        compact_localization(&answer),
        expected::<Value>("localization-declarations.json")
    );
}

/// Call sites checked by hand in the M45-release disassembly, before the expected files were
/// generated.
#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn on_actions_supply_the_scopes_that_hand_checked_call_sites_build() {
    let answer = native().on_actions().unwrap();
    assert_eq!(answer.source.basis, Basis::StaticAnalysis);
    let entries = |name: &str| -> Vec<String> {
        find(&answer.value, name, |on_action| &on_action.name)
            .entries
            .iter()
            .map(entry)
            .collect()
    };
    let fresh = "this=NotSet root=SelfLink from=[SelfLink]";

    // CGameState::OnNewGameStarted passes a new scope with nothing set.
    assert_eq!(entries("on_game_start"), [fresh]);
    // CGameState::MonthlyUpdate fires the cached list at database offset 0x28.
    assert_eq!(entries("on_monthly_pulse"), [fresh]);
    // CLeader::LevelUp links the leader as from of a country scope.
    assert!(
        entries("on_leader_level_up")
            .contains(&"this=country root=SelfLink from=[leader,SelfLink]".into())
    );
    // CPlanet::SetController links two country scopes as from and fromfrom.
    assert!(
        entries("on_planet_returned")
            .contains(&"this=planet root=SelfLink from=[country,country,SelfLink]".into())
    );
    // The fleet enters orbit of different objects; each stays its own context.
    let orbit = entries("on_fleet_enter_orbit");
    for from in ["megastructure", "planet", "starbase"] {
        let context = format!("this=fleet root=SelfLink from=[{from},SelfLink]");
        assert!(orbit.contains(&context), "{context}");
    }
    // CWar::OnEnd loads the name long before the call.
    assert!(!entries("on_war_ended").is_empty());
    // The country command builds its own scope, so the name has no entry and a gap.
    assert!(entries("on_press_begin").is_empty());

    for on_action in answer
        .value
        .iter()
        .filter(|on_action| on_action.entries.is_empty())
    {
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.subject.as_ref().map(|subject| subject.name())
                    == Some(on_action.name.as_str())),
            "{} has a gap",
            on_action.name
        );
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn game_rules_supply_the_scopes_that_hand_checked_call_sites_build() {
    let answer = native().game_rules().unwrap();
    let rule = |name: &str| find(&answer.value, name, |rule| &rule.name);

    // CGameRules::CanColonizePlanet sets the country as root and the planet as this.
    let colonize = rule("can_colonize_planet");
    assert_eq!(colonize.kind, RuleKind::Scripted);
    assert_eq!(
        colonize.entries.iter().map(entry).collect::<Vec<_>>(),
        ["this=planet root=country from=[SelfLink]"]
    );
    // CGameRules::CanOrbitalBombard links the planet as from of the fleet.
    assert!(
        rule("can_orbital_bombard")
            .entries
            .iter()
            .map(entry)
            .any(|context| context == "this=fleet root=SelfLink from=[planet,SelfLink]")
    );
    // A weighted rule lives in its own array of the rule set.
    let election = rule("leader_election_weight");
    assert_eq!(election.kind, RuleKind::Weighted);
    assert_eq!(
        election.entries.iter().map(entry).collect::<Vec<_>>(),
        ["this=leader root=SelfLink from=[SelfLink]"]
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn callbacks_match_the_recorded_m45_inventory() {
    let native = native();
    let on_actions = native.on_actions().unwrap();
    let game_rules = native.game_rules().unwrap();
    let scopes = native.scopes().unwrap().value;

    for answer in [
        &compact_on_actions(&on_actions),
        &compact_game_rules(&game_rules),
    ] {
        assert_eq!(
            answer["completeness"] == json!(Completeness::Complete),
            answer["gaps"]
                .as_array()
                .unwrap()
                .iter()
                .all(|gap| gap[0] == json!(GapKind::OutsideMethod)),
            "completeness follows the gaps"
        );
    }
    let entries = on_actions
        .value
        .iter()
        .flat_map(|on_action| &on_action.entries)
        .chain(game_rules.value.iter().flat_map(|rule| &rule.entries));
    for context in entries {
        for scope in std::iter::once(&context.this)
            .chain([&context.root])
            .chain(&context.from)
        {
            if let EntryScope::Scope(reference) = scope {
                let declared = scopes
                    .types
                    .iter()
                    .find(|scope| scope.id == reference.id)
                    .expect("an entry scope joins a scope type of Native::scopes");
                assert_eq!(declared.name, reference.name);
            }
        }
    }

    assert_eq!(
        compact_on_actions(&on_actions),
        expected::<Value>("on-actions.json")
    );
    assert_eq!(
        compact_game_rules(&game_rules),
        expected::<Value>("game-rules.json")
    );
}

/// An entry context as one line: scope names, or the kind of an entry that is not a scope.
fn entry(context: &EntryContext) -> String {
    let scope = |scope: &EntryScope| match scope {
        EntryScope::Scope(reference) => reference.name.clone(),
        other => format!("{other:?}"),
    };
    let from: Vec<_> = context.from.iter().map(scope).collect();
    format!(
        "this={} root={} from=[{}]",
        scope(&context.this),
        scope(&context.root),
        from.join(",")
    )
}

/// The on_actions answer with the entry list of each name.
fn compact_on_actions(answer: &Answer<Vec<OnAction>>) -> Value {
    compact_callbacks(answer, |on_action| {
        let entries: Vec<_> = on_action.entries.iter().map(entry).collect();
        (on_action.name.clone(), json!(entries))
    })
}

/// The game rules answer with the kind and entry list of each name.
fn compact_game_rules(answer: &Answer<Vec<GameRule>>) -> Value {
    compact_callbacks(answer, |rule| {
        let entries: Vec<_> = rule.entries.iter().map(entry).collect();
        (rule.name.clone(), json!([rule.kind, entries]))
    })
}

/// A callback answer as its completeness, its gaps and one value for each callback name.
fn compact_callbacks<T>(
    answer: &Answer<Vec<T>>,
    name_and_value: impl Fn(&T) -> (String, Value),
) -> Value {
    let names: serde_json::Map<_, _> = answer.value.iter().map(name_and_value).collect();
    let gaps: Vec<_> = answer
        .gaps
        .iter()
        .map(|gap| json!([gap.kind, gap.subject, gap.detail]))
        .collect();

    json!({
        "completeness": answer.completeness,
        "names": names,
        "gaps": gaps,
    })
}

/// Every context reference names a context of the same answer by id and name.
fn assert_references_join(localization: &LocalizationDeclarations) {
    let assert_joins = |reference: &LocalizationContextReference| {
        let context = localization
            .contexts
            .iter()
            .find(|context| context.id == reference.id)
            .expect("a reference joins a context by id");
        assert_eq!(context.name, reference.name);
    };

    for command in &localization.commands {
        command.contexts.iter().for_each(assert_joins);
    }
    for link in &localization.links {
        link.input_contexts.iter().for_each(assert_joins);
        if let LocalizationOutput::Listed(outputs) = &link.output {
            outputs.iter().for_each(assert_joins);
        }
    }
}

/// The whole answer with context references written as names, one entry for each row.
fn compact_localization(answer: &Answer<LocalizationDeclarations>) -> Value {
    let localization = &answer.value;
    let contexts: serde_json::Map<_, _> = localization
        .contexts
        .iter()
        .map(|context| {
            let scopes = match &context.scopes {
                ContextScopes::Joined(scopes) => json!({ "Joined": reference_names(scopes) }),
                ContextScopes::Partial(scopes) => json!({ "Partial": reference_names(scopes) }),
                ContextScopes::Missing => json!("Missing"),
            };
            (
                context.name.clone(),
                json!({ "id": context.id, "scopes": scopes }),
            )
        })
        .collect();
    let commands: serde_json::Map<_, _> = localization
        .commands
        .iter()
        .map(|command| {
            (
                command.name.clone(),
                json!(context_names(&command.contexts)),
            )
        })
        .collect();
    let links: Vec<_> = localization
        .links
        .iter()
        .map(|link| {
            let output = match &link.output {
                LocalizationOutput::Listed(outputs) => json!({ "Listed": context_names(outputs) }),
                other => json!(other),
            };
            json!([link.name, context_names(&link.input_contexts), output])
        })
        .collect();
    let gaps: Vec<_> = answer
        .gaps
        .iter()
        .map(|gap| json!([gap.kind, gap.subject, gap.detail]))
        .collect();

    json!({
        "completeness": answer.completeness,
        "contexts": contexts,
        "commands": commands,
        "links": links,
        "gaps": gaps,
    })
}

fn context_names(references: &[LocalizationContextReference]) -> Vec<&str> {
    references
        .iter()
        .map(|reference| reference.name.as_str())
        .collect()
}

fn listed_names(scopes: &DeclaredScopes) -> Vec<&str> {
    match scopes {
        DeclaredScopes::Listed(scopes) => reference_names(scopes),
        other => panic!("expected listed scopes, not {other:?}"),
    }
}

fn reference_names(scopes: &[ScopeReference]) -> Vec<&str> {
    scopes.iter().map(|scope| scope.name.as_str()).collect()
}

fn assert_declared<T>(answer: &Answer<Vec<T>>) {
    assert_eq!(answer.source.basis, Basis::Declared);
    let complete = answer
        .gaps
        .iter()
        .all(|gap| gap.kind == GapKind::OutsideMethod);
    assert_eq!(
        answer.completeness == Completeness::Complete,
        complete,
        "completeness follows the gaps"
    );
}

/// Each family as its template, and every gap but the method boundary.
fn compact_families(answer: &Answer<Vec<ModifierFamily>>) -> Value {
    let families: Vec<_> = answer
        .value
        .iter()
        .map(|family| {
            let template: String = family
                .name
                .iter()
                .map(|part| match part {
                    NamePart::Literal(text) => text.as_str(),
                    NamePart::ItemKey => "{key}",
                    other => panic!("unexpected part {other:?}"),
                })
                .collect();
            json!({
                "template": template,
                "category_tags": family.category_tags,
                "condition": family.condition,
                "name_limit": family.name_limit,
            })
        })
        .collect();
    let gaps: Vec<_> = answer
        .gaps
        .iter()
        .filter(|gap| gap.kind != GapKind::OutsideMethod)
        .map(|gap| json!([gap.kind, gap.subject, gap.detail]))
        .collect();
    json!({ "families": families, "gaps": gaps })
}

fn gap_counts<T>(answer: &Answer<Vec<T>>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for gap in &answer.gaps {
        *counts.entry(format!("{:?}", gap.kind)).or_default() += 1;
    }
    counts
}

fn find<'a, T>(items: &'a [T], name: &str, key: impl Fn(&T) -> &String) -> &'a T {
    items
        .iter()
        .find(|item| key(item) == name)
        .unwrap_or_else(|| panic!("{name} is in the answer"))
}

fn native() -> Native {
    Native::open(
        std::env::var_os("STELLARIS_PATH")
            .expect("STELLARIS_PATH names the installation or executable"),
    )
    .expect("the installed build is in the target catalogue")
}

fn expected<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = format!("{}/tests/expected/m45/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn registries_are_named_by_their_content_directory() {
    let answer = native().registries().unwrap();
    assert_eq!(
        answer.value.len(),
        164,
        "M45-release registry discovery changed"
    );
    let names: Vec<_> = answer.value.iter().map(|r| r.name.clone()).collect();
    assert_eq!(names, expected::<Vec<String>>("registries.json"));
    assert_eq!(answer.completeness, Completeness::Complete);
    assert_eq!(answer.source.basis, Basis::StaticAnalysis);
    // `common/ship_categories` passes a global CString; its static initializer names it.
    assert!(names.contains(&"common/ship_categories".to_owned()));
    assert!(
        answer
            .gaps
            .iter()
            .all(|gap| gap.kind != GapKind::UnnamedRegistries)
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn registry_fields_match_and_share_reader_identities_across_registries() {
    let native = native();
    let mut potential = Vec::new();
    for (registry, file) in [
        ("common/traditions", "fields-traditions.json"),
        (
            "common/tradition_categories",
            "fields-tradition_categories.json",
        ),
        ("common/council_agendas", "fields-council_agendas.json"),
        ("common/megastructures", "fields-megastructures.json"),
    ] {
        let answer = native.registry_fields(registry).unwrap();
        assert_eq!(answer.value, expected::<Vec<Field>>(file), "{registry}");
        assert_eq!(
            answer.completeness,
            if answer
                .gaps
                .iter()
                .all(|gap| gap.kind == GapKind::OutsideMethod)
            {
                Completeness::Complete
            } else {
                Completeness::Partial
            }
        );
        for field in &answer.value {
            let expected_gap = if field.reader.id.is_none() {
                Some(GapKind::UnresolvedReader)
            } else if field.reader.kind == ReaderKind::Unknown {
                Some(GapKind::ReaderSemantics)
            } else {
                None
            };
            if let Some(kind) = expected_gap {
                assert!(
                    answer.gaps.iter().any(|gap| gap.kind == kind
                        && gap.subject.as_ref().map(|subject| subject.name()) == Some(&field.name)),
                    "{registry}: {}",
                    field.name
                );
            }
        }
        potential.extend(
            answer
                .value
                .into_iter()
                .filter(|field| field.name == "potential")
                .map(|field| field.reader.id.expect("potential has one reader")),
        );
    }
    assert!(potential.len() >= 2 && potential.windows(2).all(|pair| pair[0] == pair[1]));
    assert!(matches!(
        native.registry_fields("common/no_such_registry"),
        Err(Error::UnknownRegistry { .. })
    ));
}

/// The developer entry that the sweep uses gives the public answer from its one run.
#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn megastructure_fields_behind_the_two_jump_tables_are_found() {
    let answer = native().registry_fields("common/megastructures").unwrap();
    let found: std::collections::BTreeSet<_> = answer
        .value
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    // CMegaStructureType::ReadMember dispatches these through two halfword jump tables.
    let first_table = [
        "entity_offset",
        "plane_offset",
        "construction_scale",
        "on_build_queued",
        "on_build_unqueued",
        "on_build_start",
        "on_build_cancel",
        "on_build_complete",
        "on_dismantle_start",
        "on_dismantle_cancel",
        "on_dismantle_complete",
        "build_megastructure_no_cost_localization_key",
        "build_system_tooltip",
        "tooltip_system_score",
        "tooltip_system_score_low_threshold",
        "tooltip_system_score_high_threshold",
        "tooltip_best_systems_header",
        "tooltip_system_filter",
        "tooltip_show_star_resources",
        "upgrade_from",
        "construction_entity",
        "placement_rules",
        "place_entity_on_planet_plane",
        "use_planet_resource",
    ];
    let second_table = [
        "overclock_types",
        "on_cycle_complete",
        "cycle_length_in_days",
        "cycle_title",
        "cycle_desc",
        "cycle_icon",
        "order_icon",
        "can_prevent_crisis_terraformation",
    ];
    for name in first_table.into_iter().chain(second_table) {
        assert!(found.contains(name), "{name}");
    }
    assert!(
        !answer
            .gaps
            .iter()
            .any(|gap| gap.detail.contains("jump table"))
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn the_developer_run_gives_the_public_registry_field_answer() {
    let native = native();
    for registry in ["common/traditions", "common/megastructures"] {
        let run = registry_field_stops::run(&native, registry).unwrap();
        assert_eq!(run.answer, native.registry_fields(registry).unwrap());
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn traced_questions_match_untraced_questions() {
    // Each `Native` caches its analysis, so the traced questions need their own.
    let untraced = native();
    let traced = native();
    trace_causes(|| {
        for kind in [DeclarationKind::Trigger, DeclarationKind::Effect] {
            assert_eq!(traced.declarations(kind), untraced.declarations(kind));
        }
        assert_eq!(traced.modifiers(), untraced.modifiers());
        assert_eq!(traced.modifier_categories(), untraced.modifier_categories());
        assert_eq!(traced.scopes(), untraced.scopes());
        assert_eq!(traced.scope_links(), untraced.scope_links());
        assert_eq!(
            traced.localization_declarations(),
            untraced.localization_declarations()
        );
        assert_eq!(traced.on_actions(), untraced.on_actions());
        assert_eq!(traced.game_rules(), untraced.game_rules());
        assert_eq!(traced.defines(), untraced.defines());
        let families: BTreeMap<String, Value> = expected("modifier-families.json");
        for registry in families.keys() {
            assert_eq!(
                traced.modifier_families(registry),
                untraced.modifier_families(registry),
                "{registry}"
            );
        }

        for registry in [
            "common/council_agendas",
            "common/megastructures",
            "common/tradition_categories",
            "common/traditions",
        ] {
            let traced = registry_field_stops::run(&traced, registry).unwrap();
            let untraced = registry_field_stops::run(&untraced, registry).unwrap();
            assert_eq!(traced.answer, untraced.answer, "{registry}");
            assert_eq!(traced.result.paths, untraced.result.paths, "{registry}");
            assert_eq!(traced.result.gaps, untraced.result.gaps, "{registry}");
        }

        let controls = ["and", "or", "not", "if", "else_if", "else"].map(|name| (false, name));
        let effects = [
            "if",
            "else_if",
            "else",
            "hidden_effect",
            "random_list",
            "every_owned_planet",
            "pop_change_ethic",
            "pop_force_add_ethic",
            "remove_random_starbase_building",
            "remove_random_starbase_module",
        ]
        .map(|name| (true, name));
        let triggers = [
            "has_relation_flag",
            "is_war_participant",
            "pop_ethic_amount",
            "reverse_has_relation_flag",
            "has_country_flag",
            "exists",
        ]
        .map(|name| (false, name));
        for (effect, name) in controls.into_iter().chain(effects).chain(triggers) {
            let kind = if effect {
                DeclarationKind::Effect
            } else {
                DeclarationKind::Trigger
            };
            let traced = command_grammar_stops::run(&traced, kind, name).unwrap();
            let untraced = command_grammar_stops::run(&untraced, kind, name).unwrap();
            assert_eq!(traced.answer, untraced.answer, "{kind:?}/{name}");
            match (&traced.result, &untraced.result) {
                (Err(traced), Err(untraced)) => assert_eq!(traced, untraced, "{kind:?}/{name}"),
                (Ok(traced), Ok(untraced)) => {
                    assert_eq!(traced.stops, untraced.stops, "{kind:?}/{name}");
                    assert_eq!(
                        traced.fields.paths, untraced.fields.paths,
                        "{kind:?}/{name}"
                    );
                }
                _ => panic!("{kind:?}/{name}: tracing changed whether the receiver joined"),
            }
        }

        for (kind, name) in [
            (DeclarationKind::Effect, "pop_change_ethic"),
            (DeclarationKind::Trigger, "exists"),
            (DeclarationKind::Trigger, "has_country_flag"),
        ] {
            let run = command_grammar_stops::run(&traced, kind, name).unwrap();
            let unresolved = run.result.err().expect("the receiver join stops");
            let trace = unresolved.trace.expect("a traced receiver failure");
            assert!(trace.causes().next().is_some(), "{kind:?}/{name}");
        }
    });
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn recorded_answers_equal_the_real_answers_apart_from_the_basis() {
    let directory = tempfile::tempdir().unwrap();
    let real = native().record_answers_to(directory.path());
    let registries = real.registries().unwrap();
    let fields = real.registry_fields("common/traditions").unwrap();
    let effects = real.declarations(DeclarationKind::Effect).unwrap();
    let links = real.scope_links().unwrap();
    let localization = real.localization_declarations().unwrap();
    let on_actions = real.on_actions().unwrap();
    let game_rules = real.game_rules().unwrap();
    let defines = real.defines().unwrap();
    let grammar = real
        .command_grammar(DeclarationKind::Effect, "random_list")
        .unwrap();
    let unknown_command = real.command_grammar(DeclarationKind::Effect, "native_missing_command");
    let unknown_registry_error = real.registry_fields("common/no_such_registry");

    let recorded = Native::from_recorded_answers(directory.path()).unwrap();
    assert_eq!(recorded.build(), real.build());
    let mut again = recorded.registries().unwrap();
    assert_eq!(again.source.basis, Basis::Recorded);
    again.source.basis = registries.source.basis;
    assert_eq!(again, registries);
    let mut again = recorded.registry_fields("common/traditions").unwrap();
    again.source.basis = fields.source.basis;
    assert_eq!(again, fields);
    let mut again = recorded.declarations(DeclarationKind::Effect).unwrap();
    again.source.basis = effects.source.basis;
    assert_eq!(again, effects);
    let mut again = recorded.scope_links().unwrap();
    again.source.basis = links.source.basis;
    assert_eq!(again, links);
    let mut again = recorded.localization_declarations().unwrap();
    again.source.basis = localization.source.basis;
    assert_eq!(again, localization);
    let mut again = recorded.on_actions().unwrap();
    again.source.basis = on_actions.source.basis;
    assert_eq!(again, on_actions);
    let mut again = recorded.game_rules().unwrap();
    again.source.basis = game_rules.source.basis;
    assert_eq!(again, game_rules);
    let mut again = recorded.defines().unwrap();
    again.source.basis = defines.source.basis;
    assert_eq!(again, defines);
    let mut again = recorded
        .command_grammar(DeclarationKind::Effect, "random_list")
        .unwrap();
    assert_eq!(again.source.basis, Basis::Recorded);
    again.source.basis = grammar.source.basis;
    assert_eq!(again, grammar);
    assert_eq!(
        recorded.command_grammar(DeclarationKind::Effect, "native_missing_command"),
        unknown_command
    );
    assert_eq!(
        recorded.registry_fields("common/no_such_registry"),
        unknown_registry_error
    );
    assert!(matches!(
        recorded.registry_fields("common/armies"),
        Err(Error::NotRecorded { .. })
    ));
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn control_grammar_preserves_shared_readers_and_partial_properties() {
    use pdx_native::{BlockFamily, GrammarProperty, ReaderKind};
    let native = native();
    assert_eq!(
        native.supports(Operation::CommandGrammar),
        pdx_native::Support::Supported
    );
    for (kind, family, names) in [
        (
            DeclarationKind::Trigger,
            BlockFamily::Trigger,
            ["and", "or", "not", "if", "else_if", "else"],
        ),
        (
            DeclarationKind::Effect,
            BlockFamily::Effect,
            [
                "if",
                "else_if",
                "else",
                "hidden_effect",
                "random_list",
                "every_owned_planet",
            ],
        ),
    ] {
        let mut identities = BTreeMap::new();
        for name in names {
            let answer = native.command_grammar(kind, name).unwrap();
            assert_eq!(answer.source.method, "command-grammar/v1");
            assert_eq!(answer.completeness, Completeness::Partial);
            assert!(answer.value.reader.id.is_some(), "{kind:?}/{name}");
            assert_eq!(
                answer.value.reader.kind,
                ReaderKind::Block,
                "{kind:?}/{name}"
            );
            assert_eq!(answer.value.reader.family, family, "{kind:?}/{name}");
            let child = if name == "random_list" {
                let GrammarProperty::Partial(Some(child)) = &answer.value.numeric_keys else {
                    panic!("weighted child grammar missing");
                };
                assert_ne!(answer.value.reader.id, child.reader.id);
                child.as_ref()
            } else {
                &answer.value
            };
            assert_eq!(child.child_families, GrammarProperty::Partial(vec![family]));
            if kind == DeclarationKind::Effect && ["if", "else_if", "else"].contains(&name) {
                let GrammarProperty::Partial(rules) = &answer.value.ordering else {
                    panic!("conditional reader routing missing");
                };
                assert_eq!(rules.len(), 3);
            }
            identities.insert(name, answer.value.reader.id);
        }
        assert_eq!(identities["if"], identities["else_if"]);
        assert_eq!(identities["if"], identities["else"]);
        if kind == DeclarationKind::Trigger {
            assert_eq!(identities["or"], identities["not"]);
        }
        assert!(matches!(
            native.command_grammar(kind, "limit"),
            Err(Error::UnknownCommand { .. })
        ));
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn field_shapes_agree_with_sdk533_omitted_and_repeated_storage() {
    use pdx_native::{FieldDefault, FieldMembers, RepeatBehavior, ValueShape};
    let native = native();
    let observed: Value = expected("field-storage-sdk533.json");
    assert_eq!(
        serde_json::to_value(native.build()).unwrap(),
        observed["source"]["build"]
    );
    let fields = native.registry_fields("common/traditions").unwrap().value;
    let mut omitted = 0;
    let mut repeated = 0;
    for outcome in observed["outcomes"].as_array().unwrap() {
        let name = outcome["question"]["field"].as_str().unwrap();
        let field = fields.iter().find(|field| field.name == name).unwrap();
        let historical_reader: pdx_native::Reader =
            serde_json::from_value(outcome["reader"].clone()).unwrap();
        assert_eq!(field.reader.id, historical_reader.id);
        assert_eq!(field.reader.kind, historical_reader.kind);
        assert_eq!(field.reader.family, pdx_native::BlockFamily::NotApplicable);
        let storage = &outcome["storage"]["String"];
        assert_eq!(storage["completeness"], "Complete");
        let occurrences = storage["occurrences"].as_array().unwrap();
        assert_eq!(field.shape.value, ValueShape::Scalar);
        if occurrences.is_empty() {
            omitted += 1;
            assert_eq!(field.default, FieldDefault::Unknown);
            assert_eq!(storage["final_value"], "");
        } else {
            repeated += 1;
            assert!(occurrences.len() > 1);
            assert_eq!(field.shape.repeat, RepeatBehavior::Replace, "{name}");
            assert_eq!(
                storage["final_value"],
                occurrences.last().unwrap()["value"],
                "{name}"
            );
        }
    }
    assert_eq!((omitted, repeated), (1, 1));
    let swaps = fields
        .iter()
        .find(|field| field.name == "tradition_swap")
        .unwrap();
    assert_eq!(swaps.shape.repeat, RepeatBehavior::Accumulate);
    let FieldMembers::Fields(children) = &swaps.members else {
        panic!("swap members unresolved")
    };
    for flag in ["inherit_effects", "inherit_name", "inherit_icon"] {
        assert!(children.iter().any(|field| field.name == flag));
        assert!(children.iter().flat_map(|field| &field.uses).any(|selection| {
            match &selection.condition {
                pdx_native::FieldCondition::All(terms) => terms.iter().any(|term| matches!(term,
                    pdx_native::FieldCondition::FieldZero { path, zero: true } if path == &["tradition_swap", flag])),
                _ => false,
            }
        }), "no affected field for {flag}");
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn council_presence_initialization_does_not_restrict_field_reads() {
    use pdx_native::{FieldCondition, RepeatBehavior, ValueShape};
    let fields = native()
        .registry_fields("common/council_agendas")
        .unwrap()
        .value;
    for name in ["agenda_cooldown", "agenda_finish_modifier_duration"] {
        let field = fields.iter().find(|field| field.name == name).unwrap();
        assert_eq!(field.read.len(), 1, "{name}");
        assert_eq!(field.read[0].condition, FieldCondition::Always, "{name}");
        assert_eq!(field.shape.value, ValueShape::Scalar);
        assert_eq!(field.shape.repeat, RepeatBehavior::Replace);
    }
}
