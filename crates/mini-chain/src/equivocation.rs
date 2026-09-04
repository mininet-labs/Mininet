//! Validator accountability: proving a validator voted two ways at once,
//! and excluding it on that proof (roadmap R8).
//!
//! # The gap this closes
//!
//! [`crate::verify_finality`] answers "is this block final?" and nothing
//! else. A validator that signs two conflicting precommits at the same
//! height and round is doing the thing BFT safety assumes nobody does —
//! and until now the protocol had no way to say so, let alone act. A
//! Byzantine validator could equivocate on every height forever at no cost
//! and with no record. That is the accountability hole R8 names.
//!
//! # Why this is not slashing
//!
//! In most chains "slashing" means burning a validator's stake. **Mininet
//! has no stake to burn**, by construction and on purpose: validator power
//! is equal per identity root, never balance-weighted (P1, P2, Directive
//! 16). There is no deposit, so there is no deposit to take.
//!
//! Inventing one would be the worst available answer. A penalty denominated
//! in value would make validator behaviour a function of wealth in exactly
//! the direction the voice/value wall exists to prevent — rich validators
//! could afford to equivocate, poor ones could not afford to validate. This
//! module therefore has **no amount, no balance, no economic type anywhere
//! in it**, and `mini-chain` has no dependency that could supply one.
//!
//! The penalty is **exclusion**: a proven equivocator stops being counted.
//! That is the only sanction a one-root-one-vote system can impose, and it
//! is exactly proportionate — the fault is about voting, so the consequence
//! is about voting.
//!
//! # The proof is the point
//!
//! An [`EquivocationProof`] carries the two conflicting votes themselves.
//! Anyone can check it: same validator root, same height, same round, same
//! phase, *different* block hashes, both signatures valid. No trusted
//! reporter, no committee, no adjudication — the validator's own two
//! signatures convict it, and a third party who was offline the whole time
//! can verify the fault years later.
//!
//! This is the same shape the rest of the tree already uses for
//! self-proving faults: `did_mini::ControllerDuplicityProof` for two
//! conflicting key events, and the key image for two spends of one output.
//! A fault worth punishing should be a fault anyone can demonstrate.
//!
//! # What this deliberately does not do
//!
//! - **It does not eject anybody automatically.** [`ValidatorSet::excluding`]
//!   produces a *new* set; adopting it is a governance action with its own
//!   process, not a side effect of verifying a proof. A protocol that
//!   removed validators the instant somebody presented bytes would be a
//!   protocol where fabricating a removal is the attack.
//! - **It does not detect equivocation on its own.** Something has to
//!   observe both votes and call [`EquivocationProof::assemble`]. Vote
//!   gossip is R8's remaining work; nothing here has a network.
//! - **It does not cover every Byzantine behaviour** — only double-voting.
//!   A validator that goes silent, censors transactions, or proposes
//!   invalid blocks is not caught here, and those faults are not
//!   self-proving in the way this one is.
//! - **It does not decide re-admission.** Whether a proven equivocator may
//!   ever validate again is a governance question this module has no
//!   opinion on.

use did_mini::Did;
use mini_crypto::HashAlgorithm;

use crate::error::{ChainError, Result};
use crate::finality::ValidatorOracle;
use crate::validator::ValidatorSet;
use crate::vote::{verify_vote, Vote, VoteKind};

/// Domain separator for an equivocation proof's digest.
pub const EQUIVOCATION_DOMAIN: &[u8] = b"mininet/mini-chain/equivocation-proof/v1";

/// Two votes by one validator that cannot both be honest.
///
/// Constructible only through [`Self::assemble`], which refuses anything
/// that is not actually a fault — so holding one of these is holding a
/// checked accusation rather than a claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivocationProof {
    first: Vote,
    second: Vote,
}

impl EquivocationProof {
    /// Build a proof from two votes, verifying both signatures and that
    /// they genuinely conflict.
    ///
    /// Refuses, in order: votes from different validators (not a fault by
    /// anyone), different heights/rounds/phases (a validator is *supposed*
    /// to vote at each of those), the same block hash (agreeing with
    /// yourself twice is a duplicate, which networks produce constantly and
    /// which is not misbehaviour), and either signature failing to verify.
    ///
    /// `oracle` supplies the KELs the signatures are checked against, the
    /// same way [`crate::verify_finality`] resolves them.
    pub fn assemble(first: Vote, second: Vote, oracle: &dyn ValidatorOracle) -> Result<Self> {
        if first.validator_root.scid() != second.validator_root.scid() {
            return Err(ChainError::NotAnEquivocation);
        }
        if first.height != second.height || first.round != second.round || first.kind != second.kind
        {
            return Err(ChainError::NotAnEquivocation);
        }
        // Same block twice is a re-broadcast, not a fault. Networks
        // re-deliver constantly; treating that as misbehaviour would make
        // ordinary gossip look like an attack.
        if first.block_hash == second.block_hash {
            return Err(ChainError::NotAnEquivocation);
        }

        for vote in [&first, &second] {
            let root_kel = oracle
                .kel(&vote.validator_root)
                .ok_or(ChainError::UnknownValidator)?;
            let device_kel = oracle
                .kel(&vote.validator_device)
                .ok_or(ChainError::UnknownValidator)?;
            verify_vote(vote, root_kel, device_kel)?;
        }

        // Canonical ordering by block hash, so two observers who saw the
        // same fault in opposite orders produce the same proof and the same
        // digest -- otherwise one fault would have two identities and a
        // registry keyed on the digest would hold it twice.
        let (first, second) = if first.block_hash <= second.block_hash {
            (first, second)
        } else {
            (second, first)
        };
        Ok(EquivocationProof { first, second })
    }

