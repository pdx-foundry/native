//! Recorded answers stand in for an installation and a game. No process starts in these tests.
use pdx_native::internals::registry_field_stops;
use pdx_native::{
    Basis, Completeness, ContextScopes, DeclarationKind, DeclaredScopes, DeclaredTags, Disposal,
    DynamicNameForm, DynamicNameKind, EntryScope, Error, GameOptions, GapKind, GapSubject,
    GenerationCondition, LinkData, LocalizationOutput, NamespaceOwner, Native, Operation,
    OutputScope, ReaderKind, RuleKind, Support,
};
use serde_json::json;
use std::{fs, path::Path};

fn recorded_field(name: &str, kind: &str) -> serde_json::Value {
    json!({ "name": name, "reader": { "id": if kind == "Unknown" { None } else { Some(name) }, "kind": kind, "family": "Unknown", "numeric": "Unresolved", "scoped_operand": "Unresolved" },
        "shape": { "value": "Unknown", "repeat": "Unknown" },
        "read": [{ "condition": "Unresolved", "outcome": "Unresolved" }],
        "members": "Unresolved", "domain": "Unknown", "uses": [], "reference": "NotEstablished" })
}

fn source() -> serde_json::Value {
    json!({ "build": "example-build", "native_version": "0.1.0",
            "method": "example/v1", "basis": "LiveObservation" })
}

fn write(root: &Path, file: &str, value: serde_json::Value) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

#[test]
fn modifier_block_members_read_back_with_the_existing_field_operation() {
    use pdx_native::{FieldMembers, GrammarProperty, ModifierEntry};
    let root = recorded();
    let mut field = recorded_field("modifier", "Block");
    let mut child = recorded_field("description", "String");
    child["members"] = json!("None");
    field["reader"]["family"] = json!("Modifier");
    field["members"] = json!({"ModifierBlock": {
        "fixed_keys": {"Partial": [child]},
        "entries": {"Known": [
            {"Numeric": {"value": {"id":"shared-fixed", "kind":"FixedPoint", "family":"NotApplicable", "numeric":"Unresolved", "scoped_operand":"Unresolved"}}},
            {"Reference": {"target":{"Registry":{"name":"common/static_modifiers"}}, "value":"FixedPoint"}}
        ]}
    }});
    write(
        root.path(),
        "registry_fields/common/traditions.json",
        json!({"Ok": {
            "value":[field], "completeness":"Partial", "gaps":[], "source":source()
        }}),
    );
    let answer = Native::from_recorded_answers(root.path())
        .unwrap()
        .registry_fields("common/traditions")
        .unwrap();
    assert_eq!(answer.source.basis, Basis::Recorded);
    let FieldMembers::ModifierBlock(block) = &answer.value[0].members else {
        panic!("missing block")
    };
    let GrammarProperty::Partial(keys) = &block.fixed_keys else {
        panic!("missing keys")
    };
    assert_eq!(keys[0].name, "description");
    assert_eq!(keys[0].reader.kind, ReaderKind::String);
    let GrammarProperty::Known(entries) = &block.entries else {
        panic!("missing entries")
    };
    assert!(
        matches!(&entries[0], ModifierEntry::Numeric { value } if value.kind == ReaderKind::FixedPoint)
    );
    assert!(matches!(
        &entries[1],
        ModifierEntry::Reference {
            value: ReaderKind::FixedPoint,
            ..
        }
    ));
}

