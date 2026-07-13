//! Peer exchange (PEX): learn peers beyond whatever address a node was
//! initially supplied, over an already-established connection, with no
//! central directory server.
//!
//! Founder review P1 backlog item "Invitation and peer-exchange discovery
//! with no required central server" ([roadmap #36](../../issues/36)-
//! [#45](../../issues/45)). [`RoutingTable`](crate::RoutingTable) already
//! tracks *which* [`PeerId`]s are known, but a `PeerId` alone is never
//! dialable (see that type's own docs) — nothing in this crate previously
//! recorded a *dialable address* for a known peer. [`AddressBook`] closes
//! that gap, and [`PexMessage`] is the minimal wire protocol two peers use
//! to hand each other `(PeerId, SocketAddr)` pairs:
//!
//! ```text
//! peer A -> peer B : Request(A's own PeerId)
//! peer B -> peer A : Response(up to MAX_PEX_RECORDS records B already
//!                              knows both the id and address for,
//!                              excluding A itself)
//! ```
//!
//! `Request` carries the requester's own id, not a self-declared address:
//! the responder learns the requester's dialable address from the live
//! connection's own observed source address (e.g. `TcpStream::peer_addr`),
//! never from a claim inside the message — a self-reported address would
//! invite exactly the kind of return-address spoofing observed-address
//! binding avoids. This is one request/response, not a subscription or a
//! push protocol —
//! callers repeat it against different peers to converge on a fuller view,
//! the same way a Kademlia node makes repeated `FIND_NODE` calls. Nothing
//! here dials a socket; `build_response`/`absorb_response` are pure
//! functions over [`RoutingTable`]/[`AddressBook`], matching this crate's
//! existing "landing pure, testable logic before the adapter that needs a
//! real socket" pattern (see the crate-level doc's honest limits). A
//! caller wires this over any real transport — `mini-bearer::TcpBearer` in
//! this workspace today — the same way `mini-sync`'s protocol module does.
//!
//! ## Trust model
//!
//! A `PexMessage::Response` is an *unauthenticated hint*, never a proof of
//! anything: `PeerId` is explicitly not an identity (see that type's
//! docs), and nothing here claims the address in a `PeerRecord` is honest,
//! live, or reachable. [`AddressBook::insert`] is first-seen-wins
//! specifically so a later, hostile PEX response cannot silently redirect
//! who a caller dials for an id it already resolved. Whatever a discovered
//! address is dialed for still goes through the same untrusted-until-
//! proven path every other bearer connection does (an anonymous
//! `mini_bearer::Channel` handshake, then payload-level authenticity —
//! see `mini-bearer`'s own "anonymous connection, valid transaction"
//! security model). Response size is capped at [`MAX_PEX_RECORDS`] so a
//! single response can never become an unbounded memory/bandwidth sink,
//! mirroring [`crate::GossipRouter`]'s own capacity-bounding stance.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::error::{NetError, Result};
use crate::peer::PeerId;
use crate::routing::RoutingTable;

/// Hard cap on records in one [`PexMessage::Response`] — a bounded sample,
/// never a full table dump.
pub const MAX_PEX_RECORDS: usize = 64;

const TAG_REQUEST: u8 = 0x01;
const TAG_RESPONSE: u8 = 0x02;
const ADDR_KIND_V4: u8 = 4;
const ADDR_KIND_V6: u8 = 6;

/// A peer id paired with a dialable address — what [`RoutingTable`] alone
/// cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerRecord {
    /// The peer's transport-routing id.
    pub id: PeerId,
    /// Where a caller can dial this peer.
    pub addr: SocketAddr,
}

/// The peer-exchange wire message: a request for peers, or a bounded
/// response listing some.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PexMessage {
    /// "Tell me some peers you know" — carries the requester's own
    /// [`PeerId`], so the responder can exclude it from the reply and
    /// (paired with the live connection's own observed source address,
    /// not a self-declared one — see [`build_response`]'s caller-side
    /// contract) register the requester as a newly discovered peer too.
    Request(PeerId),
    /// A bounded sample of known `(id, address)` pairs.
    Response(Vec<PeerRecord>),
}

