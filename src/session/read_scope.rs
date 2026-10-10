//! Normalize read-time scope arguments without conflating them with evaluation contexts.
use super::language::gap_for_subject;
use crate::engine::analysis::{
    declarations::ScopeType,
    fields::{PathOutcome, ReaderJoin, RootField, TokenPath, Value},
    grammar::GrammarResult,
    readers,
};
use crate::{ChildScope, Field, Gap, GapKind, GapSubject, GrammarProperty, ReadScope, ReaderKind};

pub(super) fn registry_fields(
    values: &mut [Field],
    result: &crate::engine::analysis::fields::RegistryFieldResult,
    names: Option<&[String]>,
    parent: &[String],
    gaps: &mut Vec<Gap>,
) {
    let mut local_gaps = Vec::new();
    fields(
        values,
        &result.fields,
        &result.paths,
        names,
        &mut local_gaps,
    );
    prefix_gaps(local_gaps, parent, gaps);
    for value in values {
        if let crate::FieldMembers::Fields(children) = &mut value.members
            && let Some(root) = result.fields.iter().find(|root| root.name == value.name)
            && let Some(collection) = result
                .collections
                .iter()
                .find(|collection| collection.token == root.token)
        {
            let mut path = parent.to_vec();
            path.push(value.name.clone());
            registry_fields(children, &collection.fields, names, &path, gaps);
        }
    }
}

/// The read scopes of a block's keys, with gaps at their paths below `parent`.
pub(super) fn block_keys(
    values: &mut [Field],
    roots: &[RootField],
    paths: &[TokenPath],
    names: Option<&[String]>,
    parent: &[String],
    gaps: &mut Vec<Gap>,
) {
    let mut local_gaps = Vec::new();
    fields(values, roots, paths, names, &mut local_gaps);
    prefix_gaps(local_gaps, parent, gaps);
}

fn prefix_gaps(local_gaps: Vec<Gap>, parent: &[String], gaps: &mut Vec<Gap>) {
    for mut gap in local_gaps {
        if !parent.is_empty()
            && let Some(GapSubject::Field { name }) = gap.subject
        {
            let mut path = parent.to_vec();
            path.push(name);
            gap.subject = Some(GapSubject::key_path(path));
        }
        gaps.push(gap);
    }
}

fn fields(
    fields: &mut [Field],
    roots: &[RootField],
    paths: &[TokenPath],
    names: Option<&[String]>,
    gaps: &mut Vec<Gap>,
) {
    for field in fields {
        // The weight method sets the read scope of a weight block from its stored scope.
        if matches!(field.members, crate::FieldMembers::WeightBlock(_)) {
            continue;
        }
        // A modifier block's reader takes no scope, and its entries are not read in one.
        if matches!(field.members, crate::FieldMembers::ModifierBlock(_)) {
            field.read_scope = GrammarProperty::Known(vec![]);
            continue;
        }
        let Some(root) = roots.iter().find(|root| root.name == field.name) else {
            continue;
        };
        let alternatives = super::fields::read_alternatives(root, paths);
        let outcomes: Vec<_> = if alternatives.is_empty() {
            root.readers
                .iter()
                .cloned()
                .map(PathOutcome::Reader)
                .collect()
        } else {
            alternatives
                .into_iter()
                .map(|(_, outcome)| outcome)
                .collect()
        };
        let mut arguments = Vec::new();
        let mut scalar = false;
        for outcome in outcomes {
            match outcome {
                PathOutcome::Rejected => {}
                PathOutcome::Reader(join) => {
                    let kind = readers::classify(std::slice::from_ref(&join)).kind;
                    if matches!(kind, ReaderKind::Block | ReaderKind::Unknown) {
                        arguments.push(argument(&join));
                    } else {
                        scalar = true;
                    }
                }
                PathOutcome::Gap(_) => arguments.push(None),
            }
        }
        if scalar && arguments.is_empty() {
            field.read_scope = GrammarProperty::Known(vec![]);
            continue;
        }
        field.read_scope = normalize_scopes(arguments, names, GapSubject::field(&field.name), gaps);
    }
}

/// The established reader ABI supplies the scope after the reader and destination, or after
/// the token for a member delegate. A name alone never repairs a missing routing join.
fn argument(join: &ReaderJoin) -> Option<Value> {
    let ReaderJoin::Joined {
        callee, arguments, ..
    } = join
    else {
        return None;
    };
    readers::scope_argument(callee, arguments).cloned()
}

