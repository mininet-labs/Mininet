//! Saved peers, the owner's connection policy, and shareable connection
//! cards. Everything here is plain local state the owner edits; none of it
//! is a trust root. A saved endpoint is a dial hint, a card's DID is the
//! identity to follow once its signed profile has actually been received.

use crate::network_session::{validate_endpoint, SessionLength, MAX_SESSION_PEERS};
use did_mini::Did;
use std::path::Path;

const FILE_HEADER: &str = "mininet-connections/1";
const MAX_LABEL_BYTES: usize = 64;
const MAX_FILE_BYTES: u64 = 64 * 1024;
pub const DEFAULT_PORT: u16 = 46000;

/// One peer the owner chose to keep exchanging with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerEntry {
    /// Owner-chosen name for the endpoint; not the peer's claimed name.
    pub label: String,
    /// `host:port` or `[v6]:port`.
    pub endpoint: String,
    /// The human DID the owner expects behind this endpoint, if a card
    /// carried one. Never treated as verified by itself.
    pub did: Option<String>,
}

/// Persisted connection preferences. Every network-starting flag defaults
/// to off; the owner enables each one deliberately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionSettings {
    pub listen_port: u16,
    pub session_length: SessionLength,
    /// Start a session with the saved peers when Mininet launches.
    pub session_on_launch: bool,
    /// Accept incoming connections when Mininet launches.
    pub host_on_launch: bool,
    /// With `host_on_launch`, also ask the router to forward the port.
    pub router_mapping_on_launch: bool,
    /// Sessions also exchange the owner's private conversation routes.
    pub include_private: bool,
    /// Micro-MINI per MB this device asks for what it serves.
    pub rate_micro_per_mb: u64,
    /// The most this device agrees to pay per MB it receives.
    pub max_pay_micro_per_mb: u64,
    /// Creator share of media bytes, basis points (0..=10000).
    pub creator_bps: u16,
    pub peers: Vec<PeerEntry>,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            listen_port: DEFAULT_PORT,
            session_length: SessionLength::Short,
            session_on_launch: false,
            host_on_launch: false,
            router_mapping_on_launch: false,
            include_private: false,
            rate_micro_per_mb: 10,
            max_pay_micro_per_mb: 50,
            creator_bps: 3000,
            peers: Vec::new(),
        }
    }
}

fn clean_label(label: &str) -> Result<String, String> {
    let label: String = label.trim().chars().filter(|c| !c.is_control()).collect();
    if label.is_empty() {
        return Err("Give the peer a short name.".into());
    }
    if label.len() > MAX_LABEL_BYTES {
        return Err(format!(
            "Peer names are limited to {MAX_LABEL_BYTES} bytes."
        ));
    }
    Ok(label)
}

impl ConnectionSettings {
    /// Add or update a saved peer keyed by endpoint. Returns whether a new
    /// entry was created.
    pub fn upsert_peer(
        &mut self,
        label: &str,
        endpoint: &str,
        did: Option<&str>,
    ) -> Result<bool, String> {
        let label = clean_label(label)?;
        let endpoint = validate_endpoint(endpoint)?;
        let did = match did.map(str::trim).filter(|did| !did.is_empty()) {
            Some(did) => Some(
                Did::parse(did)
                    .map(|parsed| parsed.as_str().to_string())
                    .map_err(|error| format!("The card's DID is malformed: {error}"))?,
            ),
            None => None,
        };
        if let Some(existing) = self.peers.iter_mut().find(|peer| peer.endpoint == endpoint) {
            existing.label = label;
            if did.is_some() {
                existing.did = did;
            }
            return Ok(false);
        }
        if self.peers.len() >= MAX_SESSION_PEERS {
            return Err(format!(
                "At most {MAX_SESSION_PEERS} peers can be saved. Remove one first."
            ));
        }
        self.peers.push(PeerEntry {
            label,
            endpoint,
            did,
        });
        Ok(true)
    }

    pub fn remove_peer(&mut self, endpoint: &str) -> bool {
        let before = self.peers.len();
        self.peers.retain(|peer| peer.endpoint != endpoint);
        before != self.peers.len()
    }

    pub fn endpoints(&self) -> Vec<String> {
        self.peers
            .iter()
            .map(|peer| peer.endpoint.clone())
            .collect()
    }

