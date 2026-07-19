//! Route-scoped replication for opaque v2 private envelopes.
//!
//! Unlike public `MINI/SYNC1`, this protocol never enumerates the whole private
//! store. The caller selects exactly one already-shared [`OpaqueRoute`], and
//! both peers prove they selected the same route inside the encrypted channel
//! before either reveals envelope ids. Message decryption and author
//! provenance remain application-layer work.

use mini_bearer::{Bearer, Channel};
use mini_objects::{ObjectEnvelopeV2, ObjectId, OpaqueRoute};
use mini_store::{Backend, Store};

use crate::{Result, SyncError, SyncRole};

const PRIVATE_SYNC_AAD: &[u8] = b"MINI/PRIVATE-SYNC1";
const MAX_IDS_PER_MESSAGE: usize = 4096;
const MAX_ENVELOPES_PER_MESSAGE: usize = 64;
const MAX_ID_BYTES: usize = 128;
const MAX_ENVELOPE_BYTES: usize = 9 * 1024 * 1024;
const ENVELOPE_BATCH_BYTES: usize = 4 * 1024 * 1024;
const SESSION_BYTE_BUDGET: usize = 512 * 1024 * 1024;
const MAX_WANT_ROUNDS: usize = 1024;

/// Outcome of this peer's private-route pull.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivateSyncReport {
    /// Envelopes received over the encrypted channel.
    pub received: usize,
    /// Integrity-valid envelopes for the selected route that were inserted.
    pub accepted: usize,
    /// Malformed, corrupt, or wrong-route envelopes.
    pub invalid: usize,
    /// Valid envelopes that were not requested.
    pub unsolicited: usize,
}

/// Reconcile one selected private route in both directions over an established
/// encrypted channel. The opaque route is a bearer capability: callers should
/// invoke this only for a conversation explicitly selected by the user.
pub fn sync_private_route_bidirectional<B: Backend>(
    bearer: &mut dyn Bearer,
    channel: &mut Channel,
    store: &mut Store<B>,
    route: OpaqueRoute,
    role: SyncRole,
) -> Result<PrivateSyncReport> {
    confirm_route(bearer, channel, route, role)?;
    match role {
        SyncRole::Initiator => {
            let report = pull(bearer, channel, store, route)?;
            serve_pull(bearer, channel, store, route)?;
            Ok(report)
        }
        SyncRole::Responder => {
            serve_pull(bearer, channel, store, route)?;
            pull(bearer, channel, store, route)
        }
    }
}

fn confirm_route(
    bearer: &mut dyn Bearer,
    channel: &mut Channel,
    route: OpaqueRoute,
    role: SyncRole,
) -> Result<()> {
    match role {
        SyncRole::Initiator => {
            send(bearer, channel, &PrivateMsg::Route(*route.as_bytes()))?;
            match recv(bearer, channel)? {
                PrivateMsg::RouteAck(true) => Ok(()),
                PrivateMsg::RouteAck(false) => Err(SyncError::PrivateRouteMismatch),
                _ => Err(SyncError::Protocol),
            }
        }
        SyncRole::Responder => {
            let matches = match recv(bearer, channel)? {
                PrivateMsg::Route(peer) => peer == *route.as_bytes(),
                _ => return Err(SyncError::Protocol),
            };
            send(bearer, channel, &PrivateMsg::RouteAck(matches))?;
            if matches {
                Ok(())
            } else {
                Err(SyncError::PrivateRouteMismatch)
            }
        }
    }
}

