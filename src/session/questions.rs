//! Static questions: answered from the executable, with no game process.
use super::Native;
use crate::answer::{
    Answer, Basis, BuildId, Completeness, Declaration, DeclarationKind, DeclaredScopes, Error,
    Field, Gap, GapKind, GapSubject, Operation, Reader, ReaderId, ReaderKind, Registry, ScopeId,
    ScopeReference, Source, Support,
};
use crate::binding::VerifiedAnalysis;
use crate::engine::analysis::{
    declarations::{self, DeclarationResult, ScopeOutcome, ScopeType, Site},
    directories::{self, Directory},
    fields::{self, FieldGapKind, PathOutcome, RegistryFieldResult},
    readers,
    stop::Unresolved,
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
            Operation::LoadedModifiers if !self.bound().has_modifier_table_method() => {
                Support::Unsupported("this build has no loaded modifier table recipe".into())
            }
            Operation::RegistryItems | Operation::ObserveFixture | Operation::LoadedModifiers => {
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
        let result = self.registry_field_result(registry)?;
        Ok(self.registry_field_answer(registry, &result))
    }

    /// The public answer that the registry field method's `result` gives for `registry`.
    pub(crate) fn registry_field_answer(
        &self,
        registry: &str,
        result: &RegistryFieldResult,
    ) -> Answer<Vec<Field>> {
        let gaps = normalized_gaps(result, registry.trim_end_matches('/'));
        Answer {
            value: normalized_fields(result),
            completeness: if gaps.iter().all(|gap| gap.kind == GapKind::OutsideMethod) {
                Completeness::Complete
            } else {
                Completeness::Partial
            },
            gaps,
            source: Source::new(self.build(), fields::METHOD, Basis::StaticAnalysis),
        }
    }

    /// The registry field method's own result, with every path and stop. Recorded answers do
    /// not hold it.
    pub(crate) fn registry_field_result(
        &self,
        registry: &str,
    ) -> Result<RegistryFieldResult, Error> {
        let operation = Operation::RegistryFields;
        if self.recorded().is_some() {
            return Err(Error::Unsupported {
                operation,
                reason: "recorded answers do not hold the method's internal result".into(),
            });
        }
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
        fields::analyze(&input).map_err(|e| error(operation, e.into()))
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
            } => {
                let scopes = match scopes {
                    ScopeOutcome::Any => DeclaredScopes::Any,
                    ScopeOutcome::Listed(types) => DeclaredScopes::Listed(scope_references(types)),
                    ScopeOutcome::Unresolved(Unresolved { reason: link, .. }) => {
                        if *link != "scope-table" {
                            gaps.push(Gap {
                                kind: GapKind::UnresolvedPath,
                                subject: Some(GapSubject::answer_item(name.clone())),
                                detail: format!("scope declaration not followed at {link}"),
                            });
                        }
                        DeclaredScopes::Unresolved
                    }
                };
                value.push(Declaration {
                    name: name.clone(),
                    description: description.clone(),
                    usage: usage.clone(),
                    scopes,
                });
            }
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
                subject: Some(GapSubject::answer_item(name.clone())),
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
        subject: Some(GapSubject::registry(registry)),
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
        seen.insert((kind as u8, Some(field.name.clone()), false));
        gaps.push(Gap {
            kind,
            subject: Some(GapSubject::field(field.name.clone())),
            detail: detail.into(),
        });
    }
    for gap in &result.gaps {
        let (kind, detail) = match gap.kind {
            FieldGapKind::InputBoundary | FieldGapKind::TokenTable => (
                GapKind::UnreadableInput,
                "A function or name table that the method needs could not be read.",
            ),
            FieldGapKind::ReaderJoin => (
                GapKind::UnresolvedReader,
                "The field's reader could not be established on at least one path.",
            ),
            FieldGapKind::UnresolvedTokenPath | FieldGapKind::TokenPartition => (
                GapKind::UnresolvedPath,
                "A path through the registry's reader could not be followed to its end.",
            ),
            FieldGapKind::JumpTable => (
                GapKind::UnresolvedPath,
                "The registry's reader selects some fields through a compiler jump table that could not be decoded.",
            ),
        };
        let subject = gap.path.and_then(field_of);
        let table = gap.kind == FieldGapKind::JumpTable;
        if seen.insert((kind as u8, subject.clone(), table)) {
            gaps.push(Gap {
                kind,
                subject: Some(match subject {
                    Some(field) => GapSubject::field(field),
                    None => GapSubject::registry(registry),
                }),
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
            subject: Some(GapSubject::registry(registry)),
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
                    && gap.subject == Some(GapSubject::answer_item("missing")))
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
                    scopes: ScopeOutcome::Unresolved(Unresolved::new("scope-table")),
                },
            )],
            table_gaps: vec!["scope-table"],
        };
        let answer = normalized_declarations(&result, BuildId("test".into()));
        assert_eq!(answer.value[0].scopes, DeclaredScopes::Unresolved);
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
                .any(|gap| gap.subject.as_ref().map(|subject| subject.name()) == Some("known"))
        );
    }
}

