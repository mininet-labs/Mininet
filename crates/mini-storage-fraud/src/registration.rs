//! Evidence that somebody other than the provider actually checked the seal.
//!
//! # The gap this closes
//!
//! A provider can publish any 32 bytes and call them a replica root. Signing
//! them proves only that the provider owns a key. What makes a replica claim
//! mean anything is that independent parties ran `mini_porep`'s registration
//! audit against the *full* seal commitment — sampling random `(layer, node)`
//! challenges, recomputing the labeling hash themselves, and checking every
//! answer against Merkle roots the provider published before the challenges
//! were drawn — and were satisfied.
//!
//! [`AuditAttestation`] is one auditor's signed record of having done that.
//! [`RegistrationReceipt`] is a quorum of them over the same seal. Together
//! they are the difference between "this provider asserts a root" and "this
//! root was independently checked".
//!
//! # What a quorum does not establish
//!
//! - **Distinct identity roots are not distinct humans.** Sybil resistance is
//!   this protocol's sharpest open problem (roadmap #18); until it is solved, a
//!   quorum of `n` roots may be one operator with `n` identities. The quorum is
//!   a real cost to forge and a real improvement on self-assertion. It is not a
//!   trust anchor.
//! - **Attestation timestamps are self-reported.** `observed_at_ms` is what the
//!   auditor says, and a colluding or compromised auditor can say anything.
//!   Nothing here anchors a claim to real time; that needs witnessed KEL
//!   receipts (SPEC-01 §7) or a chain height, and is named as required
//!   follow-up rather than faked with a field.
//! - **The audit is probabilistic**, exactly as `mini_porep::audit` documents:
//!   enough sampled challenges make skipping a meaningful fraction of the
//!   sealing work overwhelmingly likely to be caught, but it is not a succinct
//!   proof, and `mini-porep` itself is unaudited prototype cryptography.

use did_mini::{Capabilities, Controller, Did, IndexedSig, Kel};
use mini_crypto::HashAlgorithm;
use mini_porep::{
    sample_challenges, verify_audit_response, AuditChallenge, AuditResponse, SealCommitment,
};

use crate::codec::{
    canonicalize_signatures, decode_signatures, encode_signatures, Reader, Writer,
    MAX_EVENT_DIGEST_BYTES,
};
use crate::context::{read_did, write_did};
use crate::error::{DecodeFailure, FraudError, Result};
use crate::seal::seal_commitment_digest;

/// Domain separator for the bytes an auditor signs.
pub const AUDIT_ATTESTATION_DOMAIN: &[u8] = b"mininet/mini-storage-fraud/audit-attestation/v1";

/// Version tag carried by [`AuditAttestation`]'s wire encoding.
pub const AUDIT_ATTESTATION_VERSION: u8 = 1;

/// Version tag carried by [`RegistrationReceipt`]'s wire encoding.
pub const REGISTRATION_RECEIPT_VERSION: u8 = 1;

/// Largest quorum this profile admits in one receipt.
pub const MAX_ATTESTATIONS: usize = 64;

/// Largest number of audit challenges one attestation may claim to have run.
/// Bounds the work a verifier is asked to believe in, and the work
/// [`audit_and_attest`] will do.
pub const MAX_AUDIT_CHALLENGES: u32 = 4096;

/// Resolves the KELs this crate needs to check a signer, mirroring
/// `mini_chain::ValidatorOracle`.
///
/// Freshness is the oracle's problem and it is a real one: a revoked storage
/// device still looks delegated in a stale copy of its root's KEL. Callers
/// should pin the highest sequence they have ever seen per SCID
/// (`did_mini::FreshnessPins`) and refuse to go backwards.
pub trait StorageRegistrationOracle {
    /// The KEL this oracle vouches for, if it has one.
    fn kel(&self, did: &Did) -> Option<&Kel>;
}

/// How much independent checking a verifier insists on before treating a
/// registration as accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistrationPolicy {
    min_distinct_auditors: u32,
    min_challenges_per_audit: u32,
}