fn pull<B: Backend>(
    bearer: &mut dyn Bearer,
    channel: &mut Channel,
    store: &mut Store<B>,
    route: OpaqueRoute,
) -> Result<PrivateSyncReport> {
    let mut report = PrivateSyncReport::default();
    send(bearer, channel, &PrivateMsg::List)?;
    let offered = match recv(bearer, channel)? {
        PrivateMsg::Ids(ids) => ids,
        _ => return Err(SyncError::Protocol),
    };

    let mut wants = Vec::new();
    for id in offered {
        let parsed = ObjectId::parse(&id)?;
        if !store.contains_private(&parsed)? {
            wants.push(id);
        }
    }

    let mut budget = SESSION_BYTE_BUDGET;
    let mut cursor = 0usize;
    let mut rounds = 0usize;
    while cursor < wants.len() {
        rounds += 1;
        if rounds > MAX_WANT_ROUNDS {
            return Err(SyncError::LimitExceeded);
        }
        let end = wants.len().min(cursor + MAX_IDS_PER_MESSAGE);
        let batch = wants[cursor..end].to_vec();
        cursor = end;
        send(bearer, channel, &PrivateMsg::Want(batch.clone()))?;
        loop {
            match recv(bearer, channel)? {
                PrivateMsg::Envelopes(envelopes) if envelopes.is_empty() => break,
                PrivateMsg::Envelopes(envelopes) => {
                    for bytes in envelopes {
                        budget = budget
                            .checked_sub(bytes.len())
                            .ok_or(SyncError::LimitExceeded)?;
                        report.received += 1;
                        match ObjectEnvelopeV2::from_bytes(&bytes) {
                            Ok(envelope)
                                if envelope.route() == route
                                    && batch.iter().any(|id| id == envelope.id().as_str()) =>
                            {
                                store.insert_private(&envelope)?;
                                report.accepted += 1;
                            }
                            Ok(envelope) if envelope.route() != route => report.invalid += 1,
                            Ok(_) => report.unsolicited += 1,
                            Err(_) => report.invalid += 1,
                        }
                    }
                }
                _ => return Err(SyncError::Protocol),
            }
        }
    }
    send(bearer, channel, &PrivateMsg::Want(Vec::new()))?;
    match recv(bearer, channel)? {
        PrivateMsg::Done => Ok(report),
        _ => Err(SyncError::Protocol),
    }
}

fn serve_pull<B: Backend>(
    bearer: &mut dyn Bearer,
    channel: &mut Channel,
    store: &Store<B>,
    route: OpaqueRoute,
) -> Result<()> {
    match recv(bearer, channel)? {
        PrivateMsg::List => {}
        _ => return Err(SyncError::Protocol),
    }
    let allowed = store.private_by_route(&route)?;
    let ids: Vec<String> = allowed.iter().map(|id| id.as_str().to_string()).collect();
    send(bearer, channel, &PrivateMsg::Ids(ids))?;

    let mut rounds = 0usize;
    loop {
        rounds += 1;
        if rounds > MAX_WANT_ROUNDS {
            return Err(SyncError::LimitExceeded);
        }
        let wanted = match recv(bearer, channel)? {
            PrivateMsg::Want(ids) => ids,
            _ => return Err(SyncError::Protocol),
        };
        if wanted.is_empty() {
            send(bearer, channel, &PrivateMsg::Done)?;
            return Ok(());
        }

        let mut batch = Vec::new();
        let mut batch_bytes = 0usize;
        for requested in wanted {
            let id = ObjectId::parse(&requested)?;
            if !allowed.iter().any(|allowed_id| allowed_id == &id) {
                continue;
            }
            let envelope = match store.get_private(&id) {
                Ok(envelope) if envelope.route() == route => envelope,
                _ => continue,
            };
            let bytes = envelope.to_bytes();
            if !batch.is_empty() && batch_bytes + bytes.len() > ENVELOPE_BATCH_BYTES {
                send(
                    bearer,
                    channel,
                    &PrivateMsg::Envelopes(std::mem::take(&mut batch)),
                )?;
                batch_bytes = 0;
            }
            batch_bytes += bytes.len();
            batch.push(bytes);
            if batch.len() == MAX_ENVELOPES_PER_MESSAGE {
                send(
                    bearer,
                    channel,
                    &PrivateMsg::Envelopes(std::mem::take(&mut batch)),
                )?;
                batch_bytes = 0;
            }
        }
        if !batch.is_empty() {
            send(bearer, channel, &PrivateMsg::Envelopes(batch))?;
        }
        send(bearer, channel, &PrivateMsg::Envelopes(Vec::new()))?;
    }
}

