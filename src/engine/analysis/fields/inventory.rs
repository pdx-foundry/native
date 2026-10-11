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
            PathOutcome::Gap(unresolved) => ReaderJoin::Missing(unresolved.clone()),
        };
        if let ReaderJoin::Missing(unresolved) = &join {
            gaps.push(FieldGap {
                path: Some(index),
                ..FieldGap::unresolved(FieldGapKind::ReaderJoin, unresolved.clone())
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

#[cfg(test)]
mod tests {
    use super::super::{RegistryFieldResult, storage_offset};
    use super::*;
    use crate::ReaderKind;
    use crate::engine::analysis::names;
    use crate::engine::analysis::stop::Unresolved;

    const STORED_AT: i64 = 0x40;

    fn path(domain: [i64; 2], outcome: PathOutcome) -> TokenPath {
        TokenPath {
            domain,
            conditions: Vec::new(),
            instructions: Vec::new(),
            terminal: 0,
            outcome,
        }
    }

    fn string_field() -> PathOutcome {
        PathOutcome::Reader(ReaderJoin::Stored {
            callee: "reader".into(),
            kind: ReaderKind::String,
            destination: Some(STORED_AT),
            repeat: crate::RepeatBehavior::Replace,
        })
    }

    fn unresolved() -> PathOutcome {
        PathOutcome::Gap(Unresolved::new("reader not established"))
    }

    /// The derived storage of a registry whose only named token 5 has these paths.
    fn derived(paths: Vec<TokenPath>) -> (Option<i64>, usize) {
        let token = Token {
            name: "name".into(),
            constructor: 0,
            ambiguous: false,
        };
        let (fields, gaps) = fields_and_gaps(&paths, &BTreeMap::from([(5, token)]));
        let partition_accounted = partition_accounted(&paths);
        let offset = storage_offset(&fields[0], partition_accounted);
        let result = RegistryFieldResult {
            uses: Vec::new(),
            persistent: BTreeMap::new(),
            persistent_points: BTreeMap::new(),
            scoped_destinations: BTreeMap::new(),
            stored_words: BTreeMap::new(),
            container_masks: BTreeMap::new(),
            collections: Vec::new(),
            fields,
            paths,
            gaps,
            partition_accounted,
        };
        (offset, names::storage(&result).strings.len())
    }

    #[test]
    fn an_overlapping_wider_path_withholds_the_storage_of_a_singleton_field() {
        let overlapping = vec![
            path([i32::MIN as i64, i32::MAX as i64], unresolved()),
            path([5, 5], string_field()),
        ];
        assert_eq!(derived(overlapping), (None, 0));
    }

    #[test]
    fn an_accounted_partition_keeps_the_storage_of_a_singleton_field() {
        let accounted = vec![
            path([i32::MIN as i64, 4], unresolved()),
            path([5, 5], string_field()),
            path([6, i32::MAX as i64], unresolved()),
        ];
        assert_eq!(derived(accounted), (Some(STORED_AT), 1));
    }
}
