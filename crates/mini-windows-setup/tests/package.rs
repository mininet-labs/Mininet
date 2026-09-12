//! Package format tests: the manifest's canonical bytes and the container's
//! refusal to hand back anything it cannot account for.

use mini_windows_setup::container::{self, Container};
use mini_windows_setup::manifest::{ManifestHeader, PackageManifest, PackageShortcut};
use mini_windows_setup::PackageFile;

const DESKTOP: &[u8] = b"MZ-not-really-an-exe-desktop";
const CLI: &[u8] = b"MZ-not-really-an-exe-cli";
const README: &[u8] = b"read me\n";

fn manifest() -> PackageManifest {
    PackageManifest::new(
        ManifestHeader {
            package: "mininet-windows-client",
            version: "0.1.0",
            target: "x86_64-pc-windows-msvc",
            product: "Mininet",
            launch: "mininet-desktop.exe",
            built_at_ms: 1_757_635_200_000,
        },
        vec![
            PackageFile::describe("mininet-desktop.exe", DESKTOP).unwrap(),
            PackageFile::describe("mini.exe", CLI).unwrap(),
            PackageFile::describe("docs/README.txt", README).unwrap(),
        ],
        vec![PackageShortcut {
            target: "mininet-desktop.exe".to_string(),
            name: "Mininet".to_string(),
        }],
    )
    .unwrap()
}

fn container_bytes(manifest: &PackageManifest) -> Vec<u8> {
    container::write(manifest, |path| {
        Ok(match path {
            "mininet-desktop.exe" => DESKTOP.to_vec(),
            "mini.exe" => CLI.to_vec(),
            "docs/README.txt" => README.to_vec(),
            other => panic!("unexpected file {other}"),
        })
    })
    .unwrap()
}

#[test]
fn a_manifest_round_trips_byte_for_byte() {
    let original = manifest();
    let bytes = original.to_bytes();
    let parsed = PackageManifest::parse(&bytes).unwrap();
    assert_eq!(parsed, original);
    assert_eq!(parsed.to_bytes(), bytes);
    assert_eq!(parsed.digest_hex(), original.digest_hex());
}

#[test]
fn files_are_stored_sorted_regardless_of_the_order_they_were_described_in() {
    let text = String::from_utf8(manifest().to_bytes()).unwrap();
    let paths: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("file "))
        .map(|rest| rest.rsplit(' ').next().unwrap())
        .collect();
    assert_eq!(
        paths,
        vec!["docs/README.txt", "mini.exe", "mininet-desktop.exe"]
    );
}

#[test]
fn changing_any_single_byte_of_a_file_changes_the_package_digest() {
    let first = manifest().digest_hex();
    let mut tampered = DESKTOP.to_vec();
    tampered[3] ^= 0x01;
    let second = PackageManifest::new(
        ManifestHeader {
            package: "mininet-windows-client",
            version: "0.1.0",
            target: "x86_64-pc-windows-msvc",
            product: "Mininet",
            launch: "mininet-desktop.exe",
            built_at_ms: 1_757_635_200_000,
        },
        vec![
            PackageFile::describe("mininet-desktop.exe", &tampered).unwrap(),
            PackageFile::describe("mini.exe", CLI).unwrap(),
            PackageFile::describe("docs/README.txt", README).unwrap(),
        ],
        vec![],
    )
    .unwrap()
    .digest_hex();
    assert_ne!(first, second);
}

#[test]
fn editing_one_manifest_line_is_caught_by_the_trailing_digest() {
    let bytes = manifest().to_bytes();
    let text = String::from_utf8(bytes).unwrap();
    let edited = text.replace("version 0.1.0", "version 9.9.9");
    let error = PackageManifest::parse(edited.as_bytes()).unwrap_err();
    assert_eq!(error.code(), "manifest_digest_mismatch");
}

#[test]
fn truncating_a_manifest_is_caught_rather_than_parsed_as_a_shorter_package() {
    let text = String::from_utf8(manifest().to_bytes()).unwrap();
    let without_end: String = text
        .lines()
        .filter(|line| !line.starts_with("end "))
        .map(|line| format!("{line}\n"))
        .collect();
    let error = PackageManifest::parse(without_end.as_bytes()).unwrap_err();
    assert_eq!(error.code(), "malformed_manifest");
}

#[test]
fn an_unsorted_manifest_is_refused_rather_than_silently_normalized() {
    // Two byte strings that would otherwise be the same package: reordering
    // the file lines must not produce a manifest that parses, or the package
    // digest would stop being a unique name for one set of contents.
    let text = String::from_utf8(manifest().to_bytes()).unwrap();
    let mut lines: Vec<&str> = text.lines().collect();
    let first_file = lines.iter().position(|l| l.starts_with("file ")).unwrap();
    lines.swap(first_file, first_file + 1);
    let reordered: String = lines.iter().map(|line| format!("{line}\n")).collect();
    let error = PackageManifest::parse(reordered.as_bytes()).unwrap_err();
    // The reordering breaks the self-digest first, which is the correct and
    // strictly stronger rejection.
    assert_eq!(error.code(), "manifest_digest_mismatch");
}

#[test]
fn the_sorting_rule_is_enforced_independently_of_the_self_digest() {
    // Build the unsorted body and give it a *correct* self-digest, so the
    // only thing left to catch it is the sort check itself.
    let text = String::from_utf8(manifest().to_bytes()).unwrap();
    let mut lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.starts_with("end "))
        .collect();
    let first_file = lines.iter().position(|l| l.starts_with("file ")).unwrap();
    lines.swap(first_file, first_file + 1);
    let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
    let digest = mini_crypto::hash::blake3_256(body.as_bytes());
    let mut hex = String::new();
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    let forged = format!("{body}end {hex}\n");
    let error = PackageManifest::parse(forged.as_bytes()).unwrap_err();
    assert_eq!(error.code(), "malformed_manifest");
}

