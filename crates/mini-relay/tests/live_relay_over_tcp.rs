//! Live two-hop relay demo: proves `RelayEnvelope`/`Channel`/`TcpBearer`
//! actually compose over **real TCP sockets** across independently-
//! established hops — not just the in-process `Channel` pairs
//! `envelope.rs`'s own unit tests use (those call `Initiator::start`/
//! `Responder::respond` directly with hello bytes passed as function
//! arguments, never touching a socket at all). Closes the "no live demo
//! yet" honest limit D-0306/D-0307 both named as required follow-up.
//!
//! ## Topology and honest model
//!
//! Client → Entry relay → Rendezvous relay, over two independent real
//! TCP connections, each with its own genuine `mini_bearer::Channel`
//! handshake (X25519 + HKDF-SHA256 + ChaCha20-Poly1305). `RelayEnvelope`
//! is, by its own doc comment, "a **one-hop** AEAD-sealed relay message"
//! — the intended usage is unwrap-then-reseal at each hop, not nested
//! onion layering. That means the entry relay necessarily sees the
//! plaintext it forwards: this is a **hop-by-hop store-and-forward
//! model**, matching Tier 1's research-doc scope (§5.2, "relay +
//! rendezvous," ~2-4x cost) — not Tier 2's stronger layered-mix property
//! (Sphinx-style onion encryption, §5.3, `MN-205`, still gated behind
//! external review, D-0047/D-0305). What Tier 1 *does* still buy: no
//! single party learns both the client's real network address and the
//! final message's rendezvous mailbox — entry knows the client's address
//! but relays blindly to a rendezvous address it's configured with,
//! rendezvous knows the mailbox but never dials the client directly.
//!
//! ## What this demo does not prove
//!
//! Mailbox pickup (`MailboxGrant`/`MailboxToken`/holder-proof validation)
//! is pure, local logic that doesn't care whether its inputs arrived over
//! a socket or a function call — already exercised 21 times in
//! `mailbox.rs`'s own unit tests. Re-proving it over a third TCP hop here
//! would add wire-format code without adding truth-value, so this demo
//! stays focused on the one genuinely new slice: real multi-hop relay
//! transport.

use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;

use did_mini::Controller;
use mini_bearer::{Bearer, Initiator, Responder, TcpBearer};
use mini_relay::{
    derive_relay_identity, enforce_role_separation, ConnectionId, DeliveryAssignment,
    RelayEnvelope, RelayRole,
};
use mini_transport_policy::PayloadSizeClass;

#[test]
fn a_message_crosses_entry_and_rendezvous_hops_over_real_tcp_sockets() {
    let connection_id = ConnectionId::generate().unwrap();
    let final_message = b"hello via two real relay hops";

    // Distinct relay-operator roots, with per-role identities derived the
    // same way a real deployment would -- proving role separation on
    // genuinely independent identities, not placeholder stand-ins.
    let entry_operator = Controller::incept_single().unwrap();
    let rendezvous_operator = Controller::incept_single().unwrap();
    let entry_identity =
        derive_relay_identity(&entry_operator, RelayRole::Entry, connection_id).unwrap();
    let rendezvous_identity =
        derive_relay_identity(&rendezvous_operator, RelayRole::Rendezvous, connection_id).unwrap();
    enforce_role_separation(&[
        DeliveryAssignment {
            role: RelayRole::Entry,
            relay: entry_identity.did(),
        },
        DeliveryAssignment {
            role: RelayRole::Rendezvous,
            relay: rendezvous_identity.did(),
        },
    ])
    .expect("two distinct relay identities must satisfy role separation");

    // --- Rendezvous relay: real TCP listener, real Channel handshake,
    // real RelayEnvelope decode+open over the socket. ---
    let rendezvous_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let rendezvous_addr = rendezvous_listener.local_addr().unwrap();
    let (rendezvous_result_tx, rendezvous_result_rx) = mpsc::channel();
    let rendezvous_thread = thread::spawn(move || {
        let (stream, _) = rendezvous_listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let hello = bearer.recv().unwrap();
        let (mut channel, response) = Responder::respond(&hello).unwrap();
        bearer.send(&response).unwrap();

        let frame = bearer.recv().unwrap();
        let envelope = RelayEnvelope::from_bytes(&frame).unwrap();
        assert_eq!(envelope.role, RelayRole::Rendezvous);
        assert_eq!(envelope.connection_id, connection_id);
        assert_eq!(envelope.size_class, PayloadSizeClass::Small);
        let plaintext = envelope.open(&mut channel).unwrap();
        rendezvous_result_tx.send(plaintext).unwrap();
    });

    // --- Entry relay: accepts the client over one real TCP connection,
    // then independently dials the rendezvous relay over a second real
    // TCP connection with its own fresh Channel handshake. ---
    let entry_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let entry_addr = entry_listener.local_addr().unwrap();
    let entry_thread = thread::spawn(move || {
        let (client_stream, _) = entry_listener.accept().unwrap();
        let mut client_bearer = TcpBearer::from_stream(client_stream).unwrap();
        let hello = client_bearer.recv().unwrap();
        let (mut client_channel, response) = Responder::respond(&hello).unwrap();
        client_bearer.send(&response).unwrap();

        let frame = client_bearer.recv().unwrap();
        let entry_envelope = RelayEnvelope::from_bytes(&frame).unwrap();
        assert_eq!(entry_envelope.role, RelayRole::Entry);
        assert_eq!(entry_envelope.connection_id, connection_id);
        // Entry decrypts its own hop -- it necessarily sees this
        // plaintext (the hop-by-hop model this test's module doc names).
        let plaintext = entry_envelope.open(&mut client_channel).unwrap();

        // A second, fully independent real TCP connection + Channel
        // handshake toward the rendezvous relay -- distinct key material
        // from the client hop, proving hops don't share transport state.
        let rendezvous_stream = TcpStream::connect(rendezvous_addr).unwrap();
        let mut rendezvous_bearer = TcpBearer::from_stream(rendezvous_stream).unwrap();
        let (initiator, hello) = Initiator::start().unwrap();
        rendezvous_bearer.send(&hello).unwrap();
        let response = rendezvous_bearer.recv().unwrap();
        let mut rendezvous_channel = initiator.finish(&response).unwrap();

        let forwarded = RelayEnvelope::seal(
            &mut rendezvous_channel,
            RelayRole::Rendezvous,
            connection_id,
            PayloadSizeClass::Small,
            &plaintext,
        )
        .unwrap();
        rendezvous_bearer.send(&forwarded.to_bytes()).unwrap();
    });

    // --- Client: dials entry over a real TCP connection, real Channel
    // handshake, seals the message under a real RelayEnvelope. ---
    let client_stream = TcpStream::connect(entry_addr).unwrap();
    let mut client_bearer = TcpBearer::from_stream(client_stream).unwrap();
    let (initiator, hello) = Initiator::start().unwrap();
    client_bearer.send(&hello).unwrap();
    let response = client_bearer.recv().unwrap();
    let mut client_channel = initiator.finish(&response).unwrap();

    let envelope = RelayEnvelope::seal(
        &mut client_channel,
        RelayRole::Entry,
        connection_id,
        PayloadSizeClass::Small,
        final_message,
    )
    .unwrap();
    client_bearer.send(&envelope.to_bytes()).unwrap();

    entry_thread.join().unwrap();
    rendezvous_thread.join().unwrap();
    let received = rendezvous_result_rx.recv().unwrap();
    assert_eq!(
        received, final_message,
        "the message must arrive at the rendezvous relay byte-for-byte, \
         after crossing two independently-established real TCP+Channel hops"
    );
}
