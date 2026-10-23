use super::*;
use crate::session::questions::READER_ALTERNATIVES_OPEN;
use crate::{Basis, BuildId, Source};
use serde_json::{Value, json};

const REGISTRY: &str = "common/things";

/// A row of `key` with the facts of a plain Boolean read; `facts` replaces any of them.
fn row(key: &str, facts: Value) -> Value {
    let mut row = json!({
        "token": 1, "key": key, "kind": "scalar", "value": "Scalar", "reader_kind": "Boolean",
        "repeat": "Replace", "conditional": false, "family": null, "items": null,
        "branches": null, "nested": null, "scope_mask": null, "enum": null, "lookup": null,
        "paths": {"leaves": 1, "unknown": 0, "truncated": false}, "store": null, "callee": null
    });

    for (name, value) in facts.as_object().unwrap() {
        row[name] = value.clone();
    }

    row
}

/// A complete reader that rejects a key it does not enumerate, with `rows`.
fn reader(name: &str, rows: Vec<Value>) -> Value {
    json!({
        "name": name, "symbol": null, "class": name.trim_end_matches("::ReadMember"),
        "walked": true, "error": null,
        "dispatch": {"form": "token-switch", "domain_enumerated": true,
                     "default": {"kind": "persistent", "reader": null, "followed": false},
                     "delegates": [], "key_text_read": false, "loops": 0, "truncated": false},
        "complete": true, "incomplete_because": [], "default_routed": [], "rows": rows
    })
}

/// A nested reader reference to the walked reader at `entry`.
fn nested(entry: &str, name: &str) -> Value {
    json!({"reader": entry, "name": name, "walked": true})
}

/// An export whose registry roots at the reader `0x1`, with `readers` by entry.
fn export(readers: Value) -> ReaderExport {
    let readers = json!({
        "schema": 2, "tool": "ghidra-export", "build": "test",
        "registries": {REGISTRY: {"roots": [{"key": "", "class": "CThing", "reader": "0x1",
                                            "error": null}], "error": null}},
        "readers": readers
    });
    let tables = json!({
        "schema": 1, "tool": "native-tables", "build": {"executable": "test", "version": "test"},
        "tokens": {}, "reader": {"key_text": 0, "value_token": 0},
        "scope_types": [{"bit": 1, "id": "planet-id", "name": "planet"},
                        {"bit": 2, "id": "country-id", "name": "country"}],
        "manual_exceptions": {"zero_mask_read_scope": [
            {"reader": "CScaled::ReadMember", "keys": ["limit"], "block": true,
             "read_scope": "unresolved", "decision": "D5(a)", "written_by": "hand"}
        ]}
    });

    ReaderExport::parse(ExportFiles {
        readers: Box::leak(readers.to_string().into_boxed_str()),
        tables: Box::leak(tables.to_string().into_boxed_str()),
    })
    .unwrap()
}

/// The member readers whose objects keep themselves on a persistent read: the settings block's.
fn settings_merges() -> MemberReaders {
    MemberReaders {
        merging: BTreeSet::from(["CSettings::ReadMember".to_owned()]),
        ids: BTreeMap::new(),
    }
}

/// A reading call of `symbol`, as the export names it.
fn callee(symbol: &str) -> Value {
    json!({"name": "callee", "address": "0x100", "symbol": symbol})
}

/// Native's answer with no field and only the method's own limit.
fn native_answer(fields: Vec<Field>, gaps: Vec<Gap>) -> Answer<Vec<Field>> {
    let mut gaps = gaps;

    gaps.push(Gap {
        kind: GapKind::OutsideMethod,
        subject: Some(GapSubject::registry(REGISTRY)),
        detail: "limit".into(),
    });

    Answer {
        value: fields,
        completeness: Completeness::from_gaps(&gaps),
        gaps,
        source: Source::new(
            BuildId("test".into()),
            "registry-fields/v75",
            Basis::StaticAnalysis,
        ),
    }
}

/// A field of Native's answer whose shared reader is identified but whose value form, shape,
/// members and read scope are not established.
fn identified_field(name: &str, members: FieldMembers) -> Field {
    Field {
        name: name.into(),
        reader: Reader {
            id: Some(crate::ReaderId::from_callee(name)),
            ..plain_reader(ReaderKind::Unknown, BlockFamily::Unknown)
        },
        shape: FieldShape {
            value: ValueShape::Unknown,
            repeat: RepeatBehavior::Unknown,
        },
        read: vec![FieldReadAlternative {
            condition: FieldCondition::Always,
            outcome: FieldReadOutcome::Unresolved,
        }],
        members,
        domain: FieldDomain::Unknown,
        uses: Vec::new(),
        reference: FieldReference::NotEstablished,
        entry_contexts: Vec::new(),
        read_scope: GrammarProperty::Unresolved,
        accepted_categories: AcceptedCategories::Unresolved,
        requirement: FieldRequirement::Unknown,
    }
}

/// Native's partial answer with the keys of [`complete_tree`]: each field's value form is open.
fn native_tree() -> Answer<Vec<Field>> {
    let size = identified_field("size", FieldMembers::Unresolved);
    let fields = vec![
        identified_field("enabled", FieldMembers::Unresolved),
        identified_field("settings", FieldMembers::Fields(vec![size])),
    ];
    let gaps = ["enabled", "settings", "settings.size"]
        .map(|name| Gap {
            kind: GapKind::ReaderSemantics,
            subject: Some(GapSubject::field(name)),
            detail: VALUE_FORM_OPEN.into(),
        })
        .to_vec();

    native_answer(fields, gaps)
}

