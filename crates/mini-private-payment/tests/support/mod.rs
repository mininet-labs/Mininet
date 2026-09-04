// Each test binary compiles this module separately and uses a different
// subset, so unused-here is expected rather than dead.
#![allow(dead_code)]

//! Shared fixtures. Every payment built here goes through the real
//! primitives — real stealth derivation, a real MLSAG spend proof, real
//! Bulletproofs, and the protocol's real decoy sampling. Nothing is
//! stubbed, because a privacy or conservation test against a stub proves
//! nothing about either.

use mini_private_payment::{
    build, BuiltOutput, InMemoryOutputSet, OutputSet, PaymentPurpose, PaymentRequest,
    PrivatePaymentClaim, Recipient, SpendableOutput, MIN_RING_SIZE,
};
use mini_value::{ConfidentialAmountScheme, MininetConfidentialAmount, StealthKeypair};

pub const NETWORK: [u8; 32] = [0x5a; 32];

/// A recipient's published stealth keys plus the secrets to scan with.
pub fn recipient() -> StealthKeypair {
    StealthKeypair::generate().unwrap()
}

/// A world with spendable outputs in it.
///
/// Real conservation cannot be tested without one: a claim spends outputs
/// that already exist, with values and blinding factors the spender knows,
/// and the ring draws its decoys from the same set. A fixture that invented
/// commitments could not produce a signable spend.
pub struct Ledger {
    pub outputs: InMemoryOutputSet,
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

impl Ledger {
    pub fn new() -> Self {
        Self {
            outputs: InMemoryOutputSet::new(),
        }
    }

    /// Append `count` outputs nobody in these tests can spend — the decoy
    /// population every ring draws from.
    pub fn fill(&mut self, count: usize) {
        for _ in 0..count {
            let key = StealthKeypair::generate().unwrap();
            let blinding = mini_crypto::random_32().unwrap();
            let mut scheme = MininetConfidentialAmount;
            let (commitment, _) = scheme.commit_with_proof(1_000, &blinding).unwrap();
            self.outputs
                .push(key.spend_public_bytes().to_vec(), commitment);
        }
    }

    /// Mint one output this wallet can actually spend, and return the
    /// handle needed to spend it.
    pub fn mint(&mut self, value_micro: u64) -> SpendableOutput {
        let key = StealthKeypair::generate().unwrap();
        let blinding = mini_crypto::random_32().unwrap();
        let mut scheme = MininetConfidentialAmount;
        let (commitment, _) = scheme.commit_with_proof(value_micro, &blinding).unwrap();
        self.outputs
            .push(key.spend_public_bytes().to_vec(), commitment);
        SpendableOutput {
            set_index: self.outputs.len() - 1,
            one_time_secret: key.spend_secret_bytes(),
            value_micro,
            blinding,
        }
    }

    /// A ledger with a comfortable decoy population and one spendable
    /// output of `value_micro`.
    pub fn with_funds(value_micro: u64) -> (Self, SpendableOutput) {
        let mut ledger = Ledger::new();
        ledger.fill(MIN_RING_SIZE * 4);
        let spend = ledger.mint(value_micro);
        ledger.fill(4);
        (ledger, spend)
    }
}

impl OutputSet for Ledger {
    fn len(&self) -> usize {
        self.outputs.len()
    }
    fn key_at(&self, index: usize) -> Option<Vec<u8>> {
        self.outputs.key_at(index)
    }
    fn commitment_at(&self, index: usize) -> Option<Vec<u8>> {
        self.outputs.commitment_at(index)
    }
}

/// One recipient being paid `amount` for `purpose`.
pub fn pay(to: &StealthKeypair, amount: u64, purpose: &[u8]) -> Recipient {
    Recipient {
        spend_public: to.spend_public_bytes().to_vec(),
        view_public: to.view_public_bytes().to_vec(),
        amount_micro: amount,
        purpose: PaymentPurpose::new(purpose.to_vec()),
    }
}

/// A request spending `spends` to `recipients` with `fee`.
pub fn request_for(
    spends: Vec<SpendableOutput>,
    recipients: Vec<Recipient>,
    fee_micro: u64,
) -> PaymentRequest {
    PaymentRequest {
        network_id: NETWORK,
        spends,
        recipients,
        fee_micro,
        ring_size: MIN_RING_SIZE,
        valid_until_ms: 10_000,
        last_known_chain: b"height:1".to_vec(),
        decoy_entropy: mini_crypto::random_32().unwrap(),
    }
}

/// The common case: one input, one recipient, no change, no fee.
pub fn payment_to(
    to: &StealthKeypair,
    amount: u64,
    purpose: &[u8],
) -> (PrivatePaymentClaim, Vec<BuiltOutput>) {
    let (ledger, spend) = Ledger::with_funds(amount);
    let request = request_for(vec![spend], vec![pay(to, amount, purpose)], 0);
    build(&request, &ledger).unwrap()
}

/// The same, with an explicit ring size.
pub fn payment_with_ring(
    to: &StealthKeypair,
    amount: u64,
    purpose: &[u8],
    ring_size: usize,
) -> (PrivatePaymentClaim, Vec<BuiltOutput>) {
    let mut ledger = Ledger::new();
    ledger.fill(ring_size * 4);
    let spend = ledger.mint(amount);
    let mut request = request_for(vec![spend], vec![pay(to, amount, purpose)], 0);
    request.ring_size = ring_size;
    build(&request, &ledger).unwrap()
}
