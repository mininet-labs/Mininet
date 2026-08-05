//! Signed, expiring, network-bound peer advertisements.
//!
//! An advertisement is a dial hint, never trust in itself. Its delegated device
//! signature binds the address and X25519 routing key to a self-certifying
//! endpoint id. A successful channel-bound authentication exchange is still
//! required after dialing.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use did_mini::{verify_delegation, Capabilities, Controller, Did, FreshnessPins, IndexedSig, Kel};
use mini_crypto::{
    AgreementPublicKey, HashAlgorithm, KeyAgreementSuite, Signature, SignatureSuite,
};

use crate::auth::{TransportEndpointId, MAX_TRANSPORT_DID_BYTES};
use crate::codec::{Reader, Writer};
use crate::{ReplayCache, Result, TransportSecurityError};

pub const PEER_ADVERTISEMENT_VERSION: u8 = 1;
pub const MAX_PEER_ADVERTISEMENT_BYTES: usize = 64 * 1024;
pub const MAX_PEER_ADVERTISEMENT_SIGNATURES: usize = 64;
pub const MAX_PEER_ADVERTISEMENT_LIFETIME_MS: u64 = 30 * 60 * 1000;
pub const MAX_PEER_ADVERTISEMENT_CLOCK_SKEW_MS: u64 = 30 * 1000;
pub const MAX_SECURE_PEX_RECORDS: usize = 64;
pub const MAX_SECURE_PEX_BYTES: usize = 4 * 1024 * 1024;

const ADVERTISEMENT_DOMAIN: &[u8] = b"mini-transport-security/peer-advertisement/v1";
const ADVERTISEMENT_REPLAY_DOMAIN: &[u8] = b"mini-transport-security/advertisement-replay/v1";
const SECURE_PEX_DOMAIN: &[u8] = b"mini-transport-security/secure-pex/v1";
const ADDR_V4: u8 = 4;
const ADDR_V6: u8 = 6;

/// A signed peer advertisement. Raw values are untrusted until [`Self::verify`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerAdvertisement {
    pub network_id: [u8; 32],
    pub root: Did,
    pub device: Did,
    pub endpoint_id: TransportEndpointId,
    pub routing_key: AgreementPublicKey,
    pub address: SocketAddr,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub nonce: [u8; 32],
    pub signatures: Vec<IndexedSig>,
}

/// Advertisement that has passed KEL, delegation, capability, network,
/// freshness, signature, endpoint-id, address, and replay checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPeerAdvertisement {
    advertisement: PeerAdvertisement,
    capabilities: Capabilities,
}

impl VerifiedPeerAdvertisement {
    pub fn endpoint_id(&self) -> TransportEndpointId {
        self.advertisement.endpoint_id
    }

    pub fn routing_key(&self) -> AgreementPublicKey {
        self.advertisement.routing_key
    }

    pub fn address(&self) -> SocketAddr {
        self.advertisement.address
    }

    pub fn network_id(&self) -> [u8; 32] {
        self.advertisement.network_id
    }

    pub fn root(&self) -> &Did {
        &self.advertisement.root
    }

    pub fn device(&self) -> &Did {
        &self.advertisement.device
    }

    pub fn expires_at_ms(&self) -> u64 {
        self.advertisement.expires_at_ms
    }

    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    pub fn advertisement(&self) -> &PeerAdvertisement {
        &self.advertisement
    }
}

