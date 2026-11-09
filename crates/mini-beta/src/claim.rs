//! Private, front-run-proof claim path for accepted participation (#337).
//!
//! An accepted contribution receipt carries a [`ClaimTag`]. Here that tag is
//! the public half of a **one-time Ed25519 claim key** the contributor
//! generated for exactly this contribution. The contributor later *redeems* the
//! claim by publishing a [`ClaimRedemption`]: a signature, made with the claim
//! key, over `(contribution, epoch, account)`. Consequences:
//!
//! - no GitHub handle, DID, or persistent pseudonym is bound to the reward: the
//!   redemption object is authored by whatever fresh throwaway identity the
//!   contributor likes, and the destination is an opaque beta account;
//! - a network observer who sees the redemption cannot redirect it, because the
//!   signature binds the destination account (a revealed hash preimage could be
//!   copied by anyone; a signature cannot);
//! - separate contributions use separate claim keys, so they are not linked;
//! - a leaked claim key lets a thief redeem to their own account first, so the
//!   key is bearer material and the CLI says so. Nothing here defends against
//!   that, and it is stated rather than hidden.
//!
//! Only reviewed primitives are used (Ed25519 from `mini-crypto`); there is no
//! new cryptographic construction. Beta test value only.

use did_mini::{Controller, Did};
use mini_crypto::{SignatureSuite, SigningKey, VerifyingKey};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

use super::{
    build_grant_inner, optional_link, parse_campaign_object, parse_contribution_object,
    public_payload, put_u64, required_link, take_32, take_u8, BetaAccountId, BetaEpochId,
    BetaError, ClaimTag, GrantClass, Result, PAYLOAD_VERSION,
};

/// Object type of a claim redemption.
pub const BETA_CLAIM_REDEMPTION_TYPE: &str = "mininet.beta/claim-redemption/v1";

const CLAIM_REDEMPTION_DOMAIN: &[u8] = b"mininet/beta/claim-redemption/v1";
const SIGNATURE_LEN: usize = 64;

/// A one-time claim key. Its public half is the [`ClaimTag`] placed in the
/// contribution receipt; its secret is bearer material that must never be
/// published.
#[derive(Clone)]
pub struct ClaimKey {
    key: SigningKey,
}

impl core::fmt::Debug for ClaimKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ClaimKey").finish_non_exhaustive()
    }
}

impl ClaimKey {
    /// A fresh key from OS entropy.
    pub fn generate() -> Result<Self> {
        let key = SigningKey::generate().map_err(|_| BetaError::InvalidClaimProof)?;
        Ok(Self { key })
    }

    /// Rebuild the key from the secret a contributor saved.
    pub fn from_secret(secret: &[u8; 32]) -> Self {
        Self {
            key: SigningKey::from_seed(secret),
        }
    }

    /// The secret to hand back to the contributor (bearer material).
    pub fn secret(&self) -> [u8; 32] {
        self.key.to_seed_bytes()
    }

    /// The public claim tag this key proves.
    pub fn claim_tag(&self) -> Result<ClaimTag> {
        let bytes: [u8; 32] = self
            .key
            .verifying_key()
            .to_bytes()
            .try_into()
            .map_err(|_| BetaError::InvalidClaimProof)?;
        ClaimTag::new(bytes)
    }
}

/// Check that `tag` is a usable claim public key: a valid Ed25519 point that is
/// not small-order (a weak key would let anyone redeem without a secret).
pub fn validate_claim_tag(tag: &ClaimTag) -> Result<()> {
    let key = VerifyingKey::from_suite_bytes(SignatureSuite::Ed25519, tag.as_bytes())
        .map_err(|_| BetaError::InvalidClaimProof)?;
    if key.is_weak() {
        return Err(BetaError::InvalidClaimProof);
    }
    Ok(())
}

/// A parsed, not yet verified, claim redemption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimRedemption {
    /// Content-addressed redemption id.
    pub id: ObjectId,
    /// Record signer (a throwaway identity is expected; it proves nothing).
    pub record_author: Did,
    /// The accepted contribution being claimed.
    pub contribution_id: ObjectId,
    /// Beta epoch the claim is for.
    pub epoch: BetaEpochId,
    /// Opaque destination account chosen by the claim-key holder.
    pub account: BetaAccountId,
    signature: [u8; SIGNATURE_LEN],
    /// Object timestamp.
    pub timestamp_ms: u64,
    /// Signer-scoped sequence.
    pub sequence: u64,
}

/// The exact bytes the claim key signs. Domain-separated and bound to the
/// contribution, the epoch and the destination account.
fn redemption_message(
    contribution_id: &ObjectId,
    epoch: BetaEpochId,
    account: &BetaAccountId,
) -> Vec<u8> {
    let mut message = Vec::new();
    message.extend_from_slice(CLAIM_REDEMPTION_DOMAIN);
    let id = contribution_id.as_str().as_bytes();
    put_u64(&mut message, id.len() as u64);
    message.extend_from_slice(id);
    message.extend_from_slice(epoch.as_bytes());
    message.extend_from_slice(account.as_bytes());
    message
}

