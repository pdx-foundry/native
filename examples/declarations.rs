//! Print declarations recovered from one exact Stellaris installation.
use pdx_native::{Answer, DeclarationKind, Error, Native};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("pass a Stellaris installation path")?;
    let native = Native::open(path)?;
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        let answer = timed(&format!("{kind:?}"), || native.declarations(kind), Vec::len)?;
        for declaration in answer.value.iter().take(5) {
            println!(
                "  {}: {} ({:?})",
                declaration.name, declaration.description, declaration.scopes
            );
        }
    }

    let modifiers = timed("Modifiers", || native.modifiers(), Vec::len)?;
    for modifier in modifiers.value.iter().take(5) {
        println!("  {}: {:?}", modifier.name, modifier.category_tags);
    }
    let categories = timed(
        "Modifier categories",
        || native.modifier_categories(),
        Vec::len,
    )?;
    let names: Vec<_> = categories
        .value
        .iter()
        .map(|category| &category.name)
        .collect();
    println!("  {names:?}");
    let families = timed(
        "Building modifier families",
        || native.modifier_families("common/buildings"),
        Vec::len,
    )?;
    for family in &families.value {
        let example = family.name_for("building_foundry");
        println!("  {example:?}: {:?}", family.category_tags);
    }
    let scopes = timed("Scopes", || native.scopes(), |scopes| scopes.types.len())?;
    for scope in scopes.value.types.iter().take(5) {
        println!("  {}: {:?}", scope.name, scope.keywords);
    }
    for group in &scopes.value.groups {
        println!("  {} matches any of {:?}", group.keyword, group.scopes);
    }
    let links = timed("Scope links", || native.scope_links(), Vec::len)?;
    for link in links.value.iter().take(5) {
        println!(
            "  {}: {:?} -> {:?} ({:?})",
            link.name, link.input_scopes, link.output_scope, link.data
        );
    }
    let localization = timed(
        "Localization",
        || native.localization_declarations(),
        |localization| localization.commands.len() + localization.links.len(),
    )?;
    for context in localization.value.contexts.iter().take(5) {
        println!("  context {}: {:?}", context.name, context.scopes);
    }
    for link in localization.value.links.iter().take(5) {
        let inputs: Vec<_> = link
            .input_contexts
            .iter()
            .map(|input| &input.name)
            .collect();
        println!("  {}: {inputs:?} -> {:?}", link.name, link.output);
    }
    let on_actions = timed("On_actions", || native.on_actions(), Vec::len)?;
    for on_action in on_actions.value.iter().take(5) {
        println!("  {}: {:?}", on_action.name, on_action.entries);
    }
    let game_rules = timed("Game rules", || native.game_rules(), Vec::len)?;
    for rule in game_rules.value.iter().take(5) {
        println!("  {} ({:?}): {:?}", rule.name, rule.kind, rule.entries);
    }
    Ok(())
}

fn timed<T>(
    label: &str,
    question: impl FnOnce() -> Result<Answer<T>, Error>,
    count: impl Fn(&T) -> usize,
) -> Result<Answer<T>, Error> {
    let started = std::time::Instant::now();
    let answer = question()?;
    println!(
        "{label}: {} values, {:?}, {} gaps, {:.2?}",
        count(&answer.value),
        answer.completeness,
        answer.gaps.len(),
        started.elapsed()
    );
    Ok(answer)
}