impl PeerAdvertisement {
    #[allow(clippy::too_many_arguments)]
    pub fn issue(
        network_id: [u8; 32],
        root: &Did,
        device: &Controller,
        routing_key: AgreementPublicKey,
        address: SocketAddr,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<Self> {
        if device.delegator().is_none_or(|delegator| delegator != root) {
            return Err(TransportSecurityError::IdentityMismatch);
        }
        validate_address(address)?;
        validate_window(issued_at_ms, expires_at_ms, issued_at_ms)?;
        let nonce = mini_crypto::random_32()?;
        let endpoint_id = TransportEndpointId::derive(&device.did(), &routing_key);
        let mut advertisement = Self {
            network_id,
            root: root.clone(),
            device: device.did(),
            endpoint_id,
            routing_key,
            address,
            issued_at_ms,
            expires_at_ms,
            nonce,
            signatures: Vec::new(),
        };
        advertisement.signatures = device.sign_message(&advertisement.signing_bytes()?);
        if advertisement.signatures.is_empty()
            || advertisement.signatures.len() > MAX_PEER_ADVERTISEMENT_SIGNATURES
        {
            return Err(TransportSecurityError::LimitExceeded);
        }
        Ok(advertisement)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn verify(
        &self,
        expected_network_id: [u8; 32],
        now_ms: u64,
        root_kel: &Kel,
        device_kel: &Kel,
        freshness: &mut FreshnessPins,
        replay: &mut ReplayCache,
    ) -> Result<VerifiedPeerAdvertisement> {
        if self.network_id != expected_network_id {
            return Err(TransportSecurityError::WrongNetwork);
        }
        if root_kel.did() != self.root || device_kel.did() != self.device {
            return Err(TransportSecurityError::IdentityMismatch);
        }
        if self.endpoint_id != TransportEndpointId::derive(&self.device, &self.routing_key) {
            return Err(TransportSecurityError::EndpointMismatch);
        }
        validate_address(self.address)?;
        validate_window(self.issued_at_ms, self.expires_at_ms, now_ms)?;
        freshness.check_and_pin(root_kel)?;
        freshness.check_and_pin(device_kel)?;
        let capabilities = verify_delegation(root_kel, device_kel)?;
        if !capabilities.contains(Capabilities::SIGN) {
            return Err(TransportSecurityError::CapabilityDenied);
        }
        device_kel.verify_message(&self.signing_bytes()?, &self.signatures)?;
        replay.check_and_record(self.replay_id(), self.expires_at_ms, now_ms)?;
        Ok(VerifiedPeerAdvertisement {
            advertisement: self.clone(),
            capabilities,
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        writer.u8(PEER_ADVERTISEMENT_VERSION);
        encode_unsigned(&mut writer, self)?;
        let count = u16::try_from(self.signatures.len())
            .map_err(|_| TransportSecurityError::LimitExceeded)?;
        if self.signatures.is_empty() || self.signatures.len() > MAX_PEER_ADVERTISEMENT_SIGNATURES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        writer.u16(count);
        for signature in &self.signatures {
            writer.u32(signature.index);
            writer.u8(signature.signature.suite().tag());
            writer.bytes(&signature.signature.to_bytes())?;
        }
        let bytes = writer.finish();
        if bytes.len() > MAX_PEER_ADVERTISEMENT_BYTES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_PEER_ADVERTISEMENT_BYTES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        if reader.u8()? != PEER_ADVERTISEMENT_VERSION {
            return Err(TransportSecurityError::UnsupportedVersion);
        }
        let network_id: [u8; 32] = reader
            .take(32)?
            .try_into()
            .map_err(|_| TransportSecurityError::Malformed)?;
        let root = Did::parse(reader.string(MAX_TRANSPORT_DID_BYTES)?)?;
        let device = Did::parse(reader.string(MAX_TRANSPORT_DID_BYTES)?)?;
        let endpoint_id = TransportEndpointId::from_bytes(
            reader
                .take(32)?
                .try_into()
                .map_err(|_| TransportSecurityError::Malformed)?,
        );
        let agreement_suite = KeyAgreementSuite::from_tag(reader.u8()?)?;
        let routing_key = AgreementPublicKey::from_suite_bytes(
            agreement_suite,
            reader.take(agreement_suite.public_key_len())?,
        )?;
        let address = decode_address(&mut reader)?;
        let issued_at_ms = reader.u64()?;
        let expires_at_ms = reader.u64()?;
        let nonce: [u8; 32] = reader
            .take(32)?
            .try_into()
            .map_err(|_| TransportSecurityError::Malformed)?;
        let signature_count = reader.u16()? as usize;
        if signature_count == 0 || signature_count > MAX_PEER_ADVERTISEMENT_SIGNATURES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        let mut signatures = Vec::with_capacity(signature_count.min(8));
        for _ in 0..signature_count {
            let index = reader.u32()?;
            let suite = SignatureSuite::from_tag(reader.u8()?)?;
            let signature =
                Signature::from_suite_bytes(suite, reader.bytes(suite.signature_len())?)?;
            signatures.push(IndexedSig { index, signature });
        }
        if !reader.finished() {
            return Err(TransportSecurityError::TrailingBytes);
        }
        let advertisement = Self {
            network_id,
            root,
            device,
            endpoint_id,
            routing_key,
            address,
            issued_at_ms,
            expires_at_ms,
            nonce,
            signatures,
        };
        if advertisement.to_bytes()?.as_slice() != bytes {
            return Err(TransportSecurityError::Malformed);
        }
        Ok(advertisement)
    }

    fn signing_bytes(&self) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        writer.raw(ADVERTISEMENT_DOMAIN);
        writer.u8(PEER_ADVERTISEMENT_VERSION);
        encode_unsigned(&mut writer, self)?;
        Ok(writer.finish())
    }

    fn replay_id(&self) -> [u8; 32] {
        let mut transcript = Vec::with_capacity(ADVERTISEMENT_REPLAY_DOMAIN.len() + 32 + 32);
        transcript.extend_from_slice(ADVERTISEMENT_REPLAY_DOMAIN);
        transcript.extend_from_slice(&self.endpoint_id.to_bytes());
        transcript.extend_from_slice(&self.nonce);
        HashAlgorithm::Blake3.digest(&transcript)
    }
}

/// Bounded secure-PEX response carrying signed advertisements. The response
/// itself grants no authority; each record is independently verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurePexResponse {
    pub network_id: [u8; 32],
    pub advertisements: Vec<PeerAdvertisement>,
}

impl SecurePexResponse {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.advertisements.len() > MAX_SECURE_PEX_RECORDS {
            return Err(TransportSecurityError::LimitExceeded);
        }
        if self
            .advertisements
            .iter()
            .any(|advertisement| advertisement.network_id != self.network_id)
        {
            return Err(TransportSecurityError::WrongNetwork);
        }
        let mut writer = Writer::new();
        writer.raw(SECURE_PEX_DOMAIN);
        writer.raw(&self.network_id);
        writer.u16(
            u16::try_from(self.advertisements.len())
                .map_err(|_| TransportSecurityError::LimitExceeded)?,
        );
        for advertisement in &self.advertisements {
            writer.bytes(&advertisement.to_bytes()?)?;
        }
        let bytes = writer.finish();
        if bytes.len() > MAX_SECURE_PEX_BYTES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_SECURE_PEX_BYTES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        if reader.take(SECURE_PEX_DOMAIN.len())? != SECURE_PEX_DOMAIN {
            return Err(TransportSecurityError::Malformed);
        }
        let network_id: [u8; 32] = reader
            .take(32)?
            .try_into()
            .map_err(|_| TransportSecurityError::Malformed)?;
        let count = reader.u16()? as usize;
        if count > MAX_SECURE_PEX_RECORDS {
            return Err(TransportSecurityError::LimitExceeded);
        }
        let mut advertisements = Vec::with_capacity(count.min(16));
        for _ in 0..count {
            advertisements.push(PeerAdvertisement::from_bytes(
                reader.bytes(MAX_PEER_ADVERTISEMENT_BYTES)?,
            )?);
        }
        if !reader.finished() {
            return Err(TransportSecurityError::TrailingBytes);
        }
        let response = Self {
            network_id,
            advertisements,
        };
        if response.to_bytes()?.as_slice() != bytes {
            return Err(TransportSecurityError::Malformed);
        }
        Ok(response)
    }
}

fn encode_unsigned(writer: &mut Writer, advertisement: &PeerAdvertisement) -> Result<()> {
    writer.raw(&advertisement.network_id);
    writer.string(advertisement.root.as_str())?;
    writer.string(advertisement.device.as_str())?;
    writer.raw(&advertisement.endpoint_id.to_bytes());
    writer.u8(advertisement.routing_key.suite().tag());
    writer.raw(&advertisement.routing_key.to_bytes());
    encode_address(writer, advertisement.address);
    writer.u64(advertisement.issued_at_ms);
    writer.u64(advertisement.expires_at_ms);
    writer.raw(&advertisement.nonce);
    Ok(())
}

fn encode_address(writer: &mut Writer, address: SocketAddr) {
    match address.ip() {
        IpAddr::V4(ip) => {
            writer.u8(ADDR_V4);
            writer.raw(&ip.octets());
        }
        IpAddr::V6(ip) => {
            writer.u8(ADDR_V6);
            writer.raw(&ip.octets());
        }
    }
    writer.u16(address.port());
}

fn decode_address(reader: &mut Reader<'_>) -> Result<SocketAddr> {
    let ip = match reader.u8()? {
        ADDR_V4 => {
            let octets: [u8; 4] = reader
                .take(4)?
                .try_into()
                .map_err(|_| TransportSecurityError::Malformed)?;
            IpAddr::V4(Ipv4Addr::from(octets))
        }
        ADDR_V6 => {
            let octets: [u8; 16] = reader
                .take(16)?
                .try_into()
                .map_err(|_| TransportSecurityError::Malformed)?;
            IpAddr::V6(Ipv6Addr::from(octets))
        }
        _ => return Err(TransportSecurityError::Malformed),
    };
    Ok(SocketAddr::new(ip, reader.u16()?))
}

fn validate_address(address: SocketAddr) -> Result<()> {
    if address.port() == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
        return Err(TransportSecurityError::Malformed);
    }
    if matches!(address.ip(), IpAddr::V4(ip) if ip.is_broadcast()) {
        return Err(TransportSecurityError::Malformed);
    }
    Ok(())
}

