//! Host-wide launch exclusion for one Native-owned game at a time.
//!
//! The OS lock excludes concurrent Native supervisors. A process inventory also refuses an
//! ordinary Stellaris instance. Session ownership and disposal are reported in the work
//! directory; old reports do not control admission to a later session.
use crate::{
    binding::{self, HostReservation, ProcessIdentity},
    supervisor::SupervisorError,
};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Reserved,
    Disposed,
}

#[derive(Serialize)]
struct OwnerReport {
    attempt: String,
    owner: ProcessIdentity,
    game: Option<ProcessIdentity>,
    output: PathBuf,
    state: State,
}

pub(super) struct Reservation {
    /// Hold the host-wide launch lock until this reservation is dropped.
    #[expect(dead_code, reason = "the lock is held for its Drop")]
    lock: HostReservation,
    report: OwnerReport,
}
impl Reservation {
    pub fn acquire(attempt: String, output: PathBuf) -> Result<Self, SupervisorError> {
        let host = binding::acquire_reservation()?;
        Self::reserve(host, attempt, output)
    }
    pub(super) fn reserve(
        host: HostReservation,
        attempt: String,
        output: PathBuf,
    ) -> Result<Self, SupervisorError> {
        if binding::conflicting_game(None)? {
            return Err(SupervisorError("Another Stellaris game is running".into()));
        }
        Ok(Self {
            lock: host,
            report: OwnerReport {
                attempt,
                owner: binding::process_identity(std::process::id())?,
                game: None,
                output,
                state: State::Reserved,
            },
        })
    }
    pub fn record_game(&mut self, identity: ProcessIdentity) {
        self.report.game = Some(identity);
    }
    pub fn snapshot(&self) -> Result<serde_json::Value, SupervisorError> {
        Ok(serde_json::to_value(&self.report)?)
    }
    pub fn disposed(&mut self) {
        self.report.state = State::Disposed;
    }
}
