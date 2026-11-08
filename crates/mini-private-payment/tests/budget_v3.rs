//! T22: worst-case V3 verification cost. Run on the target low-end device with
//! `cargo test -p mini-private-payment --release --test budget_v3 -- --ignored
//! --nocapture` and record the printed numbers in the release evidence packet.
//! Ignored by default: a wall-clock bound in CI would be flaky, and the number
//! that matters is the one measured on the acceptance-baseline device.

use std::time::Instant;

use mini_private_payment::{
    build_v3, quote_fee_micro, verify_v3, InMemoryOutputSet, OutputSet, PaymentPurpose,
    PaymentRequestV3, RecipientV3, SpendableOutputV3, MAX_CLAIM_V3_BYTES, MAX_INPUTS_V3,
    MAX_OUTPUTS_V3, RING_SIZE_V3,
};
use mini_value::{pedersen_commitment_v2, StealthKeypair};

const NETWORK: [u8; 32] = [0x5a; 32];
const POLICY: [u8; 32] = [0x11; 32];

struct Set(InMemoryOutputSet);
impl OutputSet for Set {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn key_at(&self, i: usize) -> Option<Vec<u8>> {
        self.0.key_at(i)
    }
    fn commitment_at(&self, i: usize) -> Option<Vec<u8>> {
        self.0.commitment_at(i)
    }
}

#[test]
#[ignore = "measurement, not a pass/fail gate; run on the baseline device"]
fn worst_case_claim_builds_verifies_and_fits_the_wire_limit() {
    let mut set = InMemoryOutputSet::new();
    fn add(set: &mut InMemoryOutputSet, value: u64) -> (StealthKeypair, [u8; 32]) {
        let key = StealthKeypair::generate().unwrap();
        let blinding = mini_value::random_scalar_bytes().unwrap();
        let c = pedersen_commitment_v2(value, &blinding).unwrap();
        set.push(key.spend_public_bytes().to_vec(), c.to_vec());
        (key, blinding)
    }
    for _ in 0..RING_SIZE_V3 * 4 {
        add(&mut set, 1_000);
    }
    let fee = quote_fee_micro(&POLICY, MAX_INPUTS_V3, MAX_OUTPUTS_V3);
    let per_input = 100_000u64;
    let mut spends = Vec::new();
    let mut handles = Vec::new();
    for _ in 0..MAX_INPUTS_V3 {
        handles.push(add(&mut set, per_input));
    }
    let base = set.len() - MAX_INPUTS_V3;
    for (i, (key, blinding)) in handles.into_iter().enumerate() {
        spends.push(SpendableOutputV3 {
            set_index: base + i,
            one_time_secret: key.spend_secret_bytes(),
            value_micro: per_input,
            blinding,
        });
    }
    for _ in 0..8 {
        add(&mut set, 1_000);
    }
    let total = per_input * MAX_INPUTS_V3 as u64 - fee;
    let each = total / MAX_OUTPUTS_V3 as u64;
    let leftover = total - each * (MAX_OUTPUTS_V3 as u64 - 1);
    let recipients: Vec<RecipientV3> = (0..MAX_OUTPUTS_V3)
        .map(|i| {
            let to = StealthKeypair::generate().unwrap();
            RecipientV3 {
                spend_public: to.spend_public_bytes().to_vec(),
                view_public: to.view_public_bytes().to_vec(),
                amount_micro: if i + 1 == MAX_OUTPUTS_V3 {
                    leftover
                } else {
                    each
                },
                purpose: PaymentPurpose::new(b"budget".to_vec()),
            }
        })
        .collect();

    let started = Instant::now();
    let (claim, _) = build_v3(
        &PaymentRequestV3 {
            network_id: NETWORK,
            fee_policy_id: POLICY,
            spends,
            recipients,
        },
        &Set(set),
    )
    .unwrap();
    let build = started.elapsed();
    let bytes = claim.encode();
    let started = Instant::now();
    verify_v3(&claim, &NETWORK).unwrap();
    let verify = started.elapsed();
    println!(
        "T22 worst case ({MAX_INPUTS_V3} in / {MAX_OUTPUTS_V3} out, ring {RING_SIZE_V3}): {} bytes of {MAX_CLAIM_V3_BYTES}; build {build:?}; verify {verify:?}",
        bytes.len()
    );
    assert!(bytes.len() <= MAX_CLAIM_V3_BYTES);
}
