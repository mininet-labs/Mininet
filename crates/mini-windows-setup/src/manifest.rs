//! The Windows package manifest: what a release contains, and how to prove
//! the bytes on disk are those contents.
//!
//! ## Why a text format
//!
//! The manifest is the one artifact a suspicious user should be able to read
//! without running anything we shipped. It is therefore line-oriented ASCII
//! text with one fact per line, and it records a **SHA-256** digest beside
//! Mininet's own BLAKE3 one for every file --- so the check can be done with
//! `Get-FileHash` and human eyes, on a machine with no Mininet binary on it
//! at all. A format that can only be verified by the tool it ships with
//! proves nothing about that tool.
//!
//! ## Canonical bytes
//!
//! Parsing and re-emitting a manifest reproduces it byte for byte
//! ([`PackageManifest::to_bytes`]), so the manifest has a stable digest that
//! an [`crate::InstallApproval`] can name and a reviewer can quote. Files
//! are stored sorted by path, duplicates (including case-insensitive ones)
//! are rejected, and there is exactly one spelling of every field.
//!
//! ## Format
//!
//! ```text
//! MNWINPKG1
//! package mininet-windows-client
//! version 0.1.0
//! target x86_64-pc-windows-msvc
//! product Mininet
//! launch mininet-desktop.exe
//! built 1757635200000
//! file <bytes> <blake3-hex> <sha256-hex> <relative/path>
//! shortcut <relative/path> <Start Menu entry name>
//! end <blake3-hex of every preceding byte>
//! ```
//!
//! Lines are `\n`-terminated, in exactly that order; `file` lines are sorted
//! and at least one is required; `shortcut` lines are optional and each must
//! name a listed file. The path and the shortcut name come last on their
//! lines because they are the only fields that may contain a space.
//!
//! The trailing `end` line makes truncation and single-line edits detectable
//! on their own, without a signature. It is **not** a substitute for one:
//! anyone who can rewrite the file can recompute the digest. Authenticity
//! comes from `mini-forge`'s release attestations; this digest only makes
//! *accidental* corruption and careless tampering loud.

use crate::error::SetupError;
use crate::path;
use mini_crypto::hash::{blake3_256, sha2_256};
use mini_forge::Version;

/// Format tag; the first line of every manifest.
pub const MAGIC: &str = "MNWINPKG1";

/// Most files one package may contain.
///
/// A Windows client build is a handful of executables and assets. A bound
/// keeps a hostile manifest from turning into an allocation attack, and a
/// package that needs more than this is a package that should ship an
/// archive as one file.
pub const MAX_FILES: usize = 512;

/// Most shortcuts one package may request.
pub const MAX_SHORTCUTS: usize = 8;

/// Largest single file a package may contain (512 MiB).
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// Largest whole manifest, in bytes, this parser will consider.
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;

/// Longest display text (product name, shortcut name).
pub const MAX_DISPLAY_BYTES: usize = 64;

/// One file in a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFile {
    /// Relative install path, checked by [`crate::path::check`].
    pub path: String,
    /// Exact length in bytes.
    pub length: u64,
    /// BLAKE3-256 of the file's bytes: Mininet's content address.
    pub blake3: [u8; 32],
    /// SHA-256 of the same bytes, for independent verification with
    /// `Get-FileHash` or `certutil` on a machine that trusts nothing here.
    pub sha256: [u8; 32],
}

