mod binary;
mod compose;
mod groups;
mod installation;
mod machine;
mod platform;
mod targets;

#[cfg(test)]
mod tests;

use crate::qualification::{AdmissionInputs, Authority};
use crate::{ContextIdentity, ContextOrigin, OpenError, OpenRequest, UnavailableReason};

/// The only session-facing binding value. Raw target and platform descriptors stay below here.
#[derive(Debug)]
pub(crate) struct Binding {
    inputs: AdmissionInputs,
    source: Source,
    authority: Authority,
}

#[derive(Debug)]
enum Source {
    Installation(installation::Installation),
    #[cfg(feature = "test-support")]
    Synthetic(Option<UnavailableReason>),
}

impl Binding {
    pub(crate) fn open(request: OpenRequest) -> Result<Self, OpenError> {
        let (installation, bytes) = installation::Installation::open(&request.installation_hint)?;
        let image = binary::identify(&bytes)?;
        let inputs = compose::compose(&image, installation.content.clone())?;
        Ok(Self {
            inputs,
            source: Source::Installation(installation),
            authority: Authority::bundled(),
        })
    }

    pub(crate) fn identity(&self) -> ContextIdentity {
        ContextIdentity(self.inputs.composition.clone())
    }

    pub(crate) fn origin(&self) -> ContextOrigin {
        match self.source {
            Source::Installation(_) => ContextOrigin::Installation,
            #[cfg(feature = "test-support")]
            Source::Synthetic(_) => ContextOrigin::Synthetic,
        }
    }

    pub(crate) fn integrity(&self) -> Option<UnavailableReason> {
        match &self.source {
            Source::Installation(installation) => installation.integrity(),
            #[cfg(feature = "test-support")]
            Source::Synthetic(failure) => failure.clone(),
        }
    }

    pub(crate) fn inputs(&self) -> &AdmissionInputs {
        &self.inputs
    }

    pub(crate) fn authority(&self) -> &Authority {
        &self.authority
    }
}

#[cfg(feature = "test-support")]
mod synthetic;

#[cfg(feature = "test-support")]
pub(crate) use synthetic::synthetic;

#[cfg(feature = "maintainer-tools")]
pub(crate) struct InvestigationPlan {
    pub composition: String,
    installation: installation::Installation,
}

#[cfg(feature = "maintainer-tools")]
impl InvestigationPlan {
    pub fn open(hint: &std::path::Path) -> Result<Self, crate::supervisor::SupervisorError> {
        platform::lifecycle::available()?;
        let (installation, bytes) = installation::Installation::open(hint)
            .map_err(|e| crate::supervisor::SupervisorError(e.to_string()))?;
        let image = binary::identify(&bytes)
            .map_err(|e| crate::supervisor::SupervisorError(e.to_string()))?;
        let inputs = compose::compose(&image, installation.content.clone())
            .map_err(|e| crate::supervisor::SupervisorError(e.to_string()))?;
        if let Some(reason) = installation.integrity() {
            return Err(crate::supervisor::SupervisorError(format!(
                "Candidate inputs unavailable: {reason:?}"
            )));
        }
        Ok(Self {
            composition: inputs.composition,
            installation,
        })
    }

    pub fn integrity(&self) -> Result<(), crate::supervisor::SupervisorError> {
        match self.installation.integrity() {
            None => Ok(()),
            Some(reason) => Err(crate::supervisor::SupervisorError(format!(
                "Candidate inputs changed: {reason:?}"
            ))),
        }
    }

    pub fn spawn(
        &self,
        output: &std::path::Path,
    ) -> Result<OwnedGame, crate::supervisor::SupervisorError> {
        self.integrity()?;
        platform::lifecycle::spawn(
            self.installation.executable(),
            self.installation.root(),
            output,
        )
    }
}

#[cfg(feature = "maintainer-tools")]
pub(crate) use platform::lifecycle::{
    HostReservation, OwnedGame, acquire_reservation, conflicting_game, open_record, prepare_owner,
    private_directory, process_identity,
};

#[cfg(feature = "maintainer-tools")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcessIdentity {
    pub pid: u32,
    pub started_seconds: u64,
    pub started_microseconds: u64,
}

#[cfg(all(
    test,
    feature = "maintainer-tools",
    target_os = "macos",
    target_arch = "aarch64"
))]
pub(crate) fn test_reservation(
    root: &std::path::Path,
) -> Result<HostReservation, crate::supervisor::SupervisorError> {
    platform::lifecycle::acquire_at(root)
}

#[cfg(all(
    test,
    feature = "maintainer-tools",
    target_os = "macos",
    target_arch = "aarch64"
))]
pub(crate) fn test_child(
    output: &std::path::Path,
) -> Result<OwnedGame, crate::supervisor::SupervisorError> {
    platform::lifecycle::spawn(
        std::path::Path::new("/bin/sleep"),
        std::path::Path::new("/"),
        output,
    )
}

#[cfg(all(
    test,
    feature = "maintainer-tools",
    target_os = "macos",
    target_arch = "aarch64"
))]
pub(crate) static LIFECYCLE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