/// Files that a consumer could write by hand: a complete answer, a partial answer, and an error.
fn recorded() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "build.json", json!("example-build"));
    write(
        root.path(),
        "registries.json",
        json!({ "Ok": { "value": [{ "name": "common/traditions" }], "completeness": "Partial",
            "gaps": [{ "kind": "OutsideMethod", "subject": null, "detail": "Example." }],
            "source": source() } }),
    );
    write(
        root.path(),
        "defines.json",
        json!({ "Ok": {
            "value": [{ "namespace": "NCamera", "name": "FOV", "value_type": "Float" }],
            "completeness": "Partial",
            "gaps": [{ "kind": "UnresolvedReader", "subject": {"kind": "answer_item", "name": "NGraphics.ORBIT_HSV"},
                "detail": "reader path exceeds the table-search limit" }],
            "source": source()
        }}),
    );
    write(
        root.path(),
        "registry_fields/common/traditions.json",
        json!({ "Ok": { "value": [
            recorded_field("boolean", "Boolean"),
            recorded_field("integer", "Integer"),
            recorded_field("fixed", "FixedPoint"),
            recorded_field("string", "String"),
            recorded_field("reference", "Reference"),
            recorded_field("block", "Block"),
            recorded_field("unknown", "Unknown")
        ], "completeness": "Partial",
            "gaps": [{ "kind": "ReaderSemantics", "subject": {"kind": "field", "name": "unknown"}, "detail": "Example." }],
            "source": source() } }),
    );
    write(
        root.path(),
        "declarations/effect.json",
        json!({ "Ok": {
            "value": [
                { "name": "always", "description": "Always succeeds", "usage": "", "scopes": "Any" },
                { "name": "win", "description": "Wins", "usage": "win = yes", "scopes": { "Listed": [{ "id": "country-id", "name": "country" }] } }
            ],
            "completeness": "Partial",
            "gaps": [
                { "kind": "UnnamedDeclaration", "subject": null, "detail": "runtime token" },
            { "kind": "UnresolvedPath", "subject": {"kind": "answer_item", "name": "missing"}, "detail": "documentation" }
            ],
            "source": source()
        }}),
    );
    write(
        root.path(),
        "modifiers.json",
        json!({ "Ok": {
            "value": [
                { "name": "blank_modifier", "category_tags": { "Listed": ["Pops"] } },
                { "name": "unfollowed", "category_tags": "Unresolved" }
            ],
            "completeness": "Partial",
            "gaps": [
                { "kind": "UnnamedDeclaration", "subject": null, "detail": "generated family" },
                { "kind": "UnresolvedPath", "subject": {"kind": "answer_item", "name": "unfollowed"}, "detail": "category tags" }
            ],
            "source": source()
        }}),
    );
    write(
        root.path(),
        "modifier_families/common/bypass.json",
        json!({ "Ok": {
            "value": [{
                "name": ["ItemKey", { "Literal": "_ship_windup_mult" }],
                "category_tags": { "Listed": ["Ships"] },
                "condition": "Always",
                "name_limit": 24
            }],
            "completeness": "Partial",
            "gaps": [{ "kind": "UnnamedDeclaration", "subject": null, "detail": "unjoined sites" }],
            "source": source()
        }}),
    );
    write(
        root.path(),
        "scopes.json",
        json!({ "Ok": {
            "value": {
                "types": [
                    { "id": "planet-id", "name": "planet", "keywords": ["planet"] },
                    { "id": "ship-id", "name": "ship", "keywords": ["ship"] }
                ],
                "groups": [{ "keyword": "carrier", "scopes": [
                    { "id": "planet-id", "name": "planet" }, { "id": "ship-id", "name": "ship" }
                ] }]
            },
            "completeness": "Complete",
            "gaps": [],
            "source": source()
        }}),
    );
    write(
        root.path(),
        "scope_links.json",
        json!({ "Ok": {
            "value": [
                { "name": "carrier", "input_scopes": { "Listed": [{ "id": "colony-id", "name": "colony" }] },
                  "output_scope": { "Listed": [
                      { "id": "planet-id", "name": "planet" }, { "id": "ship-id", "name": "ship" }
                  ] }, "data": "None" },
                { "name": "event_target", "input_scopes": "Any",
                  "output_scope": "Various", "data": { "Prefix": "event_target:" } },
                { "name": "prev", "input_scopes": "Any", "output_scope": "Various", "data": "None" }
            ],
            "completeness": "Complete",
            "gaps": [],
            "source": source()
        }}),
    );
    root
}

