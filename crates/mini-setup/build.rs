//! Optionally embed a package container into the setup executable.
//!
//! `packaging/windows/Build-WindowsRelease.ps1` builds the client, writes a
//! `.mnpkg` container, and then rebuilds this binary with
//! `MININET_SETUP_PAYLOAD` pointing at it. That produces the single file a
//! person can be handed: `mininet-setup.exe`, with the package inside it.
//!
//! Without that variable the binary still builds and still works --- it then
//! looks for a container beside itself or at `--payload`. Two shapes, one
//! code path: the embedded bytes and a file's bytes are the same
//! `Container` either way.
//!
//! The payload is *not* compiled in as a Rust literal or an `include_str!`
//! of anything parsed at compile time; it is copied verbatim into `OUT_DIR`
//! and included as bytes, so a build cannot change a single byte of the
//! package it was handed.

fn main() {
    println!("cargo:rerun-if-env-changed=MININET_SETUP_PAYLOAD");
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let destination = std::path::Path::new(&out_dir).join("payload.mnpkg");
    match std::env::var_os("MININET_SETUP_PAYLOAD") {
        Some(source) if !source.is_empty() => {
            let source = std::path::PathBuf::from(source);
            println!("cargo:rerun-if-changed={}", source.display());
            let bytes = std::fs::read(&source).unwrap_or_else(|error| {
                panic!(
                    "MININET_SETUP_PAYLOAD points at {} which could not be read: {error}",
                    source.display()
                )
            });
            std::fs::write(&destination, bytes).expect("writing the embedded payload");
        }
        // An empty file means "no embedded payload"; a zero-length container
        // can never parse, so there is no ambiguity with a real one.
        _ => std::fs::write(&destination, []).expect("writing the empty payload placeholder"),
    }
}
