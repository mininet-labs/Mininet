//! M1/M2/M3 for private payments.
//!
//! The invariants do not relax because a payment is shielded. What changes
//! is only the key the canonical ledger is asked about: `(payer, sequence)`
//! becomes the key image.
//!
//! - **M1 — money does not merge.** There is no function here that combines
//!   two claims. A conflicting claim is
//!   [`SettlementState::RejectedConflict`], full stop.
//! - **M2 — offline is never final.** [`reconcile`] returns
//!   `mini_settlement`'s own [`SettlementState`], so a wallet has exactly
//!   one vocabulary for pending / accepted-locally / finalized rather than
//!   a private-payment dialect of it. `is_final()` is true only for
//!   `Finalized`.
//! - **M3 — canonical ordering alone decides.** Conflicts resolve by asking
//!   a [`PrivateLedgerView`], never by local preference, arrival order, or
//!   amount.
//!
//! Reusing `mini-settlement`'s state enum rather than defining a parallel
//! one is deliberate. Two enums meaning almost the same thing is how a
//! wallet ends up rendering a private payment as final under rules the
//! transparent path would have called pending.

use mini_settlement::{CanonicalRejection, SettlementState};

use crate::claim::VerifiedPrivateClaim;
use crate::error::Result;

/// The canonical ledger, as a private payment needs to see it.
///
/// Deliberately narrow: two questions, one keyed on a key image and one on
/// a claim digest, and nothing that would require the ledger to reveal
/// amounts or recipients to answer. A multi-input claim asks the first
/// question once per input — see [`reconcile`] for why every input has to
/// be asked about rather than one standing in for the rest.
pub trait PrivateLedgerView {
    /// The digest of the claim canonically finalized against `key_image`,
    /// if any.
    fn finalized_claim(&self, key_image: &[u8]) -> Option<[u8; 32]>;

    /// A canonical rejection recorded for this exact claim digest.
    fn rejected_claim(&self, _digest: &[u8; 32]) -> Option<CanonicalRejection> {
        None
    }
}

/// Reconcile a verified private claim against the canonical ledger.
///
/// `now_ms` is a device clock; expiry is therefore a local judgement, the
/// same honest limitation `mini_settlement::reconcile` carries. A claim is
/// only reported [`SettlementState::Expired`] when the ledger has not
/// finalized it — a finalized claim stays finalized regardless of what any
/// device's clock now says, because value that moved cannot un-move.
pub fn reconcile(
    claim: &VerifiedPrivateClaim,
    ledger: &impl PrivateLedgerView,
    now_ms: u64,
) -> Result<SettlementState> {
    let digest = *claim.transcript_digest();

    if let Some(reason) = ledger.rejected_claim(&digest) {
        return Ok(SettlementState::RejectedCanonical(reason));
    }

    // **Every** input is asked about, not just the first. A multi-input
    // claim conflicts if *any* of its outputs was already spent by a
    // different claim, and the overlap need not be at the same position:
    // claim A spending {X, Y} and claim B spending {Z, Y} conflict on Y
    // alone. Asking about one input would report B as merely pending and
    // let a wallet show a double-spend as awaiting inclusion.
    //
    // A conflict on any input therefore beats a finalization on any other.
    // The claim is all-or-nothing -- there is no state in which some of a
    // claim's inputs moved and the rest did not, and inventing one would be
    // the merge M1 forbids, spelled differently.
    let mut finalized_here = false;
    for key_image in claim.key_images() {
        match ledger.finalized_claim(key_image) {
            // The ledger finalized a *different* claim against this key
            // image. M3 in action -- this claim loses outright. It is never
            // merged, netted, retried, or partially honored.
            Some(finalized) if finalized != digest => {
                return Ok(SettlementState::RejectedConflict);
            }
            Some(_) => finalized_here = true,
            None => {}
        }
    }

    if finalized_here {
        // Value moved. A claim the ledger finalized against one of its
        // inputs is final even if the ledger has not recorded the rest --
        // partial finalization is not a state this vocabulary has, and
        // reporting it as pending would let value move twice.
        return Ok(SettlementState::Finalized);
    }
    if claim.claim().valid_until_ms < now_ms {
        return Ok(SettlementState::Expired);
    }
    Ok(SettlementState::PendingCanonical)
}

