//! [`ConsensusNode`]: the integration point where a networked Tendermint round
//! becomes a canonical ledger advance. It wires a [`crate::round::Round`] per
//! height to a [`mini_execution::LedgerChain`], turning "consensus decided
//! block N" into "the ledger advanced to height N" — only ever behind a real
//! quorum certificate its own [`mini_execution`] layer re-verifies.
//!
//! The node owns exactly a validator's authority and no more: it signs
//! prevotes/precommits (including `nil`) with its delegated `VOTE` device
//! ([`mini_chain::sign_vote`], a typed request — never a generic `sign(bytes)`),
//! validates every proposal against its own chain state before prevoting it,
//! builds or re-proposes values when it is the round's proposer, and advances
//! only through [`mini_execution::LedgerChain::apply_finalized_block`].
//!
//! It is transport- and clock-agnostic: it consumes [`ConsensusMessage`]s and
//! timer fires ([`ConsensusNode::on_timeout`]) and emits [`Emit`]s — messages
//! to broadcast, timers to arm, heights committed. [`crate::net`] is one host
//! that drives it over real sockets and a real clock.

use std::collections::{HashMap, VecDeque};

use did_mini::{Controller, Did};
use mini_chain::{
    sign_vote, BlockHeader, QuorumCertificate, ValidatorOracle, ValidatorSet, VoteKind,
};
use mini_execution::{apply_block, LedgerChain, SettlementBlockBody};

use crate::error::{ConsensusError, Result};
use crate::round::{proposer_for, Action, Round, Step};
use crate::wire::{sign_proposal, verify_proposal, ConsensusMessage, Proposal};

/// Builds the block body this node proposes when it is a height's proposer and
/// has no `validValue` to re-propose. Called with the height being proposed.
pub type BodySource = Box<dyn FnMut(u64) -> SettlementBlockBody + Send>;

/// Everything needed to stand up a validator node.
pub struct NodeConfig<O> {
    /// This node's validator identity root.
    pub root: Did,
    /// A `VOTE`-capable device the root delegated, used to sign this node's
    /// prevotes and precommits.
    pub device: Controller,
    /// The (static, for this run) validator set.
    pub validators: ValidatorSet,
    /// Verified KELs for every validator root and device.
    pub oracle: O,
    /// Supplies a fresh block body when this node must build a proposal.
    pub body_source: BodySource,
}

impl<O> core::fmt::Debug for NodeConfig<O> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NodeConfig")
            .field("root", &self.root)
            .field("validators", &self.validators)
            .field("body_source", &"<fn>")
            .finish_non_exhaustive()
    }
}

/// Something the node wants its host transport/clock to do or know about.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Emit {
    /// Send this message to every peer.
    Broadcast(ConsensusMessage),
    /// Arm a timer for `(height, round, step)`; call [`ConsensusNode::on_timeout`]
    /// with the same triple when it fires. The host chooses the duration
    /// (growing it with `round` keeps a partitioned network live).
    ScheduleTimeout {
        /// The height the timer belongs to.
        height: u64,
        /// The round the timer belongs to.
        round: u32,
        /// The step whose timeout this is.
        step: Step,
    },
    /// A height finalized and was applied. Carries the new finalized height and
    /// the resulting [`mini_execution::LedgerState`] commitment — the value
    /// every honest node must agree on (Directive 4).
    Committed {
        /// The height that just finalized.
        height: u64,
        /// The post-application state commitment.
        commitment: [u8; 32],
    },
    /// A validator root was caught double-signing. The evidence is surfaced
    /// for a future slashing/governance layer; the current node takes no action
    /// on it beyond reporting (the equivocator was already counted at most
    /// once, so finality is unaffected).
    Equivocation(crate::evidence::EquivocationEvidence),
}

