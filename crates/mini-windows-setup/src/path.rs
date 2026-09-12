//! Windows-safe relative package paths.
//!
//! Every path inside a package manifest is untrusted input: a manifest can
//! arrive on a USB stick, over peer sync, or from a build machine nobody in
//! the room controls. The install engine writes files at those paths, so a
//! path that escapes the install root is a remote-code-execution primitive,
//! not a cosmetic problem.
//!
//! Windows makes that harder than POSIX in ways a naive `..` check misses,
//! so this module rejects rather than sanitizes. Sanitizing a hostile path
//! produces *some* path, and the caller then has no idea what it agreed to;
//! rejecting produces an error that names the reason.
//!
//! What is rejected, and why each one matters on Windows specifically:
//!
//! * `..` / `.` components, absolute paths, and leading separators --- the
//!   ordinary escape.
//! * drive-relative (`C:foo`) and drive-absolute (`C:\foo`) forms, and UNC
//!   (`\\server\share`) --- a colon anywhere is refused, which also covers
//!   NTFS alternate data streams (`file.exe:evil`), where the visible file
//!   name looks harmless and the payload hides in a stream.
//! * backslashes --- the manifest's one true separator is `/`, so a
//!   backslash means the producer and consumer disagree about structure.
//! * components with a trailing dot or space (`evil.exe.`, `evil.exe `)
//!   --- Win32 silently strips both, so two different manifest entries can
//!   land on one file, and a checked path can differ from the written one.
//! * reserved DOS device names (`CON`, `NUL`, `COM1`, ...) with or without
//!   an extension --- these do not create files; they open devices.
//! * `<>:"|?*`, control characters, and non-ASCII bytes --- the first set is
//!   illegal in Win32 file names, and restricting to printable ASCII keeps
//!   the manifest's canonical bytes free of Unicode normalization and
//!   homoglyph ambiguity, where two visually identical entries hash
//!   differently.
//! * case-insensitive duplicates are rejected by the manifest parser rather
//!   than here, since that is a property of a *set* of paths; see
//!   [`fold_case`].

use crate::error::SetupError;

/// Longest single path component a package may contain.
pub const MAX_COMPONENT_BYTES: usize = 96;

/// Longest whole relative path a package may contain.
///
/// Deliberately far below `MAX_PATH`: the install root itself
/// (`%LOCALAPPDATA%\Programs\Mininet\versions\<version>\`) already spends
/// most of a 260-character budget, and a package that only works for users
/// with short profile names is a package that fails in the field.
pub const MAX_PATH_BYTES: usize = 120;

/// Most components a package path may have.
pub const MAX_COMPONENTS: usize = 8;

/// DOS device names that never name a file, in any directory, with or
/// without an extension.
const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "conin$", "conout$", "com0", "com1", "com2", "com3", "com4",
    "com5", "com6", "com7", "com8", "com9", "lpt0", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5",
    "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Characters Win32 forbids in a file name, plus the separator this format
/// does not use.
const FORBIDDEN: &[char] = &['<', '>', ':', '"', '|', '?', '*', '\\'];

/// Check `path` as a relative, Windows-safe package path.
///
/// Returns the path unchanged on success. Nothing is normalized: a path
/// that needs normalizing is a path the producer should have written
/// correctly, and quietly rewriting it would mean the digest a reviewer
/// checked is not the path that gets written.
pub fn check(path: &str) -> Result<&str, SetupError> {
    let reject = |reason: &'static str| {
        Err(SetupError::UnsafePath {
            path: path.to_string(),
            reason,
        })
    };
    if path.is_empty() {
        return reject("empty path");
    }
    if path.len() > MAX_PATH_BYTES {
        return reject("path longer than the package limit");
    }
    if path.starts_with('/') {
        return reject("absolute path");
    }
    for ch in path.chars() {
        if FORBIDDEN.contains(&ch) {
            return reject("character Windows forbids in a file name");
        }
        if !ch.is_ascii() {
            return reject("non-ASCII byte");
        }
        if ch.is_ascii_control() || ch == '\u{7f}' {
            return reject("control character");
        }
    }
    let components: Vec<&str> = path.split('/').collect();
    if components.len() > MAX_COMPONENTS {
        return reject("more path components than the package limit");
    }
    for component in components {
        if component.is_empty() {
            return reject("empty path component");
        }
        if component.len() > MAX_COMPONENT_BYTES {
            return reject("path component longer than the package limit");
        }
        if component == "." || component == ".." {
            return reject("relative path component");
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return reject("component ending in a dot or space, which Win32 strips");
        }
        if component.starts_with(' ') {
            return reject("component starting with a space");
        }
        let stem = component
            .split_once('.')
            .map_or(component, |(before, _)| before);
        if RESERVED.contains(&stem.to_ascii_lowercase().as_str()) {
            return reject("reserved DOS device name");
        }
    }
    Ok(path)
}

