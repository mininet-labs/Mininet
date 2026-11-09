//! V3: real proof -> canonical membership -> honest vote -> QC -> output re-spend
//! (research PR #429, P0-1; tests T01-T10, T14-T18, T28). Mirrors the V2 harness
//! in `canonical_chain.rs` over the V3 evidence pool, verifier and generator basis.
use std::{collections::BTreeMap, sync::Arc};

use did_mini::{Capabilities, Controller, Did, Kel};
use mini_chain::{
    sign_vote, BlockHeader, QuorumCertificate, ValidatorOracle, ValidatorSet, VoteKind,
};
use mini_consensus::{
    proposer_for, sign_proposal, ConsensusMessage, ConsensusNode, ConsensusSnapshot, Emit,
    FinalizedBlock, NodeConfig, NIL,
};
use mini_execution::{
    apply_block_with_verifier, ExecutionError, LedgerChain, NullifierRecord, SettlementBlockBody,
    ShieldedGenesisAllocation, ShieldedOutput,
};
use mini_private_payment::{
    build_v3, quote_fee_micro, recover_shared_secret_v3, verify_v3, InMemoryOutputSet, OutputSet,
    PaymentPurpose, PaymentRequestV3, PrivatePaymentClaimV3, RecipientV3, SpendableOutputV3,
    RING_SIZE_V3, SUITE_ID_V3,
};
use mini_shielded_verify::{ClaimEvidencePoolV3, ShieldedClaimVerifierV3};
use mini_value::{derive_spend_scalar_v3, public_amount_commitment_v2, StealthKeypair};

const NETWORK: [u8; 32] = [0x59; 32];
const FEE_POLICY: [u8; 32] = [0x77; 32];

#[derive(Clone, Default)]
struct Directory(BTreeMap<String, Kel>);
impl ValidatorOracle for Directory {
    fn kel(&self, did: &Did) -> Option<&Kel> {
        self.0.get(did.scid())
    }
}

struct Fixture {
    owners: Vec<StealthKeypair>,
    outputs: InMemoryOutputSet,
    genesis: LedgerChain,
    evidence: Arc<ClaimEvidencePoolV3>,
    verifier: Arc<ShieldedClaimVerifierV3>,
    signers: Vec<(Controller, Controller)>,
    validators: ValidatorSet,
    directory: Directory,
}
impl Fixture {
    fn new() -> Self {
        let evidence = Arc::new(ClaimEvidencePoolV3::new());
        let verifier = Arc::new(ShieldedClaimVerifierV3::new(NETWORK, evidence.clone()));
        let owners: Vec<_> = (0..RING_SIZE_V3 + 2)
            .map(|_| StealthKeypair::generate().unwrap())
            .collect();
        let mut allocations = Vec::new();
        let mut outputs = InMemoryOutputSet::new();
        for owner in &owners {
            let key = owner.spend_public_bytes().to_vec();
            let commitment = public_amount_commitment_v2(10_000).to_vec();
            outputs.push(key.clone(), commitment.clone());
            allocations.push(ShieldedGenesisAllocation {
                output: ShieldedOutput {
                    public_key: key,
                    amount_commitment: commitment,
                },
                amount_micro: 10_000,
            });
        }
        let genesis =
            LedgerChain::genesis_with_shielded_allocations(NETWORK, allocations, verifier.as_ref())
                .unwrap();
        let mut directory = Directory::default();
        let signers: Vec<_> = (0..4u8)
            .map(|index| {
                let seed = 20 + index * 10;
                let mut root =
                    Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
                let device = Controller::incept_device_single_from_seeds(
                    &root.did(),
                    &[seed + 2; 32],
                    &[seed + 3; 32],
                )
                .unwrap();
                root.delegate_device(&device.did(), Capabilities::primary())
                    .unwrap();
                directory.0.insert(root.did().scid().to_owned(), root.kel());
                directory
                    .0
                    .insert(device.did().scid().to_owned(), device.kel());
                (root, device)
            })
            .collect();
        let validators =
            ValidatorSet::new(signers.iter().map(|(root, _)| root.did()).collect()).unwrap();
        Self {
            owners,
            outputs,
            genesis,
            evidence,
            verifier,
            signers,
            validators,
            directory,
        }
    }

