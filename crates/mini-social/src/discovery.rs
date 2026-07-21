//! Opt-in LAN announcement of public profile labels.
//!
//! This is intentionally separate from `mini-bearer`'s identity-free endpoint
//! discovery. Enabling it reveals a chosen display name and DID to the local
//! network for a short user-initiated discovery window. Announcements are not
//! authenticated; clients must label them unverified until signed profile/KEL
//! objects arrive through verified sync.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::Duration;

use did_mini::Did;

use crate::{Result, SocialError, MAX_NAME_BYTES};

/// Administratively scoped multicast group used for opt-in profile discovery.
pub const PROFILE_DISCOVERY_GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 77, 2);
/// UDP port used for opt-in profile discovery.
pub const PROFILE_DISCOVERY_PORT: u16 = 7771;

const MAGIC: &[u8; 8] = b"MINIPRF1";
const MAX_DID_BYTES: usize = 256;
const MAX_ANNOUNCEMENT_BYTES: usize = 8 + 2 + 2 + MAX_DID_BYTES + 2 + MAX_NAME_BYTES;

/// One unauthenticated nearby profile label. Treat as a connection hint only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearbyProfile {
    pub address: SocketAddr,
    pub did: Did,
    pub display_name: String,
}

/// Sends a chosen public profile label during a user-initiated visible window.
#[derive(Debug)]
pub struct LocalProfileAnnouncer {
    socket: UdpSocket,
    group: SocketAddrV4,
    bytes: Vec<u8>,
}

impl LocalProfileAnnouncer {
    pub fn bind(tcp_port: u16, did: &Did, display_name: &str) -> Result<Self> {
        Self::bind_to(
            PROFILE_DISCOVERY_GROUP,
            PROFILE_DISCOVERY_PORT,
            tcp_port,
            did,
            display_name,
        )
    }

    pub fn bind_to(
        group: Ipv4Addr,
        port: u16,
        tcp_port: u16,
        did: &Did,
        display_name: &str,
    ) -> Result<Self> {
        let bytes = encode(tcp_port, did, display_name)?;
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_multicast_ttl_v4(1)?;
        Ok(Self {
            socket,
            group: SocketAddrV4::new(group, port),
            bytes,
        })
    }

    pub fn announce(&self) -> Result<()> {
        self.socket.send_to(&self.bytes, self.group)?;
        Ok(())
    }
}

/// Receives opt-in public profile labels from the local network.
#[derive(Debug)]
pub struct LocalProfileScanner {
    socket: UdpSocket,
}

impl LocalProfileScanner {
    pub fn bind() -> Result<Self> {
        Self::bind_to(PROFILE_DISCOVERY_GROUP, PROFILE_DISCOVERY_PORT)
    }

    pub fn bind_to(group: Ipv4Addr, port: u16) -> Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
        socket.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)?;
        Ok(Self { socket })
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<NearbyProfile>> {
        self.socket.set_read_timeout(Some(timeout))?;
        let mut buffer = [0u8; MAX_ANNOUNCEMENT_BYTES + 1];
        loop {
            let (len, sender) = match self.socket.recv_from(&mut buffer) {
                Ok(value) => value,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None)
                }
                Err(error) if error.raw_os_error() == Some(10040) => continue,
                Err(error) => return Err(SocialError::Io(error.to_string())),
            };
            if let Some((tcp_port, did, display_name)) = decode(&buffer[..len]) {
                return Ok(Some(NearbyProfile {
                    address: SocketAddr::new(sender.ip(), tcp_port),
                    did,
                    display_name,
                }));
            }
        }
    }
}

fn encode(tcp_port: u16, did: &Did, display_name: &str) -> Result<Vec<u8>> {
    if display_name.is_empty()
        || display_name.len() > MAX_NAME_BYTES
        || did.as_str().len() > MAX_DID_BYTES
    {
        return Err(SocialError::FieldTooLarge);
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&tcp_port.to_be_bytes());
    bytes.extend_from_slice(&(did.as_str().len() as u16).to_be_bytes());
    bytes.extend_from_slice(did.as_str().as_bytes());
    bytes.extend_from_slice(&(display_name.len() as u16).to_be_bytes());
    bytes.extend_from_slice(display_name.as_bytes());
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Option<(u16, Did, String)> {
    if bytes.len() < MAGIC.len() + 6 || bytes.get(..MAGIC.len())? != MAGIC {
        return None;
    }
    let mut offset = MAGIC.len();
    let tcp_port = u16::from_be_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?);
    offset += 2;
    let did_len = u16::from_be_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?) as usize;
    offset += 2;
    if did_len == 0 || did_len > MAX_DID_BYTES {
        return None;
    }
    let did = Did::parse(std::str::from_utf8(bytes.get(offset..offset + did_len)?).ok()?).ok()?;
    offset += did_len;
    let name_len = u16::from_be_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?) as usize;
    offset += 2;
    if name_len == 0 || name_len > MAX_NAME_BYTES || offset + name_len != bytes.len() {
        return None;
    }
    let name = std::str::from_utf8(bytes.get(offset..offset + name_len)?)
        .ok()?
        .to_string();
    Some((tcp_port, did, name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    #[test]
    fn announcement_round_trips_name_did_and_port() {
        let identity = Controller::incept_single_from_seeds(&[1; 32], &[2; 32]).unwrap();
        let bytes = encode(46000, &identity.did(), "Alice Example").unwrap();
        let (port, did, name) = decode(&bytes).unwrap();
        assert_eq!(port, 46000);
        assert_eq!(did, identity.did());
        assert_eq!(name, "Alice Example");
    }

    #[test]
    fn malformed_or_trailing_announcements_are_rejected() {
        let identity = Controller::incept_single_from_seeds(&[1; 32], &[2; 32]).unwrap();
        let mut bytes = encode(46000, &identity.did(), "Alice").unwrap();
        bytes.push(0);
        assert!(decode(&bytes).is_none());
        assert!(decode(b"not-mininet").is_none());
    }

    #[test]
    fn local_scanner_receives_an_opt_in_profile_announcement() {
        let group = Ipv4Addr::new(239, 255, 77, 20);
        let port = 47811;
        let identity = Controller::incept_single_from_seeds(&[3; 32], &[4; 32]).unwrap();
        let scanner = LocalProfileScanner::bind_to(group, port).unwrap();
        let announcer =
            LocalProfileAnnouncer::bind_to(group, port, 46000, &identity.did(), "Alice").unwrap();

        announcer.announce().unwrap();
        let found = scanner
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .expect("the same-machine multicast announcement should arrive");
        assert_eq!(found.did, identity.did());
        assert_eq!(found.display_name, "Alice");
        assert_eq!(found.address.port(), 46000);
    }
}
