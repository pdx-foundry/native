use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use super::binary::hash;
use crate::{OpenError, UnavailableReason};

/// SHA-256 of each content file, by its path relative to the installation root.
pub(crate) type ContentIdentity = BTreeMap<String, String>;

#[derive(Debug, Clone)]
pub(super) struct Installation {
    locator: PathBuf,
    executable: PathBuf,
    root: PathBuf,
    executable_hash: String,
    pub content: Result<ContentIdentity, UnavailableReason>,
}

impl Installation {
    pub fn open(hint: &Path) -> Result<(Self, Vec<u8>), OpenError> {
        let metadata = fs::metadata(hint).map_err(|error| access_error(hint, error))?;
        let executable = if metadata.is_file() {
            hint.to_owned()
        } else if metadata.is_dir() {
            resolve_directory(hint)?
        } else {
            return Err(OpenError::Unreadable(hint.into()));
        };
        let locator =
            std::path::absolute(&executable).map_err(|error| access_error(&executable, error))?;
        let executable =
            fs::canonicalize(&executable).map_err(|error| access_error(&executable, error))?;
        let root = installation_root(&executable);
        let bytes = fs::read(&executable).map_err(|error| access_error(&executable, error))?;
        Ok((
            Self {
                executable_hash: hash(&bytes),
                content: content_snapshot(&root),
                root,
                locator,
                executable,
            },
            bytes,
        ))
    }

    pub fn executable_bytes(&self) -> Result<Vec<u8>, UnavailableReason> {
        match fs::canonicalize(&self.locator) {
            Ok(current) if current != self.executable => {
                return Err(UnavailableReason::TargetChanged);
            }
            Err(_) => return Err(UnavailableReason::InputUnavailable),
            _ => {}
        }
        let Ok(bytes) = fs::read(&self.executable) else {
            return Err(UnavailableReason::InputUnavailable);
        };
        if hash(&bytes) != self.executable_hash {
            return Err(UnavailableReason::TargetChanged);
        }
        Ok(bytes)
    }

    pub fn integrity(&self) -> Option<UnavailableReason> {
        if let Err(reason) = self.executable_bytes() {
            return Some(reason);
        }
        match (&self.content, content_snapshot(&self.root)) {
            (Ok(expected), Ok(current)) if *expected == current => None,
            (Ok(_), Ok(_)) => Some(UnavailableReason::ContentChanged),
            _ => Some(UnavailableReason::InputUnavailable),
        }
    }
}

fn access_error(path: &Path, error: std::io::Error) -> OpenError {
    if error.kind() == std::io::ErrorKind::NotFound {
        OpenError::Missing(path.into())
    } else {
        OpenError::Unreadable(path.into())
    }
}

fn resolve_directory(hint: &Path) -> Result<PathBuf, OpenError> {
    let mut candidates = Vec::new();
    for relative in [
        "stellaris.app/Contents/MacOS/stellaris",
        "Contents/MacOS/stellaris",
        "stellaris.exe",
        "stellaris",
    ] {
        let path = hint.join(relative);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => candidates.push(path),
            Ok(_) => return Err(OpenError::Unreadable(path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(access_error(&path, error)),
        }
    }
    match candidates.len() {
        0 => Err(OpenError::Missing(hint.into())),
        1 => Ok(candidates.remove(0)),
        _ => Err(OpenError::Ambiguous),
    }
}

fn installation_root(executable: &Path) -> PathBuf {
    let parent = executable
        .parent()
        .expect("canonical executable has a parent");
    if parent.ends_with("Contents/MacOS")
        && parent
            .ancestors()
            .nth(2)
            .and_then(Path::extension)
            .is_some_and(|extension| extension == "app")
    {
        parent
            .ancestors()
            .nth(3)
            .expect("application path has three ancestors")
            .into()
    } else {
        parent.into()
    }
}

fn content_snapshot(root: &Path) -> Result<ContentIdentity, UnavailableReason> {
    let common = fs::symlink_metadata(root.join("common"))
        .map_err(|_| UnavailableReason::InputUnavailable)?;
    if !common.is_dir() || common.is_symlink() {
        return Err(UnavailableReason::InputUnavailable);
    }
    let mut files = vec![root.join("launcher-settings.json")];
    for directory in ["common/tradition_categories", "common/traditions"] {
        collect_content(&root.join(directory), &mut files)?;
    }
    let mut snapshot = BTreeMap::new();
    for path in files {
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| UnavailableReason::InputUnavailable)?;
        if !metadata.is_file() || metadata.is_symlink() {
            return Err(UnavailableReason::InputUnavailable);
        }
        let bytes = fs::read(&path).map_err(|_| UnavailableReason::InputUnavailable)?;
        let relative = path
            .strip_prefix(root)
            .expect("content is below installation root");
        let components = relative
            .components()
            .map(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .ok_or(UnavailableReason::InputUnavailable)
            })
            .collect::<Result<Vec<_>, _>>()?;
        snapshot.insert(components.join("/"), hash(&bytes));
    }
    Ok(snapshot)
}

fn collect_content(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), UnavailableReason> {
    let metadata =
        fs::symlink_metadata(directory).map_err(|_| UnavailableReason::InputUnavailable)?;
    if !metadata.is_dir() || metadata.is_symlink() {
        return Err(UnavailableReason::InputUnavailable);
    }
    for entry in fs::read_dir(directory).map_err(|_| UnavailableReason::InputUnavailable)? {
        let entry = entry.map_err(|_| UnavailableReason::InputUnavailable)?;
        let kind = entry
            .file_type()
            .map_err(|_| UnavailableReason::InputUnavailable)?;
        if kind.is_symlink() {
            return Err(UnavailableReason::InputUnavailable);
        }
        if kind.is_dir() {
            collect_content(&entry.path(), files)?;
        } else if kind.is_file() {
            files.push(entry.path());
        } else {
            return Err(UnavailableReason::InputUnavailable);
        }
    }
    Ok(())
}

impl Installation {
    /// SHA-256 of the executable file as it was at `open`.
    pub(super) fn executable_hash(&self) -> &str {
        &self.executable_hash
    }
    pub(super) fn locator(&self) -> &Path {
        &self.locator
    }
    pub(super) fn executable(&self) -> &Path {
        &self.executable
    }
    pub(super) fn root(&self) -> &Path {
        &self.root
    }
}