impl RegistrationPolicy {
    /// A policy requiring at least `min_distinct_auditors` distinct auditor
    /// identity roots, each having sampled at least `min_challenges_per_audit`
    /// challenges. Both must be non-zero: a policy that accepts zero auditors
    /// or zero challenges accepts self-assertion, which is what this whole
    /// module exists to stop.
    pub fn new(min_distinct_auditors: u32, min_challenges_per_audit: u32) -> Result<Self> {
        if min_distinct_auditors == 0 || min_challenges_per_audit == 0 {
            return Err(FraudError::InvalidPolicy);
        }
        if min_challenges_per_audit > MAX_AUDIT_CHALLENGES {
            return Err(FraudError::InvalidPolicy);
        }
        Ok(Self {
            min_distinct_auditors,
            min_challenges_per_audit,
        })
    }

    /// Two distinct auditor roots, 64 sampled challenges each.
    ///
    /// Two is the same floor `mini-forge` applies to code review (D-0033): the
    /// smallest quorum where no single party is deciding alone. It is a floor,
    /// not a recommendation — a deployment putting real value behind storage
    /// registration should set it far higher, and must not treat this default
    /// as a reviewed parameter choice.
    pub fn baseline() -> Self {
        Self {
            min_distinct_auditors: 2,
            min_challenges_per_audit: 64,
        }
    }

    pub fn min_distinct_auditors(&self) -> u32 {
        self.min_distinct_auditors
    }

    pub fn min_challenges_per_audit(&self) -> u32 {
        self.min_challenges_per_audit
    }
}

/// One auditor's signed statement: "I drew `challenge_count` challenges from
/// `challenge_seed` against the seal commitment digesting to `seal_digest`, and
/// every answer verified."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditAttestation {
    auditor_root: Did,
    auditor_device: Did,
    seal_digest: [u8; 32],
    challenge_seed: [u8; 32],
    challenge_count: u32,
    signing_kel_sn: u64,
    signing_kel_digest: Vec<u8>,
    observed_at_ms: u64,
    signature: Vec<IndexedSig>,
}

impl AuditAttestation {
    pub fn auditor_root(&self) -> &Did {
        &self.auditor_root
    }

    pub fn auditor_device(&self) -> &Did {
        &self.auditor_device
    }

    pub fn seal_digest(&self) -> [u8; 32] {
        self.seal_digest
    }

    pub fn challenge_seed(&self) -> [u8; 32] {
        self.challenge_seed
    }

    pub fn challenge_count(&self) -> u32 {
        self.challenge_count
    }

    /// What the auditor *says* the time was. Self-reported; see the module
    /// docs.
    pub fn observed_at_ms(&self) -> u64 {
        self.observed_at_ms
    }

    /// A stable identifier for this attestation: the digest of the exact bytes
    /// its signature covers. Used as the canonical sort key inside a receipt.
    pub fn attestation_id(&self) -> [u8; 32] {
        HashAlgorithm::Blake3.digest(&self.signing_bytes())
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.raw(AUDIT_ATTESTATION_DOMAIN);
        write_did(&mut writer, &self.auditor_root);
        write_did(&mut writer, &self.auditor_device);
        writer.raw(&self.seal_digest);
        writer.raw(&self.challenge_seed);
        writer.u32(self.challenge_count);
        writer.u64(self.signing_kel_sn);
        writer.bytes(&self.signing_kel_digest);
        writer.u64(self.observed_at_ms);
        writer.finish()
    }

    /// Verify this attestation against the auditor's identity.
    ///
    /// Checks the device is delegated by the claimed root with
    /// [`Capabilities::ATTEST`], that the cited KEL sequence and event digest
    /// really are that device's history, and that the signature verifies
    /// against the key state authoritative *at that sequence* — so an
    /// attestation survives the auditor's ordinary key rotation.
    pub fn verify(&self, oracle: &dyn StorageRegistrationOracle) -> Result<()> {
        let device_kel = resolve_signer(
            oracle,
            &self.auditor_root,
            &self.auditor_device,
            Capabilities::ATTEST,
        )?;
        verify_signed_at(
            device_kel,
            self.signing_kel_sn,
            &self.signing_kel_digest,
            &self.signing_bytes(),
            &self.signature,
        )
    }

