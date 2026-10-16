//! Root fields for the root keys of a loader that builds its owner in a scope it selects by key.
//!
//! The events loader switches on each root statement's key and constructs `CEvent` with the
//! key's scope; `CEvent::ReadMember` then reads every kind. So each accepted key is a root field
//! whose read scope is that scope, and the owner's fields are its members, the same for every
//! key.
use super::fields::{persistent_reader_id, prefix_conditions};
use super::read_scope::normalize_scopes;
use crate::engine::analysis::fields::{
    LoaderKeys, PathOutcome, ReaderJoin, RootField, TokenPath, Value,
};
use crate::{
    AcceptedCategories, Field, FieldCondition, FieldDomain, FieldMembers, FieldReadAlternative,
    FieldReadOutcome, FieldReference, FieldShape, Gap, GapKind, GapSubject, GrammarProperty,
    ReadScope, Reader, ReaderKind, RepeatBehavior, ValueShape,
};
use std::collections::BTreeSet;

/// The root fields of `registry` that `loader` accepts, each with `members` as its child fields,
/// and the gaps of the answer: `gaps` of the members repeated under each key, and the gaps of the
/// loader's walk. `names` gives scope names by mask bit.
pub(super) fn root_fields(
    members: Vec<Field>,
    gaps: Vec<Gap>,
    loader: &LoaderKeys,
    registry: &str,
    names: Option<&[String]>,
) -> (Vec<Field>, Vec<Gap>) {
    let (member_gaps, mut answer_gaps): (Vec<Gap>, Vec<Gap>) = gaps
        .into_iter()
        .partition(|gap| member_subject(gap).is_some());
    let reader = item_reader(loader);
    let mut fields = Vec::new();

    for key in &loader.keys {
        let paths = key_paths(key, loader);
        if !paths.iter().any(|path| builds_owner(path)) {
            answer_gaps.push(Gap {
                kind: GapKind::UnresolvedPath,
                subject: Some(GapSubject::registry(registry)),
                detail: format!(
                    "No path of the root key `{}` is established to build the owner or to \
                     reject the key.",
                    key.name
                ),
            });
            continue;
        }

        let read = read_alternatives(&paths, &reader);
        if read.len() > 1 {
            answer_gaps.push(Gap {
                kind: GapKind::UnresolvedCondition,
                subject: Some(GapSubject::field(&key.name)),
                detail: "Not every path of the root key builds the owner; each outcome is \
                         retained separately."
                    .into(),
            });
        }

        let mut children = members.clone();
        for child in &mut children {
            prefix_conditions(child, &key.name);
        }

        answer_gaps.extend(
            member_gaps
                .iter()
                .cloned()
                .map(|gap| under_key(gap, &key.name)),
        );
        fields.push(Field {
            name: key.name.clone(),
            reader: reader.clone(),
            shape: SHAPE,
            read,
            members: FieldMembers::Fields(children),
            domain: FieldDomain::Unknown,
            uses: Vec::new(),
            reference: FieldReference::NotEstablished,
            entry_contexts: Vec::new(),
            read_scope: read_scope(&paths, names, &key.name, &mut answer_gaps),
            accepted_categories: AcceptedCategories::NotApplicable,
            requirement: crate::FieldRequirement::Unknown,
        });
    }

    if fields.is_empty() {
        answer_gaps.push(Gap {
            kind: GapKind::UnresolvedPath,
            subject: Some(GapSubject::registry(registry)),
            detail: "No root key of the loader is established to build the owner.".into(),
        });
    }

    answer_gaps.extend(walk_gaps(loader, registry));

    (fields, answer_gaps)
}

