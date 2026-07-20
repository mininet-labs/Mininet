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

/// Longest allowed [`BountyPool::project`] label, in bytes. A generous
/// bound for a human-readable name, not a content-addressed identifier —
/// keeps a caller from embedding unbounded data in what's meant to be a
/// short organizational tag.
const MAX_PROJECT_LABEL_BYTES: usize = 128;

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
    /// Micro-MINI paid out per successfully claimed grant. Deliberately
    /// **flat across every grant in a pool, permanently** — not just a
    /// first-slice simplification. Varying the amount *within* one ring
    /// would make the payout amount itself a side channel: in a ring of
    /// grants worth different amounts, whichever amount actually pays out
    /// narrows down which grant claimed, eroding exactly the anonymity
    /// [`BountyPool`]'s own module docs explain the ring exists to
    /// provide. Paying different contributors different amounts (e.g. a
    /// maintainer worth more than a one-line fix) is done by giving them
    /// *separate* flat-amount pools, optionally grouped under the same
    /// [`BountyPool::project`] label — see that field's docs.
    pub amount_per_grant_micro: u64,
    /// Optional human-readable label grouping this pool with other
    /// funding rounds for the same project (e.g. `"mini-crawler"`). Pure
    /// organizational metadata: never used as ring-signature message
    /// context, never affects claim/verify, and carries no authority —
    /// two pools with the same label are otherwise fully independent
    /// funding rounds, each with its own id, grant set, and flat amount.
    /// This is the sanctioned way to fund one project at different rates
    /// for different contributors without weakening any single pool's
    /// anonymity set (see `amount_per_grant_micro`'s docs for why a
    /// single pool never varies its own amount).
    pub project: Option<String>,
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
            project: None,
        })
    }

    /// Attach a project label to this pool (see [`BountyPool::project`]).
    /// Rejects an empty label or one over
    /// [`MAX_PROJECT_LABEL_BYTES`]. Consumes and returns `self` so it
    /// composes with [`BountyPool::new`] at construction time.
    pub fn with_project(mut self, project: impl Into<String>) -> Result<Self> {
        let project = project.into();
        if project.is_empty() || project.len() > MAX_PROJECT_LABEL_BYTES {
            return Err(BountyError::InvalidPool);
        }
        self.project = Some(project);
        Ok(self)
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

    #[test]
    fn a_pool_has_no_project_label_by_default() {
        let pool = BountyPool::new(vec![1], vec![grant(1)], 1_000).unwrap();
        assert_eq!(pool.project, None);
    }

    #[test]
    fn a_project_label_can_be_attached() {
        let pool = BountyPool::new(vec![1], vec![grant(1)], 1_000)
            .unwrap()
            .with_project("mini-crawler")
            .unwrap();
        assert_eq!(pool.project.as_deref(), Some("mini-crawler"));
    }

    #[test]
    fn an_empty_project_label_is_rejected() {
        let pool = BountyPool::new(vec![1], vec![grant(1)], 1_000).unwrap();
        assert_eq!(pool.with_project(""), Err(BountyError::InvalidPool));
    }

    #[test]
    fn an_oversized_project_label_is_rejected() {
        let pool = BountyPool::new(vec![1], vec![grant(1)], 1_000).unwrap();
        let too_long = "x".repeat(MAX_PROJECT_LABEL_BYTES + 1);
        assert_eq!(pool.with_project(too_long), Err(BountyError::InvalidPool));
    }

    #[test]
    fn a_project_label_at_the_exact_bound_is_accepted() {
        let pool = BountyPool::new(vec![1], vec![grant(1)], 1_000).unwrap();
        let exact = "x".repeat(MAX_PROJECT_LABEL_BYTES);
        assert!(pool.with_project(exact).is_ok());
    }

    #[test]
    fn two_pools_can_share_a_project_label_at_different_flat_amounts() {
        // A maintainer's round and a first-time-contributor round for the
        // same project, at different flat rates -- the sanctioned pattern
        // for "pay contributors differently" without varying any single
        // pool's own amount.
        let maintainer_round = BountyPool::new(vec![1], vec![grant(1), grant(2)], 5_000_000)
            .unwrap()
            .with_project("mini-crawler")
            .unwrap();
        let first_timer_round = BountyPool::new(vec![2], vec![grant(3), grant(4)], 250_000)
            .unwrap()
            .with_project("mini-crawler")
            .unwrap();
        assert_eq!(maintainer_round.project, first_timer_round.project);
        assert_ne!(
            maintainer_round.amount_per_grant_micro,
            first_timer_round.amount_per_grant_micro
        );
        // Independent pools: different ids, no shared ring members.
        assert_ne!(maintainer_round.id, first_timer_round.id);
    }
}
