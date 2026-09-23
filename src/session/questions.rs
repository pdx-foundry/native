//! Static questions: answered from the executable, with no game process.
use super::Native;
use crate::answer::{
    Answer, Basis, BuildId, Completeness, Declaration, DeclarationKind, DeclaredScopes, Error,
    Field, Gap, GapKind, Operation, Reader, ReaderId, ReaderKind, Registry, ScopeId,
    ScopeReference, Source, Support,
};
use crate::binding::VerifiedAnalysis;
use crate::engine::analysis::{
    declarations::{self, DeclarationResult, ScopeOutcome, ScopeType, Site},
    directories::{self, Directory},
    fields::{self, PathOutcome, RegistryFieldResult},
    readers,
};
use crate::{AnalysisError, UnavailableReason};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub(super) fn error(operation: Operation, error: AnalysisError) -> Error {
    match error {
        AnalysisError::Unavailable { reasons }
            if reasons.contains(&UnavailableReason::TargetChanged) =>
        {
            Error::BuildChanged
        }
        AnalysisError::Unavailable { reasons } => Error::Unsupported {
            operation,
            reason: format!("{reasons:?}"),
        },
        other => Error::Method(other.to_string()),
    }
}

impl Native {
    /// Opaque identity of the exact game build, as stamped on every answer.
    pub fn build(&self) -> BuildId {
        match self.recorded() {
            Some(recorded) => recorded.build.clone(),
            None => BuildId(self.bound().build().into()),
        }
    }

    /// Answer from recorded files when they are the back end; otherwise run the method, and
    /// write the result when a recorder is set.
    pub(super) fn answer<T: serde::Serialize + serde::de::DeserializeOwned>(
        &self,
        question: &str,
        subject: Option<&str>,
        method: impl FnOnce() -> Result<Answer<T>, Error>,
    ) -> Result<Answer<T>, Error> {
        if let Some(directory) = self.recorded() {
            return directory.read(question, subject);
        }
        let answer = method();
        if let Some(directory) = self.recorder() {
            crate::recorded::write(directory, &self.build(), question, subject, &answer)?;
        }
        answer
    }

    /// Whether this build and host can answer an operation. This never starts a game; for a live
    /// operation it checks that the supervisor's tools can be found. Selected content is checked
    /// when `start_game` is called.
    pub fn supports(&self, operation: Operation) -> Support {
        if self.recorded().is_some() {
            return Support::Supported;
        }
        if operation == Operation::ObserveFixture && !self.bound().has_fixture_method() {
            return Support::Unsupported("this build has no fixture observation recipe".into());
        }
        if operation.is_declaration() && !self.bound().has_declarations_method() {
            return Support::Unsupported("this build has no declaration recipe".into());
        }
        match operation {
            Operation::Defines
            | Operation::Registries
            | Operation::RegistryFields
            | Operation::Declarations
            | Operation::Modifiers
            | Operation::ModifierCategories
            | Operation::ModifierFamilies
            | Operation::Scopes
            | Operation::ScopeLinks
            | Operation::LocalizationDeclarations
            | Operation::OnActions
            | Operation::GameRules => match &self.bound().analysis {
                Some(analysis) => match analysis.executable() {
                    Ok(_) => Support::Supported,
                    Err(reason) => Support::Unsupported(error(operation, reason).to_string()),
                },
                None => Support::Unsupported("this build has no static analysis recipe".into()),
            },
            Operation::RegistryItems | Operation::ObserveFixture => {
                match self.selected_blocking_reasons() {
                    reasons if reasons.is_empty() => Support::Supported,
                    reasons => Support::Unsupported(format!("{reasons:?}")),
                }
            }
        }
    }

    /// The bound analysis of a build whose declaration methods have been ported.
    pub(super) fn declaration_analysis(
        &self,
        operation: Operation,
    ) -> Result<&crate::binding::BoundAnalysis, Error> {
        let analysis = self
            .bound()
            .analysis
            .as_ref()
            .ok_or_else(|| Error::Unsupported {
                operation,
                reason: "this build has no static analysis recipe".into(),
            })?;
        if !analysis.has_declarations_method() {
            return Err(Error::Unsupported {
                operation,
                reason: "this build has no declaration recipe".into(),
            });
        }
        Ok(analysis)
    }

