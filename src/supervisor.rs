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

/// Serve the ordinary protocol in a consumer-created supervisor process.
///
/// No live operation is qualified yet. All requests are rejected before allocating resources;
/// maintainer requests must use the separately gated investigation entry point.
pub fn serve(input: impl Read, mut output: impl Write) -> Result<(), SupervisorError> {
    let hello: crate::protocol::Hello = crate::protocol::read(input)?;
    hello.validate()?;
    crate::protocol::write(
        &mut output,
        &crate::protocol::Reply::Rejected(
            "No admitted live operation; candidate requests require the maintainer entry point"
                .into(),
        ),
    )
}
