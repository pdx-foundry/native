//! Normalize command grammar without promoting a partial property to a complete grammar.
use super::{Native, questions::error};
use crate::engine::analysis::references::{
    ReferenceFacts,
    initialization::{Initialization, InitializationLookup},
};
use crate::engine::analysis::{
    declarations::{self, Site},
    grammar,
    stop::Unresolved,
};
use crate::{
    Answer, Basis, BlockFamily, CommandGrammar, DeclarationKind, Error, Gap, GapKind, GapSubject,
    GrammarProperty, Operation, Reader, ReaderKind, Source,
};
use std::collections::BTreeSet;

impl Native {
    /// Extract the child grammar of a registered trigger or effect without starting the game.
    ///
    /// Every property retains its own unresolved or partial state. A known child key does not
    /// establish the whole grammar, parser acceptance, storage behavior, or runtime meaning.
    /// `limit` and other child keys are queried through their owning command.
    pub fn command_grammar(
        &self,
        kind: DeclarationKind,
        name: &str,
    ) -> Result<Answer<CommandGrammar>, Error> {
        let subject = recorded_subject(kind, name);
        self.answer("command_grammar", Some(&subject), || {
            let result = self.command_grammar_result(kind, name)?;
            let references = self.reference_facts(Operation::CommandGrammar)?;
            Ok(normalize(result.as_ref(), name, self.build(), references))
        })
    }

    /// The grammar method's own result for a registered command: its analysis, or the
    /// obstruction that stopped the command's receiver join.
    pub(crate) fn command_grammar_result(
        &self,
        kind: DeclarationKind,
        name: &str,
    ) -> Result<Result<grammar::GrammarResult, Unresolved>, Error> {
        let operation = Operation::CommandGrammar;
        let (input, declarations) = self
            .declaration_analysis(operation)?
            .grammar_input(kind)
            .map_err(|failure| error(operation, failure))?;
        match registered_factory(declarations, name) {
            Ok(Some(factory)) => Ok(grammar::analyze(input, factory)),
            Ok(None) => Err(Error::UnknownCommand {
                kind,
                name: name.into(),
            }),
            Err(stop) => Ok(Err(stop)),
        }
    }
}

/// Encode only non-plain names in a separate namespace so no path normalization aliases them.
fn recorded_subject(kind: DeclarationKind, name: &str) -> String {
    let plain = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if plain {
        format!("{}/{}", kind.subject(), name)
    } else {
        let encoded: String = name.bytes().map(|byte| format!("{byte:02x}")).collect();
        format!("{}/encoded/x{encoded}", kind.subject())
    }
}

pub(super) fn registered_factory(
    declarations: &declarations::DeclarationResult,
    name: &str,
) -> Result<Option<u64>, Unresolved> {
    let mut factories = BTreeSet::new();
    for (_, site) in &declarations.sites {
        match site {
            Site::Declared {
                name: found,
                factory,
                ..
            } if found == name => {
                factories.insert(*factory);
            }
            Site::Unreadable {
                name: Some(found), ..
            } if found == name => {
                return Err(Unresolved::new("command-registration"));
            }
            _ => {}
        }
    }
    if factories.len() > 1 {
        return Err(Unresolved::new("ambiguous-command-factory"));
    }
    if declarations.sites.iter().any(|(_, site)| {
        matches!(
            site,
            Site::Unreadable { name: None, .. } | Site::RuntimeToken { .. }
        )
    }) {
        return Err(Unresolved::new("incomplete-command-inventory"));
    }
    Ok(factories.first().copied())
}

