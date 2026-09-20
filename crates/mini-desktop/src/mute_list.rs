//! A local mute list: DIDs whose posts, profile suggestions and directory
//! entries this device hides. It is a device-local viewing choice — nothing
//! is published, nothing is deleted from the store, and a muted author's
//! objects still replicate to others exactly as before. It is the local half
//! of blocking; delivery-side blocking needs the connection service.

use did_mini::Did;
use std::collections::BTreeSet;
use std::path::Path;

const FILE_HEADER: &str = "mininet-muted/1";
const MAX_FILE_BYTES: u64 = 256 * 1024;
pub const MAX_MUTED: usize = 4096;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MuteList {
    dids: BTreeSet<String>,
}

impl MuteList {
    pub fn contains(&self, did: &str) -> bool {
        self.dids.contains(did)
    }

    pub fn len(&self) -> usize {
        self.dids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dids.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.dids.iter().map(String::as_str)
    }

    /// Mute a DID. Returns `false` if it was already muted.
    pub fn mute(&mut self, did: &str) -> Result<bool, String> {
        let did = Did::parse(did.trim())
            .map(|parsed| parsed.as_str().to_string())
            .map_err(|error| format!("not a did:mini identifier: {error}"))?;
        if self.dids.len() >= MAX_MUTED && !self.dids.contains(&did) {
            return Err(format!("at most {MAX_MUTED} identities can be muted"));
        }
        Ok(self.dids.insert(did))
    }

    pub fn unmute(&mut self, did: &str) -> bool {
        self.dids.remove(did.trim())
    }

    pub fn encode(&self) -> String {
        let mut out = String::from(FILE_HEADER);
        out.push('\n');
        for did in &self.dids {
            out.push_str(did);
            out.push('\n');
        }
        out
    }

    /// Strict: a bad header or a non-DID line rejects the whole file, so a
    /// corrupted list never silently unmutes anyone.
    pub fn decode(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(FILE_HEADER) {
            return Err("unrecognized mute list header".into());
        }
        let mut list = Self::default();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            list.mute(line)?;
        }
        Ok(list)
    }
}

pub fn path(root: &Path) -> std::path::PathBuf {
    root.join("muted.txt")
}

/// Load the list; a missing or unreadable file yields an empty list and an
/// error string the caller can surface, so corruption is visible.
pub fn load(root: &Path) -> (MuteList, Option<String>) {
    let path = path(root);
    let Ok(metadata) = std::fs::metadata(&path) else {
        return (MuteList::default(), None);
    };
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return (
            MuteList::default(),
            Some("mute list is not a regular file of sane size".into()),
        );
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => match MuteList::decode(&text) {
            Ok(list) => (list, None),
            Err(error) => (MuteList::default(), Some(error)),
        },
        Err(error) => (MuteList::default(), Some(error.to_string())),
    }
}

pub fn save(root: &Path, list: &MuteList) -> Result<(), String> {
    crate::atomic_write_file(&path(root), list.encode().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn did(seed: u8) -> String {
        did_mini::Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32])
            .unwrap()
            .did()
            .as_str()
            .to_string()
    }

    #[test]
    fn mute_list_round_trips_and_rejects_garbage() {
        let mut list = MuteList::default();
        let a = did(1);
        let b = did(3);
        assert!(list.mute(&a).unwrap());
        assert!(!list.mute(&a).unwrap());
        assert!(list.mute(&format!("  {b} ")).unwrap());
        assert!(list.contains(&a));
        assert_eq!(list.len(), 2);
        let decoded = MuteList::decode(&list.encode()).unwrap();
        assert_eq!(decoded, list);
        assert!(list.unmute(&a));
        assert!(!list.unmute(&a));
        assert!(list.mute("not-a-did").is_err());
        assert!(MuteList::decode("other/1\n").is_err());
        assert!(MuteList::decode(&format!("{FILE_HEADER}\n{a}\nbroken\n")).is_err());
    }

    #[test]
    fn corrupt_file_is_reported_not_silently_ignored() {
        let root = std::env::temp_dir().join(format!(
            "mininet-desktop-muted-{}-{}",
            std::process::id(),
            crate::now_ms()
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(load(&root), (MuteList::default(), None));
        let mut list = MuteList::default();
        list.mute(&did(5)).unwrap();
        save(&root, &list).unwrap();
        assert_eq!(load(&root), (list, None));
        std::fs::write(path(&root), b"garbage").unwrap();
        let (loaded, error) = load(&root);
        assert!(loaded.is_empty());
        assert!(error.is_some());
        std::fs::remove_dir_all(root).unwrap();
    }
}
