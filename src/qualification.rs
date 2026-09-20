mod admission;

use crate::{RegistryBounds, UnavailableReason};
use std::collections::BTreeMap;

pub(crate) use admission::evaluate;
pub(crate) type ContentIdentity = BTreeMap<String, String>;

#[derive(Debug, Clone)]
pub(crate) struct AdmissionInputs {
    pub composition: String,
    pub bounds: RegistryBounds,
    pub content: Result<ContentIdentity, UnavailableReason>,
    pub prerequisites: Vec<UnavailableReason>,
    pub toolchain: Result<String, UnavailableReason>,
}

pub(crate) mod analysis;
