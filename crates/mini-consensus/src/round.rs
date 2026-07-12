//! [`Round`]: the pure, deterministic, order-insensitive Tendermint state
//! machine for a single consensus *height* — across **multiple rounds**, with
//! locking and timeouts, so a crashed or silent proposer no longer stalls the
//! height.
//!
//! This is a faithful implementation of the published, peer-reviewed,
//! widely-deployed Tendermint consensus algorithm — Buchman, Kwon &
//! Milosevic, *"The latest gossip on BFT consensus"* (arXiv:1807.04938),
//! Algorithm 1 — the same construction CometBFT runs in production. Adopting
//! it wholesale rather than inventing a bespoke view-change is the project's
//! standing rule applied to consensus: compose a construction the wider field
//! has already vetted. Each `upon` rule cites the paper's line numbers so the
//! mapping is auditable.
//!
//! It stays **clock-free and socket-free**: timeouts surface as
//! [`Action::ScheduleTimeout`] intents the host ([`crate::node`]/[`crate::net`])
//! turns into real timers and feeds back via [`Round::on_timeout`]; votes are
//! block *hashes* the host signs. The whole machine is exercised
//! deterministically in this module's tests, with no threads.
//!
//! ## Values, ids, and `nil`
//!
//! A *value* is a proposed block; its *id* is its block hash. `nil` — a
//! prevote/precommit for "no value this round" — is the all-zero hash
//! [`NIL`]. `mini_chain::verify_finality` only ever counts precommits matching
//! a *real* (non-zero) block hash, so a `nil` precommit can never contribute
//! to a certificate; the sentinel is safe. (A real block hashing to all-zero
//! is a `2^-256` BLAKE3 event and would merely read as `nil` — never a safety
//! problem.)
//!
//! ## Accountability and honest limits that remain
//!
//! Safety — never two conflicting decisions at one height — is what locking
//! buys and is implemented here in full. One identity root is counted **at
//! most once per phase**: its first prevote/precommit is authoritative, and a
//! conflicting second one is not tallied but surfaced as
//! [`Action::Equivocation`] (D-0204), verifiable proof the root double-signed,
//! for a future slashing layer that does not exist yet. Proposal authenticity
//! is the *host's* responsibility (the node checks [`crate::wire::verify_proposal`]
//! and [`proposer_for`] before calling [`Round::on_proposal`], D-0202), so this
//! layer trusts that a proposal it is told about really came from the round's
//! proposer.
//!
//! The host broadcasts each vote once (no full re-gossip of past rounds), so
//! the POLC-re-proposal path (line 28) depends on those prevotes still being
//! reachable; the crash-recovery path (a silent proposer) does not, and is
//! what the networked test exercises.

use std::collections::{HashMap, HashSet};

use did_mini::Did;
use mini_chain::{verify_vote, QuorumCertificate, ValidatorOracle, ValidatorSet, Vote, VoteKind};

use crate::evidence::EquivocationEvidence;

/// The `nil` value id — a prevote/precommit for "no block this round". Never a
/// real block hash for finality (see the module docs).
pub const NIL: [u8; 32] = [0u8; 32];

/// Deterministic proposer selection: every honest node, given the same
/// validator set (canonically sorted inside [`ValidatorSet`]) and the same
/// `(height, round)`, picks the *same* proposer with no communication. Round
/// rotates the proposer, so a silent round-0 proposer is replaced in round 1.
pub fn proposer_for(height: u64, round: u32, validators: &ValidatorSet) -> &Did {
    let roots = validators.roots();
    // roots is non-empty by ValidatorSet's construction invariant.
    let idx = (height.wrapping_add(round as u64) % roots.len() as u64) as usize;
    &roots[idx]
}

/// The three Tendermint steps within a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// Waiting for (or making) the round's proposal.
    Propose,
    /// Prevoting.
    Prevote,
    /// Precommitting.
    Precommit,
}

