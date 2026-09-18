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
    /// Prepare the complete bounded observation window for a consumer-hosted supervisor.
    /// No game is launched. The owner rechecks admission and input integrity before launch.
    pub fn prepare_observation(
        &self,
        request: crate::ObservationRequest,
        capture: crate::CaptureOptions,
    ) -> Result<crate::ObservationPlan, crate::supervisor::SupervisorError> {
        use crate::operation::{
            AttemptRequest, Authorization, ObservationControl, ObservationSpec, PlanRequest,
            PreparedPlan,
        };
        use crate::supervisor::SupervisorError;
        let spec = ObservationSpec {
            fixture: request.fixture,
            deadline_seconds: request.deadline_seconds,
            control: ObservationControl::Normal,
        };
        spec.validate()?;
        let request = AttemptRequest {
            installation_hint: self.binding.installation_hint()?,
            output: capture.output,
            hold_ms: 1,
        };
        crate::operation::validate_request(&request)?;
        let report = self.capability(&crate::CapabilityRequest::default());
        if report.availability != crate::Availability::Available {
            return Err(SupervisorError(format!(
                "Live admission refused: {:?}",
                report.reasons
            )));
        }
        Ok(crate::ObservationPlan(PreparedPlan {
            request: PlanRequest {
                request,
                composition: self.identity().0,
                authorization: Authorization::Admitted,
                observation: Some(spec),
            },
        }))
    }
}
