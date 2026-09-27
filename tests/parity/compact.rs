//! Compact views shared by parity assertions and candidate generation.
use pdx_native::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn entry(context: &EntryContext) -> String {
    let scope = |scope: &EntryScope| match scope {
        EntryScope::Scope(reference) => reference.name.clone(),
        other => format!("{other:?}"),
    };
    let from: Vec<_> = context.from.iter().map(scope).collect();
    format!(
        "this={} root={} from=[{}]",
        scope(&context.this),
        scope(&context.root),
        from.join(",")
    )
}

pub fn compact_on_actions(answer: &Answer<Vec<OnAction>>) -> Value {
    compact_callbacks(answer, |on_action| {
        let entries: Vec<_> = on_action.entries.iter().map(entry).collect();
        (on_action.name.clone(), json!(entries))
    })
}

pub fn compact_game_rules(answer: &Answer<Vec<GameRule>>) -> Value {
    compact_callbacks(answer, |rule| {
        let entries: Vec<_> = rule.entries.iter().map(entry).collect();
        (rule.name.clone(), json!([rule.kind, entries]))
    })
}

pub fn compact_callbacks<T>(
    answer: &Answer<Vec<T>>,
    name_and_value: impl Fn(&T) -> (String, Value),
) -> Value {
    let names: serde_json::Map<_, _> = answer.value.iter().map(name_and_value).collect();
    let gaps: Vec<_> = answer
        .gaps
        .iter()
        .map(|gap| json!([gap.kind, gap.subject, gap.detail]))
        .collect();

    json!({
        "completeness": answer.completeness,
        "names": names,
        "gaps": gaps,
    })
}

pub fn compact_localization(answer: &Answer<LocalizationDeclarations>) -> Value {
    let localization = &answer.value;
    let contexts: serde_json::Map<_, _> = localization
        .contexts
        .iter()
        .map(|context| {
            let scopes = match &context.scopes {
                ContextScopes::Joined(scopes) => json!({ "Joined": reference_names(scopes) }),
                ContextScopes::Partial(scopes) => json!({ "Partial": reference_names(scopes) }),
                ContextScopes::Missing => json!("Missing"),
            };
            (
                context.name.clone(),
                json!({ "id": context.id, "scopes": scopes }),
            )
        })
        .collect();
    let commands: serde_json::Map<_, _> = localization
        .commands
        .iter()
        .map(|command| {
            (
                command.name.clone(),
                json!(context_names(&command.contexts)),
            )
        })
        .collect();
    let links: Vec<_> = localization
        .links
        .iter()
        .map(|link| {
            let output = match &link.output {
                LocalizationOutput::Listed(outputs) => json!({ "Listed": context_names(outputs) }),
                other => json!(other),
            };
            json!([link.name, context_names(&link.input_contexts), output])
        })
        .collect();
    let gaps: Vec<_> = answer
        .gaps
        .iter()
        .map(|gap| json!([gap.kind, gap.subject, gap.detail]))
        .collect();

    json!({
        "completeness": answer.completeness,
        "contexts": contexts,
        "commands": commands,
        "links": links,
        "gaps": gaps,
    })
}

pub fn context_names(references: &[LocalizationContextReference]) -> Vec<&str> {
    references
        .iter()
        .map(|reference| reference.name.as_str())
        .collect()
}

pub fn reference_names(scopes: &[ScopeReference]) -> Vec<&str> {
    scopes.iter().map(|scope| scope.name.as_str()).collect()
}

pub fn compact_families(answer: &Answer<Vec<ModifierFamily>>) -> Value {
    let families: Vec<_> = answer
        .value
        .iter()
        .map(|family| {
            let template: String = family
                .name
                .iter()
                .map(|part| match part {
                    NamePart::Literal(text) => text.as_str(),
                    NamePart::ItemKey => "{key}",
                    other => panic!("unexpected part {other:?}"),
                })
                .collect();
            json!({
                "template": template,
                "category_tags": family.category_tags,
                "condition": family.condition,
                "name_limit": family.name_limit,
            })
        })
        .collect();
    let gaps: Vec<_> = answer
        .gaps
        .iter()
        .filter(|gap| gap.kind != GapKind::OutsideMethod)
        .map(|gap| json!([gap.kind, gap.subject, gap.detail]))
        .collect();
    json!({ "families": families, "gaps": gaps })
}

pub fn gap_counts<T>(answer: &Answer<Vec<T>>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for gap in &answer.gaps {
        *counts.entry(format!("{:?}", gap.kind)).or_default() += 1;
    }
    counts
}

pub fn command(owner: &str) -> Option<(DeclarationKind, &str)> {
    if let Some(name) = owner.strip_prefix("effect/") {
        return Some((DeclarationKind::Effect, name));
    }

    owner
        .strip_prefix("trigger/")
        .map(|name| (DeclarationKind::Trigger, name))
}

pub fn compact_namespace(namespace: &pdx_native::DynamicNamespace) -> Value {
    let owner = match &namespace.owner {
        pdx_native::NamespaceOwner::Global => "global".to_owned(),
        pdx_native::NamespaceOwner::Scope(scope) => scope.name.clone(),
    };
    let commands = |commands: &[pdx_native::CommandReference]| -> Vec<String> {
        commands
            .iter()
            .map(|command| format!("{:?} {}", command.kind, command.name))
            .collect()
    };
    json!({
        "owner": owner,
        "defined_by": commands(&namespace.defined_by),
        "removed_by": commands(&namespace.removed_by),
        "read_by": commands(&namespace.read_by),
        "dynamic_form": format!("{:?}", namespace.dynamic_form),
    })
}