/// What the round driver asks its host node to do. Intents, not effects: the
/// host owns the signing key, the transport, the clock, and the block
/// *content* behind each hash.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// This node is the proposer for `round`: broadcast a proposal. If `reuse`
    /// is `Some(hash)`, re-propose that exact already-known value stamped with
    /// `valid_round` (the paper's `validValue`/`validRound`, line 16); if
    /// `None`, build a fresh value (line 18, `valid_round` is then `-1`).
    Propose {
        /// The round being proposed for.
        round: u32,
        /// A value the node already holds and must re-propose verbatim, or
        /// `None` to build a fresh one.
        reuse: Option<[u8; 32]>,
        /// The `validRound` to stamp on the proposal (`-1` for none).
        valid_round: i64,
    },
    /// Sign and broadcast a prevote for `target` (a block hash, or [`NIL`]) at
    /// `round`, then feed it back in.
    SignPrevote {
        /// The round voted in.
        round: u32,
        /// The value id prevoted, or [`NIL`].
        target: [u8; 32],
    },
    /// Sign and broadcast a precommit for `target` (a block hash, or [`NIL`])
    /// at `round`, then feed it back in.
    SignPrecommit {
        /// The round voted in.
        round: u32,
        /// The value id precommitted, or [`NIL`].
        target: [u8; 32],
    },
    /// Arm a timer for `(step, round)`; the host calls [`Round::on_timeout`]
    /// when it fires. A stale one (wrong round/step now) is ignored. Durations
    /// grow with the round, but that is the host's choice — this stays
    /// clock-free.
    ScheduleTimeout {
        /// Which step's timeout.
        step: Step,
        /// The round it belongs to.
        round: u32,
    },
    /// This height is decided: apply the block behind this certificate.
    Decided(QuorumCertificate),
    /// A validator root double-signed at this `(height, round, kind)`. The
    /// evidence is surfaced for a future slashing/governance layer; it does
    /// **not** change this round's outcome (the equivocator was already
    /// counted at most once — its conflicting second vote is dropped from the
    /// tally, not merged).
    Equivocation(EquivocationEvidence),
}

/// A value proposed at a round: its id (block hash), the `validRound` it
/// carried, and whether the host judged it valid against local state.
#[derive(Debug, Clone, Copy)]
struct ProposalInfo {
    id: [u8; 32],
    valid_round: i64,
    valid: bool,
}

/// Per-round vote tallies, keyed by value id (with [`NIL`] a normal key).
#[derive(Debug, Default)]
struct RoundVotes {
    prevotes: HashMap<[u8; 32], HashSet<String>>,
    precommits: HashMap<[u8; 32], HashSet<String>>,
    /// One precommit vote per distinct root per id — the raw certificate
    /// evidence, deduplicated so a root cannot pad a certificate.
    precommit_votes: HashMap<[u8; 32], Vec<Vote>>,
    /// Every distinct root that sent *any* message at this round — the basis
    /// for the `f+1`-higher-round skip (line 55).
    participants: HashSet<String>,
    /// The *first* prevote each root cast this round, kept so a second,
    /// different prevote from the same root is detected as equivocation (and
    /// dropped from the tally rather than counted twice).
    prevote_by_root: HashMap<String, Vote>,
    /// The first precommit each root cast this round — same purpose.
    precommit_by_root: HashMap<String, Vote>,
}

impl RoundVotes {
    fn prevote_count(&self, id: &[u8; 32]) -> usize {
        self.prevotes.get(id).map_or(0, HashSet::len)
    }
    fn precommit_count(&self, id: &[u8; 32]) -> usize {
        self.precommits.get(id).map_or(0, HashSet::len)
    }
    fn total_prevotes(&self) -> usize {
        self.prevotes.values().map(HashSet::len).sum()
    }
    fn total_precommits(&self) -> usize {
        self.precommits.values().map(HashSet::len).sum()
    }
}

/// The Tendermint state machine for one height. Multi-round, locking,
/// timeout-driven — see the module docs and the cited paper.
#[derive(Debug)]
pub struct Round {
    height: u64,
    validators: ValidatorSet,
    local_root: Did,

    // Algorithm 1 state variables (lines 1-9).
    round: u32,
    step: Step,
    locked_value: Option<[u8; 32]>,
    locked_round: i64,
    valid_value: Option<[u8; 32]>,
    valid_round: i64,
    decided: bool,

    // Accumulated messages.
    proposals: HashMap<u32, ProposalInfo>,
    votes: HashMap<u32, RoundVotes>,

    // Once-guards, so re-running `evaluate` after every event is idempotent.
    prevoted_rounds: HashSet<u32>,
    precommitted_rounds: HashSet<u32>,
    prevote_timeout_armed: HashSet<u32>,
    precommit_timeout_armed: HashSet<u32>,
    prevote_quorum_seen: HashSet<u32>,
    started_rounds: HashSet<u32>,
}

