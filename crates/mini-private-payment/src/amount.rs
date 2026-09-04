//! Opening amounts to auditors, and refusing to pretend that is the same as
//! auditing an account (roadmap R6).
//!
//! # The gap this closes
//!
//! [`crate::ViewKeyDisclosure`] makes an account's *income* checkable: an
//! auditor can enumerate every payment that arrived and read every memo.
//! What it cannot do is say how much any of them was worth. A view key
//! recognizes a stealth output; it does not open a Pedersen commitment.
//!
//! For treasury accountability under D-0073 that is a real gap between what
//! "audited" sounds like and what it delivers. An auditor who can list
//! disbursements but not add them up cannot answer the only question anyone
//! actually asks.
//!
//! # What an opening is
//!
//! An output's amount lives in `C = b·G_blind + v·H_val`. The recipient
//! learns `(v, b)` from the sealed [`crate::PaymentNote`], because they need
//! both to spend it. Publishing that pair lets anyone recompute `C` and
//! check it against the commitment on the claim. Pedersen commitments are
//! computationally binding, so an opening that verifies **is** the amount —
//! there is no second `(v', b')` a discloser could have chosen instead.
//!
//! No new cryptography: this is the commitment's own opening, checked with
//! `mini_value::pedersen_commitment`.
//!
//! # What publishing one costs
//!
//! - **It is permanent and retroactive**, like every disclosure here. A
//!   published opening cannot be withdrawn.
//! - **It exposes the sender's payment, not only the recipient's receipt.**
//!   The amount was always known to both parties; opening it tells
//!   *everyone* what that sender paid. They were not asked. This is why
//!   [`AmountDisclosure::create`] takes a typed
//!   [`AcknowledgedAmountDisclosure`] rather than a flag.
//! - **It does not let anyone spend the output.** Spending needs the
//!   one-time secret key, which is not here and never travels in a memo.
//!
//! # The part that matters most: an opening is *chosen*
//!
//! A discloser picks which outputs to open. Nothing forces them to open all
//! of them, and no cryptography could. So a sum of openings is not an
//! account's income — it is an account's income *as far as the account chose
//! to show*, and reporting the first when you computed the second is the
//! kind of quiet overclaim this project treats as a bug.
//!
//! [`audit_amounts`] therefore never returns a bare total. It returns
//! [`AuditedIncome`], which carries the opened total **and** the number of
//! recognized payments left unopened, and [`AuditedIncome::is_complete`] is
//! the only thing that licenses reading the total as a total. An auditor
//! holding a view-key disclosure knows exactly how many payments exist; that
//! is what makes a missing opening visible rather than invisible.

use mini_crypto::HashAlgorithm;

use crate::claim::VerifiedPrivateClaim;
use crate::codec::{Reader, Writer};
use crate::disclosure::VerifiedDisclosure;
use crate::error::{DecodeFailure, PrivatePaymentError, Result};
use crate::memo::{PaymentNote, PaymentPurpose};

/// Domain separator for an amount disclosure's encoding.
pub const AMOUNT_DISCLOSURE_DOMAIN: &[u8] = b"mininet/mini-private-payment/amount-disclosure/v1";

/// Wire format version.
pub const AMOUNT_DISCLOSURE_VERSION: u8 = 1;

/// Proof that the caller understands what opening an amount does.
///
/// Constructible only by writing [`Self::REQUIRED_ACKNOWLEDGEMENT`]
/// verbatim, the same discipline
/// [`crate::AcknowledgedIrreversibleDisclosure`] applies to a view key and
/// `mini_installer::OwnerApproval` applies to activating a release. A `bool`
/// would have done the same job with the same risk and none of the friction
/// that makes someone read the sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcknowledgedAmountDisclosure {
    _private: (),
}

impl AcknowledgedAmountDisclosure {
    /// The exact phrase. It names the third-party exposure, because that is
    /// the part a discloser is most likely to overlook: the amount is not
    /// only theirs to reveal.
    pub const REQUIRED_ACKNOWLEDGEMENT: &'static str =
        "I understand that opening this amount is permanent, that it reveals \
         publicly what a specific sender paid me, and that they did not agree \
         to it";

    pub fn new(acknowledgement: &str) -> Option<Self> {
        (acknowledgement == Self::REQUIRED_ACKNOWLEDGEMENT).then_some(Self { _private: () })
    }
}

/// A published opening of one output's amount.
///
/// Names the claim and the output within it, so an auditor can find the
/// commitment this opens without being told where to look.
#[derive(Clone, PartialEq, Eq)]
pub struct AmountDisclosure {
    claim_digest: [u8; 32],
    output_index: u32,
    amount_micro: u64,
    blinding: [u8; 32],
}

