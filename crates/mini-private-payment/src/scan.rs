//! Finding your own payments, with the view key alone.
//!
//! There is no index to query and nobody to ask — asking would be the leak.
//! A recipient scans candidate claims locally and recognizes their own by
//! recomputing the stealth shared secret. This is the same trade the
//! CryptoNote family makes: linear scanning is the price of not having an
//! address anyone can watch.
//!
//! # Privilege separation
//!
//! Recognizing and opening a payment need only the **view** secret. Spending
//! it needs the **spend** secret, via
//! [`mini_value::derive_spend_scalar`]. Keeping them apart is what lets a
//! wallet watch its own income on a warm device while the spend key stays
//! offline — so this module never takes a spend secret, and could not spend
//! anything if it wanted to.

use mini_value::{MininetStealthAddress, StealthAddressScheme};

use crate::claim::VerifiedPrivateClaim;
use crate::error::Result;
use crate::memo::PaymentPurpose;

/// A payment recognized as belonging to the scanning wallet.
#[derive(Debug, Clone)]
pub struct RecognizedPayment {
    /// The claim, still verified.
    pub claim: VerifiedPrivateClaim,
    /// The purpose the sender sealed, once opened.
    pub purpose: PaymentPurpose,
}

/// Whether `claim` pays the holder of these keys.
///
/// Cheap enough to run over every claim a node sees, which it must be:
/// scanning is the only way a recipient learns of income, so a wallet on a
/// weak device runs this constantly (Directive 11).
pub fn recognizes(view_secret: &[u8], spend_public: &[u8], claim: &VerifiedPrivateClaim) -> bool {
    MininetStealthAddress.recognizes(view_secret, spend_public, &claim.claim().output)
}

/// Recognize a payment and open its memo in one step.
///
/// `Ok(None)` means "not mine" — the ordinary case, and deliberately not an
/// error, so a scanning loop does not treat every stranger's payment as a
/// failure. An `Err` means the payment *is* addressed here but its memo did
/// not open, which is a real anomaly worth surfacing.
pub fn scan_one(
    view_secret: &[u8],
    spend_public: &[u8],
    claim: &VerifiedPrivateClaim,
) -> Result<Option<RecognizedPayment>> {
    if !recognizes(view_secret, spend_public, claim) {
        return Ok(None);
    }
    let shared =
        match mini_value::recover_shared_secret(view_secret, &claim.claim().output.tx_public_key) {
            Some(shared) => shared,
            None => return Ok(None),
        };
    let purpose = claim.open_memo(&shared)?;
    Ok(Some(RecognizedPayment {
        claim: claim.clone(),
        purpose,
    }))
}

/// What a scan of a batch of claims found.
///
/// Two lists rather than one, because "addressed here and unreadable" is a
/// real state that must not be reported as either "yours" or "not yours".
#[derive(Debug, Clone, Default)]
pub struct ScanOutcome {
    /// Payments to these keys whose memos opened.
    pub payments: Vec<RecognizedPayment>,
    /// Payments to these keys whose memos did **not** open. The money is
    /// still there and still spendable — only the sender's stated purpose is
    /// missing.
    pub unreadable: Vec<VerifiedPrivateClaim>,
}

impl ScanOutcome {
    /// Every claim addressed to these keys, readable or not. What a balance
    /// is computed over: an unopenable memo does not make a payment vanish.
    pub fn all_claims(&self) -> impl Iterator<Item = &VerifiedPrivateClaim> {
        self.payments
            .iter()
            .map(|payment| &payment.claim)
            .chain(self.unreadable.iter())
    }
}

