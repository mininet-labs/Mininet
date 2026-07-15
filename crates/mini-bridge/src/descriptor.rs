//! [`BridgeDescriptor`]: a self-signed, one-party reachability claim —
//! "this bridge identity claims it can be reached at this endpoint over
//! this transport, until this time." Deliberately **not** the two-party
//! `MailboxGrant`/`CapabilityGrant` pattern used elsewhere in this
//! workspace: nobody else needs to countersign a bridge's own claim about
//! its own reachability, and requiring a second signer would just add a
//! coordination dependency the research report's bridge-distribution
//! model doesn't need. Consumers decide whether to trust a given
//! descriptor by how they obtained it (out-of-band channel, invitation,
//! a trusted distributor) — this type only proves the descriptor wasn't
//! forged or tampered with in transit.
//!
//! `expires_at_ms` is deliberately `u64`, not `Option<u64>` — MN-207's
//! research report calls for bridges to be "short-lived where practical."
//! Making expiry non-optional at the type level means no caller can
//! construct an unexpiring bridge descriptor by accident.

use did_mini::{Controller, Did, IndexedSig, Kel};

use crate::codec::{Reader, Writer};
use crate::error::{BridgeError, Result};
use crate::transport_id::TransportId;

/// This module's descriptor format version.
pub const DESCRIPTOR_VERSION: u8 = 1;

const SIGNING_DOMAIN: &[u8] = b"mininet/mini-bridge/bridge-descriptor/v1";

const MAX_DID_BYTES: usize = 256;
const MAX_SIGNATURES: usize = 16;
const MAX_SIG_BYTES: usize = 256;

/// An opaque, transport-specific dial target — e.g. an IP:port pair for
/// [`TransportId::DirectTlsV1`], or a bridge line for a future obfs4
/// adapter. This crate never interprets the bytes; only the matching
/// [`crate::PluggableTransport`] implementation does.
pub const MAX_ENDPOINT_BYTES: usize = 512;

/// Opaque, transport-specific parameters (e.g. obfs4's `cert=`/`iat-mode=`
/// arguments). Never interpreted by this crate.
pub const MAX_TRANSPORT_PARAMETERS_BYTES: usize = 1024;

/// An optional opaque scope naming which distribution channel handed out
/// this descriptor (e.g. "invite:alice", "public-mirror") — for the
/// consuming application's own abuse-control bookkeeping, never
/// interpreted here.
pub const MAX_DISTRIBUTOR_SCOPE_BYTES: usize = 128;

/// Opaque, transport-specific dial-target bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueEndpoint(Vec<u8>);

