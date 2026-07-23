//! Claiming an allocation: a claimant proves control of their identity
//! root with the same keys `did-mini` already trusts (no bespoke
//! signature scheme), and this module verifies that proof against the
//! snapshot and a [`crate::registry::ClaimedRegistry`] before returning a
//! [`ClaimOutcome`] -- **not** a `mini_settlement::PaymentClaim`. Actually
//! moving value is a separate step this crate deliberately does not
//! perform: whatever process holds the airdrop treasury's real signing
//! authority (a `mini-treasury` FROST quorum, in production) takes a
//! `ClaimOutcome`'s `recipient`/`amount_micro` and builds/signs its own
//! `mini_settlement::PaymentClaim` from it. This crate never sees, needs,
//! or could hold that authority.

use did_mini::{Did, IndexedSig, Kel};

use crate::error::{AirdropError, Result};
use crate::registry::ClaimedRegistry;
use crate::snapshot::AirdropSnapshot;

/// Hard limit on [`ClaimRequest::recipient`].
pub const MAX_RECIPIENT_BYTES: usize = 256;

/// A claimant's signed request to redeem their snapshot allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimRequest {
    /// Must match the snapshot's own `campaign_id` exactly, or
    /// verification fails with [`AirdropError::CampaignMismatch`] --
    /// binds a claim to one campaign so it can never be replayed against
    /// another the same identity root also happens to be eligible for.
    pub campaign_id: Vec<u8>,
    /// Which snapshot entry this claims. Must equal the scid the
    /// presented KEL actually proves control of, checked by
    /// [`verify_and_resolve_claim`] before the signature is even checked.
    pub identity_root: Did,
    /// Opaque payout address bytes (e.g. a fresh `mini_value` stealth
    /// address, or a plain settlement payee key) -- this crate has no
    /// opinion on how it was derived, mirroring `mini_bounty::BountyClaim
    /// ::payout_address`'s own convention.
    pub recipient: Vec<u8>,
    /// Caller-chosen nonce distinguishing otherwise-identical requests
    /// (e.g. a retry after a dropped response) -- not itself
    /// replay-protection (the registry is), just message uniqueness.
    pub nonce: u64,
}

impl ClaimRequest {
    fn check_wellformed(&self) -> Result<()> {
        if self.recipient.len() > MAX_RECIPIENT_BYTES {
            return Err(AirdropError::RecipientTooLong);
        }
        Ok(())
    }
}

/// The exact bytes a claimant signs: a fixed domain tag, then every field
/// length- or width-prefixed, so no two distinct requests can ever encode
/// to the same message -- the same discipline `mini_settlement::
/// claim_message`/`mini_bounty::claim_message` already use.
fn claim_request_message(
    campaign_id: &[u8],
    identity_root: &Did,
    recipient: &[u8],
    nonce: u64,
) -> Vec<u8> {
    let root_bytes = identity_root.as_str().as_bytes();
    let mut msg = Vec::with_capacity(
        24 + 4 + campaign_id.len() + 4 + root_bytes.len() + 4 + recipient.len() + 8,
    );
    msg.extend_from_slice(b"mini-airdrop/claim-request/v1");
    msg.extend_from_slice(&(campaign_id.len() as u32).to_be_bytes());
    msg.extend_from_slice(campaign_id);
    msg.extend_from_slice(&(root_bytes.len() as u32).to_be_bytes());
    msg.extend_from_slice(root_bytes);
    msg.extend_from_slice(&(recipient.len() as u32).to_be_bytes());
    msg.extend_from_slice(recipient);
    msg.extend_from_slice(&nonce.to_be_bytes());
    msg
}

/// The exact message a claimant must sign for `request` -- exposed so a
/// real wallet can call this directly (e.g. via `did_mini::Controller::
/// sign_message`) rather than reconstructing the encoding itself.
pub fn message_to_sign(request: &ClaimRequest) -> Vec<u8> {
    claim_request_message(
        &request.campaign_id,
        &request.identity_root,
        &request.recipient,
        request.nonce,
    )
}

/// What a successfully verified claim resolves to: enough for whatever
/// holds the real treasury signing authority to build a settlement claim
/// from, and nothing more. Never itself a proof that value has moved --
/// FD-05 applies unchanged here just as it does in `mini-engagement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimOutcome {
    pub identity_root: Did,
    pub amount_micro: u64,
    pub recipient: Vec<u8>,
}