/// Scan a batch of verified claims for payments to these keys.
///
/// Deliberately cannot fail. An earlier version returned
/// `Result<Vec<RecognizedPayment>>` and propagated [`scan_one`]'s error,
/// which handed anyone a way to blind a wallet completely: the account's
/// spend and view *public* keys are published, so **any** stranger can
/// derive a valid stealth output paying it, then seal the memo under a key
/// the recipient cannot derive. One such payment made the whole scan return
/// `Err`, and every other payment the wallet had ever received disappeared
/// with it — costing the attacker one payment and the victim all visibility
/// into their own income.
///
/// So a memo that will not open is recorded as exactly that and the scan
/// continues. Nothing is silently dropped: the claim lands in
/// [`ScanOutcome::unreadable`], where a wallet can show it as received with
/// an unknown purpose.
pub fn scan<'a>(
    view_secret: &[u8],
    spend_public: &[u8],
    claims: impl IntoIterator<Item = &'a VerifiedPrivateClaim>,
) -> ScanOutcome {
    let mut outcome = ScanOutcome::default();
    for claim in claims {
        match scan_one(view_secret, spend_public, claim) {
            Ok(Some(payment)) => outcome.payments.push(payment),
            Ok(None) => {}
            Err(_) => outcome.unreadable.push(claim.clone()),
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::{build, verify, PaymentRequest};
    use crate::decoy::InMemoryOutputSet;
    use crate::memo::PaymentPurpose;
    use mini_value::StealthKeypair;

    const NETWORK: [u8; 32] = [0x11; 32];

    fn payment_to(recipient: &StealthKeypair, purpose: &[u8]) -> VerifiedPrivateClaim {
        let mut outputs = InMemoryOutputSet::new();
        for _ in 0..64 {
            outputs.push(
                StealthKeypair::generate()
                    .unwrap()
                    .spend_public_bytes()
                    .to_vec(),
            );
        }
        let own = StealthKeypair::generate().unwrap();
        outputs.push(own.spend_public_bytes().to_vec());

        let request = PaymentRequest {
            network_id: NETWORK,
            recipient_spend_public: recipient.spend_public_bytes().to_vec(),
            recipient_view_public: recipient.view_public_bytes().to_vec(),
            amount_micro: 1_000,
            purpose: PaymentPurpose::new(purpose.to_vec()),
            valid_until_ms: 10_000,
            last_known_chain: b"height:1".to_vec(),
            ring_size: crate::MIN_RING_SIZE,
            real_output_index: 64,
            secret_key: own.spend_secret_bytes().to_vec(),
            decoy_entropy: mini_crypto::random_32().unwrap(),
            blinding: mini_crypto::random_32().unwrap(),
        };
        let (claim, _) = build(&request, &outputs).unwrap();
        verify(&claim, &NETWORK).unwrap()
    }

    #[test]
    fn one_unopenable_memo_does_not_hide_the_rest_of_a_wallets_income() {
        // The regression this function's totality exists to prevent. An
        // account's spend and view *public* keys are published, so anyone
        // can send it a payment whose memo it cannot open. When `scan`
        // returned `Result`, a single such payment made the whole scan fail
        // and every other payment the wallet had ever received vanished
        // with it -- one cheap payment for total blindness.
        let wallet = StealthKeypair::generate().unwrap();
        let griefing = payment_to(&wallet, b"unopenable").fabricate_unopenable_memo();
        let ledger = [
            payment_to(&wallet, b"salary"),
            griefing,
            payment_to(&wallet, b"refund"),
        ];

        let found = scan(
            &wallet.view_secret_bytes(),
            &wallet.spend_public_bytes(),
            ledger.iter(),
        );

        assert_eq!(found.payments.len(), 2, "the readable income survives");
        assert_eq!(found.unreadable.len(), 1, "and the odd one is not dropped");
        assert_eq!(found.all_claims().count(), 3);

        let mut references: Vec<_> = found
            .payments
            .iter()
            .map(|payment| payment.purpose.reference.clone())
            .collect();
        references.sort();
        assert_eq!(references, vec![b"refund".to_vec(), b"salary".to_vec()]);
    }

    #[test]
    fn a_strangers_payments_are_neither_readable_nor_counted_as_unreadable() {
        // "Not mine" and "mine but broken" are different states, and the
        // common case must stay the cheap one: a wallet scanning a public
        // ledger sees mostly strangers' payments and must not accumulate
        // them.
        let wallet = StealthKeypair::generate().unwrap();
        let stranger = StealthKeypair::generate().unwrap();
        let ledger = [
            payment_to(&stranger, b"not your business"),
            payment_to(&wallet, b"yours"),
        ];

        let found = scan(
            &wallet.view_secret_bytes(),
            &wallet.spend_public_bytes(),
            ledger.iter(),
        );
        assert_eq!(found.payments.len(), 1);
        assert!(found.unreadable.is_empty());
        assert_eq!(found.payments[0].purpose.reference, b"yours".to_vec());
    }
}
