//! The seam between this crate's protocol logic and a real canonical chain.
//!
//! `mini-chain` today verifies finality of an abstract block; it has no
//! account/balance execution engine yet (that's the state-machine layer
//! roadmap #36-#45 build toward). [`CanonicalLedgerView`] is the same kind
//! of seam `mini-forge::KelDirectory`/`IdentityOracle` and
//! `mini_presence::ReplayGuard` already use: this crate's reconciliation
//! logic is fully specified and testable *now*, against any implementation
//! of this trait, without needing the real chain to exist first.

/// A read-only view of canonical, finalized settlement state for one payer.
/// A real implementation is chain-backed; [`crate::InMemoryLedgerView`] is
/// for tests only.
pub trait CanonicalLedgerView {
    /// The highest nonce this ledger has finalized a claim at for `payer`,
    /// if any. `None` means this payer has never had a claim finalized.
    fn finalized_nonce(&self, payer: &[u8]) -> Option<u64>;

    /// The digest ([`crate::claim_digest`]) of the claim this ledger
    /// finalized for `payer` at exactly `nonce`, if any. Only meaningful
    /// when `nonce <= finalized_nonce(payer)`.
    fn finalized_claim_digest(&self, payer: &[u8], nonce: u64) -> Option<[u8; 32]>;
}

/// A trivial in-memory [`CanonicalLedgerView`] — test-only. Production
/// needs a real chain-execution-backed implementation; see this crate's
/// README for what that requires.
#[derive(Debug, Default)]
pub struct InMemoryLedgerView {
    finalized: std::collections::HashMap<Vec<u8>, Vec<(u64, [u8; 32])>>,
}

impl InMemoryLedgerView {
    /// A new, empty ledger view with nothing finalized.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `digest` as finalized for `payer` at `nonce`. Test-only
    /// helper — a real ledger reaches this state via actual chain
    /// execution and finality, not a direct setter.
    pub fn finalize(&mut self, payer: &[u8], nonce: u64, digest: [u8; 32]) {
        self.finalized
            .entry(payer.to_vec())
            .or_default()
            .push((nonce, digest));
    }
}

impl CanonicalLedgerView for InMemoryLedgerView {
    fn finalized_nonce(&self, payer: &[u8]) -> Option<u64> {
        self.finalized
            .get(payer)
            .and_then(|entries| entries.iter().map(|(n, _)| *n).max())
    }

    fn finalized_claim_digest(&self, payer: &[u8], nonce: u64) -> Option<[u8; 32]> {
        self.finalized
            .get(payer)?
            .iter()
            .find(|(n, _)| *n == nonce)
            .map(|(_, d)| *d)
    }
}
