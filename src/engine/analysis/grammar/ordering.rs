//! Recognize child-position tests from their load provenance, never from a command name.
use super::{GrammarInput, ReaderJoin, TokenPath, Value};
use crate::ChildOrderCondition;

#[derive(Debug)]
pub struct Rule {
    pub child: String,
    pub conditions: Vec<ChildOrderCondition>,
    pub outcome: super::OrderOutcome,
}

pub(super) fn rule(input: &GrammarInput, path: &TokenPath) -> Option<Rule> {
    if path.domain[0] != path.domain[1] || path.conditions.is_empty() {
        return None;
    }
    let child = input
        .tokens
        .get(&path.domain[0])
        .filter(|token| !token.ambiguous)?;
    let super::PathOutcome::Reader(join @ ReaderJoin::Joined { callee, .. }) = &path.outcome else {
        return None;
    };
    let conditions: Option<Vec<_>> = path
        .conditions
        .iter()
        .map(|condition| {
            let value = condition.value.as_ref()?;
            if owner_load(value, input.child_layout.count, 4) {
                return Some(ChildOrderCondition::First(condition.zero));
            }
            let Value::EqualsAny(value, tokens) = value else {
                return None;
            };
            if !previous_token(input, value) {
                return None;
            }
            let keys: Option<Vec<_>> = tokens
                .iter()
                .map(|token| {
                    input
                        .tokens
                        .get(token)
                        .filter(|token| !token.ambiguous)
                        .map(|token| token.name.clone())
                })
                .collect();
            Some(ChildOrderCondition::Previous {
                keys: keys?,
                matches: !condition.zero,
            })
        })
        .collect();
    let conditions = conditions?;
    if conditions
        .iter()
        .any(|condition| matches!(condition, ChildOrderCondition::Previous { .. }))
        && !conditions.contains(&ChildOrderCondition::First(false))
    {
        return None;
    }
    let outcome = match input.families.get(callee) {
        Some(&family) => super::OrderOutcome::Family(family),
        None if callee.ends_with("::ReadMember(CReader&, int, EScopeType)")
            || callee.ends_with("::ReadMember(CReader&, int)") =>
        {
            return None;
        }
        None => super::OrderOutcome::Reader(join.clone()),
    };
    Some(Rule {
        child: child.name.clone(),
        conditions,
        outcome,
    })
}

fn owner_load(value: &Value, offset: i64, width: u8) -> bool {
    matches!(value, Value::Load(base, size) if *size == width && **base == Value::Owner(offset))
}

fn previous_token(input: &GrammarInput, value: &Value) -> bool {
    let Value::Load(address, 4) = value else {
        return false;
    };
    let Value::Offset(child, token_offset) = address.as_ref() else {
        return false;
    };
    if *token_offset != input.child_layout.token {
        return false;
    }
    let Value::Load(address, 8) = child.as_ref() else {
        return false;
    };
    let Value::Offset(end, -8) = address.as_ref() else {
        return false;
    };
    let Value::Indexed(data, count, 3) = end.as_ref() else {
        return false;
    };
    owner_load(data, input.child_layout.data, 8) && owner_load(count, input.child_layout.count, 4)
}