impl PackageFile {
    /// Describe `bytes` as a package file at `path`.
    pub fn describe(path: &str, bytes: &[u8]) -> Result<Self, SetupError> {
        path::check(path)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(SetupError::LengthMismatch {
                path: path.to_string(),
                expected: MAX_FILE_BYTES,
                found: bytes.len() as u64,
            });
        }
        Ok(Self {
            path: path.to_string(),
            length: bytes.len() as u64,
            blake3: blake3_256(bytes),
            sha256: sha2_256(bytes),
        })
    }

    /// Check `bytes` against this entry's recorded length and digests.
    ///
    /// Length is checked first so a truncated file reports the honest
    /// reason instead of a bare digest mismatch. Both digests must match:
    /// agreeing with one and not the other means the manifest itself is
    /// inconsistent, which is never something to install through.
    pub fn verify(&self, bytes: &[u8]) -> Result<(), SetupError> {
        if bytes.len() as u64 != self.length {
            return Err(SetupError::LengthMismatch {
                path: self.path.clone(),
                expected: self.length,
                found: bytes.len() as u64,
            });
        }
        if blake3_256(bytes) != self.blake3 || sha2_256(bytes) != self.sha256 {
            return Err(SetupError::DigestMismatch {
                path: self.path.clone(),
            });
        }
        Ok(())
    }
}

/// A Start Menu entry the package asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageShortcut {
    /// Which installed file it launches.
    pub target: String,
    /// The name the user sees in the Start Menu.
    pub name: String,
}

/// A parsed, validated Windows package manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManifest {
    /// Stable package identifier (`mininet-windows-client`).
    pub package: String,
    /// Version as written, preserved so canonical bytes round-trip.
    pub version_text: String,
    /// The same version, parsed for ordering and rollback checks.
    pub version: Version,
    /// Rust target triple this package was built for.
    pub target: String,
    /// Product name shown in the Start Menu and in Apps & features.
    pub product: String,
    /// Which file the Start Menu entry and `status` treat as the client.
    pub launch: String,
    /// Build timestamp, milliseconds since the Unix epoch.
    pub built_at_ms: u64,
    /// Files, sorted by path, at least one.
    pub files: Vec<PackageFile>,
    /// Requested Start Menu entries.
    pub shortcuts: Vec<PackageShortcut>,
}

/// The singleton fields at the head of a manifest.
///
/// Grouped into one type so building a manifest reads as "this header, these
/// files" rather than as eight positional arguments where two adjacent
/// `&str` parameters can be swapped without the compiler noticing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestHeader<'a> {
    /// Stable package identifier (`mininet-windows-client`).
    pub package: &'a str,
    /// Dotted-numeric version.
    pub version: &'a str,
    /// Rust target triple.
    pub target: &'a str,
    /// Product name shown to the user.
    pub product: &'a str,
    /// Relative path of the client executable.
    pub launch: &'a str,
    /// Build timestamp, milliseconds since the Unix epoch.
    pub built_at_ms: u64,
}

