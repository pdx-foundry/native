fn main() {
    use sha2::{Digest, Sha256};
    fn sources(path: &std::path::Path, hash: &mut Sha256) {
        // Python cache bytes are host-generated artifacts, not package source identities.
        if path
            .file_name()
            .is_some_and(|name| name == "__pycache__" || name == ".DS_Store")
        {
            return;
        }
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(path)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            entries.sort();
            for entry in entries {
                sources(&entry, hash);
            }
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
            let bytes = std::fs::read(path).unwrap();
            hash.update(path.to_string_lossy().replace('\\', "/").as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
    }
    // Static qualification is portable across hosts. It binds source and dependencies,
    // not a debugger, compiler installation, build profile, or mutable acceptance record.
    let mut analysis = Sha256::new();
    for path in [
        "src/lib.rs",
        "src/api.rs",
        "src/session.rs",
        "src/engine.rs",
        "src/engine",
        "src/binding.rs",
        "src/binding/analysis.rs",
        "src/binding/binary.rs",
        "src/binding/binary",
        "src/binding/installation.rs",
        "src/binding/compose.rs",
        "src/binding/machine.rs",
        "src/binding/machine",
        "src/binding/targets.rs",
        "src/binding/targets/recipes.rs",
        "src/binding/targets/records.rs",
        "src/qualification/analysis.rs",
        "crates/native-evidence/src",
        "crates/native-evidence/Cargo.toml",
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
    ] {
        sources(std::path::Path::new(path), &mut analysis);
    }
    println!(
        "cargo:rustc-env=PDX_NATIVE_ANALYSIS={:x}",
        analysis.finalize()
    );
    let mut operation = Sha256::new();
    for path in [
        "src/api.rs",
        "src/session.rs",
        "src/registry.rs",
        "src/game.rs",
        "src/game",
        "src/operation.rs",
        "src/capture.rs",
        "src/execution",
        "src/protocol",
        "src/protocol.rs",
        "src/supervisor.rs",
        "src/qualification/admission.rs",
        "src/binding.rs",
        "src/binding/compose.rs",
        "src/binding/installation.rs",
        "src/binding/platform.rs",
        "crates/native-evidence/src",
        "crates/native-evidence/Cargo.toml",
        "Cargo.lock",
        "Cargo.toml",
        "build.rs",
    ] {
        sources(std::path::Path::new(path), &mut operation);
    }
    let compiler = std::process::Command::new(std::env::var_os("RUSTC").expect("Cargo rustc"))
        .arg("-vV")
        .output()
        .expect("Rust compiler identity");
    assert!(
        compiler.status.success(),
        "Rust compiler identity unavailable"
    );
    operation.update(&compiler.stdout);
    for name in [
        "TARGET",
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_CFG_PANIC",
        "CARGO_ENCODED_RUSTFLAGS",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        operation.update(name.as_bytes());
        operation.update(std::env::var(name).unwrap_or_default().as_bytes());
    }
    let implementation = operation.finalize();
    println!("cargo:rustc-env=PDX_NATIVE_OPERATION={implementation:x}");
    println!(
        "cargo:rustc-env=PDX_NATIVE_COMPILER={:x}",
        Sha256::digest(&compiler.stdout)
    );
    println!(
        "cargo:rustc-env=PDX_NATIVE_PROFILE={}",
        std::env::var("PROFILE").unwrap()
    );
    let mut hash = Sha256::new();
    hash.update(implementation);
    for path in [
        "src",
        "crates/native-evidence/src",
        "crates/native-evidence/Cargo.toml",
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
    ] {
        sources(std::path::Path::new(path), &mut hash);
    }
    for name in [
        "TARGET",
        "PROFILE",
        "CARGO_FEATURE_MAINTAINER_TOOLS",
        "CARGO_FEATURE_TEST_SUPPORT",
        "CARGO_FEATURE_PRODUCTION",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        hash.update(name.as_bytes());
        hash.update(std::env::var(name).unwrap_or_default().as_bytes());
    }
    if std::env::var("TARGET").as_deref() == Ok("aarch64-apple-darwin") {
        let source = "src/binding/platform/macos/observation/guard.m";
        let output =
            std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("guard.dylib");
        let status = std::process::Command::new("xcrun")
            .args([
                "clang",
                "-arch",
                "arm64",
                "-dynamiclib",
                "-install_name",
                "@rpath/pdx-native-observation-guard.dylib",
                "-fobjc-arc",
                "-framework",
                "AppKit",
                "-o",
            ])
            .arg(&output)
            .arg(source)
            .status()
            .expect("Xcode clang is required for the observation guard");
        assert!(status.success(), "presentation guard compilation failed");
        hash.update(std::fs::read(output).unwrap());
    }
    println!("cargo:rustc-env=PDX_NATIVE_BUILD={:x}", hash.finalize());
    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_TEST_SUPPORT");
    if std::env::var_os("CARGO_FEATURE_TEST_SUPPORT").is_some()
        && std::env::var("PROFILE").as_deref() == Ok("release")
    {
        panic!("test-support is forbidden in release-profile builds");
    }
}
