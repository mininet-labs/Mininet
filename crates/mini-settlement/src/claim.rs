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
const CLAIM_WIRE_DOMAIN: &[u8] = b"mini-settlement/payment-claim-wire/v1";

/// Maximum payer, payee, or chain-head hint bytes accepted from the wire.
pub const MAX_CLAIM_FIELD_BYTES: usize = 4_096;
/// Maximum encoded size of one standalone payment claim.
pub const MAX_PAYMENT_CLAIM_BYTES: usize = 16 * 1024;

/// Stable identifier of the canonical public Mininet settlement domain.
///
/// Private/test deployments must use a different identifier through
/// [`sign_claim_for_network`]; otherwise a valid claim could be replayed
/// between deployments running the same protocol.
pub const MININET_NETWORK_ID: [u8; 32] = *b"mininet-public-settlement-v1\0\0\0\0";

/// A signed payment claim: "I, `payer`, at sequence `sequence`, promise to pay
/// `amount_micro` to `payee`, valid until `valid_until_ms`, as of the chain
/// state I last saw (`last_known_chain`)." Nothing about this type makes it
/// final — see [`crate::SettlementState`] for what final actually requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentClaim {
    /// Exact settlement network on which this promise may be executed.
    pub network_id: [u8; 32],
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
    network_id: &[u8; 32],
    payer: &[u8],
    payee: &[u8],
    amount_micro: u64,
    sequence: u64,
    valid_until_ms: u64,
    last_known_chain: &[u8],
) -> Vec<u8> {
    let mut msg = Vec::with_capacity(
        CLAIM_DOMAIN.len()
            + 32
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
    msg.extend_from_slice(network_id);
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
    sign_claim_for_network(
        payer,
        payee,
        amount_micro,
        sequence,
        valid_until_ms,
        &MININET_NETWORK_ID,
        last_known_chain,
        now_ms,
    )
}

impl PaymentClaim {
    /// Canonical bounded bytes for wallet-to-validator submission.
    pub fn to_wire_bytes(&self) -> Result<Vec<u8>> {
        if self.payer.len() > MAX_CLAIM_FIELD_BYTES
            || self.payee.len() > MAX_CLAIM_FIELD_BYTES
            || self.last_known_chain.len() > MAX_CLAIM_FIELD_BYTES
        {
            return Err(SettlementError::ClaimTooLarge);
        }
        let mut w = Vec::new();
        w.extend_from_slice(CLAIM_WIRE_DOMAIN);
        w.extend_from_slice(&self.network_id);
        put_bytes(&mut w, &self.payer);
        put_bytes(&mut w, &self.payee);
        w.extend_from_slice(&self.amount_micro.to_be_bytes());
        w.extend_from_slice(&self.sequence.to_be_bytes());
        w.extend_from_slice(&self.valid_until_ms.to_be_bytes());
        put_bytes(&mut w, &self.last_known_chain);
        w.push(self.signature.suite().tag());
        w.extend_from_slice(&self.signature.to_bytes());
        if w.len() > MAX_PAYMENT_CLAIM_BYTES {
            return Err(SettlementError::ClaimTooLarge);
        }
        Ok(w)
    }

    /// Decode one standalone claim with allocation bounds checked first.
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_PAYMENT_CLAIM_BYTES {
            return Err(SettlementError::ClaimTooLarge);
        }
        let mut r = ClaimReader::new(bytes);
        if r.take(CLAIM_WIRE_DOMAIN.len())? != CLAIM_WIRE_DOMAIN {
            return Err(SettlementError::MalformedClaim);
        }
        let mut network_id = [0u8; 32];
        network_id.copy_from_slice(r.take(32)?);
        let payer = r.bytes(MAX_CLAIM_FIELD_BYTES)?.to_vec();
        let payee = r.bytes(MAX_CLAIM_FIELD_BYTES)?.to_vec();
        let amount_micro = r.u64()?;
        let sequence = r.u64()?;
        let valid_until_ms = r.u64()?;
        let last_known_chain = r.bytes(MAX_CLAIM_FIELD_BYTES)?.to_vec();
        let suite =
            SignatureSuite::from_tag(r.u8()?).map_err(|_| SettlementError::MalformedClaim)?;
        let signature = Signature::from_suite_bytes(suite, r.take(suite.signature_len())?)
            .map_err(|_| SettlementError::MalformedClaim)?;
        if !r.finished() {
            return Err(SettlementError::MalformedClaim);
        }
        Ok(Self {
            network_id,
            payer,
            payee,
            amount_micro,
            sequence,
            valid_until_ms,
            last_known_chain,
            signature,
        })
    }
}

