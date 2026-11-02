//! Provisional ballots: a newcomer's vote is never thrown away just because
//! their standing is not yet full, and a farm of fake identities cannot stack
//! ballots.
//!
//! Every identity root casts at most one ballot, with equal weight (P2: no
//! balance, stake, score or standing maps to extra weight). A ballot counts
//! toward the result only if its root has **full** standing when voting
//! closes; ballots from roots that are provisional or unvouched at close are
//! reported as *not counted* (so the result is auditable and a newcomer knows
//! to gain standing before the next close) rather than silently dropped or
//! half-weighted.
//!
//! Counts identity roots, not proven humans (Sybil resistance is unsolved).

use crate::policy::Standing;
use did_mini::Did;
use std::collections::HashSet;

/// Result of closing a vote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tally {
    /// Counted ballots in favour.
    pub yes: u32,
    /// Counted ballots against.
    pub no: u32,
    /// Ballots from roots that were not at full standing when voting closed.
    pub not_counted: u32,
    /// Repeat ballots from a root that had already voted (the first stands).
    pub duplicates: u32,
}

/// Tally `ballots` (root, in favour?) given each root's standing at close.
pub fn tally<'a>(
    ballots: impl IntoIterator<Item = (&'a Did, bool)>,
    standing_at_close: impl Fn(&Did) -> Standing,
) -> Tally {
    let mut seen: HashSet<&Did> = HashSet::new();
    let mut t = Tally::default();
    for (root, in_favour) in ballots {
        if !seen.insert(root) {
            t.duplicates += 1;
            continue;
        }
        if standing_at_close(root) == Standing::Full {
            if in_favour {
                t.yes += 1;
            } else {
                t.no += 1;
            }
        } else {
            t.not_counted += 1;
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn did(n: u8) -> Did {
        Controller::incept_single_from_seeds(&[n; 32], &[n.wrapping_add(100); 32])
            .unwrap()
            .did()
    }

    #[test]
    fn only_full_standing_at_close_counts_each_root_once_with_equal_weight() {
        let (a, b, c, d) = (did(1), did(2), did(3), did(4));
        let standing = |r: &Did| {
            if *r == a || *r == b {
                Standing::Full
            } else if *r == c {
                Standing::Provisional
            } else {
                Standing::Unvouched
            }
        };
        let ballots = [(&a, true), (&b, false), (&c, true), (&d, true), (&a, false)];
        let t = tally(ballots, standing);
        assert_eq!(
            t,
            Tally {
                yes: 1,
                no: 1,
                not_counted: 2,
                duplicates: 1
            }
        );
    }

    #[test]
    fn a_newcomer_who_reaches_full_before_close_is_counted() {
        let n = did(9);
        let early = tally([(&n, true)], |_| Standing::Provisional);
        let at_close = tally([(&n, true)], |_| Standing::Full);
        assert_eq!((early.yes, early.not_counted), (0, 1));
        assert_eq!((at_close.yes, at_close.not_counted), (1, 0));
    }
}
