use crate::{capture::Capture, investigation::ObservationSpec, supervisor::SupervisorError};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
fn unavailable<T>() -> Result<T, SupervisorError> {
    Err(SupervisorError(
        "HostUnavailable: early observation requires Apple Silicon macOS".into(),
    ))
}
pub(crate) fn probe_observer() -> Result<(), SupervisorError> {
    unavailable()
}
pub(crate) struct Observer {
    pub(crate) exited: Option<i64>,
}
impl Observer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(
        _: &Path,
        _: &str,
        _: &ObservationSpec,
        _: &Path,
        _: serde_json::Value,
        _: &BTreeMap<String, String>,
        _: BTreeMap<String, u64>,
    ) -> Result<(Self, Capture), SupervisorError> {
        unavailable()
    }
    pub(crate) fn guard(&self) -> PathBuf {
        unreachable!("unavailable observer")
    }
    pub(crate) fn start(&mut self, _: u32) -> Result<(), SupervisorError> {
        unavailable()
    }
    pub(crate) fn poll(&mut self) -> Result<bool, SupervisorError> {
        unavailable()
    }
    pub(crate) fn stop(&mut self) -> Result<(), SupervisorError> {
        unavailable()
    }
}
