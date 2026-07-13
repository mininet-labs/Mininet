//! Local-network (Wi-Fi/hotspot/LAN) peer discovery over UDP multicast.
//!
//! Founder review P1 backlog item "Local-Wi-Fi/mDNS adapter"
//! ([roadmap #22B](../../issues/22)/[#98](../../issues/98)). This is
//! **not** full mDNS/DNS-SD (RFC 6762/6763) — it is a minimal,
//! Mininet-owned announce datagram sent over the same UDP multicast
//! mechanism mDNS uses, serving the same purpose (find another peer on the
//! same local network with no central server, no prior coordination)
//! without adopting mDNS's much larger record/query wire format. A real
//! RFC 6762 implementation could replace this later without changing
//! anything above the [`SocketAddr`] it hands off to
//! [`crate::TcpBearer::connect`].
//!
//! Like every other primitive in this crate, discovery carries no
//! identity: an announce datagram says only "a Mininet peer is listening
//! for bearer connections on this port," never who. Bring your own
//! [`crate::Channel`] handshake once connected, exactly as with
//! [`crate::TcpBearer`].
//!
//! `docs/gates/wifi-bearer-test-protocol.md` gates whether this signal is
//! *trustworthy* evidence of real local co-presence (needs real routers,
//! phones, VPN/hotspot attack testing — W1-W7, not startable here). This
//! module only builds the underlying discovery mechanism the gate would
//! go on to test; it makes no trust claim of its own.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::Duration;

use crate::error::{BearerError, Result};

/// The administratively-scoped multicast group (RFC 2365) Mininet peers
/// announce on by default in real deployments — never leaves the local
/// network's routers. Tests use their own distinct ports (see module
/// tests) to avoid colliding with each other on one host.
pub const DEFAULT_MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 77, 1);
/// The default announce port.
pub const DEFAULT_MULTICAST_PORT: u16 = 7770;

const MAGIC: [u8; 8] = *b"MININET1";
/// `MAGIC` + a `u16` big-endian TCP port.
const ANNOUNCE_LEN: usize = MAGIC.len() + 2;

fn encode_announce(tcp_port: u16) -> [u8; ANNOUNCE_LEN] {
    let mut buf = [0u8; ANNOUNCE_LEN];
    buf[..MAGIC.len()].copy_from_slice(&MAGIC);
    buf[MAGIC.len()..].copy_from_slice(&tcp_port.to_be_bytes());
    buf
}

fn decode_announce(bytes: &[u8]) -> Option<u16> {
    if bytes.len() != ANNOUNCE_LEN || bytes[..MAGIC.len()] != MAGIC {
        return None;
    }
    Some(u16::from_be_bytes([
        bytes[MAGIC.len()],
        bytes[MAGIC.len() + 1],
    ]))
}

/// Announces that a bearer listener (e.g. a [`crate::TcpBearer`] accepting
/// on `tcp_port`) is reachable on this local network.
#[derive(Debug)]
pub struct LocalAnnouncer {
    socket: UdpSocket,
    group: SocketAddrV4,
    tcp_port: u16,
}

impl LocalAnnouncer {
    /// Bind an announcer for `tcp_port` on the default group/port.
    pub fn bind(tcp_port: u16) -> Result<Self> {
        Self::bind_to(DEFAULT_MULTICAST_GROUP, DEFAULT_MULTICAST_PORT, tcp_port)
    }

    /// Bind an announcer for `tcp_port` on an explicit group/port — tests use
    /// this to avoid colliding with each other or with a real deployment's
    /// default group on the same host.
    pub fn bind_to(group: Ipv4Addr, port: u16, tcp_port: u16) -> Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_multicast_ttl_v4(1)?; // never leave the local network
        Ok(LocalAnnouncer {
            socket,
            group: SocketAddrV4::new(group, port),
            tcp_port,
        })
    }

    /// Send one announce datagram. Callers re-announce periodically (e.g.
    /// every few seconds) since this is fire-and-forget, not a registration.
    pub fn announce(&self) -> Result<()> {
        let frame = encode_announce(self.tcp_port);
        self.socket.send_to(&frame, self.group)?;
        Ok(())
    }
}

/// Listens for [`LocalAnnouncer`] datagrams from other peers on the same
/// local network.
#[derive(Debug)]
pub struct LocalScanner {
    socket: UdpSocket,
}

