//! Live multi-process gossip demo: `mini-net`'s dedup-flooding
//! [`GossipRouter`] running over a real TCP transport
//! ([`mini_bearer::TcpBearer`]), across genuinely separate OS processes.
//!
//! ## What "live" means here, honestly
//!
//! Every earlier live demo in this workspace (the keystone demo, the FROST
//! signing demo) ran multiple simulated parties inside one process. This
//! one doesn't: you run three separate `cargo run` invocations, in three
//! separate terminals (or on three separate machines on the same network,
//! unmodified — just point leaves at the hub's real IP instead of
//! `127.0.0.1`). A message injected at one leaf travels over a real TCP
//! socket to the hub, gets deduped and forwarded by [`GossipRouter`]
//! exactly as it would in the real network, and arrives at the other leaf
//! over a second real socket. This is the thing `mini-net`'s own crate
//! docs list as its honest limit: "not yet a running network stack: real
//! transport... pending." This demo is that transport landing, for a hub
//! topology; general peer-to-peer mesh routing (`RoutingTable`) is still a
//! separate piece not exercised here.
//!
//! ## Run it
//!
//! ```sh
//! # terminal 1 -- the hub, waiting for 2 leaves
//! cargo run -p mini-net --example gossip_live_demo -- hub 9000 2
//!
//! # terminal 2 -- a leaf that sends one message
//! cargo run -p mini-net --example gossip_live_demo -- leaf 127.0.0.1:9000 alice --send "hello mininet"
//!
//! # terminal 3 -- a leaf that waits to receive it
//! cargo run -p mini-net --example gossip_live_demo -- leaf 127.0.0.1:9000 bob --expect 1
//! ```
//!
//! Start the hub first, then the leaves in either order. All three
//! processes exit cleanly once the expected traffic has passed.
//!
//! ## Honest limits
//!
//! - **Hub-and-spoke, not a mesh.** Real Mininet gossip is peer-to-peer;
//!   this demo uses a single relay process to keep the process-
//!   orchestration simple. `GossipRouter`'s dedup logic doesn't care how
//!   many neighbors a node has, so this generalizes, but the demo itself
//!   doesn't build a mesh.
//! - **No peer discovery.** Leaves are told the hub's address on the
//!   command line; `RoutingTable` (Kademlia-style discovery) isn't
//!   exercised by this demo.
//! - **No authentication or encryption on the wire.** Same honest limit as
//!   `TcpBearer` itself — a real deployment layers `mini_bearer::Channel`
//!   on top; this demo sends plaintext gossip frames to keep the example
//!   focused on transport + dedup-flooding, not the encrypted channel
//!   (already demonstrated by the keystone demo).

use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use mini_bearer::{Bearer, TcpBearer};
use mini_crypto::HashAlgorithm;
use mini_net::GossipRouter;

/// Wire format for one gossip frame: a 32-byte content-addressed message
/// id (so identical text always dedups, and a receiver never has to trust
/// a sender-supplied id) followed by the UTF-8 payload.
fn encode_message(text: &str) -> Vec<u8> {
    let id = HashAlgorithm::Blake3.digest(text.as_bytes());
    let mut frame = Vec::with_capacity(32 + text.len());
    frame.extend_from_slice(&id);
    frame.extend_from_slice(text.as_bytes());
    frame
}

fn decode_message(frame: &[u8]) -> Option<(&[u8], &str)> {
    if frame.len() < 32 {
        return None;
    }
    let (id, payload) = frame.split_at(32);
    std::str::from_utf8(payload).ok().map(|text| (id, text))
}

