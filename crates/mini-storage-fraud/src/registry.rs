//! Where replica uniqueness is actually enforced.
//!
//! Conflict evidence is a backstop. The primary defence is that a registry
//! refuses to admit a second claim over a replica root it has already accepted,
//! so the invariant is checked at the moment a provider tries to violate it
//! rather than reconstructed afterwards from published objects.
//!
//! This is a local, in-memory index with no consensus behind it: two registries
//! run by two operators can each accept one half of a conflicting pair without
//! ever learning of each other. That is precisely the residual case
//! [`crate::verify_conflict`] exists for, and precisely why a networked,
//! replicated registration surface is named as required follow-up rather than
//! implied to exist.

use std::collections::BTreeMap;

use crate::claim::{RegisteredReplicaClaim, VerifiedReplicaClaim};
use crate::conflict::{verify_conflict, ReplicaConflictEvidence, VerifiedReplicaConflict};
use crate::error::{FraudError, Result};
use crate::registration::{RegistrationPolicy, StorageRegistrationOracle};

/// What happened when a registry was offered a claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    /// Accepted: this replica root was not registered before.
    Accepted(Box<VerifiedReplicaClaim>),
    /// Refused: the identical claim is already registered. Re-sending a claim
    /// is normal network behaviour, not misconduct.
    AlreadyRegistered,
    /// Refused: a *different* identity root already registered this replica
    /// root. The registry keeps its existing entry and hands back portable
    /// evidence of the impossibility, attributing fault to nobody.
    Conflict(Box<VerifiedReplicaConflict>),
}

/// An index of accepted registrations, keyed by replica root.
#[derive(Debug, Default)]
pub struct ReplicaRegistry {
    by_replica_root: BTreeMap<[u8; 32], RegisteredReplicaClaim>,
}

impl ReplicaRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many replicas this registry has accepted.
    pub fn len(&self) -> usize {
        self.by_replica_root.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_replica_root.is_empty()
    }

    /// The claim already accepted for a replica root, if any.
    pub fn registered(&self, replica_root: &[u8; 32]) -> Option<&RegisteredReplicaClaim> {
        self.by_replica_root.get(replica_root)
    }

    /// Verify a claim and, if it is sound and its replica root is unclaimed,
    /// record it.
    ///
    /// A claim that fails verification is an error, not a conflict: the
    /// registry never stores anything it could not check, so a rejected claim
    /// leaves no trace and cannot be used to occupy a replica root.
    pub fn admit(
        &mut self,
        claim: RegisteredReplicaClaim,
        oracle: &dyn StorageRegistrationOracle,
        policy: &RegistrationPolicy,
    ) -> Result<Admission> {
        let replica_root = claim.seal().replica_root;

        if let Some(existing) = self.by_replica_root.get(&replica_root) {
            if existing.claim_id() == claim.claim_id() {
                return Ok(Admission::AlreadyRegistered);
            }
            if existing.provider_root().scid() == claim.provider_root().scid() {
                // Same root, same replica, different claim bytes: a re-issue,
                // not a cross-identity conflict. Refuse it without minting
                // evidence against anyone.
                return Err(FraudError::AlreadyRegistered);
            }
            let evidence = ReplicaConflictEvidence::new(existing.clone(), claim);
            let conflict = verify_conflict(evidence, oracle, policy)?;
            return Ok(Admission::Conflict(Box::new(conflict)));
        }

        let verified = claim.verify(oracle, policy)?;
        self.by_replica_root
            .insert(replica_root, verified.claim().clone());
        Ok(Admission::Accepted(Box::new(verified)))
    }
}