impl core::fmt::Debug for AmountDisclosure {
    /// The amount is the point of this object and is published anyway, but
    /// the blinding factor is opening material for a commitment; it is not
    /// worth having it arrive in a log through an incidental `{:?}`.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AmountDisclosure")
            .field("claim_digest", &"<32 bytes>")
            .field("output_index", &self.output_index)
            .field("amount_micro", &self.amount_micro)
            .field("blinding", &"<redacted>")
            .finish()
    }
}

impl AmountDisclosure {
    /// Publish the opening of one output the caller was paid.
    ///
    /// The `note` must be the one sealed to this output — that is what
    /// makes this an operation only the recipient can perform. The claim is
    /// taken verified so a disclosure cannot be built against a claim
    /// nobody checked.
    ///
    /// Returns [`PrivatePaymentError::DisclosedAmountMismatch`] if the note
    /// does not actually open that output's commitment: publishing an
    /// opening that fails verification would put an unfalsifiable-looking
    /// number into the world and force every auditor to discover
    /// independently that it is wrong.
    pub fn create(
        claim: &VerifiedPrivateClaim,
        output_index: usize,
        note: &PaymentNote,
        _acknowledged: &AcknowledgedAmountDisclosure,
    ) -> Result<Self> {
        let index = u32::try_from(output_index).map_err(|_| PrivatePaymentError::MalformedMemo)?;
        let disclosure = AmountDisclosure {
            claim_digest: *claim.transcript_digest(),
            output_index: index,
            amount_micro: note.amount_micro,
            blinding: note.blinding,
        };
        // Refuse to publish something that does not check out.
        disclosure.open_against(claim)?;
        Ok(disclosure)
    }

    pub fn claim_digest(&self) -> &[u8; 32] {
        &self.claim_digest
    }

    pub fn output_index(&self) -> usize {
        self.output_index as usize
    }

    /// The amount claimed. Meaningless until [`Self::open_against`] has
    /// checked it against the commitment — this accessor exists for
    /// rendering a disclosure, never for trusting one.
    pub fn claimed_amount_micro(&self) -> u64 {
        self.amount_micro
    }

    /// Check this opening against the claim it names, returning the amount
    /// it proves.
    ///
    /// This is the whole verification: recompute `b·G_blind + v·H_val` and
    /// compare it to the commitment actually on the claim. Because Pedersen
    /// commitments are binding, a match means the disclosed amount is the
    /// committed one.
    pub fn open_against(&self, claim: &VerifiedPrivateClaim) -> Result<u64> {
        if claim.transcript_digest() != &self.claim_digest {
            return Err(PrivatePaymentError::DisclosedAmountMismatch);
        }
        let output = claim
            .claim()
            .outputs
            .get(self.output_index as usize)
            .ok_or(PrivatePaymentError::DisclosedAmountMismatch)?;
        let recomputed = mini_value::pedersen_commitment(self.amount_micro, &self.blinding)
            .ok_or(PrivatePaymentError::MalformedDisclosureKey)?;
        if recomputed.as_slice() != output.amount_commitment.as_slice() {
            return Err(PrivatePaymentError::DisclosedAmountMismatch);
        }
        Ok(self.amount_micro)
    }

    /// Canonical wire encoding.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(AMOUNT_DISCLOSURE_DOMAIN);
        w.u8(AMOUNT_DISCLOSURE_VERSION);
        w.raw(&self.claim_digest);
        w.u32(self.output_index);
        w.u64(self.amount_micro);
        w.raw(&self.blinding);
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let domain = r.array::<{ AMOUNT_DISCLOSURE_DOMAIN.len() }>()?;
        if domain != AMOUNT_DISCLOSURE_DOMAIN {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        if r.u8()? != AMOUNT_DISCLOSURE_VERSION {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        let claim_digest = r.array::<32>()?;
        let output_index = r.u32()?;
        let amount_micro = r.u64()?;
        let blinding = r.array::<32>()?;
        r.finish()?;
        Ok(AmountDisclosure {
            claim_digest,
            output_index,
            amount_micro,
            blinding,
        })
    }

    /// BLAKE3 over the whole encoding, domain included — so a disclosure can
    /// be referred to by digest and can never be replayed as some other
    /// domain-separated object.
    pub fn digest(&self) -> [u8; 32] {
        HashAlgorithm::Blake3.digest(&self.encode())
    }
}

/// One payment whose amount an auditor could actually check.
#[derive(Debug, Clone)]
pub struct OpenedPayment {
    /// The claim this payment arrived in.
    pub claim_digest: [u8; 32],
    /// Which output of that claim.
    pub output_index: usize,
    /// The proven amount — checked against the commitment, not asserted.
    pub amount_micro: u64,
    /// What the sender said it was for.
    pub purpose: PaymentPurpose,
}

/// What an amount audit actually established.
///
/// Deliberately not a `u64`. A bare total would be read as "this account's
/// income", and it is not: it is the part of the income the account chose to
/// open, which is a different claim that happens to look identical once the
/// structure around it is discarded.
#[derive(Debug, Clone, Default)]
pub struct AuditedIncome {
    /// Payments whose amounts were opened and verified.
    pub opened: Vec<OpenedPayment>,
    /// Recognized payments with no valid opening — the honesty field.
    ///
    /// A payment lands here whether the discloser withheld its opening or
    /// published one that failed to verify. Both are "the auditor could not
    /// establish this amount", and distinguishing them would invite reading
    /// the first as innocent.
    pub unopened: usize,
    /// Openings that named a claim not in the audited set at all. Not
    /// counted anywhere else, because they say nothing about this account.
    pub unmatched: usize,
}

impl AuditedIncome {
    /// The sum of what was opened. Read [`Self::is_complete`] first.
    pub fn opened_total_micro(&self) -> u128 {
        self.opened
            .iter()
            .map(|payment| u128::from(payment.amount_micro))
            .sum()
    }

