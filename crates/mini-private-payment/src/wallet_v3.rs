//! V3 wallet scanning and amount disclosure (research PR #429, P1-1 / P1-2).
//!
//! The V2 [`crate::scan`] and [`crate::amount`] modules work over V2 claims and
//! the V2 commitment basis. V3 outputs commit under the `bp_range_v2` basis, so
//! they need their own scan and opening check; a V2 opening can never verify
//! against a V3 commitment (and vice versa), by construction.
//!
//! What stays true from V2, and is tested here:
//!
//! - Scanning is **local**: it takes a view secret and claims the caller already
//!   holds. It never asks a peer "is this one-time output mine?", the oracle
//!   the research names as a stop signal.
//! - A scan **cannot be blinded**: one output addressed here whose memo will not
//!   open lands in [`ScanOutcomeV3::unreadable`] instead of failing the scan,
//!   so a stranger cannot hide a wallet's other income by sending it one bad
//!   payment.
//! - **A view key reads amounts.** The memo carrying `(amount, blinding)` is
//!   sealed under the secret a view key recovers, so anyone holding a published
//!   view key can open every recognized output's amount and its commitment
//!   opening. There is no separation between recognizing a payment and reading
//!   it, and this module does not pretend otherwise. (The V2 documentation
//!   claimed amounts stay hidden under a view key; that was wrong for the same
//!   reason and is corrected with this change.) Publishing a view key is
//!   therefore a full, permanent income disclosure including amounts.
//! - **A scanned note is checked against its commitment.** A malicious payer can
//!   seal a memo whose `(amount, blinding)` do not open the output's
//!   commitment, and the claim still verifies because the memo is encrypted. A
//!   recipient that credited the stated amount would later find the output
//!   unspendable, so a note that does not open its commitment is `unreadable`,
//!   never a payment.
//! - **Amount openings** ([`AmountDisclosureV3`]) are for a recipient proving
//!   one payment to a party who does *not* hold the view key. An audit that is
//!   built on a view-key disclosure sees amounts regardless, so for that case
//!   [`AuditedIncomeV3::unopened`] counts payments with no published opening,
//!   not payments whose amount is hidden.

use mini_crypto::HashAlgorithm;
use mini_value::pedersen_commitment_v2;

use crate::amount::AcknowledgedAmountDisclosure;
use crate::claim_v3::{recover_shared_secret_v3, VerifiedPrivateClaimV3};
use crate::codec::{Reader, Writer};
use crate::disclosure::VerifiedDisclosure;
use crate::error::{DecodeFailure, PrivatePaymentError, Result};
use crate::memo::{PaymentNote, PaymentPurpose};

/// One V3 output that pays this wallet, with its opened memo.
#[derive(Debug, Clone)]
pub struct RecognizedPaymentV3 {
    /// The verified claim.
    pub claim: VerifiedPrivateClaimV3,
    /// Which output pays this wallet.
    pub output_index: usize,
    /// The sender's note: purpose, amount and the opening blinding.
    pub note: PaymentNote,
}

/// Whether any output of `claim` pays the holder of these keys.
pub fn recognizes_v3(
    view_secret: &[u8],
    spend_public: &[u8],
    claim: &VerifiedPrivateClaimV3,
) -> bool {
    claim.claim().outputs.iter().any(|output| {
        recover_shared_secret_v3(&claim.claim().network_id, view_secret, spend_public, output)
            .is_some()
    })
}

/// Scan one claim output by output. Readable outputs are kept even when another
/// output of the same claim is malformed; the flag reports that at least one
/// output addressed here could not be read (its memo would not open, or its
/// note did not open its commitment).
fn scan_claim_v3(
    view_secret: &[u8],
    spend_public: &[u8],
    claim: &VerifiedPrivateClaimV3,
) -> (Vec<RecognizedPaymentV3>, bool) {
    let mut found = Vec::new();
    let mut anomaly = false;
    for (index, output) in claim.claim().outputs.iter().enumerate() {
        let Some(shared) =
            recover_shared_secret_v3(&claim.claim().network_id, view_secret, spend_public, output)
        else {
            continue;
        };
        let Ok(note) = claim.open_memo(index, &shared) else {
            anomaly = true;
            continue;
        };
        // The memo is encrypted, so verification could not check it: a payer
        // can seal an amount and blinding that do not open the commitment.
        let opens = pedersen_commitment_v2(note.amount_micro, &note.blinding)
            .is_some_and(|c| c.as_slice() == output.amount_commitment.as_slice());
        if !opens {
            anomaly = true;
            continue;
        }
        found.push(RecognizedPaymentV3 {
            claim: claim.clone(),
            output_index: index,
            note,
        });
    }
    (found, anomaly)
}

