//! Operand storage and rejected-input controls through the paused script parser.
use super::*;
use pdx_native::{
    DeclarationKind, FixtureValue, GrammarProperty, ScopedNumericStorage, ScriptCheck, ScriptStage,
};
use serde::{Deserialize, Serialize};

const TABLE: &str = include_str!("../expected/script-numeric-m452/cases.json");
const INTEGER_EFFECT: &str = "set_timed_country_flag";
const INTEGER_KEY: &str = "days";
const FIXED_EFFECT: &str = "set_variable";
const FIXED_KEY: &str = "value";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    build: String,
    sessions: Vec<Session>,
    rejected: Vec<Rejected>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    name: String,
    before: Vec<String>,
    cases: Vec<Operand>,
    after: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Operand {
    name: String,
    operands: Vec<String>,
    stored: Option<ScopedNumericStorage>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rejected {
    name: String,
    operand: String,
    /// Parser storage of the integer operand. Rejection does not prevent storage.
    stored: Option<ScopedNumericStorage>,
    integer: Vec<Rejection>,
    fixed: Vec<Rejection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rejection {
    stage: ScriptStage,
    text: String,
}

/// A unique flag key for the parser case.
fn flag(case: &str) -> String {
    format!("n647_i_{case}")
}

/// A unique variable key for the parser case.
fn variable(case: &str) -> String {
    format!("n647_f_{case}")
}

fn assignments(key: &str, operands: &[String]) -> String {
    let assignments: Vec<_> = operands
        .iter()
        .map(|operand| format!("{key} = {operand}"))
        .collect();

    assignments.join(" ")
}

fn integer_statement(case: &str, operands: &[String]) -> String {
    let flag = flag(case);
    let days = assignments(INTEGER_KEY, operands);

    format!("{INTEGER_EFFECT} = {{ flag = {flag} {days} }}")
}

fn fixed_statement(case: &str, operands: &[String]) -> String {
    let variable = variable(case);
    let values = assignments(FIXED_KEY, operands);

    format!("{FIXED_EFFECT} = {{ which = {variable} {values} }}")
}

fn table(native: &Native) -> Result<Table, Box<dyn std::error::Error>> {
    let table: Table = serde_json::from_str(TABLE)?;
    let build = serde_json::to_value(native.build())?;
    if build != table.build.as_str() {
        return Err("the reviewed operand parser results belong to another exact build".into());
    }

    Ok(table)
}

fn write_candidate(case: &str, table: &Table) -> Result<std::path::PathBuf, std::io::Error> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".local/script-numeric")
        .join(format!("{case}.json"));
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_string_pretty(table)? + "\n")?;

    Ok(path)
}

struct Checked {
    stored: Option<ScopedNumericStorage>,
    rejections: Vec<Rejection>,
}

async fn check(
    game: &mut Game,
    scope: &pdx_native::ScopeId,
    text: String,
) -> Result<Checked, Box<dyn std::error::Error>> {
    let answer = game
        .check_script(&ScriptCheck {
            kind: DeclarationKind::Effect,
            scope: scope.clone(),
            text: text.clone(),
        })
        .await?;
    let observation = answer.value;
    if !observation.read_returned || !observation.hooks_active || observation.bound_reached {
        return Err(format!("{text}: incomplete check: {observation:?}").into());
    }

    let stored = match &observation.stored_durations {
        GrammarProperty::Known(stored) | GrammarProperty::Partial(stored) => {
            stored.iter().find_map(|duration| match &duration.count {
                FixtureValue::ScopedNumeric(storage) => Some(storage.clone()),
                _ => None,
            })
        }
        GrammarProperty::Unresolved => None,
    };
    let rejections = observation
        .diagnostics
        .into_iter()
        .chain(observation.unjoined)
        .map(|diagnostic| Rejection {
            stage: diagnostic.stage,
            text: diagnostic.text,
        })
        .collect();

    Ok(Checked { stored, rejections })
}

/// Reads every operand through `check_script`: parser storage of each integer operand, silence
/// of prepared statements, and the messages of each rejected operand.
pub(super) async fn stored(native: &Native) -> Outcome {
    let reviewed = table(native)?;
    let scope = native
        .scopes()?
        .value
        .types
        .into_iter()
        .find(|scope| scope.name == "country")
        .ok_or("country scope missing")?
        .id;
    let mut candidate = reviewed.clone();
    let mut rejected_statements = Vec::new();
    let mut game = native.start_game(options().loaded_modifiers()).await?;
    let mut result = async {
        for session in &mut candidate.sessions {
            let statements = session.before.iter().chain(&session.after);
            for statement in statements {
                let checked = check(&mut game, &scope, statement.clone()).await?;
                if !checked.rejections.is_empty() {
                    rejected_statements.push((statement.clone(), checked.rejections));
                }
            }

            for case in &mut session.cases {
                let integer = integer_statement(&case.name, &case.operands);
                let fixed = fixed_statement(&case.name, &case.operands);
                let checked_integer = check(&mut game, &scope, integer.clone()).await?;
                let checked_fixed = check(&mut game, &scope, fixed.clone()).await?;
                if !checked_integer.rejections.is_empty() {
                    rejected_statements.push((integer, checked_integer.rejections));
                }
                if !checked_fixed.rejections.is_empty() {
                    rejected_statements.push((fixed, checked_fixed.rejections));
                }
                case.stored = checked_integer.stored;
            }
        }

        // Rejected operands run last: a rejected command can report again in later validations.
        for rejected in &mut candidate.rejected {
            let operands = [rejected.operand.clone()];
            let integer = integer_statement(&rejected.name, &operands);
            let fixed = fixed_statement(&rejected.name, &operands);
            let integer = check(&mut game, &scope, integer).await?;
            let fixed = check(&mut game, &scope, fixed).await?;
            rejected.stored = integer.stored;
            rejected.integer = integer.rejections;
            rejected.fixed = fixed.rejections;
        }

        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result?;

    let path = write_candidate("stored", &candidate)?;
    if !rejected_statements.is_empty() {
        return Err(format!(
            "the engine rejects a previously accepted statement: {rejected_statements:#?}"
        )
        .into());
    }
    if candidate != reviewed {
        return Err(format!(
            "stored operands differ from the reviewed table; candidate: {}",
            path.display()
        )
        .into());
    }

    for rejected in &reviewed.rejected {
        if rejected.integer.is_empty() || rejected.fixed.is_empty() {
            return Err(format!("{}: the operand was not rejected", rejected.name).into());
        }
    }

    Ok(())
}
