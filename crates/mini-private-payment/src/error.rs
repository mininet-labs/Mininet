//! Every way a private payment can fail to be what it says it is.
//!
//! Decode failures stay distinct from cryptographic failures, and both stay
//! distinct from policy failures. A verifier that cannot tell "these bytes
//! are malformed" from "this range proof is false" from "this ring is too
//! small to hide anybody" cannot report honestly on why it refused a
//! payment, and a wallet cannot decide whether to re-fetch, warn, or drop.

/// Where a decode failed, without echoing the peer-supplied value back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeFailure {
    /// Wire bytes ended before a value they promised was fully read.
    Truncated,
    /// Wire bytes remained after decoding a value fully.
    TrailingBytes,
    /// A field exceeded a bound enforced before allocation.
    LimitExceeded,
    /// A length or count did not fit this platform's `usize`.
    LengthOutOfRange,
    /// An unrecognized encoding version tag.
    UnsupportedVersion,
    /// A curve point or scalar was not 32 bytes.
    BadFieldElement,
    /// A range proof was not exactly `mini_value::RANGE_PROOF_BYTES` long.
    BadRangeProof,
    /// Ring members were unsorted or repeated, so the same payment would
    /// have had more than one valid wire encoding — and a repeated member
    /// would inflate the apparent anonymity set for free.
    NoncanonicalRingOrder,
}

/// Why a private payment is refused. Ordered roughly structural → policy →
/// cryptographic, which is also the order [`crate::verify`] checks them:
/// the cheap refusals happen before any curve arithmetic, so a malformed
/// claim costs a verifier almost nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PrivatePaymentError {
    /// The wire bytes were not a well-formed claim.
    Decode(DecodeFailure),
    /// The claim names a different settlement network. A payment valid on
    /// a test network must never replay onto the real one.
    NetworkMismatch,
    /// The ring is smaller than [`crate::MIN_RING_SIZE`]. A ring of one
    /// names its signer outright; this is refused rather than warned about,
    /// because a payment that silently identifies its payer is worse than
    /// one that fails.
    RingTooSmall { got: usize, min: usize },
    /// The ring exceeds [`crate::MAX_RING_SIZE`].
    RingTooLarge { got: usize, max: usize },
    /// The same key appears twice in the ring. Duplicates cost the signer
    /// nothing and buy no anonymity, so they can only be an attempt to look
    /// better hidden than they are.
    DuplicateRingMember,
    /// The range proof does not show the committed amount is in bounds. A
    /// payment whose amount is not provably non-negative could mint value.
    BadRangeProof,
    /// The ring signature does not verify over this claim's transcript, so
    /// no member of the ring authorized *this* payment.
    BadRingSignature,
    /// The claim's own key image does not match its signature's. The key
    /// image is the double-spend nullifier; letting a claim carry one that
    /// its signature does not commit to would let a payer choose a fresh
    /// nullifier per broadcast and spend the same output repeatedly.
    KeyImageMismatch,
    /// A sealed memo could not be opened with the supplied view secret.
    /// Expected for payments that are not yours; an error rather than a
    /// bool so a wallet cannot confuse "not mine" with "corrupt".
    MemoNotForYou,
    /// The memo opened but its contents were not a well-formed purpose.
    MalformedMemo,
    /// A memo's plaintext exceeded [`crate::MAX_MEMO_BYTES`].
    MemoTooLarge { got: usize, max: usize },
    /// This key image was already spent. M1 in action: the second claim is
    /// refused outright, never merged with or netted against the first.
    AlreadySpent,
    /// A local cryptographic operation failed (CSPRNG, AEAD, KDF). Never
    /// caused by peer input.
    CryptoUnavailable,
}

impl From<DecodeFailure> for PrivatePaymentError {
    fn from(failure: DecodeFailure) -> Self {
        PrivatePaymentError::Decode(failure)
    }
}

impl core::fmt::Display for DecodeFailure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeFailure::Truncated => write!(f, "wire bytes ended early"),
            DecodeFailure::TrailingBytes => write!(f, "trailing bytes after decode"),
            DecodeFailure::LimitExceeded => write!(f, "value exceeded a bound"),
            DecodeFailure::LengthOutOfRange => write!(f, "length does not fit this platform"),
            DecodeFailure::UnsupportedVersion => write!(f, "unsupported encoding version"),
            DecodeFailure::BadFieldElement => write!(f, "field element is not 32 bytes"),
            DecodeFailure::BadRangeProof => write!(f, "range proof has the wrong length"),
            DecodeFailure::NoncanonicalRingOrder => {
                write!(f, "ring members are unsorted or repeated")
            }
        }
    }
}

impl core::fmt::Display for PrivatePaymentError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PrivatePaymentError::Decode(failure) => write!(f, "decode failed: {failure}"),
            PrivatePaymentError::NetworkMismatch => {
                write!(f, "claim is for a different settlement network")
            }
            PrivatePaymentError::RingTooSmall { got, min } => {
                write!(f, "ring of {got} hides nobody, minimum is {min}")
            }
            PrivatePaymentError::RingTooLarge { got, max } => {
                write!(f, "ring of {got} exceeds the maximum {max}")
            }
            PrivatePaymentError::DuplicateRingMember => {
                write!(f, "a ring member is repeated, inflating apparent anonymity")
            }
            PrivatePaymentError::BadRangeProof => {
                write!(f, "the committed amount is not provably in range")
            }
            PrivatePaymentError::BadRingSignature => {
                write!(f, "no ring member authorized this exact claim")
            }
            PrivatePaymentError::KeyImageMismatch => {
                write!(f, "the claim's key image is not the signature's")
            }
            PrivatePaymentError::MemoNotForYou => write!(f, "memo is not addressed to this viewer"),
            PrivatePaymentError::MalformedMemo => write!(f, "memo plaintext is malformed"),
            PrivatePaymentError::MemoTooLarge { got, max } => {
                write!(f, "memo is {got} bytes, maximum {max}")
            }
            PrivatePaymentError::AlreadySpent => {
                write!(f, "this key image was already spent")
            }
            PrivatePaymentError::CryptoUnavailable => {
                write!(f, "a local cryptographic operation failed")
            }
        }
    }
}

impl std::error::Error for PrivatePaymentError {}

pub type Result<T> = core::result::Result<T, PrivatePaymentError>;