/// A validator node: a chain, plus the Tendermint driver for its next height.
pub struct ConsensusNode<O> {
    root: Did,
    device: Controller,
    validators: ValidatorSet,
    oracle: O,
    body_source: BodySource,
    chain: LedgerChain,
    round: Round,
    /// Every valid block value this node has learned for the current height,
    /// by block hash — so it can re-propose its `validValue` and apply the
    /// decided block. Cleared on every height advance.
    values: HashMap<[u8; 32], Proposal>,
    /// Messages for heights beyond the current one, buffered and replayed on
    /// advance (a faster peer routinely runs a height ahead).
    pending: Vec<ConsensusMessage>,
}

impl<O> core::fmt::Debug for ConsensusNode<O> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConsensusNode")
            .field("root", &self.root)
            .field("finalized_height", &self.chain.height())
            .field("round", &self.round.round())
            .finish_non_exhaustive()
    }
}

impl<O: ValidatorOracle> ConsensusNode<O> {
    /// Stand up a node at genesis (height 0; the first consensus height is 1).
    /// Call [`ConsensusNode::start`] to begin round 0.
    pub fn new(config: NodeConfig<O>) -> Self {
        let chain = LedgerChain::genesis();
        let round = Round::new(
            chain.height() + 1,
            config.validators.clone(),
            config.root.clone(),
        );
        ConsensusNode {
            root: config.root,
            device: config.device,
            validators: config.validators,
            oracle: config.oracle,
            body_source: config.body_source,
            chain,
            round,
            values: HashMap::new(),
            pending: Vec::new(),
        }
    }

    /// The height currently being decided (one past the finalized tip).
    pub fn current_height(&self) -> u64 {
        self.chain.height() + 1
    }

    /// The finalized tip height.
    pub fn finalized_height(&self) -> u64 {
        self.chain.height()
    }

    /// A read-only view of the finalized ledger state.
    pub fn state(&self) -> &mini_execution::LedgerState {
        self.chain.state()
    }

    /// The commitment to the current finalized state.
    pub fn commitment(&self) -> [u8; 32] {
        self.chain.state().commitment()
    }

    /// This node's validator KEL oracle — what a host driving this node
    /// (e.g. [`crate::net::run_to_height`]) needs to independently re-verify
    /// an [`Emit::Equivocation`] before recording it anywhere.
    pub fn oracle(&self) -> &O {
        &self.oracle
    }

    /// Begin consensus: enter round 0 of the current height.
    pub fn start(&mut self) -> Result<Vec<Emit>> {
        let actions = self.round.start();
        let mut emits = Vec::new();
        self.drive(actions, &mut emits)?;
        Ok(emits)
    }

    /// Feed one message (peer's or this node's own) into the node.
    pub fn on_message(&mut self, msg: ConsensusMessage) -> Result<Vec<Emit>> {
        let mut emits = Vec::new();
        self.ingest(msg, &mut emits)?;
        Ok(emits)
    }

    /// A `(height, round, step)` timer the host armed has fired. Stale ones
    /// (for a height already finalized) are ignored.
    pub fn on_timeout(&mut self, height: u64, round: u32, step: Step) -> Result<Vec<Emit>> {
        let mut emits = Vec::new();
        if height == self.current_height() {
            let actions = self.round.on_timeout(step, round);
            self.drive(actions, &mut emits)?;
        }
        Ok(emits)
    }

    fn ingest(&mut self, msg: ConsensusMessage, emits: &mut Vec<Emit>) -> Result<()> {
        let height = match &msg {
            ConsensusMessage::Proposal(p) => p.header.height,
            ConsensusMessage::Vote(v) => v.height,
        };
        let current = self.current_height();
        if height > current {
            self.pending.push(msg);
            return Ok(());
        }
        if height < current {
            return Ok(()); // stale: already finalized
        }
        match msg {
            ConsensusMessage::Proposal(p) => {
                // Authenticate the sender first: a proposal not signed by the
                // designated proposer for its `(height, round)` is dropped
                // outright — this is what closes the front-running gap. Only
                // then is the value validated against our own state.
                if !self.proposal_is_authentic(&p) {
                    return Ok(());
                }
                let (hash, valid) = self.validate_proposal(&p);
                let (round, valid_round) = (p.round, p.valid_round);
                if valid {
                    self.values.entry(hash).or_insert(p);
                }
                let actions = self.round.on_proposal(round, hash, valid_round, valid);
                self.drive(actions, emits)?;
            }
            ConsensusMessage::Vote(v) => {
                let actions = self.round.on_vote(v, &self.oracle);
                self.drive(actions, emits)?;
            }
        }
        Ok(())
    }