    pub fn encode(&self) -> String {
        let mut out = String::new();
        out.push_str(FILE_HEADER);
        out.push('\n');
        out.push_str(&format!("port\t{}\n", self.listen_port));
        out.push_str(&format!("session_length\t{}\n", self.session_length.code()));
        out.push_str(&format!(
            "session_on_launch\t{}\n",
            u8::from(self.session_on_launch)
        ));
        out.push_str(&format!(
            "host_on_launch\t{}\n",
            u8::from(self.host_on_launch)
        ));
        out.push_str(&format!(
            "router_mapping_on_launch\t{}\n",
            u8::from(self.router_mapping_on_launch)
        ));
        out.push_str(&format!(
            "include_private\t{}\n",
            u8::from(self.include_private)
        ));
        out.push_str(&format!("rate_micro_per_mb\t{}\n", self.rate_micro_per_mb));
        out.push_str(&format!(
            "max_pay_micro_per_mb\t{}\n",
            self.max_pay_micro_per_mb
        ));
        out.push_str(&format!("creator_bps\t{}\n", self.creator_bps));
        for peer in &self.peers {
            out.push_str(&format!(
                "peer\t{}\t{}\t{}\n",
                peer.label,
                peer.endpoint,
                peer.did.as_deref().unwrap_or("")
            ));
        }
        out
    }

    /// Strict decode: an unknown header, a malformed flag, or a bad peer line
    /// rejects the whole file rather than half-applying a policy.
    pub fn decode(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(FILE_HEADER) {
            return Err("unrecognized connection settings header".into());
        }
        let mut settings = Self::default();
        let flag = |value: &str| -> Result<bool, String> {
            match value {
                "0" => Ok(false),
                "1" => Ok(true),
                other => Err(format!("invalid flag value {other:?}")),
            }
        };
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let mut parts = line.split('\t');
            let key = parts.next().unwrap_or_default();
            match key {
                "port" => {
                    settings.listen_port = parts
                        .next()
                        .and_then(|value| value.parse::<u16>().ok())
                        .filter(|port| *port != 0)
                        .ok_or("invalid listen port")?;
                }
                "session_length" => {
                    settings.session_length = parts
                        .next()
                        .and_then(|value| value.parse::<u8>().ok())
                        .and_then(SessionLength::from_code)
                        .ok_or("invalid session length")?;
                }
                "session_on_launch" => {
                    settings.session_on_launch = flag(parts.next().unwrap_or_default())?;
                }
                "host_on_launch" => {
                    settings.host_on_launch = flag(parts.next().unwrap_or_default())?;
                }
                "router_mapping_on_launch" => {
                    settings.router_mapping_on_launch = flag(parts.next().unwrap_or_default())?;
                }
                "include_private" => {
                    settings.include_private = flag(parts.next().unwrap_or_default())?;
                }
                "rate_micro_per_mb" => {
                    settings.rate_micro_per_mb = parts
                        .next()
                        .and_then(|value| value.parse::<u64>().ok())
                        .ok_or("invalid rate")?;
                }
                "max_pay_micro_per_mb" => {
                    settings.max_pay_micro_per_mb = parts
                        .next()
                        .and_then(|value| value.parse::<u64>().ok())
                        .ok_or("invalid ceiling")?;
                }
                "creator_bps" => {
                    settings.creator_bps = parts
                        .next()
                        .and_then(|value| value.parse::<u16>().ok())
                        .filter(|bps| *bps <= 10_000)
                        .ok_or("invalid creator share")?;
                }
                "peer" => {
                    let label = parts.next().unwrap_or_default();
                    let endpoint = parts.next().unwrap_or_default();
                    let did = parts.next().unwrap_or_default();
                    settings.upsert_peer(label, endpoint, Some(did))?;
                }
                other => return Err(format!("unknown connection setting {other:?}")),
            }
        }
        Ok(settings)
    }
}

/// The local address the OS would route outward from. Uses a UDP socket's
/// `connect`, which only consults the routing table and sends nothing; the
/// documentation-range target address never receives a packet. Owner-
/// triggered only, never called on launch.
pub fn local_address() -> Result<String, String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").map_err(|error| error.to_string())?;
    socket
        .connect("192.0.2.1:9")
        .map_err(|error| error.to_string())?;
    let address = socket.local_addr().map_err(|error| error.to_string())?;
    if address.ip().is_unspecified() {
        return Err("no route to a network".into());
    }
    Ok(address.ip().to_string())
}

/// Result of an owner-triggered router mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterMapping {
    /// The router's WAN address when it is a public one; `None` when the
    /// router reported nothing usable (0.0.0.0 or a private range), which
    /// means it sits behind another NAT or carrier-grade NAT and the
    /// mapping alone cannot make this machine reachable from the internet.
    pub external_ip: Option<std::net::Ipv4Addr>,
    pub external_port: u16,
    pub gateway: String,
    /// Seconds the router granted; 0 means the router keeps it until removed.
    pub lease_seconds: u32,
}