impl PexMessage {
    /// Serialize to bytes for transport over any bearer.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            PexMessage::Request(id) => {
                let mut out = Vec::with_capacity(33);
                out.push(TAG_REQUEST);
                out.extend_from_slice(&id.0);
                out
            }
            PexMessage::Response(records) => {
                let mut out = Vec::with_capacity(3 + records.len() * 51);
                out.push(TAG_RESPONSE);
                out.extend_from_slice(&(records.len() as u16).to_be_bytes());
                for record in records {
                    out.extend_from_slice(&record.id.0);
                    match record.addr.ip() {
                        IpAddr::V4(v4) => {
                            out.push(ADDR_KIND_V4);
                            out.extend_from_slice(&v4.octets());
                        }
                        IpAddr::V6(v6) => {
                            out.push(ADDR_KIND_V6);
                            out.extend_from_slice(&v6.octets());
                        }
                    }
                    out.extend_from_slice(&record.addr.port().to_be_bytes());
                }
                out
            }
        }
    }

    /// Parse bytes received from a peer. Rejects a truncated frame, an
    /// unknown tag/address-kind byte, an oversized record count, and any
    /// trailing bytes past a well-formed message — a malformed or hostile
    /// frame is refused, never partially interpreted.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let (tag, rest) = bytes.split_first().ok_or(NetError::MalformedPex)?;
        match *tag {
            TAG_REQUEST => {
                if rest.len() != 32 {
                    return Err(NetError::MalformedPex);
                }
                let mut id = [0u8; 32];
                id.copy_from_slice(rest);
                Ok(PexMessage::Request(PeerId(id)))
            }
            TAG_RESPONSE => {
                if rest.len() < 2 {
                    return Err(NetError::MalformedPex);
                }
                let (count_bytes, mut cursor) = rest.split_at(2);
                let count = u16::from_be_bytes([count_bytes[0], count_bytes[1]]) as usize;
                if count > MAX_PEX_RECORDS {
                    return Err(NetError::TooManyPexRecords);
                }
                let mut records = Vec::with_capacity(count);
                for _ in 0..count {
                    if cursor.len() < 33 {
                        return Err(NetError::MalformedPex);
                    }
                    let (id_bytes, after_id) = cursor.split_at(32);
                    let mut id = [0u8; 32];
                    id.copy_from_slice(id_bytes);
                    let (kind, after_kind) =
                        after_id.split_first().ok_or(NetError::MalformedPex)?;
                    let (ip, after_addr) = match *kind {
                        ADDR_KIND_V4 => {
                            if after_kind.len() < 4 {
                                return Err(NetError::MalformedPex);
                            }
                            let (a, rest2) = after_kind.split_at(4);
                            (IpAddr::V4(Ipv4Addr::new(a[0], a[1], a[2], a[3])), rest2)
                        }
                        ADDR_KIND_V6 => {
                            if after_kind.len() < 16 {
                                return Err(NetError::MalformedPex);
                            }
                            let (a, rest2) = after_kind.split_at(16);
                            let mut octets = [0u8; 16];
                            octets.copy_from_slice(a);
                            (IpAddr::V6(Ipv6Addr::from(octets)), rest2)
                        }
                        _ => return Err(NetError::MalformedPex),
                    };
                    if after_addr.len() < 2 {
                        return Err(NetError::MalformedPex);
                    }
                    let (port_bytes, rest3) = after_addr.split_at(2);
                    let port = u16::from_be_bytes([port_bytes[0], port_bytes[1]]);
                    records.push(PeerRecord {
                        id: PeerId(id),
                        addr: SocketAddr::new(ip, port),
                    });
                    cursor = rest3;
                }
                if !cursor.is_empty() {
                    return Err(NetError::MalformedPex);
                }
                Ok(PexMessage::Response(records))
            }
            _ => Err(NetError::MalformedPex),
        }
    }
}

