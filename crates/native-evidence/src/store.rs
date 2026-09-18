//! Concrete read-only artifact access. No callbacks or live execution services are accepted.

use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::{ArtifactReference, ReplayError};

/// Read-only portable artifact store; creation performs no I/O.
pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    /// Bind a relocatable storage directory. Stored paths never select an executable.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Read and verify exact bytes. Symbolic links and non-regular files are rejected.
    pub fn read(&self, reference: &ArtifactReference) -> Result<Vec<u8>, ReplayError> {
        validate_reference(reference)?;
        let mut location = self.root.clone();
        for component in Path::new(&reference.path).components() {
            location.push(component);
            let metadata = fs::symlink_metadata(&location)
                .map_err(|error| read_error(&reference.path, error))?;
            if metadata.file_type().is_symlink() {
                return Err(ReplayError::UnsafePath {
                    path: reference.path.clone(),
                });
            }
        }
        let metadata =
            fs::metadata(&location).map_err(|error| read_error(&reference.path, error))?;
        if !metadata.is_file() {
            return Err(ReplayError::UnsafePath {
                path: reference.path.clone(),
            });
        }
        if metadata.len() != reference.bytes {
            return Err(ReplayError::SizeMismatch {
                path: reference.path.clone(),
                expected: reference.bytes,
                found: metadata.len(),
            });
        }
        let file = fs::File::open(&location).map_err(|error| read_error(&reference.path, error))?;
        let mut bytes = Vec::new();
        file.take(reference.bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| read_error(&reference.path, error))?;
        if bytes.len() as u64 != reference.bytes {
            return Err(ReplayError::SizeMismatch {
                path: reference.path.clone(),
                expected: reference.bytes,
                found: bytes.len() as u64,
            });
        }
        if sha256(&bytes) != reference.sha256 {
            return Err(ReplayError::HashMismatch {
                path: reference.path.clone(),
            });
        }
        Ok(bytes)
    }
}

pub(crate) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_reference(reference: &ArtifactReference) -> Result<(), ReplayError> {
    let path = Path::new(&reference.path);
    if reference.path.is_empty()
        || reference.path.contains(['\\', ':'])
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        || reference
            .path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ReplayError::UnsafePath {
            path: reference.path.clone(),
        });
    }
    if !is_sha256(&reference.sha256) {
        return Err(ReplayError::Malformed {
            path: reference.path.clone(),
            reason: "expected lowercase SHA-256".into(),
        });
    }
    if reference.bytes > 64 * 1024 * 1024 {
        return Err(ReplayError::Malformed {
            path: reference.path.clone(),
            reason: "artifact exceeds the 64 MiB bounded replay limit".into(),
        });
    }
    Ok(())
}

fn read_error(path: &str, error: std::io::Error) -> ReplayError {
    if error.kind() == std::io::ErrorKind::NotFound {
        return ReplayError::EvidenceUnavailable { path: path.into() };
    }
    ReplayError::Read {
        path: path.into(),
        reason: error.to_string(),
    }
}
