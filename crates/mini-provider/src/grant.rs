//! The engagement grant: a scoped, revocable, non-delegable capability from
//! a human to a provider.

use did_mini::Did;
use mini_objects::ObjectId;

use crate::error::{ProviderError, Result};

/// Hard limit on [`EngagementGrant::permits`].
pub const MAX_PERMITS: usize = 16;

/// A scoped, revocable, NON-DELEGABLE grant from a human to a provider.
/// The `did:mini` root is never handed over -- the provider learns a
/// customer, not a life.
///
/// A separate typed capability domain, holder-bound in the manner of
/// `mini_objects::CapabilityGrant` and `mini_relay::MailboxGrant`, but
/// never interchangeable with either -- issuing, revoking, and verifying
/// a real signed grant against a `holder_commitment` is execution-layer
/// wiring, deferred past this Wave 1 vocabulary crate (see the crate-level
/// docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngagementGrant {
    /// Per-provider pseudonym (see `mini_objects::derive_scoped_pseudonym`
    /// with `PseudonymPurpose::CapabilityHolder`), never the human's root
    /// `Did`. Two providers cannot correlate a human by comparing grants
    /// (FD-18 Part I, T4).
    pub subject: Did,
    pub provider: Did,
    /// The [`crate::ProviderDeclaration`] this grant was made against,
    /// referenced by its content id once wrapped in a real
    /// `mini_objects::Object` (deferred, see crate docs).
    pub declaration: ObjectId,

    /// Enumerated permissions. There is no `All`.
    pub permits: Vec<Permit>,

    pub not_before_ms: u64,
    pub not_after_ms: u64,

    /// Binds the grant to one holder. A provider cannot pass a customer to
    /// a successor, an acquirer, or a data broker -- exercising the grant
    /// requires proving possession of the secret this commits to, the same
    /// discipline `mini_objects::CapabilityTokenCommitment` uses. Proof
    /// verification is deferred past this Wave 1 vocabulary crate.
    pub holder_commitment: [u8; 32],
}

impl EngagementGrant {
    /// Structural well-formedness ONLY, matching
    /// [`crate::ProviderDeclaration::check_wellformed`]'s discipline: never
    /// a judgment about the provider, only about the shape of the grant.
    pub fn check_wellformed(&self) -> Result<()> {
        if self.not_before_ms >= self.not_after_ms {
            return Err(ProviderError::InvalidGrantWindow);
        }
        if self.permits.is_empty() {
            return Err(ProviderError::EmptyPermits);
        }
        if self.permits.len() > MAX_PERMITS {
            return Err(ProviderError::TooManyPermits);
        }
        Ok(())
    }

    /// Whether the grant's validity window covers `now_ms`. This is a
    /// pure time-window check -- it says nothing about whether the grant
    /// has been revoked, since revocation tracking is execution-layer
    /// state this crate does not hold.
    pub fn is_active_at(&self, now_ms: u64) -> bool {
        now_ms >= self.not_before_ms && now_ms < self.not_after_ms
    }
}

/// One thing an [`EngagementGrant`] authorizes. Deliberately open
/// (`#[non_exhaustive]`): FD-18 Part II.1 lists the first known permits;
/// new provider service shapes will need new variants over time, and
/// adding one is a normal PR, not a protocol change.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Permit {
    QuoteConversion,
    ExecuteConversion {
        max_micromini_per_period: u64,
        period_ms: u64,
    },
    FundSpendRail {
        max_micromini_per_period: u64,
        period_ms: u64,
    },
    ReceiveDeliveryAddress,
    ReadAttestation {
        kind: AttestationKind,
    },
}

/// What kind of attestation a [`Permit::ReadAttestation`] permit covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttestationKind {
    /// The verdict only (e.g. a thumbs up/down), never the free-text body.
    Verdict,
    FullText,
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;
    use mini_objects::{ObjectBuilder, ObjectType, Payload};

    fn sample_did() -> Did {
        Controller::incept_single().unwrap().did()
    }

    /// Any well-formed object id; content doesn't matter for these tests.
    fn sample_object_id() -> ObjectId {
        let root = Controller::incept_single_from_seeds(&[1u8; 32], &[2u8; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &[3u8; 32], &[4u8; 32])
                .unwrap();
        let obj = ObjectBuilder::new(ObjectType::Custom("test".to_string()))
            .payload(Payload::Public(vec![1, 2, 3]))
            .sign(&root.did(), &device)
            .unwrap();
        obj.id().clone()
    }

    fn wellformed_grant() -> EngagementGrant {
        EngagementGrant {
            subject: sample_did(),
            provider: sample_did(),
            declaration: sample_object_id(),
            permits: vec![Permit::QuoteConversion],
            not_before_ms: 1_000,
            not_after_ms: 2_000,
            holder_commitment: [7u8; 32],
        }
    }

    #[test]
    fn a_wellformed_grant_is_accepted() {
        assert!(wellformed_grant().check_wellformed().is_ok());
    }

    #[test]
    fn an_inverted_window_is_rejected() {
        let mut g = wellformed_grant();
        g.not_before_ms = 2_000;
        g.not_after_ms = 1_000;
        assert_eq!(g.check_wellformed(), Err(ProviderError::InvalidGrantWindow));
    }

    #[test]
    fn an_equal_window_is_rejected() {
        let mut g = wellformed_grant();
        g.not_before_ms = 1_000;
        g.not_after_ms = 1_000;
        assert_eq!(g.check_wellformed(), Err(ProviderError::InvalidGrantWindow));
    }

    #[test]
    fn a_grant_with_no_permits_is_rejected() {
        let mut g = wellformed_grant();
        g.permits = vec![];
        assert_eq!(g.check_wellformed(), Err(ProviderError::EmptyPermits));
    }

    #[test]
    fn too_many_permits_is_rejected() {
        let mut g = wellformed_grant();
        g.permits = (0..MAX_PERMITS + 1)
            .map(|_| Permit::QuoteConversion)
            .collect();
        assert_eq!(g.check_wellformed(), Err(ProviderError::TooManyPermits));
    }

    #[test]
    fn is_active_at_is_a_half_open_window() {
        let g = wellformed_grant();
        assert!(!g.is_active_at(999));
        assert!(g.is_active_at(1_000));
        assert!(g.is_active_at(1_999));
        assert!(!g.is_active_at(2_000));
    }
}
