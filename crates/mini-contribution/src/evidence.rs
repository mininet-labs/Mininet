//! Binding a verified storage-serve delivery to a specific engagement.

use mini_engagement::Engagement;
use mini_storage::ServeVerdict;

use crate::error::{ContributionError, Result};

/// A [`ServeVerdict`] checked to actually belong to a given engagement: the
/// same content, delivered from the engagement's performer, witnessed by
/// the engagement's payer. Wrapping a verdict in this type is the only
/// thing standing between "a serve happened somewhere" and "this specific
/// engagement's delivery is verified" -- `mini_storage::verify_serve` alone
/// has no notion of an engagement. Only constructible via
/// [`bind_delivery_evidence`], so any caller holding one already had that
/// check pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryEvidence(ServeVerdict);

impl DeliveryEvidence {
    /// The verified serve this evidence wraps.
    pub fn verdict(&self) -> &ServeVerdict {
        &self.0
    }
}

/// Check that a [`ServeVerdict`] actually evidences `engagement`'s
/// delivery: the same content id, served by the engagement's performer,
/// witnessed by the engagement's payer. Does not re-verify the underlying
/// receipt -- callers must have already produced `verdict` via a real
/// `mini_storage::verify_serve` call.
pub fn bind_delivery_evidence(
    engagement: &Engagement,
    verdict: ServeVerdict,
) -> Result<DeliveryEvidence> {
    if verdict.content_id != engagement.terms {
        return Err(ContributionError::ContentIdMismatch);
    }
    if verdict.host_root != engagement.performer {
        return Err(ContributionError::HostRoleMismatch);
    }
    if verdict.witness_root != engagement.payer {
        return Err(ContributionError::WitnessRoleMismatch);
    }
    Ok(DeliveryEvidence(verdict))
}
