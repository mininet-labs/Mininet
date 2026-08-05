//! A provider's signed, independently-checked claim to hold one sealed
//! replica.
//!
//! # What a verified claim establishes, precisely
//!
//! [`RegisteredReplicaClaim::verify`] returning [`VerifiedReplicaClaim`] means
//! all of the following, and nothing beyond it:
//!
//! 1. A storage device delegated by the named identity root, carrying
//!    [`Capabilities::STORE`], signed these exact bytes under the key state
//!    authoritative at a sequence of its KEL that the claim names and pins by
//!    event digest.
//! 2. The seal commitment inside the claim is structurally well-formed, and its
//!    `replica_id` is exactly the value [`crate::derive_replica_id`] produces
//!    for that root, that device, and that context — so the replica is bound to
//!    the identity claiming it, not to a number the provider picked.
//! 3. A quorum of distinct identity roots, none of them the provider, signed
//!    attestations that they ran `mini_porep`'s registration audit against that
//!    same seal commitment and every sampled challenge verified.
//!
//! # What it does not establish
//!
//! - **Not that the provider still holds the replica.** A claim is a
//!   registration-time object. Continued possession is
//!   `mini_porep::challenge`'s job, and nothing here calls it.
//! - **Not a time.** `issued_at_ms` is self-reported, and so is every
//!   auditor's `observed_at_ms`. The KEL sequence and event digest pin *where
//!   in the signer's own history* the claim sits, which is a partial ordering
//!   against that identity's other acts — not a clock, and not proof that a
//!   holder of a compromised historical key did not produce the claim later.
//!   Anchoring to real time needs witnessed KEL receipts (SPEC-01 §7) or a
//!   chain height and is deliberately not faked here.
//! - **Not audit-grade cryptography.** `mini-porep` describes itself as a
//!   simplified, unaudited SDR prototype. Everything above inherits that, and
//!   the D-0047 external-audit gate applies before any value depends on it.

use did_mini::{Capabilities, Controller, Did, IndexedSig};
use mini_porep::SealCommitment;
use mini_spacetime::StorageCommitment;

use crate::codec::{
    canonicalize_signatures, decode_signatures, encode_signatures, Reader, Writer,
    MAX_EVENT_DIGEST_BYTES,
};
use crate::context::{derive_replica_id, read_did, write_did, ReplicaContextV1};
use crate::error::{DecodeFailure, FraudError, Result};
use crate::registration::{
    resolve_signer, verify_signed_at, RegistrationPolicy, RegistrationReceipt,
    StorageRegistrationOracle,
};
use crate::seal::{
    read_seal_commitment, seal_commitment_digest, storage_commitment_of, validate_seal_commitment,
    write_seal_commitment,
};

/// Domain separator for the bytes a provider signs.
pub const REPLICA_CLAIM_DOMAIN: &[u8] = b"mininet/mini-storage-fraud/replica-claim/v1";

/// Version tag carried by [`RegisteredReplicaClaim`]'s wire encoding.
pub const REPLICA_CLAIM_VERSION: u8 = 1;

/// A provider's claim to hold one sealed replica, with the registration
/// evidence that makes it checkable.
///
/// Fields are private on purpose. An object whose fields can be edited after
/// verification is an object whose verification means nothing a moment later;
/// read it through the accessors, or through [`VerifiedReplicaClaim`], which is
/// the only type in this crate that means "this checked out".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredReplicaClaim {
    provider_root: Did,
    provider_device: Did,
    context: ReplicaContextV1,
    seal: SealCommitment,
    registration: RegistrationReceipt,
    signing_kel_sn: u64,
    signing_kel_digest: Vec<u8>,
    issued_at_ms: u64,
    signature: Vec<IndexedSig>,
}

