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
    issued: std::sync::Mutex<Vec<crate::RegistrySubject>>,
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
    /// The discovery result or subject was not issued by this analysis context.
    ForeignSubject,
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
        binding.executable()?;
        let decode = capability(&binding);
        let discovery = discovery_capability(&binding);
        if decode.availability != Availability::Available
            && discovery.availability != Availability::Available
            && fields_capability(&binding).availability != Availability::Available
        {
            return Err(AnalysisError::Unavailable {
                reasons: decode.reasons,
            });
        }
        Ok(Self {
            binding,
            issued: std::sync::Mutex::new(Vec::new()),
        })
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

/// Evaluate discovery independently of the decode control and live prerequisites.
pub(crate) fn discovery_capability(binding: &BoundAnalysis) -> CapabilityReport {
    let Some(discovery) = &binding.discovery else {
        let mut report = analysis::unavailable(
            crate::ContextIdentity(binding.inputs.composition.clone()),
            ContextOrigin::Installation,
        );
        report.bounds = crate::CapabilityBounds::RegistryDiscovery;
        return report;
    };
    let integrity = match binding.executable() {
        Ok(_) => None,
        Err(AnalysisError::Unavailable { reasons }) => reasons.into_iter().next(),
        Err(_) => Some(UnavailableReason::InputUnavailable),
    };
    analysis::evaluate(
        &discovery.inputs,
        &AnalysisAuthority::bundled(),
        ContextOrigin::Installation,
        integrity,
    )
}
impl AnalysisContext {
    /// Discover bounded registry candidates without config seeds or a game launch.
    /// Static matches never establish live ownership; retain the input artifact for replay.
    pub fn discover_registries(&self) -> Result<crate::RegistryDiscoveryResult, AnalysisError> {
        let report = discovery_capability(&self.binding);
        if report.availability != Availability::Available {
            return Err(AnalysisError::Unavailable {
                reasons: report.reasons,
            });
        }
        let input = self.binding.discovery_input()?;
        let inputs = &self
            .binding
            .discovery
            .as_ref()
            .expect("admitted discovery binding")
            .inputs;
        let bytes = serde_json::to_vec(&input).expect("recorded discovery inputs");
        use sha2::{Digest, Sha256};
        let descriptor = evidence::discovery::DiscoveryDescriptor {
            format: evidence::discovery::FORMAT.into(),
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
            input: crate::ArtifactReference {
                path: "registry-discovery/input.json".into(),
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                bytes: bytes.len() as u64,
            },
            runs: vec![],
        };
        let result = evidence::discovery::derive(descriptor, &bytes, AnalysisOrigin::Executable)
            .map_err(|e| AnalysisError::Decode(e.to_string()))?;
        self.issued
            .lock()
            .expect("issued subject lock")
            .extend(result.candidates.iter().map(|c| c.subject.clone()));
        Ok(result)
    }
}

pub(crate) fn fields_capability(binding: &BoundAnalysis) -> CapabilityReport {
    let Some(inputs) = &binding.fields else {
        let mut report = analysis::unavailable(
            crate::ContextIdentity(binding.inputs.composition.clone()),
            ContextOrigin::Installation,
        );
        report.bounds = crate::CapabilityBounds::RegistryFields;
        return report;
    };
    let integrity = match binding.executable() {
        Ok(_) => None,
        Err(AnalysisError::Unavailable { reasons }) => reasons.into_iter().next(),
        Err(_) => Some(UnavailableReason::InputUnavailable),
    };
    analysis::evaluate(
        inputs,
        &AnalysisAuthority::bundled(),
        ContextOrigin::Installation,
        integrity,
    )
}
impl AnalysisContext {
    /// Analyze a candidate issued by this context's discovery operation. Foreign discovery results
    /// and subjects are rejected. Field existence and routing never imply a complete reader grammar.
    pub fn analyze_subject(
        &self,
        discovery: &crate::RegistryDiscoveryResult,
        subject: &crate::RegistrySubject,
    ) -> Result<crate::RegistryFieldResult, AnalysisError> {
        if !self
            .issued
            .lock()
            .expect("issued subject lock")
            .contains(subject)
        {
            return Err(AnalysisError::ForeignSubject);
        }
        let selection = evidence::fields::selection(discovery, subject)
            .map_err(|_| AnalysisError::ForeignSubject)?;
        let report = fields_capability(&self.binding);
        if report.availability != Availability::Available {
            return Err(AnalysisError::Unavailable {
                reasons: report.reasons,
            });
        }
        let input = self.binding.field_input(selection)?;
        let bytes = serde_json::to_vec(&input).expect("field input serialization");
        let inputs = self
            .binding
            .fields
            .as_ref()
            .expect("admitted fields binding");
        use sha2::{Digest, Sha256};
        let descriptor = evidence::fields::FieldDescriptor {
            format: evidence::fields::FORMAT.into(),
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
            input: crate::ArtifactReference {
                path: "registry-fields/input.json".into(),
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                bytes: bytes.len() as u64,
            },
        };
        evidence::fields::derive(descriptor, &bytes, AnalysisOrigin::Executable)
            .map_err(|e| AnalysisError::Decode(e.to_string()))
    }
}
