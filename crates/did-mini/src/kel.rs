//! The Key Event Log (KEL) and its verification (SPEC-01 §4, §6).
//!
//! A KEL is the public, hash-chained, append-only record of an identity's key
//! events. It carries no secrets, so it is the thing two devices exchange and
//! verify. [`Kel::verify`] walks from inception and returns the *current*
//! authoritative key state, checking — with no third party — that:
//!
//!   1. the SCID self-certifies the inception (the identity is authentic),
//!   2. every event is signed to threshold by the keys authoritative at that
//!      point,
//!   3. each rotation reveals exactly the pre-committed next keys (pre-rotation),
//!   4. the `prior` digests chain unbroken from inception to head.
//!
//! [`verify_delegation`] then answers the "many devices, one human" question
//! (SPEC-01 §6): given a human-root KEL and a device KEL, it confirms the device
//! is genuinely delegated and returns its capabilities.

use mini_crypto::VerifyingKey;

use crate::codec::{Reader, Writer};
use crate::delegation::{Capabilities, Seal};
use crate::error::{IdentityError, Result};
use crate::event::{self, Event, EventKind, IndexedSig};
use crate::limits::MAX_SCID_BYTES;
use crate::Did;

const MAX_KEL_EVENTS: usize = 1024;
const MAX_EVENT_BYTES: usize = 64 * 1024;

/// The authoritative key state of an identity at some point in its history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyState {
    /// The currently authoritative keys.
    pub keys: Vec<VerifyingKey>,
    /// How many of `keys` must sign to act.
    pub threshold: u32,
    /// Sequence number of the latest event reflected in this state.
    pub sn: u64,
    /// The standing pre-rotation commitments: multihash bytes of each *next*
    /// key (SPEC-01 §5). This is what a recovery holder's escrowed next keys
    /// must hash to — see [`crate::Controller::recover_from_kel`].
    pub next_commitments: Vec<Vec<u8>>,
    /// The threshold that will apply to the next key set once revealed.
    pub next_threshold: u32,
}

/// A public, verifiable Key Event Log for one `did:mini` identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kel {
    scid: String,
    events: Vec<Event>,
}

impl Kel {
    pub(crate) fn new(scid: String, events: Vec<Event>) -> Self {
        Kel { scid, events }
    }

    /// The self-certifying identifier (`<scid>`).
    pub fn scid(&self) -> &str {
        &self.scid
    }

    /// The `did:mini:<scid>` identifier.
    pub fn did(&self) -> Did {
        Did::from_scid_unchecked(&self.scid)
    }

    /// The events, inception first.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Number of events in the log.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the log is empty (it never should be for a valid identity).
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// If this identity is a delegated (device) identifier, the delegator's
    /// `did:mini` — otherwise `None` (SPEC-01 §6).
    pub fn delegator(&self) -> Option<Did> {
        match self.events.first().map(|e| &e.kind) {
            Some(EventKind::DelegatedInception { delegator, .. }) => Did::parse(delegator).ok(),
            _ => None,
        }
    }

    /// All delegation seals carried in this log, in order.
    pub fn seals(&self) -> Vec<Seal> {
        let mut out = Vec::new();
        for e in &self.events {
            if let EventKind::Seal { seals } = &e.kind {
                out.extend(seals.iter().cloned());
            }
        }
        out
    }

    /// The devices this identity currently delegates (Delegate minus Revoke,
    /// applied in log order; last write wins), with their capabilities.
    pub fn delegated_devices(&self) -> Vec<(Did, Capabilities)> {
        // Ordered, last-write-wins accumulation keyed by device string.
        let mut acc: Vec<(String, Capabilities)> = Vec::new();
        for seal in self.seals() {
            match seal {
                Seal::Delegate {
                    device,
                    capabilities,
                } => {
                    if let Some(slot) = acc.iter_mut().find(|(d, _)| *d == device) {
                        slot.1 = capabilities;
                    } else {
                        acc.push((device, capabilities));
                    }
                }
                Seal::Revoke { device } => {
                    acc.retain(|(d, _)| *d != device);
                }
            }
        }
        acc.into_iter()
            .filter_map(|(d, c)| Did::parse(&d).ok().map(|did| (did, c)))
            .collect()
    }

