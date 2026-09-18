fn main() {
    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_TEST_SUPPORT");
    if std::env::var_os("CARGO_FEATURE_TEST_SUPPORT").is_some()
        && std::env::var("PROFILE").as_deref() == Ok("release")
    {
        panic!("test-support is forbidden in release-profile builds");
    }
}
