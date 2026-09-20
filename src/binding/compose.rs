//! The one place that assembles the implementation for an exact build.
use super::{
    ContentIdentity,
    binary::{ImageIdentity, hash},
    groups, machine, platform,
    targets::{self},
};
use crate::{OpenError, UnavailableReason};

/// The live observation of one build, resolved for the compiled host.
pub(super) struct ResolvedObservation {
    pub machine: super::Machine,
    pub strategy: platform::StrategyResolution,
    /// SHA-256 of each installed content file that the private game profile pins.
    pub content: ContentIdentity,
    pub registries:
        std::collections::BTreeMap<String, crate::protocol::observation::RegistryBinding>,
}

impl ResolvedObservation {
    /// Why the compiled host cannot run this observation. Fixed for the life of the process.
    pub(super) fn host_prerequisites(&self) -> Vec<UnavailableReason> {
        self.strategy.unavailable.clone().into_iter().collect()
    }
}

pub(super) fn compose(image: &ImageIdentity) -> Result<ResolvedObservation, OpenError> {
    assemble(image, targets::lookup(image)?)
}

fn assemble(
    image: &ImageIdentity,
    recipe: &targets::Recipe,
) -> Result<ResolvedObservation, OpenError> {
    Ok(ResolvedObservation {
        machine: machine::resolve(image.architecture)?,
        strategy: platform::resolve(recipe.strategy),
        registries: groups::registries(recipe.groups),
        content: serde_json::from_str(recipe.content).expect("tracked content manifest"),
    })
}

/// The catalogued recipe on an authored image, for tests of what the shared code consumes.
#[cfg(test)]
pub(super) fn synthetic_variation() -> ResolvedObservation {
    let recipe = targets::lookup(&super::targets::test_identity()).unwrap();
    assemble(
        &ImageIdentity {
            executable: "synthetic-image".into(),
            slice: "synthetic-slice".into(),
            architecture: object::Architecture::Aarch64,
            format: object::BinaryFormat::MachO,
        },
        recipe,
    )
    .unwrap()
}

pub(super) fn analysis(
    image: &ImageIdentity,
    installation: super::installation::Installation,
) -> Result<super::BoundAnalysis, OpenError> {
    machine::static_methods(image.architecture)?;
    let composition = hash(
        &serde_json::to_vec(&serde_json::json!({
            "operation": "static-methods", "executable": image.executable, "slice": image.slice,
            "decoder": crate::engine::analysis::decode::DECODER,
            "implementation": env!("CARGO_PKG_VERSION"),
        }))
        .expect("static composition identity"),
    );
    let inputs = super::analysis::AnalysisInputs {
        composition,
        executable: image.executable.clone(),
        slice: image.slice.clone(),
        method: crate::engine::analysis::decode::METHOD,
        decoder: crate::engine::analysis::decode::DECODER,
        implementation: env!("CARGO_PKG_VERSION").into(),
    };
    let recipe = targets::lookup(image)?.discovery;
    let layout = crate::engine::analysis::discovery::SchedulerLayout {
        start: recipe.start,
        end: recipe.end,
        offset: recipe.offset,
        stride: recipe.stride,
        count: recipe.count,
    };
    let mut discovery_inputs = inputs.clone();
    discovery_inputs.method = crate::engine::analysis::discovery::METHOD;
    discovery_inputs.composition = hash(
        &serde_json::to_vec(&serde_json::json!({
            "operation": "registry-discovery", "executable": image.executable, "slice": image.slice,
            "layout": layout, "method": crate::engine::analysis::discovery::METHOD,
            "decoder": crate::engine::analysis::decode::DECODER, "demangler": "cpp_demangle-0.5.1",
            "implementation": env!("CARGO_PKG_VERSION"),
        }))
        .expect("discovery composition"),
    );
    let mut bound = super::BoundAnalysis::new(inputs, installation);
    bound.discovery = Some(super::analysis::BoundDiscovery {
        inputs: discovery_inputs,
        layout,
    });
    let mut fields = bound.inputs.clone();
    fields.method = crate::engine::analysis::fields::METHOD;
    fields.composition = hash(&serde_json::to_vec(&serde_json::json!({
        "operation": "registry-fields", "executable": image.executable, "slice": image.slice,
        "method": fields.method, "decoder": fields.decoder, "implementation": fields.implementation,
        "discovery": bound.discovery.as_ref().unwrap().inputs.composition,
    })).expect("field analysis composition"));
    bound.fields = Some(fields);
    Ok(bound)
}