/// Maps [`PeerId`]s to a dialable address — the piece [`RoutingTable`]
/// deliberately doesn't carry (see that type's docs: it tracks routing
/// positions, not addresses).
#[derive(Debug, Default)]
pub struct AddressBook {
    addrs: HashMap<PeerId, SocketAddr>,
}

impl AddressBook {
    /// A fresh, empty address book.
    pub fn new() -> Self {
        AddressBook {
            addrs: HashMap::new(),
        }
    }

    /// Record a peer's dialable address. **First-seen wins**: if an
    /// address is already known for this id, it is kept and this call
    /// returns `false` — a later PEX response claiming a different address
    /// for an already-known id can never silently redirect who a caller
    /// dials. Returns `true` when this was a genuinely new mapping.
    pub fn insert(&mut self, id: PeerId, addr: SocketAddr) -> bool {
        use std::collections::hash_map::Entry;
        match self.addrs.entry(id) {
            Entry::Occupied(_) => false,
            Entry::Vacant(slot) => {
                slot.insert(addr);
                true
            }
        }
    }

    /// The dialable address for `id`, if known.
    pub fn get(&self, id: &PeerId) -> Option<SocketAddr> {
        self.addrs.get(id).copied()
    }

    /// How many peers this book has an address for.
    pub fn len(&self) -> usize {
        self.addrs.len()
    }

    /// Whether this book knows no addresses yet.
    pub fn is_empty(&self) -> bool {
        self.addrs.is_empty()
    }
}

/// Build a bounded [`PexMessage::Response`] of peers this node knows both
/// the id and address for, nearest to its own routing position first
/// (the same closeness bias `RoutingTable::closest_peers` gives lookups),
/// excluding `exclude` (the requester's `PeerId`, taken from the
/// [`PexMessage::Request`] it just sent, so a peer is never handed back
/// its own record). Callers typically also feed the requester itself
/// into [`absorb_response`]-style bookkeeping first — pairing the
/// `Request`'s id with the live connection's own observed source address,
/// per this module's trust model — so answering a PEX request grows the
/// responder's own view too, not just the requester's.
pub fn build_response(routing: &RoutingTable, book: &AddressBook, exclude: &PeerId) -> PexMessage {
    let candidates = routing.closest_peers(&routing.local(), MAX_PEX_RECORDS + 1);
    let records: Vec<PeerRecord> = candidates
        .into_iter()
        .filter(|id| id != exclude)
        .filter_map(|id| book.get(&id).map(|addr| PeerRecord { id, addr }))
        .take(MAX_PEX_RECORDS)
        .collect();
    PexMessage::Response(records)
}

