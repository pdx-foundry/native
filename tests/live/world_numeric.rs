//! Numeric operands evaluated by the engine's own effect execution in a loaded world.
//!
//! An integer operand is the `days` of a timed country flag; its day-zero count is the evaluated
//! value. A fixed-point operand is the `value` of `set_variable`; the stored variable is the
//! evaluated value. `tests/expected/world-numeric-m451/cases.json` holds every case, its reviewed
//! result, and the parser storage that `Game::check_script` reports for the integer operand.
//!
//! Each live case writes the table with its own observed results to `.local/world-numeric/`.
//! Review a changed candidate against the engine before it replaces the tracked file.
use super::*;
use pdx_native::{
    DeclarationKind, FixtureValue, GrammarProperty, Reader, ScopedLiteralCondition,
    ScopedNumericStorage, ScopedOperandSelection, ScopedReferenceKind, ScriptCheck, ScriptStage,
    WorldObservation, WorldSample,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const TABLE: &str = include_str!("../expected/world-numeric-m451/cases.json");
const INTEGER_EFFECT: &str = "set_timed_country_flag";
const INTEGER_KEY: &str = "days";
const FIXED_EFFECT: &str = "set_variable";
const FIXED_KEY: &str = "value";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    /// The exact build of every reviewed result.
    build: String,
    sessions: Vec<Session>,
    /// Operands that the engine rejects while it reads or validates them. No world executes them.
    rejected: Vec<Rejected>,
}

/// One prepared effect: the statements before the cases, two statements for each case, then
/// the statements after them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    name: String,
    /// Whether the engine reports nothing while it executes the effect.
    complete: bool,
    /// Statements that prepare the world, or that are the whole observation of a session with no
    /// cases.
    before: Vec<String>,
    cases: Vec<Case>,
    /// Statements that reach the numbers of the cases by another engine route.
    after: Vec<String>,
    /// Day-zero counts of the flags that the statements outside the cases write.
    flags: BTreeMap<String, Option<i32>>,
    /// Day-zero raw values of the variables that the statements outside the cases write.
    variables: BTreeMap<String, Option<i64>>,
    /// Pairs of variables that two engine routes must set to one value.
    equal: Vec<[String; 2]>,
    /// Raw differences between two variables, such as a stockpile before and after a change.
    differences: Vec<Difference>,
    /// A fragment of each message that the engine reports while it executes the effect, in order.
    diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Difference {
    before: String,
    after: String,
    raw: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    /// One operand, or two assigned in order to the same key.
    operands: Vec<String>,
    /// The stored form whose value the results show.
    route: Route,
    /// A one-operand case of this session that must give the same results.
    same_as: Option<String>,
    /// Day-zero flag count of the integer operand.
    integer: Option<i32>,
    /// Raw variable value of the fixed-point operand.
    fixed: Option<i64>,
    /// Parser storage of the integer operand, before validation and execution.
    stored: Option<ScopedNumericStorage>,
    /// SDK-493 evaluation numbers that this case reproduces in a world scope.
    prototype: Vec<u32>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Route {
    Literal,
    Trigger,
    ScriptValue,
    Modifier,
    Variable,
}

/// The flag that holds the integer result of the named case.
fn flag(case: &str) -> String {
    format!("n647_i_{case}")
}

/// The variable that holds the fixed-point result of the named case.
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

fn raw_variables(sample: &WorldSample) -> BTreeMap<String, Option<i64>> {
    sample
        .variables
        .iter()
        .map(|variable| (variable.name.clone(), variable.value.map(|value| value.raw)))
        .collect()
}

impl Session {
    fn effect(&self) -> String {
        let cases = self.cases.iter().flat_map(|case| {
            [
                integer_statement(&case.name, &case.operands),
                fixed_statement(&case.name, &case.operands),
            ]
        });
        let statements: Vec<_> = self
            .before
            .iter()
            .cloned()
            .chain(cases)
            .chain(self.after.iter().cloned())
            .collect();

        statements.join("\n")
    }

    fn request(&self) -> pdx_native::WorldRequest {
        let mut request = world::request();
        request.effect = self.effect();
        request.flags = self
            .cases
            .iter()
            .map(|case| flag(&case.name))
            .chain(self.flags.keys().cloned())
            .collect();
        request.variables = self
            .cases
            .iter()
            .map(|case| variable(&case.name))
            .chain(self.variables.keys().cloned())
            .collect();

        request
    }

    /// The table with this session's results replaced by the observed ones.
    fn observed(&self, observation: &WorldObservation, complete: bool) -> Result<Self, String> {
        let [sample] = observation.samples.as_slice() else {
            return Err(format!("{}: expected one day-zero sample", self.name));
        };
        let flags: BTreeMap<_, _> = sample
            .flags
            .iter()
            .map(|flag| (flag.name.clone(), flag.remaining))
            .collect();
        let variables = raw_variables(sample);
        let mut session = self.clone();
        session.complete = complete;
        session.diagnostics.clone_from(&observation.diagnostics);

        for case in &mut session.cases {
            case.integer = flags[&flag(&case.name)];
            case.fixed = variables[&variable(&case.name)];
        }

        for (name, count) in &mut session.flags {
            *count = flags[name];
        }

        for (name, raw) in &mut session.variables {
            *raw = variables[name];
        }

        for difference in &mut session.differences {
            let (Some(before), Some(after)) =
                (variables[&difference.before], variables[&difference.after])
            else {
                return Err(format!(
                    "{}: a difference names an unset variable",
                    self.name
                ));
            };
            difference.raw = after - before;
        }

        Ok(session)
    }

    /// The candidate keeps each full message; the reviewed table holds a fragment of each.
    fn matches(&self, observed: &Self) -> bool {
        let fragments_match = observed.diagnostics.len() == self.diagnostics.len()
            && observed
                .diagnostics
                .iter()
                .zip(&self.diagnostics)
                .all(|(message, fragment)| message.contains(fragment.as_str()));
        let mut results = observed.clone();
        results.diagnostics.clone_from(&self.diagnostics);

        fragments_match && *self == results
    }

    /// The pairs that do not hold one set value.
    fn unequal(&self, variables: &BTreeMap<String, Option<i64>>) -> Vec<&[String; 2]> {
        self.equal
            .iter()
            .filter(|[first, second]| {
                variables[first].is_none() || variables[first] != variables[second]
            })
            .collect()
    }
}

fn table(native: &Native) -> Result<Table, Box<dyn std::error::Error>> {
    let table: Table = serde_json::from_str(TABLE)?;
    let build = serde_json::to_value(native.build())?;
    if build != table.build.as_str() {
        return Err("the reviewed world numeric results belong to another exact build".into());
    }

    Ok(table)
}

fn write_candidate(case: &str, table: &Table) -> Result<std::path::PathBuf, std::io::Error> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".local/world-numeric")
        .join(format!("{case}.json"));
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_string_pretty(table)? + "\n")?;

    Ok(path)
}

