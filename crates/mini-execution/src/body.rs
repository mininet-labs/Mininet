//! A settlement block body: the ordered list of [`PaymentClaim`]s a
//! proposer includes at one height. Order matters — it **is** the
//! canonical order [`crate::state::apply_block`] resolves conflicting
//! claims by (M3, `docs/INVARIANTS.md` §4): the first claim to establish a
//! new `(payer, sequence)` high-water-mark wins that slot, permanently.
//!
//! A body carries two lists because the chain treats them differently. A
//! [`PaymentClaim`] is verified here — signature, network, payee. A
//! [`NullifierRecord`] is not, and cannot be, because the cryptography that
//! would validate it lives on the other side of the voice/value wall. Both
//! are ordered, and order decides both.

use mini_crypto::HashAlgorithm;
use mini_economy::ScalableEpochPlan;
use mini_settlement::PaymentClaim;

use crate::nullifier::NullifierRecord;

/// Hard cap on claims per block — an allocation/CPU bound applied before
/// any signature verification, the same discipline
/// `mini_chain::MAX_VOTES_PER_CERTIFICATE` applies to untrusted vote lists.
/// This matches the default admission-pool ceiling. Keeping the consensus
/// ceiling at the same operational bound prevents a Byzantine proposer from
/// turning one otherwise-valid proposal into tens of thousands of signature
/// verifications that honest validators must complete before voting.
pub const MAX_CLAIMS_PER_BLOCK: usize = 4_096;

/// One monetary transition per block keeps issuance progression explicit and
/// bounds decoding before execution.
pub const MAX_MONETARY_EPOCHS_PER_BLOCK: usize = 1;

/// An ordered list of claims proposed for inclusion at one height.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettlementBlockBody {
    pub claims: Vec<PaymentClaim>,
    /// Shielded spends, as opaque `(key_image, claim_digest)` facts.
    ///
    /// A separate list rather than a variant of `claims` because they are
    /// not the same kind of thing to this crate: a `PaymentClaim` is
    /// verified here, and a [`NullifierRecord`] is deliberately not —
    /// see [`crate::nullifier`] for what that costs and why it is the
    /// shape the voice/value wall permits.
    pub nullifiers: Vec<NullifierRecord>,
    pub monetary_epochs: Vec<ScalableEpochPlan>,
}

impl SettlementBlockBody {
    /// An empty body — a valid block that settles nothing, the same way an
    /// empty vote list is a structurally valid (if unfinalizable) quorum
    /// certificate.
    pub fn new(claims: Vec<PaymentClaim>) -> Self {
        SettlementBlockBody {
            claims,
            nullifiers: Vec::new(),
            monetary_epochs: Vec::new(),
        }
    }

    /// Add shielded-spend records. Order is canonical here exactly as it is
    /// for `claims`: the first record to take a key image wins it.
    pub fn with_nullifiers(mut self, nullifiers: Vec<NullifierRecord>) -> Self {
        self.nullifiers = nullifiers;
        self
    }

    /// Add one or more proposed monetary epochs. Execution currently accepts
    /// at most one per block and validates it against finalized supply.
    pub fn with_monetary_epochs(mut self, epochs: Vec<ScalableEpochPlan>) -> Self {
        self.monetary_epochs = epochs;
        self
    }

    /// Content hash of the exact ordered body — what v2 block headers commit
    /// to separately from [`crate::state::LedgerState::commitment`]'s
    /// commitment to what the body resolved to.
    pub fn hash(&self) -> [u8; 32] {
        let mut w = Vec::new();
        w.extend_from_slice(b"mini-execution/settlement-block-body/v3");
        w.extend_from_slice(&(self.claims.len() as u64).to_be_bytes());
        for claim in &self.claims {
            let digest = mini_settlement::claim_digest(claim);
            w.extend_from_slice(&digest);
        }
        w.extend_from_slice(&(self.nullifiers.len() as u64).to_be_bytes());
        for record in &self.nullifiers {
            w.extend_from_slice(&record.canonical_bytes());
        }
        w.extend_from_slice(&(self.monetary_epochs.len() as u64).to_be_bytes());
        for epoch in &self.monetary_epochs {
            let bytes = epoch.canonical_bytes();
            w.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
            w.extend_from_slice(&bytes);
        }
        HashAlgorithm::Blake3.digest(&w)
    }
}
