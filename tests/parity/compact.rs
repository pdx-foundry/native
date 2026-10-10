//! Compact views shared by parity assertions and candidate generation.
use pdx_native::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Field selections refer to the separately reviewed shared modifier grammar by reader identity.
pub fn compact_fields(fields: &[Field]) -> Value {
    json!(
        fields
            .iter()
            .map(|field| {
                let mut value = json!(field);
                match &field.members {
                    FieldMembers::ModifierBlock(_) => {
                        value["members"] = json!({"ModifierBlock": field.reader.id})
                    }
                    FieldMembers::WeightBlock(_) => {
                        value["members"] = json!({"WeightBlock": field.reader.id})
                    }
                    FieldMembers::TriggeredModifier(_) => {
                        value["members"] = json!({"TriggeredModifier": field.reader.id})
                    }
                    FieldMembers::Fields(children) => {
                        value["members"] = json!({"Fields": compact_fields(children)})
                    }
                    _ => {}
                }
                value
            })
            .collect::<Vec<_>>()
    )
}

/// One representative of each constructor-bound modifier reader variant.
pub fn modifier_blocks(native: &Native) -> super::Result<Value> {
    let mut variants = BTreeMap::new();
    for registry in [
        "common/governments/councilors",
        "common/council_agendas",
        "common/megastructures",
        "common/traditions",
    ] {
        let answer = native.registry_fields(registry)?;
        let field = answer
            .value
            .iter()
            .find(|field| field.reader.family == BlockFamily::Modifier)
            .ok_or("modifier sample missing")?;
        let FieldMembers::ModifierBlock(block) = &field.members else {
            return Err(format!("{registry}: modifier grammar missing").into());
        };
        let keys = |fields: &[Field]| {
            fields
                .iter()
                .map(|field| json!([field.name, field.reader.kind]))
                .collect::<Vec<_>>()
        };
        let fixed_keys = match &block.fixed_keys {
            GrammarProperty::Known(fields) => json!({"Known": keys(fields)}),
            GrammarProperty::Partial(fields) => json!({"Partial": keys(fields)}),
            GrammarProperty::Unresolved => json!("Unresolved"),
        };
        let gaps: Vec<_> = answer
            .gaps
            .iter()
            .filter(|gap| match &gap.subject {
                Some(GapSubject::Field { name }) => name == &field.name,
                Some(GapSubject::KeyPath { path }) => path.first() == Some(&field.name),
                _ => false,
            })
            .map(|gap| json!([gap.kind, gap.subject, gap.detail]))
            .collect();
        let id = serde_json::to_value(&field.reader.id)?
            .as_str()
            .ok_or("missing modifier identity")?
            .to_owned();
        variants.insert(
            id,
            json!({"fixed_keys":fixed_keys,"entries":block.entries,"gaps":gaps}),
        );
    }
    Ok(json!(variants))
}

/// One representative of each constructor-bound weight reader variant.
pub fn weight_blocks(native: &Native) -> super::Result<Value> {
    let mut variants = BTreeMap::new();
    for (registry, name) in [
        ("common/council_agendas", "ai_weight"),
        ("common/country_customization", "weight"),
    ] {
        let answer = native.registry_fields(registry)?;
        let field = answer
            .value
            .iter()
            .find(|field| field.name == name)
            .ok_or("weight sample missing")?;
        let FieldMembers::WeightBlock(block) = &field.members else {
            return Err(format!("{registry}: weight grammar missing").into());
        };
        let gaps: Vec<_> = answer
            .gaps
            .iter()
            .filter(|gap| match &gap.subject {
                Some(GapSubject::Field { name }) => name == &field.name,
                Some(GapSubject::KeyPath { path }) => path.first() == Some(&field.name),
                _ => false,
            })
            .map(|gap| json!([gap.kind, gap.subject, gap.detail]))
            .collect();
        let id = identity(field)?;
        let mut entries = BTreeMap::new();
        let block = compact_weight(block, &mut entries)?;
        variants.insert(
            id,
            json!({
                "sample": [registry, name],
                "read_scope": field.read_scope,
                "block": block,
                "gaps": gaps,
            }),
        );
        for (id, entry) in entries {
            variants
                .entry(id)
                .or_insert_with(|| json!({ "entry_of": [registry, name], "block": entry }));
        }
    }
    Ok(json!(variants))
}