impl LocalScanner {
    /// Bind a scanner on the default group/port.
    pub fn bind() -> Result<Self> {
        Self::bind_to(DEFAULT_MULTICAST_GROUP, DEFAULT_MULTICAST_PORT)
    }

    /// Bind a scanner on an explicit group/port.
    pub fn bind_to(group: Ipv4Addr, port: u16) -> Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
        socket.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)?;
        Ok(LocalScanner { socket })
    }

    /// Block for up to `timeout` for the next discovered peer's bearer
    /// address (the sender's own source IP, paired with the TCP port it
    /// announced) — hand this straight to [`crate::TcpBearer::connect`].
    /// Returns `Ok(None)` on timeout, not an error: "nobody nearby answered
    /// yet" is the ordinary case, not a failure.
    ///
    /// A malformed or foreign (non-Mininet) multicast datagram on the same
    /// group is silently skipped, not treated as this peer's failure —
    /// the multicast group is not a security boundary, just a rendezvous.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<SocketAddr>> {
        self.socket.set_read_timeout(Some(timeout))?;
        let mut buf = [0u8; ANNOUNCE_LEN + 1]; // +1 to detect oversized noise
        loop {
            let (n, from) = match self.socket.recv_from(&mut buf) {
                Ok(v) => v,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None)
                }
                Err(e) => return Err(BearerError::from(e)),
            };
            if let Some(tcp_port) = decode_announce(&buf[..n]) {
                return Ok(Some(SocketAddr::new(from.ip(), tcp_port)));
            }
            // Not one of ours; keep listening until the timeout elapses.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    // Distinct ports per test so `cargo test`'s default parallelism never
    // has two tests fight over the same multicast rendezvous port on one
    // host (only the *scanner* side binds the port; see `LocalScanner`'s
    // doc — this is why one bind per test is enough, no SO_REUSEADDR
    // needed, and this crate stays `#![forbid(unsafe_code)]`).

    #[test]
    fn a_scanner_discovers_an_announcer_on_the_same_local_network() {
        let group = Ipv4Addr::new(239, 255, 77, 10);
        let port = 47801;
        let scanner = LocalScanner::bind_to(group, port).unwrap();
        let announcer = LocalAnnouncer::bind_to(group, port, 9443).unwrap();

        announcer.announce().unwrap();
        let found = scanner
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .expect("the announce datagram should have arrived");
        assert_eq!(found.port(), 9443);
    }

    #[test]
    fn a_scanner_times_out_cleanly_when_nobody_announces() {
        let group = Ipv4Addr::new(239, 255, 77, 11);
        let port = 47802;
        let scanner = LocalScanner::bind_to(group, port).unwrap();

        let result = scanner.recv_timeout(Duration::from_millis(200)).unwrap();
        assert_eq!(result, None, "timeout must be Ok(None), not an error");
    }

    #[test]
    fn discovery_hands_off_a_usable_address_to_a_real_tcp_bearer_connect() {
        // End-to-end: discover a peer's address purely via local multicast,
        // then actually open a `Bearer` connection to the discovered
        // address, with no address ever typed/configured by the test.
        let group = Ipv4Addr::new(239, 255, 77, 12);
        let port = 47803;

        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let tcp_port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            crate::TcpBearer::from_stream(stream).unwrap()
        });

        let announcer = LocalAnnouncer::bind_to(group, port, tcp_port).unwrap();
        let scanner = LocalScanner::bind_to(group, port).unwrap();
        announcer.announce().unwrap();
        let discovered = scanner
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .expect("discovery should find the announcer");

        let client = TcpStream::connect(discovered).unwrap();
        let _client_bearer = crate::TcpBearer::from_stream(client).unwrap();
        let _server_bearer = server.join().unwrap();
    }

    #[test]
    fn a_foreign_datagram_on_the_same_group_is_ignored_not_mistaken_for_a_peer() {
        let group = Ipv4Addr::new(239, 255, 77, 13);
        let port = 47804;
        let scanner = LocalScanner::bind_to(group, port).unwrap();

        // Something else entirely is chattering on the same multicast
        // rendezvous group -- not a Mininet peer, must not be misread as
        // one, and must not make the scanner error out.
        let noise = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        noise
            .send_to(b"not a mininet announce datagram", (group, port))
            .unwrap();

        let result = scanner.recv_timeout(Duration::from_millis(300)).unwrap();
        assert_eq!(
            result, None,
            "foreign traffic on the rendezvous group must be skipped, never surfaced as a peer"
        );
    }
}
