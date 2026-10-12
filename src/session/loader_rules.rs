//! The static question of how one registry's loader selects its files.
use super::Native;
use super::questions::error;
use super::{gap_for_subject, registry_gap};
use crate::engine::analysis::loader_rules::{self, METHOD, Naming, Obstacle, Selection};
use crate::{
    Answer, Basis, BuildId, Completeness, DefinitionForm, Error, Gap, GapKind, GapSubject,
    GrammarProperty, LoaderRules, Operation, Source,
};

/// What the method does not search.
const OUTSIDE_METHOD: &str = "The search covers the arguments of the loader's file enumeration calls and the stated definition-name rules of its loaders. Files that the loader opens by a literal name, a check of the file name after the enumeration, the mounted directories that the enumeration searches, and definitions nested in a definition's block other than the stated swap blocks are outside it.";

/// The loader rules that this method revision does not state, by the name of each gap's subject.
const NOT_STATED: [&str; 2] = ["duplicate_rule", "file_order"];

impl Native {
    /// Read how one registry's loader enumerates its files: the file-name filters and the
    /// subdirectories that the enumeration lists. The folder is the registry name.
    ///
    /// A file that the loader opens by a literal name, a check of a name after the enumeration,
    /// and the mounted directories that the enumeration searches are outside this method; an
    /// [`GapKind::OutsideMethod`] gap says so.
    ///
    /// A filter that the loader passes as a value other than a literal, several loader calls that
    /// disagree, or a loader with no file enumeration leaves the property
    /// [`GrammarProperty::Unresolved`] with a gap whose subject is the property's
    /// [`GapSubject::AnswerItem`] name, such as `extension`. The definition-name forms are
    /// stated rules of the loader's code: a loader with no stated rule, or a build that lacks the
    /// code of a stated rule, leaves `definition_names` unresolved or partial with a
    /// [`GapKind::UnresolvedPath`] gap whose subject is `definition_names`. A rule whose
    /// definitions no form can name, such as one with a field that renames a definition, adds a
    /// [`GapKind::OutsideMethod`] gap with that subject that names the obstacle. The duplicate-definition and file-order rules are not
    /// established yet: each has a gap whose subject is `duplicate_rule` or `file_order`.
    ///
    /// The name is a content directory from [`Native::registries`]; another name is
    /// [`Error::UnknownRegistry`].
    pub fn loader_rules(&self, registry: &str) -> Result<Answer<LoaderRules>, Error> {
        let registry = registry.trim_end_matches('/');
        self.answer(Operation::LoaderRules, Some(registry), || {
            let evidence = self
                .bound()
                .analysis
                .verified()
                .and_then(|verified| verified.loader_evidence(registry))
                .map_err(|failure| error(Operation::LoaderRules, failure))?
                .ok_or_else(|| Error::UnknownRegistry {
                    name: registry.into(),
                })?;

            Ok(normalized_rules(
                registry,
                &loader_rules::selection(&evidence.enumerations),
                &loader_rules::naming(&evidence.stated),
                self.build(),
            ))
        })
    }
}

/// The loader rules of one registry, with a gap for each property and rule that is not
/// established.
fn normalized_rules(
    registry: &str,
    selection: &Selection,
    naming: &Naming,
    build: BuildId,
) -> Answer<LoaderRules> {
    let mut gaps = Vec::new();
    let value = LoaderRules {
        extension: property("extension", &selection.extension, &mut gaps),
        name_contains: property("name_contains", &selection.name_contains, &mut gaps),
        recursion: property("recursion", &selection.recursion, &mut gaps),
        definition_names: definition_names(naming, &mut gaps),
    };
    gaps.extend(NOT_STATED.map(|rule| {
        gap_for_subject(
            GapKind::ReaderSemantics,
            Some(GapSubject::AnswerItem { name: rule.into() }),
            "the loader's rule is not established",
        )
    }));
    gaps.push(registry_gap(
        GapKind::OutsideMethod,
        Some(registry),
        OUTSIDE_METHOD,
    ));

    Answer {
        value,
        completeness: Completeness::from_gaps(&gaps),
        gaps,
        source: Source::new(build, METHOD, Basis::StaticAnalysis),
    }
}