    /// Serialise the whole log to a verifiable blob for peer-to-peer exchange.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.bytes(self.scid.as_bytes());
        w.u32(self.events.len() as u32);
        for e in &self.events {
            w.bytes(&e.full_bytes());
        }
        w.into_bytes()
    }

    /// Parse a blob produced by [`Kel::to_bytes`]. Does **not** verify; call
    /// [`Kel::verify`] to authenticate.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let scid = String::from_utf8(r.bytes_limited("kel.scid", MAX_SCID_BYTES)?)
            .map_err(|_| IdentityError::BadEvent)?;
        Did::parse(&format!("did:mini:{scid}"))?;
        let n = r.u32()? as usize;
        if n > MAX_KEL_EVENTS {
            return Err(IdentityError::TooManyItems {
                field: "kel.events",
                max: MAX_KEL_EVENTS,
                got: n,
            });
        }
        let mut events = Vec::with_capacity(n);
        for _ in 0..n {
            let event_bytes = r.bytes_limited("kel.event", MAX_EVENT_BYTES)?;
            let mut er = Reader::new(&event_bytes);
            let ev = event::decode(&mut er)?;
            if !er.finished() {
                return Err(IdentityError::TrailingBytes);
            }
            events.push(ev);
        }
        if !r.finished() {
            return Err(IdentityError::TrailingBytes);
        }
        Ok(Kel { scid, events })
    }

    /// Verify detached signatures over `msg` against this identity's *current* key
    /// state, to its threshold. Used for signed payloads such as presence
    /// attestations, where the signer proves control with the same keys the KEL
    /// authorizes — counting distinct public keys, not just distinct indices.
    pub fn verify_message(&self, msg: &[u8], sigs: &[IndexedSig]) -> Result<()> {
        let state = self.verify()?;
        let count = event::count_valid_signers(msg, &state.keys, sigs);
        if count >= state.threshold {
            Ok(())
        } else {
            Err(IdentityError::SignatureThresholdNotMet {
                needed: state.threshold,
                got: count,
            })
        }
    }

    /// Walk the log from inception and return the current authoritative key
    /// state, or an error identifying the first inconsistency. Fully offline.
    pub fn verify(&self) -> Result<KeyState> {
        let first = self.events.first().ok_or(IdentityError::EmptyKel)?;

        // Inception may be a plain or a delegated inception.
        let icp = match &first.kind {
            EventKind::Inception(est) => est,
            EventKind::DelegatedInception { establishment, .. } => establishment,
            _ => return Err(IdentityError::NotInception),
        };
        if first.sn != 0 || !first.prior.is_empty() {
            return Err(IdentityError::NotInception);
        }
        event::validate_establishment(icp)?;
        Did::parse(&format!("did:mini:{}", self.scid))?;
        if let EventKind::DelegatedInception { delegator, .. } = &first.kind {
            let did = Did::parse(delegator)?;
            if did.scid() == self.scid {
                return Err(IdentityError::BadEvent);
            }
        }

        // (1) The SCID must self-certify this inception, and match the log's id.
        let derived = event::derive_scid(first);
        if derived != first.scid || first.scid != self.scid {
            return Err(IdentityError::ScidMismatch);
        }

        // (2) Inception must be signed to threshold by its own keys.
        event::verify_threshold(first, &icp.keys, icp.threshold)?;

        let mut cur_keys = icp.keys.clone();
        let mut cur_threshold = icp.threshold;
        let mut next = icp.next.clone();
        let mut next_threshold = icp.next_threshold;
        let mut prev_digest = first.digest();

        for (i, ev) in self.events.iter().enumerate().skip(1) {
            let sn = i as u64;
            if ev.scid != self.scid {
                return Err(IdentityError::ScidMismatch);
            }
            if ev.sn != sn {
                return Err(IdentityError::WrongSequence {
                    expected: sn,
                    got: ev.sn,
                });
            }
            // (4) Chain integrity.
            if ev.prior != prev_digest {
                return Err(IdentityError::BrokenChain { sn });
            }

            match &ev.kind {
                // Inception of either form can only appear at sn 0.
                EventKind::Inception(_) | EventKind::DelegatedInception { .. } => {
                    return Err(IdentityError::NotInception)
                }
                EventKind::Rotation(est) => {
                    // (3) Pre-rotation: the revealed keys must equal the prior
                    // commitment, and adopt the pre-committed threshold.
                    event::validate_establishment(est)?;
                    if est.keys.len() != next.len() || est.threshold != next_threshold {
                        return Err(IdentityError::PreRotationMismatch { sn });
                    }
                    for (k, commitment) in est.keys.iter().zip(next.iter()) {
                        if &event::key_commitment(k) != commitment {
                            return Err(IdentityError::PreRotationMismatch { sn });
                        }
                    }
                    // The rotation is signed by the newly-revealed (now current)
                    // keys — a leaked old key cannot produce this.
                    event::verify_threshold(ev, &est.keys, est.threshold)?;
                    cur_keys = est.keys.clone();
                    cur_threshold = est.threshold;
                    next = est.next.clone();
                    next_threshold = est.next_threshold;
                }
                // Non-establishment events: signed by current keys, control
                // unchanged.
                EventKind::Interaction { .. } | EventKind::Seal { .. } => {
                    event::verify_threshold(ev, &cur_keys, cur_threshold)?;
                }
            }

            prev_digest = ev.digest();
        }

        Ok(KeyState {
            keys: cur_keys,
            threshold: cur_threshold,
            sn: (self.events.len() as u64) - 1,
            next_commitments: next,
            next_threshold,
        })
    }
}

