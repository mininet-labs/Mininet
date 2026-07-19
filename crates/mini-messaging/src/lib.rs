//! Private messaging over Mininet's opaque v2 object envelopes.
//!
//! This crate owns message semantics and encrypted persistence. It does not
//! invent key distribution: callers must supply an already-authenticated
//! conversation key and opaque route. A production client must obtain those
//! through the future pairwise prekey/session protocol, not by sending raw
//! keys through an unauthenticated channel.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use did_mini::{Controller, Did, Kel};
use mini_crypto::{AeadKey, AeadSuite, HashAlgorithm};
use mini_objects::{
    Link, ObjectEnvelopeV2, ObjectError, ObjectId, ObjectType, OpaqueRoute, PrivateObject,
    RetentionClass, StorageDescriptor,
};
use mini_store::{Backend, Store, StoreError};

const MESSAGE_TYPE: &str = "mininet/private-message/v1";
const PAYLOAD_VERSION: u8 = 1;
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_ATTACHMENTS: usize = 32;

/// An already-established conversation secret. Construction is explicit to
/// keep this crate from implying that key exchange has happened safely.
#[derive(Debug)]
pub struct ConversationSecret {
    route: OpaqueRoute,
    key: AeadKey,
}

impl ConversationSecret {
    /// Bind a conversation's opaque storage route to its symmetric key.
    /// Callers are responsible for authenticated key establishment.
    pub fn established(route: OpaqueRoute, key: AeadKey) -> Self {
        Self { route, key }
    }

    /// The opaque route shared with blind storage nodes.
    pub fn route(&self) -> OpaqueRoute {
        self.route
    }

    /// Generate a random conversation capability for beta invitation flows.
    /// This is not a prekey or ratcheted session.
    pub fn generate_beta() -> Result<Self> {
        Ok(Self {
            route: OpaqueRoute::random()?,
            key: AeadKey::generate(AeadSuite::DEFAULT)
                .map_err(ObjectError::Crypto)
                .map_err(MessagingError::Object)?,
        })
    }

    /// Export a beta invitation containing the route and key. Anyone holding
    /// this value can read and write conversation ciphertext; transfer it only
    /// through a trusted channel and protect it at rest.
    pub fn beta_invite(&self, inviter: Did) -> BetaInvite {
        BetaInvite {
            inviter,
            route: self.route,
            key_bytes: self.key.to_key_bytes(),
        }
    }

    /// Reconstruct the secret imported from a beta invitation.
    pub fn from_beta_invite(invite: &BetaInvite) -> Result<Self> {
        Ok(Self {
            route: invite.route,
            key: AeadKey::from_suite_bytes(AeadSuite::DEFAULT, &invite.key_bytes)
                .map_err(ObjectError::Crypto)?,
        })
    }

    /// Export key bytes for a local OS-protected vault. This must never be
    /// written to an unprotected settings or log file.
    pub fn key_bytes_for_local_vault(&self) -> [u8; 32] {
        self.key.to_key_bytes()
    }

    /// Restore a conversation capability from bytes obtained from an
    /// OS-protected local vault.
    pub fn from_local_vault(route: OpaqueRoute, key_bytes: [u8; 32]) -> Result<Self> {
        Ok(Self {
            route,
            key: AeadKey::from_suite_bytes(AeadSuite::DEFAULT, &key_bytes)
                .map_err(ObjectError::Crypto)?,
        })
    }
}

/// Portable beta conversation capability. Its text form includes a checksum
/// for copy/file corruption, not authentication. The inviter DID is a label
/// until messages are verified against current identity provenance.
#[derive(Clone, PartialEq, Eq)]
pub struct BetaInvite {
    inviter: Did,
    route: OpaqueRoute,
    key_bytes: [u8; 32],
}

impl core::fmt::Debug for BetaInvite {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BetaInvite")
            .field("inviter", &self.inviter)
            .field("route", &self.route)
            .field("key_bytes", &"[REDACTED]")
            .finish()
    }
}

impl BetaInvite {
    /// Claimed inviter DID. Verify received messages before treating it as an
    /// authenticated contact identity.
    pub fn inviter(&self) -> &Did {
        &self.inviter
    }

    /// Opaque route capability used by private sync.
    pub fn route(&self) -> OpaqueRoute {
        self.route
    }