    /// Whether `p` really comes from `(height, round)`'s designated proposer:
    /// its `proposer_root` is exactly [`proposer_for`]'s selection for that
    /// slot, and its signature verifies as a `VOTE`-capable device of that
    /// root ([`verify_proposal`]). A proposal that fails either check is not
    /// this round's business and is dropped, so a Byzantine node cannot inject
    /// a value to waste the round.
    fn proposal_is_authentic(&self, p: &Proposal) -> bool {
        let expected = proposer_for(p.header.height, p.round, &self.validators);
        if p.proposer_root.scid() != expected.scid() {
            return false;
        }
        let (Some(root_kel), Some(device_kel)) = (
            self.oracle.kel(&p.proposer_root),
            self.oracle.kel(&p.proposer_device),
        ) else {
            return false;
        };
        verify_proposal(p, root_kel, device_kel).is_ok()
    }

    /// Validate a proposal's *value* against this node's own chain state:
    /// right height, right parent, deterministic logical timestamp, and a
    /// `state_root` this node can reproduce by applying the body. Returns
    /// the value's block hash and whether it is valid. An authentic
    /// proposal whose value is invalid is still reported (with
    /// `valid = false`) so the round can prevote `nil` for it.
    fn validate_proposal(&self, p: &Proposal) -> ([u8; 32], bool) {
        let header = &p.header;
        let hash = header.hash();
        if header.height != self.current_height() || header.prev_hash != self.chain.tip_hash() {
            return (hash, false);
        }
        // `timestamp_ms` is deterministic logical time, not proposer-supplied
        // wall time (roadmap #44's timestamp-attack finding): a signature
        // only proves who proposed a value, never that it reflects real
        // time, so the proposer gets no discretion over it at all — it must
        // equal the height exactly. Rejected here, at prevote time, before
        // the round wastes a step on it; `LedgerChain::apply_finalized_block`
        // enforces the identical rule unconditionally, so this is a cheap
        // early filter, not the authoritative check.
        if header.timestamp_ms != header.height {
            return (hash, false);
        }
        match apply_block(self.chain.state(), &p.body) {
            Ok(next) if next.commitment() == header.state_root => (hash, true),
            _ => (hash, false),
        }
    }

    /// Execute the round driver's intents to a fixed point, feeding this node's
    /// own votes back in so the round counts them.
    fn drive(&mut self, actions: Vec<Action>, emits: &mut Vec<Emit>) -> Result<()> {
        let mut work: VecDeque<Action> = actions.into();
        while let Some(action) = work.pop_front() {
            match action {
                Action::Propose {
                    round,
                    reuse,
                    valid_round,
                } => {
                    let proposal = self.build_proposal(round, reuse, valid_round)?;
                    let msg = ConsensusMessage::Proposal(proposal.clone());
                    emits.push(Emit::Broadcast(msg));
                    // Feed our own proposal back so we prevote it like everyone.
                    let hash = proposal.header.hash();
                    self.values.entry(hash).or_insert(proposal);
                    work.extend(self.round.on_proposal(round, hash, valid_round, true));
                }
                Action::SignPrevote { round, target } => {
                    let vote = sign_vote(
                        VoteKind::Prevote,
                        self.current_height(),
                        round,
                        target,
                        &self.root,
                        &self.device,
                    );
                    emits.push(Emit::Broadcast(ConsensusMessage::Vote(vote.clone())));
                    work.extend(self.round.on_vote(vote, &self.oracle));
                }
                Action::SignPrecommit { round, target } => {
                    let vote = sign_vote(
                        VoteKind::Precommit,
                        self.current_height(),
                        round,
                        target,
                        &self.root,
                        &self.device,
                    );
                    emits.push(Emit::Broadcast(ConsensusMessage::Vote(vote.clone())));
                    work.extend(self.round.on_vote(vote, &self.oracle));
                }
                Action::ScheduleTimeout { step, round } => {
                    emits.push(Emit::ScheduleTimeout {
                        height: self.current_height(),
                        round,
                        step,
                    });
                }
                Action::Equivocation(evidence) => {
                    emits.push(Emit::Equivocation(evidence));
                }
                Action::Decided(qc) => {
                    self.commit(qc, emits)?;
                    // After committing we advanced height; the remaining
                    // actions (if any) belonged to the finished height and are
                    // safely dropped — the new height starts fresh via `start`.
                    work.clear();
                }
            }
        }
        Ok(())
    }

