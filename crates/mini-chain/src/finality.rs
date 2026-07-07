//! Quorum certificates and finality verification: `>2/3` of the validator
//! set's *distinct identity roots* precommitting the same block at the same
//! height/round — the guarantee that gives a Tendermint/CometBFT-style BFT
//! chain instant, deterministic finality (adapted per Founder Decision A1,
//! `docs/DECISION_LOG.md` D-0008).
//!
//! This module is the finality **math**, not the networked protocol: real
//! consensus (proposer rotation, round timeouts/view-change, vote gossip,
//! state-machine application) is `pending`. Given a set of votes, this
//! module answers one question precisely and offline: does this block have
//! enough distinct, currently-valid validator signatures to be final? —
//! exactly the same relationship `mini-forge`'s attestation-counting has to
//! the eventual chain.

use std::collections::HashSet;

use did_mini::{Did, Kel};

use crate::error::{ChainError, Result};
use crate::validator::{ValidatorSet, MAX_VALIDATORS};
use crate::vote::{verify_vote, Vote, VoteKind};

/// Hard cap on votes accepted into one certificate-verification pass: an
/// allocation/CPU bound applied *before* any signature verification, so a
/// certificate stuffed with junk votes (duplicates, wrong phase/height,
/// non-validators) cannot force unbounded crypto work. A well-formed
/// certificate has at most one relevant vote per validator; this generous
/// multiple leaves room for duplicates/noise without being unbounded.
pub const MAX_VOTES_PER_CERTIFICATE: usize = 4 * MAX_VALIDATORS;

/// Supplies verified KELs for validator roots and devices.
///
/// Deliberately the same shape as `mini-forge::IdentityOracle`, defined
/// locally rather than imported: `mini-chain` is a lower layer other crates
/// (eventually `mini-forge`'s release registry) anchor *to*, so it must not
/// depend back on them.
pub trait ValidatorOracle {
    /// The verified KEL for `did`, if this oracle vouches for it.
    fn kel(&self, did: &Did) -> Option<&Kel>;
}

/// A candidate quorum certificate: a block plus the votes claiming to
/// finalize it. Not yet trusted — [`verify_finality`] is what checks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuorumCertificate {
    /// The height being certified.
    pub height: u64,
    /// The round the quorum formed in.
    pub round: u32,
    /// The block hash being certified.
    pub block_hash: [u8; 32],
    /// The votes offered as this certificate's evidence.
    pub votes: Vec<Vote>,
}

/// Verify a quorum certificate against the *current* validator set.
///
/// A vote counts toward the quorum only if **all** of the following hold:
/// - it is a [`VoteKind::Precommit`] (Prevotes never finalize);
/// - it names exactly this certificate's `height`/`round`/`block_hash` (a
///   vote for anything else — a different block, an earlier round — is
///   simply not evidence for *this* certificate);
/// - its claimed validator root is a member of `validators` right now;
/// - it re-verifies: the device is a currently-delegated, unrevoked,
///   `VOTE`-capable device of that root, and the signature is valid.
///
/// One identity root counts **at most once**, however many of its devices
/// submitted votes (P2) — capability scoping and delegation can only narrow
/// who may act, never multiply a root's standing. Returns the number of
/// distinct validators counted on success, or [`ChainError::QuorumNotMet`]
/// if that count does not exceed 2/3 of the validator set.
pub fn verify_finality(
    qc: &QuorumCertificate,
    validators: &ValidatorSet,
    oracle: &dyn ValidatorOracle,
) -> Result<usize> {
    if qc.votes.len() > MAX_VOTES_PER_CERTIFICATE {
        return Err(ChainError::LimitExceeded);
    }
    let mut counted: HashSet<String> = HashSet::new();
    for vote in &qc.votes {
        if vote.kind != VoteKind::Precommit {
            continue;
        }
        if vote.height != qc.height || vote.round != qc.round || vote.block_hash != qc.block_hash {
            continue;
        }
        if !validators.contains(&vote.validator_root) {
            continue;
        }
        let scid = vote.validator_root.scid().to_string();
        if counted.contains(&scid) {
            continue;
        }
        let root_kel = match oracle.kel(&vote.validator_root) {
            Some(k) => k,
            None => continue,
        };
        let device_kel = match oracle.kel(&vote.validator_device) {
            Some(k) => k,
            None => continue,
        };
        if verify_vote(vote, root_kel, device_kel).is_err() {
            continue;
        }
        counted.insert(scid);
    }

    let needed = validators.quorum_threshold();
    if counted.len() < needed {
        return Err(ChainError::QuorumNotMet {
            needed,
            got: counted.len(),
        });
    }
    Ok(counted.len())
}
