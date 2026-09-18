mod admission;
mod records;

use crate::{ArtifactReference, ObservationBounds, UnavailableReason};
use std::collections::BTreeMap;

pub(crate) use admission::evaluate;
pub(crate) type ContentIdentity = BTreeMap<String, String>;

#[derive(Debug)]
pub(crate) struct AdmissionInputs {
    pub composition: String,
    pub bounds: ObservationBounds,
    pub content: Result<ContentIdentity, UnavailableReason>,
    pub prerequisites: Vec<UnavailableReason>,
}

#[derive(Debug, Clone)]
pub(crate) struct AcceptedRecord {
    pub id: String,
    pub composition: String,
    pub bounds: ObservationBounds,
    pub content: ContentIdentity,
    pub evidence: Vec<ArtifactReference>,
}

#[derive(Debug)]
pub(crate) struct Authority {
    pub accepted: Vec<AcceptedRecord>,
    pub withdrawn: Vec<String>,
}

impl Authority {
    pub(crate) fn bundled() -> Self {
        Self {
            accepted: records::ACCEPTED.to_vec(),
            withdrawn: records::WITHDRAWN.iter().map(|id| (*id).into()).collect(),
        }
    }
}