/// Recognize payments in one claim and open their memos. `Ok(vec![])` means
/// "not mine"; `Err` means an output is addressed here but unreadable. Use
/// [`scan_v3`] to keep the readable outputs of such a claim.
pub fn scan_one_v3(
    view_secret: &[u8],
    spend_public: &[u8],
    claim: &VerifiedPrivateClaimV3,
) -> Result<Vec<RecognizedPaymentV3>> {
    let (found, anomaly) = scan_claim_v3(view_secret, spend_public, claim);
    if anomaly {
        return Err(PrivatePaymentError::MalformedMemo);
    }
    Ok(found)
}

/// What scanning a batch found.
#[derive(Debug, Clone, Default)]
pub struct ScanOutcomeV3 {
    /// Payments to these keys whose memos opened.
    pub payments: Vec<RecognizedPaymentV3>,
    /// Claims with at least one output addressed to these keys that could not
    /// be read. Such a claim may also appear in `payments` for its readable
    /// outputs. The value is still there; only the stated purpose is missing.
    pub unreadable: Vec<VerifiedPrivateClaimV3>,
}

impl ScanOutcomeV3 {
    /// Every distinct claim addressed to these keys, readable or not.
    pub fn all_claims(&self) -> impl Iterator<Item = &VerifiedPrivateClaimV3> {
        let mut seen = std::collections::BTreeSet::new();
        self.payments
            .iter()
            .map(|payment| &payment.claim)
            .chain(self.unreadable.iter())
            .filter(move |claim| seen.insert(*claim.claim_id()))
    }
}

/// Scan a batch. Deliberately cannot fail (see the module docs).
pub fn scan_v3<'a>(
    view_secret: &[u8],
    spend_public: &[u8],
    claims: impl IntoIterator<Item = &'a VerifiedPrivateClaimV3>,
) -> ScanOutcomeV3 {
    let mut outcome = ScanOutcomeV3::default();
    for claim in claims {
        let (payments, anomaly) = scan_claim_v3(view_secret, spend_public, claim);
        outcome.payments.extend(payments);
        if anomaly {
            outcome.unreadable.push(claim.clone());
        }
    }
    outcome
}

/// Domain separator for a V3 amount disclosure's encoding.
pub const AMOUNT_DISCLOSURE_V3_DOMAIN: &[u8] = b"mininet/mini-private-payment/amount-disclosure/v3";

/// Wire format version.
pub const AMOUNT_DISCLOSURE_V3_VERSION: u8 = 3;

/// A published opening of one V3 output's amount. Names the claim by
/// `claim_id` (the complete-bytes identity), not a partial digest.
#[derive(Clone, PartialEq, Eq)]
pub struct AmountDisclosureV3 {
    claim_id: [u8; 32],
    output_index: u32,
    amount_micro: u64,
    blinding: [u8; 32],
}

impl core::fmt::Debug for AmountDisclosureV3 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AmountDisclosureV3")
            .field("output_index", &self.output_index)
            .field("amount_micro", &self.amount_micro)
            .field("blinding", &"<redacted>")
            .finish()
    }
}

impl AmountDisclosureV3 {
    /// Publish the opening of one output the caller was paid. Refuses to build
    /// an opening that does not check out against the claim.
    pub fn create(
        claim: &VerifiedPrivateClaimV3,
        output_index: usize,
        note: &PaymentNote,
        _acknowledged: &AcknowledgedAmountDisclosure,
    ) -> Result<Self> {
        let index = u32::try_from(output_index).map_err(|_| PrivatePaymentError::MalformedMemo)?;
        let disclosure = Self {
            claim_id: *claim.claim_id(),
            output_index: index,
            amount_micro: note.amount_micro,
            blinding: note.blinding,
        };
        disclosure.open_against(claim)?;
        Ok(disclosure)
    }

    /// The claim this opens.
    pub fn claim_id(&self) -> &[u8; 32] {
        &self.claim_id
    }