    /// Versioned printable form suitable for a protected file or trusted
    /// out-of-band transfer. The key is present in this string.
    pub fn to_code(&self) -> String {
        let route = hex_encode(self.route.as_bytes());
        let key = hex_encode(&self.key_bytes);
        let checksum = invite_checksum(&self.route, &self.key_bytes, &self.inviter);
        format!(
            "mini-invite-v1.{route}.{key}.{}.{}",
            self.inviter.as_str(),
            hex_encode(&checksum)
        )
    }

    /// Strictly parse and checksum a beta invitation code.
    pub fn parse(code: &str) -> Result<Self> {
        if code.len() > 1024 {
            return Err(MessagingError::InvalidInvite);
        }
        let parts: Vec<&str> = code.trim().split('.').collect();
        if parts.len() != 5 || parts[0] != "mini-invite-v1" {
            return Err(MessagingError::InvalidInvite);
        }
        let route = OpaqueRoute::from_bytes(hex_decode_32(parts[1])?);
        let key_bytes = hex_decode_32(parts[2])?;
        let inviter = Did::parse(parts[3]).map_err(|_| MessagingError::InvalidInvite)?;
        let checksum = hex_decode_8(parts[4])?;
        if checksum != invite_checksum(&route, &key_bytes, &inviter) {
            return Err(MessagingError::InvalidInvite);
        }
        Ok(Self {
            inviter,
            route,
            key_bytes,
        })
    }
}

/// Semantic message category encoded inside the encrypted envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// Human-authored UTF-8 text.
    Text,
    /// Conversation event text generated by a client or protocol.
    System,
    /// Delivery/read acknowledgement for another message.
    Receipt(ReceiptState),
}

/// Acknowledgement state for a receipt message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptState {
    /// Recipient device accepted the encrypted envelope.
    Delivered,
    /// Recipient explicitly opened the conversation/message.
    Read,
}

/// A message to seal and persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageDraft {
    /// Message semantics.
    pub kind: MessageKind,
    /// UTF-8 body for text/system messages; empty for receipts.
    pub body: String,
    /// Optional encrypted reply relationship.
    pub reply_to: Option<ObjectId>,
    /// Encrypted links to attachment manifests/chunks.
    pub attachments: Vec<ObjectId>,
    /// Receipt target when `kind` is [`MessageKind::Receipt`].
    pub receipt_for: Option<ObjectId>,
}

impl MessageDraft {
    /// Make an ordinary text message.
    pub fn text(body: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::Text,
            body: body.into(),
            reply_to: None,
            attachments: Vec::new(),
            receipt_for: None,
        }
    }

    /// Make a delivery/read receipt.
    pub fn receipt(target: ObjectId, state: ReceiptState) -> Self {
        Self {
            kind: MessageKind::Receipt(state),
            body: String::new(),
            reply_to: None,
            attachments: Vec::new(),
            receipt_for: Some(target),
        }
    }
}

/// One decrypted message. Authenticity is not implicit: call
/// [`ReceivedMessage::verify_signature`] with the author's device KEL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedMessage {
    /// Content id of the encrypted outer envelope.
    pub envelope_id: ObjectId,
    /// Claimed human author from the signed private object.
    pub author_human: Did,
    /// Claimed signing device from the signed private object.
    pub author_device: Did,
    /// Author-claimed time, used only as an ordering hint.
    pub timestamp_ms: u64,
    /// Author-scoped sequence, used only as an ordering hint.
    pub sequence: u64,
    /// Message semantics.
    pub kind: MessageKind,
    /// Decrypted UTF-8 body.
    pub body: String,
    /// Decrypted reply relationship.
    pub reply_to: Option<ObjectId>,
    /// Decrypted attachment relationships.
    pub attachments: Vec<ObjectId>,
    /// Decrypted receipt target.
    pub receipt_for: Option<ObjectId>,
    signed_object: PrivateObject,
}

impl ReceivedMessage {
    /// Verify the private object's device signature. Device delegation and
    /// revocation/provenance remain a separate identity-layer check.
    pub fn verify_signature(&self, device_kel: &Kel) -> Result<()> {
        self.signed_object.verify_signature(device_kel)?;
        Ok(())
    }
}

/// Result of scanning one opaque route. Invalid, undecryptable, or unrelated
/// envelopes are reported instead of making the whole conversation unreadable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationScan {
    /// Decrypted messages in deterministic display order.
    pub messages: Vec<ReceivedMessage>,
    /// Envelope ids that could not be accepted with this secret/schema.
    pub rejected: Vec<ObjectId>,
}

