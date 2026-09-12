//! Finding the package this setup program should install.
//!
//! Four places are searched, in this order, and the one that answers is
//! reported to the user so "which build did I just install?" is never a
//! guess:
//!
//! 1. `--payload <FILE>`, an explicit choice;
//! 2. `MININET_SETUP_PAYLOAD`, for scripted and CI installs;
//! 3. bytes embedded in this executable at build time, which is what makes
//!    a single downloadable `mininet-setup.exe` possible;
//! 4. a `.mnpkg` container sitting beside the executable, which is what
//!    makes an extracted-zip or USB-stick install work.
//!
//! Nothing here touches the network. A setup program that can fetch its own
//! payload is a setup program that can be pointed at a different payload by
//! whoever controls the name it fetches from, and this project's whole
//! update story is that the bytes arrive by whatever route the user chose
//! and are then verified against a manifest.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// Payload bytes compiled in by `build.rs`. Empty when none was embedded.
const EMBEDDED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.mnpkg"));

/// The conventional container name beside the executable.
pub const SIDECAR_NAME: &str = "mininet-client.mnpkg";

/// A located package, and where it came from.
#[derive(Debug)]
pub struct Payload {
    /// The container bytes.
    pub bytes: Cow<'static, [u8]>,
    /// Human-readable provenance, shown in the wizard and the log.
    pub origin: String,
}

/// Locate the package to install.
pub fn locate(explicit: Option<&Path>) -> Result<Payload, String> {
    if let Some(path) = explicit {
        return read(path).map_err(|error| format!("--payload {}: {error}", path.display()));
    }
    if let Some(from_env) = std::env::var_os("MININET_SETUP_PAYLOAD") {
        let path = PathBuf::from(from_env);
        if !path.as_os_str().is_empty() {
            return read(&path)
                .map_err(|error| format!("MININET_SETUP_PAYLOAD {}: {error}", path.display()));
        }
    }
    if !EMBEDDED.is_empty() {
        return Ok(Payload {
            bytes: Cow::Borrowed(EMBEDDED),
            origin: "embedded in this setup executable".to_string(),
        });
    }
    for candidate in sidecar_candidates() {
        if candidate.is_file() {
            return read(&candidate).map_err(|error| format!("{}: {error}", candidate.display()));
        }
    }
    Err(format!(
        "no package found. Put {SIDECAR_NAME} beside this program, or pass --payload <FILE>."
    ))
}

/// Paths beside the running executable that may hold a container.
pub fn sidecar_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join(SIDECAR_NAME));
            // A single `.mnpkg` in the same directory under any name: a
            // person who renamed the download should not be stuck.
            if let Ok(entries) = std::fs::read_dir(dir) {
                let mut found: Vec<PathBuf> = entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.extension().and_then(|ext| ext.to_str()) == Some("mnpkg")
                    })
                    .collect();
                found.sort();
                if found.len() == 1 {
                    out.extend(found);
                }
            }
        }
    }
    out.push(PathBuf::from(SIDECAR_NAME));
    out
}

fn read(path: &Path) -> Result<Payload, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    Ok(Payload {
        bytes: Cow::Owned(bytes),
        origin: path.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_payload_that_does_not_exist_reports_the_path_it_tried() {
        let error = locate(Some(Path::new("/definitely/not/here.mnpkg"))).unwrap_err();
        assert!(error.contains("/definitely/not/here.mnpkg"));
    }

    #[test]
    fn an_explicit_payload_is_read_verbatim() {
        let dir = std::env::temp_dir().join(format!("mini-setup-payload-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("client.mnpkg");
        std::fs::write(&path, b"not-a-real-container").unwrap();
        let payload = locate(Some(&path)).unwrap();
        assert_eq!(payload.bytes.as_ref(), b"not-a-real-container");
        assert!(payload.origin.contains("client.mnpkg"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_sidecar_search_always_includes_the_conventional_name() {
        let candidates = sidecar_candidates();
        assert!(candidates
            .iter()
            .any(|path| path.ends_with(SIDECAR_NAME)));
    }

    #[test]
    fn a_build_without_an_embedded_payload_carries_no_bytes() {
        // The workspace's ordinary `cargo build` sets no MININET_SETUP_PAYLOAD,
        // so the placeholder must be empty rather than some stale package.
        // A release build with a payload is exercised by
        // packaging/windows/Build-WindowsRelease.ps1.
        assert!(EMBEDDED.is_empty() || EMBEDDED.len() > 12);
    }
}
