//! Measure duration groups over every registered command and every discovered registry field.
//!
//! Commands with a key named `days`, `months` or `years` that no group covers are listed
//! separately: the method identifies groups by mechanism, and these names are only a census.
#[path = "support/population_filter.rs"]
mod population_filter;

use pdx_native::internals::duration_groups;
use pdx_native::{Duration, Gap, GrammarProperty, Native};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const UNIT_NAMES: [&str; 3] = ["days", "months", "years"];

#[derive(Default)]
struct Population {
    counts: BTreeMap<&'static str, usize>,
    inventories: BTreeMap<&'static str, usize>,
    unclassified: BTreeMap<String, usize>,
    failure_shapes: BTreeMap<String, usize>,
    groups: Vec<Value>,
    unidentified: Vec<Value>,
    /// Groups that are not complete and that no `Duration keys <keys>: ` gap describes.
    unmatched: Vec<String>,
}

impl Population {
    fn group(&mut self, command: &str, duration: &Duration, gaps: &[Gap]) {
        let factors_known = duration
            .units
            .iter()
            .all(|unit| matches!(unit.factor, GrammarProperty::Known(_)));
        let status = match (&duration.combination, factors_known) {
            (GrammarProperty::Known(_), true)
                if matches!(duration.omitted_count, GrammarProperty::Known(_)) =>
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

        if status != "complete" && relevant.is_empty() {
            self.unmatched
                .push(format!("{command}: {}", keys.join(", ")));
        }

        for gap in &relevant {
            let shape = format!("{:?}: {}", gap.kind, &gap.detail[prefix.len()..]);
            *self.failure_shapes.entry(shape).or_default() += 1;
        }

        self.groups.push(json!({
            "command": command, "status": status, "duration": duration, "gaps": relevant,
        }));
    }
}

/// A registry group's value in the shape of `GrammarProperty`: `{"Known": value}`, or
/// `"Unresolved"` when the method stopped.
fn property<T: serde::Serialize, E>(result: &Result<T, E>) -> Value {
    match result {
        Ok(value) => json!({"Known": value}),
        Err(_) => json!("Unresolved"),
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
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        println!("{}", population_filter::USAGE);
        return Ok(());
    }
    let selection = population_filter::Selection::parse(args)?;
    let native = Native::open(std::env::var_os("STELLARIS_PATH").ok_or("set STELLARIS_PATH")?)?;
    let mut population = Population::default();
    let mut failed_questions = Vec::new();
    let mut commands = 0;

    for state in ["complete", "partial", "failed"] {
        population.counts.insert(state, 0);
    }

    selection.visit_commands(&native, |name, run| {
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

        let inventory = match &answer.value.durations {
            GrammarProperty::Known(_) => "known",
            GrammarProperty::Partial(_) => "partial",
            GrammarProperty::Unresolved => "unresolved",
        };
        *population.inventories.entry(inventory).or_default() += 1;

        for gap in answer
            .gaps
            .iter()
            .filter(|gap| gap.detail.starts_with("The code after a key's reader"))
        {
            *population
                .unclassified
                .entry(gap.detail.clone())
                .or_default() += 1;
        }

        for group in &groups {
            population.group(&name, group, &answer.gaps);
        }

        if !uncovered.is_empty() {
            population.unidentified.push(json!({
                "command": name, "keys": uncovered, "durations": answer.value.durations,
            }));
        }
    })?;

    if !population.unmatched.is_empty() {
        return Err(format!(
            "no gap starts with 'Duration keys <keys>: ' for these partial or failed groups, so the \
             gap prose in src/session/durations.rs changed: {}",
            population.unmatched.join("; ")
        )
        .into());
    }

    let registries = native.registries()?;
    let mut registry_groups = Vec::new();
    let mut registry_unresolved = Vec::new();

    let selected_registries = selection.registries(
        registries
            .value
            .iter()
            .map(|registry| registry.name.as_str()),
    )?;
    for registry in &selected_registries {
        match duration_groups::registry(&native, registry) {
            Ok(owners) => {
                for owner in owners {
                    for stop in &owner.inventory.unresolved {
                        registry_unresolved.push(json!({
                            "registry": registry, "path": owner.path, "reason": stop.reason,
                        }));
                    }

                    registry_groups.extend(owner.inventory.groups.iter().map(|group| {
                        json!({
                            "registry": registry,
                            "path": owner.path,
                            "units": group.units.iter().map(|unit| json!({
                                "key": unit.key, "factor": property(&unit.factor),
                            })).collect::<Vec<_>>(),
                            "combination": property(&group.combination),
                        })
                    }));
                }
            }
            Err(error) => {
                failed_questions.push(json!({"registry": registry, "error": error}));
            }
        }
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "build": native.build(),
            "commands": commands,
            "registries": selected_registries.len(),
            "counts": population.counts,
            "duration_lists": population.inventories,
            "unclassified_candidates": population.unclassified,
            "failure_shapes": population.failure_shapes,
            "groups": population.groups,
            "unidentified_unit_keys": population.unidentified,
            "registry_groups": registry_groups,
            "registry_unresolved_candidates": registry_unresolved,
            "failed_questions": failed_questions,
        }))?
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdx_native::internals::duration_groups::Combination;
    use pdx_native::{DurationUnit, GapKind};

    fn unresolved_days() -> Duration {
        Duration {
            units: vec![DurationUnit {
                key: "days".into(),
                factor: GrammarProperty::Unresolved,
            }],
            combination: GrammarProperty::Unresolved,
            omitted_count: GrammarProperty::Unresolved,
        }
    }

    fn gap(detail: &str) -> Gap {
        Gap {
            kind: GapKind::ReaderSemantics,
            subject: None,
            detail: detail.into(),
        }
    }

    #[test]
    fn an_unresolved_group_needs_a_gap_with_the_duration_keys_prefix() {
        let mut population = Population::default();

        population.group(
            "Effect/test",
            &unresolved_days(),
            &[gap(
                "Duration keys days: the combination is not established (call).",
            )],
        );
        assert!(population.unmatched.is_empty());

        population.group(
            "Effect/test",
            &unresolved_days(),
            &[gap(
                "Duration units days: the combination is not established (call).",
            )],
        );
        assert_eq!(population.unmatched, ["Effect/test: days"]);
    }

    #[test]
    fn a_complete_group_needs_no_gap() {
        let complete = Duration {
            units: vec![DurationUnit {
                key: "days".into(),
                factor: GrammarProperty::Known(Some(1)),
            }],
            combination: GrammarProperty::Known(pdx_native::DurationCombination::ScaledAtRead),
            omitted_count: GrammarProperty::Known(0),
        };
        let mut population = Population::default();

        population.group("Effect/test", &complete, &[]);

        assert!(population.unmatched.is_empty());
    }

    #[test]
    fn registry_group_values_are_properties_not_debug_text() {
        assert_eq!(property(&Ok::<_, ()>(Some(30))), json!({"Known": 30}));
        assert_eq!(
            property(&Ok::<Option<i64>, ()>(None)),
            json!({"Known": null})
        );
        assert_eq!(
            property(&Ok::<_, ()>(Combination::ScaledAtRead)),
            json!({"Known": "ScaledAtRead"})
        );
        assert_eq!(property(&Err::<Combination, _>(())), json!("Unresolved"));
    }
}
