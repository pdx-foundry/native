use std::path::PathBuf;

use evidence::{ArtifactReference, ReplayError, ReplayResult, store::ArtifactStore};
use serde::{Deserialize, Serialize};

/// Location and immutable identity of one retained bounded attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayRequest {
    /// Relocatable directory containing the descriptor and its archive-relative artifacts.
    pub artifact_root: PathBuf,
    /// Expected descriptor bytes, including its SHA-256 identity.
    pub descriptor: ArtifactReference,
}

/// Consumer interface for engine observations; this revision supports retained replay only.
#[derive(Debug, Default)]
pub struct Engine;

impl Engine {
    /// Verify and derive one historical attempt without installation discovery or live execution.
    /// Incomplete attempts return their retained observations and gaps. Inaccessible or incompatible
    /// evidence returns an error rather than an empty successful result.
    pub fn replay(&self, request: ReplayRequest) -> Result<ReplayResult, ReplayError> {
        evidence::replay::replay(
            &ArtifactStore::new(request.artifact_root),
            &request.descriptor,
        )
    }
}