/// Fold a checked path for Windows' case-insensitive comparison.
///
/// Used to detect two manifest entries that are distinct byte strings but
/// one file on disk (`Mininet.exe` and `mininet.exe`). Plain ASCII
/// lowercasing is correct here because [`check`] has already refused every
/// non-ASCII byte.
pub fn fold_case(path: &str) -> String {
    path.to_ascii_lowercase()
}

/// Join a checked package path onto a base directory.
///
/// [`check`] is re-run rather than assumed: this is the last point before a
/// real filesystem write, and a caller that constructed the path some other
/// way should not be able to skip validation by reaching this function.
pub fn join(base: &std::path::Path, path: &str) -> Result<std::path::PathBuf, SetupError> {
    let checked = check(path)?;
    let mut out = base.to_path_buf();
    for component in checked.split('/') {
        out.push(component);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_relative_paths_are_accepted() {
        for path in [
            "mininet-desktop.exe",
            "mini.exe",
            "docs/README.txt",
            "assets/fonts/Inter-Regular.ttf",
            "a",
        ] {
            assert!(check(path).is_ok(), "{path} should be accepted");
        }
    }

    #[test]
    fn every_escape_shape_windows_understands_is_rejected() {
        for path in [
            "",
            "/etc/passwd",
            "..",
            "../outside.exe",
            "nested/../../outside.exe",
            "./here.exe",
            "C:/Windows/System32/evil.exe",
            "C:evil.exe",
            "//server/share/evil.exe",
            "dir\\evil.exe",
            "app.exe:hidden",
            "nested//double.exe",
            "trailingdot.exe.",
            "trailingspace.exe ",
            " leadingspace.exe",
            "nul",
            "NUL.txt",
            "dir/CON",
            "com1.dll",
            "CONOUT$",
            "star*.exe",
            "quote\".exe",
            "pipe|.exe",
            "question?.exe",
            "less<.exe",
            "greater>.exe",
            "control\u{1}.exe",
            "unicode\u{e9}.exe",
        ] {
            assert!(check(path).is_err(), "{path:?} should be rejected");
        }
    }

    #[test]
    fn length_and_depth_limits_are_enforced() {
        let long_component = "a".repeat(MAX_COMPONENT_BYTES + 1);
        assert!(check(&long_component).is_err());
        let deep = (0..=MAX_COMPONENTS).map(|_| "d").collect::<Vec<_>>().join("/");
        assert!(check(&deep).is_err());
        let long_path = (0..4)
            .map(|_| "a".repeat(MAX_COMPONENT_BYTES))
            .collect::<Vec<_>>()
            .join("/");
        assert!(long_path.len() > MAX_PATH_BYTES);
        assert!(check(&long_path).is_err());
    }

    #[test]
    fn join_never_leaves_the_base_directory() {
        let base = std::path::Path::new("/install/root");
        let joined = join(base, "bin/mini.exe").unwrap();
        assert!(joined.starts_with(base));
        assert!(joined.ends_with("bin/mini.exe"));
        assert!(join(base, "../escape.exe").is_err());
    }

    #[test]
    fn case_folding_detects_windows_filename_collisions() {
        assert_eq!(fold_case("Mininet.exe"), fold_case("mininet.exe"));
        assert_ne!(fold_case("mini.exe"), fold_case("mini2.exe"));
    }
}
