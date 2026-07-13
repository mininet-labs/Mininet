//! Mininet bearer layer: an identity-agnostic transport plus an anonymous,
//! forward-secret encrypted channel.
//!
//! ## Two layers, deliberately separated
//!
//! - **[`Bearer`]** is a dumb, identity-free pipe: it moves opaque frames between
//!   two endpoints and knows nothing about who is talking. BLE, local Wi-Fi/hotspot,
//!   and an internet relay are all just bearers (this crate ships an in-process
//!   bearer, a real [`TcpBearer`], and — for finding a peer's address in the
//!   first place on a shared local network — [`discovery::LocalAnnouncer`]/
//!   [`discovery::LocalScanner`] over UDP multicast; platform bearers bind
//!   behind the same trait). Anonymity starts here — the transport carries
//!   no identity, and neither does discovery.
//!
//! - **[`Channel`]** is an encrypted session over any bearer. It performs an
//!   ephemeral X25519 handshake ([`Initiator`] / [`Responder`]), derives
//!   ChaCha20-Poly1305 traffic keys via HKDF-SHA256, and gives a confidential,
//!   forward-secret duplex. **No identities appear in the handshake**, so the
//!   connection is anonymous and unlinkable, and a passive observer learns nothing
//!   but ephemeral public keys.
//!
//! ## Security model — anonymous connection, valid payload
//!
//! The channel intentionally provides confidentiality + forward secrecy + a
//! **channel-binding** value, but *not* endpoint authentication. That is the point:
//! "anonymous connection, valid transaction" (constitution P5). Authenticity lives
//! in the payload, not the pipe:
//!
//! - `did:mini` KELs are self-certifying and signed — a man-in-the-middle cannot
//!   forge one (SPEC-01).
//! - Genesis / release chunks are content-addressed — the hash validates the bytes.
//! - Presence attestations will sign a transcript that includes
//!   [`Channel::channel_binding`], both nonces, and the range challenge. The
//!   binding prevents channel-transcript substitution, but it is not sufficient
//!   by itself; anti-relay comes from the whole presence protocol and its
//!   round-trip distance bound.
//!
//! So the pipe reveals only "some anonymous peer," while each payload proves its own
//! validity. Endpoint pseudonym authentication (a SIGMA/Noise-XX upgrade keyed by a
//! per-session pairwise pseudonym) can layer on later without changing this crate's
//! shape.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod bearer;
mod channel;
mod discovery;
mod error;
mod inprocess;
mod tcp;

pub use bearer::{encode_frame, Bearer, FrameReader, MAX_FRAME_BYTES, MAX_STREAM_BUFFER_BYTES};
pub use channel::{
    Channel, Initiator, Responder, MAX_CHANNEL_CIPHERTEXT_BYTES, MAX_CHANNEL_PLAINTEXT_BYTES,
    PROTOCOL_VERSION,
};
pub use discovery::{
    LocalAnnouncer, LocalScanner, DEFAULT_MULTICAST_GROUP, DEFAULT_MULTICAST_PORT,
};
pub use error::{BearerError, Result};
pub use inprocess::{pair, InProcessBearer};
pub use tcp::TcpBearer;
