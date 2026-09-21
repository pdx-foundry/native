//! The static methods bound to one installation: every read checks that the executable is
//! still the one that was opened.
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

use super::{binary, installation::Installation};
use crate::engine::analysis::discovery::{SchedulerLayout, Symbol};
use crate::{AnalysisError, UnavailableReason};

pub(crate) struct BoundAnalysis {
    layout: SchedulerLayout,
    installation: Installation,
    /// The first change that a read saw. It stays, even when the original bytes come back.
    invalidated: Mutex<Option<UnavailableReason>>,
    catalog: OnceLock<Result<Catalog, AnalysisError>>,
}

struct Catalog {
    candidates: Vec<NamedCandidate>,
    symbols: Vec<Symbol>,
    strings: BTreeMap<u64, String>,
}

/// One fresh integrity check and the immutable analysis derived from this installation.
pub(crate) struct VerifiedAnalysis<'a> {
    executable: Vec<u8>,
    catalog: &'a Catalog,
}

impl VerifiedAnalysis<'_> {
    pub(crate) fn named_candidates(&self) -> &[NamedCandidate] {
        &self.catalog.candidates
    }

    /// Read fields for a candidate from this analysis; reject records from another source.
    pub(crate) fn field_input(
        &self,
        selection: crate::engine::analysis::discovery::CandidateRecord,
    ) -> Result<crate::engine::analysis::fields::FieldInput, AnalysisError> {
        if !self
            .catalog
            .candidates
            .iter()
            .any(|candidate| candidate.record == selection)
        {
            return Err(AnalysisError::InvalidRange);
        }
        binary::fields::read(
            &self.executable,
            &self.catalog.symbols,
            &self.catalog.strings,
            selection,
        )
    }
}

impl BoundAnalysis {
    pub(super) fn new(layout: SchedulerLayout, installation: Installation) -> Self {
        Self {
            layout,
            installation,
            invalidated: Mutex::new(None),
            catalog: OnceLock::new(),
        }
    }

    pub(crate) fn executable(&self) -> Result<Vec<u8>, AnalysisError> {
        let mut invalidated = self
            .invalidated
            .lock()
            .expect("static executable integrity lock");
        if let Some(reason) = &*invalidated {
            return Err(AnalysisError::Unavailable {
                reasons: vec![reason.clone()],
            });
        }
        // The full-file hash pins every byte of the selected slice identified at open.
        let bytes = self.installation.executable_bytes();
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(reason) => {
                *invalidated = Some(reason.clone());
                return Err(AnalysisError::Unavailable {
                    reasons: vec![reason],
                });
            }
        };
        Ok(bytes)
    }

    pub(crate) fn verified(&self) -> Result<VerifiedAnalysis<'_>, AnalysisError> {
        let executable = self.executable()?;
        let catalog = self
            .catalog
            .get_or_init(|| self.build_catalog(&executable))
            .as_ref()
            .map_err(Clone::clone)?;
        Ok(VerifiedAnalysis {
            executable,
            catalog,
        })
    }
}

#[cfg(test)]
mod tests;

impl BoundAnalysis {
    /// Resolve public field readers from the same executable analysis used by
    /// `Native::registry_fields`.
    pub(crate) fn registry_fields(
        &self,
        registry: &str,
    ) -> Result<Option<Vec<crate::Field>>, AnalysisError> {
        use crate::engine::analysis::{directories::Directory, fields};

        let verified = self.verified()?;
        let mut matching = verified
            .named_candidates()
            .iter()
            .filter(|candidate| candidate.directory == Directory::Named(registry.into()));
        let (Some(candidate), None) = (matching.next(), matching.next()) else {
            return Ok(None);
        };
        let input = verified.field_input(candidate.record.clone())?;
        let result = fields::analyze(&input).map_err(|_| AnalysisError::InvalidRange)?;
        Ok(Some(crate::session::questions::normalized_fields(&result)))
    }

    pub(crate) fn reference_inputs(
        &self,
        owners: &[&str],
    ) -> Result<Vec<crate::engine::analysis::references::ReferenceInput>, AnalysisError> {
        let bytes = self.executable()?;
        let input = binary::discovery::read(&bytes, &self.layout)?;
        owners
            .iter()
            .map(|owner| binary::references::read(&bytes, &input, owner))
            .collect()
    }
}

/// One template candidate and the content directory that its constructors establish.
#[derive(Debug, Clone)]
pub(crate) struct NamedCandidate {
    pub record: crate::engine::analysis::discovery::CandidateRecord,
    pub directory: crate::engine::analysis::directories::Directory,
}

impl BoundAnalysis {
    fn build_catalog(&self, bytes: &[u8]) -> Result<Catalog, AnalysisError> {
        use crate::engine::analysis::{directories, discovery};
        let input = binary::discovery::read(bytes, &self.layout)?;
        let records = discovery::candidates(&input.symbols);
        let constructors = binary::constructors::read(bytes, &input, &records)?;
        let anchors = binary::constructors::anchors(&input);
        let arguments: Vec<Vec<directories::Argument>> = records
            .iter()
            .map(|record| {
                constructors
                    .get(&record.database)
                    .into_iter()
                    .flatten()
                    .flat_map(|body| directories::arguments(body, &anchors, &input.strings))
                    .collect()
            })
            .collect();
        let needs_globals = arguments
            .iter()
            .flatten()
            .any(|a| matches!(a, directories::Argument::Global(_)));
        let globals = if needs_globals {
            let initializers = binary::constructors::initializers(bytes, &input)?;
            directories::globals(&initializers, &anchors, &input.strings)
        } else {
            Default::default()
        };
        let candidates = records
            .into_iter()
            .zip(arguments)
            .map(|(record, arguments)| NamedCandidate {
                directory: directories::directory(&arguments, &globals),
                record,
            })
            .collect();
        Ok(Catalog {
            candidates,
            symbols: input.symbols,
            strings: input.strings,
        })
    }
}
