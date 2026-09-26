//! Print static grammar facts for one registered command.
use pdx_native::{DeclarationKind, Native};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [installation, kind, name] = args.as_slice() else {
        return Err("usage: command-grammar <installation> <trigger|effect> <name>".into());
    };
    let kind = match kind.as_str() {
        "trigger" => DeclarationKind::Trigger,
        "effect" => DeclarationKind::Effect,
        _ => return Err("kind must be trigger or effect".into()),
    };
    let answer = Native::open(installation)?.command_grammar(kind, name)?;
    println!("{}", serde_json::to_string_pretty(&answer)?);
    Ok(())
}