    /// The validator this proof convicts.
    pub fn offender(&self) -> &Did {
        &self.first.validator_root
    }

    pub fn height(&self) -> u64 {
        self.first.height
    }

    pub fn round(&self) -> u32 {
        self.first.round
    }

    pub fn kind(&self) -> VoteKind {
        self.first.kind
    }

    /// The two conflicting votes, in canonical order.
    pub fn votes(&self) -> (&Vote, &Vote) {
        (&self.first, &self.second)
    }

    /// This proof's identity: BLAKE3 over the offender and the exact slot
    /// and blocks it conflicted on.
    ///
    /// Two observers of the same fault compute the same digest, so a
    /// registry can deduplicate reports without comparing whole votes.
    pub fn digest(&self) -> [u8; 32] {
        let mut w = Vec::new();
        w.extend_from_slice(EQUIVOCATION_DOMAIN);
        w.extend_from_slice(self.first.validator_root.as_str().as_bytes());
        w.push(0);
        w.push(self.first.kind.to_byte());
        w.extend_from_slice(&self.first.height.to_be_bytes());
        w.extend_from_slice(&self.first.round.to_be_bytes());
        w.extend_from_slice(&self.first.block_hash);
        w.extend_from_slice(&self.second.block_hash);
        HashAlgorithm::Blake3.digest(&w)
    }
}

/// Proven equivocators, accumulated locally.
///
/// Deliberately a *local* view, like `mini_private_payment::KeyImageSet` and
/// `did_mini::DuplicityRegistry`: two nodes that never meet can hold
/// different evidence. Consensus on who has been proven faulty is a
/// governance question, not something this registry claims to answer.
#[derive(Debug, Default, Clone)]
pub struct EquivocationRegistry {
    proven: Vec<EquivocationProof>,
}

impl EquivocationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a proof. Idempotent by [`EquivocationProof::digest`], so
    /// re-reporting the same fault does not accumulate.
    pub fn record(&mut self, proof: EquivocationProof) {
        let digest = proof.digest();
        if self.proven.iter().any(|held| held.digest() == digest) {
            return;
        }
        self.proven.push(proof);
    }

    /// Whether this validator has been proven to equivocate.
    pub fn is_proven_faulty(&self, root: &Did) -> bool {
        self.proven
            .iter()
            .any(|proof| proof.offender().scid() == root.scid())
    }

    /// Every validator with at least one proof against them, canonically
    /// sorted so two registries holding the same evidence agree.
    pub fn offenders(&self) -> Vec<Did> {
        let mut roots: Vec<Did> = Vec::new();
        for proof in &self.proven {
            if !roots.iter().any(|r| r.scid() == proof.offender().scid()) {
                roots.push(proof.offender().clone());
            }
        }
        roots.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        roots
    }

    pub fn len(&self) -> usize {
        self.proven.len()
    }

    pub fn is_empty(&self) -> bool {
        self.proven.is_empty()
    }
}

impl ValidatorSet {
    /// A new set with `excluded` removed.
    ///
    /// **This does not eject anyone by itself.** It computes what the set
    /// would be, and adopting that set is a governance action with its own
    /// process. A protocol that dropped validators the moment somebody
    /// presented bytes would be a protocol where fabricating a removal is
    /// the attack — so the proof convicts, and people decide.
    ///
    /// Refuses to produce an empty set: a chain with no validators cannot
    /// finalize anything, and silently reaching that state through
    /// exclusions would be a liveness failure dressed as accountability.
    /// If enough validators are provably faulty that removing them leaves
    /// nobody, that is not a set to adopt — it is a network to stop and
    /// look at.
    pub fn excluding(&self, excluded: &[Did]) -> Result<ValidatorSet> {
        let remaining: Vec<Did> = self
            .roots()
            .iter()
            .filter(|root| !excluded.iter().any(|e| e.scid() == root.scid()))
            .cloned()
            .collect();
        if remaining.is_empty() {
            return Err(ChainError::EmptyValidatorSet);
        }
        ValidatorSet::new(remaining)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_domain_separator_is_frozen() {
        assert_eq!(
            EQUIVOCATION_DOMAIN,
            b"mininet/mini-chain/equivocation-proof/v1"
        );
    }

    #[test]
    fn an_empty_registry_convicts_nobody() {
        let registry = EquivocationRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.offenders().is_empty());
    }
}
