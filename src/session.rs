use crate::{
    CapabilityReport, CapabilityRequest, ContextIdentity, ContextOrigin, OpenError, OpenRequest,
    UnavailableReason, binding::Binding, qualification,
};
use std::cell::RefCell;

/// A fixed installation or synthetic context. It cannot be rebound or used to launch a game.
#[derive(Debug)]
pub struct EngineContext {
    binding: Binding,
    invalidated: RefCell<Option<UnavailableReason>>,
}

pub(crate) fn open(request: OpenRequest) -> Result<EngineContext, OpenError> {
    Ok(EngineContext::from_binding(Binding::open(request)?))
}

impl EngineContext {
    pub(crate) fn from_binding(binding: Binding) -> Self {
        Self {
            binding,
            invalidated: RefCell::new(None),
        }
    }

    /// Fixed composition identity. Equality does not establish current input integrity.
    pub fn identity(&self) -> ContextIdentity {
        self.binding.identity()
    }

    /// Whether the context comes from an installation or authored synthetic inputs.
    pub fn origin(&self) -> ContextOrigin {
        self.binding.origin()
    }

    /// Check the requested bounds, current inputs, prerequisites, and bundled qualifications.
    /// Once an integrity check fails, reopen the context; restoring bytes does not revive it.
    pub fn capability(&self, request: &CapabilityRequest) -> CapabilityReport {
        let mut invalidated = self.invalidated.borrow_mut();
        if invalidated.is_none() {
            *invalidated = self.binding.integrity();
        }
        qualification::evaluate(
            self.binding.inputs(),
            self.binding.authority(),
            request,
            self.origin(),
            invalidated.clone(),
        )
    }
}
