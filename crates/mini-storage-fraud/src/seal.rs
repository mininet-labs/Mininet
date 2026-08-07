//! The canonical encoding of a `mini_porep::SealCommitment`, its
//! well-formedness rules, and the `mini_spacetime::StorageCommitment` that
//! follows from it.
//!
//! # Why the storage commitment is derived, never supplied
//!
//! A `StorageCommitment` is a Merkle root plus a block count and nothing else —
//! it has no internal structure a verifier can check, so a claim that carries
//! one as a *separate field* is a claim with two independent statements in it
//! that can be made to disagree. Pairing an honest provider's replica root with
//! a different block count, for instance, produces an object that still
//! verifies as a signature while quietly describing a different replica.
//!
//! So this crate never accepts one. [`storage_commitment_of`] computes it from
//! the seal commitment the registration audit actually covered, and the two
//! cannot diverge because there is only one of them.

use mini_porep::SealCommitment;
use mini_spacetime::StorageCommitment;

use crate::codec::{Reader, Writer};
use crate::error::{Result, SealDefect};

/// Domain separator for [`seal_commitment_digest`].
pub const SEAL_COMMITMENT_DOMAIN: &[u8] = b"mininet/mini-storage-fraud/seal-commitment/v1";

/// Version tag carried by the canonical seal-commitment encoding.
pub const SEAL_COMMITMENT_VERSION: u8 = 1;

/// Largest replica this protocol profile admits, in 32-byte nodes: 2^30 nodes,
/// or 32 GiB of sealed data. Not a cryptographic bound — an allocation and
/// sanity bound, so a peer-supplied count cannot ask a verifier to reason about
/// (or allocate for) a replica nobody could have sealed.
pub const MAX_NODE_COUNT: usize = 1 << 30;

/// Largest stacked-layer count this profile admits. Production SDR deployments
/// use around a dozen; 64 leaves generous room without admitting absurdities.
pub const MAX_LAYERS: u32 = 64;

/// Check that a seal commitment describes a sealing run that could exist.
///
/// This is structural only. It says nothing about whether the sealing work was
/// really done — that is what `mini_porep`'s registration audit establishes, and
/// what [`crate::RegistrationReceipt`] carries evidence of.
pub fn validate_seal_commitment(seal: &SealCommitment) -> Result<()> {
    if seal.node_count == 0 {
        return Err(SealDefect::ZeroNodes.into());
    }
    if seal.node_count > MAX_NODE_COUNT {
        return Err(SealDefect::TooManyNodes {
            node_count: seal.node_count,
            max: MAX_NODE_COUNT,
        }
        .into());
    }
    if seal.num_layers == 0 {
        return Err(SealDefect::ZeroLayers.into());
    }
    if seal.num_layers > MAX_LAYERS {
        return Err(SealDefect::TooManyLayers {
            num_layers: seal.num_layers,
            max: MAX_LAYERS,
        }
        .into());
    }
    let expected = seal.num_layers as usize + 1;
    if seal.layer_roots.len() != expected {
        return Err(SealDefect::LayerRootCountMismatch {
            expected,
            got: seal.layer_roots.len(),
        }
        .into());
    }
    Ok(())
}

/// The PDP commitment implied by a seal commitment: the sealed replica's root,
/// over exactly the nodes the seal covers.
///
/// This is the value `mini_spacetime`'s ongoing possession challenges run
/// against, and `mini_porep::challenge` already proves possession against the
/// same replica root — so deriving it here keeps one replica describing itself
/// once.
pub fn storage_commitment_of(seal: &SealCommitment) -> StorageCommitment {
    StorageCommitment {
        merkle_root: seal.replica_root,
        block_count: seal.node_count,
        // Fixed by mini-porep's sealing format, and re-checked against the
        // served bytes on every challenge -- so the byte total this implies
        // is derived, not asserted.
        block_size_bytes: mini_porep::NODE_SIZE as u32,
    }
}

/// The canonical encoding of a seal commitment, byte for byte:
///
/// ```text
/// u8     version (= SEAL_COMMITMENT_VERSION)
/// [32]   replica_id
/// u32be  num_layers
/// u64be  node_count
/// [32]   data_root
/// u64be  layer_roots.len()
/// [32]*  each layer root, in layer order
/// [32]   replica_root
/// ```
pub(crate) fn write_seal_commitment(writer: &mut Writer, seal: &SealCommitment) {
    writer.u8(SEAL_COMMITMENT_VERSION);
    writer.raw(&seal.replica_id);
    writer.u32(seal.num_layers);
    writer.count(seal.node_count);
    writer.raw(&seal.data_root);
    writer.count(seal.layer_roots.len());
    for root in &seal.layer_roots {
        writer.raw(root);
    }
    writer.raw(&seal.replica_root);
}

