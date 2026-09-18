use super::{Binding, Source, installation::Installation};
use crate::qualification::{AdmissionInputs, Authority};
use crate::{CapabilityRequest, ContextOrigin, Qualification, UnavailableReason};
use std::fs;
use tempfile::{TempDir, tempdir};

fn installation() -> (TempDir, crate::EngineContext) {
    let directory = tempdir().unwrap();
    for relative in ["common/tradition_categories", "common/traditions"] {
        fs::create_dir_all(directory.path().join(relative)).unwrap();
    }
    fs::write(directory.path().join("launcher-settings.json"), "{}").unwrap();
    fs::write(directory.path().join("stellaris"), "authored test bytes").unwrap();
    fs::write(directory.path().join("common/traditions/test.txt"), "test").unwrap();
    let (installation, _) = Installation::open(directory.path()).unwrap();
    // Private I/O control, never a catalogue entry or accepted qualification. Public contexts
    // cannot supply these bytes to the composer, and the test factory cannot accept this path.
    let request = CapabilityRequest::default();
    let binding = Binding {
        inputs: AdmissionInputs {
            composition: "private-io-control".into(),
            bounds: crate::ObservationBounds {
                registration_entries: request.registration_entries,
                category_fields: request.category_fields,
            },
            content: installation.content.clone(),
            prerequisites: Vec::new(),
        },
        source: Source::Installation(installation),
        authority: Authority::bundled(),
    };
    (
        directory,
        crate::session::EngineContext::from_binding(binding),
    )
}

#[test]
fn executable_replacement_permanently_invalidates_the_context() {
    let (directory, context) = installation();
    let request = CapabilityRequest::default();
    assert_eq!(context.origin(), ContextOrigin::Installation);
    assert_eq!(
        context.capability(&request).reasons,
        [UnavailableReason::QualificationMissing]
    );
    fs::write(directory.path().join("stellaris"), "changed executable").unwrap();
    assert!(
        context
            .capability(&request)
            .reasons
            .contains(&UnavailableReason::TargetChanged)
    );
    fs::write(directory.path().join("stellaris"), "authored test bytes").unwrap();
    assert!(
        context
            .capability(&request)
            .reasons
            .contains(&UnavailableReason::TargetChanged)
    );
}

#[test]
fn content_additions_deletions_and_edits_invalidate_the_bound_snapshot() {
    for mutation in ["add", "delete", "edit"] {
        let (directory, context) = installation();
        let original = directory.path().join("common/traditions/test.txt");
        match mutation {
            "add" => fs::write(directory.path().join("common/traditions/new.txt"), "new").unwrap(),
            "delete" => fs::remove_file(original).unwrap(),
            _ => fs::write(original, "changed").unwrap(),
        }
        let report = context.capability(&CapabilityRequest::default());
        assert_eq!(report.qualification, Qualification::Incomplete);
        assert!(
            report.reasons.contains(&UnavailableReason::ContentChanged),
            "{mutation}"
        );
    }
}

#[test]
fn missing_inputs_never_become_empty_success() {
    let (directory, context) = installation();
    fs::remove_file(directory.path().join("stellaris")).unwrap();
    let report = context.capability(&CapabilityRequest::default());
    assert!(
        report
            .reasons
            .contains(&UnavailableReason::InputUnavailable)
    );
}

#[cfg(unix)]
#[test]
fn retargeting_the_original_executable_hint_is_detected() {
    use std::os::unix::fs::symlink;
    let (directory, _) = installation();
    let hint = directory.path().join("hint");
    symlink(directory.path().join("stellaris"), &hint).unwrap();
    let (bound, _) = Installation::open(&hint).unwrap();
    assert_eq!(bound.integrity(), None);
    let replacement = directory.path().join("replacement");
    fs::write(&replacement, "authored test bytes").unwrap();
    fs::remove_file(&hint).unwrap();
    symlink(&replacement, &hint).unwrap();
    assert_eq!(bound.integrity(), Some(UnavailableReason::TargetChanged));
}

#[cfg(unix)]
#[test]
fn content_parent_symlinks_are_unavailable() {
    use std::os::unix::fs::symlink;
    let (directory, context) = installation();
    fs::rename(
        directory.path().join("common"),
        directory.path().join("moved-common"),
    )
    .unwrap();
    symlink(
        directory.path().join("moved-common"),
        directory.path().join("common"),
    )
    .unwrap();
    assert!(
        context
            .capability(&CapabilityRequest::default())
            .reasons
            .contains(&UnavailableReason::InputUnavailable)
    );
}
