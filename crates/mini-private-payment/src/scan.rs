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

/// Scan a batch of verified claims for payments to these keys.
pub fn scan<'a>(
    view_secret: &[u8],
    spend_public: &[u8],
    claims: impl IntoIterator<Item = &'a VerifiedPrivateClaim>,
) -> Result<Vec<RecognizedPayment>> {
    let mut found = Vec::new();
    for claim in claims {
        if let Some(payment) = scan_one(view_secret, spend_public, claim)? {
            found.push(payment);
        }
    }
    Ok(found)
}
