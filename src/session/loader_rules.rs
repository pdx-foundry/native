//! The static question of how one registry's loader selects its files, treats a repeated key and
//! names its definitions.
use super::Native;
use super::questions::error;
use super::{gap_for_subject, registry_gap};
use crate::engine::analysis::loader_rules::{
    self, DuplicateObstacle, KeptTextObstacle, METHOD, Naming, Obstacle, Selection,
};
use crate::{
    Answer, Basis, BuildId, Completeness, DefinitionForm, DuplicateRule, Error, Gap, GapKind,
    GapSubject, GrammarProperty, LoaderRules, Operation, Source,
};

/// What the method does not search.
const OUTSIDE_METHOD: &str = "The search covers the arguments of the loader's file enumeration calls and the stated definition-name rules of its loaders. Files that the loader opens by a literal name, a check of the file name after the enumeration, and definitions nested in a definition's block other than the stated swap blocks are outside it.";

/// The build-wide rules that the answer does not carry, by the name of each gap's subject. The
/// knowledge page states them once for the build.
const BUILD_WIDE: [(&str, &str); 3] = [
    (
        "file_order",
        "The order in which the loader reads its files is a rule of the build's file system, stated once from its code and not for each registry. A live fixture control checks it for a template loader.",
    ),
    (
        "mod_over_vanilla",
        "Whether a mod's file replaces or joins a game file in the same folder is a rule of the build's file system, stated once from its code and not for each registry. No fixture verifies it.",
    ),
    (
        "later_text_reads",
        "Whether code other than the loader reads a definition's file again later is a rule of the build, stated once from the callers of the lexer constructors and not for each registry.",
    ),
];

