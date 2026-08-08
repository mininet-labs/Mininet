//! What happens to a replica *after* it registers.
//!
//! Registration proves a replica was genuinely sealed once. It says nothing
//! about whether the provider still holds it a month later, and nothing about
//! how much storage that provider may claim to be contributing. Both are
//! separate problems, and both are places where a number can be asserted
//! rather than proven.
//!
//! # The gap this closes
//!
//! `mini_spacetime::MerkleStorageProof::new` used to take `capacity_units`
//! from its caller, with nothing tying that number to the commitment beside
//! it, and `mini_spacetime::proposer_weight` took a bare `u64` while
//! documenting that it "trusts its input completely". A provider could seal a
//! single 32-byte node, register it honestly, and declare a million units.
//!
//! That inverts the thesis the whole storage design rests on. "A thousand
//! cheap, scattered machines outcompete one warehouse" only holds if capacity
//! has to be *proven*; if it can be declared, the cheapest possible node wins
//! by typing a larger number.
//!
//! [`capacity_units_of`] derives capacity from the audited seal and takes no
//! caller figure. **The derived path is now the only path** (D-0448):
//! `proposer_weight` accepts nothing but a
//! [`mini_spacetime::ProvenCapacity`], which has no numeric constructor, and
//! `StorageCommitment::block_size_bytes` is re-checked against the served
//! bytes on every challenge — so the byte total behind those units is a
//! consequence of what a provider actually answered. Previously this crate
//! closed the hole only for callers who opted in, which for an
//! authority-bearing function is the same as leaving it open.
//!
//! # What is still not proven here
//!
//! - **Not a clock.** Windows are computed from caller-supplied milliseconds.
//!   A caller feeding a dishonest clock gets dishonest windows. Anchoring
//!   requires the same witnessed/chain-height evidence the rest of this crate
//!   is waiting on.
//! - **Not liveness.** A missed window means "this verifier saw no proof",
//!   which is indistinguishable from a network partition. That is why lapse
//!   degrades gradually and reversibly rather than punishing on first miss.
//! - **Not a reward.** Nothing here pays anyone, and no crate consumes
//!   [`ProvenCapacity`] to do so. It is a measurement, not an entitlement.

use std::collections::BTreeMap;

use mini_crypto::HashAlgorithm;
use mini_spacetime::StorageChallenge;

use crate::claim::VerifiedReplicaClaim;
use crate::codec::Writer;
use crate::error::{FraudError, Result};
use crate::seal::seal_commitment_digest;

/// Domain separator for per-window challenge derivation.
pub const WINDOW_CHALLENGE_DOMAIN: &[u8] = b"mininet/mini-storage-fraud/window-challenge/v1";

/// Largest number of challenges one window may demand, bounding both the
/// prover's work and the verifier's.
pub const MAX_CHALLENGES_PER_WINDOW: u32 = 1024;

/// Re-exported from `mini-spacetime` rather than redefined here.
///
/// This crate had its own `StorageUnitPolicy`/`ProvenCapacity` pair with the
/// same names and the same meaning. Two types called `ProvenCapacity` is how
/// a caller ends up holding one kind of "proven" and passing it somewhere
/// that means the other — the same reason `mini-private-payment` reuses
/// `mini_settlement::SettlementState` instead of defining a parallel
/// finality enum. The canonical definitions now live one layer down, beside
/// [`mini_spacetime::proposer_weight`], which is the only thing that
/// consumes them.
pub use mini_spacetime::{ProvenCapacity, StorageUnitPolicy};

/// Derive capacity from the audited seal.
///
/// Goes through the claim's own [`mini_spacetime::StorageCommitment`], which
/// is itself derived from the audited seal rather than supplied alongside it
/// — so there is exactly one statement anywhere about how much this replica
/// covers, and it is inside the object a quorum checked.
pub fn capacity_units_of(
    claim: &VerifiedReplicaClaim,
    policy: &StorageUnitPolicy,
) -> ProvenCapacity {
    ProvenCapacity::from_commitment(&claim.storage_commitment(), policy)
}

/// How often a registered replica must prove it still holds what it sealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPolicy {
    window_ms: u64,
    challenges_per_window: u32,
    grace_windows: u32,
}

impl WindowPolicy {
    pub fn new(window_ms: u64, challenges_per_window: u32, grace_windows: u32) -> Result<Self> {
        if window_ms == 0 || challenges_per_window == 0 {
            return Err(FraudError::InvalidPolicy);
        }
        if challenges_per_window > MAX_CHALLENGES_PER_WINDOW {
            return Err(FraudError::InvalidPolicy);
        }
        Ok(Self {
            window_ms,
            challenges_per_window,
            grace_windows,
        })
    }