impl Round {
    /// A fresh height driver for `local_root`. Call [`Round::start`] to enter
    /// round 0.
    pub fn new(height: u64, validators: ValidatorSet, local_root: Did) -> Self {
        Round {
            height,
            validators,
            local_root,
            round: 0,
            step: Step::Propose,
            locked_value: None,
            locked_round: -1,
            valid_value: None,
            valid_round: -1,
            decided: false,
            proposals: HashMap::new(),
            votes: HashMap::new(),
            prevoted_rounds: HashSet::new(),
            precommitted_rounds: HashSet::new(),
            prevote_timeout_armed: HashSet::new(),
            precommit_timeout_armed: HashSet::new(),
            prevote_quorum_seen: HashSet::new(),
            started_rounds: HashSet::new(),
        }
    }

    /// The height this driver decides.
    pub fn height(&self) -> u64 {
        self.height
    }

    /// The current round.
    pub fn round(&self) -> u32 {
        self.round
    }

    /// Whether this height has decided.
    pub fn is_decided(&self) -> bool {
        self.decided
    }

    /// `2f+1`: the `>2/3` quorum — the threshold for a decision, a POLC, and a
    /// step timeout.
    fn quorum(&self) -> usize {
        self.validators.quorum_threshold()
    }

    /// `f+1`: enough to guarantee at least one honest sender.
    fn f_plus_1(&self) -> usize {
        self.validators.roots().len() - self.quorum() + 1
    }

    fn is_local_proposer(&self, round: u32) -> bool {
        proposer_for(self.height, round, &self.validators).scid() == self.local_root.scid()
    }

    /// StartRound(0) — enter the height. Algorithm 1, lines 11-21.
    pub fn start(&mut self) -> Vec<Action> {
        let mut actions = self.enter_round_core(0);
        actions.extend(self.evaluate());
        actions
    }

    /// StartRound(round) — lines 11-21. Enter `round` at the propose step and
    /// emit *only* the immediate proposer/timeout action; the caller (or the
    /// `evaluate` fixed-point loop) is responsible for re-evaluating pending
    /// `upon` rules for the new round. Kept free of a nested `evaluate` so it
    /// can be called from *inside* `evaluate` (the round-skip rule) without
    /// re-entrancy.
    fn enter_round_core(&mut self, round: u32) -> Vec<Action> {
        if self.decided || !self.started_rounds.insert(round) {
            return Vec::new();
        }
        self.round = round;
        self.step = Step::Propose;

        if self.is_local_proposer(round) {
            let (reuse, valid_round) = match self.valid_value {
                Some(v) => (Some(v), self.valid_round), // line 16
                None => (None, -1),                     // line 18
            };
            vec![Action::Propose {
                round,
                reuse,
                valid_round,
            }]
        } else {
            vec![Action::ScheduleTimeout {
                step: Step::Propose,
                round,
            }] // line 21
        }
    }

    /// Record a proposal the host received (and judged `valid` against its own
    /// state) for `round`, carrying `valid_round`. The first proposal seen for
    /// a round wins; later ones are ignored (one proposal per round; honest
    /// nodes send exactly one).
    pub fn on_proposal(
        &mut self,
        round: u32,
        id: [u8; 32],
        valid_round: i64,
        valid: bool,
    ) -> Vec<Action> {
        self.proposals.entry(round).or_insert(ProposalInfo {
            id,
            valid_round,
            valid,
        });
        self.evaluate()
    }

