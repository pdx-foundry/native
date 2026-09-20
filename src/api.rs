//! Why an installation cannot be opened, and why an opened installation cannot answer now.
use std::path::PathBuf;

/// Why an installation could not be bound. No process has been created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// The hint or a recognized executable is absent.
    Missing(PathBuf),
    /// A required path cannot be read or resolved.
    Unreadable(PathBuf),
    /// The executable image is malformed or truncated.
    MalformedExecutable,
    /// More than one executable, eligible slice, or exact catalogue entry matched.
    Ambiguous,
    /// The format or architecture is outside Native's declared target scope.
    UnsupportedTarget,
    /// The image has a supported shape, but no exact registered identity.
    UnknownTarget,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "installation identification failed: {self:?}")
    }
}

impl std::error::Error for OpenError {}

/// One reason why an opened installation cannot answer a question now. Several independent
/// reasons can hold together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnavailableReason {
    /// The compiled host cannot use the build's live strategy.
    #[cfg_attr(all(target_os = "macos", target_arch = "aarch64"), allow(dead_code))]
    HostUnavailable,
    /// The executable changed after `open`; open the installation again.
    TargetChanged,
    /// Installed content that a session pins changed after `open`.
    ContentChanged,
    /// A present input cannot be read.
    InputUnavailable,
    /// The host's debugger tools cannot be found or started.
    PrerequisiteMissing,
}