    /// Daily windows, 32 challenges each, two windows of grace.
    ///
    /// The grace is not leniency about fraud — it is an admission that a
    /// missed window and an unreachable peer look identical from here.
    pub fn daily() -> Self {
        Self {
            window_ms: 86_400_000,
            challenges_per_window: 32,
            grace_windows: 2,
        }
    }

    pub fn window_ms(&self) -> u64 {
        self.window_ms
    }

    pub fn challenges_per_window(&self) -> u32 {
        self.challenges_per_window
    }

    pub fn grace_windows(&self) -> u32 {
        self.grace_windows
    }

    /// Which window `now_ms` falls in, counting from `genesis_ms`.
    pub fn window_at(&self, genesis_ms: u64, now_ms: u64) -> u64 {
        now_ms.saturating_sub(genesis_ms) / self.window_ms
    }
}

/// Where a registered replica stands right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplicaState {
    /// Proving on schedule. Contributes its full derived capacity.
    Active,
    /// Missed at least one window but still inside the grace allowance.
    /// Contributes nothing while degraded — capacity follows proof, not
    /// history — but recovers fully on the next good window.
    Degraded { missed_windows: u32 },
    /// Missed beyond grace. Contributes nothing and does not self-recover;
    /// re-entry means registering again, because a replica nobody has seen
    /// for that long is not distinguishable from one that is gone.
    Suspended,
    /// Withdrawn by the provider. Terminal and voluntary.
    Retired,
}

impl ReplicaState {
    /// Whether capacity counts in this state.
    pub fn counts_capacity(&self) -> bool {
        matches!(self, ReplicaState::Active)
    }
}

/// One registered replica's ongoing obligation and standing.
#[derive(Debug, Clone)]
pub struct ReplicaLifecycle {
    claim: VerifiedReplicaClaim,
    genesis_ms: u64,
    state: ReplicaState,
    /// The window this replica became obligated to prove in. Misses are
    /// counted from here until the first successful window replaces it.
    obligated_from: u64,
    last_proven_window: Option<u64>,
    highest_window_seen: u64,
}

impl ReplicaLifecycle {
    /// Begin tracking a verified claim from `registered_at_ms`.
    ///
    /// Starts `Degraded { missed_windows: 0 }`, not `Active`: registration
    /// proves the replica was sealed, not that it is still held. The first
    /// answered window is what makes it active.
    pub fn begin(
        claim: VerifiedReplicaClaim,
        genesis_ms: u64,
        registered_at_ms: u64,
        policy: &WindowPolicy,
    ) -> Self {
        let window = policy.window_at(genesis_ms, registered_at_ms);
        Self {
            claim,
            genesis_ms,
            state: ReplicaState::Degraded { missed_windows: 0 },
            obligated_from: window,
            last_proven_window: None,
            highest_window_seen: window,
        }
    }

    pub fn claim(&self) -> &VerifiedReplicaClaim {
        &self.claim
    }

    pub fn state(&self) -> ReplicaState {
        self.state
    }

    pub fn last_proven_window(&self) -> Option<u64> {
        self.last_proven_window
    }

    /// The challenges this replica must answer for `window`.
    ///
    /// Leaf indices are derived from the seal digest, the window index, and a
    /// `beacon` the **verifier** supplies. The provider contributes nothing to
    /// the derivation, so it cannot pre-compute which nodes it will be asked
    /// for and keep only those. The beacon must come from somewhere the
    /// provider does not control and must not be reused across windows; a
    /// recent block hash or a fresh verifier nonce both work, and this crate
    /// cannot check that it is either.
    pub fn challenges_for(
        &self,
        window: u64,
        beacon: &[u8],
        policy: &WindowPolicy,
    ) -> Vec<StorageChallenge> {
        let node_count = self.claim.seal().node_count as u64;
        let digest = seal_commitment_digest(self.claim.seal());
        (0..policy.challenges_per_window)
            .map(|index| {
                let mut writer = Writer::new();
                writer.raw(WINDOW_CHALLENGE_DOMAIN);
                writer.raw(&digest);
                writer.u64(window);
                writer.bytes(beacon);
                writer.u32(index);
                let drawn = HashAlgorithm::Blake3.digest(&writer.finish());
                let raw = u64::from_be_bytes(drawn[0..8].try_into().expect("32-byte digest"));
                StorageChallenge {
                    leaf_index: (raw % node_count) as usize,
                }
            })
            .collect()
    }