/// Ask the LAN's UPnP gateway to forward `port` to this machine and report
/// the external address. Talks only to the local router (SSDP multicast
/// discovery, then HTTP to the gateway's control URL); owner-triggered,
/// never on launch. The lease is bounded so a forgotten mapping expires.
pub fn map_port_on_router(port: u16, lease_seconds: u32) -> Result<RouterMapping, String> {
    let local_ip: std::net::Ipv4Addr = local_address()?
        .parse()
        .map_err(|_| "this machine's LAN address is not IPv4".to_string())?;
    let options = igd::SearchOptions {
        timeout: Some(std::time::Duration::from_secs(4)),
        ..Default::default()
    };
    let gateway = igd::search_gateway(options)
        .map_err(|error| format!("no UPnP gateway answered on this network: {error}"))?;
    let external_ip = gateway
        .get_external_ip()
        .map_err(|error| format!("the router did not report an external address: {error}"))?;
    gateway
        .add_port(
            igd::PortMappingProtocol::TCP,
            port,
            std::net::SocketAddrV4::new(local_ip, port),
            lease_seconds,
            "Mininet desktop hosting",
        )
        .map_err(|error| format!("the router refused the port mapping: {error}"))?;
    let external_ip = (!external_ip.is_unspecified()
        && !external_ip.is_private()
        && !external_ip.is_loopback()
        && !external_ip.is_link_local()
        && !is_cgnat(external_ip))
    .then_some(external_ip);
    Ok(RouterMapping {
        external_ip,
        external_port: port,
        gateway: gateway.addr.to_string(),
        lease_seconds,
    })
}

/// RFC 6598 shared address space (100.64.0.0/10): carrier-grade NAT.
fn is_cgnat(ip: std::net::Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    a == 100 && (64..=127).contains(&b)
}

/// Remove a mapping created by [`map_port_on_router`].
pub fn unmap_port_on_router(port: u16) -> Result<(), String> {
    let options = igd::SearchOptions {
        timeout: Some(std::time::Duration::from_secs(4)),
        ..Default::default()
    };
    let gateway = igd::search_gateway(options).map_err(|error| error.to_string())?;
    gateway
        .remove_port(igd::PortMappingProtocol::TCP, port)
        .map_err(|error| error.to_string())
}

pub fn settings_path(root: &Path) -> std::path::PathBuf {
    root.join("connections.txt")
}

pub fn load(root: &Path) -> ConnectionSettings {
    let path = settings_path(root);
    let Ok(metadata) = std::fs::metadata(&path) else {
        return ConnectionSettings::default();
    };
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return ConnectionSettings::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| ConnectionSettings::decode(&text).ok())
        .unwrap_or_default()
}

pub fn save(root: &Path, settings: &ConnectionSettings) -> Result<(), String> {
    crate::atomic_write_file(&settings_path(root), settings.encode().as_bytes())
}

/// A pasteable card another person uses to reach this instance. It carries a
/// dial hint and the owner's public identity; it is not a secret and grants
/// nothing by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionCard {
    pub endpoint: String,
    pub did: String,
    pub name: String,
}

const CARD_PREFIX: &str = "mininet-peer-v1";

impl ConnectionCard {
    pub fn encode(&self) -> String {
        let name: String = self
            .name
            .chars()
            .filter(|c| *c != ';' && !c.is_control())
            .collect();
        format!(
            "{CARD_PREFIX};endpoint={};did={};name={}",
            self.endpoint, self.did, name
        )
    }

