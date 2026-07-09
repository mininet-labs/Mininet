//! A settlement block body: the ordered list of [`PaymentClaim`]s a
//! proposer includes at one height. Order matters — it **is** the
//! canonical order [`crate::state::apply_block`] resolves conflicting
//! claims by (M3, `docs/INVARIANTS.md` §4): the first claim to establish a
//! new `(payer, sequence)` high-water-mark wins that slot, permanently.

use mini_crypto::HashAlgorithm;
use mini_settlement::PaymentClaim;

/// Hard cap on claims per block — an allocation/CPU bound applied before
/// any signature verification, the same discipline
/// `mini_chain::MAX_VOTES_PER_CERTIFICATE` applies to untrusted vote lists.
pub const MAX_CLAIMS_PER_BLOCK: usize = 100_000;

/// An ordered list of claims proposed for inclusion at one height.
#[derive(Debug, Clone, Default)]
pub struct SettlementBlockBody {
    pub claims: Vec<PaymentClaim>,
}

impl SettlementBlockBody {
    /// An empty body — a valid block that settles nothing, the same way an
    /// empty vote list is a structurally valid (if unfinalizable) quorum
    /// certificate.
    pub fn new(claims: Vec<PaymentClaim>) -> Self {
        SettlementBlockBody { claims }
    }

    /// Content hash of this body's claims, in order — what a header would
    /// reference if it wanted to commit to *which claims were proposed*,
    /// separately from [`crate::state::LedgerState::commitment`]'s
    /// commitment to *what they resolved to*. Domain-tagged and length-
    /// prefixed like every other content hash in this tree
    /// (`mini_settlement::claim_digest`'s own discipline).
    pub fn hash(&self) -> [u8; 32] {
        let mut w = Vec::new();
        w.extend_from_slice(b"mini-execution/settlement-block-body/v1");
        w.extend_from_slice(&(self.claims.len() as u64).to_be_bytes());
        for claim in &self.claims {
            let digest = mini_settlement::claim_digest(claim);
            w.extend_from_slice(&digest);
        }
        HashAlgorithm::Blake3.digest(&w)
    }
}