    fn verified_analysis(&self, operation: Operation) -> Result<VerifiedAnalysis<'_>, Error> {
        let analysis = self
            .bound()
            .analysis
            .as_ref()
            .ok_or_else(|| Error::Unsupported {
                operation,
                reason: "this build has no static analysis recipe".into(),
            })?;
        analysis.verified().map_err(|e| error(operation, e))
    }

    /// Read effect or trigger declarations from the registration calls in executable text.
    ///
    /// A call with a literal token is one declaration. A call whose name the code composes at run
    /// time gives one declaration for each chain of callers that establishes the name. A call that
    /// cannot be followed, or unreadable documentation, leaves a gap. Registration through a
    /// function pointer, argument grammar, behavior, and actual scope availability are outside
    /// this method.
    pub fn declarations(&self, kind: DeclarationKind) -> Result<Answer<Vec<Declaration>>, Error> {
        self.answer("declarations", Some(kind.subject()), || {
            let operation = Operation::Declarations;
            let input = self
                .declaration_analysis(operation)?
                .declaration_input(kind)
                .map_err(|failure| error(operation, failure))?;
            let result =
                declarations::analyze(&input).map_err(|error| Error::Method(error.to_string()))?;
            Ok(normalized_declarations(&result, self.build()))
        })
    }

    /// List the engine registries, each named by its content directory.
    ///
    /// The search covers shared database-template candidates. Custom, nested, and late loaders
    /// are outside it. Completeness says whether every candidate inside that boundary was named.
    pub fn registries(&self) -> Result<Answer<Vec<Registry>>, Error> {
        self.answer("registries", None, || self.registries_from_executable())
    }

    fn registries_from_executable(&self) -> Result<Answer<Vec<Registry>>, Error> {
        let verified = self.verified_analysis(Operation::Registries)?;
        let mut names = BTreeSet::new();
        let mut unnamed = 0;
        for candidate in verified.named_candidates() {
            match &candidate.directory {
                Directory::Named(name) => {
                    names.insert(name.clone());
                }
                Directory::Missing | Directory::Ambiguous(_) => unnamed += 1,
            }
        }
        let mut gaps = vec![Gap {
            kind: GapKind::OutsideMethod,
            subject: None,
            detail: "Registries with custom, nested, or late loaders are not listed.".into(),
        }];
        if unnamed > 0 {
            gaps.push(Gap {
                kind: GapKind::UnnamedRegistries,
                subject: None,
                detail: format!("{unnamed} registries were found without one content directory."),
            });
        }
        Ok(Answer {
            value: names.into_iter().map(|name| Registry { name }).collect(),
            completeness: if unnamed == 0 {
                Completeness::Complete
            } else {
                Completeness::Partial
            },
            gaps,
            source: Source::new(self.build(), directories::METHOD, Basis::StaticAnalysis),
        })
    }

    /// List the root fields of one registry's definitions, with the shared reader of each.
    ///
    /// The search covers root reader paths for one shared-template registry. Nested blocks,
    /// inherited readers, and dynamic names are outside it. Completeness says whether those root
    /// paths and their reader classifications were resolved.
    pub fn registry_fields(&self, registry: &str) -> Result<Answer<Vec<Field>>, Error> {
        self.answer("registry_fields", Some(registry), || {
            self.registry_fields_from_executable(registry)
        })
    }

    fn registry_fields_from_executable(&self, registry: &str) -> Result<Answer<Vec<Field>>, Error> {
        let operation = Operation::RegistryFields;
        let name = registry.trim_end_matches('/');
        let verified = self.verified_analysis(operation)?;
        let mut matching = verified
            .named_candidates()
            .iter()
            .filter(|c| c.directory == Directory::Named(name.to_owned()));
        let (Some(candidate), None) = (matching.next(), matching.next()) else {
            return Err(Error::UnknownRegistry {
                name: registry.into(),
            });
        };
        let input = verified
            .field_input(candidate.record.clone())
            .map_err(|e| error(operation, e))?;
        let result = fields::analyze(&input).map_err(|e| error(operation, e.into()))?;
        let gaps = normalized_gaps(&result, name);
        Ok(Answer {
            value: normalized_fields(&result),
            completeness: if gaps.iter().all(|gap| gap.kind == GapKind::OutsideMethod) {
                Completeness::Complete
            } else {
                Completeness::Partial
            },
            gaps,
            source: Source::new(self.build(), fields::METHOD, Basis::StaticAnalysis),
        })
    }
}

pub(crate) fn normalized_fields(result: &RegistryFieldResult) -> Vec<Field> {
    result
        .fields
        .iter()
        .map(|field| normalized_field(field, result))
        .collect()
}

