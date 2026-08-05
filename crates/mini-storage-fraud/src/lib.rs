//! Cross-identity storage-fraud detection for roadmap
//! [issue #42](https://github.com/mininet-labs/Mininet/issues/42), Phase 5.7.
//!
//! **Maturity: experimental. Not integrated, not audited, and not sufficient
//! for fraud attribution.** Nothing in this crate may drive a payment, a
//! penalty, an exclusion, or a consensus outcome. It inherits `mini-porep`'s
//! own D-0047 external-audit gate and adds nothing that would soften it.
//!
//! # The question this answers
//!
//! `mini-porep` proves that *one* provider did real sequential sealing work,
//! and `mini-spacetime` proves that provider still holds what it sealed. Issue
//! #42 asks a question neither can: are two providers who each claim their own
//! independent copy actually one warehouse serving many identities?
//!
//! # The shape of the answer
//!
//! 1. [`derive_replica_id`] fixes the replica id a provider must seal under —
//!    a function of its identity root, its storage device, and a typed
//!    [`ReplicaContextV1`]. A provider has no freedom here, and a verifier can
//!    recompute the required value from public information.
//! 2. [`audit_and_attest`] is an independent auditor actually running
//!    `mini_porep`'s registration audit against a full [`mini_porep::SealCommitment`]
//!    — sampling challenges under a seed the auditor chose, recomputing the
//!    labeling itself, refusing to sign unless every answer verifies.
//! 3. [`RegisteredReplicaClaim`] is a provider's signed claim carrying that
//!    full seal commitment and a [`RegistrationReceipt`] quorum of those
//!    attestations. Its `verify` binds all of it together, and derives the
//!    `mini_spacetime::StorageCommitment` from the audited seal instead of
//!    accepting one alongside it.
//! 4. [`ReplicaRegistry`] refuses a second claim over an already-registered
//!    replica root — the primary enforcement point.
//! 5. [`verify_conflict`] is the backstop for claims that were admitted by two
//!    registries that never met, and it reports an **unattributed** conflict:
//!    at least one registration is unsound, and this evidence does not say
//!    which.
//!
//! # What this deliberately does not do
//!
//! - **No timing or latency detection.** Issue #42's other scenario —
//!   answering challenges by fetching from a fast peer rather than holding
//!   data — needs a live deployment to calibrate any honest baseline and is an
//!   open systems-security research problem. An unreviewed heuristic wearing
//!   the word "fraud" would be worse than nothing.
//! - **No consequences.** No penalty, exclusion, reward clawback, or consensus
//!   authority, matching `mini_consensus::evidence`'s scope exactly.
//! - **No Sybil resistance.** Counting distinct identity roots is not counting
//!   distinct humans (roadmap #18). An audit quorum can be one operator.
//! - **No claim to strong soundness.** Under the collision and preimage
//!   resistance of BLAKE3, the Merkle construction, and the soundness of
//!   `mini-porep`'s simplified SDR sealing, independently sealed replicas under
//!   distinct replica ids should differ except with negligible probability.
//!   That is an assumption, resting on prototype code no external
//!   cryptographer has reviewed — not a theorem, and not something a passing
//!   unit test demonstrates.
//!
//! # Position in the dependency graph
//!
//! Depends on `did-mini`, `mini-crypto`, `mini-porep`, and `mini-spacetime`
//! only. No `mini-value`/`mini-bounty`/`mini-treasury` edge in either direction
//! and no governance-crate edge, so no voice/value wall edge exists (P1,
//! Directive 16). Every authority-bearing entry point takes a specific typed
//! request, never raw bytes.
//!
//! Full doctrine, including the normative byte-level encodings and the open
//! protocol questions, is in `docs/design/storage-fraud-detection.md`.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod claim;
mod codec;
mod conflict;
mod context;
mod error;
mod registration;
mod registry;
mod seal;

pub use claim::{
    RegisteredReplicaClaim, VerifiedReplicaClaim, REPLICA_CLAIM_DOMAIN, REPLICA_CLAIM_VERSION,
};
pub use conflict::{
    verify_conflict, ConflictAttribution, ConflictKind, ReplicaConflictEvidence,
    VerifiedReplicaConflict, REPLICA_CONFLICT_VERSION,
};
pub use context::{
    derive_replica_id, seal_params_for, ReplicaContextV1, REPLICA_CONTEXT_VERSION,
    REPLICA_ID_DOMAIN,
};
pub use error::{DecodeFailure, FraudError, Result, SealDefect};
pub use registration::{
    audit_and_attest, AuditAttestation, RegistrationPolicy, RegistrationReceipt,
    StorageRegistrationOracle, AUDIT_ATTESTATION_DOMAIN, AUDIT_ATTESTATION_VERSION,
    MAX_ATTESTATIONS, MAX_AUDIT_CHALLENGES, REGISTRATION_RECEIPT_VERSION,
};
pub use registry::{Admission, ReplicaRegistry};
pub use seal::{
    seal_commitment_digest, storage_commitment_of, validate_seal_commitment,
    SEAL_COMMITMENT_DOMAIN, SEAL_COMMITMENT_VERSION,
};