/// Confirm a device is genuinely delegated by a human-root, returning the
/// capabilities granted (SPEC-01 §6). This is the "many devices, one human"
/// check, and it is *mutual*: the device's identifier commits to its delegator
/// (its `dip`), and the root's KEL carries an unrevoked `Delegate` seal for the
/// device. Neither side alone can fake the link.
///
/// The `root` must itself be a **non-delegated** identity: delegation chains
/// (a device delegating sub-devices) are rejected with
/// [`IdentityError::RootIsDelegated`], so no caller counting "one identity
/// root" can be handed a device posing as a root. Device hierarchies, if ever
/// wanted, are a deliberate future design ([roadmap #14]) — not something this
/// check quietly permits today.
///
/// **Freshness is the caller's problem, stated loudly:** this function checks
/// the root KEL *it is given*. A revoked device stays "delegated" in any stale
/// copy of the root's KEL from before the revocation, so callers must obtain
/// the freshest root KEL they can (and should pin the highest `sn` they have
/// ever seen per SCID, refusing to go backwards). Witness receipts (SPEC-01
/// §7, M3) will strengthen this; until then this is a documented limitation
/// (see `docs/audits/issue-13-identity-recovery-audit.md`).
///
/// [roadmap #14]: ../../issues/14
pub fn verify_delegation(root: &Kel, device: &Kel) -> Result<Capabilities> {
    // Both logs must be internally valid first.
    root.verify()?;
    device.verify()?;

    // The root must be a true root: a delegated identity cannot delegate.
    if root.delegator().is_some() {
        return Err(IdentityError::RootIsDelegated);
    }

    // The device must name this root as its delegator.
    let delegator = device.delegator().ok_or(IdentityError::NotDelegated)?;
    if delegator.scid() != root.scid() {
        return Err(IdentityError::NotDelegated);
    }

    // The root must currently authorize this device.
    let device_did = device.did();
    for (dev, caps) in root.delegated_devices() {
        if dev.as_str() == device_did.as_str() {
            return Ok(caps);
        }
    }
    Err(IdentityError::NotDelegated)
}