pub(super) fn normalize(
    result: Result<&grammar::GrammarResult, &Unresolved>,
    name: &str,
    build: crate::BuildId,
    references: &ReferenceFacts,
) -> Answer<CommandGrammar> {
    let mut value = CommandGrammar {
        forms: GrammarProperty::Unresolved,
        reader: Reader {
            id: None,
            kind: ReaderKind::Unknown,
            family: BlockFamily::Unknown,
        },
        child_families: GrammarProperty::Unresolved,
        fixed_keys: GrammarProperty::Unresolved,
        numeric_keys: GrammarProperty::Unresolved,
        ordering: GrammarProperty::Unresolved,
    };
    let mut gaps = Vec::new();
    let mut key_gaps = Vec::new();
    let mut gap = |kind, detail: String| {
        let gap = Gap {
            kind,
            subject: Some(GapSubject::answer_item(name)),
            detail,
        };
        if !gaps.contains(&gap) {
            gaps.push(gap);
        }
    };
    match result {
        Err(stop) => gap(GapKind::UnresolvedReader, stop.reason.into()),
        Ok(result) => {
            let identity =
                super::fields::concrete_reader_id(&result.reader_name, &result.member_name);
            value.reader = Reader {
                id: Some(identity),
                kind: result.reader_kind,
                family: result.reader_family,
            };
            if let Some(forms) = &result.forms {
                let mut accepted = Vec::new();
                if forms.block {
                    accepted.push(crate::CommandForm::Block);
                }
                for alternative in &forms.alternatives {
                    if alternative.accepted {
                        accepted.push(crate::CommandForm::Value(form_value(
                            &alternative.value,
                            references,
                        )));
                    }
                    if !alternative.accepted
                        && alternative
                            .paths
                            .iter()
                            .any(|path| path.class != grammar::forms::PathClass::Rejecting)
                    {
                        let causes: BTreeSet<_> = alternative
                            .paths
                            .iter()
                            .flat_map(|path| &path.stages)
                            .filter_map(|stage| stage.cause.map(|cause| (stage.stage, cause)))
                            .collect();
                        if causes.is_empty() {
                            gap(
                                GapKind::UnresolvedPath,
                                "value-acceptance: Read: mixed paths or unknown reader kind".into(),
                            );
                        }
                        for (stage, cause) in causes {
                            gap(
                                GapKind::UnresolvedPath,
                                format!("value-acceptance: {stage:?}: {cause}"),
                            );
                        }
                    }
                }
                if forms.receiver_state {
                    gap(GapKind::UnresolvedPath, "receiver-state".into());
                }
                for stop in &forms.stops {
                    gap(GapKind::UnresolvedPath, stop.reason.into());
                }
                if forms.complete {
                    if let [only] = accepted.as_slice() {
                        value.reader.kind = match only {
                            crate::CommandForm::Block => ReaderKind::Block,
                            crate::CommandForm::Value(value) => value.reader.kind,
                        };
                    }
                    value.forms = GrammarProperty::Known(accepted);
                } else {
                    value.forms = GrammarProperty::Partial(accepted);
                }
            }
            let no_children = matches!(&value.forms, GrammarProperty::Known(forms) if !forms.contains(&crate::CommandForm::Block));
            if no_children {
                value.child_families = GrammarProperty::Known(vec![]);
                value.fixed_keys = GrammarProperty::Known(vec![]);
                value.numeric_keys = GrammarProperty::Known(None);
                value.ordering = GrammarProperty::Known(vec![]);
            } else {
                let initialization = result
                    .initializer
                    .as_ref()
                    .ok()
                    .and_then(|name| references.initializers.get(name));
                let lookup = match initialization {
                    Some(Initialization::Lookup(lookup)) => Some(lookup),
                    _ => None,
                };
                let keys = super::fields::grammar_fields(
                    &result.fields.fields,
                    &result.fields.paths,
                    references,
                    lookup,
                );
                key_gaps = reference_gaps(result, references, lookup);
                if let Some((kind, detail)) = initialization_gap(result, initialization) {
                    gap(kind, detail);
                }
                if !keys.is_empty() {
                    value.fixed_keys = GrammarProperty::Partial(keys);
                }
                if !result.families.is_empty() {
                    value.child_families = GrammarProperty::Partial(result.families.clone());
                }
                if !result.ordering.is_empty() {
                    value.ordering = GrammarProperty::Partial(
                        result
                            .ordering
                            .iter()
                            .map(|rule| {
                                let outcome = match &rule.outcome {
                                    grammar::OrderOutcome::Reader(join) => {
                                        let joins = std::slice::from_ref(join);
                                        crate::ChildOrderOutcome::Read(super::fields::reader(joins))
                                    }
                                    grammar::OrderOutcome::Family(family) => {
                                        crate::ChildOrderOutcome::Dispatch(*family)
                                    }
                                };
                                crate::ChildOrderRule {
                                    child: rule.child.clone(),
                                    conditions: rule.conditions.clone(),
                                    outcome,
                                }
                            })
                            .collect(),
                    );
                }
                if let Some(child) = &result.numeric {
                    let child = normalize(Ok(child), name, build.clone(), references);
                    value.numeric_keys = GrammarProperty::Partial(Some(Box::new(child.value)));
                    for child_gap in child.gaps {
                        gap(child_gap.kind, child_gap.detail);
                    }
                }
                for stop in &result.stops {
                    gap(GapKind::UnresolvedPath, stop.reason.into());
                }
                if !result.fields.gaps.is_empty() {
                    gap(
                        GapKind::UnresolvedPath,
                        "Some child dispatch paths or names remain unresolved.".into(),
                    );
                }
            }
        }
    }
    if !properties_known(&value) {
        gap(
        GapKind::ReaderSemantics,
        "Child grammar extraction is incomplete; unresolved properties and conditional paths remain."
            .into(),
    );
    }
    gap(GapKind::OutsideMethod, "Required keys, key combinations, defaults, value domains, occurrence limits, operators, child scopes, numeric grammar, weights and runtime meaning are outside this method.".into());
    for key_gap in key_gaps {
        if !gaps.contains(&key_gap) {
            gaps.push(key_gap);
        }
    }
    Answer {
        value,
        completeness: crate::Completeness::from_gaps(&gaps),
        gaps,
        source: Source::new(build, grammar::METHOD, Basis::StaticAnalysis),
    }
}

