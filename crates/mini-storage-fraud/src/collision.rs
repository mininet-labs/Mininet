//! Cross-identity storage-collision evidence: two distinct identity roots
//! each publish a signed [`StorageCommitmentClaim`] naming the *same*
//! [`mini_spacetime::StorageCommitment`], despite [`derive_replica_id`]
//! having bound each of their replicas to a different `replica_id`. See
//! the module-level and `docs/design/storage-fraud-detection.md` doc for
//! why that collision can only happen if at least one of them did not
//! actually seal an independent replica.
//!
//! ## What this does and does not do
//!
//! Mirrors `mini_consensus::evidence::EquivocationEvidence`'s own
//! restraint exactly: this module produces and verifies proof, and stops
//! there. It assigns no penalty, excludes no provider, and revokes no
//! storage reward. That consequence layer does not exist yet -- see the
//! design doc's "Required follow-up".

use did_mini::Kel;

use crate::commitment_claim::{derive_replica_id, StorageCommitmentClaim};
use crate::error::{FraudError, Result};

/// Two conflicting signed commitment claims from *different* identity
/// roots, naming the identical [`mini_spacetime::StorageCommitment`].
/// Construct these only from claims you actually received;
/// [`verify_collision`] is what decides whether they really constitute
/// proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionEvidence {
    pub first: StorageCommitmentClaim,
    pub second: StorageCommitmentClaim,
}

impl CollisionEvidence {
    /// The colliding commitment both claims name.
    pub fn merkle_root(&self) -> [u8; 32] {
        self.first.commitment.merkle_root
    }
}

/// Verify that `evidence` really is a collision: the two claims name
/// *different* provider roots, commit to the identical
/// [`mini_spacetime::StorageCommitment`], each verifies against its own
/// claimed root's KEL, and each claim's own `expected_replica_id` really
/// does differ (ruling out the degenerate case of two claims that agreed
/// in advance to reuse one context under what would otherwise be one
/// shared replica id, which is not evidence of anything). Returns an
/// error for anything that is not genuine, independently checkable proof,
/// so a fabricated or mismatched "accusation" can never be passed off as
/// real.
pub fn verify_collision(
    evidence: &CollisionEvidence,
    first_kel: &Kel,
    second_kel: &Kel,
) -> Result<()> {
    let (a, b) = (&evidence.first, &evidence.second);

    if a.provider == b.provider {
        return Err(FraudError::NotACollision);
    }
    if a.commitment.merkle_root != b.commitment.merkle_root
        || a.commitment.block_count != b.commitment.block_count
    {
        return Err(FraudError::NotACollision);
    }
    if derive_replica_id(&a.provider, &a.context) == derive_replica_id(&b.provider, &b.context) {
        return Err(FraudError::NotACollision);
    }

    a.verify(first_kel)?;
    b.verify(second_kel)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use did_mini::Controller;
    use mini_spacetime::StorageCommitment;

    use super::*;

    fn provider(seed: u8) -> Controller {
        Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
    }

    fn shared_commitment() -> StorageCommitment {
        StorageCommitment {
            merkle_root: [0x42; 32],
            block_count: 16,
        }
    }

    #[test]
    fn two_distinct_providers_naming_the_same_commitment_is_genuine_collision() {
        let a = provider(1);
        let b = provider(2);
        let first = StorageCommitmentClaim::issue(&a, [1u8; 32], shared_commitment(), 1_000);
        let second = StorageCommitmentClaim::issue(&b, [2u8; 32], shared_commitment(), 1_100);
        let evidence = CollisionEvidence { first, second };
        assert!(verify_collision(&evidence, &a.kel(), &b.kel()).is_ok());
        assert_eq!(evidence.merkle_root(), [0x42; 32]);
    }

    #[test]
    fn the_same_provider_naming_the_same_commitment_twice_is_not_collision() {
        let a = provider(3);
        let first = StorageCommitmentClaim::issue(&a, [1u8; 32], shared_commitment(), 1_000);
        let second = StorageCommitmentClaim::issue(&a, [1u8; 32], shared_commitment(), 1_100);
        let evidence = CollisionEvidence { first, second };
        assert_eq!(
            verify_collision(&evidence, &a.kel(), &a.kel()),
            Err(FraudError::NotACollision)
        );
    }

    #[test]
    fn two_providers_naming_different_commitments_is_not_collision() {
        let a = provider(4);
        let b = provider(5);
        let other = StorageCommitment {
            merkle_root: [0x99; 32],
            block_count: 16,
        };
        let first = StorageCommitmentClaim::issue(&a, [1u8; 32], shared_commitment(), 1_000);
        let second = StorageCommitmentClaim::issue(&b, [2u8; 32], other, 1_100);
        let evidence = CollisionEvidence { first, second };
        assert_eq!(
            verify_collision(&evidence, &a.kel(), &b.kel()),
            Err(FraudError::NotACollision)
        );
    }

    #[test]
    fn a_forged_second_claim_is_not_accepted_as_evidence() {
        // The "second" claim carries b's provider field but is actually
        // signed by c's key material -- a fabricated accusation, not real
        // proof, must not verify.
        let a = provider(6);
        let b = provider(7);
        let c = provider(8);
        let first = StorageCommitmentClaim::issue(&a, [1u8; 32], shared_commitment(), 1_000);
        let mut forged = StorageCommitmentClaim::issue(&c, [2u8; 32], shared_commitment(), 1_100);
        forged.provider = b.did();
        let evidence = CollisionEvidence {
            first,
            second: forged,
        };
        assert_eq!(
            verify_collision(&evidence, &a.kel(), &b.kel()),
            Err(FraudError::BadProviderSignature)
        );
    }

    #[test]
    fn a_genuine_independent_seal_under_each_derived_replica_id_never_collides() {
        // End-to-end proof this scheme is meaningful, not just the
        // derivation function in isolation: two providers independently
        // sealing the *same* source data under their own
        // derive_replica_id-bound replica ids produce two different
        // Merkle roots, exactly as mini-porep::seal's own
        // different_replica_ids_seal_to_different_replicas test already
        // proves for arbitrary replica ids.
        let a = provider(9);
        let b = provider(10);
        let context = [0xAB; 32];
        let data = vec![7u8; 4096];

        let params_a =
            mini_porep::SealParams::new(derive_replica_id(&a.did(), &context), 4).unwrap();
        let params_b =
            mini_porep::SealParams::new(derive_replica_id(&b.did(), &context), 4).unwrap();
        let replica_a = mini_porep::seal(&params_a, &data).unwrap();
        let replica_b = mini_porep::seal(&params_b, &data).unwrap();

        assert_ne!(replica_a.replica_root(), replica_b.replica_root());
    }
}
