//! The static question of the names that a registry's own code derives from an item key or a
//! field.
use super::Native;
use super::families::{not_established, public_parts, template};
use super::language::{gap_for_subject, registry_gap};
use super::questions::error;
use crate::engine::analysis::families::Part;
use crate::engine::analysis::names::{
    self, Condition, METHOD, Miss, Name, NameResult, Stage, Target, Term,
};
use crate::{
    Answer, Basis, BuildId, Completeness, DerivedName, Error, FieldCondition, Gap, GapKind,
    GapSubject, LookupStage, MissingName, NameLookup, Operation, Source,
};

/// What the method does not search.
const OUTSIDE_METHOD: &str = "The search covers the registry's own const methods and its post-read initialization. Names that other code composes or looks up, such as interface code, and the run-time choice behind a selection condition, such as which swap the engine selects and whether it is valid, are outside it. The answer is no catalogue of the names that content defines.";

impl Native {
    /// Read the names that one registry's own code composes from an item key or a string field,
    /// and checks or looks up, such as `{key}_desc` in the localisation keys.
    ///
    /// Each name has its parts, what the engine looks it up in, when, what a missing name gives,
    /// and the stored field values under which the engine uses it. A fixed key that holds no item
    /// key or field is not a derived name. Names that interface code composes are outside this
    /// method; an [`GapKind::OutsideMethod`] gap says so.
    ///
    /// The name is a content directory from [`Native::registries`]; another name is
    /// [`Error::UnknownRegistry`].
    pub fn derived_names(&self, registry: &str) -> Result<Answer<Vec<DerivedName>>, Error> {
        let registry = registry.trim_end_matches('/');
        self.answer("derived_names", Some(registry), || {
            let operation = Operation::DerivedNames;
            let analysis = self.declaration_analysis(operation)?;
            let code = analysis
                .registry_names(registry)
                .map_err(|failure| error(operation, failure))?
                .ok_or_else(|| Error::UnknownRegistry {
                    name: registry.into(),
                })?;
            let (_, fields) = self.registry_field_input_and_result(registry)?;
            let shared = &analysis
                .family_index()
                .map_err(|failure| error(operation, failure))?
                .input;
            let input = analysis
                .name_input()
                .map_err(|failure| error(operation, failure))?;

            let storage = names::storage(&fields);
            let result = names::analyze(shared, input, &code, &storage);
            Ok(normalized_names(registry, &result, self.build()))
        })
    }
}

/// The derived names of one registry, with a gap for each part that the method did not establish.
fn normalized_names(
    registry: &str,
    result: &NameResult,
    build: BuildId,
) -> Answer<Vec<DerivedName>> {
    let subject = Some(registry);
    let mut gaps = Vec::new();

    if let Err(reason) = &result.key_offset {
        gaps.push(registry_gap(
            GapKind::UnresolvedPath,
            subject,
            format!(
                "the place of the item key could not be established at {}; no name of the registry's code was followed",
                reason.reason
            ),
        ));
    }
    for (reason, count) in &result.failures {
        gaps.push(registry_gap(
            GapKind::UnresolvedPath,
            subject,
            failure(reason, *count),
        ));
    }
    if let Some(reason) = &result.not_established {
        gaps.push(registry_gap(
            GapKind::UnresolvedCondition,
            subject,
            format!(
                "that the engine initializes every item after reading it is not established, so no name of the initialization is always used: {}",
                not_established(reason)
            ),
        ));
    }

    let value: Vec<DerivedName> = result.names.iter().map(public_name).collect();
    for (name, public) in result.names.iter().zip(&value) {
        gaps.extend(name_gaps(name, &template(&public.name)));
    }
    gaps.push(registry_gap(
        GapKind::OutsideMethod,
        subject,
        OUTSIDE_METHOD,
    ));

    Answer {
        value,
        completeness: Completeness::from_gaps(&gaps),
        gaps,
        source: Source::new(build, METHOD, Basis::StaticAnalysis),
    }
}

/// The detail of `count` obstructions with `reason`.
fn failure(reason: &str, count: usize) -> String {
    match reason {
        "unreached" => {
            format!(
                "{count} lookup or check calls of the registry's methods were reached by no run"
            )
        }
        "unresolved-name" => format!(
            "{count} lookup or check calls receive a name with a part that the method could not follow"
        ),
        "name-limit" => {
            format!("{count} lookup or check calls receive a name that a fixed-size buffer bounds")
        }
        "assumed-text" => format!(
            "{count} of the registry's methods have paths that go on after text that the method could not follow; their later lookups establish no condition or miss behavior"
        ),
        "check-bound" => format!(
            "{count} of the registry's methods check more names than the method enumerates; their names are not returned"
        ),
        "flag-bound" => format!(
            "{count} of the registry's methods test more flags than the method enumerates; their names are not returned"
        ),
        other => format!(
            "{count} of the registry's methods have paths that could not be followed at {other}"
        ),
    }
}

