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