    /// Record that every challenge for `window` was answered correctly.
    ///
    /// The caller verifies the responses — `mini_porep::respond` produces them
    /// and `mini_spacetime::verify_storage_challenge` checks them against the
    /// replica root this claim carries. This records the outcome and moves the
    /// state machine; it deliberately does not re-verify, so there is exactly
    /// one place that decides whether a response was good.
    pub fn record_proven_window(&mut self, window: u64, policy: &WindowPolicy) -> Result<()> {
        if matches!(self.state, ReplicaState::Retired | ReplicaState::Suspended) {
            return Err(FraudError::ReplicaNotProving);
        }
        if let Some(last) = self.last_proven_window {
            if window <= last {
                // Replaying an already-credited window must not extend a
                // streak or reverse a lapse.
                return Err(FraudError::WindowAlreadyProven);
            }
        }
        self.advance_to(window, policy);
        if matches!(self.state, ReplicaState::Suspended) {
            return Err(FraudError::ReplicaNotProving);
        }
        self.last_proven_window = Some(window);
        self.state = ReplicaState::Active;
        Ok(())
    }

    /// Move the clock forward without a proof, crediting nothing.
    ///
    /// Idempotent for windows already accounted for, so a verifier polling
    /// repeatedly inside one window does not accumulate phantom misses.
    pub fn advance_to(&mut self, window: u64, policy: &WindowPolicy) {
        if window <= self.highest_window_seen {
            return;
        }
        self.highest_window_seen = window;
        if matches!(self.state, ReplicaState::Retired | ReplicaState::Suspended) {
            return;
        }

        // Misses are counted from the last window that counts as satisfied:
        // the most recent proven one, or the registration window if the
        // replica has never proven. Registration itself is not a proof, but
        // demanding one in the very window a replica registered would punish
        // arriving late in a window, so that window is the baseline rather
        // than the first miss.
        let satisfied = self.last_proven_window.unwrap_or(self.obligated_from);
        let missed = window.saturating_sub(satisfied).saturating_sub(1);
        let missed = u32::try_from(missed).unwrap_or(u32::MAX);

        self.state = if missed == 0 {
            self.state
        } else if missed > policy.grace_windows {
            ReplicaState::Suspended
        } else {
            ReplicaState::Degraded {
                missed_windows: missed,
            }
        };
    }

    /// Withdraw voluntarily. Terminal.
    pub fn retire(&mut self) {
        self.state = ReplicaState::Retired;
    }

    /// Capacity this replica currently contributes: its derived figure while
    /// active, nothing otherwise.
    pub fn proven_capacity(&self, units: &StorageUnitPolicy) -> ProvenCapacity {
        if self.state.counts_capacity() {
            capacity_units_of(&self.claim, units)
        } else {
            ProvenCapacity::none()
        }
    }

    pub fn genesis_ms(&self) -> u64 {
        self.genesis_ms
    }
}

/// Every replica one provider is tracking, and what they add up to.
#[derive(Debug, Default)]
pub struct ProviderStanding {
    replicas: BTreeMap<[u8; 32], ReplicaLifecycle>,
}

impl ProviderStanding {
    pub fn new() -> Self {
        Self::default()
    }

    /// Track a replica, keyed by its replica root.
    pub fn track(&mut self, lifecycle: ReplicaLifecycle) {
        self.replicas
            .insert(lifecycle.claim().replica_root(), lifecycle);
    }

    pub fn get_mut(&mut self, replica_root: &[u8; 32]) -> Option<&mut ReplicaLifecycle> {
        self.replicas.get_mut(replica_root)
    }

    pub fn get(&self, replica_root: &[u8; 32]) -> Option<&ReplicaLifecycle> {
        self.replicas.get(replica_root)
    }

    pub fn len(&self) -> usize {
        self.replicas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.replicas.is_empty()
    }

    /// Advance every tracked replica to `window`.
    pub fn advance_to(&mut self, window: u64, policy: &WindowPolicy) {
        for lifecycle in self.replicas.values_mut() {
            lifecycle.advance_to(window, policy);
        }
    }

    /// Total capacity currently proven across every actively-proving replica.
    ///
    /// Saturating: a provider tracking absurdly many replicas cannot wrap this
    /// into a small number.
    /// Keyed by replica root, so no replica is counted twice — the one
    /// caveat [`ProvenCapacity::saturating_add`] cannot enforce itself.
    pub fn proven_capacity(&self, units: &StorageUnitPolicy) -> ProvenCapacity {
        self.replicas
            .values()
            .map(|lifecycle| lifecycle.proven_capacity(units))
            .fold(ProvenCapacity::none(), ProvenCapacity::saturating_add)
    }
}
