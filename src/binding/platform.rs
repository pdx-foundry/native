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

pub(super) struct StrategyResolution {
    pub revision: &'static str,
    pub unavailable: Option<UnavailableReason>,
    pub package: std::collections::BTreeMap<String, Vec<u8>>,
    pub probe: fn() -> Result<String, crate::supervisor::SupervisorError>,
    pub prepare: fn(
        ObservationSetup<'_>,
    ) -> Result<
        (observation::Observer, crate::capture::Capture),
        crate::supervisor::SupervisorError,
    >,
}

#[cfg_attr(
    not(all(target_os = "macos", target_arch = "aarch64")),
    allow(dead_code)
)]
pub(in crate::binding) struct ObservationSetup<'a> {
    pub output: &'a std::path::Path,
    pub attempt: &'a str,
    pub spec: &'a crate::operation::ObservationSpec,
    pub executable: &'a std::path::Path,
    pub identity: serde_json::Value,
    pub expected_tool: Option<&'a str>,
    pub content: &'a crate::qualification::ContentIdentity,
    pub bindings: &'a std::collections::BTreeMap<String, u64>,
    pub machine: &'a super::Machine,
    pub package: &'a std::collections::BTreeMap<String, Vec<u8>>,
}

pub(super) fn resolve(strategy: StrategyId) -> StrategyResolution {
    host::resolve(strategy)
}

pub(in crate::binding) use host::lifecycle;
pub(in crate::binding) use host::observation;
