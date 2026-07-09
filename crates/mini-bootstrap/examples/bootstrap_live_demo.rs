//! Live two-process bootstrap demo: a genuinely fresh device — empty store,
//! zero prior trust — bootstraps a signed genesis capsule from a seed peer
//! over a real TCP socket, closing [roadmap #23](../../issues/23)
//! ("Bootstrap protocol over real transport").
//!
//! ## What this proves, and how, honestly
//!
//! `mini-bootstrap` is deliberately transport-agnostic (see its crate docs)
//! — it never gets its own wire protocol here. Instead this demo composes
//! three already-real pieces exactly as a real device would: the seed peer
//! sends its [`mini_bootstrap::GenesisSeed`] first (standing in for a BLE
//! advertisement), the two sides then handshake a
//! [`mini_bearer::Channel`] ([`mini_bearer::Initiator`]/
//! [`mini_bearer::Responder`], proven live over TCP by the keystone demo),
//! and finally run `mini_sync::sync_bidirectional` — ordinary bucketed set
//! reconciliation — to pull everything. This works because a capsule
//! header, its bundle manifest, and every chunk are just
//! `mini_objects::Object`s in a `mini_store::Store`: nothing bootstrap-
//! specific needs to exist in the wire protocol itself, only the
//! seed-then-verify discipline around it. The fresh device's `KelCache`
//! starts genuinely empty — trust bootstraps entirely from the KEL-carrier
//! objects it pulls over the wire, self-certifying as they arrive.
//!
//! ## Run it
//!
//! ```sh
//! # terminal 1 -- the seed peer, already holding a published genesis capsule
//! cargo run -p mini-bootstrap --example bootstrap_live_demo -- seed 9100
//!
//! # terminal 2 -- a fresh device with zero prior state
//! cargo run -p mini-bootstrap --example bootstrap_live_demo -- fresh 127.0.0.1:9100
//! ```
//!
//! The fresh device prints the reassembled bundle's length and BLAKE3
//! digest; compare against the seed peer's own printed digest to confirm
//! byte-identical reconstruction over the real connection.
//!
//! ## Honest limits
//!
//! - **TCP stands in for BLE.** The seed peer sends its `GenesisSeed` as
//!   the connection's first frame rather than a real BLE advertisement —
//!   the verification logic this demo exercises (seed pins the capsule
//!   hash *before* the receiver trusts anything larger) is identical
//!   either way; only the radio is simulated, per issue #22's own note
//!   that real BLE/Wi-Fi adapters need actual phone hardware this
//!   environment doesn't have.
//! - **One connection, one capsule.** No peer discovery, no multi-peer
//!   store-and-forward resumption across many short encounters (that's
//!   `mini-sync`'s own robustness scope, roadmap #26) — this demo proves
//!   the pieces interoperate over a real socket, not the full field
//!   scenario.

use std::net::{TcpListener, TcpStream};

use did_mini::{Capabilities, Controller};
use mini_bearer::{Bearer, Channel, Initiator, Responder, TcpBearer};
use mini_bootstrap::{
    assemble_capsule, capsule_hash, capsule_want_list, publish_capsule, read_capsule_header,
    seed_for, verify_header_matches_seed, CapsuleKind, PeerCard,
};
use mini_crypto::HashAlgorithm;
use mini_objects::ObjectType;
use mini_store::{MemoryBackend, Store};
use mini_sync::{kel_carrier, sync_bidirectional, KelCache, SyncRole};

const BUNDLE_BYTES: &[u8] = b"mininet genesis bootstrap bundle -- real content lives in a real release, this demo just needs *some* bytes to chunk, publish, and prove reassemble byte-identically over a real socket";

