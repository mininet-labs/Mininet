//! An anonymous, forward-secret encrypted channel.
//!
//! Handshake (two messages, **no identities**):
//!
//! ```text
//! Initiator -> Responder :  version | ka_suite | kdf_suite | aead_suite | E_i
//! Responder -> Initiator :  version | ka_suite | kdf_suite | aead_suite | E_r
//! ```
//!
//! Both sides compute `dh = X25519(own_ephemeral, peer_ephemeral)` and derive, via
//! HKDF-SHA256 bound to the full handshake transcript, two directional
//! ChaCha20-Poly1305 traffic keys plus a 32-byte **channel binding**. Ephemeral
//! secrets are dropped (and zeroized) right after derivation, giving forward
//! secrecy.
//!
//! This provides confidentiality, integrity, forward secrecy, and unlinkability —
//! but *not* endpoint authentication, by design (see the crate-level security
//! model). Authenticity is a payload concern; presence attestations sign over
//! [`Channel::channel_binding`] so a signature cannot be transplanted onto a
//! different channel. Relay/wormhole resistance requires the presence pack's
//! round-trip distance bound on top.

use mini_crypto::{
    AeadKey, AeadNonce, AeadSuite, AgreementPublicKey, AgreementSecretKey, KdfSuite,
    KeyAgreementSuite,
};

use crate::bearer::MAX_FRAME_BYTES;
use crate::error::{BearerError, Result};

/// Wire version of the channel handshake.
pub const PROTOCOL_VERSION: u8 = 1;

/// ChaCha20-Poly1305 appends a 16-byte tag.
const AEAD_TAG_BYTES: usize = 16;

/// Hard cap on ciphertext accepted by the channel before AEAD allocation.
pub const MAX_CHANNEL_CIPHERTEXT_BYTES: usize = MAX_FRAME_BYTES;

/// Hard cap on plaintext accepted by the channel before AEAD allocation.
///
/// This leaves room for the AEAD tag so the resulting ciphertext still fits in a
/// default bearer frame.
pub const MAX_CHANNEL_PLAINTEXT_BYTES: usize = MAX_CHANNEL_CIPHERTEXT_BYTES - AEAD_TAG_BYTES;

/// Key agreement suite used by protocol version 1.
const KA_SUITE: KeyAgreementSuite = KeyAgreementSuite::X25519;
/// KDF suite used by protocol version 1.
const KDF_SUITE: KdfSuite = KdfSuite::HkdfSha256;
/// AEAD suite used by protocol version 1.
const AEAD_SUITE: AeadSuite = AeadSuite::ChaCha20Poly1305;
/// HKDF salt (protocol/version separation).
const HS_SALT: &[u8] = b"MINI/CH1 v1 handshake";
/// HKDF info prefix for traffic-key derivation (transcript is appended).
const TRAFFIC_INFO: &[u8] = b"MINI/CH1 v1 traffic";
/// 32 bytes per direction key + 32 bytes channel binding.
const OKM_LEN: usize = 96;
/// `version(1) | ka_suite(1) | kdf_suite(1) | aead_suite(1) | ephemeral_public(32)`.
const HELLO_LEN: usize = 36;

/// The initiator half of a handshake, holding its ephemeral secret until it
/// receives the responder's hello.
#[derive(Debug)]
pub struct Initiator {
    ephemeral: AgreementSecretKey,
    e_i_pub: [u8; 32],
}

impl Initiator {
    /// Begin a handshake: generate an ephemeral key and produce the hello to send.
    pub fn start() -> Result<(Self, Vec<u8>)> {
        let ephemeral = AgreementSecretKey::generate()?;
        let e_i_pub = ephemeral.public_key().to_bytes();
        Ok((Initiator { ephemeral, e_i_pub }, hello_msg(&e_i_pub)))
    }

    /// Complete the handshake from the responder's hello, yielding an established
    /// channel. Consumes `self`, so the ephemeral secret is dropped and zeroized.
    pub fn finish(self, responder_hello: &[u8]) -> Result<Channel> {
        let e_r_pub = parse_hello(responder_hello)?;
        let peer = AgreementPublicKey::from_suite_bytes(KA_SUITE, &e_r_pub)?;
        let dh = self.ephemeral.agree(&peer)?;
        let initiator_hello = hello_msg(&self.e_i_pub);
        let (k_i2r, k_r2i, binding) =
            derive_keys(&initiator_hello, responder_hello, dh.as_bytes())?;
        // Initiator sends on i2r, receives on r2i.
        Ok(Channel::new(k_i2r, k_r2i, binding))
    }
}

/// The responder half of a handshake.
#[derive(Debug)]
pub struct Responder;

impl Responder {
    /// Respond to an initiator's hello: generate an ephemeral key, derive the
    /// session, and return the established channel plus the hello to send back.
    pub fn respond(initiator_hello: &[u8]) -> Result<(Channel, Vec<u8>)> {
        let e_i_pub = parse_hello(initiator_hello)?;
        let ephemeral = AgreementSecretKey::generate()?;
        let e_r_pub = ephemeral.public_key().to_bytes();
        let peer = AgreementPublicKey::from_suite_bytes(KA_SUITE, &e_i_pub)?;
        let dh = ephemeral.agree(&peer)?;
        let responder_hello = hello_msg(&e_r_pub);
        let (k_i2r, k_r2i, binding) =
            derive_keys(initiator_hello, &responder_hello, dh.as_bytes())?;
        // Responder sends on r2i, receives on i2r.
        Ok((Channel::new(k_r2i, k_i2r, binding), responder_hello))
    }
}

