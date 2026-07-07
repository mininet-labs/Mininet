//! The on-device controller — the secret-holding half of an identity.
//!
//! A [`Controller`] generates and holds the signing keys, builds and appends key
//! events, and produces the public [`Kel`] for exchange. Per SPEC-01 G1, secret
//! material lives only here and only leaves through `mini-crypto`'s explicit,
//! loudly-named on-device export — never through any wire format produced here.
//!
//! Pre-rotation in practice: the controller always holds the *next* secret keys,
//! committed (as hashes) in the latest event but unrevealed. [`Controller::rotate`]
//! reveals them as the new current keys and commits to a freshly generated next
//! set, so a stolen current key cannot rotate (SPEC-01 §5).
//!
//! Devices (SPEC-01 §6): a device is its own `Controller` created with
//! [`Controller::incept_device`], whose identifier commits to the human-root that
//! delegates it. The human-root authorizes it with [`Controller::delegate_device`]
//! and can [`Controller::revoke_device`] later.

use mini_crypto::{SignatureSuite, SigningKey};

use crate::delegation::{Capabilities, Seal};
use crate::error::{IdentityError, Result};
use crate::event::{self, Establishment, Event, EventKind, IndexedSig};
use crate::kel::{Kel, KeyState};
use crate::limits::{MAX_ANCHORS, MAX_SEALS};
use crate::Did;

/// Holds an identity's secret keys and its event history.
pub struct Controller {
    scid: String,
    suite: SignatureSuite,
    current: Vec<SigningKey>,
    current_threshold: u32,
    /// Pre-generated next keys; secret until revealed at the next rotation.
    next: Vec<SigningKey>,
    next_threshold: u32,
    /// Present iff this is a delegated (device) identity (SPEC-01 §6).
    delegator: Option<Did>,
    events: Vec<Event>,
}

impl core::fmt::Debug for Controller {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never print secret key material.
        f.debug_struct("Controller")
            .field("did", &self.did().as_str())
            .field("sn", &(self.events.len() as u64 - 1))
            .field("delegator", &self.delegator.as_ref().map(Did::as_str))
            .field("current_threshold", &self.current_threshold)
            .field("next_threshold", &self.next_threshold)
            .finish()
    }
}

impl Controller {
    /// Incept a single-key (1-of-1) identity from explicit seeds — deterministic,
    /// for tests and reproducible flows.
    pub fn incept_single_from_seeds(current_seed: &[u8; 32], next_seed: &[u8; 32]) -> Result<Self> {
        Self::incept(
            vec![SigningKey::from_seed(current_seed)],
            1,
            vec![SigningKey::from_seed(next_seed)],
            1,
        )
    }

    /// Incept a single-key identity using operating-system entropy.
    pub fn incept_single() -> Result<Self> {
        Self::incept(
            vec![SigningKey::generate()?],
            1,
            vec![SigningKey::generate()?],
            1,
        )
    }

    /// Incept with an explicit current/next key set and thresholds.
    pub fn incept(
        current: Vec<SigningKey>,
        current_threshold: u32,
        next: Vec<SigningKey>,
        next_threshold: u32,
    ) -> Result<Self> {
        Self::incept_inner(current, current_threshold, next, next_threshold, None)
    }

    /// Incept a delegated (device) single-key identity from explicit seeds, under
    /// `delegator` (the human-root). Deterministic, for tests.
    pub fn incept_device_single_from_seeds(
        delegator: &Did,
        current_seed: &[u8; 32],
        next_seed: &[u8; 32],
    ) -> Result<Self> {
        Self::incept_device(
            delegator,
            vec![SigningKey::from_seed(current_seed)],
            1,
            vec![SigningKey::from_seed(next_seed)],
            1,
        )
    }

    /// Incept a delegated (device) identity under `delegator`.
    pub fn incept_device(
        delegator: &Did,
        current: Vec<SigningKey>,
        current_threshold: u32,
        next: Vec<SigningKey>,
        next_threshold: u32,
    ) -> Result<Self> {
        Self::incept_inner(
            current,
            current_threshold,
            next,
            next_threshold,
            Some(delegator.clone()),
        )
    }

    fn incept_inner(
        current: Vec<SigningKey>,
        current_threshold: u32,
        next: Vec<SigningKey>,
        next_threshold: u32,
        delegator: Option<Did>,
    ) -> Result<Self> {
        if current.is_empty() || next.is_empty() {
            return Err(IdentityError::EmptyKeySet);
        }
        let suite = current[0].suite();

        let establishment = Establishment {
            keys: current.iter().map(|k| k.verifying_key()).collect(),
            threshold: current_threshold,
            next: next
                .iter()
                .map(|k| event::key_commitment(&k.verifying_key()))
                .collect(),
            next_threshold,
            witnesses: Vec::new(),
        };
        event::validate_establishment(&establishment)?;

        let kind = match &delegator {
            None => EventKind::Inception(establishment),
            Some(d) => EventKind::DelegatedInception {
                establishment,
                delegator: d.as_str().to_string(),
            },
        };

        // Build the unsigned inception with a blank id, derive the SCID from it,
        // then fill the id in and sign.
        let mut icp = Event {
            suite,
            scid: String::new(),
            sn: 0,
            prior: Vec::new(),
            kind,
            signatures: Vec::new(),
        };
        let scid = event::derive_scid(&icp);
        icp.scid = scid.clone();
        sign_event(&mut icp, &current);

        Ok(Controller {
            scid,
            suite,
            current,
            current_threshold,
            next,
            next_threshold,
            delegator,
            events: vec![icp],
        })
    }

