use super::targets::StrategyId;
use crate::UnavailableReason;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod macos;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use macos as host;

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
mod unavailable;
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
use unavailable as host;

/// The implementation of one live strategy on the compiled host.
pub(super) struct StrategyResolution {
    /// Why this host cannot run the strategy, when it cannot.
    pub unavailable: Option<UnavailableReason>,
    /// The files of the worker package, by file name.
    pub package: std::collections::BTreeMap<String, Vec<u8>>,
    /// Check that the host's debugger tools can be found and started.
    pub probe: fn() -> Result<(), crate::supervisor::SupervisorError>,
    pub prepare: fn(
        ObservationSetup<'_>,
    ) -> Result<observation::Observer, crate::supervisor::SupervisorError>,
}

#[cfg_attr(
    not(all(target_os = "macos", target_arch = "aarch64")),
    allow(dead_code)
)]
pub(in crate::binding) struct ObservationSetup<'a> {
    pub work_directory: &'a std::path::Path,
    pub attempt: &'a str,
    pub executable: &'a std::path::Path,
    pub registries:
        &'a std::collections::BTreeMap<String, crate::protocol::observation::RegistryBinding>,
    pub fault: Option<&'a crate::protocol::session::Fault>,
    pub fixture: Option<crate::protocol::observation::FixtureSetup>,
    pub fixture_fault: Option<crate::protocol::session::ObservationControl>,
    pub startup_seconds: u64,
    pub machine: &'a super::Machine,
    pub package: &'a std::collections::BTreeMap<String, Vec<u8>>,
}

pub(super) fn resolve(strategy: StrategyId) -> StrategyResolution {
    host::resolve(strategy)
}

pub(in crate::binding) use host::lifecycle;
pub(in crate::binding) use host::observation;
