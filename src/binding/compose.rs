//! The one place that assembles the implementation for an exact build.
use super::{
    binary::ImageIdentity,
    groups,
    installation::Installation,
    machine, platform,
    targets::{self},
};
use crate::{OpenError, UnavailableReason};
use std::sync::{Arc, Mutex};

/// The live observation of one build, resolved for the compiled host.
pub(super) struct ResolvedObservation {
    pub machine: super::Machine,
    pub fixture: Option<crate::protocol::observation::FixtureBinding>,
    pub script_checks: Option<crate::protocol::script_check::ScriptCheckBinding>,
    pub strategy: platform::StrategyResolution,
    pub registry_layout: Option<groups::RegistryLayout>,
    pub default_registries: &'static [&'static str],
}

impl ResolvedObservation {
    /// Why the compiled host cannot run this observation. Fixed for the life of the process.
    pub(super) fn host_prerequisites(&self) -> Vec<UnavailableReason> {
        self.strategy.unavailable.clone().into_iter().collect()
    }
}

/// The implementation of one exact build: its live observation, and its static methods over the
/// installation with the build's default registries pinned.
pub(super) struct Composition {
    pub operation: ResolvedObservation,
    pub analysis: super::BoundAnalysis,
    pub installation: Installation,
}

/// Look up the build's recipe once and assemble everything that it selects.
pub(super) fn compose(
    image: &ImageIdentity,
    installation: Installation,
    invalidated: Arc<Mutex<Option<UnavailableReason>>>,
) -> Result<Composition, OpenError> {
    let recipe = targets::lookup(image)?;
    let operation = assemble(image, recipe)?;
    let installation = installation.with_default_registries(operation.default_registries);
    let analysis = super::BoundAnalysis::new(
        recipe.declarations,
        recipe.registry_layout.map(groups::RegistryLayout::database),
        installation.clone(),
        invalidated,
    );

    Ok(Composition {
        operation,
        analysis,
        installation,
    })
}

fn assemble(
    image: &ImageIdentity,
    recipe: &targets::Recipe,
) -> Result<ResolvedObservation, OpenError> {
    Ok(ResolvedObservation {
        machine: machine::resolve(image.architecture)?,
        strategy: platform::resolve(recipe.strategy),
        registry_layout: recipe.registry_layout,
        default_registries: recipe.default_registries,
        fixture: recipe.fixture.map(|bind| bind()),
        script_checks: recipe.script_checks.map(|bind| bind()),
    })
}

/// The catalogued recipe on an authored image, for tests of what the shared code consumes.
#[cfg(test)]
pub(super) fn synthetic_variation() -> ResolvedObservation {
    assemble(
        &ImageIdentity {
            executable: "synthetic-image".into(),
            slice: "synthetic-slice".into(),
            architecture: object::Architecture::Aarch64,
            format: object::BinaryFormat::MachO,
        },
        targets::test_recipe(),
    )
    .unwrap()
}
