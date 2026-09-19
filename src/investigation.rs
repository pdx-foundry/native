//! Maintainer-only unqualified investigations. Candidate reports never grant live admission.
//! Use a dedicated consumer-hosted supervisor; see the `observe` and `investigate` examples.
pub use crate::operation::{
    AttemptCapture as CandidateCapture, AttemptJob as CandidateJob,
    AttemptReport as InvestigationReport, AttemptRequest as CandidateRequest, ObservationControl,
    ObservationRequest, OperationDisposal as CandidateDisposal,
    OperationOutcome as CandidateOutcome, PreparedPlan as CandidatePlan, connect, prepare,
    prepare_observation, prepare_observation_control, prepare_registry, serve,
};

/// Run the shared paused-session implementation with unqualified maintainer authority.
/// Results retain replay origin; this entry point never grants production admission.
#[doc(hidden)]
pub async fn start_game(
    native: &crate::Native,
    command: std::process::Command,
    options: crate::GameOptions,
    control_registry: String,
    control: ObservationControl,
) -> Result<crate::Game, crate::GameError> {
    if native.get_registry(&control_registry).is_err() {
        return Err(crate::GameError::InvalidOptions(
            "Unknown control registry".into(),
        ));
    }
    crate::game::start(
        native.detached_context(),
        crate::game::Hosting::new(command, options)?,
        crate::operation::Authorization::Candidate,
        Some((control_registry, control)),
    )
    .await
}