/// A gap for each property of the name that is not established: an unresolved miss behavior, an
/// unresolved condition, and the field states behind a name with a field part.
fn name_gaps(name: &Name, template: &str) -> Vec<Gap> {
    let mut gaps = Vec::new();
    let gap = |detail: String| {
        gap_for_subject(
            GapKind::UnresolvedCondition,
            Some(GapSubject::answer_item(template)),
            detail,
        )
    };

    if name.on_missing == Miss::Unresolved {
        gaps.push(gap(format!(
            "what a missing {template} gives is not established"
        )));
    }
    if name.condition == Condition::Unresolved {
        gaps.push(gap(format!(
            "the field values under which the engine uses {template} are not established"
        )));
    }
    if name.parts.iter().any(|part| matches!(part, Part::Field(_))) {
        gaps.push(gap(format!(
            "{template} also depends on field states that the method does not explore"
        )));
    }
    gaps
}

fn public_name(name: &Name) -> DerivedName {
    DerivedName {
        name: public_parts(&name.parts),
        lookup: match name.target {
            Target::Localization => NameLookup::Localization,
            Target::Sprite => NameLookup::Sprite,
            Target::File => NameLookup::File,
        },
        stage: match name.stage {
            Stage::WhenUsed => LookupStage::WhenUsed,
            Stage::OwnerInitialization => LookupStage::OwnerInitialization,
        },
        on_missing: match &name.on_missing {
            Miss::ShowsKey => MissingName::ShowsKey,
            Miss::Silent => MissingName::Silent,
            Miss::Diagnostic => MissingName::Diagnostic,
            Miss::Fallback(parts) => MissingName::Fallback(public_parts(parts)),
            Miss::Unresolved => MissingName::Unresolved,
        },
        condition: public_condition(&name.condition),
    }
}

fn public_condition(condition: &Condition) -> FieldCondition {
    match condition {
        Condition::Always => FieldCondition::Always,
        Condition::Unresolved => FieldCondition::Unresolved,
        Condition::All(terms) => FieldCondition::All(
            terms
                .iter()
                .map(|term| match term {
                    Term::Unresolved => FieldCondition::Unresolved,
                    Term::FieldZero { path, zero } => FieldCondition::FieldZero {
                        path: path.clone(),
                        zero: *zero,
                    },
                })
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::NamePart;
    use crate::engine::analysis::stop::Unresolved;

    fn name(parts: Vec<Part>, on_missing: Miss, condition: Condition) -> Name {
        Name {
            parts,
            target: Target::Localization,
            stage: Stage::WhenUsed,
            on_missing,
            condition,
        }
    }

    fn answer(result: &NameResult) -> Answer<Vec<DerivedName>> {
        normalized_names("common/x", result, BuildId("test".into()))
    }

    #[test]
    fn every_unestablished_property_of_a_name_has_a_gap_and_the_boundary_is_stated() {
        let key = vec![Part::ItemKey];
        let field = vec![
            Part::Field(vec!["swap".into(), "name".into()]),
            Part::Literal("_desc".into()),
        ];
        let result = NameResult {
            key_offset: Ok(0x10),
            names: vec![
                name(key.clone(), Miss::ShowsKey, Condition::Always),
                name(
                    field,
                    Miss::Fallback(vec![Part::ItemKey, Part::Literal("_desc".into())]),
                    Condition::All(vec![
                        Term::Unresolved,
                        Term::FieldZero {
                            path: vec!["swap".into(), "name".into()],
                            zero: false,
                        },
                    ]),
                ),
                name(
                    vec![Part::ItemKey, Part::Literal("_delayed".into())],
                    Miss::Unresolved,
                    Condition::Unresolved,
                ),
            ],
            failures: BTreeMap::from([("path-limit", 2)]),
            not_established: None,
        };

        let answer = answer(&result);

        assert_eq!(answer.completeness, Completeness::Partial);
        assert_eq!(answer.value[0].condition, FieldCondition::Always);
        assert_eq!(
            answer.value[1].on_missing,
            MissingName::Fallback(vec![NamePart::ItemKey, NamePart::Literal("_desc".into())])
        );
        let subjects = |template: &str| {
            answer
                .gaps
                .iter()
                .filter(|gap| gap.subject == Some(GapSubject::answer_item(template)))
                .count()
        };
        assert_eq!(subjects("{key}"), 0);
        assert_eq!(subjects("{swap/name}_desc"), 1);
        assert_eq!(subjects("{key}_delayed"), 2);
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::UnresolvedPath
                    && gap.detail.contains("could not be followed at path-limit"))
        );
        assert_eq!(
            answer.gaps.last().map(|gap| gap.kind),
            Some(GapKind::OutsideMethod)
        );
    }

    #[test]
    fn an_unestablished_key_offset_leaves_only_a_registry_gap() {
        let result = NameResult {
            key_offset: Err(Unresolved::new("key-storage")),
            names: Vec::new(),
            failures: BTreeMap::new(),
            not_established: None,
        };

        let answer = answer(&result);

        assert!(answer.value.is_empty());
        assert_eq!(answer.completeness, Completeness::Partial);
        assert_eq!(
            answer.gaps[0].subject,
            Some(GapSubject::registry("common/x"))
        );
        assert_eq!(answer.gaps[0].kind, GapKind::UnresolvedPath);
    }
}