fn properties_known(value: &CommandGrammar) -> bool {
    matches!(value.forms, GrammarProperty::Known(_))
        && matches!(value.child_families, GrammarProperty::Known(_))
        && matches!(value.fixed_keys, GrammarProperty::Known(_))
        && matches!(value.numeric_keys, GrammarProperty::Known(_))
        && matches!(value.ordering, GrammarProperty::Known(_))
}

fn form_value(
    value: &grammar::forms::ValueForm,
    references: &ReferenceFacts,
) -> crate::CommandValue {
    let mut reader = value
        .reader
        .as_ref()
        .map(|join| super::fields::reader(std::slice::from_ref(join)))
        .unwrap_or(Reader {
            id: None,
            kind: value.kind,
            family: BlockFamily::NotApplicable,
        });
    reader.kind = value.kind;
    let reference = if let Some(lookup) = &value.initialization {
        crate::FieldReference::Lookups(vec![super::fields::reference_lookup(
            crate::FieldCondition::Always,
            lookup.directory.clone(),
            Some(&lookup.lookup),
        )])
    } else if let Some(crate::engine::analysis::fields::ReaderJoin::Joined { callee, .. }) =
        &value.reader
    {
        match references.readers.get(callee) {
            Some(lookup) => crate::FieldReference::Lookups(vec![super::fields::reference_lookup(
                crate::FieldCondition::Always,
                lookup.directory.clone(),
                lookup.lookup.as_ref().ok(),
            )]),
            None => crate::FieldReference::NotEstablished,
        }
    } else {
        crate::FieldReference::NotEstablished
    };
    crate::CommandValue { reader, reference }
}

