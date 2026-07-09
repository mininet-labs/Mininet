//! Error type for `mini-treasury`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreasuryError {
    /// A signer set was empty, oversized, or contained a duplicate identity
    /// root.
    InvalidSignerSet,
    /// A threshold was zero or exceeded the signer set's size.
    InvalidThreshold,
    /// No governed rate is in effect at the requested time.
    NoRateInEffect,
    /// A new rate entry's effective time was not strictly after the
    /// previous entry's.
    OutOfOrderRateEntry,
    /// The OS CSPRNG failed to yield randomness.
    Entropy,
    /// A FROST threshold or participant count was zero, exceeded
    /// [`crate::MAX_FROST_PARTICIPANTS`], or set a threshold above the
    /// participant count.
    InvalidFrostParameters,
    /// Fewer signers took part in a FROST signing round than the
    /// threshold requires.
    NotEnoughSigners,
    /// A FROST participant index was reused, zero (index 0 is reserved —
    /// it is the secret-reconstruction point, never a signer), or absent
    /// from the signing package it was looked up in.
    InvalidFrostParticipant,
    /// A FROST participant's key share failed its Feldman VSS
    /// verification against the dealer's published commitments.
    InvalidFrostShare,
    /// A FROST signature share failed verification against its signer's
    /// public verification share before aggregation.
    InvalidFrostSignatureShare,
    /// A compressed Ristretto point or scalar did not decode.
    MalformedFrostEncoding,
    /// A DKG round-1 package's Schnorr proof of knowledge of its
    /// constant-term commitment did not verify — either a rogue-key
    /// attempt, a corrupted package, or a package replayed under a
    /// different session context than it was created for.
    DkgProofOfKnowledgeFailed,
    /// A resharing round-1 package's constant-term commitment did not
    /// equal `lambda_i * Y_i` for the claimed old participant — either a
    /// forged or substituted contribution, not a genuine Lagrange-weighted
    /// share of the old group secret.
    ReshareInvalidContribution,
    /// Resharing finalized to a group public key different from the old
    /// committee's — the resulting key package must never be trusted;
    /// this should be structurally impossible given verified inputs, and
    /// is checked directly rather than only relied upon algebraically.
    ReshareGroupKeyMismatch,
}

impl fmt::Display for TreasuryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreasuryError::InvalidSignerSet => write!(f, "invalid treasury signer set"),
            TreasuryError::InvalidThreshold => write!(f, "invalid signer threshold"),
            TreasuryError::NoRateInEffect => write!(f, "no governed rate in effect at this time"),
            TreasuryError::OutOfOrderRateEntry => {
                write!(f, "rate entry is not strictly after the previous one")
            }
            TreasuryError::Entropy => write!(f, "OS CSPRNG failed to yield randomness"),
            TreasuryError::InvalidFrostParameters => {
                write!(f, "invalid FROST threshold/participant count")
            }
            TreasuryError::NotEnoughSigners => {
                write!(f, "fewer signers than the FROST threshold requires")
            }
            TreasuryError::InvalidFrostParticipant => {
                write!(f, "invalid or unknown FROST participant index")
            }
            TreasuryError::InvalidFrostShare => {
                write!(f, "FROST key share failed Feldman VSS verification")
            }
            TreasuryError::InvalidFrostSignatureShare => {
                write!(f, "FROST signature share failed verification")
            }
            TreasuryError::MalformedFrostEncoding => {
                write!(f, "malformed FROST point/scalar encoding")
            }
            TreasuryError::DkgProofOfKnowledgeFailed => {
                write!(f, "DKG round-1 proof of knowledge did not verify")
            }
            TreasuryError::ReshareInvalidContribution => {
                write!(
                    f,
                    "resharing contribution did not match the claimed old participant's weighted share"
                )
            }
            TreasuryError::ReshareGroupKeyMismatch => {
                write!(
                    f,
                    "resharing produced a different group public key than the old committee held"
                )
            }
        }
    }
}

impl std::error::Error for TreasuryError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, TreasuryError>;
