//! Owner-authorized public delivery endpoints. Discovery never grants trust.
use did_mini::{verify_delegation, Kel};
use mini_app_protocol::{
    read_request, validate_operation_id, write_request, Command, DeliveryPeerView, ErrorCode,
    Request, ServiceError, MAX_DELIVERY_KEL_BYTES, MAX_DELIVERY_PEERS,
};
use std::{
    cell::Cell,
    fs,
    io::Read,
    net::SocketAddr,
    path::{Component, Path, PathBuf},
};

const MAGIC: &[u8; 8] = b"MINIDP01";
const MAX_RECORD: usize = MAX_DELIVERY_KEL_BYTES * 2 + 1024;
#[derive(Debug, Clone)]
pub(crate) struct Peer {
    pub id: String,
    pub endpoint: SocketAddr,
    pub root: Kel,
    pub device: Kel,
    pub enabled: bool,
}
impl Peer {
    pub fn view(&self) -> DeliveryPeerView {
        DeliveryPeerView {
            peer_id: self.id.clone(),
            endpoint: self.endpoint.to_string(),
            root_did: self.root.did().as_str().into(),
            device_did: self.device.did().as_str().into(),
            enabled: self.enabled,
        }
    }
}
#[derive(Debug)]
pub(crate) struct PeerBook {
    root: PathBuf,
    poisoned: Cell<bool>,
}
fn bad(message: &str) -> ServiceError {
    ServiceError::new(ErrorCode::BadRequest, message)
}
fn io(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::new(ErrorCode::Storage, error.to_string())
}
fn name(id: &str) -> String {
    format!(
        "{}.peer",
        id.bytes().map(|b| format!("{b:02x}")).collect::<String>()
    )
}
fn validate_id(id: &str) -> Result<(), ServiceError> {
    validate_operation_id(id).map_err(io)
}
fn endpoint(value: &str) -> Result<SocketAddr, ServiceError> {
    let address: SocketAddr = value
        .parse()
        .map_err(|_| bad("delivery endpoint must be numeric IP:port"))?;
    if address.port() == 0
        || address.ip().is_unspecified()
        || address.ip().is_multicast()
        || matches!(address.ip(), std::net::IpAddr::V4(ip) if ip.is_broadcast())
    {
        return Err(bad(
            "delivery endpoint must name one unicast peer with a nonzero port",
        ));
    }
    Ok(address)
}
fn decode(bytes: &[u8]) -> Result<Peer, ServiceError> {
    if bytes.len() < 9 || &bytes[..8] != MAGIC || bytes[8] > 1 {
        return Err(bad("invalid delivery peer record"));
    }
    let mut input = &bytes[9..];
    let request = read_request(&mut input)
        .map_err(io)?
        .ok_or_else(|| bad("missing peer record"))?;
    if !input.is_empty() {
        return Err(bad("trailing peer record bytes"));
    }
    let Command::AuthorizeDeliveryPeer {
        peer_id,
        endpoint: address,
        root_kel,
        device_kel,
    } = request.command
    else {
        return Err(bad("invalid peer record command"));
    };
    validate_id(&peer_id)?;
    let root = Kel::from_bytes(&root_kel).map_err(io)?;
    let device = Kel::from_bytes(&device_kel).map_err(io)?;
    root.verify().map_err(io)?;
    device.verify().map_err(io)?;
    if device.delegator() != Some(root.did()) {
        return Err(bad("peer device belongs to another root"));
    }
    let enabled = bytes[8] == 1;
    if enabled {
        verify_delegation(&root, &device).map_err(io)?;
    }
    Ok(Peer {
        id: peer_id,
        endpoint: endpoint(&address)?,
        root,
        device,
        enabled,
    })
}
fn read(path: &Path) -> Result<Vec<u8>, ServiceError> {
    let meta = fs::symlink_metadata(path).map_err(io)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_RECORD as u64 {
        return Err(bad("unsafe delivery peer file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(io)?
        .take((MAX_RECORD + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > MAX_RECORD {
        return Err(bad("delivery peer record too large"));
    }
    Ok(bytes)
}
fn extends(old: &Kel, new: &Kel) -> bool {
    old.did() == new.did()
        && old
            .verify()
            .is_ok_and(|state| new.event_digest_at(state.sn).ok() == old.head_digest().ok())
}
impl PeerBook {
    pub(crate) fn ensure_healthy(&self) -> Result<(), ServiceError> {
        if self.poisoned.get() {
            Err(ServiceError::new(
                ErrorCode::Storage,
                "peer book requires reopen after a failed durable write",
            ))
        } else {
            Ok(())
        }
    }
    pub fn open(root: PathBuf) -> Result<Self, ServiceError> {
        let root = if root.is_absolute() {
            root
        } else {
            std::env::current_dir().map_err(io)?.join(root)
        };
        let mut checked = PathBuf::new();
        for component in root.components() {
            if component == Component::ParentDir {
                return Err(bad("peer book cannot contain parent traversal"));
            }
            checked.push(component);
            match fs::symlink_metadata(&checked) {
                Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                    return Err(bad("peer book components must be real directories"))
                }
                Ok(_) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(io(error)),
            }
        }
        match fs::symlink_metadata(&root) {
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                return Err(bad("peer book must be a real directory"))
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                mini_durable::create_dir_all(&root).map_err(io)?
            }
            Err(e) => return Err(io(e)),
        }
        let book = Self {
            root,
            poisoned: Cell::new(false),
        };
        book.list()?;
        Ok(book)
    }
    pub fn list(&self) -> Result<Vec<Peer>, ServiceError> {
        self.ensure_healthy()?;
        let mut peers = Vec::new();
        let mut orphans = Vec::new();
        for (index, entry) in fs::read_dir(&self.root).map_err(io)?.enumerate() {
            if index >= usize::from(MAX_DELIVERY_PEERS) * 2 {
                return Err(bad("too many peer book entries"));
            }
            let entry = entry.map_err(io)?;
            let path = entry.path();
            let filename = entry
                .file_name()
                .into_string()
                .map_err(|_| bad("invalid peer filename"))?;
            if filename.ends_with(".peer") {
                let peer = decode(&read(&path)?)?;
                if filename != name(&peer.id) || peers.len() >= usize::from(MAX_DELIVERY_PEERS) {
                    return Err(bad("invalid peer record filename or count"));
                }
                if peers
                    .iter()
                    .any(|old: &Peer| old.root.did() == peer.root.did())
                {
                    return Err(bad("duplicate root identity in delivery peer book"));
                }
                peers.push(peer);
            } else if let Some((base, suffix)) = filename.split_once(".peer.") {
                let expected = base.len() % 2 == 0
                    && !base.is_empty()
                    && base.len() <= 192
                    && base
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
                let temporary = suffix
                    .strip_suffix(".tmp")
                    .and_then(|s| s.split_once('-'))
                    .is_some_and(|(a, b)| {
                        !a.is_empty()
                            && !b.is_empty()
                            && a.bytes().all(|x| x.is_ascii_digit())
                            && b.bytes().all(|x| x.is_ascii_digit())
                    });
                if !expected || !temporary {
                    return Err(bad("unknown peer book entry"));
                }
                read(&path)?;
                orphans.push(path);
            } else {
                return Err(bad("unknown peer book entry"));
            }
        }
        // Validate all committed records before deleting only recognized local temps.
        for path in orphans {
            fs::remove_file(&path).map_err(io)?;
            mini_durable::sync_parent(&path).map_err(io)?;
        }
        peers.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(peers)
    }
    pub fn get(&self, id: &str) -> Result<Option<Peer>, ServiceError> {
        self.ensure_healthy()?;
        validate_id(id)?;
        let path = self.root.join(name(id));
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io(e)),
            _ => (),
        }
        let peer = decode(&read(&path)?)?;
        if peer.id != id {
            return Err(bad("peer filename does not match identity"));
        }
        Ok(Some(peer))
    }
    fn write(&self, peer: &Peer) -> Result<(), ServiceError> {
        self.ensure_healthy()?;
        // A failed write may have renamed successfully but failed parent fsync.
        // Refuse all authority reads until a fresh validated reopen resolves it.
        self.poisoned.set(true);
        let path = self.root.join(name(&peer.id));
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if !meta.is_file() || meta.file_type().is_symlink() {
                return Err(bad("unsafe peer destination"));
            }
        }
        let mut bytes = MAGIC.to_vec();
        bytes.push(u8::from(peer.enabled));
        let command = Command::AuthorizeDeliveryPeer {
            peer_id: peer.id.clone(),
            endpoint: peer.endpoint.to_string(),
            root_kel: peer.root.to_bytes(),
            device_kel: peer.device.to_bytes(),
        };
        write_request(&mut bytes, &Request::new(0, command)).map_err(io)?;
        decode(&bytes)?;
        mini_durable::atomic_replace(&path, &bytes).map_err(io)?;
        self.poisoned.set(false);
        Ok(())
    }
    pub fn authorize(
        &self,
        id: &str,
        address: &str,
        root_bytes: &[u8],
        device_bytes: &[u8],
    ) -> Result<Peer, ServiceError> {
        self.ensure_healthy()?;
        validate_id(id)?;
        if root_bytes.len() > MAX_DELIVERY_KEL_BYTES {
            return Err(bad("peer KEL exceeds pairing limit"));
        }
        let root = Kel::from_bytes(root_bytes).map_err(io)?;
        root.verify().map_err(io)?;
        let old = self.get(id)?;
        if let Some(old) = &old {
            if !extends(&old.root, &root) {
                return Err(bad(
                    "peer label cannot change identity or roll back/fork trusted KEL history",
                ));
            }
            // Authentic extending root evidence can revoke the already trusted
            // device independently of any supplied device/address parameter.
            // Persist that evidence first, retaining known-good device history
            // and endpoint; invalid device input must never preserve an old grant.
            if verify_delegation(&root, &old.device).is_err() {
                let disabled = Peer {
                    id: old.id.clone(),
                    endpoint: old.endpoint,
                    root,
                    device: old.device.clone(),
                    enabled: false,
                };
                self.write(&disabled)?;
                return Ok(disabled);
            }
        }
        if device_bytes.len() > MAX_DELIVERY_KEL_BYTES {
            return Err(bad("peer KEL exceeds pairing limit"));
        }
        let device = Kel::from_bytes(device_bytes).map_err(io)?;
        device.verify().map_err(io)?;
        if device.delegator() != Some(root.did()) {
            return Err(bad("peer device belongs to another root"));
        }
        if let Some(old) = old {
            if !extends(&old.device, &device) {
                return Err(bad(
                    "peer label cannot change identity or roll back/fork trusted KEL history",
                ));
            }
        } else {
            let peers = self.list()?;
            if peers.iter().any(|peer| peer.root.did() == root.did()) {
                return Err(bad("delivery root already has a peer label; multiple devices/aliases are unsupported"));
            }
            if peers.len() >= usize::from(MAX_DELIVERY_PEERS) {
                return Err(bad("delivery peer limit reached"));
            }
        }
        // A newer authentic root that revoked this device must disable the old
        // grant, rather than leaving it active after a rejected update.
        let peer = Peer {
            id: id.into(),
            endpoint: endpoint(address)?,
            enabled: verify_delegation(&root, &device).is_ok(),
            root,
            device,
        };
        self.write(&peer)?;
        Ok(peer)
    }
    pub fn revoke(&self, id: &str) -> Result<(), ServiceError> {
        let mut peer = self.get(id)?.ok_or_else(|| bad("unknown delivery peer"))?;
        peer.enabled = false;
        self.write(&peer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!(
                    "mini-peerbook-{}-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn identity(seed: u8) -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }
    fn authorize(
        book: &PeerBook,
        id: &str,
        root: &Controller,
        device: &Controller,
    ) -> Result<Peer, ServiceError> {
        book.authorize(
            id,
            "127.0.0.1:7400",
            &root.kel().to_bytes(),
            &device.kel().to_bytes(),
        )
    }
    #[test]
    fn explicit_authorization_restart_revoke_preserve_label_identity() {
        let dir = Directory::new();
        let book = PeerBook::open(dir.0.clone()).unwrap();
        let (root, device) = identity(10);
        let (other, other_device) = identity(30);
        let peer = authorize(&book, "friend", &root, &device).unwrap();
        assert!(peer.enabled);
        assert!(authorize(&book, "friend", &other, &other_device).is_err());
        drop(book);
        let book = PeerBook::open(dir.0.clone()).unwrap();
        assert_eq!(book.get("friend").unwrap().unwrap().view(), peer.view());
        book.revoke("friend").unwrap();
        book.revoke("friend").unwrap();
        drop(book);
        let book = PeerBook::open(dir.0.clone()).unwrap();
        let disabled = book.get("friend").unwrap().unwrap();
        assert!(!disabled.enabled);
        assert_eq!(disabled.root.did(), root.did());
        assert!(book.revoke("missing").is_err());
        assert!(book.get("missing").unwrap().is_none());
    }
    #[test]
    fn kel_prefixes_are_monotonic_and_authentic_revocation_disables_durably() {
        let dir = Directory::new();
        let book = PeerBook::open(dir.0.clone()).unwrap();
        let (mut root, mut device) = identity(11);
        let (mut fork, old_device) = identity(11);
        authorize(&book, "friend", &root, &device).unwrap();
        root.rotate().unwrap();
        device.rotate().unwrap();
        authorize(&book, "friend", &root, &device).unwrap();
        assert!(authorize(&book, "friend", &fork, &old_device).is_err());
        let (_, mut fork_device) = identity(11);
        fork_device.rotate().unwrap();
        assert!(authorize(&book, "friend", &root, &fork_device).is_err());
        fork.revoke_device(&old_device.did()).unwrap();
        assert!(authorize(&book, "friend", &fork, &old_device).is_err());
        root.revoke_device(&device.did()).unwrap();
        let disabled = authorize(&book, "friend", &root, &device).unwrap();
        assert!(!disabled.enabled);
        drop(book);
        let book = PeerBook::open(dir.0.clone()).unwrap();
        assert!(!book.get("friend").unwrap().unwrap().enabled);
        assert!(authorize(&book, "friend", &fork, &old_device).is_err());
    }
    #[test]
    fn root_aliases_cannot_bypass_learned_revocation() {
        let dir = Directory::new();
        let book = PeerBook::open(dir.0.clone()).unwrap();
        let (mut root, device) = identity(14);
        let stale = root.kel().to_bytes();
        authorize(&book, "friend", &root, &device).unwrap();
        assert!(authorize(&book, "alias", &root, &device).is_err());
        root.revoke_device(&device.did()).unwrap();
        authorize(&book, "friend", &root, &device).unwrap();
        assert!(book
            .authorize("alias", "127.0.0.1:7400", &stale, &device.kel().to_bytes())
            .is_err());
        // Simulate a preexisting on-disk alias: reopening also refuses it.
        let mut duplicate = book.get("friend").unwrap().unwrap();
        duplicate.id = "alias".into();
        book.write(&duplicate).unwrap();
        assert!(PeerBook::open(dir.0.clone()).is_err());
    }
    #[test]
    fn extending_root_revocation_persists_despite_invalid_device_parameters() {
        for mode in 0..4 {
            let dir = Directory::new();
            let book = PeerBook::open(dir.0.clone()).unwrap();
            let (mut root, mut device) = identity(13);
            let (_, mut fork_device) = identity(13);
            let stale = device.kel().to_bytes();
            device.rotate().unwrap();
            fork_device.rotate().unwrap();
            authorize(&book, "friend", &root, &device).unwrap();
            root.revoke_device(&device.did()).unwrap();
            let supplied = match mode {
                0 => stale,
                1 => fork_device.kel().to_bytes(),
                2 => vec![0xff],
                _ => vec![0; MAX_DELIVERY_KEL_BYTES + 1],
            };
            let disabled = book
                .authorize(
                    "friend",
                    "invalid endpoint",
                    &root.kel().to_bytes(),
                    &supplied,
                )
                .unwrap();
            assert!(!disabled.enabled);
            assert_eq!(disabled.device.to_bytes(), device.kel().to_bytes());
            drop(book);
            let book = PeerBook::open(dir.0.clone()).unwrap();
            assert!(!book.get("friend").unwrap().unwrap().enabled);
            assert_eq!(
                book.get("friend").unwrap().unwrap().root.to_bytes(),
                root.kel().to_bytes()
            );
        }
    }
    #[test]
    fn malformed_oversized_and_missing_records_fail_closed_and_tombstones_count() {
        let dir = Directory::new();
        let book = PeerBook::open(dir.0.clone()).unwrap();
        let (root, device) = identity(12);
        assert!(book
            .authorize(
                "bad",
                "localhost:7400",
                &root.kel().to_bytes(),
                &device.kel().to_bytes()
            )
            .is_err());
        for index in 0..MAX_DELIVERY_PEERS {
            let (peer_root, peer_device) = identity(index as u8 + 60);
            authorize(&book, &format!("peer{index:02}"), &peer_root, &peer_device).unwrap();
        }
        book.revoke("peer00").unwrap();
        assert!(authorize(&book, "overflow", &root, &device).is_err());
        assert_eq!(book.list().unwrap().len(), usize::from(MAX_DELIVERY_PEERS));
        let path = dir.0.join(name("peer00"));
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"broken").unwrap();
        assert!(book.get("peer00").is_err());
        assert!(PeerBook::open(dir.0.clone()).is_err());
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_RECORD as u64 + 1).unwrap();
        assert!(book.get("peer00").is_err());
        drop(file);
        fs::write(&path, &original).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(book.get("peer00").unwrap().is_none());
        fs::remove_dir_all(&dir.0).unwrap();
        assert!(book.list().is_err());
    }
    #[cfg(unix)]
    #[test]
    fn failed_peer_write_poison_blocks_authority_until_validated_reopen() {
        use std::os::unix::fs::symlink;
        let dir = Directory::new();
        let book = PeerBook::open(dir.0.join("book")).unwrap();
        let (root, device) = identity(15);
        let peer = authorize(&book, "friend", &root, &device).unwrap();
        let path = book.root.join(name("friend"));
        let original = fs::read(&path).unwrap();
        let outside = dir.0.join("outside");
        fs::write(&outside, b"untouched").unwrap();
        fs::remove_file(&path).unwrap();
        symlink(&outside, &path).unwrap();
        assert!(book.write(&peer).is_err());
        fs::remove_file(&path).unwrap();
        fs::write(&path, original).unwrap();
        assert!(book.get("friend").is_err());
        assert!(book.list().is_err());
        assert!(authorize(&book, "friend", &root, &device).is_err());
        assert!(book.revoke("friend").is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"untouched");
        let reopened = PeerBook::open(book.root.clone()).unwrap();
        assert!(reopened.get("friend").unwrap().unwrap().enabled);
    }
    #[cfg(unix)]
    #[test]
    fn symlink_records_and_ancestor_directories_are_not_followed() {
        use std::os::unix::fs::symlink;
        let dir = Directory::new();
        let book = PeerBook::open(dir.0.join("book")).unwrap();
        let outside = dir.0.join("outside");
        fs::write(&outside, b"untouched").unwrap();
        symlink(&outside, book.root.join(name("friend"))).unwrap();
        assert!(book.get("friend").is_err());
        assert!(book.list().is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"untouched");
        symlink(&book.root, dir.0.join("link")).unwrap();
        assert!(PeerBook::open(dir.0.join("link/newbook")).is_err());
        assert!(!book.root.join("newbook").exists());
    }
}