/// Absorb a [`PexMessage::Response`]'s records into this node's own
/// routing table and address book. Never learns the local node's own id
/// as a peer. Best-effort: a record whose id is already known, or whose
/// bucket is full (see [`RoutingTable::insert`]'s own honest limit), is
/// silently skipped rather than treated as an error — an unauthenticated
/// hint that turns out to be redundant is not a protocol violation.
pub fn absorb_response(records: &[PeerRecord], routing: &mut RoutingTable, book: &mut AddressBook) {
    let local = routing.local();
    for record in records {
        if record.id == local {
            continue;
        }
        routing.insert(record.id);
        book.insert(record.id, record.addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> PeerId {
        PeerId([byte; 32])
    }

    fn addr(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
    }

    #[test]
    fn a_request_round_trips_through_encode_decode() {
        let msg = PexMessage::Request(id(9));
        assert_eq!(PexMessage::decode(&msg.encode()).unwrap(), msg);
    }

    #[test]
    fn a_truncated_request_is_rejected_not_partially_parsed() {
        let full = PexMessage::Request(id(9)).encode();
        for cut in 1..full.len() {
            assert!(
                PexMessage::decode(&full[..cut]).is_err(),
                "truncating a request to {cut} bytes must be rejected"
            );
        }
    }

    #[test]
    fn a_response_round_trips_through_encode_decode() {
        let records = vec![
            PeerRecord {
                id: id(1),
                addr: addr(9000),
            },
            PeerRecord {
                id: id(2),
                addr: SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 9001),
            },
        ];
        let msg = PexMessage::Response(records.clone());
        let decoded = PexMessage::decode(&msg.encode()).unwrap();
        assert_eq!(decoded, PexMessage::Response(records));
    }

    #[test]
    fn an_empty_response_round_trips() {
        let msg = PexMessage::Response(Vec::new());
        assert_eq!(PexMessage::decode(&msg.encode()).unwrap(), msg);
    }

    #[test]
    fn a_truncated_frame_is_rejected_not_partially_parsed() {
        let full = PexMessage::Response(vec![PeerRecord {
            id: id(1),
            addr: addr(9000),
        }])
        .encode();
        for cut in 1..full.len() {
            assert!(
                PexMessage::decode(&full[..cut]).is_err(),
                "truncating to {cut} bytes must be rejected"
            );
        }
    }

    #[test]
    fn trailing_bytes_after_a_well_formed_message_are_rejected() {
        let mut bytes = PexMessage::Request(id(9)).encode();
        bytes.push(0xff);
        assert!(PexMessage::decode(&bytes).is_err());
    }

    #[test]
    fn an_unknown_tag_is_rejected() {
        assert!(PexMessage::decode(&[0xee]).is_err());
    }

    #[test]
    fn a_claimed_count_over_the_cap_is_rejected_before_allocating() {
        let mut bytes = vec![TAG_RESPONSE];
        bytes.extend_from_slice(&((MAX_PEX_RECORDS as u16) + 1).to_be_bytes());
        assert_eq!(PexMessage::decode(&bytes), Err(NetError::TooManyPexRecords));
    }

    #[test]
    fn address_book_first_seen_wins() {
        let mut book = AddressBook::new();
        assert!(book.insert(id(1), addr(9000)));
        assert!(
            !book.insert(id(1), addr(9999)),
            "a second insert for the same id must be refused"
        );
        assert_eq!(
            book.get(&id(1)),
            Some(addr(9000)),
            "the first address must be kept"
        );
    }

    #[test]
    fn build_response_excludes_the_requester_and_addressless_peers() {
        let local = id(0);
        let requester = id(1);
        let known_with_addr = id(2);
        let known_without_addr = id(3);

        let mut routing = RoutingTable::new(local);
        routing.insert(requester);
        routing.insert(known_with_addr);
        routing.insert(known_without_addr);

        let mut book = AddressBook::new();
        book.insert(known_with_addr, addr(9002));
        // known_without_addr deliberately has no address recorded.

        let PexMessage::Response(records) = build_response(&routing, &book, &requester) else {
            panic!("expected a Response");
        };
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, known_with_addr);
        assert_eq!(records[0].addr, addr(9002));
    }

    #[test]
    fn absorb_response_learns_new_peers_but_never_the_local_id() {
        let local = id(0);
        let mut routing = RoutingTable::new(local);
        let mut book = AddressBook::new();

        let discovered = id(7);
        let records = vec![
            PeerRecord {
                id: local,
                addr: addr(1),
            }, // must be ignored -- never learn ourselves
            PeerRecord {
                id: discovered,
                addr: addr(9007),
            },
        ];
        absorb_response(&records, &mut routing, &mut book);

        assert!(!routing.contains(&local));
        assert!(routing.contains(&discovered));
        assert_eq!(book.get(&discovered), Some(addr(9007)));
    }

    #[test]
    fn absorb_response_does_not_overwrite_an_already_known_address() {
        let local = id(0);
        let mut routing = RoutingTable::new(local);
        let mut book = AddressBook::new();
        let peer = id(5);
        book.insert(peer, addr(1111));

        absorb_response(
            &[PeerRecord {
                id: peer,
                addr: addr(2222),
            }],
            &mut routing,
            &mut book,
        );

        assert_eq!(
            book.get(&peer),
            Some(addr(1111)),
            "an already-known address must not be silently replaced by a later PEX hint"
        );
    }
}
