//! Two verified replica claims that cannot both be sound, and the deliberately
//! narrow thing that proves.
//!
//! # What a conflict is
//!
//! Every registered claim carries a seal commitment whose `replica_id` is
//! derived from the claiming identity. Sealing is deterministic in that id, and
//! distinct ids produce unrelated labels, so two claims from two different
//! identity roots should never arrive at the same `replica_root` — not because
//! that is forbidden, but because reaching it requires either a BLAKE3/Merkle
//! collision or an audit quorum that signed off on labeling it never checked.
//!
//! So a verified conflict says: **at least one of these two registrations is
//! unsound.** That is a real, portable, independently checkable finding, and it
//! is the entire finding.
//!
//! # What a conflict is not
//!
//! - **Not an accusation against either party.** It does not say which
//!   registration is bad, and it cannot. [`ConflictAttribution`] has exactly
//!   one value today — `Unattributed` — and that is not a placeholder waiting
//!   to be filled in from these two objects. Attribution needs evidence this
//!   type does not contain: fresh individual audits under seeds neither
//!   provider chose, and scrutiny of the two auditor quorums.
//! - **Not proof of collusion, sharing, or Sybil operation.** A conflict is
//!   consistent with two honest providers and one corrupt auditor quorum. It is
//!   equally consistent with one operator running both roots. Distinguishing
//!   those is the personhood problem (roadmap #18/#21), untouched here.
//! - **Not grounds for penalty.** No provider may be penalised on this object
//!   alone. This module assigns none, and no crate in this tree consumes it to
//!   assign any — the same boundary
//!   `mini_consensus::evidence::EquivocationEvidence` draws for double-signing.
//!
//! It is also **not the primary defence**. [`crate::ReplicaRegistry`] refuses a
//! duplicate replica root at admission, which is where the invariant should
//! normally be enforced. Conflict evidence is what remains portable when two
//! independently-operated registries each accepted one of the pair, and neither
//! ever saw the other.
//!
//! # Not equivalent to consensus equivocation
//!
//! Equivocation is two *incompatible statements by one signer* — self-evidently
//! contradictory. This is two *different signers* whose statements are
//! individually well-formed and jointly impossible. That asymmetry is exactly
//! why the culprit is unknown here and known there, and why this module names
//! its output a conflict rather than a proof of fraud.

use did_mini::Did;

use crate::claim::{RegisteredReplicaClaim, VerifiedReplicaClaim};
use crate::codec::{Reader, Writer};
use crate::error::{DecodeFailure, FraudError, Result};
use crate::registration::{RegistrationPolicy, StorageRegistrationOracle};

/// Version tag carried by [`ReplicaConflictEvidence`]'s wire encoding.
pub const REPLICA_CONFLICT_VERSION: u8 = 1;

/// Which impossibility the two claims exhibit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConflictKind {
    /// The two claims commit to the same sealed replica root under different
    /// identity-derived replica ids.
    DuplicateReplicaRoot,
    /// Same replica root, and the seal commitments disagree about the replica's
    /// shape (node count, layer count, or data root) as well.
    ///
    /// Reported separately because it is strictly stranger: identical roots are
    /// at least a self-consistent story about one replica, whereas identical
    /// roots over differently-shaped replicas means at least one commitment is
    /// internally fabricated rather than merely duplicated.
    DuplicateReplicaRootWithDivergentShape,
}

/// Who is at fault.
///
/// One variant, on purpose. See the module docs: this object cannot attribute,
/// and an enum that offered `First`/`Second` would invite callers to guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConflictAttribution {
    /// At least one of the two registrations is unsound; which one is not
    /// determined by this evidence and must not be inferred from it.
    Unattributed,
}

/// Two claims offered as conflicting. Untrusted until [`verify_conflict`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaConflictEvidence {
    first: RegisteredReplicaClaim,
    second: RegisteredReplicaClaim,
}

impl ReplicaConflictEvidence {
    /// Pair two claims in canonical order, so `(a, b)` and `(b, a)` are the
    /// same object and hash to one evidence id.
    pub fn new(first: RegisteredReplicaClaim, second: RegisteredReplicaClaim) -> Self {
        if first.claim_id() <= second.claim_id() {
            Self { first, second }
        } else {
            Self {
                first: second,
                second: first,
            }
        }
    }

    pub fn first(&self) -> &RegisteredReplicaClaim {
        &self.first
    }

    pub fn second(&self) -> &RegisteredReplicaClaim {
        &self.second
    }