#[test]
fn language_declarations_read_recorded_values_and_missing_files_are_not_recorded() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path()).unwrap();

    let modifiers = native.modifiers().unwrap();
    assert_eq!(modifiers.source.basis, Basis::Recorded);
    assert_eq!(
        modifiers.value[0].category_tags,
        DeclaredTags::Listed(vec!["Pops".into()])
    );
    assert_eq!(modifiers.value[1].category_tags, DeclaredTags::Unresolved);

    let families = native.modifier_families("common/bypass/").unwrap();
    assert_eq!(families.source.basis, Basis::Recorded);
    let family = &families.value[0];
    assert_eq!(family.condition, GenerationCondition::Always);
    assert_eq!(
        family.name_for("lgate").as_deref(),
        Some("lgate_ship_windup_mult")
    );
    assert_eq!(family.name_for("a_long_bypass_key"), None);
    assert!(matches!(
        native.modifier_families("common/buildings"),
        Err(Error::NotRecorded { .. })
    ));

    let links = native.scope_links().unwrap();
    assert_eq!(links.source.basis, Basis::Recorded);
    let OutputScope::Listed(output) = &links.value[0].output_scope else {
        panic!("carrier declares its output");
    };
    let output: Vec<_> = output.iter().map(|scope| scope.name.as_str()).collect();
    assert_eq!(output, ["planet", "ship"]);
    assert_eq!(
        links.value[1].data,
        LinkData::Prefix("event_target:".into())
    );
    assert_eq!(links.value[2].output_scope, OutputScope::Various);

    assert!(matches!(
        native.modifier_categories(),
        Err(Error::NotRecorded { .. })
    ));
    let scopes = native.scopes().unwrap();
    assert_eq!(scopes.source.basis, Basis::Recorded);
    let group = &scopes.value.groups[0];
    assert_eq!(group.keyword, "carrier");
    for (reference, scope) in group.scopes.iter().zip(&scopes.value.types) {
        assert_eq!(reference.id, scope.id);
    }
}

#[test]
fn defines_read_recorded_names_types_gaps_and_basis() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path()).unwrap();
    let answer = native.defines().unwrap();
    assert_eq!(answer.source.basis, Basis::Recorded);
    assert_eq!(answer.value[0].namespace, "NCamera");
    assert_eq!(answer.value[0].name, "FOV");
    assert_eq!(
        answer.value[0].value_type,
        pdx_native::DefineValueType::Float
    );
    assert_eq!(
        answer.gaps[0].subject.as_ref(),
        Some(&GapSubject::AnswerItem {
            name: "NGraphics.ORBIT_HSV".into(),
        })
    );

    fs::remove_file(root.path().join("defines.json")).unwrap();
    assert!(matches!(native.defines(), Err(Error::NotRecorded { .. })));
}

#[test]
fn dynamic_names_read_recorded_namespaces_and_basis() {
    let root = recorded();
    write(
        root.path(),
        "dynamic_names.json",
        json!({ "Ok": {
            "value": [{
                "id": "0123456789abcdef", "kind": "IntegerFlag", "owner": "Global",
                "defined_by": [{ "kind": "Effect", "name": "set_global_flag" }],
                "removed_by": [],
                "read_by": [{ "kind": "Trigger", "name": "has_global_flag" }],
                "dynamic_form": "TargetSuffix"
            }],
            "completeness": "Partial",
            "gaps": [{ "kind": "UnresolvedReader", "subject": {"kind": "answer_item", "name": "exists"},
                "detail": "trigger was not followed to its flag store (command-vtable)" }],
            "source": source()
        }}),
    );
    let native = Native::from_recorded_answers(root.path()).unwrap();
    let answer = native.dynamic_names().unwrap();
    assert_eq!(answer.source.basis, Basis::Recorded);
    let [namespace] = answer.value.as_slice() else {
        panic!("one recorded namespace");
    };
    assert_eq!(namespace.owner, NamespaceOwner::Global);
    assert_eq!(namespace.kind, DynamicNameKind::IntegerFlag);
    assert_eq!(namespace.read_by[0].kind, DeclarationKind::Trigger);
    assert_eq!(namespace.read_by[0].name, "has_global_flag");
    assert_eq!(namespace.dynamic_form, DynamicNameForm::TargetSuffix);

    fs::remove_file(root.path().join("dynamic_names.json")).unwrap();
    assert!(matches!(
        native.dynamic_names(),
        Err(Error::NotRecorded { .. })
    ));
}