/// `export` merged into [`native_tree`].
fn merged(export: &ReaderExport) -> Answer<Vec<Field>> {
    let mut answer = native_tree();

    assert!(export.merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer));

    answer
}

/// `export` merged into Native's answer with no field.
fn merged_into_nothing(export: &ReaderExport) -> Answer<Vec<Field>> {
    let mut answer = native_answer(Vec::new(), Vec::new());

    assert!(export.merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer));

    answer
}

fn names(fields: &[Field]) -> Vec<&str> {
    fields.iter().map(|field| field.name.as_str()).collect()
}

fn field<'a>(fields: &'a [Field], name: &str) -> &'a Field {
    fields
        .iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("no field {name}"))
}

fn members(field: &Field) -> &[Field] {
    match &field.members {
        FieldMembers::Fields(fields) => fields,
        other => panic!("{other:?}"),
    }
}

/// A complete tree: a root reader with a Boolean key and a block key whose nested reader reads
/// one more key. The block's literal scope mask answers its read scope.
fn complete_tree() -> Value {
    json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("enabled", json!({})),
            row("settings", json!({"token": 2, "kind": "persistent", "value": "Block",
                                   "reader_kind": "Block", "repeat": "Unknown",
                                   "scope_mask": 0,
                                   "nested": nested("0x2", "CSettings::ReadMember")})),
        ]),
        "0x2": reader("CSettings::ReadMember", vec![row("size", json!({"token": 3}))]),
    })
}

#[test]
fn a_complete_tree_gives_a_complete_answer_with_only_the_exported_keys() {
    let answer = merged(&export(complete_tree()));

    assert_eq!(
        answer.completeness,
        Completeness::Complete,
        "{:?}",
        answer.gaps
    );
    assert_eq!(names(&answer.value), ["enabled", "settings"]);
    assert_eq!(names(members(field(&answer.value, "settings"))), ["size"]);
    assert_eq!(answer.source.method, "registry-fields/ghidra-v2");
    assert_eq!(answer.source.basis, Basis::StaticAnalysis);
}

#[test]
fn a_key_outside_the_exported_tokens_is_absent_under_a_complete_answer() {
    let mut readers = complete_tree();

    readers["0x1"]["default_routed"] = json!([99]);

    let answer = merged(&export(readers));

    assert_eq!(answer.completeness, Completeness::Complete);
    assert!(answer.value.iter().all(|field| field.name != "unlisted"));
    assert_eq!(names(&answer.value), ["enabled", "settings"]);
}

#[test]
fn a_key_that_the_complete_base_reader_reads_is_a_field() {
    let mut readers = complete_tree();

    readers["0x1"]["dispatch"]["default"] =
        json!({"kind": "base-reader", "reader": "0x3", "followed": true});
    readers["0x3"] = reader(
        "CBase::ReadMember",
        vec![row("inherited", json!({"token": 4}))],
    );

    let answer = merged(&export(readers));

    assert_eq!(names(&answer.value), ["enabled", "settings", "inherited"]);
}

#[test]
fn one_incomplete_nested_reader_makes_the_answer_partial() {
    let mut readers = complete_tree();

    readers["0x2"]["complete"] = json!(false);
    readers["0x2"]["incomplete_because"] = json!(["loop-cut"]);

    let answer = merged(&export(readers));

    assert_eq!(answer.completeness, Completeness::Partial);
    assert!(
        answer.gaps.iter().any(|gap| gap.detail == VALUE_FORM_OPEN
            && gap.subject == Some(GapSubject::field("settings")))
    );
}

#[test]
fn a_truncated_reader_gives_a_partial_answer() {
    let mut readers = complete_tree();

    readers["0x1"]["dispatch"]["truncated"] = json!(true);
    readers["0x1"]["complete"] = json!(false);
    readers["0x1"]["incomplete_because"] = json!(["budget"]);

    let answer = merged(&export(readers));

    assert_eq!(answer.completeness, Completeness::Partial);
    assert!(
        answer
            .gaps
            .iter()
            .any(|gap| gap.kind == GapKind::UnresolvedPath
                && gap.subject == Some(GapSubject::registry(REGISTRY))
                && gap.detail.contains("a definition")
                && gap.detail.contains("budget"))
    );
}

#[test]
fn a_native_complete_answer_is_kept_when_the_export_tree_is_incomplete() {
    let mut readers = complete_tree();

    readers["0x2"]["complete"] = json!(false);
    readers["0x2"]["incomplete_because"] = json!(["loop-cut"]);

    let mut answer = native_tree();

    answer.gaps.retain(|gap| gap.kind == GapKind::OutsideMethod);
    answer.completeness = Completeness::Complete;
    export(readers).merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer);

    assert!(
        answer
            .gaps
            .iter()
            .all(|gap| !gap.detail.contains("loop-cut"))
    );
}