impl PackageManifest {
    /// Build a manifest from described files, sorting and validating them.
    pub fn new(
        header: ManifestHeader<'_>,
        mut files: Vec<PackageFile>,
        shortcuts: Vec<PackageShortcut>,
    ) -> Result<Self, SetupError> {
        let ManifestHeader {
            package,
            version: version_text,
            target,
            product,
            launch,
            built_at_ms,
        } = header;
        check_token("package", package)?;
        check_token("target", target)?;
        check_display("product", product)?;
        path::check(launch)?;
        let version = Version::parse(version_text).map_err(|_| SetupError::BadVersion {
            value: version_text.to_string(),
        })?;
        if files.is_empty() {
            return Err(SetupError::MalformedManifest {
                line: 0,
                reason: "a package must contain at least one file",
            });
        }
        if files.len() > MAX_FILES {
            return Err(SetupError::MalformedManifest {
                line: 0,
                reason: "more files than the package limit",
            });
        }
        if shortcuts.len() > MAX_SHORTCUTS {
            return Err(SetupError::MalformedManifest {
                line: 0,
                reason: "more shortcuts than the package limit",
            });
        }
        for file in &files {
            path::check(&file.path)?;
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let mut seen = std::collections::BTreeSet::new();
        for file in &files {
            if !seen.insert(path::fold_case(&file.path)) {
                return Err(SetupError::DuplicatePath {
                    path: file.path.clone(),
                });
            }
        }
        if !seen.contains(&path::fold_case(launch)) {
            return Err(SetupError::MalformedManifest {
                line: 0,
                reason: "launch target is not one of the package files",
            });
        }
        for shortcut in &shortcuts {
            path::check(&shortcut.target)?;
            check_display("shortcut", &shortcut.name)?;
            if !seen.contains(&path::fold_case(&shortcut.target)) {
                return Err(SetupError::MalformedManifest {
                    line: 0,
                    reason: "shortcut target is not one of the package files",
                });
            }
        }
        Ok(Self {
            package: package.to_string(),
            version_text: version_text.to_string(),
            version,
            target: target.to_string(),
            product: product.to_string(),
            launch: launch.to_string(),
            built_at_ms,
            files,
            shortcuts,
        })
    }

    /// Total installed size in bytes, for the wizard's disk-space line.
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.length).sum()
    }

    /// Look up a file entry by exact path.
    pub fn file(&self, path: &str) -> Option<&PackageFile> {
        self.files.iter().find(|file| file.path == path)
    }

    /// The canonical bytes of this manifest, including the trailing digest.
    pub fn to_bytes(&self) -> Vec<u8> {
        let body = self.body();
        let mut out = body.clone();
        out.extend_from_slice(format!("end {}\n", hex(&blake3_256(&body))).as_bytes());
        out
    }

    /// BLAKE3 digest of this manifest's canonical bytes.
    ///
    /// This is the package's identity for approval purposes: an
    /// [`crate::InstallApproval`] names this value, so approving an install
    /// approves one exact set of files with one exact set of digests.
    pub fn digest(&self) -> [u8; 32] {
        blake3_256(&self.to_bytes())
    }

    /// The package digest as lowercase hex.
    pub fn digest_hex(&self) -> String {
        hex(&self.digest())
    }

    fn body(&self) -> Vec<u8> {
        let mut text = String::new();
        text.push_str(MAGIC);
        text.push('\n');
        text.push_str(&format!("package {}\n", self.package));
        text.push_str(&format!("version {}\n", self.version_text));
        text.push_str(&format!("target {}\n", self.target));
        text.push_str(&format!("product {}\n", self.product));
        text.push_str(&format!("launch {}\n", self.launch));
        text.push_str(&format!("built {}\n", self.built_at_ms));
        for file in &self.files {
            text.push_str(&format!(
                "file {} {} {} {}\n",
                file.length,
                hex(&file.blake3),
                hex(&file.sha256),
                file.path
            ));
        }
        for shortcut in &self.shortcuts {
            // Length-prefixed target, because *both* fields may contain a
            // space: package paths allow them and display names allow them.
            // Splitting on the first space would make this writer produce
            // manifests its own parser rejects.
            text.push_str(&format!(
                "shortcut {} {} {}\n",
                shortcut.target.len(),
                shortcut.target,
                shortcut.name
            ));
        }
        text.into_bytes()
    }

    /// Parse canonical manifest bytes.
    ///
    /// Strict on purpose: unknown lines, reordered fields, a missing or
    /// wrong `end` digest, unsorted or duplicate files, and a launch target
    /// that is not in the file list are all refused. An installer that
    /// guesses at a malformed manifest is an installer whose behaviour
    /// nobody can predict from the file they reviewed.
    pub fn parse(bytes: &[u8]) -> Result<Self, SetupError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(SetupError::MalformedManifest {
                line: 0,
                reason: "manifest larger than the format limit",
            });
        }
        let text = core::str::from_utf8(bytes).map_err(|_| SetupError::MalformedManifest {
            line: 0,
            reason: "manifest is not UTF-8",
        })?;
        if !text.ends_with('\n') {
            return Err(SetupError::MalformedManifest {
                line: 0,
                reason: "manifest does not end with a newline",
            });
        }
        if text.contains('\r') {
            return Err(SetupError::MalformedManifest {
                line: 0,
                reason: "manifest contains a carriage return; lines end with \\n only",
            });
        }
        let lines: Vec<&str> = text
            .strip_suffix('\n')
            .unwrap_or(text)
            .split('\n')
            .collect();
        // MAGIC + 6 singletons + >=1 file + end
        if lines.len() < 9 {
            return Err(SetupError::MalformedManifest {
                line: 0,
                reason: "manifest has too few lines to be complete",
            });
        }
        if lines[0] != MAGIC {
            return Err(SetupError::MalformedManifest {
                line: 1,
                reason: "first line is not the MNWINPKG1 format tag",
            });
        }
        let field = |index: usize, name: &'static str| -> Result<&str, SetupError> {
            lines[index]
                .strip_prefix(name)
                .and_then(|rest| rest.strip_prefix(' '))
                .filter(|value| !value.is_empty())
                .ok_or(SetupError::MalformedManifest {
                    line: index + 1,
                    reason: "expected a different field here",
                })
        };
        let package = field(1, "package")?;
        let version_text = field(2, "version")?;
        let target = field(3, "target")?;
        let product = field(4, "product")?;
        let launch = field(5, "launch")?;
        let built_text = field(6, "built")?;
        let built_at_ms: u64 = built_text
            .parse()
            .map_err(|_| SetupError::MalformedManifest {
                line: 7,
                reason: "build timestamp is not a u64 millisecond value",
            })?;

        let mut files = Vec::new();
        let mut shortcuts = Vec::new();
        let mut index = 7;
        while index < lines.len() {
            let line = lines[index];
            let number = index + 1;
            if let Some(rest) = line.strip_prefix("file ") {
                if !shortcuts.is_empty() {
                    return Err(SetupError::MalformedManifest {
                        line: number,
                        reason: "file line after a shortcut line",
                    });
                }
                files.push(parse_file_line(rest, number)?);
            } else if let Some(rest) = line.strip_prefix("shortcut ") {
                shortcuts.push(parse_shortcut_line(rest, number)?);
            } else if let Some(rest) = line.strip_prefix("end ") {
                if index + 1 != lines.len() {
                    return Err(SetupError::MalformedManifest {
                        line: number,
                        reason: "content after the end line",
                    });
                }
                let body_len = bytes.len() - (line.len() + 1);
                let expected = unhex(rest).ok_or(SetupError::MalformedManifest {
                    line: number,
                    reason: "end digest is not 32 hex bytes",
                })?;
                if blake3_256(&bytes[..body_len]) != expected {
                    return Err(SetupError::ManifestDigestMismatch);
                }
                // `new` re-validates everything and re-sorts; compare against
                // the parsed order so an unsorted manifest is rejected
                // rather than silently normalized, which would give two
                // byte strings the same package identity.
                let parsed_order: Vec<String> =
                    files.iter().map(|file| file.path.clone()).collect();
                let manifest = Self::new(
                    ManifestHeader {
                        package,
                        version: version_text,
                        target,
                        product,
                        launch,
                        built_at_ms,
                    },
                    files,
                    shortcuts,
                )?;
                let canonical_order: Vec<String> = manifest
                    .files
                    .iter()
                    .map(|file| file.path.clone())
                    .collect();
                if parsed_order != canonical_order {
                    return Err(SetupError::MalformedManifest {
                        line: 0,
                        reason: "file lines are not sorted by path",
                    });
                }
                return Ok(manifest);
            } else {
                return Err(SetupError::MalformedManifest {
                    line: number,
                    reason: "unrecognized line",
                });
            }
            index += 1;
        }
        Err(SetupError::MalformedManifest {
            line: 0,
            reason: "manifest has no end line",
        })
    }
}

