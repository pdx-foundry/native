//! Print declarations recovered from one exact Stellaris installation.
use pdx_native::{DeclarationKind, Native};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("pass a Stellaris installation path")?;
    let native = Native::open(path)?;
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        let started = std::time::Instant::now();
        let answer = native.declarations(kind)?;
        println!(
            "{kind:?}: {} names, {:?}, {} gaps, {:.2?}",
            answer.value.len(),
            answer.completeness,
            answer.gaps.len(),
            started.elapsed()
        );
        for declaration in answer.value.iter().take(5) {
            println!(
                "{}: {} ({:?})",
                declaration.name, declaration.description, declaration.scopes
            );
        }
    }
    Ok(())
}