/// One gap for each child key whose reference lookups are not fully established: the lookups of
/// its reader, and the receiver initializer's lookup of the string it stores.
fn reference_gaps(
    result: &grammar::GrammarResult,
    references: &ReferenceFacts,
    initialization: Option<&InitializationLookup>,
) -> Vec<Gap> {
    result
        .fields
        .fields
        .iter()
        .flat_map(|field| {
            let joined = initialization
                .filter(|lookup| super::fields::stores_initialization_key(field, lookup));
            let details = [
                super::fields::reference_gap(field, references),
                joined.and_then(super::fields::initialization_gap),
            ];
            details.into_iter().flatten().map(|detail| Gap {
                kind: GapKind::ReaderSemantics,
                subject: Some(GapSubject::field(field.name.clone())),
                detail,
            })
        })
        .collect()
}

/// Why the receiver initializer's lookup is not established or joins no child key.
fn initialization_gap(
    result: &grammar::GrammarResult,
    initialization: Option<&Initialization>,
) -> Option<(GapKind, String)> {
    match initialization {
        None => Some((
            GapKind::UnreadableInput,
            "The receiver's initializer could not be read, so a lookup that it makes is not \
             established."
                .into(),
        )),
        Some(Initialization::NoLookup) => None,
        Some(Initialization::Unresolved(stop)) => Some((
            GapKind::ReaderSemantics,
            format!(
                "The receiver's initializer reads a global instance, but no lookup of it is \
                 established: {}.",
                initialization_obstacle(stop.reason)
            ),
        )),
        Some(Initialization::Lookup(lookup)) => {
            let joined = result
                .fields
                .fields
                .iter()
                .any(|field| super::fields::stores_initialization_key(field, lookup));
            (!joined).then(|| {
                (
                    GapKind::ReaderSemantics,
                    "The receiver's initializer looks up a stored key that no child key's string \
                     reader is joined to."
                        .into(),
                )
            })
        }
    }
}

