//! Every way a replica claim, a registration receipt, or a conflict can fail
//! to be what it says it is.
//!
//! Decode failures are kept distinct from semantic failures on purpose: a
//! verifier that cannot tell "these bytes are malformed" from "this signature
//! is wrong" from "this quorum is too small" cannot report honestly on why it
//! refused something, and a caller cannot decide whether to re-fetch, re-audit,
//! or escalate.

/// Where a decode failed, without leaking the peer-supplied value itself.
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
    /// A DID string did not parse.
    InvalidDid,
    /// A signature suite tag or signature body was not valid.
    InvalidSignatureEncoding,
    /// Signature indices were unsorted or repeated, so the same claim would
    /// have had more than one valid wire encoding.
    NoncanonicalSignatureOrder,
    /// Attestations inside a receipt were unsorted or repeated, so the same
    /// receipt would have had more than one valid wire encoding.
    NoncanonicalAttestationOrder,
    /// A conflict's two claims arrived in the non-canonical order, so `(a, b)`
    /// and `(b, a)` would have been two different evidence objects.
    NoncanonicalConflictOrder,
}

/// Why a seal commitment is not structurally usable, independent of who signed
/// anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SealDefect {
    /// A replica covering zero nodes proves nothing and cannot be challenged.
    ZeroNodes,
    /// More nodes than this protocol profile admits.
    TooManyNodes { node_count: usize, max: usize },
    /// Zero stacked layers: no sequential sealing work is claimed at all.
    ZeroLayers,
    /// More layers than this protocol profile admits.
    TooManyLayers { num_layers: u32, max: u32 },
    /// `layer_roots` must hold exactly `num_layers + 1` roots (layer 0 through
    /// the final layer); anything else cannot describe a real sealing run.
    LayerRootCountMismatch { expected: usize, got: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FraudError {
    /// The wire bytes were not a well-formed object of the expected type.
    Decode(DecodeFailure),
    /// The seal commitment is structurally impossible.
    Seal(SealDefect),
    /// The seal commitment's `replica_id` is not the value this provider's
    /// identity and context derive to. This is the binding the whole scheme
    /// rests on: a replica whose id is not identity-derived says nothing about
    /// who sealed it.
    ReplicaIdNotIdentityBound,
    /// The claim's oracle could not produce a KEL for a DID the object names.
    UnknownIdentity,
    /// The named device is not delegated by the named root, or the delegation
    /// does not carry the capability the object exercises.
    DelegationRejected,
    /// The device is delegated but lacks [`did_mini::Capabilities::STORE`]
    /// (for a provider) or [`did_mini::Capabilities::ATTEST`] (for an
    /// auditor).
    MissingCapability,
    /// The claimed signing sequence does not exist in the signer's KEL, or the
    /// event digest cited for it does not match that KEL's event.
    SigningHistoryMismatch,
    /// No signatures, or too few valid ones for the signer's threshold at the
    /// sequence it claims to have signed under.
    BadSignature,
    /// A root tried to attest to its own registration.
    SelfAttestation,
    /// The attestation quorum names more than one seal commitment.
    AttestationTargetMismatch,
    /// Two attestations in one receipt reused the same challenge seed, so the
    /// quorum did not independently sample the replica.
    RepeatedChallengeSeed,
    /// An auditor sampled fewer challenges than policy requires.
    InsufficientAuditSampling { needed: u32, got: u32 },
    /// A sampled audit challenge went unanswered. An unanswered challenge is a
    /// failed challenge, never a skipped one.
    AuditUnanswered,
    /// A sampled audit challenge was answered with a response that does not
    /// satisfy `mini_porep`'s labeling construction.
    AuditFailed,
    /// Fewer distinct auditor identity roots than policy requires.
    InsufficientAuditQuorum { needed: u32, got: u32 },
    /// A registration policy was itself unusable (e.g. a zero quorum).
    InvalidPolicy,
    /// Two claims offered as a conflict do not actually conflict.
    NotAConflict,
    /// A registry was asked to admit a claim it already holds verbatim.
    AlreadyRegistered,
    /// A replica that is suspended or retired cannot be credited with a
    /// proof; re-entry means registering again.
    ReplicaNotProving,
    /// A window already credited was submitted again. Replaying one must
    /// not extend a proving streak or reverse a lapse.
    WindowAlreadyProven,
}

impl From<DecodeFailure> for FraudError {
    fn from(failure: DecodeFailure) -> Self {
        FraudError::Decode(failure)
    }
}

impl From<SealDefect> for FraudError {
    fn from(defect: SealDefect) -> Self {
        FraudError::Seal(defect)
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
            DecodeFailure::InvalidDid => write!(f, "invalid DID"),
            DecodeFailure::InvalidSignatureEncoding => write!(f, "invalid signature encoding"),
            DecodeFailure::NoncanonicalSignatureOrder => {
                write!(f, "signature indices are unsorted or repeated")
            }
            DecodeFailure::NoncanonicalAttestationOrder => {
                write!(f, "attestations are unsorted or repeated")
            }
            DecodeFailure::NoncanonicalConflictOrder => {
                write!(f, "conflicting claims are in non-canonical order")
            }
        }
    }
}

