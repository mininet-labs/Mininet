//! Optional self-certifying authentication above anonymous CH1.
//!
//! The bearer remains identity-agnostic. This module signs the already unique
//! CH1 channel binding with a delegated `did:mini` device and verifies it using
//! caller-supplied KELs and rollback pins. No certificate authority, DNS name,
//! hosted registry, or trust-on-first-use rule is introduced.

use did_mini::{
    verify_delegation, Capabilities, Controller, Did, FreshnessPins, IndexedSig, Kel,
};
use mini_crypto::{
    AgreementPublicKey, HashAlgorithm, KeyAgreementSuite, Signature, SignatureSuite,
};

use crate::codec::{Reader, Writer};
use crate::{ReplayCache, Result, TransportSecurityError};

pub const SESSION_AUTH_VERSION: u8 = 1;
pub const MAX_SESSION_AUTH_BYTES: usize = 64 * 1024;
pub const MAX_SESSION_AUTH_SIGNATURES: usize = 64;
pub const MAX_TRANSPORT_DID_BYTES: usize = 4 * 1024;
pub const MAX_SESSION_AUTH_LIFETIME_MS: u64 = 5 * 60 * 1000;
pub const MAX_SESSION_CLOCK_SKEW_MS: u64 = 30 * 1000;

const AUTH_DOMAIN: &[u8] = b"mini-transport-security/session-auth/v1";
const AUTH_REPLAY_DOMAIN: &[u8] = b"mini-transport-security/session-replay/v1";
const ENDPOINT_DOMAIN: &[u8] = b"mini-transport-security/endpoint/v1";

/// Which side of the anonymous CH1 transcript this proof authenticates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionRole {
    Initiator,
    Responder,
}

impl SessionRole {
    const fn tag(self) -> u8 {
        match self {
            Self::Initiator => 1,
            Self::Responder => 2,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            1 => Ok(Self::Initiator),
            2 => Ok(Self::Responder),
            _ => Err(TransportSecurityError::Malformed),
        }
    }
}

/// Typed reason for disclosing an authenticated endpoint to this counterparty.
/// The purpose is signed and verified exactly, so a peer cannot replay a broad
/// proof in a more sensitive protocol or silently downgrade the capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportPurpose {
    PeerExchange,
    Relay,
    Messaging,
    StateSync,
    Consensus,
}

impl TransportPurpose {
    const fn tag(self) -> u8 {
        match self {
            Self::PeerExchange => 1,
            Self::Relay => 2,
            Self::Messaging => 3,
            Self::StateSync => 4,
            Self::Consensus => 5,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            1 => Ok(Self::PeerExchange),
            2 => Ok(Self::Relay),
            3 => Ok(Self::Messaging),
            4 => Ok(Self::StateSync),
            5 => Ok(Self::Consensus),
            _ => Err(TransportSecurityError::Malformed),
        }
    }

    pub const fn required_capability(self) -> Capabilities {
        match self {
            Self::PeerExchange | Self::Relay | Self::Messaging | Self::StateSync => {
                Capabilities::SIGN
            }
            Self::Consensus => Capabilities::VOTE,
        }
    }
}

/// A rotating, self-certifying transport endpoint. It binds a `did:mini`
/// device/pairwise identity to the X25519 routing key currently advertised for
/// that endpoint. Rotating the routing key rotates this id and reduces linkability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TransportEndpointId([u8; 32]);

impl TransportEndpointId {
    pub fn derive(device: &Did, routing_key: &AgreementPublicKey) -> Self {
        let mut transcript = Vec::with_capacity(
            ENDPOINT_DOMAIN.len() + device.as_str().len() + 1 + routing_key.to_bytes().len(),
        );
        transcript.extend_from_slice(ENDPOINT_DOMAIN);
        transcript.extend_from_slice(&(device.as_str().len() as u32).to_be_bytes());
        transcript.extend_from_slice(device.as_str().as_bytes());
        transcript.push(routing_key.suite().tag());
        transcript.extend_from_slice(&routing_key.to_bytes());
        Self(HashAlgorithm::Blake3.digest(&transcript))
    }

    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn to_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Wire proof that one side of one exact CH1 session controls a currently
/// delegated device and the presented routing key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAuthClaim {
    pub role: SessionRole,
    pub purpose: TransportPurpose,
    pub root: Did,
    pub device: Did,
    pub endpoint_id: TransportEndpointId,
    pub routing_key: AgreementPublicKey,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub nonce: [u8; 32],
    pub signatures: Vec<IndexedSig>,
}

