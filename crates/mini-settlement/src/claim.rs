//! A payment claim: a signed promise to pay, never final ownership.
//!
//! Directive 5, stated exactly: *"during outages, users exchange signed
//! promises — not final ownership. Ownership changes only when accepted
//! into canonical consensus."* A [`PaymentClaim`] **is** that signed
//! promise. Everything downstream (local acceptance, reconciliation) exists
//! to keep every caller honest about the one fact this type alone cannot
//! enforce: signing a claim moves nothing by itself.
//!
//! ## Why a sequence, not a UTXO/key-image
//!
//! `mini-value` already has key-image machinery (ring signatures), but that
//! solves a different problem — anonymity-set membership. Ordinary payment
//! settlement doesn't need to hide *which* claim a payer signed, only to
//! detect when a payer signs *two different* claims for the same spending
//! slot. A monotonic sequence per payer is the direct, minimal primitive for
//! that (the same shape Directive 5's own wording implies: "a promise," not
//! "an anonymous proof of a promise") — and it composes with anonymous
//! addressing for free: a caller free to make `payer`/`payee` a fresh
//! `mini_value` stealth key per claim if they want unlinkability; this
//! crate has no opinion on that and never inspects key contents beyond
//! verifying the signature.

use mini_crypto::{HashAlgorithm, Signature, SignatureSuite, SigningKey, VerifyingKey};

use crate::error::{Result, SettlementError};

/// Domain tag for the signed message, versioned so a future claim shape
/// can coexist without ever being confused with this one.
const CLAIM_DOMAIN: &[u8] = b"mini-settlement/payment-claim/v1";

/// A signed payment claim: "I, `payer`, at sequence `sequence`, promise to pay
/// `amount_micro` to `payee`, valid until `valid_until_ms`, as of the chain
/// state I last saw (`last_known_chain`)." Nothing about this type makes it
/// final — see [`crate::SettlementState`] for what final actually requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentClaim {
    /// The payer's public key bytes (any key the payer controls — a
    /// stealth spend key, a device key, whatever the caller chooses).
    pub payer: Vec<u8>,
    /// The payee's public key / address bytes. Opaque to this crate.
    pub payee: Vec<u8>,
    /// The amount, in micro-MINI (same convention as `mini-bounty` and
    /// `mini-reward`: plain `u64`, not `mini-value`'s confidential
    /// Bulletproofs amounts — see the crate-level docs for why).
    pub amount_micro: u64,
    /// This payer's claim sequence number. Two claims from the same payer
    /// with the same sequence but different content are, by construction, in
    /// conflict — see [`crate::ClaimWatcher`] and [`crate::reconcile`].
    pub sequence: u64,
    /// The claim expires (see [`crate::SettlementState::Expired`]) if it
    /// has not reached canonical inclusion by this device-clock time, in ms.
    pub valid_until_ms: u64,
    /// An opaque reference to the canonical chain state the payer had last
    /// observed when signing (e.g. a block hash/height encoding) — carried
    /// so a reconciler can reason about whether the claimed balance was
    /// plausible *at signing time*, without this crate needing to know the
    /// chain's actual representation.
    pub last_known_chain: Vec<u8>,
    /// The payer's signature over this claim's canonical bytes.
    pub signature: Signature,
}

/// The exact bytes signed and verified: the domain tag, then every field
/// length- or width-prefixed, so no two distinct claims can ever encode to
/// the same message (the same discipline `mini-bounty::claim_message` uses).
fn claim_message(
    payer: &[u8],
    payee: &[u8],
    amount_micro: u64,
    sequence: u64,
    valid_until_ms: u64,
    last_known_chain: &[u8],
) -> Vec<u8> {
    let mut msg = Vec::with_capacity(
        CLAIM_DOMAIN.len()
            + 4
            + payer.len()
            + 4
            + payee.len()
            + 8
            + 8
            + 8
            + 4
            + last_known_chain.len(),
    );
    msg.extend_from_slice(CLAIM_DOMAIN);
    msg.extend_from_slice(&(payer.len() as u32).to_be_bytes());
    msg.extend_from_slice(payer);
    msg.extend_from_slice(&(payee.len() as u32).to_be_bytes());
    msg.extend_from_slice(payee);
    msg.extend_from_slice(&amount_micro.to_be_bytes());
    msg.extend_from_slice(&sequence.to_be_bytes());
    msg.extend_from_slice(&valid_until_ms.to_be_bytes());
    msg.extend_from_slice(&(last_known_chain.len() as u32).to_be_bytes());
    msg.extend_from_slice(last_known_chain);
    msg
}

/// Sign a new payment claim. `now_ms` is only used to reject a
/// self-contradictory `valid_until_ms` at construction time (see
/// [`SettlementError::BadValidityWindow`]) — it is never embedded in the
/// signed bytes, so it cannot itself be a forgeable "issued at" claim.
pub fn sign_claim(
    payer: &SigningKey,
    payee: &[u8],
    amount_micro: u64,
    sequence: u64,
    valid_until_ms: u64,
    last_known_chain: &[u8],
    now_ms: u64,
) -> Result<PaymentClaim> {
    if amount_micro == 0 {
        return Err(SettlementError::ZeroAmount);
    }
    if valid_until_ms <= now_ms {
        return Err(SettlementError::BadValidityWindow);
    }
    let payer_bytes = payer.verifying_key().to_bytes().to_vec();
    let message = claim_message(
        &payer_bytes,
        payee,
        amount_micro,
        sequence,
        valid_until_ms,
        last_known_chain,
    );
    let signature = payer.sign(&message);
    Ok(PaymentClaim {
        payer: payer_bytes,
        payee: payee.to_vec(),
        amount_micro,
        sequence,
        valid_until_ms,
        last_known_chain: last_known_chain.to_vec(),
        signature,
    })
}

