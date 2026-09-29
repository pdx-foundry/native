//! Measure scoped numeric answers over every discovered registry field.
use pdx_native::internals::registry_field_stops;
use pdx_native::{
    Field, FieldMembers, FieldReadOutcome, Gap, GapSubject, GrammarProperty, Native, Reader,
    ReaderKind,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
struct Population {
    counts: BTreeMap<&'static str, usize>,
    failure_shapes: BTreeMap<String, usize>,
    fields: Vec<Value>,
}

impl Population {
    fn reader(&mut self, registry: &str, path: &[String], reader: &Reader, gaps: &[Gap]) {
        if reader.kind != ReaderKind::ScopedNumeric {
            return;
        }
        let state = match (&reader.numeric, &reader.scoped_operand) {
            (GrammarProperty::Known(Some(_)), GrammarProperty::Known(Some(operand)))
                if matches!(operand.forms, GrammarProperty::Known(_))
                    && matches!(operand.selection, GrammarProperty::Known(_)) =>
            {
                "complete"
            }
            (GrammarProperty::Known(Some(_)) | GrammarProperty::Partial(Some(_)), _) => "partial",
            _ => "failed",
        };
        *self.counts.entry(state).or_default() += 1;
        let relevant: Vec<_> = gaps
            .iter()
            .filter(|gap| match &gap.subject {
                Some(GapSubject::Field { name }) => path.len() == 1 && name == &path[0],
                Some(GapSubject::KeyPath { path: subject }) => subject == path,
                _ => false,
            })
            .collect();
        for gap in &relevant {
            *self
                .failure_shapes
                .entry(format!("{:?}: {}", gap.kind, gap.detail))
                .or_default() += 1;
        }
        self.fields.push(
            json!({"registry":registry,"path":path,"status":state,"reader":reader,"gaps":relevant}),
        );
    }

    fn fields(&mut self, registry: &str, values: &[Field], parent: &[String], gaps: &[Gap]) {
        for field in values {
            let mut path = parent.to_vec();
            path.push(field.name.clone());
            self.reader(registry, &path, &field.reader, gaps);
            if field.reader.kind != ReaderKind::ScopedNumeric {
                let mut seen = Vec::new();
                for alternative in &field.read {
                    if let FieldReadOutcome::Read { reader, .. } = &alternative.outcome
                        && !seen.contains(&reader)
                    {
                        self.reader(registry, &path, reader, gaps);
                        seen.push(reader);
                    }
                }
            }
            if let FieldMembers::Fields(children) = &field.members {
                self.fields(registry, children, &path, gaps);
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let native = Native::open(std::env::var_os("STELLARIS_PATH").ok_or("set STELLARIS_PATH")?)?;
    let registries = native.registries()?;
    let mut population = Population::default();
    for state in ["complete", "partial", "failed"] {
        population.counts.insert(state, 0);
    }
    let mut failed_questions = Vec::new();
    for registry in &registries.value {
        match registry_field_stops::run(&native, &registry.name) {
            Ok(run) => population.fields(&registry.name, &run.answer.value, &[], &run.answer.gaps),
            Err(error) => failed_questions.push(json!({"registry":registry.name,"error":error})),
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "build":native.build(),"registries":registries.value.len(),
            "registry_completeness":registries.completeness,"counts":population.counts,
            "failure_shapes":population.failure_shapes,"fields":population.fields,"failed_questions":failed_questions
        }))?
    );
    Ok(())
}
