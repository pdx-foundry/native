//! Supported bounded observation plans and results.
use crate::{operation, supervisor::SupervisorError};
use serde::Serialize;
use std::{
    io::{Read, Write},
    path::PathBuf,
};

/// Request the initial three registration entries and the retained category's two field reads.
/// Admission is restricted to the exact retained fixture; this is not a complete registry or validation.
#[derive(Debug)]
pub struct ObservationRequest {
    /// Category fixture bytes containing `tree_template` and an empty `traditions` list.
    pub fixture: String,
    /// Observation budget, in seconds, from 1 through 180; disposal has a separate budget.
    pub deadline_seconds: u64,
}

/// Retention location for a new live attempt.
#[derive(Debug)]
pub struct CaptureOptions {
    /// New absolute directory beneath an existing parent; existing captures are never replaced.
    pub output: PathBuf,
}

/// Pinned request produced by `EngineContext::prepare_observation`; not a transferable permit.
/// The independent supervisor repeats ordinary admission before allocating the game.
pub struct ObservationPlan(pub(crate) operation::PreparedPlan);

/// An admitted controller connection; dropping it requests cleanup but cannot confirm disposal.
pub struct ObservationJob<R, W>(pub(crate) operation::AttemptJob<R, W>);

/// Live operation termination, evidence, and independent resource disposal.
#[derive(Debug, Serialize)]
pub struct ObservationReport {
    /// Fixed target composition identity; consumers retain it without parsing it.
    pub context: crate::ContextIdentity,
    /// Unique attempt identity, also recorded with retained evidence.
    pub attempt: String,
    /// Why execution ended; completion of observations is reported separately in `evidence`.
    pub outcome: crate::OperationOutcome,
    /// Independent owner result, including attempts which never launched a game.
    pub disposal: crate::OperationDisposal,
    /// Whether the durable host reservation was resolved after worker and game cleanup.
    pub reservation_resolved: bool,
    /// Normalized observations, activation and completion, or an explicit evidence failure.
    /// A retention failure does not erase the independent disposal result above.
    pub evidence: Result<crate::ObservationResult, String>,
    /// Relocatable retained attempt for `Engine::replay`, when finalization succeeded.
    pub replay: Option<crate::ReplayRequest>,
    /// Failures retaining evidence or reporting resource cleanup.
    pub diagnostics: Vec<String>,
}

impl<R: Read, W: Write> ObservationJob<R, W> {
    /// Await game ownership. `false` means startup ended; `finish` still provides its report.
    pub fn started(&mut self) -> Result<bool, SupervisorError> {
        self.0.started().map(|started| started.is_some())
    }
    /// Request bounded cleanup. Call `finish` to receive independent disposal confirmation.
    pub fn cancel(&mut self) -> Result<(), SupervisorError> {
        self.0.cancel()
    }
    /// Await the owner result and validate its retained observations using the replay contract.
    /// A lost supervisor channel is an error, never disposal confirmation.
    pub fn finish(self) -> Result<ObservationReport, SupervisorError> {
        let report = self.0.finish()?;
        if report.origin != operation::Authorization::Admitted.origin() {
            return Err(SupervisorError("Expected admitted live report".into()));
        }
        let replay = report.replay.map(|descriptor| crate::ReplayRequest {
            artifact_root: report.output.join("evidence"),
            descriptor,
        });
        let evidence = replay
            .as_ref()
            .ok_or_else(|| "Observation evidence was not finalized".to_string())
            .and_then(|request| {
                crate::Engine
                    .replay(request.clone())
                    .map_err(|error| error.to_string())
            })
            .map(|mut result| {
                result.origin = crate::ResultOrigin::Live;
                result
            });
        Ok(ObservationReport {
            context: crate::ContextIdentity(report.composition),
            attempt: report.attempt,
            outcome: report.outcome,
            disposal: report.disposal,
            reservation_resolved: report.reservation_resolved,
            evidence,
            replay,
            diagnostics: report.diagnostics,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report(root: PathBuf) -> operation::AttemptReport {
        operation::AttemptReport {
            origin: operation::Authorization::Admitted.origin().into(),
            attempt: "unit".into(),
            composition: "opaque".into(),
            outcome: crate::OperationOutcome::Cancelled,
            disposal: crate::OperationDisposal::Reaped,
            reservation_resolved: true,
            output: root,
            diagnostics: vec!["retention failed".into()],
            replay: None,
        }
    }
    #[test]
    fn evidence_failure_keeps_owner_disposal_and_termination_cause() {
        let temp = tempfile::tempdir().unwrap();
        let result = ObservationJob(operation::reported_job(report(temp.path().into())))
            .finish()
            .unwrap();
        assert_eq!(result.disposal, crate::OperationDisposal::Reaped);
        assert_eq!(result.outcome, crate::OperationOutcome::Cancelled);
        assert!(result.reservation_resolved && result.evidence.is_err());
        assert_eq!(result.diagnostics, ["retention failed"]);
    }
    #[test]
    fn candidate_reports_cannot_be_returned_as_qualified_live_results() {
        let mut report = report(std::env::temp_dir());
        report.origin = "unqualified-candidate".into();
        assert!(
            ObservationJob(operation::reported_job(report))
                .finish()
                .is_err()
        );
    }
}
