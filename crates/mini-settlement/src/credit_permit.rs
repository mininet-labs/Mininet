//! Experimental online-issued offline credit authorization (not a guarantee).
//!
//! A permit signs one network, device key, allowance and bounded height window.
//! IOUs bind that permit, a sequence, one payee commitment and an amount. Signatures
//! prove authorization bytes, NOT remaining allowance, solvency or unique humanity.
//! The canonical host must admit permits under collective policy and track their
//! aggregate use/debt. A copied offline permit cannot prevent conflicting spending.
//!
//! This experimental encoding is NOT confidential: deliver only over an encrypted
//! channel. It must not enter the public ledger before private-credit proofs are
//! designed/audited. No production issuer, single authority key or mint is installed.

use mini_crypto::{HashAlgorithm, Signature, SignatureSuite, SigningKey, VerifyingKey};

const PERMIT_DOMAIN: &[u8] = b"mini-settlement/offline-credit-permit/v1";
const IOU_DOMAIN: &[u8] = b"mini-settlement/offline-credit-iou/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditSignatureError {
    UnsupportedSuite,
    InvalidTerms,
    WrongNetwork,
    WrongBorrower,
    WrongPermit,
    BadSignature,
    OutsideWindow,
    AllowanceExhausted,
    JournalFailure,
    MalformedEncoding,
}
impl core::fmt::Display for CreditSignatureError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "credit authorization refused: {self:?}")
    }
}
impl std::error::Error for CreditSignatureError {}