    /// Feed in a vote (from the network, or this node's own). It is fully
    /// re-verified against `oracle`; a vote that fails verification or comes
    /// from a non-validator is ignored. Votes for *any* round are accepted and
    /// filed by their own round (past rounds feed POLC; future rounds feed the
    /// `f+1` skip). One root counts **at most once per (round, phase)**: its
    /// first vote is authoritative, and a second, *different* vote for the same
    /// slot is not tallied — instead it is surfaced as
    /// [`Action::Equivocation`] (proof the root double-signed), while an exact
    /// duplicate is silently dropped.
    pub fn on_vote(&mut self, vote: Vote, oracle: &dyn ValidatorOracle) -> Vec<Action> {
        if !self.accept_vote(&vote, oracle) {
            return Vec::new();
        }
        let scid = vote.validator_root.scid().to_string();
        let round = vote.round;
        let id = vote.block_hash;
        let tally = self.votes.entry(round).or_default();
        tally.participants.insert(scid.clone());

        let by_root = match vote.kind {
            VoteKind::Prevote => &mut tally.prevote_by_root,
            VoteKind::Precommit => &mut tally.precommit_by_root,
            _ => return Vec::new(),
        };
        if let Some(prior) = by_root.get(&scid) {
            // This root already voted in this phase this round.
            if prior.block_hash == id {
                return Vec::new(); // exact duplicate — no-op
            }
            // Conflicting second vote: equivocation. Keep the first in the
            // tally, do not count this one, and surface the proof.
            let evidence = EquivocationEvidence {
                first: prior.clone(),
                second: vote,
            };
            return vec![Action::Equivocation(evidence)];
        }
        by_root.insert(scid.clone(), vote.clone());

        match vote.kind {
            VoteKind::Prevote => {
                tally.prevotes.entry(id).or_default().insert(scid);
            }
            VoteKind::Precommit => {
                if tally.precommits.entry(id).or_default().insert(scid) {
                    tally.precommit_votes.entry(id).or_default().push(vote);
                }
            }
            _ => return Vec::new(),
        }
        self.evaluate()
    }

    fn accept_vote(&self, vote: &Vote, oracle: &dyn ValidatorOracle) -> bool {
        if vote.height != self.height {
            return false;
        }
        if !self.validators.contains(&vote.validator_root) {
            return false;
        }
        let (Some(root_kel), Some(device_kel)) = (
            oracle.kel(&vote.validator_root),
            oracle.kel(&vote.validator_device),
        ) else {
            return false;
        };
        verify_vote(vote, root_kel, device_kel).is_ok()
    }

    /// A `(step, round)` timer fired. Stale timers (not for the current round,
    /// or the step already advanced) are ignored. Lines 57-67.
    pub fn on_timeout(&mut self, step: Step, round: u32) -> Vec<Action> {
        if self.decided || round != self.round {
            return Vec::new();
        }
        match step {
            // OnTimeoutPropose — lines 57-60.
            Step::Propose if self.step == Step::Propose => {
                self.prevote(round, NIL);
                let mut a = vec![Action::SignPrevote { round, target: NIL }];
                a.extend(self.evaluate());
                a
            }
            // OnTimeoutPrevote — lines 61-64.
            Step::Prevote if self.step == Step::Prevote => {
                self.precommit(round, NIL);
                let mut a = vec![Action::SignPrecommit { round, target: NIL }];
                a.extend(self.evaluate());
                a
            }
            // OnTimeoutPrecommit — lines 65-67.
            Step::Precommit => {
                let mut a = self.enter_round_core(round + 1);
                a.extend(self.evaluate());
                a
            }
            _ => Vec::new(),
        }
    }

    /// Mark that we prevoted `target` at `round` and moved to the prevote step.
    fn prevote(&mut self, round: u32, _target: [u8; 32]) {
        self.prevoted_rounds.insert(round);
        if round == self.round {
            self.step = self.step.max(Step::Prevote);
        }
    }

    /// Mark that we precommitted `target` at `round` and moved to precommit.
    fn precommit(&mut self, round: u32, _target: [u8; 32]) {
        self.precommitted_rounds.insert(round);
        if round == self.round {
            self.step = self.step.max(Step::Precommit);
        }
    }

    /// Re-derive every action the accumulated state now warrants, guarded so
    /// re-running after every event is idempotent. This is the body of
    /// Algorithm 1's message-driven `upon` rules (lines 22-56).
    fn evaluate(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.decided {
            return actions;
        }

        // Loop to a fixed point: one rule firing (e.g. a prevote quorum) can
        // enable another (a precommit), and a skip can enter a new round.
        loop {
            let before = actions.len();
            self.rule_propose_fresh(&mut actions); // lines 22-27
            self.rule_propose_polc(&mut actions); // lines 28-33
            self.rule_prevote_timeout(&mut actions); // lines 34-35
            self.rule_prevote_quorum(&mut actions); // lines 36-43
            self.rule_prevote_nil(&mut actions); // lines 44-46
            self.rule_precommit_timeout(&mut actions); // lines 47-48
            self.rule_decide(&mut actions); // lines 49-54
            self.rule_skip_round(&mut actions); // lines 55-56
            if self.decided || actions.len() == before {
                break;
            }
        }
        actions
    }