fn publisher() -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[201u8; 32], &[202u8; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[203u8; 32], &[204u8; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn handshake_responder(bearer: &mut TcpBearer) -> Channel {
    let hello = bearer.recv().expect("receive initiator hello");
    let (chan, response) = Responder::respond(&hello).expect("respond to handshake");
    bearer.send(&response).expect("send responder hello");
    chan
}

fn handshake_initiator(bearer: &mut TcpBearer) -> Channel {
    let (init, hello) = Initiator::start().expect("start handshake");
    bearer.send(&hello).expect("send initiator hello");
    let response = bearer.recv().expect("receive responder hello");
    init.finish(&response).expect("finish handshake")
}

fn run_seed(port: u16) {
    let (root, device) = publisher();
    let mut store: Store<MemoryBackend> = Store::new(MemoryBackend::new());

    // Publish the human/device KELs as ordinary carrier objects so a
    // fresh peer's ingest pipeline can bootstrap trust from the wire --
    // no pre-shared identity, exactly the real "zero prior state" case.
    store
        .insert(&kel_carrier(&root.kel(), &root.did(), &device).unwrap())
        .unwrap();
    store
        .insert(&kel_carrier(&device.kel(), &root.did(), &device).unwrap())
        .unwrap();

    let header = publish_capsule(
        &mut store,
        &root.did(),
        &device,
        CapsuleKind::Genesis,
        [7u8; 16],
        HashAlgorithm::Blake3.digest(b"mininet-constitution-v1"),
        1,
        "application/octet-stream",
        BUNDLE_BYTES,
        1_000,
        0,
    )
    .expect("publish genesis capsule");

    let header_obj = store.get(&header.id).expect("fetch published header");
    let seed = seed_for(
        &header_obj,
        &header,
        PeerCard {
            protocol_tag: 1,
            chain_id_prefix: [7, 7, 7, 7],
            capsule_hash_prefix: capsule_hash(&header_obj)[..8].try_into().unwrap(),
            device_key_hash: HashAlgorithm::Blake3.digest(device.did().as_str().as_bytes()),
        },
    );
    println!(
        "[seed] published genesis capsule, {} bytes, digest {}",
        BUNDLE_BYTES.len(),
        hex(&HashAlgorithm::Blake3.digest(BUNDLE_BYTES))
    );

    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind seed listener");
    println!("[seed] listening on 127.0.0.1:{port}, waiting for a fresh device");
    let (stream, addr) = listener.accept().expect("accept fresh device");
    println!("[seed] fresh device connected from {addr}");
    let mut bearer = TcpBearer::from_stream(stream).expect("wrap stream");

    bearer
        .send(&seed.to_bytes())
        .expect("advertise genesis seed");
    println!("[seed] sent GenesisSeed (standing in for a BLE advertisement)");

    let mut chan = handshake_responder(&mut bearer);
    println!("[seed] channel established -- serving the fresh device's pull");
    sync_bidirectional(
        &mut bearer,
        &mut chan,
        &mut store,
        &mut KelCache::new(),
        SyncRole::Responder,
    )
    .expect("serve sync");
    println!("[seed] sync complete -- exiting");
}

fn run_fresh(seed_addr: &str) {
    let stream = TcpStream::connect(seed_addr).expect("connect to seed peer");
    let mut bearer = TcpBearer::from_stream(stream).expect("wrap stream");
    println!("[fresh] connected to seed peer at {seed_addr}, zero prior state");

    let seed_frame = bearer.recv().expect("receive genesis seed");
    let seed = mini_bootstrap::GenesisSeed::from_bytes(&seed_frame).expect("decode genesis seed");
    println!(
        "[fresh] received GenesisSeed pinning capsule hash {}",
        hex(&seed.capsule_hash)
    );

    let mut chan = handshake_initiator(&mut bearer);
    println!("[fresh] channel established -- pulling everything the seed peer has");

    let mut store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
    let mut cache = KelCache::new(); // genuinely empty: no prior trust at all
    let report = sync_bidirectional(
        &mut bearer,
        &mut chan,
        &mut store,
        &mut cache,
        SyncRole::Initiator,
    )
    .expect("pull from seed peer");
    println!(
        "[fresh] sync complete: {} objects received, {} accepted, {} KEL carriers absorbed",
        report.received, report.accepted, report.carriers
    );

    // Find the capsule header the seed advertised -- pinned by hash, not
    // trusted merely because it decoded (see verify_header_matches_seed).
    let mut header = None;
    for id in store.all_ids().expect("list synced object ids") {
        let obj = store.get(&id).expect("fetch synced object");
        if obj.object_type != ObjectType::Custom("mini/genesis-capsule".to_string()) {
            continue;
        }
        if verify_header_matches_seed(&obj, &seed).is_ok() {
            header = Some(read_capsule_header(&obj).expect("parse verified capsule header"));
            break;
        }
    }
    let header = header.expect("the advertised capsule header must be among the synced objects");
    println!("[fresh] verified capsule header matches the advertised seed");

    let remaining = capsule_want_list(&store, &header).expect("compute remaining want-list");
    assert!(
        remaining.is_empty(),
        "bucketed sync should have already pulled the full bundle"
    );

    let bytes = assemble_capsule(&store, &header).expect("reassemble and digest-verify bundle");
    println!(
        "[fresh] reassembled bundle: {} bytes, digest {}",
        bytes.len(),
        hex(&HashAlgorithm::Blake3.digest(&bytes))
    );
    println!("[fresh] bootstrap complete over a real TCP connection -- exiting");
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("seed") => {
            let port: u16 = args
                .get(2)
                .expect("usage: seed <port>")
                .parse()
                .expect("port must be a number");
            run_seed(port);
        }
        Some("fresh") => {
            let seed_addr = args.get(2).expect("usage: fresh <seed-addr>");
            run_fresh(seed_addr);
        }
        _ => {
            eprintln!("usage: bootstrap_live_demo <seed <port> | fresh <seed-addr>>");
            std::process::exit(1);
        }
    }
}