fn parse_file_line(rest: &str, line: usize) -> Result<PackageFile, SetupError> {
    let malformed = |reason: &'static str| SetupError::MalformedManifest { line, reason };
    let mut parts = rest.splitn(4, ' ');
    let length_text = parts.next().ok_or(malformed("file line is empty"))?;
    let blake3_text = parts.next().ok_or(malformed("file line has no blake3"))?;
    let sha256_text = parts.next().ok_or(malformed("file line has no sha256"))?;
    let path_text = parts.next().ok_or(malformed("file line has no path"))?;
    let length: u64 = length_text
        .parse()
        .map_err(|_| malformed("file length is not a u64"))?;
    if length > MAX_FILE_BYTES {
        return Err(malformed("file longer than the package limit"));
    }
    let blake3 = unhex(blake3_text).ok_or(malformed("blake3 is not 32 hex bytes"))?;
    let sha256 = unhex(sha256_text).ok_or(malformed("sha256 is not 32 hex bytes"))?;
    path::check(path_text)?;
    Ok(PackageFile {
        path: path_text.to_string(),
        length,
        blake3,
        sha256,
    })
}

fn parse_shortcut_line(rest: &str, line: usize) -> Result<PackageShortcut, SetupError> {
    let malformed = |reason: &'static str| SetupError::MalformedManifest { line, reason };
    // `shortcut <target-byte-length> <target> <name>`: the length makes the
    // boundary unambiguous when either field contains a space.
    let (length_text, after_length) = rest
        .split_once(' ')
        .ok_or(malformed("shortcut line has no target length"))?;
    let length: usize = length_text
        .parse()
        .map_err(|_| malformed("shortcut target length is not a number"))?;
    if after_length.len() < length + 1 {
        return Err(malformed("shortcut target length runs past the line"));
    }
    let (target, remainder) = after_length.split_at(length);
    let name = remainder
        .strip_prefix(' ')
        .ok_or(malformed("shortcut target length does not end at a space"))?;
    path::check(target)?;
    check_display("shortcut", name)?;
    Ok(PackageShortcut {
        target: target.to_string(),
        name: name.to_string(),
    })
}

