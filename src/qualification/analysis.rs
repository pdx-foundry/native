use crate::{
    Availability, CapabilityBounds, CapabilityReport, ContextIdentity, ContextOrigin,
    Qualification, UnavailableReason,
};

/// Static composition inputs contain no content or live-tool prerequisites.
#[derive(Debug, Clone)]
pub(crate) struct AnalysisInputs {
    pub composition: String,
    pub executable: String,
    pub slice: String,
    pub implementation: String,
    pub method: &'static str,
    pub decoder: &'static str,
}

/// A static method is available when its target composed and the executable is unchanged.
pub(crate) fn evaluate(
    inputs: &AnalysisInputs,
    origin: ContextOrigin,
    integrity: Option<UnavailableReason>,
) -> CapabilityReport {
    let mut report = unavailable(ContextIdentity(inputs.composition.clone()), origin);
    if inputs.method == crate::engine::analysis::discovery::METHOD {
        report.bounds = CapabilityBounds::RegistryDiscovery;
    }
    if inputs.method == crate::engine::analysis::fields::METHOD {
        report.bounds = CapabilityBounds::RegistryFields;
    }
    report.reasons.clear();
    if let Some(reason) = integrity {
        report.reasons.push(reason);
        return report;
    }
    report.qualification = Qualification::Qualified;
    report.availability = Availability::Available;
    report.accepted_bounds.push(report.bounds.clone());
    report
}

pub(crate) fn unavailable(context: ContextIdentity, origin: ContextOrigin) -> CapabilityReport {
    CapabilityReport {
        context,
        origin,
        qualification: Qualification::Incomplete,
        availability: Availability::Unavailable,
        bounds: CapabilityBounds::StaticDecode,
        accepted_bounds: Vec::new(),
        reasons: vec![UnavailableReason::ImplementationUnavailable],
        qualification_records: Vec::new(),
        evidence: Vec::new(),
    }
}