impl RegisteredReplicaClaim {
    /// Sign a claim.
    ///
    /// Fails rather than signing if the seal commitment is malformed or its
    /// `replica_id` is not the one this provider's identity and context derive
    /// to — an unbindable claim should never come into existence, let alone
    /// carry a signature.
    ///
    /// Pass `provider_root == provider_device.did()` when a root stores
    /// directly with no delegated device.
    pub fn issue(
        provider_root: &Did,
        provider_device: &Controller,
        context: ReplicaContextV1,
        seal: SealCommitment,
        registration: RegistrationReceipt,
        issued_at_ms: u64,
    ) -> Result<Self> {
        validate_seal_commitment(&seal)?;
        let device_did = provider_device.did();
        if seal.replica_id != derive_replica_id(provider_root, &device_did, &context) {
            return Err(FraudError::ReplicaIdNotIdentityBound);
        }

        let device_kel = provider_device.kel();
        let signing_kel_sn = device_kel
            .verify()
            .map_err(|_| FraudError::SigningHistoryMismatch)?
            .sn;
        let signing_kel_digest = device_kel
            .event_digest_at(signing_kel_sn)
            .map_err(|_| FraudError::SigningHistoryMismatch)?;

        let mut claim = Self {
            provider_root: provider_root.clone(),
            provider_device: device_did,
            context,
            seal,
            registration,
            signing_kel_sn,
            signing_kel_digest,
            issued_at_ms,
            signature: Vec::new(),
        };
        claim.signature =
            canonicalize_signatures(provider_device.sign_message(&claim.signing_bytes()));
        if claim.signature.is_empty() {
            return Err(FraudError::BadSignature);
        }
        Ok(claim)
    }

    pub fn provider_root(&self) -> &Did {
        &self.provider_root
    }

    pub fn provider_device(&self) -> &Did {
        &self.provider_device
    }

    pub fn context(&self) -> &ReplicaContextV1 {
        &self.context
    }

    pub fn seal(&self) -> &SealCommitment {
        &self.seal
    }

    pub fn registration(&self) -> &RegistrationReceipt {
        &self.registration
    }

    /// Which point of the signing device's own key history this claim was
    /// signed at. Not a timestamp; see the module docs.
    pub fn signing_kel_sn(&self) -> u64 {
        self.signing_kel_sn
    }

    /// Self-reported issuance time. Carries no authority whatsoever.
    pub fn issued_at_ms(&self) -> u64 {
        self.issued_at_ms
    }

    /// The replica id this claim's identity and context require.
    pub fn required_replica_id(&self) -> [u8; 32] {
        derive_replica_id(&self.provider_root, &self.provider_device, &self.context)
    }

    /// A stable identifier: the digest of the exact bytes the signature covers.
    pub fn claim_id(&self) -> [u8; 32] {
        mini_crypto::HashAlgorithm::Blake3.digest(&self.signing_bytes())
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.raw(REPLICA_CLAIM_DOMAIN);
        write_did(&mut writer, &self.provider_root);
        write_did(&mut writer, &self.provider_device);
        self.context.write_into(&mut writer);
        write_seal_commitment(&mut writer, &self.seal);
        writer.raw(&seal_registration_digest(&self.registration));
        writer.u64(self.signing_kel_sn);
        writer.bytes(&self.signing_kel_digest);
        writer.u64(self.issued_at_ms);
        writer.finish()
    }

