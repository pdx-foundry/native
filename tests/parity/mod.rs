//! Questions and selections for the tracked static parity files.
mod compact;
pub mod comparison;
mod layout;

/// The accepted categories of each modifier container field, and of each modifier key of a
/// triggered modifier clause as `field.key`.
fn container_categories(fields: &[Field]) -> BTreeMap<String, AcceptedCategories> {
    let mut containers = BTreeMap::new();
    for field in fields.iter().filter(|field| {
        matches!(
            field.reader.family,
            BlockFamily::Modifier | BlockFamily::TriggeredModifier
        )
    }) {
        containers.insert(field.name.clone(), field.accepted_categories.clone());
        let FieldMembers::TriggeredModifier(block) = &field.members else {
            continue;
        };
        let (GrammarProperty::Known(keys) | GrammarProperty::Partial(keys)) = &block.fixed_keys
        else {
            continue;
        };
        for key in keys
            .iter()
            .filter(|key| key.reader.family == BlockFamily::Modifier)
        {
            let name = format!("{}.{}", field.name, key.name);
            containers.insert(name, key.accepted_categories.clone());
        }
    }
    containers
}

/// Compact read-time scope answers; the tracked file supplies only sample identities.
pub fn read_scopes(native: &Native, expected: &Value) -> Result<Value> {
    let mut registries = BTreeMap::new();
    for registry in expected["registries"]
        .as_object()
        .ok_or("expected registry selection")?
        .keys()
    {
        let answer = current_answer(native, native.registry_fields(registry)?)?;
        let fields: BTreeMap<_, _> = answer
            .value
            .into_iter()
            .filter(|field| matches!(field.reader.kind, ReaderKind::Block | ReaderKind::Unknown))
            .map(|field| (field.name, field.read_scope))
            .collect();
        registries.insert(registry, json!({"fields": fields, "gaps": answer.gaps.into_iter().filter(|gap| gap.detail.starts_with("read-scope:")).collect::<Vec<_>>()}));
    }
    let mut commands = BTreeMap::new();
    for subject in expected["commands"]
        .as_object()
        .ok_or("expected command selection")?
        .keys()
    {
        let (kind, name) = command(subject).ok_or("invalid command selection")?;
        let answer = current_answer(native, native.command_grammar(kind, name)?)?;
        let fields = match &answer.value.fixed_keys {
            GrammarProperty::Known(fields) | GrammarProperty::Partial(fields) => fields
                .iter()
                .map(|field| (field.name.clone(), field.read_scope.clone()))
                .collect::<BTreeMap<_, _>>(),
            _ => BTreeMap::new(),
        };
        commands.insert(subject, json!({"children": answer.value.child_scopes, "fields": fields,
            "gaps": answer.gaps.into_iter().filter(|gap| gap.detail.starts_with("read-scope:")).collect::<Vec<_>>()}));
    }
    Ok(json!({"build": native.build(), "registries": registries, "commands": commands}))
}

#[cfg(test)]
mod comparison_tests;

pub use compact::*;
use pdx_native::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub const FIELD_FILES: [(&str, &str); 4] = [
    ("common/traditions", "fields-traditions.json"),
    (
        "common/tradition_categories",
        "fields-tradition_categories.json",
    ),
    ("common/council_agendas", "fields-council_agendas.json"),
    ("common/megastructures", "fields-megastructures.json"),
];

pub const DERIVED_NAME_FILES: [(&str, &str); 2] = [
    ("common/traditions", "derived-names-traditions.json"),
    (
        "common/tradition_categories",
        "derived-names-tradition_categories.json",
    ),
];

