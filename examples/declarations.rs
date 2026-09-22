//! Print declarations recovered from one exact Stellaris installation.
use pdx_native::{Answer, DeclarationKind, Error, Native};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("pass a Stellaris installation path")?;
    let native = Native::open(path)?;
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        let answer = timed(&format!("{kind:?}"), || native.declarations(kind))?;
        for declaration in answer.value.iter().take(5) {
            println!(
                "  {}: {} ({:?})",
                declaration.name, declaration.description, declaration.scopes
            );
        }
    }

    let modifiers = timed("Modifiers", || native.modifiers())?;
    for modifier in modifiers.value.iter().take(5) {
        println!("  {}: {:?}", modifier.name, modifier.category_tags);
    }
    let categories = timed("Modifier categories", || native.modifier_categories())?;
    let names: Vec<_> = categories
        .value
        .iter()
        .map(|category| &category.name)
        .collect();
    println!("  {names:?}");
    let scopes = timed("Scopes", || native.scopes())?;
    for scope in scopes.value.iter().take(5) {
        println!("  {}: {:?}", scope.name, scope.keywords);
    }
    let links = timed("Scope links", || native.scope_links())?;
    for link in links.value.iter().take(5) {
        println!(
            "  {}: {:?} -> {:?} ({:?})",
            link.name, link.input_scopes, link.output_scope, link.data
        );
    }
    Ok(())
}

fn timed<T>(
    label: &str,
    question: impl FnOnce() -> Result<Answer<Vec<T>>, Error>,
) -> Result<Answer<Vec<T>>, Error> {
    let started = std::time::Instant::now();
    let answer = question()?;
    println!(
        "{label}: {} values, {:?}, {} gaps, {:.2?}",
        answer.value.len(),
        answer.completeness,
        answer.gaps.len(),
        started.elapsed()
    );
    Ok(answer)
}