/// The root field for the root statements with the key `key`, which the loader reads into
/// another class than the registry's owner, and the gaps of that field. `members` and `gaps` are
/// the answer for that class's items.
pub(super) fn reserved_key_field(
    key: &str,
    members: Vec<Field>,
    gaps: Vec<Gap>,
    registry: &str,
) -> (Field, Vec<Gap>) {
    let mut children = members;
    for child in &mut children {
        prefix_conditions(child, key);
    }

    // The class's registry-wide gaps are the key's; its scope statements repeat the owner's.
    let mut field_gaps: Vec<Gap> = gaps
        .into_iter()
        .filter(|gap| gap.kind != GapKind::OutsideMethod)
        .map(|mut gap| {
            if member_subject(&gap).is_some() {
                return under_key(gap, key);
            }
            if gap.subject == Some(GapSubject::registry(registry)) {
                gap.subject = Some(GapSubject::field(key));
            }

            gap
        })
        .collect();
    field_gaps.push(Gap {
        kind: GapKind::UnresolvedPath,
        subject: Some(GapSubject::field(key)),
        detail: "The scope that the loader reads the root key's block in is not established."
            .into(),
    });
    let reader = Reader {
        numeric: GrammarProperty::Known(None),
        scoped_operand: GrammarProperty::Known(None),
        id: None,
        kind: ReaderKind::Block,
        family: crate::BlockFamily::Unknown,
    };
    let field = Field {
        name: key.into(),
        reader: reader.clone(),
        shape: SHAPE,
        read: vec![FieldReadAlternative {
            condition: FieldCondition::Always,
            outcome: FieldReadOutcome::Read {
                reader,
                shape: SHAPE,
            },
        }],
        members: FieldMembers::Fields(children),
        domain: FieldDomain::Unknown,
        uses: Vec::new(),
        reference: FieldReference::NotEstablished,
        entry_contexts: Vec::new(),
        read_scope: GrammarProperty::Unresolved,
        accepted_categories: AcceptedCategories::NotApplicable,
        requirement: crate::FieldRequirement::Unknown,
    };

    (field, field_gaps)
}

/// The gaps outside the method that [`reserved_key_field`] leaves out of `gaps`, the answer for
/// the class of the root key `key`: a member's gap at the same member below the key, and every
/// other gap as it is. A registry with no owner answer to repeat them keeps these.
pub(super) fn class_limits(gaps: &[Gap], key: &str) -> Vec<Gap> {
    gaps.iter()
        .filter(|gap| gap.kind == GapKind::OutsideMethod)
        .cloned()
        .map(|gap| match member_subject(&gap) {
            Some(_) => under_key(gap, key),
            None => gap,
        })
        .collect()
}

/// The owner's own fields in `fields`, an answer of `result`: the members of each root key when
/// the loader builds the owner, else the answer itself.
pub(super) fn owner_fields<'a>(
    fields: &'a [Field],
    result: &crate::engine::analysis::fields::RegistryFieldResult,
) -> &'a [Field] {
    if result.loader.is_none() {
        return fields;
    }

    match fields.first().map(|field| &field.members) {
        Some(FieldMembers::Fields(members)) => members,
        _ => &[],
    }
}

/// A root key reads one block; how a repeated key is stored is the loader rule's question.
const SHAPE: FieldShape = FieldShape {
    value: ValueShape::Block,
    repeat: RepeatBehavior::Unknown,
};

/// The storage gap of the root key `key`: each statement builds a new item, and the loader's
/// handling of a duplicate item decides which one is kept.
pub(super) fn repeat_gap(key: &str) -> Gap {
    Gap {
        kind: GapKind::UnresolvedStorage,
        subject: Some(GapSubject::field(key)),
        detail: "Repeat behavior belongs to the loader: each statement builds a new item, and \
                 the loader's handling of a duplicate item decides which one is kept."
            .into(),
    }
}

/// The reader of the constructed owner, read through its primary address point.
fn item_reader(loader: &LoaderKeys) -> Reader {
    Reader {
        numeric: GrammarProperty::Known(None),
        scoped_operand: GrammarProperty::Known(None),
        id: loader.item_reader.as_ref().map(persistent_reader_id),
        kind: ReaderKind::Block,
        family: loader
            .item_reader
            .as_ref()
            .map_or(crate::BlockFamily::Unknown, |reader| reader.family),
    }
}

/// Every path of the loader's switch that `key`'s token can take, including the rejected and
/// wider paths that the key's readers leave out.
fn key_paths<'a>(key: &RootField, loader: &'a LoaderKeys) -> Vec<&'a TokenPath> {
    loader
        .paths
        .iter()
        .filter(|path| path.domain[0] <= key.token && key.token <= path.domain[1])
        .collect()
}