    pub fn decode(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let mut fields = text.split(';');
        if fields.next() != Some(CARD_PREFIX) {
            return Err("This is not a Mininet connection card.".into());
        }
        let mut endpoint = None;
        let mut did = None;
        let mut name = None;
        for field in fields {
            let Some((key, value)) = field.split_once('=') else {
                return Err("The connection card is malformed.".into());
            };
            match key {
                "endpoint" => endpoint = Some(validate_endpoint(value)?),
                "did" => {
                    did = Some(
                        Did::parse(value.trim())
                            .map(|parsed| parsed.as_str().to_string())
                            .map_err(|error| format!("The card's DID is malformed: {error}"))?,
                    )
                }
                "name" => name = Some(clean_label(value)?),
                _ => return Err("The connection card has an unknown field.".into()),
            }
        }
        Ok(Self {
            endpoint: endpoint.ok_or("The connection card has no endpoint.")?,
            did: did.ok_or("The connection card has no DID.")?,
            name: name.ok_or("The connection card has no name.")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DID: &str =
        "did:mini:EAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    fn valid_did() -> String {
        // A real did:mini so the parser accepts it regardless of the exact
        // encoding rules; derived from a deterministic root.
        did_mini::Controller::incept_single_from_seeds(&[7; 32], &[8; 32])
            .unwrap()
            .did()
            .as_str()
            .to_string()
    }

    #[test]
    fn defaults_start_no_network_activity() {
        let settings = ConnectionSettings::default();
        assert!(!settings.session_on_launch);
        assert!(!settings.host_on_launch);
        assert!(!settings.include_private);
        assert!(settings.peers.is_empty());
        assert_eq!(settings.listen_port, DEFAULT_PORT);
    }

    #[test]
    fn settings_round_trip_including_peers() {
        let did = valid_did();
        let mut settings = ConnectionSettings {
            listen_port: 47001,
            session_length: SessionLength::WhileOpen,
            session_on_launch: true,
            host_on_launch: true,
            router_mapping_on_launch: true,
            include_private: true,
            rate_micro_per_mb: 25,
            max_pay_micro_per_mb: 40,
            creator_bps: 5000,
            peers: Vec::new(),
        };
        assert!(settings
            .upsert_peer("Alice", "alice.example:46000", Some(&did))
            .unwrap());
        assert!(settings
            .upsert_peer("Bob", "[2001:db8::2]:46000", None)
            .unwrap());
        // Re-adding the same endpoint renames rather than duplicates.
        assert!(!settings
            .upsert_peer("Alice (home)", "alice.example:46000", None)
            .unwrap());
        let decoded = ConnectionSettings::decode(&settings.encode()).unwrap();
        assert_eq!(decoded, settings);
        assert_eq!(decoded.peers.len(), 2);
        assert_eq!(decoded.peers[0].label, "Alice (home)");
        assert_eq!(decoded.peers[0].did.as_deref(), Some(did.as_str()));
        assert_eq!(decoded.peers[1].did, None);
        assert!(settings.remove_peer("[2001:db8::2]:46000"));
        assert!(!settings.remove_peer("[2001:db8::2]:46000"));
    }

    #[test]
    fn malformed_settings_are_rejected_whole() {
        assert!(ConnectionSettings::decode("something-else\nport\t1\n").is_err());
        assert!(ConnectionSettings::decode(&format!("{FILE_HEADER}\nport\t0\n")).is_err());
        assert!(
            ConnectionSettings::decode(&format!("{FILE_HEADER}\nsession_on_launch\tyes\n"))
                .is_err()
        );
        assert!(
            ConnectionSettings::decode(&format!("{FILE_HEADER}\npeer\tAlice\tno-port\t\n"))
                .is_err()
        );
        assert!(ConnectionSettings::decode(&format!(
            "{FILE_HEADER}\npeer\tAlice\thost:1\t{DID}-not-a-did\n"
        ))
        .is_err());
        assert!(ConnectionSettings::decode(&format!("{FILE_HEADER}\nmystery\t1\n")).is_err());
        let ok = ConnectionSettings::decode(&format!("{FILE_HEADER}\n\nport\t5000\n")).unwrap();
        assert_eq!(ok.listen_port, 5000);
    }

    #[test]
    fn connection_card_round_trips_and_rejects_garbage() {
        let did = valid_did();
        let card = ConnectionCard {
            endpoint: "203.0.113.5:46000".into(),
            did: did.clone(),
            name: "Alice; the first".into(),
        };
        let text = card.encode();
        let decoded = ConnectionCard::decode(&format!("  {text}\n")).unwrap();
        assert_eq!(decoded.endpoint, card.endpoint);
        assert_eq!(decoded.did, did);
        assert_eq!(decoded.name, "Alice the first");
        assert!(ConnectionCard::decode("mini-invite-v1.abc").is_err());
        assert!(ConnectionCard::decode("mininet-peer-v1;endpoint=host:1").is_err());
        assert!(ConnectionCard::decode(&format!(
            "mininet-peer-v1;endpoint=host:1;did={did};name=A;extra=1"
        ))
        .is_err());
        assert!(
            ConnectionCard::decode(&format!("mininet-peer-v1;endpoint=host;did={did};name=A"))
                .is_err()
        );
    }

    #[test]
    fn carrier_grade_nat_range_is_recognised() {
        assert!(is_cgnat("100.64.0.1".parse().unwrap()));
        assert!(is_cgnat("100.127.255.254".parse().unwrap()));
        assert!(!is_cgnat("100.128.0.1".parse().unwrap()));
        assert!(!is_cgnat("203.0.113.5".parse().unwrap()));
    }

    #[test]
    fn saving_and_loading_uses_the_data_root() {
        let root = std::env::temp_dir().join(format!(
            "mininet-desktop-connections-{}-{}",
            std::process::id(),
            crate::now_ms()
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(load(&root), ConnectionSettings::default());
        let mut settings = ConnectionSettings::default();
        settings
            .upsert_peer("Alice", "alice.example:46000", None)
            .unwrap();
        save(&root, &settings).unwrap();
        assert_eq!(load(&root), settings);
        std::fs::write(settings_path(&root), b"garbage").unwrap();
        assert_eq!(load(&root), ConnectionSettings::default());
        std::fs::remove_dir_all(root).unwrap();
    }
}
