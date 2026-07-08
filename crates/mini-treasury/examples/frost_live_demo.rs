//! Live multi-device FROST treasury-custody signing demo.
//!
//! Run it with:
//!
//! ```sh
//! cargo run -p mini-treasury --example frost_live_demo
//! ```
//!
//! ## What "live" means here, honestly
//!
//! This runs five genuinely separate OS threads, one per committee device,
//! each holding **only its own** [`mini_treasury::KeyPackage`] (moved into
//! that thread, never shared with any other thread) and talking to a
//! coordinator thread exclusively through `std::sync::mpsc` channels — the
//! same request/response shape a real network transport would carry. No
//! function call in this program has access to more than one device's
//! secret share at once; the group secret key itself is never
//! reconstructed anywhere after the (explicitly-labeled, trusted-dealer)
//! keygen step.
//!
//! What this demo is **not**: separate physical machines, a real network
//! (mini-net's transport isn't wired to this yet), or DKG keygen (see
//! `mini_treasury`'s crate docs and `frost_keygen`'s honest limit). Treat it
//! as proof that the *signing protocol's* device-isolation and threshold
//! properties hold under real concurrent message-passing, not as a
//! deployment.
//!
//! ## What it demonstrates
//!
//! 1. A 3-of-5 committee signs a treasury payout using only 3 of the 5
//!    devices online — the other two are never even asked to participate.
//! 2. A second signing session where one device's reported response is
//!    tampered with in transit (simulating a compromised or faulty
//!    device): the coordinator's per-share verification catches it and
//!    attributes it to that device, *before* producing any signature —
//!    it never silently emits a bad aggregate.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use mini_treasury::{
    aggregate, round1_commit, round2_sign, trusted_dealer_keygen, verify, KeyPackage,
    NonceCommitment, PublicKeyPackage, Signature, SigningPackage,
};

// mini_treasury deliberately keeps its curve module private (see
// mini_treasury::curve's docs); pull the scalar type in directly here, the
// one place this demo needs to construct a tampering offset.
use curve25519_dalek::scalar::Scalar;

const PAUSE: Duration = Duration::from_millis(180);

