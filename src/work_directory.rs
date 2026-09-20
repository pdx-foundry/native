//! File rules for a session's work directory.
//!
//! The supervisor and the debugger worker exchange small files in one private directory. Three
//! rules keep a reader from taking a half-written or foreign file as a message:
//!
//! - A file is created once and never replaced ([`write_new`]).
//! - A control message appears under its final name only when it is complete ([`publish_json`]).
//! - A reader accepts only a regular file of bounded size ([`read_bounded`]).
use crate::supervisor::SupervisorError;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

/// Lowercase SHA-256 of the bytes.
pub(crate) fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

/// Create a file and make it durable. An existing file is an error and stays unchanged.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), SupervisorError> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}

/// [`write_new`] with a JSON value.
pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), SupervisorError> {
    write_new(path, &serde_json::to_vec_pretty(value)?)
}

/// Publish a complete control message. A reader never sees partial bytes under the final name,
/// and an earlier message with that name is never replaced.
#[cfg_attr(
    not(all(target_os = "macos", target_arch = "aarch64")),
    allow(dead_code)
)]
pub(crate) fn publish_json(path: &Path, value: &impl Serialize) -> Result<(), SupervisorError> {
    let pending = path.with_extension("pending");
    write_json(&pending, value)?;
    fs::hard_link(&pending, path)?;
    fs::remove_file(&pending)?;
    #[cfg(unix)]
    File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}

/// Read a regular file of at most `limit` bytes. A symbolic link, a directory, a special file
/// and a larger file are errors.
pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, SupervisorError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.is_symlink() || metadata.len() > limit as u64 {
        return Err(SupervisorError(format!(
            "Unsafe or oversized file: {}",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(SupervisorError("File exceeded its read bound".into()));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_written_once_and_a_read_is_bounded() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("file");
        write_new(&path, b"original").unwrap();
        assert!(write_new(&path, b"replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert!(read_bounded(&path, 2).is_err());
        assert!(read_bounded(root.path(), 1024).is_err());
    }

    #[test]
    fn a_published_message_is_complete_and_never_replaced() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("grant.json");
        publish_json(&path, &serde_json::json!({"attempt":"first"})).unwrap();
        assert!(!path.with_extension("pending").exists());
        assert!(publish_json(&path, &serde_json::json!({"attempt":"second"})).is_err());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap()["attempt"],
            "first"
        );
    }
}