impl OpaqueEndpoint {
    pub fn new(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() > MAX_ENDPOINT_BYTES {
            return Err(BridgeError::LimitExceeded);
        }
        Ok(OpaqueEndpoint(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Opaque, transport-specific parameter bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportParameters(Vec<u8>);

impl TransportParameters {
    pub fn new(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() > MAX_TRANSPORT_PARAMETERS_BYTES {
            return Err(BridgeError::LimitExceeded);
        }
        Ok(TransportParameters(bytes))
    }

    pub fn empty() -> Self {
        TransportParameters(Vec::new())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// An opaque scope naming which distribution channel handed out a
/// descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributorScope(Vec<u8>);

impl DistributorScope {
    pub fn new(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() > MAX_DISTRIBUTOR_SCOPE_BYTES {
            return Err(BridgeError::LimitExceeded);
        }
        Ok(DistributorScope(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// A self-signed reachability claim for one bridge identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeDescriptor {
    pub bridge: Did,
    pub transport: TransportId,
    pub endpoint: OpaqueEndpoint,
    pub transport_parameters: TransportParameters,
    pub distributor_scope: Option<DistributorScope>,
    pub valid_from_ms: u64,
    pub expires_at_ms: u64,
    nonce: [u8; 16],
    signature: Vec<IndexedSig>,
}

impl BridgeDescriptor {
    /// Issue and sign a bridge descriptor. `expires_at_ms` must be
    /// strictly greater than `valid_from_ms` — see
    /// [`BridgeError::NotYetValid`]/[`BridgeError::Expired`] for what an
    /// inverted window produces at verification time (always rejected,
    /// never a footgun that silently "works").
    pub fn issue(
        bridge: &Controller,
        transport: TransportId,
        endpoint: OpaqueEndpoint,
        transport_parameters: TransportParameters,
        distributor_scope: Option<DistributorScope>,
        valid_from_ms: u64,
        expires_at_ms: u64,
    ) -> Result<Self> {
        let nonce = mini_crypto::random_32().map_err(BridgeError::Crypto)?;
        let nonce: [u8; 16] = nonce[..16]
            .try_into()
            .expect("first 16 bytes of a 32-byte array always convert");
        let mut descriptor = BridgeDescriptor {
            bridge: bridge.did(),
            transport,
            endpoint,
            transport_parameters,
            distributor_scope,
            valid_from_ms,
            expires_at_ms,
            nonce,
            signature: Vec::new(),
        };
        descriptor.signature = bridge.sign_message(&descriptor.signing_bytes());
        Ok(descriptor)
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(SIGNING_DOMAIN);
        w.u8(DESCRIPTOR_VERSION);
        w.bytes(self.bridge.as_str().as_bytes());
        w.u8(self.transport.tag());
        w.bytes(self.endpoint.as_bytes());
        w.bytes(self.transport_parameters.as_bytes());
        w.u8(self.distributor_scope.is_some() as u8);
        w.bytes(
            self.distributor_scope
                .as_ref()
                .map(DistributorScope::as_bytes)
                .unwrap_or(&[]),
        );
        w.u64(self.valid_from_ms);
        w.u64(self.expires_at_ms);
        w.raw(&self.nonce);
        w.into_bytes()
    }

    /// Verify the issuer's signature and the descriptor's validity window
    /// against `now_ms`. Does not dial anything.
    pub fn verify(&self, bridge_kel: &Kel, now_ms: u64) -> Result<()> {
        if bridge_kel.did().as_str() != self.bridge.as_str() {
            return Err(BridgeError::BadSignature);
        }
        bridge_kel
            .verify_message(&self.signing_bytes(), &self.signature)
            .map_err(|_| BridgeError::BadSignature)?;
        if now_ms < self.valid_from_ms {
            return Err(BridgeError::NotYetValid);
        }
        if now_ms >= self.expires_at_ms {
            return Err(BridgeError::Expired);
        }
        Ok(())
    }

    /// Canonical wire bytes (fields + signature).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(DESCRIPTOR_VERSION);
        w.bytes(self.bridge.as_str().as_bytes());
        w.u8(self.transport.tag());
        w.bytes(self.endpoint.as_bytes());
        w.bytes(self.transport_parameters.as_bytes());
        w.u8(self.distributor_scope.is_some() as u8);
        w.bytes(
            self.distributor_scope
                .as_ref()
                .map(DistributorScope::as_bytes)
                .unwrap_or(&[]),
        );
        w.u64(self.valid_from_ms);
        w.u64(self.expires_at_ms);
        w.raw(&self.nonce);
        w.u32(self.signature.len() as u32);
        for s in &self.signature {
            w.u32(s.index);
            w.u8(s.signature.suite().tag());
            w.bytes(&s.signature.to_bytes());
        }
        w.into_bytes()
    }

    /// Decode a descriptor from untrusted bytes. Rejects an unrecognized
    /// [`DESCRIPTOR_VERSION`] and trailing bytes. Does **not** verify the
    /// signature — call [`BridgeDescriptor::verify`] with the bridge's KEL
    /// for that.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        if r.u8()? != DESCRIPTOR_VERSION {
            return Err(BridgeError::UnsupportedDescriptorVersion);
        }
        let bridge = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;
        let transport = TransportId::from_tag(r.u8()?)?;
        let endpoint = OpaqueEndpoint::new(r.bytes_limited(MAX_ENDPOINT_BYTES)?)?;
        let transport_parameters =
            TransportParameters::new(r.bytes_limited(MAX_TRANSPORT_PARAMETERS_BYTES)?)?;
        let has_distributor_scope = r.u8()? != 0;
        let distributor_scope_bytes = r.bytes_limited(MAX_DISTRIBUTOR_SCOPE_BYTES)?;
        let distributor_scope = if has_distributor_scope {
            Some(DistributorScope::new(distributor_scope_bytes)?)
        } else {
            None
        };
        let valid_from_ms = r.u64()?;
        let expires_at_ms = r.u64()?;
        let nonce: [u8; 16] = r
            .raw(16)?
            .try_into()
            .expect("Reader::raw(16) always returns exactly 16 bytes");
        let nsigs = r.u32()? as usize;
        if nsigs > MAX_SIGNATURES {
            return Err(BridgeError::LimitExceeded);
        }
        let mut signature = Vec::with_capacity(nsigs);
        for _ in 0..nsigs {
            let index = r.u32()?;
            let sig_suite =
                mini_crypto::SignatureSuite::from_tag(r.u8()?).map_err(BridgeError::Crypto)?;
            let sig_bytes = r.bytes_limited(MAX_SIG_BYTES)?;
            let sig = mini_crypto::Signature::from_suite_bytes(sig_suite, &sig_bytes)
                .map_err(BridgeError::Crypto)?;
            signature.push(IndexedSig {
                index,
                signature: sig,
            });
        }
        if !r.finished() {
            return Err(BridgeError::TrailingBytes);
        }
        Ok(BridgeDescriptor {
            bridge,
            transport,
            endpoint,
            transport_parameters,
            distributor_scope,
            valid_from_ms,
            expires_at_ms,
            nonce,
            signature,
        })
    }
}

fn parse_did(bytes: Vec<u8>) -> Result<Did> {
    let s = String::from_utf8(bytes).map_err(|_| BridgeError::TrailingBytes)?;
    Ok(Did::parse(&s)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_descriptor(bridge: &Controller) -> BridgeDescriptor {
        BridgeDescriptor::issue(
            bridge,
            TransportId::DirectTlsV1,
            OpaqueEndpoint::new(b"127.0.0.1:9999".to_vec()).unwrap(),
            TransportParameters::empty(),
            None,
            1_000,
            2_000,
        )
        .unwrap()
    }

    #[test]
    fn a_descriptor_verifies_against_its_own_issuer_kel_within_its_window() {
        let bridge = Controller::incept_single().unwrap();
        let descriptor = sample_descriptor(&bridge);
        descriptor.verify(&bridge.kel(), 1_500).unwrap();
    }

    #[test]
    fn a_descriptor_is_rejected_before_its_valid_from_time() {
        let bridge = Controller::incept_single().unwrap();
        let descriptor = sample_descriptor(&bridge);
        assert_eq!(
            descriptor.verify(&bridge.kel(), 500),
            Err(BridgeError::NotYetValid)
        );
    }

    #[test]
    fn a_descriptor_is_rejected_at_or_after_its_expiry() {
        let bridge = Controller::incept_single().unwrap();
        let descriptor = sample_descriptor(&bridge);
        assert_eq!(
            descriptor.verify(&bridge.kel(), 2_000),
            Err(BridgeError::Expired)
        );
        assert_eq!(
            descriptor.verify(&bridge.kel(), 5_000),
            Err(BridgeError::Expired)
        );
    }

    #[test]
    fn a_descriptor_signed_by_one_bridge_does_not_verify_under_another_bridges_kel() {
        let bridge = Controller::incept_single().unwrap();
        let other = Controller::incept_single().unwrap();
        let descriptor = sample_descriptor(&bridge);
        assert_eq!(
            descriptor.verify(&other.kel(), 1_500),
            Err(BridgeError::BadSignature)
        );
    }

    #[test]
    fn tampering_with_the_endpoint_after_signing_breaks_verification() {
        let bridge = Controller::incept_single().unwrap();
        let mut descriptor = sample_descriptor(&bridge);
        descriptor.endpoint = OpaqueEndpoint::new(b"10.0.0.1:1".to_vec()).unwrap();
        assert_eq!(
            descriptor.verify(&bridge.kel(), 1_500),
            Err(BridgeError::BadSignature)
        );
    }

    #[test]
    fn tampering_with_the_expiry_after_signing_breaks_verification() {
        let bridge = Controller::incept_single().unwrap();
        let mut descriptor = sample_descriptor(&bridge);
        descriptor.expires_at_ms = 9_999_999;
        assert_eq!(
            descriptor.verify(&bridge.kel(), 1_500),
            Err(BridgeError::BadSignature)
        );
    }

    #[test]
    fn a_descriptor_round_trips_through_bytes() {
        let bridge = Controller::incept_single().unwrap();
        let descriptor = sample_descriptor(&bridge);
        let bytes = descriptor.to_bytes();
        let decoded = BridgeDescriptor::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, descriptor);
        decoded.verify(&bridge.kel(), 1_500).unwrap();
    }

    #[test]
    fn an_unsupported_version_is_rejected() {
        let bridge = Controller::incept_single().unwrap();
        let descriptor = sample_descriptor(&bridge);
        let mut bytes = descriptor.to_bytes();
        bytes[0] = 99;
        assert_eq!(
            BridgeDescriptor::from_bytes(&bytes),
            Err(BridgeError::UnsupportedDescriptorVersion)
        );
    }

    #[test]
    fn trailing_bytes_after_a_complete_decode_are_rejected() {
        let bridge = Controller::incept_single().unwrap();
        let descriptor = sample_descriptor(&bridge);
        let mut bytes = descriptor.to_bytes();
        bytes.push(0xFF);
        assert_eq!(
            BridgeDescriptor::from_bytes(&bytes),
            Err(BridgeError::TrailingBytes)
        );
    }

    #[test]
    fn an_oversized_endpoint_is_rejected_before_signing() {
        let oversized = vec![0u8; MAX_ENDPOINT_BYTES + 1];
        assert_eq!(
            OpaqueEndpoint::new(oversized),
            Err(BridgeError::LimitExceeded)
        );
    }

    #[test]
    fn an_oversized_transport_parameters_field_is_rejected() {
        let oversized = vec![0u8; MAX_TRANSPORT_PARAMETERS_BYTES + 1];
        assert_eq!(
            TransportParameters::new(oversized),
            Err(BridgeError::LimitExceeded)
        );
    }

    #[test]
    fn a_distributor_scope_round_trips_when_present() {
        let bridge = Controller::incept_single().unwrap();
        let descriptor = BridgeDescriptor::issue(
            &bridge,
            TransportId::DirectTlsV1,
            OpaqueEndpoint::new(b"127.0.0.1:9999".to_vec()).unwrap(),
            TransportParameters::empty(),
            Some(DistributorScope::new(b"invite:alice".to_vec()).unwrap()),
            1_000,
            2_000,
        )
        .unwrap();
        let decoded = BridgeDescriptor::from_bytes(&descriptor.to_bytes()).unwrap();
        assert_eq!(
            decoded.distributor_scope.unwrap().as_bytes(),
            b"invite:alice"
        );
    }
}