    /// Whether every recognized payment was opened.
    ///
    /// **Only when this is true may [`Self::opened_total_micro`] be
    /// described as the account's income**, and even then only its *income*
    /// — a view key sees nothing an account spent, so no disclosure here
    /// produces a balance.
    pub fn is_complete(&self) -> bool {
        self.unopened == 0
    }
}

/// Audit an account's income *by amount*, over claims and published
/// openings.
///
/// `disclosure` establishes which payments are this account's;
/// `openings` are the amount disclosures it published. Every recognized
/// payment is accounted for exactly once: opened, or counted in
/// [`AuditedIncome::unopened`].
///
/// The two-step structure is the point. Recognition comes from the view key,
/// so the auditor learns the payment *count* from cryptography rather than
/// from the discloser's cooperation — and a withheld opening is then a
/// visible hole rather than a payment nobody knew to ask about.
pub fn audit_amounts<'a>(
    disclosure: &VerifiedDisclosure,
    claims: impl IntoIterator<Item = &'a VerifiedPrivateClaim>,
    openings: &[AmountDisclosure],
) -> AuditedIncome {
    let recognized = crate::disclosure::audit(disclosure, claims);

    let mut audited = AuditedIncome::default();
    let mut matched: Vec<[u8; 32]> = Vec::new();

    for payment in &recognized.payments {
        let digest = *payment.claim.transcript_digest();
        let opened = openings
            .iter()
            .filter(|opening| {
                opening.claim_digest == digest && opening.output_index() == payment.output_index
            })
            .find_map(|opening| {
                opening.open_against(&payment.claim).ok().map(|amount| {
                    matched.push(digest);
                    OpenedPayment {
                        claim_digest: digest,
                        output_index: payment.output_index,
                        amount_micro: amount,
                        purpose: payment.note.purpose.clone(),
                    }
                })
            });
        match opened {
            Some(payment) => audited.opened.push(payment),
            None => audited.unopened += 1,
        }
    }

    // A recognized-but-unreadable payment is still a payment this account
    // received, so it is unopened rather than absent.
    audited.unopened += recognized.unreadable.len();

    audited.unmatched = openings
        .iter()
        .filter(|opening| !matched.contains(&opening.claim_digest))
        .count();
    audited
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_acknowledgement_cannot_be_given_by_accident() {
        assert!(AcknowledgedAmountDisclosure::new("yes").is_none());
        assert!(AcknowledgedAmountDisclosure::new("").is_none());
        let almost = format!(
            "{}.",
            AcknowledgedAmountDisclosure::REQUIRED_ACKNOWLEDGEMENT
        );
        assert!(AcknowledgedAmountDisclosure::new(&almost).is_none());
        assert!(AcknowledgedAmountDisclosure::new(
            AcknowledgedAmountDisclosure::REQUIRED_ACKNOWLEDGEMENT
        )
        .is_some());
    }

    #[test]
    fn the_acknowledgement_names_the_third_party_exposure() {
        // The part a discloser would otherwise miss: the amount is not only
        // theirs to reveal. Softening this later is a real regression in
        // honesty, so it is pinned rather than left to review.
        let phrase = AcknowledgedAmountDisclosure::REQUIRED_ACKNOWLEDGEMENT;
        assert!(phrase.contains("permanent"));
        assert!(phrase.contains("sender"));
        assert!(phrase.contains("did not agree"));
    }

    #[test]
    fn an_empty_audit_is_complete_and_sums_to_nothing() {
        let audited = AuditedIncome::default();
        assert!(audited.is_complete());
        assert_eq!(audited.opened_total_micro(), 0);
    }

    #[test]
    fn one_unopened_payment_makes_the_total_incomplete() {
        let audited = AuditedIncome {
            opened: Vec::new(),
            unopened: 1,
            unmatched: 0,
        };
        assert!(!audited.is_complete());
    }
}
