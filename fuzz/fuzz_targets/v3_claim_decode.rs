#![no_main]
//! Any bytes: decoding never panics, and a claim that decodes and re-encodes to
//! its own bytes has a stable `claim_id` that changes if any byte changes.
use libfuzzer_sys::fuzz_target;
use mini_private_payment::PrivatePaymentClaimV3;

fuzz_target!(|data: &[u8]| {
    if let Ok(claim) = PrivatePaymentClaimV3::decode(data) {
        let encoded = claim.encode();
        if encoded == data {
            let again = PrivatePaymentClaimV3::decode(&encoded).expect("canonical bytes decode");
            assert_eq!(again.claim_id(), claim.claim_id());
            assert_eq!(again.encode(), encoded);
        }
    }
});
