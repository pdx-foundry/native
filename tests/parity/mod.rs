//! Questions and selections for the tracked static parity files.
//! Historical live observations are retained, never recreated from static answers.
mod compact;
mod layout;

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

pub const FILES: &[&str] = &[
    "registries.json",
    "fields-traditions.json",
    "fields-tradition_categories.json",
    "fields-council_agendas.json",
    "fields-megastructures.json",
    "field-storage-sdk533.json",
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
    "on-actions.json",
    "game-rules.json",
    "localization-declarations.json",
    "scope-inventory.json",
    "scope-links.json",
];

pub fn expected_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/expected/m45")
}

/// Read the tracked selection and ask its question. Formatting uses the tracked key order.
pub fn candidate(native: &Native, name: &str) -> Result<Vec<u8>> {
    let template = std::fs::read(expected_directory().join(name))?;
    let expected: Value = serde_json::from_slice(&template)?;
    let value = question(native, name, &expected)?;
    if name == "field-storage-sdk533.json" {
        return Ok(template);
    }
    layout::render(name, &value, &template)
}

/// Compare static facts while checking current command stamps separately from reviewed provenance.
#[cfg(test)]
pub fn static_facts_match(
    build: &BuildId,
    name: &str,
    candidate: &[u8],
    reviewed: &[u8],
) -> Result<bool> {
    if name == "dynamic-names.json" {
        let mut candidate: Value = serde_json::from_slice(candidate)?;
        let mut reviewed: Value = serde_json::from_slice(reviewed)?;
        // Internal store IDs depend on relocated addresses; full rows retain duplicates and facts.
        for report in [&mut candidate, &mut reviewed] {
            report["namespaces"]
                .as_array_mut()
                .ok_or("dynamic namespace selection must be an array")?
                .sort_by_cached_key(Value::to_string);
        }
        return Ok(candidate == reviewed);
    }
    if name != "command-grammars.json" {
        return Ok(candidate == reviewed);
    }
    let mut candidate: BTreeMap<String, Value> = serde_json::from_slice(candidate)?;
    let mut reviewed: BTreeMap<String, Value> = serde_json::from_slice(reviewed)?;
    for (subject, answer) in &mut candidate {
        let source: Source = serde_json::from_value(answer["source"].clone())?;
        if &source.build != build {
            return Err(format!("{subject}: candidate source does not match Native::build").into());
        }
        answer["source"]
            .as_object_mut()
            .ok_or("command source must be an object")?
            .remove("build");
    }
    for answer in reviewed.values_mut() {
        let _: Source = serde_json::from_value(answer["source"].clone())?;
        answer["source"]
            .as_object_mut()
            .ok_or("command source must be an object")?
            .remove("build");
    }
    Ok(candidate == reviewed)
}

