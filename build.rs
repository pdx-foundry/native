fn main() {
    use sha2::{Digest, Sha256};
    fn sources(path: &std::path::Path, hash: &mut Sha256) {
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
            hash.update(path.to_string_lossy().as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
    }
    let mut hash = Sha256::new();
    for path in [
        "src",
        "crates/native-evidence/src",
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
    println!("cargo:rustc-env=PDX_NATIVE_BUILD={:x}", hash.finalize());
    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_TEST_SUPPORT");
    if std::env::var_os("CARGO_FEATURE_TEST_SUPPORT").is_some()
        && std::env::var("PROFILE").as_deref() == Ok("release")
    {
        panic!("test-support is forbidden in release-profile builds");
    }
}
