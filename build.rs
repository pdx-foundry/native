//! Compiles the presentation guard and stamps the build.
//!
//! The stamp lets a caller and its supervisor process check that they link the same Native
//! build. It is the time at which Cargo ran this script. Cargo runs the script again when a file
//! in `src`, the manifest, or this script changes, so two different states of the source do not
//! share a stamp. Two builds of the same source, for example with different profiles, also get
//! different stamps; the check then refuses them, which is the safe direction.
fn main() {
    for path in ["src", "Cargo.toml", "build.rs"] {
        println!("cargo:rerun-if-changed={path}");
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_nanos();
    println!("cargo:rustc-env=PDX_NATIVE_BUILD_STAMP={stamp}");

    if std::env::var("TARGET").as_deref() == Ok("aarch64-apple-darwin") {
        // A small library that the game loads, so that it shows no window and takes no focus.
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
    }
}
