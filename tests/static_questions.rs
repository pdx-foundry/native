//! Parity of the static questions with tracked expected output for the M45 build.
//! Needs the real executable: set `STELLARIS_PATH` and run with `--ignored`. No game starts.
use pdx_native::{
    Answer, Basis, Completeness, ContextScopes, Declaration, DeclarationKind, DeclaredScopes,
    DeclaredTags, EntryContext, EntryScope, Error, Field, GapKind, LinkData,
    LocalizationContextReference, LocalizationDeclarations, LocalizationOutput,
    ModifierDeclaration, Native, OutputScope, ReaderKind, RuleKind, ScopeId, ScopeInventory,
    ScopeLink, ScopeReference,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn direct_declarations_match_the_recorded_m45_boundary() {
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
                    && !names.contains(gap.subject.as_deref().unwrap_or(""))
            })
            .filter_map(|gap| gap.subject.clone())
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
            gap_counts[subject]["runtime_token_sites"].as_u64().unwrap() as usize
        );
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
            assert_eq!(item.targets, DeclaredScopes::Unresolved);
            assert!(!item.description.contains("Supported Scopes:"));
            assert!(!item.usage.contains("Supported Scopes:"));
            if item.scopes == DeclaredScopes::Unresolved && !global_scope_gap {
                assert!(
                    answer
                        .gaps
                        .iter()
                        .any(|gap| gap.kind == GapKind::UnresolvedPath
                            && gap.subject.as_deref() == Some(&item.name))
                );
            }
        }
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::UnresolvedPath
                    && gap.subject.is_none()
                    && gap.detail == "target declarations are not followed by this method")
        );
        let samples: Vec<Declaration> = expected(&format!("declaration-samples-{subject}.json"));
        assert_eq!(samples.len(), 10);
        for sample in samples {
            assert_eq!(
                answer.value.iter().find(|item| item.name == sample.name),
                Some(&sample)
            );
        }
    }
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
    let unresolved: Vec<_> = answer
        .gaps
        .iter()
        .filter(|gap| gap.kind == GapKind::UnresolvedPath)
        .filter_map(|gap| gap.subject.as_deref())
        .collect();
    assert_eq!(unresolved, ["event_target", "parameter"]);
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
                .any(|gap| gap.subject.as_deref() == Some(on_action.name.as_str())),
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
        &compact_callbacks(&on_actions),
        &compact_callbacks(&game_rules),
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
        compact_callbacks(&on_actions),
        expected::<Value>("on-actions.json")
    );
    assert_eq!(
        compact_callbacks(&game_rules),
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

/// A callback answer with one entry list for each name. Game rules carry their kind.
fn compact_callbacks<T: serde::Serialize>(answer: &Answer<Vec<T>>) -> Value {
    let names: serde_json::Map<_, _> = serde_json::to_value(&answer.value)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            let entries: Vec<_> =
                serde_json::from_value::<Vec<EntryContext>>(item["entries"].clone())
                    .unwrap()
                    .iter()
                    .map(entry)
                    .collect();
            let value = match item.get("kind") {
                Some(kind) => json!([kind, entries]),
                None => json!(entries),
            };
            (item["name"].as_str().unwrap().to_owned(), value)
        })
        .collect();
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
                    answer
                        .gaps
                        .iter()
                        .any(|gap| gap.kind == kind && gap.subject.as_deref() == Some(&field.name)),
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
    let unknown = real.registry_fields("common/no_such_registry");

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
    // Errors are recorded too.
    assert_eq!(recorded.registry_fields("common/no_such_registry"), unknown);
    assert!(matches!(
        recorded.registry_fields("common/armies"),
        Err(Error::NotRecorded { .. })
    ));
}
