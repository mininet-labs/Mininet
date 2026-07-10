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
//!
//! ## Batch 3 additions (D-0066, `docs/design/self-hosted-forge-spine.md`)
//!
//! Three gates the design doc named as missing from the adoption path,
//! layered in front of [`mini_forge::verify_governed_release`] rather than
//! inside it (`mini-forge`'s verification is stateless; these gates need
//! this crate's own device-local state or a second crate):
//!
//! - **Freshness** ([`FreshnessPolicy`]): refuses an adoption decision if
//!   the device's own view of the network is too stale, an explicit,
//!   caller-supplied check rather than a hidden wall-clock comparison --
//!   TUF's timestamp-role freeze-attack defense, adapted: there is no
//!   separately signed "as of now" metadata object here, since the thing
//!   being bounded is how recently *this device* last synced, not a
//!   repository's own claim of currency.
//! - **Rollback protection** ([`mini_forge::check_no_rollback`]): refuses a
//!   candidate whose version is not strictly greater than whatever this
//!   device is currently running.
//! - **Independent build-provenance quorum** ([`ProvenancePolicy`]):
//!   optional defense-in-depth requiring `mini-provenance`'s
//!   `independent_agreement` over the release's source commit to meet a
//!   threshold, alongside `mini-forge`'s existing release-attestation
//!   quorum -- two independently-computed distinct-identity-root counts
//!   rather than one.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use mini_forge::{
    check_no_rollback, release_version, verify_governed_release, ForgeError, IdentityOracle,
    ReleasePolicy, VerifiedRelease,
};
use mini_objects::ObjectId;
use mini_provenance::independent_agreement;
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
    /// malformed object, rollback). Re-evaluating without new facts will
    /// not change this.
    Rejected(ForgeError),
    /// The device's own view of the network is too stale to trust for an
    /// adoption decision — sync first, then re-evaluate. Distinct from
    /// [`NotYetReason`] because re-evaluating with the same stale
    /// `last_synced_ms` will not change the answer; the device must
    /// actually sync.
    ViewTooStale {
        last_synced_ms: u64,
        max_staleness_ms: u64,
    },
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
        /// Found.
        got: u32,
    },
    /// The artifact is incomplete or unverified locally yet (still syncing
    /// chunks, or the digest does not match what's assembled so far).
    ArtifactUnavailable,
    /// Too few independent build-provenance agreements so far (see
    /// [`ProvenancePolicy`]).
    NotEnoughProvenanceAgreement { needed: u32, got: u32 },
}

/// Why [`AdoptionState::adopt`] refused to adopt. Wraps [`ForgeError`] for
/// every gate `mini-forge` itself enforces, plus the one gate this crate
/// enforces on its own (staleness) that `mini-forge`'s stateless
/// verification has no way to know about.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdoptError {
    /// The device's own view of the network is too stale to trust (see
    /// [`AdoptionDecision::ViewTooStale`]).
    ViewTooStale {
        last_synced_ms: u64,
        max_staleness_ms: u64,
    },
    /// Any gate `mini-forge` itself enforces (timelock, attestations,
    /// governance, artifact completeness, rollback) failed.
    Forge(ForgeError),
}

impl From<ForgeError> for AdoptError {
    fn from(e: ForgeError) -> Self {
        AdoptError::Forge(e)
    }
}

