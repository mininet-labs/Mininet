//! [`DirectBridgeTransport`]: the one real, Phase-2 [`PluggableTransport`]
//! implementation in this crate. Dials a real TCP socket and performs a
//! genuine `mini_bearer::Channel` handshake.
//!
//! ## Honest naming
//!
//! [`TransportId::DirectTlsV1`]'s `Tls` in its name is a wire-tag label
//! carried over from the research report's transport taxonomy, **not** a
//! claim that this implementation speaks real TLS. It composes this
//! workspace's existing, already-reviewed `mini-bearer` channel (X25519 +
//! HKDF-SHA256 + ChaCha20-Poly1305) instead of adding a new TLS dependency
//! — consistent with CLAUDE.md's no-new-cryptography rule. A real
//! TLS-mimicking transport (or [`crate::TransportId::DirectQuicV1`]) is
//! future work, not a claim this module makes.
//!
//! ## Verify-before-dial
//!
//! [`DirectBridgeTransport::connect`] checks the descriptor's transport
//! match, signature, and validity window **before** any socket is
//! touched — an unverifiable or expired descriptor never causes a single
//! packet to leave the machine.
//!
//! ## Why `Channel = (TcpBearer, mini_bearer::Channel)`
//!
//! `mini_bearer::Channel` is a pure crypto object (seal/open) with no
//! socket of its own — the socket lives in a separate [`TcpBearer`], the
//! same split `mini-relay`'s live TCP demo uses. A caller needs both to
//! actually exchange further frames after connecting, so this transport's
//! associated `Channel` type is the pair.

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use did_mini::Kel;
use mini_bearer::{Bearer, Channel, Initiator, TcpBearer};

use crate::capabilities::{capabilities_for, TransportCapabilities};
use crate::descriptor::BridgeDescriptor;
use crate::error::{BridgeError, Result};
use crate::transport::PluggableTransport;
use crate::transport_id::TransportId;

/// Dials [`TransportId::DirectTlsV1`] bridge descriptors over a real TCP
/// socket, then performs a real `mini_bearer::Channel` handshake.
#[derive(Debug, Default, Clone, Copy)]
pub struct DirectBridgeTransport;

impl PluggableTransport for DirectBridgeTransport {
    type Channel = (TcpBearer, Channel);
    type Error = BridgeError;

    fn transport_id(&self) -> TransportId {
        TransportId::DirectTlsV1
    }