pub const FILES: &[&str] = &[
    "registries.json",
    "read-scopes.json",
    "fields-traditions.json",
    "fields-tradition_categories.json",
    "fields-council_agendas.json",
    "fields-megastructures.json",
    "references.json",
    "command-grammars.json",
    "dynamic-names.json",
    "declarations-effect.json",
    "declarations-trigger.json",
    "declaration-gaps.json",
    "declaration-recovered.json",
    "declaration-samples-effect.json",
    "declaration-samples-trigger.json",
    "defines.json",
    "modifier-declarations.json",
    "modifier-categories.json",
    "modifier-families.json",
    "modifier-nodes.json",
    "modifier-category-keys.json",
    "script-expansions.json",
    "modifier-containers.json",
    "derived-names-traditions.json",
    "derived-names-tradition_categories.json",
    "modifier-blocks.json",
    "weight-blocks.json",
    "triggered-modifiers.json",
    "on-actions.json",
    "game-rules.json",
    "localization-declarations.json",
    "scope-inventory.json",
    "scope-links.json",
];

pub fn expected_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/expected/m451")
}

/// Ask each static selection. Formatting uses the tracked key order.
pub fn candidate(native: &Native, name: &str) -> Result<Vec<u8>> {
    let template = std::fs::read(expected_directory().join(name))?;
    let expected: Value = serde_json::from_slice(&template)?;
    let value = question(native, name, &expected)?;
    layout::render(name, &value, &template)
}

fn current_answer<T>(native: &Native, answer: Answer<T>) -> Result<Answer<T>> {
    if answer.source.build != native.build() {
        return Err("candidate source does not match Native::build".into());
    }
    Ok(answer)
}