#[test]
fn localization_declarations_read_recorded_joins_and_outputs() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path()).unwrap();
    assert!(matches!(
        native.localization_declarations(),
        Err(Error::NotRecorded { .. })
    ));

    let country = json!({ "id": "country-context", "name": "Country" });
    let dead_country = json!({ "id": "dead-country-context", "name": "Dead Country" });
    let planet = json!({ "id": "planet-context", "name": "Planet" });
    write(
        root.path(),
        "localization_declarations.json",
        json!({ "Ok": {
            "value": {
                "contexts": [
                    { "id": "country-context", "name": "Country",
                      "scopes": { "Joined": [{ "id": "country-id", "name": "country" }] } },
                    { "id": "dead-country-context", "name": "Dead Country", "scopes": "Missing" },
                    { "id": "planet-context", "name": "Planet", "scopes": { "Partial": [] } }
                ],
                "commands": [{ "name": "GetName", "contexts": [country, dead_country] }],
                "links": [
                    { "name": "Owner", "input_contexts": [planet], "output": { "Listed": [country] } },
                    { "name": "Root", "input_contexts": [country], "output": "Various" },
                    { "name": "Third_party", "input_contexts": [country], "output": "Unchanged" },
                    { "name": "MainAttacker", "input_contexts": [dead_country], "output": "Unresolved" }
                ]
            },
            "completeness": "Partial",
            "gaps": [
                { "kind": "UnresolvedPath", "subject": {"kind": "scope_type", "id": "planet-id", "name": "planet"}, "detail": "scope join" },
                { "kind": "UnresolvedPath", "subject": {"kind": "localization_link", "name": "MainAttacker"}, "detail": "dead object" }
            ],
            "source": source()
        }}),
    );

    let answer = native.localization_declarations().unwrap();
    assert_eq!(answer.source.basis, Basis::Recorded);
    assert!(
        matches!(answer.gaps[0].subject.as_ref(), Some(GapSubject::ScopeType { id, name })
        if serde_json::to_value(id).unwrap() == json!("planet-id") && name == "planet")
    );
    assert_eq!(
        answer.gaps[1].subject.as_ref(),
        Some(&GapSubject::LocalizationLink {
            name: "MainAttacker".into(),
        })
    );
    let localization = answer.value;
    let scopes: Vec<_> = localization
        .contexts
        .iter()
        .map(|context| &context.scopes)
        .collect();
    assert!(matches!(scopes[0], ContextScopes::Joined(joined) if joined[0].name == "country"));
    assert_eq!(scopes[1], &ContextScopes::Missing);
    assert_eq!(scopes[2], &ContextScopes::Partial(Vec::new()));

    let dead = &localization.commands[0].contexts[1];
    assert_eq!(
        dead.id, localization.contexts[1].id,
        "a missing join keeps its command"
    );
    let outputs: Vec<_> = localization.links.iter().map(|link| &link.output).collect();
    assert!(
        matches!(outputs[0], LocalizationOutput::Listed(listed) if listed[0].name == "Country")
    );
    assert_eq!(outputs[1], &LocalizationOutput::Various);
    assert_eq!(outputs[2], &LocalizationOutput::Unchanged);
    assert_eq!(outputs[3], &LocalizationOutput::Unresolved);
}

#[test]
fn declarations_read_recorded_values_and_missing_kind_is_not_recorded() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path()).unwrap();
    let effect = native.declarations(DeclarationKind::Effect).unwrap();
    assert_eq!(effect.source.basis, Basis::Recorded);
    assert_eq!(effect.completeness, Completeness::Partial);
    assert_eq!(effect.value[0].scopes, DeclaredScopes::Any);
    let DeclaredScopes::Listed(scopes) = &effect.value[1].scopes else {
        panic!("win lists its scopes");
    };
    assert_eq!(scopes[0].name, "country");
    assert!(matches!(
        native.declarations(DeclarationKind::Trigger),
        Err(Error::NotRecorded { .. })
    ));
}

