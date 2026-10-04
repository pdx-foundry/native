//! Shared bounds, token domains and receiver translation for member delegates.
use super::{Condition, DispatchInput, FieldGap, PathOutcome, ReaderJoin, TokenPath, Value};
use crate::engine::analysis::stop::Unresolved;

/// Follow one member edge, preserving its token domain and owner-relative provenance.
pub(crate) fn follow_member(
    input: &DispatchInput<'_>,
    parent: &TokenPath,
    chain: &[String],
) -> Result<(Vec<TokenPath>, Vec<FieldGap>), Unresolved> {
    let PathOutcome::Reader(ReaderJoin::Joined {
        callee, arguments, ..
    }) = &parent.outcome
    else {
        return Err(Unresolved::new("delegate-receiver"));
    };
    let Some(Value::Owner(offset)) = arguments.get("x0") else {
        return Err(Unresolved::new("delegate-receiver"));
    };
    if chain.len() >= 8 || chain.contains(callee) {
        return Err(Unresolved::new("grammar-delegation-limit"));
    }
    // The known address point belongs to the root object, not an embedded delegate.
    let owner_vtable = input.owner_vtable.filter(|_| *offset == 0);
    let mut dispatch = input.with_owner_vtable(owner_vtable);
    dispatch.scope = crate::engine::analysis::readers::scope_argument(callee, arguments).cloned();
    let (children, gaps) = super::explore_member(&dispatch, callee);
    let children = children
        .into_iter()
        .filter_map(|mut child| {
            translate(&mut child, *offset);
            inherit_path(
                child,
                parent.domain,
                &parent.conditions,
                &parent.instructions,
            )
        })
        .collect();
    Ok((children, gaps))
}

/// Intersect a delegated path with its caller and preserve caller provenance.
pub(super) fn inherit_path(
    mut child: TokenPath,
    domain: [i64; 2],
    conditions: &[Condition],
    instructions: &[u64],
) -> Option<TokenPath> {
    child.domain = [
        child.domain[0].max(domain[0]),
        child.domain[1].min(domain[1]),
    ];
    if child.domain[0] > child.domain[1] {
        return None;
    }
    child.conditions.splice(0..0, conditions.iter().cloned());
    child
        .instructions
        .splice(0..0, instructions.iter().copied());
    Some(child)
}

fn translate(path: &mut TokenPath, offset: i64) {
    for Condition { value: tested, .. } in &mut path.conditions {
        if let Some(tested) = tested {
            translate_value(tested, offset);
        }
    }
    if let PathOutcome::Reader(ReaderJoin::Stored { destination, .. }) = &mut path.outcome {
        *destination += offset;
    }
    if let PathOutcome::Reader(ReaderJoin::Joined { arguments, .. }) = &mut path.outcome {
        for argument in arguments.values_mut() {
            translate_value(argument, offset);
        }
    }
}

fn translate_value(value: &mut Value, offset: i64) {
    match value {
        Value::Owner(at) => *at += offset,
        Value::Load(base, _) | Value::Offset(base, _) | Value::EqualsAny(base, _) => {
            translate_value(base, offset)
        }
        Value::Indexed(base, index, _) | Value::SumProduct(base, index, _) => {
            translate_value(base, offset);
            translate_value(index, offset);
        }
        _ => {}
    }
}
/// A constructor that installs a vtable and saves its wrapped owner in the next pointer slot.
pub(crate) fn serializer_owner_slot(
    rows: &[crate::engine::analysis::decode::Instruction],
) -> Option<i64> {
    use crate::engine::analysis::references::shapes::{Shape, canonical};
    let lines = canonical(rows, &Default::default());
    Shape::parse("adrp xr0,PAGE\nadd xr0,xr0,G = {vtable}\nstp xr0,x1,[x0]\nret")
        .matches(&lines)
        .map(|_| 8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::{assembler::arm64, declarations::Function, discovery::Symbol};
    use std::collections::BTreeMap;

    #[test]
    fn member_chain_translates_owner_offsets_and_retains_parent_domain() {
        let names = [
            "A::ReadMember(CReader&, int)",
            "B::ReadMember(CReader&, int)",
            "C::ReadMember(CReader&, int)",
            "CReader::Read(int&)",
        ];
        let symbols: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(index, name)| Symbol {
                name: (*name).into(),
                address: 0x1000 + index as u64 * 0x1000,
            })
            .collect();
        let functions = BTreeMap::from([
            (
                0x1000,
                Function {
                    address: 0x1000,
                    code: arm64!(at 0x1000; add x0, x0, #0x10; b extern 0x2000),
                },
            ),
            (
                0x2000,
                Function {
                    address: 0x2000,
                    code: arm64!(at 0x2000; add x0, x0, #0x20; b extern 0x3000),
                },
            ),
            (
                0x3000,
                Function {
                    address: 0x3000,
                    code: arm64!(at 0x3000; add x8, x0, #8; mov x0, x1; mov x1, x8; b extern 0x4000),
                },
            ),
        ]);
        let readers = super::super::KeyReaders::default();
        let input = DispatchInput::command(&functions, &symbols, &[], 0x38, &readers);
        let (mut paths, _) = super::super::explore_member(&input, names[0]);
        paths[0].domain = [7, 7];
        let chain = vec![names[0].into()];
        let (children, _) = follow_member(&input, &paths[0], &chain).unwrap();
        let (leaves, _) =
            follow_member(&input, &children[0], &[names[0].into(), names[1].into()]).unwrap();
        assert_eq!(leaves[0].domain, [7, 7]);
        assert_eq!(
            crate::engine::analysis::readers::destination(match &leaves[0].outcome {
                PathOutcome::Reader(join) => join,
                _ => panic!(),
            }),
            Some(0x38)
        );
        assert_eq!(leaves[0].instructions.len(), 8);
        assert_eq!(
            follow_member(&input, &paths[0], &[names[1].into()])
                .unwrap_err()
                .reason,
            "grammar-delegation-limit"
        );
        assert_eq!(
            follow_member(&input, &paths[0], &vec![names[0].into(); 8])
                .unwrap_err()
                .reason,
            "grammar-delegation-limit"
        );
    }
}