fn initialization_obstacle(reason: &str) -> &'static str {
    match reason {
        "initializer-several-lookups" => "the initializer holds several lookups",
        "initializer-null-type" => "the null object's type differs from the collection's items",
        "initializer-null-object" => "a key that selects no item selects no typed null object",
        "initializer-string-layout" => "the compared offsets do not form one string layout",
        "initializer-getter-shape" | "initializer-getter-body" => {
            "no qualified lookup shape matched the getter that it calls"
        }
        _ => "no qualified lookup shape matched the initializer",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::{
        declarations::{DeclarationResult, ScopeOutcome},
        stop::Unresolved,
    };

    const INITIALIZER: &str = "CEntry::PostInit()";

    fn facts(initialization: Initialization) -> ReferenceFacts {
        ReferenceFacts {
            readers: Default::default(),
            initializers: [(INITIALIZER.to_owned(), initialization)].into(),
        }
    }

    /// A receiver whose child key `district_type` stores a string at `this+0xa8`.
    fn keyed(initializer: Result<String, Unresolved>) -> grammar::GrammarResult {
        use crate::engine::analysis::fields::{
            PathOutcome, ReaderJoin, RootField, TokenPath, Value,
        };

        let join = ReaderJoin::Joined {
            callee: "CReader::Read(CString&, bool)".into(),
            arguments: [
                ("x0".into(), Value::Reader(0)),
                ("x1".into(), Value::Owner(0xa8)),
            ]
            .into(),
            tail: true,
        };
        grammar::GrammarResult {
            forms: None,
            reader: declarations::CommandReader {
                vtable: 1,
                read: 2,
                member: 3,
            },
            delegates: Default::default(),
            reader_name: "CEffect::Read(CReader&, EScopeType)".into(),
            reader_kind: ReaderKind::Block,
            reader_family: BlockFamily::Effect,
            member_name: "CEntry::ReadMember(CReader&, int, EScopeType)".into(),
            initializer,
            numeric: None,
            ordering: vec![],
            families: vec![],
            stops: vec![],
            fields: grammar::ChildFields {
                fields: vec![RootField {
                    name: "district_type".into(),
                    token: 7,
                    constructor: 0,
                    paths: vec![0],
                    readers: vec![join.clone()],
                }],
                paths: vec![TokenPath {
                    domain: [7, 7],
                    conditions: vec![],
                    instructions: vec![],
                    terminal: 0,
                    outcome: PathOutcome::Reader(join),
                }],
                gaps: vec![],
            },
        }
    }

    /// An initialization lookup of the key string at `this+key_offset`.
    fn scan_at(key_offset: i64) -> Initialization {
        use crate::engine::analysis::references::{KeyMatch, Lookup, Stage};

        Initialization::Lookup(InitializationLookup {
            database: "CDistrictTypeDatabase".into(),
            directory: Some("common/districts".into()),
            key_offset,
            item_offset: key_offset + 0x28,
            lookup: Lookup {
                stage: Stage::OwnerInitialization,
                key_match: Some(KeyMatch::FirstEqual),
                empty_key_looked_up: Some(true),
                missing_yields_null: Some(true),
            },
        })
    }

    fn key_reference(answer: &Answer<CommandGrammar>) -> crate::FieldReference {
        let GrammarProperty::Partial(keys) = &answer.value.fixed_keys else {
            panic!("no fixed keys");
        };

        keys[0].reference.clone()
    }

    fn joins_no_key(answer: &Answer<CommandGrammar>) -> bool {
        answer.gaps.iter().any(|gap| {
            gap.detail
                .contains("no child key's string reader is joined to")
        })
    }

    fn with_forms(complete: bool) -> grammar::GrammarResult {
        let mut result = keyed(Ok(INITIALIZER.into()));
        result.families = vec![BlockFamily::Effect];
        result.forms = Some(std::sync::Arc::new(grammar::forms::Result {
            key: grammar::forms::CacheKey {
                functions: [None; 6],
                receiver: Default::default(),
            },
            block: false,
            alternatives: vec![],
            complete,
            receiver_state: false,
            stops: vec![],
        }));
        result
    }

    #[test]
    fn known_value_only_forms_remove_inherited_children_and_incomplete_gap() {
        let answer = normalize(
            Ok(&with_forms(true)),
            "example",
            crate::BuildId("authored".into()),
            &ReferenceFacts::default(),
        );
        assert_eq!(answer.value.child_families, GrammarProperty::Known(vec![]));
        assert_eq!(answer.value.fixed_keys, GrammarProperty::Known(vec![]));
        assert_eq!(answer.value.numeric_keys, GrammarProperty::Known(None));
        assert_eq!(answer.value.ordering, GrammarProperty::Known(vec![]));
        assert_eq!(answer.completeness, crate::Completeness::Complete);
        assert!(
            !answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::ReaderSemantics)
        );
    }

    #[test]
    fn incomplete_forms_cannot_promote_inherited_children() {
        let answer = normalize(
            Ok(&with_forms(false)),
            "example",
            crate::BuildId("authored".into()),
            &ReferenceFacts::default(),
        );
        assert!(matches!(
            answer.value.child_families,
            GrammarProperty::Partial(_)
        ));
        assert!(matches!(
            answer.value.fixed_keys,
            GrammarProperty::Partial(_)
        ));
        assert_eq!(answer.completeness, crate::Completeness::Partial);
    }

    #[test]
    fn false_validation_is_partial_with_its_acceptance_gap() {
        use grammar::forms::{Alternative, ChainPath, PathClass, Stage, StageResult, ValueForm};
        let mut result = with_forms(false);
        std::sync::Arc::make_mut(result.forms.as_mut().unwrap())
            .alternatives
            .push(Alternative {
                value: ValueForm {
                    kind: ReaderKind::String,
                    destination: Some(64),
                    reader: None,
                    initialization: None,
                    deferred_null: None,
                },
                paths: vec![ChainPath {
                    class: PathClass::Unresolved,
                    stops: vec![],
                    stages: vec![StageResult {
                        stage: Stage::PostValidate,
                        returned: Some(false),
                        diagnostic: false,
                        cause: Some("false without diagnostic"),
                    }],
                }],
                missing: vec![],
                accepted: false,
            });
        let answer = normalize(
            Ok(&result),
            "example",
            crate::BuildId("authored".into()),
            &ReferenceFacts::default(),
        );
        assert_eq!(answer.value.forms, GrammarProperty::Partial(vec![]));
        assert!(answer.gaps.iter().any(|gap| gap.detail == "value-acceptance: PostValidate: false without diagnostic"));
    }

    #[test]
    fn a_key_whose_string_the_initializer_looks_up_joins_the_lookup() {
        use crate::{EmptyKey, FieldCondition, KeyMatch, LookupStage, MissingResult};

        let build = crate::BuildId("authored".into());
        let answer = normalize(
            Ok(&keyed(Ok(INITIALIZER.into()))),
            "add_district",
            build,
            &facts(scan_at(0xa8)),
        );

        assert_eq!(
            key_reference(&answer),
            crate::FieldReference::Lookups(vec![crate::ReferenceLookup {
                condition: FieldCondition::Always,
                target: crate::ReferenceTarget::Registry {
                    name: "common/districts".into()
                },
                stage: LookupStage::OwnerInitialization,
                key_match: KeyMatch::FirstEqual,
                empty_key: EmptyKey::LookedUp,
                on_missing: MissingResult::NullObject,
            }])
        );
        assert!(!joins_no_key(&answer));
    }

    #[test]
    fn control_17_changed_slots_no_longer_join_the_old_field() {
        let build = crate::BuildId("authored".into());
        let answer = normalize(
            Ok(&keyed(Ok(INITIALIZER.into()))),
            "add_district",
            build,
            &facts(scan_at(0x1a8)),
        );

        assert_eq!(
            key_reference(&answer),
            crate::FieldReference::NotEstablished
        );
        assert!(joins_no_key(&answer));
    }

    #[test]
    fn control_26_an_owner_without_an_initializer_is_an_input_gap() {
        let build = crate::BuildId("authored".into());
        for initializer in [
            Err(Unresolved::new("owner-initializer-slot")),
            Ok("CUnknown::PostInit()".into()),
        ] {
            let answer = normalize(
                Ok(&keyed(initializer)),
                "add_district",
                build.clone(),
                &facts(scan_at(0xa8)),
            );

            assert_eq!(
                key_reference(&answer),
                crate::FieldReference::NotEstablished
            );
            assert!(
                answer
                    .gaps
                    .iter()
                    .any(|gap| gap.kind == GapKind::UnreadableInput),
                "{:?}",
                answer.gaps
            );
        }
    }

    #[test]
    fn an_unestablished_initializer_lookup_names_its_obstacle() {
        let build = crate::BuildId("authored".into());
        let answer = normalize(
            Ok(&keyed(Ok(INITIALIZER.into()))),
            "add_district",
            build,
            &facts(Initialization::Unresolved(Unresolved::new(
                "initializer-several-lookups",
            ))),
        );

        assert!(answer.gaps.iter().any(|gap| {
            gap.kind == GapKind::ReaderSemantics
                && gap
                    .detail
                    .ends_with("the initializer holds several lookups.")
        }));
    }

    fn declared(factory: u64) -> Site {
        Site::Declared {
            name: "example".into(),
            factory,
            description: String::new(),
            usage: String::new(),
            scopes: ScopeOutcome::Any,
        }
    }

    fn inventory(sites: Vec<Site>) -> DeclarationResult {
        DeclarationResult {
            sites: sites
                .into_iter()
                .enumerate()
                .map(|(index, site)| (index as u64, site))
                .collect(),
            table_gaps: vec![],
        }
    }

    #[test]
    fn command_lookup_distinguishes_missing_from_ambiguous_or_unreadable_registration() {
        let known = inventory(vec![declared(1), declared(1)]);
        assert_eq!(registered_factory(&known, "example"), Ok(Some(1)));
        assert_eq!(registered_factory(&known, "limit"), Ok(None));
        let unnamed = inventory(vec![Site::Unreadable {
            name: None,
            what: "name",
        }]);
        assert_eq!(
            registered_factory(&unnamed, "limit"),
            Err(Unresolved::new("incomplete-command-inventory"))
        );
        let ambiguous = inventory(vec![declared(1), declared(2)]);
        assert_eq!(
            registered_factory(&ambiguous, "example"),
            Err(Unresolved::new("ambiguous-command-factory"))
        );
        let partial = inventory(vec![
            declared(1),
            Site::Unreadable {
                name: Some("example".into()),
                what: "entry-shape",
            },
        ]);
        assert_eq!(
            registered_factory(&partial, "example"),
            Err(Unresolved::new("command-registration"))
        );
    }

    #[test]
    fn unnamed_registrations_keep_known_factories_ambiguous() {
        for unknown in [
            Site::RuntimeToken { obstacle: "token" },
            Site::Unreadable {
                name: None,
                what: "name",
            },
        ] {
            let result = inventory(vec![declared(1), unknown]);
            assert_eq!(
                registered_factory(&result, "example"),
                Err(Unresolved::new("incomplete-command-inventory"))
            );
        }
    }

    #[test]
    fn recorded_command_names_cannot_overwrite_other_names() {
        let root = tempfile::tempdir().unwrap();
        let build = crate::BuildId("authored".into());
        let names = [
            "if",
            "if/",
            "if//",
            "/if",
            "if/../if",
            "",
            "IF",
            "encoded",
            "encoded/x69662f",
        ];
        for name in names {
            let answer: Result<Answer<CommandGrammar>, Error> = Err(Error::UnknownCommand {
                kind: DeclarationKind::Effect,
                name: name.into(),
            });
            crate::recorded::write(
                root.path(),
                &build,
                "command_grammar",
                Some(&recorded_subject(DeclarationKind::Effect, name)),
                &answer,
            )
            .unwrap();
        }
        let native = Native::from_recorded_answers(root.path()).unwrap();
        for name in names {
            assert_eq!(
                native.command_grammar(DeclarationKind::Effect, name),
                Err(Error::UnknownCommand {
                    kind: DeclarationKind::Effect,
                    name: name.into()
                })
            );
        }
    }

    #[test]
    fn nested_numeric_grammar_reports_each_gap_once() {
        let make = |numeric| grammar::GrammarResult {
            forms: None,
            reader: declarations::CommandReader {
                vtable: 1,
                read: 2,
                member: 3,
            },
            delegates: Default::default(),
            reader_name: "CEffect::Read(CReader&, EScopeType)".into(),
            reader_kind: ReaderKind::Block,
            reader_family: BlockFamily::Effect,
            member_name: "CEntry::ReadMember(CReader&, int, EScopeType)".into(),
            initializer: Ok(INITIALIZER.into()),
            numeric,
            ordering: vec![],
            families: vec![BlockFamily::Effect],
            stops: vec![Unresolved::new("reader-routing")],
            fields: grammar::ChildFields {
                fields: vec![],
                paths: vec![],
                gaps: vec![],
            },
        };
        let result = make(Some(Box::new(make(None))));
        let answer = normalize(
            Ok(&result),
            "example",
            crate::BuildId("authored".into()),
            &facts(Initialization::NoLookup),
        );
        assert_eq!(answer.gaps.len(), 3);
        for kind in [
            GapKind::OutsideMethod,
            GapKind::ReaderSemantics,
            GapKind::UnresolvedPath,
        ] {
            assert_eq!(answer.gaps.iter().filter(|gap| gap.kind == kind).count(), 1);
        }
        assert!(matches!(
            answer.value.numeric_keys,
            GrammarProperty::Partial(Some(_))
        ));
    }

    #[test]
    fn a_fixed_key_with_an_unresolved_lookup_has_its_own_gap() {
        use crate::engine::analysis::fields::{ReaderJoin, RootField, Value};

        let deferred = "void NParserUtil::ReadKeyReferenceDeferred<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CShipDatabase::ValueType const**)";
        let key = RootField {
            name: "ship".into(),
            token: 7,
            constructor: 0,
            paths: vec![],
            readers: vec![ReaderJoin::Joined {
                callee: deferred.into(),
                arguments: [
                    ("x0".into(), Value::Owner(0)),
                    ("x1".into(), Value::Reader(0)),
                    ("x2".into(), Value::Owner(0x40)),
                ]
                .into(),
                tail: true,
            }],
        };
        let result = grammar::GrammarResult {
            forms: None,
            reader: declarations::CommandReader {
                vtable: 1,
                read: 2,
                member: 3,
            },
            delegates: Default::default(),
            reader_name: "CEffect::Read(CReader&, EScopeType)".into(),
            reader_kind: ReaderKind::Block,
            reader_family: BlockFamily::Effect,
            member_name: "CEntry::ReadMember(CReader&, int, EScopeType)".into(),
            initializer: Ok(INITIALIZER.into()),
            numeric: None,
            ordering: vec![],
            families: vec![],
            stops: vec![],
            fields: grammar::ChildFields {
                fields: vec![key],
                paths: vec![],
                gaps: vec![],
            },
        };
        let answer = normalize(
            Ok(&result),
            "example",
            crate::BuildId("authored".into()),
            &ReferenceFacts::default(),
        );

        let key_gap = answer
            .gaps
            .iter()
            .find(|gap| gap.subject == Some(GapSubject::field("ship")))
            .expect("the key's lookup gap");
        assert_eq!(key_gap.kind, GapKind::ReaderSemantics);
        assert!(key_gap.detail.contains("the reader was not analyzed"));
    }

    #[test]
    fn concrete_identity_does_not_invent_a_kind_or_empty_grammar() {
        let result = grammar::GrammarResult {
            forms: None,
            reader: declarations::CommandReader {
                vtable: 1,
                read: 2,
                member: 3,
            },
            delegates: Default::default(),
            reader_name: "CCustom::Read(CReader&)".into(),
            member_name: "CCustom::ReadMember(CReader&, int)".into(),
            initializer: Ok(INITIALIZER.into()),
            reader_kind: ReaderKind::Unknown,
            reader_family: BlockFamily::Unknown,
            numeric: None,
            ordering: vec![],
            families: vec![],
            stops: vec![],
            fields: grammar::ChildFields {
                fields: vec![],
                paths: vec![],
                gaps: vec![],
            },
        };
        let answer = normalize(
            Ok(&result),
            "example",
            crate::BuildId("authored".into()),
            &ReferenceFacts::default(),
        );
        assert!(answer.value.reader.id.is_some());
        assert_eq!(answer.value.reader.kind, ReaderKind::Unknown);
        assert_eq!(answer.value.reader.family, BlockFamily::Unknown);
        assert_eq!(answer.value.fixed_keys, GrammarProperty::Unresolved);
        assert_eq!(answer.value.child_families, GrammarProperty::Unresolved);
        assert_eq!(answer.value.numeric_keys, GrammarProperty::Unresolved);
        assert_eq!(answer.value.ordering, GrammarProperty::Unresolved);
    }

    #[test]
    fn a_failed_receiver_join_keeps_every_grammar_property_unresolved() {
        let answer = normalize(
            Err(&Unresolved::new("factory-return")),
            "example",
            crate::BuildId("authored".into()),
            &ReferenceFacts::default(),
        );
        assert_eq!(answer.completeness, crate::Completeness::Partial);
        assert_eq!(answer.value.reader.family, BlockFamily::Unknown);
        assert!(answer.value.reader.id.is_none());
        assert_eq!(answer.value.child_families, GrammarProperty::Unresolved);
        assert_eq!(answer.value.fixed_keys, GrammarProperty::Unresolved);
        assert_eq!(answer.value.numeric_keys, GrammarProperty::Unresolved);
        assert_eq!(answer.value.ordering, GrammarProperty::Unresolved);
    }
}

#[cfg(test)]
mod population;
