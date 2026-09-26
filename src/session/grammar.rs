//! Normalize command grammar without promoting a partial property to a complete grammar.
use super::{Native, questions::error};
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
            Ok(normalize(result.as_ref(), name, self.build()))
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
        match registered_factory(&declarations, name) {
            Ok(Some(factory)) => Ok(grammar::analyze(&input, factory)),
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

fn registered_factory(
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
) -> Answer<CommandGrammar> {
    let mut value = CommandGrammar {
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
            let keys = super::fields::grammar_fields(&result.fields.fields, &result.fields.paths);
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
                let child = normalize(Ok(child), name, build.clone());
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
    gap(
        GapKind::ReaderSemantics,
        "Child grammar extraction is incomplete; unresolved properties and conditional paths remain."
            .into(),
    );
    gap(GapKind::OutsideMethod, "Argument values, scope propagation, storage behavior and runtime meaning are outside this method.".into());
    Answer {
        value,
        completeness: crate::Completeness::from_gaps(&gaps),
        gaps,
        source: Source::new(build, grammar::METHOD, Basis::StaticAnalysis),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::{
        declarations::{DeclarationResult, ScopeOutcome},
        stop::Unresolved,
    };

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
            reader: declarations::CommandReader {
                vtable: 1,
                read: 2,
                member: 3,
            },
            reader_name: "CEffect::Read(CReader&, EScopeType)".into(),
            reader_kind: ReaderKind::Block,
            reader_family: BlockFamily::Effect,
            member_name: "CEntry::ReadMember(CReader&, int, EScopeType)".into(),
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
        let answer = normalize(Ok(&result), "example", crate::BuildId("authored".into()));
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
    fn concrete_identity_does_not_invent_a_kind_or_empty_grammar() {
        let result = grammar::GrammarResult {
            reader: declarations::CommandReader {
                vtable: 1,
                read: 2,
                member: 3,
            },
            reader_name: "CCustom::Read(CReader&)".into(),
            member_name: "CCustom::ReadMember(CReader&, int)".into(),
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
        let answer = normalize(Ok(&result), "example", crate::BuildId("authored".into()));
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
