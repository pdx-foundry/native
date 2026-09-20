use super::AdmissionInputs;
use crate::{
    Availability, CapabilityReport, CapabilityRequest, ContextIdentity, ContextOrigin,
    Qualification, RegistryBounds, UnavailableReason,
};

pub(crate) fn evaluate(
    inputs: &AdmissionInputs,
    request: &CapabilityRequest,
    origin: ContextOrigin,
    integrity: Option<UnavailableReason>,
) -> CapabilityReport {
    let mut report = CapabilityReport {
        context: ContextIdentity(inputs.composition.clone()),
        origin,
        qualification: Qualification::Incomplete,
        availability: Availability::Unavailable,
        bounds: crate::CapabilityBounds::Registry(inputs.bounds.clone()),
        accepted_bounds: Vec::new(),
        reasons: inputs.prerequisites.clone(),
        qualification_records: Vec::new(),
        evidence: Vec::new(),
    };
    if let Some(reason) = integrity {
        report.reasons.push(reason);
    }
    if let Err(reason) = &inputs.content
        && !report.reasons.contains(reason)
    {
        report.reasons.push(reason.clone());
    }
    if let Err(reason) = &inputs.toolchain
        && !report.reasons.contains(reason)
    {
        report.reasons.push(reason.clone());
    }
    if !covers(&inputs.bounds, request) {
        report.qualification = Qualification::OutsideSupport;
        report.reasons.push(UnavailableReason::OutsideBounds);
        return report;
    }

    // A live operation is available when the build composed, the request is inside the declared
    // bounds, and the present inputs and tools can be read. No acceptance record is needed.
    report.qualification = Qualification::Qualified;
    if report.reasons.is_empty() {
        report.availability = Availability::Available;
    }
    report
}

fn covers(bounds: &RegistryBounds, request: &CapabilityRequest) -> bool {
    matches!(request, CapabilityRequest::Registry { registry } if bounds.registries.contains(registry))
}