#[test]
fn an_all_zero_mask_reads_in_no_scope_and_a_mask_names_its_types() {
    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("potential", json!({"kind": "trigger", "value": "Block", "reader_kind": "Block",
                                    "family": "Trigger", "scope_mask": 0})),
            row("allow", json!({"token": 2, "kind": "trigger", "value": "Block",
                                "reader_kind": "Block", "family": "Trigger", "scope_mask": 6})),
        ]),
    })));

    assert_eq!(
        field(&answer.value, "potential").read_scope,
        GrammarProperty::Known(Vec::new())
    );

    let GrammarProperty::Known(scopes) = &field(&answer.value, "allow").read_scope else {
        panic!()
    };
    let [ReadScope::Types(types)] = scopes.as_slice() else {
        panic!("{scopes:?}")
    };

    assert_eq!(
        types
            .iter()
            .map(|scope| scope.name.as_str())
            .collect::<Vec<_>>(),
        ["planet", "country"]
    );
}

#[test]
fn a_zero_mask_exception_reader_keeps_unresolved_read_scope() {
    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("scaled_modifier", json!({"kind": "persistent", "value": "Block",
                                          "reader_kind": "Block",
                                          "nested": nested("0x2", "CScaled::ReadMember")})),
        ]),
        "0x2": reader("CScaled::ReadMember", vec![
            row("limit", json!({"kind": "trigger", "value": "Block", "reader_kind": "Block",
                                "family": "Trigger", "scope_mask": 0})),
        ]),
    })));
    let scaled = field(&answer.value, "scaled_modifier");
    let limit = field(members(scaled), "limit");

    for (field, path) in [
        (scaled, vec!["scaled_modifier"]),
        (limit, vec!["scaled_modifier", "limit"]),
    ] {
        let path: Vec<String> = path.into_iter().map(Into::into).collect();

        assert_eq!(field.read_scope, GrammarProperty::Unresolved);
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::OutsideMethod
                    && gap.subject == Some(field_subject(&path)))
        );
    }
}

#[test]
fn a_complete_keyword_table_lists_the_domain() {
    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("category", json!({"kind": "enum", "reader_kind": "Keyword",
                                   "enum": {"converter": "TokenToEnum<E>", "tokens": [7, 8],
                                            "keys": ["neutral", "negative"], "complete": true,
                                            "unknown": "silent"}})),
        ]),
    })));

    assert_eq!(
        field(&answer.value, "category").domain,
        FieldDomain::Listed(vec!["negative".into(), "neutral".into()])
    );
}

#[test]
fn a_persistent_read_into_the_owner_merges_a_repeat() {
    let answer = merged(&export(complete_tree()));

    assert_eq!(
        field(&answer.value, "settings").shape.repeat,
        RepeatBehavior::Merges
    );
}

#[test]
fn a_persistent_read_whose_object_may_reset_keeps_its_repeat_open() {
    let mut answer = native_tree();

    export(complete_tree()).merge(
        REGISTRY,
        RootLayout::Owner,
        &MemberReaders::default(),
        &mut answer,
    );

    assert_eq!(
        field(&answer.value, "settings").shape.repeat,
        RepeatBehavior::Unknown
    );
    assert!(
        answer
            .gaps
            .iter()
            .any(|gap| gap.kind == GapKind::UnresolvedStorage
                && gap.subject == Some(GapSubject::field("settings")))
    );
}

#[test]
fn a_block_reader_that_lists_no_key_proves_no_keys() {
    let mut readers = complete_tree();

    readers["0x2"] = reader("CPersistent::ReadMember", Vec::new());

    let export = export(readers);
    let failures = export.tree_failures(export.roots(REGISTRY).unwrap());

    assert!(failures.iter().any(|failure| failure.path == ["settings"]
        && failure.conditions.contains(&"no-listed-key".to_owned())));
    assert!(!export.proves_tree("0x2"));
}

#[test]
fn a_row_whose_value_branches_are_not_both_proved_is_any_with_an_unknown_reader_and_a_gap() {
    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("design", json!({"value": "Any", "reader_kind": null,
                                 "branches": [list_branch(true), boolean_branch(false)]})),
        ]),
    })));
    let design = field(&answer.value, "design");

    assert_eq!(design.shape.value, ValueShape::Any);
    assert_eq!(design.reader.kind, ReaderKind::Unknown);
    assert_eq!(design.read.len(), 1);
    assert_eq!(design.read[0].outcome, FieldReadOutcome::Unresolved);
    assert!(
        answer
            .gaps
            .iter()
            .any(|gap| gap.kind == GapKind::ReaderSemantics
                && gap.subject == Some(GapSubject::field("design"))
                && gap.detail == BRANCHES_OPEN)
    );
}

/// A value branch for `value_token` that reads with `facts`, which the export proves or not.
fn branch(value_token: &str, proved: bool, facts: Value) -> Value {
    let mut branch = json!({
        "value_token": value_token, "proved": proved, "kind": "scalar", "value": "Scalar",
        "reader_kind": "Boolean", "repeat": "Replace", "family": null, "items": null,
        "nested": null, "paths": {"leaves": 1, "unknown": 0, "truncated": false}, "callee": null
    });

    for (name, value) in facts.as_object().unwrap() {
        branch[name] = value.clone();
    }

    branch
}

/// A block branch that reads a list of strings.
fn list_branch(proved: bool) -> Value {
    branch(
        "block",
        proved,
        json!({"kind": "list", "value": "Block", "reader_kind": "Block", "items": "String",
               "repeat": "Accumulate"}),
    )
}

/// A scalar branch that reads a Boolean.
fn boolean_branch(proved: bool) -> Value {
    branch("other", proved, json!({}))
}

