//! Votes: a validator device's signed commitment to a block hash at a given
//! height/round.
//!
//! ## `Capabilities::VOTE`'s first real consumer
//!
//! `did-mini` has carried `Capabilities::VOTE` since SPEC-01 §6 with the
//! documented meaning "this device may cast the root's single equal vote,"
//! but until this crate nothing actually checked it. [`verify_vote`] is that
//! check: a vote only counts if signed by a device the human-root delegated
//! with `VOTE` — capability scoping narrows *which* device may vote, it
//! never adds a vote (the root is still counted at most once, enforced one
//! layer up in [`crate::finality::verify_finality`]).

use did_mini::{verify_delegation, Capabilities, Controller, Did, IndexedSig, Kel};

use crate::error::{ChainError, Result};

/// The two Tendermint-style voting phases. Only [`VoteKind::Precommit`]
/// counts toward finality ([`crate::finality::verify_finality`]);
/// `Prevote` exists so the type is honest about the two-phase protocol even
/// though this batch only verifies the phase that finalizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoteKind {
    /// First-round signal of intent to commit a block.
    Prevote,
    /// The phase that finalizes: `>2/3` distinct Precommits on the same
    /// block at the same height/round is what quorum certificates count.
    Precommit,
}

impl VoteKind {
    fn to_byte(self) -> u8 {
        match self {
            VoteKind::Prevote => 0,
            VoteKind::Precommit => 1,
        }
    }
}

/// A signed vote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vote {
    /// Which voting phase this is.
    pub kind: VoteKind,
    /// The height being voted on.
    pub height: u64,
    /// The consensus round within that height.
    pub round: u32,
    /// The block hash being voted for.
    pub block_hash: [u8; 32],
    /// The voting validator's identity root.
    pub validator_root: Did,
    /// The delegated device that signed this vote.
    pub validator_device: Did,
    signature: Vec<IndexedSig>,
}

impl Vote {
    /// What the device actually signs: everything that must be pinned for
    /// the vote to be unambiguous and unforgeable across heights/rounds/
    /// blocks/phases.
    fn transcript(kind: VoteKind, height: u64, round: u32, block_hash: &[u8; 32]) -> Vec<u8> {
        let mut w = Vec::with_capacity(1 + 8 + 4 + 32);
        w.push(kind.to_byte());
        w.extend_from_slice(&height.to_be_bytes());
        w.extend_from_slice(&round.to_be_bytes());
        w.extend_from_slice(block_hash);
        w
    }

    /// The device signatures over this vote's transcript.
    pub fn signature(&self) -> &[IndexedSig] {
        &self.signature
    }
}

/// Sign a vote with a delegated device. Does not itself check that `device`
/// holds `VOTE` — that is enforced on the verifying side
/// ([`verify_vote`]), matching this tree's convention that signing is
/// local and free, verification is where trust decisions happen.
pub fn sign_vote(
    kind: VoteKind,
    height: u64,
    round: u32,
    block_hash: [u8; 32],
    validator_root: &Did,
    device: &Controller,
) -> Vote {
    let transcript = Vote::transcript(kind, height, round, &block_hash);
    let signature = device.sign_message(&transcript);
    Vote {
        kind,
        height,
        round,
        block_hash,
        validator_root: validator_root.clone(),
        validator_device: device.did(),
        signature,
    }
}

/// Verify one vote: the supplied KELs must actually be this vote's claimed
/// root/device, the device must currently be a delegated, unrevoked,
/// `VOTE`-capable device of that root, and the signature must verify over
/// the exact (kind, height, round, block_hash) transcript.
pub fn verify_vote(vote: &Vote, root_kel: &Kel, device_kel: &Kel) -> Result<()> {
    if device_kel.did().as_str() != vote.validator_device.as_str() {
        return Err(ChainError::DeviceMismatch);
    }
    if root_kel.did().as_str() != vote.validator_root.as_str() {
        return Err(ChainError::DeviceMismatch);
    }
    let caps = verify_delegation(root_kel, device_kel)?;
    if !caps.contains(Capabilities::VOTE) {
        return Err(ChainError::MissingVoteCapability);
    }
    let transcript = Vote::transcript(vote.kind, vote.height, vote.round, &vote.block_hash);
    device_kel.verify_message(&transcript, &vote.signature)?;
    Ok(())
}