fn put_bytes(w: &mut Vec<u8>, bytes: &[u8]) {
    w.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    w.extend_from_slice(bytes);
}

struct ClaimReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> ClaimReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(SettlementError::MalformedClaim)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(SettlementError::MalformedClaim)?;
        self.position = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| SettlementError::MalformedClaim)?,
        ))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| SettlementError::MalformedClaim)?,
        ))
    }

    fn bytes(&mut self, maximum: usize) -> Result<&'a [u8]> {
        let length = usize::try_from(self.u32()?).map_err(|_| SettlementError::MalformedClaim)?;
        if length > maximum {
            return Err(SettlementError::ClaimTooLarge);
        }
        self.take(length)
    }

    fn finished(&self) -> bool {
        self.position == self.bytes.len()
    }
}

/// Sign a payment claim for one exact settlement network.
#[allow(clippy::too_many_arguments)]
pub fn sign_claim_for_network(
    payer: &SigningKey,
    payee: &[u8],
    amount_micro: u64,
    sequence: u64,
    valid_until_ms: u64,
    network_id: &[u8; 32],
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
        network_id,
        &payer_bytes,
        payee,
        amount_micro,
        sequence,
        valid_until_ms,
        last_known_chain,
    );
    let signature = payer.sign(&message);
    Ok(PaymentClaim {
        network_id: *network_id,
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
        &claim.network_id,
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
        &claim.network_id,
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

        let mut tampered_network = claim.clone();
        tampered_network.network_id = [0x77; 32];
        assert_eq!(
            verify_claim_signature(&tampered_network).unwrap_err(),
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
    fn network_id_domain_separates_otherwise_identical_claims() {
        let network_a = [0xA1; 32];
        let network_b = [0xB2; 32];
        let a = sign_claim_for_network(
            &payer_key(),
            b"payee-a",
            1_000,
            0,
            10_000,
            &network_a,
            b"same-head",
            0,
        )
        .unwrap();
        let b = sign_claim_for_network(
            &payer_key(),
            b"payee-a",
            1_000,
            0,
            10_000,
            &network_b,
            b"same-head",
            0,
        )
        .unwrap();
        assert_ne!(claim_digest(&a), claim_digest(&b));
        assert_ne!(a.signature, b.signature);
    }

    #[test]
    fn standalone_wire_round_trip_preserves_digest_and_signature() {
        let claim = sign_claim(
            &payer_key(),
            b"payee-a",
            1_000,
            7,
            10_000,
            b"chain-head-1",
            0,
        )
        .unwrap();
        let decoded = PaymentClaim::from_wire_bytes(&claim.to_wire_bytes().unwrap()).unwrap();
        assert_eq!(decoded, claim);
        assert_eq!(claim_digest(&decoded), claim_digest(&claim));
        verify_claim_signature(&decoded).unwrap();
    }

    #[test]
    fn standalone_wire_rejects_every_truncation_and_trailing_bytes() {
        let claim = sign_claim(
            &payer_key(),
            b"payee-a",
            1_000,
            7,
            10_000,
            b"chain-head-1",
            0,
        )
        .unwrap();
        let bytes = claim.to_wire_bytes().unwrap();
        for cut in 0..bytes.len() {
            assert!(PaymentClaim::from_wire_bytes(&bytes[..cut]).is_err());
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            PaymentClaim::from_wire_bytes(&trailing).unwrap_err(),
            SettlementError::MalformedClaim
        );
    }

    #[test]
    fn standalone_wire_rejects_oversized_fields_before_encoding() {
        let mut claim = sign_claim(
            &payer_key(),
            b"payee-a",
            1_000,
            7,
            10_000,
            b"chain-head-1",
            0,
        )
        .unwrap();
        claim.last_known_chain = vec![0; MAX_CLAIM_FIELD_BYTES + 1];
        assert_eq!(
            claim.to_wire_bytes().unwrap_err(),
            SettlementError::ClaimTooLarge
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
