//! The one place that assembles the implementation for an exact build.
use super::{
    binary::ImageIdentity,
    groups, machine, platform,
    targets::{self},
};
use crate::{OpenError, UnavailableReason};

/// The live observation of one build, resolved for the compiled host.
pub(super) struct ResolvedObservation {
    pub machine: super::Machine,
    pub fixture: Option<crate::protocol::observation::FixtureBinding>,
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
        registry_layout: groups::registry_layout(recipe.groups),
        default_registries: recipe.default_registries,
        fixture: groups::fixture(recipe.groups),
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

/// The static methods of one build: the executable identities that they check before each
/// read, and the recipe's scheduling-table layout.
pub(super) fn analysis(
    image: &ImageIdentity,
    installation: super::installation::Installation,
) -> Result<super::BoundAnalysis, OpenError> {
    machine::static_methods(image.architecture)?;
    let recipe = targets::lookup(image)?.discovery;
    let layout = crate::engine::analysis::discovery::SchedulerLayout {
        start: recipe.start,
        end: recipe.end,
        offset: recipe.offset,
        stride: recipe.stride,
        count: recipe.count,
    };
    Ok(super::BoundAnalysis::new(layout, installation))
}
