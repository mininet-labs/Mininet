//! Mobile-facing bridge for `mini-messaging`: encrypted, route+key-scoped
//! conversations sent/read through the same [`crate::RootCore`] that already
//! owns this process's root and delegated-device signing keys.
//!
//! `mini-messaging` itself is explicit that it "does not invent key
//! distribution": every [`ConversationSecretHandle`] this module accepts is
//! a route+key pair the caller must already hold from an out-of-band
//! authenticated exchange (today: `ConversationSecret::key_bytes_for_local_vault`
//! copied out of one process and into another's OS-protected vault by the
//! app itself, e.g. for a user's own multi-device conversation; a real
//! pairwise session-establishment protocol is future work, same as
//! `mini-messaging`'s own doc comment already says). This boundary does not
//! widen that: it does not generate, exchange, or store secrets on Rust's
//! own initiative.
//!
//! `signature_verified` on a scanned message is **only ever computed
//! against one of this process's own currently-known devices' KELs**
//! (`state.devices`, matched by the message's claimed `author_device`),
//! and only when the message also claims `author_human` equal to this
//! process's own root. A message decrypts successfully whenever the caller
//! holds the right conversation key, regardless of who wrote it
//! (`mini-messaging::scan`'s own doc: "decryption alone does not
//! authenticate device delegation"). For a message this device is
//! reading in a conversation scoped to *our own* root+devices (the "control
//! your own delegated devices" case `RootCore`'s device-delegation methods
//! already support), that check is exactly the right one -- it can
//! recognize a genuinely still-delegated device of *this* root. It cannot,
//! and does not claim to, authenticate a message from a different person's
//! root; that needs their KEL, which this module has no way to fetch yet.

use did_mini::{Controller, Did, Kel};
use mini_messaging::{
    ConversationSecret, MessageDraft, MessageKind, ReceiptState, ReceivedMessage,
};
use mini_objects::{ObjectEnvelopeV2, ObjectId, OpaqueRoute};
use mini_store::{MemoryBackend, Store};

use crate::{PersistReader, RootCore};

const ROUTE_LEN: usize = 32;
const KEY_LEN: usize = 32;
const MAX_ENVELOPES: usize = 4096;
const MAX_ENVELOPE_BYTES: usize = 1 << 20;
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_ATTACHMENTS: usize = 32;
const MAX_OBJECT_ID_BYTES: usize = 256;