#[test]
fn a_launch_target_outside_the_file_list_is_refused() {
    let error = PackageManifest::new(
        ManifestHeader {
            package: "mininet-windows-client",
            version: "0.1.0",
            target: "x86_64-pc-windows-msvc",
            product: "Mininet",
            launch: "not-shipped.exe",
            built_at_ms: 1,
        },
        vec![PackageFile::describe("mini.exe", CLI).unwrap()],
        vec![],
    )
    .unwrap_err();
    assert_eq!(error.code(), "malformed_manifest");
}

#[test]
fn two_paths_that_are_one_file_on_windows_are_refused() {
    let error = PackageManifest::new(
        ManifestHeader {
            package: "mininet-windows-client",
            version: "0.1.0",
            target: "x86_64-pc-windows-msvc",
            product: "Mininet",
            launch: "Mini.exe",
            built_at_ms: 1,
        },
        vec![
            PackageFile::describe("Mini.exe", CLI).unwrap(),
            PackageFile::describe("mini.exe", DESKTOP).unwrap(),
        ],
        vec![],
    )
    .unwrap_err();
    assert_eq!(error.code(), "duplicate_path");
}

#[test]
fn a_shortcut_to_a_file_the_package_does_not_ship_is_refused() {
    let error = PackageManifest::new(
        ManifestHeader {
            package: "mininet-windows-client",
            version: "0.1.0",
            target: "x86_64-pc-windows-msvc",
            product: "Mininet",
            launch: "mini.exe",
            built_at_ms: 1,
        },
        vec![PackageFile::describe("mini.exe", CLI).unwrap()],
        vec![PackageShortcut {
            target: "elsewhere.exe".to_string(),
            name: "Mininet".to_string(),
        }],
    )
    .unwrap_err();
    assert_eq!(error.code(), "malformed_manifest");
}

#[test]
fn a_windows_line_ending_is_refused_so_canonical_bytes_stay_canonical() {
    let text = String::from_utf8(manifest().to_bytes()).unwrap();
    let crlf = text.replace('\n', "\r\n");
    let error = PackageManifest::parse(crlf.as_bytes()).unwrap_err();
    assert_eq!(error.code(), "malformed_manifest");
}

#[test]
fn a_path_that_escapes_the_install_root_cannot_get_into_a_manifest() {
    for hostile in [
        "../../Windows/System32/evil.dll",
        "C:/Windows/evil.dll",
        "app.exe:stream",
        "nul",
    ] {
        assert!(
            PackageFile::describe(hostile, CLI).is_err(),
            "{hostile} must be refused"
        );
    }
}

#[test]
fn a_container_round_trips_every_file_it_carries() {
    let manifest = manifest();
    let bytes = container_bytes(&manifest);
    let container = Container::open(&bytes).unwrap();
    assert_eq!(container.manifest(), &manifest);
    assert_eq!(container.file("mininet-desktop.exe").unwrap(), DESKTOP);
    assert_eq!(container.file("mini.exe").unwrap(), CLI);
    assert_eq!(container.file("docs/README.txt").unwrap(), README);
    container.verify_all().unwrap();
}

#[test]
fn a_truncated_container_is_refused_before_any_file_is_extracted() {
    let manifest = manifest();
    let bytes = container_bytes(&manifest);
    let error = Container::open(&bytes[..bytes.len() - 1]).unwrap_err();
    assert_eq!(error.code(), "malformed_container");
}

#[test]
fn a_container_with_extra_trailing_bytes_is_refused() {
    let manifest = manifest();
    let mut bytes = container_bytes(&manifest);
    bytes.push(0);
    let error = Container::open(&bytes).unwrap_err();
    assert_eq!(error.code(), "malformed_container");
}

#[test]
fn flipping_one_payload_byte_is_caught_when_that_file_is_read() {
    let manifest = manifest();
    let mut bytes = container_bytes(&manifest);
    let length = bytes.len();
    bytes[length - 1] ^= 0x01;
    let container = Container::open(&bytes).unwrap();
    // The damaged byte is in docs/README.txt, the last file in sorted order
    // only if it sorts last; assert through verify_all, which checks all.
    let error = container.verify_all().unwrap_err();
    assert_eq!(error.code(), "digest_mismatch");
}

#[test]
fn a_container_whose_header_claims_an_impossible_manifest_length_is_refused() {
    let manifest = manifest();
    let mut bytes = container_bytes(&manifest);
    bytes[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    let error = Container::open(&bytes).unwrap_err();
    assert_eq!(error.code(), "malformed_container");
}

#[test]
fn asking_a_container_for_a_file_it_does_not_have_is_an_error_not_a_guess() {
    let manifest = manifest();
    let bytes = container_bytes(&manifest);
    let container = Container::open(&bytes).unwrap();
    let error = container.file("mini-other.exe").unwrap_err();
    assert_eq!(error.code(), "missing_file");
}

#[test]
fn building_a_container_from_the_wrong_bytes_fails_at_build_time() {
    let manifest = manifest();
    let error = container::write(&manifest, |_| Ok(b"wrong".to_vec())).unwrap_err();
    assert!(matches!(
        error.code(),
        "length_mismatch" | "digest_mismatch"
    ));
}

#[test]
fn the_same_inputs_always_produce_the_same_container_bytes() {
    let first = container_bytes(&manifest());
    let second = container_bytes(&manifest());
    assert_eq!(first, second);
}
