use super::dispatch::OrderCheck;
use super::tokens::{Token, named_key};
use super::uses::storage_offset;
use super::{
    FieldGap, FieldGapKind, KeyOrder, PathOutcome, ReaderJoin, RootField, TokenPath, Value,
};
use std::collections::BTreeMap;

pub(crate) fn fields_and_gaps(
    paths: &[TokenPath],
    tokens: &BTreeMap<i64, Token>,
) -> (Vec<RootField>, Vec<FieldGap>) {
    let mut gaps = Vec::new();
    let mut fields = BTreeMap::<i64, RootField>::new();
    for path in paths {
        if !matches!(path.outcome, PathOutcome::Reader(_)) {
            continue;
        }
        if let Some((token, name)) = named_key(path.domain, tokens) {
            fields.entry(path.domain[0]).or_insert_with(|| RootField {
                name: name.to_owned(),
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

/// The key order of each order check in `checks`: the named keys whose paths do not pass the
/// check, and the field that sets the tested word.
pub(super) fn key_order(
    checks: &[OrderCheck],
    paths: &[TokenPath],
    fields: &[RootField],
    partition_accounted: bool,
) -> Vec<KeyOrder> {
    let mut order: Vec<KeyOrder> = Vec::new();

    for check in checks {
        let passes = |field: &&RootField| {
            field
                .paths
                .iter()
                .any(|&index| paths[index].instructions.contains(&check.at))
        };
        let mut allowed: Vec<_> = fields
            .iter()
            .filter(|field| !passes(field))
            .map(|field| field.name.clone())
            .collect();
        allowed.sort();
        let set_by = match &check.tested {
            Value::Load(word, _) => match word.as_ref() {
                Value::Owner(at) => fields
                    .iter()
                    .find(|field| storage_offset(field, partition_accounted) == Some(*at))
                    .map(|field| field.name.clone()),
                _ => None,
            },
            _ => None,
        };
        let entry = KeyOrder { allowed, set_by };

        if !order.contains(&entry) {
            order.push(entry);
        }
    }

    order
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
    use super::super::{RegistryFieldResult, Value, storage_offset};
    use super::*;
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
        PathOutcome::Reader(ReaderJoin::Joined {
            callee: "CReader::Read(CString&, bool)".into(),
            arguments: BTreeMap::from([
                ("x0".into(), Value::Reader(0)),
                ("x1".into(), Value::Owner(STORED_AT)),
            ]),
            tail: true,
        })
    }

    fn unresolved() -> PathOutcome {
        PathOutcome::Gap(Unresolved::new("reader not established"))
    }

    /// The derived storage of a registry whose only named token 5 has these paths.
    fn derived(paths: Vec<TokenPath>) -> (Option<i64>, usize) {
        let token = Token {
            name: Some("name".into()),
            constructor: 0,
        };
        let (fields, gaps) = fields_and_gaps(&paths, &BTreeMap::from([(5, token)]));
        let partition_accounted = partition_accounted(&paths);
        let offset = storage_offset(&fields[0], partition_accounted);
        let result = RegistryFieldResult {
            key_order: Vec::new(),
            keyword_domains: Default::default(),
            loader: None,
            uses: Vec::new(),
            persistent: BTreeMap::new(),
            persistent_points: BTreeMap::new(),
            scoped_destinations: BTreeMap::new(),
            stored_words: BTreeMap::new(),
            container_masks: BTreeMap::new(),
            collections: Vec::new(),
            embedded: Vec::new(),
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
