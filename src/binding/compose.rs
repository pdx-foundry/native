use super::{
    binary::{ImageIdentity, hash},
    groups, machine, platform,
    targets::{self, MethodId},
};
use crate::qualification::{AdmissionInputs, ContentIdentity};
use crate::{ObservationBounds, OpenError, UnavailableReason};

pub(super) const METHOD: &str = "registration-category-read-entries/candidate-v1";

pub(super) fn compose(
    image: &ImageIdentity,
    content: Result<ContentIdentity, UnavailableReason>,
) -> Result<AdmissionInputs, OpenError> {
    let recipe = targets::lookup(image)?;
    let bindings: Vec<_> = recipe
        .groups
        .iter()
        .map(|group| groups::resolve(*group))
        .collect();
    let machine = machine::resolve(image.architecture)?;
    let strategy = platform::resolve(recipe.strategy);
    let (method, bounds) = match recipe.method {
        MethodId::BoundedRegistrationCategoryReads => (
            METHOD,
            ObservationBounds {
                registration_entries: 3,
                category_fields: vec!["tree_template".into(), "traditions".into()],
            },
        ),
    };
    // Length-delimited parts prevent ambiguous concatenations. Include declarations as well as
    // revisions so a binding edit cannot silently keep the former composition identity.
    let parts = [
        image.executable.as_str(),
        image.slice.as_str(),
        recipe.revision,
        method,
        machine,
        strategy.revision,
    ];
    let mut bytes = Vec::new();
    for part in parts.into_iter().chain(bindings.iter().map(String::as_str)) {
        bytes.extend((part.len() as u64).to_le_bytes());
        bytes.extend(part.as_bytes());
    }
    for source in [
        include_bytes!("platform/macos/observation/worker.py").as_slice(),
        include_bytes!("platform/macos/observation/protocol.py").as_slice(),
        include_bytes!("platform/macos/observation/guard.m").as_slice(),
    ] {
        bytes.extend((source.len() as u64).to_le_bytes());
        bytes.extend(source);
    }
    Ok(AdmissionInputs {
        composition: hash(&bytes),
        bounds,
        content,
        prerequisites: vec![strategy.unavailable],
    })
}
