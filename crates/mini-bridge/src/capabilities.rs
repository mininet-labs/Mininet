//! [`TransportCapabilities`]: declared policy facts about a transport,
//! separate from whether the transport is implemented. This lets policy
//! code (and, eventually, `mini-transport-policy`) reason about what a
//! transport *claims* to buy before anything dials out — MN-207 research
//! report's "transport selection policy" and "traffic-shape policy"
//! sections.
//!
//! These are **declared** facts about a design, not measured ones. A
//! transport's actual real-world probe resistance depends on deployment
//! and the current adversary — see the design doc's honesty section.

use crate::transport_id::TransportId;

/// How resistant a transport's live traffic is to active probing by a
/// censor that already knows (or guesses) the server address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ProbeResistance {
    /// No defense — an active prober that dials the address learns it
    /// speaks this protocol.
    None,
    /// Resistance depends on an unpublished/secret value (an unlisted
    /// bridge address, a shared secret) rather than any cryptographic
    /// property of the wire format itself.
    Secret,
    /// The wire format itself is designed to be indistinguishable from
    /// some cover protocol to a prober without the shared secret (e.g.
    /// obfs4's approach).
    Cryptographic,
}

/// How often/whether a transport's network-visible endpoint changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum AddressAgility {
    /// The endpoint is long-lived and does not rotate on its own.
    Static,
    /// The endpoint is expected to rotate on some cadence (e.g. bridge
    /// distribution policy issuing fresh descriptors).
    Rotating,
    /// A fresh endpoint is used essentially per-session (e.g. Snowflake's
    /// rendezvous-assigned proxies).
    Ephemeral,
}

/// A coarse relative overhead class versus a bare direct connection —
/// intentionally not a measured number, since real overhead depends on
/// deployment (research report's "measurement" section is the place for
/// real numbers later).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum CostClass {
    Low,
    Medium,
    High,
}

/// Declared policy facts about one [`TransportId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportCapabilities {
    /// Whether the transport carries an ordered byte stream.
    pub stream: bool,
    /// Whether the transport carries unordered datagrams.
    pub datagram: bool,
    pub active_probe_resistance: ProbeResistance,
    pub address_agility: AddressAgility,
    /// Whether the transport requires a registered domain name it rides
    /// on top of (e.g. domain fronting, WebTunnel).
    pub requires_domain: bool,
    /// Whether the transport requires a third-party broker/rendezvous
    /// service to establish a connection (e.g. Snowflake).
    pub requires_broker: bool,
    /// Whether the transport can operate with no wide-area network path
    /// at all (pure local radio).
    pub supports_local_only: bool,
    pub expected_overhead: CostClass,
}

/// The fixed capability table for every known [`TransportId`] — declared
/// facts, not measurements. Reserved (unimplemented) transports still get
/// an honest capability row, since policy code needs to reason about them
/// even before an adapter exists.
pub fn capabilities_for(id: TransportId) -> TransportCapabilities {
    match id {
        TransportId::DirectTlsV1 => TransportCapabilities {
            stream: true,
            datagram: false,
            active_probe_resistance: ProbeResistance::Secret,
            address_agility: AddressAgility::Static,
            requires_domain: false,
            requires_broker: false,
            supports_local_only: false,
            expected_overhead: CostClass::Low,
        },
        TransportId::DirectQuicV1 => TransportCapabilities {
            stream: true,
            datagram: true,
            active_probe_resistance: ProbeResistance::Secret,
            address_agility: AddressAgility::Static,
            requires_domain: false,
            requires_broker: false,
            supports_local_only: false,
            expected_overhead: CostClass::Low,
        },
        TransportId::Obfs4V1 => TransportCapabilities {
            stream: true,
            datagram: false,
            active_probe_resistance: ProbeResistance::Cryptographic,
            address_agility: AddressAgility::Rotating,
            requires_domain: false,
            requires_broker: false,
            supports_local_only: false,
            expected_overhead: CostClass::Medium,
        },
        TransportId::WebTunnelV1 => TransportCapabilities {
            stream: true,
            datagram: false,
            active_probe_resistance: ProbeResistance::Cryptographic,
            address_agility: AddressAgility::Static,
            requires_domain: true,
            requires_broker: false,
            supports_local_only: false,
            expected_overhead: CostClass::Medium,
        },
        TransportId::SnowflakeV1 => TransportCapabilities {
            stream: true,
            datagram: true,
            active_probe_resistance: ProbeResistance::Cryptographic,
            address_agility: AddressAgility::Ephemeral,
            requires_domain: false,
            requires_broker: true,
            supports_local_only: false,
            expected_overhead: CostClass::High,
        },
        TransportId::TorStreamV1 => TransportCapabilities {
            stream: true,
            datagram: false,
            active_probe_resistance: ProbeResistance::None,
            address_agility: AddressAgility::Static,
            requires_domain: false,
            requires_broker: false,
            supports_local_only: false,
            expected_overhead: CostClass::High,
        },
        TransportId::I2pStreamV1 => TransportCapabilities {
            stream: true,
            datagram: false,
            active_probe_resistance: ProbeResistance::None,
            address_agility: AddressAgility::Static,
            requires_domain: false,
            requires_broker: false,
            supports_local_only: false,
            expected_overhead: CostClass::High,
        },
        TransportId::LocalBleV1 => TransportCapabilities {
            stream: true,
            datagram: false,
            active_probe_resistance: ProbeResistance::None,
            address_agility: AddressAgility::Rotating,
            requires_domain: false,
            requires_broker: false,
            supports_local_only: true,
            expected_overhead: CostClass::Low,
        },
        TransportId::LocalWifiV1 => TransportCapabilities {
            stream: true,
            datagram: true,
            active_probe_resistance: ProbeResistance::None,
            address_agility: AddressAgility::Rotating,
            requires_domain: false,
            requires_broker: false,
            supports_local_only: true,
            expected_overhead: CostClass::Low,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_tls_v1_is_a_real_stream_transport_with_no_broker_or_domain() {
        let caps = capabilities_for(TransportId::DirectTlsV1);
        assert!(caps.stream);
        assert!(!caps.requires_domain);
        assert!(!caps.requires_broker);
    }

    #[test]
    fn snowflake_declares_a_broker_and_ephemeral_addressing() {
        let caps = capabilities_for(TransportId::SnowflakeV1);
        assert!(caps.requires_broker);
        assert_eq!(caps.address_agility, AddressAgility::Ephemeral);
    }

    #[test]
    fn local_transports_declare_local_only_support() {
        assert!(capabilities_for(TransportId::LocalBleV1).supports_local_only);
        assert!(capabilities_for(TransportId::LocalWifiV1).supports_local_only);
        assert!(!capabilities_for(TransportId::DirectTlsV1).supports_local_only);
    }

    #[test]
    fn probe_resistance_is_orderable_for_policy_comparisons() {
        assert!(ProbeResistance::None < ProbeResistance::Secret);
        assert!(ProbeResistance::Secret < ProbeResistance::Cryptographic);
    }
}