#[test]
fn proved_value_branches_give_one_alternative_for_each_value_form() {
    let uniform = "__ZN7CReader11ReadUniformI7CStringEEvR9CPdxArrayIT_iE";
    let mut list = list_branch(true);
    let mut boolean = boolean_branch(true);

    list["callee"] = callee(uniform);
    boolean["callee"] = callee("__ZN7CReader4ReadERb");

    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("upgrades", json!({"value": "Any", "reader_kind": null, "conditional": true,
                                   "paths": {"leaves": 2, "unknown": 2, "truncated": false},
                                   "branches": [list, boolean]})),
        ]),
    })));
    let upgrades = field(&answer.value, "upgrades");
    let conditions: Vec<_> = upgrades
        .read
        .iter()
        .map(|alternative| &alternative.condition)
        .collect();
    let readers: Vec<_> = upgrades
        .read
        .iter()
        .map(|alternative| match &alternative.outcome {
            FieldReadOutcome::Read { reader, shape } => {
                (reader.kind, shape.value, reader.id.clone())
            }
            other => panic!("{other:?}"),
        })
        .collect();
    let gaps: Vec<_> = answer
        .gaps
        .iter()
        .filter(|gap| gap.subject == Some(GapSubject::field("upgrades")))
        .map(|gap| gap.detail.as_str())
        .collect();

    assert_eq!(
        conditions,
        [
            &FieldCondition::ValueForm { block: true },
            &FieldCondition::ValueForm { block: false },
        ]
    );
    assert_eq!(
        readers,
        [
            (
                ReaderKind::Block,
                ValueShape::Block,
                Some(ReaderId::from_callee(&display_name(uniform)))
            ),
            (
                ReaderKind::Boolean,
                ValueShape::Scalar,
                Some(ReaderId::from_callee("CReader::Read(bool&)"))
            ),
        ]
    );
    assert_eq!(upgrades.reader.kind, ReaderKind::Unknown);
    assert_eq!(upgrades.reader.id, None);
    assert_eq!(upgrades.shape.value, ValueShape::Any);
    assert_eq!(
        upgrades.members,
        FieldMembers::Items {
            reader: plain_reader(ReaderKind::String, BlockFamily::NotApplicable)
        }
    );
    assert!(gaps.contains(&READER_ALTERNATIVES_OPEN), "{gaps:?}");
    assert!(
        !gaps.contains(&BRANCHES_OPEN)
            && !gaps.contains(&CONDITION_OPEN)
            && !gaps.contains(&PATHS_OPEN),
        "{gaps:?}"
    );
}

/// A field of Native's answer with two alternatives under unresolved conditions, with
/// `outcomes`, and Native's gap for them.
fn unresolved_conditions(name: &str, outcomes: [FieldReadOutcome; 2]) -> (Field, Gap) {
    let mut field = identified_field(name, FieldMembers::Unresolved);

    field.reader.id = None;
    field.read = outcomes
        .into_iter()
        .map(|outcome| FieldReadAlternative {
            condition: FieldCondition::Unresolved,
            outcome,
        })
        .collect();

    let gap = Gap {
        kind: GapKind::UnresolvedCondition,
        subject: Some(GapSubject::field(name)),
        detail: CONDITION_OPEN.into(),
    };

    (field, gap)
}

#[test]
fn native_alternatives_under_unresolved_conditions_take_the_value_forms_that_they_fit() {
    let block = FieldReadOutcome::Read {
        reader: plain_reader(ReaderKind::Block, BlockFamily::NotApplicable),
        shape: FieldShape {
            value: ValueShape::Block,
            repeat: RepeatBehavior::Unknown,
        },
    };
    let integer = FieldReadOutcome::Read {
        reader: plain_reader(ReaderKind::Integer, BlockFamily::NotApplicable),
        shape: FieldShape {
            value: ValueShape::Scalar,
            repeat: RepeatBehavior::Replace,
        },
    };
    let (upgrades, upgrades_gap) =
        unresolved_conditions("upgrades", [FieldReadOutcome::Unresolved, block.clone()]);
    let (count, count_gap) = unresolved_conditions("count", [block, integer]);
    let branches = |token| {
        json!({"token": token, "value": "Any", "reader_kind": null,
               "branches": [list_branch(true), boolean_branch(true)]})
    };
    let export = export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("upgrades", branches(1)),
            row("count", branches(2)),
        ]),
    }));
    let mut answer = native_answer(vec![upgrades, count], vec![upgrades_gap, count_gap]);

    assert!(export.merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer));

    let upgrades = field(&answer.value, "upgrades");
    let read: Vec<_> = upgrades
        .read
        .iter()
        .map(|alternative| match &alternative.outcome {
            FieldReadOutcome::Read { reader, .. } => (alternative.condition.clone(), reader.kind),
            other => panic!("{other:?}"),
        })
        .collect();
    let condition_gaps: Vec<_> = answer
        .gaps
        .iter()
        .filter(|gap| gap.kind == GapKind::UnresolvedCondition)
        .filter_map(|gap| gap.subject.clone())
        .collect();

    assert_eq!(
        read,
        [
            (
                FieldCondition::ValueForm { block: false },
                ReaderKind::Boolean
            ),
            (FieldCondition::ValueForm { block: true }, ReaderKind::Block),
        ]
    );
    assert_eq!(upgrades.shape.value, ValueShape::Any);
    // An integer outcome fits neither form of a block and a Boolean, so `count` keeps its
    // unresolved conditions.
    assert!(
        field(&answer.value, "count")
            .read
            .iter()
            .all(|alternative| alternative.condition == FieldCondition::Unresolved)
    );
    assert_eq!(condition_gaps, [GapSubject::field("count")]);
}