    /// The output within the claim.
    pub fn output_index(&self) -> usize {
        self.output_index as usize
    }

    /// The amount claimed; meaningless until [`Self::open_against`] succeeds.
    pub fn claimed_amount_micro(&self) -> u64 {
        self.amount_micro
    }

    /// Check the opening against the commitment actually on the claim.
    pub fn open_against(&self, claim: &VerifiedPrivateClaimV3) -> Result<u64> {
        if claim.claim_id() != &self.claim_id {
            return Err(PrivatePaymentError::DisclosedAmountMismatch);
        }
        let output = claim
            .claim()
            .outputs
            .get(self.output_index as usize)
            .ok_or(PrivatePaymentError::DisclosedAmountMismatch)?;
        let recomputed = pedersen_commitment_v2(self.amount_micro, &self.blinding)
            .ok_or(PrivatePaymentError::MalformedDisclosureKey)?;
        if recomputed.as_slice() != output.amount_commitment.as_slice() {
            return Err(PrivatePaymentError::DisclosedAmountMismatch);
        }
        Ok(self.amount_micro)
    }

    /// Canonical wire encoding.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(AMOUNT_DISCLOSURE_V3_DOMAIN);
        w.u8(AMOUNT_DISCLOSURE_V3_VERSION);
        w.raw(&self.claim_id);
        w.u32(self.output_index);
        w.u64(self.amount_micro);
        w.raw(&self.blinding);
        w.finish()
    }

    /// Strict decode.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let domain = r.array::<{ AMOUNT_DISCLOSURE_V3_DOMAIN.len() }>()?;
        if domain != AMOUNT_DISCLOSURE_V3_DOMAIN || r.u8()? != AMOUNT_DISCLOSURE_V3_VERSION {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        let claim_id = r.array::<32>()?;
        let output_index = r.u32()?;
        let amount_micro = r.u64()?;
        let blinding = r.array::<32>()?;
        r.finish()?;
        Ok(Self {
            claim_id,
            output_index,
            amount_micro,
            blinding,
        })
    }

    /// BLAKE3 over the whole encoding, domain included.
    pub fn digest(&self) -> [u8; 32] {
        HashAlgorithm::Blake3.digest(&self.encode())
    }
}

/// One payment whose amount an auditor could check.
#[derive(Debug, Clone)]
pub struct OpenedPaymentV3 {
    /// The claim.
    pub claim_id: [u8; 32],
    /// Which output.
    pub output_index: usize,
    /// The proven amount.
    pub amount_micro: u64,
    /// What the sender said it was for.
    pub purpose: PaymentPurpose,
}

/// What an amount audit actually established. Never a bare total.
#[derive(Debug, Clone, Default)]
pub struct AuditedIncomeV3 {
    /// Payments whose amounts were opened and verified.
    pub opened: Vec<OpenedPaymentV3>,
    /// Recognized payments with no valid opening.
    pub unopened: usize,
    /// Openings naming a claim not in the audited set.
    pub unmatched: usize,
}

impl AuditedIncomeV3 {
    /// The sum of what was opened. Read [`Self::is_complete`] first.
    pub fn opened_total_micro(&self) -> u128 {
        self.opened
            .iter()
            .map(|payment| u128::from(payment.amount_micro))
            .sum()
    }

    /// Whether every recognized payment was opened. Only then may the opened
    /// total be called the account's income, and even then only income.
    pub fn is_complete(&self) -> bool {
        self.unopened == 0
    }
}

