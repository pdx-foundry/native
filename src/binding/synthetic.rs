use super::{Binding, Source, platform, targets::StrategyId};
use crate::qualification::{AcceptedRecord, AdmissionInputs, Authority};
use crate::test_support::SyntheticCase;
use crate::{ArtifactReference, RegistryBounds, UnavailableReason};
use std::collections::BTreeMap;

pub(crate) fn synthetic(case: SyntheticCase) -> Binding {
    let bounds = RegistryBounds {
        registries: vec!["traditions".into(), "tradition_categories".into()],
    };
    let content = BTreeMap::from([(
        "synthetic/category.txt".into(),
        super::binary::hash(b"synthetic content"),
    )]);
    let mut inputs = AdmissionInputs {
        composition: super::binary::hash(
            b"synthetic exact target / recipe / method / bindings / simulator-v1",
        ),
        bounds: bounds.clone(),
        content: Ok(content.clone()),
        prerequisites: Vec::new(),
        toolchain: Ok("synthetic-toolchain".into()),
    };
    let mut record = AcceptedRecord {
        id: "synthetic-acceptance-v1".into(),
        toolchain: "synthetic-toolchain".into(),
        composition: inputs.composition.clone(),
        bounds,
        content,
        // Deliberately unavailable: admission must never open historical evidence.
        evidence: vec![ArtifactReference {
            path: "unavailable-history/synthetic-qualification.json".into(),
            sha256: super::binary::hash(b"synthetic qualification"),
            bytes: 23,
        }],
    };
    let mut withdrawn = Vec::new();
    let mut integrity = None;
    match case {
        SyntheticCase::Accepted
        | SyntheticCase::RecipeOnly
        | SyntheticCase::SplitQualifications
        | SyntheticCase::ReplacementAcceptance => {}
        SyntheticCase::Withdrawn => withdrawn.push(record.id.clone()),
        SyntheticCase::RevisionMismatch => {
            record.composition = super::binary::hash(b"different composition")
        }
        SyntheticCase::ContentMismatch => {
            record.content.insert(
                "synthetic/category.txt".into(),
                super::binary::hash(b"old content"),
            );
        }
        SyntheticCase::HelperMismatch => inputs.toolchain = Ok("changed-toolchain".into()),
        SyntheticCase::HelperUnavailable => {
            inputs.toolchain = Err(UnavailableReason::PrerequisiteMissing)
        }
        SyntheticCase::ContentChanged => integrity = Some(UnavailableReason::ContentChanged),
        SyntheticCase::TargetChanged => integrity = Some(UnavailableReason::TargetChanged),
        SyntheticCase::InputUnavailable => integrity = Some(UnavailableReason::InputUnavailable),
        SyntheticCase::ContentUnavailable => {
            inputs.content = Err(UnavailableReason::InputUnavailable)
        }
        SyntheticCase::MissingPrerequisite => inputs
            .prerequisites
            .push(UnavailableReason::PrerequisiteMissing),
        SyntheticCase::NarrowQualification => {
            record.bounds.registries = vec!["tradition_categories".into()]
        }
        SyntheticCase::RealStrategy => inputs.prerequisites.extend(
            platform::resolve(StrategyId::MacSuspendedChildLoaderEntry)
                .unavailable
                .or(Some(UnavailableReason::ImplementationUnavailable)),
        ),
    }
    let accepted = match case {
        SyntheticCase::RecipeOnly => Vec::new(),
        SyntheticCase::SplitQualifications => {
            let mut second = record.clone();
            second.id = "synthetic-second-field".into();
            second.bounds.registries = vec!["tradition_categories".into()];
            record.bounds.registries = vec!["traditions".into()];
            vec![record, second]
        }
        SyntheticCase::ReplacementAcceptance => {
            let mut replacement = record.clone();
            replacement.id = "synthetic-replacement".into();
            withdrawn.push(record.id.clone());
            vec![record, replacement]
        }
        _ => vec![record],
    };
    Binding {
        inputs,
        operation: None,
        source: Source::Synthetic(integrity),
        authority: Authority {
            accepted,
            withdrawn,
        },
    }
}