/// Seal and persist one message, returning its encrypted envelope id.
#[allow(clippy::too_many_arguments)]
pub fn send<B: Backend>(
    store: &mut Store<B>,
    secret: &ConversationSecret,
    author_human: Did,
    author_device: &Controller,
    timestamp_ms: u64,
    sequence: u64,
    draft: MessageDraft,
) -> Result<ObjectId> {
    validate_draft(&draft)?;
    let links = draft_links(&draft);
    let payload = encode_payload(draft.kind, &draft.body);
    let private = PrivateObject::new(
        ObjectType::Custom(MESSAGE_TYPE.to_string()),
        author_human,
        author_device.did(),
        timestamp_ms,
        sequence,
        links,
        Vec::new(),
        payload,
    )
    .sign_with(author_device);
    let envelope = ObjectEnvelopeV2::seal(
        &private,
        &secret.key,
        secret.route,
        StorageDescriptor {
            retention: RetentionClass::Standard,
        },
    )?;
    let id = envelope.id().clone();
    store.insert_private(&envelope)?;
    Ok(id)
}

/// Read every envelope currently indexed under this conversation route.
/// Decryption alone does not authenticate device delegation; callers should
/// verify each accepted message against current identity state.
pub fn scan<B: Backend>(store: &Store<B>, secret: &ConversationSecret) -> Result<ConversationScan> {
    let mut messages = Vec::new();
    let mut rejected = Vec::new();
    for id in store.private_by_route(&secret.route)? {
        let envelope = match store.get_private(&id) {
            Ok(envelope) => envelope,
            Err(StoreError::Object(_) | StoreError::Corrupt | StoreError::NotFound) => {
                rejected.push(id);
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let accepted = if envelope.route() != secret.route {
            None
        } else {
            envelope
                .open(&secret.key)
                .ok()
                .and_then(|private| decode_message(id.clone(), private).ok())
        };
        match accepted {
            Some(message) => messages.push(message),
            None => rejected.push(id),
        }
    }
    messages.sort_by(|a, b| {
        (a.timestamp_ms, a.sequence, a.envelope_id.as_str()).cmp(&(
            b.timestamp_ms,
            b.sequence,
            b.envelope_id.as_str(),
        ))
    });
    Ok(ConversationScan { messages, rejected })
}

fn validate_draft(draft: &MessageDraft) -> Result<()> {
    if draft.body.len() > MAX_BODY_BYTES || draft.attachments.len() > MAX_ATTACHMENTS {
        return Err(MessagingError::LimitExceeded);
    }
    match draft.kind {
        MessageKind::Text | MessageKind::System => {
            if draft.body.is_empty() || draft.receipt_for.is_some() {
                return Err(MessagingError::InvalidMessage);
            }
        }
        MessageKind::Receipt(_) => {
            if !draft.body.is_empty()
                || draft.receipt_for.is_none()
                || draft.reply_to.is_some()
                || !draft.attachments.is_empty()
            {
                return Err(MessagingError::InvalidMessage);
            }
        }
    }
    Ok(())
}

fn draft_links(draft: &MessageDraft) -> Vec<Link> {
    let mut links = Vec::new();
    if let Some(target) = &draft.reply_to {
        links.push(Link {
            rel: "reply".to_string(),
            target: target.clone(),
        });
    }
    for target in &draft.attachments {
        links.push(Link {
            rel: "attachment".to_string(),
            target: target.clone(),
        });
    }
    if let Some(target) = &draft.receipt_for {
        links.push(Link {
            rel: "receipt".to_string(),
            target: target.clone(),
        });
    }
    links
}

fn encode_payload(kind: MessageKind, body: &str) -> Vec<u8> {
    let (kind_tag, receipt_tag) = match kind {
        MessageKind::Text => (1, 0),
        MessageKind::System => (2, 0),
        MessageKind::Receipt(ReceiptState::Delivered) => (3, 1),
        MessageKind::Receipt(ReceiptState::Read) => (3, 2),
    };
    let mut out = Vec::with_capacity(7 + body.len());
    out.push(PAYLOAD_VERSION);
    out.push(kind_tag);
    out.push(receipt_tag);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body.as_bytes());
    out
}

fn decode_message(envelope_id: ObjectId, private: PrivateObject) -> Result<ReceivedMessage> {
    if private.object_type != ObjectType::Custom(MESSAGE_TYPE.to_string()) {
        return Err(MessagingError::InvalidMessage);
    }
    let (kind, body) = decode_payload(&private.payload)?;
    let mut reply_to = None;
    let mut receipt_for = None;
    let mut attachments = Vec::new();
    for link in &private.links {
        match link.rel.as_str() {
            "reply" if reply_to.is_none() => reply_to = Some(link.target.clone()),
            "attachment" if attachments.len() < MAX_ATTACHMENTS => {
                attachments.push(link.target.clone())
            }
            "receipt" if receipt_for.is_none() => receipt_for = Some(link.target.clone()),
            _ => return Err(MessagingError::InvalidMessage),
        }
    }
    let draft = MessageDraft {
        kind,
        body: body.clone(),
        reply_to: reply_to.clone(),
        attachments: attachments.clone(),
        receipt_for: receipt_for.clone(),
    };
    validate_draft(&draft)?;
    Ok(ReceivedMessage {
        envelope_id,
        author_human: private.author_human.clone(),
        author_device: private.author_device.clone(),
        timestamp_ms: private.timestamp_ms,
        sequence: private.sequence,
        kind,
        body,
        reply_to,
        attachments,
        receipt_for,
        signed_object: private,
    })
}

fn decode_payload(bytes: &[u8]) -> Result<(MessageKind, String)> {
    if bytes.len() < 7 || bytes[0] != PAYLOAD_VERSION {
        return Err(MessagingError::InvalidMessage);
    }
    let kind = match (bytes[1], bytes[2]) {
        (1, 0) => MessageKind::Text,
        (2, 0) => MessageKind::System,
        (3, 1) => MessageKind::Receipt(ReceiptState::Delivered),
        (3, 2) => MessageKind::Receipt(ReceiptState::Read),
        _ => return Err(MessagingError::InvalidMessage),
    };
    let len = u32::from_be_bytes(bytes[3..7].try_into().expect("four-byte slice")) as usize;
    if len > MAX_BODY_BYTES || bytes.len() != 7 + len {
        return Err(MessagingError::InvalidMessage);
    }
    let body =
        String::from_utf8(bytes[7..].to_vec()).map_err(|_| MessagingError::InvalidMessage)?;
    Ok((kind, body))
}

fn invite_checksum(route: &OpaqueRoute, key: &[u8; 32], inviter: &Did) -> [u8; 8] {
    let mut bytes = Vec::with_capacity(64 + inviter.as_str().len());
    bytes.extend_from_slice(route.as_bytes());
    bytes.extend_from_slice(key);
    bytes.extend_from_slice(inviter.as_str().as_bytes());
    let digest = HashAlgorithm::Blake3.digest(&bytes);
    digest[..8]
        .try_into()
        .expect("an eight-byte digest prefix has exact length")
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn hex_decode_32(value: &str) -> Result<[u8; 32]> {
    hex_decode(value)?
        .try_into()
        .map_err(|_| MessagingError::InvalidInvite)
}

fn hex_decode_8(value: &str) -> Result<[u8; 8]> {
    hex_decode(value)?
        .try_into()
        .map_err(|_| MessagingError::InvalidInvite)
}

fn hex_decode(value: &str) -> Result<Vec<u8>> {
    if value.len() % 2 != 0 {
        return Err(MessagingError::InvalidInvite);
    }
    let mut decoded = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let text = core::str::from_utf8(pair).map_err(|_| MessagingError::InvalidInvite)?;
        decoded.push(u8::from_str_radix(text, 16).map_err(|_| MessagingError::InvalidInvite)?);
    }
    Ok(decoded)
}

/// Why a private messaging operation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum MessagingError {
    /// Object encryption, decoding, or signature verification failed.
    Object(ObjectError),
    /// Persistence or private-route lookup failed.
    Store(StoreError),
    /// Message fields do not match the versioned schema.
    InvalidMessage,
    /// A bounded field exceeded its protocol limit.
    LimitExceeded,
    /// A beta invite was malformed or failed its copy-integrity checksum.
    InvalidInvite,
}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, MessagingError>;

impl core::fmt::Display for MessagingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MessagingError::Object(error) => write!(f, "private message object: {error}"),
            MessagingError::Store(error) => write!(f, "private message store: {error}"),
            MessagingError::InvalidMessage => write!(f, "invalid private message"),
            MessagingError::LimitExceeded => write!(f, "private message limit exceeded"),
            MessagingError::InvalidInvite => write!(f, "invalid beta conversation invite"),
        }
    }
}

impl std::error::Error for MessagingError {}

impl From<ObjectError> for MessagingError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}

impl From<StoreError> for MessagingError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}