/// One representative of each triggered modifier clause variant. A modifier block names its
/// reader identity; its grammar is in `modifier-blocks.json`.
pub fn triggered_modifiers(native: &Native) -> super::Result<Value> {
    let mut variants = BTreeMap::new();
    for (registry, name) in [
        ("common/megastructures", "triggered_country_modifier"),
        ("common/pop_jobs", "triggered_planet_modifier"),
        ("common/traditions", "triggered_modifier"),
    ] {
        let answer = native.registry_fields(registry)?;
        let field = answer
            .value
            .iter()
            .find(|field| field.name == name)
            .ok_or("triggered modifier sample missing")?;
        let FieldMembers::TriggeredModifier(block) = &field.members else {
            return Err(format!("{registry}: triggered modifier grammar missing").into());
        };
        let keys = |fields: &Vec<Field>| {
            json!(
                fields
                    .iter()
                    .map(|key| {
                        let members = match &key.members {
                            FieldMembers::ModifierBlock(_) => {
                                json!({"ModifierBlock": key.reader.id})
                            }
                            FieldMembers::None => Value::Null,
                            other => json!(other),
                        };
                        json!([
                            key.name,
                            key.reader.kind,
                            key.reader.family,
                            key.shape.repeat,
                            members
                        ])
                    })
                    .collect::<Vec<_>>()
            )
        };
        let gaps: Vec<_> = answer
            .gaps
            .iter()
            .filter(|gap| match &gap.subject {
                Some(GapSubject::Field { name }) => name == &field.name,
                Some(GapSubject::KeyPath { path }) => path.first() == Some(&field.name),
                _ => false,
            })
            .map(|gap| json!([gap.kind, gap.subject, gap.detail]))
            .collect();
        variants.insert(
            identity(field)?,
            json!({
                "sample": [registry, name],
                "fixed_keys": property(&block.fixed_keys, keys),
                "other_keys": property(&block.other_keys, |members| json!({"ModifierBlock": members.reader.id})),
                "gaps": gaps,
            }),
        );
    }
    Ok(json!(variants))
}

fn identity(field: &Field) -> super::Result<String> {
    Ok(serde_json::to_value(&field.reader.id)?
        .as_str()
        .ok_or("missing reader identity")?
        .to_owned())
}

/// A weight grammar with each key's name, kind, repeat behavior and read scope. A nested entry
/// names its reader identity; its grammar goes to `entries`.
fn compact_weight(
    block: &WeightBlock,
    entries: &mut BTreeMap<String, Value>,
) -> super::Result<Value> {
    let mut keys = Vec::new();
    if let GrammarProperty::Known(fields) | GrammarProperty::Partial(fields) = &block.fixed_keys {
        for field in fields {
            let members = match &field.members {
                FieldMembers::WeightBlock(nested) => {
                    let id = identity(field)?;
                    let entry = compact_weight(nested, entries)?;
                    entries.insert(id.clone(), entry);
                    json!({ "WeightBlock": id })
                }
                FieldMembers::None => Value::Null,
                other => json!(other),
            };
            keys.push(json!([
                field.name,
                field.reader.kind,
                field.shape.repeat,
                field.read_scope,
                members,
                field.domain
            ]));
        }
    }
    let operations: Vec<_> = match &block.operations {
        GrammarProperty::Known(operations) | GrammarProperty::Partial(operations) => operations
            .iter()
            .map(|operation| {
                let operand = operation.operand.as_ref().map(|operand| operand.kind);
                json!([operation.key, operand])
            })
            .collect(),
        GrammarProperty::Unresolved => Vec::new(),
    };

    Ok(json!({
        "scalar": property(&block.scalar, |reader| json!(reader.as_ref().map(|reader| reader.kind))),
        "fixed_keys": property(&block.fixed_keys, |_| json!(keys)),
        "operations": property(&block.operations, |_| json!(operations)),
        "operation_repeat": block.operation_repeat,
        "other_keys": block.other_keys,
    }))
}

fn property<T>(value: &GrammarProperty<T>, compact: impl Fn(&T) -> Value) -> Value {
    match value {
        GrammarProperty::Known(value) => json!({"Known": compact(value)}),
        GrammarProperty::Partial(value) => json!({"Partial": compact(value)}),
        GrammarProperty::Unresolved => json!("Unresolved"),
    }
}

/// Equal public reader identities must describe the same weight grammar, for the field and for
/// every nested entry inside it.
#[cfg(test)]
pub fn check_weight_identity(
    variants: &mut BTreeMap<String, (String, WeightBlock)>,
    field: &Field,
) -> super::Result<()> {
    let FieldMembers::WeightBlock(block) = &field.members else {
        return Ok(());
    };
    let id = serde_json::to_value(&field.reader.id)?
        .as_str()
        .ok_or("weight block has no reader identity")?
        .to_owned();
    match variants.get(&id) {
        Some((first, previous)) if previous != &**block => {
            return Err(format!("weight grammar of {} differs from {first}", field.name).into());
        }
        Some(_) => {}
        None => {
            variants.insert(id, (field.name.clone(), (**block).clone()));
        }
    }

    let keys = match &block.fixed_keys {
        GrammarProperty::Known(keys) | GrammarProperty::Partial(keys) => keys.as_slice(),
        GrammarProperty::Unresolved => &[],
    };
    for key in keys {
        check_weight_identity(variants, key)?;
    }
    Ok(())
}