#[test]
fn native_value_branches_are_filled_only_where_native_names_no_shared_reader() {
    let export = export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("enabled", json!({"value": "Any", "reader_kind": null,
                                  "branches": [list_branch(true), boolean_branch(true)]})),
            row("settings", json!({"token": 2, "value": "Any", "reader_kind": null,
                                   "branches": [list_branch(true), boolean_branch(true)]})),
        ]),
    }));
    let mut enabled = identified_field("enabled", FieldMembers::Unresolved);
    let identified = identified_field("settings", FieldMembers::Unresolved);

    enabled.reader.id = None;

    let mut answer = native_answer(vec![enabled, identified], Vec::new());

    assert!(export.merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer));

    let conditions = |name| {
        field(&answer.value, name)
            .read
            .iter()
            .map(|alternative| alternative.condition.clone())
            .collect::<Vec<_>>()
    };

    assert_eq!(
        conditions("enabled"),
        [
            FieldCondition::ValueForm { block: true },
            FieldCondition::ValueForm { block: false },
        ]
    );
    assert_eq!(conditions("settings"), [FieldCondition::Always]);
}

#[test]
fn a_presence_row_reads_no_value_and_keeps_no_block() {
    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("marked", json!({"kind": "presence", "reader_kind": "Presence",
                                 "store": {"offset": 8, "size": 1}})),
        ]),
    })));
    let marked = field(&answer.value, "marked");

    assert_eq!(marked.reader.kind, ReaderKind::Presence);
    assert!(matches!(&marked.read[..], [alternative]
        if alternative.condition == FieldCondition::Always
            && matches!(&alternative.outcome, FieldReadOutcome::Read { reader, .. }
                if reader.kind == ReaderKind::Presence)));
    assert_eq!(marked.members, FieldMembers::None);
    assert_eq!(marked.read_scope, GrammarProperty::Known(Vec::new()));
}

#[test]
fn a_store_row_is_a_field_with_an_unresolved_read_and_a_gap() {
    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("flag", json!({"kind": "store", "value": null, "reader_kind": null,
                               "repeat": "Unknown", "store": {"offset": 8, "size": 1}})),
        ]),
    })));
    let flag = field(&answer.value, "flag");

    assert_eq!(flag.read[0].outcome, FieldReadOutcome::Unresolved);
    assert!(
        answer
            .gaps
            .iter()
            .any(|gap| gap.kind == GapKind::ReaderSemantics
                && gap.subject == Some(GapSubject::field("flag"))
                && gap.detail == STORE_OPEN)
    );
}

/// Native's `enabled` field with no shared reader, merged with a Boolean row of `facts` that names
/// its reading call, and the gaps that stay at the field.
fn merged_unidentified(facts: Value) -> (Field, Vec<String>) {
    let unresolved = Field {
        name: "enabled".into(),
        reader: plain_reader(ReaderKind::Unknown, BlockFamily::Unknown),
        shape: FieldShape {
            value: ValueShape::Unknown,
            repeat: RepeatBehavior::Unknown,
        },
        read: vec![FieldReadAlternative {
            condition: FieldCondition::Always,
            outcome: FieldReadOutcome::Unresolved,
        }],
        members: FieldMembers::Unresolved,
        domain: FieldDomain::Unknown,
        uses: Vec::new(),
        reference: FieldReference::NotEstablished,
        entry_contexts: Vec::new(),
        read_scope: GrammarProperty::Unresolved,
        accepted_categories: AcceptedCategories::Unresolved,
        requirement: FieldRequirement::Unknown,
    };
    let gap = |kind, detail: &str| Gap {
        kind,
        subject: Some(GapSubject::field("enabled")),
        detail: detail.into(),
    };
    let mut answer = native_answer(
        vec![unresolved],
        vec![
            gap(GapKind::UnresolvedReader, READER_ALTERNATIVES_OPEN),
            gap(GapKind::UnresolvedPath, "read-scope: reader-path"),
            gap(
                GapKind::UnresolvedStorage,
                "Repeat behavior waits for one shared reader. Nested fields remain unresolved.",
            ),
            gap(
                GapKind::UnresolvedStorage,
                "Scoped destination vtable is not established.",
            ),
            gap(GapKind::NumericConversion, "kept"),
        ],
    );

    let mut facts = facts;

    facts["callee"] = callee("__ZN7CReader4ReadERb");
    export(json!({
        "0x1": reader("CThing::ReadMember", vec![row("enabled", facts)]),
    }))
    .merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer);

    let remaining = answer
        .gaps
        .iter()
        .filter(|gap| gap.subject == Some(GapSubject::field("enabled")))
        .map(|gap| gap.detail.clone())
        .collect();

    (field(&answer.value, "enabled").clone(), remaining)
}

#[test]
fn a_native_gap_that_the_export_answers_is_removed_with_the_named_reader() {
    let (enabled, remaining) = merged_unidentified(json!({}));

    assert_eq!(enabled.reader.kind, ReaderKind::Boolean);
    assert_eq!(
        enabled.reader.id,
        Some(ReaderId::from_callee("CReader::Read(bool&)"))
    );
    assert_eq!(enabled.read_scope, GrammarProperty::Known(Vec::new()));
    assert_eq!(
        remaining,
        ["Scoped destination vtable is not established.", "kept"]
    );
}