/// Build and sign a redemption WITHOUT storing it (for journaling callers).
/// Fails unless `claim_key` is exactly the key the contribution committed to.
#[allow(clippy::too_many_arguments)]
pub fn build_claim_redemption<B: Backend>(
    store: &Store<B>,
    record_human: &Did,
    record_device: &Controller,
    contribution_id: &ObjectId,
    account: BetaAccountId,
    claim_key: &ClaimKey,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let contribution = parse_contribution_object(&store.get(contribution_id)?)?;
    if claim_key.claim_tag()? != contribution.claim_tag {
        return Err(BetaError::InvalidClaimProof);
    }
    let campaign_id = contribution
        .campaign_id
        .as_ref()
        .ok_or(BetaError::InvalidObject)?;
    let epoch = parse_campaign_object(&store.get(campaign_id)?)?.epoch;
    let signature = claim_key
        .key
        .sign(&redemption_message(contribution_id, epoch, &account));
    let signature_bytes = signature.to_bytes();
    if signature_bytes.len() != SIGNATURE_LEN {
        return Err(BetaError::InvalidClaimProof);
    }

    let mut payload = vec![PAYLOAD_VERSION];
    payload.extend_from_slice(epoch.as_bytes());
    payload.extend_from_slice(account.as_bytes());
    payload.extend_from_slice(&signature_bytes);
    Ok(
        ObjectBuilder::new(ObjectType::Custom(BETA_CLAIM_REDEMPTION_TYPE.to_string()))
            .timestamp_ms(timestamp_ms)
            .sequence(sequence)
            .payload(Payload::Public(payload))
            .link("contribution", contribution_id.clone())
            .sign(record_human, record_device)?,
    )
}

/// Build, sign and store a redemption.
#[allow(clippy::too_many_arguments)]
pub fn create_claim_redemption<B: Backend>(
    store: &mut Store<B>,
    record_human: &Did,
    record_device: &Controller,
    contribution_id: &ObjectId,
    account: BetaAccountId,
    claim_key: &ClaimKey,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let object = build_claim_redemption(
        store,
        record_human,
        record_device,
        contribution_id,
        account,
        claim_key,
        timestamp_ms,
        sequence,
    )?;
    store.insert(&object)?;
    Ok(object)
}

/// Strictly parse a redemption object (shape only; see
/// [`verify_claim_redemption`] for the proof check).
pub fn parse_claim_redemption_object(object: &Object) -> Result<ClaimRedemption> {
    if object.object_type != ObjectType::Custom(BETA_CLAIM_REDEMPTION_TYPE.to_string()) {
        return Err(BetaError::InvalidObject);
    }
    if object.links.len() != 1 {
        return Err(BetaError::InvalidObject);
    }
    let bytes = public_payload(object)?;
    let mut off = 0usize;
    if take_u8(bytes, &mut off)? != PAYLOAD_VERSION {
        return Err(BetaError::InvalidObject);
    }
    let epoch = BetaEpochId::new(take_32(bytes, &mut off)?)?;
    let account = BetaAccountId::new(take_32(bytes, &mut off)?)?;
    let rest = &bytes[off..];
    let signature: [u8; SIGNATURE_LEN] = rest.try_into().map_err(|_| BetaError::InvalidObject)?;
    let contribution_id = required_link(object, "contribution")?;
    if optional_link(object, "campaign")?.is_some() {
        return Err(BetaError::InvalidObject);
    }
    Ok(ClaimRedemption {
        id: object.id().clone(),
        record_author: object.author_human.clone(),
        contribution_id,
        epoch,
        account,
        signature,
        timestamp_ms: object.timestamp_ms,
        sequence: object.sequence,
    })
}

/// Read and parse a stored redemption (shape only).
pub fn read_claim_redemption<B: Backend>(
    store: &Store<B>,
    id: &ObjectId,
) -> Result<ClaimRedemption> {
    parse_claim_redemption_object(&store.get(id)?)
}

/// Load a redemption and check the whole proof: the contribution exists and
/// belongs to a campaign of the same epoch, and the claim key the contribution
/// committed to signed exactly `(contribution, epoch, account)`.
pub fn verify_claim_redemption<B: Backend>(
    store: &Store<B>,
    id: &ObjectId,
) -> Result<ClaimRedemption> {
    let redemption = read_claim_redemption(store, id)?;
    let contribution = parse_contribution_object(&store.get(&redemption.contribution_id)?)?;
    let campaign_id = contribution
        .campaign_id
        .as_ref()
        .ok_or(BetaError::InvalidObject)?;
    let campaign = parse_campaign_object(&store.get(campaign_id)?)?;
    if campaign.epoch != redemption.epoch {
        return Err(BetaError::WrongEpoch);
    }
    let verifying =
        VerifyingKey::from_suite_bytes(SignatureSuite::Ed25519, contribution.claim_tag.as_bytes())
            .map_err(|_| BetaError::InvalidClaimProof)?;
    let signature =
        mini_crypto::Signature::from_suite_bytes(SignatureSuite::Ed25519, &redemption.signature)
            .map_err(|_| BetaError::InvalidClaimProof)?;
    // Strict: a small-order claim key must not let anyone redeem without a secret.
    verifying
        .verify_strict(
            &redemption_message(
                &redemption.contribution_id,
                redemption.epoch,
                &redemption.account,
            ),
            &signature,
        )
        .map_err(|_| BetaError::InvalidClaimProof)?;
    Ok(redemption)
}

/// Build a participation grant for a verified redemption: the destination
/// account, contribution, campaign and epoch all come from the redemption and
/// its contribution, never from the caller.
#[allow(clippy::too_many_arguments)]
pub fn build_redeemed_participation_grant<B: Backend>(
    store: &Store<B>,
    record_human: &Did,
    record_device: &Controller,
    redemption_id: &ObjectId,
    amount: u64,
    memo: &str,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let redemption = verify_claim_redemption(store, redemption_id)?;
    let contribution = parse_contribution_object(&store.get(&redemption.contribution_id)?)?;
    let campaign_id = contribution.campaign_id.ok_or(BetaError::InvalidObject)?;
    build_grant_inner(
        store,
        record_human,
        record_device,
        &campaign_id,
        Some(&redemption.contribution_id),
        Some(redemption_id),
        redemption.epoch,
        redemption.account,
        GrantClass::Participation,
        amount,
        memo,
        timestamp_ms,
        sequence,
    )
}