pub(super) fn grammar(
    value: &mut crate::CommandGrammar,
    result: &GrammarResult,
    name: &str,
    gaps: &mut Vec<Gap>,
) {
    let names = result.read_scopes.names.as_deref();
    if let GrammarProperty::Known(fields_value) | GrammarProperty::Partial(fields_value) =
        &mut value.fixed_keys
    {
        nested_fields(fields_value, result, &[], gaps);
    }
    let mut families = Vec::new();
    for (family, _) in &result.read_scopes.children {
        if !families.contains(family) {
            families.push(*family);
        }
    }
    let scopes: Vec<_> = families
        .into_iter()
        .map(|family| {
            let mut arguments: Vec<_> = result
                .read_scopes
                .children
                .iter()
                .filter(|(candidate, _)| *candidate == family)
                .map(|(_, scope)| scope.clone())
                .collect();
            if !result.read_scopes.complete {
                arguments.push(None);
            }
            ChildScope {
                family,
                scope: normalize_scopes(arguments, names, GapSubject::answer_item(name), gaps),
            }
        })
        .collect();
    value.child_scopes = if result.value_only() {
        GrammarProperty::Known(vec![])
    } else if scopes.is_empty() {
        if result.read_scopes.complete {
            GrammarProperty::Known(vec![])
        } else {
            gaps.push(gap_for_subject(
                GapKind::UnresolvedPath,
                Some(GapSubject::answer_item(name)),
                "read-scope: child-family-boundary",
            ));
            GrammarProperty::Unresolved
        }
    } else if scopes
        .iter()
        .all(|scope| matches!(scope.scope, GrammarProperty::Known(_)))
        && result.read_scopes.complete
    {
        GrammarProperty::Known(scopes)
    } else {
        GrammarProperty::Partial(scopes)
    };
}

fn nested_fields(
    values: &mut [Field],
    result: &GrammarResult,
    parent: &[String],
    gaps: &mut Vec<Gap>,
) {
    let mut local_gaps = Vec::new();
    fields(
        values,
        &result.fields.fields,
        &result.read_scopes.paths,
        result.read_scopes.names.as_deref(),
        &mut local_gaps,
    );
    prefix_gaps(local_gaps, parent, gaps);
    for field in values {
        if let crate::FieldMembers::Fields(children) = &mut field.members
            && let Some(child_result) = result.nested.get(&field.name)
        {
            let mut path = parent.to_vec();
            path.push(field.name.clone());
            nested_fields(children, child_result, &path, gaps);
        }
    }
}

/// The read scopes that the scope `arguments` give, with a gap at `subject` for each argument
/// that names no scope type.
pub(super) fn normalize_scopes(
    arguments: Vec<Option<Value>>,
    names: Option<&[String]>,
    subject: GapSubject,
    gaps: &mut Vec<Gap>,
) -> GrammarProperty<Vec<ReadScope>> {
    let mut scopes = Vec::new();
    let mut reasons = Vec::new();
    if arguments.is_empty() {
        reasons.push("reader-boundary");
    }
    for argument in arguments {
        let result = match argument {
            Some(Value::EnclosingScope) => Ok(ReadScope::Enclosing),
            Some(Value::Constant(0)) => Err("zero-mask"),
            Some(Value::Constant(mask)) if mask > 0 => types(mask as u64, names),
            Some(Value::Load(..)) => Err("stored-scope"),
            _ => Err("scope-argument"),
        };
        match result {
            Ok(scope) if !scopes.contains(&scope) => scopes.push(scope),
            Ok(_) => {}
            Err(reason) if !reasons.contains(&reason) => reasons.push(reason),
            Err(_) => {}
        }
    }
    for reason in &reasons {
        gaps.push(gap_for_subject(
            GapKind::UnresolvedPath,
            Some(subject.clone()),
            format!("read-scope: {reason}"),
        ));
    }
    if reasons.is_empty() {
        GrammarProperty::Known(scopes)
    } else if scopes.is_empty() {
        GrammarProperty::Unresolved
    } else {
        GrammarProperty::Partial(scopes)
    }
}