impl core::fmt::Display for SealDefect {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SealDefect::ZeroNodes => write!(f, "seal commitment covers zero nodes"),
            SealDefect::TooManyNodes { node_count, max } => {
                write!(f, "seal commitment covers {node_count} nodes, max {max}")
            }
            SealDefect::ZeroLayers => write!(f, "seal commitment claims zero stacked layers"),
            SealDefect::TooManyLayers { num_layers, max } => {
                write!(f, "seal commitment claims {num_layers} layers, max {max}")
            }
            SealDefect::LayerRootCountMismatch { expected, got } => write!(
                f,
                "seal commitment carries {got} layer roots, expected {expected}"
            ),
        }
    }
}

impl core::fmt::Display for FraudError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FraudError::Decode(failure) => write!(f, "decode failed: {failure}"),
            FraudError::Seal(defect) => write!(f, "malformed seal commitment: {defect}"),
            FraudError::ReplicaIdNotIdentityBound => write!(
                f,
                "seal commitment's replica id is not derived from this provider's identity and context"
            ),
            FraudError::UnknownIdentity => write!(f, "no KEL available for a named identity"),
            FraudError::DelegationRejected => {
                write!(f, "device is not delegated by the named root")
            }
            FraudError::MissingCapability => {
                write!(f, "device lacks the capability this object exercises")
            }
            FraudError::SigningHistoryMismatch => write!(
                f,
                "the claimed signing sequence or event digest is not in the signer's KEL"
            ),
            FraudError::BadSignature => write!(f, "bad or missing signature"),
            FraudError::SelfAttestation => {
                write!(f, "a provider root cannot attest to its own registration")
            }
            FraudError::AttestationTargetMismatch => {
                write!(f, "attestations name different seal commitments")
            }
            FraudError::RepeatedChallengeSeed => {
                write!(f, "two attestations reused one challenge seed")
            }
            FraudError::InsufficientAuditSampling { needed, got } => write!(
                f,
                "audit sampled {got} challenges, policy requires {needed}"
            ),
            FraudError::AuditUnanswered => write!(f, "an audit challenge went unanswered"),
            FraudError::AuditFailed => write!(f, "an audit challenge response did not verify"),
            FraudError::InsufficientAuditQuorum { needed, got } => write!(
                f,
                "registration has {got} distinct auditor roots, policy requires {needed}"
            ),
            FraudError::InvalidPolicy => write!(f, "unusable registration policy"),
            FraudError::NotAConflict => write!(f, "the two claims do not actually conflict"),
            FraudError::AlreadyRegistered => write!(f, "this exact claim is already registered"),
            FraudError::ReplicaNotProving => {
                write!(f, "replica is suspended or retired and cannot be credited")
            }
            FraudError::WindowAlreadyProven => {
                write!(f, "this proof window was already credited")
            }
        }
    }
}

impl std::error::Error for FraudError {}

pub type Result<T> = core::result::Result<T, FraudError>;
