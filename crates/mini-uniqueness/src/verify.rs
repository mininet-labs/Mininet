//! Verifying vouch attestations.

use std::collections::HashSet;

use did_mini::{verify_delegation, Capabilities, Did, Kel};

use crate::error::{Result, UniquenessError};
use crate::vouch::{VouchAttestation, VoucherParty, VOUCH_VERSION};

/// Everything a verifier needs besides the attestation itself.
#[derive(Debug)]
pub struct VerifyContext<'a> {
    /// Party `a`'s identity root KEL.
    pub a_root: &'a Kel,
    /// Party `b`'s identity root KEL.
    pub b_root: &'a Kel,
    /// Party `a`'s device KEL.
    pub a_device: &'a Kel,
    /// Party `b`'s device KEL.
    pub b_device: &'a Kel,
}

/// A verified mutual vouch: the two **identity roots** who vouched for each
/// other, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VouchVerdict {
    /// Party `a`'s identity root.
    pub a_root: Did,
    /// Party `b`'s identity root.
    pub b_root: Did,
    /// When the vouch was asserted (ms).
    pub at_ms: u64,
}

impl VouchVerdict {
    /// An order-independent key for the identity-root pair, so the graph
    /// layer treats a vouch as one undirected edge regardless of which side
    /// is named `a` or `b`.
    pub fn pair_key(&self) -> (String, String) {
        let a = self.a_root.as_str().to_string();
        let b = self.b_root.as_str().to_string();
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }
}

/// Tracks seen `(device, nonce)` pairs to reject replays. Same shape and same
/// durability requirement as `mini_presence::ReplayGuard` — production
/// verifiers must back this with durable storage.
pub trait ReplayGuard {
    /// Whether `(device, nonce)` has been seen before (no recording).
    fn is_seen(&self, device: &Did, nonce: &[u8; 32]) -> bool;

    /// Record `(device, nonce)` as seen. Return `true` if it was fresh,
    /// `false` if it had been seen before.
    fn check_and_record(&mut self, device: &Did, nonce: &[u8; 32]) -> bool;
}

/// A simple in-memory [`ReplayGuard`], for tests.
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

/// Verify a vouch attestation, returning the two identity roots on success.
///
/// Requires: matching protocol version, distinct nonces, both devices
/// currently delegated/unrevoked `ATTEST`-capable devices of their named
/// identity root, valid signatures over the transcript, distinct identity
/// roots (an identity root cannot vouch for itself), and non-replayed nonces.
/// Unlike `mini_presence::verify_presence`, there is no proximity or range
/// requirement — vouching may happen over any channel.
pub fn verify_vouch(
    att: &VouchAttestation,
    ctx: &VerifyContext<'_>,
    replay: &mut dyn ReplayGuard,
) -> Result<VouchVerdict> {
    let f = &att.fields;

    if f.version != VOUCH_VERSION {
        return Err(UniquenessError::UnsupportedVersion(f.version));
    }

    // The two nonces must differ (a party can't echo the other's nonce).
    if f.a.nonce == f.b.nonce {
        return Err(UniquenessError::Replay);
    }

    let transcript = f.transcript();
    check_party(&f.a, ctx.a_root, ctx.a_device, &att.a_sig, &transcript)?;
    check_party(&f.b, ctx.b_root, ctx.b_device, &att.b_sig, &transcript)?;

    // An identity root cannot vouch for itself.
    if ctx.a_root.scid() == ctx.b_root.scid() {
        return Err(UniquenessError::SelfVouch);
    }

    // Record nonces only after everything above passed (two-phase, same
    // rationale as mini-presence: a partially-invalid attestation never
    // mutates replay state).
    if replay.is_seen(&f.a.device, &f.a.nonce) || replay.is_seen(&f.b.device, &f.b.nonce) {
        return Err(UniquenessError::Replay);
    }
    replay.check_and_record(&f.a.device, &f.a.nonce);
    replay.check_and_record(&f.b.device, &f.b.nonce);

    Ok(VouchVerdict {
        a_root: ctx.a_root.did(),
        b_root: ctx.b_root.did(),
        at_ms: f.asserted_at_ms,
    })
}

fn check_party(
    party: &VoucherParty,
    root: &Kel,
    device: &Kel,
    sig: &[did_mini::IndexedSig],
    transcript: &[u8],
) -> Result<()> {
    if device.did().as_str() != party.device.as_str() {
        return Err(UniquenessError::DeviceMismatch);
    }
    if mini_presence::kel_digest(device) != party.kel_digest {
        return Err(UniquenessError::KelDigestMismatch);
    }

    let caps = verify_delegation(root, device)?;
    if !caps.contains(Capabilities::ATTEST) {
        return Err(UniquenessError::MissingAttestCapability);
    }

    device.verify_message(transcript, sig)?;
    Ok(())
}
