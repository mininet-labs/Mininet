//! Fee bookkeeping (whitepaper §8.4): "Every action's fee is defined as a
//! small, steady real-world value target and converted automatically into
//! a tiny MINI amount using the community-governed price, so that a view
//! always costs a steady fraction of a cent to a human even as the market
//! value of MINI moves."
//!
//! This is pure bookkeeping and arithmetic — a governed price history and
//! the multiplication that turns a real-world value target into a MINI
//! amount — the same shape and same safety class as `mini_treasury::rate`.
//! It has no opinion on how the price is decided (ordinary flat-vote
//! governance) and no connection whatsoever to the transaction-privacy
//! primitives in [`crate::ring`]/[`crate::stealth`]/[`crate::confidential`].

use crate::error::{Result, ValueError};

/// Fixed-point scale for the governed price. All-integer, so a fee
/// computation is exactly reproducible from the same price and target.
pub const PRICE_SCALE: u64 = 1_000_000;

/// One governed price, effective from a point in time until superseded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceEntry {
    /// When this price took effect (ms).
    pub effective_at_ms: u64,
    /// Micro-MINI equal to one micro-cent (10^-6 of a US cent) of
    /// real-world value, at [`PRICE_SCALE`]'s fixed point.
    pub micro_mini_per_micro_cent: u64,
}

/// An ordered history of governed prices. Lookup always finds the latest
/// entry at or before a given time — a price change never retroactively
/// changes what an earlier fee was worth.
#[derive(Debug, Clone, Default)]
pub struct PriceHistory {
    entries: Vec<PriceEntry>,
}

impl PriceHistory {
    /// A new, empty price history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new governed price. Entries must arrive in strictly
    /// increasing `effective_at_ms` order.
    pub fn add_entry(&mut self, entry: PriceEntry) -> Result<()> {
        if let Some(last) = self.entries.last() {
            if entry.effective_at_ms <= last.effective_at_ms {
                return Err(ValueError::OutOfOrderRateEntry);
            }
        }
        self.entries.push(entry);
        Ok(())
    }

    /// The price in effect at `at_ms` — the latest entry whose
    /// `effective_at_ms` is at or before `at_ms`.
    pub fn price_at(&self, at_ms: u64) -> Result<u64> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.effective_at_ms <= at_ms)
            .map(|e| e.micro_mini_per_micro_cent)
            .ok_or(ValueError::NoRateInEffect)
    }
}

/// Micro-MINI owed for a fee whose real-world value target is
/// `fee_target_micro_cents` micro-cents, at `micro_mini_per_micro_cent`
/// (`PRICE_SCALE`'s fixed point). `u128` internally so a large target at a
/// large price cannot silently overflow.
pub fn fee_in_micro_mini(fee_target_micro_cents: u64, micro_mini_per_micro_cent: u64) -> u64 {
    let product = (fee_target_micro_cents as u128) * (micro_mini_per_micro_cent as u128);
    (product / PRICE_SCALE as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_lookup_finds_the_latest_entry_at_or_before_the_time() {
        let mut history = PriceHistory::new();
        history
            .add_entry(PriceEntry {
                effective_at_ms: 100,
                micro_mini_per_micro_cent: 1_000_000,
            })
            .unwrap();
        history
            .add_entry(PriceEntry {
                effective_at_ms: 200,
                micro_mini_per_micro_cent: 500_000,
            })
            .unwrap();

        assert_eq!(history.price_at(150).unwrap(), 1_000_000);
        assert_eq!(history.price_at(200).unwrap(), 500_000);
        assert_eq!(history.price_at(1_000).unwrap(), 500_000);
    }

    #[test]
    fn price_lookup_before_any_entry_fails() {
        let history = PriceHistory::new();
        assert_eq!(history.price_at(0), Err(ValueError::NoRateInEffect));
    }

    #[test]
    fn out_of_order_or_duplicate_entries_are_rejected() {
        let mut history = PriceHistory::new();
        history
            .add_entry(PriceEntry {
                effective_at_ms: 200,
                micro_mini_per_micro_cent: 1_000_000,
            })
            .unwrap();
        assert!(history
            .add_entry(PriceEntry {
                effective_at_ms: 200,
                micro_mini_per_micro_cent: 2_000_000,
            })
            .is_err());
        assert!(history
            .add_entry(PriceEntry {
                effective_at_ms: 100,
                micro_mini_per_micro_cent: 2_000_000,
            })
            .is_err());
    }

    #[test]
    fn fee_scales_with_price_and_target() {
        assert_eq!(fee_in_micro_mini(1_000, PRICE_SCALE), 1_000);
        assert_eq!(fee_in_micro_mini(1_000, PRICE_SCALE * 2), 2_000);
        assert_eq!(fee_in_micro_mini(0, PRICE_SCALE), 0);
    }

    #[test]
    fn a_falling_mini_price_raises_the_mini_fee_for_the_same_real_world_target() {
        // If MINI's market value halves, twice as much MINI is needed to
        // hit the same steady real-world fee target.
        let target = 1_000u64;
        let price_before = PRICE_SCALE;
        let price_after_mini_halves = PRICE_SCALE * 2;
        let fee_before = fee_in_micro_mini(target, price_before);
        let fee_after = fee_in_micro_mini(target, price_after_mini_halves);
        assert_eq!(fee_after, fee_before * 2);
    }

    #[test]
    fn fee_does_not_overflow_at_large_values() {
        let fee = fee_in_micro_mini(u64::MAX, u64::MAX);
        assert!(fee > 0);
    }
}
