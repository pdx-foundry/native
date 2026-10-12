//! Host-wide launch exclusion for one Native-owned game at a time.
//!
//! The OS lock excludes concurrent Native supervisors. A process inventory also refuses an
//! ordinary Stellaris instance. The process identities of a session are recorded in the work
//! directory; old reports do not control admission to a later session.
use crate::{
    binding::{self, HostReservation, ProcessIdentity},
    supervisor::SupervisorError,
};
use serde::Serialize;

#[derive(Serialize)]
struct OwnerReport {
    owner: ProcessIdentity,
    game: Option<ProcessIdentity>,
}

pub(super) struct Reservation {
    /// Hold the host-wide launch lock until this reservation is dropped.
    #[expect(dead_code, reason = "the lock is held for its Drop")]
    lock: HostReservation,
    report: OwnerReport,
}
impl Reservation {
    pub fn acquire() -> Result<Self, SupervisorError> {
        Self::reserve(binding::acquire_reservation()?)
    }
    pub(super) fn reserve(host: HostReservation) -> Result<Self, SupervisorError> {
        if binding::conflicting_game(None)? {
            return Err(SupervisorError("Another Stellaris game is running".into()));
        }
        Ok(Self {
            lock: host,
            report: OwnerReport {
                owner: binding::process_identity(std::process::id())?,
                game: None,
            },
        })
    }
    pub fn record_game(&mut self, identity: ProcessIdentity) {
        self.report.game = Some(identity);
    }
    pub fn snapshot(&self) -> Result<serde_json::Value, SupervisorError> {
        Ok(serde_json::to_value(&self.report)?)
    }
}
