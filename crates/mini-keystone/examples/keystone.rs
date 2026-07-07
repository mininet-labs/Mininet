//! Run the keystone demo and print what happened:
//! `cargo run -p mini-keystone --example keystone`

use mini_bearer::pair;
use mini_keystone::{run_demo, Participant};
use mini_presence::TransportKind;

fn main() {
    let alice = Participant::from_seeds([1; 32], [2; 32], [3; 32], [4; 32]).expect("alice");
    let bob = Participant::from_seeds([5; 32], [6; 32], [7; 32], [8; 32]).expect("bob");
    let (mut bearer_a, mut bearer_b) = pair();

    let report = run_demo(
        &alice,
        &bob,
        &mut bearer_a,
        &mut bearer_b,
        TransportKind::InProcess,
        1_000_000,
    )
    .expect("keystone demo");

    println!("Mininet keystone demo — no internet, no server, no identity on the wire");
    println!();
    println!("identity root A : {}", report.initiator_root);
    println!("identity root B : {}", report.responder_root);
    println!(
        "channel : anonymous, forward-secret (binding {:02x?}...)",
        &report.channel_binding[..4]
    );
    println!();
    println!(
        "A accrued {} points ({} vested yet — value matures slowly, P4)",
        report.initiator_account.accrued_points, report.initiator_account.vested_points
    );
    println!(
        "B accrued {} points ({} vested yet)",
        report.responder_account.accrued_points, report.responder_account.vested_points
    );
    println!();
    println!("identity verified offline · presence range-bound & mutually signed ·");
    println!(
        "reward non-spendable, no governance weight (P1) · one identity root, one accrual (P2)"
    );
}