/// Historical live storage applies only to its recorded exact build, never to a hotfix by inference.
pub fn historical_storage_applies(build: &BuildId, observed: &Value) -> Result<bool> {
    let source: Source = serde_json::from_value(observed["source"].clone())?;
    if source.basis != Basis::LiveObservation {
        return Err("SDK-533 storage must retain its live observation provenance".into());
    }
    Ok(&source.build == build)
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
        return Ok(json!(
            current_answer(native, native.registry_fields(registry)?)?.value
        ));
    }
    match name {
        "registries.json" => Ok(json!(
            current_answer(native, native.registries()?)?
                .value
                .iter()
                .map(|item| &item.name)
                .collect::<Vec<_>>()
        )),
        "field-storage-sdk533.json" => {
            if !historical_storage_applies(&native.build(), expected)? {
                return Err(
                    "the historical SDK-533 storage observation is for a different build".into(),
                );
            }
            Ok(expected.clone())
        }
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
                .map(|item| &item.name)
                .collect::<Vec<_>>()
        )),
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

    fn command_record(build: &str) -> Value {
        json!({
            "effect/sample": {
                "source": {
                    "build": build,
                    "native_version": "0.1.0",
                    "method": "command-grammar/v2",
                    "basis": "StaticAnalysis"
                },
                "value": {"forms": ["Literal"]},
                "completeness": "Complete",
                "gaps": []
            }
        })
    }

    fn build(value: &str) -> BuildId {
        serde_json::from_value(json!(value)).unwrap()
    }

    #[test]
    fn cross_build_command_parity_checks_current_stamp_and_keeps_fact_differences() {
        let reviewed = command_record("release");
        let candidate = command_record("hotfix");
        let matches = |value: &Value| {
            static_facts_match(
                &build("hotfix"),
                "command-grammars.json",
                &serde_json::to_vec(value).unwrap(),
                &serde_json::to_vec(&reviewed).unwrap(),
            )
        };
        assert!(matches(&candidate).unwrap());
        assert!(matches(&reviewed).is_err());
        for (pointer, replacement) in [
            ("/effect~1sample/value/forms", json!("Unresolved")),
            ("/effect~1sample/completeness", json!("Partial")),
            ("/effect~1sample/gaps", json!([{"detail": "lost form"}])),
            (
                "/effect~1sample/source/method",
                json!("different-method/v1"),
            ),
            ("/effect~1sample/source/native_version", json!("different")),
        ] {
            let mut changed = candidate.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(!matches(&changed).unwrap(), "{pointer}");
        }
        let mut missing_stamp = candidate.clone();
        missing_stamp["effect/sample"]["source"]
            .as_object_mut()
            .unwrap()
            .remove("build");
        assert!(matches(&missing_stamp).is_err());
        assert_eq!(candidate["effect/sample"]["source"]["build"], "hotfix");
        assert_eq!(reviewed["effect/sample"]["source"]["build"], "release");
    }

    #[test]
    fn same_build_commands_and_other_files_keep_their_existing_comparison() {
        let recorded = serde_json::to_vec(&command_record("release")).unwrap();
        assert!(
            static_facts_match(
                &build("release"),
                "command-grammars.json",
                &recorded,
                &recorded
            )
            .unwrap()
        );
        let changed = serde_json::to_vec(&command_record("hotfix")).unwrap();
        assert!(
            !static_facts_match(
                &build("hotfix"),
                "fields-traditions.json",
                &changed,
                &recorded
            )
            .unwrap()
        );
    }

    #[test]
    fn namespace_order_can_change_but_roles_counts_gaps_and_duplicates_cannot() {
        let reviewed = json!({"count":2,"gaps":{"UnresolvedReader":1},"namespaces":[
            {"owner":"star","read_by":["has_star_flag"]},
            {"owner":"star","defined_by":["set_star_flag"]}
        ]});
        let matches = |value: &Value| {
            static_facts_match(
                &build("hotfix"),
                "dynamic-names.json",
                &serde_json::to_vec(value).unwrap(),
                &serde_json::to_vec(&reviewed).unwrap(),
            )
            .unwrap()
        };
        let mut reordered = reviewed.clone();
        reordered["namespaces"].as_array_mut().unwrap().reverse();
        assert!(matches(&reordered));
        for (pointer, replacement) in [
            ("/count", json!(3)),
            ("/gaps/UnresolvedReader", json!(2)),
            ("/namespaces/0/owner", json!("country")),
            ("/namespaces/1/read_by", json!(["has_country_flag"])),
        ] {
            let mut changed = reordered.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(!matches(&changed), "{pointer}");
        }
        let mut duplicate = reordered.clone();
        let row = duplicate["namespaces"][0].clone();
        duplicate["namespaces"][1] = row;
        assert!(!matches(&duplicate));
    }

    #[test]
    fn historical_storage_is_applicable_only_to_its_exact_live_build() {
        let mut observed = json!({
            "source": {
                "build": "release",
                "native_version": "0.1.0",
                "method": "observe-fixture/v1",
                "basis": "LiveObservation"
            },
            "outcomes": [{"stored": "original observation"}]
        });
        let unchanged = observed.clone();
        assert!(historical_storage_applies(&build("release"), &observed).unwrap());
        assert!(!historical_storage_applies(&build("hotfix"), &observed).unwrap());
        assert_eq!(observed, unchanged);
        observed["source"]["basis"] = json!("StaticAnalysis");
        assert!(historical_storage_applies(&build("release"), &observed).is_err());
    }

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
