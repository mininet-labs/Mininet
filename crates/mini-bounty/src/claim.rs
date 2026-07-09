//! Claiming a bounty grant: prove membership in a pool's ring, direct
//! payout to a caller-supplied address, and let the ring signature's key
//! image be the network's only defense against claiming the same grant
//! twice.
//!
//! ## What binds a claim to one exact (pool, payout) pair
//!
//! The ring signature signs a message, not just "prove you know a ring
//! member's key" in the abstract — and the message here is a length-
//! prefixed encoding of the pool id and the payout address together. This
//! closes two attacks a naive "just sign anything" scheme would allow:
//! replaying a valid claim against a different pool that happens to share
//! ring members, and a man-in-the-middle swapping the payout address on a
//! claim in transit (the signature would no longer verify against the
//! tampered address, since the address is *inside* what was signed, not
//! attached alongside it).

use mini_value::{RingSignature, RingSignatureScheme};

use crate::error::{BountyError, Result};
use crate::ledger::KeyImageLedger;
use crate::pool::BountyPool;

/// A completed claim against one pool: which pool, where the payout
/// should go, and the ring signature proving membership without
/// revealing which grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BountyClaim {
    /// Which pool this claims against.
    pub pool_id: Vec<u8>,
    /// Where the payout should go (e.g. a `mini_value::StealthOutput`'s
    /// `one_time_address` bytes — this crate takes it as opaque bytes and
    /// has no opinion on how it was derived).
    pub payout_address: Vec<u8>,
    /// The ring signature proving membership in `pool.grants` and
    /// providing the key image double-claim prevention relies on.
    pub signature: RingSignature,
}

/// The exact bytes signed and verified: a fixed domain tag, then the pool
/// id and payout address, each length-prefixed so no input combination
/// can be confused for another (e.g. `pool_id=b"AB"` + `addr=b"C"` is
/// never mistaken for `pool_id=b"A"` + `addr=b"BC"`).
fn claim_message(pool_id: &[u8], payout_address: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(21 + 4 + pool_id.len() + 4 + payout_address.len());
    msg.extend_from_slice(b"mini-bounty/claim/v1");
    msg.extend_from_slice(&(pool_id.len() as u32).to_be_bytes());
    msg.extend_from_slice(pool_id);
    msg.extend_from_slice(&(payout_address.len() as u32).to_be_bytes());
    msg.extend_from_slice(payout_address);
    msg
}

/// Claim a grant in `pool`. `scheme` must already hold the claimant's ring
/// position and secret key (see `mini_value::MininetRingSignature::new`)
/// — this function never sees or handles a secret key directly, the same
/// out-of-band-secret discipline `RingSignatureScheme` itself documents.
/// `None` from the underlying scheme (no real implementation wired in, or
/// an invalid ring position) becomes [`BountyError::InvalidClaim`].
pub fn claim(
    pool: &BountyPool,
    scheme: &mut impl RingSignatureScheme,
    payout_address: &[u8],
) -> Result<BountyClaim> {
    let message = claim_message(&pool.id, payout_address);
    let signature = scheme
        .sign(&pool.ring(), &message)
        .ok_or(BountyError::InvalidClaim)?;
    Ok(BountyClaim {
        pool_id: pool.id.clone(),
        payout_address: payout_address.to_vec(),
        signature,
    })
}

/// Verify `claim` against `pool`: the ring signature must actually prove
/// membership in this exact pool's grant set for this exact payout
/// address, and the grant it proves membership of must not have already
/// paid out (per `ledger`). Returns the amount to pay on success.
pub fn verify_claim(
    pool: &BountyPool,
    claim: &BountyClaim,
    scheme: &impl RingSignatureScheme,
    ledger: &mut impl KeyImageLedger,
) -> Result<u64> {
    if claim.pool_id != pool.id {
        return Err(BountyError::InvalidClaim);
    }
    let message = claim_message(&pool.id, &claim.payout_address);
    if !scheme.verify(&pool.ring(), &message, &claim.signature) {
        return Err(BountyError::InvalidClaim);
    }
    if !ledger.check_and_record(&pool.id, &claim.signature.key_image) {
        return Err(BountyError::AlreadyClaimed);
    }
    Ok(pool.amount_per_grant_micro)
}

#[cfg(test)]
mod tests {
    use mini_value::{MininetRingSignature, StealthKeypair};

    use super::*;
    use crate::ledger::InMemoryKeyImageLedger;
    use crate::pool::BountyGrant;