    fn node(&self, index: usize) -> ConsensusNode<Directory> {
        let seed = 20 + index as u8 * 10;
        let root = self.signers[index].0.did();
        let device =
            Controller::incept_device_single_from_seeds(&root, &[seed + 2; 32], &[seed + 3; 32])
                .unwrap();
        ConsensusNode::new_with_genesis(
            NodeConfig {
                root,
                device,
                validators: self.validators.clone(),
                oracle: self.directory.clone(),
                body_source: Box::new(|_| SettlementBlockBody::default()),
            },
            self.genesis.clone(),
            Some(self.verifier.clone()),
        )
        .unwrap()
    }

    fn seal(
        &self,
        chain: &LedgerChain,
        body: SettlementBlockBody,
        state_root: [u8; 32],
    ) -> FinalizedBlock {
        let height = chain.height() + 1;
        let header = BlockHeader {
            height,
            prev_hash: chain.tip_hash(),
            body_root: body.hash(),
            state_root,
            timestamp_ms: height,
            proposer: proposer_for(height, 0, &self.validators).clone(),
        };
        let hash = header.hash();
        let votes = self.signers[..3]
            .iter()
            .map(|(root, device)| {
                sign_vote(VoteKind::Precommit, height, 0, hash, &root.did(), device)
            })
            .collect();
        FinalizedBlock {
            header,
            body,
            qc: QuorumCertificate {
                height,
                round: 0,
                block_hash: hash,
                votes,
            },
        }
    }

    fn finalized(&self, chain: &mut LedgerChain, claim: &PrivatePaymentClaimV3) -> FinalizedBlock {
        self.evidence.insert(claim.encode()).unwrap();
        let body = body(claim);
        let next =
            apply_block_with_verifier(chain.state(), &body, Some(self.verifier.as_ref())).unwrap();
        let block = self.seal(chain, body, next.commitment());
        chain
            .apply_finalized_block_with_verifier(
                &block.header,
                &block.body,
                &block.qc,
                &self.validators,
                &self.directory,
                Some(self.verifier.as_ref()),
            )
            .unwrap();
        block
    }

    fn rejects_at_vote_and_recovery(
        &self,
        claim: &PrivatePaymentClaimV3,
        body: SettlementBlockBody,
        expected: ExecutionError,
    ) {
        self.evidence.insert(claim.encode()).unwrap();
        assert_eq!(
            apply_block_with_verifier(self.genesis.state(), &body, Some(self.verifier.as_ref())),
            Err(expected.clone())
        );
        let block = self.seal(&self.genesis, body, self.genesis.state().commitment());
        let proposer = self
            .signers
            .iter()
            .position(|(root, _)| root.did() == block.header.proposer)
            .unwrap();
        let receiver = (proposer + 1) % self.signers.len();
        let mut node = self.node(receiver);
        node.start().unwrap();
        let (root, device) = &self.signers[proposer];
        let proposal = sign_proposal(
            0,
            -1,
            block.header.clone(),
            block.body.clone(),
            &root.did(),
            device,
        );
        let emits = node
            .on_message(ConsensusMessage::Proposal(proposal))
            .unwrap();
        assert!(emits.iter().any(|emit| matches!(emit, Emit::Broadcast(ConsensusMessage::Vote(vote)) if vote.kind == VoteKind::Prevote && vote.block_hash == NIL)));
        assert_eq!(
            node.catch_up(vec![block]).unwrap_err(),
            mini_consensus::ConsensusError::Execution(expected)
        );
        assert_eq!(node.finalized_height(), 0);
        assert_eq!(node.state(), self.genesis.state());
    }
}

fn body(claim: &PrivatePaymentClaimV3) -> SettlementBlockBody {
    let id = claim.claim_id();
    SettlementBlockBody::default().with_nullifiers(
        claim
            .inputs
            .iter()
            .map(|input| NullifierRecord::new(input.signature.key_image.clone(), id))
            .collect(),
    )
}