    /// A stable identifier for this pairing, independent of the order it was
    /// assembled in.
    pub fn evidence_id(&self) -> [u8; 32] {
        let mut writer = Writer::new();
        writer.raw(b"mininet/mini-storage-fraud/replica-conflict/v1");
        writer.raw(&self.first.claim_id());
        writer.raw(&self.second.claim_id());
        mini_crypto::HashAlgorithm::Blake3.digest(&writer.finish())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.u8(REPLICA_CONFLICT_VERSION);
        self.first.write_into(&mut writer);
        self.second.write_into(&mut writer);
        writer.finish()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        if reader.u8()? != REPLICA_CONFLICT_VERSION {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        let first = RegisteredReplicaClaim::read_from(&mut reader)?;
        let second = RegisteredReplicaClaim::read_from(&mut reader)?;
        reader.finish()?;
        if first.claim_id() > second.claim_id() {
            return Err(DecodeFailure::NoncanonicalConflictOrder.into());
        }
        Ok(Self { first, second })
    }
}

/// A conflict that checked out. Private fields, accessors only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedReplicaConflict {
    kind: ConflictKind,
    attribution: ConflictAttribution,
    replica_root: [u8; 32],
    first: VerifiedReplicaClaim,
    second: VerifiedReplicaClaim,
}

impl VerifiedReplicaConflict {
    pub fn kind(&self) -> ConflictKind {
        self.kind
    }

    /// Always [`ConflictAttribution::Unattributed`]. Kept as a field rather
    /// than left implicit so a consumer reading this object has to see, in its
    /// own type, that nobody has been blamed.
    pub fn attribution(&self) -> ConflictAttribution {
        self.attribution
    }

    /// The replica root both claims commit to.
    pub fn replica_root(&self) -> [u8; 32] {
        self.replica_root
    }

    pub fn first(&self) -> &VerifiedReplicaClaim {
        &self.first
    }

    pub fn second(&self) -> &VerifiedReplicaClaim {
        &self.second
    }

    /// The two provider roots involved, in the evidence's canonical order.
    ///
    /// Named `involved_roots`, not `culprits`: being named here is not a
    /// finding against either root.
    pub fn involved_roots(&self) -> (&Did, &Did) {
        (self.first.provider_root(), self.second.provider_root())
    }

    /// What a consumer should do next, spelled out because the type cannot
    /// enforce it: re-audit both replicas individually, under seeds neither
    /// provider influenced, and examine both auditor quorums. Do not penalise
    /// on this object alone.
    pub fn required_follow_up(&self) -> &'static str {
        "re-audit both replicas independently under fresh verifier-chosen seeds, \
         and review both registration quorums; this evidence attributes fault to neither party"
    }
}

/// Verify that two claims really are jointly impossible.
///
/// Both must verify completely on their own — identity, delegation, capability,
/// signing history, identity-bound replica id, and registration quorum — before
/// the pair is examined at all. A conflict between two objects that are not
/// individually sound is not evidence of anything.
pub fn verify_conflict(
    evidence: ReplicaConflictEvidence,
    oracle: &dyn StorageRegistrationOracle,
    policy: &RegistrationPolicy,
) -> Result<VerifiedReplicaConflict> {
    let ReplicaConflictEvidence { first, second } = evidence;

    if first.provider_root().scid() == second.provider_root().scid() {
        // One root re-registering the same replica is a duplicate registration,
        // not a cross-identity conflict, and is the registry's business.
        return Err(FraudError::NotAConflict);
    }
    if first.seal().replica_root != second.seal().replica_root {
        return Err(FraudError::NotAConflict);
    }
    if first.required_replica_id() == second.required_replica_id() {
        // Unreachable for two distinct roots given the derivation, checked
        // anyway: if it ever fires, the derivation is broken and the pair says
        // nothing about either provider's conduct.
        return Err(FraudError::NotAConflict);
    }

    let replica_root = first.seal().replica_root;
    let shape_diverges = first.seal().node_count != second.seal().node_count
        || first.seal().num_layers != second.seal().num_layers
        || first.seal().data_root != second.seal().data_root;

    let first = first.verify(oracle, policy)?;
    let second = second.verify(oracle, policy)?;

    Ok(VerifiedReplicaConflict {
        kind: if shape_diverges {
            ConflictKind::DuplicateReplicaRootWithDivergentShape
        } else {
            ConflictKind::DuplicateReplicaRoot
        },
        attribution: ConflictAttribution::Unattributed,
        replica_root,
        first,
        second,
    })
}
