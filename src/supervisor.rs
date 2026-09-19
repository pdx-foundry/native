//! Consumer-hosted supervision. The consumer supplies a dedicated process and its channels.
//!
//! Keep the control input open for the lifetime of a job. EOF requests disposal; it is not
//! evidence of disposal. Only an owner report confirms disposal. Do not share stdout with logs.
use std::io::{Read, Write};

/// A connection, admission, isolation, or storage prerequisite failed.
#[derive(Debug)]
pub struct SupervisorError(pub(crate) String);

impl std::fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for SupervisorError {}
impl From<std::io::Error> for SupervisorError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}
impl From<serde_json::Error> for SupervisorError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

/// Run ordinary Native supervision in a dedicated direct child of the controller.
/// Both processes must link the same Native build. Use private pipes, keep control input open,
/// and send logs to stderr. Do not pre-create a process group or install another child reaper.
/// Let this function finish cleanup before exiting the supervisor process; input EOF cancels.
pub fn serve(
    input: impl Read + Send + 'static,
    output: impl Write + Send + 'static,
) -> Result<(), SupervisorError> {
    crate::execution::supervisor::serve(input, output, crate::operation::Authorization::Admitted)
}