    /// A pool of `n` grants with a real, known secret key planted at
    /// `real_index`. Every grant key is a genuine `StealthKeypair` spend
    /// keypair (public entirely through `mini_value`'s own public API —
    /// this crate never reaches into `mini_value`'s internal curve
    /// module), so each grant's claim key is a real, valid ring member:
    /// `spend_public_bytes() == spend_secret_bytes() * G`.
    fn pool_with_real_grant_at(n: usize, real_index: usize) -> (BountyPool, [u8; 32]) {
        let mut real_secret_bytes = [0u8; 32];
        let mut grants = Vec::with_capacity(n);
        for i in 0..n {
            let keypair = StealthKeypair::generate().unwrap();
            if i == real_index {
                real_secret_bytes = keypair.spend_secret_bytes();
            }
            grants.push(BountyGrant {
                claim_pubkey: keypair.spend_public_bytes().to_vec(),
            });
        }
        let pool = BountyPool::new(b"pool-round-1".to_vec(), grants, 5_000_000).unwrap();
        (pool, real_secret_bytes)
    }

    fn dummy_verifier() -> MininetRingSignature {
        MininetRingSignature::new(0, &[0u8; 32]).unwrap()
    }

    #[test]
    fn a_valid_claim_verifies_and_pays_the_grant_amount() {
        let (pool, secret) = pool_with_real_grant_at(5, 2);
        let mut signer = MininetRingSignature::new(2, &secret).unwrap();
        let payout = b"stealth-address-bytes".to_vec();
        let bounty_claim = claim(&pool, &mut signer, &payout).unwrap();

        let mut ledger = InMemoryKeyImageLedger::new();
        let paid = verify_claim(&pool, &bounty_claim, &dummy_verifier(), &mut ledger).unwrap();
        assert_eq!(paid, 5_000_000);
    }

    #[test]
    fn the_same_grant_cannot_claim_twice() {
        let (pool, secret) = pool_with_real_grant_at(4, 1);
        let mut signer = MininetRingSignature::new(1, &secret).unwrap();
        let payout = b"addr-a".to_vec();
        let bounty_claim = claim(&pool, &mut signer, &payout).unwrap();

        let mut ledger = InMemoryKeyImageLedger::new();
        assert!(verify_claim(&pool, &bounty_claim, &dummy_verifier(), &mut ledger).is_ok());
        assert_eq!(
            verify_claim(&pool, &bounty_claim, &dummy_verifier(), &mut ledger),
            Err(BountyError::AlreadyClaimed)
        );
    }

    #[test]
    fn a_claim_cannot_be_replayed_against_a_different_pool_with_overlapping_grants() {
        let (pool_a, secret) = pool_with_real_grant_at(3, 0);
        let mut signer = MininetRingSignature::new(0, &secret).unwrap();
        let payout = b"addr-a".to_vec();
        let bounty_claim = claim(&pool_a, &mut signer, &payout).unwrap();

        // A different pool that happens to reuse the exact same grants.
        let pool_b =
            BountyPool::new(b"pool-round-2".to_vec(), pool_a.grants.clone(), 5_000_000).unwrap();

        let mut ledger = InMemoryKeyImageLedger::new();
        assert_eq!(
            verify_claim(&pool_b, &bounty_claim, &dummy_verifier(), &mut ledger),
            Err(BountyError::InvalidClaim)
        );
    }

    #[test]
    fn swapping_the_payout_address_after_signing_invalidates_the_claim() {
        let (pool, secret) = pool_with_real_grant_at(3, 1);
        let mut signer = MininetRingSignature::new(1, &secret).unwrap();
        let mut bounty_claim = claim(&pool, &mut signer, b"honest-address").unwrap();
        bounty_claim.payout_address = b"attacker-address".to_vec();

        let mut ledger = InMemoryKeyImageLedger::new();
        assert_eq!(
            verify_claim(&pool, &bounty_claim, &dummy_verifier(), &mut ledger),
            Err(BountyError::InvalidClaim)
        );
    }

    #[test]
    fn a_claim_from_outside_the_pool_is_rejected() {
        let (pool, _secret) = pool_with_real_grant_at(3, 0);
        let outsider = StealthKeypair::generate().unwrap();
        let mut signer = MininetRingSignature::new(0, &outsider.spend_secret_bytes()).unwrap();
        let bounty_claim = claim(&pool, &mut signer, b"addr").unwrap();

        let mut ledger = InMemoryKeyImageLedger::new();
        assert_eq!(
            verify_claim(&pool, &bounty_claim, &dummy_verifier(), &mut ledger),
            Err(BountyError::InvalidClaim)
        );
    }
}
