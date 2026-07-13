//! Interim KEL-freshness rule (SPEC-01 §7, M3's launch blocker): until real
//! witness receipts and gossip-based duplicity proofs exist, a verifier can
//! at least refuse to go backwards. [`crate::verify_delegation`]'s own docs
//! already recommend it in prose — "pin the highest `sn` [a caller has] ever
//! seen per SCID, refusing to go backwards" — [`FreshnessPins`] is that
//! recommendation made real instead of only described.
//!
//! ## What this closes and what it does not
//!
//! A verifier that pins the highest sequence number it has ever seen for a
//! SCID and rejects any KEL claiming a lower `sn` can no longer be handed a
//! *stale, previously-seen* copy of that KEL and fooled into treating a
//! since-revoked device as still delegated — the classic replay of an old
//! but validly-signed log. It does **not** solve the harder case this
//! crate's own docs already name: a verifier who has *never seen* a fresher
//! KEL at all has nothing to pin against and cannot detect that one exists
//! elsewhere. That gap is exactly what witness receipts and gossip-based
//! duplicity proofs (SPEC-01 §7, still unbuilt) are for — this is the
//! documented interim rule, not a replacement for them.

use std::collections::HashMap;

use crate::error::{IdentityError, Result};
use crate::kel::Kel;

/// Per-SCID highest-sequence-number pin. Construct one per verifier
/// (persist it across sessions for a stronger guarantee — an in-memory-only
/// pin protects nothing across a restart); feed every KEL you intend to
/// trust through [`Self::check_and_pin`] rather than calling
/// [`Kel::verify`] directly.
#[derive(Debug, Clone, Default)]
pub struct FreshnessPins {
    highest_sn_seen: HashMap<String, u64>,
}

impl FreshnessPins {
    /// A fresh tracker with no pins yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// This SCID's currently pinned highest `sn`, if any KEL for it has ever
    /// been checked.
    pub fn pinned_sn(&self, scid: &str) -> Option<u64> {
        self.highest_sn_seen.get(scid).copied()
    }

    /// Verify `kel` normally, then check its resulting `sn` against this
    /// SCID's pin — rejecting it as [`IdentityError::StaleKel`] if it is
    /// lower than one already seen. [`Kel::verify`] alone only proves
    /// internal continuity (the log is authentic and unbroken); it has no
    /// way to know a *fresher* log exists elsewhere, since it never sees
    /// more than the one log it is given. This is that missing check.
    ///
    /// On success the pin advances to the max of what it already held and
    /// this KEL's `sn` — it never regresses, including when called again
    /// with an equal or lower (but not rejected) `sn`.
    pub fn check_and_pin(&mut self, kel: &Kel) -> Result<u64> {
        let state = kel.verify()?;
        let scid = kel.scid();
        if let Some(&pinned) = self.highest_sn_seen.get(scid) {
            if state.sn < pinned {
                return Err(IdentityError::StaleKel {
                    pinned,
                    got: state.sn,
                });
            }
        }
        self.highest_sn_seen
            .entry(scid.to_string())
            .and_modify(|p| *p = (*p).max(state.sn))
            .or_insert(state.sn);
        Ok(state.sn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Controller;

    #[test]
    fn a_fresh_scid_is_pinned_on_first_check() {
        let alice = Controller::incept_single().unwrap();
        let mut pins = FreshnessPins::new();
        assert_eq!(pins.pinned_sn(alice.scid()), None);
        let sn = pins.check_and_pin(&alice.kel()).unwrap();
        assert_eq!(sn, 0);
        assert_eq!(pins.pinned_sn(alice.scid()), Some(0));
    }

    #[test]
    fn a_kel_that_advances_the_pin_is_accepted_and_raises_it() {
        let mut alice = Controller::incept_single().unwrap();
        let mut pins = FreshnessPins::new();
        pins.check_and_pin(&alice.kel()).unwrap();

        alice.rotate().unwrap();
        let sn = pins.check_and_pin(&alice.kel()).unwrap();
        assert_eq!(sn, 1);
        assert_eq!(pins.pinned_sn(alice.scid()), Some(1));
    }

    #[test]
    fn a_stale_kel_below_the_pin_is_rejected() {
        let mut alice = Controller::incept_single().unwrap();
        let mut pins = FreshnessPins::new();

        // Snapshot the KEL at sn 0, then rotate so the real state moves to
        // sn 1 and the pin advances.
        let stale_kel = alice.kel();
        alice.rotate().unwrap();
        pins.check_and_pin(&alice.kel()).unwrap();
        assert_eq!(pins.pinned_sn(alice.scid()), Some(1));

        // Handing the verifier the old, pre-rotation snapshot must be
        // rejected -- this is exactly the "revoked device still looks
        // delegated" replay `verify_delegation`'s docs warn about.
        let err = pins.check_and_pin(&stale_kel).unwrap_err();
        assert_eq!(err, IdentityError::StaleKel { pinned: 1, got: 0 });
        // Rejection must not corrupt the pin.
        assert_eq!(pins.pinned_sn(alice.scid()), Some(1));
    }

    #[test]
    fn re_checking_the_same_kel_twice_is_accepted_and_does_not_regress() {
        let alice = Controller::incept_single().unwrap();
        let mut pins = FreshnessPins::new();
        pins.check_and_pin(&alice.kel()).unwrap();
        // Same sn again (e.g. a peer resending the same KEL) is not a
        // regression -- only strictly-lower sn is rejected.
        let sn = pins.check_and_pin(&alice.kel()).unwrap();
        assert_eq!(sn, 0);
        assert_eq!(pins.pinned_sn(alice.scid()), Some(0));
    }

    #[test]
    fn distinct_scids_are_pinned_independently() {
        let alice = Controller::incept_single().unwrap();
        let bob = Controller::incept_single().unwrap();
        let mut pins = FreshnessPins::new();
        pins.check_and_pin(&alice.kel()).unwrap();
        assert_eq!(pins.pinned_sn(bob.scid()), None);
        pins.check_and_pin(&bob.kel()).unwrap();
        assert_eq!(pins.pinned_sn(alice.scid()), Some(0));
        assert_eq!(pins.pinned_sn(bob.scid()), Some(0));
    }

    #[test]
    fn an_internally_invalid_kel_is_rejected_before_any_freshness_check() {
        // A structurally-broken KEL must fail via the normal `verify()`
        // path, never silently pinned as if it were legitimate.
        let alice = Controller::incept_single().unwrap();
        let mut bytes = alice.kel().to_bytes();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        let tampered = Kel::from_bytes(&bytes);
        let mut pins = FreshnessPins::new();
        match tampered {
            Ok(kel) => {
                assert!(pins.check_and_pin(&kel).is_err());
            }
            Err(_) => {
                // Also acceptable: corruption caught at decode time.
            }
        }
        assert_eq!(pins.pinned_sn(alice.scid()), None);
    }
}
