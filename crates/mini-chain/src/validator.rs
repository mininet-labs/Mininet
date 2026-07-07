//! Validator sets: equal voting power per verified identity root, never
//! stake \[FREEZE: P1/P2\]. There is deliberately no weight field anywhere in
//! this module — a validator either is in the set (one equal vote) or is
//! not. Personhood (SPEC-02) later upgrades "identity root" to "human," the
//! same honesty boundary every quorum-counting module in this tree states.

use std::collections::HashSet;

use did_mini::Did;

use crate::error::{ChainError, Result};

/// A validator set: distinct identity roots, each with exactly one equal
/// vote. Construction rejects an empty or duplicate-containing set so no
/// caller can silently build an unsafe or double-counted set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatorSet {
    roots: Vec<Did>,
}

impl ValidatorSet {
    /// Build a validator set. Order does not matter; the set is canonically
    /// sorted so two callers building "the same" set always compare equal.
    pub fn new(mut roots: Vec<Did>) -> Result<Self> {
        if roots.is_empty() {
            return Err(ChainError::EmptyValidatorSet);
        }
        roots.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let mut seen: HashSet<&str> = HashSet::with_capacity(roots.len());
        for r in &roots {
            if !seen.insert(r.scid()) {
                return Err(ChainError::DuplicateValidator);
            }
        }
        Ok(ValidatorSet { roots })
    }

    /// Number of validators.
    pub fn len(&self) -> usize {
        self.roots.len()
    }

    /// Whether the set is empty (construction forbids this, kept for API
    /// completeness alongside [`Self::len`]).
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Whether `root` is a current validator.
    pub fn contains(&self, root: &Did) -> bool {
        self.roots.iter().any(|r| r.scid() == root.scid())
    }

    /// The validator roots, canonically sorted.
    pub fn roots(&self) -> &[Did] {
        &self.roots
    }

    /// The BFT safety threshold: strictly more than 2/3 of the set
    /// (`floor(2n/3) + 1`), the standard quorum size a Tendermint-style
    /// design requires before treating a Precommit round as final.
    pub fn quorum_threshold(&self) -> usize {
        (2 * self.roots.len()) / 3 + 1
    }
}
