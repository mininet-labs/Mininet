//! Wide-area peer discovery for the consensus mesh, over a real TCP socket
//! (roadmap R8's "peers are supplied rather than discovered" gap).
//!
//! `mini_net::pex` already has the peer-exchange logic — [`PexMessage`],
//! [`RoutingTable`], [`AddressBook`], [`build_response`]/[`absorb_response`]
//! — fully unit-tested without ever touching a socket, matching this
//! workspace's pattern of landing pure, testable logic before the adapter
//! that needs a real network. That crate's own docs name the adapter as
//! pending: "a caller wires this over any real transport —
//! `mini-bearer::TcpBearer` in this workspace today — the same way
//! `mini-sync`'s protocol module does." This module is that adapter for the
//! consensus mesh: the same anonymous, forward-secret [`mini_bearer::Channel`]
//! handshake [`crate::net::catch_up_over_tcp`]/[`crate::net::state_sync_over_tcp`]
//! already use, carrying [`PexMessage`] bytes instead of block history.
//!
//! ## What this closes, and what it does not
//!
//! Before this module, a [`crate::net::TcpMesh`] could only be built from a
//! fixed address list every node had to be handed out of band — this
//! crate's own module-level "Honest limits" named this as separate, later
//! work.
//! [`pex_over_tcp`]/[`serve_pex_over_tcp`] let a node ask one already-known
//! peer for others it knows, growing a local [`RoutingTable`]/[`AddressBook`]
//! with no directory server — the Kademlia-style "repeat the request against
//! different peers to converge on a fuller view" pattern `mini_net::pex`'s
//! own docs describe.
//!
//! **Honest limits, restated rather than hidden:**
//! - A [`PexMessage::Response`] is still exactly what `mini_net::pex`
//!   already documents it as: an unauthenticated hint, never a proof of
//!   liveness or honesty. This module adds a real transport underneath, not
//!   new trust — an on-path attacker cannot forge or read a response
//!   ([`mini_bearer::Channel`] is authenticated-encrypted end to end), but a
//!   genuine peer's honest response is still just a hint about who else
//!   might exist, exactly as before.
//! - The address a responder records for a requester is the live
//!   connection's own observed source address, per `mini_net::pex`'s own
//!   contract (never a self-declared one — that would invite return-address
//!   spoofing). For a requester that dialed out from an ephemeral port
//!   rather than its own listening socket, that observed address is not its
//!   dialable one. That is `mini_net::pex`'s own already-documented
//!   limitation, not a new one introduced here; deployments where a node's
//!   dial and listen addresses coincide (every test below, or any network
//!   with a fixed announce port) are unaffected.
//! - One request, one peer, one response per call. No periodic refresh, no
//!   bucket-liveness eviction, no automatic re-query on a stale book — a
//!   caller wanting a fuller view repeats this against more peers, the same
//!   caller responsibility [`mini_net`]'s own docs already place here.
//! - **Not wired into [`crate::net::TcpMesh::establish`].** That
//!   constructor's deadlock-free dial/accept convention needs every node
//!   agreeing on one consistent, fully-resolved address list up front (see
//!   its own doc's "every listener in the mesh must already be bound...
//!   before any node calls this"); a partial, per-node discovered view
//!   cannot honestly supply that without a separate agreement step this
//!   slice does not build. A discovered [`AddressBook`] is a real, useful
//!   thing a caller now has — turning it into a mesh topology is still a
//!   host decision.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use mini_bearer::{Bearer, Initiator, Responder, TcpBearer};
use mini_net::{absorb_response, build_response, AddressBook, PeerId, PexMessage, RoutingTable};

use crate::error::{ConsensusError, Result};

/// AEAD associated data for peer-exchange frames — distinct from every other
/// purpose [`mini_bearer::Channel`] serves in this crate ([`crate::net`]'s
/// `CATCHUP_AAD`/`STATE_SYNC_AAD`/`CONSENSUS_AAD`), so a PEX ciphertext can
/// never be replayed as if it meant something else even though all reuse the
/// same `Channel` primitive.
const PEX_AAD: &[u8] = b"mini-consensus/pex-channel/v1";

/// Bound on how long one PEX round trip may take before giving up — a peer
/// that connects but never completes the exchange must not hang the caller
/// forever. Generous for a real network; instant on loopback.
const PEX_IO_TIMEOUT: Duration = Duration::from_secs(30);

fn configure(stream: &TcpStream, timeout: Duration) -> Result<()> {
    stream
        .set_read_timeout(Some(timeout))
        .map_err(mini_bearer::BearerError::from)?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(mini_bearer::BearerError::from)?;
    Ok(())
}