/// A [`PrivateLedgerView`] backed by whatever the canonical chain actually
/// finalized, supplied as a plain lookup closure.
///
/// # Why a closure and not a trait the chain implements
///
/// The chain-side state lives in `mini-execution`, which depends on
/// `mini-chain`. This crate depends on `mini-value`. A dependency edge
/// between the two — in either direction, for either crate to name the
/// other's trait — would be the first path in this tree from a value crate
/// to the crate that counts votes, and the voice/value wall (P1, Directive
/// 16) forbids it. Not "discourages": there is no such path today, and this
/// is exactly the change that would create one.
///
/// So the two halves meet through `(Vec<u8>, [u8; 32])` — a key image and a
/// claim digest, standard-library types, no shared crate and no shared
/// format either side could drift from. The wiring is one line at the call
/// site:
///
/// ```ignore
/// // in an application that legitimately sees both layers
/// let ledger = ChainBackedPrivateLedger::new(|key_image| {
///     chain.state().finalized_nullifier(key_image)
/// });
/// let state = reconcile(&claim, &ledger, now_ms)?;
/// ```
///
/// The wall is doing its job here rather than getting in the way: the chain
/// finalizes *opaque facts about which output was spent first*, and never
/// needs to verify a range proof or a ring signature to make progress.
///
/// # What this does not check
///
/// That some valid claim produced the key image the chain finalized. The
/// chain cannot check it — the cryptography is on the other side of the
/// wall — so a Byzantine proposer can finalize a key image nobody proved,
/// burning an output that is not theirs. Reconciliation reports what the
/// canonical ledger says; making the ledger's *contents* trustworthy is a
/// validity-rule question for the consensus layer, and it is open. See
/// `mini_execution::nullifier`'s own docs, which state the same limit from
/// the other side.
pub struct ChainBackedPrivateLedger<F, R = fn(&[u8; 32]) -> Option<CanonicalRejection>> {
    finalized: F,
    rejected: Option<R>,
}

impl<F> ChainBackedPrivateLedger<F>
where
    F: Fn(&[u8]) -> Option<[u8; 32]>,
{
    /// Read finality from `finalized`, with no canonical-rejection source.
    ///
    /// `reconcile` then never returns
    /// [`SettlementState::RejectedCanonical`], which is honest rather than
    /// lossy: a chain that records no rejection reasons has none to report,
    /// and inventing one would be worse than its absence.
    pub fn new(finalized: F) -> Self {
        ChainBackedPrivateLedger {
            finalized,
            rejected: None,
        }
    }
}

impl<F, R> ChainBackedPrivateLedger<F, R>
where
    F: Fn(&[u8]) -> Option<[u8; 32]>,
    R: Fn(&[u8; 32]) -> Option<CanonicalRejection>,
{
    /// Read finality from `finalized` and canonical rejections from
    /// `rejected`.
    pub fn with_rejections(finalized: F, rejected: R) -> Self {
        ChainBackedPrivateLedger {
            finalized,
            rejected: Some(rejected),
        }
    }
}

impl<F, R> core::fmt::Debug for ChainBackedPrivateLedger<F, R> {
    /// The closures are opaque, and what they close over may be an entire
    /// ledger; printing it here would be a surprising amount of state
    /// arriving through a `{:?}`.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ChainBackedPrivateLedger")
            .field("rejections", &self.rejected.is_some())
            .finish_non_exhaustive()
    }
}

impl<F, R> PrivateLedgerView for ChainBackedPrivateLedger<F, R>
where
    F: Fn(&[u8]) -> Option<[u8; 32]>,
    R: Fn(&[u8; 32]) -> Option<CanonicalRejection>,
{
    fn finalized_claim(&self, key_image: &[u8]) -> Option<[u8; 32]> {
        (self.finalized)(key_image)
    }

    fn rejected_claim(&self, digest: &[u8; 32]) -> Option<CanonicalRejection> {
        self.rejected.as_ref().and_then(|lookup| lookup(digest))
    }
}

/// A trivial in-memory [`PrivateLedgerView`] for tests and local
/// experiments.
///
/// **Not a ledger.** It finalizes whatever it is told to finalize, with no
/// consensus behind it. Production needs a chain-execution-backed
/// implementation the way `mini_execution::LedgerChain` backs
/// `mini_settlement::CanonicalLedgerView`; nothing here should be mistaken
/// for one.
#[derive(Debug, Default)]
pub struct InMemoryPrivateLedger {
    finalized: std::collections::BTreeMap<Vec<u8>, [u8; 32]>,
    rejected: std::collections::BTreeMap<[u8; 32], CanonicalRejection>,
}

impl InMemoryPrivateLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that the canonical ledger finalized `claim`.
    ///
    /// Every input is recorded, not just the first: a later claim
    /// overlapping on any one of them must be detectable as a conflict.
    pub fn finalize(&mut self, claim: &VerifiedPrivateClaim) {
        for key_image in claim.key_images() {
            self.finalized
                .insert(key_image.to_vec(), *claim.transcript_digest());
        }
    }

    /// Record a canonical rejection for an exact claim digest.
    pub fn reject(&mut self, digest: [u8; 32], reason: CanonicalRejection) {
        self.rejected.insert(digest, reason);
    }
}

impl PrivateLedgerView for InMemoryPrivateLedger {
    fn finalized_claim(&self, key_image: &[u8]) -> Option<[u8; 32]> {
        self.finalized.get(key_image).copied()
    }

    fn rejected_claim(&self, digest: &[u8; 32]) -> Option<CanonicalRejection> {
        self.rejected.get(digest).copied()
    }
}