    /// Check everything, and return the checked view.
    ///
    /// Consumes the claim so a caller cannot keep the unverified object around
    /// next to the verified one and reach for the wrong one later.
    pub fn verify(
        self,
        oracle: &dyn StorageRegistrationOracle,
        policy: &RegistrationPolicy,
    ) -> Result<VerifiedReplicaClaim> {
        validate_seal_commitment(&self.seal)?;
        if self.seal.replica_id != self.required_replica_id() {
            return Err(FraudError::ReplicaIdNotIdentityBound);
        }

        let device_kel = resolve_signer(
            oracle,
            &self.provider_root,
            &self.provider_device,
            Capabilities::STORE,
        )?;
        verify_signed_at(
            device_kel,
            self.signing_kel_sn,
            &self.signing_kel_digest,
            &self.signing_bytes(),
            &self.signature,
        )?;

        let seal_digest = seal_commitment_digest(&self.seal);
        let auditors =
            self.registration
                .verify(seal_digest, &self.provider_root, oracle, policy)?;

        Ok(VerifiedReplicaClaim {
            claim: self,
            seal_digest,
            distinct_auditor_roots: auditors,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.u8(REPLICA_CLAIM_VERSION);
        self.write_into(&mut writer);
        writer.finish()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        if reader.u8()? != REPLICA_CLAIM_VERSION {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        let claim = Self::read_from(&mut reader)?;
        reader.finish()?;
        Ok(claim)
    }

    pub(crate) fn write_into(&self, writer: &mut Writer) {
        write_did(writer, &self.provider_root);
        write_did(writer, &self.provider_device);
        self.context.write_into(writer);
        write_seal_commitment(writer, &self.seal);
        self.registration.write_into(writer);
        writer.u64(self.signing_kel_sn);
        writer.bytes(&self.signing_kel_digest);
        writer.u64(self.issued_at_ms);
        encode_signatures(writer, &self.signature);
    }

    pub(crate) fn read_from(reader: &mut Reader<'_>) -> Result<Self> {
        let provider_root = read_did(reader)?;
        let provider_device = read_did(reader)?;
        let context = ReplicaContextV1::read_from(reader)?;
        let seal = read_seal_commitment(reader)?;
        let registration = RegistrationReceipt::read_from(reader)?;
        let signing_kel_sn = reader.u64()?;
        let signing_kel_digest = reader.bytes_limited(MAX_EVENT_DIGEST_BYTES)?;
        let issued_at_ms = reader.u64()?;
        let signature = decode_signatures(reader)?;
        if signature.is_empty() {
            return Err(FraudError::BadSignature);
        }
        Ok(Self {
            provider_root,
            provider_device,
            context,
            seal,
            registration,
            signing_kel_sn,
            signing_kel_digest,
            issued_at_ms,
            signature,
        })
    }
}

/// A claim that passed [`RegisteredReplicaClaim::verify`].
///
/// The only way to obtain one is to verify a claim, and its fields are
/// unreachable except through accessors, so "I have a `VerifiedReplicaClaim`"
/// cannot drift from "this claim verified under some policy".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedReplicaClaim {
    claim: RegisteredReplicaClaim,
    seal_digest: [u8; 32],
    distinct_auditor_roots: u32,
}

impl VerifiedReplicaClaim {
    pub fn provider_root(&self) -> &Did {
        &self.claim.provider_root
    }

    pub fn provider_device(&self) -> &Did {
        &self.claim.provider_device
    }

    pub fn context(&self) -> &ReplicaContextV1 {
        &self.claim.context
    }

    pub fn seal(&self) -> &SealCommitment {
        &self.claim.seal
    }

    pub fn seal_digest(&self) -> [u8; 32] {
        self.seal_digest
    }

    /// The sealed replica's Merkle root — the value a duplicate of which is
    /// what [`crate::verify_conflict`] reasons about.
    pub fn replica_root(&self) -> [u8; 32] {
        self.claim.seal.replica_root
    }

    /// The PDP commitment for ongoing possession challenges, *derived* from the
    /// audited seal rather than carried alongside it.
    pub fn storage_commitment(&self) -> StorageCommitment {
        storage_commitment_of(&self.claim.seal)
    }

    /// How many distinct auditor identity roots vouched for this registration.
    pub fn distinct_auditor_roots(&self) -> u32 {
        self.distinct_auditor_roots
    }

    pub fn claim_id(&self) -> [u8; 32] {
        self.claim.claim_id()
    }

    /// Back to the wire object, e.g. to relay it onward.
    pub fn into_claim(self) -> RegisteredReplicaClaim {
        self.claim
    }

    pub fn claim(&self) -> &RegisteredReplicaClaim {
        &self.claim
    }
}

/// A digest over the whole registration receipt, so the provider's signature
/// commits to the exact quorum it presented without re-hashing every
/// attestation body into the claim transcript.
fn seal_registration_digest(registration: &RegistrationReceipt) -> [u8; 32] {
    let mut writer = Writer::new();
    writer.raw(b"mininet/mini-storage-fraud/registration-receipt/v1");
    registration.write_into(&mut writer);
    mini_crypto::HashAlgorithm::Blake3.digest(&writer.finish())
}