#[test]
fn static_questions_read_recorded_files_and_always_report_the_recorded_basis() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path()).unwrap();
    assert_eq!(
        native.supports(pdx_native::Operation::Registries),
        Support::Supported
    );
    let answer = native.registries().unwrap();
    assert_eq!(answer.value[0].name, "common/traditions");
    assert_eq!(answer.completeness, Completeness::Partial);
    assert_eq!(answer.gaps[0].kind, GapKind::OutsideMethod);
    // The file says LiveObservation. A recorded answer never passes as an observation.
    assert_eq!(answer.source.basis, Basis::Recorded);
    assert_eq!(answer.source.build, native.build());

    let fields = native.registry_fields("common/traditions").unwrap();
    assert_eq!(
        fields
            .value
            .iter()
            .map(|field| field.reader.kind)
            .collect::<Vec<_>>(),
        [
            ReaderKind::Boolean,
            ReaderKind::Integer,
            ReaderKind::FixedPoint,
            ReaderKind::String,
            ReaderKind::Reference,
            ReaderKind::Block,
            ReaderKind::Unknown,
        ]
    );
    assert_eq!(fields.source.basis, Basis::Recorded);
    assert!(
        fields
            .value
            .iter()
            .all(|field| field.reader.numeric == pdx_native::GrammarProperty::Unresolved)
    );
}

#[test]
fn a_question_with_no_file_is_not_recorded_and_never_an_empty_answer() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path()).unwrap();
    for registry in ["common/armies", "../registries", "common/../../escape", ""] {
        assert!(
            matches!(
                native.registry_fields(registry),
                Err(Error::NotRecorded { .. })
            ),
            "{registry}"
        );
    }
    fs::write(root.path().join("registries.json"), "damaged").unwrap();
    assert!(matches!(native.registries(), Err(Error::Recorded(_))));
}

#[test]
fn recorded_answers_hold_no_method_result_and_are_not_recorded_again() {
    let root = recorded();
    let copy = tempfile::tempdir().unwrap();
    let native = Native::from_recorded_answers(root.path())
        .unwrap()
        .record_answers_to(copy.path());
    assert!(matches!(
        registry_field_stops::run(&native, "common/traditions"),
        Err(Error::Unsupported {
            operation: Operation::RegistryFields,
            ..
        })
    ));
    assert_eq!(native.registries().unwrap().source.basis, Basis::Recorded);
    assert_eq!(fs::read_dir(copy.path()).unwrap().count(), 0);
}

#[test]
fn opening_recorded_answers_requires_valid_build_metadata() {
    let root = tempfile::tempdir().unwrap();
    assert!(matches!(
        Native::from_recorded_answers(root.path()),
        Err(Error::Recorded(_))
    ));
    for invalid in ["broken", "{}", "null"] {
        fs::write(root.path().join("build.json"), invalid).unwrap();
        assert!(matches!(
            Native::from_recorded_answers(root.path()),
            Err(Error::Recorded(_))
        ));
    }
}

#[tokio::test]
async fn static_and_live_answers_from_another_build_are_refused() {
    let root = recorded();
    write(
        root.path(),
        "loaded_modifiers.json",
        json!({ "Ok": {
            "value": {"content": "Installation", "modifiers": [], "registry_items": {}},
            "completeness": "Complete", "gaps": [], "source": source()
        }}),
    );
    write(root.path(), "build.json", json!("another-build"));
    let native = Native::from_recorded_answers(root.path()).unwrap();
    assert!(matches!(native.registries(), Err(Error::Recorded(_))));
    let mut game = native
        .start_game(GameOptions::new(std::process::Command::new(
            "must-not-start",
        )))
        .await
        .unwrap();
    assert!(matches!(
        game.loaded_modifiers().await,
        Err(Error::Recorded(_))
    ));
    assert_eq!(game.close().await.unwrap(), Disposal::NotApplicable);
}