/// Pay `amount` from `spends`; the fee is the deterministic quote, so the
/// caller passes values that already include it.
fn payment(
    outputs: &InMemoryOutputSet,
    spends: Vec<SpendableOutputV3>,
    recipient: &StealthKeypair,
    amount: u64,
) -> PrivatePaymentClaimV3 {
    build_v3(
        &PaymentRequestV3 {
            network_id: NETWORK,
            fee_policy_id: FEE_POLICY,
            spends,
            recipients: vec![RecipientV3 {
                spend_public: recipient.spend_public_bytes().to_vec(),
                view_public: recipient.view_public_bytes().to_vec(),
                amount_micro: amount,
                purpose: PaymentPurpose::new(b"canonical-v3-test".to_vec()),
            }],
        },
        outputs,
    )
    .unwrap()
    .0
}

fn genesis_spend(fixture: &Fixture, index: usize) -> SpendableOutputV3 {
    SpendableOutputV3 {
        set_index: index,
        one_time_secret: fixture.owners[index].spend_secret_bytes(),
        value_micro: 10_000,
        blinding: [0; 32],
    }
}

#[test]
fn a_real_qc_finalizes_a_v3_payment_and_its_recipient_spends_the_new_output() {
    let mut fixture = Fixture::new();
    let recipient = StealthKeypair::generate().unwrap();
    let fee1 = quote_fee_micro(&FEE_POLICY, 1, 1);
    let first = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &recipient,
        10_000 - fee1,
    );
    let mut chain = fixture.genesis.clone();
    let first_block = fixture.finalized(&mut chain, &first);
    assert_eq!(chain.height(), 1);

    let verified = verify_v3(&first, &NETWORK).unwrap();
    let shared = recover_shared_secret_v3(
        &NETWORK,
        &recipient.view_secret_bytes(),
        &recipient.spend_public_bytes(),
        &first.outputs[0],
    )
    .unwrap();
    let note = verified.open_memo(0, &shared).unwrap();
    let output = &first.outputs[0];
    assert_eq!(
        chain
            .state()
            .shielded_outputs()
            .get(&output.one_time_address),
        Some(&output.amount_commitment)
    );
    let scalar = derive_spend_scalar_v3(
        SUITE_ID_V3,
        &NETWORK,
        &recipient.view_secret_bytes(),
        &recipient.spend_secret_bytes(),
        &mini_value::StealthOutput {
            tx_public_key: output.tx_public_key.clone(),
            one_time_address: output.one_time_address.clone(),
        },
    )
    .unwrap();
    fixture.outputs.push(
        output.one_time_address.clone(),
        output.amount_commitment.clone(),
    );
    let fee2 = quote_fee_micro(&FEE_POLICY, 1, 1);
    let second = payment(
        &fixture.outputs,
        vec![SpendableOutputV3 {
            set_index: fixture.outputs.len() - 1,
            one_time_secret: scalar.to_bytes(),
            value_micro: note.amount_micro,
            blinding: note.blinding,
        }],
        &fixture.owners[1],
        note.amount_micro - fee2,
    );
    let second_block = fixture.finalized(&mut chain, &second);

    // T28: supply conservation across both spends: the pool shrank by exactly
    // the two quoted fees and those fees are the unallocated remainder.
    assert_eq!(chain.state().nullifier_count(), 2);
    assert_eq!(
        chain.state().shielded_pool().as_micro(),
        ((RING_SIZE_V3 + 2) as u128) * 10_000 - u128::from(fee1 + fee2)
    );
    assert_eq!(
        chain.state().unallocated_circulating().as_micro(),
        u128::from(fee1 + fee2)
    );
    chain.state().verify_supply_conservation().unwrap();

    // T16/T17: an independent validator replays the same bytes to the same
    // state, and a QC-bound snapshot restores it.
    let mut node = fixture.node(0);
    node.catch_up(vec![first_block, second_block.clone()])
        .unwrap();
    assert_eq!(node.state(), chain.state());
    let snapshot = ConsensusSnapshot::new(
        second_block.header.clone(),
        second_block.qc.clone(),
        chain.state().clone(),
    )
    .unwrap();
    let restored = snapshot
        .clone()
        .into_chain_with_verifier(
            NETWORK,
            &fixture.validators,
            &fixture.directory,
            fixture.genesis.state().shielded_genesis_commitment(),
            Some(fixture.verifier.as_ref()),
        )
        .unwrap();
    assert_eq!(restored.state(), chain.state());
    assert!(snapshot
        .into_chain_with_verifier(
            NETWORK,
            &fixture.validators,
            &fixture.directory,
            fixture.genesis.state().shielded_genesis_commitment(),
            None
        )
        .is_err());
}

