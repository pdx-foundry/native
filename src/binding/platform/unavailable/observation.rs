use crate::supervisor::SupervisorError;
use std::path::PathBuf;
fn unavailable<T>() -> Result<T, SupervisorError> {
    Err(SupervisorError(
        "HostUnavailable: live observation requires Apple Silicon macOS".into(),
    ))
}
pub(crate) fn probe_observer() -> Result<(), SupervisorError> {
    unavailable()
}
pub(crate) struct Observer {
    pub(crate) exited: Option<i64>,
}
impl Observer {
    pub(in crate::binding) fn prepare(
        _: crate::binding::platform::ObservationSetup<'_>,
    ) -> Result<Self, SupervisorError> {
        unavailable()
    }
    pub(crate) fn guard(&self) -> PathBuf {
        unreachable!("unavailable observer")
    }
    pub(crate) fn start(&mut self, _: u32) -> Result<(), SupervisorError> {
        unavailable()
    }
    pub(crate) fn advance_worker(&mut self) -> Result<bool, SupervisorError> {
        unavailable()
    }
    pub(crate) fn pause_witness(
        &mut self,
    ) -> Result<Option<crate::protocol::observation::PauseWitness>, SupervisorError> {
        unavailable()
    }
    pub(crate) fn stop(&mut self) -> Result<(), SupervisorError> {
        unavailable()
    }
    pub(crate) fn pause_witness_before(
        &mut self,
        _: std::time::Instant,
    ) -> Result<Option<crate::protocol::observation::PauseWitness>, SupervisorError> {
        unavailable()
    }
    pub(crate) fn prepare_check(
        &self,
        _: u64,
        _: &crate::ScriptCheck,
        _: Vec<crate::protocol::script_check::DurationReceiver>,
    ) -> Result<crate::protocol::script_check::CheckRequest, crate::Error> {
        Err(crate::Error::Unsupported {
            operation: crate::Operation::CheckScript,
            reason: "host unavailable".into(),
        })
    }
    pub(crate) fn start_check(
        &self,
        _: &crate::protocol::script_check::CheckRequest,
    ) -> Result<(), SupervisorError> {
        unavailable()
    }
    pub(crate) fn check_reply(
        &self,
        _: u64,
    ) -> Result<Option<crate::ScriptObservation>, SupervisorError> {
        unavailable()
    }
}
