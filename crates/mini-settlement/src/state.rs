//! The settlement state machine — the wallet-facing vocabulary M2 requires.
//!
//! Invariant **M2** (`docs/INVARIANTS.md` §4, D-0045): *"Offline/local
//! payment is never final. It is a signed pending claim until canonical
//! chain inclusion; wallets must distinguish pending / accepted /
//! finalized."* D-0045's own failure point names the exact risk this file
//! exists to close: a wallet UI that shows "accepted" without visibly
//! distinguishing it from canonical finality lets a user reasonably
//! believe a transaction is settled when it is not. [`SettlementState`]
//! makes that distinction a type, not a UI convention — and
//! [`SettlementState::is_final`] is the one function any wallet or
//! merchant-facing code should ever call to decide "is this money mine."

/// Where a [`crate::PaymentClaim`] stands, from first signature to
/// canonical resolution. There is deliberately no "merged" or "reconciled"
/// state — **M1** (`docs/INVARIANTS.md` §4) forbids money from ever
/// CRDT-merging, so the state machine has no operation that could produce
/// one: a claim is Finalized, or it is not, and two conflicting claims are
/// never combined into a third answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementState {
    /// Signed, not yet shown to anyone.
    SignedLocal,
    /// A recipient has locally decided to treat this claim as plausible
    /// enough to act on (e.g. release goods) *before* canonical
    /// finality — a risk decision the recipient made, not a fact about
    /// the claim. **Never final** — see [`SettlementState::is_final`].
    AcceptedLocal,
    /// Submitted toward canonical inclusion; the canonical ledger has not
    /// yet resolved this claim's `(payer, nonce)` slot one way or another.
    PendingCanonical,
    /// The canonical ledger has included **exactly this claim** at its
    /// `(payer, nonce)` slot. This is the only state where value has
    /// actually moved.
    Finalized,
    /// The canonical ledger resolved this claim's `(payer, nonce)` slot
    /// with a **different** claim — M3's canonical-ordering rule in
    /// action. This claim is rejected outright; it is never merged,
    /// retried, or partially honored.
    RejectedConflict,
    /// `valid_until_ms` passed before the claim reached canonical
    /// inclusion, and it was never referenced by anything the canonical
    /// ledger finalized.
    Expired,
}

impl SettlementState {
    /// The single question that matters: has value actually, irreversibly
    /// moved? `true` only for [`SettlementState::Finalized`] — in
    /// particular, `false` for [`SettlementState::AcceptedLocal`], which
    /// exists precisely to be distinguishable from this.
    pub const fn is_final(self) -> bool {
        matches!(self, SettlementState::Finalized)
    }

    /// The three-way wallet vocabulary M2 names explicitly: `pending`,
    /// `accepted`, `finalized`. A UI should render from this, not from
    /// [`SettlementState`] directly, so the "accepted-but-not-final"
    /// distinction can never be flattened away by an ad hoc match
    /// somewhere in client code.
    pub const fn wallet_label(self) -> WalletLabel {
        match self {
            SettlementState::SignedLocal | SettlementState::PendingCanonical => {
                WalletLabel::Pending
            }
            SettlementState::AcceptedLocal => WalletLabel::AcceptedNotFinal,
            SettlementState::Finalized => WalletLabel::Finalized,
            SettlementState::RejectedConflict => WalletLabel::Rejected,
            SettlementState::Expired => WalletLabel::Expired,
        }
    }
}

/// The wallet-facing label a client should actually render. Deliberately a
/// smaller vocabulary than [`SettlementState`] — a wallet doesn't need to
/// distinguish *why* something is pending, only that it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletLabel {
    /// Not yet resolved either way. Do not treat as received.
    Pending,
    /// A recipient's own risk decision to proceed before finality. Must be
    /// visually distinct from `Finalized` in any UI — this is the exact
    /// distinction D-0045's failure point calls out by name.
    AcceptedNotFinal,
    /// Canonically final. Value has moved.
    Finalized,
    /// Lost to a conflicting claim at the same `(payer, nonce)`.
    Rejected,
    /// Timed out before resolution.
    Expired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_finalized_is_final() {
        assert!(SettlementState::Finalized.is_final());
        assert!(!SettlementState::SignedLocal.is_final());
        assert!(!SettlementState::AcceptedLocal.is_final());
        assert!(!SettlementState::PendingCanonical.is_final());
        assert!(!SettlementState::RejectedConflict.is_final());
        assert!(!SettlementState::Expired.is_final());
    }

    #[test]
    fn accepted_local_is_never_the_finalized_wallet_label() {
        assert_eq!(
            SettlementState::AcceptedLocal.wallet_label(),
            WalletLabel::AcceptedNotFinal
        );
        assert_ne!(
            SettlementState::AcceptedLocal.wallet_label(),
            SettlementState::Finalized.wallet_label()
        );
    }
}
