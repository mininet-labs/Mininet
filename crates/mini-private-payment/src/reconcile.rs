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
/// Deliberately narrow: three questions keyed on the key image, and
/// nothing that would require the ledger to reveal amounts or recipients
/// to answer.
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

    match ledger.finalized_claim(claim.key_image()) {
        // The ledger finalized exactly this claim: value moved.
        Some(finalized) if finalized == digest => Ok(SettlementState::Finalized),
        // The ledger finalized a *different* claim against this key image.
        // M3 in action -- this claim loses outright. It is never merged,
        // netted, retried, or partially honored.
        Some(_) => Ok(SettlementState::RejectedConflict),
        None if claim.claim().valid_until_ms < now_ms => Ok(SettlementState::Expired),
        None => Ok(SettlementState::PendingCanonical),
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
    pub fn finalize(&mut self, claim: &VerifiedPrivateClaim) {
        self.finalized
            .insert(claim.key_image().to_vec(), *claim.transcript_digest());
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
