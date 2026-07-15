//! MN-207 pluggable entry-transport framework (D-0309, `docs/research/
//! MN207_BRIDGE_PLUGGABLE_TRANSPORT_RESEARCH_20260714.md`).
//!
//! The research report's executive conclusion is explicit: MN-207 is not
//! "invent a Mininet obfuscation protocol." It is a small, typed
//! pluggable-transport interface that lets Mininet reach relay/rendezvous
//! services through multiple censorship-resistant entry mechanisms
//! without coupling the core network to any one disguise, plus adapters
//! to proven systems added over time.
//!
//! ## What's implemented here (Phase 0 + Phase 1 + one real Phase 2)
//!
//! - [`TransportId`]: a closed, wire-stable naming of nine transport
//!   kinds the research report identifies, `#[non_exhaustive]` so future
//!   decisions can add more.
//! - [`TransportCapabilities`]/[`capabilities_for`]: declared policy
//!   facts (probe resistance, address agility, overhead class) for every
//!   named transport — real today even for transports with no adapter
//!   yet, since policy code needs to reason about them.
//! - [`BridgeDescriptor`]: a self-signed, one-party reachability claim,
//!   with a mandatory (non-`Option`) expiry enforcing "short-lived where
//!   practical" at the type level.
//! - [`PluggableTransport`]: the synchronous adapter trait every
//!   transport implementation satisfies.
//! - [`DirectBridgeTransport`]: the one real, tested implementation —
//!   dials a real TCP socket and performs a genuine `mini_bearer::Channel`
//!   handshake. See `direct.rs`'s module docs for why `DirectTlsV1`'s name
//!   is a wire-tag label, not a claim of real TLS.
//! - [`PtProcessManager`]/[`VerifiedExecutable`]/[`PtProcessHandle`]/
//!   [`PtClientMethod`] (D-0097, `docs/research/
//!   BRIDGE_ADAPTER_INTEGRATION_RESEARCH_20260715.md`): a generic Tor
//!   Pluggable Transport v1 process manager proving the safety boundary
//!   a real circumvention adapter will later dial through — spawn a
//!   pinned, digest-verified executable with no shell and a minimal
//!   environment, parse its startup handshake, terminate cleanly. No
//!   real PT binary is a dependency of this crate; see `docs/design/
//!   external-bridge-adapter-integration.md`.
//!
//! ## What's deliberately NOT implemented
//!
//! No real Lyrebird/obfs4, WebTunnel, or Snowflake binary is integrated
//! — each needs an audited external implementation this crate would
//! compose, plus its own [`PluggableTransport`] implementation dialing
//! through [`PtProcessManager`]'s local endpoint, both future PRs (D-0097's
//! research report §24, PR3+). BLE/local-Wi-Fi bridging depends on
//! hardware this environment cannot exercise (`mini-presence`'s existing
//! honest limits apply here too). See `docs/design/
//! bridge-pluggable-transport.md` and `docs/design/
//! external-bridge-adapter-integration.md` for the full status tables.
//!
//! No new cryptography: [`BridgeDescriptor`] composes `did-mini`'s
//! existing KEL/signature machinery, and [`DirectBridgeTransport`]
//! composes `mini-bearer`'s existing `Channel` — nothing here invents a
//! primitive.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod capabilities;
mod codec;
mod descriptor;
mod direct;
mod error;
mod pt_process;
mod transport;
mod transport_id;

pub use capabilities::{
    capabilities_for, AddressAgility, CostClass, ProbeResistance, TransportCapabilities,
};
pub use descriptor::{
    BridgeDescriptor, DistributorScope, OpaqueEndpoint, TransportParameters, DESCRIPTOR_VERSION,
    MAX_DISTRIBUTOR_SCOPE_BYTES, MAX_ENDPOINT_BYTES, MAX_TRANSPORT_PARAMETERS_BYTES,
};
pub use direct::DirectBridgeTransport;
pub use error::{BridgeError, Result};
pub use pt_process::{PtClientMethod, PtProcessHandle, PtProcessManager, VerifiedExecutable};
pub use transport::PluggableTransport;
pub use transport_id::TransportId;