/// One property, or `Unresolved` with a gap whose subject is `property` and whose detail is the
/// obstacle.
fn property<T: Clone>(
    property: &str,
    selected: &Result<T, Obstacle>,
    gaps: &mut Vec<Gap>,
) -> GrammarProperty<T> {
    let obstacle = match selected {
        Ok(value) => return GrammarProperty::Known(value.clone()),
        Err(obstacle) => obstacle,
    };
    let reason = match obstacle {
        Obstacle::NoEnumeration => {
            "the loader makes no file enumeration call that the method reads".to_owned()
        }
        Obstacle::NotLiteral => {
            "a file enumeration call passes a value that is not a literal".to_owned()
        }
        Obstacle::Several(count) => {
            format!("the loader's file enumeration calls pass {count} different values")
        }
    };
    gaps.push(gap_for_subject(
        GapKind::UnresolvedPath,
        Some(GapSubject::AnswerItem {
            name: property.into(),
        }),
        reason,
    ));

    GrammarProperty::Unresolved
}

/// The definition forms: `Known` when every stated rule holds and has no obstacle, `Partial` when
/// some forms are stated but a rule is missing or has an obstacle, and `Unresolved` otherwise. A
/// missing rule or a loader with no rule adds an `UnresolvedPath` gap, and each obstacle adds an
/// `OutsideMethod` gap.
fn definition_names(naming: &Naming, gaps: &mut Vec<Gap>) -> GrammarProperty<Vec<DefinitionForm>> {
    let subject = || {
        Some(GapSubject::AnswerItem {
            name: "definition_names".into(),
        })
    };
    let unresolved = match (naming.rules, naming.missing) {
        (0, _) => Some("the registry's loader has no stated definition-name rule".to_owned()),
        (_, 0) => None,
        (_, missing) => Some(format!(
            "the build lacks the code of {missing} stated definition-name rules"
        )),
    };

    gaps.extend(
        unresolved
            .into_iter()
            .map(|reason| gap_for_subject(GapKind::UnresolvedPath, subject(), reason)),
    );
    gaps.extend(
        naming
            .obstacles
            .iter()
            .map(|&obstacle| gap_for_subject(GapKind::OutsideMethod, subject(), obstacle)),
    );

    let established = naming.rules > 0 && naming.missing == 0 && naming.obstacles.is_empty();
    if established {
        GrammarProperty::Known(naming.forms.clone())
    } else if naming.forms.is_empty() {
        GrammarProperty::Unresolved
    } else {
        GrammarProperty::Partial(naming.forms.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DefinitionName, DefinitionStatement, Recursion};

    fn build() -> BuildId {
        BuildId("test".into())
    }

    fn root_key() -> DefinitionForm {
        DefinitionForm {
            statement: DefinitionStatement::Root,
            name: DefinitionName::Key,
        }
    }

    fn definition_name_gaps(answer: &Answer<LoaderRules>) -> Vec<(GapKind, &str)> {
        answer
            .gaps
            .iter()
            .filter(|gap| gap.subject.as_ref().map(GapSubject::name) == Some("definition_names"))
            .map(|gap| (gap.kind, gap.detail.as_str()))
            .collect()
    }

    fn selection() -> Selection {
        Selection {
            extension: Ok(None),
            name_contains: Ok(None),
            recursion: Ok(Recursion::None),
        }
    }

    #[test]
    fn established_properties_are_known_and_the_other_rules_keep_the_answer_partial() {
        let selection = Selection {
            extension: Ok(Some(".txt".into())),
            name_contains: Ok(None),
            recursion: Ok(Recursion::None),
        };

        let answer = normalized_rules(
            "common/examples",
            &selection,
            &Naming {
                rules: 1,
                forms: vec![root_key()],
                missing: 0,
                obstacles: vec![],
            },
            build(),
        );

        assert_eq!(
            answer.value,
            LoaderRules {
                extension: GrammarProperty::Known(Some(".txt".into())),
                name_contains: GrammarProperty::Known(None),
                recursion: GrammarProperty::Known(Recursion::None),
                definition_names: GrammarProperty::Known(vec![root_key()]),
            }
        );
        assert_eq!(answer.completeness, Completeness::Partial);
        let gaps: Vec<_> = answer
            .gaps
            .iter()
            .map(|gap| (gap.kind, gap.subject.as_ref().map(GapSubject::name)))
            .collect();
        assert_eq!(
            gaps,
            [
                (GapKind::ReaderSemantics, Some("duplicate_rule")),
                (GapKind::ReaderSemantics, Some("file_order")),
                (GapKind::OutsideMethod, Some("common/examples")),
            ]
        );
    }

    #[test]
    fn a_property_with_an_obstacle_is_unresolved_with_a_gap_that_names_it() {
        let selection = Selection {
            extension: Err(Obstacle::NotLiteral),
            name_contains: Ok(None),
            recursion: Err(Obstacle::NoEnumeration),
        };

        let answer = normalized_rules(
            "common/examples",
            &selection,
            &Naming {
                rules: 1,
                forms: vec![root_key()],
                missing: 0,
                obstacles: vec![],
            },
            build(),
        );

        assert_eq!(answer.value.extension, GrammarProperty::Unresolved);
        assert_eq!(answer.value.recursion, GrammarProperty::Unresolved);
        let unresolved: Vec<_> = answer
            .gaps
            .iter()
            .filter(|gap| gap.kind == GapKind::UnresolvedPath)
            .map(|gap| {
                let subject = gap.subject.as_ref().map(GapSubject::name);
                (subject, gap.detail.as_str())
            })
            .collect();
        assert_eq!(
            unresolved,
            [
                (
                    Some("extension"),
                    "a file enumeration call passes a value that is not a literal"
                ),
                (
                    Some("recursion"),
                    "the loader makes no file enumeration call that the method reads"
                ),
            ]
        );
    }

    #[test]
    fn a_held_rule_with_no_form_states_that_the_loader_names_no_definition() {
        let answer = normalized_rules(
            "common/examples",
            &selection(),
            &Naming {
                rules: 1,
                forms: vec![],
                missing: 0,
                obstacles: vec![],
            },
            build(),
        );

        assert_eq!(
            answer.value.definition_names,
            GrammarProperty::Known(vec![])
        );
        assert!(definition_name_gaps(&answer).is_empty());
    }

    #[test]
    fn an_obstacle_leaves_the_stated_forms_partial_with_a_gap_that_names_it() {
        let answer = normalized_rules(
            "common/examples",
            &selection(),
            &Naming {
                rules: 2,
                forms: vec![root_key()],
                missing: 0,
                obstacles: vec!["a `key` field renames a definition"],
            },
            build(),
        );

        assert_eq!(
            answer.value.definition_names,
            GrammarProperty::Partial(vec![root_key()])
        );
        assert_eq!(
            definition_name_gaps(&answer),
            [(GapKind::OutsideMethod, "a `key` field renames a definition")]
        );
    }

    #[test]
    fn an_obstacle_with_no_stated_form_leaves_definition_names_unresolved() {
        let answer = normalized_rules(
            "common/examples",
            &selection(),
            &Naming {
                rules: 1,
                forms: vec![],
                missing: 0,
                obstacles: vec!["definitions are named by their file name"],
            },
            build(),
        );

        assert_eq!(answer.value.definition_names, GrammarProperty::Unresolved);
        assert_eq!(definition_name_gaps(&answer).len(), 1);
    }

    #[test]
    fn a_loader_with_no_stated_rule_leaves_definition_names_unresolved() {
        let answer = normalized_rules(
            "common/examples",
            &selection(),
            &Naming {
                rules: 0,
                forms: vec![],
                missing: 0,
                obstacles: vec![],
            },
            build(),
        );

        assert_eq!(answer.value.definition_names, GrammarProperty::Unresolved);
        assert_eq!(
            definition_name_gaps(&answer),
            [(
                GapKind::UnresolvedPath,
                "the registry's loader has no stated definition-name rule"
            )]
        );
    }

    #[test]
    fn a_stated_rule_whose_code_is_missing_keeps_the_other_forms_partial() {
        let answer = normalized_rules(
            "common/examples",
            &selection(),
            &Naming {
                rules: 2,
                forms: vec![root_key()],
                missing: 1,
                obstacles: vec![],
            },
            build(),
        );

        assert_eq!(
            answer.value.definition_names,
            GrammarProperty::Partial(vec![root_key()])
        );
        assert_eq!(
            definition_name_gaps(&answer),
            [(
                GapKind::UnresolvedPath,
                "the build lacks the code of 1 stated definition-name rules"
            )]
        );
    }

    #[test]
    fn no_held_rule_leaves_definition_names_unresolved() {
        let answer = normalized_rules(
            "common/examples",
            &selection(),
            &Naming {
                rules: 2,
                forms: vec![],
                missing: 2,
                obstacles: vec![],
            },
            build(),
        );

        assert_eq!(answer.value.definition_names, GrammarProperty::Unresolved);
        assert_eq!(definition_name_gaps(&answer).len(), 1);
    }
}