#[test]
fn a_reading_that_some_path_may_miss_names_no_reader() {
    let (enabled, remaining) =
        merged_unidentified(json!({"paths": {"leaves": 2, "unknown": 1, "truncated": false}}));

    assert_eq!(enabled.reader.kind, ReaderKind::Boolean);
    assert_eq!(enabled.reader.id, None);
    assert!(
        remaining.iter().any(|gap| gap == READER_ALTERNATIVES_OPEN),
        "{remaining:?}"
    );
}

#[test]
fn a_field_that_native_reads_under_conditions_keeps_its_open_members() {
    let mut modification = identified_field("modification", FieldMembers::Unresolved);

    modification.read = [true, false]
        .map(|zero| FieldReadAlternative {
            condition: FieldCondition::FieldZero {
                path: vec!["other".into()],
                zero,
            },
            outcome: FieldReadOutcome::Unresolved,
        })
        .to_vec();

    let mut answer = native_answer(vec![modification], Vec::new());

    export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("modification", json!({"kind": "persistent", "value": "Block",
                                       "reader_kind": "Block", "conditional": true,
                                       "nested": nested("0x2", "CModification::ReadMember")})),
        ]),
        "0x2": reader("CModification::ReadMember", vec![row("add", json!({}))]),
    }))
    .merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer);

    let modification = field(&answer.value, "modification");

    assert_eq!(modification.members, FieldMembers::Unresolved);
    assert_eq!(modification.reader.kind, ReaderKind::Unknown);
}

#[test]
fn a_numeric_reader_that_the_export_classifies_keeps_a_conversion_gap() {
    let mut answer = native_answer(
        vec![identified_field("windup_time", FieldMembers::Unresolved)],
        Vec::new(),
    );

    export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("windup_time", json!({"reader_kind": "FixedPoint"})),
        ]),
    }))
    .merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer);

    assert_eq!(
        field(&answer.value, "windup_time").reader.kind,
        ReaderKind::FixedPoint
    );
    assert!(
        answer
            .gaps
            .iter()
            .any(|gap| gap.kind == GapKind::NumericConversion
                && gap.subject == Some(GapSubject::field("windup_time")))
    );
}

#[test]
fn a_member_reader_merges_and_is_named_only_as_every_object_with_it_agrees() {
    let holder = |read: Option<&str>, hook: &str| MemberHolder {
        member: "CBase::ReadMember(CReader&, int)".into(),
        reader: read.map(|read| crate::engine::analysis::fields::ConcreteReader {
            read: read.into(),
            member: "CBase::ReadMember(CReader&, int)".into(),
            post_read: [
                Some("CPersistent::InitPostRead()".into()),
                Some(hook.into()),
            ],
            family: BlockFamily::Unknown,
            delegate: None,
        }),
    };
    let base_hook = "CPersistent::InitPostRead(CString const&, int, int)";
    let base = || holder(Some("CPersistent::Read(CReader&)"), base_hook);
    let overridden_hook = holder(
        Some("CPersistent::Read(CReader&)"),
        "CDerived::InitPostRead(CString const&, int, int)",
    );
    let own_read = holder(Some("CDerived::Read(CReader&)"), base_hook);
    let unread = holder(None, base_hook);
    let members = || BTreeSet::from(["CBase::ReadMember", "CUnheld::ReadMember"]);
    let readers = |holders: &[MemberHolder]| member_readers(members(), holders);
    let base_id = super::super::fields::concrete_reader_id(
        "CPersistent::Read(CReader&)",
        "CBase::ReadMember(CReader&, int)",
    );

    assert_eq!(
        readers(&[base()]).merging,
        BTreeSet::from(["CBase::ReadMember".to_owned()])
    );
    assert_eq!(
        readers(&[base()]).ids,
        BTreeMap::from([("CBase::ReadMember".to_owned(), base_id.clone())])
    );

    let overridden = readers(&[base(), overridden_hook]);

    assert!(overridden.merging.is_empty());
    assert_eq!(overridden.ids.get("CBase::ReadMember"), Some(&base_id));
    assert!(readers(&[base(), own_read]).ids.is_empty());

    let unread = readers(&[base(), unread]);

    assert!(unread.merging.is_empty() && unread.ids.is_empty());
}

fn delegating(facts: Value) -> Answer<Vec<Field>> {
    let mut delegation = json!({"kind": "delegates", "value": null, "reader_kind": null,
                                "nested": nested("0x2", "CDescription::ReadMember")});

    for (name, value) in facts.as_object().unwrap() {
        delegation[name] = value.clone();
    }

    merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![row("description", delegation)]),
        "0x2": reader("CDescription::ReadMember", vec![
            row("description", json!({})),
            row("description_parameters", json!({"token": 2})),
        ]),
    })))
}

#[test]
fn an_uncertain_delegation_keeps_the_key_unresolved() {
    for facts in [
        json!({"conditional": true}),
        json!({"paths": {"leaves": 2, "unknown": 1, "truncated": false}}),
    ] {
        let answer = delegating(facts);
        let description = field(&answer.value, "description");

        assert_eq!(
            description.reader.kind,
            ReaderKind::Unknown,
            "{description:?}"
        );
        assert_eq!(description.read[0].outcome, FieldReadOutcome::Unresolved);
    }
}

