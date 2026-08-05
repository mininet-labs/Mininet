//! # mini-transport-security
//!
//! Optional endpoint authentication and secure discovery above Mininet's
//! anonymous CH1 bearer. The low-level bearer stays identity-agnostic so onion
//! and future mix hops do not become mandatory identity disclosures.
//!
//! What this crate proves:
//! - one exact CH1 transcript is signed by a currently delegated `did:mini`
//!   device for one typed purpose;
//! - one X25519 routing key and dial address are bound to a signed,
//!   self-certifying, expiring endpoint advertisement;
//! - peer selection is bounded, locally seeded, duplicate-resistant, and capped
//!   per IPv4 /24 or IPv6 /48 prefix;
//! - runtime execution refuses Mixed/Burst until the exact mix executor is
//!   independently reviewed.
//!
//! What it does not prove: humanness, independent network ownership, global KEL
//! freshness on first contact, traffic-analysis resistance, or Sphinx/Loopix
//! anonymity. Those floors remain explicit rather than converted into claims.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod advertisement;
mod auth;
mod codec;
mod error;
mod gate;
mod replay;
mod runtime;
mod selection;

pub use advertisement::{
    PeerAdvertisement, SecurePexResponse, VerifiedPeerAdvertisement, MAX_PEER_ADVERTISEMENT_BYTES,
    MAX_PEER_ADVERTISEMENT_LIFETIME_MS, MAX_SECURE_PEX_BYTES, MAX_SECURE_PEX_RECORDS,
    PEER_ADVERTISEMENT_VERSION,
};
pub use auth::{
    AuthenticatedPeer, SessionAuthClaim, SessionRole, TransportEndpointId, TransportPurpose,
    MAX_SESSION_AUTH_BYTES, MAX_SESSION_AUTH_LIFETIME_MS, SESSION_AUTH_VERSION,
};
pub use error::{Result, TransportSecurityError};
pub use gate::{executable_transport, ExecutableTransport};
pub use replay::{ReplayCache, MAX_REPLAY_CACHE_ENTRIES};
pub use runtime::{
    authenticate_established_initiator, authenticate_established_responder,
    build_verified_onion_route, connect_authenticated_tcp, connect_first_authenticated_tcp,
    AuthenticatedConnection, AuthenticatedDialTarget, LocalSessionIdentity, PeerExpectation,
    VerifiedRelay, SESSION_AUTH_FRAME_AAD,
};
pub use selection::{
    diverse_dial_plan, DialAttempt, PeerSelectionPolicy, MAX_DIAL_TIMEOUT_MS, MAX_SELECTED_PEERS,
    MAX_SELECTION_CANDIDATES, MIN_DIAL_TIMEOUT_MS,
};
