//! The seam a real stealth-address scheme fills in.
//!
//! A stealth address lets a sender derive a fresh, one-time output address
//! for every payment from a recipient's two published keys (a spend key
//! and a view key), so on-chain outputs are unlinkable to the recipient's
//! real address — an outside observer cannot tell that two payments went
//! to the same person. The recipient scans incoming outputs with their
//! view key to recognize which ones are theirs, without needing their
//! spend key online.
//!
//! ## Honest limit — do not implement this without a human cryptographer
//!
//! The elliptic-curve key derivation here is easy to get subtly wrong in
//! ways that either link outputs that should be unlinkable, or — worse —
//! let someone else's scan incorrectly recognize (and potentially spend)
//! an output that isn't theirs. Same D-0035 point 5 requirement as
//! [`crate::ring`]. [`NoStealthAddress`] is the only implementation here:
//! it derives nothing and recognizes nothing, fail-closed.

/// A source of stealth-address derivation and recognition.
pub trait StealthAddressScheme {
    /// Derive a fresh one-time output address for a recipient identified by
    /// `recipient_spend_key`/`recipient_view_key`, using transaction-
    /// specific randomness `tx_random` (so the same recipient gets a
    /// different address every time). `None` means no real implementation
    /// is available.
    fn derive_output_address(
        &self,
        recipient_spend_key: &[u8],
        recipient_view_key: &[u8],
        tx_random: &[u8],
    ) -> Option<Vec<u8>>;

    /// Whether `output_address` was derived for the recipient owning
    /// `own_view_key`/`own_spend_key` — the scanning operation a recipient
    /// runs against the ledger to find their own payments.
    fn recognizes(&self, own_view_key: &[u8], own_spend_key: &[u8], output_address: &[u8]) -> bool;
}

/// The reference [`StealthAddressScheme`]: never derives an address, never
/// recognizes one as belonging to anybody. Correct, permanent behavior
/// until the human-authored, externally-audited implementation described
/// above exists — incorrectly recognizing an output as one's own would be
/// far worse than recognizing nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoStealthAddress;

impl StealthAddressScheme for NoStealthAddress {
    fn derive_output_address(
        &self,
        _recipient_spend_key: &[u8],
        _recipient_view_key: &[u8],
        _tx_random: &[u8],
    ) -> Option<Vec<u8>> {
        None
    }

    fn recognizes(
        &self,
        _own_view_key: &[u8],
        _own_spend_key: &[u8],
        _output_address: &[u8],
    ) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_stealth_address_never_derives_an_address() {
        let scheme = NoStealthAddress;
        assert_eq!(
            scheme.derive_output_address(b"spend", b"view", b"random"),
            None
        );
    }

    #[test]
    fn no_stealth_address_never_recognizes_an_output() {
        let scheme = NoStealthAddress;
        assert!(!scheme.recognizes(b"view", b"spend", b"some-output-address"));
    }
}
