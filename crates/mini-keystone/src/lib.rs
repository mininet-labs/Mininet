//! The keystone demo (SPEC-03), composed end to end from the beta crates:
//!
//! 1. Two identity roots each incept a `did:mini` root and delegate an `ATTEST`-capable
//!    device (`did-mini`).
//! 2. The devices form an **anonymous, forward-secret encrypted channel** over a
//!    bearer (`mini-bearer`) — BLE / local Wi-Fi in the field, in-process in CI.
//! 3. They **exchange KELs through the encrypted channel** and verify each other's
//!    identity and delegation offline — self-certifying, no registry, no server.
//! 4. Both devices sign a **range-bound presence attestation** bound to this very
//!    channel (`mini-presence`), and each side verifies it.
//! 5. Verified presence **accrues non-spendable reward** per identity root (`mini-reward`).
//!
//! No internet. No server. No identity revealed by the transport (P5). One identity
//! root, one accrual in this alpha; one human requires SPEC-02 personhood.
//! Slow, diversity-weighted value (P4). Nothing here is money or a vote (P1).
//!
//! [`run_demo`] drives the whole flow over any two connected [`Bearer`] endpoints
//! and returns a [`DemoReport`]; the integration test runs it over the in-process
//! bearer, and a real BLE/Wi-Fi adapter can drive the identical flow on phones.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use did_mini::{Capabilities, Controller, Kel};
use mini_bearer::{Bearer, BearerError, Initiator, Responder};
use mini_presence::{
    kel_digest, verify_presence, AttestationFields, InMemoryReplayGuard, Party,
    PresenceAttestation, RangePolicy, TransportKind, VerifyContext, PRESENCE_VERSION,
};
use mini_reward::{accrue, RewardAccount, RewardParams};

/// One participant: an identity root and their delegated device.
#[derive(Debug)]
pub struct Participant {
    /// The identity root controller (stays "at home"; only its public KEL travels).
    pub root: Controller,
    /// The delegated, `ATTEST`-capable device controller.
    pub device: Controller,
}

impl Participant {
    /// Incept an identity root and delegate one primary device, from explicit seeds
    /// (deterministic, for tests/demo).
    pub fn from_seeds(
        root_current: [u8; 32],
        root_next: [u8; 32],
        device_current: [u8; 32],
        device_next: [u8; 32],
    ) -> Result<Self, DemoError> {
        let mut root = Controller::incept_single_from_seeds(&root_current, &root_next)
            .map_err(DemoError::Identity)?;
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &device_current, &device_next)
                .map_err(DemoError::Identity)?;
        root.delegate_device(&device.did(), Capabilities::primary())
            .map_err(DemoError::Identity)?;
        Ok(Participant { root, device })
    }
}

/// What the demo proved, for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoReport {
    /// The initiator identity root's `did:mini`.
    pub initiator_root: String,
    /// The responder identity root's `did:mini`.
    pub responder_root: String,
    /// The channel binding both ends derived (hex-free raw bytes).
    pub channel_binding: [u8; 32],
    /// The initiator identity root's accrual after the encounter.
    pub initiator_account: RewardAccount,
    /// The responder identity root's accrual after the encounter.
    pub responder_account: RewardAccount,
}

/// A failure anywhere in the composed flow.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DemoError {
    /// Transport/channel failure.
    Bearer(BearerError),
    /// Identity/delegation failure.
    Identity(did_mini::IdentityError),
    /// Presence attestation failure.
    Presence(mini_presence::PresenceError),
    /// A peer sent something malformed.
    BadPeerMessage,
}

impl core::fmt::Display for DemoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DemoError::Bearer(e) => write!(f, "bearer: {e}"),
            DemoError::Identity(e) => write!(f, "identity: {e}"),
            DemoError::Presence(e) => write!(f, "presence: {e}"),
            DemoError::BadPeerMessage => write!(f, "malformed peer message"),
        }
    }
}

impl std::error::Error for DemoError {}

impl From<BearerError> for DemoError {
    fn from(e: BearerError) -> Self {
        DemoError::Bearer(e)
    }
}
impl From<did_mini::IdentityError> for DemoError {
    fn from(e: did_mini::IdentityError) -> Self {
        DemoError::Identity(e)
    }
}
impl From<mini_presence::PresenceError> for DemoError {
    fn from(e: mini_presence::PresenceError) -> Self {
        DemoError::Presence(e)
    }
}

/// AAD labels for the encrypted application messages.
const AAD_KEL_ROOT: &[u8] = b"MINI/DEMO kel-root";
const AAD_KEL_DEVICE: &[u8] = b"MINI/DEMO kel-device";

