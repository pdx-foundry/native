//! Stored duration counts of flags, modifiers, stack literals and timed traits, read by
//! `Game::check_script`.
//!
//! Storage and diagnostics are recorded separately: a malformed value keeps the count that the
//! parser stored, next to the diagnostic that rejects it.
use super::*;
use pdx_native::{
    DeclarationKind, FixtureValue, GrammarProperty, ScopedNumericLiteral, ScriptCheck,
    StoredDuration,
};
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

/// Every effect and trigger case in one session, by name.
fn checks() -> Vec<(String, String, DeclarationKind, &'static str)> {
    let flags = FLAG_CASES.iter().map(|(name, keys)| {
        (
            format!("set_timed_country_flag/{name}"),
            format!("set_timed_country_flag = {{ flag = native_duration_{name} {keys} }}"),
            DeclarationKind::Effect,
            "country",
        )
    });
    let modifiers = MODIFIER_CASES.iter().map(|(name, keys)| {
        (
            format!("add_modifier/{name}"),
            format!("add_modifier = {{ modifier = {MODIFIER} {keys} }}"),
            DeclarationKind::Effect,
            "country",
        )
    });

    let added = [
        ("country_event/mixed_units", "country_event = { id = native_duration_missing.1 months = 2 years = 1 }", DeclarationKind::Effect, "country"),
        ("has_passed_resolution/mixed_units", "has_passed_resolution = { months = 2 years = 1 }", DeclarationKind::Trigger, "country"),
        ("set_timed_relation_flag/mixed_units", "set_timed_relation_flag = { who = root flag = native_duration_relation months = 2 days = 3 }", DeclarationKind::Effect, "country"),
        ("add_timed_trait/mixed_units", "add_timed_trait = { trait = leader_trait_adaptable months = 2 days = 3 }", DeclarationKind::Effect, "leader"),
        ("country_event/omitted", "country_event = { id = native_duration_missing.1 }", DeclarationKind::Effect, "country"),
        ("add_stage_modifier/omitted", "add_stage_modifier = { modifier = astral_rift_difficulty_increase_2 }", DeclarationKind::Effect, "astral_rift"),
        ("has_passed_resolution/omitted", "has_passed_resolution = { }", DeclarationKind::Trigger, "country"),
    ].into_iter().map(|(name, text, kind, scope)| (name.into(), text.into(), kind, scope));

    flags.chain(modifiers).chain(added).collect()
}

pub(super) async fn stored(native: &Native) -> Outcome {
    let scopes = native.scopes()?.value.types;
    let mut report = BTreeMap::new();
    let mut game = native.start_game(options().loaded_modifiers()).await?;
    let mut result = async {
        for (name, text, kind, scope_name) in checks() {
            let scope = scopes
                .iter()
                .find(|scope| scope.name == scope_name)
                .ok_or("scope missing")?
                .id
                .clone();
            let answer = game
                .check_script(&ScriptCheck {
                    kind,
                    scope,
                    text: text.clone(),
                })
                .await?;
            if answer.source.build != native.build()
                || answer.source.basis != Basis::LiveObservation
            {
                return Err(format!(
                    "{name}: stored duration source differs from the opened build"
                )
                .into());
            }
            let observation = &answer.value;
            // A receiver whose static inventory is partial gives a partial list with its reads.
            let (GrammarProperty::Known(stored) | GrammarProperty::Partial(stored)) =
                &observation.stored_durations
            else {
                return Err(format!("{name}: no stored durations: {answer:?}").into());
            };
            if observation.children != 1 {
                return Err(format!("{name}: expected one child: {answer:?}").into());
            }
            let omitted = name.ends_with("/omitted");
            match stored.as_slice() {
                [only] if only.child == 0 => {}
                [] if matches!(observation.stored_durations, GrammarProperty::Partial(_))
                    && !omitted => {}
                _ => {
                    return Err(format!(
                        "{name}: expected one stored count or a partial storage gap: {answer:?}"
                    )
                    .into());
                }
            }
            if omitted {
                check_omitted_count(native, &name, kind, &stored[0])?;
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

/// Check that an omitted case stores the static omitted count of its duration group.
fn check_omitted_count(
    native: &Native,
    name: &str,
    kind: DeclarationKind,
    stored: &StoredDuration,
) -> Outcome {
    let command = name.split('/').next().unwrap_or(name);
    let grammar = native.command_grammar(kind, command)?.value;
    let (GrammarProperty::Known(groups) | GrammarProperty::Partial(groups)) = &grammar.durations
    else {
        return Err(format!("{name}: no static duration groups").into());
    };
    let group = groups
        .iter()
        .find(|group| {
            group
                .units
                .iter()
                .map(|unit| &unit.key)
                .eq(stored.units.iter())
        })
        .ok_or_else(|| format!("{name}: no static group for {:?}", stored.units))?;
    let GrammarProperty::Known(expected) = group.omitted_count else {
        return Err(format!("{name}: static omitted count is {:?}", group.omitted_count).into());
    };
    let observed = match &stored.count {
        FixtureValue::Integer(count) => i64::from(*count),
        FixtureValue::ScopedNumeric(operand) => match operand.literal {
            ScopedNumericLiteral::Integer(count) => i64::from(count),
            ref literal => return Err(format!("{name}: stored literal {literal:?}").into()),
        },
        count => return Err(format!("{name}: stored count {count:?}").into()),
    };
    if observed != expected {
        return Err(format!("{name}: live count {observed} differs from static {expected}").into());
    }

    Ok(())
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

fn check_report(native: &Native, cases: &BTreeMap<String, serde_json::Value>) -> Outcome {
    let actual = serde_json::json!({"build": native.build(), "cases": cases});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".local/durations/live.json");
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_string_pretty(&actual)?)?;
    let report = comparison::compare_durations(
        &native.build(),
        "duration-m451/live.json",
        include_bytes!("../expected/duration-m451/live.json"),
        &serde_json::to_vec(&actual)?,
    );
    eprint!("{}", report.render_and_save()?);
    if !report.passes() {
        return Err(format!(
            "stored duration parity failed; candidate: {}",
            path.display()
        )
        .into());
    }

    Ok(())
}