fn established<T>(property: &GrammarProperty<T>) -> Option<&T> {
    match property {
        GrammarProperty::Known(value) | GrammarProperty::Partial(value) => Some(value),
        GrammarProperty::Unresolved => None,
    }
}

/// The reader of one effect key. Facts belong to this destination, not to its shared reader.
fn destination(native: &Native, effect: &str, key: &str) -> Result<Reader, String> {
    let grammar = native
        .command_grammar(DeclarationKind::Effect, effect)
        .map_err(|error| format!("{effect}: {error:?}"))?;
    let keys = established(&grammar.value.fixed_keys).ok_or(format!("{effect}: no fixed keys"))?;
    let field = keys
        .iter()
        .find(|field| field.name == key)
        .ok_or(format!("{effect}: key {key} is not established"))?;

    Ok(field.reader.clone())
}

fn scale(reader: &Reader) -> Option<u64> {
    let numeric = established(&reader.numeric)?.as_ref()?;

    *established(&numeric.scale)?
}

fn selection(reader: &Reader) -> Option<&ScopedOperandSelection> {
    let operand = established(&reader.scoped_operand)?.as_ref()?;

    established(&operand.selection)
}

/// The stored form that the static selection rules choose for this parser storage.
fn selected(stored: &ScopedNumericStorage, selection: &ScopedOperandSelection) -> Option<Route> {
    let ScopedLiteralCondition::EmptySourceLocation = selection.literal_condition else {
        return None;
    };
    if !stored.has_source_location {
        return Some(Route::Literal);
    }

    // The engine tries each stored reference in this order. The variable route is the last one
    // and needs no stored name.
    selection
        .reference_priority
        .iter()
        .find_map(|kind| match kind {
            ScopedReferenceKind::Trigger if stored.has_trigger => Some(Route::Trigger),
            ScopedReferenceKind::ScriptValue if stored.has_script_value => Some(Route::ScriptValue),
            ScopedReferenceKind::Modifier if stored.has_modifier => Some(Route::Modifier),
            ScopedReferenceKind::Variable => Some(Route::Variable),
            _ => None,
        })
}

