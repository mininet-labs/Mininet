//! Stealth addresses: a fresh, one-time output address for every payment,
//! derived from a recipient's two published keys (a spend key and a view
//! key), so on-chain outputs are unlinkable to the recipient's real
//! address. The recipient scans incoming outputs with their view key alone
//! to recognize which ones are theirs, keeping the spend key offline until
//! actually spending.
//!
//! [`crate::stealth_impl`] is a real (CryptoNote-style) implementation of
//! this trait, per the founder override recorded in D-0036.
//! [`NoStealthAddress`] remains available as the fail-closed reference for
//! anyone not opting into the prototype.
//!
//! ## Honest limit [D-0036]
//!
//! This is a founder-overridden, AI-authored prototype, not the human-
//! authored, externally-audited implementation D-0035 point 5 otherwise
//! requires for transaction privacy. The elliptic-curve key derivation
//! here is easy to get subtly wrong in ways that either link outputs that
//! should be unlinkable, or — worse — let someone else's scan incorrectly
//! recognize (and potentially spend) an output that isn't theirs. Treat
//! [`crate::stealth_impl::MininetStealthAddress`] as a prototype pending a
//! specialized external audit before any real value depends on it.

/// One derived output: the per-transaction public key the sender
/// publishes, and the one-time output address the payment actually goes
/// to. Both are needed for a recipient to later recognize the payment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StealthOutput {
    /// The sender's ephemeral per-transaction public key (`R`).
    pub tx_public_key: Vec<u8>,
    /// The one-time output address (`P`) the payment is sent to.
    pub one_time_address: Vec<u8>,
}

/// A source of stealth-address derivation and recognition.
pub trait StealthAddressScheme {
    /// Derive a fresh [`StealthOutput`] for a recipient identified by
    /// `recipient_spend_public`/`recipient_view_public`. Fresh per-call
    /// randomness means the same recipient gets a different, unlinkable
    /// output every time. `None` means no real implementation is
    /// available.
    fn derive_output(
        &mut self,
        recipient_spend_public: &[u8],
        recipient_view_public: &[u8],
    ) -> Option<StealthOutput>;

    /// Whether `output` was derived for the recipient owning
    /// `own_view_secret`/`own_spend_public` — the scanning operation a
    /// recipient runs against the ledger to find their own payments,
    /// using only their view secret (the spend secret stays offline).
    fn recognizes(
        &self,
        own_view_secret: &[u8],
        own_spend_public: &[u8],
        output: &StealthOutput,
    ) -> bool;
}

/// The reference [`StealthAddressScheme`]: never derives an output, never
/// recognizes one as belonging to anybody. Correct, permanent behavior for
/// anyone not opting into the D-0036 prototype — incorrectly recognizing
/// an output as one's own would be far worse than recognizing nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoStealthAddress;

impl StealthAddressScheme for NoStealthAddress {
    fn derive_output(
        &mut self,
        _recipient_spend_public: &[u8],
        _recipient_view_public: &[u8],
    ) -> Option<StealthOutput> {
        None
    }

    fn recognizes(
        &self,
        _own_view_secret: &[u8],
        _own_spend_public: &[u8],
        _output: &StealthOutput,
    ) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_stealth_address_never_derives_an_output() {
        let mut scheme = NoStealthAddress;
        assert_eq!(scheme.derive_output(b"spend", b"view"), None);
    }

    #[test]
    fn no_stealth_address_never_recognizes_an_output() {
        let scheme = NoStealthAddress;
        let fake = StealthOutput {
            tx_public_key: vec![0u8; 32],
            one_time_address: vec![0u8; 32],
        };
        assert!(!scheme.recognizes(b"view", b"spend", &fake));
    }
}
