//! A signed, typed statement binding one identity root to one
//! [`mini_spacetime::StorageCommitment`] over a `replica_id` this module
//! itself derives from the signer's own DID -- see the module-level doc
//! for why binding `replica_id` to identity (rather than letting a caller
//! choose it freely) is what makes [`crate::CollisionEvidence`] meaningful.

use did_mini::{Controller, Did, IndexedSig, Kel};
use mini_crypto::{HashAlgorithm, Signature, SignatureSuite};
use mini_spacetime::StorageCommitment;

use crate::codec::{Reader, Writer};
use crate::error::{FraudError, Result};

pub const STORAGE_COMMITMENT_CLAIM_VERSION: u8 = 1;

const REPLICA_ID_DOMAIN: &[u8] = b"mininet/mini-storage-fraud/replica-id/v1";
const CLAIM_SIGNING_DOMAIN: &[u8] = b"mininet/mini-storage-fraud/commitment-claim/v1";

const MAX_DID_BYTES: usize = 256;
const MAX_SIGNATURE_BYTES: usize = 8 * 1024;

/// Derive the `replica_id` a provider's [`mini_porep::seal`] call must use
/// for a given `context` (e.g. a content-addressed segment id it is
/// storing a replica of). Deterministic and identity-bound: two distinct
/// providers deriving a replica id for the same `context` always get two
/// different 32-byte values, so an honest independent sealing of the same
/// source data under each provider's own derived id can never collide at
/// the resulting [`mini_porep::SealedReplica::replica_root`] -- that is
/// exactly the property [`crate::verify_collision`] relies on.
pub fn derive_replica_id(provider: &Did, context: &[u8; 32]) -> [u8; 32] {
    let mut writer = Writer::new();
    writer.raw(REPLICA_ID_DOMAIN);
    writer.bytes(provider.as_str().as_bytes());
    writer.raw(context);
    HashAlgorithm::Blake3.digest(&writer.finish())
}

/// One provider's signed claim: "I, `provider`, hold a sealed replica
/// (sealed under `derive_replica_id(provider, context)`) whose PDP
/// commitment is `commitment`." Raw values are untrusted until
/// [`Self::verify`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageCommitmentClaim {
    pub provider: Did,
    pub context: [u8; 32],
    pub commitment: StorageCommitment,
    pub issued_at_ms: u64,
    pub signature: Vec<IndexedSig>,
}

impl StorageCommitmentClaim {
    fn signing_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.raw(CLAIM_SIGNING_DOMAIN);
        writer.bytes(self.provider.as_str().as_bytes());
        writer.raw(&self.context);
        writer.raw(&self.commitment.merkle_root);
        writer.u64(self.commitment.block_count as u64);
        writer.u64(self.issued_at_ms);
        writer.finish()
    }

    /// Sign a claim for `provider`, over the replica id
    /// [`derive_replica_id`] derives for `(provider.did(), context)`. The
    /// caller is responsible for having actually sealed and committed to
    /// that replica id already -- this function only produces the signed
    /// statement, it does not seal anything itself.
    pub fn issue(
        provider: &Controller,
        context: [u8; 32],
        commitment: StorageCommitment,
        issued_at_ms: u64,
    ) -> Self {
        let mut claim = Self {
            provider: provider.did(),
            context,
            commitment,
            issued_at_ms,
            signature: Vec::new(),
        };
        claim.signature = provider.sign_message(&claim.signing_bytes());
        claim
    }

    /// Verify the claim's signature against `provider_kel`, which must
    /// belong to [`Self::provider`].
    pub fn verify(&self, provider_kel: &Kel) -> Result<()> {
        if provider_kel.did() != self.provider {
            return Err(FraudError::ProviderMismatch);
        }
        if self.signature.is_empty() {
            return Err(FraudError::BadProviderSignature);
        }
        provider_kel
            .verify_message(&self.signing_bytes(), &self.signature)
            .map_err(|_| FraudError::BadProviderSignature)
    }

    /// The replica id [`Self::provider`]'s seal must have used for this
    /// claim's commitment to be honest.
    pub fn expected_replica_id(&self) -> [u8; 32] {
        derive_replica_id(&self.provider, &self.context)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.u8(STORAGE_COMMITMENT_CLAIM_VERSION);
        writer.bytes(self.provider.as_str().as_bytes());
        writer.raw(&self.context);
        writer.raw(&self.commitment.merkle_root);
        writer.u64(self.commitment.block_count as u64);
        writer.u64(self.issued_at_ms);
        encode_signatures(&mut writer, &self.signature);
        writer.finish()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        if reader.u8()? != STORAGE_COMMITMENT_CLAIM_VERSION {
            return Err(FraudError::UnsupportedVersion);
        }
        let provider = parse_did(reader.bytes_limited(MAX_DID_BYTES)?)?;
        let context = reader.raw_array::<32>()?;
        let merkle_root = reader.raw_array::<32>()?;
        let block_count = reader.u64()? as usize;
        let issued_at_ms = reader.u64()?;
        let signature = decode_signatures(&mut reader)?;
        reader.finish()?;
        if signature.is_empty() {
            return Err(FraudError::BadProviderSignature);
        }
        Ok(Self {
            provider,
            context,
            commitment: StorageCommitment {
                merkle_root,
                block_count,
            },
            issued_at_ms,
            signature,
        })
    }
}

