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
        if entry.micro_mini_per_micro_cent == 0 {
            return Err(ValueError::ZeroPrice);
        }
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

    /// Quote a fee using the governed price in effect at `at_ms` — looking
    /// up the historical price and converting it in one call, so a caller
    /// can never accidentally apply today's price to historical work by
    /// forgetting to thread `at_ms` through a separate conversion step.
    pub fn fee_at(&self, fee_target_micro_cents: u64, at_ms: u64) -> Result<u64> {
        fee_in_micro_mini(fee_target_micro_cents, self.price_at(at_ms)?)
    }
}

/// Micro-MINI owed for a fee whose real-world value target is
/// `fee_target_micro_cents` micro-cents, at `micro_mini_per_micro_cent`
/// (`PRICE_SCALE`'s fixed point). `u128` internally so a large target at a
/// large price cannot silently overflow the intermediate product; the
/// final `u64` conversion is checked, not cast, so a quote too large for
/// the ledger's amount type is rejected rather than silently truncated.
pub fn fee_in_micro_mini(
    fee_target_micro_cents: u64,
    micro_mini_per_micro_cent: u64,
) -> Result<u64> {
    if micro_mini_per_micro_cent == 0 {
        return Err(ValueError::ZeroPrice);
    }
    let product = (fee_target_micro_cents as u128) * (micro_mini_per_micro_cent as u128);
    u64::try_from(product / PRICE_SCALE as u128).map_err(|_| ValueError::FeeOverflow)
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
    fn a_zero_price_entry_is_rejected() {
        // Fee-manipulation finding (roadmap #44): a governed price of zero
        // would make every fee free regardless of the real-world value
        // target, defeating the whole mechanism. Never a legitimate value.
        let mut history = PriceHistory::new();
        let err = history
            .add_entry(PriceEntry {
                effective_at_ms: 100,
                micro_mini_per_micro_cent: 0,
            })
            .unwrap_err();
        assert_eq!(err, ValueError::ZeroPrice);
        assert_eq!(
            history.price_at(100),
            Err(ValueError::NoRateInEffect),
            "the rejected zero-price entry must not have been recorded"
        );

        // A zero price is rejected even when it would otherwise be a
        // perfectly well-ordered entry following a real one.
        history
            .add_entry(PriceEntry {
                effective_at_ms: 100,
                micro_mini_per_micro_cent: 1_000_000,
            })
            .unwrap();
        let err = history
            .add_entry(PriceEntry {
                effective_at_ms: 200,
                micro_mini_per_micro_cent: 0,
            })
            .unwrap_err();
        assert_eq!(err, ValueError::ZeroPrice);
        assert_eq!(history.price_at(200).unwrap(), 1_000_000);
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
        assert_eq!(fee_in_micro_mini(1_000, PRICE_SCALE).unwrap(), 1_000);
        assert_eq!(fee_in_micro_mini(1_000, PRICE_SCALE * 2).unwrap(), 2_000);
        assert_eq!(fee_in_micro_mini(0, PRICE_SCALE).unwrap(), 0);
    }

    #[test]
    fn a_falling_mini_price_raises_the_mini_fee_for_the_same_real_world_target() {
        // If MINI's market value halves, twice as much MINI is needed to
        // hit the same steady real-world fee target.
        let target = 1_000u64;
        let price_before = PRICE_SCALE;
        let price_after_mini_halves = PRICE_SCALE * 2;
        let fee_before = fee_in_micro_mini(target, price_before).unwrap();
        let fee_after = fee_in_micro_mini(target, price_after_mini_halves).unwrap();
        assert_eq!(fee_after, fee_before * 2);
    }

    #[test]
    fn fee_overflow_is_rejected_instead_of_silently_truncated() {
        // Pre-fix, `(product / PRICE_SCALE as u128) as u64` truncated a
        // too-large quote down to some wrapped value instead of failing —
        // a real, previously-undetected bug: this exact input silently
        // produced a wrong (and much too small) fee rather than an error.
        assert_eq!(
            fee_in_micro_mini(u64::MAX, u64::MAX),
            Err(ValueError::FeeOverflow)
        );
    }

    #[test]
    fn a_zero_rate_is_rejected_at_quote_time_too_not_only_at_ingress() {
        // `PriceHistory::add_entry` already rejects a zero price, but
        // `fee_in_micro_mini` is also called directly (and a `PriceEntry`'s
        // fields are public) — defense in depth means the conversion
        // itself refuses a zero rate regardless of how it arrived.
        assert_eq!(
            fee_in_micro_mini(1, 0),
            Err(ValueError::ZeroPrice),
            "a zero rate must never convert a positive target into a free fee"
        );
    }

    #[test]
    fn fee_at_binds_historical_lookup_and_checked_conversion_in_one_call() {
        let mut history = PriceHistory::new();
        history
            .add_entry(PriceEntry {
                effective_at_ms: 100,
                micro_mini_per_micro_cent: PRICE_SCALE * 2,
            })
            .unwrap();
        assert_eq!(history.fee_at(500, 99), Err(ValueError::NoRateInEffect));
        assert_eq!(history.fee_at(500, 100).unwrap(), 1_000);
    }
}
