//! Gate #93 remediation: production threshold-custody DKG ceremony.
//!
//! An anonymous external audit report ("Mininet External FROST DKG &
//! Custody Audit Report", Gate #93, 2026-09-11) reviewed
//! `mini_treasury`'s hand-rolled Pedersen/Feldman DKG (`frost_dkg.rs`) and
//! found two critical defects: `dkg_generate_round2_shares` evaluated a
//! participant's secret polynomial at any caller-supplied index with no
//! check (index `0` is the polynomial's constant term -- the raw secret),
//! and the complaint/rebuttal exclusion mechanism used an unvalidated
//! `accuser` field the same unsafe way. Both were independently verified
//! against the real code and patched with a minimal boundary check
//! (D-0506, `mini_treasury::frost_dkg`).
//!
//! That patch closes the acute vulnerability but not the report's actual
//! recommendation: stop maintaining a second, bespoke DKG/complaint
//! implementation at all. This crate is that remediation --
//! `frost_ristretto255::keys::dkg`'s `part1`/`part2`/`part3` (the Zcash
//! Foundation's audited implementation of the *original FROST paper's*
//! Pedersen-style DKG -- **not** RFC 9591, which explicitly places key
//! generation out of scope; `mini_treasury::frost_dkg`'s old module docs
//! misattributed it and are corrected here) does the actual cryptography.
//! `frost_ristretto255::Identifier: TryFrom<u16>` already rejects `0` at
//! the type level, closing the whole vulnerability class structurally
//! rather than by a caller remembering to check.
//!
//! What this crate adds on top, none of which `frost_ristretto255` itself
//! provides:
//!
//! - [`manifest`] — one immutable, signed session identity every ceremony
//!   message binds to (network, custody domain, epoch, roster, threshold,
//!   authorization object, prior key) -- `frost_ristretto255::keys::dkg`
//!   takes no session concept of its own.
//! - [`session`] — the Phase A-G ceremony state machine: manifest
//!   acceptance, Round-1 consistent-broadcast (a *root hash* every
//!   participant must acknowledge identically before Round 2 starts, not
//!   just point-to-point sends -- the ZF FROST Book's own explicit
//!   warning), Round 2, and unanimous (11-of-11) completion attestation
//!   before a key is ever treated as active.
//! - [`transport`] — binds `mini_bearer::Channel` (the same anonymous,
//!   forward-secret encrypted channel construction the BLE mesh relay work
//!   already exercised over both in-process and real TCP sockets, PR
//!   #333) to a specific session/sender/receiver identity via a signed
//!   channel-binding assertion, since `Channel`'s own docs are explicit
//!   that the handshake alone provides "confidentiality, integrity,
//!   forward secrecy, and unlinkability -- but *not* endpoint
//!   authentication, by design".
//! - [`abort`] — the ceremony's only failure-handling path. **No public
//!   share complaint/rebuttal mechanism exists here** (unlike the old
//!   `frost_dkg` module) -- any invalid, missing, or ambiguous input
//!   aborts and restarts the whole ceremony with fresh randomness. A
//!   treasury DKG is infrequent with a known roster; the audit report's
//!   own reasoning (R93-05) is that keeping the secret surface minimal is
//!   worth more here than the liveness a rebuttal path buys.
//! - [`share_store`] — per-signer encrypted [`crate::frost_ristretto255::keys::KeyPackage`]
//!   persistence (Argon2id-stretched wrapping key, `mini_crypto`'s AEAD),
//!   written only after unanimous completion.
//! - [`domains`]/[`rotation`] — independent group keys per custody domain,
//!   and the fresh-key-per-rotation rule (same-key resharing/refresh is
//!   explicitly rejected for production -- old shares mathematically
//!   remain valid under same-key resharing, so only a fresh key actually
//!   revokes them).
//!
//! ## Honest limits, stated plainly
//!
//! - **No network transport is implemented here.** Every ceremony/signing
//!   primitive in `mini_treasury` already carries this same "no network,
//!   no transport" honest limit (`frost_sign`/`frost_dkg`'s own module
//!   docs); this crate's [`session`] state machine is the same shape --
//!   pure, fully testable logic over caller-supplied messages, proven
//!   in-process and over real loopback TCP via `mini_bearer` exactly the
//!   way `mini-mesh` proved its own relay logic. Wiring an actual 11-node
//!   ceremony across real machines is separate, unstarted integration
//!   work.
//! - **No external chain integration.** [`rotation::CustodyKeyTransitionV1`]
//!   is the typed authorization/binding record a rotation produces; it
//!   does not itself sweep BTC UTXOs, reconcile XMR balances, or rotate
//!   XRPL signing authority. Those require real, chain-specific
//!   engineering this codebase does not yet have (the exact same honest
//!   limit `mini_treasury::receipt::NoExternalReceiptOracle` already
//!   states for the *inbound* direction).
//! - **Not externally audited.** This crate composes `frost_ristretto255`
//!   (itself independently audited by NCC Group) rather than inventing
//!   new cryptography, but the ceremony/transport/storage/rotation layer
//!   built around it is new, AI-authored code that has not been reviewed
//!   by anyone outside this project. Marking Gate #93 "closed" requires a
//!   real, accountable external audit of that layer -- an anonymous,
//!   unsigned report is not that, however technically sound its findings
//!   are (and on the one claim checked in depth here, it was).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod abort;
pub mod anchor;
pub mod domains;
pub mod manifest;
pub mod rotation;
pub mod session;
pub mod share_store;
pub mod signing;
pub mod transport;

mod error;
mod wire;

pub use error::{CustodyError, Result};
pub use manifest::{
    CustodyDomain, CustodyParticipantV1, DkgSessionManifestV1, FROST_SUITE_ID, SIGNER_COUNT,
    THRESHOLD,
};

/// Re-exported so a caller never needs `frost-ristretto255` as its own
/// direct dependency just to hold the [`frost_ristretto255::keys::KeyPackage`]/
/// [`frost_ristretto255::keys::PublicKeyPackage`] this crate's ceremony
/// produces.
pub use frost_ristretto255;