/// Cross-check published openings against V3 claims. The disclosure's view key
/// establishes which payments are this account's, and (see the module docs) also
/// lets the auditor read their amounts; this counts payments that have no
/// verified published opening, and never reports a partial total as income.
pub fn audit_amounts_v3<'a>(
    disclosure: &VerifiedDisclosure,
    claims: impl IntoIterator<Item = &'a VerifiedPrivateClaimV3>,
    openings: &[AmountDisclosureV3],
) -> AuditedIncomeV3 {
    let recognized = scan_v3(
        disclosure.disclosure().view_secret(),
        disclosure.disclosure().spend_public(),
        claims,
    );
    let mut audited = AuditedIncomeV3::default();
    let mut matched: Vec<[u8; 32]> = Vec::new();
    // The same claim fed twice (for example from merged replicated feeds) must
    // not be counted twice: one payment is one (claim_id, output_index).
    let mut counted: std::collections::BTreeSet<([u8; 32], usize)> =
        std::collections::BTreeSet::new();
    for payment in &recognized.payments {
        let id = *payment.claim.claim_id();
        if !counted.insert((id, payment.output_index)) {
            continue;
        }
        let opened = openings
            .iter()
            .filter(|opening| {
                opening.claim_id == id && opening.output_index() == payment.output_index
            })
            .find_map(|opening| {
                opening.open_against(&payment.claim).ok().map(|amount| {
                    matched.push(id);
                    OpenedPaymentV3 {
                        claim_id: id,
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
    let unreadable: std::collections::BTreeSet<[u8; 32]> = recognized
        .unreadable
        .iter()
        .map(|claim| *claim.claim_id())
        .collect();
    audited.unopened += unreadable.len();
    audited.unmatched = openings
        .iter()
        .filter(|opening| !matched.contains(&opening.claim_id))
        .count();
    audited
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim_v3::{
        build_v3, quote_fee_micro, verify_v3, PaymentRequestV3, RecipientV3, SpendableOutputV3,
        RING_SIZE_V3,
    };
    use crate::decoy::{InMemoryOutputSet, OutputSet};
    use mini_value::StealthKeypair;

    const NETWORK: [u8; 32] = [0x5a; 32];
    const POLICY: [u8; 32] = [0x11; 32];

    struct Set(InMemoryOutputSet);
    impl OutputSet for Set {
        fn len(&self) -> usize {
            self.0.len()
        }
        fn key_at(&self, i: usize) -> Option<Vec<u8>> {
            self.0.key_at(i)
        }
        fn commitment_at(&self, i: usize) -> Option<Vec<u8>> {
            self.0.commitment_at(i)
        }
    }

    fn pay(to: &StealthKeypair, amount: u64) -> VerifiedPrivateClaimV3 {
        let mut set = InMemoryOutputSet::new();
        let add = |set: &mut InMemoryOutputSet, value: u64| {
            let key = StealthKeypair::generate().unwrap();
            let blinding = mini_value::random_scalar_bytes().unwrap();
            let c = mini_value::pedersen_commitment_v2(value, &blinding).unwrap();
            set.push(key.spend_public_bytes().to_vec(), c.to_vec());
            (key, blinding)
        };
        for _ in 0..RING_SIZE_V3 * 4 {
            add(&mut set, 1_000);
        }
        let fee = quote_fee_micro(&POLICY, 1, 1);
        let (key, blinding) = add(&mut set, amount + fee);
        let spend = SpendableOutputV3 {
            set_index: set.len() - 1,
            one_time_secret: key.spend_secret_bytes(),
            value_micro: amount + fee,
            blinding,
        };
        for _ in 0..4 {
            add(&mut set, 1_000);
        }
        let (claim, _) = build_v3(
            &PaymentRequestV3 {
                network_id: NETWORK,
                fee_policy_id: POLICY,
                spends: vec![spend],
                recipients: vec![RecipientV3 {
                    spend_public: to.spend_public_bytes().to_vec(),
                    view_public: to.view_public_bytes().to_vec(),
                    amount_micro: amount,
                    purpose: PaymentPurpose::new(b"p".to_vec()),
                }],
            },
            &Set(set),
        )
        .unwrap();
        verify_v3(&claim, &NETWORK).unwrap()
    }

    #[test]
    fn a_note_that_does_not_open_its_commitment_is_never_a_payment() {
        let me = StealthKeypair::generate().unwrap();
        let claim = pay(&me, 3_000);
        let mut inner = claim.claim().clone();
        let shared = recover_shared_secret_v3(
            &NETWORK,
            &me.view_secret_bytes(),
            &me.spend_public_bytes(),
            &inner.outputs[0],
        )
        .unwrap();
        // A malicious payer seals a larger amount with the right context.
        let digest = inner.memo_context_digest(0);
        let context = crate::memo_v3::MemoContextV3 {
            network_id: &inner.network_id,
            memo_context_digest: &digest,
            output_index: 0,
            tx_public_key: &inner.outputs[0].tx_public_key,
            one_time_address: &inner.outputs[0].one_time_address,
            amount_commitment: &inner.outputs[0].amount_commitment,
        };
        let forged = PaymentNote::new(
            PaymentPurpose::new(b"forged".to_vec()),
            9_000_000,
            [7u8; 32],
        );
        inner.outputs[0].sealed_memo =
            crate::memo_v3::SealedMemoV3::seal(&forged, &shared, &context).unwrap();
        let wrapped = VerifiedPrivateClaimV3::unverified_for_test(inner);
        assert!(scan_one_v3(&me.view_secret_bytes(), &me.spend_public_bytes(), &wrapped).is_err());
        let outcome = scan_v3(
            &me.view_secret_bytes(),
            &me.spend_public_bytes(),
            [&wrapped],
        );
        assert!(outcome.payments.is_empty());
        assert_eq!(outcome.unreadable.len(), 1);
    }

    #[test]
    fn a_malformed_output_does_not_hide_a_readable_one_in_the_same_claim() {
        let me = StealthKeypair::generate().unwrap();
        let fee = quote_fee_micro(&POLICY, 1, 2);
        let mut set = InMemoryOutputSet::new();
        let add = |set: &mut InMemoryOutputSet, value: u64| {
            let key = StealthKeypair::generate().unwrap();
            let blinding = mini_value::random_scalar_bytes().unwrap();
            let c = mini_value::pedersen_commitment_v2(value, &blinding).unwrap();
            set.push(key.spend_public_bytes().to_vec(), c.to_vec());
            (key, blinding)
        };
        for _ in 0..RING_SIZE_V3 * 4 {
            add(&mut set, 1_000);
        }
        let (key, blinding) = add(&mut set, 10_000 + fee);
        let spend = SpendableOutputV3 {
            set_index: set.len() - 1,
            one_time_secret: key.spend_secret_bytes(),
            value_micro: 10_000 + fee,
            blinding,
        };
        for _ in 0..4 {
            add(&mut set, 1_000);
        }
        let recipient = |amount: u64| RecipientV3 {
            spend_public: me.spend_public_bytes().to_vec(),
            view_public: me.view_public_bytes().to_vec(),
            amount_micro: amount,
            purpose: PaymentPurpose::new(b"two".to_vec()),
        };
        let (claim, _) = build_v3(
            &PaymentRequestV3 {
                network_id: NETWORK,
                fee_policy_id: POLICY,
                spends: vec![spend],
                recipients: vec![recipient(4_000), recipient(6_000)],
            },
            &Set(set),
        )
        .unwrap();
        let donor = pay(&StealthKeypair::generate().unwrap(), 1_000);
        let mut inner = verify_v3(&claim, &NETWORK).unwrap().claim().clone();
        inner.outputs[1].sealed_memo = donor.claim().outputs[0].sealed_memo.clone();
        let wrapped = VerifiedPrivateClaimV3::unverified_for_test(inner);
        let outcome = scan_v3(
            &me.view_secret_bytes(),
            &me.spend_public_bytes(),
            [&wrapped],
        );
        assert_eq!(outcome.payments.len(), 1, "the readable output is kept");
        assert_eq!(outcome.payments[0].note.amount_micro, 4_000);
        assert_eq!(outcome.unreadable.len(), 1, "and the anomaly is reported");
        assert_eq!(outcome.all_claims().count(), 1);
    }

    #[test]
    fn one_unreadable_payment_cannot_blind_a_wallet_to_its_other_income() {
        let me = StealthKeypair::generate().unwrap();
        let good = pay(&me, 2_000);
        let donor = pay(&StealthKeypair::generate().unwrap(), 1_000);
        // A payment addressed to this wallet whose memo was sealed to someone
        // else (only a malicious sender can produce one that also verifies).
        let mut bad_inner = pay(&me, 3_000).claim().clone();
        bad_inner.outputs[0].sealed_memo = donor.claim().outputs[0].sealed_memo.clone();
        let bad = VerifiedPrivateClaimV3::unverified_for_test(bad_inner);

        assert!(scan_one_v3(&me.view_secret_bytes(), &me.spend_public_bytes(), &bad).is_err());
        let outcome = scan_v3(
            &me.view_secret_bytes(),
            &me.spend_public_bytes(),
            [&good, &bad],
        );
        assert_eq!(outcome.payments.len(), 1);
        assert_eq!(outcome.payments[0].note.amount_micro, 2_000);
        assert_eq!(outcome.unreadable.len(), 1);
        assert_eq!(outcome.all_claims().count(), 2);
    }
}
