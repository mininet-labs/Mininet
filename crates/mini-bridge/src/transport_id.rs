//! [`TransportId`]: a closed, wire-stable naming of the entry-transport
//! kinds this workspace knows about (MN-207 research report §"technologies
//! considered" and recommended implementation phases). Naming a transport
//! here is not a claim that it is implemented — see
//! `docs/design/bridge-pluggable-transport.md` for what's real today
//! versus what's a reserved name for a later adapter.

use crate::error::{BridgeError, Result};

/// A wire-stable identifier for one entry-transport kind.
///
/// `#[non_exhaustive]`: new transports are added by extending this enum in
/// a later decision, never by repurposing an existing tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TransportId {
    /// Direct connection to a bridge's own address, secured by this
    /// workspace's existing `mini-bearer` `Channel` (X25519, HKDF-SHA256,
    /// ChaCha20-Poly1305). The `TlsV1` name is a wire-tag label, not a
    /// claim of real TLS. See `direct.rs`'s module docs.
    DirectTlsV1,
    /// Reserved for a future direct QUIC-based transport. Not implemented.
    DirectQuicV1,
    /// obfs4 (Lyrebird) pluggable transport. Not implemented — requires
    /// an audited external implementation (research report, Phase 3).
    Obfs4V1,
    /// WebTunnel pluggable transport. Not implemented (research report,
    /// Phase 4).
    WebTunnelV1,
    /// Snowflake pluggable transport (WebRTC + broker). Not implemented
    /// (research report, Phase 5).
    SnowflakeV1,
    /// Transport over the Tor network via a local Tor client. Not
    /// implemented (research report, Phase 2 PT-compatibility track).
    TorStreamV1,
    /// Transport over I2P. Not implemented — reserved name only.
    I2pStreamV1,
    /// Local Bluetooth Low Energy forwarding bridge (no ISP/carrier path
    /// at all). Not implemented (research report, Phase 6).
    LocalBleV1,
    /// Local Wi-Fi/hotspot forwarding bridge. Not implemented (research
    /// report, Phase 6).
    LocalWifiV1,
}

impl TransportId {
    /// Stable single-byte wire tag.
    pub const fn tag(self) -> u8 {
        match self {
            TransportId::DirectTlsV1 => 1,
            TransportId::DirectQuicV1 => 2,
            TransportId::Obfs4V1 => 3,
            TransportId::WebTunnelV1 => 4,
            TransportId::SnowflakeV1 => 5,
            TransportId::TorStreamV1 => 6,
            TransportId::I2pStreamV1 => 7,
            TransportId::LocalBleV1 => 8,
            TransportId::LocalWifiV1 => 9,
        }
    }

    /// Parse a transport id from a wire tag.
    pub fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            1 => Ok(TransportId::DirectTlsV1),
            2 => Ok(TransportId::DirectQuicV1),
            3 => Ok(TransportId::Obfs4V1),
            4 => Ok(TransportId::WebTunnelV1),
            5 => Ok(TransportId::SnowflakeV1),
            6 => Ok(TransportId::TorStreamV1),
            7 => Ok(TransportId::I2pStreamV1),
            8 => Ok(TransportId::LocalBleV1),
            9 => Ok(TransportId::LocalWifiV1),
            other => {
                let _ = other;
                Err(BridgeError::BadTransportId)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [TransportId; 9] = [
        TransportId::DirectTlsV1,
        TransportId::DirectQuicV1,
        TransportId::Obfs4V1,
        TransportId::WebTunnelV1,
        TransportId::SnowflakeV1,
        TransportId::TorStreamV1,
        TransportId::I2pStreamV1,
        TransportId::LocalBleV1,
        TransportId::LocalWifiV1,
    ];

    #[test]
    fn every_transport_id_round_trips_through_its_tag() {
        for id in ALL {
            assert_eq!(TransportId::from_tag(id.tag()).unwrap(), id);
        }
    }

    #[test]
    fn an_unrecognized_tag_is_rejected() {
        assert_eq!(TransportId::from_tag(0), Err(BridgeError::BadTransportId));
        assert_eq!(TransportId::from_tag(200), Err(BridgeError::BadTransportId));
    }
}
