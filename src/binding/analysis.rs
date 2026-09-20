use std::sync::Mutex;

use super::{binary, installation::Installation};
use crate::{AnalysisError, UnavailableReason, qualification::analysis::AnalysisInputs};

pub(crate) type Decoder = fn(
    &[u8],
    u64,
) -> Result<
    Vec<crate::engine::analysis::decode::Instruction>,
    crate::engine::analysis::decode::DecodeError,
>;

/// Bound static methods share one executable integrity state and independent admission.
#[derive(Debug)]
pub(crate) struct BoundAnalysis {
    pub inputs: AnalysisInputs,
    pub control: DecodeControl,
    pub decoder: Decoder,
    pub discovery: Option<BoundDiscovery>,
    pub fields: Option<AnalysisInputs>,
    installation: Installation,
    invalidated: Mutex<Option<UnavailableReason>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct DecodeControl {
    pub address: u64,
    pub length: u64,
    pub code: crate::ArtifactReference,
}

impl BoundAnalysis {
    pub(super) fn new(
        inputs: AnalysisInputs,
        control: DecodeControl,
        decoder: Decoder,
        installation: Installation,
    ) -> Self {
        Self {
            inputs,
            control,
            decoder,
            discovery: None,
            fields: None,
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
            if image.executable != self.inputs.executable || image.slice != self.inputs.slice {
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

    pub(crate) fn read(&self) -> Result<Vec<u8>, AnalysisError> {
        let bytes = self.executable()?;
        let code = binary::code_range(&bytes, self.control.address, self.control.length)?;
        if binary::hash(&code) != self.control.code.sha256
            || code.len() as u64 != self.control.code.bytes
        {
            return Err(AnalysisError::InvalidRange);
        }
        Ok(code)
    }
}

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(crate) struct BoundDiscovery {
    pub inputs: AnalysisInputs,
    pub layout: crate::engine::analysis::discovery::SchedulerLayout,
}
impl BoundAnalysis {
    pub(crate) fn discovery_input(
        &self,
    ) -> Result<crate::engine::analysis::discovery::StaticInput, AnalysisError> {
        let bytes = self.executable()?;
        let discovery = self
            .discovery
            .as_ref()
            .ok_or_else(|| AnalysisError::Unavailable {
                reasons: vec![UnavailableReason::ImplementationUnavailable],
            })?;
        binary::discovery::read(&bytes, &discovery.layout)
    }
}

impl BoundAnalysis {
    pub(crate) fn field_input(
        &self,
        selection: crate::engine::analysis::discovery::CandidateRecord,
    ) -> Result<crate::engine::analysis::fields::FieldInput, AnalysisError> {
        let bytes = self.executable()?;
        let discovery = self.discovery.as_ref().ok_or(AnalysisError::InvalidRange)?;
        let input = binary::discovery::read(&bytes, &discovery.layout)?;
        if !crate::engine::analysis::discovery::candidates(&input.symbols).contains(&selection) {
            return Err(AnalysisError::InvalidRange);
        }
        binary::fields::read(&bytes, input, selection)
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
        let layout = &self
            .discovery
            .as_ref()
            .ok_or_else(|| AnalysisError::Unavailable {
                reasons: vec![UnavailableReason::ImplementationUnavailable],
            })?
            .layout;
        let input = binary::discovery::read(&bytes, layout)?;
        let records = discovery::candidates(&input.symbols);
        let constructors = binary::constructors::read(&bytes, &input, &records)?;
        Ok(records
            .into_iter()
            .map(|record| NamedCandidate {
                directory: directories::directory(
                    constructors
                        .get(&record.database)
                        .map_or(&[][..], Vec::as_slice),
                    &input.strings,
                ),
                record,
            })
            .collect())
    }
}
