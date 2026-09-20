use super::{
    binary::{ImageIdentity, hash},
    groups, machine, platform,
    targets::{self, MethodId},
};
use crate::qualification::{AdmissionInputs, ContentIdentity};
use crate::{OpenError, RegistryBounds, UnavailableReason};

pub(super) const METHOD: &str = "tradition-registry-snapshot/v1";

pub(super) struct ResolvedObservation {
    pub image: ImageIdentity,
    pub machine: super::Machine,
    pub strategy: platform::StrategyResolution,
    pub bindings: std::collections::BTreeMap<String, u64>,
    pub content: ContentIdentity,
    pub method: &'static str,
    pub session_method: &'static str,
    pub early_method: &'static str,
    pub registries:
        std::collections::BTreeMap<String, crate::protocol::observation::RegistryBinding>,
}

pub(super) fn compose(
    image: &ImageIdentity,
    content: Result<ContentIdentity, UnavailableReason>,
) -> Result<(AdmissionInputs, ResolvedObservation), OpenError> {
    assemble(image, content, targets::lookup(image)?)
}

fn assemble(
    image: &ImageIdentity,
    content: Result<ContentIdentity, UnavailableReason>,
    recipe: &targets::Recipe,
) -> Result<(AdmissionInputs, ResolvedObservation), OpenError> {
    let machine = machine::resolve(image.architecture)?;
    let strategy = platform::resolve(recipe.strategy);
    let method = match recipe.method {
        MethodId::TraditionRegistryKeys => METHOD,
    };
    let session_method = "tradition-registry-session/v1";
    let early_method = "registration-category-read-entries/v2";
    let bindings = groups::observation(recipe.groups);
    let registries = groups::registries(&bindings);
    let bounds = RegistryBounds {
        registries: registries.keys().cloned().collect(),
    };
    let expected: ContentIdentity =
        serde_json::from_str(recipe.content).expect("tracked content manifest");
    let declarations: Vec<_> = recipe
        .groups
        .iter()
        .map(|group| groups::resolve(*group))
        .collect();
    let packages: std::collections::BTreeMap<_, _> = strategy
        .package
        .iter()
        .map(|(name, bytes)| (name, hash(bytes)))
        .collect();
    let identity = serde_json::json!({
        "target": image.executable, "slice": image.slice, "recipe": recipe.revision,
        "method": method, "sessionMethod": session_method, "earlyMethod": early_method, "machine": machine, "strategy": strategy.revision,
        "bindings": bindings, "registries": registries, "declarations": declarations, "package": packages,
        "content": expected, "implementation": env!("PDX_NATIVE_OPERATION"),
    });
    let inputs = AdmissionInputs {
        composition: hash(&serde_json::to_vec(&identity).expect("composition identity")),
        bounds,
        content,
        toolchain: Err(UnavailableReason::PrerequisiteMissing),
        prerequisites: strategy.unavailable.clone().into_iter().collect(),
    };
    Ok((
        inputs,
        ResolvedObservation {
            image: image.clone(),
            machine,
            strategy,
            bindings,
            registries,
            content: expected,
            method,
            session_method,
            early_method,
        },
    ))
}

#[cfg(test)]
pub(super) fn synthetic_variation(
    content: ContentIdentity,
) -> (AdmissionInputs, ResolvedObservation) {
    let recipe = targets::Recipe {
        revision: "synthetic-recipe",
        groups: &[
            targets::BindingGroupId::SyntheticRegistration,
            targets::BindingGroupId::CategoryReader,
            targets::BindingGroupId::Registries,
        ],
        method: MethodId::TraditionRegistryKeys,
        strategy: targets::StrategyId::MacSuspendedChildLoaderEntry,
        content: "{}",
        discovery: targets::lookup(&super::targets::test_identity())
            .unwrap()
            .discovery,
    };
    assemble(
        &ImageIdentity {
            executable: "synthetic-image".into(),
            slice: "synthetic-slice".into(),
            architecture: object::Architecture::Aarch64,
            format: object::BinaryFormat::MachO,
        },
        Ok(content),
        &recipe,
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
    let inputs = crate::qualification::analysis::AnalysisInputs {
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
            "recipe": recipe.revision, "layout": layout, "method": crate::engine::analysis::discovery::METHOD,
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