#[test]
fn a_delegated_key_is_read_at_its_own_level() {
    let answer = delegating(json!({}));
    let description = field(&answer.value, "description");

    assert_eq!(names(&answer.value), ["description"]);
    assert_eq!(description.reader.kind, ReaderKind::Boolean);
    assert_eq!(description.members, FieldMembers::None);
}

#[test]
fn an_added_block_states_its_open_container_and_entry_contexts() {
    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("potential", json!({"kind": "trigger", "value": "Block", "reader_kind": "Block",
                                    "family": "Trigger", "scope_mask": 0})),
            row("list", json!({"token": 2, "kind": "list", "value": "Block",
                               "reader_kind": "Block", "items": "String"})),
            row("object", json!({"token": 3, "kind": "object", "value": "Block",
                                 "reader_kind": "Block"})),
        ]),
    })));
    let details_at = |name: &str| -> Vec<&str> {
        answer
            .gaps
            .iter()
            .filter(|gap| gap.subject == Some(GapSubject::field(name)))
            .map(|gap| gap.detail.as_str())
            .collect()
    };

    for name in ["potential", "list"] {
        assert_eq!(
            field(&answer.value, name).accepted_categories,
            AcceptedCategories::NotApplicable
        );
    }
    assert!(details_at("potential").contains(&ENTRY_CONTEXTS_OPEN));
    assert_eq!(
        field(&answer.value, "object").accepted_categories,
        AcceptedCategories::Unresolved
    );
    assert!(details_at("object").contains(&super::super::container_masks::CONTAINER_OPEN));
}

#[test]
fn a_scalar_reference_and_a_block_of_references_take_the_forms_of_their_shapes() {
    let reference = |value| FieldReadOutcome::Read {
        reader: plain_reader(ReaderKind::Reference, BlockFamily::NotApplicable),
        shape: FieldShape {
            value,
            repeat: RepeatBehavior::Unknown,
        },
    };
    let (targets, gap) = unresolved_conditions(
        "targets",
        [reference(ValueShape::Scalar), reference(ValueShape::Block)],
    );
    let export = export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("targets", json!({"value": "Any", "reader_kind": null, "branches": [
                branch("block", true, json!({"kind": "list", "value": "Block",
                                             "reader_kind": "Reference", "items": "Reference",
                                             "repeat": "Accumulate"})),
                branch("other", true, json!({"kind": "keyref", "reader_kind": "Reference"})),
            ]})),
        ]),
    }));
    let mut answer = native_answer(vec![targets], vec![gap]);

    assert!(export.merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer));

    let conditions: Vec<_> = field(&answer.value, "targets")
        .read
        .iter()
        .map(|alternative| alternative.condition.clone())
        .collect();

    assert_eq!(
        conditions,
        [
            FieldCondition::ValueForm { block: false },
            FieldCondition::ValueForm { block: true },
        ]
    );
}

#[test]
fn proved_branches_that_keep_nothing_read_in_no_scope_with_their_kind_facts() {
    let skipped = |value_token, value| {
        branch(
            value_token,
            true,
            json!({"kind": "skipped", "value": value, "reader_kind": "Skipped",
                   "repeat": "Unknown"}),
        )
    };
    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("legacy", json!({"value": "Any", "reader_kind": null,
                                 "branches": [skipped("block", "Block"), skipped("other", "Scalar")]})),
        ]),
    })));
    let legacy = field(&answer.value, "legacy");

    assert_eq!(legacy.read_scope, GrammarProperty::Known(Vec::new()));
    assert!(legacy.read.iter().all(|alternative| matches!(
        &alternative.outcome,
        FieldReadOutcome::Read { reader, .. }
            if reader.numeric == GrammarProperty::Known(None)
                && reader.scoped_operand == GrammarProperty::Known(None)
    )));
    assert!(
        !answer
            .gaps
            .iter()
            .any(|gap| gap.detail.starts_with(READ_SCOPE_PREFIX)),
        "{:?}",
        answer.gaps
    );
}

#[test]
fn a_pair_is_not_merged_over_a_known_condition_or_a_shared_reader() {
    let pair = |token| {
        json!({"token": token, "value": "Any", "reader_kind": null,
               "branches": [list_branch(true), boolean_branch(true)]})
    };
    let export = export(json!({
        "0x1": reader("CThing::ReadMember", vec![row("flagged", pair(1)), row("shared", pair(2))]),
    }));
    let mut flagged = identified_field("flagged", FieldMembers::Unresolved);
    let condition = FieldCondition::FieldZero {
        path: vec!["enabled".into()],
        zero: false,
    };

    flagged.reader.id = None;
    flagged.read[0].condition = condition.clone();

    let shared = identified_field("shared", FieldMembers::Unresolved);
    let value_form_gap = Gap {
        kind: GapKind::ReaderSemantics,
        subject: Some(GapSubject::field("shared")),
        detail: VALUE_FORM_OPEN.into(),
    };
    let mut answer = native_answer(vec![flagged, shared], vec![value_form_gap.clone()]);

    assert!(export.merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer));
    assert_eq!(field(&answer.value, "flagged").read[0].condition, condition);
    assert_eq!(
        field(&answer.value, "shared").read[0].condition,
        FieldCondition::Always
    );
    assert!(answer.gaps.contains(&value_form_gap), "{:?}", answer.gaps);
}

