//! Local update-adoption state machine (`docs/BOOTSTRAP_AND_UPDATE.md`
//! "Update adoption rule" + "No forced updates"): wraps `mini-forge`'s
//! verification gates ([`mini_forge::verify_governed_release`]) with a tiny,
//! local, explicit state machine.
//!
//! ## No forced updates [FREEZE]
//!
//! Nothing in this crate executes, fetches, or installs anything. It only
//! answers "could this be adopted right now" ([`AdoptionState::evaluate`])
//! and records what the device owner explicitly chose
//! ([`AdoptionState::adopt`] / [`AdoptionState::refuse`]). A refusal is a
//! normal, first-class outcome — "the cost of refusal is normal protocol
//! compatibility, not remote control." There is no code path here, or in
//! `mini-forge` beneath it, that can install software without this being
//! called, and calling it never itself executes anything — it only updates
//! which release id this state considers "running."

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use mini_forge::{
    verify_governed_release, ForgeError, IdentityOracle, ReleasePolicy, VerifiedRelease,
};
use mini_objects::ObjectId;
use mini_store::{Backend, Store};

/// The outcome of evaluating a candidate release, without adopting it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdoptionDecision {
    /// Every gate passed; [`AdoptionState::adopt`] would succeed right now.
    Adoptable(VerifiedRelease),
    /// A gate that time or more attestations could still satisfy has not
    /// been met yet (timelock still active, or too few attestations so
    /// far). Not a rejection — re-evaluate later.
    NotYetAdoptable(NotYetReason),
    /// The device owner already explicitly refused this exact release id.
    Refused,
    /// A hard gate failed (governance fork, non-canonical source commit,
    /// malformed object). Re-evaluating without new facts will not change
    /// this.
    Rejected(ForgeError),
}

/// Why a candidate is not yet adoptable — distinguishes "keep watching" from
/// "something is wrong."
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotYetReason {
    /// The timelock has not elapsed.
    TimelockActive,
    /// Too few independent attestations so far.
    NotEnoughAttestations {
        /// Required distinct attesting identity roots.
        needed: u32,
        /// Found so far.
        got: u32,
    },
    /// The artifact is incomplete or unverified locally yet (still syncing
    /// chunks, or the digest does not match what's assembled so far).
    ArtifactUnavailable,
}

/// One device's local update-adoption state: what it is running, and what it
/// has explicitly refused. Nothing here is shared, signed, or synced — it is
/// purely local bookkeeping, the device owner's own record of their own
/// choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptionState {
    /// The release id this device currently considers itself running, if it
    /// has bootstrapped/adopted anything yet.
    pub running: Option<ObjectId>,
    /// Release ids this device has explicitly refused to adopt.
    pub refused: Vec<ObjectId>,
}

impl AdoptionState {
    /// A fresh device that has not adopted anything yet.
    pub fn new() -> Self {
        AdoptionState {
            running: None,
            refused: Vec::new(),
        }
    }

    /// Evaluate whether `candidate` could be adopted right now. A pure
    /// query: it never mutates `self` and never touches anything beyond
    /// reading `store`.
    pub fn evaluate<B: Backend>(
        &self,
        store: &Store<B>,
        oracle: &dyn IdentityOracle,
        candidate: &ObjectId,
        project_id: &ObjectId,
        branch: &str,
        policy: &ReleasePolicy,
    ) -> AdoptionDecision {
        if self
            .refused
            .iter()
            .any(|r| r.as_str() == candidate.as_str())
        {
            return AdoptionDecision::Refused;
        }
        match verify_governed_release(store, oracle, candidate, project_id, branch, policy) {
            Ok(verified) => AdoptionDecision::Adoptable(verified),
            Err(ForgeError::TimelockActive) => {
                AdoptionDecision::NotYetAdoptable(NotYetReason::TimelockActive)
            }
            Err(ForgeError::NotEnoughAttestations { needed, got }) => {
                AdoptionDecision::NotYetAdoptable(NotYetReason::NotEnoughAttestations {
                    needed,
                    got,
                })
            }
            Err(ForgeError::ArtifactUnavailable) => {
                AdoptionDecision::NotYetAdoptable(NotYetReason::ArtifactUnavailable)
            }
            Err(e) => AdoptionDecision::Rejected(e),
        }
    }

    /// Explicitly adopt `candidate` — the device owner's local act.
    /// Re-verifies from scratch rather than trusting a stale
    /// [`AdoptionDecision`], so nothing can "arm" an adoption ahead of time
    /// and have it fire later without a fresh check.
    pub fn adopt<B: Backend>(
        &mut self,
        store: &Store<B>,
        oracle: &dyn IdentityOracle,
        candidate: &ObjectId,
        project_id: &ObjectId,
        branch: &str,
        policy: &ReleasePolicy,
    ) -> mini_forge::Result<VerifiedRelease> {
        let verified =
            verify_governed_release(store, oracle, candidate, project_id, branch, policy)?;
        self.running = Some(candidate.clone());
        Ok(verified)
    }

    /// Explicitly refuse `candidate`. The device stays on whatever it is
    /// currently running (or joins a fork) — refusal never blocks ordinary
    /// operation, and a refused id can be reconsidered later by simply not
    /// calling this again (there is deliberately no "unrefuse": choosing to
    /// adopt a previously-refused release is just calling [`Self::adopt`]
    /// directly, which re-checks everything fresh).
    pub fn refuse(&mut self, candidate: &ObjectId) {
        if !self
            .refused
            .iter()
            .any(|r| r.as_str() == candidate.as_str())
        {
            self.refused.push(candidate.clone());
        }
    }
}

impl Default for AdoptionState {
    fn default() -> Self {
        Self::new()
    }
}
