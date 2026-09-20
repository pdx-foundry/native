#!/usr/bin/env python3
"""Compile negative controls for binding privacy, synthetic construction, and release features."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target/admission-boundary"


def cargo(directory, arguments, expected=None):
    result = subprocess.run(
        ["cargo", *arguments, "--offline", "--target-dir", str(TARGET)],
        cwd=directory, text=True, capture_output=True,
    )
    if expected is None:
        if result.returncode:
            raise SystemExit(result.stderr)
    elif result.returncode == 0 or expected not in result.stderr:
        raise SystemExit(f"Negative control did not fail for {expected!r}:\n{result.stderr}")
    return result


def operation_fingerprint(build, key="PDX_NATIVE_OPERATION"):
    for row in map(json.loads, build.stdout.splitlines()):
        environment = dict(row.get('env', []))
        if key in environment:
            return environment[key]
    raise SystemExit('Operation fingerprint missing from build metadata')


def main():
    # Build the ordinary release and inspect its resolved features before testing forbidden sets.
    cargo(ROOT, ["build", "--locked", "--release", "--examples", "--features", "production"])
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--offline", "--format-version", "1", "--features", "production"], cwd=ROOT,
    ))
    native = next(package for package in metadata["packages"] if package["name"] == "pdx-native")
    features = next(node["features"] for node in metadata["resolve"]["nodes"] if node["id"] == native["id"])
    if set(features) != {"default", "production"}:
        raise SystemExit(f"Unexpected production feature closure: {features}")
    for features in ["production,maintainer-tools"]:
        cargo(ROOT, ["check", "--locked", "--features", features], "production cannot include")

    with tempfile.TemporaryDirectory(prefix="native-admission-boundary-") as temporary:
        package = Path(temporary) / "native"
        package.mkdir()
        for name in ["Cargo.toml", "Cargo.lock", "build.rs"]:
            shutil.copyfile(ROOT / name, package / name)
        for name in ["src", "crates", "examples"]:
            shutil.copytree(ROOT / name, package / name)
        analysis = package / "src/engine/analysis.rs"
        analysis.write_bytes(analysis.read_bytes() + b"\nmod prohibited_operation;\n")
        (package / "src/engine/analysis").mkdir(exist_ok=True)
        probe = package / "src/engine/analysis/prohibited_operation.rs"
        probe.write_text("pub fn harmless() {}\n")
        original = operation_fingerprint(cargo(package, ["check", "--lib", "--locked", "--message-format=json"]))
        evidence_manifest = package / 'crates/native-evidence/Cargo.toml'
        manifest_bytes = evidence_manifest.read_bytes()
        evidence_manifest.write_bytes(manifest_bytes + b'\n# Fingerprint variation control.\n')
        changed = operation_fingerprint(cargo(package, ["check", "--lib", "--locked", "--message-format=json"]))
        if original == changed:
            raise SystemExit('Evidence manifest change retained the qualified operation identity')
        evidence_manifest.write_bytes(manifest_bytes)
        cfg = subprocess.check_output(["rustc", "--print", "cfg"], text=True)
        leaf = "macos" if 'target_os="macos"' in cfg and 'target_arch="aarch64"' in cfg else "unavailable"
        imports = [
            "crate::binding::targets::records::CATALOGUE",
            "crate::binding::targets::Recipe",
            f"crate::binding::platform::{leaf}::resolve",
            "crate::binding::machine::arm64::READ_ENTRY_REVISION",
        ]
        for path in imports:
            probe.write_text(f"use {path};\n")
            cargo(package, ["check", "--lib", "--locked"], "is private")

        # Test the actual public dependency boundary rather than a same-crate import.
        consumer = Path(temporary) / "consumer"
        (consumer / "src").mkdir(parents=True)
        (consumer / "Cargo.toml").write_text(
            '[package]\nname = "admission-consumer"\nversion = "0.0.0"\nedition = "2024"\n'
            f'[dependencies]\npdx-native = {{ path = {json.dumps(str(ROOT))} }}\n'
        )
        main_rs = consumer / "src/main.rs"
        main_rs.write_text("fn main() { let _ = pdx_native::Engine; }\n")
        cargo(consumer, ["check"])
        main_rs.write_text("use pdx_native::investigation;\nfn main() {}\n")
        cargo(consumer, ["check"], "no `investigation` in the root")
        main_rs.write_text("fn main() { let _ = pdx_native::EngineContext {}; }\n")
        cargo(consumer, ["check"], "private fields")
        main_rs.write_text("fn main() { let _ = pdx_native::Game {}; }\n")
        cargo(consumer, ["check"], "private fields")
        main_rs.write_text("fn main() { let _ = pdx_native::GameOptions { retention_directory: Default::default(), startup_seconds: 180, idle_seconds: 180, control: () }; }\n")
        cargo(consumer, ["check"], "has no field named `control`")
        # A gated candidate report cannot be converted into a supported replay result.
        manifest = consumer / "Cargo.toml"
        manifest.write_text(manifest.read_text().replace(' }', ', features = ["maintainer-tools"] }'))
        main_rs.write_text("fn promote(value: pdx_native::investigation::InvestigationReport) -> pdx_native::ReplayResult { value.into() }\nfn main() {}\n")
        cargo(consumer, ["check"], "is not satisfied")
        main_rs.write_text("use pdx_native::binding::ExecutionPlan;\nfn main() {}\n")
        cargo(consumer, ["check"], "is private")
    print("Admission boundary verified: production features, release exclusion, private leaves, and opaque context construction")


if __name__ == "__main__":
    main()
