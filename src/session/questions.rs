//! Static questions: answered from the executable, with no game process.
use super::{Backend, Native};
use crate::answer::{
    Answer, Basis, BuildId, Completeness, Declaration, DeclarationKind, DeclaredScopes, Error,
    Field, Gap, GapKind, GapSubject, Operation, ReaderKind, Registry, ScopeId, ScopeReference,
    Source, Support,
};
use crate::binding::{Binding, VerifiedAnalysis, unique_named_candidate};
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
        match &self.backend {
            Backend::Live { binding, .. } => BuildId(binding.build().into()),
            Backend::Recorded(answers) => answers.build.clone(),
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
        let recorder = match &self.backend {
            Backend::Recorded(answers) => return answers.read(question, subject),
            Backend::Live { recorder, .. } => recorder,
        };
        let answer = method();
        if let Some(directory) = recorder {
            crate::recorded::write(directory, &self.build(), question, subject, &answer)?;
        }
        answer
    }

    /// Run a method whose own result recorded answers do not hold. With recorded answers this is
    /// `Error::Unsupported`. The result is not written to a recorder.
    pub(super) fn method_result<T>(
        &self,
        operation: Operation,
        method: impl FnOnce() -> Result<T, Error>,
    ) -> Result<T, Error> {
        match &self.backend {
            Backend::Recorded(_) => Err(Error::Unsupported {
                operation,
                reason: "recorded answers do not hold the method's internal result".into(),
            }),
            Backend::Live { .. } => method(),
        }
    }

    /// Whether this build and host can answer an operation. This never starts a game; for a live
    /// operation it checks that the supervisor's tools can be found. Selected content is checked
    /// when `start_game` is called. With recorded answers every operation is `Supported`; this
    /// does not check that an answer file exists, so a question can still return
    /// `Error::NotRecorded`.
    pub fn supports(&self, operation: Operation) -> Support {
        match &self.backend {
            Backend::Recorded(_) => Support::Supported,
            Backend::Live { binding, .. } => self.live_support(binding, operation),
        }
    }

    fn live_support(&self, binding: &Binding, operation: Operation) -> Support {
        if operation == Operation::ObserveFixture && !binding.has_fixture_method() {
            return Support::Unsupported("this build has no fixture observation recipe".into());
        }
        if operation.is_declaration() && !binding.has_declarations_method() {
            return Support::Unsupported("this build has no declaration recipe".into());
        }
        match operation {
            Operation::Defines
            | Operation::Registries
            | Operation::RegistryFields
            | Operation::Declarations
            | Operation::CommandGrammar
            | Operation::Modifiers
            | Operation::ModifierCategories
            | Operation::ModifierFamilies
            | Operation::Scopes
            | Operation::ScopeLinks
            | Operation::LocalizationDeclarations
            | Operation::OnActions
            | Operation::GameRules => match &binding.analysis {
                Some(analysis) => match analysis.executable() {
                    Ok(_) => Support::Supported,
                    Err(reason) => Support::Unsupported(error(operation, reason).to_string()),
                },
                None => Support::Unsupported("this build has no static analysis recipe".into()),
            },
            Operation::LoadedModifiers if !binding.has_modifier_table_method() => {
                Support::Unsupported("this build has no loaded modifier table recipe".into())
            }
            Operation::RegistryItems | Operation::ObserveFixture | Operation::LoadedModifiers => {
                match self.selected_blocking_reasons(binding) {
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
            completeness: Completeness::from_gaps(&gaps),
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
            completeness: Completeness::from_gaps(&gaps),
            gaps,
            source: Source::new(self.build(), fields::METHOD, Basis::StaticAnalysis),
        }
    }

    /// The registry field method's own result, with every path and stop.
    pub(crate) fn registry_field_result(
        &self,
        registry: &str,
    ) -> Result<RegistryFieldResult, Error> {
        let operation = Operation::RegistryFields;
        let name = registry.trim_end_matches('/');
        let verified = self.verified_analysis(operation)?;
        let Some(candidate) = unique_named_candidate(verified.named_candidates(), name) else {
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
        .map(|field| super::fields::field(field, result))
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
                ..
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
        completeness: Completeness::from_gaps(&gaps),
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

fn normalized_gaps(result: &RegistryFieldResult, registry: &str) -> Vec<Gap> {
    let mut gaps = vec![Gap {
        kind: GapKind::OutsideMethod,
        subject: Some(GapSubject::registry(registry)),
        detail: "Defaults, exhaustive enum domains, occurrence limits, deeper nested grammars, and non-owner runtime callers are not established by this method.".into(),
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
        if result
            .collections
            .iter()
            .any(|collection| collection.token == field.token)
        {
            continue;
        }
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
        if gap.path.is_some_and(|index| {
            let path = &result.paths[index];
            path.domain[0] == path.domain[1]
                && result
                    .collections
                    .iter()
                    .any(|collection| collection.token == path.domain[0])
        }) {
            continue;
        }
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
            FieldGapKind::RuntimeSelection => (
                GapKind::UnresolvedCondition,
                "Use-time analysis covers local Boolean selections only; enclosing context, other callers and bounded or unsupported paths remain unresolved.",
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
            detail: format!("{unnamed} reader paths have no recovered literal field name; anonymous or dynamic keys remain unresolved."),
        });
    }
    for field in normalized_fields(result) {
        if field.shape.repeat == crate::RepeatBehavior::Unknown
            || matches!(field.members, crate::FieldMembers::Unresolved)
        {
            gaps.push(Gap {
                kind: GapKind::UnresolvedStorage,
                subject: Some(GapSubject::field(&field.name)),
                detail: "Repeat behavior or nested fields remain unresolved.".into(),
            });
        }
        if field
            .read
            .iter()
            .any(|alternative| unresolved_condition(&alternative.condition))
        {
            gaps.push(Gap {
                kind: GapKind::UnresolvedCondition,
                subject: Some(GapSubject::field(&field.name)),
                detail:
                    "A loader condition remains unresolved; its outcome is retained separately."
                        .into(),
            });
        }
    }
    for collection in &result.collections {
        let Some(parent) = result
            .fields
            .iter()
            .find(|field| field.token == collection.token)
        else {
            continue;
        };
        for mut gap in normalized_gaps(&collection.fields, registry) {
            if gap.kind == GapKind::OutsideMethod {
                continue;
            }
            let name = gap
                .subject
                .as_ref()
                .map(|subject| subject.name())
                .unwrap_or("");
            gap.subject = Some(GapSubject::field(format!("{}.{}", parent.name, name)));
            gaps.push(gap);
        }
    }
    for selection in &result.uses {
        gaps.push(Gap { kind: GapKind::UnresolvedCondition, subject: Some(GapSubject::field(selection.field.join("."))), detail: "The local use-time flag test is established; the enclosing selection context is unresolved.".into() });
    }
    gaps
}

fn unresolved_condition(condition: &crate::FieldCondition) -> bool {
    match condition {
        crate::FieldCondition::Unresolved => true,
        crate::FieldCondition::All(terms) => terms.iter().any(unresolved_condition),
        _ => false,
    }
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
                        factory: 0,
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
                    factory: 0,
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
            tail: true,
        };
        RegistryFieldResult {
            persistent: Default::default(),
            uses: vec![],
            collections: vec![],
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
            (FieldGapKind::RuntimeSelection, GapKind::UnresolvedCondition),
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