/// Locally verified peer result. This is session authority only; it is not
/// personhood, governance standing, trust, reputation, or payment authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPeer {
    pub root: Did,
    pub device: Did,
    pub endpoint_id: TransportEndpointId,
    pub routing_key: AgreementPublicKey,
    pub capabilities: Capabilities,
    pub purpose: TransportPurpose,
}

impl SessionAuthClaim {
    #[allow(clippy::too_many_arguments)]
    pub fn issue(
        root: &Did,
        device: &Controller,
        role: SessionRole,
        purpose: TransportPurpose,
        routing_key: AgreementPublicKey,
        channel_binding: &[u8; 32],
        issued_at_ms: u64,
        expires_at_ms: u64,
        nonce: [u8; 32],
    ) -> Result<Self> {
        if device.delegator().is_none_or(|delegator| delegator != root) {
            return Err(TransportSecurityError::IdentityMismatch);
        }
        validate_window(issued_at_ms, expires_at_ms, issued_at_ms)?;
        let endpoint_id = TransportEndpointId::derive(&device.did(), &routing_key);
        let mut claim = Self {
            role,
            purpose,
            root: root.clone(),
            device: device.did(),
            endpoint_id,
            routing_key,
            issued_at_ms,
            expires_at_ms,
            nonce,
            signatures: Vec::new(),
        };
        claim.signatures = device.sign_message(&claim.signing_bytes(channel_binding)?);
        if claim.signatures.is_empty() || claim.signatures.len() > MAX_SESSION_AUTH_SIGNATURES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        Ok(claim)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn verify(
        &self,
        expected_role: SessionRole,
        expected_purpose: TransportPurpose,
        channel_binding: &[u8; 32],
        now_ms: u64,
        root_kel: &Kel,
        device_kel: &Kel,
        freshness: &mut FreshnessPins,
        replay: &mut ReplayCache,
    ) -> Result<AuthenticatedPeer> {
        if self.role != expected_role {
            return Err(TransportSecurityError::WrongRole);
        }
        if self.purpose != expected_purpose {
            return Err(TransportSecurityError::WrongPurpose);
        }
        if root_kel.did() != self.root || device_kel.did() != self.device {
            return Err(TransportSecurityError::IdentityMismatch);
        }
        if self.endpoint_id != TransportEndpointId::derive(&self.device, &self.routing_key) {
            return Err(TransportSecurityError::EndpointMismatch);
        }
        validate_window(self.issued_at_ms, self.expires_at_ms, now_ms)?;

        // Refuse rollback below any KEL sequence already observed by this
        // verifier. First-contact unknown-freshness remains an explicit floor.
        freshness.check_and_pin(root_kel)?;
        freshness.check_and_pin(device_kel)?;

        let capabilities = verify_delegation(root_kel, device_kel)?;
        let required = self.purpose.required_capability();
        if required == Capabilities::empty() {
            return Err(TransportSecurityError::EmptyCapability);
        }
        if !capabilities.contains(required) {
            return Err(TransportSecurityError::CapabilityDenied);
        }
        device_kel.verify_message(&self.signing_bytes(channel_binding)?, &self.signatures)?;
        replay.check_and_record(self.replay_id(channel_binding))?;

        Ok(AuthenticatedPeer {
            root: self.root.clone(),
            device: self.device.clone(),
            endpoint_id: self.endpoint_id,
            routing_key: self.routing_key,
            capabilities,
            purpose: self.purpose,
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        writer.u8(SESSION_AUTH_VERSION);
        writer.u8(self.role.tag());
        writer.u8(self.purpose.tag());
        writer.string(self.root.as_str())?;
        writer.string(self.device.as_str())?;
        writer.raw(&self.endpoint_id.to_bytes());
        writer.u8(self.routing_key.suite().tag());
        writer.raw(&self.routing_key.to_bytes());
        writer.u64(self.issued_at_ms);
        writer.u64(self.expires_at_ms);
        writer.raw(&self.nonce);
        let count = u16::try_from(self.signatures.len())
            .map_err(|_| TransportSecurityError::LimitExceeded)?;
        if self.signatures.is_empty() || self.signatures.len() > MAX_SESSION_AUTH_SIGNATURES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        writer.u16(count);
        for signature in &self.signatures {
            writer.u32(signature.index);
            writer.u8(signature.signature.suite().tag());
            writer.bytes(&signature.signature.to_bytes())?;
        }
        let bytes = writer.finish();
        if bytes.len() > MAX_SESSION_AUTH_BYTES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_SESSION_AUTH_BYTES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        if reader.u8()? != SESSION_AUTH_VERSION {
            return Err(TransportSecurityError::UnsupportedVersion);
        }
        let role = SessionRole::from_tag(reader.u8()?)?;
        let purpose = TransportPurpose::from_tag(reader.u8()?)?;
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
        let issued_at_ms = reader.u64()?;
        let expires_at_ms = reader.u64()?;
        let nonce: [u8; 32] = reader
            .take(32)?
            .try_into()
            .map_err(|_| TransportSecurityError::Malformed)?;
        let signature_count = reader.u16()? as usize;
        if signature_count == 0 || signature_count > MAX_SESSION_AUTH_SIGNATURES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        let mut signatures = Vec::with_capacity(signature_count.min(8));
        for _ in 0..signature_count {
            let index = reader.u32()?;
            let suite = SignatureSuite::from_tag(reader.u8()?)?;
            let signature = Signature::from_suite_bytes(
                suite,
                reader.bytes(suite.signature_len())?,
            )?;
            signatures.push(IndexedSig { index, signature });
        }
        if !reader.finished() {
            return Err(TransportSecurityError::TrailingBytes);
        }
        let claim = Self {
            role,
            purpose,
            root,
            device,
            endpoint_id,
            routing_key,
            issued_at_ms,
            expires_at_ms,
            nonce,
            signatures,
        };
        if claim.to_bytes()?.as_slice() != bytes {
            return Err(TransportSecurityError::Malformed);
        }
        Ok(claim)
    }

    fn signing_bytes(&self, channel_binding: &[u8; 32]) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        writer.raw(AUTH_DOMAIN);
        writer.u8(SESSION_AUTH_VERSION);
        writer.u8(self.role.tag());
        writer.u8(self.purpose.tag());
        writer.string(self.root.as_str())?;
        writer.string(self.device.as_str())?;
        writer.raw(&self.endpoint_id.to_bytes());
        writer.u8(self.routing_key.suite().tag());
        writer.raw(&self.routing_key.to_bytes());
        writer.u64(self.issued_at_ms);
        writer.u64(self.expires_at_ms);
        writer.raw(&self.nonce);
        writer.raw(channel_binding);
        Ok(writer.finish())
    }

    fn replay_id(&self, channel_binding: &[u8; 32]) -> [u8; 32] {
        let mut transcript = Vec::with_capacity(
            AUTH_REPLAY_DOMAIN.len() + 32 + 32 + 32 + 2,
        );
        transcript.extend_from_slice(AUTH_REPLAY_DOMAIN);
        transcript.extend_from_slice(channel_binding);
        transcript.extend_from_slice(&self.endpoint_id.to_bytes());
        transcript.extend_from_slice(&self.nonce);
        transcript.push(self.role.tag());
        transcript.push(self.purpose.tag());
        HashAlgorithm::Blake3.digest(&transcript)
    }
}

fn validate_window(issued_at_ms: u64, expires_at_ms: u64, now_ms: u64) -> Result<()> {
    let lifetime = expires_at_ms
        .checked_sub(issued_at_ms)
        .ok_or(TransportSecurityError::Malformed)?;
    if lifetime == 0 || lifetime > MAX_SESSION_AUTH_LIFETIME_MS {
        return Err(TransportSecurityError::LifetimeTooLong);
    }
    if issued_at_ms > now_ms.saturating_add(MAX_SESSION_CLOCK_SKEW_MS) {
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
    use mini_bearer::{Initiator, Responder};
    use mini_crypto::AgreementSecretKey;

    use super::*;

    fn identity() -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[1; 32], &[2; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[3; 32],
            &[4; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    fn binding() -> [u8; 32] {
        let (initiator, hello) = Initiator::start().unwrap();
        let (responder, response) = Responder::respond(&hello).unwrap();
        let initiator = initiator.finish(&response).unwrap();
        assert_eq!(initiator.channel_binding(), responder.channel_binding());
        initiator.channel_binding()
    }

    #[test]
    fn claim_round_trips_and_authenticates_the_exact_session() {
        let (root, device) = identity();
        let routing = AgreementSecretKey::from_seed(&[8; 32]).public_key();
        let binding = binding();
        let claim = SessionAuthClaim::issue(
            &root.did(),
            &device,
            SessionRole::Initiator,
            TransportPurpose::Relay,
            routing,
            &binding,
            1_000,
            2_000,
            [9; 32],
        )
        .unwrap();
        let decoded = SessionAuthClaim::from_bytes(&claim.to_bytes().unwrap()).unwrap();
        let mut pins = FreshnessPins::new();
        let mut replay = ReplayCache::new(8).unwrap();
        let peer = decoded
            .verify(
                SessionRole::Initiator,
                TransportPurpose::Relay,
                &binding,
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            )
            .unwrap();
        assert_eq!(peer.endpoint_id, claim.endpoint_id);
        assert_eq!(peer.routing_key, routing);
    }

    #[test]
    fn another_channel_role_or_purpose_cannot_reuse_the_proof() {
        let (root, device) = identity();
        let routing = AgreementSecretKey::from_seed(&[8; 32]).public_key();
        let binding = binding();
        let claim = SessionAuthClaim::issue(
            &root.did(),
            &device,
            SessionRole::Initiator,
            TransportPurpose::Relay,
            routing,
            &binding,
            1_000,
            2_000,
            [9; 32],
        )
        .unwrap();
        let mut pins = FreshnessPins::new();
        let mut replay = ReplayCache::new(8).unwrap();
        assert_eq!(
            claim.verify(
                SessionRole::Responder,
                TransportPurpose::Relay,
                &binding,
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            ),
            Err(TransportSecurityError::WrongRole)
        );
        assert_eq!(
            claim.verify(
                SessionRole::Initiator,
                TransportPurpose::Consensus,
                &binding,
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            ),
            Err(TransportSecurityError::WrongPurpose)
        );
        let other_binding = [0x55; 32];
        assert!(claim
            .verify(
                SessionRole::Initiator,
                TransportPurpose::Relay,
                &other_binding,
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            )
            .is_err());
    }

    #[test]
    fn revoked_device_and_duplicate_claim_fail_closed() {
        let (mut root, device) = identity();
        let routing = AgreementSecretKey::from_seed(&[8; 32]).public_key();
        let binding = binding();
        let claim = SessionAuthClaim::issue(
            &root.did(),
            &device,
            SessionRole::Initiator,
            TransportPurpose::Relay,
            routing,
            &binding,
            1_000,
            2_000,
            [9; 32],
        )
        .unwrap();
        let pre_revoke = root.kel();
        root.revoke_device(&device.did()).unwrap();

        let mut pins = FreshnessPins::new();
        pins.check_and_pin(&root.kel()).unwrap();
        let mut replay = ReplayCache::new(8).unwrap();
        assert!(claim
            .verify(
                SessionRole::Initiator,
                TransportPurpose::Relay,
                &binding,
                1_500,
                &pre_revoke,
                &device.kel(),
                &mut pins,
                &mut replay,
            )
            .is_err());
        assert!(claim
            .verify(
                SessionRole::Initiator,
                TransportPurpose::Relay,
                &binding,
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            )
            .is_err());

        let (root, device) = identity();
        let claim = SessionAuthClaim::issue(
            &root.did(),
            &device,
            SessionRole::Initiator,
            TransportPurpose::Relay,
            routing,
            &binding,
            1_000,
            2_000,
            [10; 32],
        )
        .unwrap();
        let mut pins = FreshnessPins::new();
        let mut replay = ReplayCache::new(8).unwrap();
        claim
            .verify(
                SessionRole::Initiator,
                TransportPurpose::Relay,
                &binding,
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            )
            .unwrap();
        assert_eq!(
            claim.verify(
                SessionRole::Initiator,
                TransportPurpose::Relay,
                &binding,
                1_500,
                &root.kel(),
                &device.kel(),
                &mut pins,
                &mut replay,
            ),
            Err(TransportSecurityError::Replay)
        );
    }
}