/// Produce the current answer's selected view, without accepting it as correct.
pub fn question(native: &Native, name: &str, expected: &Value) -> Result<Value> {
    if let Some((registry, _)) = FIELD_FILES.iter().find(|(_, file)| *file == name) {
        return Ok(compact_fields(
            &current_answer(native, native.registry_fields(registry)?)?.value,
        ));
    }
    if let Some((registry, _)) = DERIVED_NAME_FILES.iter().find(|(_, file)| *file == name) {
        return Ok(compact_derived_names(&current_answer(
            native,
            native.derived_names(registry)?,
        )?));
    }
    match name {
        "registries.json" => Ok(json!(
            current_answer(native, native.registries()?)?
                .value
                .iter()
                .map(|item| &item.name)
                .collect::<Vec<_>>()
        )),
        "read-scopes.json" => read_scopes(native, expected),
        "references.json" => references(native, expected),
        "command-grammars.json" => {
            assert_sdk492_keys(native);
            let mut answers = BTreeMap::new();
            for subject in expected
                .as_object()
                .ok_or("expected command selection")?
                .keys()
            {
                let (kind, name) = command(subject).ok_or("invalid command selection")?;
                let mut answer = current_answer(native, native.command_grammar(kind, name)?)?;
                // Recorded answers describe the same static facts with a different Basis.
                answer.source.basis = Basis::StaticAnalysis;
                answers.insert(subject, answer);
            }
            Ok(json!(answers))
        }
        "dynamic-names.json" => {
            let answer = current_answer(native, native.dynamic_names()?)?;
            Ok(dynamic_names(&answer, expected))
        }
        "declarations-effect.json" | "declarations-trigger.json" => {
            let kind = declaration_kind(name);
            Ok(json!(
                declaration_inventory(&current_answer(native, native.declarations(kind)?)?).0
            ))
        }
        "declaration-gaps.json" => {
            let mut gaps = BTreeMap::new();
            for (subject, kind) in DECLARATIONS {
                let answer = current_answer(native, native.declarations(kind)?)?;
                gaps.insert(subject, declaration_inventory(&answer).1);
            }
            Ok(json!(gaps))
        }
        "declaration-recovered.json" => {
            let mut recovered = expected.clone();
            for (subject, kind) in DECLARATIONS {
                let answer = current_answer(native, native.declarations(kind)?)?;
                recovered[subject] = recovered_declarations(&answer.value, &expected[subject])?;
            }
            Ok(recovered)
        }
        "declaration-samples-effect.json" | "declaration-samples-trigger.json" => {
            let answer = current_answer(native, native.declarations(declaration_kind(name))?)?;
            select_samples(&json!(answer.value), expected, &["name"])
        }
        "defines.json" => {
            let answer = current_answer(native, native.defines()?)?;
            compact_defines(&answer, expected)
        }
        "modifier-declarations.json" => {
            let answer = current_answer(native, native.modifiers()?)?;
            Ok(json!({
                "count": answer.value.len(), "gaps": gap_counts(&answer),
                "samples": select_samples(&json!(answer.value), &expected["samples"], &["name"])?
            }))
        }
        "modifier-categories.json" => Ok(json!(
            current_answer(native, native.modifier_categories()?)?
                .value
                .iter()
                .map(|item| (&item.name, &item.categories))
                .collect::<BTreeMap<_, _>>()
        )),
        "modifier-category-keys.json" => {
            let answer = current_answer(native, native.modifier_category_keys()?)?;
            let keys: BTreeMap<_, _> = answer
                .value
                .iter()
                .map(|key| (&key.name, &key.categories))
                .collect();
            Ok(json!({
                "completeness": answer.completeness, "gaps": answer.gaps, "keys": keys
            }))
        }
        "script-expansions.json" => {
            let answer = current_answer(native, native.script_expansions()?)?;
            Ok(json!({
                "completeness": answer.completeness, "gaps": answer.gaps, "expansions": answer.value
            }))
        }
        "modifier-nodes.json" => {
            let answer = current_answer(native, native.modifier_nodes()?)?;
            Ok(json!({
                "completeness": answer.completeness, "gaps": answer.gaps, "nodes": answer.value
            }))
        }
        "modifier-containers.json" => {
            let mut registries = BTreeMap::new();
            for registry in expected
                .as_object()
                .ok_or("expected container registry selection")?
                .keys()
            {
                let answer = current_answer(native, native.registry_fields(registry)?)?;
                registries.insert(registry, container_categories(&answer.value));
            }
            Ok(json!(registries))
        }
        "modifier-blocks.json" => modifier_blocks(native),
        "weight-blocks.json" => weight_blocks(native),
        "triggered-modifiers.json" => triggered_modifiers(native),
        "modifier-families.json" => {
            let mut families = BTreeMap::new();
            for registry in expected
                .as_object()
                .ok_or("expected modifier registry selection")?
                .keys()
            {
                families.insert(
                    registry,
                    compact_families(&current_answer(
                        native,
                        native.modifier_families(registry)?,
                    )?),
                );
            }
            Ok(json!(families))
        }
        "on-actions.json" => Ok(compact_on_actions(&current_answer(
            native,
            native.on_actions()?,
        )?)),
        "game-rules.json" => Ok(compact_game_rules(&current_answer(
            native,
            native.game_rules()?,
        )?)),
        "localization-declarations.json" => Ok(compact_localization(&current_answer(
            native,
            native.localization_declarations()?,
        )?)),
        "scope-inventory.json" => Ok(json!(current_answer(native, native.scopes()?)?.value)),
        "scope-links.json" => {
            let answer = current_answer(native, native.scope_links()?)?;
            Ok(json!({
                "names": answer.value.iter().map(|item| &item.name).collect::<Vec<_>>(),
                "samples": select_samples(&json!(answer.value), &expected["samples"], &["name"])?
            }))
        }
        _ => Err(format!("no parity question for {name}").into()),
    }
}

const DECLARATIONS: [(&str, DeclarationKind); 2] = [
    ("effect", DeclarationKind::Effect),
    ("trigger", DeclarationKind::Trigger),
];

fn declaration_kind(name: &str) -> DeclarationKind {
    if name.ends_with("-effect.json") {
        DeclarationKind::Effect
    } else {
        DeclarationKind::Trigger
    }
}