    /// Sign an attestation statement.
    ///
    /// This is the signing primitive; [`audit_and_attest`] is the function
    /// honest auditor software should call, because it runs the audit first and
    /// refuses to sign if any sampled challenge fails.
    ///
    /// **An auditor that calls this without having run the audit is lying, and
    /// no verifier can tell.** That is not a defect in this API — it is the
    /// actual threat model. A signature can only ever prove that a key was
    /// used, never that the signer looked before signing. It is why quorum size
    /// is the parameter that matters, why auditors must be distinct roots that
    /// are not the provider, why challenge seeds must differ across the quorum,
    /// and why [`crate::verify_conflict`] exists at all. Exposing the primitive
    /// lets that residual attack be written down and tested rather than assumed
    /// away.
    pub fn issue(
        auditor_root: &Did,
        auditor_device: &Controller,
        seal: &SealCommitment,
        challenge_seed: [u8; 32],
        challenge_count: u32,
        observed_at_ms: u64,
    ) -> Result<Self> {
        if challenge_count == 0 || challenge_count > MAX_AUDIT_CHALLENGES {
            return Err(FraudError::InsufficientAuditSampling {
                needed: 1,
                got: challenge_count,
            });
        }
        let device_kel = auditor_device.kel();
        let signing_kel_sn = device_kel
            .verify()
            .map_err(|_| FraudError::SigningHistoryMismatch)?
            .sn;
        let signing_kel_digest = device_kel
            .event_digest_at(signing_kel_sn)
            .map_err(|_| FraudError::SigningHistoryMismatch)?;

        let mut attestation = Self {
            auditor_root: auditor_root.clone(),
            auditor_device: auditor_device.did(),
            seal_digest: seal_commitment_digest(seal),
            challenge_seed,
            challenge_count,
            signing_kel_sn,
            signing_kel_digest,
            observed_at_ms,
            signature: Vec::new(),
        };
        attestation.signature =
            canonicalize_signatures(auditor_device.sign_message(&attestation.signing_bytes()));
        if attestation.signature.is_empty() {
            return Err(FraudError::BadSignature);
        }
        Ok(attestation)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.u8(AUDIT_ATTESTATION_VERSION);
        self.write_into(&mut writer);
        writer.finish()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        if reader.u8()? != AUDIT_ATTESTATION_VERSION {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        let attestation = Self::read_from(&mut reader)?;
        reader.finish()?;
        Ok(attestation)
    }

    fn write_into(&self, writer: &mut Writer) {
        write_did(writer, &self.auditor_root);
        write_did(writer, &self.auditor_device);
        writer.raw(&self.seal_digest);
        writer.raw(&self.challenge_seed);
        writer.u32(self.challenge_count);
        writer.u64(self.signing_kel_sn);
        writer.bytes(&self.signing_kel_digest);
        writer.u64(self.observed_at_ms);
        encode_signatures(writer, &self.signature);
    }

    fn read_from(reader: &mut Reader<'_>) -> Result<Self> {
        let auditor_root = read_did(reader)?;
        let auditor_device = read_did(reader)?;
        let seal_digest = reader.raw_array::<32>()?;
        let challenge_seed = reader.raw_array::<32>()?;
        let challenge_count = reader.u32()?;
        let signing_kel_sn = reader.u64()?;
        let signing_kel_digest = reader.bytes_limited(MAX_EVENT_DIGEST_BYTES)?;
        let observed_at_ms = reader.u64()?;
        let signature = decode_signatures(reader)?;
        if signature.is_empty() {
            return Err(FraudError::BadSignature);
        }
        if challenge_count == 0 || challenge_count > MAX_AUDIT_CHALLENGES {
            return Err(DecodeFailure::LimitExceeded.into());
        }
        Ok(Self {
            auditor_root,
            auditor_device,
            seal_digest,
            challenge_seed,
            challenge_count,
            signing_kel_sn,
            signing_kel_digest,
            observed_at_ms,
            signature,
        })
    }
}

/// Run `mini_porep`'s registration audit against a seal commitment and, only if
/// every sampled challenge verifies, sign an attestation to that effect.
///
/// `answer` is how this auditor asks the provider for a response — a network
/// round trip, a local call in tests, whatever the deployment uses. Returning
/// `None` means the provider did not answer, which fails the audit: an
/// unanswered challenge is a failed challenge, never a skipped one.
///
/// `auditor_device` signs. Pass the same `Controller` as both root and device
/// (via `auditor_root == auditor_device.did()`) when a root audits directly.
///
/// The seed must be chosen by the *auditor*, after the provider has published
/// its commitment. A provider that gets to pick the seed can pre-seal only the
/// nodes it knows will be challenged.
#[allow(clippy::too_many_arguments)]
pub fn audit_and_attest(
    auditor_root: &Did,
    auditor_device: &Controller,
    seal: &SealCommitment,
    challenge_seed: [u8; 32],
    challenge_count: u32,
    observed_at_ms: u64,
    mut answer: impl FnMut(&AuditChallenge) -> Option<AuditResponse>,
) -> Result<AuditAttestation> {
    crate::seal::validate_seal_commitment(seal)?;
    if challenge_count == 0 || challenge_count > MAX_AUDIT_CHALLENGES {
        return Err(FraudError::InsufficientAuditSampling {
            needed: 1,
            got: challenge_count,
        });
    }

    let challenges = sample_challenges(seal, &challenge_seed, challenge_count as usize);
    for challenge in &challenges {
        let response = answer(challenge).ok_or(FraudError::AuditUnanswered)?;
        if !verify_audit_response(seal, challenge, &response) {
            return Err(FraudError::AuditFailed);
        }
    }

    AuditAttestation::issue(
        auditor_root,
        auditor_device,
        seal,
        challenge_seed,
        challenge_count,
        observed_at_ms,
    )
}

/// A quorum of [`AuditAttestation`]s over one seal commitment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationReceipt {
    attestations: Vec<AuditAttestation>,
}