impl core::fmt::Display for AdoptError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AdoptError::ViewTooStale {
                last_synced_ms,
                max_staleness_ms,
            } => write!(
                f,
                "view too stale to adopt: last synced {last_synced_ms}ms ago, max allowed {max_staleness_ms}ms"
            ),
            AdoptError::Forge(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AdoptError {}

/// Freshness policy: how recently this device must have synced before its
/// local view of the world is trusted for an adoption decision.
#[derive(Debug, Clone, Copy)]
pub struct FreshnessPolicy {
    /// Maximum age, in milliseconds, of the device's last sync before an
    /// adoption decision is refused as [`AdoptionDecision::ViewTooStale`].
    pub max_staleness_ms: u64,
}

/// Ceiling: a freshness policy cannot weaken this bound by claiming a
/// staleness window wider than this (30 days, provisional — on-chain
/// governance sets the real value later, downward only, the mirror image
/// of [`mini_forge::ADOPTION_MIN_TIMELOCK_MS`]'s upward-only floor).
pub const FRESHNESS_MAX_ALLOWED_STALENESS_MS: u64 = 30 * 24 * 3_600_000;

impl FreshnessPolicy {
    /// Enforce the freshness ceiling: a caller cannot weaken the check
    /// past [`FRESHNESS_MAX_ALLOWED_STALENESS_MS`].
    pub fn validate(&self) -> Result<(), ForgeError> {
        if self.max_staleness_ms == 0 || self.max_staleness_ms > FRESHNESS_MAX_ALLOWED_STALENESS_MS
        {
            return Err(ForgeError::BadObject);
        }
        Ok(())
    }

    fn check(&self, now_ms: u64, last_synced_ms: u64) -> Option<AdoptionDecision> {
        if now_ms.saturating_sub(last_synced_ms) > self.max_staleness_ms {
            return Some(AdoptionDecision::ViewTooStale {
                last_synced_ms,
                max_staleness_ms: self.max_staleness_ms,
            });
        }
        None
    }
}

/// Optional additional gate: require `min_independent_builders` distinct
/// verified identity roots (excluding the release's own author) to have
/// recorded `mini-provenance` build-provenance claims for the release's
/// source commit that agree on the release's artifact digest. Defense in
/// depth alongside `mini-forge`'s existing release-attestation quorum --
/// two independently-computed counts rather than trusting one mechanism
/// alone. Same honest limit as both underlying mechanisms: this counts
/// **distinct identity roots**, not *administratively independent
/// infrastructure* — three containers on one host under keys one person
/// controls are indistinguishable from three real builders.
#[derive(Debug, Clone, Copy)]
pub struct ProvenancePolicy {
    pub min_independent_builders: u32,
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

    /// Check freshness, refusal, and rollback -- the three gates that can
    /// be decided without touching `mini-forge`'s governance verification
    /// at all. Returns `Some(decision)` if any of them already settles the
    /// outcome.
    fn pre_gates<B: Backend>(
        &self,
        store: &Store<B>,
        candidate: &ObjectId,
        policy: &ReleasePolicy,
        freshness: &FreshnessPolicy,
        last_synced_ms: u64,
    ) -> Option<AdoptionDecision> {
        if let Some(stale) = freshness.check(policy.now_ms, last_synced_ms) {
            return Some(stale);
        }
        if self
            .refused
            .iter()
            .any(|r| r.as_str() == candidate.as_str())
        {
            return Some(AdoptionDecision::Refused);
        }
        if let Some(running) = &self.running {
            let running_version = match release_version(store, running) {
                Ok(v) => v,
                Err(e) => return Some(AdoptionDecision::Rejected(e)),
            };
            let candidate_version = match release_version(store, candidate) {
                Ok(v) => v,
                Err(e) => return Some(AdoptionDecision::Rejected(e)),
            };
            if let Err(e) = check_no_rollback(Some(&running_version), &candidate_version) {
                return Some(AdoptionDecision::Rejected(e));
            }
        }
        None
    }

    /// Evaluate whether `candidate` could be adopted right now. A pure
    /// query: it never mutates `self` and never touches anything beyond
    /// reading `store`. `last_synced_ms` is the caller's own record of when
    /// it last synced with the network — never inferred from a local
    /// wall clock, which a device could be tricked about.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate<B: Backend>(
        &self,
        store: &Store<B>,
        oracle: &dyn IdentityOracle,
        candidate: &ObjectId,
        project_id: &ObjectId,
        branch: &str,
        policy: &ReleasePolicy,
        freshness: &FreshnessPolicy,
        last_synced_ms: u64,
    ) -> AdoptionDecision {
        if let Some(decision) = self.pre_gates(store, candidate, policy, freshness, last_synced_ms)
        {
            return decision;
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

    /// Like [`Self::evaluate`], but also requires `provenance` agreement
    /// over the release's source commit to meet `provenance_policy`'s
    /// threshold — the optional independent-build-provenance quorum gate.
    /// Only reachable once every other gate has already passed, so a
    /// caller not using this gate at all sees identical behavior to
    /// [`Self::evaluate`].
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_with_provenance<B: Backend>(
        &self,
        store: &Store<B>,
        oracle: &dyn IdentityOracle,
        candidate: &ObjectId,
        project_id: &ObjectId,
        branch: &str,
        policy: &ReleasePolicy,
        freshness: &FreshnessPolicy,
        last_synced_ms: u64,
        provenance_policy: &ProvenancePolicy,
    ) -> AdoptionDecision {
        let decision = self.evaluate(
            store,
            oracle,
            candidate,
            project_id,
            branch,
            policy,
            freshness,
            last_synced_ms,
        );
        let AdoptionDecision::Adoptable(verified) = decision else {
            return decision;
        };
        let source_commit = match release_source_commit(store, candidate) {
            Ok(id) => id,
            Err(e) => return AdoptionDecision::Rejected(e),
        };
        let expected_output = verified.artifact.digest;
        let agreement: u32 = independent_agreement(store, oracle, &source_commit, expected_output)
            .unwrap_or_default();
        if agreement < provenance_policy.min_independent_builders {
            return AdoptionDecision::NotYetAdoptable(NotYetReason::NotEnoughProvenanceAgreement {
                needed: provenance_policy.min_independent_builders,
                got: agreement,
            });
        }
        AdoptionDecision::Adoptable(verified)
    }

    /// Explicitly adopt `candidate` — the device owner's local act.
    /// Re-verifies from scratch rather than trusting a stale
    /// [`AdoptionDecision`], so nothing can "arm" an adoption ahead of time
    /// and have it fire later without a fresh check.
    #[allow(clippy::too_many_arguments)]
    pub fn adopt<B: Backend>(
        &mut self,
        store: &Store<B>,
        oracle: &dyn IdentityOracle,
        candidate: &ObjectId,
        project_id: &ObjectId,
        branch: &str,
        policy: &ReleasePolicy,
        freshness: &FreshnessPolicy,
        last_synced_ms: u64,
    ) -> Result<VerifiedRelease, AdoptError> {
        freshness.validate()?;
        if let Some(AdoptionDecision::ViewTooStale {
            last_synced_ms,
            max_staleness_ms,
        }) = freshness.check(policy.now_ms, last_synced_ms)
        {
            return Err(AdoptError::ViewTooStale {
                last_synced_ms,
                max_staleness_ms,
            });
        }
        if let Some(running) = &self.running {
            let running_version = release_version(store, running)?;
            let candidate_version = release_version(store, candidate)?;
            check_no_rollback(Some(&running_version), &candidate_version)?;
        }
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

fn release_source_commit<B: Backend>(
    store: &Store<B>,
    release_id: &ObjectId,
) -> mini_forge::Result<ObjectId> {
    let rel = store.get(release_id)?;
    rel.links
        .iter()
        .find(|l| l.rel == "commit")
        .map(|l| l.target.clone())
        .ok_or(ForgeError::BadObject)
}