    // --- lines 22-27: fresh proposal, no POLC ---
    fn rule_propose_fresh(&mut self, actions: &mut Vec<Action>) {
        let r = self.round;
        if self.step != Step::Propose || self.prevoted_rounds.contains(&r) {
            return;
        }
        let Some(p) = self.proposals.get(&r).copied() else {
            return;
        };
        if p.valid_round != -1 {
            return;
        }
        let target = if p.valid && (self.locked_round == -1 || self.locked_value == Some(p.id)) {
            p.id
        } else {
            NIL
        };
        self.prevote(r, target);
        actions.push(Action::SignPrevote { round: r, target });
    }

    // --- lines 28-33: proposal that re-proposes a value with a POLC ---
    fn rule_propose_polc(&mut self, actions: &mut Vec<Action>) {
        let r = self.round;
        if self.step != Step::Propose || self.prevoted_rounds.contains(&r) {
            return;
        }
        let Some(p) = self.proposals.get(&r).copied() else {
            return;
        };
        let vr = p.valid_round;
        if !(vr >= 0 && (vr as u32) < r) {
            return;
        }
        // 2f+1 prevotes for p.id at round vr must exist.
        let polc = self
            .votes
            .get(&(vr as u32))
            .map_or(0, |t| t.prevote_count(&p.id))
            >= self.quorum();
        if !polc {
            return;
        }
        let target = if p.valid && (self.locked_round <= vr || self.locked_value == Some(p.id)) {
            p.id
        } else {
            NIL
        };
        self.prevote(r, target);
        actions.push(Action::SignPrevote { round: r, target });
    }

    // --- lines 34-35: first 2f+1 prevotes (any) at this round in prevote step ---
    fn rule_prevote_timeout(&mut self, actions: &mut Vec<Action>) {
        let r = self.round;
        if self.step != Step::Prevote || self.prevote_timeout_armed.contains(&r) {
            return;
        }
        if self.votes.get(&r).map_or(0, RoundVotes::total_prevotes) >= self.quorum() {
            self.prevote_timeout_armed.insert(r);
            actions.push(Action::ScheduleTimeout {
                step: Step::Prevote,
                round: r,
            });
        }
    }

    // --- lines 36-43: 2f+1 prevotes for the round's valid proposal ---
    fn rule_prevote_quorum(&mut self, actions: &mut Vec<Action>) {
        let r = self.round;
        if self.step < Step::Prevote || self.prevote_quorum_seen.contains(&r) {
            return;
        }
        let Some(p) = self.proposals.get(&r).copied() else {
            return;
        };
        if !p.valid {
            return;
        }
        if self.votes.get(&r).map_or(0, |t| t.prevote_count(&p.id)) < self.quorum() {
            return;
        }
        self.prevote_quorum_seen.insert(r);
        // validValue/validRound always update (line 42-43).
        self.valid_value = Some(p.id);
        self.valid_round = r as i64;
        if self.step == Step::Prevote && !self.precommitted_rounds.contains(&r) {
            // lock and precommit the value (lines 38-41).
            self.locked_value = Some(p.id);
            self.locked_round = r as i64;
            self.precommit(r, p.id);
            actions.push(Action::SignPrecommit {
                round: r,
                target: p.id,
            });
        }
    }

    // --- lines 44-46: 2f+1 nil prevotes -> precommit nil ---
    fn rule_prevote_nil(&mut self, actions: &mut Vec<Action>) {
        let r = self.round;
        if self.step != Step::Prevote || self.precommitted_rounds.contains(&r) {
            return;
        }
        if self.votes.get(&r).map_or(0, |t| t.prevote_count(&NIL)) >= self.quorum() {
            self.precommit(r, NIL);
            actions.push(Action::SignPrecommit {
                round: r,
                target: NIL,
            });
        }
    }

    // --- lines 47-48: first 2f+1 precommits (any) at this round ---
    fn rule_precommit_timeout(&mut self, actions: &mut Vec<Action>) {
        let r = self.round;
        if self.precommit_timeout_armed.contains(&r) {
            return;
        }
        if self.votes.get(&r).map_or(0, RoundVotes::total_precommits) >= self.quorum() {
            self.precommit_timeout_armed.insert(r);
            actions.push(Action::ScheduleTimeout {
                step: Step::Precommit,
                round: r,
            });
        }
    }

