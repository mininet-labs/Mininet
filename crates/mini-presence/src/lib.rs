//! Range-bound co-presence attestations.
//!
//! A presence attestation is the honest core of the keystone demo: proof that two
//! delegated devices — each bound to a `did:mini` **identity root** — were
//! physically near each other at a moment in time, established offline over an
//! encrypted [`mini_bearer`] channel — not "two internet peers signed something."
//! (Identity root, not human: unique personhood is SPEC-02, unimplemented; this
//! layer proves proximity of delegated devices, which SPEC-02 later builds on.)
//!
//! Both devices sign one transcript ([`AttestationFields::transcript`]) that binds:
//! the session's channel binding, each device's `did:mini` and KEL digest, fresh
//! nonces, the time window, the RTT range samples, and the transport. Verification
//! ([`verify_presence`]) then requires, for *both* sides:
//!
//! - the device KEL verifies and is a delegated device of a identity root, unrevoked,
//!   with the `ATTEST` capability (SPEC-01 §6 delegation feeds SPEC-02 presence);
//! - the signature verifies against the device's current keys (distinct-key
//!   threshold);
//! - the attestation is bound to *this* channel and to fresh, non-replayed nonces;
//! - the transport is a proximity bearer and the round-trip range is under policy.
//!
//! The verdict names the two **identity roots** (the delegators), so the scoring
//! layer counts a co-presence once per identity-root pair (the P2 *target* is one
//! human-pair; personhood pending — SPEC-02), and can discount repeated pairings
//! via [`PresenceVerdict::pair_key`].
//!
//! ## Honest limits
//!
//! The RTT check is a *thresholding hook*, not a complete distance-bounding
//! protocol. Real relay/wormhole resistance needs a tight challenge-response
//! round-trip timing bound over the BLE / Wi-Fi link. With no dedicated ranging
//! radio (a deliberate no-radio tradeoff), this is a *software* bound — weaker
//! than hardware ranging, and plain RSSI is only a weak hint. This crate provides
//! the signed, bound, replay-checked envelope those measurements slot into.
//!
//! Where a device has a UWB (ultra-wideband) chip, [`UwbRanging`] carries a
//! hardware-timed distance measurement that *additionally* tightens the bound
//! (D-0034 point 1) — never a replacement for the RTT check above, which
//! `verify_presence` always enforces regardless. [`ranging::RangingSource`]
//! is the seam a platform shell fills in to supply it; see that module's
//! honest limit for what does and doesn't exist yet.
//!
//! ## Nonces: test fixtures vs. real use
//!
//! [`Party::nonce`] must be unpredictable in real use — generate it with
//! [`mini_crypto::random_32`], never a fixed value. This crate's own test
//! suite deliberately uses fixed byte arrays (`[1u8; 32]`, `[21; 32]`, …)
//! instead: tests need deterministic, reproducible inputs (e.g. "these two
//! nonces are equal" to exercise replay rejection on purpose), and a nonce
//! is not a secret the way a signing key is — its job is freshness, not
//! confidentiality, so a fixed value in a test fixture leaks nothing. That
//! convention is specific to tests and must never be copied into real
//! attestation-building code, where a predictable nonce defeats the replay
//! resistance it exists to provide.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod attestation;
mod error;
mod ranging;
mod verify;

pub use attestation::{
    kel_digest, AttestationFields, Party, PresenceAttestation, TransportKind, UwbRanging,
    PRESENCE_VERSION,
};
pub use error::{PresenceError, Result};
pub use ranging::{NoUwb, RangingError, RangingSource};
pub use verify::{
    verify_presence, InMemoryReplayGuard, PresenceVerdict, RangePolicy, ReplayGuard, VerifyContext,
};
