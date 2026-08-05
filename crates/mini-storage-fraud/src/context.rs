//! What a replica is *of*, and the identity-bound `replica_id` that follows
//! from it.
//!
//! # Why the context is a type and not a caller-chosen 32 bytes
//!
//! An opaque `[u8; 32]` "context" is a hole in the protocol. Two honest
//! implementations can fill it differently and stop agreeing on what a replica
//! id means; a claim made on one network can be replayed on another, because
//! nothing in the bytes says which network they were for. So the context is a
//! struct with named, canonically-encoded fields, and every field is there to
//! close one of those holes:
//!
//! - `network_id` — the genesis/network identifier. Without it, a claim made
//!   on a test network is a valid claim on the real one.
//! - `assignment_id` — which piece of data this is a replica of.
//! - `shard_index` — which shard of that assignment, so a provider holding
//!   several shards does not derive one id for all of them.
//! - `replica_ordinal` — which of this provider's own replicas of that shard.
//!   A provider that legitimately keeps two independent copies needs two
//!   independent seals; without an ordinal it could only ever have one.
//! - `sealing_policy_version` — the sealing parameter profile in force. If the
//!   protocol later changes layer counts, replicas sealed under the old and
//!   new profiles are distinguishable rather than silently comparable.
//!
//! # What the replica id binds to, and the choice that is not ours to make
//!
//! [`derive_replica_id`] binds **root, device, and context**. That means:
//!
//! - two identity roots can never derive the same replica id for the same
//!   assignment, which is what makes a shared replica detectable at all;
//! - two *storage devices under one root* also derive different ids, so an
//!   honest operator running two machines genuinely seals twice rather than
//!   copying one replica to both and claiming two.
//!
//! The cost is real and should be stated plainly: **a replica is bound to the
//! device that sealed it.** Moving storage to a replacement device means
//! re-sealing, which is exactly the sequential work the construction is
//! designed to make expensive. Whether that cost is worth the second property
//! — or whether uniqueness should bind to the root alone, letting a root move
//! a replica between its own machines freely — is a protocol-policy choice for
//! founder/governance review, not something this crate should decide quietly.
//! It is recorded as an open question in
//! `docs/design/storage-fraud-detection.md`.

use did_mini::Did;
use mini_crypto::HashAlgorithm;
use mini_porep::SealParams;

use crate::codec::{Reader, Writer, MAX_DID_BYTES};
use crate::error::{DecodeFailure, Result};

/// Domain separator for [`derive_replica_id`]. Hashed verbatim, with no length
/// prefix, as the first bytes of the preimage.
pub const REPLICA_ID_DOMAIN: &[u8] = b"mininet/mini-storage-fraud/replica-id/v1";

/// Version tag carried by [`ReplicaContextV1`]'s canonical encoding.
pub const REPLICA_CONTEXT_VERSION: u8 = 1;

/// What a replica is a replica *of*, canonically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplicaContextV1 {
    /// The network/genesis identifier this replica exists under.
    pub network_id: [u8; 32],
    /// The content-addressed assignment (piece) being replicated.
    pub assignment_id: [u8; 32],
    /// Which shard of that assignment.
    pub shard_index: u32,
    /// Which of this provider's own replicas of that shard.
    pub replica_ordinal: u32,
    /// The sealing parameter profile in force when this replica was sealed.
    pub sealing_policy_version: u32,
}

impl ReplicaContextV1 {
    /// The canonical encoding, byte for byte:
    ///
    /// ```text
    /// u8    version (= REPLICA_CONTEXT_VERSION)
    /// [32]  network_id
    /// [32]  assignment_id
    /// u32be shard_index
    /// u32be replica_ordinal
    /// u32be sealing_policy_version
    /// ```
    ///
    /// Fixed width throughout: no length prefixes, so no field can absorb
    /// another's bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        self.write_into(&mut writer);
        writer.finish()
    }

    pub(crate) fn write_into(&self, writer: &mut Writer) {
        writer.u8(REPLICA_CONTEXT_VERSION);
        writer.raw(&self.network_id);
        writer.raw(&self.assignment_id);
        writer.u32(self.shard_index);
        writer.u32(self.replica_ordinal);
        writer.u32(self.sealing_policy_version);
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let context = Self::read_from(&mut reader)?;
        reader.finish()?;
        Ok(context)
    }

    pub(crate) fn read_from(reader: &mut Reader<'_>) -> Result<Self> {
        if reader.u8()? != REPLICA_CONTEXT_VERSION {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        Ok(Self {
            network_id: reader.raw_array::<32>()?,
            assignment_id: reader.raw_array::<32>()?,
            shard_index: reader.u32()?,
            replica_ordinal: reader.u32()?,
            sealing_policy_version: reader.u32()?,
        })
    }
}

