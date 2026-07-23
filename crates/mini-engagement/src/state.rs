//! The engagement: an escrowed unit of work, and the state it moves through.

use did_mini::Did;
use mini_objects::ObjectId;
use mini_settlement::PaymentClaim;

/// Which side of an engagement raised a dispute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Party {
    Payer,
    Performer,
}

/// Deliberately generic. There is no `CardIssuance` variant, no `Courier`
/// variant, no per-industry logic (FD-18 non-negotiable #10) -- every edge
/// service that needs escrowed work uses this same state shape.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EngagementState {
    Offered,
    Accepted {
        by: Did,
        at_ms: u64,
    },
    /// Partial performance, partial release -- the primitive that makes
    /// MINI a currency rather than a scoreboard.
    Milestone {
        index: u16,
        released_micromini: u64,
    },
    Completed {
        at_ms: u64,
    },
    Disputed {
        raised_by: Party,
        arbiters: Vec<Did>,
    },
    /// No counterparty cooperation required -- the default good outcome
    /// when a provider disappears mid-engagement (FD-02, FD-06).
    TimedOut {
        refunded_to: Did,
    },
}

impl EngagementState {
    /// `true` once the engagement can never transition again.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            EngagementState::Completed { .. } | EngagementState::TimedOut { .. }
        )
    }
}

/// One escrowed engagement: the general work primitive FD-18 Part II.2
/// builds settlement on top of.
///
/// `escrow_claim` is a real, already-signed `mini_settlement::PaymentClaim`
/// -- FD-05 applies unchanged here: **a signed promise is never final
/// ownership.** This type never invents ownership; it only tracks how much
/// of that claim's amount has been released so far and through which
/// state transitions. [`crate::settlement::canonical_completion_status`]
/// reconciles `escrow_claim` against a real `CanonicalLedgerView` so a
/// caller can tell a locally-recorded release from a canonically settled
/// one -- this crate is the state machine plus that read-only bridge, not
/// a settlement executor that submits anything toward consensus itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engagement {
    /// Content id of the engagement's terms document.
    pub terms: ObjectId,
    pub payer: Did,
    pub performer: Did,
    pub escrow_claim: PaymentClaim,
    /// If `state` has not reached a terminal variant by this device-clock
    /// time, [`crate::transitions::timeout`] refunds the unreleased
    /// balance to `payer`. Every state has this edge -- FD-18 Part II.2's
    /// first obligation: "a provider that disappears cannot strand
    /// funds."
    pub deadline_ms: u64,
    /// Running total already released across every `Milestone` transition
    /// so far. Never exceeds `escrow_claim.amount_micro` --
    /// [`crate::transitions::release_milestone`] enforces this.
    pub released_micromini: u64,
    pub state: EngagementState,
}

impl Engagement {
    /// Offer a new engagement. Starts in [`EngagementState::Offered`] with
    /// nothing released yet.
    pub fn offer(
        terms: ObjectId,
        payer: Did,
        performer: Did,
        escrow_claim: PaymentClaim,
        deadline_ms: u64,
    ) -> Self {
        Engagement {
            terms,
            payer,
            performer,
            escrow_claim,
            deadline_ms,
            released_micromini: 0,
            state: EngagementState::Offered,
        }
    }

    /// How much of the escrowed claim has not yet been released.
    pub fn remaining_micromini(&self) -> u64 {
        self.escrow_claim
            .amount_micro
            .saturating_sub(self.released_micromini)
    }
}
