//! Capability-derived rotating lookup labels (research report §"recommended
//! architecture", the `Capability -> derives a rotating, unlinkable lookup
//! label` step). A [`LookupLabel`] is what actually goes out over the
//! wire to a private index service — never the plaintext object ID,
//! mailbox address, or capability itself.
//!
//! No new cryptography: [`derive_lookup_label`] is HKDF-SHA256 via
//! `mini_crypto::KdfSuite`, the same already-reviewed primitive
//! `mini-bearer`'s channel and `mini-treasury`'s key derivation use.

use zeroize::Zeroize;

use crate::error::{IndexError, Result};

const DOMAIN: &[u8] = b"mininet/mini-private-index/lookup-label/v1";
const LABEL_LEN: usize = 32;

/// A device-local secret from which every [`LookupLabel`] for a given
/// capability is derived. Zeroized on drop; never transmitted.
pub struct CapabilitySecret([u8; 32]);

impl CapabilitySecret {
    /// A fresh random secret from OS entropy.
    pub fn generate() -> Result<Self> {
        Ok(CapabilitySecret(
            mini_crypto::random_32().map_err(IndexError::Crypto)?,
        ))
    }

    /// Rebuild from bytes already generated (device-local storage only).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        CapabilitySecret(bytes)
    }
}

impl core::fmt::Debug for CapabilitySecret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CapabilitySecret(REDACTED)")
    }
}

impl Drop for CapabilitySecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// A rotation period. The research report calls for "short epochs" —
/// this crate leaves the epoch's real-world duration to the caller's
/// policy, and only wraps the counter itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IndexEpoch(pub u64);

/// A disjoint HKDF derivation domain for one kind of private lookup.
/// `#[non_exhaustive]`: new purposes are added by extending this enum,
/// never by reusing an existing tag for a different meaning (that would
/// let one purpose's labels collide with another's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LookupPurpose {
    /// Resolving which service currently stores a given provider's
    /// opaque object.
    ProviderLookup,
    /// Resolving a `mini-relay` mailbox's current pickup location.
    MailboxLookup,
    /// Resolving a social feed's current head pointer.
    FeedHeadLookup,
    /// Resolving a subscription's current delivery target.
    SubscriptionLookup,
    /// Resolving which nodes hold a given erasure-coded shard.
    ShardLookup,
    /// Resolving a `mini-bridge` descriptor's current distribution point.
    BridgeLookup,
    /// Deriving the *next* epoch's label, without revealing this epoch's
    /// label, to support look-ahead prefetch without correlation.
    NextLabelRotation,
    /// Domain for encrypting a private index record's opaque payload
    /// (reserved for a future symmetric-encryption wiring; this crate
    /// stores the encrypted bytes opaquely and never decrypts them).
    RecordEncryption,
    /// Domain for authenticating a private index record's opaque payload
    /// (reserved, see [`LookupPurpose::RecordEncryption`]).
    RecordAuthentication,
}

impl LookupPurpose {
    const fn tag(self) -> u8 {
        match self {
            LookupPurpose::ProviderLookup => 1,
            LookupPurpose::MailboxLookup => 2,
            LookupPurpose::FeedHeadLookup => 3,
            LookupPurpose::SubscriptionLookup => 4,
            LookupPurpose::ShardLookup => 5,
            LookupPurpose::BridgeLookup => 6,
            LookupPurpose::NextLabelRotation => 7,
            LookupPurpose::RecordEncryption => 8,
            LookupPurpose::RecordAuthentication => 9,
        }
    }
}

/// An opaque, derived lookup label — safe to send to an untrusted index
/// service, since it reveals nothing about the capability, scope, or
/// purpose it was derived from without also knowing the
/// [`CapabilitySecret`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LookupLabel([u8; LABEL_LEN]);

impl LookupLabel {
    pub fn to_bytes(self) -> [u8; LABEL_LEN] {
        self.0
    }

    pub fn from_bytes(bytes: [u8; LABEL_LEN]) -> Self {
        LookupLabel(bytes)
    }
}