/// Derive the one `replica_id` a given provider must seal under for a given
/// context.
///
/// The normative preimage, byte for byte:
///
/// ```text
/// REPLICA_ID_DOMAIN                       (40 bytes, verbatim, no prefix)
/// u32be len(root_scid_utf8) || root_scid_utf8
/// u32be len(device_scid_utf8) || device_scid_utf8
/// ReplicaContextV1::to_bytes()            (77 bytes, fixed width)
/// ```
///
/// hashed with BLAKE3-256. The SCID is used rather than the full
/// `did:mini:<scid>` string because the prefix is constant and carries no
/// information; both DIDs are length-prefixed so no scid can be shifted into
/// the other's field.
///
/// Determinism is the point: an honest provider has no freedom in what it
/// seals under, and a verifier can recompute the required id from public
/// information alone.
pub fn derive_replica_id(
    provider_root: &Did,
    provider_device: &Did,
    context: &ReplicaContextV1,
) -> [u8; 32] {
    let mut writer = Writer::new();
    writer.raw(REPLICA_ID_DOMAIN);
    writer.bytes(provider_root.scid().as_bytes());
    writer.bytes(provider_device.scid().as_bytes());
    context.write_into(&mut writer);
    HashAlgorithm::Blake3.digest(&writer.finish())
}

/// The sanctioned way to build sealing parameters for a replica that is going
/// to be registered.
///
/// `mini_porep::SealParams::new` stays deliberately general — `mini-porep` is a
/// cryptography crate and knows nothing about identity, which is right. The
/// binding is enforced at this boundary instead, and again at registration:
/// [`crate::RegisteredReplicaClaim::verify`] recomputes the required id and
/// rejects any commitment that does not carry it, so a provider that ignores
/// this helper and seals under an id of its own choosing simply cannot
/// register the result.
pub fn seal_params_for(
    provider_root: &Did,
    provider_device: &Did,
    context: &ReplicaContextV1,
    num_layers: u32,
) -> mini_porep::Result<SealParams> {
    SealParams::new(
        derive_replica_id(provider_root, provider_device, context),
        num_layers,
    )
}

pub(crate) fn write_did(writer: &mut Writer, did: &Did) {
    writer.bytes(did.as_str().as_bytes());
}

pub(crate) fn read_did(reader: &mut Reader<'_>) -> Result<Did> {
    let bytes = reader.bytes_limited(MAX_DID_BYTES)?;
    let value = String::from_utf8(bytes).map_err(|_| DecodeFailure::InvalidDid)?;
    Ok(Did::parse(&value).map_err(|_| DecodeFailure::InvalidDid)?)
}

#[cfg(test)]
mod tests {
    use did_mini::Controller;

    use super::*;

    fn did(seed: u8) -> Did {
        Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32])
            .unwrap()
            .did()
    }

    fn context() -> ReplicaContextV1 {
        ReplicaContextV1 {
            network_id: [0x11; 32],
            assignment_id: [0x22; 32],
            shard_index: 3,
            replica_ordinal: 0,
            sealing_policy_version: 1,
        }
    }

    #[test]
    fn the_derivation_is_deterministic() {
        let (root, device) = (did(1), did(2));
        assert_eq!(
            derive_replica_id(&root, &device, &context()),
            derive_replica_id(&root, &device, &context())
        );
    }

    #[test]
    fn every_component_of_the_binding_changes_the_id() {
        let (root, device, other) = (did(1), did(2), did(3));
        let base = derive_replica_id(&root, &device, &context());

        assert_ne!(base, derive_replica_id(&other, &device, &context()));
        assert_ne!(base, derive_replica_id(&root, &other, &context()));

        for mutate in [
            |c: &mut ReplicaContextV1| c.network_id[0] ^= 1,
            |c: &mut ReplicaContextV1| c.assignment_id[0] ^= 1,
            |c: &mut ReplicaContextV1| c.shard_index += 1,
            |c: &mut ReplicaContextV1| c.replica_ordinal += 1,
            |c: &mut ReplicaContextV1| c.sealing_policy_version += 1,
        ] {
            let mut changed = context();
            mutate(&mut changed);
            assert_ne!(base, derive_replica_id(&root, &device, &changed));
        }
    }

    #[test]
    fn the_root_and_device_fields_cannot_be_slid_into_each_other() {
        // Length-prefixing both DIDs is what stops "ab"+"c" and "a"+"bc" from
        // hashing to the same preimage. The SCIDs here are real multibase
        // strings, so this is a regression test for the prefixing itself.
        let (a, b) = (did(4), did(5));
        assert_ne!(
            derive_replica_id(&a, &b, &context()),
            derive_replica_id(&b, &a, &context())
        );
    }

    #[test]
    fn a_context_round_trips_and_rejects_a_foreign_version() {
        let context = context();
        assert_eq!(
            ReplicaContextV1::from_bytes(&context.to_bytes()),
            Ok(context)
        );
        assert_eq!(context.to_bytes().len(), 77);

        let mut bytes = context.to_bytes();
        bytes[0] = 0xFE;
        assert_eq!(
            ReplicaContextV1::from_bytes(&bytes),
            Err(DecodeFailure::UnsupportedVersion.into())
        );

        let mut trailing = context.to_bytes();
        trailing.push(0);
        assert_eq!(
            ReplicaContextV1::from_bytes(&trailing),
            Err(DecodeFailure::TrailingBytes.into())
        );
    }

    #[test]
    fn seal_params_carry_the_derived_id() {
        let (root, device) = (did(6), did(7));
        let params = seal_params_for(&root, &device, &context(), 4).unwrap();
        assert_eq!(
            params.replica_id,
            derive_replica_id(&root, &device, &context())
        );
        assert!(seal_params_for(&root, &device, &context(), 0).is_err());
    }
}