/// Ask `peer_addr` for peers it knows, over a fresh encrypted connection.
/// Absorbs whatever it returns into `routing`/`book` and returns how many
/// were genuinely new records ([`AddressBook::insert`]'s first-seen-wins
/// rule means a response repeating an already-known id/address is not
/// counted again).
pub fn pex_over_tcp(
    local_id: PeerId,
    routing: &mut RoutingTable,
    book: &mut AddressBook,
    peer_addr: SocketAddr,
) -> Result<usize> {
    pex_over_tcp_with_timeout(local_id, routing, book, peer_addr, PEX_IO_TIMEOUT)
}

fn pex_over_tcp_with_timeout(
    local_id: PeerId,
    routing: &mut RoutingTable,
    book: &mut AddressBook,
    peer_addr: SocketAddr,
    timeout: Duration,
) -> Result<usize> {
    let stream =
        TcpStream::connect_timeout(&peer_addr, timeout).map_err(mini_bearer::BearerError::from)?;
    configure(&stream, timeout)?;
    let mut bearer = TcpBearer::from_stream(stream)?;

    let (initiator, hello) = Initiator::start()?;
    bearer.send(&hello)?;
    let hello_response = bearer.recv()?;
    let mut channel = initiator.finish(&hello_response)?;

    let request = PexMessage::Request(local_id);
    bearer.send(&channel.seal(&request.encode(), PEX_AAD)?)?;

    let sealed_response = bearer.recv()?;
    let plaintext = channel.open(&sealed_response, PEX_AAD)?;
    let response = PexMessage::decode(&plaintext)?;
    let PexMessage::Response(records) = response else {
        // A well-formed peer never replies to a Request with another
        // Request; treat it the same as any other structurally wrong frame.
        return Err(ConsensusError::Malformed);
    };

    let before = book.len();
    absorb_response(&records, routing, book);
    Ok(book.len() - before)
}

/// Serve one PEX request on `listener`: accept a connection, learn the
/// requester's own id at its observed source address, and answer with a
/// bounded sample of peers this node already knows both the id and address
/// for. Blocks until a peer connects and completes the round trip.
pub fn serve_pex_over_tcp(
    routing: &mut RoutingTable,
    book: &mut AddressBook,
    listener: &TcpListener,
) -> Result<()> {
    serve_pex_over_tcp_with_timeout(routing, book, listener, PEX_IO_TIMEOUT)
}