fn types(mask: u64, names: Option<&[String]>) -> Result<ReadScope, &'static str> {
    let names = names.ok_or("scope-names")?;
    let mut types = Vec::new();
    for bit in 0..64 {
        if mask & (1 << bit) == 0 {
            continue;
        }
        let name = names
            .get(bit)
            .filter(|name| !name.is_empty())
            .ok_or("scope-name")?;
        types.push(ScopeType {
            bit,
            name: name.clone(),
        });
    }
    Ok(ReadScope::Types(super::questions::scope_references(&types)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scopes(arguments: Vec<Option<Value>>) -> (GrammarProperty<Vec<ReadScope>>, Vec<Gap>) {
        let names = ["none", "planet", "country", "pop"].map(String::from);
        let mut gaps = Vec::new();
        let value = normalize_scopes(
            arguments,
            Some(&names),
            GapSubject::field("potential"),
            &mut gaps,
        );
        (value, gaps)
    }

    #[test]
    fn read_scope_retains_sets_and_distinct_entry_alternatives() {
        let (result, gaps) = scopes(vec![
            Some(Value::Constant(10)),
            Some(Value::Constant(4)),
            Some(Value::Constant(10)),
        ]);
        let GrammarProperty::Known(alternatives) = result else {
            panic!("{result:?}")
        };
        assert_eq!(alternatives.len(), 2);
        let ReadScope::Types(types) = &alternatives[0] else {
            panic!()
        };
        assert_eq!(
            types.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["planet", "pop"]
        );
        assert!(gaps.is_empty());
    }

    #[test]
    fn read_scope_zero_unknown_bits_and_missing_paths_remain_gaps() {
        for unknown in [
            Some(Value::Constant(0)),
            Some(Value::Constant(1 << 20)),
            None,
        ] {
            let (result, gaps) = scopes(vec![Some(Value::Constant(4)), unknown]);
            assert!(matches!(result, GrammarProperty::Partial(ref values) if values.len() == 1));
            assert_eq!(gaps.len(), 1);
        }
        assert!(matches!(scopes(vec![]).0, GrammarProperty::Unresolved));
    }

    #[test]
    fn a_modifier_block_is_read_in_no_scope_beside_a_trigger_block_that_keeps_its_scope() {
        use crate::engine::analysis::fields::ReaderJoin;
        use serde_json::json;

        let field = |name: &str, members: serde_json::Value| -> Field {
            serde_json::from_value(json!({
                "name": name,
                "reader": { "id": null, "kind": "Block", "family": "Unknown",
                    "numeric": "Unresolved", "scoped_operand": "Unresolved" },
                "shape": { "value": "Unknown", "repeat": "Unknown" }, "read": [],
                "members": members, "domain": "Unknown", "uses": [],
                "reference": "NotEstablished", "entry_contexts": [], "read_scope": "Unresolved",
                "accepted_categories": "NotApplicable"
            }))
            .unwrap()
        };
        let root = |name: &str, callee: &str, arguments: Vec<(&str, Value)>| RootField {
            name: name.into(),
            token: 7,
            constructor: 0,
            paths: vec![],
            readers: vec![ReaderJoin::Joined {
                callee: callee.into(),
                arguments: arguments
                    .into_iter()
                    .map(|(register, value)| (register.to_owned(), value))
                    .collect(),
                tail: true,
            }],
        };
        let modifier_block = json!({ "ModifierBlock": {
            "fixed_keys": { "Known": [] }, "entries": { "Known": [] } } });
        let mut values = [
            field("modifier", modifier_block),
            field("potential", json!("Unresolved")),
        ];
        let roots = [
            root(
                "modifier",
                "CReader::Read(CPersistent&)",
                vec![("x0", Value::Reader(0)), ("x1", Value::Owner(0x40))],
            ),
            root(
                "potential",
                "void NParserUtil::ReadTrigger<CRootTrigger>(CReader&, CRootTrigger&, EScopeType)",
                vec![
                    ("x0", Value::Reader(0)),
                    ("x1", Value::Owner(0x80)),
                    ("x2", Value::Constant(4)),
                ],
            ),
        ];
        let names = ["none", "planet", "country"].map(String::from);
        let mut gaps = Vec::new();

        fields(&mut values, &roots, &[], Some(&names), &mut gaps);

        assert_eq!(values[0].read_scope, GrammarProperty::Known(vec![]));
        let GrammarProperty::Known(scopes) = &values[1].read_scope else {
            panic!("{:?}", values[1].read_scope);
        };
        assert!(
            matches!(scopes.as_slice(), [ReadScope::Types(types)] if types.len() == 1 && types[0].name == "country")
        );
        assert!(gaps.is_empty(), "{gaps:?}");
    }

    #[test]
    fn enclosing_scope_stays_distinct_from_a_named_set() {
        assert_eq!(
            scopes(vec![Some(Value::EnclosingScope)]).0,
            GrammarProperty::Known(vec![ReadScope::Enclosing])
        );
    }
}
