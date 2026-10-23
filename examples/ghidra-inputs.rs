//! Write the inputs of the Ghidra export (`tools/ghidra/README.md`) for an installation:
//! `tables.json` for the build directory, and the root readers of each `common/` and `events`
//! registry for the export's driver. No game starts.
//!
//! Usage: `ghidra-inputs <installation> <tables.json> <roots.json>`
use std::path::Path;

use pdx_native::Native;
use pdx_native::internals::{build_tables, registry_field_stops};
use serde_json::{Value, json};

/// The schema revision of `tables.json`.
const TABLES_SCHEMA: u64 = 1;

/// The readers whose literal scope mask 0 does not mean "no scope": decision D5(a) in
/// `docs/design/simplification.md`. Recorded by hand; the loader applies them before the mask-0
/// rule.
const ZERO_MASK_EXCEPTIONS: [(&str, &str); 2] = [
    ("CScaledMTTHModifier::ReadMember(CReader&, int)", "limit"),
    (
        "CComplexTriggerMTTHModifier::ReadMember(CReader&, int)",
        "potential",
    ),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let usage = "usage: ghidra-inputs <installation> <tables.json> <roots.json>";
    let mut arguments = std::env::args().skip(1);
    let installation = arguments.next().ok_or(usage)?;
    let tables_path = arguments.next().ok_or(usage)?;
    let roots_path = arguments.next().ok_or(usage)?;
    let native = Native::open(&installation)?;

    let tables = tables(&native, &version_label(Path::new(&installation))?)?;
    std::fs::write(&tables_path, serde_json::to_string_pretty(&tables)? + "\n")?;

    let roots = roots(&native)?;
    std::fs::write(&roots_path, serde_json::to_string_pretty(&roots)? + "\n")?;
    Ok(())
}

/// The launcher's version label of the installed game, such as `Cygnus v4.5.2 (9776)`.
fn version_label(installation: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let settings: Value =
        serde_json::from_slice(&std::fs::read(installation.join("launcher-settings.json"))?)?;
    let label = settings["version"]
        .as_str()
        .ok_or("launcher-settings.json has no version label")?;

    Ok(label.to_owned())
}

fn tables(native: &Native, version: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let tables = build_tables::read(native)?;
    let exceptions: Vec<Value> = ZERO_MASK_EXCEPTIONS
        .iter()
        .map(|(reader, key)| {
            json!({
                "reader": reader,
                "block": true,
                "keys": [key],
                "read_scope": "unresolved",
                "decision": "docs/design/simplification.md#decisions-of-2026-10-07",
                "written_by": "hand",
            })
        })
        .collect();

    Ok(json!({
        "schema": TABLES_SCHEMA,
        "tool": "native-tables",
        "build": {"executable": tables.build, "version": version},
        "tokens": tables.tokens,
        "scope_types": tables.scope_types,
        "reader": tables.reader,
        "manual_exceptions": {"zero_mask_read_scope": exceptions},
    }))
}

/// The root readers of each registry under `common/` and of `events`, the directories that the
/// Milestone 5 measure counts. A registry with no one owner keeps its error.
fn roots(native: &Native) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let registries = native.registries()?;
    let selected = registries
        .value
        .iter()
        .map(|registry| registry.name.as_str())
        .filter(|name| {
            name.starts_with("common/") || *name == "events" || name.starts_with("events/")
        });

    Ok(selected
        .map(
            |name| match registry_field_stops::root_readers(native, name) {
                Ok(roots) => json!({"registry": name, "roots": roots}),
                Err(error) => json!({"registry": name, "error": error.to_string()}),
            },
        )
        .collect())
}