fn serve_pex_over_tcp_with_timeout(
    routing: &mut RoutingTable,
    book: &mut AddressBook,
    listener: &TcpListener,
    timeout: Duration,
) -> Result<()> {
    let (stream, observed_addr) = listener.accept().map_err(mini_bearer::BearerError::from)?;
    configure(&stream, timeout)?;
    let mut bearer = TcpBearer::from_stream(stream)?;

    let hello = bearer.recv()?;
    let (mut channel, hello_response) = Responder::respond(&hello)?;
    bearer.send(&hello_response)?;

    let sealed_request = bearer.recv()?;
    let plaintext = channel.open(&sealed_request, PEX_AAD)?;
    let request = PexMessage::decode(&plaintext)?;
    let PexMessage::Request(requester_id) = request else {
        return Err(ConsensusError::Malformed);
    };

    // Learn the requester itself at its live observed address, never at a
    // self-declared one -- `mini_net::pex`'s own trust model, restated in
    // this module's doc.
    if requester_id != routing.local() {
        routing.insert(requester_id);
        book.insert(requester_id, observed_addr);
    }

    let response = build_response(routing, book, &requester_id);
    bearer.send(&channel.seal(&response.encode(), PEX_AAD)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn id(byte: u8) -> PeerId {
        PeerId([byte; 32])
    }

    #[test]
    fn a_node_learns_a_peer_it_never_heard_of_directly_from_one_hop() {
        // A is the requester, B is the responder and already knows C.
        // A asks B and must come away knowing about both B (from the
        // observed connection) and C (from B's response).
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let b_addr = listener.local_addr().unwrap();

        let a_id = id(1);
        let b_id = id(2);
        let c_id = id(3);
        let c_addr: SocketAddr = "127.0.0.1:9".parse().unwrap();

        let server = thread::spawn(move || {
            let mut b_routing = RoutingTable::new(b_id);
            let mut b_book = AddressBook::new();
            b_routing.insert(c_id);
            b_book.insert(c_id, c_addr);
            serve_pex_over_tcp(&mut b_routing, &mut b_book, &listener).unwrap();
            (b_routing, b_book)
        });

        let mut a_routing = RoutingTable::new(a_id);
        let mut a_book = AddressBook::new();
        let learned = pex_over_tcp(a_id, &mut a_routing, &mut a_book, b_addr).unwrap();

        let (b_routing, b_book) = server.join().unwrap();

        assert_eq!(
            learned, 1,
            "only C is new -- B is not a member of its own response"
        );
        assert!(a_routing.contains(&c_id));
        assert_eq!(a_book.get(&c_id), Some(c_addr));

        // And B, purely by answering, learned A at A's real dialing address
        // -- never a value A could have claimed inside the message.
        assert!(b_routing.contains(&a_id));
        assert_eq!(b_book.get(&a_id).unwrap().ip(), b_addr.ip());
    }

    #[test]
    fn the_requester_is_never_handed_back_its_own_record() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let a_id = id(1);
        let b_id = id(2);

        let server = thread::spawn(move || {
            let mut routing = RoutingTable::new(b_id);
            let mut book = AddressBook::new();
            serve_pex_over_tcp(&mut routing, &mut book, &listener).unwrap();
        });

        let mut a_routing = RoutingTable::new(a_id);
        let mut a_book = AddressBook::new();
        let learned = pex_over_tcp(a_id, &mut a_routing, &mut a_book, addr).unwrap();
        server.join().unwrap();

        assert_eq!(learned, 0, "B had nobody else to offer");
        assert!(
            !a_routing.contains(&a_id),
            "a node must never learn itself as a peer"
        );
    }

    #[test]
    fn a_repeated_exchange_does_not_recount_or_overwrite_already_known_peers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let b_addr = listener.local_addr().unwrap();
        let a_id = id(1);
        let b_id = id(2);
        let c_id = id(3);
        let c_addr: SocketAddr = "127.0.0.1:9".parse().unwrap();

        let server = thread::spawn(move || {
            let mut routing = RoutingTable::new(b_id);
            let mut book = AddressBook::new();
            routing.insert(c_id);
            book.insert(c_id, c_addr);
            serve_pex_over_tcp(&mut routing, &mut book, &listener).unwrap();
        });

        let mut a_routing = RoutingTable::new(a_id);
        let mut a_book = AddressBook::new();
        // A already knows C at a different, earlier-learned address.
        let stale_addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        a_routing.insert(c_id);
        a_book.insert(c_id, stale_addr);

        let learned = pex_over_tcp(a_id, &mut a_routing, &mut a_book, b_addr).unwrap();
        server.join().unwrap();

        assert_eq!(
            learned, 0,
            "C was already known -- first-seen-wins, not recounted"
        );
        assert_eq!(
            a_book.get(&c_id),
            Some(stale_addr),
            "an already-known address must not be silently replaced by a PEX hint"
        );
    }

    #[test]
    fn a_pex_request_crosses_the_wire_as_ciphertext_never_plaintext() {
        // Same regression class as crate::net's own
        // `queued_frames_cross_the_wire_as_ciphertext_never_plaintext`: play
        // the responder's handshake role by hand so the raw sealed request
        // can be inspected *before* it is ever opened, then complete the
        // exchange for real so the client does not hang.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let requester_id = id(9);

        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut bearer = TcpBearer::from_stream(stream).unwrap();
            let hello = bearer.recv().unwrap();
            let (mut channel, hello_response) = Responder::respond(&hello).unwrap();
            bearer.send(&hello_response).unwrap();

            let sealed_request = bearer.recv().unwrap();
            assert!(
                !sealed_request.windows(32).any(|w| w == requester_id.0),
                "the requester's plaintext id must never appear on the wire unencrypted"
            );

            let plaintext = channel.open(&sealed_request, PEX_AAD).unwrap();
            let PexMessage::Request(got_id) = PexMessage::decode(&plaintext).unwrap() else {
                panic!("expected a Request");
            };
            assert_eq!(got_id, requester_id);

            let response = PexMessage::Response(Vec::new());
            bearer
                .send(&channel.seal(&response.encode(), PEX_AAD).unwrap())
                .unwrap();
        });

        let mut routing = RoutingTable::new(requester_id);
        let mut book = AddressBook::new();
        let learned = pex_over_tcp(requester_id, &mut routing, &mut book, addr).unwrap();
        server.join().unwrap();

        assert_eq!(learned, 0);
    }

    #[test]
    fn a_client_that_never_gets_a_response_does_not_hang_forever() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(250));
        });

        let mut routing = RoutingTable::new(id(1));
        let mut book = AddressBook::new();
        let started = std::time::Instant::now();
        assert!(pex_over_tcp_with_timeout(
            id(1),
            &mut routing,
            &mut book,
            addr,
            Duration::from_millis(30)
        )
        .is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        server.join().unwrap();
    }
}
