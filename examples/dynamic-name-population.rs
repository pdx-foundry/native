//! Run the dynamic-name method over every effect and trigger and print its population counts:
//! commands by outcome, flag commands by role, failure shapes, and each namespace with its
//! commands. The counts are distinct commands; one command can have several failure shapes.
//!
//! usage: dynamic-name-population <installation>
use std::collections::{BTreeMap, BTreeSet};

use pdx_native::internals::dynamic_name_commands::{self, NameOutcome};
use pdx_native::{NamespaceOwner, Native};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [installation] = args.as_slice() else {
        return Err("usage: dynamic-name-population <installation>".into());
    };
    let native = Native::open(installation)?;
    let started = std::time::Instant::now();
    let commands = dynamic_name_commands::run(&native)?;
    println!("method run: {:?}", started.elapsed());

    let mut outcomes = BTreeMap::<(String, &str), usize>::new();
    let mut roles = BTreeMap::<(String, String), BTreeSet<String>>::new();
    let mut shapes = BTreeMap::<(String, String), BTreeSet<String>>::new();
    let mut without_role = BTreeSet::new();
    for command in &commands {
        let kind = format!("{:?}", command.kind);
        let outcome = match &command.outcome {
            NameOutcome::NotFlag => "not a flag command",
            NameOutcome::NotExamined(stop) => {
                shapes
                    .entry((kind.clone(), stop.reason.into()))
                    .or_default()
                    .insert(command.name.clone());
                "not examined"
            }
            NameOutcome::Unresolved(stop) => {
                shapes
                    .entry((kind.clone(), stop.reason.into()))
                    .or_default()
                    .insert(command.name.clone());
                "flag name without a stored index"
            }
            NameOutcome::Flag(flag) => {
                for stop in &flag.stops {
                    shapes
                        .entry((kind.clone(), stop.reason.into()))
                        .or_default()
                        .insert(command.name.clone());
                }
                for role_use in &flag.uses {
                    roles
                        .entry((kind.clone(), format!("{:?}", role_use.role)))
                        .or_default()
                        .insert(command.name.clone());
                    if let Err(stop) = &role_use.route {
                        shapes
                            .entry((kind.clone(), format!("route {}", stop.reason)))
                            .or_default()
                            .insert(format!("{} in {}", command.name, role_use.scope.name));
                    }
                }
                if flag.uses.is_empty() {
                    without_role.insert(format!("{kind} {}", command.name));
                }
                "flag command"
            }
        };
        *outcomes.entry((kind, outcome)).or_default() += 1;
    }

    println!("\n== commands by outcome ==");
    for ((kind, outcome), count) in &outcomes {
        println!("{kind:8} {outcome:32} {count}");
    }
    println!("\n== flag commands by role ==");
    for ((kind, role), names) in &roles {
        println!("{kind:8} {role:8} {}", names.len());
    }
    println!("\n== flag commands without an established role or scope set ==");
    for name in &without_role {
        println!("{name}");
    }
    println!("\n== failure shapes (distinct commands) ==");
    for ((kind, reason), names) in &shapes {
        let sample: Vec<_> = names.iter().take(6).map(String::as_str).collect();
        println!(
            "{kind:8} {reason:28} {:4}  {}",
            names.len(),
            sample.join(", ")
        );
    }

    let answer = native.dynamic_names()?;
    println!(
        "\n== namespaces: {} ({:?}) ==",
        answer.value.len(),
        answer.completeness
    );
    for namespace in &answer.value {
        let owner = match &namespace.owner {
            NamespaceOwner::Global => "global".to_owned(),
            NamespaceOwner::Scope(scope) => scope.name.clone(),
        };
        let names = |commands: &[pdx_native::CommandReference]| {
            commands
                .iter()
                .map(|command| command.name.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        println!(
            "{owner} {:?} {:?}\n  defined: {}\n  removed: {}\n  read: {}",
            namespace.id,
            namespace.dynamic_form,
            names(&namespace.defined_by),
            names(&namespace.removed_by),
            names(&namespace.read_by)
        );
    }
    Ok(())
}
