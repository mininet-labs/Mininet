//! Client composition is checked independently of hashes and review receipts.
use mini_windows_setup::manifest::ManifestHeader;
use mini_windows_setup::{PackageFile, PackageManifest};
fn bundle(
    package: &str,
    launch: &str,
    service: Option<(&str, &[u8])>,
) -> Result<PackageManifest, mini_windows_setup::SetupError> {
    let mut files = vec![PackageFile::describe(launch, b"desktop fixture").unwrap()];
    if let Some((path, bytes)) = service {
        files.push(PackageFile::describe(path, bytes).unwrap());
    }
    PackageManifest::new(
        ManifestHeader {
            package,
            version: "0.1.0",
            target: "x86_64-pc-windows-msvc",
            product: "Fixture",
            launch,
            built_at_ms: 1,
        },
        files,
        vec![],
    )
}
#[test]
fn desktop_client_requires_nonempty_sibling_even_for_generic_id_or_nested_launch() {
    assert!(bundle("mininet-windows-client", "client.exe", None).is_err());
    assert!(bundle("generic", "MININET-DESKTOP.EXE", None).is_err());
    assert!(bundle(
        "generic",
        "nested/mininet-desktop.exe",
        Some(("mininet-app-service.exe", b"core"))
    )
    .is_err());
    assert!(bundle(
        "generic",
        "nested/mininet-desktop.exe",
        Some(("other/mininet-app-service.exe", b"core"))
    )
    .is_err());
    assert!(bundle(
        "generic",
        "mininet-desktop.exe",
        Some(("mininet-app-service.exe", b""))
    )
    .is_err());
    let valid = bundle(
        "generic",
        "Nested/MININET-DESKTOP.EXE",
        Some(("nEsTeD/MININET-APP-SERVICE.ExE", b"core")),
    )
    .unwrap();
    valid.validate_client_bundle().unwrap();
    assert_eq!(PackageManifest::parse(&valid.to_bytes()).unwrap(), valid);
    assert!(bundle("generic", "tool.exe", None).is_ok());
}
#[test]
fn mutable_manifest_cannot_bypass_parse_or_container_composition_check() {
    let mut manifest = bundle(
        "mininet-windows-client",
        "mininet-desktop.exe",
        Some(("mininet-app-service.exe", b"core")),
    )
    .unwrap();
    manifest
        .files
        .retain(|file| file.path != "mininet-app-service.exe");
    assert!(manifest.validate_client_bundle().is_err());
    assert!(PackageManifest::parse(&manifest.to_bytes()).is_err());
    assert!(mini_windows_setup::container::write(&manifest, |_| panic!(
        "invalid bundle read payload bytes"
    ))
    .is_err());
}
