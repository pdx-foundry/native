use crate::supervisor::SupervisorError;
use std::{
    fs::File,
    path::{Path, PathBuf},
    time::Duration,
};
fn unavailable<T>() -> Result<T, SupervisorError> {
    Err(SupervisorError(
        "HostUnavailable: candidate lifecycle requires Apple Silicon macOS".into(),
    ))
}
pub(crate) fn available() -> Result<(), SupervisorError> {
    unavailable()
}
use crate::binding::ProcessIdentity;
pub(crate) struct HostReservation {
    pub root: PathBuf,
}
pub(crate) fn acquire_reservation() -> Result<HostReservation, SupervisorError> {
    unavailable()
}
pub(crate) fn prepare_owner(_: u32) -> Result<(), SupervisorError> {
    unavailable()
}
pub(crate) fn process_identity(_: u32) -> Result<ProcessIdentity, SupervisorError> {
    unavailable()
}
pub(crate) fn conflicting_game(_: Option<u32>) -> Result<bool, SupervisorError> {
    unavailable()
}
pub(crate) fn open_record(_: &Path) -> Result<File, SupervisorError> {
    unavailable()
}
pub(crate) fn private_directory(_: &Path) -> Result<(), SupervisorError> {
    unavailable()
}
pub(crate) struct OwnedGame;
impl OwnedGame {
    pub fn exit_status(&self) -> Option<i64> {
        None
    }
    pub fn pid(&self) -> u32 {
        unreachable!("unavailable hosts never construct a game")
    }
    pub fn identity(&self) -> Result<ProcessIdentity, SupervisorError> {
        unavailable()
    }
    pub fn suspended(&self) -> Result<bool, SupervisorError> {
        unavailable()
    }
    pub fn dispose(&mut self, _: Duration) -> Result<(), SupervisorError> {
        unavailable()
    }
}
pub(crate) fn spawn(_: &Path, _: &Path, _: &Path) -> Result<OwnedGame, SupervisorError> {
    unavailable()
}

pub(crate) fn spawn_guarded(
    _: &Path,
    _: &Path,
    _: &Path,
    _: Option<&Path>,
) -> Result<OwnedGame, SupervisorError> {
    unavailable()
}