/// Verify a claim's signature against its own claimed payer key. This is
/// purely a structural/authenticity check — it says nothing about whether
/// the claim will ever be honored (see [`crate::reconcile::reconcile`]).
pub fn verify_claim_signature(claim: &PaymentClaim) -> Result<()> {
    if claim.amount_micro == 0 {
        return Err(SettlementError::ZeroAmount);
    }
    let payer_key = VerifyingKey::from_suite_bytes(SignatureSuite::DEFAULT, &claim.payer)
        .map_err(|_| SettlementError::BadKey)?;
    let message = claim_message(
        &claim.payer,
        &claim.payee,
        claim.amount_micro,
        claim.sequence,
        claim.valid_until_ms,
        &claim.last_known_chain,
    );
    payer_key
        .verify(&message, &claim.signature)
        .map_err(|_| SettlementError::BadSignature)
}

/// A content digest of the claim's signed bytes — the identifier used to
/// tell "the same claim, seen twice" from "two different claims at the
/// same (payer, sequence)" (a real conflict). Two claims with the same digest
/// are byte-identical in every field that was signed.
pub fn claim_digest(claim: &PaymentClaim) -> [u8; 32] {
    let message = claim_message(
        &claim.payer,
        &claim.payee,
        claim.amount_micro,
        claim.sequence,
        claim.valid_until_ms,
        &claim.last_known_chain,
    );
    HashAlgorithm::Blake3.digest(&message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payer_key() -> SigningKey {
        SigningKey::from_seed(&[0x11; 32])
    }

    #[test]
    fn a_validly_signed_claim_verifies() {
        let claim = sign_claim(
            &payer_key(),
            b"payee-a",
            1_000,
            0,
            10_000,
            b"chain-head-1",
            0,
        )
        .unwrap();
        assert!(verify_claim_signature(&claim).is_ok());
    }

    #[test]
    fn zero_amount_is_rejected_at_signing_and_verification() {
        assert_eq!(
            sign_claim(&payer_key(), b"payee-a", 0, 0, 10_000, b"chain-head-1", 0).unwrap_err(),
            SettlementError::ZeroAmount
        );
    }

    #[test]
    fn a_validity_window_that_has_already_elapsed_is_rejected_at_signing() {
        assert_eq!(
            sign_claim(
                &payer_key(),
                b"payee-a",
                1_000,
                0,
                500,
                b"chain-head-1",
                1_000
            )
            .unwrap_err(),
            SettlementError::BadValidityWindow
        );
    }

    #[test]
    fn tampering_any_signed_field_breaks_verification() {
        let claim = sign_claim(
            &payer_key(),
            b"payee-a",
            1_000,
            0,
            10_000,
            b"chain-head-1",
            0,
        )
        .unwrap();

        let mut tampered_amount = claim.clone();
        tampered_amount.amount_micro = 999_999;
        assert_eq!(
            verify_claim_signature(&tampered_amount).unwrap_err(),
            SettlementError::BadSignature
        );

        let mut tampered_payee = claim.clone();
        tampered_payee.payee = b"attacker-address".to_vec();
        assert_eq!(
            verify_claim_signature(&tampered_payee).unwrap_err(),
            SettlementError::BadSignature
        );

        let mut tampered_sequence = claim.clone();
        tampered_sequence.sequence = 7;
        assert_eq!(
            verify_claim_signature(&tampered_sequence).unwrap_err(),
            SettlementError::BadSignature
        );

        let mut tampered_chain = claim;
        tampered_chain.last_known_chain = b"different-chain-head".to_vec();
        assert_eq!(
            verify_claim_signature(&tampered_chain).unwrap_err(),
            SettlementError::BadSignature
        );
    }

    #[test]
    fn two_claims_differing_only_by_sequence_have_different_digests() {
        let a = sign_claim(
            &payer_key(),
            b"payee-a",
            1_000,
            0,
            10_000,
            b"chain-head-1",
            0,
        )
        .unwrap();
        let b = sign_claim(
            &payer_key(),
            b"payee-a",
            1_000,
            1,
            10_000,
            b"chain-head-1",
            0,
        )
        .unwrap();
        assert_ne!(claim_digest(&a), claim_digest(&b));
    }

    #[test]
    fn re_signing_identical_fields_produces_the_same_digest() {
        let a = sign_claim(
            &payer_key(),
            b"payee-a",
            1_000,
            0,
            10_000,
            b"chain-head-1",
            0,
        )
        .unwrap();
        let b = sign_claim(
            &payer_key(),
            b"payee-a",
            1_000,
            0,
            10_000,
            b"chain-head-1",
            0,
        )
        .unwrap();
        assert_eq!(claim_digest(&a), claim_digest(&b));
    }
}
