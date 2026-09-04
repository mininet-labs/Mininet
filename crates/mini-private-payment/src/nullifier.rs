//! Double-spend detection without a public payer.
//!
//! A transparent claim is keyed on `(payer, sequence)`: two claims from the
//! same payer at the same sequence conflict. A private claim has no payer
//! and no sequence, so the conflict key has to come from the cryptography
//! itself. That key is the **key image** — a value deterministic in the
//! one-time secret being spent, produced by the ring signature and
//! verifiable by anyone without learning which ring member produced it.
//!
//! # M1, restated for the private case
//!
//! "Money does not merge." When two claims carry the same key image, this
//! set keeps the first and refuses the second. It does not net them, sum
//! them, pick the larger, or ask a policy — because those are all merges,
//! and the invariant admits no version of merging that is spelled
//! differently. [`KeyImageSet::observe`] returns
//! [`SpendOutcome::Conflict`] carrying the digest of the claim already
//! held, so a caller can present both to whoever adjudicates.
//!
//! # What this set is and is not
//!
//! It is a **local** view. Two nodes that never meet can each accept one
//! half of a conflicting pair, exactly as two `mini_settlement`
//! reconcilers can. Only canonical ordering resolves that (M3), which is
//! why [`crate::reconcile`] consults a [`crate::PrivateLedgerView`] and
//! this set never claims finality on its own.

use std::collections::BTreeMap;

use crate::claim::VerifiedPrivateClaim;
use crate::error::{PrivatePaymentError, Result};

/// What happened when a claim was offered to a [`KeyImageSet`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpendOutcome {
    /// First time this key image has been seen here.
    Accepted,
    /// This exact claim was already recorded. Idempotent, not a conflict:
    /// re-broadcasting is normal network behavior, not double-spending.
    AlreadyRecorded,
    /// A *different* claim already spent this key image. Both digests are
    /// carried so a caller can hand the pair to an adjudicator.
    Conflict { held: [u8; 32], offered: [u8; 32] },
}

/// Key images observed locally, and the claim digest each was first seen
/// with.
#[derive(Debug, Default)]
pub struct KeyImageSet {
    spent: BTreeMap<Vec<u8>, [u8; 32]>,
}

impl KeyImageSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Offer a verified claim to the set.
    ///
    /// Takes a [`VerifiedPrivateClaim`] rather than a raw one on purpose:
    /// recording an unverified claim's key image would let anyone burn
    /// another party's output by broadcasting a garbage claim carrying its
    /// key image.
    pub fn observe(&mut self, claim: &VerifiedPrivateClaim) -> SpendOutcome {
        let digest = *claim.transcript_digest();
        let images: Vec<Vec<u8>> = claim.key_images().map(|image| image.to_vec()).collect();

        // A multi-input claim is all-or-nothing. Recording the inputs that
        // happen to be unspent and ignoring the rest would admit a claim
        // that spends a mix of fresh and already-spent outputs -- half a
        // double-spend, recorded as a success.
        for image in &images {
            match self.spent.get(image) {
                Some(held) if *held != digest => {
                    return SpendOutcome::Conflict {
                        held: *held,
                        offered: digest,
                    };
                }
                _ => {}
            }
        }

        // Idempotent re-broadcast: every input already recorded, under this
        // same claim. Normal network behaviour, not a double-spend.
        if !images.is_empty() && images.iter().all(|image| self.spent.contains_key(image)) {
            return SpendOutcome::AlreadyRecorded;
        }

        for image in images {
            self.spent.insert(image, digest);
        }
        SpendOutcome::Accepted
    }

    /// Offer a claim, returning an error rather than an outcome when it
    /// conflicts — for callers that want `?` rather than a match.
    pub fn admit(&mut self, claim: &VerifiedPrivateClaim) -> Result<SpendOutcome> {
        match self.observe(claim) {
            SpendOutcome::Conflict { .. } => Err(PrivatePaymentError::AlreadySpent),
            other => Ok(other),
        }
    }

    /// Whether this key image has been seen.
    pub fn contains(&self, key_image: &[u8]) -> bool {
        self.spent.contains_key(key_image)
    }

    /// The digest of the claim first seen with `key_image`.
    pub fn claim_for(&self, key_image: &[u8]) -> Option<[u8; 32]> {
        self.spent.get(key_image).copied()
    }

    pub fn len(&self) -> usize {
        self.spent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spent.is_empty()
    }
}
