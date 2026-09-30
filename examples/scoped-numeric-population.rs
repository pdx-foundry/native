//! Measure scoped numeric answers over every discovered registry field and every argument of
//! every registered command.
use pdx_native::internals::{command_grammar_stops, registry_field_stops};
use pdx_native::{
    CommandForm, CommandGrammar, DeclarationKind, Field, FieldMembers, FieldReadOutcome, Gap,
    GapSubject, GrammarProperty, Native, Reader, ReaderKind,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
struct Population {
    counts: BTreeMap<&'static str, usize>,
    /// Destinations by established storage, which selects the engine's evaluation body.
    storage: BTreeMap<String, usize>,
    failure_shapes: BTreeMap<String, usize>,
    destinations: Vec<Value>,
}

fn established<T>(property: &GrammarProperty<T>) -> Option<&T> {
    match property {
        GrammarProperty::Known(value) | GrammarProperty::Partial(value) => Some(value),
        GrammarProperty::Unresolved => None,
    }
}

fn storage(reader: &Reader) -> String {
    let numeric = established(&reader.numeric).and_then(Option::as_ref);
    let width = numeric.and_then(|numeric| established(&numeric.width_bits));
    let scale = numeric.and_then(|numeric| established(&numeric.scale).copied().flatten());

    match (width, scale) {
        (Some(width), Some(scale)) => format!("{width}-bit, scale {scale}"),
        _ => "unresolved".to_owned(),
    }
}

impl Population {
    fn new() -> Self {
        let mut population = Self::default();

        for state in ["complete", "partial", "failed"] {
            population.counts.insert(state, 0);
        }

        population
    }

    fn report(&self) -> Value {
        json!({
            "counts": self.counts, "storage": self.storage,
            "failure_shapes": self.failure_shapes, "destinations": self.destinations,
        })
    }

    fn reader(&mut self, owner: &str, path: &[String], reader: &Reader, gaps: &[Gap]) {
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
        *self.storage.entry(storage(reader)).or_default() += 1;
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
        self.destinations.push(
            json!({"owner":owner,"path":path,"status":state,"reader":reader,"gaps":relevant}),
        );
    }

    fn fields(&mut self, owner: &str, values: &[Field], parent: &[String], gaps: &[Gap]) {
        for field in values {
            let mut path = parent.to_vec();
            path.push(field.name.clone());
            self.reader(owner, &path, &field.reader, gaps);
            if field.reader.kind != ReaderKind::ScopedNumeric {
                let mut seen = Vec::new();
                for alternative in &field.read {
                    if let FieldReadOutcome::Read { reader, .. } = &alternative.outcome
                        && !seen.contains(&reader)
                    {
                        self.reader(owner, &path, reader, gaps);
                        seen.push(reader);
                    }
                }
            }
            if let FieldMembers::Fields(children) = &field.members {
                self.fields(owner, children, &path, gaps);
            }
        }
    }

    /// The value forms, fixed keys and integer-keyed children of one command.
    fn command(&mut self, owner: &str, grammar: &CommandGrammar, gaps: &[Gap]) {
        for form in established(&grammar.forms).into_iter().flatten() {
            if let CommandForm::Value(value) = form {
                self.reader(owner, &[], &value.reader, gaps);
            }
        }

        if let Some(keys) = established(&grammar.fixed_keys) {
            self.fields(owner, keys, &[], gaps);
        }

        if let Some(Some(children)) = established(&grammar.numeric_keys) {
            self.command(owner, children, gaps);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let native = Native::open(std::env::var_os("STELLARIS_PATH").ok_or("set STELLARIS_PATH")?)?;
    let registries = native.registries()?;
    let mut fields = Population::new();
    let mut failed_questions = Vec::new();

    for registry in &registries.value {
        match registry_field_stops::run(&native, &registry.name) {
            Ok(run) => fields.fields(&registry.name, &run.answer.value, &[], &run.answer.gaps),
            Err(error) => failed_questions.push(json!({"registry":registry.name,"error":error})),
        }
    }

    let mut arguments = Population::new();
    let mut commands = 0;

    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        command_grammar_stops::population(&native, kind, |name, run| {
            let owner = format!("{kind:?}/{name}");
            commands += 1;
            arguments.command(&owner, &run.answer.value, &run.answer.gaps);
        })?;
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "build":native.build(),"registries":registries.value.len(),
            "registry_completeness":registries.completeness,"commands":commands,
            "fields":fields.report(),"arguments":arguments.report(),
            "failed_questions":failed_questions
        }))?
    );
    Ok(())
}
