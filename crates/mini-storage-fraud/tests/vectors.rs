//! Golden vectors for every encoding this crate treats as normative.
//!
//! These are here so a second implementation — in another language, or a later
//! rewrite of this one — can be checked against fixed bytes rather than against
//! whatever this code happens to do today. A test that says "encode then decode
//! agrees with itself" cannot catch an encoding change; these can.
//!
//! **If any of these vectors changes, the wire format changed.** That is a
//! protocol break requiring a version bump and a decision-log entry, not a test
//! update.

mod support;

use did_mini::{Controller, Did};
use mini_storage_fraud::{
    derive_replica_id, seal_commitment_digest, ReplicaContextV1, REPLICA_CONTEXT_VERSION,
    REPLICA_ID_DOMAIN, SEAL_COMMITMENT_VERSION,
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Fixed identities, derived from fixed seeds, so every vector below is
/// reproducible from these four constants alone.
const ROOT_CURRENT_SEED: [u8; 32] = [0x01; 32];
const ROOT_NEXT_SEED: [u8; 32] = [0x02; 32];
const DEVICE_CURRENT_SEED: [u8; 32] = [0x03; 32];
const DEVICE_NEXT_SEED: [u8; 32] = [0x04; 32];

fn fixed_identities() -> (Did, Did) {
    let root = Controller::incept_single_from_seeds(&ROOT_CURRENT_SEED, &ROOT_NEXT_SEED).unwrap();
    let device = Controller::incept_device_single_from_seeds(
        &root.did(),
        &DEVICE_CURRENT_SEED,
        &DEVICE_NEXT_SEED,
    )
    .unwrap();
    (root.did(), device.did())
}

fn fixed_context() -> ReplicaContextV1 {
    ReplicaContextV1 {
        network_id: [0x11; 32],
        assignment_id: [0x22; 32],
        shard_index: 7,
        replica_ordinal: 1,
        sealing_policy_version: 1,
    }
}

#[test]
fn identity_vectors() {
    // The identities the rest of the vectors derive from. If did-mini's
    // inception encoding ever changes, this fails first and explains why every
    // other vector below moved.
    let (root, device) = fixed_identities();
    assert_eq!(
        root.as_str(),
        "did:mini:zgW8R2o2aFUokTr2tFE8PcjsuuTUu5fSQANJ7CPhPWtwcR9"
    );
    assert_eq!(
        device.as_str(),
        "did:mini:zgW7EXjP1EcKkVXatfVejd5Di1LyhA5saSxn1bXddB4NvaC"
    );
}

#[test]
fn replica_context_encoding_vector() {
    let bytes = fixed_context().to_bytes();
    assert_eq!(bytes.len(), 77);
    assert_eq!(bytes[0], REPLICA_CONTEXT_VERSION);
    assert_eq!(
        hex(&bytes),
        concat!(
            "01",
            "1111111111111111111111111111111111111111111111111111111111111111",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "00000007",
            "00000001",
            "00000001",
        )
    );
    assert_eq!(
        ReplicaContextV1::from_bytes(&bytes).unwrap(),
        fixed_context()
    );
}

#[test]
fn replica_id_derivation_vector() {
    let (root, device) = fixed_identities();
    assert_eq!(
        REPLICA_ID_DOMAIN,
        b"mininet/mini-storage-fraud/replica-id/v1"
    );
    assert_eq!(REPLICA_ID_DOMAIN.len(), 40);

    // The full preimage, spelled out independently of the implementation, so
    // this vector checks the documented construction rather than restating it.
    let mut preimage = Vec::new();
    preimage.extend_from_slice(REPLICA_ID_DOMAIN);
    preimage.extend_from_slice(&(root.scid().len() as u32).to_be_bytes());
    preimage.extend_from_slice(root.scid().as_bytes());
    preimage.extend_from_slice(&(device.scid().len() as u32).to_be_bytes());
    preimage.extend_from_slice(device.scid().as_bytes());
    preimage.extend_from_slice(&fixed_context().to_bytes());
    let expected = mini_crypto::HashAlgorithm::Blake3.digest(&preimage);

    let derived = derive_replica_id(&root, &device, &fixed_context());
    assert_eq!(derived, expected);
    assert_eq!(
        hex(&derived),
        "741f7f1476243caa826d12d968e9ca389053bc224c92a9ab97d1fad9bea9860a"
    );
}

#[test]
fn seal_commitment_encoding_and_digest_vector() {
    let (root, device) = fixed_identities();
    let params = mini_storage_fraud::seal_params_for(&root, &device, &fixed_context(), 2).unwrap();
    let payload: Vec<u8> = (0..4 * mini_porep::NODE_SIZE)
        .map(|i| (i % 251) as u8)
        .collect();
    let commitment = mini_porep::seal(&params, &payload).unwrap().commitment();

    assert_eq!(commitment.replica_id, params.replica_id);
    assert_eq!(commitment.num_layers, 2);
    assert_eq!(commitment.node_count, 4);
    assert_eq!(commitment.layer_roots.len(), 3);
    assert_eq!(SEAL_COMMITMENT_VERSION, 1);

    assert_eq!(
        hex(&commitment.replica_root),
        "d49e8307bcc50345404162108da31fc0a1ac2010781f1e946567e20f5055aaf9"
    );
    assert_eq!(
        hex(&seal_commitment_digest(&commitment)),
        "9a9cbc70a5caa6eaecd3a79083afd42b6244dd91df8ad6b8dd023681e303b43f"
    );
}

#[test]
fn a_claim_encodes_to_stable_bytes() {
    // The claim carries live signatures, so its full bytes are not a fixed
    // vector -- but its structure is. This pins the field order and the fact
    // that the claim's own transcript digest is stable across encode/decode.
    let provider = support::Party::provider(200);
    let (first_auditor, second_auditor) =
        (support::Party::auditor(201), support::Party::auditor(202));
    let (claim, _) = support::registered_claim(
        &provider,
        &[&first_auditor, &second_auditor],
        &support::context(1),
        &support::data(0),
    );

    let bytes = claim.to_bytes();
    assert_eq!(bytes[0], mini_storage_fraud::REPLICA_CLAIM_VERSION);
    let decoded = mini_storage_fraud::RegisteredReplicaClaim::from_bytes(&bytes).unwrap();
    assert_eq!(decoded.claim_id(), claim.claim_id());
    assert_eq!(decoded.to_bytes(), bytes);

    // The domain separators are part of the protocol, not an implementation
    // detail: changing one silently invalidates every signature ever made.
    assert_eq!(
        mini_storage_fraud::REPLICA_CLAIM_DOMAIN,
        b"mininet/mini-storage-fraud/replica-claim/v1"
    );
    assert_eq!(
        mini_storage_fraud::AUDIT_ATTESTATION_DOMAIN,
        b"mininet/mini-storage-fraud/audit-attestation/v1"
    );
    assert_eq!(
        mini_storage_fraud::SEAL_COMMITMENT_DOMAIN,
        b"mininet/mini-storage-fraud/seal-commitment/v1"
    );
}
