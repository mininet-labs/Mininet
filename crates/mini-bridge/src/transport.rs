//! [`PluggableTransport`]: the interface every entry-transport adapter
//! implements. Deliberately **synchronous** — this workspace has no async
//! runtime anywhere (`mini-bearer::Bearer` is the existing sync-trait
//! precedent this mirrors), diverging from the research report's own
//! illustrative `async fn connect` pseudocode for that reason.

use did_mini::Kel;

use crate::capabilities::TransportCapabilities;
use crate::descriptor::BridgeDescriptor;
use crate::transport_id::TransportId;

/// Dial a bridge over one specific transport kind and yield a usable
/// channel of `Self::Channel`. Implementations must verify `bridge`'s
/// signature and validity window (via [`BridgeDescriptor::verify`])
/// **before** touching the network — see `direct.rs` for the reference
/// implementation.
pub trait PluggableTransport {
    /// The connected channel type this transport yields on success.
    type Channel;
    /// This transport's error type.
    type Error;

    /// Which [`TransportId`] this implementation dials.
    fn transport_id(&self) -> TransportId;

    /// Attempt to connect to `bridge` before `deadline_ms`. `now_ms` and
    /// `deadline_ms` are caller-supplied (not read from the system clock
    /// internally) so tests can drive validity-window checks
    /// deterministically.
    fn connect(
        &self,
        bridge: &BridgeDescriptor,
        bridge_kel: &Kel,
        now_ms: u64,
        deadline_ms: u64,
    ) -> Result<Self::Channel, Self::Error>;

    /// This transport's declared policy facts.
    fn capabilities(&self) -> TransportCapabilities;
}