pub(crate) fn read_seal_commitment(reader: &mut Reader<'_>) -> Result<SealCommitment> {
    if reader.u8()? != SEAL_COMMITMENT_VERSION {
        return Err(crate::error::DecodeFailure::UnsupportedVersion.into());
    }
    let replica_id = reader.raw_array::<32>()?;
    let num_layers = reader.u32()?;
    let node_count = reader.count(MAX_NODE_COUNT)?;
    let data_root = reader.raw_array::<32>()?;
    let layer_root_count = reader.count(MAX_LAYERS as usize + 1)?;
    let mut layer_roots = Vec::with_capacity(layer_root_count);
    for _ in 0..layer_root_count {
        layer_roots.push(reader.raw_array::<32>()?);
    }
    let replica_root = reader.raw_array::<32>()?;
    let seal = SealCommitment {
        replica_id,
        num_layers,
        node_count,
        data_root,
        layer_roots,
        replica_root,
    };
    validate_seal_commitment(&seal)?;
    Ok(seal)
}

/// A domain-separated digest over the canonical encoding above.
///
/// This is what an auditor signs and what a registration quorum is counted
/// against, so that "these attestations are about the same replica" is a byte
/// comparison rather than a field-by-field argument.
pub fn seal_commitment_digest(seal: &SealCommitment) -> [u8; 32] {
    let mut writer = Writer::new();
    writer.raw(SEAL_COMMITMENT_DOMAIN);
    write_seal_commitment(&mut writer, seal);
    mini_crypto::HashAlgorithm::Blake3.digest(&writer.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::FraudError;

    fn commitment() -> SealCommitment {
        mini_porep::seal(
            &mini_porep::SealParams::new([9u8; 32], 2).unwrap(),
            &vec![3u8; 8 * mini_porep::NODE_SIZE],
        )
        .unwrap()
        .commitment()
    }

    #[test]
    fn a_real_commitment_is_well_formed_and_round_trips() {
        let seal = commitment();
        assert_eq!(validate_seal_commitment(&seal), Ok(()));

        let mut writer = Writer::new();
        write_seal_commitment(&mut writer, &seal);
        let bytes = writer.finish();
        let mut reader = Reader::new(&bytes);
        assert_eq!(read_seal_commitment(&mut reader).unwrap(), seal);
        assert!(reader.finish().is_ok());
    }

    #[test]
    fn the_storage_commitment_is_the_replicas_own_root_and_size() {
        let seal = commitment();
        let derived = storage_commitment_of(&seal);
        assert_eq!(derived.merkle_root, seal.replica_root);
        assert_eq!(derived.block_count, seal.node_count);
    }

    #[test]
    fn structurally_impossible_commitments_are_rejected() {
        let mut zero_nodes = commitment();
        zero_nodes.node_count = 0;
        assert_eq!(
            validate_seal_commitment(&zero_nodes),
            Err(SealDefect::ZeroNodes.into())
        );

        let mut zero_layers = commitment();
        zero_layers.num_layers = 0;
        assert_eq!(
            validate_seal_commitment(&zero_layers),
            Err(SealDefect::ZeroLayers.into())
        );

        let mut too_many_layers = commitment();
        too_many_layers.num_layers = MAX_LAYERS + 1;
        assert!(matches!(
            validate_seal_commitment(&too_many_layers),
            Err(FraudError::Seal(SealDefect::TooManyLayers { .. }))
        ));

        let mut too_many_nodes = commitment();
        too_many_nodes.node_count = MAX_NODE_COUNT + 1;
        assert!(matches!(
            validate_seal_commitment(&too_many_nodes),
            Err(FraudError::Seal(SealDefect::TooManyNodes { .. }))
        ));

        let mut wrong_roots = commitment();
        wrong_roots.layer_roots.pop();
        assert!(matches!(
            validate_seal_commitment(&wrong_roots),
            Err(FraudError::Seal(SealDefect::LayerRootCountMismatch { .. }))
        ));
    }

    #[test]
    fn the_digest_covers_every_field() {
        let seal = commitment();
        let base = seal_commitment_digest(&seal);

        let mut changed = seal.clone();
        changed.replica_id[0] ^= 1;
        assert_ne!(base, seal_commitment_digest(&changed));

        let mut changed = seal.clone();
        changed.data_root[0] ^= 1;
        assert_ne!(base, seal_commitment_digest(&changed));

        let mut changed = seal.clone();
        changed.replica_root[0] ^= 1;
        assert_ne!(base, seal_commitment_digest(&changed));

        let mut changed = seal.clone();
        changed.layer_roots[0][0] ^= 1;
        assert_ne!(base, seal_commitment_digest(&changed));

        let mut changed = seal.clone();
        changed.node_count += 1;
        assert_ne!(base, seal_commitment_digest(&changed));
    }
}
