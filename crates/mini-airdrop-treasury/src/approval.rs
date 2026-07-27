//! Turning a [`mini_airdrop::ClaimOutcome`] into a
//! [`TreasuryApprovedPayout`]: enough of a real treasury signer set's
//! members, each proving their own approval with the same `did-mini` keys
//! their identity already trusts, agreed to release this exact payout.
//!
//! This is **not** a signed `mini_settlement::PaymentClaim`, and this
//! module never touches `mini_treasury::frost_sign` (that module's own
//! docs name it the "permanent honeypot" component requiring external
//! audit before real use, D-0035/D-0047). What this module produces is
//! evidence that enough authorized humans agreed -- the same
//! distinct-identity-approval-counting discipline `mini_treasury::signers`
//! already uses for every other treasury action, composed here rather
//! than reimplemented. Actually constructing and signing the real
//! settlement claim from a [`TreasuryApprovedPayout`] is separate, later,
//! still-gated work.

use std::collections::HashSet;

use did_mini::{Did, IndexedSig, Kel};
use mini_airdrop::ClaimOutcome;
use mini_treasury::{meets_threshold, TreasurySignerSet};

use crate::error::{PayoutApprovalError, Result};

/// The exact bytes a treasury signer approves: a fixed domain tag, then
/// the campaign id and every `ClaimOutcome` field, length- or
/// width-prefixed -- the same discipline `mini_airdrop::message_to_sign`
/// and `mini_settlement::claim_message` already use.
pub fn payout_message(campaign_id: &[u8], outcome: &ClaimOutcome) -> Vec<u8> {
    let root_bytes = outcome.identity_root.as_str().as_bytes();
    let mut msg = Vec::with_capacity(
        32 + 4 + campaign_id.len() + 4 + root_bytes.len() + 8 + 4 + outcome.recipient.len(),
    );
    msg.extend_from_slice(b"mini-airdrop-treasury/payout-approval/v1");
    msg.extend_from_slice(&(campaign_id.len() as u32).to_be_bytes());
    msg.extend_from_slice(campaign_id);
    msg.extend_from_slice(&(root_bytes.len() as u32).to_be_bytes());
    msg.extend_from_slice(root_bytes);
    msg.extend_from_slice(&outcome.amount_micro.to_be_bytes());
    msg.extend_from_slice(&(outcome.recipient.len() as u32).to_be_bytes());
    msg.extend_from_slice(&outcome.recipient);
    msg
}

/// A payout the treasury signer set has approved -- not itself a
/// settlement claim, only the evidence that enough authorized signers
/// agreed to one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreasuryApprovedPayout {
    pub outcome: ClaimOutcome,
    /// The distinct, verified, authorized signers whose approval counted
    /// toward the threshold, canonically sorted.
    pub approving_signers: Vec<Did>,
}

/// One candidate approval: a signer's real KEL plus signatures over
/// [`payout_message`]. Verification never errors out the whole batch on
/// one bad candidate -- a malformed KEL, a non-member signer, or a bad
/// signature simply does not count toward the threshold, the same
/// tolerance real asynchronous approval collection needs (an attacker
/// submitting garbage approvals cannot block honest ones from counting).
pub type CandidateApproval<'a> = (&'a Kel, &'a [IndexedSig]);