#[tokio::test]
async fn the_loaded_modifier_inventory_reads_its_recorded_file_with_no_game() {
    let root = recorded();
    write(
        root.path(),
        "loaded_modifiers.json",
        json!({ "Ok": {
            "value": {
                "modifiers": [
                    { "name": "pop_happiness", "category_tags": { "Listed": ["Pops"] },
                      "declared": true, "generated_by": [] },
                    { "name": "planet_building_capital_build_speed_mult",
                      "category_tags": { "Listed": ["Colony"] }, "declared": false,
                      "generated_by": [{ "registry": "common/buildings", "item": "building_capital",
                          "template": [{ "Literal": "planet_" }, "ItemKey",
                                       { "Literal": "_build_speed_mult" }] }] },
                    { "name": "job_example_add", "category_tags": "Unresolved",
                      "declared": false, "generated_by": [] }
                ],
                "registry_items": { "common/buildings": ["building_capital"] },
                "content": "Installation"
            },
            "completeness": "Partial",
            "gaps": [{ "kind": "UnnamedDeclaration", "subject": null,
                "detail": "1 loaded modifiers are neither declared nor generated" }],
            "source": source()
        }}),
    );
    let native = Native::from_recorded_answers(root.path()).unwrap();
    let options = GameOptions::new(std::process::Command::new("must-not-start")).loaded_modifiers();
    let mut game = native.start_game(options).await.unwrap();
    let answer = game.loaded_modifiers().await.unwrap();
    assert_eq!(answer.source.basis, Basis::Recorded);
    assert_eq!(answer.value.modifiers.len(), 3);
    assert_eq!(
        answer.value.content,
        pdx_native::LoadedContent::Installation
    );
    assert_eq!(
        answer.value.modifiers[1].generated_by[0].item,
        "building_capital"
    );
    assert_eq!(game.close().await.unwrap(), Disposal::NotApplicable);

    // A session with a fixture reads the recording of that fixture, which does not exist here.
    let fixture = pdx_native::FixtureRequest::field_outcomes(
        "common/tradition_categories/example.txt",
        "example = {}\n",
        [pdx_native::FixtureFieldQuestion::new(
            "common/tradition_categories",
            "example",
            "traditions",
        )
        .with_parsing()],
    );
    let mut game = native
        .start_game(GameOptions::new(std::process::Command::new("must-not-start")).fixture(fixture))
        .await
        .unwrap();
    assert!(matches!(
        game.loaded_modifiers().await,
        Err(Error::NotRecorded { .. })
    ));
}

#[test]
fn callbacks_read_recorded_alternatives_candidates_and_gaps() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path()).unwrap();
    assert!(matches!(
        native.on_actions(),
        Err(Error::NotRecorded { .. })
    ));
    assert!(matches!(
        native.game_rules(),
        Err(Error::NotRecorded { .. })
    ));

    let country = json!({ "Scope": { "id": "country-id", "name": "country" } });
    let fleet = json!({ "Scope": { "id": "fleet-id", "name": "fleet" } });
    let planet = json!({ "Scope": { "id": "planet-id", "name": "planet" } });
    let colony = json!({ "Scope": { "id": "colony-id", "name": "colony" } });
    write(
        root.path(),
        "on_actions.json",
        json!({ "Ok": {
            "value": [
                { "name": "on_fleet_enter_orbit", "entries": [
                    { "this": fleet, "root": "SelfLink", "from": [planet, "SelfLink"], "prev": ["SelfLink"] },
                    { "this": fleet, "root": "SelfLink", "from": ["Unresolved"], "prev": ["SelfLink"] }
                ] },
                { "name": "on_game_start", "entries": [
                    { "this": "NotSet", "root": "SelfLink", "from": ["SelfLink"], "prev": ["NotSet", colony, "SelfLink"] }
                ] },
                { "name": "on_press_begin", "entries": [] }
            ],
            "completeness": "Partial",
            "gaps": [
                { "kind": "UnresolvedPath", "subject": {"kind": "answer_item", "name": "on_press_begin"},
                  "detail": "a call site passes a command that builds its own scope" },
                { "kind": "UnnamedDeclaration", "subject": null,
                  "detail": "31 call sites could not be named" }
            ],
            "source": source()
        }}),
    );
    write(
        root.path(),
        "game_rules.json",
        json!({ "Ok": {
            "value": [
                { "name": "can_colonize_planet", "kind": "Scripted", "entries": [
                    { "this": planet, "root": country, "from": ["SelfLink"], "prev": ["SelfLink"] }
                ] },
                { "name": "leader_election_weight", "kind": "Weighted", "entries": [] }
            ],
            "completeness": "Partial",
            "gaps": [
                { "kind": "UnresolvedPath", "subject": {"kind": "answer_item", "name": "leader_election_weight"},
                  "detail": "no followed call site" }
            ],
            "source": source()
        }}),
    );

    let answer = native.on_actions().unwrap();
    assert_eq!(answer.source.basis, Basis::Recorded);
    assert_eq!(answer.completeness, Completeness::Partial);
    let orbit = &answer.value[0];
    assert_eq!(orbit.entries.len(), 2, "alternatives stay separate");
    assert!(
        matches!(&orbit.entries[0].from[0], EntryScope::Scope(scope) if scope.name == "planet")
    );
    assert_eq!(orbit.entries[1].from, [EntryScope::Unresolved]);
    let start = &answer.value[1].entries[0];
    assert_eq!(
        (&start.this, &start.root, start.from.as_slice()),
        (
            &EntryScope::NotSet,
            &EntryScope::SelfLink,
            &[EntryScope::SelfLink][..]
        )
    );
    assert!(
        matches!(&start.prev[..], [EntryScope::NotSet, EntryScope::Scope(scope), EntryScope::SelfLink] if scope.name == "colony")
    );
    assert!(answer.value[2].entries.is_empty());
    assert_eq!(
        answer.gaps[0]
            .subject
            .as_ref()
            .map(|subject| subject.name()),
        Some("on_press_begin")
    );

    let rules = native.game_rules().unwrap().value;
    assert_eq!(rules[0].kind, RuleKind::Scripted);
    assert!(
        matches!(&rules[0].entries[0].root, EntryScope::Scope(scope) if scope.name == "country")
    );
    assert_eq!(rules[1].kind, RuleKind::Weighted);
    assert!(rules[1].entries.is_empty());
}

