//! Equivocation evidence: cryptographic proof that one validator root
//! double-signed at a single `(height, round, kind)`.
//!
//! Now that every vote is signed ([`mini_chain::sign_vote`]) and every
//! proposal too (D-0202), a Byzantine validator that casts two *different*
//! votes for the same height/round/phase cannot hide it — the two signatures
//! are self-authenticating and, taken together, are a portable, independently-
//! verifiable accusation. This module is the *detection and proof* half only.
//!
//! ## What this does and does not do
//!
//! Equivocation was never a **safety** threat here: one identity root is
//! counted at most once at every layer (P2), so a double-signer can split its
//! own vote across two blocks but can never push either past quorum — the
//! [`crate::round::Round`] already enforces that, and does so more tightly now
//! (it counts a root's *first* vote per phase and ignores the conflicting
//! second). What was missing is **accountability**: surfacing the misbehavior
//! as evidence a future slashing/governance layer can act on. That layer does
//! not exist yet — this crate produces and verifies the proof, and stops
//! there. It assigns no penalty, mutates no validator set, and gates no
//! finality on it.

use mini_chain::{verify_vote, ValidatorOracle, Vote, VoteKind};

/// Two conflicting signed votes from the *same* identity root: same height,
/// same round, same phase ([`VoteKind`]), but different block hashes. Held
/// together they prove the root equivocated. Construct these only from votes
/// you actually received; [`verify_equivocation`] is what decides whether they
/// really constitute proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivocationEvidence {
    /// The first conflicting vote (the one a node had already counted).
    pub first: Vote,
    /// The second conflicting vote (a different block, same slot).
    pub second: Vote,
}

impl EquivocationEvidence {
    /// The height the two votes conflict at.
    pub fn height(&self) -> u64 {
        self.first.height
    }

    /// The round the two votes conflict at.
    pub fn round(&self) -> u32 {
        self.first.round
    }

    /// The phase (`Prevote`/`Precommit`) the two votes conflict in.
    pub fn kind(&self) -> VoteKind {
        self.first.kind
    }
}

/// Verify that `evidence` really is equivocation: the two votes name the same
/// root, the same `(height, round, kind)`, *different* block hashes, and
/// **both** cryptographically verify as that root's votes (each possibly via a
/// different `VOTE`-capable device of the root). Returns `false` for anything
/// that is not genuine, independently-checkable proof — so a fabricated or
/// mismatched "accusation" can never be passed off as real.
pub fn verify_equivocation(evidence: &EquivocationEvidence, oracle: &dyn ValidatorOracle) -> bool {
    let (a, b) = (&evidence.first, &evidence.second);

    // Same root, same slot, but genuinely different values — otherwise it is
    // not a conflict at all (identical votes are just a duplicate, and votes
    // from different roots or slots are simply unrelated).
    if a.validator_root.scid() != b.validator_root.scid() {
        return false;
    }
    if a.height != b.height || a.round != b.round || a.kind != b.kind {
        return false;
    }
    if a.block_hash == b.block_hash {
        return false;
    }

    // Both must verify as real votes of that root (devices may differ — a root
    // with two VOTE-capable devices can still equivocate through them).
    verifies(a, oracle) && verifies(b, oracle)
}

fn verifies(vote: &Vote, oracle: &dyn ValidatorOracle) -> bool {
    matches!(
        (oracle.kel(&vote.validator_root), oracle.kel(&vote.validator_device)),
        (Some(root_kel), Some(device_kel)) if verify_vote(vote, root_kel, device_kel).is_ok()
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use did_mini::{Capabilities, Controller, Did, Kel};
    use mini_chain::sign_vote;

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

    /// A KEL directory over the given controllers (roots and/or devices).
    fn dir(controllers: &[&Controller]) -> Directory {
        let mut d = Directory::default();
        for c in controllers {
            d.insert(c.kel());
        }
        d
    }

    #[test]
    fn two_conflicting_prevotes_from_one_root_are_genuine_evidence() {
        let (root, device) = validator(10);
        let oracle = dir(&[&root, &device]);
        let a = sign_vote(VoteKind::Prevote, 1, 0, [0xAA; 32], &root.did(), &device);
        let b = sign_vote(VoteKind::Prevote, 1, 0, [0xBB; 32], &root.did(), &device);
        let ev = EquivocationEvidence {
            first: a,
            second: b,
        };
        assert!(verify_equivocation(&ev, &oracle));
        assert_eq!(ev.height(), 1);
        assert_eq!(ev.round(), 0);
        assert_eq!(ev.kind(), VoteKind::Prevote);
    }

    #[test]
    fn identical_votes_are_not_equivocation() {
        let (root, device) = validator(20);
        let oracle = dir(&[&root, &device]);
        let a = sign_vote(VoteKind::Precommit, 3, 1, [0x11; 32], &root.did(), &device);
        let b = a.clone();
        assert!(!verify_equivocation(
            &EquivocationEvidence {
                first: a,
                second: b
            },
            &oracle
        ));
    }

    #[test]
    fn votes_at_different_slots_are_not_equivocation() {
        let (root, device) = validator(30);
        let oracle = dir(&[&root, &device]);
        // Same root, different rounds — unrelated, not a conflict.
        let a = sign_vote(VoteKind::Prevote, 1, 0, [0xAA; 32], &root.did(), &device);
        let b = sign_vote(VoteKind::Prevote, 1, 1, [0xBB; 32], &root.did(), &device);
        assert!(!verify_equivocation(
            &EquivocationEvidence {
                first: a,
                second: b
            },
            &oracle
        ));
        // Different phases at the same round — also not a conflict.
        let c = sign_vote(VoteKind::Prevote, 1, 0, [0xAA; 32], &root.did(), &device);
        let d = sign_vote(VoteKind::Precommit, 1, 0, [0xBB; 32], &root.did(), &device);
        assert!(!verify_equivocation(
            &EquivocationEvidence {
                first: c,
                second: d
            },
            &oracle
        ));
    }

    #[test]
    fn votes_from_two_different_roots_are_not_equivocation() {
        let (r1, d1) = validator(40);
        let (r2, d2) = validator(50);
        let oracle = dir(&[&r1, &d1, &r2, &d2]);
        let a = sign_vote(VoteKind::Prevote, 1, 0, [0xAA; 32], &r1.did(), &d1);
        let b = sign_vote(VoteKind::Prevote, 1, 0, [0xBB; 32], &r2.did(), &d2);
        assert!(!verify_equivocation(
            &EquivocationEvidence {
                first: a,
                second: b
            },
            &oracle
        ));
    }

    #[test]
    fn a_forged_second_vote_is_not_accepted_as_evidence() {
        // The "second" vote claims root r1 but is signed by r2's device: it
        // does not verify, so the pair is not usable evidence (a fabricated
        // accusation cannot be laundered into proof).
        let (r1, d1) = validator(60);
        let (_r2, d2) = validator(70);
        let mut oracle = Directory::default();
        oracle.insert(r1.kel());
        oracle.insert(d1.kel());
        oracle.insert(d2.kel());

        let a = sign_vote(VoteKind::Prevote, 1, 0, [0xAA; 32], &r1.did(), &d1);
        // Signed by d2 (not delegated by r1) but claiming r1 as root.
        let forged = sign_vote(VoteKind::Prevote, 1, 0, [0xBB; 32], &r1.did(), &d2);
        assert!(!verify_equivocation(
            &EquivocationEvidence {
                first: a,
                second: forged
            },
            &oracle
        ));
    }
}
