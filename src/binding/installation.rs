use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use super::{binary::hash, targets::M45_DEFAULT_REGISTRIES};
use crate::{OpenError, UnavailableReason};

/// SHA-256 of each content file, by its path relative to the installation root.
pub(crate) type ContentIdentity = BTreeMap<String, String>;

#[derive(Clone)]
pub(super) struct Installation {
    locator: PathBuf,
    executable: PathBuf,
    root: PathBuf,
    executable_hash: String,
    /// The bytes that `executable_hash` was taken from.
    executable_bytes: Arc<[u8]>,
    /// The executable file's metadata when `executable_bytes` were read.
    executable_stamp: Stamp,
    default_directories: Vec<String>,
    pub content: Result<ContentIdentity, UnavailableReason>,
}

impl Installation {
    pub fn open(hint: &Path) -> Result<(Self, Arc<[u8]>), OpenError> {
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
        let (executable_stamp, bytes) =
            read_stamped(&executable).map_err(|error| access_error(&executable, error))?;
        let bytes: Arc<[u8]> = bytes.into();
        let default_directories: Vec<String> = M45_DEFAULT_REGISTRIES
            .iter()
            .map(|name| (*name).into())
            .collect();
        Ok((
            Self {
                executable_hash: hash(&bytes),
                executable_bytes: bytes.clone(),
                executable_stamp,
                content: content_snapshot(&root, &default_directories, false),
                default_directories,
                root,
                locator,
                executable,
            },
            bytes,
        ))
    }

    /// The executable bytes verified at `open`. The file is read and hashed again only when its
    /// metadata stamp differs from the one taken at `open`; performance.md states the limit.
    pub fn executable_bytes(&self) -> Result<Arc<[u8]>, UnavailableReason> {
        match fs::canonicalize(&self.locator) {
            Ok(current) if current != self.executable => {
                return Err(UnavailableReason::TargetChanged);
            }
            Err(_) => return Err(UnavailableReason::InputUnavailable),
            _ => {}
        }
        let Ok(metadata) = fs::metadata(&self.executable) else {
            return Err(UnavailableReason::InputUnavailable);
        };
        if Stamp::of(&metadata) != self.executable_stamp {
            let Ok(bytes) = fs::read(&self.executable) else {
                return Err(UnavailableReason::InputUnavailable);
            };
            if hash(&bytes) != self.executable_hash {
                return Err(UnavailableReason::TargetChanged);
            }
        }
        Ok(self.executable_bytes.clone())
    }

    pub fn target_integrity(&self) -> Option<UnavailableReason> {
        self.executable_bytes().err()
    }

    pub fn default_content_integrity(&self) -> Option<UnavailableReason> {
        match (
            &self.content,
            content_snapshot(&self.root, &self.default_directories, false),
        ) {
            (Ok(expected), Ok(current)) if *expected == current => None,
            (Ok(_), Ok(_)) => Some(UnavailableReason::ContentChanged),
            _ => Some(UnavailableReason::InputUnavailable),
        }
    }

    pub(super) fn session_content(
        &self,
        directories: &[String],
    ) -> Result<ContentIdentity, UnavailableReason> {
        content_snapshot(&self.root, directories, true)
    }

    pub(super) fn session_content_unchanged(
        &self,
        directories: &[String],
        expected: &ContentIdentity,
    ) -> bool {
        content_snapshot(&self.root, directories, true).is_ok_and(|current| current == *expected)
    }
}

/// File metadata that a write or a replacement changes. On Unix the status-change time changes
/// on each write, rename or permission change, and a user cannot set it. Windows has only the
/// length and the modification time.
#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    length: u64,
    modified: Option<SystemTime>,
    /// Device, inode, and status-change seconds and nanoseconds.
    #[cfg(unix)]
    unix: (u64, u64, i64, i64),
}

impl Stamp {
    fn of(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            unix: (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        }
    }
}

/// The stamp is taken from the open file before the read, so a write or a replacement during
/// the read leaves a stamp that the next check sees as changed.
fn read_stamped(path: &Path) -> std::io::Result<(Stamp, Vec<u8>)> {
    let mut file = fs::File::open(path)?;
    let stamp = Stamp::of(&file.metadata()?);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok((stamp, bytes))
}

fn access_error(path: &Path, error: std::io::Error) -> OpenError {
    if error.kind() == std::io::ErrorKind::NotFound {
        OpenError::Missing(path.into())
    } else {
        OpenError::Unreadable(path.into())
    }
}

pub(super) fn resolve_directory(hint: &Path) -> Result<PathBuf, OpenError> {
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

/// The directory that holds the game content: the one that holds the `.app` bundle on macOS,
/// otherwise the executable's own directory.
fn installation_root(executable: &Path) -> PathBuf {
    let parent = executable
        .parent()
        .expect("canonical executable has a parent");
    match app_bundle(parent) {
        Some(bundle) => bundle
            .parent()
            .expect("canonical application bundle has a parent")
            .into(),
        None => parent.into(),
    }
}

/// The `.app` bundle whose `Contents/MacOS` directory holds the executable.
fn app_bundle(executable_directory: &Path) -> Option<&Path> {
    if !executable_directory.ends_with("Contents/MacOS") {
        return None;
    }
    let bundle = executable_directory.parent()?.parent()?;

    bundle
        .extension()
        .is_some_and(|extension| extension == "app")
        .then_some(bundle)
}

fn content_snapshot(
    root: &Path,
    directories: &[String],
    allow_missing: bool,
) -> Result<ContentIdentity, UnavailableReason> {
    let common = fs::symlink_metadata(root.join("common"))
        .map_err(|_| UnavailableReason::InputUnavailable)?;
    if !common.is_dir() || common.is_symlink() {
        return Err(UnavailableReason::InputUnavailable);
    }
    let mut files = vec![root.join("launcher-settings.json")];
    for directory in directories {
        let mut path = root.to_path_buf();
        let mut missing = false;
        for segment in directory.split('/') {
            path.push(segment);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_symlink() || !metadata.is_dir() => {
                    return Err(UnavailableReason::InputUnavailable);
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && allow_missing => {
                    missing = true;
                    break;
                }
                Err(_) => return Err(UnavailableReason::InputUnavailable),
            }
        }
        if !missing {
            collect_content(&path, &mut files)?;
        }
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
