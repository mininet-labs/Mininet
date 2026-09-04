//! Shielded spends, as consensus sees them: **opaque bytes and nothing
//! else**.
//!
//! A private payment is a ring signature, a set of Pedersen commitments, a
//! balance equation and a stack of range proofs. None of that appears here,
//! and none of it may. What the canonical ledger has to decide about a
//! shielded spend is exactly one thing — *which* claim first spent a given
//! output — and that question is answerable from two opaque values: the
//! **key image** the spend published, and the **digest** of the claim that
//! published it.
//!
//! # Why the chain does not understand private payments
//!
//! Three reasons, in increasing order of how badly they would bite.
//!
//! 1. **The voice/value wall (P1, Directive 16).** `mini-private-payment`
//!    reaches `mini-value`, and this crate reaches `mini-chain`. A
//!    dependency edge between them would be the first path in this tree
//!    from a value crate to the crate that counts votes. There is none
//!    today and there must be none tomorrow, so the two halves meet through
//!    `(Vec<u8>, [u8; 32])` — standard-library types, no shared crate, no
//!    format either side can drift from.
//! 2. **Liveness.** A validator that had to verify a Bulletproof and a
//!    16-member MLSAG per shielded spend before it could vote would be a
//!    validator whose block time is set by the most expensive cryptography
//!    in the protocol. Verification belongs where it already is —
//!    `mini_private_payment::verify`, run by whoever cares — and the chain
//!    orders the results.
//! 3. **Neutrality.** A chain that could read a payment's contents is a
//!    chain that could be made to treat some payments differently. This is
//!    the same argument `mini-private-payment` makes for not depending on
//!    `mini-social`: the layer that orders transactions must not be able to
//!    tell what they are for.
//!
//! # What this costs, stated plainly
//!
//! The chain finalizes a key image on a proposer's say-so. It does **not**
//! check that some valid claim produced it, because it cannot — that check
//! is the cryptography it deliberately cannot see. A Byzantine proposer can
//! therefore burn an output that is not theirs by including a record naming
//! its key image, and honest nodes would finalize it.
//!
//! That is a real hole and it is not closed here. Closing it needs a
//! validity rule the chain *can* check — a succinct proof, or a validator
//! set that does verify claims and is measured for it — and that is
//! [roadmap R8](../../../docs/ROADMAP_TO_RELEASE.md)'s territory, not this
//! module's. What this module does is make the *ordering* real, which is
//! what M3 requires and what nothing implemented before it.

use mini_crypto::HashAlgorithm;

/// Hard cap on shielded-spend records per block, applied before any
/// allocation — the same discipline [`crate::MAX_CLAIMS_PER_BLOCK`] applies
/// to the transparent list.
pub const MAX_NULLIFIERS_PER_BLOCK: usize = 4_096;

/// Longest key image the ledger will store. Ristretto key images are 32
/// bytes; the ceiling leaves room for a post-quantum successor without
/// letting an untrusted field amplify state memory.
pub const MAX_KEY_IMAGE_BYTES: usize = 64;

/// One shielded spend's claim on one output.
///
/// A claim spending several outputs contributes several records, all
/// carrying that claim's digest. That grouping is load-bearing — see
/// [`crate::apply_block`] for why a group is all-or-nothing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NullifierRecord {
    /// The double-spend nullifier, opaque here.
    ///
    /// Produced by `mini_private_payment::VerifiedPrivateClaim::key_images`,
    /// which this crate cannot name and does not link.
    pub key_image: Vec<u8>,
    /// The digest of the claim that spent it — the claim's identity, from
    /// `VerifiedPrivateClaim::transcript_digest`.
    pub claim_digest: [u8; 32],
}

impl NullifierRecord {
    pub fn new(key_image: impl Into<Vec<u8>>, claim_digest: [u8; 32]) -> Self {
        NullifierRecord {
            key_image: key_image.into(),
            claim_digest,
        }
    }

    /// Whether this record is storable at all: a key image must be
    /// non-empty and within [`MAX_KEY_IMAGE_BYTES`].
    ///
    /// Checked before anything is inserted, so an oversized field from an
    /// untrusted proposer cannot amplify state memory.
    pub fn is_well_formed(&self) -> bool {
        !self.key_image.is_empty() && self.key_image.len() <= MAX_KEY_IMAGE_BYTES
    }

    /// Canonical bytes, for the body hash.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Vec::with_capacity(4 + self.key_image.len() + 32);
        w.extend_from_slice(&(self.key_image.len() as u32).to_be_bytes());
        w.extend_from_slice(&self.key_image);
        w.extend_from_slice(&self.claim_digest);
        w
    }

    pub fn digest(&self) -> [u8; 32] {
        let mut w = Vec::new();
        w.extend_from_slice(b"mini-execution/nullifier-record/v1");
        w.extend_from_slice(&self.canonical_bytes());
        HashAlgorithm::Blake3.digest(&w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_is_well_formed_only_within_its_bounds() {
        assert!(NullifierRecord::new(vec![7u8; 32], [1u8; 32]).is_well_formed());
        assert!(NullifierRecord::new(vec![7u8; MAX_KEY_IMAGE_BYTES], [1u8; 32]).is_well_formed());
        // Empty names no output; oversized amplifies state memory from an
        // untrusted field.
        assert!(!NullifierRecord::new(Vec::new(), [1u8; 32]).is_well_formed());
        assert!(
            !NullifierRecord::new(vec![7u8; MAX_KEY_IMAGE_BYTES + 1], [1u8; 32]).is_well_formed()
        );
    }

    #[test]
    fn the_length_prefix_stops_two_different_records_hashing_alike() {
        // Without the prefix, ("ab", digest) and ("a", "b" ++ digest) would
        // serialize to the same bytes. The key image is attacker-chosen, so
        // this is a real collision to close rather than a theoretical one.
        let left = NullifierRecord::new(b"ab".to_vec(), [0u8; 32]);
        let right = NullifierRecord::new(b"a".to_vec(), [0u8; 32]);
        assert_ne!(left.digest(), right.digest());
        assert_ne!(left.canonical_bytes(), right.canonical_bytes());
    }

    #[test]
    fn the_record_digest_is_a_stable_vector() {
        // Pinned: this is what a block body commits to, so a change here is
        // a consensus-format change.
        let record = NullifierRecord::new(vec![0xabu8; 32], [0xcd; 32]);
        let hex: String = record.digest().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "779184dc2b4dbc6fe3e383fa851abdda5cb76f3a51d4f9698fab488b02587935"
        );
    }
}
