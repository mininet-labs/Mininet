//! Role-only consequence for detected equivocation (founder review's
//! `consensus-evidence` P0 finding, 2026-07-12): until now, a
//! [`crate::Emit::Equivocation`] was surfaced by [`crate::ConsensusNode`]
//! but silently dropped by [`crate::net::run_to_height`]'s network driver —
//! real, independently-verifiable proof of double-signing that reached the
//! wire had nowhere to go. This module is that "somewhere": an equivocator
//! registry a real slashing/validator-set-transition layer can consult.
//!
//! ## What this is and is not
//!
//! This is a **role-only** consequence: it never touches personhood or
//! [`did_mini`]'s identity layer, and it never removes a root from an
//! already-running [`mini_chain::ValidatorSet`] — that set is still static
//! for the life of a run (dynamic validator-set transitions are separate,
//! larger, later roadmap work, issues #36-#45). Recording a root here does
//! not change today's consensus behavior at all: the round driver already
//! counts an equivocator's vote at most once, so safety never depended on
//! this. What changes is that the evidence is no longer thrown away — it is
//! kept, deduplicated, and queryable, which is the real prerequisite any
//! future exclusion-from-the-next-epoch or governance-visible-strike
//! mechanism needs before it can act on anything.

use std::collections::HashSet;

use did_mini::Did;
use mini_chain::ValidatorOracle;

use crate::evidence::{verify_equivocation, EquivocationEvidence};

/// What happened when a piece of equivocation evidence was offered to an
/// [`EquivocatorRegistry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordOutcome {
    /// The evidence verified and named a root not previously flagged.
    NewlyFlagged(Did),
    /// The evidence verified, but that root was already flagged — recorded
    /// evidence is deduplicated per root, not accumulated per accusation.
    AlreadyFlagged(Did),
    /// The evidence did not independently verify. This registry never
    /// trusts a caller's claim that evidence is genuine; it always re-checks
    /// via [`verify_equivocation`] for itself before recording anything.
    InvalidEvidence,
}

/// Validator roots with independently-verified equivocation evidence
/// recorded against them.
#[derive(Debug, Clone, Default)]
pub struct EquivocatorRegistry {
    flagged: HashSet<String>,
}

impl EquivocatorRegistry {
    /// A fresh, empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Independently re-verify `evidence` and record its root if genuine.
    pub fn record(
        &mut self,
        evidence: &EquivocationEvidence,
        oracle: &dyn ValidatorOracle,
    ) -> RecordOutcome {
        if !verify_equivocation(evidence, oracle) {
            return RecordOutcome::InvalidEvidence;
        }
        let root = evidence.first.validator_root.clone();
        if self.flagged.insert(root.scid().to_string()) {
            RecordOutcome::NewlyFlagged(root)
        } else {
            RecordOutcome::AlreadyFlagged(root)
        }
    }

    /// Whether `root` has verified equivocation evidence recorded against it.
    pub fn is_flagged(&self, root: &Did) -> bool {
        self.flagged.contains(root.scid())
    }

