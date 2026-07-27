//! A real, on-disk [`ClaimedRegistry`] -- an append-only log of
//! `(identity root, claimed-at)` records, fsynced on every
//! [`FileClaimedRegistry::mark_claimed`] call so a crash immediately after
//! this crate reports a successful claim still leaves that claim durably
//! recorded before whatever caller is about to trigger a real payout ever
//! sees the outcome.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

use did_mini::Did;

use crate::error::{AirdropError, Result};
use crate::registry::ClaimedRegistry;

const RECORD_DOMAIN: &[u8] = b"mini-airdrop/claimed-registry-record/v1";
/// Defensive bound on how large a single scid this reader accepts from one
/// record before giving up on the rest of the file -- the same
/// defensive-decoding discipline (ID5) every bounded read in this
/// workspace applies.
const MAX_SCID_BYTES: usize = 4_096;

/// A [`ClaimedRegistry`] backed by a real append-only file on disk.
#[derive(Debug)]
pub struct FileClaimedRegistry {
    path: PathBuf,
    claimed: HashMap<Did, u64>,
}

impl FileClaimedRegistry {
    /// Open (or create) the registry file at `path`, replaying every
    /// well-formed record already on disk.
    ///
    /// A truncated or corrupt final record (e.g. from a crash mid-write)
    /// is silently stopped at rather than rejected outright -- every
    /// record *before* it stays trusted, the same recovery discipline
    /// `mini-forge`'s release transparency log already uses for its own
    /// append-only history.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let mut claimed = HashMap::new();
        if path.exists() {
            let mut buf = Vec::new();
            File::open(&path)
                .and_then(|mut f| f.read_to_end(&mut buf))
                .map_err(|e| AirdropError::RegistryWriteFailed(e.to_string()))?;
            let mut offset = 0;
            while offset < buf.len() {
                match Self::decode_record(&buf[offset..]) {
                    Some((did, at_ms, consumed)) => {
                        claimed.insert(did, at_ms);
                        offset += consumed;
                    }
                    None => break,
                }
            }
        }
        Ok(FileClaimedRegistry { path, claimed })
    }

    /// How many claims are currently on record.
    pub fn len(&self) -> usize {
        self.claimed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.claimed.is_empty()
    }

    fn decode_record(buf: &[u8]) -> Option<(Did, u64, usize)> {
        let mut pos = 0;
        let tag = buf.get(pos..pos + RECORD_DOMAIN.len())?;
        if tag != RECORD_DOMAIN {
            return None;
        }
        pos += RECORD_DOMAIN.len();
        let len_bytes: [u8; 4] = buf.get(pos..pos + 4)?.try_into().ok()?;
        let scid_len = u32::from_be_bytes(len_bytes) as usize;
        pos += 4;
        if scid_len > MAX_SCID_BYTES {
            return None;
        }
        let scid_bytes = buf.get(pos..pos + scid_len)?;
        let scid = std::str::from_utf8(scid_bytes).ok()?;
        pos += scid_len;
        let at_bytes: [u8; 8] = buf.get(pos..pos + 8)?.try_into().ok()?;
        let at_ms = u64::from_be_bytes(at_bytes);
        pos += 8;
        let did = Did::from_scid(scid).ok()?;
        Some((did, at_ms, pos))
    }

    fn encode_record(did: &Did, at_ms: u64) -> Vec<u8> {
        let scid_bytes = did.scid().as_bytes();
        let mut out = Vec::with_capacity(RECORD_DOMAIN.len() + 4 + scid_bytes.len() + 8);
        out.extend_from_slice(RECORD_DOMAIN);
        out.extend_from_slice(&(scid_bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(scid_bytes);
        out.extend_from_slice(&at_ms.to_be_bytes());
        out
    }

    /// Append one record and fsync before returning, so a crash right
    /// after this call still leaves the claim durably recorded.
    fn append(&self, did: &Did, at_ms: u64) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(&Self::encode_record(did, at_ms))?;
        file.sync_all()
    }
}

impl ClaimedRegistry for FileClaimedRegistry {
    fn already_claimed(&self, identity_root: &Did) -> bool {
        self.claimed.contains_key(identity_root)
    }

    fn mark_claimed(&mut self, identity_root: &Did, at_ms: u64) -> Result<()> {
        self.append(identity_root, at_ms)
            .map_err(|e| AirdropError::RegistryWriteFailed(e.to_string()))?;
        self.claimed.insert(identity_root.clone(), at_ms);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn root() -> Did {
        Controller::incept_single().unwrap().did()
    }

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "mini-airdrop-test-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    #[test]
    fn a_fresh_file_registry_has_no_claims() {
        let path = temp_path("fresh");
        let registry = FileClaimedRegistry::open(&path).unwrap();
        assert!(registry.is_empty());
        assert!(!registry.already_claimed(&root()));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_marked_claim_persists_across_reopening_the_same_file() {
        let path = temp_path("reopen");
        let r = root();

        {
            let mut registry = FileClaimedRegistry::open(&path).unwrap();
            registry.mark_claimed(&r, 1_000).unwrap();
        }

        let reopened = FileClaimedRegistry::open(&path).unwrap();
        assert!(reopened.already_claimed(&r));
        assert_eq!(reopened.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn multiple_claims_all_survive_a_reopen() {
        let path = temp_path("multi");
        let a = root();
        let b = root();

        {
            let mut registry = FileClaimedRegistry::open(&path).unwrap();
            registry.mark_claimed(&a, 100).unwrap();
            registry.mark_claimed(&b, 200).unwrap();
        }

        let reopened = FileClaimedRegistry::open(&path).unwrap();
        assert!(reopened.already_claimed(&a));
        assert!(reopened.already_claimed(&b));
        assert_eq!(reopened.len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_truncated_trailing_record_is_tolerated_and_earlier_records_still_load() {
        let path = temp_path("truncated");
        let a = root();
        let b = root();

        {
            let mut registry = FileClaimedRegistry::open(&path).unwrap();
            registry.mark_claimed(&a, 100).unwrap();
            registry.mark_claimed(&b, 200).unwrap();
        }

        // Simulate a crash mid-write: chop the last few bytes off the file.
        let mut bytes = std::fs::read(&path).unwrap();
        let cut = bytes.len() - 3;
        bytes.truncate(cut);
        std::fs::write(&path, &bytes).unwrap();

        let reopened = FileClaimedRegistry::open(&path).unwrap();
        assert!(reopened.already_claimed(&a));
        assert!(!reopened.already_claimed(&b));
        assert_eq!(reopened.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn two_registry_instances_over_the_same_file_agree_after_reopening() {
        let path = temp_path("agree");
        let r = root();

        let mut first = FileClaimedRegistry::open(&path).unwrap();
        first.mark_claimed(&r, 500).unwrap();

        // A second instance opened fresh from the same path sees the claim
        // -- it does not share the first instance's in-memory state, only
        // the file.
        let second = FileClaimedRegistry::open(&path).unwrap();
        assert!(second.already_claimed(&r));
        let _ = std::fs::remove_file(&path);
    }
}