fn declaration_inventory(answer: &Answer<Vec<Declaration>>) -> (Vec<String>, Value) {
    let names: BTreeSet<_> = answer.value.iter().map(|item| item.name.clone()).collect();
    let omitted: BTreeSet<_> = answer
        .gaps
        .iter()
        .filter(|gap| gap.kind == GapKind::UnresolvedPath)
        .filter_map(|gap| {
            gap.subject
                .as_ref()
                .map(|subject| subject.name().to_owned())
        })
        .filter(|name| !names.contains(name))
        .collect();
    let accounted = names.union(&omitted).cloned().collect();
    let gaps = json!({
        "unreadable": omitted,
        "unnamed_registrations": answer.gaps.iter().filter(|gap| gap.kind == GapKind::UnnamedDeclaration).count(),
        "unresolved_scopes": answer.value.iter().filter(|item| item.scopes == DeclaredScopes::Unresolved).map(|item| &item.name).collect::<Vec<_>>()
    });
    (accounted, gaps)
}

fn recovered_declarations(answer: &[Declaration], expected: &Value) -> Result<Value> {
    let mut recovered = expected.clone();
    recovered["sdk_488_live_inventory"] = json!(answer.len());
    for sample in recovered["recovered"]
        .as_array_mut()
        .ok_or("missing recovered samples")?
    {
        let name = sample["declaration"]["name"]
            .as_str()
            .ok_or("missing declaration name")?;
        sample["declaration"] = json!(answer.iter().find(|item| item.name == name));
    }
    Ok(recovered)
}

fn compact_defines(answer: &Answer<Vec<Define>>, expected: &Value) -> Result<Value> {
    let mut types = BTreeMap::new();
    for define in &answer.value {
        *types
            .entry(format!("{:?}", define.value_type))
            .or_insert(0usize) += 1;
    }
    Ok(json!({
        "count": answer.value.len(), "gaps": gap_counts(answer), "types": types,
        "samples": select_samples(&json!(answer.value), &expected["samples"], &["namespace", "name"])?
    }))
}

/// Missing samples become null so a candidate exposes the loss instead of keeping stale values.
fn select_samples(answer: &Value, samples: &Value, keys: &[&str]) -> Result<Value> {
    let values = answer.as_array().ok_or("expected an answer array")?;
    let samples = samples.as_array().ok_or("expected a sample array")?;
    Ok(json!(
        samples
            .iter()
            .map(|sample| {
                values
                    .iter()
                    .find(|value| keys.iter().all(|key| value[key] == sample[key]))
            })
            .collect::<Vec<_>>()
    ))
}

pub fn reference_answer(native: &Native, owner: &str) -> Result<(String, Vec<Gap>, Vec<Field>)> {
    match command(owner) {
        Some((kind, command)) => {
            let answer = current_answer(native, native.command_grammar(kind, command)?)?;
            let fields = match &answer.value.fixed_keys {
                GrammarProperty::Partial(fields) | GrammarProperty::Known(fields) => fields.clone(),
                _ => Vec::new(),
            };
            Ok((serde_json::to_string(&answer)?, answer.gaps, fields))
        }
        None => {
            let answer = current_answer(native, native.registry_fields(owner)?)?;
            Ok((serde_json::to_string(&answer)?, answer.gaps, answer.value))
        }
    }
}

fn references(native: &Native, expected: &Value) -> Result<Value> {
    let mut references = BTreeMap::new();
    for (subject, sample) in expected.as_object().ok_or("expected reference selection")? {
        let (owner, name) = subject.split_once('#').unwrap_or((subject, ""));
        let (_, gaps, fields) = reference_answer(native, owner)?;
        let value = if name.is_empty() {
            let selected = sample
                .as_array()
                .ok_or("expected reference gap selection")?;
            json!(
                gaps.iter()
                    .filter(|gap| {
                        gap.detail.contains("initializer")
                            || selected
                                .iter()
                                .any(|detail| detail.as_str() == Some(&gap.detail))
                    })
                    .map(|gap| &gap.detail)
                    .collect::<Vec<_>>()
            )
        } else {
            json!(
                fields
                    .iter()
                    .find(|field| field.name == name)
                    .map(|field| &field.reference)
            )
        };
        references.insert(subject, value);
    }
    Ok(json!(references))
}

