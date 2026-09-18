//! Maintainer-only unqualified investigations. Candidate reports never grant live admission.
//! Use a dedicated consumer-hosted supervisor; see the `observe` and `investigate` examples.
pub use crate::operation::{
    AttemptCapture as CandidateCapture, AttemptJob as CandidateJob,
    AttemptReport as InvestigationReport, AttemptRequest as CandidateRequest, ObservationControl,
    ObservationRequest, OperationDisposal as CandidateDisposal,
    OperationOutcome as CandidateOutcome, PreparedPlan as CandidatePlan, connect, prepare,
    prepare_observation, prepare_observation_control, serve,
};
