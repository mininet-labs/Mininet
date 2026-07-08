//! Verifying presence attestations.

use std::collections::HashSet;

use did_mini::{verify_delegation, Capabilities, Did, Kel};

use crate::attestation::{kel_digest, Party, PresenceAttestation, PRESENCE_VERSION};
use crate::error::{PresenceError, Result};

/// Range/timing policy for accepting an attestation.
#[derive(Debug, Clone)]
pub struct RangePolicy {
    /// The best (minimum) round-trip sample must be at or under this, in ms.
    pub max_rtt_ms: u32,
    /// At least this many round-trip samples are required.
    pub min_rtt_samples: u32,
    /// The whole session must complete within this many ms.
    pub max_session_ms: u64,
    /// Tolerated difference between a verifier's clock and the finish time.
    pub max_clock_skew_ms: u64,
    /// Attestations older than this (relative to the verifier's `now_ms`) are
    /// refused outright — replay resistance must not depend only on the
    /// replay guard's memory surviving restarts. `0` disables the check (a
    /// deliberate caller choice, e.g. offline re-verification of history).
    pub max_age_ms: u64,
    /// Optional tighter distance bound, in centimeters, enforced only when
    /// the attestation carries [`crate::attestation::UwbRanging`] evidence
    /// (D-0034 point 1). `None` means this policy does not require or check
    /// hardware ranging — the software RTT bound above is still always
    /// enforced regardless of this setting. Additive, never a substitute for
    /// the RTT check.
    pub max_uwb_distance_cm: Option<u32>,
}

impl RangePolicy {
    /// A conservative default for a BLE round-trip proximity bound. No UWB
    /// threshold by default — devices without a UWB chip must still pass on
    /// software RTT alone.
    pub fn ble_default() -> Self {
        RangePolicy {
            max_rtt_ms: 50,
            min_rtt_samples: 4,
            max_session_ms: 30_000,
            max_clock_skew_ms: 120_000,
            // One day: with the guard persisted (see `ReplayGuard`), a nonce
            // only needs to be remembered for this window.
            max_age_ms: 86_400_000,
            max_uwb_distance_cm: None,
        }
    }
}

/// Everything a verifier needs besides the attestation itself.
#[derive(Debug)]
pub struct VerifyContext<'a> {
    /// The initiating identity root's KEL.
    pub initiator_root: &'a Kel,
    /// The responding identity root's KEL.
    pub responder_root: &'a Kel,
    /// The initiating device's KEL.
    pub initiator_device: &'a Kel,
    /// The responding device's KEL.
    pub responder_device: &'a Kel,
    /// Range/timing policy.
    pub policy: &'a RangePolicy,
    /// The verifier's current time in ms, if available (enables freshness checks).
    pub now_ms: Option<u64>,
    /// The channel binding the verifier observed for this session, if it
    /// participated (enables channel-binding enforcement).
    pub expected_binding: Option<[u8; 32]>,
}

/// A verified co-presence: the two **identity roots** (delegators) who were together, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresenceVerdict {
    /// The initiating identity root (the device's delegator).
    pub initiator_root: Did,
    /// The responding identity root.
    pub responder_root: Did,
    /// Finish time of the attested session (ms).
    pub at_ms: u64,
    /// Whether this presence was corroborated by hardware (UWB) ranging
    /// evidence, not just the software RTT bound. Carried for downstream
    /// consumers that want to weight hardware-ranged presence more heavily
    /// (e.g. `mini-uniqueness`'s physical-presence signal) — this crate
    /// itself treats both as valid presence once policy checks pass.
    pub hardware_ranged: bool,
}

impl PresenceVerdict {
    /// An order-independent key for the identity-root pair, so the scoring layer can
    /// count a pairing once and discount repeats between the same two roots.
    pub fn pair_key(&self) -> (String, String) {
        let a = self.initiator_root.as_str().to_string();
        let b = self.responder_root.as_str().to_string();
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }
}

/// Tracks seen `(device, nonce)` pairs to reject replays.
///
/// **This trait is the persistence interface**: production verifiers must back
/// it with durable storage (the device store), because replay resistance has
/// to survive process restarts. `InMemoryReplayGuard` is for tests and for
/// composing with [`RangePolicy::max_age_ms`], which bounds how long any guard
/// must remember a nonce.
pub trait ReplayGuard {
    /// Whether `(device, nonce)` has been seen before (no recording).
    fn is_seen(&self, device: &Did, nonce: &[u8; 32]) -> bool;

    /// Record `(device, nonce)` as seen. Return `true` if it was fresh, `false`
    /// if it had been seen before.
    fn check_and_record(&mut self, device: &Did, nonce: &[u8; 32]) -> bool;
}

/// A simple in-memory [`ReplayGuard`].
#[derive(Debug, Default)]
pub struct InMemoryReplayGuard {
    seen: HashSet<(String, [u8; 32])>,
}

impl InMemoryReplayGuard {
    /// A new, empty guard.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ReplayGuard for InMemoryReplayGuard {
    fn is_seen(&self, device: &Did, nonce: &[u8; 32]) -> bool {
        self.seen.contains(&(device.as_str().to_string(), *nonce))
    }