/// Drive the full keystone flow over two connected bearer endpoints.
///
/// `a` initiates, `b` responds. `now_ms` is the demo clock (caller-supplied so
/// the flow stays deterministic and I/O-free); `transport` names the physical
/// bearer kind for the attestation.
pub fn run_demo(
    a: &Participant,
    b: &Participant,
    bearer_a: &mut dyn Bearer,
    bearer_b: &mut dyn Bearer,
    transport: TransportKind,
    now_ms: u64,
) -> Result<DemoReport, DemoError> {
    // --- 1. Anonymous encrypted channel (no identities in the handshake). ---
    let (initiator, hello1) = Initiator::start()?;
    bearer_a.send(&hello1)?;
    let got1 = bearer_b.recv()?;
    let (mut chan_b, hello2) = Responder::respond(&got1)?;
    bearer_b.send(&hello2)?;
    let got2 = bearer_a.recv()?;
    let mut chan_a = initiator.finish(&got2)?;
    let binding = chan_a.channel_binding();

    // --- 2. Identity exchange THROUGH the encrypted channel. ---
    // Each side sends (root KEL, device KEL); only public logs travel; secrets
    // never leave either device (G1).
    send_kels(bearer_a, &mut chan_a, &a.root.kel(), &a.device.kel())?;
    let (peer_root_at_b, peer_device_at_b) = recv_kels(bearer_b, &mut chan_b)?; // A's logs, held by B
    send_kels(bearer_b, &mut chan_b, &b.root.kel(), &b.device.kel())?;
    let (peer_root_at_a, peer_device_at_a) = recv_kels(bearer_a, &mut chan_a)?; // B's logs, held by A

    // Each side verifies the peer's identity + delegation, fully offline: the
    // KELs self-certify (SCID re-derivation), and the device must be a delegated,
    // unrevoked, ATTEST-capable device of its identity root.
    let peer_caps_at_a = did_mini::verify_delegation(&peer_root_at_a, &peer_device_at_a)?;
    let peer_caps_at_b = did_mini::verify_delegation(&peer_root_at_b, &peer_device_at_b)?;
    if !peer_caps_at_a.contains(Capabilities::ATTEST)
        || !peer_caps_at_b.contains(Capabilities::ATTEST)
    {
        return Err(DemoError::Presence(
            mini_presence::PresenceError::MissingAttestCapability,
        ));
    }

    // --- 3. Mutually-signed, range-bound presence attestation. ---
    let fields = AttestationFields {
        version: PRESENCE_VERSION,
        channel_binding: binding,
        initiator: Party {
            device: a.device.did(),
            kel_digest: kel_digest(&a.device.kel()),
            nonce: demo_nonce(&binding, 1),
        },
        responder: Party {
            device: b.device.did(),
            kel_digest: kel_digest(&b.device.kel()),
            nonce: demo_nonce(&binding, 2),
        },
        started_at_ms: now_ms.saturating_sub(6),
        finished_at_ms: now_ms,
        rtt_samples_ms: vec![9, 11, 10, 12],
        transport,
        location_commitment: None,
    };
    let att = PresenceAttestation::new(
        fields.clone(),
        fields.sign(&a.device),
        fields.sign(&b.device),
    );

    // Each side verifies with ITS OWN replay guard and the binding IT derived.
    let policy = RangePolicy::ble_default();
    let a_root_pub = a.root.kel();
    let a_dev_pub = a.device.kel();
    let verdict = {
        // Side A verifies using the peer logs it received over the channel.
        let ctx = VerifyContext {
            initiator_root: &a_root_pub,
            responder_root: &peer_root_at_a, // B's root, as received by A
            initiator_device: &a_dev_pub,
            responder_device: &peer_device_at_a, // B's device, as received by A
            policy: &policy,
            now_ms: Some(now_ms),
            expected_binding: Some(chan_a.channel_binding()),
        };
        let mut guard = InMemoryReplayGuard::new();
        verify_presence(&att, &ctx, &mut guard)?
    };
    {
        // Side B verifies symmetrically with the logs it received.
        let b_root_pub = b.root.kel();
        let b_dev_pub = b.device.kel();
        let ctx = VerifyContext {
            initiator_root: &peer_root_at_b, // A's root, as received by B
            responder_root: &b_root_pub,
            initiator_device: &peer_device_at_b, // A's device, as received by B
            responder_device: &b_dev_pub,
            policy: &policy,
            now_ms: Some(now_ms),
            expected_binding: Some(binding),
        };
        let mut guard = InMemoryReplayGuard::new();
        verify_presence(&att, &ctx, &mut guard)?;
    }

    // --- 4. Verified presence becomes (non-spendable) local value. ---
    let params = RewardParams::demo_default();
    let verdicts = vec![verdict];
    let initiator_account = accrue(&a.root.did(), &verdicts, &params, now_ms);
    let responder_account = accrue(&b.root.did(), &verdicts, &params, now_ms);

    Ok(DemoReport {
        initiator_root: a.root.did().as_str().to_string(),
        responder_root: b.root.did().as_str().to_string(),
        channel_binding: binding,
        initiator_account,
        responder_account,
    })
}

/// Send (root KEL, device KEL) as two encrypted messages.
fn send_kels(
    bearer: &mut dyn Bearer,
    chan: &mut mini_bearer::Channel,
    root: &Kel,
    device: &Kel,
) -> Result<(), DemoError> {
    let ct1 = chan.seal(&root.to_bytes(), AAD_KEL_ROOT)?;
    bearer.send(&ct1)?;
    let ct2 = chan.seal(&device.to_bytes(), AAD_KEL_DEVICE)?;
    bearer.send(&ct2)?;
    Ok(())
}

/// Receive (root KEL, device KEL) as two encrypted messages and parse them.
fn recv_kels(
    bearer: &mut dyn Bearer,
    chan: &mut mini_bearer::Channel,
) -> Result<(Kel, Kel), DemoError> {
    let ct1 = bearer.recv()?;
    let root_bytes = chan.open(&ct1, AAD_KEL_ROOT)?;
    let ct2 = bearer.recv()?;
    let device_bytes = chan.open(&ct2, AAD_KEL_DEVICE)?;
    let root = Kel::from_bytes(&root_bytes).map_err(|_| DemoError::BadPeerMessage)?;
    let device = Kel::from_bytes(&device_bytes).map_err(|_| DemoError::BadPeerMessage)?;
    Ok((root, device))
}

/// Derive a demo nonce from the channel binding and a role tag — session-unique
/// because the binding is, and different per role. (Real devices use OS entropy;
/// the demo stays deterministic per session.)
fn demo_nonce(binding: &[u8; 32], role: u8) -> [u8; 32] {
    let mut input = Vec::with_capacity(33);
    input.extend_from_slice(binding);
    input.push(role);
    mini_crypto::HashAlgorithm::Blake3.digest(&input)
}
