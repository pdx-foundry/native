use crate::{capture::Capture, supervisor::SupervisorError};
use std::path::PathBuf;
fn unavailable<T>() -> Result<T, SupervisorError> {
    Err(SupervisorError(
        "HostUnavailable: early observation requires Apple Silicon macOS".into(),
    ))
}
pub(crate) fn probe_observer() -> Result<String, SupervisorError> {
    unavailable()
}
pub(crate) struct Observer {
    pub(crate) exited: Option<i64>,
}
impl Observer {
    pub(in crate::binding) fn prepare(
        _: crate::binding::platform::ObservationSetup<'_>,
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
