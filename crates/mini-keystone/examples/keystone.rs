//! Run the keystone demo and print what happened:
//! `cargo run -p mini-keystone --example keystone`
//!
//! ## Why this prints per-participant, not a shared log
//!
//! In reality Alice and Bob are on two separate phones; nobody — no server,
//! no third observer — ever sees both sides' identity + reward data
//! together in one place. This demo runs both devices in one process only
//! for convenience, but it deliberately prints **two separate reports**,
//! each showing only what that one device would show its own owner, to
//! model the real boundary: a human's identity root and reward accrual are
//! their own device's business, visible to them and to whoever they
//! directly transacted with (who already learned it during the mutual
//! handshake) — never casually aggregated into one shared view. Real
//! application code must not build a "both sides at once" log or dashboard
//! from this data; the constitution's "no component can unmask a user" and
//! "users are sovereign over their own data" hold for demo/example code too.

use mini_bearer::pair;
use mini_keystone::{run_demo, Participant};
use mini_presence::{FileReplayGuard, TransportKind};

fn print_own_device_view(
    owner_label: &str,
    own_root: &str,
    own_account: &mini_reward::RewardAccount,
) {
    println!("--- what {owner_label}'s own device shows {owner_label} ---");
    println!("my identity root : {own_root}");
    println!(
        "my accrued reward: {} points ({} vested — value matures slowly, P4)",
        own_account.accrued_points, own_account.vested_points
    );
    println!("(reward is non-spendable, carries no governance weight — P1)");
    println!();
}

fn main() {
    let alice = Participant::from_seeds([1; 32], [2; 32], [3; 32], [4; 32]).expect("alice");
    let bob = Participant::from_seeds([5; 32], [6; 32], [7; 32], [8; 32]).expect("bob");
    let (mut bearer_a, mut bearer_b) = pair();

    // Each side owns its own durable replay guard, backed by its own file --
    // exactly as two separate phones would each hold their own on-device
    // state (D-0366). Real replay resistance across app restarts requires a
    // caller to open these at a persistent path, not a fresh temp file per
    // run as this demo does for convenience.
    let mut guard_alice = FileReplayGuard::open(
        std::env::temp_dir().join("mini-keystone-demo-alice.log"),
        60_000,
    )
    .expect("alice's replay guard");
    let mut guard_bob = FileReplayGuard::open(
        std::env::temp_dir().join("mini-keystone-demo-bob.log"),
        60_000,
    )
    .expect("bob's replay guard");

    let report = run_demo(
        &alice,
        &bob,
        &mut bearer_a,
        &mut bearer_b,
        TransportKind::InProcess,
        1_000_000,
        &mut guard_alice,
        &mut guard_bob,
    )
    .expect("keystone demo");

    println!("Mininet keystone demo — no internet, no server, no identity on the wire");
    println!(
        "channel : anonymous, forward-secret (binding {:02x?}...)",
        &report.channel_binding[..4]
    );
    println!();
    print_own_device_view("Alice", &report.initiator_root, &report.initiator_account);
    print_own_device_view("Bob", &report.responder_root, &report.responder_account);
    println!("identity verified offline · presence range-bound & mutually signed ·");
    println!("one identity root, one accrual (P2)");
}
