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
