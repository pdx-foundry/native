//! Static questions: answered from the executable, with no game process.
use super::{Backend, Native};
use crate::answer::{
    Answer, Basis, BuildId, Completeness, Declaration, DeclarationKind, DeclaredScopes, Error,
    Field, Gap, GapKind, GapSubject, Operation, ReaderKind, Registry, ScopeId, ScopeReference,
    Source, Support, opaque_id,
};
use crate::binding::{Binding, VerifiedAnalysis};
use crate::engine::analysis::{
    declarations::{self, DeclarationResult, ScopeOutcome, ScopeType, Site},
    directories::{self, Directory},
    fields::{self, FieldGapKind, PathOutcome, RegistryFieldResult},
    modifiers::{self, CategoryInput, CategoryNames},
    references::ReferenceFacts,
    stop::Unresolved,
};
use crate::{AnalysisError, UnavailableReason};
use std::collections::BTreeSet;
use std::sync::Arc;

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
        operation: Operation,
        subject: Option<&str>,
        method: impl FnOnce() -> Result<Answer<T>, Error>,
    ) -> Result<Answer<T>, Error> {
        let recorder = match &self.backend {
            Backend::Recorded(answers) => return answers.read(operation, subject),
            Backend::Live { recorder, .. } => recorder,
        };
        let answer = method();
        if let Some(directory) = recorder {
            crate::recorded::write(directory, &self.build(), operation, subject, &answer)?;
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
    /// when `start_game` is called. A recorded operation is supported when at least one answer
    /// file exists. Individual subjects can still be absent or have malformed recordings.
    pub fn supports(&self, operation: Operation) -> Support {
        match &self.backend {
            Backend::Recorded(answers) => {
                if answers.contains(operation) {
                    Support::Supported
                } else {
                    Support::Unsupported(format!("no recorded answer for {}", operation.name()))
                }
            }
            Backend::Live { binding, .. } => self.live_support(binding, operation),
        }
    }

    fn live_support(&self, binding: &Binding, operation: Operation) -> Support {
        if operation == Operation::CheckScript && !binding.has_script_check_method() {
            return Support::Unsupported("this build has no script-check recipe".into());
        }
        if operation == Operation::ObserveFixture && !binding.has_fixture_method() {
            return Support::Unsupported("this build has no fixture observation recipe".into());
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
            | Operation::GameRules
            | Operation::DynamicNames
            | Operation::DerivedNames
            | Operation::ModifierNodes
            | Operation::ModifierCategoryKeys
            | Operation::ScriptExpansions
            | Operation::LoaderRules => match binding.analysis.executable() {
                Ok(_) => Support::Supported,
                Err(reason) => Support::Unsupported(error(operation, reason).to_string()),
            },
            Operation::LoadedModifiers if !binding.has_modifier_table_method() => {
                Support::Unsupported("this build has no loaded modifier table recipe".into())
            }
            Operation::ObserveFixture | Operation::LoadedModifiers | Operation::CheckScript => {
                match self.selected_blocking_reasons(binding) {
                    reasons if reasons.is_empty() => Support::Supported,
                    reasons => Support::Unsupported(
                        error(operation, AnalysisError::Unavailable { reasons }).to_string(),
                    ),
                }
            }
        }
    }

    fn verified_analysis(&self, operation: Operation) -> Result<VerifiedAnalysis<'_>, Error> {
        self.bound()
            .analysis
            .verified()
            .map_err(|e| error(operation, e))
    }

    /// Read effect or trigger declarations from the registration calls in executable text.
    ///
    /// A call with a literal token is one declaration. A call whose name the code composes at run
    /// time gives one declaration for each chain of callers that establishes the name. A call that
    /// cannot be followed, or unreadable documentation, leaves a gap. Registration through a
    /// function pointer, argument grammar, behavior, and actual scope availability are outside
    /// this method.
    pub fn declarations(&self, kind: DeclarationKind) -> Result<Answer<Vec<Declaration>>, Error> {
        self.answer(Operation::Declarations, Some(kind.subject()), || {
            let operation = Operation::Declarations;
            let input = self
                .bound()
                .analysis
                .declaration_input(kind)
                .map_err(|failure| error(operation, failure))?;
            let result =
                declarations::analyze(&input).map_err(|error| Error::Method(error.to_string()))?;
            Ok(normalized_declarations(&result, self.build()))
        })
    }

    /// List the engine registries, each named by its content directory.
    ///
    /// The search covers shared database-template candidates and the scheduled databases outside
    /// them whose own loader enumerates one content directory. Nested and late loaders are
    /// outside it. Completeness says whether every template candidate and every scheduled
    /// database was named.
    pub fn registries(&self) -> Result<Answer<Vec<Registry>>, Error> {
        self.answer(Operation::Registries, None, || {
            self.registries_from_executable()
        })
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
        let schedule = verified.database_schedule();
        let mut gaps = vec![Gap {
            kind: GapKind::OutsideMethod,
            subject: None,
            detail: "Registries with nested or late loaders, or with loaders that the database \
                     schedule does not list, are not listed."
                .into(),
        }];
        let mut unnamed_details = Vec::new();
        if unnamed > 0 {
            unnamed_details.push(format!(
                "{unnamed} template registries were found without one content directory."
            ));
        }
        match schedule {
            Ok(schedule) => {
                let mut unnamed_loaders = 0;
                for directory in schedule.loader_directories.values() {
                    match directory {
                        Directory::Named(name) => {
                            names.insert(name.clone());
                        }
                        Directory::Missing | Directory::Ambiguous(_) => unnamed_loaders += 1,
                    }
                }
                let inventory = &schedule.inventory;
                if unnamed_loaders > 0 {
                    unnamed_details.push(format!(
                        "{unnamed_loaders} of {} scheduled databases have no template loader, and \
                         their own functions enumerate no single content directory.",
                        inventory.rows
                    ));
                }
                if inventory.unresolved > 0 {
                    gaps.push(Gap {
                        kind: GapKind::UnreadableInput,
                        subject: None,
                        detail: format!(
                            "{} of {} scheduled databases name no single database type, so \
                             whether a template loader covers them is not established.",
                            inventory.unresolved, inventory.rows
                        ),
                    });
                }
            }
            Err(_) => gaps.push(Gap {
                kind: GapKind::UnreadableInput,
                subject: None,
                detail: "The database schedule could not be read, so databases outside the \
                         template method are not counted."
                    .into(),
            }),
        }
        if !unnamed_details.is_empty() {
            gaps.push(Gap {
                kind: GapKind::UnnamedRegistries,
                subject: None,
                detail: unnamed_details.join(" "),
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
    /// The search covers root reader paths for the items of one registry: the owner of a
    /// shared-template registry, or the one owner that a custom loader builds. Nested blocks,
    /// inherited readers, and dynamic names are outside it. Completeness says whether those root
    /// paths and their reader classifications were resolved.
    ///
    /// The name is a content directory from [`Native::registries`]. A listed registry whose
    /// loader builds no one owner is [`Error::Unsupported`] with the reason; another name is
    /// [`Error::UnknownRegistry`].
    pub fn registry_fields(&self, registry: &str) -> Result<Answer<Vec<Field>>, Error> {
        let registry = registry.trim_end_matches('/');
        self.answer(Operation::RegistryFields, Some(registry), || {
            self.registry_fields_from_executable(registry)
        })
    }

    fn registry_fields_from_executable(&self, registry: &str) -> Result<Answer<Vec<Field>>, Error> {
        self.registry_field_run(registry).map(|run| run.answer)
    }

    /// The public registry field answer, with the method's own result that it is derived from.
    pub(crate) fn registry_field_run(&self, registry: &str) -> Result<RegistryFieldRun, Error> {
        let (owner, result) = self.registry_field_result(registry)?;
        let facts = RegistryFieldFacts {
            references: self.reference_facts(Operation::RegistryFields)?,
            numeric: self.numeric_facts(Operation::RegistryFields)?,
            scoped: self.scoped_numeric_facts(Operation::RegistryFields)?,
            modifiers: self.modifier_block_facts()?,
            weights: self.weight_block_facts()?,
            triggered: self.triggered_modifier_facts()?,
            blocks: self.block_facts()?,
            categories: modifiers::category_names(self.category_input()?, []),
        };
        let (answer, entry_gaps) = self.registry_field_answer(registry, &owner, &result, &facts);
        Ok(RegistryFieldRun {
            answer,
            result,
            owner,
            entry_gaps,
        })
    }

    pub(crate) fn block_facts(&self) -> Result<&crate::binding::BlockFacts, Error> {
        self.bound()
            .analysis
            .block_facts()
            .map_err(|failure| error(Operation::RegistryFields, failure))
    }

    pub(crate) fn modifier_block_facts(
        &self,
    ) -> Result<&crate::engine::analysis::modifier_blocks::ModifierBlockFacts, Error> {
        self.bound()
            .analysis
            .modifier_block_facts()
            .map_err(|failure| error(Operation::RegistryFields, failure))
    }

    pub(crate) fn weight_block_facts(
        &self,
    ) -> Result<&crate::engine::analysis::weight_blocks::WeightBlockFacts, Error> {
        self.bound()
            .analysis
            .weight_block_facts()
            .map_err(|failure| error(Operation::RegistryFields, failure))
    }

    fn category_input(&self) -> Result<&CategoryInput, Error> {
        self.bound()
            .analysis
            .category_input()
            .map_err(|failure| error(Operation::RegistryFields, failure))
    }

    pub(crate) fn triggered_modifier_facts(
        &self,
    ) -> Result<&crate::engine::analysis::modifier_blocks::triggered::TriggeredFacts, Error> {
        self.bound()
            .analysis
            .triggered_modifier_facts()
            .map_err(|failure| error(Operation::RegistryFields, failure))
    }

    pub(crate) fn numeric_facts(
        &self,
        operation: Operation,
    ) -> Result<&crate::engine::analysis::numeric::NumericFacts, Error> {
        self.bound()
            .analysis
            .numeric_facts()
            .map_err(|failure| error(operation, failure))
    }

    pub(crate) fn scoped_numeric_facts(
        &self,
        operation: Operation,
    ) -> Result<&crate::engine::analysis::scoped_numeric::Facts, Error> {
        self.bound()
            .analysis
            .scoped_numeric_facts()
            .map_err(|failure| error(operation, failure))
    }

    /// The lookup of every reference reader in the opened executable.
    pub(crate) fn reference_facts(&self, operation: Operation) -> Result<&ReferenceFacts, Error> {
        self.bound()
            .analysis
            .reference_facts()
            .map_err(|failure| error(operation, failure))
    }

    /// The public answer that the registry field method's `result` gives for `registry`, and the
    /// gaps of that answer that block entry contexts add.
    fn registry_field_answer(
        &self,
        registry: &str,
        owner: &str,
        result: &RegistryFieldResult,
        facts: &RegistryFieldFacts<'_>,
    ) -> (Answer<Vec<Field>>, Vec<Gap>) {
        let registry = registry.trim_end_matches('/');
        let references = facts.references;
        let mut value = normalized_fields(result, references);
        let mut gaps = normalized_gaps(result, registry, references, &value);
        super::modifier_blocks::attach(
            &mut value,
            result,
            facts.modifiers,
            references,
            facts.blocks.scope_names.as_deref(),
            &mut gaps,
        );
        let clauses = super::triggered_modifiers::ClauseFacts {
            clauses: facts.triggered,
            modifiers: facts.modifiers,
            references,
            numeric: facts.numeric,
            scoped: facts.scoped,
            scope_names: facts.blocks.scope_names.as_deref(),
        };
        super::triggered_modifiers::attach(&mut value, result, &clauses, &mut gaps);
        super::container_masks::attach(
            &mut value,
            result,
            facts.triggered,
            &facts.categories,
            &mut gaps,
        );
        super::numeric::fields(&mut value, facts.numeric, &[], &mut gaps);
        super::scoped_numeric::fields(&mut value, result, facts.scoped, facts.numeric, &mut gaps);
        let weights = super::weight_blocks::WeightFacts {
            weights: facts.weights,
            references,
            numeric: facts.numeric,
            scoped: facts.scoped,
            scope_names: facts.blocks.scope_names.as_deref(),
        };
        super::weight_blocks::attach(&mut value, result, &weights, &mut gaps);
        super::read_scope::registry_fields(
            &mut value,
            result,
            facts.blocks.scope_names.as_deref(),
            &[],
            &mut gaps,
        );
        let mut entry_gaps = Vec::new();
        super::field_entries::attach(
            &mut value,
            result,
            registry,
            owner,
            facts.blocks,
            &mut entry_gaps,
        );
        gaps.extend(entry_gaps.iter().cloned());
        storage_gaps(&value, None, &mut gaps);
        let answer = Answer {
            value,
            completeness: Completeness::from_gaps(&gaps),
            gaps,
            source: Source::new(self.build(), fields::METHOD, Basis::StaticAnalysis),
        };
        (answer, entry_gaps)
    }

    /// The owner type of a registry's items and the registry field method's result, which the
    /// bound analysis keeps after its first run.
    pub(crate) fn registry_field_result(
        &self,
        registry: &str,
    ) -> Result<(String, Arc<RegistryFieldResult>), Error> {
        self.bound()
            .analysis
            .registry_field_result(registry.trim_end_matches('/'))
            .map_err(|e| error(Operation::RegistryFields, e))?
            .ok_or_else(|| self.unanswered_registry(Operation::RegistryFields, registry))
    }

    /// The error for a registry that the operation does not read. A registry that
    /// [`Native::registries`] lists through its own loader is [`Error::Unsupported`], with the
    /// reason of its owner join for the three registry queries; any other name is
    /// [`Error::UnknownRegistry`]. A changed executable keeps its own error.
    pub(crate) fn unanswered_registry(&self, operation: Operation, registry: &str) -> Error {
        let registry = registry.trim_end_matches('/');
        let verified = match self.bound().analysis.verified() {
            Ok(verified) => verified,
            Err(failure) => return error(operation, failure),
        };
        let Some(owner) = verified.registry_owner(registry) else {
            return Error::UnknownRegistry {
                name: registry.into(),
            };
        };
        let reason = match (operation, owner) {
            (
                Operation::RegistryFields | Operation::DerivedNames | Operation::ModifierFamilies,
                Err(reason),
            ) => reason,
            _ => "the registry has no template loader, and this operation reads only template \
                  registries"
                .into(),
        };

        Error::Unsupported { operation, reason }
    }

    /// The registry field method's input and a new run of the method over it.
    pub(crate) fn registry_field_input_and_result(
        &self,
        registry: &str,
    ) -> Result<(fields::FieldInput, RegistryFieldResult), Error> {
        let operation = Operation::RegistryFields;
        let name = registry.trim_end_matches('/');
        let verified = self.verified_analysis(operation)?;
        let Some(Ok(owner)) = verified.registry_owner(name) else {
            return Err(self.unanswered_registry(operation, registry));
        };
        let input = self
            .bound()
            .analysis
            .registry_field_input(&verified, owner)
            .map_err(|e| error(operation, e))?;
        let result = fields::analyze(&input).map_err(|e| error(operation, e.into()))?;

        Ok((input, result))
    }
}

/// One run of the registry field method, with the public answer derived from it.
pub(crate) struct RegistryFieldRun {
    pub answer: Answer<Vec<Field>>,
    pub result: Arc<RegistryFieldResult>,
    /// The owner type of the registry's items.
    pub owner: String,
    /// The gaps of `answer` that block entry contexts add.
    pub entry_gaps: Vec<Gap>,
}

/// The executable-wide facts that a registry field answer joins.
struct RegistryFieldFacts<'a> {
    references: &'a ReferenceFacts,
    numeric: &'a crate::engine::analysis::numeric::NumericFacts,
    scoped: &'a crate::engine::analysis::scoped_numeric::Facts,
    modifiers: &'a crate::engine::analysis::modifier_blocks::ModifierBlockFacts,
    weights: &'a crate::engine::analysis::weight_blocks::WeightBlockFacts,
    triggered: &'a crate::engine::analysis::modifier_blocks::triggered::TriggeredFacts,
    blocks: &'a crate::binding::BlockFacts,
    /// The name of each single category, which container masks are listed by.
    categories: CategoryNames,
}

pub(crate) fn normalized_fields(
    result: &RegistryFieldResult,
    references: &ReferenceFacts,
) -> Vec<Field> {
    result
        .fields
        .iter()
        .map(|field| super::fields::field(field, result, references))
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
                        if *link != declarations::SCOPE_TABLE_MISSING {
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
pub(crate) fn scope_id(scope: &ScopeType) -> ScopeId {
    ScopeId(opaque_id(&format!("scope-type/{}", scope.bit)))
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

/// The gaps that `result` gives, with its normalized `fields` in the order of `result.fields`.
/// The reader gaps follow the fields' own readers, so a field that has no reader id has one.
fn normalized_gaps(
    result: &RegistryFieldResult,
    registry: &str,
    references: &ReferenceFacts,
    fields: &[Field],
) -> Vec<Gap> {
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
    for (field, built) in result.fields.iter().zip(fields) {
        if result
            .collections
            .iter()
            .any(|collection| collection.token == field.token)
        {
            continue;
        }
        if let Some(detail) = super::fields::reference_gap(field, references) {
            gaps.push(Gap {
                kind: GapKind::ReaderSemantics,
                subject: Some(GapSubject::field(field.name.clone())),
                detail,
            });
        }
        if let Some((kind, detail)) = reader_gap(&built.reader) {
            seen.insert((kind as u8, Some(field.name.clone()), false));
            gaps.push(Gap {
                kind,
                subject: Some(GapSubject::field(field.name.clone())),
                detail: detail.into(),
            });
        }
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
    for field in fields {
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
    for (token, nested) in result.nested_results() {
        let Some((parent, built)) = result
            .fields
            .iter()
            .zip(fields)
            .find(|(root, _)| root.token == token)
        else {
            continue;
        };
        let members = match &built.members {
            crate::FieldMembers::Fields(members) => members.as_slice(),
            _ => &[],
        };
        for mut gap in normalized_gaps(nested, registry, references, members) {
            if gap.kind == GapKind::OutsideMethod {
                continue;
            }
            // A gap on the nested reader as a whole belongs to the parent field.
            let name = match &gap.subject {
                Some(GapSubject::Field { name }) => format!("{}.{name}", parent.name),
                _ => parent.name.clone(),
            };
            gap.subject = Some(GapSubject::field(name));
            gaps.push(gap);
        }
    }
    for selection in &result.uses {
        gaps.push(Gap { kind: GapKind::UnresolvedCondition, subject: Some(GapSubject::field(selection.field.join("."))), detail: "The local use-time flag test is established; the enclosing selection context is unresolved.".into() });
    }
    gaps
}

/// A storage gap for each assembled field, at any `FieldMembers::Fields` depth, whose repeat
/// behavior is unknown or whose members no block family or attached grammar answers. A trigger or
/// effect family is the members answer; a grammar family whose grammar did not attach already has
/// the attach step's gap.
fn storage_gaps(fields: &[Field], parent: Option<&str>, gaps: &mut Vec<Gap>) {
    for field in fields {
        let name = match parent {
            Some(parent) => format!("{parent}.{}", field.name),
            None => field.name.clone(),
        };
        let repeat_unresolved = field.shape.repeat == crate::RepeatBehavior::Unknown;
        let members_unresolved = matches!(field.members, crate::FieldMembers::Unresolved)
            && field.reader.family == crate::BlockFamily::Unknown;
        let detail = match (repeat_unresolved, members_unresolved) {
            (true, true) => Some("Repeat behavior and nested fields remain unresolved."),
            (true, false) => Some("Repeat behavior remains unresolved."),
            (false, true) => Some("Nested fields remain unresolved."),
            (false, false) => None,
        };
        if let Some(detail) = detail {
            gaps.push(Gap {
                kind: GapKind::UnresolvedStorage,
                subject: Some(GapSubject::field(&name)),
                detail: detail.into(),
            });
        }

        if let crate::FieldMembers::Fields(children) = &field.members {
            storage_gaps(children, Some(&name), gaps);
        }
    }
}

/// The gap that a field's reader leaves: no identity means that the field's paths did not
/// establish one shared reader, and an identity with an unknown kind means that the value form is
/// not established.
fn reader_gap(reader: &crate::Reader) -> Option<(GapKind, &'static str)> {
    if reader.id.is_none() {
        Some((
            GapKind::UnresolvedReader,
            "The field's alternatives do not establish one shared reader.",
        ))
    } else if reader.kind == ReaderKind::Unknown {
        Some((
            GapKind::ReaderSemantics,
            "The shared reader is identified, but its broad value form is not established.",
        ))
    } else {
        None
    }
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
                    scopes: ScopeOutcome::Unresolved(Unresolved::new(
                        declarations::SCOPE_TABLE_MISSING,
                    )),
                },
            )],
            table_gaps: vec![declarations::SCOPE_TABLE_MISSING],
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
    use crate::engine::analysis::stop::{Obstacle, Unknown, Unresolved};
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
            persistent_points: Default::default(),
            scoped_destinations: Default::default(),
            stored_words: Default::default(),
            container_masks: Default::default(),
            uses: vec![],
            collections: vec![],
            embedded: Vec::new(),
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

    /// The gaps of `result` with its normalized fields.
    fn field_gaps(result: &RegistryFieldResult) -> Vec<Gap> {
        let references = ReferenceFacts::default();
        let fields = normalized_fields(result, &references);
        normalized_gaps(result, REGISTRY, &references, &fields)
    }

    /// A persistent read whose destination holds no known reader, so its field has no identity.
    fn unplaced_persistent_read() -> ReaderJoin {
        use crate::engine::analysis::fields::Value;

        ReaderJoin::Joined {
            callee: "CReader::Read(CPersistent&)".into(),
            arguments: [("x1".into(), Value::Owner(0x10))].into(),
            tail: true,
        }
    }

    /// The public gaps besides the method's boundary, which every answer carries.
    fn public_gaps(gaps: Vec<FieldGap>) -> Vec<Gap> {
        let mut public = field_gaps(&result(gaps));
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
    fn a_wide_unresolved_path_through_a_field_is_a_reader_gap_of_that_field() {
        let mut result = result(vec![]);
        result.paths.push(TokenPath {
            domain: [6, 8],
            conditions: vec![],
            instructions: vec![0x1004],
            terminal: 0x1004,
            outcome: PathOutcome::Gap(Unresolved::new("wide-path")),
        });

        let public = field_gaps(&result);

        assert!(
            public
                .iter()
                .any(|gap| gap.kind == GapKind::UnresolvedReader
                    && gap.subject == Some(GapSubject::field("known")))
        );
    }

    #[test]
    fn persistent_reads_of_different_destinations_are_a_reader_gap_of_that_field() {
        use crate::engine::analysis::fields::{ConcreteReader, Value};

        let persistent = |destination: i64| ReaderJoin::Joined {
            callee: "CReader::Read(CPersistent&)".into(),
            arguments: [("x1".into(), Value::Owner(destination))].into(),
            tail: true,
        };
        let concrete = |member: &str| ConcreteReader {
            read: "CPersistent::Read(CReader&)".into(),
            member: member.into(),
            family: crate::BlockFamily::Unknown,
            delegate: None,
        };
        let with_wide_path = |destination: i64| {
            let mut result = result(vec![]);
            result.persistent = [(0x10, concrete("A::Read")), (0x20, concrete("B::Read"))].into();
            result.fields[0].readers = vec![persistent(0x10)];
            result.paths[0].outcome = PathOutcome::Reader(persistent(0x10));
            result.paths.push(TokenPath {
                domain: [6, 8],
                conditions: vec![],
                instructions: vec![0x1004],
                terminal: 0x1004,
                outcome: PathOutcome::Reader(persistent(destination)),
            });
            result
        };
        let reader_gap = |result: &RegistryFieldResult| {
            field_gaps(result).iter().any(|gap| {
                gap.kind == GapKind::UnresolvedReader
                    && gap.subject == Some(GapSubject::field("known"))
            })
        };

        let disagreeing = with_wide_path(0x20);
        let field = &normalized_fields(&disagreeing, &ReferenceFacts::default())[0];
        assert_eq!(field.reader.id, None);
        assert!(reader_gap(&disagreeing));

        let agreeing = with_wide_path(0x10);
        let field = &normalized_fields(&agreeing, &ReferenceFacts::default())[0];
        assert!(field.reader.id.is_some());
        assert!(!reader_gap(&agreeing));
    }

    #[test]
    fn a_persistent_read_of_an_unknown_destination_is_a_reader_gap_of_that_field() {
        let mut result = result(vec![]);
        let persistent = unplaced_persistent_read();
        result.fields[0].readers = vec![persistent.clone()];
        result.paths[0].outcome = PathOutcome::Reader(persistent);

        let field = &normalized_fields(&result, &ReferenceFacts::default())[0];
        assert_eq!(field.reader.id, None);

        let reader_gaps: Vec<_> = field_gaps(&result)
            .into_iter()
            .filter(|gap| gap.kind == GapKind::UnresolvedReader)
            .collect();
        assert_eq!(reader_gaps.len(), 1);
        assert_eq!(reader_gaps[0].subject, Some(GapSubject::field("known")));
    }

    #[test]
    fn a_reader_with_an_identity_and_an_unknown_kind_is_a_semantics_gap() {
        let mut result = result(vec![]);
        let unclassified = ReaderJoin::Joined {
            callee: "CUnclassified::Read(CReader&)".into(),
            arguments: BTreeMap::new(),
            tail: true,
        };
        result.fields[0].readers = vec![unclassified.clone()];
        result.paths[0].outcome = PathOutcome::Reader(unclassified);

        let field = &normalized_fields(&result, &ReferenceFacts::default())[0];
        assert!(field.reader.id.is_some());
        assert_eq!(field.reader.kind, ReaderKind::Unknown);

        let public = field_gaps(&result);
        assert!(public.iter().any(|gap| gap.kind == GapKind::ReaderSemantics
            && gap.subject == Some(GapSubject::field("known"))));
        assert!(
            public
                .iter()
                .all(|gap| gap.kind != GapKind::UnresolvedReader)
        );
    }

    #[test]
    fn a_field_with_a_reader_path_gap_and_no_reader_id_has_one_reader_gap() {
        let mut result = result(vec![FieldGap {
            path: Some(0),
            ..FieldGap::new(FieldGapKind::ReaderJoin, "stopped")
        }]);
        let persistent = unplaced_persistent_read();
        result.fields[0].readers = vec![persistent.clone()];
        result.paths[0].outcome = PathOutcome::Reader(persistent);

        let reader_gaps: Vec<_> = field_gaps(&result)
            .into_iter()
            .filter(|gap| gap.subject == Some(GapSubject::field("known")))
            .collect();

        assert_eq!(reader_gaps.len(), 1);
        assert_eq!(reader_gaps[0].kind, GapKind::UnresolvedReader);
    }

    #[test]
    fn a_collection_member_without_a_reader_id_is_a_reader_gap_of_its_dotted_name() {
        use crate::engine::analysis::fields::CollectionField;

        let persistent = unplaced_persistent_read();
        let mut member = result(vec![]);
        member.fields[0].name = "child".into();
        member.fields[0].readers = vec![persistent.clone()];
        member.paths[0].outcome = PathOutcome::Reader(persistent);
        let mut parent = result(vec![]);
        parent.collections.push(CollectionField {
            token: 7,
            offset: 0x40,
            data_offset: None,
            count_offset: None,
            class: "CChild".into(),
            reader: None,
            fields: Box::new(member),
            container_mask: None,
        });

        let reader_gaps: Vec<_> = field_gaps(&parent)
            .into_iter()
            .filter(|gap| gap.kind == GapKind::UnresolvedReader)
            .collect();

        assert_eq!(reader_gaps.len(), 1);
        assert_eq!(
            reader_gaps[0].subject,
            Some(GapSubject::field("known.child"))
        );
    }

    #[test]
    fn storage_gaps_follow_the_assembled_family_grammar_and_repeat() {
        use crate::{
            BlockFamily, FieldDomain, FieldMembers, FieldReference, FieldShape, GrammarProperty,
            ModifierBlock, Reader, RepeatBehavior, ValueShape,
        };
        let field = |name: &str, family, members, repeat| Field {
            name: name.into(),
            reader: Reader {
                numeric: GrammarProperty::Unresolved,
                scoped_operand: GrammarProperty::Unresolved,
                id: None,
                kind: ReaderKind::Block,
                family,
            },
            shape: FieldShape {
                value: ValueShape::Block,
                repeat,
            },
            read: Vec::new(),
            members,
            domain: FieldDomain::Unknown,
            uses: Vec::new(),
            entry_contexts: Vec::new(),
            read_scope: GrammarProperty::Unresolved,
            accepted_categories: crate::AcceptedCategories::NotApplicable,
            reference: FieldReference::NotEstablished,
        };
        let grammar = FieldMembers::ModifierBlock(ModifierBlock {
            fixed_keys: GrammarProperty::Unresolved,
            entries: GrammarProperty::Unresolved,
        });
        let unknown_child = field(
            "child",
            BlockFamily::Trigger,
            FieldMembers::Unresolved,
            RepeatBehavior::Unknown,
        );
        let fields = [
            field(
                "trigger",
                BlockFamily::Trigger,
                FieldMembers::Unresolved,
                RepeatBehavior::Replace,
            ),
            field(
                "grammar",
                BlockFamily::Modifier,
                grammar,
                RepeatBehavior::Merges,
            ),
            field(
                "unclassified",
                BlockFamily::Unknown,
                FieldMembers::Unresolved,
                RepeatBehavior::Replace,
            ),
            field(
                "parent",
                BlockFamily::Unknown,
                FieldMembers::Fields(vec![unknown_child]),
                RepeatBehavior::Accumulate,
            ),
        ];
        let mut gaps = Vec::new();
        storage_gaps(&fields, None, &mut gaps);
        let found: Vec<_> = gaps
            .iter()
            .map(|gap| (gap.subject.as_ref().unwrap().name(), gap.detail.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                ("unclassified", "Nested fields remain unresolved."),
                ("parent.child", "Repeat behavior remains unresolved."),
            ]
        );
        assert!(
            gaps.iter()
                .all(|gap| gap.kind == GapKind::UnresolvedStorage)
        );
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
