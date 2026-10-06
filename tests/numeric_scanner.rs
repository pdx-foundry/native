//! Exact-platform boundary checks supporting faithful-storage ranges; overflow remains sampled.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::path::PathBuf;
use std::process::Command;

fn output(command: &mut Command) -> String {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

#[test]
#[ignore = "requires M452, the recorded macOS libc image, and the C compiler"]
fn m452_numeric_scanner_platform_observations() {
    let hint = PathBuf::from(std::env::var_os("STELLARIS_PATH").expect("set STELLARIS_PATH"));
    let native = pdx_native::Native::open(&hint).unwrap();
    let executable = if hint.is_file() {
        hint
    } else {
        let candidates: Vec<_> = [
            "stellaris.app/Contents/MacOS/stellaris",
            "Contents/MacOS/stellaris",
            "stellaris",
        ]
        .map(|relative| hint.join(relative))
        .into_iter()
        .filter(|path| path.is_file())
        .collect();
        assert_eq!(candidates.len(), 1, "requires a unique macOS executable");
        candidates[0].clone()
    };
    let symbols = output(Command::new("nm").arg("-m").arg(executable));
    let imports: Vec<_> = symbols
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("(undefined)")
                && (line.contains(" _sscanf ") || line.contains(" _atoll "))
        })
        .collect();
    assert_eq!(
        imports,
        [
            "(undefined) external _atoll (from libSystem)",
            "(undefined) external _sscanf (from libSystem)"
        ]
    );
    let directory = tempfile::tempdir().unwrap();
    let probe = directory.path().join("numeric-scanner");
    output(
        Command::new("cc")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-O0"])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tools/numeric_scanner.c"
            ))
            .arg("-o")
            .arg(&probe),
    );
    let observations: Vec<serde_json::Value> = output(&mut Command::new(probe))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let actual = serde_json::json!({
        "build": native.build(), "imports": imports, "observations": observations,
    });
    if let Some(path) = std::env::var_os("NATIVE_SCANNER_REPORT") {
        std::fs::write(path, serde_json::to_string_pretty(&actual).unwrap()).unwrap();
    }
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("expected/numeric-m452/scanner-platform.json")).unwrap();
    assert_eq!(actual["build"], expected["build"], "game build");
    assert_eq!(actual["imports"], expected["imports"], "game imports");
    let actual = actual["observations"].as_array().unwrap();
    let expected = expected["observations"].as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "observation count");
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual, expected, "platform or observation row {index}");
    }
}