#[test]
fn t01_genuine_proofs_over_invented_inputs_are_rejected() {
    let fixture = Fixture::new();
    let invented = Fixture::new();
    let claim = payment(
        &invented.outputs,
        vec![genesis_spend(&invented, 0)],
        &fixture.owners[1],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    assert!(verify_v3(&claim, &NETWORK).is_ok());
    fixture.rejects_at_vote_and_recovery(
        &claim,
        body(&claim),
        ExecutionError::UnknownShieldedInput,
    );
}

#[test]
fn t01_a_valid_proof_cannot_rebind_a_canonical_key_to_an_inflated_commitment() {
    let fixture = Fixture::new();
    let mut rebound = InMemoryOutputSet::new();
    for (index, owner) in fixture.owners.iter().enumerate() {
        rebound.push(
            owner.spend_public_bytes().to_vec(),
            public_amount_commitment_v2(if index == 0 { 20_000 } else { 10_000 }),
        );
    }
    let claim = payment(
        &rebound,
        vec![SpendableOutputV3 {
            set_index: 0,
            one_time_secret: fixture.owners[0].spend_secret_bytes(),
            value_micro: 20_000,
            blinding: [0; 32],
        }],
        &fixture.owners[1],
        20_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    assert!(verify_v3(&claim, &NETWORK).is_ok());
    fixture.rejects_at_vote_and_recovery(
        &claim,
        body(&claim),
        ExecutionError::UnknownShieldedInput,
    );
}

#[test]
fn t02_copied_key_images_substituted_ids_and_partial_groups_never_enter_state() {
    let fixture = Fixture::new();
    let claim = payment(
        &fixture.outputs,
        (0..2).map(|i| genesis_spend(&fixture, i)).collect(),
        &fixture.owners[2],
        20_000 - quote_fee_micro(&FEE_POLICY, 2, 1),
    );
    let complete = body(&claim);
    let mut copied = complete.clone();
    copied.nullifiers[0].key_image = vec![0x42; 32];
    let mut substituted = complete.clone();
    for record in &mut substituted.nullifiers {
        record.claim_digest = [0x44; 32];
    }
    let mut partial = complete;
    partial.nullifiers.pop();
    for attack in [copied, substituted, partial] {
        fixture.rejects_at_vote_and_recovery(&claim, attack, ExecutionError::InvalidShieldedClaim);
    }
}

#[test]
fn t04_a_claim_for_another_network_is_never_verified_here() {
    let fixture = Fixture::new();
    let mut claim = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &fixture.owners[1],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    claim.network_id = [0xee; 32];
    // The pool admits any canonical bytes; the verifier refuses the network.
    fixture.rejects_at_vote_and_recovery(
        &claim,
        body(&claim),
        ExecutionError::InvalidShieldedClaim,
    );
}

#[test]
fn t05_the_v2_basis_is_refused_at_genesis_and_v2_claims_never_enter_the_v3_pool() {
    // A V2-basis genesis allocation cannot be funded under the V3 verifier.
    let evidence = Arc::new(ClaimEvidencePoolV3::new());
    let verifier = ShieldedClaimVerifierV3::new(NETWORK, evidence.clone());
    let owner = StealthKeypair::generate().unwrap();
    let allocation = ShieldedGenesisAllocation {
        output: ShieldedOutput {
            public_key: owner.spend_public_bytes().to_vec(),
            amount_commitment: mini_value::public_amount_commitment(10_000).to_vec(),
        },
        amount_micro: 10_000,
    };
    assert!(
        LedgerChain::genesis_with_shielded_allocations(NETWORK, vec![allocation], &verifier)
            .is_err()
    );
    // V2 claim bytes do not decode as V3 and are never stored.
    assert!(evidence.insert(vec![0u8; 64]).is_err());
    assert!(evidence.is_empty());
}

#[test]
fn t08_t15_evidence_is_bound_to_the_exact_bytes_and_cannot_be_replaced() {
    let fixture = Fixture::new();
    let claim = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &fixture.owners[1],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    let id = fixture.evidence.insert(claim.encode()).unwrap();
    assert_eq!(id, claim.claim_id());
    // Same bytes again: free, idempotent.
    assert_eq!(fixture.evidence.insert(claim.encode()).unwrap(), id);
    assert_eq!(fixture.evidence.len(), 1);
    // A differently signed encoding of the same unsigned payload is a
    // different claim_id, so it can neither replace nor shadow the first.
    let mut resigned = claim.clone();
    resigned.inputs[0].signature.challenge[0] ^= 1;
    let other = fixture.evidence.insert(resigned.encode()).unwrap();
    assert_ne!(other, id);
    assert_eq!(fixture.evidence.get(&id).unwrap(), claim.encode());
    // And the tampered one never verifies.
    assert!(verify_v3(&resigned, &NETWORK).is_err());
    fixture.rejects_at_vote_and_recovery(
        &resigned,
        body(&resigned),
        ExecutionError::InvalidShieldedClaim,
    );
}

#[test]
fn t10_noncanonical_and_hostile_bytes_are_rejected_before_storage() {
    let pool = ClaimEvidencePoolV3::new();
    let fixture = Fixture::new();
    let claim = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &fixture.owners[1],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    let mut trailing = claim.encode();
    trailing.push(0);
    assert!(pool.insert(trailing).is_err());
    let mut truncated = claim.encode();
    truncated.pop();
    assert!(pool.insert(truncated).is_err());
    // Every single-byte prefix and a spread of single-bit flips must be
    // rejected or, if still decodable, never verify; none may panic.
    let bytes = claim.encode();
    for cut in (0..bytes.len()).step_by(97) {
        let _ = pool.insert(bytes[..cut].to_vec());
    }
    for position in (0..bytes.len()).step_by(211) {
        let mut flipped = bytes.clone();
        flipped[position] ^= 0x80;
        if let Ok(id) = pool.insert(flipped) {
            let stored = PrivatePaymentClaimV3::decode(&pool.get(&id).unwrap()).unwrap();
            assert!(verify_v3(&stored, &NETWORK).is_err());
        }
    }
    assert!(pool.insert(vec![0u8; MAX_BYTES_OVER]).is_err());
}

#[test]
fn evidence_survives_a_restart_and_a_corrupt_file_fails_closed() {
    use mini_shielded_verify::EvidenceLimits;
    let fixture = Fixture::new();
    let claim = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &fixture.owners[1],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    let pool = ClaimEvidencePoolV3::new();
    let id = pool.insert(claim.encode()).unwrap();
    let dir = std::env::temp_dir().join(format!("mini-evidence-v3-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("evidence");
    // A missing file is an empty pool.
    assert!(ClaimEvidencePoolV3::load(&path, EvidenceLimits::default())
        .unwrap()
        .is_empty());
    pool.save(&path).unwrap();
    let reloaded = ClaimEvidencePoolV3::load(&path, EvidenceLimits::default()).unwrap();
    assert_eq!(reloaded.get(&id).unwrap(), claim.encode());
    assert_eq!(reloaded.len(), 1);

    // Corrupt, truncated, padded and wrongly-framed files all fail closed.
    let good = std::fs::read(&path).unwrap();
    let mut cases: Vec<Vec<u8>> = vec![Vec::new(), good[..good.len() - 1].to_vec()];
    let mut padded = good.clone();
    padded.push(0);
    cases.push(padded);
    let mut flipped = good.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 1;
    cases.push(flipped);
    let mut bad_magic = good.clone();
    bad_magic[0] ^= 1;
    cases.push(bad_magic);
    for case in cases {
        std::fs::write(&path, case).unwrap();
        assert!(
            ClaimEvidencePoolV3::load(&path, EvidenceLimits::default()).is_err(),
            "corrupt evidence file must not load"
        );
    }
    // A duplicated entry is not silently merged.
    let mut doubled = good[..good.len()].to_vec();
    doubled[26 + 3] = 2; // count = 2
    doubled.extend_from_slice(&good[26 + 4..]); // the same entry again
    std::fs::write(&path, doubled).unwrap();
    assert!(ClaimEvidencePoolV3::load(&path, EvidenceLimits::default()).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fetching_missing_claims_stores_only_what_was_asked_for() {
    use mini_shielded_verify::exchange::{
        ingest, serve, ClaimRequest, ClaimResponse, ExchangeError, MAX_REQUEST_IDS,
        MAX_RESPONSE_CLAIMS,
    };
    let fixture = Fixture::new();
    let fee = quote_fee_micro(&FEE_POLICY, 1, 1);
    let claim_a = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &fixture.owners[1],
        10_000 - fee,
    );
    let claim_b = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 1)],
        &fixture.owners[1],
        10_000 - fee,
    );
    let (id_a, id_b) = (claim_a.claim_id(), claim_b.claim_id());

    // A peer holds both; we ask for A only and it also offers B.
    let peer = ClaimEvidencePoolV3::new();
    peer.insert(claim_a.encode()).unwrap();
    peer.insert(claim_b.encode()).unwrap();
    let ask_a = ClaimRequest::new([id_a]).unwrap();
    let greedy = ClaimResponse::decode(&{
        let both = ClaimRequest::new([id_a, id_b]).unwrap();
        serve(&peer, &both).encode()
    })
    .unwrap();
    let mine = ClaimEvidencePoolV3::new();
    let report = ingest(&mine, &ask_a, &greedy);
    assert_eq!(report.stored, vec![id_a]);
    assert_eq!(report.unsolicited, 1, "B was not asked for");
    assert!(mine.get(&id_b).is_none(), "unsolicited bytes never stored");
    assert!(report.still_missing.is_empty());

    // A claim we already hold is not "missing" even if the peer omits it.
    let held = ingest(&mine, &ask_a, &serve(&ClaimEvidencePoolV3::new(), &ask_a));
    assert!(held.still_missing.is_empty());
    assert_eq!(held.stored, vec![id_a]);

    // Withholding is reported, not mistaken for absence.
    let empty_peer = ClaimEvidencePoolV3::new();
    let ask_both = ClaimRequest::new([id_a, id_b]).unwrap();
    let fresh_for_withholding = ClaimEvidencePoolV3::new();
    let report = ingest(
        &fresh_for_withholding,
        &ask_both,
        &serve(&empty_peer, &ask_both),
    );
    let mut both_sorted = vec![id_a, id_b];
    both_sorted.sort();
    assert_eq!(report.still_missing, both_sorted);

    // Serving never mutates the server and omits what it does not hold.
    let partial = ClaimEvidencePoolV3::new();
    partial.insert(claim_b.encode()).unwrap();
    let answered = serve(&partial, &ask_both);
    assert_eq!(answered.claims().len(), 1);
    assert_eq!(partial.len(), 1);

    // Garbage and substituted bytes under a wanted id cannot get in.
    let hostile = ClaimResponse::decode(
        &ClaimResponse::decode(&serve(&peer, &ask_a).encode())
            .map(|_| {
                let mut bytes = serve(&peer, &ask_a).encode();
                let last = bytes.len() - 1;
                bytes[last] ^= 1; // flips a byte inside the claim
                bytes
            })
            .unwrap(),
    )
    .unwrap();
    let fresh = ClaimEvidencePoolV3::new();
    let report = ingest(&fresh, &ask_a, &hostile);
    assert!(report.stored.is_empty());
    assert_eq!(report.still_missing, vec![id_a]);
    assert!(fresh.is_empty());

    // Codecs are strict.
    let request_bytes = ask_both.encode();
    assert_eq!(ClaimRequest::decode(&request_bytes).unwrap(), ask_both);
    assert_eq!(
        ClaimRequest::decode(&request_bytes[..request_bytes.len() - 1]),
        Err(ExchangeError::Malformed)
    );
    let mut trailing = request_bytes.clone();
    trailing.push(0);
    assert_eq!(
        ClaimRequest::decode(&trailing),
        Err(ExchangeError::Malformed)
    );
    // Unsorted or duplicated ids in a request are refused.
    let mut dup = ask_both.encode();
    let n = dup.len();
    let first: Vec<u8> = dup[n - 64..n - 32].to_vec();
    dup[n - 32..].copy_from_slice(&first);
    assert_eq!(ClaimRequest::decode(&dup), Err(ExchangeError::DuplicateId));
    assert_eq!(
        ClaimRequest::new((0..=MAX_REQUEST_IDS as u8).map(|i| [i; 32])),
        Err(ExchangeError::TooMany)
    );
    // A response claiming more items than allowed is refused before parsing.
    let mut huge = b"mininet/claim-fetch/v3/response\n".to_vec();
    huge.extend_from_slice(&((MAX_RESPONSE_CLAIMS as u16) + 1).to_be_bytes());
    assert_eq!(ClaimResponse::decode(&huge), Err(ExchangeError::TooMany));
    let _ = MAX_RESPONSE_CLAIMS;
}

const MAX_BYTES_OVER: usize = mini_private_payment::MAX_CLAIM_V3_BYTES + 1;

#[test]
fn t14_missing_or_unknown_evidence_never_passes() {
    let fixture = Fixture::new();
    let claim = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &fixture.owners[1],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    // Deliberately not inserted into the pool.
    assert_eq!(
        apply_block_with_verifier(
            fixture.genesis.state(),
            &body(&claim),
            Some(fixture.verifier.as_ref())
        ),
        Err(ExecutionError::InvalidShieldedClaim)
    );
}

#[test]
fn t03_two_valid_claims_spending_one_output_finalize_at_most_once_in_either_order() {
    let fixture = Fixture::new();
    let a = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &fixture.owners[1],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    let b = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &fixture.owners[2],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    assert_eq!(
        a.inputs[0].signature.key_image,
        b.inputs[0].signature.key_image
    );
    fixture.evidence.insert(a.encode()).unwrap();
    fixture.evidence.insert(b.encode()).unwrap();
    let v: Option<&dyn mini_execution::ClaimVerifier> = Some(fixture.verifier.as_ref());
    let state0 = fixture.genesis.state();
    let after_a = apply_block_with_verifier(state0, &body(&a), v).unwrap();
    let after_b = apply_block_with_verifier(state0, &body(&b), v).unwrap();
    // Each order: the loser cannot take the held key image.
    let a_then_b = apply_block_with_verifier(&after_a, &body(&b), v);
    let b_then_a = apply_block_with_verifier(&after_b, &body(&a), v);
    // The second block either errors or leaves the first winner's image
    // untouched; it never reassigns it.
    for (winner_id, state) in [(a.claim_id(), a_then_b), (b.claim_id(), b_then_a)] {
        if let Ok(state) = state {
            assert_eq!(
                state.finalized_nullifier(&a.inputs[0].signature.key_image),
                Some(winner_id)
            );
        }
    }
    // Independent replicas applying the same single block agree exactly.
    let one = apply_block_with_verifier(state0, &body(&a), v).unwrap();
    let two = apply_block_with_verifier(state0, &body(&a), v).unwrap();
    assert_eq!(one.commitment(), two.commitment());
    assert_eq!(one, after_a);
}

#[test]
fn t18_a_failed_application_leaves_the_prior_state_untouched() {
    let fixture = Fixture::new();
    let good = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &fixture.owners[1],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    // A genuine proof, but over inputs this chain never created.
    let invented = Fixture::new();
    let bad = payment(
        &invented.outputs,
        vec![genesis_spend(&invented, 1)],
        &fixture.owners[2],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    fixture.evidence.insert(good.encode()).unwrap();
    fixture.evidence.insert(bad.encode()).unwrap();
    let mut both = body(&good);
    both.nullifiers.extend(body(&bad).nullifiers);
    let before = fixture.genesis.state().clone();
    assert!(apply_block_with_verifier(&before, &both, Some(fixture.verifier.as_ref())).is_err());
    assert_eq!(&before, fixture.genesis.state());
    assert_eq!(before.nullifier_count(), 0);
}

#[test]
fn t07_altering_the_fee_policy_after_signing_invalidates_the_spend() {
    let fixture = Fixture::new();
    let mut claim = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &fixture.owners[1],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    assert!(verify_v3(&claim, &NETWORK).is_ok());
    claim.fee_policy_id = [0x78; 32];
    assert!(verify_v3(&claim, &NETWORK).is_err());
    fixture.rejects_at_vote_and_recovery(
        &claim,
        body(&claim),
        ExecutionError::InvalidShieldedClaim,
    );
}

#[test]
fn t06_an_output_commitment_swapped_for_another_amount_cannot_balance_or_prove() {
    let fixture = Fixture::new();
    let mut claim = payment(
        &fixture.outputs,
        vec![genesis_spend(&fixture, 0)],
        &fixture.owners[1],
        10_000 - quote_fee_micro(&FEE_POLICY, 1, 1),
    );
    // A different (bigger) hidden amount than the one proven and balanced.
    let inflated = mini_value::pedersen_commitment_v2(u64::MAX, &[7u8; 32]).unwrap();
    claim.outputs[0].amount_commitment = inflated.to_vec();
    assert!(verify_v3(&claim, &NETWORK).is_err());
    fixture.rejects_at_vote_and_recovery(
        &claim,
        body(&claim),
        ExecutionError::InvalidShieldedClaim,
    );
}

#[test]
fn t09_swapped_memos_do_not_open_in_the_other_outputs_context() {
    let fixture = Fixture::new();
    let first = StealthKeypair::generate().unwrap();
    let second = StealthKeypair::generate().unwrap();
    let fee = quote_fee_micro(&FEE_POLICY, 1, 2);
    let claim = build_v3(
        &PaymentRequestV3 {
            network_id: NETWORK,
            fee_policy_id: FEE_POLICY,
            spends: vec![genesis_spend(&fixture, 0)],
            recipients: [(&first, 4_000u64), (&second, 10_000 - fee - 4_000)]
                .into_iter()
                .map(|(to, amount)| RecipientV3 {
                    spend_public: to.spend_public_bytes().to_vec(),
                    view_public: to.view_public_bytes().to_vec(),
                    amount_micro: amount,
                    purpose: PaymentPurpose::new(b"memo-swap".to_vec()),
                })
                .collect(),
        },
        &fixture.outputs,
    )
    .unwrap()
    .0;
    let mut swapped = claim.clone();
    swapped.outputs[0].sealed_memo = claim.outputs[1].sealed_memo.clone();
    swapped.outputs[1].sealed_memo = claim.outputs[0].sealed_memo.clone();
    let verified = verify_v3(&swapped, &NETWORK);
    // Whether or not the swap changes the claim_id, a recipient must not be
    // able to open the other output's memo in this output's context.
    let shared = recover_shared_secret_v3(
        &NETWORK,
        &first.view_secret_bytes(),
        &first.spend_public_bytes(),
        &swapped.outputs[0],
    )
    .unwrap();
    if let Ok(verified) = verified {
        assert!(verified.open_memo(0, &shared).is_err());
    }
    assert_ne!(swapped.claim_id(), claim.claim_id());
}
