//! A bounty pool: the set of approved-but-not-yet-necessarily-claimed
//! grants a contributor can anonymously claim against.
//!
//! ## Why the ring is the whole pool, not just "eligible" claimants
//!
//! A ring signature's anonymity set is exactly the ring passed to it —
//! shrink the ring and you shrink the anonymity. If a pool's ring only
//! ever contained *unclaimed* grants, the very last person to claim would
//! sign against a ring of size one, unmasking themselves immediately.
//! [`BountyPool`] therefore keeps every grant ever issued in the ring,
//! claimed or not — the same choice Monero's own ring signatures make
//! (a ring mixes spent and unspent outputs together on purpose). Double-
//! claiming the same grant is prevented separately, by the ring
//! signature's key image (see [`crate::claim`]), not by shrinking who's
//! eligible to sign.

use crate::error::{BountyError, Result};

/// One approved contribution's slot in a pool: only a public key,
/// published when the contribution was approved. Nothing here identifies
/// *who* was approved — that link is known only to whoever approved the
/// contribution (a human maintainer reading GitHub, which is never
/// anonymous on its own) and to the contributor who holds the matching
/// secret key. Mininet, the pool, and every other observer see only this
/// public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BountyGrant {
    /// The contributor's one-time claim public key for this grant
    /// (compressed Ristretto point, 32 bytes) — generate a fresh one per
    /// grant, never reuse a persistent identity key here, or claims
    /// across different pools become linkable to each other even though
    /// any single claim stays anonymous within its own pool.
    pub claim_pubkey: Vec<u8>,
}

/// A funding round: an opaque pool identifier (binds every claim to
/// *this* pool, so a valid claim can never be replayed against a
/// different pool even if the two pools happen to share ring members),
/// the grant set, and the amount each grant is worth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BountyPool {
    /// Opaque identifier for this funding round (e.g. a content hash of
    /// the funding proposal). Only used as signed-message context here —
    /// this crate has no opinion on how it's minted or governed.
    pub id: Vec<u8>,
    /// Every grant ever issued against this pool, claimed or not — see
    /// the module docs for why claimed grants are never removed.
    pub grants: Vec<BountyGrant>,
    /// Micro-MINI paid out per successfully claimed grant. Flat per-grant
    /// amount for this first slice; variable per-grant amounts are a
    /// straightforward future extension, not built here.
    pub amount_per_grant_micro: u64,
}

impl BountyPool {
    /// Build a pool. Rejects an empty grant set, a zero-length pool id, a
    /// zero amount, or any grant whose claim key is not a well-formed
    /// 32-byte value, or a duplicate claim key (two grants resolving to
    /// the same underlying secret would make the ring ambiguous about
    /// which slot a claim actually consumed).
    pub fn new(id: Vec<u8>, grants: Vec<BountyGrant>, amount_per_grant_micro: u64) -> Result<Self> {
        if id.is_empty() || grants.is_empty() || amount_per_grant_micro == 0 {
            return Err(BountyError::InvalidPool);
        }
        let mut seen = std::collections::HashSet::with_capacity(grants.len());
        for grant in &grants {
            if grant.claim_pubkey.len() != 32 {
                return Err(BountyError::InvalidPool);
            }
            if !seen.insert(grant.claim_pubkey.clone()) {
                return Err(BountyError::InvalidPool);
            }
        }
        Ok(BountyPool {
            id,
            grants,
            amount_per_grant_micro,
        })
    }

    /// The current anonymity-set size — how many grants a claim against
    /// this pool hides among. Callers deciding whether a pool is "big
    /// enough" to claim against privately should check this directly;
    /// this crate enforces no minimum, since a single-grant pool is a
    /// legitimate (if non-private) degenerate case.
    pub fn ring_size(&self) -> usize {
        self.grants.len()
    }

    /// The ring, in the exact byte form `RingSignatureScheme` expects —
    /// every grant's claim key, same order as `self.grants`.
    pub(crate) fn ring(&self) -> Vec<Vec<u8>> {
        self.grants.iter().map(|g| g.claim_pubkey.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(byte: u8) -> BountyGrant {
        BountyGrant {
            claim_pubkey: vec![byte; 32],
        }
    }

    #[test]
    fn empty_grant_set_is_rejected() {
        assert_eq!(
            BountyPool::new(vec![1], vec![], 1_000),
            Err(BountyError::InvalidPool)
        );
    }

    #[test]
    fn empty_pool_id_is_rejected() {
        assert_eq!(
            BountyPool::new(vec![], vec![grant(1)], 1_000),
            Err(BountyError::InvalidPool)
        );
    }

    #[test]
    fn zero_amount_is_rejected() {
        assert_eq!(
            BountyPool::new(vec![1], vec![grant(1)], 0),
            Err(BountyError::InvalidPool)
        );
    }

    #[test]
    fn malformed_claim_key_length_is_rejected() {
        let bad = BountyGrant {
            claim_pubkey: vec![1, 2, 3],
        };
        assert_eq!(
            BountyPool::new(vec![1], vec![bad], 1_000),
            Err(BountyError::InvalidPool)
        );
    }

    #[test]
    fn duplicate_claim_keys_are_rejected() {
        assert_eq!(
            BountyPool::new(vec![1], vec![grant(9), grant(9)], 1_000),
            Err(BountyError::InvalidPool)
        );
    }

    #[test]
    fn a_valid_pool_reports_its_ring_size() {
        let pool = BountyPool::new(vec![1], vec![grant(1), grant(2), grant(3)], 1_000).unwrap();
        assert_eq!(pool.ring_size(), 3);
    }
}