const MAX_SIGNATURES: usize = 16;

fn encode_signatures(writer: &mut Writer, signatures: &[IndexedSig]) {
    writer.u32(signatures.len() as u32);
    for signature in signatures {
        writer.u32(signature.index);
        writer.u8(signature.signature.suite().tag());
        writer.bytes(&signature.signature.to_bytes());
    }
}

fn decode_signatures(reader: &mut Reader<'_>) -> Result<Vec<IndexedSig>> {
    let count = reader.u32()? as usize;
    if count > MAX_SIGNATURES {
        return Err(FraudError::LimitExceeded);
    }
    let mut signatures = Vec::with_capacity(count);
    for _ in 0..count {
        let index = reader.u32()?;
        let suite = SignatureSuite::from_tag(reader.u8()?).map_err(|_| FraudError::Truncated)?;
        let bytes = reader.bytes_limited(MAX_SIGNATURE_BYTES)?;
        let signature =
            Signature::from_suite_bytes(suite, &bytes).map_err(|_| FraudError::Truncated)?;
        signatures.push(IndexedSig { index, signature });
    }
    Ok(signatures)
}

fn parse_did(bytes: Vec<u8>) -> Result<Did> {
    let value = String::from_utf8(bytes).map_err(|_| FraudError::InvalidDid)?;
    Did::parse(&value).map_err(|_| FraudError::InvalidDid)
}

#[cfg(test)]
mod tests {
    use did_mini::Controller;

    use super::*;

    fn provider(seed: u8) -> Controller {
        Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
    }

    fn commitment(root: [u8; 32]) -> StorageCommitment {
        StorageCommitment {
            merkle_root: root,
            block_count: 8,
        }
    }

    #[test]
    fn derive_replica_id_differs_across_providers_for_the_same_context() {
        let a = provider(1);
        let b = provider(2);
        let context = [7u8; 32];
        assert_ne!(
            derive_replica_id(&a.did(), &context),
            derive_replica_id(&b.did(), &context)
        );
    }

    #[test]
    fn derive_replica_id_differs_across_contexts_for_the_same_provider() {
        let a = provider(1);
        assert_ne!(
            derive_replica_id(&a.did(), &[1u8; 32]),
            derive_replica_id(&a.did(), &[2u8; 32])
        );
    }

    #[test]
    fn derive_replica_id_is_deterministic() {
        let a = provider(1);
        let context = [9u8; 32];
        assert_eq!(
            derive_replica_id(&a.did(), &context),
            derive_replica_id(&a.did(), &context)
        );
    }

    #[test]
    fn a_genuine_claim_verifies_against_its_own_kel() {
        let p = provider(3);
        let claim = StorageCommitmentClaim::issue(&p, [1u8; 32], commitment([2u8; 32]), 1_000);
        assert!(claim.verify(&p.kel()).is_ok());
    }

    #[test]
    fn a_claim_does_not_verify_against_a_different_providers_kel() {
        let p = provider(4);
        let other = provider(5);
        let claim = StorageCommitmentClaim::issue(&p, [1u8; 32], commitment([2u8; 32]), 1_000);
        assert_eq!(
            claim.verify(&other.kel()),
            Err(FraudError::ProviderMismatch)
        );
    }

    #[test]
    fn a_tampered_commitment_fails_verification() {
        let p = provider(6);
        let mut claim = StorageCommitmentClaim::issue(&p, [1u8; 32], commitment([2u8; 32]), 1_000);
        claim.commitment.merkle_root = [0xFF; 32];
        assert_eq!(
            claim.verify(&p.kel()),
            Err(FraudError::BadProviderSignature)
        );
    }

    #[test]
    fn a_claim_round_trips_through_its_wire_encoding() {
        let p = provider(7);
        let claim = StorageCommitmentClaim::issue(&p, [3u8; 32], commitment([4u8; 32]), 5_000);
        let decoded = StorageCommitmentClaim::from_bytes(&claim.to_bytes()).unwrap();
        assert_eq!(decoded, claim);
        assert!(decoded.verify(&p.kel()).is_ok());
    }
}