/// An established encrypted channel: a confidential, forward-secret duplex.
///
/// Each direction has its own key and a monotonic counter used as the AEAD nonce,
/// so a nonce never repeats under a key. Messages must be processed in order.
#[derive(Debug)]
pub struct Channel {
    send_key: AeadKey,
    recv_key: AeadKey,
    send_ctr: u64,
    recv_ctr: u64,
    binding: [u8; 32],
}

impl Channel {
    fn new(send_key: AeadKey, recv_key: AeadKey, binding: [u8; 32]) -> Self {
        Channel {
            send_key,
            recv_key,
            send_ctr: 0,
            recv_ctr: 0,
            binding,
        }
    }

    /// The 32-byte channel binding — identical on both ends, unique per session.
    /// Higher layers sign over this to bind a signed payload to *this* channel.
    /// It prevents transcript substitution; anti-relay distance bounding is added
    /// by `mini-presence`.
    pub fn channel_binding(&self) -> [u8; 32] {
        self.binding
    }

    /// Encrypt `plaintext` for the peer with associated data `aad`. Returns the
    /// ciphertext to hand to a [`crate::Bearer`].
    pub fn seal(&mut self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        if plaintext.len() > MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(BearerError::FrameTooLarge {
                max: MAX_CHANNEL_PLAINTEXT_BYTES,
                got: plaintext.len(),
            });
        }
        if self.send_ctr == u64::MAX {
            return Err(BearerError::CounterExhausted);
        }
        let nonce = counter_nonce(self.send_ctr)?;
        let ct = self.send_key.encrypt(&nonce, plaintext, aad)?;
        self.send_ctr += 1;
        Ok(ct)
    }

    /// Decrypt and authenticate a `ciphertext` received from the peer with
    /// associated data `aad`.
    pub fn open(&mut self, ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() > MAX_CHANNEL_CIPHERTEXT_BYTES {
            return Err(BearerError::FrameTooLarge {
                max: MAX_CHANNEL_CIPHERTEXT_BYTES,
                got: ciphertext.len(),
            });
        }
        if self.recv_ctr == u64::MAX {
            return Err(BearerError::CounterExhausted);
        }
        let nonce = counter_nonce(self.recv_ctr)?;
        let pt = self.recv_key.decrypt(&nonce, ciphertext, aad)?;
        self.recv_ctr += 1;
        Ok(pt)
    }
}

fn hello_msg(ephemeral_public: &[u8; 32]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(HELLO_LEN);
    msg.push(PROTOCOL_VERSION);
    msg.push(KA_SUITE.tag());
    msg.push(KDF_SUITE.tag());
    msg.push(AEAD_SUITE.tag());
    msg.extend_from_slice(ephemeral_public);
    msg
}

fn parse_hello(msg: &[u8]) -> Result<[u8; 32]> {
    if msg.len() != HELLO_LEN {
        return Err(BearerError::BadHandshake);
    }
    if msg[0] != PROTOCOL_VERSION {
        return Err(BearerError::UnsupportedVersion(msg[0]));
    }
    if KeyAgreementSuite::from_tag(msg[1])? != KA_SUITE {
        return Err(BearerError::BadHandshake);
    }
    if KdfSuite::from_tag(msg[2])? != KDF_SUITE {
        return Err(BearerError::BadHandshake);
    }
    if AeadSuite::from_tag(msg[3])? != AEAD_SUITE {
        return Err(BearerError::BadHandshake);
    }
    let mut pk = [0u8; 32];
    pk.copy_from_slice(&msg[4..HELLO_LEN]);
    Ok(pk)
}

/// Derive the two directional AEAD keys and the channel binding from the DH
/// output, bound to the full handshake transcript.
fn derive_keys(
    initiator_hello: &[u8],
    responder_hello: &[u8],
    dh: &[u8; 32],
) -> Result<(AeadKey, AeadKey, [u8; 32])> {
    let mut info = Vec::with_capacity(
        TRAFFIC_INFO.len() + initiator_hello.len() + responder_hello.len(),
    );
    info.extend_from_slice(TRAFFIC_INFO);
    info.extend_from_slice(initiator_hello);
    info.extend_from_slice(responder_hello);

    let mut okm = KDF_SUITE.derive_bytes(Some(HS_SALT), dh, &info, OKM_LEN)?;
    let k_i2r = match AeadKey::from_suite_bytes(AEAD_SUITE, &okm[0..32]) {
        Ok(key) => key,
        Err(e) => {
            okm.fill(0);
            return Err(e.into());
        }
    };
    let k_r2i = match AeadKey::from_suite_bytes(AEAD_SUITE, &okm[32..64]) {
        Ok(key) => key,
        Err(e) => {
            okm.fill(0);
            return Err(e.into());
        }
    };
    let mut binding = [0u8; 32];
    binding.copy_from_slice(&okm[64..96]);
    okm.fill(0);
    Ok((k_i2r, k_r2i, binding))
}

/// Map a monotonic counter to a 96-bit nonce: 4 zero bytes then the big-endian
/// counter. Unique per key because each direction has its own key and counter.
fn counter_nonce(counter: u64) -> Result<AeadNonce> {
    let mut n = [0u8; 12];
    n[4..12].copy_from_slice(&counter.to_be_bytes());
    AeadNonce::from_bytes(&n).map_err(BearerError::Crypto)
}