fn dynamic_names(answer: &Answer<Vec<DynamicNamespace>>, expected: &Value) -> Value {
    let roles = ["defined_by", "removed_by", "read_by"];
    let commands: BTreeSet<_> = expected["namespaces"]
        .as_array()
        .expect("namespace samples")
        .iter()
        .flat_map(|namespace| {
            roles
                .iter()
                .flat_map(move |role| namespace[role].as_array().expect("command samples"))
        })
        .filter_map(Value::as_str)
        .collect();
    let namespaces: Vec<_> = answer
        .value
        .iter()
        .map(compact_namespace)
        .filter(|namespace| {
            roles.iter().any(|role| {
                namespace[role]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|command| commands.contains(command.as_str().unwrap()))
            })
        })
        .collect();
    json!({ "count": answer.value.len(), "gaps": gap_counts(answer), "namespaces": namespaces })
}

/// SDK-492's fixed-key vocabulary, independent of the completeness of value forms.
pub fn assert_sdk492_keys(native: &Native) {
    use pdx_native::{GrammarProperty, ReaderKind, RepeatBehavior};
    for (name, expected) in [
        (
            "create_starbase",
            vec![
                ("size", ReaderKind::String),
                ("effect", ReaderKind::Block),
                ("owner", ReaderKind::Target),
                ("design", ReaderKind::String),
                ("module", ReaderKind::String),
                ("building", ReaderKind::String),
            ],
        ),
        (
            "add_district",
            vec![
                ("district_type", ReaderKind::String),
                ("ignore_cap", ReaderKind::Boolean),
                ("type_conversion", ReaderKind::Boolean),
            ],
        ),
    ] {
        let answer = native
            .command_grammar(DeclarationKind::Effect, name)
            .unwrap();
        let (GrammarProperty::Known(keys) | GrammarProperty::Partial(keys)) =
            &answer.value.fixed_keys
        else {
            panic!("{name}: fixed keys unresolved");
        };
        assert_eq!(keys.len(), expected.len(), "{name}");
        for (key, kind) in expected {
            let field = keys.iter().find(|field| field.name == key).unwrap();
            assert_eq!(field.reader.kind, kind, "{name}/{key}");
            if ["module", "building"].contains(&key) {
                assert_eq!(
                    field.shape.repeat,
                    RepeatBehavior::Accumulate,
                    "{name}/{key}"
                );
            }
        }
        assert!(
            answer.completeness == pdx_native::Completeness::Complete || !answer.gaps.is_empty()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tracked_file_has_one_question() {
        let tracked: BTreeSet<_> = std::fs::read_dir(expected_directory())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        let questions: BTreeSet<_> = FILES.iter().map(|name| name.to_string()).collect();
        assert_eq!(FILES.len(), questions.len());
        assert_eq!(tracked, questions);
    }

    #[test]
    fn selections_keep_order_and_expose_changed_and_missing_samples() {
        let samples = json!([
            {"namespace": "one", "name": "same", "value_type": "Integer"},
            {"namespace": "two", "name": "same", "value_type": "Integer"},
            {"namespace": "one", "name": "gone", "value_type": "Integer"}
        ]);
        let answer = json!([
            {"namespace": "two", "name": "same", "value_type": "Float"},
            {"namespace": "one", "name": "same", "value_type": "Integer"},
            {"namespace": "one", "name": "not_selected", "value_type": "Boolean"}
        ]);
        assert_eq!(
            select_samples(&answer, &samples, &["namespace", "name"]).unwrap(),
            json!([answer[1], answer[0], null])
        );
    }
}