/// Derive a [`LookupLabel`] for one `(scope, replica_id, epoch, purpose)`
/// combination. `scope` is caller-defined context (e.g. an object ID's
/// hash, a mailbox ID's bytes) that only makes sense alongside the
/// `secret` that produced it; `replica_id` lets the same logical lookup
/// target distinct, non-colluding index replicas with unlinkable labels
/// (research report §"replication/non-collusion").
pub fn derive_lookup_label(
    secret: &CapabilitySecret,
    scope: &[u8],
    replica_id: &[u8],
    epoch: IndexEpoch,
    purpose: LookupPurpose,
) -> Result<LookupLabel> {
    let mut info = Vec::new();
    info.extend_from_slice(DOMAIN);
    info.push(purpose.tag());
    info.extend_from_slice(&epoch.0.to_be_bytes());
    info.extend_from_slice(&(scope.len() as u32).to_be_bytes());
    info.extend_from_slice(scope);
    info.extend_from_slice(&(replica_id.len() as u32).to_be_bytes());
    info.extend_from_slice(replica_id);

    let derived = mini_crypto::KdfSuite::HkdfSha256
        .derive_bytes(None, &secret.0, &info, LABEL_LEN)
        .map_err(IndexError::Crypto)?;
    let bytes: [u8; LABEL_LEN] = derived
        .try_into()
        .expect("derive_bytes(.., LABEL_LEN) always returns LABEL_LEN bytes");
    Ok(LookupLabel(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_inputs_always_derive_the_same_label() {
        let secret = CapabilitySecret::from_bytes([7u8; 32]);
        let a = derive_lookup_label(
            &secret,
            b"scope",
            b"replica-a",
            IndexEpoch(1),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        let b = derive_lookup_label(
            &secret,
            b"scope",
            b"replica-a",
            IndexEpoch(1),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn different_secrets_derive_different_labels() {
        let secret_a = CapabilitySecret::from_bytes([1u8; 32]);
        let secret_b = CapabilitySecret::from_bytes([2u8; 32]);
        let a = derive_lookup_label(
            &secret_a,
            b"scope",
            b"replica",
            IndexEpoch(1),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        let b = derive_lookup_label(
            &secret_b,
            b"scope",
            b"replica",
            IndexEpoch(1),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn different_epochs_derive_different_labels() {
        let secret = CapabilitySecret::from_bytes([7u8; 32]);
        let e1 = derive_lookup_label(
            &secret,
            b"scope",
            b"replica",
            IndexEpoch(1),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        let e2 = derive_lookup_label(
            &secret,
            b"scope",
            b"replica",
            IndexEpoch(2),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        assert_ne!(e1, e2);
    }

    #[test]
    fn different_purposes_derive_different_labels_for_the_same_secret_and_scope() {
        let secret = CapabilitySecret::from_bytes([7u8; 32]);
        let provider = derive_lookup_label(
            &secret,
            b"scope",
            b"replica",
            IndexEpoch(1),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        let mailbox = derive_lookup_label(
            &secret,
            b"scope",
            b"replica",
            IndexEpoch(1),
            LookupPurpose::MailboxLookup,
        )
        .unwrap();
        assert_ne!(provider, mailbox);
    }

    #[test]
    fn different_scopes_derive_different_labels() {
        let secret = CapabilitySecret::from_bytes([7u8; 32]);
        let a = derive_lookup_label(
            &secret,
            b"scope-a",
            b"replica",
            IndexEpoch(1),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        let b = derive_lookup_label(
            &secret,
            b"scope-b",
            b"replica",
            IndexEpoch(1),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn different_replicas_derive_unlinkable_labels_for_the_same_logical_target() {
        let secret = CapabilitySecret::from_bytes([7u8; 32]);
        let replica_a = derive_lookup_label(
            &secret,
            b"scope",
            b"replica-a",
            IndexEpoch(1),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        let replica_b = derive_lookup_label(
            &secret,
            b"scope",
            b"replica-b",
            IndexEpoch(1),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        assert_ne!(replica_a, replica_b);
    }

    #[test]
    fn a_label_round_trips_through_bytes() {
        let secret = CapabilitySecret::from_bytes([7u8; 32]);
        let label = derive_lookup_label(
            &secret,
            b"scope",
            b"replica",
            IndexEpoch(1),
            LookupPurpose::ProviderLookup,
        )
        .unwrap();
        assert_eq!(LookupLabel::from_bytes(label.to_bytes()), label);
    }
}