fn builds_owner(path: &TokenPath) -> bool {
    matches!(path.outcome, PathOutcome::Reader(ReaderJoin::Joined { .. }))
}

/// The distinct outcomes of a key's `paths`. One outcome holds on every path; several are each
/// held under a condition that the method does not resolve.
fn read_alternatives(paths: &[&TokenPath], reader: &Reader) -> Vec<FieldReadAlternative> {
    let mut outcomes = Vec::new();

    for path in paths {
        let outcome = match &path.outcome {
            PathOutcome::Reader(ReaderJoin::Joined { .. }) => FieldReadOutcome::Read {
                reader: reader.clone(),
                shape: SHAPE,
            },
            PathOutcome::Rejected => FieldReadOutcome::Rejected,
            _ => FieldReadOutcome::Unresolved,
        };

        if !outcomes.contains(&outcome) {
            outcomes.push(outcome);
        }
    }

    let condition = if outcomes.len() == 1 {
        FieldCondition::Always
    } else {
        FieldCondition::Unresolved
    };

    outcomes
        .into_iter()
        .map(|outcome| FieldReadAlternative {
            condition: condition.clone(),
            outcome,
        })
        .collect()
}

/// The scopes that the constructor receives on a key's `paths`; a path that the walk could not
/// finish may build the owner in a scope that no path names.
fn read_scope(
    paths: &[&TokenPath],
    names: Option<&[String]>,
    key: &str,
    gaps: &mut Vec<Gap>,
) -> GrammarProperty<Vec<ReadScope>> {
    let scopes: Vec<Option<Value>> = paths
        .iter()
        .filter_map(|path| match &path.outcome {
            PathOutcome::Rejected => None,
            PathOutcome::Reader(ReaderJoin::Joined { arguments, .. }) => {
                Some(arguments.get("x1").cloned())
            }
            _ => Some(None),
        })
        .collect();

    normalize_scopes(scopes, names, GapSubject::field(key), gaps)
}

/// The member field path that a gap names, if it names one.
fn member_subject(gap: &Gap) -> Option<&GapSubject> {
    gap.subject.as_ref().filter(|subject| {
        matches!(
            subject,
            GapSubject::Field { .. } | GapSubject::KeyPath { .. }
        )
    })
}

/// A member's gap at the same member below the root key `key`.
fn under_key(mut gap: Gap, key: &str) -> Gap {
    gap.subject = match gap.subject {
        Some(GapSubject::Field { name }) => Some(GapSubject::field(format!("{key}.{name}"))),
        Some(GapSubject::KeyPath { mut path }) => {
            path.insert(0, key.into());
            Some(GapSubject::key_path(path))
        }
        subject => subject,
    };

    gap
}