/// A bare identifier field: printable ASCII, no spaces.
fn check_token(field: &'static str, value: &str) -> Result<(), SetupError> {
    if value.is_empty() || value.len() > MAX_DISPLAY_BYTES {
        return Err(SetupError::UnsafeDisplayText {
            field,
            reason: "empty or longer than the format limit",
        });
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
    {
        return Err(SetupError::UnsafeDisplayText {
            field,
            reason: "only ASCII letters, digits, '-', '_', and '.' are allowed",
        });
    }
    Ok(())
}

/// Text shown to a person: a product name or a Start Menu entry.
///
/// Restricted to printable ASCII without the quote and backtick characters,
/// because this text is later interpolated into a generated PowerShell
/// script and a registry value. The restriction is enforced here, at parse
/// time, rather than escaped later at use time: a value that cannot be
/// dangerous is a stronger guarantee than a value that is escaped correctly
/// by every one of its callers.
pub(crate) fn check_display(field: &'static str, value: &str) -> Result<(), SetupError> {
    if value.is_empty() || value.len() > MAX_DISPLAY_BYTES {
        return Err(SetupError::UnsafeDisplayText {
            field,
            reason: "empty or longer than the format limit",
        });
    }
    for ch in value.chars() {
        if !ch.is_ascii() || ch.is_ascii_control() {
            return Err(SetupError::UnsafeDisplayText {
                field,
                reason: "only printable ASCII is allowed",
            });
        }
        if matches!(
            ch,
            '\'' | '"' | '`' | '$' | '%' | '&' | '|' | '<' | '>' | '^'
        ) {
            return Err(SetupError::UnsafeDisplayText {
                field,
                reason: "shell and script metacharacters are not allowed",
            });
        }
    }
    if value.starts_with(' ') || value.ends_with(' ') {
        return Err(SetupError::UnsafeDisplayText {
            field,
            reason: "leading or trailing space",
        });
    }
    Ok(())
}

/// Lowercase hex of a 32-byte digest.
pub fn hex(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Parse exactly 64 lowercase hex characters into a 32-byte digest.
pub fn unhex(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let mut out = [0u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(out)
}
