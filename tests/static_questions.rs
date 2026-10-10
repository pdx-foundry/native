//! Parity of the static questions with tracked expected output for the M452 build.
//! Needs the real executable: set `STELLARIS_PATH` and run with `--ignored`. No game starts.
mod parity;
use parity::*;

use pdx_native::internals::registry_field_stops::{FieldGap, TokenPath, Unresolved};
use pdx_native::internals::{
    COMMAND_GRAMMAR_METHOD, DEFINES_METHOD, DERIVED_NAMES_METHOD, DYNAMIC_NAMES_METHOD,
    MODIFIER_NODES_METHOD, command_grammar_stops, registry_field_stops, trace_causes,
};
use pdx_native::{
    Answer, Basis, Completeness, ContextScopes, DeclarationKind, DeclaredScopes, DeclaredTags,
    EntryScope, Error, FieldDomain, FieldReference, GapKind, GapSubject, KeptCategories, KeyMatch,
    LinkData, LocalizationContextReference, LocalizationDeclarations, LocalizationOutput,
    LookupStage, Native, Operation, OutputScope, ReaderKind, ReferenceTarget, RuleKind, ScopeId,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn triggered_modifiers_match_the_recorded_variants_and_shared_identities() {
    use pdx_native::{BlockFamily, Field, FieldMembers, GrammarProperty};

    let native = native();
    assert_eq!(
        triggered_modifiers(&native).unwrap(),
        expected::<Value>("triggered-modifiers.json")
    );
    let mut variants = BTreeMap::new();
    for registry in native.registries().unwrap().value {
        for field in native.registry_fields(&registry.name).unwrap().value {
            check_triggered_identity(&mut variants, &field).unwrap();
        }
    }
    assert_eq!(variants.len(), 3);

    let fields = native.registry_fields("common/traditions").unwrap().value;
    let field = |name: &str| -> &Field { fields.iter().find(|field| field.name == name).unwrap() };
    let clause = field("triggered_modifier");
    assert_eq!(clause.reader.family, BlockFamily::TriggeredModifier);
    let FieldMembers::TriggeredModifier(block) = &clause.members else {
        panic!("{:?}", clause.members);
    };
    let (GrammarProperty::Known(keys) | GrammarProperty::Partial(keys)) = &block.fixed_keys else {
        panic!("{:?}", block.fixed_keys);
    };
    let key = |name: &str| keys.iter().find(|key| key.name == name).unwrap();

    // Independent expectations: the config's `triggered_modifier_clause_base` keys.
    let mut names: Vec<_> = keys.iter().map(|key| key.name.as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "key",
            "modifier",
            "mult",
            "multiplier",
            "not_potential_override_text_key",
            "potential",
            "show_if_not_potential"
        ]
    );
    assert_eq!(key("potential").reader.kind, ReaderKind::Block);
    assert_eq!(key("potential").reader.family, BlockFamily::Trigger);
    assert_eq!(
        key("show_if_not_potential").reader.kind,
        ReaderKind::Boolean
    );
    for name in ["key", "not_potential_override_text_key"] {
        assert_eq!(key(name).reader.kind, ReaderKind::String);
        assert_eq!(key(name).reference, FieldReference::NotEstablished);
    }
    for name in ["mult", "multiplier"] {
        assert_eq!(key(name).reader.kind, ReaderKind::ScopedNumeric);
        assert!(!matches!(
            key(name).reader.numeric,
            GrammarProperty::Unresolved
        ));
    }

    // The nested block and direct entries reach the tradition's own modifier reader.
    let modifier = field("modifier");
    assert_eq!(key("modifier").reader.id, modifier.reader.id);
    assert!(matches!(
        key("modifier").members,
        FieldMembers::ModifierBlock(_)
    ));
    let GrammarProperty::Known(other_keys) = &block.other_keys else {
        panic!("{:?}", block.other_keys);
    };
    assert_eq!(other_keys.reader.id, modifier.reader.id);
    let GrammarProperty::Partial(modifier_keys) = &other_keys.block.fixed_keys else {
        panic!("{:?}", other_keys.block.fixed_keys);
    };
    for name in [
        "custom_tooltip",
        "show_only_custom_tooltip",
        "description",
        "description_parameters",
        "divide_over_pop_groups",
    ] {
        assert!(modifier_keys.iter().any(|key| key.name == name), "{name}");
    }

    // Negative control: `common/federation_perks` skips the body of its `triggered_modifier`.
    let perks = native
        .registry_fields("common/federation_perks")
        .unwrap()
        .value;
    assert!(
        perks
            .iter()
            .filter(|field| field.name == "triggered_modifier")
            .all(|field| field.reader.family != BlockFamily::TriggeredModifier)
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn weight_blocks_match_the_recorded_variants_and_shared_identities() {
    use pdx_native::{
        BlockFamily, EmptyKey, Field, FieldCondition, FieldMembers, FieldReference,
        GrammarProperty, KeyMatch, LookupStage, MissingResult, ReadScope, ReferenceLookup,
        ReferenceTarget, RepeatBehavior, WeightBlock, WeightOtherKeys,
    };
    use std::collections::BTreeSet;

    let native = native();
    assert_eq!(
        weight_blocks(&native).unwrap(),
        expected::<Value>("weight-blocks.json")
    );
    let mut variants = BTreeMap::new();
    for registry in native.registries().unwrap().value {
        for field in native.registry_fields(&registry.name).unwrap().value {
            check_weight_identity(&mut variants, &field).unwrap();
        }
    }
    assert_eq!(variants.len(), 5);

    let weight = |registry: &str| -> Field {
        let answer = native.registry_fields(registry).unwrap();
        answer
            .value
            .into_iter()
            .find(|field| field.name == "ai_weight")
            .unwrap()
    };
    let agenda = weight("common/council_agendas");
    let tradition = weight("common/traditions");
    assert_eq!(agenda.reader.id, tradition.reader.id);
    assert_eq!(agenda.reader.family, BlockFamily::Weight);
    for field in [&agenda, &tradition] {
        let GrammarProperty::Known(scopes) = &field.read_scope else {
            panic!("{:?}", field.read_scope);
        };
        assert!(
            matches!(scopes.as_slice(), [ReadScope::Types(types)] if types.len() == 1 && types[0].name == "country")
        );
    }

    // Independent expectations: the config's `modifier_rule` grammar and the prototype's switch
    // table of nineteen spellings. The fixed `factor` key shadows its operation spelling.
    let FieldMembers::WeightBlock(block) = &agenda.members else {
        panic!("{:?}", agenda.members);
    };
    let keys = |block: &WeightBlock| -> Vec<Field> {
        match &block.fixed_keys {
            GrammarProperty::Known(keys) | GrammarProperty::Partial(keys) => keys.clone(),
            GrammarProperty::Unresolved => panic!("fixed keys unresolved"),
        }
    };
    let operations = |block: &WeightBlock| -> BTreeMap<String, Option<ReaderKind>> {
        let GrammarProperty::Known(operations) = &block.operations else {
            panic!("{:?}", block.operations);
        };
        operations
            .iter()
            .map(|operation| {
                let operand = operation.operand.as_ref().map(|reader| reader.kind);
                (operation.key.clone(), operand)
            })
            .collect()
    };
    let kind = |keys: &[Field], name: &str| {
        keys.iter()
            .find(|key| key.name == name)
            .map(|key| key.reader.kind)
    };
    let domain = |keys: &[Field], name: &str| {
        keys.iter()
            .find(|key| key.name == name)
            .map(|key| key.domain.clone())
    };
    let top = keys(block);
    assert_eq!(kind(&top, "base"), Some(ReaderKind::FixedPoint));
    assert_eq!(kind(&top, "days"), Some(ReaderKind::Integer));
    assert_eq!(kind(&top, "modifier"), Some(ReaderKind::Block));
    let simple: BTreeSet<_> = ["round", "floor", "ceiling", "abs", "square", "square_root"]
        .into_iter()
        .collect();
    let spellings: BTreeSet<_> = [
        "weight",
        "set",
        "add",
        "subtract",
        "factor",
        "mult",
        "multiply",
        "divide",
        "modulo",
        "round_to",
        "round",
        "floor",
        "ceiling",
        "max",
        "min",
        "abs",
        "square",
        "pow",
        "square_root",
    ]
    .into_iter()
    .collect();
    let top_operations = operations(block);
    let expected: BTreeSet<_> = spellings
        .iter()
        .copied()
        .filter(|key| *key != "factor")
        .collect();
    assert_eq!(
        top_operations
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        expected
    );
    for (key, operand) in &top_operations {
        let expected = (!simple.contains(key.as_str())).then_some(ReaderKind::ScopedNumeric);
        assert_eq!(*operand, expected, "{key}");
    }
    assert_eq!(block.operation_repeat, RepeatBehavior::Accumulate);
    assert_eq!(block.other_keys, WeightOtherKeys::Rejected);

    let modifier = top.iter().find(|key| key.name == "modifier").unwrap();
    let FieldMembers::WeightBlock(entry) = &modifier.members else {
        panic!("{:?}", modifier.members);
    };
    let entry_operations = operations(entry);
    assert_eq!(
        entry_operations
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        spellings
    );
    assert_eq!(entry.operation_repeat, RepeatBehavior::Replace);
    assert_eq!(
        entry.other_keys,
        WeightOtherKeys::Triggers(GrammarProperty::Known(vec![ReadScope::Enclosing]))
    );

    // Independent expectations from the hand-read scaled and complex member readers.
    assert_eq!(kind(&top, "factor"), Some(ReaderKind::FixedPoint));
    let entry_keys = |name: &str| -> Vec<Field> {
        let key = top.iter().find(|key| key.name == name).unwrap();
        let FieldMembers::WeightBlock(entry) = &key.members else {
            panic!("{:?}", key.members);
        };
        assert_eq!(entry.scalar, GrammarProperty::Known(None), "{name}");
        keys(entry)
    };
    let scaled = entry_keys("scaled_modifier");
    assert_eq!(kind(&scaled, "scope"), Some(ReaderKind::Target));
    assert_eq!(kind(&scaled, "calc"), Some(ReaderKind::Keyword));
    assert_eq!(
        domain(&scaled, "calc"),
        Some(FieldDomain::Listed(vec![
            "planet_distance_empire".into(),
            "planets_in_country".into(),
            "pop_amount".into(),
            "pop_happiness".into(),
        ]))
    );
    let complex = entry_keys("complex_trigger_modifier");
    assert_eq!(kind(&complex, "trigger_scope"), Some(ReaderKind::Target));
    assert_eq!(kind(&complex, "mode"), Some(ReaderKind::Keyword));
    // `mode` reads the operation switch, and `InitPostRead` reports and replaces an operation
    // without an operand.
    assert_eq!(
        domain(&complex, "mode"),
        Some(FieldDomain::Listed(
            spellings
                .difference(&simple)
                .map(|&spelling| spelling.into())
                .collect()
        ))
    );
    let trigger = complex.iter().find(|key| key.name == "trigger").unwrap();
    assert_eq!(trigger.reader.kind, ReaderKind::Reference);
    assert_eq!(
        trigger.reference,
        FieldReference::Lookups(vec![ReferenceLookup {
            condition: FieldCondition::Always,
            target: ReferenceTarget::Triggers,
            stage: LookupStage::WhileReading,
            key_match: KeyMatch::Unresolved,
            empty_key: EmptyKey::LookedUp,
            on_missing: MissingResult::ScriptedTriggerPlaceholder,
        }])
    );
    let nested = top.iter().find(|key| key.name == "modifier").unwrap();
    assert_eq!(
        nested.read_scope,
        GrammarProperty::Known(vec![ReadScope::Enclosing])
    );

    let answer = native.registry_fields("common/council_agendas").unwrap();
    let gaps = |path: &[&str]| -> Vec<(GapKind, String)> {
        answer
            .gaps
            .iter()
            .filter(|gap| {
                let subject = serde_json::to_value(&gap.subject).unwrap();
                subject["path"] == serde_json::json!(path)
            })
            .map(|gap| (gap.kind, gap.detail.clone()))
            .collect()
    };
    let zero_mask = "Read with scope mask 0; the engine checks the scope later, when it \
                     validates the block, which the method does not report.";
    for path in [
        &["ai_weight", "scaled_modifier"][..],
        &["ai_weight", "scaled_modifier", "limit"],
        &["ai_weight", "complex_trigger_modifier"],
        &["ai_weight", "complex_trigger_modifier", "potential"],
    ] {
        assert_eq!(
            gaps(path),
            [(GapKind::OutsideMethod, zero_mask.to_owned())],
            "{path:?}"
        );
    }
    assert_eq!(
        gaps(&["ai_weight", "complex_trigger_modifier", "parameters"]),
        [(
            GapKind::OutsideMethod,
            "The value is read by the trigger that `trigger` names: a built-in trigger's form \
             and grammar are its `command_grammar` answer, and a scripted trigger's are its \
             parameter forms."
                .to_owned()
        )]
    );
    assert_eq!(
        gaps(&["ai_weight", "complex_trigger_modifier", "trigger"]),
        [(
            GapKind::OutsideMethod,
            "The name is matched by its lexer token; how the lexer matches a name is outside the \
             method."
                .to_owned()
        )]
    );
    let field = |name: &str| {
        answer
            .value
            .iter()
            .find(|field| field.name == name)
            .unwrap()
    };
    assert_eq!(field("modifier").read_scope, GrammarProperty::Known(vec![]));
    assert_eq!(field("potential").read_scope, agenda.read_scope);

    // A stored zero mask has no later scope check, so it stays an unresolved path.
    let buildings = native.registry_fields("common/buildings").unwrap();
    assert!(buildings.gaps.iter().any(|gap| {
        gap.kind == GapKind::UnresolvedPath && gap.detail == "read-scope: zero-mask"
    }));
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn defines_match_the_recorded_m452_boundary() {
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn declarations_match_the_recorded_m452_inventory() {
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn declarations_give_the_scopes_of_known_m452_commands() {
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn modifier_declarations_match_the_recorded_m452_boundary() {
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn modifier_families_match_the_recorded_m452_generators() {
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

/// Names checked by hand against the M451-hotfix tradition and tradition category getters and
/// `CTraditionType::PostReadInit` (`docs/native/derived-names.md`).
#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn derived_names_follow_the_tradition_getters_and_keep_swap_conditions() {
    use pdx_native::{DerivedName, FieldCondition, MissingName, NameLookup, NamePart, Support};

    let native = native();
    assert_eq!(native.supports(Operation::DerivedNames), Support::Supported);
    for (registry, _) in DERIVED_NAME_FILES {
        let answer = native.derived_names(registry).unwrap();
        assert_eq!(answer.source.basis, Basis::StaticAnalysis);
        assert_eq!(answer.source.method, DERIVED_NAMES_METHOD);
        assert_eq!(answer.completeness, Completeness::Partial);
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::OutsideMethod)
        );
        for name in &answer.value {
            assert_eq!(name.lookup, NameLookup::Localization, "{name:?}");
            let fixed = name
                .name
                .iter()
                .all(|part| matches!(part, NamePart::Literal(_)));
            assert!(!fixed, "a fixed key is not a derived name: {name:?}");
        }
    }

    let text = |text: &str| NamePart::Literal(text.into());
    let field = |path: &[&str]| NamePart::Field(path.iter().map(|part| part.to_string()).collect());
    let zero = |path: &[&str], zero: bool| FieldCondition::FieldZero {
        path: path.iter().map(|part| part.to_string()).collect(),
        zero,
    };
    let find = |names: &[DerivedName], parts: Vec<NamePart>, stage, on_missing| -> DerivedName {
        names
            .iter()
            .find(|name| name.name == parts && name.stage == stage && name.on_missing == on_missing)
            .unwrap_or_else(|| panic!("{parts:?} {stage:?} {on_missing:?} in {names:#?}"))
            .clone()
    };
    let terms = |name: &DerivedName| match &name.condition {
        FieldCondition::All(terms) => terms.clone(),
        other => panic!("{:?} has condition {other:?}", name.name),
    };

    let traditions = native.derived_names("common/traditions").unwrap().value;
    let base = find(
        &traditions,
        vec![NamePart::ItemKey],
        LookupStage::WhenUsed,
        MissingName::ShowsKey,
    );
    assert_eq!(base.condition, FieldCondition::Always);
    let swap_name = find(
        &traditions,
        vec![field(&["tradition_swap", "name"])],
        LookupStage::WhenUsed,
        MissingName::Unresolved,
    );
    assert!(terms(&swap_name).contains(&FieldCondition::Unresolved));
    assert!(terms(&swap_name).contains(&zero(&["tradition_swap", "inherit_name"], true)));
    for suffix in ["_desc", "_delayed"] {
        let swap_desc = find(
            &traditions,
            vec![field(&["tradition_swap", "name"]), text(suffix)],
            LookupStage::WhenUsed,
            MissingName::Unresolved,
        );
        assert!(terms(&swap_desc).contains(&zero(&["tradition_swap", "name"], false)));
        find(
            &traditions,
            vec![NamePart::ItemKey, text(suffix)],
            LookupStage::WhenUsed,
            MissingName::Silent,
        );
    }
    for path in [
        &["custom_tooltip"][..],
        &["custom_tooltip_with_modifiers"],
        &["tradition_swap", "custom_tooltip"],
        &["tradition_swap", "custom_tooltip_with_modifiers"],
    ] {
        let tooltip = find(
            &traditions,
            vec![field(path)],
            LookupStage::OwnerInitialization,
            MissingName::Unresolved,
        );
        assert!(terms(&tooltip).contains(&zero(path, false)));
    }
    assert!(
        traditions
            .iter()
            .filter(|name| name
                .name
                .iter()
                .any(|part| matches!(part, NamePart::Field(_))))
            .all(|name| name.condition != FieldCondition::Always
                && name.on_missing == MissingName::Unresolved),
        "a field-derived name is never unconditional and has no established miss behavior"
    );

    let categories = native
        .derived_names("common/tradition_categories")
        .unwrap()
        .value;
    let base = find(
        &categories,
        vec![NamePart::ItemKey],
        LookupStage::WhenUsed,
        MissingName::ShowsKey,
    );
    assert_eq!(base.condition, FieldCondition::Always);
    let desc = find(
        &categories,
        vec![NamePart::ItemKey, text("_desc")],
        LookupStage::WhenUsed,
        MissingName::ShowsKey,
    );
    assert_ne!(
        desc.condition,
        FieldCondition::Always,
        "the description block decides"
    );

    assert!(matches!(
        native.derived_names("common/not_a_registry"),
        Err(Error::UnknownRegistry { .. })
    ));
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn modifier_categories_are_the_names_of_the_category_switch() {
    let answer = native().modifier_categories().unwrap();
    assert_declared(&answer);
    assert_eq!(answer.completeness, Completeness::Complete);
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn modifier_category_keys_are_the_parsed_masks_with_an_empty_none() {
    let native = native();
    assert_eq!(
        native.supports(Operation::ModifierCategoryKeys),
        pdx_native::Support::Supported
    );
    let answer = native.modifier_category_keys().unwrap();
    assert_declared(&answer);
    assert_eq!(answer.completeness, Completeness::Complete);
    let key = |name: &str| {
        let key = answer.value.iter().find(|key| key.name == name);
        key.map(|key| key.categories.clone())
    };

    let categories = native.modifier_categories().unwrap();
    let all = categories
        .value
        .iter()
        .find(|category| category.name == "All");

    assert_eq!(answer.value.len(), 24);
    assert_eq!(key("none"), Some(DeclaredTags::Listed(vec![])));
    assert_eq!(key("all").as_ref(), all.map(|all| &all.categories));
    assert_eq!(key("pop_job"), None);
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn script_expansions_give_hosts_stages_and_checks() {
    use pdx_native::{
        CallForm, ExpansionCheck, ExpansionHost, ExpansionMechanism, ExpansionStage,
        GrammarProperty, MissingParameter,
    };
    let native = native();
    assert_eq!(
        native.supports(Operation::ScriptExpansions),
        pdx_native::Support::Supported
    );
    let answer = native.script_expansions().unwrap();
    assert_eq!(answer.source.basis, Basis::StaticAnalysis);
    assert_eq!(
        answer.source.method,
        pdx_native::internals::SCRIPT_EXPANSIONS_METHOD
    );
    let expansion = |mechanism| {
        let found = answer.value.iter().find(|item| item.mechanism == mechanism);
        found.unwrap().clone()
    };

    let inline = expansion(ExpansionMechanism::InlineScript);
    let GrammarProperty::Partial(hosts) = &inline.hosts else {
        panic!("inline hosts: {:?}", inline.hosts);
    };
    let roots = hosts
        .iter()
        .filter(|host| matches!(host, ExpansionHost::RegistryRoot { .. }))
        .count();
    assert_eq!(roots, 164);
    assert!(hosts.contains(&ExpansionHost::RegistryRoot {
        registry: "common/traditions".into()
    }));
    assert!(hosts.contains(&ExpansionHost::ObjectBlock));
    assert_eq!(inline.stage, GrammarProperty::Known(ExpansionStage::Read));
    assert_eq!(
        inline.missing_parameter,
        GrammarProperty::Known(Some(MissingParameter::KeptAsText))
    );

    let trigger = expansion(ExpansionMechanism::ScriptedTrigger);
    assert_eq!(
        trigger.hosts,
        GrammarProperty::Known(vec![
            ExpansionHost::Commands(pdx_native::BlockFamily::Trigger),
            ExpansionHost::TriggerReference,
            ExpansionHost::ScopedOperand,
        ])
    );
    assert_eq!(
        trigger.stage,
        GrammarProperty::Known(ExpansionStage::Compile)
    );
    assert_eq!(
        trigger.checks,
        GrammarProperty::Partial(vec![
            ExpansionCheck::UnknownName,
            ExpansionCheck::DepthLimit
        ])
    );

    let value = expansion(ExpansionMechanism::ScriptValue);
    assert_eq!(
        value.call_forms,
        GrammarProperty::Known(vec![CallForm::Pipe])
    );
    assert_eq!(
        value.missing_parameter,
        GrammarProperty::Known(Some(MissingParameter::Diagnostic))
    );

    let variable = expansion(ExpansionMechanism::ScriptedVariable);
    assert_eq!(variable.stage, GrammarProperty::Known(ExpansionStage::Lex));
    assert_eq!(variable.missing_parameter, GrammarProperty::Known(None));

    let unresolved: Vec<_> = answer
        .gaps
        .iter()
        .filter(|gap| gap.kind != pdx_native::GapKind::OutsideMethod)
        .map(|gap| gap.detail.as_str())
        .collect();
    assert_eq!(unresolved.len(), 2, "{unresolved:?}");
    assert_eq!(answer.completeness, Completeness::Partial);
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn modifier_containers_list_their_categories_or_keep_a_gap() {
    use pdx_native::{AcceptedCategories, GapSubject};
    let native = native();
    let fields = |registry: &str| native.registry_fields(registry).unwrap();
    let accepted = |answer: &Answer<Vec<pdx_native::Field>>, name: &str| {
        let field = answer.value.iter().find(|field| field.name == name);
        field.unwrap().accepted_categories.clone()
    };
    let named = |answer: &Answer<Vec<pdx_native::Field>>, path: &[&str]| {
        answer.gaps.iter().any(|gap| match &gap.subject {
            Some(GapSubject::Field { name }) => [name.as_str()] == path,
            Some(GapSubject::KeyPath { path: gap_path }) => gap_path == path,
            _ => false,
        })
    };
    let categories = native.modifier_categories().unwrap();
    let all = categories
        .value
        .iter()
        .find(|category| category.name == "All")
        .unwrap();

    let traditions = fields("common/traditions");
    let DeclaredTags::Listed(all) = &all.categories else {
        panic!("All has no single categories");
    };
    assert_eq!(
        accepted(&traditions, "modifier"),
        AcceptedCategories::Listed(all.clone())
    );
    let AcceptedCategories::Listed(country) = accepted(&traditions, "triggered_modifier") else {
        panic!("the tradition clause has no categories");
    };
    assert!(country.contains(&"Countries".to_owned()));
    assert!(!country.contains(&"Federations".to_owned()));
    assert!(named(
        &traditions,
        &["tradition_swap", "triggered_modifier"]
    ));

    let pop_categories = fields("common/pop_categories");
    assert_eq!(
        accepted(&pop_categories, "pop_group_modifier"),
        AcceptedCategories::Unresolved
    );
    assert!(named(&pop_categories, &["pop_group_modifier"]));

    let starbase_modules = fields("common/starbase_modules");
    assert!(
        starbase_modules
            .value
            .iter()
            .all(|field| !matches!(field.accepted_categories, AcceptedCategories::Listed(_)))
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn modifier_nodes_give_owners_masks_and_sources_but_no_scopes() {
    let native = native();
    assert_eq!(
        native.supports(Operation::ModifierNodes),
        pdx_native::Support::Supported
    );
    let answer = native.modifier_nodes().unwrap();
    assert_eq!(answer.source.basis, Basis::StaticAnalysis);
    assert_eq!(answer.source.method, MODIFIER_NODES_METHOD);
    assert_eq!(answer.completeness, Completeness::Partial);
    assert_eq!(answer.value.len(), 34);

    let node = |id: usize| &answer.value[id];
    let owners = |id: usize| -> Vec<&str> {
        node(id)
            .owners
            .iter()
            .map(|owner| owner.owner.as_str())
            .collect()
    };
    assert!(node(0).owners.is_empty());
    assert_eq!(owners(14), ["CStarbase", "CMegaStructure"]);
    assert_eq!(owners(17), ["CColony"]);
    assert_eq!(node(17).source_nodes, [16, 31, 7, 32].map(|id| node(id).id));
    let KeptCategories::Recalculated(ship) = &node(31).owners[0].kept_categories else {
        panic!("the ship's calculation sets its mask")
    };
    assert_eq!(ship.len(), 2);
    assert!(ship[0].contains(&"Owned Ships".to_string()) && !ship[0].contains(&"Pops".to_string()));
    assert!(ship[1].contains(&"Pops".to_string()) && ship[1].contains(&"Colony".to_string()));

    assert_eq!(answer.gaps[0].kind, GapKind::OutsideMethod);
    assert_eq!(answer.gaps[0].subject, None);
    assert!(
        answer.gaps[0]
            .detail
            .starts_with("Where a modifier takes effect is decided at application")
    );
    assert!(
        answer
            .gaps
            .iter()
            .any(|gap| gap.kind == GapKind::UnresolvedPath
                && gap.subject.as_ref().map(|subject| subject.name()) == Some("modifier node 0"))
    );
    let categories: Vec<_> = answer
        .gaps
        .iter()
        .filter(|gap| {
            gap.detail
                .starts_with("no resolved modifier node mask keeps")
        })
        .map(|gap| gap.subject.as_ref().unwrap().name())
        .collect();
    assert_eq!(
        categories,
        ["Pop Factions", "AI Economy", "Ship Design Stats"]
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn scope_links_match_the_recorded_m452_boundary() {
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn localization_declarations_match_the_recorded_m452_inventory() {
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

/// Call sites checked by hand in the disassembly, before the expected files were generated.
#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
    let fresh = "this=NotSet root=SelfLink from=[SelfLink] prev=[SelfLink]";

    // CGameState::OnNewGameStarted passes a new scope with nothing set.
    assert_eq!(entries("on_game_start"), [fresh]);
    // CGameState::MonthlyUpdate fires the cached list at database offset 0x28.
    assert_eq!(entries("on_monthly_pulse"), [fresh]);
    // CGameState::YearlyUpdate fires six cached lists with one new scope; an earlier fire leaves
    // it as it found it for the later ones, such as the list at database offset 0x40.
    assert_eq!(entries("on_five_year_pulse"), [fresh]);
    // CFleetCombatManager::OnCombatEnded links a country, a fleet and a fleet behind the losing
    // country; x8 still holds the from scope at CFleet::GetControllerRef, which ignores it.
    assert_eq!(
        entries("on_space_battle_lost"),
        ["this=country root=SelfLink from=[country,fleet,fleet,SelfLink] prev=[SelfLink]"]
    );
    // CLeader::LevelUp links the leader as from of a country scope.
    assert!(
        entries("on_leader_level_up")
            .contains(&"this=country root=SelfLink from=[leader,SelfLink] prev=[SelfLink]".into())
    );
    // CPlanet::SetController links two country scopes as from and fromfrom.
    assert!(entries("on_planet_returned").contains(
        &"this=planet root=SelfLink from=[country,country,SelfLink] prev=[SelfLink]".into()
    ));
    // CSpecialProjectInstance::OnSuccessSpeciesModification links two species as from and
    // fromfrom, and the colony as prev.
    assert_eq!(
        entries("on_modification_complete"),
        ["this=country root=SelfLink from=[species,species,SelfLink] prev=[colony,SelfLink]"]
    );
    // The fleet enters orbit of different objects; each stays its own context.
    let orbit = entries("on_fleet_enter_orbit");
    for from in ["megastructure", "planet", "starbase"] {
        let context = format!("this=fleet root=SelfLink from=[{from},SelfLink] prev=[SelfLink]");
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

/// Registry field-block call sites checked by hand in the disassembly, before the expected files
/// were generated.
#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn registry_field_blocks_supply_the_scopes_that_hand_checked_call_sites_build() {
    let native = native();
    let field = |registry: &str, name: &str| -> (Vec<String>, Vec<String>) {
        let answer = native.registry_fields(registry).unwrap();
        let entries = find(&answer.value, name, |field| &field.name)
            .entry_contexts
            .iter()
            .map(entry)
            .collect();
        let gaps = answer
            .gaps
            .iter()
            .filter(|gap| matches!(&gap.subject, Some(GapSubject::Field { name: subject }) if subject == name))
            .filter(|gap| matches!(gap.kind, GapKind::UnresolvedPath | GapKind::UnreadableInput))
            .map(|gap| gap.detail.clone())
            .collect();
        (entries, gaps)
    };
    let country = "this=country root=SelfLink from=[SelfLink] prev=[SelfLink]";

    // CGovernment::UpdateCouncilAgenda builds a country scope and passes it to
    // CCouncilAgenda::IsPotential and IsAllowed, which evaluate the blocks at this + 0x1c8 and
    // this + 0x280. ExecuteTradition passes its country scope through
    // CTraditionType::GetUnlocksAgenda, whose virtual call on the null tradition swap
    // (TPdxNullObject<CTraditionSwap>::IsValid) reads only its receiver.
    assert_eq!(
        field("common/council_agendas", "potential"),
        (vec![country.to_string()], vec![])
    );
    assert_eq!(
        field("common/council_agendas", "allow"),
        (vec![country.to_string()], vec![])
    );
    // CGovernment::SetCouncilAgenda builds a country scope for
    // CCouncilAgenda::ExecuteInitialEffect, which tail-calls CEffect::Execute on this + 0x580.
    assert_eq!(
        field("common/council_agendas", "init_effect"),
        (vec![country.to_string()], vec![])
    );
    // CArmyType::IsPotentialTrigger links a species scope as from of a colony scope and passes
    // this + 0x4f8 to CAndTrigger::ActualEvaluate directly.
    assert_eq!(
        field("common/armies", "potential"),
        (
            vec!["this=colony root=SelfLink from=[species,SelfLink] prev=[SelfLink]".to_string()],
            vec![]
        )
    );
    // CTraditionType::IsPotential builds its own country scope and evaluates this + 0x108.
    assert_eq!(
        field("common/traditions", "potential"),
        (vec![country.to_string()], vec![])
    );
    // CDiplomaticActionType::OnAccept builds two country scopes, links the second as the
    // first's from, and runs the effect at this + 0x2f8 with the first.
    assert!(
        field("common/diplomatic_actions", "on_accept")
            .0
            .contains(&"this=country root=SelfLink from=[country,SelfLink] prev=[SelfLink]".into())
    );
    // CCountry::AddEdict(CEdict const*) builds a country scope and runs the effect at x1 + 0x2e0
    // through its own vtable slot +0x48.
    assert_eq!(
        field("common/edicts", "effect"),
        (vec![country.to_string()], vec![])
    );
    // CGalacticCommunity::PassResolution loads the resolution type from x1 + 0x18 and runs its
    // effect at +0x3a8 through slot +0x48: a targeted resolution runs it on the target country with
    // the resolution's country as from, another on the resolution's country.
    assert_eq!(
        field("common/resolutions", "effect"),
        (
            vec![
                "this=country root=SelfLink from=[country,SelfLink] prev=[SelfLink]".to_string(),
                country.to_string()
            ],
            vec![]
        )
    );
    // CMission::Start runs the effect at [this + 0x18] + 0x190 through slot +0x48 with the scope
    // that BuildEffectScopeForOperator fills; its from goes through a jump table.
    let (entries, _) = field("common/missions/missions", "on_start");
    assert!(
        entries
            .contains(&"this=country root=SelfLink from=[Unresolved] prev=[SelfLink]".to_string())
    );
    // CMission::Succeed tail-calls the helper CMission::Stop(CRootEffect const&, EMissionStatus)
    // with x1 = [this + 0x18] + 0x388; Stop builds its own scope and runs slot +0x48 of x1.
    let (entries, _) = field("common/missions/missions", "on_success");
    assert!(
        entries
            .contains(&"this=NotSet root=SelfLink from=[Unresolved] prev=[SelfLink]".to_string())
    );
    // CContractManager::IssueContract(CCountry&, CMission&) builds a country scope and runs slot
    // +0x48 of [x2 + 0x18] + 0x40, which a pre-index load (`ldr x8,[x0,#0x40]!`) forms.
    let (entries, _) = field("common/missions/missions", "on_issue");
    assert!(
        entries
            .contains(&"this=country root=SelfLink from=[Unresolved] prev=[SelfLink]".to_string())
    );
    // CSubjectSpecialization::FinishConversion(CSpecialistSubjectType const&, CAgreement const&)
    // builds an agreement scope and runs the effect that the offset getter
    // GetOnProgressCompleteEffect returns at x1 + 0xa0.
    assert_eq!(
        field("common/specialist_subject_types", "on_progress_complete"),
        (
            vec!["this=agreement root=SelfLink from=[SelfLink] prev=[SelfLink]".to_string()],
            vec![]
        )
    );
    // CDecision::GetToolTip calls CCustomTooltipTrigger::BuildToolTip on this + 0x198, and no
    // evaluation names the block.
    assert_eq!(
        field("common/decisions", "custom_tooltip"),
        (
            vec![],
            vec![
                "only tooltip calls name this block; the method established no evaluation of it"
                    .to_string()
            ]
        )
    );
    // Both registries use CTraditionType. OnEnabled and OnDisabled select the swap's or
    // the owner's effect. The owner paths build country scopes; unknown swap identities keep
    // an explicit gap and make the path bound relevant.
    for registry in ["common/traditions", "common/ascension_perks"] {
        for name in ["on_enabled", "on_disabled"] {
            let (entries, gaps) = field(registry, name);
            assert_eq!(entries, [country], "{registry}/{name}");
            assert_eq!(
                gaps,
                [
                    "a call site has more paths than the method follows (path-limit)",
                    "a path to a call site could not be followed (selected-block-identity)",
                ],
                "{registry}/{name}"
            );
        }
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn game_rules_supply_the_scopes_that_hand_checked_call_sites_build() {
    let answer = native().game_rules().unwrap();
    let rule = |name: &str| find(&answer.value, name, |rule| &rule.name);

    // CGameRules::CanColonizePlanet sets the country as root and the planet as this.
    let colonize = rule("can_colonize_planet");
    assert_eq!(colonize.kind, RuleKind::Scripted);
    assert_eq!(
        colonize.entries.iter().map(entry).collect::<Vec<_>>(),
        ["this=planet root=country from=[SelfLink] prev=[SelfLink]"]
    );
    // CGameRules::CanAddClaim sets the claiming country as root and the system as this.
    assert_eq!(
        rule("can_add_claim")
            .entries
            .iter()
            .map(entry)
            .collect::<Vec<_>>(),
        ["this=galactic_object root=country from=[SelfLink] prev=[SelfLink]"]
    );
    // CGameRules::CanOrbitalBombard links the planet as from of the fleet.
    assert!(rule("can_orbital_bombard").entries.iter().map(entry).any(
        |context| context == "this=fleet root=SelfLink from=[planet,SelfLink] prev=[SelfLink]"
    ));
    // A weighted rule lives in its own array of the rule set.
    let election = rule("leader_election_weight");
    assert_eq!(election.kind, RuleKind::Weighted);
    assert_eq!(
        election.entries.iter().map(entry).collect::<Vec<_>>(),
        ["this=leader root=SelfLink from=[SelfLink] prev=[SelfLink]"]
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn callbacks_match_the_recorded_m452_inventory() {
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
            .chain(&context.prev)
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
    let path = format!("{}/tests/expected/m452/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn registries_are_named_by_their_content_directory() {
    let answer = native().registries().unwrap();
    assert_eq!(answer.value.len(), 164, "M452 registry discovery changed");
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn the_developer_run_gives_the_public_registry_field_answer() {
    let native = native();
    for registry in ["common/traditions", "common/megastructures"] {
        let run = registry_field_stops::run(&native, registry).unwrap();
        assert_eq!(run.answer, native.registry_fields(registry).unwrap());
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn traced_questions_match_untraced_questions() {
    let untraced = observe(&native());
    // Each `Native` caches its analysis, so the traced questions need their own.
    let traced_native = native();
    let traced = trace_causes(|| observe(&traced_native));

    assert_eq!(traced.answers, untraced.answers);
    assert_eq!(traced.fields, untraced.fields);
    assert_eq!(traced.grammars, untraced.grammars);
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
/// four registries and 22 commands.
struct Observed {
    answers: Vec<Value>,
    fields: Vec<(Value, Vec<TokenPath>, Vec<FieldGap>)>,
    grammars: Vec<(Value, GrammarOutcome)>,
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
    answers.push(json!(native.modifier_nodes()));
    answers.push(json!(native.modifier_category_keys()));
    answers.push(json!(native.script_expansions()));
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

    Observed {
        answers,
        fields,
        grammars,
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
            let families_covered = kind == DeclarationKind::Trigger
                || ["hidden_effect", "every_owned_planet"].contains(&name);
            assert_eq!(
                answer.completeness,
                if kind == DeclarationKind::Trigger || name == "hidden_effect" {
                    Completeness::Complete
                } else {
                    Completeness::Partial
                }
            );
            if name == "every_owned_planet" {
                assert!(
                    answer
                        .gaps
                        .iter()
                        .any(|gap| gap.detail == "read-scope: stored-scope")
                );
                assert_eq!(
                    answer.value.child_scopes,
                    GrammarProperty::Partial(vec![pdx_native::ChildScope {
                        family,
                        scope: GrammarProperty::Unresolved,
                    }])
                );
            }
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
                if families_covered {
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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

/// The entry context gap of council agenda `ai_weight`, the one typed gap that the Milestone 4
/// council agenda test accepts. The engine evaluates that block on an array element in a
/// template helper, not in an owner method, and no Atlas `replace_scopes` claim needs its
/// contexts. Jackson, 2026-10-08: the gate is for Atlas's sake; it should not go red for
/// something Atlas does not need.
fn is_accepted_ai_weight_gap(gap: &pdx_native::Gap) -> bool {
    gap.kind == GapKind::UnresolvedPath
        && gap.subject
            == Some(GapSubject::Field {
                name: "ai_weight".into(),
            })
        && gap.detail == "no evaluation that the method attributes evaluates this block"
}

/// The range gap of a council agenda weight key, the one typed numeric gap that the Milestone 4
/// council agenda test accepts. `days`, `months` and `years` read `CToken::GetInt()`, and one
/// `factor` alternative reads `CToken::GetFloat()`. Their storage is established, but no live
/// fixture reaches a weight key, so the faithful-storage range stays unresolved (SDK-739). No
/// Atlas claim needs it: the config's `modifier_rule` gives these keys no range. Jackson's rule
/// of 2026-10-08 applies: the gate should not go red for something Atlas does not need.
fn is_accepted_weight_range_gap(gap: &pdx_native::Gap, fields: &[pdx_native::Field]) -> bool {
    use pdx_native::{FieldMembers, FieldReadOutcome, GrammarProperty};
    let Some(GapSubject::KeyPath { path }) = &gap.subject else {
        return false;
    };
    let [block, key] = path.as_slice() else {
        return false;
    };
    let Some(FieldMembers::WeightBlock(weight)) = fields
        .iter()
        .find(|field| &field.name == block)
        .map(|field| &field.members)
    else {
        return false;
    };
    let GrammarProperty::Known(keys) = &weight.fixed_keys else {
        return false;
    };
    let Some(key) = keys.iter().find(|candidate| &candidate.name == key) else {
        return false;
    };
    let storage_known = |numeric: &GrammarProperty<Option<pdx_native::NumericConversion>>| {
        matches!(numeric, GrammarProperty::Partial(Some(conversion))
            if matches!(conversion.representation, GrammarProperty::Known(_))
                && matches!(conversion.width_bits, GrammarProperty::Known(_))
                && matches!(conversion.signedness, GrammarProperty::Known(_))
                && matches!(conversion.scale, GrammarProperty::Known(_)))
    };

    gap.kind == GapKind::NumericConversion
        && block == "ai_weight"
        && ["days", "months", "years", "factor"].contains(&key.name.as_str())
        && !key.read.is_empty()
        && key.read.iter().all(|alternative| {
            matches!(&alternative.outcome, FieldReadOutcome::Read { reader, .. }
                if storage_known(&reader.numeric))
        })
}

#[test]
fn the_council_agenda_gate_accepts_only_the_ai_weight_entry_gap() {
    use pdx_native::Gap;
    const ENTRY_GAP: &str = "no evaluation that the method attributes evaluates this block";
    let gap = |kind, field: &str, detail: &str| Gap {
        kind,
        subject: Some(GapSubject::Field { name: field.into() }),
        detail: detail.into(),
    };

    assert!(is_accepted_ai_weight_gap(&gap(
        GapKind::UnresolvedPath,
        "ai_weight",
        ENTRY_GAP
    )));
    assert!(!is_accepted_ai_weight_gap(&gap(
        GapKind::NumericConversion,
        "agenda_cost",
        "Numeric conversion is incomplete: a storage property, the accepted range or an engine conversion path is not established."
    )));
    assert!(!is_accepted_ai_weight_gap(&gap(
        GapKind::UnresolvedPath,
        "ai_weight",
        "the block's storage is not established, so no evaluation joins it"
    )));
    assert!(!is_accepted_ai_weight_gap(&gap(
        GapKind::UnresolvedPath,
        "potential",
        ENTRY_GAP
    )));
}

/// The Milestone 4 council agenda acceptance test (SDK-600; `docs/roadmap.md`, "Milestone 4
/// acceptance"). Every required fact is checked on the answer's value, so a missing field, an
/// unresolved context or an `OutsideMethod` exclusion cannot satisfy it; the answer must also
/// have no typed gap other than [`is_accepted_ai_weight_gap`] and [`is_accepted_weight_range_gap`]. The test reports every missing
/// fact at once.
#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn council_agenda_fields_are_complete_with_every_milestone_4_fact() {
    use pdx_native::{
        BlockFamily, EntryContext, EntryScope, Field, FieldCondition, FieldMembers,
        FieldReadOutcome, FieldReference, GrammarProperty, NumericRepresentation,
        NumericSignedness, ReadScope, ReferenceTarget, ScopeReference, ScopedOperandForm,
        ScopedReferenceKind, WeightOtherKeys,
    };

    const FIELDS: [&str; 10] = [
        "agenda_cost",
        "agenda_cooldown",
        "agenda_finish_modifier_duration",
        "potential",
        "allow",
        "effect",
        "init_effect",
        "finish_modifier",
        "modifier",
        "ai_weight",
    ];

    let native = native();
    let answer = native.registry_fields("common/council_agendas").unwrap();
    let country = native
        .scopes()
        .unwrap()
        .value
        .types
        .into_iter()
        .find(|scope| scope.name == "country")
        .map(|scope| ScopeReference {
            id: scope.id,
            name: scope.name,
        })
        .expect("the engine declares the country scope type");
    let mut missing = Vec::new();
    let mut require = |holds: bool, fact: String| {
        if !holds {
            missing.push(fact);
        }
    };

    require(
        answer.source.basis == Basis::StaticAnalysis,
        format!("basis is static analysis, not {:?}", answer.source.basis),
    );
    let typed_gaps: Vec<_> = answer
        .gaps
        .iter()
        .filter(|gap| {
            gap.kind != GapKind::OutsideMethod
                && !is_accepted_ai_weight_gap(gap)
                && !is_accepted_weight_range_gap(gap, &answer.value)
        })
        .map(|gap| format!("  {:?} {:?}: {}", gap.kind, gap.subject, gap.detail))
        .collect();
    require(
        typed_gaps.is_empty(),
        format!(
            "the answer is complete apart from the ai_weight entry gap; typed gaps:\n{}",
            typed_gaps.join("\n")
        ),
    );
    let mut names: Vec<_> = answer
        .value
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    names.sort_unstable();
    let mut expected = FIELDS;
    expected.sort_unstable();
    require(names == expected, format!("the ten fields are {names:?}"));

    let field =
        |name: &str| -> Option<&Field> { answer.value.iter().find(|field| field.name == name) };
    for name in FIELDS {
        let Some(field) = field(name) else { continue };
        require(
            field.reader.id.is_some() && field.reader.kind != ReaderKind::Unknown,
            format!(
                "{name} has an established reader kind, not {:?}",
                field.reader.kind
            ),
        );
    }

    // SDK-544: agenda_cost's numeric storage kind, scale, and script-value acceptance.
    if let Some(cost) = field("agenda_cost") {
        let numeric = match &cost.reader.numeric {
            GrammarProperty::Known(Some(numeric)) | GrammarProperty::Partial(Some(numeric)) => {
                Some(numeric)
            }
            _ => None,
        };
        require(
            numeric.is_some_and(|numeric| {
                numeric.representation == GrammarProperty::Known(NumericRepresentation::Integer)
                    && numeric.width_bits == GrammarProperty::Known(32)
                    && numeric.signedness == GrammarProperty::Known(NumericSignedness::Signed)
                    && numeric.scale == GrammarProperty::Known(Some(1))
            }),
            format!(
                "agenda_cost is stored as a signed 32-bit integer with scale 1: {:?}",
                cost.reader.numeric
            ),
        );
        let forms = match &cost.reader.scoped_operand {
            GrammarProperty::Known(Some(operand)) | GrammarProperty::Partial(Some(operand)) => {
                match &operand.forms {
                    GrammarProperty::Known(forms) | GrammarProperty::Partial(forms) => {
                        forms.as_slice()
                    }
                    GrammarProperty::Unresolved => &[],
                }
            }
            _ => &[],
        };
        require(
            forms.iter().any(|form| {
                matches!(
                    form,
                    ScopedOperandForm::Prefixed {
                        kind: ScopedReferenceKind::ScriptValue,
                        ..
                    }
                )
            }),
            format!("agenda_cost accepts a script value: {forms:?}"),
        );
    }

    // SDK-541: the normalized read condition of each duration field.
    for name in ["agenda_cooldown", "agenda_finish_modifier_duration"] {
        let Some(duration) = field(name) else {
            continue;
        };
        require(
            duration.read.len() == 1
                && duration.read[0].condition == FieldCondition::Always
                && matches!(&duration.read[0].outcome, FieldReadOutcome::Read { reader, .. } if reader.kind != ReaderKind::Unknown),
            format!(
                "{name} has one normalized read condition: {:?}",
                duration.read
            ),
        );
    }

    // SDK-542, SDK-549 and SDK-677: the family, read scope and entry contexts of each block.
    // Scope references join by id, so the country scope is the one that `scopes` declares.
    let country_context = EntryContext {
        this: EntryScope::Scope(country.clone()),
        root: EntryScope::SelfLink,
        from: vec![EntryScope::SelfLink],
        prev: vec![EntryScope::SelfLink],
    };
    for (name, family) in [
        ("potential", BlockFamily::Trigger),
        ("allow", BlockFamily::Trigger),
        ("effect", BlockFamily::Effect),
        ("init_effect", BlockFamily::Effect),
    ] {
        let Some(block) = field(name) else { continue };
        require(
            block.reader.family == family,
            format!(
                "{name} accepts {family:?} commands, not {:?}",
                block.reader.family
            ),
        );
        require(
            block.read_scope
                == GrammarProperty::Known(vec![ReadScope::Types(vec![country.clone()])]),
            format!("{name} is read in a country scope: {:?}", block.read_scope),
        );
        let contexts: Vec<_> = block.entry_contexts.iter().map(entry).collect();
        let resolved = |scope: &EntryScope| !matches!(scope, EntryScope::Unresolved);
        require(
            !block.entry_contexts.is_empty()
                && block.entry_contexts.iter().all(|context| {
                    resolved(&context.this)
                        && resolved(&context.root)
                        && context.from.iter().all(resolved)
                        && context.prev.iter().all(resolved)
                }),
            format!("{name} has only resolved entry contexts: {contexts:?}"),
        );
        require(
            block.entry_contexts.contains(&country_context),
            format!("{name} is entered as a country with self-linked links: {contexts:?}"),
        );
    }

    // SDK-543: the content directory that finish_modifier's value is looked up in.
    if let Some(reference) = field("finish_modifier") {
        let lookups = match &reference.reference {
            FieldReference::Lookups(lookups) => lookups.as_slice(),
            _ => &[],
        };
        require(
            !lookups.is_empty()
                && lookups.iter().all(|lookup| {
                    matches!(&lookup.target, ReferenceTarget::Registry { name } if name == "common/static_modifiers")
                }),
            format!("finish_modifier is looked up in common/static_modifiers: {:?}", reference.reference),
        );
    }

    // SDK-542 and SDK-607: the member family of modifier.
    if let Some(modifier) = field("modifier") {
        require(
            modifier.reader.family == BlockFamily::Modifier
                && matches!(modifier.members, FieldMembers::ModifierBlock(_)),
            format!(
                "modifier accepts modifier entries: {:?}",
                modifier.reader.family
            ),
        );
    }

    // SDK-545 and SDK-705: ai_weight's keys, each key's reader kind, and the nested modifier entries.
    if let Some(weight) = field("ai_weight") {
        require(
            weight.reader.family == BlockFamily::Weight,
            format!(
                "ai_weight accepts weight entries: {:?}",
                weight.reader.family
            ),
        );
        if let FieldMembers::WeightBlock(block) = &weight.members {
            // Independent expectations: the config's `modifier_rule` grammar and the engine's
            // switch of nineteen spellings, where the fixed `factor` key shadows its operation.
            let keys = match &block.fixed_keys {
                GrammarProperty::Known(keys) => keys.as_slice(),
                _ => &[],
            };
            let mut inventory: Vec<_> = keys
                .iter()
                .map(|key| (key.name.as_str(), key.reader.kind))
                .collect();
            inventory.sort_unstable();
            require(
                inventory
                    == [
                        ("base", ReaderKind::FixedPoint),
                        ("complex_trigger_modifier", ReaderKind::Block),
                        ("days", ReaderKind::Integer),
                        ("factor", ReaderKind::FixedPoint),
                        ("modifier", ReaderKind::Block),
                        ("months", ReaderKind::Integer),
                        ("scaled_modifier", ReaderKind::Block),
                        ("years", ReaderKind::Integer),
                    ],
                format!("ai_weight has the eight keys with their reader kinds: {inventory:?}"),
            );
            let operations = match &block.operations {
                GrammarProperty::Known(operations) => operations.as_slice(),
                _ => &[],
            };
            let mut spellings: Vec<_> = operations
                .iter()
                .map(|operation| operation.key.as_str())
                .collect();
            spellings.sort_unstable();
            require(
                spellings
                    == [
                        "abs",
                        "add",
                        "ceiling",
                        "divide",
                        "floor",
                        "max",
                        "min",
                        "modulo",
                        "mult",
                        "multiply",
                        "pow",
                        "round",
                        "round_to",
                        "set",
                        "square",
                        "square_root",
                        "subtract",
                        "weight",
                    ],
                format!("ai_weight has the eighteen operation keys: {spellings:?}"),
            );
            require(
                operations.iter().all(|operation| {
                    operation
                        .operand
                        .as_ref()
                        .is_none_or(|operand| operand.kind != ReaderKind::Unknown)
                }),
                format!(
                    "every ai_weight operation has an operand kind: {:?}",
                    block.operations
                ),
            );
            require(
                !matches!(block.other_keys, WeightOtherKeys::Unresolved),
                "ai_weight's other keys have a known disposition".into(),
            );
            let nested = keys.iter().find(|key| key.name == "modifier");
            require(
                nested.is_some_and(|entry| {
                    entry.reader.kind == ReaderKind::Block
                        && matches!(&entry.members, FieldMembers::WeightBlock(nested)
                            if matches!(nested.fixed_keys, GrammarProperty::Known(_))
                                && matches!(nested.operations, GrammarProperty::Known(_)))
                }),
                format!(
                    "ai_weight nests modifier entries with their own keys: {:?}",
                    nested.map(|entry| &entry.members)
                ),
            );
        } else {
            require(
                false,
                format!("ai_weight carries its weight block: {:?}", weight.members),
            );
        }
    }

    assert!(
        missing.is_empty(),
        "missing Milestone 4 facts:\n{}",
        missing.join("\n")
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn every_tracked_candidate_matches_the_reviewed_tree() {
    let recordings = tempfile::tempdir().unwrap();
    let native = native().record_answers_to(recordings.path());
    for name in parity::FILES {
        let tracked = std::fs::read(parity::expected_directory().join(name)).unwrap();
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
        let candidate = parity::candidate(&changed, name).unwrap();
        let report =
            comparison::compare_static(&changed.build(), name, expected.as_bytes(), &candidate);
        assert!(report.passes(), "{}", report.render_and_save().unwrap());
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn command_forms_keep_m452_acceptance_and_named_stage_gaps() {
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn sdk492_fixed_key_grammars_match_the_engine() {
    assert_sdk492_keys(&native());
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build; also audited by command-population"]
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
        let key_gaps: Vec<_> = answer
            .gaps
            .iter()
            .filter(|gap| gap.subject == Some(pdx_native::GapSubject::Field { name: key.into() }))
            .collect();
        assert!(
            key_gaps.iter().any(|gap| gap.kind == GapKind::OutsideMethod
                && gap
                    .detail
                    .starts_with("The platform scanner's own conversion")),
            "{command}/{key}: {key_gaps:?}"
        );
        assert!(
            key_gaps
                .iter()
                .all(|gap| gap.kind != GapKind::NumericConversion),
            "{command}/{key}: {key_gaps:?}"
        );
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn read_scopes_match_the_engine_and_link_output_ids() {
    use pdx_native::GrammarProperty;
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("expected/m452/read-scopes.json")).unwrap();
    let actual = parity::read_scopes(&native, &expected).unwrap();
    assert_eq!(actual, expected);
    // Independent config expectations at cwtools-stellaris-config 85747602a614ad7daa8cc66453777ecb023463a8:
    // common/traditions.cwt replace_scopes.this; triggers.cwt push_scope for any_owned_army
    // and count_owned_army.limit. effects.cwt if/else/else_if keep the surrounding scope.
    let named_scope = |property: &serde_json::Value, name: &str| {
        let alternatives = property["Known"].as_array().expect("known read scope");
        assert_eq!(alternatives.len(), 1);
        let types = alternatives[0]["Types"]
            .as_array()
            .expect("named scope set");
        assert_eq!(types.len(), 1);
        assert_eq!(types[0]["name"], name);
    };
    for field in ["possible", "on_enabled"] {
        named_scope(
            &actual["registries"]["common/traditions"]["fields"][field],
            "country",
        );
    }
    named_scope(
        &actual["commands"]["trigger/any_owned_army"]["children"]["Known"][0]["scope"],
        "army",
    );
    named_scope(
        &actual["commands"]["trigger/count_owned_army"]["fields"]["limit"],
        "army",
    );
    // The config says planet; the member's explicit mask on this build names colony.
    named_scope(
        &actual["commands"]["trigger/any_owned_planet"]["children"]["Known"][0]["scope"],
        "colony",
    );
    let links = native.scope_links().unwrap();
    let army = native
        .command_grammar(DeclarationKind::Trigger, "any_owned_army")
        .unwrap();
    let GrammarProperty::Known(children) = army.value.child_scopes else {
        panic!("army child scopes")
    };
    let GrammarProperty::Known(scopes) = &children[0].scope else {
        panic!("army read scope")
    };
    let pdx_native::ReadScope::Types(types) = &scopes[0] else {
        panic!("army scope types")
    };
    assert!(links.value.iter().any(|link| matches!(&link.output_scope, pdx_native::OutputScope::Listed(output) if output == types)));
    for (kind, name) in [
        (DeclarationKind::Effect, "if"),
        (DeclarationKind::Effect, "else"),
        (DeclarationKind::Effect, "else_if"),
        (DeclarationKind::Effect, "hidden_effect"),
        (DeclarationKind::Trigger, "and"),
        (DeclarationKind::Trigger, "or"),
        (DeclarationKind::Trigger, "not"),
    ] {
        let answer = native.command_grammar(kind, name).unwrap();
        let GrammarProperty::Known(children) = answer.value.child_scopes else {
            panic!("{name}: child scope")
        };
        assert_eq!(
            children[0].scope,
            GrammarProperty::Known(vec![pdx_native::ReadScope::Enclosing])
        );
    }
}