fn step(msg: impl AsRef<str>) {
    println!("{}", msg.as_ref());
    thread::sleep(PAUSE);
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

enum ToDevice {
    Round1Request,
    Round2Request(SigningPackage),
    Shutdown,
}

enum ToCoordinator {
    Round1Response {
        index: u16,
        commitment: Box<NonceCommitment>,
    },
    Round2Response {
        index: u16,
        z: Scalar,
    },
}

/// One committee device's whole world: its own key share, a channel to
/// receive instructions, and a channel to report back. It never sees any
/// other device's secret material.
fn device_thread(key_package: KeyPackage, rx: Receiver<ToDevice>, tx: Sender<ToCoordinator>) {
    let index = key_package.index;
    let mut pending_nonces = None;

    while let Ok(message) = rx.recv() {
        match message {
            ToDevice::Round1Request => {
                let (nonces, commitment) = round1_commit(index).expect("device entropy source");
                pending_nonces = Some(nonces);
                println!("  [device {index}] round 1: generated a fresh nonce commitment");
                let _ = tx.send(ToCoordinator::Round1Response {
                    index,
                    commitment: Box::new(commitment),
                });
            }
            ToDevice::Round2Request(signing_package) => {
                let nonces = pending_nonces
                    .take()
                    .expect("coordinator must request round 1 before round 2");
                let z = round2_sign(&key_package, &nonces, &signing_package)
                    .expect("this device took part in round 1");
                println!("  [device {index}] round 2: computed its signature share");
                let _ = tx.send(ToCoordinator::Round2Response { index, z });
            }
            ToDevice::Shutdown => break,
        }
    }
}

struct Committee {
    device_txs: HashMap<u16, Sender<ToDevice>>,
    coord_rx: Receiver<ToCoordinator>,
    handles: Vec<thread::JoinHandle<()>>,
}

fn spawn_committee(key_packages: Vec<KeyPackage>) -> Committee {
    let (coord_tx, coord_rx) = mpsc::channel::<ToCoordinator>();
    let mut device_txs = HashMap::new();
    let mut handles = Vec::new();

    for key_package in key_packages {
        let index = key_package.index;
        let (tx, rx) = mpsc::channel::<ToDevice>();
        let coord_tx = coord_tx.clone();
        handles.push(thread::spawn(move || {
            device_thread(key_package, rx, coord_tx)
        }));
        device_txs.insert(index, tx);
    }

    Committee {
        device_txs,
        coord_rx,
        handles,
    }
}

/// Run one full 2-round signing session with the given online device
/// subset. If `tamper_index` is `Some(i)`, device `i`'s round-2 response is
/// corrupted in transit before aggregation, to demonstrate live fault
/// attribution rather than only the happy path.
fn run_signing_session(
    committee: &Committee,
    label: &str,
    message: &[u8],
    online: &[u16],
    threshold: u16,
    public: &PublicKeyPackage,
    tamper_index: Option<u16>,
) {
    println!("\n--- {label} ---");
    step(format!(
        "[coordinator] round 1: requesting nonce commitments from devices {online:?} ({}-of-{} committee)",
        threshold,
        committee.device_txs.len()
    ));

    for &index in online {
        committee.device_txs[&index]
            .send(ToDevice::Round1Request)
            .unwrap();
    }
    let mut commitments = Vec::new();
    for _ in online {
        match committee.coord_rx.recv().unwrap() {
            ToCoordinator::Round1Response { index, commitment } => {
                println!("  [coordinator] received device {index}'s round-1 commitment");
                commitments.push(*commitment);
            }
            ToCoordinator::Round2Response { .. } => unreachable!("round 2 requested too early"),
        }
    }
    step("[coordinator] round 1 complete: threshold reached, moving to round 2");

    let signing_package = SigningPackage::new(threshold, message.to_vec(), commitments).unwrap();
    step("[coordinator] round 2: broadcasting the signing package (message + all commitments)");
    for &index in online {
        committee.device_txs[&index]
            .send(ToDevice::Round2Request(signing_package.clone()))
            .unwrap();
    }

    let mut shares = std::collections::BTreeMap::new();
    for _ in online {
        match committee.coord_rx.recv().unwrap() {
            ToCoordinator::Round2Response { index, z } => {
                println!("  [coordinator] received device {index}'s signature share");
                shares.insert(index, z);
            }
            ToCoordinator::Round1Response { .. } => unreachable!("round 1 already finished"),
        }
    }

    if let Some(bad_index) = tamper_index {
        println!(
            "  [ADVERSARIAL TEST] tampering with device {bad_index}'s share in transit \
             (simulating a compromised or faulty device)"
        );
        *shares.get_mut(&bad_index).unwrap() += Scalar::ONE;
    }

    step("[coordinator] verifying every share individually, then aggregating");
    match aggregate(&signing_package, &shares, public) {
        Ok(signature) => {
            report_result(signature, message, public);
        }
        Err(err) => {
            println!("  [coordinator] REJECTED before producing any signature: {err}");
            println!(
                "  [coordinator] no aggregate signature was ever formed -- a bad or malicious \
                 share never gets a free forgery attempt"
            );
        }
    }
}

fn report_result(signature: Signature, message: &[u8], public: &PublicKeyPackage) {
    println!(
        "  [coordinator] final signature: {}",
        hex(&signature.to_bytes())
    );
    let ok = verify(&signature, message, public.group_public_key);
    println!(
        "  [coordinator] independent verification against the group public key: {}",
        if ok { "VALID" } else { "INVALID" }
    );
    assert!(
        ok,
        "a signature that passed per-share verification must also verify as a whole"
    );
}

fn main() {
    println!("=== Mininet FROST live multi-device treasury signing demo ===");
    println!(
        "(D-0037/D-0041 -- founder-reviewed, AI-authored prototype, pending external audit)\n"
    );

    let n = 5;
    let threshold = 3;
    step(format!(
        "[dealer] generating a {n}-signer, {threshold}-of-{n} threshold group key"
    ));
    let (key_packages, public) = trusted_dealer_keygen(n, threshold).unwrap();
    step(format!(
        "[dealer] group public key: {}",
        hex(public.group_public_key.compress().as_bytes())
    ));
    step("[dealer] distributing one key share to each device thread -- no thread receives another's share");

    let committee = spawn_committee(key_packages);

    run_signing_session(
        &committee,
        "Session 1: 3-of-5 payout, two devices offline",
        b"treasury payout #1: mint MINI for a 12 BTC-equivalent contribution",
        &[1, 3, 5],
        threshold,
        &public,
        None,
    );

    run_signing_session(
        &committee,
        "Session 2: adversarial test -- a tampered share must be caught, not just fail silently",
        b"treasury payout #2: mint MINI for a 4 XMR-equivalent contribution",
        &[1, 2, 4],
        threshold,
        &public,
        Some(2),
    );

    for tx in committee.device_txs.values() {
        let _ = tx.send(ToDevice::Shutdown);
    }
    for handle in committee.handles {
        let _ = handle.join();
    }

    println!("\n=== Demo complete ===");
}
