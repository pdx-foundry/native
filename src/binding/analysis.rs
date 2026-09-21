//! The static methods bound to one installation: every read checks that the executable is
//! still the one that was opened.
use std::sync::Mutex;

use super::{binary, installation::Installation};
use crate::engine::analysis::discovery::SchedulerLayout;
use crate::{AnalysisError, UnavailableReason};

#[derive(Debug)]
pub(crate) struct BoundAnalysis {
    /// SHA-256 of the executable file and of its selected slice, as they were at `open`.
    executable: String,
    slice: String,
    layout: SchedulerLayout,
    installation: Installation,
    /// The first change that a read saw. It stays, even when the original bytes come back.
    invalidated: Mutex<Option<UnavailableReason>>,
}

impl BoundAnalysis {
    pub(super) fn new(
        executable: String,
        slice: String,
        layout: SchedulerLayout,
        installation: Installation,
    ) -> Self {
        Self {
            executable,
            slice,
            layout,
            installation,
            invalidated: Mutex::new(None),
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
        let bytes = self.installation.executable_bytes().and_then(|bytes| {
            let image = binary::identify(&bytes).map_err(|_| UnavailableReason::TargetChanged)?;
            if image.executable != self.executable || image.slice != self.slice {
                return Err(UnavailableReason::TargetChanged);
            }
            Ok(bytes)
        });
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
}

#[cfg(test)]
mod tests;

impl BoundAnalysis {
    /// Resolve one public field reader from the same executable analysis used by
    /// `Native::registry_fields`.
    pub(crate) fn registry_field(
        &self,
        registry: &str,
        field_name: &str,
    ) -> Result<Option<crate::Field>, AnalysisError> {
        use crate::engine::analysis::{directories::Directory, fields};

        let candidates = self.named_candidates()?;
        let mut matching = candidates
            .iter()
            .filter(|candidate| candidate.directory == Directory::Named(registry.into()));
        let (Some(candidate), None) = (matching.next(), matching.next()) else {
            return Ok(None);
        };
        let input = self.field_input(candidate.record.clone())?;
        let result = fields::analyze(&input).map_err(|_| AnalysisError::InvalidRange)?;
        Ok(result
            .fields
            .iter()
            .find(|field| field.name == field_name)
            .map(|field| crate::session::questions::normalized_field(field, &result)))
    }

    pub(crate) fn field_input(
        &self,
        selection: crate::engine::analysis::discovery::CandidateRecord,
    ) -> Result<crate::engine::analysis::fields::FieldInput, AnalysisError> {
        let bytes = self.executable()?;
        let input = binary::discovery::read(&bytes, &self.layout)?;
        if !crate::engine::analysis::discovery::candidates(&input.symbols).contains(&selection) {
            return Err(AnalysisError::InvalidRange);
        }
        binary::fields::read(&bytes, input, selection)
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
    /// Template candidates with their directories, in candidate order.
    pub(crate) fn named_candidates(&self) -> Result<Vec<NamedCandidate>, AnalysisError> {
        use crate::engine::analysis::{directories, discovery};
        let bytes = self.executable()?;
        let input = binary::discovery::read(&bytes, &self.layout)?;
        let records = discovery::candidates(&input.symbols);
        let constructors = binary::constructors::read(&bytes, &input, &records)?;
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
        // Static initializers are read only when a constructor passes a global.
        let needs_globals = arguments
            .iter()
            .flatten()
            .any(|a| matches!(a, directories::Argument::Global(_)));
        let globals = if needs_globals {
            let initializers = binary::constructors::initializers(&bytes, &input)?;
            directories::globals(&initializers, &anchors, &input.strings)
        } else {
            Default::default()
        };
        Ok(records
            .into_iter()
            .zip(arguments)
            .map(|(record, arguments)| NamedCandidate {
                directory: directories::directory(&arguments, &globals),
                record,
            })
            .collect())
    }
}