fn run_hub(port: u16, expected_leaves: usize) {
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind hub listener");
    println!("[hub] listening on 127.0.0.1:{port}, waiting for {expected_leaves} leaves");

    // Accept every leaf up front -- a fixed, known set for this demo. Each
    // connection is split into an independent receive handle (owned solely
    // by that leaf's reader thread) and an independent send handle (shared
    // for forwarding), via TcpStream::try_clone -- two handles on the same
    // socket, so no thread ever blocks another's turn on a shared lock
    // during a blocking recv().
    let mut recv_handles = Vec::new();
    let mut send_handles = Vec::new();
    for _ in 0..expected_leaves {
        let (stream, addr) = listener.accept().expect("accept leaf connection");
        let recv_stream = stream.try_clone().expect("clone stream for recv half");
        recv_handles.push(TcpBearer::from_stream(recv_stream).expect("wrap recv half"));
        send_handles.push(Arc::new(Mutex::new(
            TcpBearer::from_stream(stream).expect("wrap send half"),
        )));
        println!(
            "[hub] leaf connected from {addr} ({}/{expected_leaves})",
            send_handles.len()
        );
    }
    println!("[hub] all {expected_leaves} leaves connected -- relaying gossip");

    let send_handles = Arc::new(send_handles);
    let router = Arc::new(Mutex::new(GossipRouter::new(1024)));

    let handles: Vec<_> = recv_handles
        .into_iter()
        .enumerate()
        .map(|(id, mut recv_bearer)| {
            let send_handles = Arc::clone(&send_handles);
            let router = Arc::clone(&router);
            thread::spawn(move || loop {
                let frame = match recv_bearer.recv() {
                    Ok(frame) => frame,
                    Err(_) => {
                        println!("[hub] leaf {id} disconnected");
                        return;
                    }
                };
                let Some((msg_id_bytes, text)) = decode_message(&frame) else {
                    println!("[hub] malformed frame from leaf {id}, dropping");
                    continue;
                };
                let mut msg_id = [0u8; 32];
                msg_id.copy_from_slice(msg_id_bytes);

                let is_new = router.lock().unwrap().record_seen(msg_id);
                if !is_new {
                    println!("[hub] duplicate of an already-seen message from leaf {id}, dropping");
                    continue;
                }
                println!(
                    "[hub] new message from leaf {id}: {text:?} -- forwarding to other leaves"
                );
                for (peer_id, sender) in send_handles.iter().enumerate() {
                    if peer_id == id {
                        continue; // never forward back to where it came from
                    }
                    if let Err(e) = sender.lock().unwrap().send(&frame) {
                        println!("[hub] failed to forward to leaf {peer_id}: {e}");
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        let _ = handle.join();
    }
    println!("[hub] all leaves disconnected -- exiting");
}

fn run_leaf(hub_addr: &str, label: &str, send_text: Option<&str>, expect: usize) {
    let stream = TcpStream::connect(hub_addr).expect("connect to hub");
    let recv_stream = stream.try_clone().expect("clone stream for recv half");
    let mut recv_bearer = TcpBearer::from_stream(recv_stream).expect("wrap recv half");
    let mut send_bearer = TcpBearer::from_stream(stream).expect("wrap send half");
    println!("[leaf {label}] connected to hub at {hub_addr}");

    if let Some(text) = send_text {
        // Give sibling leaves a moment to connect too, so the demo's
        // forwarding is visible even when all processes are started at
        // roughly the same time.
        thread::sleep(Duration::from_millis(400));
        println!("[leaf {label}] sending: {text:?}");
        send_bearer
            .send(&encode_message(text))
            .expect("send gossip message");
    }

    for i in 0..expect {
        let frame = recv_bearer.recv().expect("receive gossiped message");
        match decode_message(&frame) {
            Some((_, text)) => {
                println!(
                    "[leaf {label}] received via gossip ({}/{expect}): {text:?}",
                    i + 1
                );
            }
            None => println!("[leaf {label}] received a malformed frame, ignoring"),
        }
    }
    println!("[leaf {label}] done -- exiting");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("hub") => {
            let port: u16 = args
                .get(2)
                .expect("usage: hub <port> <expected-leaves>")
                .parse()
                .expect("port must be a number");
            let expected_leaves: usize = args
                .get(3)
                .expect("usage: hub <port> <expected-leaves>")
                .parse()
                .expect("expected-leaves must be a number");
            run_hub(port, expected_leaves);
        }
        Some("leaf") => {
            let hub_addr = args
                .get(2)
                .expect("usage: leaf <hub-addr> <label> [--send TEXT] [--expect N]");
            let label = args
                .get(3)
                .expect("usage: leaf <hub-addr> <label> [--send TEXT] [--expect N]");
            let mut send_text = None;
            let mut expect = 0usize;
            let mut i = 4;
            while i < args.len() {
                match args[i].as_str() {
                    "--send" => {
                        send_text = Some(args.get(i + 1).expect("--send needs a message").as_str());
                        i += 2;
                    }
                    "--expect" => {
                        expect = args
                            .get(i + 1)
                            .expect("--expect needs a count")
                            .parse()
                            .expect("--expect count must be a number");
                        i += 2;
                    }
                    other => {
                        eprintln!("unrecognized argument: {other}");
                        std::process::exit(1);
                    }
                }
            }
            run_leaf(hub_addr, label, send_text, expect);
        }
        _ => {
            eprintln!(
                "usage:\n  gossip_live_demo hub <port> <expected-leaves>\n  gossip_live_demo leaf <hub-addr> <label> [--send TEXT] [--expect N]"
            );
            std::process::exit(1);
        }
    }
}
