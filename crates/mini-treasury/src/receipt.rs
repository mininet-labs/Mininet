//! Contribution receipts, and the seam a real cross-chain verifier fills in.
//!
//! Whitepaper §8.2: "Anyone may send Bitcoin or Monero into a community-
//! controlled treasury and receive MINI at a community-governed rate...
//! permissionless protocol, not a sale by anyone." A [`ContributionReceipt`]
//! is the bookkeeping record of that — which external asset, how much, at
//! what rate, minting how much MINI — and is deliberately **not** proof
//! that the external funds actually arrived. That proof requires verifying
//! a real Bitcoin or Monero transaction, which is a real cross-chain
//! verification problem (SPV proofs, confirmation depth, Monero's own
//! privacy properties complicating straightforward verification) — see
//! [`ExternalReceiptOracle`]'s honest limit.

/// Which external asset a contribution brought in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContributionKind {
    /// Bitcoin.
    Bitcoin,
    /// Monero.
    Monero,
}

/// A bookkeeping record of one contribution: an external asset amount,
/// exchanged at a governed rate, for a minted MINI amount. Constructing one
/// of these does **not** mean the external funds were verified as received
/// — see [`ExternalReceiptOracle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContributionReceipt {
    /// Which external asset was contributed.
    pub kind: ContributionKind,
    /// Amount contributed, in the asset's smallest unit (satoshi/piconero).
    pub contributed_units: u64,
    /// The governed rate applied (micro-MINI per contributed unit, see
    /// [`crate::rate::RATE_SCALE`]).
    pub mini_per_unit_micro: u64,
    /// Micro-MINI minted for this contribution
    /// ([`crate::rate::mint_amount_micro`] of the two fields above).
    pub minted_micro: u64,
    /// When the contribution was recorded (ms).
    pub at_ms: u64,
}

impl ContributionReceipt {
    /// Build a receipt, computing the minted amount from the contribution
    /// and rate.
    pub fn new(
        kind: ContributionKind,
        contributed_units: u64,
        mini_per_unit_micro: u64,
        at_ms: u64,
    ) -> Self {
        let minted_micro = crate::rate::mint_amount_micro(contributed_units, mini_per_unit_micro);
        ContributionReceipt {
            kind,
            contributed_units,
            mini_per_unit_micro,
            minted_micro,
            at_ms,
        }
    }
}

/// The seam a real cross-chain verifier fills in: confirmation that a
/// specific Bitcoin or Monero transaction actually paid the treasury
/// address the amount a [`ContributionReceipt`] claims.
///
/// ## Honest limit — do not implement this without a human engineer
///
/// Verifying an external chain's transaction correctly (confirmation depth,
/// reorg safety, and for Monero specifically the view-key/output-scanning
/// machinery its privacy design requires) is real, security-critical
/// cross-chain engineering — the same risk class as treasury custody itself
/// (whitepaper §11: "bridge and treasury custody is a permanent honeypot by
/// nature"). D-0035 point 5 extends the whitepaper's human-authorship and
/// external-audit requirement to this component. [`NoExternalReceiptOracle`]
/// is the only implementation here, and it is the correct, permanent
/// choice — treating a contribution as verified without it would be
/// treating an unverified claim as real money.
pub trait ExternalReceiptOracle {
    /// Whether the given receipt's external transaction has been
    /// independently confirmed on its source chain. `Ok(false)` (not yet
    /// confirmed) is a normal, unremarkable outcome; this trait makes no
    /// claim about *when* a real implementation would return `true`.
    fn is_confirmed(&mut self, receipt: &ContributionReceipt) -> bool;
}

/// The reference [`ExternalReceiptOracle`]: nothing is ever confirmed,
/// because no real cross-chain verifier exists here. Every deployment is a
/// `NoExternalReceiptOracle` deployment until the human-authored,
/// externally-audited implementation described above lands.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoExternalReceiptOracle;

impl ExternalReceiptOracle for NoExternalReceiptOracle {
    fn is_confirmed(&mut self, _receipt: &ContributionReceipt) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_computes_minted_amount_from_rate() {
        let receipt = ContributionReceipt::new(
            ContributionKind::Bitcoin,
            1_000,
            crate::rate::RATE_SCALE * 2,
            5_000,
        );
        assert_eq!(receipt.minted_micro, 2_000);
    }

    #[test]
    fn no_external_receipt_oracle_never_confirms() {
        let mut oracle = NoExternalReceiptOracle;
        let receipt =
            ContributionReceipt::new(ContributionKind::Monero, 1_000, crate::rate::RATE_SCALE, 0);
        assert!(!oracle.is_confirmed(&receipt));
    }
}
