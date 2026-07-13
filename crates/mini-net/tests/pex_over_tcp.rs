//! Proves `mini_net::pex`'s whole point over a real socket: a node
//! supplied only one peer's address discovers a *second* peer's address
//! purely through peer exchange, and the discovered address is actually
//! dialable — not just present in a data structure.
//!
//! Founder review P1 backlog item "Invitation and peer-exchange discovery
//! with no required central server." Topology: A knows only B's address.
//! B knows both A (once A connects) and C (supplied ahead of time, as if
//! from an earlier encounter). A sends one PEX request to B; B answers
//! with C's record (excluding A itself); A absorbs it, and is proven to
//! know C's dialable address — an address A was never told directly by
//! any command-line argument or prior step, only by B vouching for it.

use std::net::{TcpListener, TcpStream};
use std::thread;

use mini_bearer::{Bearer, TcpBearer};
use mini_net::{absorb_response, build_response, AddressBook, PeerId, PexMessage, RoutingTable};

#[test]
fn a_node_discovers_a_second_peers_address_purely_through_pex_over_real_tcp() {
    // C: a third node, listening, known to B ahead of time (as if from an
    // earlier encounter), never told to A directly.
    let c_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let c_addr = c_listener.local_addr().unwrap();
    let c_id = PeerId::generate().unwrap();
    let c_thread = thread::spawn(move || {
        // C just needs to be dialable once A discovers it via PEX.
        let (_stream, _) = c_listener.accept().unwrap();
    });

    // B: knows C already (address book seeded as if from a prior PEX
    // round or direct connection), and will answer A's PEX request.
    let b_id = PeerId::generate().unwrap();
    let mut b_routing = RoutingTable::new(b_id);
    let mut b_book = AddressBook::new();
    b_routing.insert(c_id);
    b_book.insert(c_id, c_addr);

    let b_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let b_addr = b_listener.local_addr().unwrap();
    let b_thread = thread::spawn(move || {
        let (stream, _peer_addr) = b_listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();

        let request_bytes = bearer.recv().unwrap();
        let PexMessage::Request(requester_id) = PexMessage::decode(&request_bytes).unwrap() else {
            panic!("expected a Request");
        };

        // B learns A too, from this exchange -- paired with the live
        // connection's own observed source address, never a claim inside
        // the message (see pex.rs's trust-model doc).
        absorb_response(
            &[mini_net::PeerRecord {
                id: requester_id,
                addr: _peer_addr,
            }],
            &mut b_routing,
            &mut b_book,
        );

        let response = build_response(&b_routing, &b_book, &requester_id);
        bearer.send(&response.encode()).unwrap();
    });

    // A: knows only B's address up front. Never told C's address by
    // anything except the PEX exchange below.
    let a_id = PeerId::generate().unwrap();
    let mut a_routing = RoutingTable::new(a_id);
    let mut a_book = AddressBook::new();
    assert!(
        a_book.get(&c_id).is_none(),
        "A must not know C's address before the exchange"
    );

    let stream = TcpStream::connect(b_addr).unwrap();
    let mut bearer = TcpBearer::from_stream(stream).unwrap();
    bearer.send(&PexMessage::Request(a_id).encode()).unwrap();
    let response_bytes = bearer.recv().unwrap();
    let PexMessage::Response(records) = PexMessage::decode(&response_bytes).unwrap() else {
        panic!("expected a Response");
    };
    absorb_response(&records, &mut a_routing, &mut a_book);

    b_thread.join().unwrap();

    // A now knows C -- an address it was never handed directly.
    assert!(a_routing.contains(&c_id));
    let discovered_addr = a_book
        .get(&c_id)
        .expect("A should have learned C's address purely through PEX");
    assert_eq!(discovered_addr, c_addr);

    // And it's not just a data structure entry -- the discovered address
    // is actually dialable, over a fresh real socket.
    TcpStream::connect(discovered_addr).expect("the discovered address must actually be dialable");
    c_thread.join().unwrap();
}

#[test]
fn pex_never_hands_the_requester_back_its_own_record_over_real_tcp() {
    let b_id = PeerId::generate().unwrap();
    let mut b_routing = RoutingTable::new(b_id);

    let b_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let b_addr = b_listener.local_addr().unwrap();
    let b_thread = thread::spawn(move || {
        let (stream, peer_addr) = b_listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let request_bytes = bearer.recv().unwrap();
        let PexMessage::Request(requester_id) = PexMessage::decode(&request_bytes).unwrap() else {
            panic!("expected a Request");
        };
        // B learns the requester before answering -- if `exclude` didn't
        // work, the very next response would hand A back its own record.
        b_routing.insert(requester_id);
        let mut book = AddressBook::new();
        book.insert(requester_id, peer_addr);
        let response = build_response(&b_routing, &book, &requester_id);
        bearer.send(&response.encode()).unwrap();
    });

    let a_id = PeerId::generate().unwrap();
    let stream = TcpStream::connect(b_addr).unwrap();
    let mut bearer = TcpBearer::from_stream(stream).unwrap();
    bearer.send(&PexMessage::Request(a_id).encode()).unwrap();
    let response_bytes = bearer.recv().unwrap();
    let PexMessage::Response(records) = PexMessage::decode(&response_bytes).unwrap() else {
        panic!("expected a Response");
    };

    b_thread.join().unwrap();
    assert!(
        records.iter().all(|r| r.id != a_id),
        "a PEX response must never include the requester's own record"
    );
}
