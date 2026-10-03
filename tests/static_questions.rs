//! Parity of the static questions with tracked expected output for the M45 build.
//! Needs the real executable: set `STELLARIS_PATH` and run with `--ignored`. No game starts.
mod parity;
use parity::*;

use pdx_native::internals::registry_field_stops::{FieldGap, TokenPath, Trace, Unresolved};
use pdx_native::internals::{
    COMMAND_GRAMMAR_METHOD, DEFINES_METHOD, DYNAMIC_NAMES_METHOD, command_grammar_stops,
    registry_field_stops, trace_causes,
};
use pdx_native::{
    Answer, Basis, Completeness, ContextScopes, DeclarationKind, DeclaredScopes, DeclaredTags,
    EntryScope, Error, FieldReference, GapKind, KeyMatch, LinkData, LocalizationContextReference,
    LocalizationDeclarations, LocalizationOutput, LookupStage, Native, Operation, OutputScope,
    ReaderKind, ReferenceTarget, RuleKind, ScopeId,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn modifier_blocks_match_the_recorded_variants_and_shared_identities() {
    use pdx_native::{GenerationCondition, GrammarProperty, ModifierEntry, NamePart};
    let native = native();
    assert_eq!(
        modifier_blocks(&native).unwrap(),
        expected::<Value>("modifier-blocks.json")
    );
    let mut variants = BTreeMap::new();
    for registry in native.registries().unwrap().value {
        for field in native.registry_fields(&registry.name).unwrap().value {
            check_modifier_identity(&mut variants, &field).unwrap();
        }
    }
    assert_eq!(variants.len(), 4);
    for block in variants.values() {
        let GrammarProperty::Known(entries) = &block.entries else {
            panic!("unresolved entries")
        };
        assert!(entries.iter().any(|entry| matches!(entry,
            ModifierEntry::Reference {target: ReferenceTarget::Registry {name}, value: ReaderKind::FixedPoint}
            if name == "common/static_modifiers")));
        assert!(entries.iter().any(|entry| matches!(entry, ModifierEntry::Numeric {value} if value.kind == ReaderKind::FixedPoint)));
    }
    let families = native
        .modifier_families("common/scripted_modifiers")
        .unwrap();
    assert_eq!(families.value.len(), 1);
    assert_eq!(families.value[0].name, vec![NamePart::ItemKey]);
    assert_eq!(families.value[0].condition, GenerationCondition::Always);
    assert_eq!(families.value[0].name_limit, None);
}

/// Report an inapplicable live observation outside harness capture, so a passing run shows the skip.
fn report_historical_storage_skip(
    build: &pdx_native::BuildId,
    observed: &Value,
) -> parity::Result<()> {
    let report = comparison::historical_storage_report(build, observed);
    use std::io::Write;
    writeln!(std::io::stderr(), "{}", report.render_and_save()?)?;
    Ok(())
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn defines_match_the_recorded_m45_boundary() {
    let native = native();
    assert_eq!(
        native.supports(Operation::Defines),
        pdx_native::Support::Supported
    );
    let answer = native.defines().unwrap();
    assert_eq!(answer.source.basis, Basis::StaticAnalysis);
    assert_eq!(answer.source.method, DEFINES_METHOD);
    assert_eq!(answer.completeness, Completeness::Partial);
    assert!(answer.gaps.iter().any(|gap| {
        gap.subject.as_ref().map(|subject| subject.name()) == Some("NGraphics.ORBIT_HSV")
            && gap.detail == "reader path exceeds the table-search limit"
    }));
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn declarations_match_the_recorded_m45_inventory() {
    let native = native();
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        let answer = native.declarations(kind).unwrap();
        assert_eq!(answer.source.basis, Basis::Declared);
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
        assert!(answer.gaps.iter().all(|gap| gap.subject.is_some()
            || gap.kind == GapKind::OutsideMethod
            || gap.kind == GapKind::UnreadableInput));
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
    let answer = native().modifiers().unwrap();
    assert_declared(&answer);

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
    for registry in expected.keys() {
        let answer = native.modifier_families(registry).unwrap();
        assert_eq!(answer.source.basis, Basis::StaticAnalysis);
        assert_eq!(answer.completeness, Completeness::Partial);
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
    let answer = native().scope_links().unwrap();
    assert_declared(&answer);

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

fn listed_names(scopes: &DeclaredScopes) -> Vec<&str> {
    match scopes {
        DeclaredScopes::Listed(scopes) => reference_names(scopes),
        other => panic!("expected listed scopes, not {other:?}"),
    }
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
fn reference_lookups_name_their_registry_and_keep_unresolved_facts() {
    let native = native();
    let expected = expected::<BTreeMap<String, Value>>("references.json");
    for (subject, value) in &expected {
        let (owner, name) = subject.split_once('#').unwrap_or((subject, ""));
        let (text, gaps, fields) = reference_answer(&native, owner).unwrap();
        assert!(!text.contains("Database"), "{owner} names a database class");

        if name.is_empty() {
            let details: Vec<String> = serde_json::from_value(value.clone()).unwrap();
            for detail in &details {
                assert!(
                    gaps.iter().any(|gap| &gap.detail == detail),
                    "{subject}: {detail} in {gaps:?}"
                );
            }
            let initializer_gaps = gaps
                .iter()
                .filter(|gap| gap.detail.contains("initializer"))
                .filter(|gap| !details.contains(&gap.detail));
            assert_eq!(initializer_gaps.count(), 0, "{subject}: {gaps:?}");
            continue;
        }

        let reference: FieldReference = serde_json::from_value(value.clone()).unwrap();
        let field = find(&fields, name, |field| &field.name);
        assert_eq!(field.reference, reference, "{subject}");
        let unresolved = match &reference {
            FieldReference::Lookups(lookups) => lookups.iter().any(|lookup| {
                lookup.target == ReferenceTarget::Unresolved
                    || lookup.stage == LookupStage::Unresolved
                    || lookup.key_match == KeyMatch::Unresolved
            }),
            _ if command(owner).is_some() => continue,
            _ => true,
        };
        let explained = gaps.iter().any(|gap| {
            gap.kind == GapKind::ReaderSemantics
                && gap
                    .subject
                    .as_ref()
                    .is_some_and(|subject| subject.name() == name)
        });
        assert_eq!(
            unresolved, explained,
            "{subject}: an unresolved fact has a gap"
        );
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn registry_fields_match_and_share_reader_identities_across_registries() {
    let native = native();
    let mut potential = Vec::new();
    for (registry, _) in FIELD_FILES {
        let answer = native.registry_fields(registry).unwrap();
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
    let untraced = observe(&native());
    // Each `Native` caches its analysis, so the traced questions need their own.
    let traced_native = native();
    let traced = trace_causes(|| observe(&traced_native));

    assert_eq!(traced.answers, untraced.answers);
    assert_eq!(traced.fields, untraced.fields);
    assert_eq!(traced.grammars, untraced.grammars);
    assert!(untraced.receiver_failures.iter().all(Option::is_none));
    for trace in &traced.receiver_failures {
        let trace = trace.as_ref().expect("a traced receiver failure");
        assert!(trace.causes().next().is_some());
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn traced_initial_state_keeps_destinations_across_later_member_constructors() {
    let untraced_native = native();
    // Each `Native` caches its analysis, so the traced questions need their own.
    let traced_native = native();

    // Both lost their storage to later event-target and registration calls before SDK-660.
    for (command, key) in [
        ("country_event", "days"),
        ("effect_on_blob", "owned_planets_percentage"),
    ] {
        let run = |native| command_grammar_stops::run(native, DeclarationKind::Effect, command);
        let untraced = run(&untraced_native).unwrap();
        let traced = trace_causes(|| run(&traced_native)).unwrap();

        assert_eq!(traced.answer, untraced.answer, "{command}");
        assert_eq!(traced.state_stops, untraced.state_stops, "{command}");
        assert!(!untraced.state_stops.contains_key(key), "{command}.{key}");
    }
}

/// What tracing must not change on one `Native`: every static answer, the internal results of
/// four registries and 22 commands, and the traces of three known receiver failures.
struct Observed {
    answers: Vec<Value>,
    fields: Vec<(Value, Vec<TokenPath>, Vec<FieldGap>)>,
    grammars: Vec<(Value, GrammarOutcome)>,
    receiver_failures: Vec<Option<Box<Trace>>>,
}

type GrammarOutcome = std::result::Result<(Vec<Unresolved>, Vec<TokenPath>), Unresolved>;

fn observe(native: &Native) -> Observed {
    let mut answers = Vec::new();
    for kind in [DeclarationKind::Trigger, DeclarationKind::Effect] {
        answers.push(json!(native.declarations(kind)));
    }
    answers.push(json!(native.modifiers()));
    answers.push(json!(native.modifier_categories()));
    answers.push(json!(native.scopes()));
    answers.push(json!(native.scope_links()));
    answers.push(json!(native.localization_declarations()));
    answers.push(json!(native.on_actions()));
    answers.push(json!(native.game_rules()));
    answers.push(json!(native.defines()));
    let families: BTreeMap<String, Value> = expected("modifier-families.json");
    for registry in families.keys() {
        answers.push(json!(native.modifier_families(registry)));
    }

    let fields = [
        "common/council_agendas",
        "common/megastructures",
        "common/tradition_categories",
        "common/traditions",
    ]
    .map(|registry| {
        let run = registry_field_stops::run(native, registry).unwrap();
        (json!(run.answer), run.result.paths, run.result.gaps)
    })
    .into();

    let triggers = [
        "and",
        "or",
        "not",
        "if",
        "else_if",
        "else",
        "has_relation_flag",
        "is_war_participant",
        "pop_ethic_amount",
        "reverse_has_relation_flag",
        "has_country_flag",
        "exists",
    ]
    .map(|name| (DeclarationKind::Trigger, name));
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
    .map(|name| (DeclarationKind::Effect, name));
    let grammars = triggers
        .into_iter()
        .chain(effects)
        .map(|(kind, name)| {
            let run = command_grammar_stops::run(native, kind, name).unwrap();
            let outcome = run.result.map(|result| (result.stops, result.fields.paths));
            (json!(run.answer), outcome)
        })
        .collect();

    let receiver_failures = [
        (DeclarationKind::Effect, "pop_change_ethic"),
        (DeclarationKind::Trigger, "switch"),
        (DeclarationKind::Trigger, "inverted_switch"),
    ]
    .map(|(kind, name)| {
        let run = command_grammar_stops::run(native, kind, name).unwrap();
        run.result.err().expect("the receiver join stops").trace
    })
    .into();

    Observed {
        answers,
        fields,
        grammars,
        receiver_failures,
    }
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
    let dynamic_names = real.dynamic_names().unwrap();
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
    let mut again = recorded.dynamic_names().unwrap();
    again.source.basis = dynamic_names.source.basis;
    assert_eq!(again, dynamic_names);
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
fn control_grammar_preserves_shared_readers_and_covered_properties() {
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
            assert_eq!(answer.source.method, COMMAND_GRAMMAR_METHOD);
            let covered = kind == DeclarationKind::Trigger
                || ["hidden_effect", "every_owned_planet"].contains(&name);
            assert_eq!(
                answer.completeness,
                if covered {
                    Completeness::Complete
                } else {
                    Completeness::Partial
                }
            );
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
            assert_eq!(
                child.child_families,
                if covered {
                    GrammarProperty::Known(vec![family])
                } else {
                    GrammarProperty::Partial(vec![family])
                }
            );
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
    use pdx_native::{FieldMembers, RepeatBehavior, ValueShape};
    let native = native();
    let observed: Value = expected("field-storage-sdk533.json");
    if !parity::historical_storage_applies(&native.build(), &observed).unwrap() {
        report_historical_storage_skip(&native.build(), &observed).unwrap();
        return;
    }
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

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn dynamic_names_group_flag_commands_by_the_store_they_reach() {
    let native = native();
    assert_eq!(
        native.supports(Operation::DynamicNames),
        pdx_native::Support::Supported
    );
    let answer = native.dynamic_names().unwrap();
    assert_eq!(answer.source.basis, Basis::StaticAnalysis);
    assert_eq!(answer.source.method, DYNAMIC_NAMES_METHOD);
    assert_eq!(answer.completeness, Completeness::Partial);
    let text = serde_json::to_string(&answer).unwrap();
    assert!(!text.contains("Flag("), "no native type name in the answer");
    if let Some(directory) = std::env::var_os("NATIVE_DYNAMIC_EXPECTED_OUT") {
        std::fs::create_dir_all(&directory).unwrap();
        let candidate = parity::candidate(&native, "dynamic-names.json").unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join("dynamic-names.json"),
            candidate,
        )
        .unwrap();
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn every_tracked_candidate_matches_the_reviewed_tree() {
    let recordings = tempfile::tempdir().unwrap();
    let native = native().record_answers_to(recordings.path());
    for name in parity::FILES {
        let tracked = std::fs::read(parity::expected_directory().join(name)).unwrap();
        if *name == "field-storage-sdk533.json" {
            let observed = serde_json::from_slice(&tracked).unwrap();
            if !parity::historical_storage_applies(&native.build(), &observed).unwrap() {
                report_historical_storage_skip(&native.build(), &observed).unwrap();
                continue;
            }
        }
        let candidate = parity::candidate(&native, name).unwrap();
        let report = comparison::compare_static(&native.build(), name, &tracked, &candidate);
        assert!(report.passes(), "{}", report.render_and_save().unwrap());
    }

    let path = recordings.path().join("registries.json");
    let mut answer: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let original = answer["Ok"]["value"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    answer["Ok"]["value"][0]["name"] = json!("deliberately_changed_registry");
    std::fs::write(path, serde_json::to_vec(&answer).unwrap()).unwrap();
    let changed = Native::from_recorded_answers(recordings.path()).unwrap();
    for name in parity::FILES {
        let mut expected =
            std::fs::read_to_string(parity::expected_directory().join(name)).unwrap();
        if *name == "registries.json" {
            expected = expected.replacen(&original, "deliberately_changed_registry", 1);
        }
        if *name == "field-storage-sdk533.json" {
            let observed = serde_json::from_str(&expected).unwrap();
            if !parity::historical_storage_applies(&changed.build(), &observed).unwrap() {
                report_historical_storage_skip(&changed.build(), &observed).unwrap();
                continue;
            }
        }
        let candidate = parity::candidate(&changed, name).unwrap();
        let report =
            comparison::compare_static(&changed.build(), name, expected.as_bytes(), &candidate);
        assert!(report.passes(), "{}", report.render_and_save().unwrap());
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn command_forms_keep_m45_acceptance_and_named_stage_gaps() {
    use pdx_native::{CommandForm, GrammarProperty, ReaderKind};
    let native = native();
    let owner = native
        .command_grammar(DeclarationKind::Effect, "set_owner")
        .unwrap();
    let GrammarProperty::Known(forms) = &owner.value.forms else {
        panic!("{owner:?}")
    };
    assert!(
        matches!(forms.as_slice(), [CommandForm::Value(value)] if value.reader.kind == ReaderKind::Target)
    );
    assert_eq!(owner.value.fixed_keys, GrammarProperty::Known(vec![]));
    // The country result getter has unproved owner-conversion routes on this build.
    let GrammarProperty::Partial(targets) = &owner.value.targets else {
        panic!("{owner:?}");
    };
    assert!(
        matches!(targets.as_slice(), [target] if target.argument == pdx_native::ArgumentPath::Value
        && target.scopes == pdx_native::DeclaredScopes::Unresolved
        && target.stage == pdx_native::TargetCheckStage::Unresolved)
    );
    assert!(
        owner
            .gaps
            .iter()
            .any(|gap| gap.detail == "target-scope-check: unclassified getter call")
    );

    for (kind, name, gap) in [
        (
            DeclarationKind::Trigger,
            "always",
            "value-acceptance: Assign: false without diagnostic",
        ),
        (
            DeclarationKind::Effect,
            "add_district",
            "value-acceptance: Assign: form-reader-call",
        ),
        (
            DeclarationKind::Trigger,
            "has_tradition",
            "value-acceptance: PostValidate: branch-value",
        ),
        (
            DeclarationKind::Effect,
            "set_country_flag",
            "value-acceptance: Read: mixed paths or unknown reader kind",
        ),
        (
            DeclarationKind::Effect,
            "copy_ethos_and_authority",
            "value-acceptance: PostValidate: path limit",
        ),
    ] {
        let answer = native.command_grammar(kind, name).unwrap();
        assert_eq!(answer.completeness, Completeness::Partial, "{name}");
        assert!(
            matches!(answer.value.forms, GrammarProperty::Partial(_)),
            "{name}"
        );
        assert!(
            answer.gaps.iter().any(|found| found.detail == gap),
            "{name}: {answer:?}"
        );
        if name == "add_district" {
            assert_eq!(
                answer.value.forms,
                GrammarProperty::Partial(vec![CommandForm::Block])
            );
        } else {
            assert_eq!(
                answer.value.forms,
                GrammarProperty::Partial(vec![]),
                "{name}"
            );
        }
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn sdk492_fixed_key_grammars_match_the_engine() {
    assert_sdk492_keys(&native());
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build; also audited by command-population"]
fn known_target_lists_have_covered_arguments() {
    use pdx_native::{CommandForm, Field, FieldMembers, GrammarProperty, ReaderKind};
    fn known_fields(fields: &[Field]) -> bool {
        fields.iter().all(|field| {
            field.reader.kind != ReaderKind::Unknown
                && match &field.members {
                    FieldMembers::Fields(children) => known_fields(children),
                    _ => true,
                }
        })
    }
    let native = native();
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        command_grammar_stops::population(&native, kind, |name, run| {
            if !matches!(run.answer.value.targets, GrammarProperty::Known(_)) {
                return;
            }
            let result = run.result.as_ref().unwrap();
            assert!(result.value_only() || result.coverage().covered(), "{name}");
            let GrammarProperty::Known(forms) = &run.answer.value.forms else {
                panic!("{name}");
            };
            assert!(
                forms.iter().all(|form| match form {
                    CommandForm::Value(value) => value.reader.kind != ReaderKind::Unknown,
                    CommandForm::Block => true,
                    _ => false,
                }),
                "{name}"
            );
            let GrammarProperty::Known(keys) = &run.answer.value.fixed_keys else {
                panic!("{name}");
            };
            assert!(known_fields(keys), "{name}");
        })
        .unwrap();
    }
}

/// The changed API properties are checked separately from unrelated static methods.
#[test]
#[ignore = "requires STELLARIS_PATH; NATIVE_NUMERIC_EXPECTED_OUT retains candidates for review"]
fn numeric_reader_api_parity() {
    let native = native();
    let mut differences = comparison::Report::default();
    let names = FIELD_FILES
        .iter()
        .map(|(_, file)| *file)
        .chain(["command-grammars.json"]);
    for name in names {
        let candidate = parity::candidate(&native, name).unwrap();
        if let Some(directory) = std::env::var_os("NATIVE_NUMERIC_EXPECTED_OUT") {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(std::path::Path::new(&directory).join(name), &candidate).unwrap();
        }
        let expected = std::fs::read(parity::expected_directory().join(name)).unwrap();
        differences.extend(comparison::compare_static(
            &native.build(),
            name,
            &expected,
            &candidate,
        ));
    }
    assert!(
        differences.passes(),
        "{}",
        differences.render_and_save().unwrap()
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH; review refreshed field and grammar recordings"]
fn scoped_numeric_api_parity() {
    let native = native();
    let mut differences = comparison::Report::default();
    for name in FIELD_FILES
        .iter()
        .map(|(_, file)| *file)
        .chain(["command-grammars.json"])
    {
        let candidate = parity::candidate(&native, name).unwrap();
        let expected = std::fs::read(parity::expected_directory().join(name)).unwrap();
        differences.extend(comparison::compare_static(
            &native.build(),
            name,
            &expected,
            &candidate,
        ));
    }
    assert!(
        differences.passes(),
        "{}",
        differences.render_and_save().unwrap()
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn numeric_command_arguments_share_registry_conversion_facts() {
    use pdx_native::GrammarProperty;
    let native = native();
    let fields = native.registry_fields("common/megastructures").unwrap();
    for (command, key, registry_field, scale) in [
        ("add_asteroid_belt", "radius", "build_time", 100000),
        ("add_intel_report", "days", "sensor_range", 1),
    ] {
        let answer = native
            .command_grammar(DeclarationKind::Effect, command)
            .unwrap();
        let (GrammarProperty::Known(keys) | GrammarProperty::Partial(keys)) =
            &answer.value.fixed_keys
        else {
            panic!("{command}: keys unresolved");
        };
        let argument = keys.iter().find(|field| field.name == key).unwrap();
        let field = fields
            .value
            .iter()
            .find(|field| field.name == registry_field)
            .unwrap();
        assert_eq!(argument.reader.id, field.reader.id, "{command}/{key}");
        assert_eq!(
            argument.reader.numeric, field.reader.numeric,
            "{command}/{key}"
        );
        let GrammarProperty::Partial(Some(conversion)) = &argument.reader.numeric else {
            panic!("{command}/{key}: numeric facts absent");
        };
        assert_eq!(conversion.scale, GrammarProperty::Known(Some(scale)));
        let (minimum, maximum) = if scale == 1 {
            (
                pdx_native::NumericBound::Signed(-2147483648),
                pdx_native::NumericBound::Signed(2147483647),
            )
        } else {
            (
                pdx_native::NumericBound::Rational {
                    numerator: i64::MIN,
                    denominator: scale,
                },
                pdx_native::NumericBound::Rational {
                    numerator: i64::MAX,
                    denominator: scale,
                },
            )
        };
        assert_eq!(
            conversion.accepted_range,
            GrammarProperty::Known(Box::new(pdx_native::NumericRange {
                minimum: GrammarProperty::Known(minimum),
                maximum: GrammarProperty::Known(maximum),
            }))
        );
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == pdx_native::GapKind::NumericConversion
                    && gap.subject == Some(pdx_native::GapSubject::Field { name: key.into() }))
        );
        assert_eq!(answer.completeness, pdx_native::Completeness::Partial);
    }
}