#[cfg(test)]
mod field_gap_tests {
    use super::*;
    use crate::engine::analysis::fields::{FieldGap, ReaderJoin, RootField, TokenPath};
    use crate::engine::analysis::stop::{Obstacle, Unknown};
    use std::collections::BTreeMap;

    const REGISTRY: &str = "common/examples";

    /// One field, `known`, whose only path joins a reader of a known kind, and `gaps`.
    fn result(gaps: Vec<FieldGap>) -> RegistryFieldResult {
        let reader = ReaderJoin::Joined {
            callee: "CReader::Read(bool&)".into(),
            arguments: BTreeMap::new(),
        };
        RegistryFieldResult {
            fields: vec![RootField {
                name: "known".into(),
                token: 7,
                constructor: 0x2000,
                paths: vec![0],
                readers: vec![reader.clone()],
            }],
            paths: vec![TokenPath {
                domain: [7, 7],
                conditions: vec![],
                instructions: vec![0x1000],
                terminal: 0x1000,
                outcome: PathOutcome::Reader(reader),
            }],
            gaps,
            partition_accounted: true,
        }
    }

    /// The public gaps besides the method's boundary, which every answer carries.
    fn public_gaps(gaps: Vec<FieldGap>) -> Vec<Gap> {
        let mut public = normalized_gaps(&result(gaps), REGISTRY);
        assert_eq!(public.remove(0).kind, GapKind::OutsideMethod);
        public
    }

    #[test]
    fn each_field_gap_kind_has_one_public_kind_and_subject() {
        for (kind, public) in [
            (FieldGapKind::InputBoundary, GapKind::UnreadableInput),
            (FieldGapKind::TokenTable, GapKind::UnreadableInput),
            (FieldGapKind::ReaderJoin, GapKind::UnresolvedReader),
            (FieldGapKind::UnresolvedTokenPath, GapKind::UnresolvedPath),
            (FieldGapKind::TokenPartition, GapKind::UnresolvedPath),
            (FieldGapKind::JumpTable, GapKind::UnresolvedPath),
        ] {
            let registry_wide = public_gaps(vec![FieldGap::new(kind, "reason")]);
            assert_eq!(registry_wide.len(), 1, "{kind:?}");
            assert_eq!(registry_wide[0].kind, public, "{kind:?}");
            assert_eq!(
                registry_wide[0].subject,
                Some(GapSubject::registry(REGISTRY))
            );

            let on_field = public_gaps(vec![FieldGap {
                path: Some(0),
                ..FieldGap::new(kind, "reason")
            }]);
            assert_eq!(on_field.len(), 1, "{kind:?}");
            assert_eq!(on_field[0].kind, public, "{kind:?}");
            assert_eq!(on_field[0].subject, Some(GapSubject::field("known")));
        }
    }

    #[test]
    fn gaps_of_one_public_kind_and_subject_become_one_gap() {
        let public = public_gaps(vec![
            FieldGap::new(FieldGapKind::InputBoundary, "first"),
            FieldGap::new(FieldGapKind::TokenTable, "second"),
            FieldGap {
                path: Some(0),
                ..FieldGap::new(FieldGapKind::ReaderJoin, "third")
            },
            FieldGap {
                path: Some(0),
                ..FieldGap::new(FieldGapKind::ReaderJoin, "fourth")
            },
        ]);

        let kinds: Vec<_> = public.iter().map(|gap| gap.kind).collect();
        assert_eq!(kinds, [GapKind::UnreadableInput, GapKind::UnresolvedReader]);
    }

    #[test]
    fn a_jump_table_gap_stays_apart_from_the_general_path_gap() {
        let public = public_gaps(vec![
            FieldGap::new(FieldGapKind::UnresolvedTokenPath, "path"),
            FieldGap::new(FieldGapKind::JumpTable, "jump table at 0x8040"),
            FieldGap::new(FieldGapKind::JumpTable, "jump table at 0x8080"),
        ]);

        let details: Vec<_> = public.iter().map(|gap| gap.detail.as_str()).collect();
        assert_eq!(details.len(), 2);
        assert!(details[1].contains("jump table"));
        assert!(!details[1].contains("0x"));
    }

    #[test]
    fn a_located_stop_does_not_reach_public_text() {
        let stop = Unresolved::at("flags", 0x1004, 0x1000, Obstacle::Unknown(Unknown::Flags));
        let public = public_gaps(vec![FieldGap {
            path: Some(0),
            ..FieldGap::unresolved(FieldGapKind::ReaderJoin, stop)
        }]);
        let located = public_gaps(vec![FieldGap {
            path: Some(0),
            ..FieldGap::new(FieldGapKind::ReaderJoin, "flags")
        }]);

        assert_eq!(public, located);
    }
}
