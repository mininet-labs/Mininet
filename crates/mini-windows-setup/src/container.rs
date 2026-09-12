//! The payload container: a manifest plus the bytes it describes, in one
//! file.
//!
//! ## Why not a zip
//!
//! A zip would add a compression and archive-parsing dependency to the one
//! binary a user runs before they have any reason to trust us, and zip
//! parsers are a well-populated CVE category. The manifest already records
//! every file's exact length, digest, and order, so the container needs to
//! carry no structure of its own: it is the manifest followed by the file
//! bytes, concatenated in manifest order.
//!
//! That makes the format trivially auditable --- a reader can be written in
//! an afternoon in any language --- and means the container cannot disagree
//! with the manifest about structure, because it has no opinion about
//! structure. Length and digest disagreements are caught by
//! [`PackageFile::verify`](crate::PackageFile::verify) at extraction time.
//!
//! Uncompressed also means reproducible: two builds of the same files
//! produce identical container bytes, with no compression-level or
//! timestamp-in-header variance to explain away.
//!
//! ## Layout
//!
//! ```text
//! MNPKGC1\n                     8 bytes, format tag
//! <u32 be: manifest length>     4 bytes
//! <manifest bytes>              canonical MNWINPKG1 manifest
//! <file bytes>...               manifest order, lengths from the manifest
//! ```

use crate::error::SetupError;
use crate::manifest::PackageManifest;

/// Format tag; the first bytes of every container.
pub const MAGIC: &[u8; 8] = b"MNPKGC1\n";

/// Largest container this reader will consider (1 GiB).
///
/// Bounded so a hostile header cannot ask for an unbounded allocation, and
/// so a truncated download fails fast with a clear reason.
pub const MAX_CONTAINER_BYTES: u64 = 1024 * 1024 * 1024;

/// Build container bytes from a manifest and a resolver for file contents.
///
/// `contents` is called once per manifest file, in manifest order. Each
/// returned slice is verified against the manifest entry before it is
/// written, so a build script that hands back the wrong file gets an error
/// instead of shipping a package whose digests are wrong.
pub fn write<F>(manifest: &PackageManifest, mut contents: F) -> Result<Vec<u8>, SetupError>
where
    F: FnMut(&str) -> Result<Vec<u8>, SetupError>,
{
    let manifest_bytes = manifest.to_bytes();
    let mut out = Vec::with_capacity(manifest_bytes.len() + manifest.total_bytes() as usize + 12);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(manifest_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(&manifest_bytes);
    for file in &manifest.files {
        let bytes = contents(&file.path)?;
        file.verify(&bytes)?;
        out.extend_from_slice(&bytes);
    }
    Ok(out)
}

/// A container opened over borrowed bytes.
#[derive(Debug)]
pub struct Container<'a> {
    manifest: PackageManifest,
    payload: &'a [u8],
}

impl<'a> Container<'a> {
    /// Parse a container's header and manifest, and check that the payload
    /// is exactly as long as the manifest says it should be.
    ///
    /// The total-length check happens here, before any file is extracted, so
    /// a truncated or padded container is refused as a whole rather than
    /// discovered halfway through writing files to disk.
    pub fn open(bytes: &'a [u8]) -> Result<Self, SetupError> {
        let malformed = |reason: &'static str| SetupError::MalformedContainer { reason };
        if bytes.len() as u64 > MAX_CONTAINER_BYTES {
            return Err(malformed("container larger than the format limit"));
        }
        if bytes.len() < MAGIC.len() + 4 {
            return Err(malformed("shorter than a container header"));
        }
        if &bytes[..MAGIC.len()] != MAGIC {
            return Err(malformed("first bytes are not the MNPKGC1 format tag"));
        }
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&bytes[MAGIC.len()..MAGIC.len() + 4]);
        let manifest_length = u32::from_be_bytes(length_bytes) as usize;
        let manifest_start = MAGIC.len() + 4;
        let manifest_end = manifest_start
            .checked_add(manifest_length)
            .ok_or_else(|| malformed("manifest length overflows"))?;
        if manifest_end > bytes.len() {
            return Err(malformed("manifest length runs past the end of the file"));
        }
        let manifest = PackageManifest::parse(&bytes[manifest_start..manifest_end])?;
        let payload = &bytes[manifest_end..];
        if payload.len() as u64 != manifest.total_bytes() {
            return Err(malformed(
                "payload length does not match the manifest's total file size",
            ));
        }
        Ok(Self { manifest, payload })
    }

    /// The container's manifest.
    pub fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }

    /// The bytes of one manifest file, verified against its entry.
    ///
    /// Offsets are recomputed from manifest order rather than cached, so
    /// this cannot return bytes from a neighbouring file even if the
    /// manifest were somehow inconsistent about lengths.
    pub fn file(&self, path: &str) -> Result<&'a [u8], SetupError> {
        let mut offset = 0usize;
        for file in &self.manifest.files {
            let end = offset + file.length as usize;
            if file.path == path {
                let bytes = self
                    .payload
                    .get(offset..end)
                    .ok_or(SetupError::MissingFile {
                        path: path.to_string(),
                    })?;
                file.verify(bytes)?;
                return Ok(bytes);
            }
            offset = end;
        }
        Err(SetupError::MissingFile {
            path: path.to_string(),
        })
    }

    /// Verify every file in the container without writing anything.
    ///
    /// This is what `--verify` runs: it answers "is this package intact and
    /// internally consistent?" with no filesystem side effects at all.
    pub fn verify_all(&self) -> Result<(), SetupError> {
        for file in &self.manifest.files {
            self.file(&file.path)?;
        }
        Ok(())
    }
}
