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
        "paths": {"leaves": 1, "unknown": 0, "truncated": false}, "store": null
    });

    for (name, value) in facts.as_object().unwrap() {
        row[name] = value.clone();
    }

    row
}

/// A complete reader that rejects a key it does not enumerate, with `rows`.
fn reader(name: &str, rows: Vec<Value>) -> Value {
    json!({
        "name": name, "class": name.trim_end_matches("::ReadMember"), "walked": true,
        "error": null,
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
        "schema": 1, "tool": "ghidra-export", "build": "test",
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
fn settings_merges() -> BTreeSet<String> {
    BTreeSet::from(["CSettings::ReadMember".to_owned()])
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
    assert_eq!(answer.source.method, "registry-fields/ghidra-v1");
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

    export(complete_tree()).merge(REGISTRY, RootLayout::Owner, &BTreeSet::new(), &mut answer);

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
fn a_row_that_tests_the_value_token_is_any_with_an_unknown_reader_and_a_gap() {
    let answer = merged_into_nothing(&export(json!({
        "0x1": reader("CThing::ReadMember", vec![
            row("design", json!({"value": "Any", "reader_kind": null,
                                 "branches": [{"value_token": "block", "kind": "object"},
                                              {"value_token": "other", "kind": "scalar"}]})),
        ]),
    })));
    let design = field(&answer.value, "design");

    assert_eq!(design.shape.value, ValueShape::Any);
    assert_eq!(design.reader.kind, ReaderKind::Unknown);
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

#[test]
fn a_native_gap_that_the_export_answers_is_removed_and_the_reader_identity_stays_open() {
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

    export(complete_tree()).merge(REGISTRY, RootLayout::Owner, &settings_merges(), &mut answer);

    let enabled = field(&answer.value, "enabled");
    let remaining: Vec<_> = answer
        .gaps
        .iter()
        .filter(|gap| gap.subject == Some(GapSubject::field("enabled")))
        .map(|gap| gap.detail.as_str())
        .collect();

    assert_eq!(enabled.reader.kind, ReaderKind::Boolean);
    assert_eq!(enabled.read_scope, GrammarProperty::Known(Vec::new()));
    assert_eq!(
        remaining,
        [
            READER_ALTERNATIVES_OPEN,
            "Scoped destination vtable is not established.",
            "kept"
        ]
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
fn a_member_reader_merges_only_when_every_object_with_it_keeps_itself() {
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
    let unread = holder(None, base_hook);
    let members = || BTreeSet::from(["CBase::ReadMember", "CUnheld::ReadMember"]);

    assert_eq!(
        merging_members(members(), &[base()]),
        BTreeSet::from(["CBase::ReadMember".to_owned()])
    );
    assert!(merging_members(members(), &[base(), overridden_hook]).is_empty());
    assert!(merging_members(members(), &[base(), unread]).is_empty());
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
