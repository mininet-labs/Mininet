//! Verifying storage-served receipts.

use std::collections::HashSet;

use did_mini::{verify_delegation, Capabilities, Did, Kel};
use mini_objects::ObjectId;

use crate::error::{Result, StorageProofError};
use crate::receipt::{ServeReceipt, RECEIPT_VERSION};

/// Freshness policy for accepting a receipt.
#[derive(Debug, Clone)]
pub struct FreshnessPolicy {
    /// Receipts older than this (relative to the verifier's `now_ms`) are
    /// refused outright — replay resistance must not depend only on the
    /// replay guard's memory surviving restarts. `0` disables the check (a
    /// deliberate caller choice, e.g. offline re-verification of history).
    pub max_age_ms: u64,
}

impl FreshnessPolicy {
    /// A conservative default: a receipt is stale after one day.
    pub fn default_policy() -> Self {
        FreshnessPolicy {
            max_age_ms: 86_400_000,
        }
    }
}

/// Everything a verifier needs besides the receipt itself.
#[derive(Debug)]
pub struct VerifyContext<'a> {
    /// The host's identity root KEL.
    pub host_root: &'a Kel,
    /// The witness's identity root KEL.
    pub witness_root: &'a Kel,
    /// The host device's KEL.
    pub host_device: &'a Kel,
    /// The witness device's KEL.
    pub witness_device: &'a Kel,
    /// Freshness policy.
    pub policy: &'a FreshnessPolicy,
    /// The verifier's current time in ms, if available (enables freshness checks).
    pub now_ms: Option<u64>,
}

/// A verified storage serve: the two **identity roots** involved, what was
/// served, and when. The same shape `mini_reward::accrue_storage` expects —
/// mirrors how `mini_presence::PresenceVerdict` feeds `mini_reward::accrue`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeVerdict {
    /// The identity root that hosted/served the content.
    pub host_root: Did,
    /// The identity root that witnessed the serve.
    pub witness_root: Did,
    /// What was served.
    pub content_id: ObjectId,
    /// Bytes served.
    pub bytes: u64,
    /// When the serve completed (ms).
    pub at_ms: u64,
}

/// Tracks seen `(device, nonce)` pairs to reject replays. Same shape and
/// same persistence requirement as `mini_presence::ReplayGuard`.
pub trait ReplayGuard {
    /// Whether `(device, nonce)` has been seen before (no recording).
    fn is_seen(&self, device: &Did, nonce: &[u8; 32]) -> bool;

    /// Record `(device, nonce)` as seen. Returns `true` if it was fresh,
    /// `false` if it had been seen before.
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

/// Verify a storage-served receipt, returning the verdict on success.
///
/// **What this proves, and what it doesn't:** this proves two delegated,
/// `ATTEST`-capable devices of two distinct identity roots mutually signed
/// a matching claim that HOST served WITNESS `bytes` of `content_id` at
/// `at_ms`. It does **not** prove the host will keep serving that content
/// tomorrow — durable storage-over-time needs a harder property
/// (challenge-response proof-of-storage), which remains `pending`, the same
/// honest limit `mini-presence` states for distance-bounding. This is the
/// signed envelope such a proof can slot into later.
pub fn verify_serve(
    receipt: &ServeReceipt,
    ctx: &VerifyContext<'_>,
    replay: &mut dyn ReplayGuard,
) -> Result<ServeVerdict> {
    let f = &receipt.fields;

    if f.version != RECEIPT_VERSION {
        return Err(StorageProofError::UnsupportedVersion(f.version));
    }
    if f.bytes == 0 {
        return Err(StorageProofError::ZeroBytes);
    }
    if let Some(now) = ctx.now_ms {
        if ctx.policy.max_age_ms > 0 && f.at_ms < now.saturating_sub(ctx.policy.max_age_ms) {
            return Err(StorageProofError::TooOld);
        }
    }
    // The two nonces must differ (a party can't echo the other's nonce).
    if f.host_nonce == f.witness_nonce {
        return Err(StorageProofError::Replay);
    }

    let transcript = f.transcript();
    check_party(
        &f.host_device,
        ctx.host_root,
        ctx.host_device,
        &receipt.host_sig,
        &transcript,
    )?;
    check_party(
        &f.witness_device,
        ctx.witness_root,
        ctx.witness_device,
        &receipt.witness_sig,
        &transcript,
    )?;

    // A host cannot witness (and be paid for) its own storage. Checked after
    // delegation so the roots are verified, same ordering mini-presence uses
    // for its self-presence check.
    if ctx.host_root.scid() == ctx.witness_root.scid() {
        return Err(StorageProofError::SelfServe);
    }

    // Record nonces only after EVERYTHING above passed, so a partially-
    // invalid receipt never mutates replay state.
    if replay.is_seen(&f.host_device, &f.host_nonce)
        || replay.is_seen(&f.witness_device, &f.witness_nonce)
    {
        return Err(StorageProofError::Replay);
    }
    replay.check_and_record(&f.host_device, &f.host_nonce);
    replay.check_and_record(&f.witness_device, &f.witness_nonce);

    Ok(ServeVerdict {
        host_root: ctx.host_root.did(),
        witness_root: ctx.witness_root.did(),
        content_id: f.content_id.clone(),
        bytes: f.bytes,
        at_ms: f.at_ms,
    })
}

fn check_party(
    claimed_device: &Did,
    root: &Kel,
    device: &Kel,
    sig: &[did_mini::IndexedSig],
    transcript: &[u8],
) -> Result<()> {
    if device.did().as_str() != claimed_device.as_str() {
        return Err(StorageProofError::DeviceMismatch);
    }
    // The device must be a currently-delegated device of this identity root
    // (verifies both KELs, rejects revoked devices) and hold ATTEST.
    let caps = verify_delegation(root, device)?;
    if !caps.contains(Capabilities::ATTEST) {
        return Err(StorageProofError::MissingAttestCapability);
    }
    device.verify_message(transcript, sig)?;
    Ok(())
}
