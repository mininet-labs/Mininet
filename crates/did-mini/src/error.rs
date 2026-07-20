//! Error types for `did-mini`.

use core::fmt;
use mini_crypto::CryptoError;

/// Errors produced while building, encoding, or verifying a `did:mini` identity.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdentityError {
    /// A cryptographic primitive failed (bad key/signature length, etc.).
    Crypto(CryptoError),
    /// The byte buffer ended before a field could be fully read.
    Truncated,
    /// Bytes remained after a structure was fully decoded.
    TrailingBytes,
    /// An event carried an unrecognised type tag.
    UnknownEventTag(u8),
    /// The Key Event Log had no events.
    EmptyKel,
    /// The first event was not a well-formed inception (`icp`, sn 0, no prior),
    /// or a later event claimed to be an inception.
    NotInception,
    /// An event's sequence number did not follow the previous one.
    WrongSequence { expected: u64, got: u64 },
    /// The self-certifying identifier did not match the inception it claims to
    /// derive from — the identity is not authentic.
    ScidMismatch,
    /// An event's `prior` digest did not match the previous event — the log was
    /// tampered with or reordered.
    BrokenChain { sn: u64 },
    /// A rotation revealed keys that do not match the prior pre-rotation
    /// commitment — the rotation is not authorised by the legitimate controller.
    PreRotationMismatch { sn: u64 },
    /// Too few valid signatures from the authoritative keys to meet the
    /// signing threshold.
    ThresholdNotMet { sn: u64, needed: u32, got: u32 },
    /// A structurally invalid event (e.g. a non-UTF-8 identifier, a malformed
    /// establishment, or an out-of-spec field).
    BadEvent,
    /// A decoded field was larger than this wire profile permits.
    FieldTooLarge {
        field: &'static str,
        max: usize,
        got: usize,
    },
    /// A decoded vector had too many entries for this wire profile.
    TooManyItems {
        field: &'static str,
        max: usize,
        got: usize,
    },
    /// An establishment event had no keys.
    EmptyKeySet,
    /// An establishment threshold was zero or larger than the key set.
    InvalidThreshold { threshold: u32, key_count: usize },
    /// An establishment event repeated the same public key.
    DuplicateKey,
    /// A pre-rotation commitment set was empty or had an invalid next threshold.
    InvalidNextThreshold {
        threshold: u32,
        commitment_count: usize,
    },
    /// A string was not a valid `did:mini:<scid>` identifier.
    DidFormat,
    /// A device is not (or no longer) delegated by the claimed human-root, or the
    /// device does not name that root as its delegator (SPEC-01 §6).
    NotDelegated,
    /// Detached signatures over a message did not reach the identity's threshold.
    SignatureThresholdNotMet {
        /// Distinct valid signatures required.
        needed: u32,
        /// Distinct valid signatures found.
        got: u32,
    },
    /// A pairwise pseudonym was requested from a multi-key/threshold root —
    /// there is no canonical single key to derive from.
    PairwiseRequiresSingleKey,
    /// The claimed delegator is itself a delegated (device) identity. Delegation
    /// chains are rejected: every device must chain to a true (non-delegated)
    /// root, or "one identity root" counting could be handed a device posing as
    /// a root (SPEC-01 §6; device hierarchies are roadmap #14, not implicit).
    RootIsDelegated,
    /// Recovery keys did not match the KEL's standing pre-rotation commitments —
    /// whoever supplied them does not hold the committed next keys.
    RecoveryKeysMismatch,
    /// A KEL's sequence number was lower than one this verifier has already
    /// pinned for the same SCID — the interim freshness rule
    /// ([`crate::FreshnessPins`]) rejecting a stale replay.
    StaleKel { pinned: u64, got: u64 },
    /// A [`crate::WitnessPolicy`] had no witnesses, or a
    /// [`crate::WitnessedEventCertificate`] had no receipts.
    EmptyWitnessSet,
    /// A [`crate::WitnessPolicy`]'s threshold was zero or larger than its
    /// witness count.
    InvalidWitnessThreshold {
        threshold: u16,
        witness_count: usize,
    },
    /// A [`crate::WitnessPolicy`] repeated the same witness identifier.
    DuplicateWitness,
    /// A decoded [`crate::WitnessReceiptStatement`]/[`crate::
    /// WitnessReceipt`] carried an unrecognised version tag.
    UnknownWitnessReceiptVersion(u8),
    /// A decoded [`crate::WitnessedEventCertificate`] carried an
    /// unrecognised version tag.
    UnknownWitnessCertificateVersion(u8),
    /// A decoded [`crate::WitnessReceiptStatement`] carried an
    /// unrecognised [`crate::KeyEventKind`] tag.
    UnknownKeyEventKindTag(u8),
    /// A [`crate::WitnessReceipt`] admitted into (or checked against) a
    /// [`crate::WitnessedEventCertificate`] claimed a different identity,
    /// sequence, event digest, or witness-policy generation than the
    /// certificate itself.
    WitnessReceiptMismatch,
    /// A [`crate::WitnessedEventCertificate`] carried a receipt from a
    /// witness that is not a member of the [`crate::WitnessPolicy`] it
    /// was checked against.
    WitnessNotInPolicy,
    /// A [`crate::WitnessedEventCertificate`]'s claimed
    /// `witness_policy_generation` did not match the [`crate::
    /// WitnessPolicy`] it was checked against.
    WitnessPolicyGenerationMismatch { expected: u64, got: u64 },
    /// [`crate::WitnessedEventCertificate::verify`]'s caller-supplied
    /// resolver could not produce a verifying key for a witness named in
    /// the certificate.
    UnresolvedWitnessKey,
    /// A [`crate::WitnessedEventCertificate`] did not carry enough
    /// distinct, valid witness signatures to meet its policy's threshold.
    WitnessThresholdNotMet { needed: u16, got: u16 },
    /// [`crate::WitnessJournal::observe`] was given an event that neither
    /// matches, extends, precedes, nor conflicts-at-the-same-sequence with
    /// this witness's accepted state for the identity — e.g. it claims a
    /// later sequence but its `prior` digest does not match what this
    /// witness actually accepted. Phase 2 rejects this outright rather
    /// than attempting the fork-proof construction the research report's
    /// harder "conflicting descendant" case describes; that remains
    /// future work.
    WitnessConflictingDescendant { sequence: u64 },
    /// [`crate::ControllerDuplicityProof::assemble`]'s two events did not
    /// actually demonstrate duplicity: different identity, different
    /// sequence, or identical digest.
    ControllerDuplicityMismatch,
    /// [`crate::WitnessEquivocationProof::assemble`]'s two receipts did
    /// not actually demonstrate equivocation: different witness, identity,
    /// sequence, or policy generation, or an identical event digest.
    WitnessEquivocationMismatch,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdentityError::Crypto(e) => write!(f, "crypto error: {e}"),
            IdentityError::Truncated => write!(f, "buffer truncated"),
            IdentityError::TrailingBytes => write!(f, "unexpected trailing bytes"),
            IdentityError::UnknownEventTag(t) => write!(f, "unknown event tag: 0x{t:02x}"),
            IdentityError::EmptyKel => write!(f, "empty key event log"),
            IdentityError::NotInception => write!(f, "malformed or misplaced inception event"),
            IdentityError::WrongSequence { expected, got } => {
                write!(f, "out-of-order event: expected sn {expected}, got {got}")
            }
            IdentityError::ScidMismatch => {
                write!(f, "scid does not match inception (not authentic)")
            }
            IdentityError::BrokenChain { sn } => write!(f, "broken kel chain at sn {sn}"),
            IdentityError::PreRotationMismatch { sn } => {
                write!(
                    f,
                    "rotation at sn {sn} does not match pre-rotation commitment"
                )
            }
            IdentityError::ThresholdNotMet { sn, needed, got } => {
                write!(
                    f,
                    "signing threshold not met at sn {sn}: needed {needed}, got {got}"
                )
            }
            IdentityError::BadEvent => write!(f, "structurally invalid event"),
            IdentityError::FieldTooLarge { field, max, got } => {
                write!(
                    f,
                    "field {field} too large: max {max} bytes/items, got {got}"
                )
            }
            IdentityError::TooManyItems { field, max, got } => {
                write!(f, "too many {field}: max {max}, got {got}")
            }
            IdentityError::EmptyKeySet => write!(f, "establishment event has no keys"),
            IdentityError::InvalidThreshold {
                threshold,
                key_count,
            } => write!(
                f,
                "invalid threshold {threshold} for key set of size {key_count}"
            ),
            IdentityError::DuplicateKey => write!(f, "establishment event repeats a public key"),
            IdentityError::InvalidNextThreshold {
                threshold,
                commitment_count,
            } => write!(
                f,
                "invalid next threshold {threshold} for {commitment_count} commitments"
            ),
            IdentityError::DidFormat => write!(f, "not a valid did:mini identifier"),
            IdentityError::NotDelegated => write!(f, "device is not delegated by this root"),
            IdentityError::SignatureThresholdNotMet { needed, got } => {
                write!(f, "signature threshold not met: needed {needed}, got {got}")
            }
            IdentityError::PairwiseRequiresSingleKey => write!(
                f,
                "pairwise pseudonym derivation requires a single-key (1-of-1) root"
            ),
            IdentityError::RootIsDelegated => write!(
                f,
                "delegator is itself a delegated identity: devices must chain to a true root"
            ),
            IdentityError::RecoveryKeysMismatch => write!(
                f,
                "recovery keys do not match the KEL's pre-rotation commitments"
            ),
            IdentityError::StaleKel { pinned, got } => write!(
                f,
                "stale kel: previously pinned sn {pinned}, this kel only reaches sn {got}"
            ),
            IdentityError::EmptyWitnessSet => write!(f, "empty witness set"),
            IdentityError::InvalidWitnessThreshold {
                threshold,
                witness_count,
            } => write!(
                f,
                "invalid witness threshold {threshold} for witness set of size {witness_count}"
            ),
            IdentityError::DuplicateWitness => write!(f, "witness policy repeats a witness id"),
            IdentityError::UnknownWitnessReceiptVersion(v) => {
                write!(f, "unknown witness receipt version: {v}")
            }
            IdentityError::UnknownWitnessCertificateVersion(v) => {
                write!(f, "unknown witness certificate version: {v}")
            }
            IdentityError::UnknownKeyEventKindTag(t) => {
                write!(f, "unknown key event kind tag: {t}")
            }
            IdentityError::WitnessReceiptMismatch => write!(
                f,
                "witness receipt does not match the certificate's claimed event"
            ),
            IdentityError::WitnessNotInPolicy => {
                write!(f, "witness receipt from a witness outside the policy")
            }
            IdentityError::WitnessPolicyGenerationMismatch { expected, got } => write!(
                f,
                "witness policy generation mismatch: policy is generation {expected}, certificate claims {got}"
            ),
            IdentityError::UnresolvedWitnessKey => {
                write!(f, "could not resolve a verifying key for a witness")
            }
            IdentityError::WitnessThresholdNotMet { needed, got } => write!(
                f,
                "witness threshold not met: needed {needed}, got {got}"
            ),
            IdentityError::WitnessConflictingDescendant { sequence } => write!(
                f,
                "event at sequence {sequence} neither matches, extends, nor precedes this witness's accepted state"
            ),
            IdentityError::ControllerDuplicityMismatch => write!(
                f,
                "the two events do not demonstrate controller duplicity"
            ),
            IdentityError::WitnessEquivocationMismatch => write!(
                f,
                "the two receipts do not demonstrate witness equivocation"
            ),
        }
    }
}

impl std::error::Error for IdentityError {}

impl From<CryptoError> for IdentityError {
    fn from(e: CryptoError) -> Self {
        IdentityError::Crypto(e)
    }
}

/// Convenience result type for this crate.
pub type Result<T> = core::result::Result<T, IdentityError>;