    // --- lines 49-54: 2f+1 precommits for a proposed value at ANY round -> decide ---
    fn rule_decide(&mut self, actions: &mut Vec<Action>) {
        // Check every round for which we have both a proposal and enough
        // precommits for that proposal's id.
        let candidate = self.proposals.iter().find_map(|(&r, p)| {
            if !p.valid {
                return None;
            }
            let enough =
                self.votes.get(&r).map_or(0, |t| t.precommit_count(&p.id)) >= self.quorum();
            if enough {
                Some((r, p.id))
            } else {
                None
            }
        });
        if let Some((r, id)) = candidate {
            let votes = self
                .votes
                .get(&r)
                .and_then(|t| t.precommit_votes.get(&id))
                .cloned()
                .unwrap_or_default();
            self.decided = true;
            actions.push(Action::Decided(QuorumCertificate {
                height: self.height,
                round: r,
                block_hash: id,
                votes,
            }));
        }
    }

    // --- lines 55-56: f+1 messages at a higher round -> skip to it ---
    fn rule_skip_round(&mut self, actions: &mut Vec<Action>) {
        let current = self.round;
        let target = self
            .votes
            .iter()
            .filter(|(&r, _)| r > current)
            .filter(|(_, t)| t.participants.len() >= self.f_plus_1())
            .map(|(&r, _)| r)
            .min();
        if let Some(r) = target {
            actions.extend(self.enter_round_core(r));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use did_mini::{Capabilities, Controller, Kel};
    use mini_chain::sign_vote;

    use super::*;

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

    #[derive(Default)]
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

    struct Fx {
        validators: ValidatorSet,
        oracle: Directory,
        signers: Vec<(Controller, Controller)>,
    }

    fn fixture(n: u8) -> Fx {
        let signers: Vec<(Controller, Controller)> =
            (0..n).map(|i| validator(10 + i * 10)).collect();
        let mut oracle = Directory::default();
        for (r, d) in &signers {
            oracle.insert(r.kel());
            oracle.insert(d.kel());
        }
        let validators = ValidatorSet::new(signers.iter().map(|(r, _)| r.did()).collect()).unwrap();
        Fx {
            validators,
            oracle,
            signers,
        }
    }

    /// The fixture index of the designated proposer at `(height, round)`.
    fn proposer_idx(fx: &Fx, height: u64, round: u32) -> usize {
        let did = proposer_for(height, round, &fx.validators);
        fx.signers
            .iter()
            .position(|(r, _)| r.did().scid() == did.scid())
            .unwrap()
    }

    fn vote(fx: &Fx, i: usize, kind: VoteKind, height: u64, round: u32, id: [u8; 32]) -> Vote {
        let (root, device) = &fx.signers[i];
        sign_vote(kind, height, round, id, &root.did(), device)
    }

    /// A non-proposer node's Round at height 1, so `start` never auto-proposes
    /// and the test drives proposals in by hand.
    fn non_proposer_round(fx: &Fx, height: u64) -> Round {
        let p = proposer_idx(fx, height, 0);
        let me = (p + 1) % fx.signers.len();
        Round::new(height, fx.validators.clone(), fx.signers[me].0.did())
    }

    #[test]
    fn start_proposes_iff_local_node_is_the_round_proposer() {
        let fx = fixture(4);
        let p = proposer_idx(&fx, 1, 0);
        let mut as_proposer = Round::new(1, fx.validators.clone(), fx.signers[p].0.did());
        assert!(matches!(
            as_proposer.start().as_slice(),
            [Action::Propose {
                round: 0,
                reuse: None,
                valid_round: -1
            }]
        ));

        let mut as_other = non_proposer_round(&fx, 1);
        assert!(matches!(
            as_other.start().as_slice(),
            [Action::ScheduleTimeout {
                step: Step::Propose,
                round: 0
            }]
        ));
    }

    #[test]
    fn happy_path_prevote_precommit_decide() {
        let fx = fixture(4); // quorum 3
        let id = [0xAB; 32];
        let mut round = non_proposer_round(&fx, 1);
        round.start();

        // A valid proposal arrives -> we prevote it.
        let a = round.on_proposal(0, id, -1, true);
        assert!(a.contains(&Action::SignPrevote {
            round: 0,
            target: id
        }));

        // Three distinct prevotes for it -> we lock and precommit it.
        let mut precommitted = false;
        for i in 0..3 {
            for act in round.on_vote(vote(&fx, i, VoteKind::Prevote, 1, 0, id), &fx.oracle) {
                if act
                    == (Action::SignPrecommit {
                        round: 0,
                        target: id,
                    })
                {
                    precommitted = true;
                }
            }
        }
        assert!(precommitted);

        // Three distinct precommits for it -> decide with a real certificate.
        let mut decided = None;
        for i in 0..3 {
            for act in round.on_vote(vote(&fx, i, VoteKind::Precommit, 1, 0, id), &fx.oracle) {
                if let Action::Decided(qc) = act {
                    decided = Some(qc);
                }
            }
        }
        let qc = decided.expect("3 precommits must decide");
        assert_eq!(qc.block_hash, id);
        assert_eq!(qc.round, 0);
        assert!(mini_chain::verify_finality(&qc, &fx.validators, &fx.oracle).is_ok());
    }

    #[test]
    fn a_silent_proposer_is_survived_by_advancing_to_the_next_round() {
        let fx = fixture(4);
        let mut round = non_proposer_round(&fx, 1);
        round.start();

        // No proposal ever arrives; the propose timer fires -> prevote nil.
        let a = round.on_timeout(Step::Propose, 0);
        assert!(a.contains(&Action::SignPrevote {
            round: 0,
            target: NIL
        }));

        // 2f+1 nil prevotes -> precommit nil.
        let mut precommit_nil = false;
        for i in 0..3 {
            for act in round.on_vote(vote(&fx, i, VoteKind::Prevote, 1, 0, NIL), &fx.oracle) {
                if act
                    == (Action::SignPrecommit {
                        round: 0,
                        target: NIL,
                    })
                {
                    precommit_nil = true;
                }
            }
        }
        assert!(precommit_nil, "2f+1 nil prevotes must precommit nil");

        // 2f+1 nil precommits, then the precommit timer -> we enter round 1.
        for i in 0..3 {
            round.on_vote(vote(&fx, i, VoteKind::Precommit, 1, 0, NIL), &fx.oracle);
        }
        let a = round.on_timeout(Step::Precommit, 0);
        assert_eq!(round.round(), 1, "must have advanced to round 1");
        // Round 1's proposer is a fresh validator (view-change).
        assert!(a
            .iter()
            .any(|x| matches!(x, Action::Propose { round: 1, .. })
                || matches!(x, Action::ScheduleTimeout { round: 1, .. })));
        assert!(!round.is_decided());
    }

    #[test]
    fn a_locked_node_prevotes_nil_for_a_different_value_in_a_later_round() {
        // Safety core: once locked on A in round 0, a fresh (valid_round = -1)
        // proposal for a different value B in round 1 must NOT be prevoted.
        let fx = fixture(4);
        let id_a = [0xAA; 32];
        let id_b = [0xBB; 32];
        let mut round = non_proposer_round(&fx, 1);
        round.start();

        round.on_proposal(0, id_a, -1, true);
        for i in 0..3 {
            round.on_vote(vote(&fx, i, VoteKind::Prevote, 1, 0, id_a), &fx.oracle);
        }
        // Now locked on A. Drive to round 1.
        for i in 0..3 {
            round.on_vote(vote(&fx, i, VoteKind::Precommit, 1, 0, NIL), &fx.oracle);
        }
        round.on_timeout(Step::Precommit, 0);
        assert_eq!(round.round(), 1);

        // A different value B is freshly proposed in round 1.
        let a = round.on_proposal(1, id_b, -1, true);
        assert!(
            a.contains(&Action::SignPrevote {
                round: 1,
                target: NIL
            }),
            "a node locked on A must prevote nil for a different value B, not B"
        );
        assert!(!a.contains(&Action::SignPrevote {
            round: 1,
            target: id_b
        }));
    }

    #[test]
    fn a_locked_node_may_reprevote_its_locked_value_when_reproposed_with_a_polc() {
        // The other half: a value re-proposed with a valid POLC that the node
        // locked on IS prevoted again (liveness under locking).
        let fx = fixture(4);
        let id_a = [0xAA; 32];
        let mut round = non_proposer_round(&fx, 1);
        round.start();

        round.on_proposal(0, id_a, -1, true);
        // The 2f+1 prevotes for A at round 0 form its POLC (and lock us on A).
        for i in 0..3 {
            round.on_vote(vote(&fx, i, VoteKind::Prevote, 1, 0, id_a), &fx.oracle);
        }
        for i in 0..3 {
            round.on_vote(vote(&fx, i, VoteKind::Precommit, 1, 0, NIL), &fx.oracle);
        }
        round.on_timeout(Step::Precommit, 0);
        assert_eq!(round.round(), 1);

        // A re-proposed in round 1 with validRound = 0 (its POLC round).
        let a = round.on_proposal(1, id_a, 0, true);
        assert!(
            a.contains(&Action::SignPrevote {
                round: 1,
                target: id_a
            }),
            "a locked value re-proposed with its POLC must be prevoted again"
        );
    }

    #[test]
    fn f_plus_one_messages_from_a_higher_round_skip_ahead() {
        let fx = fixture(4); // f+1 = 2
        let mut round = non_proposer_round(&fx, 1);
        round.start();
        assert_eq!(round.round(), 0);

        // Two distinct validators are already in round 3 -> we skip to round 3.
        round.on_vote(vote(&fx, 0, VoteKind::Prevote, 1, 3, [1u8; 32]), &fx.oracle);
        let a = round.on_vote(vote(&fx, 1, VoteKind::Prevote, 1, 3, [1u8; 32]), &fx.oracle);
        assert_eq!(
            round.round(),
            3,
            "f+1 higher-round votes must skip us ahead"
        );
        assert!(a
            .iter()
            .any(|x| matches!(x, Action::Propose { round: 3, .. })
                || matches!(x, Action::ScheduleTimeout { round: 3, .. })));
    }

    #[test]
    fn a_decision_can_form_from_precommits_seen_before_the_proposal() {
        // Order-insensitivity across the decide rule: precommits first, the
        // proposal (with its content) last, still decides.
        let fx = fixture(4);
        let id = [0x77; 32];
        let mut round = non_proposer_round(&fx, 1);
        round.start();

        for i in 0..3 {
            round.on_vote(vote(&fx, i, VoteKind::Precommit, 1, 0, id), &fx.oracle);
        }
        assert!(
            !round.is_decided(),
            "cannot decide without the block content"
        );

        let a = round.on_proposal(0, id, -1, true);
        assert!(a.iter().any(|x| matches!(x, Action::Decided(_))));
        assert!(round.is_decided());
    }

    #[test]
    fn a_double_signing_root_is_reported_and_counted_only_once() {
        let fx = fixture(4); // quorum 3
        let mut round = non_proposer_round(&fx, 1);
        round.start();
        let (id_a, id_b) = ([0xAA; 32], [0xBB; 32]);

        // Validator 0 prevotes A, then prevotes B at the same (height, round):
        // the second is reported as equivocation and NOT tallied.
        round.on_vote(vote(&fx, 0, VoteKind::Prevote, 1, 0, id_a), &fx.oracle);
        let actions = round.on_vote(vote(&fx, 0, VoteKind::Prevote, 1, 0, id_b), &fx.oracle);

        let reported = actions
            .iter()
            .find_map(|x| match x {
                Action::Equivocation(ev) => Some(ev.clone()),
                _ => None,
            })
            .expect("a conflicting second prevote must be reported as equivocation");
        assert!(crate::evidence::verify_equivocation(&reported, &fx.oracle));

        // Its two prevotes did not both count: A has the root, B does not.
        round.on_proposal(0, id_a, -1, true);
        // Only validators 1 and 2 additionally prevote A → 3 distinct incl. v0.
        let mut precommitted = false;
        for i in 1..3 {
            for act in round.on_vote(vote(&fx, i, VoteKind::Prevote, 1, 0, id_a), &fx.oracle) {
                if act
                    == (Action::SignPrecommit {
                        round: 0,
                        target: id_a,
                    })
                {
                    precommitted = true;
                }
            }
        }
        assert!(
            precommitted,
            "the equivocator's first (A) prevote still counts once toward A's quorum"
        );

        // An exact duplicate of an already-counted vote is a silent no-op, not
        // equivocation.
        let dup = round.on_vote(vote(&fx, 1, VoteKind::Prevote, 1, 0, id_a), &fx.oracle);
        assert!(!dup.iter().any(|x| matches!(x, Action::Equivocation(_))));
    }
}