    fn connect(
        &self,
        bridge: &BridgeDescriptor,
        bridge_kel: &Kel,
        now_ms: u64,
        deadline_ms: u64,
    ) -> Result<(TcpBearer, Channel)> {
        if bridge.transport != TransportId::DirectTlsV1 {
            return Err(BridgeError::TransportMismatch);
        }
        // Verification happens strictly before the network is touched.
        bridge.verify(bridge_kel, now_ms)?;

        let endpoint_str = core::str::from_utf8(bridge.endpoint.as_bytes())
            .map_err(|_| BridgeError::BadEndpoint)?;
        let addr: SocketAddr = endpoint_str.parse().map_err(|_| BridgeError::BadEndpoint)?;

        let timeout_ms = deadline_ms.saturating_sub(now_ms).max(1);
        let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms))
            .map_err(|e| BridgeError::Bearer(e.into()))?;
        let mut tcp_bearer = TcpBearer::from_stream(stream).map_err(BridgeError::Bearer)?;

        let (initiator, hello) = Initiator::start().map_err(BridgeError::Bearer)?;
        tcp_bearer.send(&hello).map_err(BridgeError::Bearer)?;
        let response = tcp_bearer.recv().map_err(BridgeError::Bearer)?;
        let channel = initiator.finish(&response).map_err(BridgeError::Bearer)?;
        Ok((tcp_bearer, channel))
    }

    fn capabilities(&self) -> TransportCapabilities {
        capabilities_for(TransportId::DirectTlsV1)
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::thread;

    use did_mini::Controller;
    use mini_bearer::Responder;

    use super::*;
    use crate::descriptor::{OpaqueEndpoint, TransportParameters};

    fn listening_bridge() -> (Controller, TcpListener, SocketAddr) {
        let bridge = Controller::incept_single().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        (bridge, listener, addr)
    }

    #[test]
    fn connect_succeeds_and_a_sealed_message_round_trips_over_the_real_socket() {
        let (bridge, listener, addr) = listening_bridge();
        let descriptor = BridgeDescriptor::issue(
            &bridge,
            TransportId::DirectTlsV1,
            OpaqueEndpoint::new(addr.to_string().into_bytes()).unwrap(),
            TransportParameters::empty(),
            None,
            0,
            60_000,
        )
        .unwrap();

        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut server_bearer = TcpBearer::from_stream(stream).unwrap();
            let hello = server_bearer.recv().unwrap();
            let (mut channel, response) = Responder::respond(&hello).unwrap();
            server_bearer.send(&response).unwrap();
            let frame = server_bearer.recv().unwrap();
            channel.open(&frame, b"").unwrap()
        });

        let transport = DirectBridgeTransport;
        let (mut bearer, mut channel) = transport
            .connect(&descriptor, &bridge.kel(), 1_000, 5_000)
            .unwrap();

        let sealed = channel.seal(b"hello bridge", b"").unwrap();
        bearer.send(&sealed).unwrap();

        let received_plaintext = server.join().unwrap();
        assert_eq!(received_plaintext, b"hello bridge");
    }

    #[test]
    fn a_transport_mismatched_descriptor_is_rejected_before_dialing() {
        let bridge = Controller::incept_single().unwrap();
        let descriptor = BridgeDescriptor::issue(
            &bridge,
            TransportId::Obfs4V1,
            OpaqueEndpoint::new(b"127.0.0.1:1".to_vec()).unwrap(),
            TransportParameters::empty(),
            None,
            0,
            60_000,
        )
        .unwrap();
        let transport = DirectBridgeTransport;
        assert_eq!(
            transport
                .connect(&descriptor, &bridge.kel(), 1_000, 5_000)
                .unwrap_err(),
            BridgeError::TransportMismatch
        );
    }

    #[test]
    fn an_unverifiable_descriptor_is_rejected_before_dialing() {
        let bridge = Controller::incept_single().unwrap();
        let other = Controller::incept_single().unwrap();
        let descriptor = BridgeDescriptor::issue(
            &bridge,
            TransportId::DirectTlsV1,
            OpaqueEndpoint::new(b"127.0.0.1:1".to_vec()).unwrap(),
            TransportParameters::empty(),
            None,
            0,
            60_000,
        )
        .unwrap();
        let transport = DirectBridgeTransport;
        assert_eq!(
            transport
                .connect(&descriptor, &other.kel(), 1_000, 5_000)
                .unwrap_err(),
            BridgeError::BadSignature
        );
    }

    #[test]
    fn an_expired_descriptor_is_rejected_before_dialing() {
        let bridge = Controller::incept_single().unwrap();
        let descriptor = BridgeDescriptor::issue(
            &bridge,
            TransportId::DirectTlsV1,
            OpaqueEndpoint::new(b"127.0.0.1:1".to_vec()).unwrap(),
            TransportParameters::empty(),
            None,
            0,
            1_000,
        )
        .unwrap();
        let transport = DirectBridgeTransport;
        assert_eq!(
            transport
                .connect(&descriptor, &bridge.kel(), 2_000, 5_000)
                .unwrap_err(),
            BridgeError::Expired
        );
    }

    #[test]
    fn a_malformed_endpoint_is_rejected_as_a_bad_endpoint() {
        let bridge = Controller::incept_single().unwrap();
        let descriptor = BridgeDescriptor::issue(
            &bridge,
            TransportId::DirectTlsV1,
            OpaqueEndpoint::new(b"not-a-socket-address".to_vec()).unwrap(),
            TransportParameters::empty(),
            None,
            0,
            60_000,
        )
        .unwrap();
        let transport = DirectBridgeTransport;
        assert_eq!(
            transport
                .connect(&descriptor, &bridge.kel(), 1_000, 5_000)
                .unwrap_err(),
            BridgeError::BadEndpoint
        );
    }
}
