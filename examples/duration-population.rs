//! Measure duration groups over every registered command and every discovered registry field.
//!
//! Commands with a key named `days`, `months` or `years` that no group covers are listed
//! separately: the method identifies groups by mechanism, and these names are only a census.
use pdx_native::internals::{command_grammar_stops, duration_groups};
use pdx_native::{DeclarationKind, Duration, Gap, GrammarProperty, Native};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const UNIT_NAMES: [&str; 3] = ["days", "months", "years"];

#[derive(Default)]
struct Population {
    counts: BTreeMap<&'static str, usize>,
    failure_shapes: BTreeMap<String, usize>,
    groups: Vec<Value>,
    unidentified: Vec<Value>,
}

impl Population {
    fn group(&mut self, command: &str, duration: &Duration, gaps: &[Gap]) {
        let factors_known = duration
            .units
            .iter()
            .all(|unit| matches!(unit.factor, GrammarProperty::Known(_)));
        let status = match (&duration.combination, factors_known) {
            (GrammarProperty::Known(_), true)
                if matches!(duration.consumption, GrammarProperty::Known(_))
                    && matches!(duration.omitted_count, GrammarProperty::Known(_)) =>
            {
                "complete"
            }
            (GrammarProperty::Known(_), _) => "partial",
            _ if duration
                .units
                .iter()
                .any(|unit| !matches!(unit.factor, GrammarProperty::Unresolved)) =>
            {
                "partial"
            }
            _ => "failed",
        };
        let keys: Vec<_> = duration
            .units
            .iter()
            .map(|unit| unit.key.as_str())
            .collect();
        let prefix = format!("Duration keys {}: ", keys.join(", "));
        let relevant: Vec<_> = gaps
            .iter()
            .filter(|gap| gap.detail.starts_with(&prefix))
            .collect();

        *self.counts.entry(status).or_default() += 1;

        for gap in &relevant {
            let shape = format!("{:?}: {}", gap.kind, &gap.detail[prefix.len()..]);
            *self.failure_shapes.entry(shape).or_default() += 1;
        }

        self.groups.push(json!({
            "command": command, "status": status, "duration": duration, "gaps": relevant,
        }));
    }
}

fn unit_keys(fixed_keys: &GrammarProperty<Vec<pdx_native::Field>>) -> Vec<String> {
    match fixed_keys {
        GrammarProperty::Known(keys) | GrammarProperty::Partial(keys) => keys
            .iter()
            .filter(|key| UNIT_NAMES.contains(&key.name.as_str()))
            .map(|key| key.name.clone())
            .collect(),
        GrammarProperty::Unresolved => Vec::new(),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let native = Native::open(std::env::var_os("STELLARIS_PATH").ok_or("set STELLARIS_PATH")?)?;
    let mut population = Population::default();
    let mut failed_questions = Vec::new();
    let mut commands = 0;

    for state in ["complete", "partial", "failed"] {
        population.counts.insert(state, 0);
    }

    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        command_grammar_stops::population(&native, kind, |name, run| {
            let name = format!("{kind:?}/{name}");
            let answer = run.answer;
            let groups = match &answer.value.durations {
                GrammarProperty::Known(groups) | GrammarProperty::Partial(groups) => groups.clone(),
                GrammarProperty::Unresolved => Vec::new(),
            };
            let covered: Vec<_> = groups
                .iter()
                .flat_map(|group| group.units.iter().map(|unit| unit.key.clone()))
                .collect();
            let uncovered: Vec<_> = unit_keys(&answer.value.fixed_keys)
                .into_iter()
                .filter(|key| !covered.contains(key))
                .collect();

            commands += 1;

            for group in &groups {
                population.group(&name, group, &answer.gaps);
            }

            if !uncovered.is_empty() {
                population.unidentified.push(json!({
                    "command": name, "keys": uncovered, "durations": answer.value.durations,
                }));
            }
        })?;
    }

    let registries = native.registries()?;
    let mut registry_groups = Vec::new();

    for registry in &registries.value {
        match duration_groups::registry(&native, &registry.name) {
            Ok(groups) => registry_groups.extend(groups.into_iter().map(|group| {
                json!({
                    "registry": registry.name,
                    "path": group.path,
                    "units": group.group.units.iter().map(|unit| json!({
                        "key": unit.key, "factor": format!("{:?}", unit.factor),
                    })).collect::<Vec<_>>(),
                    "combination": format!("{:?}", group.group.combination),
                })
            })),
            Err(error) => {
                failed_questions.push(json!({"registry": registry.name, "error": error}));
            }
        }
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "build": native.build(),
            "commands": commands,
            "registries": registries.value.len(),
            "counts": population.counts,
            "failure_shapes": population.failure_shapes,
            "groups": population.groups,
            "unidentified_unit_keys": population.unidentified,
            "registry_groups": registry_groups,
            "failed_questions": failed_questions,
        }))?
    );

    Ok(())
}
