use super::tokens::Token;
use super::{FieldGap, FieldGapKind, PathOutcome, ReaderJoin, RootField, TokenPath};
use std::collections::BTreeMap;

pub(crate) fn fields_and_gaps(
    paths: &[TokenPath],
    tokens: &BTreeMap<i64, Token>,
) -> (Vec<RootField>, Vec<FieldGap>) {
    let mut gaps = Vec::new();
    let mut fields = BTreeMap::<i64, RootField>::new();
    for path in paths {
        if path.domain[0] != path.domain[1] || !matches!(path.outcome, PathOutcome::Reader(_)) {
            continue;
        }
        if let Some(token) = tokens.get(&path.domain[0]).filter(|t| !t.ambiguous) {
            fields.entry(path.domain[0]).or_insert_with(|| RootField {
                name: token.name.clone(),
                token: path.domain[0],
                constructor: token.constructor,
                paths: vec![],
                readers: vec![],
            });
        }
    }
    for (index, path) in paths.iter().enumerate() {
        let join = match &path.outcome {
            PathOutcome::Rejected => continue,
            PathOutcome::Reader(join) => join.clone(),
            PathOutcome::Gap(unresolved) => ReaderJoin::Missing(*unresolved),
        };
        if let ReaderJoin::Missing(unresolved) = join {
            gaps.push(FieldGap {
                path: Some(index),
                ..FieldGap::unresolved(FieldGapKind::ReaderJoin, unresolved)
            });
        }
        // An obstruction on another state alternative must stay attached to an established field.
        if path.domain[0] == path.domain[1]
            && let Some(field) = fields.get_mut(&path.domain[0])
        {
            field.paths.push(index);
            field.readers.push(join);
        } else {
            gaps.push(FieldGap {
                path: Some(index),
                ..FieldGap::new(
                    FieldGapKind::UnresolvedTokenPath,
                    "non-singleton, missing/ambiguous token name, or unestablished member path",
                )
            });
        }
    }
    (fields.into_values().collect(), gaps)
}

pub(super) fn partition_accounted(paths: &[TokenPath]) -> bool {
    let mut intervals: Vec<_> = paths.iter().map(|p| p.domain).collect();
    intervals.sort();
    intervals.dedup();
    let mut cursor = i32::MIN as i64;
    let mut partition_accounted = true;
    for [low, high] in intervals {
        if low != cursor || high < low {
            partition_accounted = false;
        }
        cursor = high + 1;
    }
    partition_accounted &= cursor == i32::MAX as i64 + 1;
    partition_accounted
}
