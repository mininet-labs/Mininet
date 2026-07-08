//! The community-governed BTC/XMR-to-MINI exchange rate (whitepaper §8.2:
//! "receive MINI at a community-governed rate... no seller to regulate and
//! no contract to sign").
//!
//! This module is pure bookkeeping and arithmetic: a rate history lookup
//! and the multiplication that turns a contributed amount into a minted
//! amount at whatever rate was in effect. It has no opinion on *how* the
//! rate is decided (that is ordinary flat-vote governance, whitepaper §10)
//! and, critically, no opinion on whether a contribution actually arrived —
//! see [`crate::receipt`] for that boundary.

/// Fixed-point scale for the rate: `mini_per_unit_micro` is how many
/// micro-MINI one contributed unit (e.g. one satoshi, one piconero — the
/// caller's choice of base unit) is worth. All-integer, so a mint
/// computation is exactly reproducible from the same rate and amount.
pub const RATE_SCALE: u64 = 1_000_000;

/// One governed rate, effective from a point in time until superseded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateEntry {
    /// When this rate took effect (ms).
    pub effective_at_ms: u64,
    /// Micro-MINI minted per one contributed base unit, at
    /// [`RATE_SCALE`]'s fixed point.
    pub mini_per_unit_micro: u64,
}

/// An ordered history of governed rates. Lookup always finds the latest
/// entry at or before a given time — a rate change never retroactively
/// changes what an earlier contribution was worth.
#[derive(Debug, Clone, Default)]
pub struct RateHistory {
    entries: Vec<RateEntry>,
}

impl RateHistory {
    /// A new, empty rate history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new governed rate. Entries must arrive in strictly
    /// increasing `effective_at_ms` order — this is a history, not a
    /// mutable current value, so out-of-order or duplicate-time entries are
    /// rejected rather than silently reordered.
    pub fn add_entry(&mut self, entry: RateEntry) -> crate::error::Result<()> {
        if let Some(last) = self.entries.last() {
            if entry.effective_at_ms <= last.effective_at_ms {
                return Err(crate::error::TreasuryError::OutOfOrderRateEntry);
            }
        }
        self.entries.push(entry);
        Ok(())
    }

    /// The rate in effect at `at_ms` — the latest entry whose
    /// `effective_at_ms` is at or before `at_ms`.
    pub fn rate_at(&self, at_ms: u64) -> crate::error::Result<u64> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.effective_at_ms <= at_ms)
            .map(|e| e.mini_per_unit_micro)
            .ok_or(crate::error::TreasuryError::NoRateInEffect)
    }
}

/// Micro-MINI minted for `contributed_units` at `mini_per_unit_micro`
/// (`RATE_SCALE`'s fixed point). Uses `u128` internally so a large
/// contribution at a large rate cannot silently overflow.
pub fn mint_amount_micro(contributed_units: u64, mini_per_unit_micro: u64) -> u64 {
    let product = (contributed_units as u128) * (mini_per_unit_micro as u128);
    (product / RATE_SCALE as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_lookup_finds_the_latest_entry_at_or_before_the_time() {
        let mut history = RateHistory::new();
        history
            .add_entry(RateEntry {
                effective_at_ms: 100,
                mini_per_unit_micro: 1_000_000,
            })
            .unwrap();
        history
            .add_entry(RateEntry {
                effective_at_ms: 200,
                mini_per_unit_micro: 2_000_000,
            })
            .unwrap();

        assert_eq!(history.rate_at(150).unwrap(), 1_000_000);
        assert_eq!(history.rate_at(200).unwrap(), 2_000_000);
        assert_eq!(history.rate_at(1_000).unwrap(), 2_000_000);
    }

    #[test]
    fn rate_lookup_before_any_entry_fails() {
        let mut history = RateHistory::new();
        history
            .add_entry(RateEntry {
                effective_at_ms: 100,
                mini_per_unit_micro: 1_000_000,
            })
            .unwrap();
        assert_eq!(
            history.rate_at(50),
            Err(crate::error::TreasuryError::NoRateInEffect)
        );
    }

    #[test]
    fn out_of_order_or_duplicate_entries_are_rejected() {
        let mut history = RateHistory::new();
        history
            .add_entry(RateEntry {
                effective_at_ms: 200,
                mini_per_unit_micro: 1_000_000,
            })
            .unwrap();
        assert!(history
            .add_entry(RateEntry {
                effective_at_ms: 200,
                mini_per_unit_micro: 2_000_000,
            })
            .is_err());
        assert!(history
            .add_entry(RateEntry {
                effective_at_ms: 100,
                mini_per_unit_micro: 2_000_000,
            })
            .is_err());
    }

    #[test]
    fn mint_amount_scales_with_rate() {
        // 1:1 rate: contributing 100 units mints 100 micro-MINI-units.
        assert_eq!(mint_amount_micro(100, RATE_SCALE), 100);
        // Double the rate, double the mint.
        assert_eq!(mint_amount_micro(100, RATE_SCALE * 2), 200);
        // Zero contribution mints nothing.
        assert_eq!(mint_amount_micro(0, RATE_SCALE), 0);
    }

    #[test]
    fn mint_amount_does_not_overflow_at_large_values() {
        let amount = mint_amount_micro(u64::MAX, u64::MAX);
        assert!(amount > 0);
    }
}