    /// Build a proposal for `round`: either re-propose a cached `validValue`
    /// (`reuse`) stamped with `valid_round`, or build a fresh value from the
    /// body source with `valid_round = -1`.
    fn build_proposal(
        &mut self,
        round: u32,
        reuse: Option<[u8; 32]>,
        valid_round: i64,
    ) -> Result<Proposal> {
        let (header, body) = if let Some(hash) = reuse {
            // Re-propose the exact value verbatim, only re-stamping (and
            // re-signing, below) the consensus round metadata.
            let cached = self
                .values
                .get(&hash)
                .ok_or(ConsensusError::Stalled)?
                .clone();
            (cached.header, cached.body)
        } else {
            let height = self.current_height();
            let body = (self.body_source)(height);
            let next = apply_block(self.chain.state(), &body)?;
            let header = BlockHeader {
                height,
                prev_hash: self.chain.tip_hash(),
                state_root: next.commitment(),
                timestamp_ms: height, // deterministic logical time, enforced below
                proposer: self.root.clone(),
            };
            (header, body)
        };
        // Sign as this round's proposer (a typed request over the transcript,
        // never a generic sign(bytes)); receivers authenticate it before the
        // value is even considered.
        Ok(sign_proposal(
            round,
            valid_round,
            header,
            body,
            &self.root,
            &self.device,
        ))
    }

