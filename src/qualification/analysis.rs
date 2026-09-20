use crate::{
    ArtifactReference, Availability, CapabilityBounds, CapabilityReport, ContextIdentity,
    ContextOrigin, Qualification, UnavailableReason,
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

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnalysisRecord {
    pub id: String,
    pub composition: String,
    pub evidence: Vec<ArtifactReference>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnalysisAuthority {
    pub accepted: Vec<AnalysisRecord>,
    pub withdrawn: Vec<String>,
}

impl AnalysisAuthority {
    pub(crate) fn bundled() -> Self {
        serde_json::from_str(include_str!("records/analysis.json"))
            .expect("tracked static qualification records")
    }
}

pub(crate) fn evaluate(
    inputs: &AnalysisInputs,
    authority: &AnalysisAuthority,
    origin: ContextOrigin,
    integrity: Option<UnavailableReason>,
) -> CapabilityReport {
    let mut report = unavailable(ContextIdentity(inputs.composition.clone()), origin);
    if inputs.method == evidence::discovery::METHOD {
        report.bounds = CapabilityBounds::RegistryDiscovery;
    }
    if inputs.method == evidence::fields::METHOD {
        report.bounds = CapabilityBounds::RegistryFields;
    }
    report.reasons.clear();
    if let Some(reason) = integrity {
        report.reasons.push(reason);
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
    for record in matching {
        if !authority.withdrawn.contains(&record.id) {
            report.qualification_records.push(record.id.clone());
            report.evidence.extend(record.evidence.clone());
        }
    }
    if report.qualification_records.is_empty() {
        report
            .reasons
            .push(UnavailableReason::QualificationWithdrawn);
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