/// Equal public reader identities must describe the same triggered modifier clause.
#[cfg(test)]
pub fn check_triggered_identity(
    variants: &mut BTreeMap<String, (String, TriggeredModifierBlock)>,
    field: &Field,
) -> super::Result<()> {
    let FieldMembers::TriggeredModifier(block) = &field.members else {
        return Ok(());
    };
    let id = identity(field)?;
    match variants.get(&id) {
        Some((first, previous)) if previous != &**block => {
            Err(format!("clause grammar of {} differs from {first}", field.name).into())
        }
        Some(_) => Ok(()),
        None => {
            variants.insert(id, (field.name.clone(), (**block).clone()));
            Ok(())
        }
    }
}

/// Equal public reader identities must describe the same modifier grammar.
#[cfg(test)]
pub fn check_modifier_identity(
    variants: &mut BTreeMap<String, ModifierBlock>,
    field: &Field,
) -> super::Result<()> {
    if let FieldMembers::ModifierBlock(block) = &field.members {
        let id = field
            .reader
            .id
            .as_ref()
            .ok_or("modifier block has no reader identity")?;
        let id = serde_json::to_value(id)?
            .as_str()
            .ok_or("invalid reader identity")?
            .to_owned();
        if let Some(previous) = variants.get(&id) {
            if previous != block {
                return Err(format!("modifier grammar differs for {}", field.name).into());
            }
        } else {
            variants.insert(id.clone(), block.clone());
        }
    }
    Ok(())
}