#[test]
fn a_native_reader_whose_kind_the_export_fills_takes_the_facts_of_that_kind() {
    let answer = merged(&export(complete_tree()));
    let enabled = field(&answer.value, "enabled");
    let FieldReadOutcome::Read { reader, .. } = &enabled.read[0].outcome else {
        panic!("{:?}", enabled.read);
    };

    for reader in [&enabled.reader, reader] {
        assert_eq!(reader.kind, ReaderKind::Boolean);
        assert_eq!(reader.numeric, GrammarProperty::Known(None));
        assert_eq!(reader.scoped_operand, GrammarProperty::Known(None));
    }
}

#[test]
fn a_pair_takes_its_container_and_entry_contexts_from_its_block_form() {
    let trigger = branch(
        "block",
        true,
        json!({"kind": "trigger", "value": "Block", "reader_kind": "Block", "family": "Trigger"}),
    );
    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("potential", json!({"value": "Any", "reader_kind": null,
                                    "branches": [trigger, boolean_branch(true)]})),
            row("design", json!({"token": 2, "value": "Any", "reader_kind": null,
                                 "branches": [list_branch(true), boolean_branch(true)]})),
        ]),
    })));
    let details_at = |name: &str| -> Vec<&str> {
        answer
            .gaps
            .iter()
            .filter(|gap| gap.subject == Some(GapSubject::field(name)))
            .map(|gap| gap.detail.as_str())
            .collect()
    };

    assert_eq!(
        field(&answer.value, "potential").accepted_categories,
        AcceptedCategories::NotApplicable
    );
    assert!(details_at("potential").contains(&ENTRY_CONTEXTS_OPEN));
    assert_eq!(
        field(&answer.value, "design").accepted_categories,
        AcceptedCategories::NotApplicable
    );
    assert!(!details_at("design").contains(&ENTRY_CONTEXTS_OPEN));
}

#[test]
fn a_native_field_that_takes_a_list_pair_reads_in_no_scope_and_no_container() {
    let block = FieldReadOutcome::Read {
        reader: plain_reader(ReaderKind::Block, BlockFamily::NotApplicable),
        shape: FieldShape {
            value: ValueShape::Block,
            repeat: RepeatBehavior::Unknown,
        },
    };
    let (upgrades, condition_gap) =
        unresolved_conditions("upgrades", [FieldReadOutcome::Unresolved, block]);
    let container_gap = Gap {
        kind: GapKind::UnresolvedPath,
        subject: Some(GapSubject::path(&["upgrades".to_owned()])),
        detail: super::super::container_masks::CONTAINER_OPEN.into(),
    };
    let export = export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("upgrades", json!({"value": "Any", "reader_kind": null,
                                   "branches": [list_branch(true), boolean_branch(true)]})),
        ]),
    }));
    let mut answer = native_answer(vec![upgrades], vec![condition_gap, container_gap.clone()]);

    assert!(export.merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer));

    let upgrades = field(&answer.value, "upgrades");

    assert!(matches!(upgrades.members, FieldMembers::Items { .. }));
    assert_eq!(upgrades.read_scope, GrammarProperty::Known(Vec::new()));
    assert_eq!(
        upgrades.accepted_categories,
        AcceptedCategories::NotApplicable
    );
    assert!(!answer.gaps.contains(&container_gap), "{:?}", answer.gaps);
}

#[test]
fn a_serializer_is_named_by_its_member_reader_as_the_walker_names_it() {
    let serializer = |key: &str, entry: &str| {
        row(
            key,
            json!({"kind": "serializer", "value": "Block", "reader_kind": "Block",
                        "repeat": "Unknown", "nested": nested(entry, "CTerm::CSerializer::ReadMember")}),
        )
    };
    let mut member = reader(
        "CTerm::CSerializer::ReadMember",
        vec![row("value", json!({}))],
    );
    let mut keyless = reader("CPersistent::ReadMember", Vec::new());

    member["symbol"] = json!("__ZN5CTerm11CSerializer10ReadMemberER7CReaderi");
    keyless["symbol"] = json!("__ZN11CPersistent10ReadMemberER7CReaderi");

    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![serializer("term", "0x2"), serializer("modifier", "0x3")]),
        "0x2": member,
        "0x3": keyless,
    })));

    assert_eq!(
        field(&answer.value, "term").reader.id,
        Some(ReaderId::from_callee(
            "CTerm::CSerializer::ReadMember(CReader&, int)"
        ))
    );
    assert_eq!(field(&answer.value, "modifier").reader.id, None);
}

#[test]
fn each_file_must_have_the_schema_that_this_release_reads() {
    let files = |readers: u32, tables: u32| {
        let readers = json!({"schema": readers, "tool": "ghidra-export", "build": "test",
                             "registries": {}, "readers": {}});
        let tables = json!({"schema": tables, "scope_types": [],
                            "manual_exceptions": {"zero_mask_read_scope": []}});

        ExportFiles {
            readers: Box::leak(readers.to_string().into_boxed_str()),
            tables: Box::leak(tables.to_string().into_boxed_str()),
        }
    };

    assert!(ReaderExport::parse(files(2, 1)).is_ok());
    assert!(ReaderExport::parse(files(1, 1)).is_err());
    assert!(ReaderExport::parse(files(2, 2)).is_err());
}
