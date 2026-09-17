//! Bounded, versioned IPC contract between the Mininet desktop renderer and
//! the per-user application core.
//!
//! The protocol is deliberately dependency-free and capability-shaped. There
//! is no generic "execute" command, no unbounded collection, and no frame is
//! allocated before its declared length has been checked. The first transport
//! is a child process over stdin/stdout; the contract itself is transport
//! independent so Windows named pipes can replace that transport later without
//! changing application commands.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use std::io::{Read, Write};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_OPERATION_ID_BYTES: usize = 96;
pub const MAX_POST_BYTES: usize = 16 * 1024;
pub const MAX_PROFILE_NAME_BYTES: usize = 64;
pub const MAX_PROFILE_BIO_BYTES: usize = 1024;
pub const MAX_FEED_ITEMS: u16 = 100;
pub const MAX_EVENTS_PER_RESPONSE: u16 = 64;
const MAX_DID_BYTES: usize = 256;
const MAX_OBJECT_ID_BYTES: usize = 256;
const MAX_ERROR_BYTES: usize = 4096;
const MAX_REASONABLE_TEXT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedOrder {
    Chronological,
    MostSupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedScope {
    Following,
    Everyone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedReason {
    Own,
    Followed,
    Received,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Profile,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileView {
    pub did: String,
    pub display_name: String,
    pub bio: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStatus {
    pub root_created: bool,
    pub identity_unlocked: bool,
    pub human_did: Option<String>,
    pub profile: Option<ProfileView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedCard {
    pub id: String,
    pub author: String,
    pub did: String,
    pub body: String,
    pub timestamp_ms: u64,
    pub reason: FeedReason,
    pub support_count: u32,
    pub comment_count: u32,
    pub media: Option<String>,
    pub own: bool,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedObject {
    pub kind: ObjectKind,
    pub object_id: String,
    /// True when the service returned an already committed mutation for the
    /// same operation id rather than signing a second object.
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEvent {
    pub event_id: u64,
    pub kind: ServiceEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceEventKind {
    RootCreated { did: String },
    IdentityChanged { unlocked: bool },
    ObjectPublished { kind: ObjectKind, object_id: String },
    FeedChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub version: u16,
    pub request_id: u64,
    pub command: Command,
}

impl Request {
    pub fn new(request_id: u64, command: Command) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            command,
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.version != PROTOCOL_VERSION {
            return Err(ProtocolError::InvalidRequest(format!(
                "unsupported protocol version {}",
                self.version
            )));
        }
        match &self.command {
            Command::PublishProfile {
                operation_id,
                display_name,
                bio,
            } => {
                validate_operation_id(operation_id)?;
                if display_name.is_empty() || display_name.len() > MAX_PROFILE_NAME_BYTES {
                    return Err(ProtocolError::LimitExceeded("profile display name"));
                }
                if bio.len() > MAX_PROFILE_BIO_BYTES {
                    return Err(ProtocolError::LimitExceeded("profile bio"));
                }
            }
            Command::PublishPost { operation_id, text } => {
                validate_operation_id(operation_id)?;
                if text.is_empty() || text.len() > MAX_POST_BYTES {
                    return Err(ProtocolError::LimitExceeded("post text"));
                }
            }
            Command::FeedSnapshot { limit, .. } => {
                if *limit == 0 || *limit > MAX_FEED_ITEMS {
                    return Err(ProtocolError::LimitExceeded("feed item count"));
                }
            }
            Command::DrainEvents { limit } => {
                if *limit == 0 || *limit > MAX_EVENTS_PER_RESPONSE {
                    return Err(ProtocolError::LimitExceeded("event count"));
                }
            }
            Command::Status
            | Command::CreateRoot
            | Command::UnlockIdentity
            | Command::LockIdentity
            | Command::CurrentProfile
            | Command::Shutdown => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Status,
    CreateRoot,
    UnlockIdentity,
    LockIdentity,
    CurrentProfile,
    PublishProfile {
        operation_id: String,
        display_name: String,
        bio: String,
    },
    FeedSnapshot {
        order: FeedOrder,
        scope: FeedScope,
        limit: u16,
    },
    PublishPost {
        operation_id: String,
        text: String,
    },
    DrainEvents {
        limit: u16,
    },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub version: u16,
    pub request_id: u64,
    pub body: ResponseBody,
}

impl Response {
    pub fn ok(request_id: u64, reply: Reply) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            body: ResponseBody::Ok(reply),
        }
    }

    pub fn error(request_id: u64, error: ServiceError) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            body: ResponseBody::Err(error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseBody {
    Ok(Reply),
    Err(ServiceError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Ack,
    Status(AccountStatus),
    Profile(Option<ProfileView>),
    Feed(Vec<FeedCard>),
    Published(PublishedObject),
    Events(Vec<ServiceEvent>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    BadRequest,
    IdentityLocked,
    RootMissing,
    Identity,
    Storage,
    Io,
    Busy,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceError {
    pub code: ErrorCode,
    pub message: String,
}

impl ServiceError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    Io(String),
    FrameTooLarge(usize),
    Encode(String),
    Decode(String),
    InvalidRequest(String),
    LimitExceeded(&'static str),
}

impl core::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "ipc i/o: {error}"),
            Self::FrameTooLarge(size) => {
                write!(f, "ipc frame is {size} bytes; maximum is {MAX_FRAME_BYTES}")
            }
            Self::Encode(error) => write!(f, "ipc encode: {error}"),
            Self::Decode(error) => write!(f, "ipc decode: {error}"),
            Self::InvalidRequest(error) => write!(f, "invalid ipc request: {error}"),
            Self::LimitExceeded(field) => write!(f, "ipc limit exceeded: {field}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

pub fn validate_operation_id(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty() || value.len() > MAX_OPERATION_ID_BYTES {
        return Err(ProtocolError::LimitExceeded("operation id"));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(ProtocolError::InvalidRequest(
            "operation id contains unsupported characters".to_string(),
        ));
    }
    Ok(())
}

pub fn write_request<W: Write>(writer: &mut W, request: &Request) -> Result<(), ProtocolError> {
    request.validate()?;
    let mut payload = Encoder::new();
    payload.u16(request.version);
    payload.u64(request.request_id);
    encode_command(&mut payload, &request.command)?;
    write_frame(writer, &payload.finish())
}

pub fn read_request<R: Read>(reader: &mut R) -> Result<Option<Request>, ProtocolError> {
    let Some(payload) = read_frame(reader)? else {
        return Ok(None);
    };
    let mut decoder = Decoder::new(&payload);
    let request = Request {
        version: decoder.u16()?,
        request_id: decoder.u64()?,
        command: decode_command(&mut decoder)?,
    };
    decoder.finish()?;
    request.validate()?;
    Ok(Some(request))
}

pub fn write_response<W: Write>(
    writer: &mut W,
    response: &Response,
) -> Result<(), ProtocolError> {
    if response.version != PROTOCOL_VERSION {
        return Err(ProtocolError::Encode(
            "cannot encode unsupported response version".to_string(),
        ));
    }
    let mut payload = Encoder::new();
    payload.u16(response.version);
    payload.u64(response.request_id);
    match &response.body {
        ResponseBody::Ok(reply) => {
            payload.u8(0);
            encode_reply(&mut payload, reply)?;
        }
        ResponseBody::Err(error) => {
            payload.u8(1);
            encode_service_error(&mut payload, error)?;
        }
    }
    write_frame(writer, &payload.finish())
}

pub fn read_response<R: Read>(reader: &mut R) -> Result<Option<Response>, ProtocolError> {
    let Some(payload) = read_frame(reader)? else {
        return Ok(None);
    };
    let mut decoder = Decoder::new(&payload);
    let version = decoder.u16()?;
    if version != PROTOCOL_VERSION {
        return Err(ProtocolError::Decode(format!(
            "unsupported response version {version}"
        )));
    }
    let request_id = decoder.u64()?;
    let body = match decoder.u8()? {
        0 => ResponseBody::Ok(decode_reply(&mut decoder)?),
        1 => ResponseBody::Err(decode_service_error(&mut decoder)?),
        tag => {
            return Err(ProtocolError::Decode(format!(
                "unknown response body tag {tag}"
            )))
        }
    };
    decoder.finish()?;
    Ok(Some(Response {
        version,
        request_id,
        body,
    }))
}

fn encode_command(encoder: &mut Encoder, command: &Command) -> Result<(), ProtocolError> {
    match command {
        Command::Status => encoder.u8(0),
        Command::CreateRoot => encoder.u8(1),
        Command::UnlockIdentity => encoder.u8(2),
        Command::LockIdentity => encoder.u8(3),
        Command::CurrentProfile => encoder.u8(4),
        Command::PublishProfile {
            operation_id,
            display_name,
            bio,
        } => {
            encoder.u8(5);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(display_name, MAX_PROFILE_NAME_BYTES)?;
            encoder.string(bio, MAX_PROFILE_BIO_BYTES)?;
        }
        Command::FeedSnapshot {
            order,
            scope,
            limit,
        } => {
            encoder.u8(6);
            encoder.u8(match order {
                FeedOrder::Chronological => 0,
                FeedOrder::MostSupported => 1,
            });
            encoder.u8(match scope {
                FeedScope::Following => 0,
                FeedScope::Everyone => 1,
            });
            encoder.u16(*limit);
        }
        Command::PublishPost { operation_id, text } => {
            encoder.u8(7);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(text, MAX_POST_BYTES)?;
        }
        Command::DrainEvents { limit } => {
            encoder.u8(8);
            encoder.u16(*limit);
        }
        Command::Shutdown => encoder.u8(9),
    }
    Ok(())
}

fn decode_command(decoder: &mut Decoder<'_>) -> Result<Command, ProtocolError> {
    match decoder.u8()? {
        0 => Ok(Command::Status),
        1 => Ok(Command::CreateRoot),
        2 => Ok(Command::UnlockIdentity),
        3 => Ok(Command::LockIdentity),
        4 => Ok(Command::CurrentProfile),
        5 => Ok(Command::PublishProfile {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            display_name: decoder.string(MAX_PROFILE_NAME_BYTES)?,
            bio: decoder.string(MAX_PROFILE_BIO_BYTES)?,
        }),
        6 => {
            let order = match decoder.u8()? {
                0 => FeedOrder::Chronological,
                1 => FeedOrder::MostSupported,
                tag => {
                    return Err(ProtocolError::Decode(format!(
                        "unknown feed order tag {tag}"
                    )))
                }
            };
            let scope = match decoder.u8()? {
                0 => FeedScope::Following,
                1 => FeedScope::Everyone,
                tag => {
                    return Err(ProtocolError::Decode(format!(
                        "unknown feed scope tag {tag}"
                    )))
                }
            };
            Ok(Command::FeedSnapshot {
                order,
                scope,
                limit: decoder.u16()?,
            })
        }
        7 => Ok(Command::PublishPost {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            text: decoder.string(MAX_POST_BYTES)?,
        }),
        8 => Ok(Command::DrainEvents {
            limit: decoder.u16()?,
        }),
        9 => Ok(Command::Shutdown),
        tag => Err(ProtocolError::Decode(format!(
            "unknown command tag {tag}"
        ))),
    }
}

fn encode_reply(encoder: &mut Encoder, reply: &Reply) -> Result<(), ProtocolError> {
    match reply {
        Reply::Ack => encoder.u8(0),
        Reply::Status(status) => {
            encoder.u8(1);
            encode_status(encoder, status)?;
        }
        Reply::Profile(profile) => {
            encoder.u8(2);
            encode_profile_option(encoder, profile.as_ref())?;
        }
        Reply::Feed(cards) => {
            encoder.u8(3);
            if cards.len() > usize::from(MAX_FEED_ITEMS) {
                return Err(ProtocolError::LimitExceeded("feed item count"));
            }
            encoder.u16(cards.len() as u16);
            for card in cards {
                encode_feed_card(encoder, card)?;
            }
        }
        Reply::Published(published) => {
            encoder.u8(4);
            encode_published(encoder, published)?;
        }
        Reply::Events(events) => {
            encoder.u8(5);
            if events.len() > usize::from(MAX_EVENTS_PER_RESPONSE) {
                return Err(ProtocolError::LimitExceeded("event count"));
            }
            encoder.u16(events.len() as u16);
            for event in events {
                encode_event(encoder, event)?;
            }
        }
    }
    Ok(())
}

fn decode_reply(decoder: &mut Decoder<'_>) -> Result<Reply, ProtocolError> {
    match decoder.u8()? {
        0 => Ok(Reply::Ack),
        1 => Ok(Reply::Status(decode_status(decoder)?)),
        2 => Ok(Reply::Profile(decode_profile_option(decoder)?)),
        3 => {
            let count = decoder.u16()?;
            if count > MAX_FEED_ITEMS {
                return Err(ProtocolError::LimitExceeded("feed item count"));
            }
            let mut cards = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                cards.push(decode_feed_card(decoder)?);
            }
            Ok(Reply::Feed(cards))
        }
        4 => Ok(Reply::Published(decode_published(decoder)?)),
        5 => {
            let count = decoder.u16()?;
            if count > MAX_EVENTS_PER_RESPONSE {
                return Err(ProtocolError::LimitExceeded("event count"));
            }
            let mut events = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                events.push(decode_event(decoder)?);
            }
            Ok(Reply::Events(events))
        }
        tag => Err(ProtocolError::Decode(format!("unknown reply tag {tag}"))),
    }
}

fn encode_status(encoder: &mut Encoder, status: &AccountStatus) -> Result<(), ProtocolError> {
    encoder.bool(status.root_created);
    encoder.bool(status.identity_unlocked);
    encoder.option_string(status.human_did.as_deref(), MAX_DID_BYTES)?;
    encode_profile_option(encoder, status.profile.as_ref())
}

fn decode_status(decoder: &mut Decoder<'_>) -> Result<AccountStatus, ProtocolError> {
    Ok(AccountStatus {
        root_created: decoder.bool()?,
        identity_unlocked: decoder.bool()?,
        human_did: decoder.option_string(MAX_DID_BYTES)?,
        profile: decode_profile_option(decoder)?,
    })
}

fn encode_profile_option(
    encoder: &mut Encoder,
    profile: Option<&ProfileView>,
) -> Result<(), ProtocolError> {
    encoder.bool(profile.is_some());
    if let Some(profile) = profile {
        encoder.string(&profile.did, MAX_DID_BYTES)?;
        encoder.string(&profile.display_name, MAX_PROFILE_NAME_BYTES)?;
        encoder.string(&profile.bio, MAX_PROFILE_BIO_BYTES)?;
        encoder.option_string(profile.avatar.as_deref(), MAX_OBJECT_ID_BYTES)?;
    }
    Ok(())
}

fn decode_profile_option(
    decoder: &mut Decoder<'_>,
) -> Result<Option<ProfileView>, ProtocolError> {
    if !decoder.bool()? {
        return Ok(None);
    }
    Ok(Some(ProfileView {
        did: decoder.string(MAX_DID_BYTES)?,
        display_name: decoder.string(MAX_PROFILE_NAME_BYTES)?,
        bio: decoder.string(MAX_PROFILE_BIO_BYTES)?,
        avatar: decoder.option_string(MAX_OBJECT_ID_BYTES)?,
    }))
}

fn encode_feed_card(encoder: &mut Encoder, card: &FeedCard) -> Result<(), ProtocolError> {
    encoder.string(&card.id, MAX_OBJECT_ID_BYTES)?;
    encoder.string(&card.author, MAX_PROFILE_NAME_BYTES)?;
    encoder.string(&card.did, MAX_DID_BYTES)?;
    encoder.string(&card.body, MAX_REASONABLE_TEXT_BYTES)?;
    encoder.u64(card.timestamp_ms);
    encoder.u8(match card.reason {
        FeedReason::Own => 0,
        FeedReason::Followed => 1,
        FeedReason::Received => 2,
    });
    encoder.u32(card.support_count);
    encoder.u32(card.comment_count);
    encoder.option_string(card.media.as_deref(), MAX_OBJECT_ID_BYTES)?;
    encoder.bool(card.own);
    encoder.option_string(card.avatar.as_deref(), MAX_OBJECT_ID_BYTES)
}

fn decode_feed_card(decoder: &mut Decoder<'_>) -> Result<FeedCard, ProtocolError> {
    let id = decoder.string(MAX_OBJECT_ID_BYTES)?;
    let author = decoder.string(MAX_PROFILE_NAME_BYTES)?;
    let did = decoder.string(MAX_DID_BYTES)?;
    let body = decoder.string(MAX_REASONABLE_TEXT_BYTES)?;
    let timestamp_ms = decoder.u64()?;
    let reason = match decoder.u8()? {
        0 => FeedReason::Own,
        1 => FeedReason::Followed,
        2 => FeedReason::Received,
        tag => {
            return Err(ProtocolError::Decode(format!(
                "unknown feed reason tag {tag}"
            )))
        }
    };
    Ok(FeedCard {
        id,
        author,
        did,
        body,
        timestamp_ms,
        reason,
        support_count: decoder.u32()?,
        comment_count: decoder.u32()?,
        media: decoder.option_string(MAX_OBJECT_ID_BYTES)?,
        own: decoder.bool()?,
        avatar: decoder.option_string(MAX_OBJECT_ID_BYTES)?,
    })
}

fn encode_published(
    encoder: &mut Encoder,
    published: &PublishedObject,
) -> Result<(), ProtocolError> {
    encoder.u8(match published.kind {
        ObjectKind::Profile => 0,
        ObjectKind::Post => 1,
    });
    encoder.string(&published.object_id, MAX_OBJECT_ID_BYTES)?;
    encoder.bool(published.duplicate);
    Ok(())
}

fn decode_published(decoder: &mut Decoder<'_>) -> Result<PublishedObject, ProtocolError> {
    let kind = match decoder.u8()? {
        0 => ObjectKind::Profile,
        1 => ObjectKind::Post,
        tag => {
            return Err(ProtocolError::Decode(format!(
                "unknown object kind tag {tag}"
            )))
        }
    };
    Ok(PublishedObject {
        kind,
        object_id: decoder.string(MAX_OBJECT_ID_BYTES)?,
        duplicate: decoder.bool()?,
    })
}

fn encode_event(encoder: &mut Encoder, event: &ServiceEvent) -> Result<(), ProtocolError> {
    encoder.u64(event.event_id);
    match &event.kind {
        ServiceEventKind::RootCreated { did } => {
            encoder.u8(0);
            encoder.string(did, MAX_DID_BYTES)?;
        }
        ServiceEventKind::IdentityChanged { unlocked } => {
            encoder.u8(1);
            encoder.bool(*unlocked);
        }
        ServiceEventKind::ObjectPublished { kind, object_id } => {
            encoder.u8(2);
            encoder.u8(match kind {
                ObjectKind::Profile => 0,
                ObjectKind::Post => 1,
            });
            encoder.string(object_id, MAX_OBJECT_ID_BYTES)?;
        }
        ServiceEventKind::FeedChanged => encoder.u8(3),
    }
    Ok(())
}

fn decode_event(decoder: &mut Decoder<'_>) -> Result<ServiceEvent, ProtocolError> {
    let event_id = decoder.u64()?;
    let kind = match decoder.u8()? {
        0 => ServiceEventKind::RootCreated {
            did: decoder.string(MAX_DID_BYTES)?,
        },
        1 => ServiceEventKind::IdentityChanged {
            unlocked: decoder.bool()?,
        },
        2 => {
            let kind = match decoder.u8()? {
                0 => ObjectKind::Profile,
                1 => ObjectKind::Post,
                tag => {
                    return Err(ProtocolError::Decode(format!(
                        "unknown event object kind tag {tag}"
                    )))
                }
            };
            ServiceEventKind::ObjectPublished {
                kind,
                object_id: decoder.string(MAX_OBJECT_ID_BYTES)?,
            }
        }
        3 => ServiceEventKind::FeedChanged,
        tag => return Err(ProtocolError::Decode(format!("unknown event tag {tag}"))),
    };
    Ok(ServiceEvent { event_id, kind })
}

fn encode_service_error(
    encoder: &mut Encoder,
    error: &ServiceError,
) -> Result<(), ProtocolError> {
    encoder.u8(match error.code {
        ErrorCode::BadRequest => 0,
        ErrorCode::IdentityLocked => 1,
        ErrorCode::RootMissing => 2,
        ErrorCode::Identity => 3,
        ErrorCode::Storage => 4,
        ErrorCode::Io => 5,
        ErrorCode::Busy => 6,
        ErrorCode::Internal => 7,
    });
    encoder.string(&error.message, MAX_ERROR_BYTES)
}

fn decode_service_error(decoder: &mut Decoder<'_>) -> Result<ServiceError, ProtocolError> {
    let code = match decoder.u8()? {
        0 => ErrorCode::BadRequest,
        1 => ErrorCode::IdentityLocked,
        2 => ErrorCode::RootMissing,
        3 => ErrorCode::Identity,
        4 => ErrorCode::Storage,
        5 => ErrorCode::Io,
        6 => ErrorCode::Busy,
        7 => ErrorCode::Internal,
        tag => {
            return Err(ProtocolError::Decode(format!(
                "unknown service error tag {tag}"
            )))
        }
    };
    Ok(ServiceError {
        code,
        message: decoder.string(MAX_ERROR_BYTES)?,
    })
}

fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> Result<(), ProtocolError> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge(payload.len()));
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| ProtocolError::FrameTooLarge(payload.len()))?
        .to_be_bytes();
    writer
        .write_all(&length)
        .and_then(|_| writer.write_all(payload))
        .and_then(|_| writer.flush())
        .map_err(|error| ProtocolError::Io(error.to_string()))
}

fn read_frame<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>, ProtocolError> {
    let mut length = [0u8; 4];
    match reader.read(&mut length[..1]) {
        Ok(0) => return Ok(None),
        Ok(1) => {}
        Ok(_) => unreachable!("one-byte read buffer"),
        Err(error) => return Err(ProtocolError::Io(error.to_string())),
    }
    reader
        .read_exact(&mut length[1..])
        .map_err(|error| ProtocolError::Io(error.to_string()))?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge(length));
    }
    let mut payload = vec![0u8; length];
    reader
        .read_exact(&mut payload)
        .map_err(|error| ProtocolError::Io(error.to_string()))?;
    Ok(Some(payload))
}

#[derive(Debug, Default)]
struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn new() -> Self {
        Self::default()
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn string(&mut self, value: &str, max: usize) -> Result<(), ProtocolError> {
        if value.len() > max {
            return Err(ProtocolError::LimitExceeded("string"));
        }
        let length = u32::try_from(value.len())
            .map_err(|_| ProtocolError::LimitExceeded("string"))?;
        self.u32(length);
        self.bytes.extend_from_slice(value.as_bytes());
        Ok(())
    }

    fn option_string(
        &mut self,
        value: Option<&str>,
        max: usize,
    ) -> Result<(), ProtocolError> {
        self.bool(value.is_some());
        if let Some(value) = value {
            self.string(value, max)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn finish(&self) -> Result<(), ProtocolError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ProtocolError::Decode("trailing bytes".to_string()))
        }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| ProtocolError::Decode("length overflow".to_string()))?;
        let out = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| ProtocolError::Decode("truncated frame".to_string()))?;
        self.offset = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    fn bool(&mut self) -> Result<bool, ProtocolError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(ProtocolError::Decode(format!(
                "invalid boolean byte {value}"
            ))),
        }
    }

    fn u16(&mut self) -> Result<u16, ProtocolError> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .expect("two-byte slice has exact length");
        Ok(u16::from_be_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, ProtocolError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .expect("four-byte slice has exact length");
        Ok(u32::from_be_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, ProtocolError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .expect("eight-byte slice has exact length");
        Ok(u64::from_be_bytes(bytes))
    }

    fn string(&mut self, max: usize) -> Result<String, ProtocolError> {
        let length = self.u32()? as usize;
        if length > max {
            return Err(ProtocolError::LimitExceeded("string"));
        }
        String::from_utf8(self.take(length)?.to_vec())
            .map_err(|error| ProtocolError::Decode(error.to_string()))
    }

    fn option_string(&mut self, max: usize) -> Result<Option<String>, ProtocolError> {
        if self.bool()? {
            self.string(max).map(Some)
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trip_is_framed_versioned_and_bounded() {
        let request = Request::new(
            7,
            Command::PublishPost {
                operation_id: "desktop:1:2:3".to_string(),
                text: "hello".to_string(),
            },
        );
        let mut bytes = Vec::new();
        write_request(&mut bytes, &request).unwrap();
        let decoded = read_request(&mut bytes.as_slice()).unwrap().unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn response_round_trip_carries_feed_and_events() {
        let response = Response::ok(
            9,
            Reply::Feed(vec![FeedCard {
                id: "obj".to_string(),
                author: "Alice".to_string(),
                did: "did:mini:alice".to_string(),
                body: "hello".to_string(),
                timestamp_ms: 42,
                reason: FeedReason::Own,
                support_count: 3,
                comment_count: 1,
                media: None,
                own: true,
                avatar: Some("avatar".to_string()),
            }]),
        );
        let mut bytes = Vec::new();
        write_response(&mut bytes, &response).unwrap();
        assert_eq!(
            read_response(&mut bytes.as_slice()).unwrap().unwrap(),
            response
        );
    }

    #[test]
    fn malformed_limits_are_rejected_before_dispatch() {
        let bad_id = Request::new(
            1,
            Command::PublishPost {
                operation_id: "bad id".to_string(),
                text: "hello".to_string(),
            },
        );
        assert!(bad_id.validate().is_err());

        let too_many = Request::new(
            2,
            Command::FeedSnapshot {
                order: FeedOrder::Chronological,
                scope: FeedScope::Following,
                limit: MAX_FEED_ITEMS + 1,
            },
        );
        assert!(too_many.validate().is_err());
    }

    #[test]
    fn declared_oversized_frame_is_rejected_before_allocation() {
        let mut bytes = ((MAX_FRAME_BYTES as u32) + 1).to_be_bytes().to_vec();
        bytes.extend_from_slice(b"ignored");
        assert!(matches!(
            read_request(&mut bytes.as_slice()),
            Err(ProtocolError::FrameTooLarge(_))
        ));
    }

    #[test]
    fn clean_eof_is_not_a_protocol_failure() {
        assert!(read_request(&mut &[][..]).unwrap().is_none());
    }
}
