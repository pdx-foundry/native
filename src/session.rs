use crate::{
    CapabilityReport, CapabilityRequest, ContextIdentity, ContextOrigin, OpenError, OpenRequest,
    UnavailableReason, binding::Binding, qualification,
};
use std::cell::RefCell;

/// A fixed installation or synthetic context. It cannot be rebound; live requests require ordinary admission.
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
            &self.binding.current_inputs(),
            self.binding.authority(),
            request,
            self.origin(),
            invalidated.clone(),
        )
    }
}

impl EngineContext {
    /// Configure consumer-hosted supervision and retention once, then call `get_registry`.
    /// The command must start a dedicated direct child that calls `supervisor::serve`.
    pub fn with_supervisor(
        self,
        command: std::process::Command,
        options: crate::RegistryOptions,
    ) -> Result<crate::RegistryClient, crate::RegistryError> {
        crate::registry::client(self, command, options)
    }

    pub(crate) fn prepare_registry(
        &self,
        name: &str,
        output: std::path::PathBuf,
        deadline_seconds: u64,
    ) -> Result<crate::operation::PreparedPlan, crate::RegistryError> {
        use crate::operation::{
            AttemptRequest, Authorization, ObservationControl, ObservationSpec, PlanRequest,
            PreparedPlan,
        };
        let report = self.capability(&CapabilityRequest {
            registry: name.into(),
        });
        if !report
            .bounds
            .registries
            .iter()
            .any(|supported| supported == name)
        {
            return Err(crate::RegistryError::Unsupported {
                registry: name.into(),
            });
        }
        let installation_hint = self
            .binding
            .installation_hint()
            .map_err(crate::RegistryError::from)?;
        if report.availability != crate::Availability::Available {
            return Err(crate::RegistryError::Unavailable {
                reasons: report.reasons,
            });
        }
        let spec = ObservationSpec {
            registry: Some(name.into()),
            fixture: String::new(),
            deadline_seconds,
            control: ObservationControl::Normal,
        };
        spec.validate()?;
        let request = AttemptRequest {
            installation_hint,
            output,
            hold_ms: 1,
        };
        crate::operation::validate_request(&request)?;
        Ok(PreparedPlan {
            request: PlanRequest {
                request,
                composition: self.identity().0,
                authorization: Authorization::Admitted,
                observation: Some(spec),
            },
        })
    }
}
