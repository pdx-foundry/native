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
