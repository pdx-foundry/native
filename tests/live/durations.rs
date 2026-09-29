//! Stored duration counts of a timed flag and a timed modifier, read by `Game::check_script`.
//!
//! Storage and diagnostics are recorded separately: a malformed value keeps the count that the
//! parser stored, next to the diagnostic that rejects it.
use super::*;
use pdx_native::{DeclarationKind, GrammarProperty, ScriptCheck};
use std::collections::BTreeMap;

/// A static modifier of the game's `common/static_modifiers`, which events add to a country.
const MODIFIER: &str = "gave_up_pop";

/// Case name and the duration keys after the command's other keys.
const FLAG_CASES: [(&str, &str); 13] = [
    ("omitted", ""),
    ("days", "days = 7"),
    ("months", "months = 2"),
    ("years", "years = 2"),
    ("months_then_days", "months = 2 days = 3"),
    ("zero_days", "days = 0"),
    ("negative_days", "days = -1"),
    ("largest_days", "days = 2147483647"),
    ("years_overflow", "years = 5965233"),
    ("fractional_days", "days = 2.75"),
    ("malformed_after_literal", "days = 7 days = x"),
    (
        "missing_value_then_days",
        "months = value:native_missing_value days = 3",
    ),
    ("variable_then_days", "days = native_variable days = 4"),
];

const MODIFIER_CASES: [(&str, &str); 9] = [
    ("omitted", ""),
    ("days", "days = 7"),
    ("months", "months = 2"),
    ("years", "years = 2"),
    ("months_then_days", "months = 2 days = 3"),
    ("negative_days", "days = -1"),
    ("fractional_days", "days = 2.75"),
    ("months_overflow", "months = 71582789"),
    ("malformed_after_literal", "days = 7 days = x"),
];

/// Every case of one effect session, by name.
fn checks() -> Vec<(String, String)> {
    let flags = FLAG_CASES.iter().map(|(name, keys)| {
        (
            format!("set_timed_country_flag/{name}"),
            format!("set_timed_country_flag = {{ flag = native_duration_{name} {keys} }}"),
        )
    });
    let modifiers = MODIFIER_CASES.iter().map(|(name, keys)| {
        (
            format!("add_modifier/{name}"),
            format!("add_modifier = {{ modifier = {MODIFIER} {keys} }}"),
        )
    });

    flags.chain(modifiers).collect()
}

pub(super) async fn stored(native: &Native) -> Outcome {
    let scope = native
        .scopes()?
        .value
        .types
        .into_iter()
        .find(|scope| scope.name == "country")
        .ok_or("country scope missing")?
        .id;
    let mut report = BTreeMap::new();
    let mut game = native.start_game(options().loaded_modifiers()).await?;
    let mut result = async {
        for (name, text) in checks() {
            let answer = game
                .check_script(&ScriptCheck {
                    kind: DeclarationKind::Effect,
                    scope: scope.clone(),
                    text: text.clone(),
                })
                .await?;
            let observation = &answer.value;
            let GrammarProperty::Known(stored) = &observation.stored_durations else {
                return Err(format!("{name}: unclassified child: {answer:?}").into());
            };
            let [only] = stored.as_slice() else {
                return Err(format!("{name}: expected one stored count: {answer:?}").into());
            };
            if observation.children != 1 || only.child != 0 {
                return Err(format!("{name}: expected one child: {answer:?}").into());
            }

            report.insert(name, record(&text, &answer));
        }

        Ok(())
    }
    .await;

    and_close(&mut result, &mut game).await;
    result?;
    check_report(native, &report)
}

fn record(text: &str, answer: &Answer<pdx_native::ScriptObservation>) -> serde_json::Value {
    let diagnostics: Vec<_> = answer
        .value
        .diagnostics
        .iter()
        .map(|diagnostic| {
            serde_json::json!({
                "text": diagnostic.text,
                "stage": diagnostic.stage,
                "line": diagnostic.line,
            })
        })
        .collect();

    serde_json::json!({
        "text": text,
        "stored_durations": answer.value.stored_durations,
        "diagnostics": diagnostics,
        "completeness": answer.completeness,
    })
}

fn check_report(native: &Native, report: &BTreeMap<String, serde_json::Value>) -> Outcome {
    let actual = serde_json::json!({"build": native.build(), "cases": report});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".local/durations/live.json");
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_string_pretty(&actual)?)?;
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../expected/duration-m45/live.json"))?;

    if actual != expected {
        return Err(format!("stored durations differ; inspect {}", path.display()).into());
    }

    Ok(())
}
