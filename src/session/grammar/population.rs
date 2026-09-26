//! Apply the public grammar normalization to every registration using one input per inventory.
use super::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn state<T>(property: &GrammarProperty<T>) -> &'static str {
    match property {
        GrammarProperty::Known(_) => "known",
        GrammarProperty::Partial(_) => "partial",
        GrammarProperty::Unresolved => "unresolved",
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH; optionally writes NATIVE_GRAMMAR_REPORT"]
fn m45_command_grammar_population() {
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let started = std::time::Instant::now();
    let mut inventories = Vec::new();
    for (kind, controls) in [
        (
            DeclarationKind::Trigger,
            ["and", "or", "not", "if", "else_if", "else"],
        ),
        (
            DeclarationKind::Effect,
            [
                "if",
                "else_if",
                "else",
                "hidden_effect",
                "random_list",
                "every_owned_planet",
            ],
        ),
    ] {
        let (input, inventory) = native
            .declaration_analysis(Operation::CommandGrammar)
            .unwrap()
            .grammar_input(kind)
            .unwrap();
        let mut names = BTreeSet::new();
        let mut unnamed = 0;
        for (_, site) in &inventory.sites {
            match site {
                Site::Declared { name, .. }
                | Site::Unreadable {
                    name: Some(name), ..
                } => {
                    names.insert(name.clone());
                }
                _ => unnamed += 1,
            }
        }
        let mut counts = BTreeMap::<String, usize>::new();
        let mut failures = BTreeMap::<String, Vec<String>>::new();
        let mut cases = Vec::<Value>::new();
        for name in &names {
            let result = match registered_factory(&inventory, name) {
                Ok(Some(factory)) => grammar::analyze(&input, factory),
                Ok(None) => unreachable!("the name came from this inventory"),
                Err(stop) => Err(stop),
            };
            let answer = normalize(result.as_ref(), name, native.build());
            let group = if controls.contains(&name.as_str()) {
                "target_controls"
            } else {
                "other_commands"
            };
            *counts.entry(format!("{group}.total")).or_default() += 1;
            if answer.value.reader.id.is_some() {
                *counts
                    .entry(format!("{group}.reader_identity"))
                    .or_default() += 1;
            }
            *counts
                .entry(format!(
                    "{group}.reader_kind.{:?}",
                    answer.value.reader.kind
                ))
                .or_default() += 1;
            for (property, status) in [
                ("child_families", state(&answer.value.child_families)),
                ("fixed_keys", state(&answer.value.fixed_keys)),
                ("numeric_keys", state(&answer.value.numeric_keys)),
                ("ordering", state(&answer.value.ordering)),
            ] {
                *counts
                    .entry(format!("{group}.{property}.{status}"))
                    .or_default() += 1;
            }
            let reasons: BTreeSet<_> = answer.gaps.iter().map(|gap| gap.detail.clone()).collect();
            for reason in reasons {
                failures.entry(reason).or_default().push(name.clone());
            }
            cases.push(json!({"name": name, "group": group, "answer": answer}));
        }
        inventories.push(json!({
            "kind": kind, "registration_sites": inventory.sites.len(),
            "named_commands": names.len(), "unnamed_registrations": unnamed,
            "denominator": names.len() + unnamed,
            "counts": counts, "failures": failures, "cases": cases,
        }));
    }
    let report = json!({"build": native.build(), "method": grammar::METHOD,
        "elapsed_ms": started.elapsed().as_millis(), "inventories": inventories});
    if let Some(path) = std::env::var_os("NATIVE_GRAMMAR_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    for inventory in report["inventories"].as_array().unwrap() {
        eprintln!(
            "{}",
            json!({"kind": inventory["kind"], "denominator": inventory["denominator"],
            "unnamed": inventory["unnamed_registrations"], "counts": inventory["counts"]})
        );
    }
}