/// All terms are signed, including policy revision and a unique online serial.
#[derive(Clone, PartialEq, Eq)]
pub struct CreditPermitTerms {
    pub network_id: [u8; 32],
    pub policy_revision: [u8; 32],
    pub serial: [u8; 32],
    pub borrower_key: [u8; 32],
    pub limit_micro: u64,
    pub issued_height: u64,
    pub valid_through_height: u64,
}
impl CreditPermitTerms {
    fn message(&self) -> Vec<u8> {
        let mut out = PERMIT_DOMAIN.to_vec();
        for bytes in [
            &self.network_id,
            &self.policy_revision,
            &self.serial,
            &self.borrower_key,
        ] {
            out.extend_from_slice(bytes);
        }
        for n in [
            self.limit_micro,
            self.issued_height,
            self.valid_through_height,
        ] {
            out.extend_from_slice(&n.to_be_bytes());
        }
        out
    }
    fn validate(&self, max_height_span: u64) -> Result<(), CreditSignatureError> {
        if self.limit_micro == 0
            || self.serial == [0; 32]
            || self.policy_revision == [0; 32]
            || self.valid_through_height <= self.issued_height
            || self.valid_through_height - self.issued_height > max_height_span
        {
            return Err(CreditSignatureError::InvalidTerms);
        }
        VerifyingKey::from_suite_bytes(SignatureSuite::DEFAULT, &self.borrower_key)
            .map_err(|_| CreditSignatureError::InvalidTerms)?;
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SignedCreditPermit {
    pub terms: CreditPermitTerms,
    pub signature: Signature,
}

impl SignedCreditPermit {
    pub fn encode(&self) -> Result<Vec<u8>, CreditSignatureError> {
        if self.signature.suite() != SignatureSuite::DEFAULT {
            return Err(CreditSignatureError::UnsupportedSuite);
        }
        let mut bytes = self.terms.message();
        bytes.extend_from_slice(&self.signature.to_bytes());
        Ok(bytes)
    }
    /// Fixed-size, version/domain-tagged transport bytes. Decoding is not verification.
    pub fn decode(bytes: &[u8]) -> Result<Self, CreditSignatureError> {
        if bytes.len() != PERMIT_DOMAIN.len() + 128 + 24 + 64 || !bytes.starts_with(PERMIT_DOMAIN) {
            return Err(CreditSignatureError::MalformedEncoding);
        }
        let mut at = PERMIT_DOMAIN.len();
        let terms = CreditPermitTerms {
            network_id: take(bytes, &mut at),
            policy_revision: take(bytes, &mut at),
            serial: take(bytes, &mut at),
            borrower_key: take(bytes, &mut at),
            limit_micro: u64::from_be_bytes(take(bytes, &mut at)),
            issued_height: u64::from_be_bytes(take(bytes, &mut at)),
            valid_through_height: u64::from_be_bytes(take(bytes, &mut at)),
        };
        let signature = Signature::from_suite_bytes(SignatureSuite::DEFAULT, &bytes[at..])
            .map_err(|_| CreditSignatureError::MalformedEncoding)?;
        Ok(Self { terms, signature })
    }
}

fn take<const N: usize>(bytes: &[u8], at: &mut usize) -> [u8; N] {
    let result = bytes[*at..*at + N]
        .try_into()
        .expect("fixed encoding length checked");
    *at += N;
    result
}

/// Signature-checked only. Host must independently prove issuer authority,
/// canonical permit registration and policy/eligibility at the issuing checkpoint.
#[derive(Clone, PartialEq, Eq)]
pub struct VerifiedCreditPermit {
    terms: CreditPermitTerms,
    id: [u8; 32],
}
impl VerifiedCreditPermit {
    pub fn terms(&self) -> &CreditPermitTerms {
        &self.terms
    }
    pub fn id(&self) -> &[u8; 32] {
        &self.id
    }
}

pub fn sign_credit_permit(
    issuer: &SigningKey,
    terms: CreditPermitTerms,
    max_height_span: u64,
) -> Result<SignedCreditPermit, CreditSignatureError> {
    if issuer.suite() != SignatureSuite::DEFAULT {
        return Err(CreditSignatureError::UnsupportedSuite);
    }
    terms.validate(max_height_span)?;
    let signature = issuer.sign(&terms.message());
    Ok(SignedCreditPermit { terms, signature })
}

pub fn verify_credit_permit(
    permit: &SignedCreditPermit,
    authorized_issuer: &VerifyingKey,
    network_id: &[u8; 32],
    max_height_span: u64,
) -> Result<VerifiedCreditPermit, CreditSignatureError> {
    if authorized_issuer.suite() != SignatureSuite::DEFAULT
        || permit.signature.suite() != SignatureSuite::DEFAULT
    {
        return Err(CreditSignatureError::UnsupportedSuite);
    }
    if &permit.terms.network_id != network_id {
        return Err(CreditSignatureError::WrongNetwork);
    }
    permit.terms.validate(max_height_span)?;
    let message = permit.terms.message();
    authorized_issuer
        .verify(&message, &permit.signature)
        .map_err(|_| CreditSignatureError::BadSignature)?;
    Ok(VerifiedCreditPermit {
        terms: permit.terms.clone(),
        id: HashAlgorithm::Blake3.digest(&message),
    })
}

#[derive(Clone, PartialEq, Eq)]
pub struct CreditIouTerms {
    pub permit_id: [u8; 32],
    pub sequence: u64,
    /// Opaque commitment to the recipient's payment instruction; not a root DID.
    pub payee_commitment: [u8; 32],
    pub amount_micro: u64,
}
impl CreditIouTerms {
    fn message(&self) -> Vec<u8> {
        let mut out = IOU_DOMAIN.to_vec();
        out.extend_from_slice(&self.permit_id);
        out.extend_from_slice(&self.sequence.to_be_bytes());
        out.extend_from_slice(&self.payee_commitment);
        out.extend_from_slice(&self.amount_micro.to_be_bytes());
        out
    }
    fn validate(&self, permit: &VerifiedCreditPermit) -> Result<(), CreditSignatureError> {
        if &self.permit_id != permit.id() {
            return Err(CreditSignatureError::WrongPermit);
        }
        if self.amount_micro == 0
            || self.amount_micro > permit.terms.limit_micro
            || self.payee_commitment == [0; 32]
        {
            return Err(CreditSignatureError::InvalidTerms);
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SignedCreditIou {
    pub terms: CreditIouTerms,
    pub signature: Signature,
}
impl SignedCreditIou {
    pub fn encode(&self) -> Result<Vec<u8>, CreditSignatureError> {
        if self.signature.suite() != SignatureSuite::DEFAULT {
            return Err(CreditSignatureError::UnsupportedSuite);
        }
        let mut bytes = self.terms.message();
        bytes.extend_from_slice(&self.signature.to_bytes());
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, CreditSignatureError> {
        if bytes.len() != IOU_DOMAIN.len() + 32 + 8 + 32 + 8 + 64 || !bytes.starts_with(IOU_DOMAIN)
        {
            return Err(CreditSignatureError::MalformedEncoding);
        }
        let mut at = IOU_DOMAIN.len();
        let terms = CreditIouTerms {
            permit_id: take(bytes, &mut at),
            sequence: u64::from_be_bytes(take(bytes, &mut at)),
            payee_commitment: take(bytes, &mut at),
            amount_micro: u64::from_be_bytes(take(bytes, &mut at)),
        };
        let signature = Signature::from_suite_bytes(SignatureSuite::DEFAULT, &bytes[at..])
            .map_err(|_| CreditSignatureError::MalformedEncoding)?;
        Ok(Self { terms, signature })
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct VerifiedCreditIou {
    terms: CreditIouTerms,
    digest: [u8; 32],
}
impl VerifiedCreditIou {
    pub fn terms(&self) -> &CreditIouTerms {
        &self.terms
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

/// Per-IOU check only. Honest wallets must also durably track cumulative signed
/// use before transmitting; the canonical accounting engine enforces total use.
pub fn sign_credit_iou(
    borrower: &SigningKey,
    permit: &VerifiedCreditPermit,
    terms: CreditIouTerms,
) -> Result<SignedCreditIou, CreditSignatureError> {
    if borrower.suite() != SignatureSuite::DEFAULT {
        return Err(CreditSignatureError::UnsupportedSuite);
    }
    if borrower.verifying_key().to_bytes() != permit.terms.borrower_key {
        return Err(CreditSignatureError::WrongBorrower);
    }
    terms.validate(permit)?;
    let signature = borrower.sign(&terms.message());
    Ok(SignedCreditIou { terms, signature })
}

pub fn verify_credit_iou(
    iou: &SignedCreditIou,
    permit: &VerifiedCreditPermit,
    observed_height: u64,
) -> Result<VerifiedCreditIou, CreditSignatureError> {
    if iou.signature.suite() != SignatureSuite::DEFAULT {
        return Err(CreditSignatureError::UnsupportedSuite);
    }
    iou.terms.validate(permit)?;
    if observed_height < permit.terms.issued_height
        || observed_height > permit.terms.valid_through_height
    {
        return Err(CreditSignatureError::OutsideWindow);
    }
    let key = VerifyingKey::from_suite_bytes(SignatureSuite::DEFAULT, &permit.terms.borrower_key)
        .map_err(|_| CreditSignatureError::InvalidTerms)?;
    let message = iou.terms.message();
    key.verify(&message, &iou.signature)
        .map_err(|_| CreditSignatureError::BadSignature)?;
    Ok(VerifiedCreditIou {
        terms: iou.terms.clone(),
        digest: HashAlgorithm::Blake3.digest(&message),
    })
}

impl core::fmt::Debug for CreditPermitTerms {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CreditPermitTerms(<private>)")
    }
}

impl core::fmt::Debug for SignedCreditPermit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedCreditPermit(<private>)")
    }
}

impl core::fmt::Debug for VerifiedCreditPermit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("VerifiedCreditPermit(<private>)")
    }
}

impl core::fmt::Debug for CreditIouTerms {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CreditIouTerms(<private>)")
    }
}

impl core::fmt::Debug for SignedCreditIou {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedCreditIou(<private>)")
    }
}

impl core::fmt::Debug for VerifiedCreditIou {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("VerifiedCreditIou(<private>)")
    }
}

/// Local counters are a durable signing authorization, not a network balance.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct CreditUse {
    pub signed_micro: u64,
    pub next_sequence: u64,
}

/// Implementations MUST durably compare-and-record before returning success.
/// Losing/copying/rolling back this journal can cause conflicting offline IOUs;
/// canonical redemption still caps total debt but cannot protect both recipients.
pub trait CreditUseJournal {
    fn load(&self, permit_id: &[u8; 32]) -> Result<CreditUse, CreditSignatureError>;
    /// Atomically require current use == expected, save next use AND exact signed
    /// coupon in a recoverable outbox, then fsync/commit. False means a race.
    fn compare_and_record(
        &mut self,
        expected: CreditUse,
        next: CreditUse,
        coupon: &SignedCreditIou,
    ) -> Result<bool, CreditSignatureError>;
}

/// Honest-wallet signing path. Returning a coupon requires both remaining
/// allowance and a successful journal commit. Recovery retransmits the saved
/// coupon instead of signing a replacement after a crash.
pub fn sign_next_credit_iou(
    borrower: &SigningKey,
    permit: &VerifiedCreditPermit,
    amount_micro: u64,
    payee_commitment: [u8; 32],
    journal: &mut impl CreditUseJournal,
) -> Result<SignedCreditIou, CreditSignatureError> {
    let prior = journal.load(permit.id())?;
    let signed_micro = prior
        .signed_micro
        .checked_add(amount_micro)
        .ok_or(CreditSignatureError::AllowanceExhausted)?;
    if signed_micro > permit.terms.limit_micro {
        return Err(CreditSignatureError::AllowanceExhausted);
    }
    let next_sequence = prior
        .next_sequence
        .checked_add(1)
        .ok_or(CreditSignatureError::AllowanceExhausted)?;
    let coupon = sign_credit_iou(
        borrower,
        permit,
        CreditIouTerms {
            permit_id: *permit.id(),
            sequence: prior.next_sequence,
            payee_commitment,
            amount_micro,
        },
    )?;
    if !journal.compare_and_record(
        prior,
        CreditUse {
            signed_micro,
            next_sequence,
        },
        &coupon,
    )? {
        return Err(CreditSignatureError::JournalFailure);
    }
    Ok(coupon)
}

impl core::fmt::Debug for CreditUse {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CreditUse(<private>)")
    }
}
