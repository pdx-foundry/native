//! The static question of how one registry's loader selects its files.
use super::Native;
use super::questions::error;
use super::{gap_for_subject, registry_gap};
use crate::engine::analysis::loader_rules::{self, METHOD, Obstacle, Selection};
use crate::{
    Answer, Basis, BuildId, Completeness, Error, Gap, GapKind, GapSubject, GrammarProperty,
    LoaderRules, Operation, Source,
};

/// What the method does not search.
const OUTSIDE_METHOD: &str = "The search covers the arguments of the loader's file enumeration calls. Files that the loader opens by a literal name, a check of the file name after the enumeration, and the mounted directories that the enumeration searches are outside it.";

/// The loader rules that this method revision does not state, by the name of each gap's subject.
const NOT_STATED: [&str; 3] = ["duplicate_rule", "file_order", "definition_names"];

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
    /// [`GapSubject::AnswerItem`] name, such as `extension`. The duplicate-definition, file-order
    /// and definition-name rules are not established yet: each has a gap whose subject is
    /// `duplicate_rule`, `file_order` or `definition_names`.
    ///
    /// The name is a content directory from [`Native::registries`]; another name is
    /// [`Error::UnknownRegistry`].
    pub fn loader_rules(&self, registry: &str) -> Result<Answer<LoaderRules>, Error> {
        let registry = registry.trim_end_matches('/');
        self.answer(Operation::LoaderRules, Some(registry), || {
            let calls = self
                .bound()
                .analysis
                .verified()
                .and_then(|verified| verified.loader_enumerations(registry))
                .map_err(|failure| error(Operation::LoaderRules, failure))?
                .ok_or_else(|| Error::UnknownRegistry {
                    name: registry.into(),
                })?;

            Ok(normalized_rules(
                registry,
                &loader_rules::selection(&calls),
                self.build(),
            ))
        })
    }
}

/// The loader rules of one registry, with a gap for each property and rule that is not
/// established.
fn normalized_rules(registry: &str, selection: &Selection, build: BuildId) -> Answer<LoaderRules> {
    let mut gaps = Vec::new();
    let value = LoaderRules {
        extension: property("extension", &selection.extension, &mut gaps),
        name_contains: property("name_contains", &selection.name_contains, &mut gaps),
        recursion: property("recursion", &selection.recursion, &mut gaps),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Recursion;

    fn build() -> BuildId {
        BuildId("test".into())
    }

    #[test]
    fn established_properties_are_known_and_the_other_rules_keep_the_answer_partial() {
        let selection = Selection {
            extension: Ok(Some(".txt".into())),
            name_contains: Ok(None),
            recursion: Ok(Recursion::None),
        };

        let answer = normalized_rules("common/examples", &selection, build());

        assert_eq!(
            answer.value,
            LoaderRules {
                extension: GrammarProperty::Known(Some(".txt".into())),
                name_contains: GrammarProperty::Known(None),
                recursion: GrammarProperty::Known(Recursion::None),
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
                (GapKind::ReaderSemantics, Some("definition_names")),
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

        let answer = normalized_rules("common/examples", &selection, build());

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
}
