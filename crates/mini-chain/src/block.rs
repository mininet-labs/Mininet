//! Block headers: canonical encoding + self-certifying content hash, the
//! same content-addressing discipline used everywhere else in this tree.

use did_mini::Did;
use mini_crypto::HashAlgorithm;

/// A block header. Deliberately minimal for this batch: enough to hash,
/// chain, and finalize. Real transaction/state-machine content is `pending`
/// (this crate is the finality-verification core, not the state machine).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHeader {
    /// Block height (genesis is 0).
    pub height: u64,
    /// Hash of the previous block header (all-zero at genesis).
    pub prev_hash: [u8; 32],
    /// Commitment to the post-block application state (content-addressed,
    /// meaning left to the state machine that eventually anchors here).
    pub state_root: [u8; 32],
    /// Protocol timestamp. `mini-consensus` fixes this to the block height
    /// as deterministic logical time — a signature only proves who proposed
    /// a value, never that it reflects real time, so no consumer of this
    /// field may treat it as proposer-supplied wall time.
    pub timestamp_ms: u64,
    /// The proposing validator's identity root.
    pub proposer: Did,
}

impl BlockHeader {
    /// Canonical bytes this header's hash is derived from.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Vec::with_capacity(8 + 32 + 32 + 8 + 4 + self.proposer.as_str().len());
        w.extend_from_slice(&self.height.to_be_bytes());
        w.extend_from_slice(&self.prev_hash);
        w.extend_from_slice(&self.state_root);
        w.extend_from_slice(&self.timestamp_ms.to_be_bytes());
        let p = self.proposer.as_str().as_bytes();
        w.extend_from_slice(&(p.len() as u32).to_be_bytes());
        w.extend_from_slice(p);
        w
    }

    /// The block hash: BLAKE3 of the canonical bytes, what votes commit to.
    pub fn hash(&self) -> [u8; 32] {
        HashAlgorithm::Blake3.digest(&self.canonical_bytes())
    }
}
