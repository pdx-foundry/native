use super::{AdmissionInputs, Authority};
use crate::{
    Availability, CapabilityReport, CapabilityRequest, ContextIdentity, ContextOrigin,
    Qualification, RegistryBounds, UnavailableReason,
};

pub(crate) fn evaluate(
    inputs: &AdmissionInputs,
    authority: &Authority,
    request: &CapabilityRequest,
    origin: ContextOrigin,
    integrity: Option<UnavailableReason>,
) -> CapabilityReport {
    let mut report = CapabilityReport {
        context: ContextIdentity(inputs.composition.clone()),
        origin,
        qualification: Qualification::Incomplete,
        availability: Availability::Unavailable,
        bounds: inputs.bounds.clone(),
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

    let matching: Vec<_> = authority
        .accepted
        .iter()
        .filter(|record| record.composition == inputs.composition)
        .collect();
    if matching.is_empty() {
        report.reasons.push(if authority.accepted.is_empty() {
            UnavailableReason::QualificationMissing
        } else {
            UnavailableReason::RevisionMismatch
        });
        return report;
    }
    let active: Vec<_> = matching
        .into_iter()
        .filter(|record| !authority.withdrawn.contains(&record.id))
        .collect();
    if active.is_empty() {
        report
            .reasons
            .push(UnavailableReason::QualificationWithdrawn);
        return report;
    }
    let Ok(content) = &inputs.content else {
        return report;
    };
    let applicable: Vec<_> = active
        .into_iter()
        .filter(|record| *content == record.content)
        .collect();
    if applicable.is_empty() {
        report.reasons.push(UnavailableReason::ContentMismatch);
        return report;
    }
    let applicable: Vec<_> = match &inputs.toolchain {
        Ok(toolchain) => applicable
            .into_iter()
            .filter(|record| record.toolchain == *toolchain)
            .collect(),
        Err(_) => return report,
    };
    if applicable.is_empty() {
        report.reasons.push(UnavailableReason::HelperMismatch);
        return report;
    }
    // Qualification applies only to the bound inputs. A changed/unreadable installation cannot
    // retain a current qualified claim, even if its old immutable snapshot matches an acceptance.
    if report.reasons.iter().any(|reason| {
        matches!(
            reason,
            UnavailableReason::TargetChanged
                | UnavailableReason::ContentChanged
                | UnavailableReason::InputUnavailable
        )
    }) {
        return report;
    }
    report.accepted_bounds = applicable
        .iter()
        .map(|record| record.bounds.clone())
        .collect();
    // One accepted record must cover the whole request; combining partial records could invent
    // a composition or observation window that was never qualified.
    let qualified: Vec<_> = applicable
        .into_iter()
        .filter(|record| covers(&record.bounds, request))
        .collect();
    if qualified.is_empty() {
        report.qualification = Qualification::OutsideSupport;
        report.reasons.push(UnavailableReason::OutsideBounds);
        return report;
    }
    report.qualification = Qualification::Qualified;
    for record in qualified {
        report.qualification_records.push(record.id.clone());
        report.evidence.extend(record.evidence.clone());
    }
    if report.reasons.is_empty() {
        report.availability = Availability::Available;
    }
    report
}

fn covers(bounds: &RegistryBounds, request: &CapabilityRequest) -> bool {
    bounds.registries.contains(&request.registry)
}
