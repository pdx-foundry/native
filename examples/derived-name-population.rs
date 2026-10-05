//! Run the derived-name method over every registry and print its population counts: answers by
//! completeness, gap shapes, entries by lookup target, stage, miss behavior and condition, and
//! each registry's names. Counts are operation answers per registry, gaps or answer entries, as
//! each section says.
//!
//! usage: derived-name-population <installation>
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use pdx_native::{
    Answer, Completeness, DerivedName, FieldCondition, Gap, GapKind, MissingName, NamePart, Native,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [installation] = args.as_slice() else {
        return Err("usage: derived-name-population <installation>".into());
    };
    let native = Native::open(installation)?;
    let registries = native.registries()?.value;

    let started = Instant::now();
    let runs: Vec<Run> = registries
        .iter()
        .map(|registry| {
            let started = Instant::now();
            let answer = native.derived_names(&registry.name);

            Run {
                registry: registry.name.clone(),
                elapsed: started.elapsed(),
                answer,
            }
        })
        .collect();
    println!("method run: {:?}", started.elapsed());

    print_outcomes(&runs);
    print_gap_shapes(&runs);
    print_entry_counts(&runs);
    print_registries(&runs);
    print_slowest(&runs);
    Ok(())
}

struct Run {
    registry: String,
    elapsed: Duration,
    answer: Result<Answer<Vec<DerivedName>>, pdx_native::Error>,
}

impl Run {
    fn names(&self) -> &[DerivedName] {
        self.answer
            .as_ref()
            .map_or(&[], |answer| answer.value.as_slice())
    }
}

fn print_outcomes(runs: &[Run]) {
    let mut outcomes = BTreeMap::<&str, usize>::new();

    for run in runs {
        let outcome = match &run.answer {
            Ok(answer) if answer.completeness == Completeness::Complete => "complete",
            Ok(_) => "partial",
            Err(_) => "failed",
        };
        *outcomes.entry(outcome).or_default() += 1;
    }

    println!("\n== answers by outcome ({} registries) ==", runs.len());
    for (outcome, count) in &outcomes {
        println!("{outcome:10} {count}");
    }

    let with_names = runs.iter().filter(|run| !run.names().is_empty()).count();
    println!("{:10} {with_names}", "with names");

    for run in runs {
        if let Err(error) = &run.answer {
            println!("failed     {}: {error}", run.registry);
        }
    }
}

/// Group gaps by kind and detail, with counts and each name template made generic.
fn print_gap_shapes(runs: &[Run]) {
    let mut shapes = BTreeMap::<(String, String), (usize, BTreeSet<&str>)>::new();

    for run in runs {
        let Ok(answer) = &run.answer else { continue };

        for gap in &answer.gaps {
            let (gaps, registries) = shapes
                .entry((format!("{:?}", gap.kind), gap_shape(gap)))
                .or_default();
            *gaps += 1;
            registries.insert(&run.registry);
        }
    }

    println!("\n== gap shapes (gaps, registries) ==");
    for ((kind, shape), (gaps, registries)) in &shapes {
        let sample: Vec<_> = registries.iter().take(4).copied().collect();
        println!(
            "{kind:20} {gaps:5} {:4}  {shape}\n{:32}{}",
            registries.len(),
            "",
            sample.join(", ")
        );
    }
}

/// The detail with a name subject written as `NAME` and each number as `N`.
fn gap_shape(gap: &Gap) -> String {
    let detail = match &gap.subject {
        Some(subject) if gap.kind != GapKind::OutsideMethod && subject.name().contains('{') => {
            gap.detail.replace(subject.name(), "NAME")
        }
        _ => gap.detail.clone(),
    };

    detail
        .split(' ')
        .map(|word| {
            if word.chars().all(|character| character.is_ascii_digit()) {
                "N"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn print_entry_counts(runs: &[Run]) {
    let names: Vec<&DerivedName> = runs.iter().flat_map(Run::names).collect();
    let mut by_kind = BTreeMap::<(String, String, &str), usize>::new();
    let mut by_condition = BTreeMap::<&str, usize>::new();

    for name in &names {
        let kind = (
            format!("{:?}", name.lookup),
            format!("{:?}", name.stage),
            miss_kind(&name.on_missing),
        );
        *by_kind.entry(kind).or_default() += 1;
        *by_condition
            .entry(condition_kind(&name.condition))
            .or_default() += 1;
    }

    println!(
        "\n== entries by lookup, stage and miss behavior ({}) ==",
        names.len()
    );
    for ((lookup, stage, miss), count) in &by_kind {
        println!("{lookup:13} {stage:20} {miss:10} {count}");
    }

    println!("\n== entries by condition ==");
    for (condition, count) in &by_condition {
        println!("{condition:24} {count}");
    }
}

fn miss_kind(miss: &MissingName) -> &'static str {
    match miss {
        MissingName::ShowsKey => "ShowsKey",
        MissingName::Silent => "Silent",
        MissingName::Diagnostic => "Diagnostic",
        MissingName::Fallback(_) => "Fallback",
        _ => "Unresolved",
    }
}

fn condition_kind(condition: &FieldCondition) -> &'static str {
    match condition {
        FieldCondition::Always => "Always",
        FieldCondition::Unresolved => "Unresolved",
        FieldCondition::All(terms)
            if terms.iter().any(|term| *term != FieldCondition::Unresolved) =>
        {
            "All with field terms"
        }
        _ => "other",
    }
}

fn print_registries(runs: &[Run]) {
    println!("\n== registries with names ==");
    for run in runs {
        let Ok(answer) = &run.answer else { continue };
        if answer.value.is_empty() {
            continue;
        }

        println!(
            "{} {:?} {:?}",
            run.registry, answer.completeness, run.elapsed
        );
        for name in &answer.value {
            println!(
                "  {} {:?} {:?} {} {}",
                template(&name.name),
                name.lookup,
                name.stage,
                miss_text(&name.on_missing),
                serde_json::to_string(&name.condition).expect("a condition serializes")
            );
        }
    }
}

fn miss_text(miss: &MissingName) -> String {
    match miss {
        MissingName::Fallback(name) => format!("Fallback({})", template(name)),
        other => miss_kind(other).to_owned(),
    }
}

fn template(name: &[NamePart]) -> String {
    name.iter()
        .map(|part| match part {
            NamePart::Literal(text) => text.clone(),
            NamePart::ItemKey => "{key}".into(),
            NamePart::Field(path) => format!("{{{}}}", path.join("/")),
            other => panic!("unexpected part {other:?}"),
        })
        .collect()
}

fn print_slowest(runs: &[Run]) {
    let mut runs: Vec<&Run> = runs.iter().collect();
    runs.sort_by_key(|run| std::cmp::Reverse(run.elapsed));

    println!("\n== slowest registries ==");
    for run in runs.iter().take(10) {
        println!("{:40} {:?}", run.registry, run.elapsed);
    }
}