    fn check_and_record(&mut self, device: &Did, nonce: &[u8; 32]) -> bool {
        self.seen.insert((device.as_str().to_string(), *nonce))
    }
}

/// Verify a presence attestation, returning the two co-present identity roots on success.
///
/// Rejects anything a hostile peer could try: wrong version, non-proximity
/// transport, wrong channel, bad time window, insufficient/too-far range, a device
/// that isn't a delegated `ATTEST`-capable device of the named identity root, a bad
/// signature, or a replayed nonce.
pub fn verify_presence(
    att: &PresenceAttestation,
    ctx: &VerifyContext<'_>,
    replay: &mut dyn ReplayGuard,
) -> Result<PresenceVerdict> {
    let f = &att.fields;

    if f.version != PRESENCE_VERSION {
        return Err(PresenceError::UnsupportedVersion(f.version));
    }
    if !f.transport.is_proximity() {
        return Err(PresenceError::NotProximityTransport);
    }
    if let Some(expected) = ctx.expected_binding {
        if expected != f.channel_binding {
            return Err(PresenceError::BindingMismatch);
        }
    }

    // Time window.
    if f.finished_at_ms < f.started_at_ms
        || f.finished_at_ms - f.started_at_ms > ctx.policy.max_session_ms
    {
        return Err(PresenceError::BadTimeWindow);
    }
    if let Some(now) = ctx.now_ms {
        if f.finished_at_ms > now.saturating_add(ctx.policy.max_clock_skew_ms) {
            return Err(PresenceError::BadTimeWindow);
        }
        // Max age: too-old attestations are refused so replay windows are
        // finite even across verifier restarts.
        if ctx.policy.max_age_ms > 0 && f.finished_at_ms < now.saturating_sub(ctx.policy.max_age_ms)
        {
            return Err(PresenceError::BadTimeWindow);
        }
    }

    // Range: enough samples, and the best (tightest) round-trip within policy.
    if (f.rtt_samples_ms.len() as u32) < ctx.policy.min_rtt_samples {
        return Err(PresenceError::NotEnoughRangeSamples);
    }
    let best = f.rtt_samples_ms.iter().copied().min().unwrap_or(u32::MAX);
    if best > ctx.policy.max_rtt_ms {
        return Err(PresenceError::RangeExceeded);
    }

    // Hardware (UWB) ranging: additive tightening only. The RTT bound above
    // always applies regardless; this only adds a stricter check on top when
    // both the policy asks for it and the attestation actually carries the
    // evidence (D-0034 point 1 — devices without a UWB chip are unaffected).
    if let (Some(max_cm), Some(uwb)) = (ctx.policy.max_uwb_distance_cm, &f.uwb) {
        if uwb.distance_cm > max_cm {
            return Err(PresenceError::UwbRangeExceeded);
        }
    }

    // The two nonces must differ (a party can't echo the other's nonce).
    if f.initiator.nonce == f.responder.nonce {
        return Err(PresenceError::Replay);
    }

    let transcript = f.transcript();
    check_party(
        &f.initiator,
        ctx.initiator_root,
        ctx.initiator_device,
        &att.initiator_sig,
        &transcript,
    )?;
    check_party(
        &f.responder,
        ctx.responder_root,
        ctx.responder_device,
        &att.responder_sig,
        &transcript,
    )?;

    // An identity root cannot be co-present with itself: presence is evidence of two
    // delegated devices of distinct identity roots meeting (P2 target: two identity roots).
    // Checked after delegation so the roots are verified.
    if ctx.initiator_root.scid() == ctx.responder_root.scid() {
        return Err(PresenceError::SelfPresence);
    }

    // Record nonces only after EVERYTHING above passed, so a partially-invalid
    // attestation never mutates replay state. Two-phase: check both first, then
    // record both — atomic even when one nonce is fresh and the other replayed.
    if replay.is_seen(&f.initiator.device, &f.initiator.nonce)
        || replay.is_seen(&f.responder.device, &f.responder.nonce)
    {
        return Err(PresenceError::Replay);
    }
    replay.check_and_record(&f.initiator.device, &f.initiator.nonce);
    replay.check_and_record(&f.responder.device, &f.responder.nonce);

    Ok(PresenceVerdict {
        initiator_root: ctx.initiator_root.did(),
        responder_root: ctx.responder_root.did(),
        at_ms: f.finished_at_ms,
        hardware_ranged: f.uwb.is_some(),
    })
}

fn check_party(
    party: &Party,
    root: &Kel,
    device: &Kel,
    sig: &[did_mini::IndexedSig],
    transcript: &[u8],
) -> Result<()> {
    // The named device must be the KEL supplied for it, at the pinned state.
    if device.did().as_str() != party.device.as_str() {
        return Err(PresenceError::DeviceMismatch);
    }
    if kel_digest(device) != party.kel_digest {
        return Err(PresenceError::KelDigestMismatch);
    }

    // The device must be a currently-delegated device of this identity root (this
    // verifies both KELs and rejects revoked devices) and hold ATTEST.
    let caps = verify_delegation(root, device)?;
    if !caps.contains(Capabilities::ATTEST) {
        return Err(PresenceError::MissingAttestCapability);
    }

    // The signature must verify against the device's current keys.
    device.verify_message(transcript, sig)?;
    Ok(())
}