fn normalized_declarations(result: &DeclarationResult, build: BuildId) -> Answer<Vec<Declaration>> {
    let mut value = Vec::new();
    let mut gaps = Vec::new();
    for (_, site) in &result.sites {
        match site {
            Site::Declared {
                name,
                description,
                usage,
                scopes,
                targets,
            } => value.push(Declaration {
                name: name.clone(),
                description: description.clone(),
                usage: usage.clone(),
                scopes: declared_scopes(scopes, name, "scope", &mut gaps),
                targets: declared_scopes(targets, name, "target", &mut gaps),
            }),
            Site::RuntimeToken { obstacle } => gaps.push(Gap {
                kind: GapKind::UnnamedDeclaration,
                subject: None,
                detail: format!(
                    "a registration composes its name at run time and was not followed at {obstacle}"
                ),
            }),
            Site::Unreadable {
                name: Some(name),
                what,
            } => gaps.push(Gap {
                kind: GapKind::UnresolvedPath,
                subject: Some(name.clone()),
                detail: format!("{what} could not be read"),
            }),
            Site::Unreadable { name: None, .. } => gaps.push(Gap {
                kind: GapKind::UnreadableInput,
                subject: None,
                detail: "registration token table could not be read".into(),
            }),
        }
    }
    if !result.table_gaps.is_empty() {
        gaps.push(Gap {
            kind: GapKind::UnreadableInput,
            subject: None,
            detail: "scope name table not found".into(),
        });
    }
    gaps.push(Gap {
        kind: GapKind::OutsideMethod,
        subject: None,
        detail: format!("The search covers every call and tail call in executable text to the register function or to a registry helper constructor, and follows run-time names through up to {} callers. Registration through a function pointer, argument grammar, behavior, and actual scope availability are outside it.", declarations::CALLER_DEPTH),
    });
    value.sort_by(|left, right| left.name.cmp(&right.name));
    Answer {
        value,
        completeness: if gaps.iter().all(|gap| gap.kind == GapKind::OutsideMethod) {
            Completeness::Complete
        } else {
            Completeness::Partial
        },
        gaps,
        source: Source::new(build, declarations::METHOD, Basis::Declared),
    }
}

/// A declared scope or target set. An unresolved set has a gap on its declaration, except when
/// the scope name table is missing, which is one gap for the answer.
fn declared_scopes(
    outcome: &ScopeOutcome,
    name: &str,
    set: &str,
    gaps: &mut Vec<Gap>,
) -> DeclaredScopes {
    match outcome {
        ScopeOutcome::Any => DeclaredScopes::Any,
        ScopeOutcome::Listed(types) => DeclaredScopes::Listed(scope_references(types)),
        ScopeOutcome::Unresolved(link) => {
            if *link != "scope-table" {
                gaps.push(Gap {
                    kind: GapKind::UnresolvedPath,
                    subject: Some(name.into()),
                    detail: format!("{set} declaration not followed at {link}"),
                });
            }
            DeclaredScopes::Unresolved
        }
    }
}

/// The public identity of a scope type. It hides the engine's bit number.
pub(super) fn scope_id(scope: &ScopeType) -> ScopeId {
    let digest = Sha256::digest(format!("scope-type/{}", scope.bit).as_bytes());
    ScopeId(format!("{digest:x}")[..16].to_owned())
}

pub(super) fn scope_references(types: &[ScopeType]) -> Vec<ScopeReference> {
    types
        .iter()
        .map(|scope| ScopeReference {
            id: scope_id(scope),
            name: scope.name.clone(),
        })
        .collect()
}

pub(crate) fn normalized_field(
    field: &crate::engine::analysis::fields::RootField,
    result: &RegistryFieldResult,
) -> Field {
    let classification = readers::classify(&field.readers);
    let id = classification.callee.map(|callee| {
        let digest = Sha256::digest(callee.as_bytes());
        ReaderId(format!("{digest:x}")[..16].to_owned())
    });
    Field {
        name: field.name.clone(),
        reader: Reader {
            id,
            kind: classification.kind,
        },
        conditional: field.readers.len() > 1
            || field
                .paths
                .iter()
                .any(|&path| !result.paths[path].conditions.is_empty()),
    }
}

fn normalized_gaps(result: &RegistryFieldResult, registry: &str) -> Vec<Gap> {
    let mut gaps = vec![Gap {
        kind: GapKind::OutsideMethod,
        subject: Some(registry.into()),
        detail: "Nested grammar, accepted occurrences, and runtime behavior are outside this bounded reader classification.".into(),
    }];
    let field_of = |path: usize| {
        result
            .fields
            .iter()
            .find(|field| field.paths.contains(&path))
            .map(|field| field.name.clone())
    };
    let mut seen = BTreeSet::new();
    for field in &result.fields {
        let classification = readers::classify(&field.readers);
        let (kind, detail) = if classification.callee.is_none() {
            (
                GapKind::UnresolvedReader,
                "The field's alternatives do not establish one shared reader.",
            )
        } else if classification.kind == ReaderKind::Unknown {
            (
                GapKind::ReaderSemantics,
                "The shared reader is identified, but its broad value form is not established.",
            )
        } else {
            continue;
        };
        seen.insert((kind as u8, Some(field.name.clone())));
        gaps.push(Gap {
            kind,
            subject: Some(field.name.clone()),
            detail: detail.into(),
        });
    }
    for gap in &result.gaps {
        let (kind, detail) = match gap.kind.as_str() {
            "input-boundary" | "token-table" => (
                GapKind::UnreadableInput,
                "A function or name table that the method needs could not be read.",
            ),
            "reader-join" => (
                GapKind::UnresolvedReader,
                "The field's reader could not be established on at least one path.",
            ),
            "reader-contract" => continue,
            _ => (
                GapKind::UnresolvedPath,
                "A path through the registry's reader could not be followed to its end.",
            ),
        };
        let subject = gap.path.and_then(field_of);
        if seen.insert((kind as u8, subject.clone())) {
            gaps.push(Gap {
                kind,
                subject: subject.or_else(|| Some(registry.into())),
                detail: detail.into(),
            });
        }
    }
    let unnamed = result
        .paths
        .iter()
        .enumerate()
        .filter(|(index, path)| {
            matches!(path.outcome, PathOutcome::Reader(_)) && field_of(*index).is_none()
        })
        .count();
    if unnamed > 0 {
        gaps.push(Gap {
            kind: GapKind::UnnamedField,
            subject: Some(registry.into()),
            detail: format!("{unnamed} reader paths have no recovered field name."),
        });
    }
    gaps
}

