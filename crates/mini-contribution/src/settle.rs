//! Turning a completed, evidence-bound engagement into real settlement
//! claims.

use mini_crypto::SigningKey;
use mini_engagement::{Engagement, EngagementState};
use mini_settlement::{sign_claim_for_network, PaymentClaim};

use crate::error::{ContributionError, Result};
use crate::evidence::DeliveryEvidence;
use crate::split::{split_amount, RewardSplit};

/// Build the settlement claims for a completed, delivery-evidenced
/// engagement: one [`PaymentClaim`] per non-zero payee share, signed by the
/// payer's own key, each at its own fresh sequence number starting at
/// `start_sequence`. Builds but does **not** submit anything -- the caller
/// admits each returned claim into a real `PaymentAdmissionPool` (or
/// equivalent) themselves.
///
/// `engagement.escrow_claim` itself is never submitted here: it remains the
/// locally recorded terms of the deal (FD-05 -- a signed promise is never
/// final ownership by itself). These freshly built claims, once actually
/// admitted and finalized by a caller, are the real payment.
///
/// `_evidence` is not read by this function -- its role is purely as a
/// caller-side proof that [`crate::bind_delivery_evidence`] already checked
/// a real verified serve against this exact engagement before settlement
/// was attempted. A `DeliveryEvidence` value cannot exist otherwise.
#[allow(clippy::too_many_arguments)]
pub fn settle_completed_engagement(
    engagement: &Engagement,
    _evidence: &DeliveryEvidence,
    payer_key: &SigningKey,
    split: RewardSplit,
    creator_account: Vec<u8>,
    seeder_account: Vec<u8>,
    start_sequence: u64,
    valid_until_ms: u64,
    last_known_chain: &[u8],
    now_ms: u64,
) -> Result<Vec<PaymentClaim>> {
    if !matches!(engagement.state, EngagementState::Completed { .. }) {
        return Err(ContributionError::EngagementNotCompleted);
    }

    let shares = split_amount(
        engagement.escrow_claim.amount_micro,
        split,
        creator_account,
        seeder_account,
    );

    let network_id = engagement.escrow_claim.network_id;
    let mut claims = Vec::with_capacity(shares.len());
    for (i, share) in shares.into_iter().enumerate() {
        let sequence = start_sequence
            .checked_add(i as u64)
            .ok_or(ContributionError::SequenceOverflow)?;
        let claim = sign_claim_for_network(
            payer_key,
            &share.account,
            share.amount_micro,
            sequence,
            valid_until_ms,
            &network_id,
            last_known_chain,
            now_ms,
        )
        .map_err(ContributionError::ClaimSigning)?;
        claims.push(claim);
    }
    Ok(claims)
}