/// Differences between the results and the static facts of the two destinations.
fn conflicts(native: &Native, session: &Session) -> Result<Vec<String>, String> {
    let integer = destination(native, INTEGER_EFFECT, INTEGER_KEY)?;
    let fixed = destination(native, FIXED_EFFECT, FIXED_KEY)?;
    let selection = selection(&integer).ok_or("integer selection is not established")?;
    let fixed_scale = scale(&fixed).ok_or("fixed-point scale is not established")?;
    let mut conflicts = Vec::new();

    if scale(&integer) != Some(1) {
        conflicts.push("the integer destination's static scale is not 1".to_owned());
    }

    for case in &session.cases {
        if let Some(stored) = &case.stored {
            let chosen = selected(stored, selection);
            if chosen != Some(case.route) {
                conflicts.push(format!(
                    "{}: static selection chooses {chosen:?}, the results show {:?}",
                    case.name, case.route
                ));
            }
        }

        // Only these routes convert one fixed-point result in the matched integer body.
        let divides = matches!(
            case.route,
            Route::ScriptValue | Route::Modifier | Route::Variable
        );
        if let (true, Some(integer), Some(raw)) = (divides, case.integer, case.fixed)
            && i64::from(integer) != raw / fixed_scale as i64
        {
            conflicts.push(format!(
                "{}: integer {integer} is not fixed {raw} divided by {fixed_scale}",
                case.name
            ));
        }

        if let Some(name) = &case.same_as {
            let single = session
                .cases
                .iter()
                .find(|other| &other.name == name)
                .ok_or(format!("{}: no case named {name}", case.name))?;
            if (case.integer, case.fixed) != (single.integer, single.fixed) {
                conflicts.push(format!(
                    "{}: results differ from its selected operand {name}",
                    case.name
                ));
            }
        }
    }

    Ok(conflicts)
}

/// Runs one session's effect in the prepared world and compares it with the reviewed table.
pub(super) async fn evaluated(native: &Native, name: &str) -> Outcome {
    let reviewed = table(native)?;
    let index = reviewed
        .sessions
        .iter()
        .position(|session| session.name == name)
        .ok_or("session missing from the reviewed table")?;
    let session = &reviewed.sessions[index];
    let request = session.request();
    let mut game = native.start_game(options().world(request.clone())).await?;
    let mut result = async {
        let answer = game.observe_world().await?;
        println!("{name}: {}", serde_json::to_string(&answer)?);
        if !answer.value.executed {
            return Err(format!("{name}: the effect did not execute: {answer:?}").into());
        }
        let [sample] = answer.value.samples.as_slice() else {
            return Err(format!("{name}: expected one day-zero sample").into());
        };
        let static_scale = scale(&destination(native, FIXED_EFFECT, FIXED_KEY)?);
        let stored_values = sample
            .variables
            .iter()
            .filter_map(|variable| variable.value);
        if stored_values
            .into_iter()
            .any(|value| Some(value.scale) != static_scale)
        {
            return Err(format!(
                "{name}: a variable scale differs from the static scale {static_scale:?}"
            )
            .into());
        }

        let complete = answer.completeness == Completeness::Complete;
        let observed = session.observed(&answer.value, complete)?;
        let mut candidate = reviewed.clone();
        candidate.sessions[index] = observed.clone();
        let path = write_candidate(name, &candidate)?;
        if !session.matches(&observed) {
            return Err(format!(
                "{name}: results differ from the reviewed table; candidate: {}",
                path.display()
            )
            .into());
        }

        for case in &session.cases {
            if case.integer.is_none() || case.fixed.is_none() {
                return Err(format!("{name}/{}: a case result was not written", case.name).into());
            }
        }

        let unequal = session.unequal(&raw_variables(sample));
        if !unequal.is_empty() {
            return Err(format!("{name}: two routes gave different values: {unequal:?}").into());
        }

        let conflicts = conflicts(native, session)?;
        if !conflicts.is_empty() {
            return Err(format!("{name}: static conflicts: {conflicts:#?}").into());
        }

        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;

    result
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
/// of each statement that a world executes, and the messages of each rejected operand.
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
    let mut rejected_world_statements = Vec::new();
    let mut game = native.start_game(options().loaded_modifiers()).await?;
    let mut result = async {
        for session in &mut candidate.sessions {
            let statements = session.before.iter().chain(&session.after);
            for statement in statements {
                let checked = check(&mut game, &scope, statement.clone()).await?;
                if !checked.rejections.is_empty() {
                    rejected_world_statements.push((statement.clone(), checked.rejections));
                }
            }

            for case in &mut session.cases {
                let integer = integer_statement(&case.name, &case.operands);
                let fixed = fixed_statement(&case.name, &case.operands);
                let checked_integer = check(&mut game, &scope, integer.clone()).await?;
                let checked_fixed = check(&mut game, &scope, fixed.clone()).await?;
                if !checked_integer.rejections.is_empty() {
                    rejected_world_statements.push((integer, checked_integer.rejections));
                }
                if !checked_fixed.rejections.is_empty() {
                    rejected_world_statements.push((fixed, checked_fixed.rejections));
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
    if !rejected_world_statements.is_empty() {
        return Err(format!(
            "the engine rejects statements that a world must execute: {rejected_world_statements:#?}"
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