/// One gap for the paths of the loader's switch that no named root key holds, and one for an input or
/// jump table that the walk could not read.
fn walk_gaps(loader: &LoaderKeys, registry: &str) -> Vec<Gap> {
    let held: BTreeSet<usize> = loader
        .keys
        .iter()
        .flat_map(|key| key.paths.iter().copied())
        .collect();
    let unheld_path = loader
        .gaps
        .iter()
        .any(|gap| gap.path.is_some_and(|path| !held.contains(&path)));
    let unread_input = loader.gaps.iter().any(|gap| gap.path.is_none());
    let gap = |detail: &str| Gap {
        kind: GapKind::UnresolvedPath,
        subject: Some(GapSubject::registry(registry)),
        detail: detail.into(),
    };
    let mut gaps = Vec::new();

    if unheld_path {
        gaps.push(gap(
            "A path through the loader's root switch could not be followed to its end, so \
             whether its key builds the owner is not established.",
        ));
    }

    if unread_input {
        gaps.push(gap(
            "The loader's root switch could not be read completely; a root key may be missing.",
        ));
    }

    gaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::stop::Unresolved;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn member() -> Field {
        serde_json::from_value(json!({
            "name": "trigger",
            "reader": { "id": null, "kind": "Block", "family": "Trigger",
                "numeric": "Unresolved", "scoped_operand": "Unresolved" },
            "shape": { "value": "Block", "repeat": "Replace" }, "read": [],
            "members": "None", "domain": "Unknown",
            "uses": [{ "id": "u", "condition": { "FieldZero": { "path": ["id"], "zero": true } } }],
            "reference": "NotEstablished", "entry_contexts": [], "read_scope": "Unresolved",
            "accepted_categories": "NotApplicable",
            "requirement": "Unknown"
        }))
        .unwrap()
    }

    fn path(token: i64, outcome: PathOutcome) -> TokenPath {
        TokenPath {
            domain: [token, token],
            conditions: vec![],
            instructions: vec![],
            terminal: 0,
            outcome,
        }
    }

    fn construction(scope: i64) -> ReaderJoin {
        ReaderJoin::Joined {
            callee: "CEvent::CEvent(EScopeType)".into(),
            arguments: BTreeMap::from([("x1".into(), Value::Constant(scope))]),
            tail: false,
        }
    }

    fn key(name: &str, token: i64, paths: Vec<usize>, readers: Vec<ReaderJoin>) -> RootField {
        RootField {
            name: name.into(),
            token,
            constructor: 0,
            paths,
            readers,
        }
    }

    fn loader() -> LoaderKeys {
        let missing = ReaderJoin::Missing(Unresolved::new("instruction"));
        LoaderKeys {
            keys: vec![
                key("country_event", 5, vec![0], vec![construction(4)]),
                key("event", 6, vec![1], vec![construction(0)]),
                key("namespace", 9, vec![3], vec![missing.clone()]),
            ],
            paths: vec![
                path(5, PathOutcome::Reader(construction(4))),
                path(6, PathOutcome::Reader(construction(0))),
                path(6, PathOutcome::Rejected),
                path(9, PathOutcome::Reader(missing)),
            ],
            gaps: vec![],
            item_reader: None,
        }
    }

    fn answer(loader: &LoaderKeys) -> (Vec<Field>, Vec<Gap>) {
        let names = ["none", "planet", "country"].map(String::from);
        let gaps = vec![
            Gap {
                kind: GapKind::UnresolvedReader,
                subject: Some(GapSubject::field("trigger")),
                detail: "member".into(),
            },
            Gap {
                kind: GapKind::OutsideMethod,
                subject: Some(GapSubject::registry("events")),
                detail: "registry".into(),
            },
        ];

        root_fields(vec![member()], gaps, loader, "events", Some(&names))
    }

    fn subjects(gaps: &[Gap], detail: &str) -> Vec<GapSubject> {
        gaps.iter()
            .filter(|gap| gap.detail == detail)
            .filter_map(|gap| gap.subject.clone())
            .collect()
    }

    #[test]
    fn a_reserved_key_holds_its_class_fields_and_their_gaps() {
        let gap = |kind, subject, detail: &str| Gap {
            kind,
            subject: Some(subject),
            detail: detail.into(),
        };
        let gaps = vec![
            gap(
                GapKind::UnresolvedReader,
                GapSubject::field("trigger"),
                "member",
            ),
            gap(
                GapKind::UnresolvedPath,
                GapSubject::registry("planets"),
                "path",
            ),
            gap(
                GapKind::OutsideMethod,
                GapSubject::registry("planets"),
                "scope",
            ),
        ];

        let (field, gaps) = reserved_key_field("random_list", vec![member()], gaps, "planets");

        assert_eq!(field.name, "random_list");
        assert_eq!(field.reader.kind, ReaderKind::Block);
        let FieldMembers::Fields(children) = &field.members else {
            panic!("{:?}", field.members);
        };
        assert_eq!(
            children[0].uses[0].condition,
            FieldCondition::FieldZero {
                path: vec!["random_list".into(), "id".into()],
                zero: true
            }
        );
        assert_eq!(
            subjects(&gaps, "member"),
            [GapSubject::field("random_list.trigger")]
        );
        assert_eq!(subjects(&gaps, "path"), [GapSubject::field("random_list")]);
        assert!(subjects(&gaps, "scope").is_empty());
        assert_eq!(field.read_scope, GrammarProperty::Unresolved);
    }

    #[test]
    fn each_accepted_key_is_a_root_field_read_in_its_scope_with_the_owner_fields() {
        let (fields, gaps) = answer(&loader());
        let names: Vec<_> = fields.iter().map(|field| field.name.as_str()).collect();

        assert_eq!(names, ["country_event", "event"]);
        let GrammarProperty::Known(scopes) = &fields[0].read_scope else {
            panic!("{:?}", fields[0].read_scope);
        };
        assert!(
            matches!(scopes.as_slice(), [ReadScope::Types(types)] if types.len() == 1 && types[0].name == "country")
        );
        assert_eq!(fields[1].read_scope, GrammarProperty::Known(vec![]));
        let FieldMembers::Fields(children) = &fields[0].members else {
            panic!("{:?}", fields[0].members);
        };
        assert_eq!(
            children[0].uses[0].condition,
            FieldCondition::FieldZero {
                path: vec!["country_event".into(), "id".into()],
                zero: true
            }
        );
        assert_eq!(
            subjects(&gaps, "member"),
            [
                GapSubject::field("country_event.trigger"),
                GapSubject::field("event.trigger")
            ]
        );
        assert_eq!(subjects(&gaps, "registry").len(), 1);
    }

    #[test]
    fn a_key_that_is_also_rejected_keeps_each_outcome_under_an_unresolved_condition() {
        let (fields, gaps) = answer(&loader());

        assert_eq!(fields[0].read[0].condition, FieldCondition::Always);
        let outcomes: Vec<_> = fields[1].read.iter().map(|read| &read.outcome).collect();
        assert!(matches!(
            outcomes.as_slice(),
            [FieldReadOutcome::Read { .. }, FieldReadOutcome::Rejected]
        ));
        assert!(
            fields[1]
                .read
                .iter()
                .all(|read| read.condition == FieldCondition::Unresolved)
        );
        assert!(
            gaps.iter()
                .any(|gap| gap.kind == GapKind::UnresolvedCondition
                    && gap.subject == Some(GapSubject::field("event")))
        );
    }

    #[test]
    fn a_wider_unresolved_path_over_a_key_leaves_its_read_and_scope_partial() {
        let mut loader = loader();
        loader.paths.push(TokenPath {
            domain: [4, 5],
            ..path(5, PathOutcome::Gap(Unresolved::new("instruction")))
        });
        let (fields, gaps) = answer(&loader);

        let outcomes: Vec<_> = fields[0].read.iter().map(|read| &read.outcome).collect();
        assert!(matches!(
            outcomes.as_slice(),
            [FieldReadOutcome::Read { .. }, FieldReadOutcome::Unresolved]
        ));
        assert!(
            matches!(fields[0].read_scope, GrammarProperty::Partial(ref scopes) if scopes.len() == 1)
        );
        assert!(gaps.iter().any(|gap| gap.detail.starts_with("read-scope")
            && gap.subject == Some(GapSubject::field("country_event"))));
    }

    #[test]
    fn a_key_built_in_no_scope_on_one_path_and_a_scope_on_another_is_partial() {
        let mut loader = loader();
        loader.paths.push(TokenPath {
            domain: [4, 5],
            ..path(5, PathOutcome::Reader(construction(0)))
        });
        let (fields, _) = answer(&loader);

        assert!(
            matches!(fields[0].read_scope, GrammarProperty::Partial(ref scopes) if scopes.len() == 1)
        );
    }

    #[test]
    fn the_owner_fields_of_a_loader_answer_are_the_members_of_its_keys() {
        let loader = loader();
        let (fields, _) = answer(&loader);
        let result = crate::engine::analysis::fields::RegistryFieldResult {
            loader: Some(loader),
            ..Default::default()
        };

        let names: Vec<_> = owner_fields(&fields, &result)
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        assert_eq!(names, ["trigger"]);
        assert_eq!(owner_fields(&fields, &Default::default()).len(), 2);
    }

    #[test]
    fn a_key_with_no_established_construction_is_a_gap_and_not_a_field() {
        let (_, gaps) = answer(&loader());

        assert!(gaps.iter().any(|gap| gap.detail.contains("`namespace`")));
    }

    #[test]
    fn a_loader_with_no_established_key_answers_no_owner_fields_as_roots() {
        let mut loader = loader();
        loader.keys.clear();
        let (fields, gaps) = answer(&loader);

        assert!(fields.is_empty());
        assert!(
            gaps.iter()
                .any(|gap| gap.detail.starts_with("No root key of the loader"))
        );
    }
}
