//! Static questions: answered from the executable, with no game process.
use super::Native;
use crate::answer::{
    Answer, Basis, BuildId, Completeness, Error, Field, Gap, GapKind, Operation, Reader, ReaderId,
    ReaderKind, Registry, Source, Support,
};
use crate::binding::NamedCandidate;
use crate::engine::analysis::{
    decode::{AnalysisOrigin, AnalysisProvenance},
    directories::{self, Directory},
    fields::{self, FieldDescriptor, PathOutcome, ReaderJoin, RegistryFieldResult},
};
use crate::{AnalysisError, UnavailableReason};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

fn error(operation: Operation, error: AnalysisError) -> Error {
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
            Some(_) => BuildId("recorded".into()),
            None => BuildId(self.bound().build().into()),
        }
    }

    /// Answer from recorded files when they are the back end; otherwise run the method, and
    /// write the result when a recorder is set.
    fn answer<T: serde::Serialize + serde::de::DeserializeOwned>(
        &self,
        question: &str,
        subject: Option<&str>,
        method: impl FnOnce() -> Result<Answer<T>, Error>,
    ) -> Result<Answer<T>, Error> {
        if let Some(directory) = self.recorded() {
            return crate::recorded::read(directory, question, subject);
        }
        let answer = method();
        if let Some(directory) = self.recorder() {
            crate::recorded::write(directory, question, subject, &answer)?;
        }
        answer
    }

    /// Whether this build and host can answer an operation. This never starts a game; for a live
    /// operation it checks that the supervisor's tools can be found.
    pub fn supports(&self, operation: Operation) -> Support {
        if self.recorded().is_some() {
            return Support::Supported;
        }
        match operation {
            Operation::Registries | Operation::RegistryFields => {
                match self
                    .bound()
                    .analysis
                    .as_ref()
                    .map(|a| a.discovery.is_some())
                {
                    Some(true) => Support::Supported,
                    _ => Support::Unsupported("this build has no static analysis recipe".into()),
                }
            }
            Operation::RegistryItems => match self.blocking_reasons() {
                reasons if reasons.is_empty() => Support::Supported,
                reasons => Support::Unsupported(format!("{reasons:?}")),
            },
        }
    }

    fn named_candidates(&self, operation: Operation) -> Result<&[NamedCandidate], Error> {
        self.candidates
            .get_or_init(|| {
                self.bound()
                    .analysis
                    .as_ref()
                    .ok_or(AnalysisError::Unavailable {
                        reasons: vec![UnavailableReason::ImplementationUnavailable],
                    })
                    .and_then(|analysis| analysis.named_candidates())
            })
            .as_deref()
            .map_err(|e| error(operation, e.clone()))
    }

    /// List the engine registries, each named by its content directory.
    ///
    /// The answer is always partial: the method finds registries that use the engine's shared
    /// database template, and does not find custom, nested, or late loaders.
    pub fn registries(&self) -> Result<Answer<Vec<Registry>>, Error> {
        self.answer("registries", None, || self.registries_from_executable())
    }

    fn registries_from_executable(&self) -> Result<Answer<Vec<Registry>>, Error> {
        let candidates = self.named_candidates(Operation::Registries)?;
        let mut names = BTreeSet::new();
        let mut unnamed = 0;
        for candidate in candidates {
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
            completeness: Completeness::Partial,
            gaps,
            source: Source::new(self.build(), directories::METHOD, Basis::StaticAnalysis),
        })
    }

    /// List the root fields of one registry's definitions, with the shared reader of each.
    ///
    /// The answer is always partial: a reader identity establishes routing only, and nested
    /// blocks, inherited readers, and dynamic names are outside the method.
    pub fn registry_fields(&self, registry: &str) -> Result<Answer<Vec<Field>>, Error> {
        self.answer("registry_fields", Some(registry), || {
            self.registry_fields_from_executable(registry)
        })
    }

    fn registry_fields_from_executable(&self, registry: &str) -> Result<Answer<Vec<Field>>, Error> {
        let operation = Operation::RegistryFields;
        let name = registry.trim_end_matches('/');
        let mut matching = self
            .named_candidates(operation)?
            .iter()
            .filter(|c| c.directory == Directory::Named(name.to_owned()));
        let (Some(candidate), None) = (matching.next(), matching.next()) else {
            return Err(Error::UnknownRegistry {
                name: registry.into(),
            });
        };
        let analysis = self.bound().analysis.as_ref().expect("candidates exist");
        let input = analysis
            .field_input(candidate.record.clone())
            .map_err(|e| error(operation, e))?;
        let result = fields::analyze(
            descriptor(analysis),
            input,
            AnalysisOrigin::Executable,
            Vec::new(),
        )
        .map_err(|e| Error::Method(e.to_string()))?;
        Ok(Answer {
            value: normalized_fields(&result),
            completeness: Completeness::Partial,
            gaps: normalized_gaps(&result, name),
            source: Source::new(self.build(), fields::METHOD, Basis::StaticAnalysis),
        })
    }
}

// The method's internal result still carries a replay descriptor; step 4 removes it.
fn descriptor(analysis: &crate::binding::BoundAnalysis) -> FieldDescriptor {
    let inputs = analysis.fields.as_ref().unwrap_or(&analysis.inputs);
    FieldDescriptor {
        format: fields::FORMAT.into(),
        capture_origin: evidence::CaptureOrigin::Captured,
        provenance: AnalysisProvenance {
            executable: inputs.executable.clone(),
            slice: inputs.slice.clone(),
            composition: inputs.composition.clone(),
            method: fields::METHOD.into(),
            decoder: inputs.decoder.into(),
            implementation: inputs.implementation.clone(),
            qualification_records: Vec::new(),
            evidence: Vec::new(),
        },
        input: evidence::ArtifactReference {
            path: "executable".into(),
            sha256: inputs.executable.clone(),
            bytes: 0,
        },
    }
}

fn normalized_fields(result: &RegistryFieldResult) -> Vec<Field> {
    result
        .fields
        .iter()
        .map(|field| {
            let callees: BTreeSet<&str> = field
                .readers
                .iter()
                .filter_map(|reader| match reader {
                    ReaderJoin::Joined { callee, .. } => Some(callee.as_str()),
                    ReaderJoin::Missing { .. } => None,
                })
                .collect();
            let all_joined = field
                .readers
                .iter()
                .all(|reader| matches!(reader, ReaderJoin::Joined { .. }));
            let id = (all_joined && callees.len() == 1).then(|| {
                let digest = Sha256::digest(callees.first().expect("one callee").as_bytes());
                ReaderId(format!("{digest:x}")[..16].to_owned())
            });
            Field {
                name: field.name.clone(),
                reader: Reader {
                    id,
                    kind: ReaderKind::Unknown,
                },
                conditional: field.readers.len() > 1
                    || field
                        .paths
                        .iter()
                        .any(|&path| !result.paths[path].conditions.is_empty()),
            }
        })
        .collect()
}

fn normalized_gaps(result: &RegistryFieldResult, registry: &str) -> Vec<Gap> {
    let mut gaps = vec![Gap {
        kind: GapKind::ReaderSemantics,
        subject: Some(registry.into()),
        detail: "A reader identity establishes routing only, not the accepted values.".into(),
    }];
    let field_of = |path: usize| {
        result
            .fields
            .iter()
            .find(|field| field.paths.contains(&path))
            .map(|field| field.name.clone())
    };
    let mut seen = BTreeSet::new();
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
