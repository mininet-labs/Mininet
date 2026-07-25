//! Holder-token binding for Tier-0 review presentation.
//!
//! The token is never serialized into a public receipt or review. A verifier
//! receives it over the application's chosen protected channel and checks its
//! commitment against the provider-signed receipt. The commitment is bound to
//! the pairwise subject, provider, and declaration, preventing a commitment
//! copied between grants from validating.

use did_mini::Did;
use mini_crypto::HashAlgorithm;
use mini_objects::ObjectId;

use crate::codec::Writer;
use crate::{AttestError, Result};

const HOLDER_COMMITMENT_DOMAIN: &[u8] = b"mininet/mini-attest/tier0-holder-commitment/v1";

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct HolderCommitment([u8; 32]);

impl HolderCommitment {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl core::fmt::Debug for HolderCommitment {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("HolderCommitment").field(&self.0).finish()
    }
}

pub struct EngagementHolderToken([u8; 32]);

impl EngagementHolderToken {
    pub fn generate() -> Result<Self> {
        Ok(Self(mini_crypto::random_32()?))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn commit(
        &self,
        subject: &Did,
        provider: &Did,
        declaration: &ObjectId,
    ) -> HolderCommitment {
        let mut writer = Writer::new();
        writer.raw(HOLDER_COMMITMENT_DOMAIN);
        writer.bytes(subject.as_str().as_bytes());
        writer.bytes(provider.as_str().as_bytes());
        writer.bytes(declaration.as_str().as_bytes());
        writer.raw(&self.0);
        HolderCommitment(HashAlgorithm::Blake3.digest(&writer.finish()))
    }

    pub(crate) fn verify(
        &self,
        expected: HolderCommitment,
        subject: &Did,
        provider: &Did,
        declaration: &ObjectId,
    ) -> Result<()> {
        if self.commit(subject, provider, declaration) == expected {
            Ok(())
        } else {
            Err(AttestError::HolderCommitmentMismatch)
        }
    }
}

impl core::fmt::Debug for EngagementHolderToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("EngagementHolderToken(REDACTED)")
    }
}

impl Drop for EngagementHolderToken {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.0.zeroize();
    }
}