impl Native {
    /// Read how one registry's loader enumerates its files (the file-name filters and the
    /// subdirectories that the enumeration lists), what it does with a later definition of a key
    /// that it holds, and how it names its definitions. The folder is the registry name.
    ///
    /// A file that the loader opens by a literal name and a check of a name after the enumeration
    /// are outside this method; an [`GapKind::OutsideMethod`] gap says so.
    ///
    /// A filter that the loader passes as a value other than a literal, several loader calls that
    /// disagree, or a loader with no file enumeration leaves the property
    /// [`GrammarProperty::Unresolved`] with a gap whose subject is the property's
    /// [`GapSubject::AnswerItem`] name, such as `extension`. The definition-name forms are
    /// stated rules of the loader's code: a loader with no stated rule, or a build that lacks the
    /// code of a stated rule, leaves `definition_names` unresolved or partial with a
    /// [`GapKind::UnresolvedPath`] gap whose subject is `definition_names`. A rule whose
    /// definitions no form can name, such as one with a field that renames a definition, adds a
    /// [`GapKind::OutsideMethod`] gap with that subject that names the obstacle. The duplicate rule is established for a template registry;
    /// a custom loader's is `Unresolved` with a gap whose subject is `duplicate_rule`. The file
    /// order and whether a mod's file replaces a game file are build-wide rules outside the
    /// answer, with gaps whose subjects are `file_order` and `mod_over_vanilla`.
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
                &loader_rules::duplicate_rule(&evidence.existing_entries),
                &loader_rules::naming(&evidence.stated),
                &loader_rules::keeps_definition_text(&evidence.item_reads),
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
    duplicate: &Result<DuplicateRule, DuplicateObstacle>,
    naming: &Naming,
    kept_text: &Result<bool, KeptTextObstacle>,
    build: BuildId,
) -> Answer<LoaderRules> {
    let mut gaps = Vec::new();
    let value = LoaderRules {
        extension: property("extension", &selection.extension, &mut gaps),
        name_contains: property("name_contains", &selection.name_contains, &mut gaps),
        recursion: property("recursion", &selection.recursion, &mut gaps),
        duplicate_rule: duplicate_rule(duplicate, &mut gaps),
        definition_names: definition_names(naming, &mut gaps),
        keeps_definition_text: keeps_definition_text(kept_text, &mut gaps),
    };
    gaps.extend(BUILD_WIDE.map(|(rule, detail)| {
        gap_for_subject(
            GapKind::OutsideMethod,
            Some(GapSubject::AnswerItem { name: rule.into() }),
            detail,
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

/// The duplicate rule, or `Unresolved` with a gap whose subject is `duplicate_rule` and whose
/// detail is the obstacle.
fn duplicate_rule(
    duplicate: &Result<DuplicateRule, DuplicateObstacle>,
    gaps: &mut Vec<Gap>,
) -> GrammarProperty<DuplicateRule> {
    let obstacle = match duplicate {
        Ok(rule) => return GrammarProperty::Known(*rule),
        Err(obstacle) => obstacle,
    };
    let reason = match obstacle {
        DuplicateObstacle::NotFollowed => {
            "the method does not follow what this loader does with a key that it already holds"
                .to_owned()
        }
        DuplicateObstacle::UnreadBody => {
            "the loader's code for a key that it already holds has a shape that the method does not read"
                .to_owned()
        }
        DuplicateObstacle::Several(count) => {
            format!("the registry's loaders give {count} different duplicate rules")
        }
    };
    gaps.push(gap_for_subject(
        GapKind::UnresolvedPath,
        Some(GapSubject::AnswerItem {
            name: "duplicate_rule".into(),
        }),
        reason,
    ));

    GrammarProperty::Unresolved
}

/// Whether the loader keeps a definition's text, or `Unresolved` with a gap whose subject is
/// `keeps_definition_text` and whose detail is the obstacle.
fn keeps_definition_text(
    kept_text: &Result<bool, KeptTextObstacle>,
    gaps: &mut Vec<Gap>,
) -> GrammarProperty<bool> {
    let reason = match kept_text {
        Ok(kept) => return GrammarProperty::Known(*kept),
        Err(KeptTextObstacle::NotFollowed) => {
            "the method does not follow what this loader keeps of a definition"
        }
        Err(KeptTextObstacle::OwnRead) => {
            "an item of the registry reads its definition with a read other than the member-reader loop"
        }
    };
    gaps.push(gap_for_subject(
        GapKind::UnresolvedPath,
        Some(GapSubject::AnswerItem {
            name: "keeps_definition_text".into(),
        }),
        reason,
    ));

    GrammarProperty::Unresolved
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
    fn established_rules_are_known_and_the_build_wide_rules_are_outside_the_method() {
        let selection = Selection {
            extension: Ok(Some(".txt".into())),
            name_contains: Ok(None),
            recursion: Ok(Recursion::None),
        };

        let answer = normalized_rules(
            "common/examples",
            &selection,
            &Ok(DuplicateRule::LastWins),
            &Naming {
                rules: 1,
                forms: vec![root_key()],
                missing: 0,
                obstacles: vec![],
            },
            &Ok(false),
            build(),
        );

        assert_eq!(
            answer.value,
            LoaderRules {
                extension: GrammarProperty::Known(Some(".txt".into())),
                name_contains: GrammarProperty::Known(None),
                recursion: GrammarProperty::Known(Recursion::None),
                duplicate_rule: GrammarProperty::Known(DuplicateRule::LastWins),
                definition_names: GrammarProperty::Known(vec![root_key()]),
                keeps_definition_text: GrammarProperty::Known(false),
            }
        );
        assert_eq!(answer.completeness, Completeness::Complete);
        let gaps: Vec<_> = answer
            .gaps
            .iter()
            .map(|gap| (gap.kind, gap.subject.as_ref().map(GapSubject::name)))
            .collect();
        assert_eq!(
            gaps,
            [
                (GapKind::OutsideMethod, Some("file_order")),
                (GapKind::OutsideMethod, Some("mod_over_vanilla")),
                (GapKind::OutsideMethod, Some("later_text_reads")),
                (GapKind::OutsideMethod, Some("common/examples")),
            ]
        );
        assert!(
            answer.gaps[0]
                .detail
                .ends_with("A live fixture control checks it for a template loader.")
        );
        assert!(answer.gaps[1].detail.ends_with("No fixture verifies it."));
    }

    #[test]
    fn a_loader_whose_duplicate_path_is_not_followed_has_an_unresolved_rule() {
        let answer = normalized_rules(
            "common/examples",
            &selection(),
            &Err(DuplicateObstacle::NotFollowed),
            &Naming {
                rules: 1,
                forms: vec![root_key()],
                missing: 0,
                obstacles: vec![],
            },
            &Ok(false),
            build(),
        );

        assert_eq!(answer.value.duplicate_rule, GrammarProperty::Unresolved);
        let duplicate_gaps: Vec<_> = answer
            .gaps
            .iter()
            .filter(|gap| gap.subject.as_ref().map(GapSubject::name) == Some("duplicate_rule"))
            .map(|gap| (gap.kind, gap.detail.as_str()))
            .collect();
        assert_eq!(
            duplicate_gaps,
            [(
                GapKind::UnresolvedPath,
                "the method does not follow what this loader does with a key that it already holds"
            )]
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
            &Ok(DuplicateRule::LastWins),
            &Naming {
                rules: 1,
                forms: vec![root_key()],
                missing: 0,
                obstacles: vec![],
            },
            &Ok(false),
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
            &Ok(DuplicateRule::LastWins),
            &Naming {
                rules: 1,
                forms: vec![],
                missing: 0,
                obstacles: vec![],
            },
            &Ok(false),
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
            &Ok(DuplicateRule::LastWins),
            &Naming {
                rules: 2,
                forms: vec![root_key()],
                missing: 0,
                obstacles: vec!["a `key` field renames a definition"],
            },
            &Ok(false),
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
            &Ok(DuplicateRule::LastWins),
            &Naming {
                rules: 1,
                forms: vec![],
                missing: 0,
                obstacles: vec!["definitions are named by their file name"],
            },
            &Ok(false),
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
            &Ok(DuplicateRule::LastWins),
            &Naming {
                rules: 0,
                forms: vec![],
                missing: 0,
                obstacles: vec![],
            },
            &Ok(false),
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
            &Ok(DuplicateRule::LastWins),
            &Naming {
                rules: 2,
                forms: vec![root_key()],
                missing: 1,
                obstacles: vec![],
            },
            &Ok(false),
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
            &Ok(DuplicateRule::LastWins),
            &Naming {
                rules: 2,
                forms: vec![],
                missing: 2,
                obstacles: vec![],
            },
            &Ok(false),
            build(),
        );

        assert_eq!(answer.value.definition_names, GrammarProperty::Unresolved);
        assert_eq!(definition_name_gaps(&answer).len(), 1);
    }
}