#[cfg(test)]
mod declaration_tests {
    use super::*;

    #[test]
    fn omitted_sites_make_a_partial_answer_without_fabricated_text() {
        let result = DeclarationResult {
            sites: vec![
                (
                    1,
                    Site::Declared {
                        name: "known".into(),
                        description: "description".into(),
                        usage: "".into(),
                        scopes: ScopeOutcome::Listed(vec![ScopeType {
                            bit: 2,
                            name: "country".into(),
                        }]),
                        targets: ScopeOutcome::Any,
                    },
                ),
                (2, Site::RuntimeToken { obstacle: "token" }),
                (
                    3,
                    Site::Unreadable {
                        name: Some("missing".into()),
                        what: "documentation",
                    },
                ),
            ],
            table_gaps: vec![],
        };
        let answer = normalized_declarations(&result, BuildId("test".into()));
        assert_eq!(answer.value.len(), 1);
        assert_eq!(answer.value[0].name, "known");
        assert_eq!(answer.completeness, Completeness::Partial);
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::UnnamedDeclaration)
        );
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::UnresolvedPath
                    && gap.subject.as_deref() == Some("missing"))
        );
    }

    #[test]
    fn global_scope_gap_accounts_for_every_unresolved_scope() {
        let result = DeclarationResult {
            sites: vec![(
                1,
                Site::Declared {
                    name: "known".into(),
                    description: "description".into(),
                    usage: "".into(),
                    scopes: ScopeOutcome::Unresolved("scope-table"),
                    targets: ScopeOutcome::Unresolved("scope-table"),
                },
            )],
            table_gaps: vec!["scope-table"],
        };
        let answer = normalized_declarations(&result, BuildId("test".into()));
        assert_eq!(answer.value[0].scopes, DeclaredScopes::Unresolved);
        assert_eq!(answer.value[0].targets, DeclaredScopes::Unresolved);
        assert_eq!(
            answer
                .gaps
                .iter()
                .filter(|gap| gap.kind == GapKind::UnreadableInput)
                .count(),
            1
        );
        assert!(
            !answer
                .gaps
                .iter()
                .any(|gap| gap.subject.as_deref() == Some("known"))
        );
    }

    #[test]
    fn each_target_set_is_on_its_declaration() {
        let declared = |name: &str, targets| Site::Declared {
            name: name.into(),
            description: "description".into(),
            usage: "".into(),
            scopes: ScopeOutcome::Any,
            targets,
        };
        let country = ScopeType {
            bit: 2,
            name: "country".into(),
        };
        let result = DeclarationResult {
            sites: vec![
                (1, declared("any", ScopeOutcome::Any)),
                (
                    2,
                    declared("listed", ScopeOutcome::Listed(vec![country.clone()])),
                ),
                (
                    3,
                    declared("unfollowed", ScopeOutcome::Unresolved("target-mask")),
                ),
            ],
            table_gaps: vec![],
        };
        let answer = normalized_declarations(&result, BuildId("test".into()));
        let targets: Vec<_> = answer.value.iter().map(|item| &item.targets).collect();
        assert_eq!(
            targets,
            [
                &DeclaredScopes::Any,
                &DeclaredScopes::Listed(scope_references(&[country])),
                &DeclaredScopes::Unresolved,
            ]
        );
        let unresolved: Vec<_> = answer
            .gaps
            .iter()
            .filter(|gap| gap.kind == GapKind::UnresolvedPath)
            .map(|gap| (gap.subject.as_deref(), gap.detail.as_str()))
            .collect();
        assert_eq!(
            unresolved,
            [(
                Some("unfollowed"),
                "target declaration not followed at target-mask"
            )]
        );
        assert_eq!(answer.completeness, Completeness::Partial);
    }
}