    /// The `did:mini:<scid>` identifier (stable across rotations).
    pub fn did(&self) -> Did {
        Did::from_scid_unchecked(&self.scid)
    }

    /// The self-certifying identifier.
    pub fn scid(&self) -> &str {
        &self.scid
    }

    /// The delegator, if this is a delegated (device) identity.
    pub fn delegator(&self) -> Option<&Did> {
        self.delegator.as_ref()
    }

    /// The current authoritative key state, as this controller sees it.
    pub fn key_state(&self) -> KeyState {
        KeyState {
            keys: self.current.iter().map(|k| k.verifying_key()).collect(),
            threshold: self.current_threshold,
            sn: (self.events.len() as u64) - 1,
        }
    }

    /// The public, verifiable Key Event Log (no secrets) for exchange.
    pub fn kel(&self) -> Kel {
        Kel::new(self.scid.clone(), self.events.clone())
    }

    /// Rotate to the pre-committed next keys, committing to a fresh next set
    /// drawn from operating-system entropy.
    pub fn rotate(&mut self) -> Result<()> {
        let new_next = generate_like(&self.next)?;
        self.rotate_with_next(new_next)
    }

    /// Rotate using an explicit next key set — deterministic, for tests.
    pub fn rotate_with_next(&mut self, new_next: Vec<SigningKey>) -> Result<()> {
        if new_next.is_empty() {
            return Err(IdentityError::EmptyKeySet);
        }
        let new_current = self.next.clone();
        let new_current_threshold = self.next_threshold;
        let new_next_threshold = new_next.len() as u32;

        let establishment = Establishment {
            keys: new_current.iter().map(|k| k.verifying_key()).collect(),
            threshold: new_current_threshold,
            next: new_next
                .iter()
                .map(|k| event::key_commitment(&k.verifying_key()))
                .collect(),
            next_threshold: new_next_threshold,
            witnesses: Vec::new(),
        };
        event::validate_establishment(&establishment)?;

        self.append(EventKind::Rotation(establishment), &new_current);

        self.current = new_current;
        self.current_threshold = new_current_threshold;
        self.next = new_next;
        self.next_threshold = new_next_threshold;
        Ok(())
    }

    /// Sign an arbitrary message with the current keys — for detached payloads
    /// like a presence-attestation transcript. Produces one indexed signature per
    /// current key; a verifier checks them against this identity's current key
    /// state and threshold via [`crate::Kel::verify_message`]. Secrets never leave
    /// the device; only signatures do.
    pub fn sign_message(&self, msg: &[u8]) -> Vec<IndexedSig> {
        self.current
            .iter()
            .enumerate()
            .map(|(i, sk)| IndexedSig {
                index: i as u32,
                signature: sk.sign(msg),
            })
            .collect()
    }

    /// Append an interaction event anchoring `anchors` under the current keys.
    pub fn interact(&mut self, anchors: Vec<[u8; 32]>) -> Result<()> {
        if anchors.len() > MAX_ANCHORS {
            return Err(IdentityError::TooManyItems {
                field: "anchors",
                max: MAX_ANCHORS,
                got: anchors.len(),
            });
        }
        let signers = self.current.clone();
        self.append(EventKind::Interaction { anchors }, &signers);
        Ok(())
    }

    /// Authorize a delegated device with a capability set (SPEC-01 §6).
    pub fn delegate_device(&mut self, device: &Did, capabilities: Capabilities) -> Result<()> {
        self.seal(vec![Seal::Delegate {
            device: device.as_str().to_string(),
            capabilities,
        }])
    }

    /// Revoke a previously delegated device (SPEC-01 §6).
    pub fn revoke_device(&mut self, device: &Did) -> Result<()> {
        self.seal(vec![Seal::Revoke {
            device: device.as_str().to_string(),
        }])
    }

    /// Append a seal event carrying `seals`, signed by the current keys.
    pub fn seal(&mut self, seals: Vec<Seal>) -> Result<()> {
        if seals.len() > MAX_SEALS {
            return Err(IdentityError::TooManyItems {
                field: "seals",
                max: MAX_SEALS,
                got: seals.len(),
            });
        }
        for seal in &seals {
            match seal {
                Seal::Delegate { device, .. } | Seal::Revoke { device } => {
                    Did::parse(device)?;
                }
            }
        }
        let signers = self.current.clone();
        self.append(EventKind::Seal { seals }, &signers);
        Ok(())
    }

    /// Build, sign, and append a non-inception event of the given `kind`.
    fn append(&mut self, kind: EventKind, signers: &[SigningKey]) {
        let prior = self
            .events
            .last()
            .expect("controller always has an inception")
            .digest();
        let sn = self.events.len() as u64;
        let mut ev = Event {
            suite: self.suite,
            scid: self.scid.clone(),
            sn,
            prior,
            kind,
            signatures: Vec::new(),
        };
        sign_event(&mut ev, signers);
        self.events.push(ev);
    }
}

/// Sign `event` with `signers`, recording each signature at the signer's index
/// in the authoritative key set (signers are in key-set order).
fn sign_event(event: &mut Event, signers: &[SigningKey]) {
    let msg = event.signing_bytes();
    event.signatures = signers
        .iter()
        .enumerate()
        .map(|(i, sk)| IndexedSig {
            index: i as u32,
            signature: sk.sign(&msg),
        })
        .collect();
}

/// Generate a fresh key set with the same cardinality as `prototype`.
fn generate_like(prototype: &[SigningKey]) -> Result<Vec<SigningKey>> {
    let mut keys = Vec::with_capacity(prototype.len());
    for _ in prototype {
        keys.push(SigningKey::generate()?);
    }
    Ok(keys)
}