impl RegistrationReceipt {
    /// Collect attestations into a receipt in canonical order (ascending
    /// [`AuditAttestation::attestation_id`], duplicates dropped).
    pub fn new(attestations: Vec<AuditAttestation>) -> Result<Self> {
        let mut attestations = attestations;
        attestations.sort_by_key(|attestation| attestation.attestation_id());
        attestations.dedup_by_key(|attestation| attestation.attestation_id());
        if attestations.is_empty() || attestations.len() > MAX_ATTESTATIONS {
            return Err(FraudError::InsufficientAuditQuorum {
                needed: 1,
                got: attestations.len() as u32,
            });
        }
        Ok(Self { attestations })
    }

    pub fn attestations(&self) -> &[AuditAttestation] {
        &self.attestations
    }

    /// How many *distinct auditor identity roots* this receipt carries. Two
    /// attestations from two devices of one root count once — the same
    /// one-root-one-voice rule the rest of the protocol counts by.
    pub fn distinct_auditor_roots(&self) -> u32 {
        let mut roots: Vec<&str> = self
            .attestations
            .iter()
            .map(|attestation| attestation.auditor_root.scid())
            .collect();
        roots.sort_unstable();
        roots.dedup();
        roots.len() as u32
    }

    /// Verify the whole quorum against `policy`, for a specific seal and
    /// provider.
    ///
    /// Returns the number of distinct auditor roots that checked out.
    pub fn verify(
        &self,
        seal_digest: [u8; 32],
        provider_root: &Did,
        oracle: &dyn StorageRegistrationOracle,
        policy: &RegistrationPolicy,
    ) -> Result<u32> {
        let mut seeds: Vec<[u8; 32]> = Vec::with_capacity(self.attestations.len());
        for attestation in &self.attestations {
            if attestation.seal_digest != seal_digest {
                return Err(FraudError::AttestationTargetMismatch);
            }
            if attestation.auditor_root.scid() == provider_root.scid() {
                return Err(FraudError::SelfAttestation);
            }
            if attestation.challenge_count < policy.min_challenges_per_audit {
                return Err(FraudError::InsufficientAuditSampling {
                    needed: policy.min_challenges_per_audit,
                    got: attestation.challenge_count,
                });
            }
            if seeds.contains(&attestation.challenge_seed) {
                return Err(FraudError::RepeatedChallengeSeed);
            }
            seeds.push(attestation.challenge_seed);
            attestation.verify(oracle)?;
        }

        let distinct = self.distinct_auditor_roots();
        if distinct < policy.min_distinct_auditors {
            return Err(FraudError::InsufficientAuditQuorum {
                needed: policy.min_distinct_auditors,
                got: distinct,
            });
        }
        Ok(distinct)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.u8(REGISTRATION_RECEIPT_VERSION);
        self.write_into(&mut writer);
        writer.finish()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        if reader.u8()? != REGISTRATION_RECEIPT_VERSION {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        let receipt = Self::read_from(&mut reader)?;
        reader.finish()?;
        Ok(receipt)
    }

    pub(crate) fn write_into(&self, writer: &mut Writer) {
        writer.count(self.attestations.len());
        for attestation in &self.attestations {
            attestation.write_into(writer);
        }
    }

    pub(crate) fn read_from(reader: &mut Reader<'_>) -> Result<Self> {
        let count = reader.count(MAX_ATTESTATIONS)?;
        if count == 0 {
            return Err(FraudError::InsufficientAuditQuorum { needed: 1, got: 0 });
        }
        let mut attestations: Vec<AuditAttestation> = Vec::with_capacity(count);
        for _ in 0..count {
            let attestation = AuditAttestation::read_from(reader)?;
            if let Some(previous) = attestations.last() {
                if previous.attestation_id() >= attestation.attestation_id() {
                    return Err(DecodeFailure::NoncanonicalAttestationOrder.into());
                }
            }
            attestations.push(attestation);
        }
        Ok(Self { attestations })
    }
}

/// Resolve a `(root, device)` signer pair and confirm the device may act.
///
/// A root signing for itself (`root == device`) is allowed and needs no
/// capability — a root already holds every authority a delegation could grant.
/// It must still be a genuine non-delegated root, so a device cannot be passed
/// twice to launder itself into root standing.
pub(crate) fn resolve_signer<'a>(
    oracle: &'a dyn StorageRegistrationOracle,
    root: &Did,
    device: &Did,
    required: Capabilities,
) -> Result<&'a Kel> {
    let root_kel = oracle.kel(root).ok_or(FraudError::UnknownIdentity)?;
    if root_kel.did().as_str() != root.as_str() {
        return Err(FraudError::UnknownIdentity);
    }

    if root.scid() == device.scid() {
        root_kel.verify().map_err(|_| FraudError::UnknownIdentity)?;
        if root_kel.delegator().is_some() {
            return Err(FraudError::DelegationRejected);
        }
        return Ok(root_kel);
    }

    let device_kel = oracle.kel(device).ok_or(FraudError::UnknownIdentity)?;
    if device_kel.did().as_str() != device.as_str() {
        return Err(FraudError::UnknownIdentity);
    }
    let capabilities = did_mini::verify_delegation(root_kel, device_kel)
        .map_err(|_| FraudError::DelegationRejected)?;
    if !capabilities.contains(required) {
        return Err(FraudError::MissingCapability);
    }
    Ok(device_kel)
}

/// Verify detached signatures against the signer's key state *at the sequence
/// the object claims to have been signed under*, after confirming that
/// sequence's event digest is the one the object cites.
///
/// The digest check is what stops a claimed sequence from being a free
/// parameter: without it, a signer could name any sequence whose key state
/// happened to suit it.
pub(crate) fn verify_signed_at(
    device_kel: &Kel,
    signing_kel_sn: u64,
    signing_kel_digest: &[u8],
    message: &[u8],
    signature: &[IndexedSig],
) -> Result<()> {
    if signature.is_empty() {
        return Err(FraudError::BadSignature);
    }
    let digest = device_kel
        .event_digest_at(signing_kel_sn)
        .map_err(|_| FraudError::SigningHistoryMismatch)?;
    if digest != signing_kel_digest {
        return Err(FraudError::SigningHistoryMismatch);
    }
    device_kel
        .verify_message_at(signing_kel_sn, message, signature)
        .map_err(|_| FraudError::BadSignature)
}
