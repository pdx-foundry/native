//! Generate static parity candidates for review. Starts no game and never accepts an answer.
//! Usage: cargo run --release --example expected -- --out NEW_DIRECTORY
//! STELLARIS_PATH names the installation.
#[path = "../tests/parity/mod.rs"]
mod parity;

use std::io::Write;
use std::path::{Path, PathBuf};

fn main() -> parity::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let directory = match args.as_slice() {
        [flag, directory] if flag == "--out" => PathBuf::from(directory),
        _ => return Err("usage: expected --out NEW_DIRECTORY (set STELLARIS_PATH)".into()),
    };
    let directory = prepare_output(&directory)?;
    let installation = std::env::var_os("STELLARIS_PATH")
        .ok_or("set STELLARIS_PATH to the installation or executable")?;
    let native = pdx_native::Native::open(installation)?;
    for name in parity::FILES {
        let bytes = parity::candidate(&native, name)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))?;
        file.write_all(&bytes)?;
        eprintln!("wrote {name}");
    }
    eprintln!("Candidates only: review the diff before copying any file into tests/expected/m45.");
    Ok(())
}

/// Require a fresh directory and resolve its existing parent before any writes.
/// This rejects symlink aliases and prevents truncating an existing file or hard link.
fn prepare_output(path: &Path) -> parity::Result<PathBuf> {
    let name = path.file_name().ok_or("output must name a new directory")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize()?;
    let expected = parity::expected_directory()
        .parent()
        .unwrap()
        .canonicalize()?;
    if parent.starts_with(&expected) {
        return Err("candidate output must be outside tests/expected".into());
    }
    let output = parent.join(name);
    std::fs::create_dir(&output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_tracked_output_and_existing_directories() {
        assert!(prepare_output(&parity::expected_directory()).is_err());
        assert!(prepare_output(&parity::expected_directory().join("new-candidate")).is_err());
        let temporary = tempfile::tempdir().unwrap();
        assert!(prepare_output(temporary.path()).is_err());
        let candidate = temporary.path().join("candidate");
        assert_eq!(
            prepare_output(&candidate).unwrap(),
            candidate.canonicalize().unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_routes_into_expected() {
        let temporary = tempfile::tempdir().unwrap();
        let link = temporary.path().join("alias");
        std::os::unix::fs::symlink(parity::expected_directory(), &link).unwrap();
        assert!(prepare_output(&link.join("new-candidate")).is_err());
        assert!(prepare_output(&link).is_err());
    }
}