    /// How many distinct roots are currently flagged.
    pub fn flagged_count(&self) -> usize {
        self.flagged.len()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use did_mini::{Capabilities, Controller, Kel};
    use mini_chain::{sign_vote, VoteKind};

    use super::*;

    fn validator(seed: u8) -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    #[derive(Default)]
    struct Directory(BTreeMap<String, Kel>);
    impl Directory {
        fn insert(&mut self, kel: Kel) {
            self.0.insert(kel.scid().to_string(), kel);
        }
    }
    impl ValidatorOracle for Directory {
        fn kel(&self, did: &Did) -> Option<&Kel> {
            self.0.get(did.scid())
        }
    }

    fn dir(controllers: &[&Controller]) -> Directory {
        let mut d = Directory::default();
        for c in controllers {
            d.insert(c.kel());
        }
        d
    }

    #[test]
    fn genuine_evidence_newly_flags_its_root() {
        let (root, device) = validator(10);
        let oracle = dir(&[&root, &device]);
        let a = sign_vote(VoteKind::Prevote, 1, 0, [0xAA; 32], &root.did(), &device);
        let b = sign_vote(VoteKind::Prevote, 1, 0, [0xBB; 32], &root.did(), &device);
        let evidence = EquivocationEvidence {
            first: a,
            second: b,
        };

        let mut registry = EquivocatorRegistry::new();
        assert!(!registry.is_flagged(&root.did()));
        let outcome = registry.record(&evidence, &oracle);
        assert_eq!(outcome, RecordOutcome::NewlyFlagged(root.did()));
        assert!(registry.is_flagged(&root.did()));
        assert_eq!(registry.flagged_count(), 1);
    }

    #[test]
    fn the_same_root_is_not_double_counted_across_separate_accusations() {
        let (root, device) = validator(20);
        let oracle = dir(&[&root, &device]);
        let a = sign_vote(VoteKind::Prevote, 1, 0, [0xAA; 32], &root.did(), &device);
        let b = sign_vote(VoteKind::Prevote, 1, 0, [0xBB; 32], &root.did(), &device);
        // A second, independent conflicting pair at a different height --
        // still the same root, so it must not inflate the flagged count.
        let c = sign_vote(VoteKind::Prevote, 2, 0, [0x11; 32], &root.did(), &device);
        let d = sign_vote(VoteKind::Prevote, 2, 0, [0x22; 32], &root.did(), &device);

        let mut registry = EquivocatorRegistry::new();
        let first = registry.record(
            &EquivocationEvidence {
                first: a,
                second: b,
            },
            &oracle,
        );
        let second = registry.record(
            &EquivocationEvidence {
                first: c,
                second: d,
            },
            &oracle,
        );
        assert_eq!(first, RecordOutcome::NewlyFlagged(root.did()));
        assert_eq!(second, RecordOutcome::AlreadyFlagged(root.did()));
        assert_eq!(registry.flagged_count(), 1);
    }

    #[test]
    fn invalid_evidence_flags_nothing() {
        // Same vote twice is not a conflict -- `verify_equivocation` itself
        // rejects it, and the registry must never flag a root off the back
        // of a claim that doesn't independently verify.
        let (root, device) = validator(30);
        let oracle = dir(&[&root, &device]);
        let a = sign_vote(VoteKind::Prevote, 1, 0, [0xAA; 32], &root.did(), &device);
        let b = a.clone();

        let mut registry = EquivocatorRegistry::new();
        let outcome = registry.record(
            &EquivocationEvidence {
                first: a,
                second: b,
            },
            &oracle,
        );
        assert_eq!(outcome, RecordOutcome::InvalidEvidence);
        assert_eq!(registry.flagged_count(), 0);
        assert!(!registry.is_flagged(&root.did()));
    }

    #[test]
    fn distinct_equivocating_roots_are_flagged_independently() {
        let (r1, d1) = validator(40);
        let (r2, d2) = validator(50);
        let oracle = dir(&[&r1, &d1, &r2, &d2]);
        let mut registry = EquivocatorRegistry::new();

        registry.record(
            &EquivocationEvidence {
                first: sign_vote(VoteKind::Prevote, 1, 0, [0xAA; 32], &r1.did(), &d1),
                second: sign_vote(VoteKind::Prevote, 1, 0, [0xBB; 32], &r1.did(), &d1),
            },
            &oracle,
        );
        registry.record(
            &EquivocationEvidence {
                first: sign_vote(VoteKind::Precommit, 5, 2, [0x11; 32], &r2.did(), &d2),
                second: sign_vote(VoteKind::Precommit, 5, 2, [0x22; 32], &r2.did(), &d2),
            },
            &oracle,
        );

        assert!(registry.is_flagged(&r1.did()));
        assert!(registry.is_flagged(&r2.did()));
        assert_eq!(registry.flagged_count(), 2);
    }
}