/// Verify `request` against `snapshot`, `claimant_kel`, `sigs`, and
/// `registry`, in order:
///
/// 1. `request.campaign_id` must equal `snapshot.campaign_id()`.
/// 2. `claimant_kel` must self-verify and its scid must equal
///    `request.identity_root`'s scid -- the presented KEL must actually
///    be *this* identity's KEL, not just any valid KEL.
/// 3. `sigs` must meet `claimant_kel`'s current signing threshold over
///    [`message_to_sign`] -- the claimant proved control with the same
///    keys `did-mini` already trusts.
/// 4. `request.identity_root` must have an entry in `snapshot`.
/// 5. `request.identity_root` must not already be in `registry`.
///
/// Only on success does this function call `registry.mark_claimed` --
/// every failure path leaves `registry` untouched, so a caller can retry
/// after fixing e.g. a malformed request without burning the claim.
pub fn verify_and_resolve_claim(
    snapshot: &AirdropSnapshot,
    request: &ClaimRequest,
    sigs: &[IndexedSig],
    claimant_kel: &Kel,
    registry: &mut impl ClaimedRegistry,
    now_ms: u64,
) -> Result<ClaimOutcome> {
    request.check_wellformed()?;

    if request.campaign_id != snapshot.campaign_id() {
        return Err(AirdropError::CampaignMismatch);
    }

    claimant_kel.verify().map_err(AirdropError::BadKel)?;
    let claimant_did =
        did_mini::Did::from_scid(claimant_kel.scid()).map_err(AirdropError::BadKel)?;
    if claimant_did != request.identity_root {
        return Err(AirdropError::IdentityMismatch);
    }

    let msg = message_to_sign(request);
    claimant_kel
        .verify_message(&msg, sigs)
        .map_err(|_| AirdropError::SignatureThresholdNotMet)?;

    let entry = snapshot
        .entry_for(&request.identity_root)
        .ok_or(AirdropError::NotEligible)?;

    if registry.already_claimed(&request.identity_root) {
        return Err(AirdropError::AlreadyClaimed);
    }

    registry.mark_claimed(&request.identity_root, now_ms);

    Ok(ClaimOutcome {
        identity_root: entry.identity_root.clone(),
        amount_micro: entry.amount_micro,
        recipient: request.recipient.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::InMemoryClaimedRegistry;
    use crate::snapshot::{AllocationEntry, SnapshotBuilder};
    use did_mini::Controller;
    use mini_crypto::SigningKey;

    fn snapshot_with(controller: &Controller, amount_micro: u64) -> AirdropSnapshot {
        let mut b = SnapshotBuilder::new(b"campaign-1".to_vec()).unwrap();
        b.insert(AllocationEntry {
            identity_root: controller.did(),
            amount_micro,
            human_status: None,
            reason: "test".to_string(),
        })
        .unwrap();
        b.build()
    }

    fn request_for(controller: &Controller, recipient: &[u8], nonce: u64) -> ClaimRequest {
        ClaimRequest {
            campaign_id: b"campaign-1".to_vec(),
            identity_root: controller.did(),
            recipient: recipient.to_vec(),
            nonce,
        }
    }

    #[test]
    fn a_valid_claim_resolves_and_marks_the_registry() {
        let claimant = Controller::incept_single().unwrap();
        let snapshot = snapshot_with(&claimant, 1_000);
        let request = request_for(&claimant, b"payee-address", 0);
        let sigs = claimant.sign_message(&message_to_sign(&request));
        let mut registry = InMemoryClaimedRegistry::new();

        let outcome = verify_and_resolve_claim(
            &snapshot,
            &request,
            &sigs,
            &claimant.kel(),
            &mut registry,
            500,
        )
        .unwrap();

        assert_eq!(outcome.identity_root, claimant.did());
        assert_eq!(outcome.amount_micro, 1_000);
        assert_eq!(outcome.recipient, b"payee-address");
        assert!(registry.already_claimed(&claimant.did()));
        assert_eq!(registry.claimed_at(&claimant.did()), Some(500));
    }

    #[test]
    fn a_second_claim_by_the_same_identity_root_is_rejected() {
        let claimant = Controller::incept_single().unwrap();
        let snapshot = snapshot_with(&claimant, 1_000);
        let mut registry = InMemoryClaimedRegistry::new();

        let request1 = request_for(&claimant, b"payee-1", 0);
        let sigs1 = claimant.sign_message(&message_to_sign(&request1));
        verify_and_resolve_claim(
            &snapshot,
            &request1,
            &sigs1,
            &claimant.kel(),
            &mut registry,
            100,
        )
        .unwrap();

        let request2 = request_for(&claimant, b"payee-2", 1);
        let sigs2 = claimant.sign_message(&message_to_sign(&request2));
        assert_eq!(
            verify_and_resolve_claim(
                &snapshot,
                &request2,
                &sigs2,
                &claimant.kel(),
                &mut registry,
                200
            )
            .unwrap_err(),
            AirdropError::AlreadyClaimed
        );
    }

    #[test]
    fn a_claim_against_the_wrong_campaign_is_rejected() {
        let claimant = Controller::incept_single().unwrap();
        let snapshot = snapshot_with(&claimant, 1_000);
        let mut request = request_for(&claimant, b"payee", 0);
        request.campaign_id = b"different-campaign".to_vec();
        let sigs = claimant.sign_message(&message_to_sign(&request));
        let mut registry = InMemoryClaimedRegistry::new();

        assert_eq!(
            verify_and_resolve_claim(
                &snapshot,
                &request,
                &sigs,
                &claimant.kel(),
                &mut registry,
                0
            )
            .unwrap_err(),
            AirdropError::CampaignMismatch
        );
    }

    #[test]
    fn an_identity_root_with_no_snapshot_entry_is_not_eligible() {
        let claimant = Controller::incept_single().unwrap();
        let someone_else = Controller::incept_single().unwrap();
        let snapshot = snapshot_with(&someone_else, 1_000); // claimant is NOT in this snapshot
        let request = request_for(&claimant, b"payee", 0);
        let sigs = claimant.sign_message(&message_to_sign(&request));
        let mut registry = InMemoryClaimedRegistry::new();

        assert_eq!(
            verify_and_resolve_claim(
                &snapshot,
                &request,
                &sigs,
                &claimant.kel(),
                &mut registry,
                0
            )
            .unwrap_err(),
            AirdropError::NotEligible
        );
        // A failed verification must never mark anything claimed.
        assert!(!registry.already_claimed(&claimant.did()));
    }

    #[test]
    fn presenting_a_different_identity_roots_kel_is_rejected() {
        let claimant = Controller::incept_single().unwrap();
        let attacker = Controller::incept_single().unwrap();
        let snapshot = snapshot_with(&claimant, 1_000);
        // The request claims to be `claimant`, but the KEL presented (and
        // therefore the keys that actually signed) belongs to `attacker`.
        let request = request_for(&claimant, b"payee", 0);
        let sigs = attacker.sign_message(&message_to_sign(&request));
        let mut registry = InMemoryClaimedRegistry::new();

        assert_eq!(
            verify_and_resolve_claim(
                &snapshot,
                &request,
                &sigs,
                &attacker.kel(),
                &mut registry,
                0
            )
            .unwrap_err(),
            AirdropError::IdentityMismatch
        );
    }

    #[test]
    fn a_signature_from_an_unrelated_key_is_rejected() {
        let claimant = Controller::incept_single().unwrap();
        let snapshot = snapshot_with(&claimant, 1_000);
        let request = request_for(&claimant, b"payee", 0);
        // Sign with a key that is not part of claimant's KEL at all.
        let forged_key = SigningKey::from_seed(&[0x77; 32]);
        let sigs = vec![did_mini::IndexedSig {
            index: 0,
            signature: forged_key.sign(&message_to_sign(&request)),
        }];
        let mut registry = InMemoryClaimedRegistry::new();

        assert_eq!(
            verify_and_resolve_claim(
                &snapshot,
                &request,
                &sigs,
                &claimant.kel(),
                &mut registry,
                0
            )
            .unwrap_err(),
            AirdropError::SignatureThresholdNotMet
        );
    }

    #[test]
    fn tampering_with_the_recipient_after_signing_invalidates_the_signature() {
        let claimant = Controller::incept_single().unwrap();
        let snapshot = snapshot_with(&claimant, 1_000);
        let request = request_for(&claimant, b"honest-payee", 0);
        let sigs = claimant.sign_message(&message_to_sign(&request));

        let mut tampered = request.clone();
        tampered.recipient = b"attacker-payee".to_vec();
        let mut registry = InMemoryClaimedRegistry::new();

        assert_eq!(
            verify_and_resolve_claim(
                &snapshot,
                &tampered,
                &sigs,
                &claimant.kel(),
                &mut registry,
                0
            )
            .unwrap_err(),
            AirdropError::SignatureThresholdNotMet
        );
    }

    #[test]
    fn an_oversized_recipient_is_rejected_before_any_crypto_runs() {
        let claimant = Controller::incept_single().unwrap();
        let snapshot = snapshot_with(&claimant, 1_000);
        let request = request_for(&claimant, &vec![0u8; MAX_RECIPIENT_BYTES + 1], 0);
        let mut registry = InMemoryClaimedRegistry::new();

        assert_eq!(
            verify_and_resolve_claim(&snapshot, &request, &[], &claimant.kel(), &mut registry, 0)
                .unwrap_err(),
            AirdropError::RecipientTooLong
        );
    }
}