#[test]
fn command_grammar_round_trip_preserves_partial_properties_and_unknown_commands() {
    use pdx_native::{BlockFamily, CommandGrammar, GrammarProperty};
    let root = recorded();
    let value = json!({
        "forms": "Unresolved",
        "targets": "Unresolved",
        "reader": {"id": "shared-control-reader", "kind": "Block", "family": "Effect", "numeric": "Unresolved", "scoped_operand": "Unresolved"},
        "child_families": {"Known": ["Effect"]},
        "fixed_keys": {"Partial": []},
        "numeric_keys": {"Partial": {
            "forms": "Unresolved",
        "targets": "Unresolved",
            "reader": {"id": "weighted-entry", "kind": "Block", "family": "Effect", "numeric": "Unresolved", "scoped_operand": "Unresolved"},
            "child_families": {"Partial": ["Effect"]},
            "fixed_keys": "Unresolved",
            "numeric_keys": "Unresolved",
            "ordering": "Unresolved",
            "durations": "Unresolved"
        }},
        "ordering": {"Partial": [{
            "child": "else",
            "conditions": [{"First": false}, {"Previous": {"keys": ["if", "else_if"], "matches": true}}],
            "outcome": {"Dispatch": "Effect"}
        }]},
        "durations": {"Known": [{
            "units": [
                {"key": "days", "factor": {"Known": null}},
                {"key": "months", "factor": {"Known": 30}}
            ],
            "combination": {"Known": {"SharedFactor": {"initial_factor": 1}}},
            "omitted_count": {"Known": 0}
        }]}
    });
    let grammar: CommandGrammar = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&grammar).unwrap(), value);
    write(
        root.path(),
        "command_grammar/effect/if.json",
        json!({"Ok": {
            "value": value, "completeness": "Partial",
            "gaps": [{"kind": "ReaderSemantics", "subject": null, "detail": "Ordering unresolved."}],
            "source": source()
        }}),
    );
    let error = Error::UnknownCommand {
        kind: DeclarationKind::Effect,
        name: "limit".into(),
    };
    write(
        root.path(),
        "command_grammar/effect/limit.json",
        json!({"Err": error}),
    );
    let native = Native::from_recorded_answers(root.path()).unwrap();
    assert_eq!(
        native.supports(Operation::CommandGrammar),
        Support::Supported
    );
    let answer = native
        .command_grammar(DeclarationKind::Effect, "if")
        .unwrap();
    assert_eq!(answer.value, grammar);
    assert_eq!(answer.value.reader.family, BlockFamily::Effect);
    assert_eq!(answer.value.fixed_keys, GrammarProperty::Partial(vec![]));
    assert_eq!(answer.completeness, Completeness::Partial);
    assert_eq!(answer.source.basis, Basis::Recorded);
    assert_eq!(
        native.command_grammar(DeclarationKind::Effect, "limit"),
        Err(error)
    );
}

