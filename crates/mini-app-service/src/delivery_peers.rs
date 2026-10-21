//! Owner-authorized public delivery endpoints. Discovery never grants trust.
use did_mini::{verify_delegation, Kel};
use mini_app_protocol::{
    read_request, validate_operation_id, write_request, Command, DeliveryPeerView, ErrorCode,
    Request, ServiceError, MAX_DELIVERY_KEL_BYTES, MAX_DELIVERY_PEERS,
};
use std::{
    fs,
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
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
    pub fn open(root: PathBuf) -> Result<Self, ServiceError> {
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
        let book = Self { root };
        book.list()?;
        Ok(book)
    }
    pub fn list(&self) -> Result<Vec<Peer>, ServiceError> {
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
        mini_durable::atomic_replace(&path, &bytes).map_err(io)
    }
    pub fn authorize(
        &self,
        id: &str,
        address: &str,
        root_bytes: &[u8],
        device_bytes: &[u8],
    ) -> Result<Peer, ServiceError> {
        validate_id(id)?;
        if root_bytes.len() > MAX_DELIVERY_KEL_BYTES || device_bytes.len() > MAX_DELIVERY_KEL_BYTES
        {
            return Err(bad("peer KEL exceeds pairing limit"));
        }
        let root = Kel::from_bytes(root_bytes).map_err(io)?;
        let device = Kel::from_bytes(device_bytes).map_err(io)?;
        root.verify().map_err(io)?;
        device.verify().map_err(io)?;
        if device.delegator() != Some(root.did()) {
            return Err(bad("peer device belongs to another root"));
        }
        if let Some(old) = self.get(id)? {
            if !extends(&old.root, &root) || !extends(&old.device, &device) {
                return Err(bad(
                    "peer label cannot change identity or roll back/fork trusted KEL history",
                ));
            }
        } else if self.list()?.len() >= usize::from(MAX_DELIVERY_PEERS) {
            return Err(bad("delivery peer limit reached"));
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
