use std::sync::Arc;

use crate::{
    Availability, CapabilityReport, ContextOrigin, UnavailableReason,
    binding::BoundAnalysis,
    qualification::analysis::{self, AnalysisAuthority},
};
use evidence::analysis::{AnalysisDescriptor, AnalysisOrigin, AnalysisProvenance, AnalysisResult};

/// An admitted static analysis context. Every decode checks the executable again.
#[derive(Debug)]
pub struct AnalysisContext {
    binding: Arc<BoundAnalysis>,
}

/// Why a static operation could not produce complete decoded instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisError {
    /// Admission or current executable integrity failed.
    Unavailable {
        /// Independent admission gaps.
        reasons: Vec<UnavailableReason>,
    },
    /// The selected range is ambiguous, unmapped, truncated, or differs from the control.
    InvalidRange,
    /// The decoder could not account for every input byte.
    Decode(String),
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "static analysis failed: {self:?}")
    }
}
impl std::error::Error for AnalysisError {}

fn admit(binding: &BoundAnalysis) -> Result<(CapabilityReport, Vec<u8>), AnalysisError> {
    let bytes = binding.read()?;
    let report = analysis::evaluate(
        &binding.inputs,
        &AnalysisAuthority::bundled(),
        ContextOrigin::Installation,
        None,
    );
    if report.availability != Availability::Available {
        return Err(AnalysisError::Unavailable {
            reasons: report.reasons,
        });
    }
    Ok((report, bytes))
}

pub(crate) fn capability(binding: &BoundAnalysis) -> CapabilityReport {
    let integrity = match binding.read() {
        Ok(_) => None,
        Err(AnalysisError::Unavailable { reasons }) => reasons.into_iter().next(),
        Err(_) => Some(UnavailableReason::InputUnavailable),
    };
    analysis::evaluate(
        &binding.inputs,
        &AnalysisAuthority::bundled(),
        ContextOrigin::Installation,
        integrity,
    )
}

impl AnalysisContext {
    pub(crate) fn open(binding: Arc<BoundAnalysis>) -> Result<Self, AnalysisError> {
        admit(&binding)?;
        Ok(Self { binding })
    }

    /// Decode exactly the qualified recipe control. This does not infer function semantics,
    /// discover other functions, access content, probe tools, or start a process.
    pub fn decode_control(&self) -> Result<AnalysisResult, AnalysisError> {
        let (report, bytes) = admit(&self.binding)?;
        let control = &self.binding.control;
        let inputs = &self.binding.inputs;
        let instructions = (self.binding.decoder)(&bytes, control.address)
            .map_err(|error| AnalysisError::Decode(error.to_string()))?;
        Ok(AnalysisResult {
            origin: AnalysisOrigin::Executable,
            descriptor: AnalysisDescriptor {
                format: evidence::analysis::FORMAT.into(),
                capture_origin: evidence::CaptureOrigin::Captured,
                provenance: AnalysisProvenance {
                    executable: inputs.executable.clone(),
                    slice: inputs.slice.clone(),
                    composition: inputs.composition.clone(),
                    method: inputs.method.into(),
                    decoder: inputs.decoder.into(),
                    implementation: inputs.implementation.clone(),
                    qualification_records: report.qualification_records,
                    evidence: report.evidence,
                },
                address: control.address,
                code: control.code.clone(),
            },
            instructions,
        })
    }
}
