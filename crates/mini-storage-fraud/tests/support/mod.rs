//! Shared scaffolding: real identities, real sealing, real audits.
//!
//! Nothing here fakes a signature or a seal. Every claim these helpers build
//! goes through `mini_porep::seal`, a genuine `mini_porep` registration audit
//! per auditor, and real `did-mini` delegation — so a test that passes says
//! something about the production path rather than about a mock.

// Each test binary that includes this module uses a different subset of it,
// so anything unused by one of them would otherwise trip `dead_code`.
#![allow(dead_code)]

use did_mini::{Capabilities, Controller, Did, Kel};
use mini_porep::{answer_challenge, seal, SealCommitment, SealedReplica, NODE_SIZE};
use mini_storage_fraud::{
    audit_and_attest, derive_replica_id, seal_params_for, AuditAttestation, RegisteredReplicaClaim,
    RegistrationPolicy, RegistrationReceipt, ReplicaContextV1, StorageRegistrationOracle,
};
use std::collections::BTreeMap;

/// Small enough to seal instantly, large enough to sample distinct challenges.
pub const NODES: usize = 8;
pub const LAYERS: u32 = 2;

/// Two auditors, eight challenges each: the smallest policy that still exercises
/// the quorum and sampling rules. A real deployment would set both far higher.
pub fn policy() -> RegistrationPolicy {
    RegistrationPolicy::new(2, 8).unwrap()
}

#[derive(Default)]
pub struct Directory(BTreeMap<String, Kel>);

impl Directory {
    pub fn insert(&mut self, kel: Kel) {
        self.0.insert(kel.scid().to_string(), kel);
    }

    pub fn refresh(&mut self, controller: &Controller) {
        self.insert(controller.kel());
    }

    pub fn forget(&mut self, did: &Did) {
        self.0.remove(did.scid());
    }
}

impl StorageRegistrationOracle for Directory {
    fn kel(&self, did: &Did) -> Option<&Kel> {
        self.0.get(did.scid())
    }
}

/// A root plus one delegated device carrying `capabilities`.
pub struct Party {
    pub root: Controller,
    pub device: Controller,
}

impl Party {
    pub fn new(seed: u8, capabilities: Capabilities) -> Self {
        let mut root =
            Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed.wrapping_add(2); 32],
            &[seed.wrapping_add(3); 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), capabilities).unwrap();
        Self { root, device }
    }

    /// A storage provider: STORE granted on top of the secondary default.
    pub fn provider(seed: u8) -> Self {
        Self::new(seed, Capabilities::secondary().with(Capabilities::STORE))
    }

    /// An auditor: ATTEST is in the primary default.
    pub fn auditor(seed: u8) -> Self {
        Self::new(seed, Capabilities::primary())
    }

    pub fn root_did(&self) -> Did {
        self.root.did()
    }

    pub fn device_did(&self) -> Did {
        self.device.did()
    }

    pub fn register(&self, directory: &mut Directory) {
        directory.refresh(&self.root);
        directory.refresh(&self.device);
    }
}

pub fn context(assignment: u8) -> ReplicaContextV1 {
    ReplicaContextV1 {
        network_id: [0xA1; 32],
        assignment_id: [assignment; 32],
        shard_index: 0,
        replica_ordinal: 0,
        sealing_policy_version: 1,
    }
}

pub fn data(fill: u8) -> Vec<u8> {
    (0..NODES * NODE_SIZE)
        .map(|i| fill.wrapping_add((i % 251) as u8))
        .collect()
}

/// Seal `payload` under the replica id `provider` is required to use.
pub fn seal_for(provider: &Party, context: &ReplicaContextV1, payload: &[u8]) -> SealedReplica {
    let params = seal_params_for(
        &provider.root_did(),
        &provider.device_did(),
        context,
        LAYERS,
    )
    .unwrap();
    assert_eq!(
        params.replica_id,
        derive_replica_id(&provider.root_did(), &provider.device_did(), context)
    );
    seal(&params, payload).unwrap()
}

/// Run a real audit and produce a real attestation, answering from `replica`.
pub fn attest(
    auditor: &Party,
    seal_commitment: &SealCommitment,
    replica: &SealedReplica,
    seed: u8,
    challenges: u32,
) -> Result<AuditAttestation, mini_storage_fraud::FraudError> {
    audit_and_attest(
        &auditor.root_did(),
        &auditor.device,
        seal_commitment,
        [seed; 32],
        challenges,
        1_700_000_000_000,
        |challenge| answer_challenge(replica, challenge).ok(),
    )
}

/// A quorum of genuine attestations from `auditors`, each under its own seed.
pub fn receipt(
    auditors: &[&Party],
    seal_commitment: &SealCommitment,
    replica: &SealedReplica,
) -> RegistrationReceipt {
    let attestations = auditors
        .iter()
        .enumerate()
        .map(|(index, auditor)| {
            attest(auditor, seal_commitment, replica, 0x40 + index as u8, 8).unwrap()
        })
        .collect();
    RegistrationReceipt::new(attestations).unwrap()
}

/// The whole honest path: seal, get audited, sign the claim.
pub fn registered_claim(
    provider: &Party,
    auditors: &[&Party],
    context: &ReplicaContextV1,
    payload: &[u8],
) -> (RegisteredReplicaClaim, SealedReplica) {
    let replica = seal_for(provider, context, payload);
    let seal_commitment = replica.commitment();
    let receipt = receipt(auditors, &seal_commitment, &replica);
    let claim = RegisteredReplicaClaim::issue(
        &provider.root_did(),
        &provider.device,
        *context,
        seal_commitment,
        receipt,
        1_700_000_000_001,
    )
    .unwrap();
    (claim, replica)
}

/// A directory holding every party's root and device KEL.
pub fn directory_of(parties: &[&Party]) -> Directory {
    let mut directory = Directory::default();
    for party in parties {
        party.register(&mut directory);
    }
    directory
}
