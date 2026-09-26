//! Normalize command grammar without promoting a partial property to a complete grammar.
use super::{Native, questions::error};
use crate::engine::analysis::{
    declarations::{self, Site},
    grammar,
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
        let subject = format!("{}/{}", kind.subject(), name);
        self.answer("command_grammar", Some(&subject), || {
            let operation = Operation::CommandGrammar;
            let input = self
                .declaration_analysis(operation)?
                .grammar_input(kind)
                .map_err(|failure| error(operation, failure))?;
            let declarations = declarations::analyze(&input.declarations)
                .map_err(|failure| Error::Method(failure.to_string()))?;
            let factory = registered_factory(&declarations, name);
            let result = match factory {
                Ok(Some(factory)) => grammar::analyze(&input, factory),
                Ok(None) => {
                    return Err(Error::UnknownCommand {
                        kind,
                        name: name.into(),
                    });
                }
                Err(stop) => Err(stop),
            };
            Ok(normalize(result, name, self.build()))
        })
    }
}

fn registered_factory(
    declarations: &declarations::DeclarationResult,
    name: &str,
) -> Result<Option<u64>, crate::engine::analysis::stop::Unresolved> {
    use crate::engine::analysis::stop::Unresolved;
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
    if factories.is_empty()
        && declarations.sites.iter().any(|(_, site)| {
            matches!(
                site,
                Site::Unreadable { name: None, .. } | Site::RuntimeToken { .. }
            )
        })
    {
        return Err(Unresolved::new("incomplete-command-inventory"));
    }
    Ok(factories.first().copied())
}

fn normalize(
    result: Result<grammar::GrammarResult, crate::engine::analysis::stop::Unresolved>,
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
        gaps.push(Gap {
            kind,
            subject: Some(GapSubject::answer_item(name)),
            detail,
        })
    };
    match result {
        Err(stop) => gap(GapKind::UnresolvedReader, stop.reason.into()),
        Ok(result) => {
            let identity =
                super::fields::concrete_reader_id(&result.reader_name, &result.member_name);
            let joins = [crate::engine::analysis::fields::ReaderJoin::Joined {
                callee: result.reader_name,
                arguments: Default::default(),
                tail: false,
            }];
            let classification = crate::engine::analysis::readers::classify(&joins);
            value.reader = Reader {
                id: Some(identity),
                kind: classification.kind,
                family: classification.family,
            };
            let keys: Vec<_> = result
                .fields
                .fields
                .iter()
                .map(|field| super::fields::field(field, &result.fields))
                .collect();
            value.fixed_keys = GrammarProperty::Partial(keys);
            if !result.families.is_empty() {
                value.child_families = GrammarProperty::Partial(result.families);
            }
            if !result.ordering.is_empty() {
                value.ordering = GrammarProperty::Partial(
                    result
                        .ordering
                        .into_iter()
                        .map(|rule| {
                            let outcome = match rule.outcome {
                                grammar::OrderOutcome::Reader(join) => {
                                    crate::ChildOrderOutcome::Read(super::fields::reader(&[join]))
                                }
                                grammar::OrderOutcome::Family(family) => {
                                    crate::ChildOrderOutcome::Dispatch(family)
                                }
                            };
                            crate::ChildOrderRule {
                                child: rule.child,
                                conditions: rule.conditions,
                                outcome,
                            }
                        })
                        .collect(),
                );
            }
            if let Some(child) = result.numeric {
                let child = normalize(Ok(*child), name, build.clone());
                value.numeric_keys = GrammarProperty::Partial(Some(Box::new(child.value)));
                for child_gap in child.gaps {
                    gap(child_gap.kind, child_gap.detail);
                }
            }
            for stop in result.stops {
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
    fn a_failed_receiver_join_keeps_every_grammar_property_unresolved() {
        let answer = normalize(
            Err(Unresolved::new("factory-return")),
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