/// Verify `candidate_approvals` against `signer_set` for `outcome` under
/// `campaign_id`, returning a [`TreasuryApprovedPayout`] only if enough
/// distinct, verified, authorized signers approved.
pub fn verify_payout_approvals(
    campaign_id: &[u8],
    outcome: &ClaimOutcome,
    signer_set: &TreasurySignerSet,
    candidate_approvals: &[CandidateApproval<'_>],
) -> Result<TreasuryApprovedPayout> {
    if candidate_approvals.is_empty() {
        return Err(PayoutApprovalError::NoApprovalsPresented);
    }

    let msg = payout_message(campaign_id, outcome);
    let mut verified: Vec<Did> = Vec::new();

    for (kel, sigs) in candidate_approvals {
        let Ok(_) = kel.verify() else {
            continue;
        };
        let Ok(signer_did) = Did::from_scid(kel.scid()) else {
            continue;
        };
        if !signer_set.contains(&signer_did) {
            continue;
        }
        if kel.verify_message(&msg, sigs).is_err() {
            continue;
        }
        verified.push(signer_did);
    }

    if !meets_threshold(signer_set, &verified) {
        return Err(PayoutApprovalError::ThresholdNotMet);
    }

    let mut seen: HashSet<&str> = HashSet::new();
    let mut approving_signers: Vec<Did> = Vec::new();
    for did in &verified {
        if seen.insert(did.scid()) {
            approving_signers.push(did.clone());
        }
    }
    approving_signers.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    Ok(TreasuryApprovedPayout {
        outcome: outcome.clone(),
        approving_signers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn signer() -> (Controller, Did) {
        let c = Controller::incept_single().unwrap();
        let did = c.did();
        (c, did)
    }

    fn sample_outcome(identity_root: Did) -> ClaimOutcome {
        ClaimOutcome {
            identity_root,
            amount_micro: 1_000,
            recipient: b"payee".to_vec(),
        }
    }

    #[test]
    fn no_candidate_approvals_is_rejected_before_anything_else() {
        let (_c, claimant_root) = signer();
        let outcome = sample_outcome(claimant_root);
        let (_s1, d1) = signer();
        let set = TreasurySignerSet::new(vec![d1], 1).unwrap();

        assert_eq!(
            verify_payout_approvals(b"campaign-1", &outcome, &set, &[]).unwrap_err(),
            PayoutApprovalError::NoApprovalsPresented
        );
    }

    #[test]
    fn a_single_authorized_signers_approval_meets_a_threshold_of_one() {
        let (_c, claimant_root) = signer();
        let outcome = sample_outcome(claimant_root);
        let (signer1, d1) = signer();
        let set = TreasurySignerSet::new(vec![d1.clone()], 1).unwrap();

        let msg = payout_message(b"campaign-1", &outcome);
        let sigs = signer1.sign_message(&msg);
        let kel1 = signer1.kel();
        let candidates = vec![(&kel1, sigs.as_slice())];

        let approved = verify_payout_approvals(b"campaign-1", &outcome, &set, &candidates).unwrap();
        assert_eq!(approved.approving_signers, vec![d1]);
    }

    #[test]
    fn a_single_approval_does_not_meet_a_threshold_of_two() {
        let (_c, claimant_root) = signer();
        let outcome = sample_outcome(claimant_root);
        let (signer1, d1) = signer();
        let (_signer2, d2) = signer();
        let set = TreasurySignerSet::new(vec![d1, d2], 2).unwrap();

        let msg = payout_message(b"campaign-1", &outcome);
        let sigs = signer1.sign_message(&msg);
        let kel1 = signer1.kel();
        let candidates = vec![(&kel1, sigs.as_slice())];

        assert_eq!(
            verify_payout_approvals(b"campaign-1", &outcome, &set, &candidates).unwrap_err(),
            PayoutApprovalError::ThresholdNotMet
        );
    }

    #[test]
    fn two_distinct_authorized_approvals_meet_a_threshold_of_two() {
        let (_c, claimant_root) = signer();
        let outcome = sample_outcome(claimant_root);
        let (signer1, d1) = signer();
        let (signer2, d2) = signer();
        let set = TreasurySignerSet::new(vec![d1.clone(), d2.clone()], 2).unwrap();

        let msg = payout_message(b"campaign-1", &outcome);
        let sigs1 = signer1.sign_message(&msg);
        let sigs2 = signer2.sign_message(&msg);
        let kel1 = signer1.kel();
        let kel2 = signer2.kel();
        let candidates = vec![(&kel1, sigs1.as_slice()), (&kel2, sigs2.as_slice())];

        let approved = verify_payout_approvals(b"campaign-1", &outcome, &set, &candidates).unwrap();
        let mut expected = vec![d1, d2];
        expected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        assert_eq!(approved.approving_signers, expected);
    }

    #[test]
    fn a_non_member_signers_valid_signature_never_counts() {
        let (_c, claimant_root) = signer();
        let outcome = sample_outcome(claimant_root);
        let (member, d1) = signer();
        let (_other_member, d2) = signer(); // a real second member who never approves
        let (outsider, _d_outsider) = signer(); // never added to the signer set
        let set = TreasurySignerSet::new(vec![d1, d2], 2).unwrap();

        let msg = payout_message(b"campaign-1", &outcome);
        let sigs_member = member.sign_message(&msg);
        let sigs_outsider = outsider.sign_message(&msg);
        let kel_member = member.kel();
        let kel_outsider = outsider.kel();
        let candidates = vec![
            (&kel_member, sigs_member.as_slice()),
            (&kel_outsider, sigs_outsider.as_slice()),
        ];

        // Outsider's approval is real and valid but not a set member, so
        // it can never push the count past 1 -- threshold of 2 is unmet.
        assert_eq!(
            verify_payout_approvals(b"campaign-1", &outcome, &set, &candidates).unwrap_err(),
            PayoutApprovalError::ThresholdNotMet
        );
    }

    #[test]
    fn a_member_kel_presented_with_someone_elses_signature_never_counts() {
        let (_c, claimant_root) = signer();
        let outcome = sample_outcome(claimant_root);
        let (member, d1) = signer();
        let (attacker, _da) = signer();
        let set = TreasurySignerSet::new(vec![d1], 1).unwrap();

        let msg = payout_message(b"campaign-1", &outcome);
        // `member`'s real KEL is presented (a genuine set member), but the
        // signature bytes actually came from `attacker`'s unrelated key --
        // `Kel::verify_message` must reject this, not just check KEL
        // self-consistency and membership.
        let forged_sigs = attacker.sign_message(&msg);
        let kel_member = member.kel();
        let candidates = vec![(&kel_member, forged_sigs.as_slice())];

        assert_eq!(
            verify_payout_approvals(b"campaign-1", &outcome, &set, &candidates).unwrap_err(),
            PayoutApprovalError::ThresholdNotMet
        );
    }

    #[test]
    fn duplicate_approvals_from_the_same_signer_only_count_once() {
        let (_c, claimant_root) = signer();
        let outcome = sample_outcome(claimant_root);
        let (signer1, d1) = signer();
        let (_signer2, d2) = signer();
        let set = TreasurySignerSet::new(vec![d1, d2], 2).unwrap();

        let msg = payout_message(b"campaign-1", &outcome);
        let sigs = signer1.sign_message(&msg);
        // The same signer's approval presented twice.
        let kel1 = signer1.kel();
        let candidates = vec![(&kel1, sigs.as_slice()), (&kel1, sigs.as_slice())];

        assert_eq!(
            verify_payout_approvals(b"campaign-1", &outcome, &set, &candidates).unwrap_err(),
            PayoutApprovalError::ThresholdNotMet
        );
    }

    #[test]
    fn an_approval_signed_for_a_different_campaign_does_not_count() {
        let (_c, claimant_root) = signer();
        let outcome = sample_outcome(claimant_root);
        let (signer1, d1) = signer();
        let set = TreasurySignerSet::new(vec![d1], 1).unwrap();

        let msg = payout_message(b"different-campaign", &outcome);
        let sigs = signer1.sign_message(&msg);
        let kel1 = signer1.kel();
        let candidates = vec![(&kel1, sigs.as_slice())];

        assert_eq!(
            verify_payout_approvals(b"campaign-1", &outcome, &set, &candidates).unwrap_err(),
            PayoutApprovalError::ThresholdNotMet
        );
    }
}