    /// Apply a decided height's block behind its certificate, then advance and
    /// replay buffered messages.
    fn commit(&mut self, qc: QuorumCertificate, emits: &mut Vec<Emit>) -> Result<()> {
        let value = self
            .values
            .get(&qc.block_hash)
            .ok_or(ConsensusError::Stalled)?
            .clone();
        let commitment = self.chain.apply_finalized_block(
            &value.header,
            &value.body,
            &qc,
            &self.validators,
            &self.oracle,
        )?;
        emits.push(Emit::Committed {
            height: value.header.height,
            commitment,
        });

        // Advance to the next height and replay anything that was waiting.
        self.round = Round::new(
            self.current_height(),
            self.validators.clone(),
            self.root.clone(),
        );
        self.values.clear();
        let start_actions = self.round.start();
        self.drive(start_actions, emits)?;
        let replay = core::mem::take(&mut self.pending);
        for msg in replay {
            self.ingest(msg, emits)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use did_mini::{Capabilities, Kel};
    use mini_crypto::SigningKey;
    use mini_settlement::sign_claim;

    use super::*;
    use crate::wire::sign_proposal;

    fn validator(seed: u8) -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    #[derive(Default, Clone)]
    struct Directory(BTreeMap<String, Kel>);
    impl Directory {
        fn insert(&mut self, kel: Kel) {
            self.0.insert(kel.scid().to_string(), kel);
        }
    }
    impl ValidatorOracle for Directory {
        fn kel(&self, did: &Did) -> Option<&Kel> {
            self.0.get(did.scid())
        }
    }

    fn body() -> SettlementBlockBody {
        let payer = SigningKey::from_seed(&[100u8; 32]);
        let claim = sign_claim(&payer, b"merchant", 100, 0, 1_000_000, b"genesis", 0).unwrap();
        SettlementBlockBody::new(vec![claim])
    }

    struct Fx {
        signers: Vec<(Controller, Controller)>,
        validators: ValidatorSet,
        oracle: Directory,
    }

    fn fixture() -> Fx {
        let signers: Vec<(Controller, Controller)> =
            (0..4u8).map(|i| validator(10 + i * 10)).collect();
        let mut oracle = Directory::default();
        for (r, d) in &signers {
            oracle.insert(r.kel());
            oracle.insert(d.kel());
        }
        let validators = ValidatorSet::new(signers.iter().map(|(r, _)| r.did()).collect()).unwrap();
        Fx {
            signers,
            validators,
            oracle,
        }
    }

    fn a_node(fx: &Fx, idx: usize) -> ConsensusNode<Directory> {
        let seed = 10 + (idx as u8) * 10;
        let device = Controller::incept_device_single_from_seeds(
            &fx.signers[idx].0.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        ConsensusNode::new(NodeConfig {
            root: fx.signers[idx].0.did(),
            device,
            validators: fx.validators.clone(),
            oracle: fx.oracle.clone(),
            body_source: Box::new(|_| body()),
        })
    }

    fn proposer_index(fx: &Fx, height: u64, round: u32) -> usize {
        let did = proposer_for(height, round, &fx.validators);
        fx.signers
            .iter()
            .position(|(r, _)| r.did().scid() == did.scid())
            .unwrap()
    }

    /// A signed, valid height-1 proposal built by fixture signer `by_idx`,
    /// claiming to be round 0's proposer.
    fn proposal_from(fx: &Fx, by_idx: usize) -> ConsensusMessage {
        let genesis = LedgerChain::genesis();
        let b = body();
        let next = apply_block(genesis.state(), &b).unwrap();
        let header = BlockHeader {
            height: 1,
            prev_hash: genesis.tip_hash(),
            state_root: next.commitment(),
            timestamp_ms: 1,
            proposer: fx.signers[by_idx].0.did(),
        };
        let (root, device) = &fx.signers[by_idx];
        ConsensusMessage::Proposal(sign_proposal(0, -1, header, b, &root.did(), device))
    }

    fn prevoted(emits: &[Emit]) -> bool {
        emits.iter().any(|e| {
            matches!(e, Emit::Broadcast(ConsensusMessage::Vote(v))
                if v.height == 1 && v.kind == VoteKind::Prevote)
        })
    }

    #[test]
    fn a_proposal_from_the_designated_proposer_is_prevoted() {
        let fx = fixture();
        let p_idx = proposer_index(&fx, 1, 0);
        // The receiver is some other validator.
        let mut node = a_node(&fx, (p_idx + 1) % 4);
        let _ = node.start().unwrap();
        let emits = node.on_message(proposal_from(&fx, p_idx)).unwrap();
        assert!(
            prevoted(&emits),
            "a valid proposal from the round proposer must be prevoted"
        );
    }

    #[test]
    fn a_signed_proposal_from_the_wrong_proposer_is_dropped() {
        let fx = fixture();
        let p_idx = proposer_index(&fx, 1, 0);
        let wrong_idx = (p_idx + 1) % 4;
        let mut node = a_node(&fx, (p_idx + 2) % 4);
        let _ = node.start().unwrap();
        // A perfectly-signed proposal — but by a validator who is NOT round 0's
        // proposer. Front-running defense: it is dropped, no prevote.
        let emits = node.on_message(proposal_from(&fx, wrong_idx)).unwrap();
        assert!(
            !prevoted(&emits),
            "a proposal from a non-designated proposer must be dropped, not prevoted"
        );
    }

    #[test]
    fn a_proposal_with_a_forged_signature_is_dropped() {
        let fx = fixture();
        let p_idx = proposer_index(&fx, 1, 0);
        let mut node = a_node(&fx, (p_idx + 1) % 4);
        let _ = node.start().unwrap();

        // Correct proposer_root, but the value is signed by a *different*
        // validator's device (a forgery of the sender). verify_proposal fails.
        let genesis = LedgerChain::genesis();
        let b = body();
        let next = apply_block(genesis.state(), &b).unwrap();
        let header = BlockHeader {
            height: 1,
            prev_hash: genesis.tip_hash(),
            state_root: next.commitment(),
            timestamp_ms: 1,
            proposer: fx.signers[p_idx].0.did(),
        };
        // Signer is the wrong validator's device, but we claim the right root.
        let wrong_device = &fx.signers[(p_idx + 1) % 4].1;
        let forged = sign_proposal(
            0,
            -1,
            header,
            b,
            &fx.signers[p_idx].0.did(), // claims the designated proposer's root
            wrong_device,               // ...but signed by someone else's device
        );
        let emits = node.on_message(ConsensusMessage::Proposal(forged)).unwrap();
        assert!(
            !prevoted(&emits),
            "a proposal whose signer is not a delegated device of the claimed root must be dropped"
        );
    }

    #[test]
    fn a_proposal_whose_timestamp_is_not_deterministic_is_prevoted_nil() {
        // Timestamp-attack finding (roadmap #44): `timestamp_ms` is
        // deterministic logical time, not proposer-supplied wall time -- at
        // height 1 the only valid value is 1. It is still an authentic
        // proposal (correct signature, correct designated proposer), so per
        // the round driver's own rules it is prevoted `nil` rather than
        // silently dropped -- exactly like a wrong height or wrong parent
        // hash's value already is, never the proposal's own (invalid) hash.
        let fx = fixture();
        let p_idx = proposer_index(&fx, 1, 0);
        let mut node = a_node(&fx, (p_idx + 1) % 4);
        let _ = node.start().unwrap();

        let genesis = LedgerChain::genesis();
        let b = body();
        let next = apply_block(genesis.state(), &b).unwrap();
        let header = BlockHeader {
            height: 1,
            prev_hash: genesis.tip_hash(),
            state_root: next.commitment(),
            timestamp_ms: 0, // does not equal the required height, 1
            proposer: fx.signers[p_idx].0.did(),
        };
        let bad_hash = header.hash();
        let (root, device) = &fx.signers[p_idx];
        let proposal = sign_proposal(0, -1, header, b, &root.did(), device);

        let emits = node
            .on_message(ConsensusMessage::Proposal(proposal))
            .unwrap();
        let prevote_target = emits.iter().find_map(|e| match e {
            Emit::Broadcast(ConsensusMessage::Vote(v))
                if v.height == 1 && v.kind == VoteKind::Prevote =>
            {
                Some(v.block_hash)
            }
            _ => None,
        });
        assert_eq!(
            prevote_target,
            Some(crate::round::NIL),
            "a proposal whose timestamp is not the required deterministic value must be \
             prevoted nil, never for its own (invalid) hash: {bad_hash:?}"
        );
    }

    #[test]
    fn a_proposer_cannot_use_an_increasing_timestamp_to_evade_the_deterministic_check() {
        // A merely-monotonic check (timestamp must exceed the previous
        // block's) would let a proposer jump straight to `u64::MAX` -- still
        // "increasing," so it would slip through. Deterministic logical time
        // gives the proposer no discretion at all: only `height` is valid.
        let fx = fixture();
        let p_idx = proposer_index(&fx, 1, 0);
        let mut node = a_node(&fx, (p_idx + 1) % 4);
        let _ = node.start().unwrap();

        let genesis = LedgerChain::genesis();
        let b = body();
        let next = apply_block(genesis.state(), &b).unwrap();
        let header = BlockHeader {
            height: 1,
            prev_hash: genesis.tip_hash(),
            state_root: next.commitment(),
            timestamp_ms: u64::MAX, // increasing, but not the required value
            proposer: fx.signers[p_idx].0.did(),
        };
        let (root, device) = &fx.signers[p_idx];
        let proposal = sign_proposal(0, -1, header, b, &root.did(), device);

        let emits = node
            .on_message(ConsensusMessage::Proposal(proposal))
            .unwrap();
        let prevote_target = emits.iter().find_map(|e| match e {
            Emit::Broadcast(ConsensusMessage::Vote(v))
                if v.height == 1 && v.kind == VoteKind::Prevote =>
            {
                Some(v.block_hash)
            }
            _ => None,
        });
        assert_eq!(
            prevote_target,
            Some(crate::round::NIL),
            "an increasing but non-deterministic timestamp must still be prevoted nil"
        );
    }
}
