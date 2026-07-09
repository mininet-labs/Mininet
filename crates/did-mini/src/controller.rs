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
//!
//! Pairwise pseudonyms (SPEC-01 §10): [`Controller::incept_pairwise_pseudonym`]
//! deterministically mints an independent, unlinkable-by-default root per
//! context from this root's key material, so one human can run many
//! pseudonym identities as one function call each, not N hand-managed seeds.

use mini_crypto::{SignatureSuite, SigningKey};
use zeroize::Zeroize;

use crate::delegation::{Capabilities, Seal};
use crate::error::{IdentityError, Result};
use crate::event::{self, Establishment, Event, EventKind, IndexedSig};
use crate::kel::{Kel, KeyState};
use crate::limits::{MAX_ANCHORS, MAX_SEALS};
use crate::Did;

/// Domain-separation salt for [`Controller::incept_pairwise_pseudonym`]'s
/// HKDF derivation. Versioned in the string itself (`v1`) so a future
/// derivation scheme can coexist without silently colliding with this one.
const PAIRWISE_PSEUDONYM_SALT: &[u8] = b"mininet/did-mini/pairwise-pseudonym/v1";

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

    /// Deterministically derive an independent pairwise pseudonym root from
    /// this root's current key material and an arbitrary `context` (SPEC-01
    /// §10; founder decision 2026-07-07): e.g. a counterparty's DID, a
    /// community name, `b"wall:my-project"` — anything the caller wants to
    /// keep stable to recover the *same* pseudonym again later, with no
    /// extra seed storage. Different contexts yield unlinkable,
    /// independent-looking roots; the same root + same context always yields
    /// the same pseudonym.
    ///
    /// The derived root is an **ordinary, independent `did:mini` identity**
    /// by every check this crate can run — its own SCID, KEL, and
    /// pre-rotation commitments. Nothing in its wire form links it back to
    /// this root; the derivation itself never leaves the device (G1) and the
    /// KDF's pseudorandomness is what stands between an observer and
    /// correlating the two, not any protocol-visible fact.
    ///
    /// Requires this root to be a single-key (1-of-1) identity, the common
    /// case for `incept_single*` — there is no canonical "the" key to derive
    /// from on a multi-key/threshold root, so those return
    /// [`IdentityError::PairwiseRequiresSingleKey`].
    pub fn incept_pairwise_pseudonym(&self, context: &[u8]) -> Result<Controller> {
        if self.current.len() != 1 || self.current_threshold != 1 {
            return Err(IdentityError::PairwiseRequiresSingleKey);
        }
        let mut ikm = self.current[0].to_seed_bytes();
        let derived = mini_crypto::KdfSuite::HkdfSha256.derive_bytes(
            Some(PAIRWISE_PSEUDONYM_SALT),
            &ikm,
            context,
            64,
        );
        ikm.zeroize();
        let mut derived = derived.map_err(IdentityError::Crypto)?;
        let mut current_seed = [0u8; 32];
        let mut next_seed = [0u8; 32];
        current_seed.copy_from_slice(&derived[..32]);
        next_seed.copy_from_slice(&derived[32..]);
        derived.zeroize();
        let out = Controller::incept_single_from_seeds(&current_seed, &next_seed);
        // Best-effort scrub of every local copy of secret material (issue #12
        // audit): the root seed copy, the KDF output, and the derived seeds all
        // leave no residue beyond the keys now held inside the controller.
        current_seed.zeroize();
        next_seed.zeroize();
        out
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
            next_commitments: self
                .next
                .iter()
                .map(|k| event::key_commitment(&k.verifying_key()))
                .collect(),
            next_threshold: self.next_threshold,
        }
    }

    /// The public, verifiable Key Event Log (no secrets) for exchange.
    pub fn kel(&self) -> Kel {
        Kel::new(self.scid.clone(), self.events.clone())
    }

    /// Rotate to the pre-committed next keys, committing to a fresh next set
    /// drawn from operating-system entropy. The next-set threshold policy is
    /// **preserved** across the rotation (a 2-of-3 identity stays 2-of-3) —
    /// see [`Controller::rotate_with_next_and_threshold`] to change it.
    pub fn rotate(&mut self) -> Result<()> {
        let new_next = generate_like(&self.next)?;
        let threshold = self.next_threshold;
        self.rotate_with_next_and_threshold(new_next, threshold)
    }

    /// Rotate using an explicit next key set, preserving the current
    /// next-threshold policy — deterministic, for tests.
    ///
    /// Audit note (issue #12): this previously hardcoded the new next-set
    /// threshold to N-of-N (`new_next.len()`), which silently converted any
    /// M-of-N identity into N-of-N after its first rotation — changing the
    /// availability/security policy chosen at inception and making every
    /// future rotation fail if even one next key was lost. The threshold now
    /// carries forward unchanged; `validate_establishment` rejects the
    /// rotation explicitly if the preserved threshold cannot fit the new set.
    pub fn rotate_with_next(&mut self, new_next: Vec<SigningKey>) -> Result<()> {
        let threshold = self.next_threshold;
        self.rotate_with_next_and_threshold(new_next, threshold)
    }

    /// Rotate using an explicit next key set **and** an explicit threshold for
    /// that set — the only way the M-of-N policy changes is this deliberate
    /// call, never as a side effect of an ordinary rotation.
    pub fn rotate_with_next_and_threshold(
        &mut self,
        new_next: Vec<SigningKey>,
        new_next_threshold: u32,
    ) -> Result<()> {
        if new_next.is_empty() {
            return Err(IdentityError::EmptyKeySet);
        }
        let new_current = self.next.clone();
        let new_current_threshold = self.next_threshold;

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

    /// Recover control of an identity from its public KEL plus the escrowed
    /// **next** secret keys — the lost-device recovery path (SPEC-01 §5's
    /// pre-rotation, used as designed; issue #13).
    ///
    /// This is why pre-rotation exists: the *next* keys are committed but
    /// unrevealed, so their seeds can live somewhere that is not the device —
    /// a paper backup, a safe, an heir's envelope. When the device (holding
    /// the current keys) is lost, stolen, or its holder dies, whoever holds
    /// the next-key seeds reconstructs a controller from:
    ///
    ///   1. the identity's public KEL (fetched from any peer — it's public), and
    ///   2. the escrowed next signing keys,
    ///
    /// and this function appends the recovery rotation: the escrowed keys are
    /// revealed as the new current keys (they must hash to the KEL's standing
    /// pre-rotation commitments, in order — otherwise
    /// [`IdentityError::RecoveryKeysMismatch`]), and `new_next` /
    /// `new_next_threshold` become the fresh escrow commitment. The old
    /// device's keys are dead from this event onward: they can no longer
    /// extend the KEL, because control now requires the newly revealed set.
    ///
    /// **What this is not:** it cannot recover an identity whose next-key
    /// seeds are also lost (nothing can — that identity is permanently
    /// orphaned, by design, because anything that could recover it without
    /// the committed keys could also steal it), and it does not resolve
    /// *races*: a thief holding the stolen device's current keys can keep
    /// signing until peers see this recovery rotation, and if the thief
    /// somehow also obtained the next seeds, whichever rotation a verifier
    /// sees first wins that verifier — divergence detection is the witness
    /// batch (M3). See `docs/audits/issue-13-identity-recovery-audit.md`.
    pub fn recover_from_kel(
        kel: &Kel,
        recovered_next: Vec<SigningKey>,
        new_next: Vec<SigningKey>,
        new_next_threshold: u32,
    ) -> Result<Controller> {
        if recovered_next.is_empty() || new_next.is_empty() {
            return Err(IdentityError::EmptyKeySet);
        }
        // The KEL must be internally valid before we extend it.
        let state = kel.verify()?;

        // The escrowed keys must be exactly the committed next set, in
        // commitment order. Order-sensitive on purpose: the rotation verifier
        // zips revealed keys with commitments positionally, so we surface a
        // mismatch here rather than emit an event that can never verify.
        if recovered_next.len() != state.next_commitments.len() {
            return Err(IdentityError::RecoveryKeysMismatch);
        }
        for (k, commitment) in recovered_next.iter().zip(state.next_commitments.iter()) {
            if &event::key_commitment(&k.verifying_key()) != commitment {
                return Err(IdentityError::RecoveryKeysMismatch);
            }
        }

        let suite = recovered_next[0].suite();
        let mut ctrl = Controller {
            scid: kel.scid().to_string(),
            suite,
            current: recovered_next,
            current_threshold: state.next_threshold,
            next: new_next,
            next_threshold: new_next_threshold,
            delegator: kel.delegator(),
            events: kel.events().to_vec(),
        };

        let establishment = Establishment {
            keys: ctrl.current.iter().map(|k| k.verifying_key()).collect(),
            threshold: ctrl.current_threshold,
            next: ctrl
                .next
                .iter()
                .map(|k| event::key_commitment(&k.verifying_key()))
                .collect(),
            next_threshold: ctrl.next_threshold,
            witnesses: Vec::new(),
        };
        event::validate_establishment(&establishment)?;

        let signers = ctrl.current.clone();
        ctrl.append(EventKind::Rotation(establishment), &signers);

        // The result must verify end-to-end as an ordinary KEL — recovery
        // produces a standard rotation, indistinguishable from a planned one.
        ctrl.kel().verify()?;
        Ok(ctrl)
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
