mod support;

use pdx_native::{Engine, OpenError, OpenRequest};
use std::fs;
use tempfile::tempdir;

fn open(path: &std::path::Path) -> OpenError {
    Engine::open(OpenRequest {
        installation_hint: path.into(),
    })
    .unwrap_err()
}

#[test]
fn installation_errors_are_precise_without_a_game() {
    let directory = tempdir().unwrap();
    assert!(matches!(open(directory.path()), OpenError::Missing(_)));
    let executable = directory.path().join("stellaris");
    fs::write(&executable, b"not an image").unwrap();
    assert_eq!(open(&executable), OpenError::MalformedExecutable);
    fs::write(&executable, support::macho(0x01000007)).unwrap();
    assert_eq!(open(&executable), OpenError::UnsupportedTarget);
    fs::write(&executable, support::macho(0x0100000c)).unwrap();
    assert_eq!(open(&executable), OpenError::UnknownTarget);
    fs::write(directory.path().join("stellaris.exe"), b"another candidate").unwrap();
    assert_eq!(open(directory.path()), OpenError::Ambiguous);
}

#[test]
fn a_directory_in_place_of_an_executable_is_unreadable() {
    let directory = tempdir().unwrap();
    fs::create_dir(directory.path().join("stellaris")).unwrap();
    assert!(matches!(open(directory.path()), OpenError::Unreadable(_)));
}

#[test]
fn a_windows_image_is_identified_without_loading_windows_code() {
    let directory = tempdir().unwrap();
    let executable = directory.path().join("stellaris.exe");
    fs::write(&executable, support::pe()).unwrap();
    assert_eq!(open(&executable), OpenError::UnknownTarget);
}

#[test]
fn a_64_bit_fat_header_selects_the_arm64_slice() {
    let directory = tempdir().unwrap();
    let executable = directory.path().join("stellaris");
    let mut bytes = Vec::new();
    for value in [0xcafebabfu32, 1, 0x0100000c, 0] {
        bytes.extend(value.to_be_bytes());
    }
    for value in [40u64, 32] {
        bytes.extend(value.to_be_bytes());
    }
    bytes.extend([0; 8]);
    bytes.extend(support::macho(0x0100000c));
    fs::write(&executable, bytes).unwrap();
    assert_eq!(open(&executable), OpenError::UnknownTarget);
}

#[test]
fn bundle_and_installation_hints_find_the_same_unknown_image() {
    let directory = tempdir().unwrap();
    let bundle = directory.path().join("stellaris.app");
    let binary = bundle.join("Contents/MacOS/stellaris");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, support::macho(0x0100000c)).unwrap();
    for hint in [directory.path(), bundle.as_path(), binary.as_path()] {
        assert_eq!(open(hint), OpenError::UnknownTarget);
    }
}

#[test]
fn universal_images_require_one_valid_arm64_slice() {
    let directory = tempdir().unwrap();
    let binary = directory.path().join("stellaris");
    let arm = (0x0100000c, support::macho(0x0100000c));
    let intel = (0x01000007, support::macho(0x01000007));
    fs::write(&binary, support::fat(&[intel.clone(), arm.clone()])).unwrap();
    assert_eq!(open(&binary), OpenError::UnknownTarget);
    fs::write(&binary, support::fat(&[arm.clone(), arm])).unwrap();
    assert_eq!(open(&binary), OpenError::Ambiguous);
    fs::write(&binary, support::fat(std::slice::from_ref(&intel))).unwrap();
    assert_eq!(open(&binary), OpenError::UnsupportedTarget);
    fs::write(&binary, support::fat(&[(0x0100000c, intel.1)])).unwrap();
    assert_eq!(open(&binary), OpenError::MalformedExecutable);
    let mut truncated = support::fat(&[(0x0100000c, support::macho(0x0100000c))]);
    truncated.pop();
    fs::write(&binary, truncated).unwrap();
    assert_eq!(open(&binary), OpenError::MalformedExecutable);
}