fn validate_window(issued_at_ms: u64, expires_at_ms: u64, now_ms: u64) -> Result<()> {
    let lifetime = expires_at_ms
        .checked_sub(issued_at_ms)
        .ok_or(TransportSecurityError::Malformed)?;
    if lifetime == 0 || lifetime > MAX_PEER_ADVERTISEMENT_LIFETIME_MS {
        return Err(TransportSecurityError::LifetimeTooLong);
    }
    if issued_at_ms > now_ms.saturating_add(MAX_PEER_ADVERTISEMENT_CLOCK_SKEW_MS) {
        return Err(TransportSecurityError::NotYetValid);
    }
    if now_ms > expires_at_ms {
        return Err(TransportSecurityError::Expired);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use did_mini::Controller;
    use mini_crypto::AgreementSecretKey;

    use super::*;

    fn identity(seed: u8) -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    #[test]
    fn issue_generates_fresh_nonce_internally() {
        let (root, device) = identity(10);
        let routing = AgreementSecretKey::from_seed(&[20; 32]).public_key();
        let first = PeerAdvertisement::issue(
            [7; 32],
            &root.did(),
            &device,
            routing,
            "127.0.0.1:9000".parse().unwrap(),
            1_000,
            2_000,
        )
        .unwrap();
        let second = PeerAdvertisement::issue(
            [7; 32],
            &root.did(),
            &device,
            routing,
            "127.0.0.1:9000".parse().unwrap(),
            1_000,
            2_000,
        )
        .unwrap();

        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.replay_id(), second.replay_id());
    }

    #[test]
    fn signed_advertisement_round_trips_and_verifies() {
        let (root, device) = identity(10);
        let routing = AgreementSecretKey::from_seed(&[20; 32]).public_key();
        let advertisement = PeerAdvertisement::issue(
            [7; 32],
            &root.did(),
            &device,
            routing,
            "127.0.0.1:9000".parse().unwrap(),
            1_000,
            2_000,
        )
        .unwrap();
        let decoded = PeerAdvertisement::from_bytes(&advertisement.to_bytes().unwrap()).unwrap();
        let mut pins = FreshnessPins::new();
        let mut replay = ReplayCache::new(8).unwrap();
        let verified = decoded
            .verify(
                [7; 32],
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            )
            .unwrap();
        assert_eq!(verified.routing_key(), routing);
        assert_eq!(verified.address().port(), 9000);
    }

    #[test]
    fn redirect_network_expiry_and_replay_fail_closed() {
        let (root, device) = identity(10);
        let routing = AgreementSecretKey::from_seed(&[20; 32]).public_key();
        let advertisement = PeerAdvertisement::issue(
            [7; 32],
            &root.did(),
            &device,
            routing,
            "127.0.0.1:9000".parse().unwrap(),
            1_000,
            2_000,
        )
        .unwrap();
        let mut pins = FreshnessPins::new();
        let mut replay = ReplayCache::new(8).unwrap();
        assert_eq!(
            advertisement.verify(
                [9; 32],
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            ),
            Err(TransportSecurityError::WrongNetwork)
        );
        assert_eq!(
            advertisement.verify(
                [7; 32],
                2_001,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            ),
            Err(TransportSecurityError::Expired)
        );
        advertisement
            .verify(
                [7; 32],
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            )
            .unwrap();
        assert_eq!(
            advertisement.verify(
                [7; 32],
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            ),
            Err(TransportSecurityError::Replay)
        );

        let mut redirected = advertisement.clone();
        redirected.address = "127.0.0.1:9999".parse().unwrap();
        let mut other_replay = ReplayCache::new(8).unwrap();
        assert!(redirected
            .verify(
                [7; 32],
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut other_replay,
            )
            .is_err());
    }

    #[test]
    fn secure_pex_is_bounded_and_canonical() {
        let (root, device) = identity(10);
        let routing = AgreementSecretKey::from_seed(&[20; 32]).public_key();
        let advertisement = PeerAdvertisement::issue(
            [7; 32],
            &root.did(),
            &device,
            routing,
            "127.0.0.1:9000".parse().unwrap(),
            1_000,
            2_000,
        )
        .unwrap();
        let response = SecurePexResponse {
            network_id: [7; 32],
            advertisements: vec![advertisement],
        };
        let bytes = response.to_bytes().unwrap();
        assert_eq!(SecurePexResponse::from_bytes(&bytes).unwrap(), response);
        let mismatched = SecurePexResponse {
            network_id: [8; 32],
            advertisements: response.advertisements.clone(),
        };
        assert_eq!(
            mismatched.to_bytes(),
            Err(TransportSecurityError::WrongNetwork)
        );
        let mut trailing = bytes;
        trailing.push(0);
        assert!(SecurePexResponse::from_bytes(&trailing).is_err());
    }
}