#[test]
fn recorded_answer_properties_must_be_present() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path()).unwrap();
    for property in [
        "reference",
        "reader/numeric",
        "reader/scoped_operand",
        "reader/family",
    ] {
        let mut field = recorded_field("number", "Integer");
        let (parent, key) = property.rsplit_once('/').unwrap_or(("", property));
        let object = if parent.is_empty() {
            &mut field
        } else {
            &mut field[parent]
        };
        object.as_object_mut().unwrap().remove(key);
        write(
            root.path(),
            "registry_fields/common/traditions.json",
            json!({"Ok": {
                "value": [field], "completeness": "Complete", "gaps": [], "source": source()
            }}),
        );
        assert!(
            matches!(
                native.registry_fields("common/traditions"),
                Err(Error::Recorded(_))
            ),
            "{property}"
        );
    }
    let grammar = json!({
        "reader": {"id": null, "kind": "Unknown", "family": "Unknown", "numeric":"Unresolved", "scoped_operand":"Unresolved"},
        "forms": "Unresolved", "targets": "Unresolved", "durations": "Unresolved",
        "child_families": "Unresolved", "fixed_keys": "Unresolved",
        "numeric_keys": "Unresolved", "ordering": "Unresolved"
    });
    for property in ["forms", "targets", "durations"] {
        let mut missing = grammar.clone();
        missing.as_object_mut().unwrap().remove(property);
        assert!(
            serde_json::from_value::<pdx_native::CommandGrammar>(missing).is_err(),
            "{property}"
        );
    }
}

#[test]
fn recorded_numeric_facts_keep_partial_properties_and_exact_bounds() {
    use pdx_native::{GrammarProperty, NumericBound, NumericConversion, NumericRange};
    let root = recorded();
    let mut field = recorded_field("number", "Integer");
    let conversion = NumericConversion {
        width_bits: GrammarProperty::Known(64),
        accepted_range: GrammarProperty::Partial(Box::new(NumericRange {
            minimum: GrammarProperty::Known(NumericBound::Signed(i64::MIN)),
            maximum: GrammarProperty::Known(NumericBound::Unsigned(u64::MAX)),
        })),
        ..NumericConversion::default()
    };
    let numeric = GrammarProperty::Partial(Some(conversion));
    field["reader"]["numeric"] = serde_json::to_value(&numeric).unwrap();
    write(
        root.path(),
        "registry_fields/common/numeric.json",
        json!({"Ok": {
            "value": [field], "completeness": "Partial", "gaps": [], "source": source()
        }}),
    );
    let native = Native::from_recorded_answers(root.path()).unwrap();
    let answer = native.registry_fields("common/numeric").unwrap();
    assert_eq!(answer.value[0].reader.numeric, numeric);
    assert_eq!(answer.source.basis, Basis::Recorded);
}

#[test]
fn recorded_support_requires_a_file_for_the_operation() {
    use pdx_native::Operation;
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "build.json", json!("authored-build"));
    let native = Native::from_recorded_answers(root.path()).unwrap();
    for operation in Operation::ALL {
        assert!(matches!(
            native.supports(*operation),
            Support::Unsupported(_)
        ));
    }
    fs::create_dir_all(root.path().join("registry_fields/common/empty")).unwrap();
    fs::write(
        root.path().join("registry_fields/common/notes.txt"),
        "not an answer",
    )
    .unwrap();
    assert!(matches!(
        native.supports(Operation::RegistryFields),
        Support::Unsupported(_)
    ));
    write(
        root.path(),
        "registry_fields/common/example.json",
        json!({"Err": "BuildChanged"}),
    );
    write(
        root.path(),
        "loaded_modifiers.json",
        json!({"Err": "BuildChanged"}),
    );
    assert_eq!(
        native.supports(Operation::RegistryFields),
        Support::Supported
    );
    assert_eq!(
        native.supports(Operation::LoadedModifiers),
        Support::Supported
    );
    assert!(matches!(
        native.supports(Operation::ObserveFixture),
        Support::Unsupported(_)
    ));
}
