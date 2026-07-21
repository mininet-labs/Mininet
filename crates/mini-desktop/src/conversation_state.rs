//! DPAPI-protected local beta conversation capability records.

use std::path::Path;

use did_mini::Did;
use mini_messaging::{BetaInvite, ConversationSecret};
use mini_objects::OpaqueRoute;
use mini_windows_vault::{load_user_data, save_user_data};

const VERSION: u8 = 1;
const MAX_CONVERSATIONS: usize = 128;
const MAX_LABEL_BYTES: usize = 80;
const MAX_DID_BYTES: usize = 256;

#[derive(Debug)]
pub(crate) struct ConversationRecord {
    pub(crate) label: String,
    pub(crate) peer: Did,
    route: [u8; 32],
    key: [u8; 32],
}

impl ConversationRecord {
    pub(crate) fn create(label: String, peer: Did, inviter: Did) -> Result<(Self, String), String> {
        validate_label(&label)?;
        let secret = ConversationSecret::generate_beta().map_err(|error| error.to_string())?;
        let invite = secret.beta_invite(inviter).to_code();
        Ok((
            Self {
                label,
                peer,
                route: *secret.route().as_bytes(),
                key: secret.key_bytes_for_local_vault(),
            },
            invite,
        ))
    }

    pub(crate) fn import(label: String, code: &str) -> Result<Self, String> {
        validate_label(&label)?;
        let invite = BetaInvite::parse(code).map_err(|error| error.to_string())?;
        let secret =
            ConversationSecret::from_beta_invite(&invite).map_err(|error| error.to_string())?;
        Ok(Self {
            label,
            peer: invite.inviter().clone(),
            route: *invite.route().as_bytes(),
            key: secret.key_bytes_for_local_vault(),
        })
    }

    pub(crate) fn secret(&self) -> Result<ConversationSecret, String> {
        ConversationSecret::from_local_vault(OpaqueRoute::from_bytes(self.route), self.key)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn route(&self) -> OpaqueRoute {
        OpaqueRoute::from_bytes(self.route)
    }
}

pub(crate) fn load(path: &Path) -> Result<Vec<ConversationRecord>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = load_user_data(path).map_err(|error| error.to_string())?;
    decode(&bytes)
}

pub(crate) fn save(path: &Path, conversations: &[ConversationRecord]) -> Result<(), String> {
    save_user_data(path, &encode(conversations)?).map_err(|error| error.to_string())
}

fn validate_label(label: &str) -> Result<(), String> {
    if label.trim().is_empty() || label.len() > MAX_LABEL_BYTES {
        return Err("conversation label must be 1-80 bytes".to_string());
    }
    Ok(())
}

fn encode(conversations: &[ConversationRecord]) -> Result<Vec<u8>, String> {
    if conversations.len() > MAX_CONVERSATIONS {
        return Err("conversation limit exceeded".to_string());
    }
    let mut bytes = Vec::new();
    bytes.push(VERSION);
    bytes.extend_from_slice(&(conversations.len() as u32).to_be_bytes());
    for conversation in conversations {
        validate_label(&conversation.label)?;
        write_bounded(&mut bytes, conversation.label.as_bytes(), MAX_LABEL_BYTES)?;
        write_bounded(
            &mut bytes,
            conversation.peer.as_str().as_bytes(),
            MAX_DID_BYTES,
        )?;
        bytes.extend_from_slice(&conversation.route);
        bytes.extend_from_slice(&conversation.key);
    }
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<Vec<ConversationRecord>, String> {
    let mut cursor = Cursor { bytes, offset: 0 };
    if cursor.u8()? != VERSION {
        return Err("unsupported conversation vault version".to_string());
    }
    let count = cursor.u32()? as usize;
    if count > MAX_CONVERSATIONS {
        return Err("conversation limit exceeded".to_string());
    }
    let mut conversations = Vec::with_capacity(count);
    for _ in 0..count {
        let label = String::from_utf8(cursor.bounded(MAX_LABEL_BYTES)?.to_vec())
            .map_err(|_| "conversation label is not UTF-8".to_string())?;
        validate_label(&label)?;
        let did = String::from_utf8(cursor.bounded(MAX_DID_BYTES)?.to_vec())
            .map_err(|_| "conversation DID is not UTF-8".to_string())?;
        let peer = Did::parse(&did).map_err(|error| error.to_string())?;
        let route = cursor.array_32()?;
        let key = cursor.array_32()?;
        conversations.push(ConversationRecord {
            label,
            peer,
            route,
            key,
        });
    }
    if cursor.offset != bytes.len() {
        return Err("conversation vault has trailing bytes".to_string());
    }
    Ok(conversations)
}

fn write_bounded(out: &mut Vec<u8>, value: &[u8], max: usize) -> Result<(), String> {
    if value.is_empty() || value.len() > max || value.len() > u16::MAX as usize {
        return Err("conversation field exceeds its limit".to_string());
    }
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| "conversation vault length overflow".to_string())?;
        if end > self.bytes.len() {
            return Err("truncated conversation vault".to_string());
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let value: [u8; 2] = self.take(2)?.try_into().expect("two-byte slice");
        Ok(u16::from_be_bytes(value))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let value: [u8; 4] = self.take(4)?.try_into().expect("four-byte slice");
        Ok(u32::from_be_bytes(value))
    }

    fn bounded(&mut self, max: usize) -> Result<&'a [u8], String> {
        let len = self.u16()? as usize;
        if len == 0 || len > max {
            return Err("conversation field exceeds its limit".to_string());
        }
        self.take(len)
    }

    fn array_32(&mut self) -> Result<[u8; 32], String> {
        Ok(self.take(32)?.try_into().expect("32-byte slice"))
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, ConversationRecord};
    use did_mini::Controller;

    #[test]
    fn conversation_records_round_trip_without_plaintext_files() {
        let alice = Controller::incept_single_from_seeds(&[1; 32], &[2; 32]).unwrap();
        let bob = Controller::incept_single_from_seeds(&[3; 32], &[4; 32]).unwrap();
        let (record, _) =
            ConversationRecord::create("Bob".to_string(), bob.did(), alice.did()).unwrap();
        let bytes = encode(&[record]).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].label, "Bob");
        assert_eq!(decoded[0].peer, bob.did());
    }

    #[test]
    fn truncated_or_oversized_records_fail_closed() {
        assert!(decode(&[]).is_err());
        assert!(decode(&[1, 0, 0, 0, 129]).is_err());
    }
}