pub fn entry(context: &EntryContext) -> String {
    let scope = |scope: &EntryScope| match scope {
        EntryScope::Scope(reference) => reference.name.clone(),
        other => format!("{other:?}"),
    };
    let from: Vec<_> = context.from.iter().map(scope).collect();
    let prev: Vec<_> = context.prev.iter().map(scope).collect();
    format!(
        "this={} root={} from=[{}] prev=[{}]",
        scope(&context.this),
        scope(&context.root),
        from.join(","),
        prev.join(",")
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

/// Names as templates such as `{key}_desc`; the boundary statement is not part of the selection.
pub fn compact_derived_names(answer: &Answer<Vec<DerivedName>>) -> Value {
    let names: Vec<_> = answer
        .value
        .iter()
        .map(|name| {
            let on_missing = match &name.on_missing {
                MissingName::Fallback(parts) => json!({ "Fallback": template(parts) }),
                other => json!(other),
            };

            json!({
                "template": template(&name.name),
                "lookup": name.lookup,
                "stage": name.stage,
                "on_missing": on_missing,
                "condition": name.condition,
            })
        })
        .collect();
    let gaps: Vec<_> = answer
        .gaps
        .iter()
        .filter(|gap| gap.kind != GapKind::OutsideMethod)
        .map(|gap| json!([gap.kind, gap.subject, gap.detail]))
        .collect();

    json!({ "completeness": answer.completeness, "names": names, "gaps": gaps })
}

fn template(name: &[NamePart]) -> String {
    name.iter()
        .map(|part| match part {
            NamePart::Literal(text) => text.clone(),
            NamePart::ItemKey => "{key}".into(),
            NamePart::Field(path) => format!("{{{}}}", path.join("/")),
            other => panic!("unexpected part {other:?}"),
        })
        .collect()
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

#[cfg(test)]
mod triggered_tests {
    use super::*;

    #[test]
    fn shared_identity_rejects_different_clauses() {
        let field: Field = serde_json::from_value(json!({
            "name":"triggered_modifier", "reader":{"id":"shared","kind":"Block","family":"TriggeredModifier","numeric":"Unresolved","scoped_operand":"Unresolved"},
            "shape":{"value":"Block","repeat":"Accumulate"}, "read":[],
            "members":{"TriggeredModifier":{"fixed_keys":{"Known":[]},"other_keys":"Unresolved"}},
            "domain":"Unknown","reference":"NotEstablished","uses":[],"entry_contexts":[],"read_scope":"Unresolved","accepted_categories":"NotApplicable"
        }))
        .unwrap();
        let mut variants = BTreeMap::new();
        check_triggered_identity(&mut variants, &field).unwrap();
        let mut other = field.clone();
        other.name = "triggered_planet_modifier".into();
        check_triggered_identity(&mut variants, &other).unwrap();
        let FieldMembers::TriggeredModifier(block) = &mut other.members else {
            unreachable!()
        };
        block.fixed_keys = GrammarProperty::Unresolved;
        assert!(check_triggered_identity(&mut variants, &other).is_err());
    }
}

#[cfg(test)]
mod weight_tests {
    use super::*;

    fn field() -> Field {
        serde_json::from_value(json!({
            "name":"ai_weight", "reader":{"id":"shared","kind":"Block","family":"Weight","numeric":"Unresolved","scoped_operand":"Unresolved"},
            "shape":{"value":"Block","repeat":"Unknown"}, "read":[],
            "members":{"WeightBlock":{"scalar":{"Known":null},"fixed_keys":{"Known":[]},
                "operations":{"Known":[{"key":"add","operand":null}]},
                "operation_repeat":"Accumulate","other_keys":"Rejected"}},
            "domain":"Unknown","reference":"NotEstablished","uses":[],"entry_contexts":[],"read_scope":"Unresolved","accepted_categories":"NotApplicable"
        })).unwrap()
    }

    #[test]
    fn shared_identity_rejects_different_blocks() {
        let field = field();
        let mut variants = BTreeMap::new();
        check_weight_identity(&mut variants, &field).unwrap();
        let mut other = field.clone();
        other.name = "random_weight".into();
        check_weight_identity(&mut variants, &other).unwrap();
        let FieldMembers::WeightBlock(block) = &mut other.members else {
            unreachable!()
        };
        block.operation_repeat = RepeatBehavior::Replace;
        assert!(check_weight_identity(&mut variants, &other).is_err());
    }

    /// A root weight field with its own identity and one nested `modifier` entry.
    fn with_entry(root: &str, entry_repeat: RepeatBehavior) -> Field {
        let mut entry = field();
        entry.name = "modifier".into();
        entry.reader.id = serde_json::from_value(json!("entry")).unwrap();
        let FieldMembers::WeightBlock(block) = &mut entry.members else {
            unreachable!()
        };
        block.operation_repeat = entry_repeat;

        let mut parent = field();
        parent.reader.id = serde_json::from_value(json!(root)).unwrap();
        let FieldMembers::WeightBlock(block) = &mut parent.members else {
            unreachable!()
        };
        block.fixed_keys = GrammarProperty::Known(vec![entry]);
        parent
    }

    #[test]
    fn shared_nested_identity_rejects_different_blocks_under_different_roots() {
        let mut variants = BTreeMap::new();
        check_weight_identity(
            &mut variants,
            &with_entry("shared", RepeatBehavior::Replace),
        )
        .unwrap();

        let differing = with_entry("variant", RepeatBehavior::Accumulate);
        assert!(check_weight_identity(&mut variants, &differing).is_err());
        assert!(variants.contains_key("entry"));
    }
}

#[cfg(test)]
mod modifier_tests {
    use super::*;

    fn field() -> Field {
        serde_json::from_value(json!({
            "name":"modifier", "reader":{"id":"shared","kind":"Block","family":"Modifier","numeric":"Unresolved","scoped_operand":"Unresolved"},
            "shape":{"value":"Unknown","repeat":"Unknown"}, "read":[],
            "members":{"ModifierBlock":{"fixed_keys":{"Known":[]},"entries":{"Known":[
                {"Reference":{"target":{"Registry":{"name":"common/static_modifiers"}},"value":"FixedPoint"}}
            ]}}}, "domain":"Unknown","reference":"NotEstablished","uses":[],"entry_contexts":[],"read_scope":"Unresolved","accepted_categories":"NotApplicable"
        })).unwrap()
    }

    #[test]
    fn shared_identity_rejects_different_blocks() {
        let field = field();
        let mut variants = BTreeMap::new();
        check_modifier_identity(&mut variants, &field).unwrap();
        let mut other = field.clone();
        other.name = "other_use".into();
        check_modifier_identity(&mut variants, &other).unwrap();
        let FieldMembers::ModifierBlock(block) = &mut other.members else {
            unreachable!()
        };
        block.entries = GrammarProperty::Unresolved;
        assert!(check_modifier_identity(&mut variants, &other).is_err());
    }

    #[test]
    fn modifier_parity_rejects_removed_keys_and_changed_reference_kinds() {
        let expected = json!({"shared":{
            "fixed_keys":{"Partial":[["description","String"]]},
            "entries":{"Known":[{"Reference":{"target":{"Registry":{"name":"common/static_modifiers"}},"value":"FixedPoint"}}]}
        }});
        let build = serde_json::from_value(json!("test-build")).unwrap();
        let mut removed = expected.clone();
        removed["shared"]["fixed_keys"]["Partial"] = json!([]);
        let mut changed_kind = expected.clone();
        changed_kind["shared"]["entries"]["Known"][0]["Reference"]["value"] = json!("Float");
        for changed in [removed, changed_kind] {
            let report = super::super::comparison::compare_static(
                &build,
                "modifier-blocks.json",
                &serde_json::to_vec(&expected).unwrap(),
                &serde_json::to_vec(&changed).unwrap(),
            );
            assert!(!report.passes());
        }
    }
}
