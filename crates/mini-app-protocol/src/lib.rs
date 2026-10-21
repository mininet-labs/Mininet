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
pub const MAX_COMMENT_BYTES: usize = 16 * 1024;
pub const MAX_COMMUNITY_NAME_BYTES: usize = 96;
pub const MAX_COMMUNITY_CHARTER_BYTES: usize = 4096;
pub const MAX_PROFILE_NAME_BYTES: usize = 64;
pub const MAX_PROFILE_BIO_BYTES: usize = 1024;
pub const MAX_FEED_ITEMS: u16 = 50;
pub const MAX_EVENTS_PER_RESPONSE: u16 = 64;
pub const MAX_DELIVERIES_PER_RESPONSE: u16 = 64;
pub const MAX_DELIVERY_KEL_BYTES: usize = 16 * 1024;
pub const MAX_DELIVERY_PEERS: u16 = 64;
pub const MAX_DID_BYTES: usize = 256;
pub const MAX_OBJECT_ID_BYTES: usize = 256;
const MAX_ERROR_BYTES: usize = 4096;
pub const MAX_LOCATION_BYTES: usize = 128;
pub const MAX_WALL_NAME_BYTES: usize = 64;
pub const MAX_WALL_BIO_BYTES: usize = 1024;
pub const MAX_WALL_LINKS: usize = 16;
pub const MAX_WALL_LINK_BYTES: usize = 256;
pub const MAX_PROFILE_PHOTO_BYTES: usize = 96 * 1024;
pub const MAX_PROFILE_FIELDS: usize = 16;
pub const MAX_PROFILE_FIELD_LABEL_BYTES: usize = 32;
pub const MAX_PROFILE_FIELD_VALUE_BYTES: usize = 256;
const MAX_FEED_BODY_BYTES: usize = MAX_POST_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallVisibility {
    Public,
    Unlisted,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicWallDraft {
    pub display_name: String,
    pub bio: String,
    pub public_links: Vec<String>,
    pub visibility: WallVisibility,
}
impl PublicWallDraft {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.display_name.len() > MAX_WALL_NAME_BYTES
            || self.display_name.trim().is_empty()
            || self.bio.len() > MAX_WALL_BIO_BYTES
            || self.public_links.len() > MAX_WALL_LINKS
            || self
                .public_links
                .iter()
                .any(|value| value.len() > MAX_WALL_LINK_BYTES)
        {
            return Err(ProtocolError::LimitExceeded("public wall draft"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileField {
    pub label: String,
    pub value: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicProfileDetails {
    pub display_name: String,
    pub bio: String,
    pub avatar: Option<String>,
    pub location: Option<String>,
    pub age: Option<u8>,
    pub fields: Vec<ProfileField>,
}
impl PublicProfileDetails {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.display_name.len() > MAX_PROFILE_NAME_BYTES
            || self.display_name.trim().is_empty()
            || self.bio.len() > MAX_PROFILE_BIO_BYTES
            || self
                .avatar
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > MAX_OBJECT_ID_BYTES)
            || self
                .location
                .as_ref()
                .is_some_and(|value| value.is_empty() || value.len() > MAX_LOCATION_BYTES)
            || self.age == Some(0)
            || self.fields.len() > MAX_PROFILE_FIELDS
            || self.fields.iter().any(|field| {
                field.label.is_empty()
                    || field.label.len() > MAX_PROFILE_FIELD_LABEL_BYTES
                    || field.value.is_empty()
                    || field.value.len() > MAX_PROFILE_FIELD_VALUE_BYTES
            })
        {
            return Err(ProtocolError::LimitExceeded("public profile details"));
        }
        Ok(())
    }
}
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
    Follow,
    Reaction,
    Comment,
    Membership,
    Community,
    Wall,
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

/// Public identity material explicitly exported for pairing. Never seed bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryIdentity {
    pub root_kel: Vec<u8>,
    pub device_kel: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryPeerView {
    pub peer_id: String,
    pub endpoint: String,
    pub root_did: String,
    pub device_did: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryRuntimeView {
    pub enabled: bool,
    pub listener_peer: Option<String>,
    pub listener_endpoint: Option<String>,
    pub listening: bool,
}

/// Durable outbound state. Delivered means the named peer acknowledged verified
/// storage, not that a person read it or governance accepted it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryState {
    Queued,
    InFlight,
    RetryWaiting,
    Delivered,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryFailure {
    Unavailable,
    Rejected,
    Interrupted,
}

/// Owner-reviewed summary of one canonical public object, never signed bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryObjectView {
    pub object_id: String,
    pub kind: ObjectKind,
    pub summary: String,
    pub targets: Vec<String>,
}

/// Metadata only: signed bytes and identity secrets never cross this reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryView {
    pub operation_id: String,
    pub peer_id: String,
    pub object_id: String,
    pub state: DeliveryState,
    pub attempts: u32,
    pub next_attempt_ms: u64,
    pub last_failure: Option<DeliveryFailure>,
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
    DeliveryChanged(DeliveryView),
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
            Command::PublishWall {
                operation_id,
                draft,
            } => {
                validate_operation_id(operation_id)?;
                draft.validate()?;
            }
            Command::PublishProfileWithPhoto {
                operation_id,
                draft,
                photo,
            } => {
                validate_operation_id(operation_id)?;
                draft.validate()?;
                if draft.avatar.is_some()
                    || photo.is_empty()
                    || photo.len() > MAX_PROFILE_PHOTO_BYTES
                {
                    return Err(ProtocolError::LimitExceeded("reviewed profile JPEG"));
                }
            }
            Command::PublishProfileDetails {
                operation_id,
                draft,
            } => {
                validate_operation_id(operation_id)?;
                draft.validate()?;
            }
            Command::PreviewPublicDelivery { object_id } => {
                if object_id.is_empty() || object_id.len() > MAX_OBJECT_ID_BYTES {
                    return Err(ProtocolError::LimitExceeded("delivery preview object ID"));
                }
            }
            Command::PublishCommunity {
                operation_id,
                name,
                charter,
            } => {
                validate_operation_id(operation_id)?;
                if name.len() > MAX_COMMUNITY_NAME_BYTES
                    || name.trim().is_empty()
                    || charter.len() > MAX_COMMUNITY_CHARTER_BYTES
                {
                    return Err(ProtocolError::LimitExceeded("community name or charter"));
                }
            }
            Command::SetMembership {
                operation_id,
                community_id,
                ..
            } => {
                validate_operation_id(operation_id)?;
                if community_id.is_empty() || community_id.len() > MAX_OBJECT_ID_BYTES {
                    return Err(ProtocolError::LimitExceeded("membership community ID"));
                }
            }
            Command::PublishComment {
                operation_id,
                parent_id,
                text,
            } => {
                validate_operation_id(operation_id)?;
                if parent_id.is_empty() || parent_id.len() > MAX_OBJECT_ID_BYTES {
                    return Err(ProtocolError::LimitExceeded("comment parent ID"));
                }
                if text.len() > MAX_COMMENT_BYTES || text.trim().is_empty() {
                    return Err(ProtocolError::LimitExceeded("comment text"));
                }
            }
            Command::SetLike {
                operation_id,
                target_id,
                ..
            } => {
                validate_operation_id(operation_id)?;
                if target_id.is_empty() || target_id.len() > MAX_OBJECT_ID_BYTES {
                    return Err(ProtocolError::LimitExceeded("like target ID"));
                }
            }
            Command::SetFollow {
                operation_id,
                target_did,
                ..
            } => {
                validate_operation_id(operation_id)?;
                if target_did.is_empty() || target_did.len() > MAX_DID_BYTES {
                    return Err(ProtocolError::LimitExceeded("follow target DID"));
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
            Command::QueuePublicDelivery {
                operation_id,
                peer_id,
                object_id,
            } => {
                validate_operation_id(operation_id)?;
                validate_operation_id(peer_id)?;
                if object_id.is_empty() || object_id.len() > MAX_OBJECT_ID_BYTES {
                    return Err(ProtocolError::LimitExceeded("delivery object id"));
                }
            }
            Command::DeliveryStatus { operation_id } | Command::CancelDelivery { operation_id } => {
                validate_operation_id(operation_id)?;
            }
            Command::ListDeliveriesAfter {
                after_operation_id,
                limit,
            } => {
                if let Some(after) = after_operation_id {
                    validate_operation_id(after)?;
                }
                if *limit == 0 || *limit > MAX_DELIVERIES_PER_RESPONSE {
                    return Err(ProtocolError::LimitExceeded("delivery count"));
                }
            }
            Command::ListDeliveries { limit } => {
                if *limit == 0 || *limit > MAX_DELIVERIES_PER_RESPONSE {
                    return Err(ProtocolError::LimitExceeded("delivery count"));
                }
            }
            Command::AuthorizeDeliveryPeer {
                peer_id,
                endpoint,
                root_kel,
                device_kel,
            } => {
                validate_operation_id(peer_id)?;
                if endpoint.is_empty() || endpoint.len() > 128 {
                    return Err(ProtocolError::LimitExceeded("delivery endpoint"));
                }
                if root_kel.is_empty()
                    || device_kel.is_empty()
                    || root_kel.len() > MAX_DELIVERY_KEL_BYTES
                    || device_kel.len() > MAX_DELIVERY_KEL_BYTES
                {
                    return Err(ProtocolError::LimitExceeded("delivery KEL"));
                }
            }
            Command::RevokeDeliveryPeer { peer_id } => validate_operation_id(peer_id)?,
            Command::AttemptDelivery { operation_id } => validate_operation_id(operation_id)?,
            Command::SetDeliveryListener { peer_id, endpoint } => {
                validate_operation_id(peer_id)?;
                if endpoint.is_empty() || endpoint.len() > 128 {
                    return Err(ProtocolError::LimitExceeded("delivery bind endpoint"));
                }
            }
            Command::DeliveryRuntimeStatus
            | Command::ExportDeliveryIdentity
            | Command::DeliveryPeers
            | Command::SetDeliveryEnabled { .. }
            | Command::StopDeliveryListener => {}
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
    PublishWall {
        operation_id: String,
        draft: PublicWallDraft,
    },
    PublishProfileWithPhoto {
        operation_id: String,
        draft: PublicProfileDetails,
        photo: Vec<u8>,
    },
    PublishProfileDetails {
        operation_id: String,
        draft: PublicProfileDetails,
    },
    PreviewPublicDelivery {
        object_id: String,
    },
    PublishCommunity {
        operation_id: String,
        name: String,
        charter: String,
    },
    SetMembership {
        operation_id: String,
        community_id: String,
        joined: bool,
    },
    PublishComment {
        operation_id: String,
        parent_id: String,
        text: String,
    },
    SetLike {
        operation_id: String,
        target_id: String,
        active: bool,
    },
    SetFollow {
        operation_id: String,
        target_did: String,
        following: bool,
    },
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
    QueuePublicDelivery {
        operation_id: String,
        peer_id: String,
        object_id: String,
    },
    DeliveryStatus {
        operation_id: String,
    },
    ListDeliveriesAfter {
        after_operation_id: Option<String>,
        limit: u16,
    },
    ListDeliveries {
        limit: u16,
    },
    CancelDelivery {
        operation_id: String,
    },
    DeliveryRuntimeStatus,
    ExportDeliveryIdentity,
    AuthorizeDeliveryPeer {
        peer_id: String,
        endpoint: String,
        root_kel: Vec<u8>,
        device_kel: Vec<u8>,
    },
    RevokeDeliveryPeer {
        peer_id: String,
    },
    DeliveryPeers,
    AttemptDelivery {
        operation_id: String,
    },
    SetDeliveryEnabled {
        enabled: bool,
    },
    SetDeliveryListener {
        peer_id: String,
        endpoint: String,
    },
    StopDeliveryListener,
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
    PublicDeliveryObject(DeliveryObjectView),
    Ack,
    Status(AccountStatus),
    Profile(Option<ProfileView>),
    Feed(Vec<FeedCard>),
    Published(PublishedObject),
    Events(Vec<ServiceEvent>),
    Delivery(DeliveryView),
    Deliveries(Vec<DeliveryView>),
    DeliveryIdentity(DeliveryIdentity),
    DeliveryPeers(Vec<DeliveryPeerView>),
    DeliveryRuntime(DeliveryRuntimeView),
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

pub fn write_response<W: Write>(writer: &mut W, response: &Response) -> Result<(), ProtocolError> {
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

fn encode_profile_details(
    encoder: &mut Encoder,
    draft: &PublicProfileDetails,
) -> Result<(), ProtocolError> {
    draft.validate()?;
    encoder.string(&draft.display_name, MAX_PROFILE_NAME_BYTES)?;
    encoder.string(&draft.bio, MAX_PROFILE_BIO_BYTES)?;
    encoder.option_string(draft.avatar.as_deref(), MAX_OBJECT_ID_BYTES)?;
    encoder.option_string(draft.location.as_deref(), MAX_LOCATION_BYTES)?;
    encoder.bool(draft.age.is_some());
    if let Some(age) = draft.age {
        encoder.u8(age);
    }
    encoder.u8(draft.fields.len() as u8);
    for field in &draft.fields {
        encoder.string(&field.label, MAX_PROFILE_FIELD_LABEL_BYTES)?;
        encoder.string(&field.value, MAX_PROFILE_FIELD_VALUE_BYTES)?;
    }
    Ok(())
}
fn decode_profile_details(
    decoder: &mut Decoder<'_>,
) -> Result<PublicProfileDetails, ProtocolError> {
    let display_name = decoder.string(MAX_PROFILE_NAME_BYTES)?;
    let bio = decoder.string(MAX_PROFILE_BIO_BYTES)?;
    let avatar = decoder.option_string(MAX_OBJECT_ID_BYTES)?;
    let location = decoder.option_string(MAX_LOCATION_BYTES)?;
    let age = if decoder.bool()? {
        Some(decoder.u8()?)
    } else {
        None
    };
    let count = decoder.u8()? as usize;
    if count > MAX_PROFILE_FIELDS {
        return Err(ProtocolError::LimitExceeded("public profile field count"));
    }
    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        fields.push(ProfileField {
            label: decoder.string(MAX_PROFILE_FIELD_LABEL_BYTES)?,
            value: decoder.string(MAX_PROFILE_FIELD_VALUE_BYTES)?,
        });
    }
    let draft = PublicProfileDetails {
        display_name,
        bio,
        avatar,
        location,
        age,
        fields,
    };
    draft.validate()?;
    Ok(draft)
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
        Command::PublishWall {
            operation_id,
            draft,
        } => {
            encoder.u8(32);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(&draft.display_name, MAX_WALL_NAME_BYTES)?;
            encoder.string(&draft.bio, MAX_WALL_BIO_BYTES)?;
            encoder.u8(match draft.visibility {
                WallVisibility::Public => 0,
                WallVisibility::Unlisted => 1,
            });
            encoder.u8(draft.public_links.len() as u8);
            for link in &draft.public_links {
                encoder.string(link, MAX_WALL_LINK_BYTES)?;
            }
        }
        Command::PublishProfileWithPhoto {
            operation_id,
            draft,
            photo,
        } => {
            encoder.u8(31);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
            encode_profile_details(encoder, draft)?;
            encoder.byte_string(photo, MAX_PROFILE_PHOTO_BYTES)?;
        }
        Command::PublishProfileDetails {
            operation_id,
            draft,
        } => {
            encoder.u8(30);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
            encode_profile_details(encoder, draft)?;
        }
        Command::PreviewPublicDelivery { object_id } => {
            encoder.u8(29);
            encoder.string(object_id, MAX_OBJECT_ID_BYTES)?;
        }
        Command::PublishCommunity {
            operation_id,
            name,
            charter,
        } => {
            encoder.u8(28);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(name, MAX_COMMUNITY_NAME_BYTES)?;
            encoder.string(charter, MAX_COMMUNITY_CHARTER_BYTES)?;
        }
        Command::SetMembership {
            operation_id,
            community_id,
            joined,
        } => {
            encoder.u8(27);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(community_id, MAX_OBJECT_ID_BYTES)?;
            encoder.u8(u8::from(*joined));
        }
        Command::PublishComment {
            operation_id,
            parent_id,
            text,
        } => {
            encoder.u8(26);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(parent_id, MAX_OBJECT_ID_BYTES)?;
            encoder.string(text, MAX_COMMENT_BYTES)?;
        }
        Command::SetLike {
            operation_id,
            target_id,
            active,
        } => {
            encoder.u8(25);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(target_id, MAX_OBJECT_ID_BYTES)?;
            encoder.u8(u8::from(*active));
        }
        Command::SetFollow {
            operation_id,
            target_did,
            following,
        } => {
            encoder.u8(24);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(target_did, MAX_DID_BYTES)?;
            encoder.u8(u8::from(*following));
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
        Command::DeliveryRuntimeStatus => encoder.u8(22),
        Command::ExportDeliveryIdentity => encoder.u8(14),
        Command::AuthorizeDeliveryPeer {
            peer_id,
            endpoint,
            root_kel,
            device_kel,
        } => {
            encoder.u8(15);
            encoder.string(peer_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(endpoint, 128)?;
            encoder.byte_string(root_kel, MAX_DELIVERY_KEL_BYTES)?;
            encoder.byte_string(device_kel, MAX_DELIVERY_KEL_BYTES)?;
        }
        Command::RevokeDeliveryPeer { peer_id } => {
            encoder.u8(16);
            encoder.string(peer_id, MAX_OPERATION_ID_BYTES)?;
        }
        Command::DeliveryPeers => encoder.u8(17),
        Command::AttemptDelivery { operation_id } => {
            encoder.u8(18);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
        }
        Command::SetDeliveryEnabled { enabled } => {
            encoder.u8(19);
            encoder.bool(*enabled);
        }
        Command::SetDeliveryListener { peer_id, endpoint } => {
            encoder.u8(20);
            encoder.string(peer_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(endpoint, 128)?;
        }
        Command::StopDeliveryListener => encoder.u8(21),
        Command::Shutdown => encoder.u8(9),
        Command::QueuePublicDelivery {
            operation_id,
            peer_id,
            object_id,
        } => {
            encoder.u8(10);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(peer_id, MAX_OPERATION_ID_BYTES)?;
            encoder.string(object_id, MAX_OBJECT_ID_BYTES)?;
        }
        Command::DeliveryStatus { operation_id } => {
            encoder.u8(11);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
        }
        Command::ListDeliveriesAfter {
            after_operation_id,
            limit,
        } => {
            encoder.u8(23);
            encoder.option_string(after_operation_id.as_deref(), MAX_OPERATION_ID_BYTES)?;
            encoder.u16(*limit);
        }
        Command::ListDeliveries { limit } => {
            encoder.u8(12);
            encoder.u16(*limit);
        }
        Command::CancelDelivery { operation_id } => {
            encoder.u8(13);
            encoder.string(operation_id, MAX_OPERATION_ID_BYTES)?;
        }
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
        32 => {
            let operation_id = decoder.string(MAX_OPERATION_ID_BYTES)?;
            let display_name = decoder.string(MAX_WALL_NAME_BYTES)?;
            let bio = decoder.string(MAX_WALL_BIO_BYTES)?;
            let visibility = match decoder.u8()? {
                0 => WallVisibility::Public,
                1 => WallVisibility::Unlisted,
                _ => return Err(ProtocolError::Decode("invalid wall visibility".into())),
            };
            let count = decoder.u8()? as usize;
            if count > MAX_WALL_LINKS {
                return Err(ProtocolError::LimitExceeded("wall link count"));
            }
            let mut public_links = Vec::with_capacity(count);
            for _ in 0..count {
                public_links.push(decoder.string(MAX_WALL_LINK_BYTES)?);
            }
            Ok(Command::PublishWall {
                operation_id,
                draft: PublicWallDraft {
                    display_name,
                    bio,
                    public_links,
                    visibility,
                },
            })
        }
        31 => Ok(Command::PublishProfileWithPhoto {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            draft: decode_profile_details(decoder)?,
            photo: decoder.byte_string(MAX_PROFILE_PHOTO_BYTES)?,
        }),
        30 => Ok(Command::PublishProfileDetails {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            draft: decode_profile_details(decoder)?,
        }),
        29 => Ok(Command::PreviewPublicDelivery {
            object_id: decoder.string(MAX_OBJECT_ID_BYTES)?,
        }),
        28 => Ok(Command::PublishCommunity {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            name: decoder.string(MAX_COMMUNITY_NAME_BYTES)?,
            charter: decoder.string(MAX_COMMUNITY_CHARTER_BYTES)?,
        }),
        27 => Ok(Command::SetMembership {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            community_id: decoder.string(MAX_OBJECT_ID_BYTES)?,
            joined: match decoder.u8()? {
                0 => false,
                1 => true,
                _ => return Err(ProtocolError::Decode("invalid membership action".into())),
            },
        }),
        26 => Ok(Command::PublishComment {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            parent_id: decoder.string(MAX_OBJECT_ID_BYTES)?,
            text: decoder.string(MAX_COMMENT_BYTES)?,
        }),
        25 => Ok(Command::SetLike {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            target_id: decoder.string(MAX_OBJECT_ID_BYTES)?,
            active: match decoder.u8()? {
                0 => false,
                1 => true,
                _ => return Err(ProtocolError::Decode("invalid like action".into())),
            },
        }),
        24 => Ok(Command::SetFollow {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            target_did: decoder.string(MAX_DID_BYTES)?,
            following: match decoder.u8()? {
                0 => false,
                1 => true,
                _ => return Err(ProtocolError::Decode("invalid follow action".into())),
            },
        }),
        7 => Ok(Command::PublishPost {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            text: decoder.string(MAX_POST_BYTES)?,
        }),
        8 => Ok(Command::DrainEvents {
            limit: decoder.u16()?,
        }),
        9 => Ok(Command::Shutdown),
        22 => Ok(Command::DeliveryRuntimeStatus),
        14 => Ok(Command::ExportDeliveryIdentity),
        15 => Ok(Command::AuthorizeDeliveryPeer {
            peer_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            endpoint: decoder.string(128)?,
            root_kel: decoder.byte_string(MAX_DELIVERY_KEL_BYTES)?,
            device_kel: decoder.byte_string(MAX_DELIVERY_KEL_BYTES)?,
        }),
        16 => Ok(Command::RevokeDeliveryPeer {
            peer_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
        }),
        17 => Ok(Command::DeliveryPeers),
        18 => Ok(Command::AttemptDelivery {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
        }),
        19 => Ok(Command::SetDeliveryEnabled {
            enabled: decoder.bool()?,
        }),
        20 => Ok(Command::SetDeliveryListener {
            peer_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            endpoint: decoder.string(128)?,
        }),
        21 => Ok(Command::StopDeliveryListener),
        10 => Ok(Command::QueuePublicDelivery {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            peer_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
            object_id: decoder.string(MAX_OBJECT_ID_BYTES)?,
        }),
        11 => Ok(Command::DeliveryStatus {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
        }),
        23 => Ok(Command::ListDeliveriesAfter {
            after_operation_id: decoder.option_string(MAX_OPERATION_ID_BYTES)?,
            limit: decoder.u16()?,
        }),
        12 => Ok(Command::ListDeliveries {
            limit: decoder.u16()?,
        }),
        13 => Ok(Command::CancelDelivery {
            operation_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
        }),
        tag => Err(ProtocolError::Decode(format!("unknown command tag {tag}"))),
    }
}

fn encode_reply(encoder: &mut Encoder, reply: &Reply) -> Result<(), ProtocolError> {
    match reply {
        Reply::PublicDeliveryObject(view) => {
            if view.targets.len() > 1 {
                return Err(ProtocolError::LimitExceeded("delivery preview targets"));
            }
            encoder.u8(11);
            encoder.string(&view.object_id, MAX_OBJECT_ID_BYTES)?;
            encoder.u8(match view.kind {
                ObjectKind::Post => 1,
                ObjectKind::Follow => 2,
                ObjectKind::Reaction => 3,
                ObjectKind::Comment => 4,
                ObjectKind::Membership => 5,
                ObjectKind::Community => 6,
                ObjectKind::Profile | ObjectKind::Wall => {
                    return Err(ProtocolError::InvalidRequest(
                        "profile and wall delivery are unsupported".into(),
                    ))
                }
            });
            encoder.string(&view.summary, MAX_POST_BYTES)?;
            encoder.u8(view.targets.len() as u8);
            for target in &view.targets {
                encoder.string(target, MAX_OBJECT_ID_BYTES)?;
            }
        }
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
        Reply::Delivery(delivery) => {
            encoder.u8(6);
            encode_delivery(encoder, delivery)?;
        }
        Reply::Deliveries(deliveries) => {
            encoder.u8(7);
            if deliveries.len() > usize::from(MAX_DELIVERIES_PER_RESPONSE) {
                return Err(ProtocolError::LimitExceeded("delivery count"));
            }
            encoder.u16(deliveries.len() as u16);
            for delivery in deliveries {
                encode_delivery(encoder, delivery)?;
            }
        }
        Reply::DeliveryIdentity(identity) => {
            encoder.u8(8);
            encoder.byte_string(&identity.root_kel, MAX_DELIVERY_KEL_BYTES)?;
            encoder.byte_string(&identity.device_kel, MAX_DELIVERY_KEL_BYTES)?;
        }
        Reply::DeliveryPeers(peers) => {
            encoder.u8(9);
            if peers.len() > usize::from(MAX_DELIVERY_PEERS) {
                return Err(ProtocolError::LimitExceeded("delivery peer count"));
            }
            encoder.u16(peers.len() as u16);
            for peer in peers {
                encoder.string(&peer.peer_id, MAX_OPERATION_ID_BYTES)?;
                encoder.string(&peer.endpoint, 128)?;
                encoder.string(&peer.root_did, MAX_DID_BYTES)?;
                encoder.string(&peer.device_did, MAX_DID_BYTES)?;
                encoder.bool(peer.enabled);
            }
        }
        Reply::DeliveryRuntime(view) => {
            encoder.u8(10);
            encoder.bool(view.enabled);
            encoder.option_string(view.listener_peer.as_deref(), MAX_OPERATION_ID_BYTES)?;
            encoder.option_string(view.listener_endpoint.as_deref(), 128)?;
            encoder.bool(view.listening);
        }
    }
    Ok(())
}

fn decode_reply(decoder: &mut Decoder<'_>) -> Result<Reply, ProtocolError> {
    match decoder.u8()? {
        11 => {
            let object_id = decoder.string(MAX_OBJECT_ID_BYTES)?;
            let kind = match decoder.u8()? {
                1 => ObjectKind::Post,
                2 => ObjectKind::Follow,
                3 => ObjectKind::Reaction,
                4 => ObjectKind::Comment,
                5 => ObjectKind::Membership,
                6 => ObjectKind::Community,
                _ => {
                    return Err(ProtocolError::Decode(
                        "unsupported delivery preview kind".into(),
                    ))
                }
            };
            let summary = decoder.string(MAX_POST_BYTES)?;
            let count = decoder.u8()?;
            if count > 1 {
                return Err(ProtocolError::LimitExceeded("delivery preview targets"));
            }
            let mut targets = Vec::with_capacity(count as usize);
            for _ in 0..count {
                targets.push(decoder.string(MAX_OBJECT_ID_BYTES)?);
            }
            Ok(Reply::PublicDeliveryObject(DeliveryObjectView {
                object_id,
                kind,
                summary,
                targets,
            }))
        }
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
        6 => Ok(Reply::Delivery(decode_delivery(decoder)?)),
        7 => {
            let count = decoder.u16()?;
            if count > MAX_DELIVERIES_PER_RESPONSE {
                return Err(ProtocolError::LimitExceeded("delivery count"));
            }
            let mut deliveries = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                deliveries.push(decode_delivery(decoder)?);
            }
            Ok(Reply::Deliveries(deliveries))
        }
        8 => Ok(Reply::DeliveryIdentity(DeliveryIdentity {
            root_kel: decoder.byte_string(MAX_DELIVERY_KEL_BYTES)?,
            device_kel: decoder.byte_string(MAX_DELIVERY_KEL_BYTES)?,
        })),
        9 => {
            let count = decoder.u16()?;
            if count > MAX_DELIVERY_PEERS {
                return Err(ProtocolError::LimitExceeded("delivery peer count"));
            }
            let mut peers = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                peers.push(DeliveryPeerView {
                    peer_id: decoder.string(MAX_OPERATION_ID_BYTES)?,
                    endpoint: decoder.string(128)?,
                    root_did: decoder.string(MAX_DID_BYTES)?,
                    device_did: decoder.string(MAX_DID_BYTES)?,
                    enabled: decoder.bool()?,
                });
            }
            Ok(Reply::DeliveryPeers(peers))
        }
        10 => Ok(Reply::DeliveryRuntime(DeliveryRuntimeView {
            enabled: decoder.bool()?,
            listener_peer: decoder.option_string(MAX_OPERATION_ID_BYTES)?,
            listener_endpoint: decoder.option_string(128)?,
            listening: decoder.bool()?,
        })),
        tag => Err(ProtocolError::Decode(format!("unknown reply tag {tag}"))),
    }
}

fn encode_delivery(encoder: &mut Encoder, delivery: &DeliveryView) -> Result<(), ProtocolError> {
    validate_operation_id(&delivery.operation_id)?;
    validate_operation_id(&delivery.peer_id)?;
    encoder.string(&delivery.operation_id, MAX_OPERATION_ID_BYTES)?;
    encoder.string(&delivery.peer_id, MAX_OPERATION_ID_BYTES)?;
    encoder.string(&delivery.object_id, MAX_OBJECT_ID_BYTES)?;
    encoder.u8(match delivery.state {
        DeliveryState::Queued => 0,
        DeliveryState::InFlight => 1,
        DeliveryState::RetryWaiting => 2,
        DeliveryState::Delivered => 3,
        DeliveryState::Cancelled => 4,
    });
    encoder.u32(delivery.attempts);
    encoder.u64(delivery.next_attempt_ms);
    encoder.u8(match delivery.last_failure {
        None => 0,
        Some(DeliveryFailure::Unavailable) => 1,
        Some(DeliveryFailure::Rejected) => 2,
        Some(DeliveryFailure::Interrupted) => 3,
    });
    Ok(())
}

fn decode_delivery(decoder: &mut Decoder<'_>) -> Result<DeliveryView, ProtocolError> {
    let operation_id = decoder.string(MAX_OPERATION_ID_BYTES)?;
    let peer_id = decoder.string(MAX_OPERATION_ID_BYTES)?;
    validate_operation_id(&operation_id)?;
    validate_operation_id(&peer_id)?;
    let object_id = decoder.string(MAX_OBJECT_ID_BYTES)?;
    let state = match decoder.u8()? {
        0 => DeliveryState::Queued,
        1 => DeliveryState::InFlight,
        2 => DeliveryState::RetryWaiting,
        3 => DeliveryState::Delivered,
        4 => DeliveryState::Cancelled,
        _ => return Err(ProtocolError::Decode("invalid delivery state".into())),
    };
    let attempts = decoder.u32()?;
    let next_attempt_ms = decoder.u64()?;
    let last_failure = match decoder.u8()? {
        0 => None,
        1 => Some(DeliveryFailure::Unavailable),
        2 => Some(DeliveryFailure::Rejected),
        3 => Some(DeliveryFailure::Interrupted),
        _ => return Err(ProtocolError::Decode("invalid delivery failure".into())),
    };
    Ok(DeliveryView {
        operation_id,
        peer_id,
        object_id,
        state,
        attempts,
        next_attempt_ms,
        last_failure,
    })
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

fn decode_profile_option(decoder: &mut Decoder<'_>) -> Result<Option<ProfileView>, ProtocolError> {
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
    encoder.string(&card.body, MAX_FEED_BODY_BYTES)?;
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
    let body = decoder.string(MAX_FEED_BODY_BYTES)?;
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
        ObjectKind::Follow => 2,
        ObjectKind::Reaction => 3,
        ObjectKind::Comment => 4,
        ObjectKind::Membership => 5,
        ObjectKind::Community => 6,
        ObjectKind::Wall => 7,
    });
    encoder.string(&published.object_id, MAX_OBJECT_ID_BYTES)?;
    encoder.bool(published.duplicate);
    Ok(())
}

fn decode_published(decoder: &mut Decoder<'_>) -> Result<PublishedObject, ProtocolError> {
    let kind = match decoder.u8()? {
        0 => ObjectKind::Profile,
        1 => ObjectKind::Post,
        2 => ObjectKind::Follow,
        3 => ObjectKind::Reaction,
        4 => ObjectKind::Comment,
        5 => ObjectKind::Membership,
        6 => ObjectKind::Community,
        7 => ObjectKind::Wall,
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
                ObjectKind::Follow => 2,
                ObjectKind::Reaction => 3,
                ObjectKind::Comment => 4,
                ObjectKind::Membership => 5,
                ObjectKind::Community => 6,
                ObjectKind::Wall => 7,
            });
            encoder.string(object_id, MAX_OBJECT_ID_BYTES)?;
        }
        ServiceEventKind::FeedChanged => encoder.u8(3),
        ServiceEventKind::DeliveryChanged(delivery) => {
            encoder.u8(4);
            encode_delivery(encoder, delivery)?;
        }
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
                2 => ObjectKind::Follow,
                3 => ObjectKind::Reaction,
                4 => ObjectKind::Comment,
                5 => ObjectKind::Membership,
                6 => ObjectKind::Community,
                7 => ObjectKind::Wall,
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
        4 => ServiceEventKind::DeliveryChanged(decode_delivery(decoder)?),
        tag => return Err(ProtocolError::Decode(format!("unknown event tag {tag}"))),
    };
    Ok(ServiceEvent { event_id, kind })
}

fn encode_service_error(encoder: &mut Encoder, error: &ServiceError) -> Result<(), ProtocolError> {
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

    fn byte_string(&mut self, value: &[u8], max: usize) -> Result<(), ProtocolError> {
        if value.len() > max {
            return Err(ProtocolError::LimitExceeded("byte string"));
        }
        self.u32(value.len() as u32);
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    fn string(&mut self, value: &str, max: usize) -> Result<(), ProtocolError> {
        if value.len() > max {
            return Err(ProtocolError::LimitExceeded("string"));
        }
        let length =
            u32::try_from(value.len()).map_err(|_| ProtocolError::LimitExceeded("string"))?;
        self.u32(length);
        self.bytes.extend_from_slice(value.as_bytes());
        Ok(())
    }

    fn option_string(&mut self, value: Option<&str>, max: usize) -> Result<(), ProtocolError> {
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

    fn byte_string(&mut self, max: usize) -> Result<Vec<u8>, ProtocolError> {
        let length = self.u32()? as usize;
        if length > max {
            return Err(ProtocolError::LimitExceeded("byte string"));
        }
        Ok(self.take(length)?.to_vec())
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
    fn delivery_peer_and_runtime_capabilities_round_trip_with_bounded_public_material() {
        let commands = vec![
            Command::ExportDeliveryIdentity,
            Command::AuthorizeDeliveryPeer {
                peer_id: "alice".into(),
                endpoint: "127.0.0.1:7773".into(),
                root_kel: vec![1, 2],
                device_kel: vec![3, 4],
            },
            Command::RevokeDeliveryPeer {
                peer_id: "alice".into(),
            },
            Command::DeliveryPeers,
            Command::AttemptDelivery {
                operation_id: "delivery:1".into(),
            },
            Command::SetDeliveryEnabled { enabled: true },
            Command::SetDeliveryListener {
                peer_id: "alice".into(),
                endpoint: "0.0.0.0:7773".into(),
            },
            Command::StopDeliveryListener,
            Command::DeliveryRuntimeStatus,
            Command::ListDeliveriesAfter {
                after_operation_id: Some("send:previous".into()),
                limit: 64,
            },
        ];
        for command in commands {
            let request = Request::new(3, command);
            let mut bytes = Vec::new();
            write_request(&mut bytes, &request).unwrap();
            assert_eq!(
                read_request(&mut bytes.as_slice()).unwrap().unwrap(),
                request
            );
        }
        for reply in [
            Reply::DeliveryIdentity(DeliveryIdentity {
                root_kel: vec![1, 2],
                device_kel: vec![3, 4],
            }),
            Reply::DeliveryPeers(vec![DeliveryPeerView {
                peer_id: "alice".into(),
                endpoint: "127.0.0.1:7773".into(),
                root_did: "root".into(),
                device_did: "device".into(),
                enabled: false,
            }]),
            Reply::DeliveryRuntime(DeliveryRuntimeView {
                enabled: false,
                listener_peer: Some("alice".into()),
                listener_endpoint: Some("0.0.0.0:7773".into()),
                listening: false,
            }),
        ] {
            let response = Response::ok(3, reply);
            let mut bytes = Vec::new();
            write_response(&mut bytes, &response).unwrap();
            assert_eq!(
                read_response(&mut bytes.as_slice()).unwrap().unwrap(),
                response
            );
        }
        assert!(Request::new(
            3,
            Command::AuthorizeDeliveryPeer {
                peer_id: "alice".into(),
                endpoint: "127.0.0.1:7773".into(),
                root_kel: vec![0; MAX_DELIVERY_KEL_BYTES + 1],
                device_kel: vec![3]
            }
        )
        .validate()
        .is_err());
    }

    #[test]
    fn durable_delivery_contract_round_trips_and_rejects_unbounded_lists() {
        let commands = vec![
            Command::QueuePublicDelivery {
                operation_id: "delivery:1".into(),
                peer_id: "alice".into(),
                object_id: "exact-object".into(),
            },
            Command::DeliveryStatus {
                operation_id: "delivery:1".into(),
            },
            Command::CancelDelivery {
                operation_id: "delivery:1".into(),
            },
            Command::ListDeliveries {
                limit: MAX_DELIVERIES_PER_RESPONSE,
            },
        ];
        for command in commands {
            let request = Request::new(17, command);
            let mut bytes = Vec::new();
            write_request(&mut bytes, &request).unwrap();
            assert_eq!(
                read_request(&mut bytes.as_slice()).unwrap().unwrap(),
                request
            );
        }
        for state in [
            DeliveryState::Queued,
            DeliveryState::InFlight,
            DeliveryState::RetryWaiting,
            DeliveryState::Delivered,
            DeliveryState::Cancelled,
        ] {
            let delivery = DeliveryView {
                operation_id: "delivery:1".into(),
                peer_id: "alice".into(),
                object_id: "exact-object".into(),
                state,
                attempts: 4,
                next_attempt_ms: 1234,
                last_failure: Some(DeliveryFailure::Interrupted),
            };
            for reply in [
                Reply::Delivery(delivery.clone()),
                Reply::Deliveries(vec![delivery.clone()]),
                Reply::Events(vec![ServiceEvent {
                    event_id: 9,
                    kind: ServiceEventKind::DeliveryChanged(delivery.clone()),
                }]),
            ] {
                let response = Response::ok(17, reply);
                let mut bytes = Vec::new();
                write_response(&mut bytes, &response).unwrap();
                assert_eq!(
                    read_response(&mut bytes.as_slice()).unwrap().unwrap(),
                    response
                );
            }
        }
        assert!(Request::new(
            1,
            Command::ListDeliveries {
                limit: MAX_DELIVERIES_PER_RESPONSE + 1
            }
        )
        .validate()
        .is_err());
        assert!(Request::new(
            1,
            Command::QueuePublicDelivery {
                operation_id: "../escape".into(),
                peer_id: "alice".into(),
                object_id: "obj".into()
            }
        )
        .validate()
        .is_err());
    }

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
    fn maximum_feed_response_fits_the_declared_frame() {
        let long_id = "o".repeat(MAX_OBJECT_ID_BYTES);
        let long_did = "d".repeat(MAX_DID_BYTES);
        let long_author = "a".repeat(MAX_PROFILE_NAME_BYTES);
        let body = "x".repeat(MAX_POST_BYTES);
        let cards = (0..MAX_FEED_ITEMS)
            .map(|_| FeedCard {
                id: long_id.clone(),
                author: long_author.clone(),
                did: long_did.clone(),
                body: body.clone(),
                timestamp_ms: u64::MAX,
                reason: FeedReason::Received,
                support_count: u32::MAX,
                comment_count: u32::MAX,
                media: Some(long_id.clone()),
                own: false,
                avatar: Some(long_id.clone()),
            })
            .collect();
        let response = Response::ok(11, Reply::Feed(cards));
        let mut bytes = Vec::new();
        write_response(&mut bytes, &response).unwrap();
        assert!(bytes.len() <= MAX_FRAME_BYTES + 4);
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

#[cfg(test)]
mod follow_protocol_tests {
    use super::*;
    #[test]
    fn follow_command_and_published_events_round_trip_with_bounded_fields() {
        for following in [true, false] {
            let request = Request::new(
                19,
                Command::SetFollow {
                    operation_id: "follow:1".into(),
                    target_did: "did:mini:public-target".into(),
                    following,
                },
            );
            let mut bytes = Vec::new();
            write_request(&mut bytes, &request).unwrap();
            assert_eq!(
                read_request(&mut bytes.as_slice()).unwrap().unwrap(),
                request
            );
        }
        for reply in [
            Reply::Published(PublishedObject {
                kind: ObjectKind::Follow,
                object_id: "object".into(),
                duplicate: true,
            }),
            Reply::Events(vec![ServiceEvent {
                event_id: 1,
                kind: ServiceEventKind::ObjectPublished {
                    kind: ObjectKind::Follow,
                    object_id: "object".into(),
                },
            }]),
        ] {
            let response = Response::ok(19, reply);
            let mut bytes = Vec::new();
            write_response(&mut bytes, &response).unwrap();
            assert_eq!(
                read_response(&mut bytes.as_slice()).unwrap().unwrap(),
                response
            );
        }
        for (operation_id, target_did) in [
            ("../invalid".into(), "did".into()),
            ("valid".into(), String::new()),
            ("valid".into(), "x".repeat(MAX_DID_BYTES + 1)),
        ] {
            assert!(Request::new(
                1,
                Command::SetFollow {
                    operation_id,
                    target_did,
                    following: true
                }
            )
            .validate()
            .is_err());
        }
    }
}

#[cfg(test)]
mod like_protocol_tests {
    use super::*;
    #[test]
    fn like_command_publication_and_events_round_trip_and_reject_invalid_bounds() {
        for active in [true, false] {
            let request = Request::new(
                20,
                Command::SetLike {
                    operation_id: "like:1".into(),
                    target_id: "public-object".into(),
                    active,
                },
            );
            let mut bytes = Vec::new();
            write_request(&mut bytes, &request).unwrap();
            assert_eq!(
                read_request(&mut bytes.as_slice()).unwrap().unwrap(),
                request
            );
            *bytes.last_mut().unwrap() = 2;
            assert!(read_request(&mut bytes.as_slice()).is_err());
        }
        for reply in [
            Reply::Published(PublishedObject {
                kind: ObjectKind::Reaction,
                object_id: "object".into(),
                duplicate: true,
            }),
            Reply::Events(vec![ServiceEvent {
                event_id: 2,
                kind: ServiceEventKind::ObjectPublished {
                    kind: ObjectKind::Reaction,
                    object_id: "object".into(),
                },
            }]),
        ] {
            let response = Response::ok(20, reply);
            let mut bytes = Vec::new();
            write_response(&mut bytes, &response).unwrap();
            assert_eq!(
                read_response(&mut bytes.as_slice()).unwrap().unwrap(),
                response
            );
        }
        for (operation_id, target_id) in [
            ("../invalid".into(), "object".into()),
            ("valid".into(), String::new()),
            ("valid".into(), "x".repeat(MAX_OBJECT_ID_BYTES + 1)),
        ] {
            assert!(Request::new(
                1,
                Command::SetLike {
                    operation_id,
                    target_id,
                    active: true
                }
            )
            .validate()
            .is_err());
        }
    }
}

#[cfg(test)]
mod comment_protocol_tests {
    use super::*;
    #[test]
    fn comment_commands_publications_events_round_trip_and_text_is_bounded_nonblank() {
        let request = Request::new(
            21,
            Command::PublishComment {
                operation_id: "comment:1".into(),
                parent_id: "parent".into(),
                text: "hello".into(),
            },
        );
        let mut bytes = Vec::new();
        write_request(&mut bytes, &request).unwrap();
        assert_eq!(
            read_request(&mut bytes.as_slice()).unwrap().unwrap(),
            request
        );
        for reply in [
            Reply::Published(PublishedObject {
                kind: ObjectKind::Comment,
                object_id: "object".into(),
                duplicate: true,
            }),
            Reply::Events(vec![ServiceEvent {
                event_id: 3,
                kind: ServiceEventKind::ObjectPublished {
                    kind: ObjectKind::Comment,
                    object_id: "object".into(),
                },
            }]),
        ] {
            let response = Response::ok(21, reply);
            let mut bytes = Vec::new();
            write_response(&mut bytes, &response).unwrap();
            assert_eq!(
                read_response(&mut bytes.as_slice()).unwrap().unwrap(),
                response
            );
        }
        for (operation_id, parent_id, text) in [
            ("../invalid".into(), "parent".into(), "hi".into()),
            ("valid".into(), String::new(), "hi".into()),
            (
                "valid".into(),
                "x".repeat(MAX_OBJECT_ID_BYTES + 1),
                "hi".into(),
            ),
            ("valid".into(), "parent".into(), String::new()),
            ("valid".into(), "parent".into(), " \n\t".into()),
            (
                "valid".into(),
                "parent".into(),
                "x".repeat(MAX_COMMENT_BYTES + 1),
            ),
        ] {
            assert!(Request::new(
                1,
                Command::PublishComment {
                    operation_id,
                    parent_id,
                    text
                }
            )
            .validate()
            .is_err());
        }
    }
}

#[cfg(test)]
mod membership_protocol_tests {
    use super::*;
    #[test]
    fn membership_commands_publications_events_round_trip_and_bounds_reject() {
        for joined in [true, false] {
            let request = Request::new(
                22,
                Command::SetMembership {
                    operation_id: "membership:1".into(),
                    community_id: "community".into(),
                    joined,
                },
            );
            let mut bytes = Vec::new();
            write_request(&mut bytes, &request).unwrap();
            assert_eq!(
                read_request(&mut bytes.as_slice()).unwrap().unwrap(),
                request
            );
            *bytes.last_mut().unwrap() = 2;
            assert!(read_request(&mut bytes.as_slice()).is_err());
        }
        for reply in [
            Reply::Published(PublishedObject {
                kind: ObjectKind::Membership,
                object_id: "object".into(),
                duplicate: true,
            }),
            Reply::Events(vec![ServiceEvent {
                event_id: 4,
                kind: ServiceEventKind::ObjectPublished {
                    kind: ObjectKind::Membership,
                    object_id: "object".into(),
                },
            }]),
        ] {
            let response = Response::ok(22, reply);
            let mut bytes = Vec::new();
            write_response(&mut bytes, &response).unwrap();
            assert_eq!(
                read_response(&mut bytes.as_slice()).unwrap().unwrap(),
                response
            );
        }
        for (operation_id, community_id) in [
            ("../bad".into(), "community".into()),
            ("valid".into(), String::new()),
            ("valid".into(), "x".repeat(MAX_OBJECT_ID_BYTES + 1)),
        ] {
            assert!(Request::new(
                1,
                Command::SetMembership {
                    operation_id,
                    community_id,
                    joined: true
                }
            )
            .validate()
            .is_err());
        }
    }
}

#[cfg(test)]
mod community_protocol_tests {
    use super::*;
    #[test]
    fn community_commands_publications_events_round_trip_and_name_charter_bounds_reject() {
        let request = Request::new(
            23,
            Command::PublishCommunity {
                operation_id: "community:1".into(),
                name: "Community".into(),
                charter: "Charter".into(),
            },
        );
        let mut bytes = Vec::new();
        write_request(&mut bytes, &request).unwrap();
        assert_eq!(
            read_request(&mut bytes.as_slice()).unwrap().unwrap(),
            request
        );
        for reply in [
            Reply::Published(PublishedObject {
                kind: ObjectKind::Community,
                object_id: "object".into(),
                duplicate: true,
            }),
            Reply::Events(vec![ServiceEvent {
                event_id: 5,
                kind: ServiceEventKind::ObjectPublished {
                    kind: ObjectKind::Community,
                    object_id: "object".into(),
                },
            }]),
        ] {
            let response = Response::ok(23, reply);
            let mut bytes = Vec::new();
            write_response(&mut bytes, &response).unwrap();
            assert_eq!(
                read_response(&mut bytes.as_slice()).unwrap().unwrap(),
                response
            );
        }
        for (operation_id, name, charter) in [
            ("../bad".into(), "Name".into(), String::new()),
            ("valid".into(), " \n".into(), String::new()),
            (
                "valid".into(),
                "x".repeat(MAX_COMMUNITY_NAME_BYTES + 1),
                String::new(),
            ),
            (
                "valid".into(),
                "Name".into(),
                "x".repeat(MAX_COMMUNITY_CHARTER_BYTES + 1),
            ),
        ] {
            assert!(Request::new(
                1,
                Command::PublishCommunity {
                    operation_id,
                    name,
                    charter
                }
            )
            .validate()
            .is_err());
        }
    }
}

#[cfg(test)]
mod public_delivery_preview_tests {
    use super::*;
    #[test]
    fn public_preview_round_trips_bounded_metadata_and_rejects_larger_summaries_or_targets() {
        let request = Request::new(
            24,
            Command::PreviewPublicDelivery {
                object_id: "object".into(),
            },
        );
        let mut bytes = Vec::new();
        write_request(&mut bytes, &request).unwrap();
        assert_eq!(
            read_request(&mut bytes.as_slice()).unwrap().unwrap(),
            request
        );
        let view = DeliveryObjectView {
            object_id: "object".into(),
            kind: ObjectKind::Comment,
            summary: "x".repeat(MAX_POST_BYTES),
            targets: vec!["t".repeat(MAX_OBJECT_ID_BYTES)],
        };
        let response = Response::ok(24, Reply::PublicDeliveryObject(view.clone()));
        let mut bytes = Vec::new();
        write_response(&mut bytes, &response).unwrap();
        assert_eq!(
            read_response(&mut bytes.as_slice()).unwrap().unwrap(),
            response
        );
        for invalid in [
            DeliveryObjectView {
                summary: "x".repeat(MAX_POST_BYTES + 1),
                ..view.clone()
            },
            DeliveryObjectView {
                targets: vec!["one".into(), "two".into()],
                ..view.clone()
            },
            DeliveryObjectView {
                targets: vec!["t".repeat(MAX_OBJECT_ID_BYTES + 1)],
                ..view.clone()
            },
            DeliveryObjectView {
                kind: ObjectKind::Profile,
                ..view
            },
        ] {
            assert!(write_response(
                &mut Vec::new(),
                &Response::ok(24, Reply::PublicDeliveryObject(invalid))
            )
            .is_err());
        }
        assert!(Request::new(
            1,
            Command::PreviewPublicDelivery {
                object_id: "x".repeat(MAX_OBJECT_ID_BYTES + 1)
            }
        )
        .validate()
        .is_err());
    }
}

#[cfg(test)]
mod profile_details_protocol_tests {
    use super::*;
    fn draft() -> PublicProfileDetails {
        PublicProfileDetails {
            display_name: "Alice".into(),
            bio: "About".into(),
            avatar: Some("avatar".into()),
            location: Some("Chosen city".into()),
            age: Some(25),
            fields: vec![
                ProfileField {
                    label: "First".into(),
                    value: "One".into(),
                },
                ProfileField {
                    label: "Second".into(),
                    value: "Two".into(),
                },
            ],
        }
    }
    #[test]
    fn profile_details_options_and_order_round_trip_and_every_field_limit_is_checked() {
        for details in [
            draft(),
            PublicProfileDetails {
                avatar: None,
                location: None,
                age: None,
                fields: Vec::new(),
                ..draft()
            },
        ] {
            let request = Request::new(
                25,
                Command::PublishProfileDetails {
                    operation_id: "details:1".into(),
                    draft: details,
                },
            );
            let mut bytes = Vec::new();
            write_request(&mut bytes, &request).unwrap();
            assert_eq!(
                read_request(&mut bytes.as_slice()).unwrap().unwrap(),
                request
            );
        }
        let base = draft();
        let invalid = vec![
            PublicProfileDetails {
                display_name: "x".repeat(MAX_PROFILE_NAME_BYTES + 1),
                ..base.clone()
            },
            PublicProfileDetails {
                display_name: String::new(),
                ..base.clone()
            },
            PublicProfileDetails {
                bio: "x".repeat(MAX_PROFILE_BIO_BYTES + 1),
                ..base.clone()
            },
            PublicProfileDetails {
                avatar: Some("x".repeat(MAX_OBJECT_ID_BYTES + 1)),
                ..base.clone()
            },
            PublicProfileDetails {
                avatar: Some(String::new()),
                ..base.clone()
            },
            PublicProfileDetails {
                location: Some("x".repeat(MAX_LOCATION_BYTES + 1)),
                ..base.clone()
            },
            PublicProfileDetails {
                location: Some(String::new()),
                ..base.clone()
            },
            PublicProfileDetails {
                age: Some(0),
                ..base.clone()
            },
            PublicProfileDetails {
                fields: vec![base.fields[0].clone(); MAX_PROFILE_FIELDS + 1],
                ..base.clone()
            },
            PublicProfileDetails {
                fields: vec![ProfileField {
                    label: "x".repeat(MAX_PROFILE_FIELD_LABEL_BYTES + 1),
                    value: "v".into(),
                }],
                ..base.clone()
            },
            PublicProfileDetails {
                fields: vec![ProfileField {
                    label: "label".into(),
                    value: "x".repeat(MAX_PROFILE_FIELD_VALUE_BYTES + 1),
                }],
                ..base.clone()
            },
            PublicProfileDetails {
                fields: vec![ProfileField {
                    label: String::new(),
                    value: "v".into(),
                }],
                ..base
            },
        ];
        for draft in invalid {
            assert!(write_request(
                &mut Vec::new(),
                &Request::new(
                    25,
                    Command::PublishProfileDetails {
                        operation_id: "details:1".into(),
                        draft
                    }
                )
            )
            .is_err());
        }
    }
}

#[cfg(test)]
mod profile_photo_protocol_tests {
    use super::*;
    fn draft() -> PublicProfileDetails {
        PublicProfileDetails {
            display_name: "Alice".into(),
            bio: "About".into(),
            avatar: None,
            location: None,
            age: None,
            fields: Vec::new(),
        }
    }
    #[test]
    fn reviewed_photo_command_round_trip_rejects_large_empty_or_preexisting_avatar() {
        let request = Request::new(
            26,
            Command::PublishProfileWithPhoto {
                operation_id: "photo:1".into(),
                draft: draft(),
                photo: vec![1; MAX_PROFILE_PHOTO_BYTES],
            },
        );
        let mut bytes = Vec::new();
        write_request(&mut bytes, &request).unwrap();
        assert_eq!(
            read_request(&mut bytes.as_slice()).unwrap().unwrap(),
            request
        );
        for (details, photo) in [
            (draft(), Vec::new()),
            (draft(), vec![0; MAX_PROFILE_PHOTO_BYTES + 1]),
            (
                PublicProfileDetails {
                    avatar: Some("old-avatar".into()),
                    ..draft()
                },
                vec![1],
            ),
        ] {
            assert!(write_request(
                &mut Vec::new(),
                &Request::new(
                    26,
                    Command::PublishProfileWithPhoto {
                        operation_id: "photo:1".into(),
                        draft: details,
                        photo
                    }
                )
            )
            .is_err());
        }
    }
}

#[cfg(test)]
mod wall_protocol_tests {
    use super::*;
    fn draft() -> PublicWallDraft {
        PublicWallDraft {
            display_name: "Wall".into(),
            bio: "Bio".into(),
            public_links: vec!["https://example.test".into()],
            visibility: WallVisibility::Public,
        }
    }
    #[test]
    fn wall_visibility_round_trips_and_all_draft_bounds_are_checked() {
        for visibility in [WallVisibility::Public, WallVisibility::Unlisted] {
            let request = Request::new(
                27,
                Command::PublishWall {
                    operation_id: "wall:1".into(),
                    draft: PublicWallDraft {
                        visibility,
                        ..draft()
                    },
                },
            );
            let mut bytes = Vec::new();
            write_request(&mut bytes, &request).unwrap();
            assert_eq!(
                read_request(&mut bytes.as_slice()).unwrap().unwrap(),
                request
            );
        }
        for reply in [
            Reply::Published(PublishedObject {
                kind: ObjectKind::Wall,
                object_id: "wall".into(),
                duplicate: true,
            }),
            Reply::Events(vec![ServiceEvent {
                event_id: 6,
                kind: ServiceEventKind::ObjectPublished {
                    kind: ObjectKind::Wall,
                    object_id: "wall".into(),
                },
            }]),
        ] {
            let response = Response::ok(27, reply);
            let mut bytes = Vec::new();
            write_response(&mut bytes, &response).unwrap();
            assert_eq!(
                read_response(&mut bytes.as_slice()).unwrap().unwrap(),
                response
            );
        }
        for invalid in [
            PublicWallDraft {
                display_name: "x".repeat(MAX_WALL_NAME_BYTES + 1),
                ..draft()
            },
            PublicWallDraft {
                display_name: " ".into(),
                ..draft()
            },
            PublicWallDraft {
                bio: "x".repeat(MAX_WALL_BIO_BYTES + 1),
                ..draft()
            },
            PublicWallDraft {
                public_links: vec![String::new(); MAX_WALL_LINKS + 1],
                ..draft()
            },
            PublicWallDraft {
                public_links: vec!["x".repeat(MAX_WALL_LINK_BYTES + 1)],
                ..draft()
            },
        ] {
            assert!(Request::new(
                1,
                Command::PublishWall {
                    operation_id: "wall:1".into(),
                    draft: invalid
                }
            )
            .validate()
            .is_err());
        }
        assert!(write_response(
            &mut Vec::new(),
            &Response::ok(
                1,
                Reply::PublicDeliveryObject(DeliveryObjectView {
                    object_id: "wall".into(),
                    kind: ObjectKind::Wall,
                    summary: "wall".into(),
                    targets: Vec::new()
                })
            )
        )
        .is_err());
    }
}
