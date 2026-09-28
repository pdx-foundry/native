//! Record every effect and trigger grammar of one installation, or verify a recording.
//!
//! `record-command-grammars INSTALLATION [DIRECTORY]` asks `declarations` for both kinds and
//! `command_grammar` for each declared name, and records the answers. The default directory is
//! `.local/sdk-548/recorded-answers`.
//!
//! `record-command-grammars --verify DIRECTORY INSTALLATION` reads each answer again through
//! `Native::from_recorded_answers` and compares it with a new static answer, apart from `Basis`.
use pdx_native::{Answer, CommandGrammar, DeclarationKind, Native};
use std::time::Instant;

const DEFAULT_DIRECTORY: &str = ".local/sdk-548/recorded-answers";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let started = Instant::now();
    let count = match args.as_slice() {
        [flag, directory, installation] if flag == "--verify" => verify(directory, installation)?,
        [installation] => record(installation, DEFAULT_DIRECTORY)?,
        [installation, directory] => record(installation, directory)?,
        _ => {
            return Err(
                "usage: record-command-grammars INSTALLATION [DIRECTORY] | --verify DIRECTORY INSTALLATION"
                    .into(),
            );
        }
    };
    eprintln!(
        "{count} grammars in {:.1} s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn record(installation: &str, directory: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let native = Native::open(installation)?.record_answers_to(directory);
    let mut count = 0;
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        for declaration in native.declarations(kind)?.value {
            native.command_grammar(kind, &declaration.name)?;
            count += 1;
        }
    }
    Ok(count)
}

fn verify(directory: &str, installation: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let recorded = Native::from_recorded_answers(directory)?;
    let native = Native::open(installation)?;
    let mut failures = Vec::new();
    let mut count = 0;
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        let declarations = recorded.declarations(kind)?;
        if !same_apart_from_basis(&declarations, &native.declarations(kind)?) {
            failures.push(format!("{kind:?} declarations"));
        }
        for declaration in declarations.value {
            let name = &declaration.name;
            let answer: Answer<CommandGrammar> = recorded.command_grammar(kind, name)?;
            if !same_apart_from_basis(&answer, &native.command_grammar(kind, name)?) {
                failures.push(format!("{kind:?}/{name}"));
            }
            count += 1;
        }
    }
    if failures.is_empty() {
        Ok(count)
    } else {
        Err(format!("{} answers differ: {}", failures.len(), failures.join(", ")).into())
    }
}

fn same_apart_from_basis<T: PartialEq>(recorded: &Answer<T>, analysed: &Answer<T>) -> bool {
    let mut source = recorded.source.clone();
    source.basis = analysed.source.basis;
    recorded.value == analysed.value
        && recorded.completeness == analysed.completeness
        && recorded.gaps == analysed.gaps
        && source == analysed.source
}