fn send(bearer: &mut dyn Bearer, channel: &mut Channel, message: &PrivateMsg) -> Result<()> {
    let ciphertext = channel.seal(&message.encode(), PRIVATE_SYNC_AAD)?;
    bearer.send(&ciphertext)?;
    Ok(())
}

fn recv(bearer: &mut dyn Bearer, channel: &mut Channel) -> Result<PrivateMsg> {
    let ciphertext = bearer.recv()?;
    let plaintext = channel.open(&ciphertext, PRIVATE_SYNC_AAD)?;
    PrivateMsg::decode(&plaintext)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PrivateMsg {
    Route([u8; 32]),
    RouteAck(bool),
    List,
    Ids(Vec<String>),
    Want(Vec<String>),
    Envelopes(Vec<Vec<u8>>),
    Done,
}

impl PrivateMsg {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Self::Route(route) => {
                out.push(1);
                out.extend_from_slice(route);
            }
            Self::RouteAck(matches) => {
                out.push(2);
                out.push(u8::from(*matches));
            }
            Self::List => out.push(3),
            Self::Ids(ids) => encode_strings(&mut out, 4, ids),
            Self::Want(ids) => encode_strings(&mut out, 5, ids),
            Self::Envelopes(envelopes) => {
                out.push(6);
                out.extend_from_slice(&(envelopes.len() as u32).to_be_bytes());
                for envelope in envelopes {
                    out.extend_from_slice(&(envelope.len() as u32).to_be_bytes());
                    out.extend_from_slice(envelope);
                }
            }
            Self::Done => out.push(7),
        }
        out
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut cursor = Cursor { bytes, offset: 0 };
        let message = match cursor.u8()? {
            1 => Self::Route(cursor.array_32()?),
            2 => match cursor.u8()? {
                0 => Self::RouteAck(false),
                1 => Self::RouteAck(true),
                _ => return Err(SyncError::Protocol),
            },
            3 => Self::List,
            4 => Self::Ids(decode_strings(&mut cursor)?),
            5 => Self::Want(decode_strings(&mut cursor)?),
            6 => {
                let count = cursor.u32()? as usize;
                if count > MAX_ENVELOPES_PER_MESSAGE {
                    return Err(SyncError::LimitExceeded);
                }
                let mut envelopes = Vec::with_capacity(count);
                for _ in 0..count {
                    let len = cursor.u32()? as usize;
                    if len > MAX_ENVELOPE_BYTES {
                        return Err(SyncError::LimitExceeded);
                    }
                    envelopes.push(cursor.take(len)?.to_vec());
                }
                Self::Envelopes(envelopes)
            }
            7 => Self::Done,
            _ => return Err(SyncError::Protocol),
        };
        if cursor.offset != cursor.bytes.len() {
            return Err(SyncError::Protocol);
        }
        Ok(message)
    }
}

fn encode_strings(out: &mut Vec<u8>, tag: u8, values: &[String]) {
    out.push(tag);
    out.extend_from_slice(&(values.len() as u32).to_be_bytes());
    for value in values {
        out.extend_from_slice(&(value.len() as u32).to_be_bytes());
        out.extend_from_slice(value.as_bytes());
    }
}

fn decode_strings(cursor: &mut Cursor<'_>) -> Result<Vec<String>> {
    let count = cursor.u32()? as usize;
    if count > MAX_IDS_PER_MESSAGE {
        return Err(SyncError::LimitExceeded);
    }
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let len = cursor.u32()? as usize;
        if len > MAX_ID_BYTES {
            return Err(SyncError::LimitExceeded);
        }
        let value =
            String::from_utf8(cursor.take(len)?.to_vec()).map_err(|_| SyncError::Protocol)?;
        values.push(value);
    }
    Ok(values)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(SyncError::LimitExceeded)?;
        if end > self.bytes.len() {
            return Err(SyncError::Protocol);
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .expect("four-byte slice has exact length");
        Ok(u32::from_be_bytes(bytes))
    }

    fn array_32(&mut self) -> Result<[u8; 32]> {
        Ok(self
            .take(32)?
            .try_into()
            .expect("32-byte slice has exact length"))
    }
}
