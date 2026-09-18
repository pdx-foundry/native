mod admission;
mod records;

use crate::{ArtifactReference, ObservationBounds, UnavailableReason};
use std::collections::BTreeMap;

pub(crate) use admission::evaluate;
pub(crate) type ContentIdentity = BTreeMap<String, String>;

#[derive(Debug, Clone)]
pub(crate) struct AdmissionInputs {
    pub composition: String,
    pub bounds: ObservationBounds,
    pub content: Result<ContentIdentity, UnavailableReason>,
    pub prerequisites: Vec<UnavailableReason>,
    pub toolchain: Result<String, UnavailableReason>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AcceptedRecord {
    pub id: String,
    pub composition: String,
    pub bounds: ObservationBounds,
    pub content: ContentIdentity,
    pub evidence: Vec<ArtifactReference>,
    pub toolchain: String,
}

#[derive(Debug)]
pub(crate) struct Authority {
    pub accepted: Vec<AcceptedRecord>,
    pub withdrawn: Vec<String>,
}

impl Authority {
    pub(crate) fn bundled() -> Self {
        Self {
            accepted: records::accepted(),
            withdrawn: records::WITHDRAWN.iter().map(|id| (*id).into()).collect(),
        }
    }
}