/// A conversation's opaque storage route bound to its symmetric key, as
/// raw bytes crossing the FFI boundary. See this module's doc comment for
/// what establishing one of these safely actually requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSecretHandle {
    pub route: Vec<u8>,
    pub key: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKindInput {
    Text,
    System,
    ReceiptDelivered,
    ReceiptRead,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageDraftInput {
    pub kind: MessageKindInput,
    pub body: String,
    pub reply_to: Option<String>,
    pub attachments: Vec<String>,
    pub receipt_for: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedMessageView {
    pub envelope_id: String,
    pub author_human: String,
    pub author_device: String,
    pub timestamp_ms: u64,
    pub sequence: u64,
    pub kind: MessageKindInput,
    pub body: String,
    pub reply_to: Option<String>,
    pub attachments: Vec<String>,
    pub receipt_for: Option<String>,
    /// See this module's top-level doc comment: only ever true for a
    /// message claiming to be authored by *this process's own root* and
    /// one of its own currently-known devices, verified against that
    /// device's own current KEL.
    pub signature_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationScanView {
    pub messages: Vec<ReceivedMessageView>,
    pub rejected: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessagingFfiError {
    NoRoot,
    NoDevice,
    InvalidSecret,
    InvalidMessage,
    LimitExceeded,
    CorruptState,
    Protocol(String),
}

impl core::fmt::Display for MessagingFfiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoRoot => f.write_str("create or restore an identity before messaging"),
            Self::NoDevice => f.write_str("no delegated device is available to sign messages"),
            Self::InvalidSecret => f.write_str("conversation route/key must each be 32 bytes"),
            Self::InvalidMessage => f.write_str("invalid message draft or object id"),
            Self::LimitExceeded => f.write_str("messaging state capacity reached"),
            Self::CorruptState => f.write_str("persisted messaging state is corrupt"),
            Self::Protocol(message) => {
                write!(f, "messaging protocol rejected the request: {message}")
            }
        }
    }
}

impl std::error::Error for MessagingFfiError {}

fn messaging_err(error: mini_messaging::MessagingError) -> MessagingFfiError {
    match error {
        mini_messaging::MessagingError::InvalidMessage => MessagingFfiError::InvalidMessage,
        mini_messaging::MessagingError::LimitExceeded => MessagingFfiError::LimitExceeded,
        other => MessagingFfiError::Protocol(other.to_string()),
    }
}

/// Per-`RootCore` messaging state. Envelopes are the same encrypted bytes
/// `mini-messaging` seals -- this module never stores plaintext -- and are
/// carried inside `RootCore::persist_state`'s existing ciphertext, the same
/// way `pairing::PairingState`'s follow objects already are, rather than a
/// second, separately-keyed storage location.
#[derive(Debug, Default)]
pub(super) struct MessagingState {
    envelopes: Vec<Vec<u8>>,
    next_sequence: u64,
}

fn to_secret(handle: &ConversationSecretHandle) -> Result<ConversationSecret, MessagingFfiError> {
    let route: [u8; ROUTE_LEN] = handle
        .route
        .as_slice()
        .try_into()
        .map_err(|_| MessagingFfiError::InvalidSecret)?;
    let key: [u8; KEY_LEN] = handle
        .key
        .as_slice()
        .try_into()
        .map_err(|_| MessagingFfiError::InvalidSecret)?;
    ConversationSecret::from_local_vault(OpaqueRoute::from_bytes(route), key).map_err(messaging_err)
}

fn to_object_id(text: &str) -> Result<ObjectId, MessagingFfiError> {
    if text.len() > MAX_OBJECT_ID_BYTES {
        return Err(MessagingFfiError::InvalidMessage);
    }
    ObjectId::parse(text).map_err(|_| MessagingFfiError::InvalidMessage)
}

fn to_draft(input: MessageDraftInput) -> Result<MessageDraft, MessagingFfiError> {
    if input.body.len() > MAX_BODY_BYTES || input.attachments.len() > MAX_ATTACHMENTS {
        return Err(MessagingFfiError::LimitExceeded);
    }
    let reply_to = input.reply_to.as_deref().map(to_object_id).transpose()?;
    let attachments = input
        .attachments
        .iter()
        .map(|id| to_object_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let receipt_for = input.receipt_for.as_deref().map(to_object_id).transpose()?;
    let kind = match input.kind {
        MessageKindInput::Text => MessageKind::Text,
        MessageKindInput::System => MessageKind::System,
        MessageKindInput::ReceiptDelivered => MessageKind::Receipt(ReceiptState::Delivered),
        MessageKindInput::ReceiptRead => MessageKind::Receipt(ReceiptState::Read),
    };
    if matches!(kind, MessageKind::Receipt(_)) && receipt_for.is_none() {
        return Err(MessagingFfiError::InvalidMessage);
    }
    Ok(MessageDraft {
        kind,
        body: input.body,
        reply_to,
        attachments,
        receipt_for,
    })
}

fn from_kind(kind: MessageKind) -> MessageKindInput {
    match kind {
        MessageKind::Text => MessageKindInput::Text,
        MessageKind::System => MessageKindInput::System,
        MessageKind::Receipt(ReceiptState::Delivered) => MessageKindInput::ReceiptDelivered,
        MessageKind::Receipt(ReceiptState::Read) => MessageKindInput::ReceiptRead,
    }
}

fn view_from_received(
    message: ReceivedMessage,
    own_root_did: Option<&Did>,
    own_device_kel: Option<&Kel>,
) -> ReceivedMessageView {
    // `verify_signature` checks the signature against the claimed *device*'s
    // own KEL (`PrivateObject::verify_signature`'s own contract -- it
    // rejects outright if the KEL's DID doesn't match `author_device`), not
    // the root's. `own_device_kel` is already looked up by the caller to be
    // one of this root's own currently-known devices matching
    // `message.author_device`; requiring `own_root_did == author_human` on
    // top of that keeps this restricted to "my own root talking to my own
    // devices", not just "some device I happen to also custody".
    let signature_verified = match (own_root_did, own_device_kel) {
        (Some(root_did), Some(kel)) if &message.author_human == root_did => {
            message.verify_signature(kel).is_ok()
        }
        _ => false,
    };
    ReceivedMessageView {
        envelope_id: message.envelope_id.as_str().to_string(),
        author_human: message.author_human.as_str().to_string(),
        author_device: message.author_device.as_str().to_string(),
        timestamp_ms: message.timestamp_ms,
        sequence: message.sequence,
        kind: from_kind(message.kind),
        body: message.body,
        reply_to: message.reply_to.map(|id| id.as_str().to_string()),
        attachments: message
            .attachments
            .into_iter()
            .map(|id| id.as_str().to_string())
            .collect(),
        receipt_for: message.receipt_for.map(|id| id.as_str().to_string()),
        signature_verified,
    }
}

/// Rebuild an ephemeral in-memory store from this `RootCore`'s persisted
/// envelope bytes. `mini-store`'s `MemoryBackend` is cheap enough that
/// replaying the whole (bounded, `MAX_ENVELOPES`-capped) history per call is
/// simpler and easier to review than keeping a long-lived store handle
/// alive across the `std::sync::Mutex<RootState>` boundary.
fn rebuild_store(envelopes: &[Vec<u8>]) -> Result<Store<MemoryBackend>, MessagingFfiError> {
    let mut store = Store::new(MemoryBackend::new());
    for bytes in envelopes {
        let envelope =
            ObjectEnvelopeV2::from_bytes(bytes).map_err(|_| MessagingFfiError::CorruptState)?;
        store
            .insert_private(&envelope)
            .map_err(|_| MessagingFfiError::CorruptState)?;
    }
    Ok(store)
}

impl RootCore {
    /// Seal `draft` into this conversation's route under this process's
    /// root+device identity, and persist the resulting encrypted envelope.
    /// Returns the new envelope's id.
    pub fn send_message(
        &self,
        secret: ConversationSecretHandle,
        timestamp_ms: u64,
        draft: MessageDraftInput,
    ) -> Result<String, MessagingFfiError> {
        let secret = to_secret(&secret)?;
        let draft = to_draft(draft)?;
        let mut guard = self.lock();
        // Split-borrow distinct fields directly (rather than calling a
        // `&self`/`&mut self` helper) so `messaging` stays mutably
        // borrowable while `root`/`devices` are only read -- and so
        // `device` never needs `Controller::clone` (deliberately
        // unimplemented: it would let secret key material be duplicated
        // in memory without a good reason).
        let crate::RootState {
            root,
            devices,
            messaging,
            ..
        } = &mut *guard;
        let author_human = root.as_ref().ok_or(MessagingFfiError::NoRoot)?.did();
        let device = devices.first().ok_or(MessagingFfiError::NoDevice)?;
        if messaging.envelopes.len() >= MAX_ENVELOPES {
            return Err(MessagingFfiError::LimitExceeded);
        }
        let mut store = rebuild_store(&messaging.envelopes)?;
        let sequence = messaging.next_sequence;
        let id = mini_messaging::send(
            &mut store,
            &secret,
            author_human,
            device,
            timestamp_ms,
            sequence,
            draft,
        )
        .map_err(messaging_err)?;
        let envelope = store
            .get_private(&id)
            .map_err(|_| MessagingFfiError::CorruptState)?;
        let bytes = envelope.to_bytes();
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(MessagingFfiError::LimitExceeded);
        }
        messaging.envelopes.push(bytes);
        messaging.next_sequence = sequence
            .checked_add(1)
            .ok_or(MessagingFfiError::LimitExceeded)?;
        Ok(id.as_str().to_string())
    }

    /// Decrypt every persisted envelope this conversation's secret can open.
    /// Full re-scan each call, matching `mini-messaging::scan`'s own model
    /// (no cursor/incremental API exists yet).
    pub fn scan_conversation(
        &self,
        secret: ConversationSecretHandle,
    ) -> Result<ConversationScanView, MessagingFfiError> {
        let secret = to_secret(&secret)?;
        let state = self.lock();
        let store = rebuild_store(&state.messaging.envelopes)?;
        let scan = mini_messaging::scan(&store, &secret).map_err(messaging_err)?;
        let own_root_did = state.root.as_ref().map(Controller::did);
        let messages = scan
            .messages
            .into_iter()
            .map(|message| {
                let own_device_kel = state
                    .devices
                    .iter()
                    .find(|device| device.did() == message.author_device)
                    .map(Controller::kel);
                view_from_received(message, own_root_did.as_ref(), own_device_kel.as_ref())
            })
            .collect();
        let rejected = scan
            .rejected
            .into_iter()
            .map(|id| id.as_str().to_string())
            .collect();
        Ok(ConversationScanView { messages, rejected })
    }
}

pub(super) fn encode_messaging_state(out: &mut Vec<u8>, state: &MessagingState) {
    out.extend_from_slice(&state.next_sequence.to_le_bytes());
    out.extend_from_slice(&(state.envelopes.len() as u32).to_le_bytes());
    for envelope in &state.envelopes {
        out.extend_from_slice(&(envelope.len() as u32).to_le_bytes());
        out.extend_from_slice(envelope);
    }
}

pub(super) fn decode_messaging_state(
    reader: &mut PersistReader<'_>,
) -> Result<MessagingState, crate::RootError> {
    let next_sequence = {
        let bytes = reader.take(8)?;
        u64::from_le_bytes(
            bytes
                .try_into()
                .map_err(|_| crate::RootError::CorruptState)?,
        )
    };
    let envelope_count = reader.u32()? as usize;
    if envelope_count > MAX_ENVELOPES {
        return Err(crate::RootError::CorruptState);
    }
    let mut envelopes = Vec::with_capacity(envelope_count);
    for _ in 0..envelope_count {
        let len = reader.u32()? as usize;
        if len > MAX_ENVELOPE_BYTES {
            return Err(crate::RootError::CorruptState);
        }
        let bytes = reader.take(len)?.to_vec();
        ObjectEnvelopeV2::from_bytes(&bytes).map_err(|_| crate::RootError::CorruptState)?;
        envelopes.push(bytes);
    }
    Ok(MessagingState {
        envelopes,
        next_sequence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StorageCipher, StorageCipherError};

    #[derive(Debug)]
    struct XorCipher(u8);

    impl StorageCipher for XorCipher {
        fn encrypt(&self, plaintext: Vec<u8>) -> Result<Vec<u8>, StorageCipherError> {
            Ok(plaintext.into_iter().map(|b| b ^ self.0).collect())
        }
        fn decrypt(&self, ciphertext: Vec<u8>) -> Result<Vec<u8>, StorageCipherError> {
            Ok(ciphertext.into_iter().map(|b| b ^ self.0).collect())
        }
    }

    fn secret() -> ConversationSecretHandle {
        ConversationSecretHandle {
            route: vec![7u8; ROUTE_LEN],
            key: vec![9u8; KEY_LEN],
        }
    }

    #[test]
    fn send_then_scan_round_trips_a_text_message_with_verified_signature() {
        let core = RootCore::new();
        core.create_root().unwrap();
        core.create_device().unwrap();

        let id = core
            .send_message(
                secret(),
                1_000,
                MessageDraftInput {
                    kind: MessageKindInput::Text,
                    body: "hello from my other device".to_string(),
                    reply_to: None,
                    attachments: Vec::new(),
                    receipt_for: None,
                },
            )
            .unwrap();

        let scan = core.scan_conversation(secret()).unwrap();
        assert_eq!(scan.rejected, Vec::<String>::new());
        assert_eq!(scan.messages.len(), 1);
        let message = &scan.messages[0];
        assert_eq!(message.envelope_id, id);
        assert_eq!(message.body, "hello from my other device");
        assert_eq!(message.kind, MessageKindInput::Text);
        assert!(
            message.signature_verified,
            "a message this same RootCore just signed for its own root must verify"
        );
    }

    #[test]
    fn wrong_key_scan_rejects_without_reading_plaintext() {
        let core = RootCore::new();
        core.create_root().unwrap();
        core.create_device().unwrap();
        core.send_message(
            secret(),
            1_000,
            MessageDraftInput {
                kind: MessageKindInput::Text,
                body: "secret".to_string(),
                reply_to: None,
                attachments: Vec::new(),
                receipt_for: None,
            },
        )
        .unwrap();

        let mut wrong = secret();
        wrong.key = vec![1u8; KEY_LEN];
        let scan = core.scan_conversation(wrong).unwrap();
        assert!(scan.messages.is_empty());
        assert_eq!(scan.rejected.len(), 1);
    }

    #[test]
    fn invalid_secret_length_is_rejected() {
        let core = RootCore::new();
        core.create_root().unwrap();
        core.create_device().unwrap();
        let bad = ConversationSecretHandle {
            route: vec![1u8; 4],
            key: vec![2u8; KEY_LEN],
        };
        assert_eq!(
            core.send_message(
                bad,
                1_000,
                MessageDraftInput {
                    kind: MessageKindInput::Text,
                    body: String::new(),
                    reply_to: None,
                    attachments: Vec::new(),
                    receipt_for: None,
                },
            ),
            Err(MessagingFfiError::InvalidSecret)
        );
    }

    #[test]
    fn sending_without_a_root_is_rejected() {
        let core = RootCore::new();
        assert_eq!(
            core.send_message(
                secret(),
                1_000,
                MessageDraftInput {
                    kind: MessageKindInput::Text,
                    body: "x".to_string(),
                    reply_to: None,
                    attachments: Vec::new(),
                    receipt_for: None,
                },
            ),
            Err(MessagingFfiError::NoRoot)
        );
    }

    #[test]
    fn persisting_and_restoring_round_trips_conversation_history() {
        let core = RootCore::new();
        core.create_root().unwrap();
        core.create_device().unwrap();
        core.send_message(
            secret(),
            1_000,
            MessageDraftInput {
                kind: MessageKindInput::Text,
                body: "durable".to_string(),
                reply_to: None,
                attachments: Vec::new(),
                receipt_for: None,
            },
        )
        .unwrap();

        let blob = core.persist_state(Box::new(XorCipher(0x5A))).unwrap();
        let restored = RootCore::restore(blob, Box::new(XorCipher(0x5A))).unwrap();

        let scan = restored.scan_conversation(secret()).unwrap();
        assert_eq!(scan.messages.len(), 1);
        assert_eq!(scan.messages[0].body, "durable");
    }

    #[test]
    fn receipt_without_a_target_is_rejected() {
        let core = RootCore::new();
        core.create_root().unwrap();
        core.create_device().unwrap();
        assert_eq!(
            core.send_message(
                secret(),
                1_000,
                MessageDraftInput {
                    kind: MessageKindInput::ReceiptDelivered,
                    body: String::new(),
                    reply_to: None,
                    attachments: Vec::new(),
                    receipt_for: None,
                },
            ),
            Err(MessagingFfiError::InvalidMessage)
        );
    }
}
