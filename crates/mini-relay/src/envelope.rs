//! Per-hop relay envelope: seals a next-hop payload under an already-
//! established `mini_bearer::Channel`, binding role/connection-id/size-
//! class as AEAD associated data so tampering with routing metadata
//! breaks decryption — the same discipline `mini_objects::ObjectEnvelopeV2`
//! already applies to its own public fields. Key distribution (how the
//! `Channel` was established) is out of scope here, same as
//! `mini_objects::ObjectEnvelopeV2`.

use mini_bearer::Channel;
use mini_transport_policy::PayloadSizeClass;

use crate::codec::{Reader, Writer};
use crate::connection::ConnectionId;
use crate::error::{RelayError, Result};
use crate::role::RelayRole;

/// This module's envelope format version.
pub const ENVELOPE_VERSION: u8 = 1;

fn size_class_tag(class: PayloadSizeClass) -> u8 {
    match class {
        PayloadSizeClass::Small => 1,
        PayloadSizeClass::Medium => 2,
        PayloadSizeClass::Large => 3,
    }
}

fn size_class_from_tag(tag: u8) -> Result<PayloadSizeClass> {
    match tag {
        1 => Ok(PayloadSizeClass::Small),
        2 => Ok(PayloadSizeClass::Medium),
        3 => Ok(PayloadSizeClass::Large),
        _ => Err(RelayError::BadSizeClass),
    }
}

/// A one-hop, AEAD-sealed relay message. `role`, `connection_id`, and
/// `size_class` are public (a relay hop must read them to route), but all
/// three are bound as associated data, so tampering with any of them
/// breaks decryption of the payload underneath.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayEnvelope {
    pub role: RelayRole,
    pub connection_id: ConnectionId,
    pub size_class: PayloadSizeClass,
    ciphertext: Vec<u8>,
}

impl RelayEnvelope {
    /// Seal `plaintext` for one hop over an already-established `channel`.
    pub fn seal(
        channel: &mut Channel,
        role: RelayRole,
        connection_id: ConnectionId,
        size_class: PayloadSizeClass,
        plaintext: &[u8],
    ) -> Result<Self> {
        let aad = associated_data(role, connection_id, size_class);
        let ciphertext = channel.seal(plaintext, &aad)?;
        Ok(RelayEnvelope {
            role,
            connection_id,
            size_class,
            ciphertext,
        })
    }

    /// Open this envelope's ciphertext over `channel`, verifying that
    /// `role`/`connection_id`/`size_class` were not tampered with since
    /// sealing.
    pub fn open(&self, channel: &mut Channel) -> Result<Vec<u8>> {
        let aad = associated_data(self.role, self.connection_id, self.size_class);
        Ok(channel.open(&self.ciphertext, &aad)?)
    }

    /// Canonical wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(ENVELOPE_VERSION);
        w.u8(self.role.tag());
        w.raw(&self.connection_id.to_bytes());
        w.u8(size_class_tag(self.size_class));
        w.bytes(&self.ciphertext);
        w.into_bytes()
    }

    /// Decode an envelope from untrusted bytes. Rejects an unrecognized
    /// [`ENVELOPE_VERSION`], a malformed role/size-class tag, and
    /// trailing bytes. Does **not** decrypt — call
    /// [`RelayEnvelope::open`] with the hop's channel for that.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        if r.u8()? != ENVELOPE_VERSION {
            return Err(RelayError::UnsupportedEnvelopeVersion);
        }
        let role = RelayRole::from_tag(r.u8()?)?;
        let connection_bytes: [u8; 16] = r
            .raw(16)?
            .try_into()
            .expect("Reader::raw(16) always returns exactly 16 bytes");
        let connection_id = ConnectionId::from_bytes(connection_bytes);
        let size_class = size_class_from_tag(r.u8()?)?;
        const MAX_CIPHERTEXT_BYTES: usize = mini_bearer::MAX_CHANNEL_CIPHERTEXT_BYTES;
        let ciphertext = r.bytes_limited(MAX_CIPHERTEXT_BYTES)?;
        if !r.finished() {
            return Err(RelayError::TrailingBytes);
        }
        Ok(RelayEnvelope {
            role,
            connection_id,
            size_class,
            ciphertext,
        })
    }
}

fn associated_data(
    role: RelayRole,
    connection_id: ConnectionId,
    size_class: PayloadSizeClass,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(ENVELOPE_VERSION);
    w.u8(role.tag());
    w.raw(&connection_id.to_bytes());
    w.u8(size_class_tag(size_class));
    w.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_bearer::{Initiator, Responder};

    fn linked_channels() -> (Channel, Channel) {
        let (initiator, hello_i) = Initiator::start().unwrap();
        let (responder_channel, hello_r) = Responder::respond(&hello_i).unwrap();
        let initiator_channel = initiator.finish(&hello_r).unwrap();
        // Sanity: both ends agree on the channel binding before any test uses them.
        assert_eq!(
            initiator_channel.channel_binding(),
            responder_channel.channel_binding()
        );
        (initiator_channel, responder_channel)
    }

    #[test]
    fn a_sealed_envelope_opens_on_the_peer_channel() {
        let (mut a, mut b) = linked_channels();
        let connection_id = ConnectionId::generate().unwrap();
        let envelope = RelayEnvelope::seal(
            &mut a,
            RelayRole::Entry,
            connection_id,
            PayloadSizeClass::Small,
            b"hop payload",
        )
        .unwrap();
        let opened = envelope.open(&mut b).unwrap();
        assert_eq!(opened, b"hop payload");
    }

    #[test]
    fn an_envelope_round_trips_through_wire_bytes() {
        let (mut a, _b) = linked_channels();
        let connection_id = ConnectionId::generate().unwrap();
        let envelope = RelayEnvelope::seal(
            &mut a,
            RelayRole::Rendezvous,
            connection_id,
            PayloadSizeClass::Medium,
            b"mailbox pickup",
        )
        .unwrap();
        let decoded = RelayEnvelope::from_bytes(&envelope.to_bytes()).unwrap();
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn a_decoded_envelope_still_opens() {
        let (mut a, mut b) = linked_channels();
        let connection_id = ConnectionId::generate().unwrap();
        let envelope = RelayEnvelope::seal(
            &mut a,
            RelayRole::Delivery,
            connection_id,
            PayloadSizeClass::Large,
            b"final hop",
        )
        .unwrap();
        let decoded = RelayEnvelope::from_bytes(&envelope.to_bytes()).unwrap();
        let opened = decoded.open(&mut b).unwrap();
        assert_eq!(opened, b"final hop");
    }

    #[test]
    fn tampering_with_the_role_breaks_decryption() {
        let (mut a, mut b) = linked_channels();
        let connection_id = ConnectionId::generate().unwrap();
        let mut envelope = RelayEnvelope::seal(
            &mut a,
            RelayRole::Entry,
            connection_id,
            PayloadSizeClass::Small,
            b"hop payload",
        )
        .unwrap();
        envelope.role = RelayRole::Rendezvous;
        assert!(envelope.open(&mut b).is_err());
    }

    #[test]
    fn tampering_with_the_connection_id_breaks_decryption() {
        let (mut a, mut b) = linked_channels();
        let connection_id = ConnectionId::generate().unwrap();
        let mut envelope = RelayEnvelope::seal(
            &mut a,
            RelayRole::Entry,
            connection_id,
            PayloadSizeClass::Small,
            b"hop payload",
        )
        .unwrap();
        envelope.connection_id = ConnectionId::generate().unwrap();
        assert!(envelope.open(&mut b).is_err());
    }

    #[test]
    fn tampering_with_the_size_class_breaks_decryption() {
        let (mut a, mut b) = linked_channels();
        let connection_id = ConnectionId::generate().unwrap();
        let mut envelope = RelayEnvelope::seal(
            &mut a,
            RelayRole::Entry,
            connection_id,
            PayloadSizeClass::Small,
            b"hop payload",
        )
        .unwrap();
        envelope.size_class = PayloadSizeClass::Large;
        assert!(envelope.open(&mut b).is_err());
    }

    #[test]
    fn opening_with_the_wrong_channel_fails() {
        let (mut a, _b) = linked_channels();
        let (_c, mut d) = linked_channels();
        let connection_id = ConnectionId::generate().unwrap();
        let envelope = RelayEnvelope::seal(
            &mut a,
            RelayRole::Entry,
            connection_id,
            PayloadSizeClass::Small,
            b"hop payload",
        )
        .unwrap();
        assert!(envelope.open(&mut d).is_err());
    }

    #[test]
    fn an_unknown_envelope_version_is_rejected() {
        let mut w = Writer::new();
        w.u8(0xee);
        assert_eq!(
            RelayEnvelope::from_bytes(&w.into_bytes()),
            Err(RelayError::UnsupportedEnvelopeVersion)
        );
    }

    #[test]
    fn a_truncated_envelope_is_rejected_at_every_length() {
        let (mut a, _b) = linked_channels();
        let connection_id = ConnectionId::generate().unwrap();
        let envelope = RelayEnvelope::seal(
            &mut a,
            RelayRole::Entry,
            connection_id,
            PayloadSizeClass::Small,
            b"hop payload",
        )
        .unwrap();
        let full = envelope.to_bytes();
        for cut in 0..full.len() {
            assert!(
                RelayEnvelope::from_bytes(&full[..cut]).is_err(),
                "truncating to {cut} bytes must be rejected"
            );
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let (mut a, _b) = linked_channels();
        let connection_id = ConnectionId::generate().unwrap();
        let envelope = RelayEnvelope::seal(
            &mut a,
            RelayRole::Entry,
            connection_id,
            PayloadSizeClass::Small,
            b"hop payload",
        )
        .unwrap();
        let mut bytes = envelope.to_bytes();
        bytes.push(0xff);
        assert_eq!(
            RelayEnvelope::from_bytes(&bytes),
            Err(RelayError::TrailingBytes)
        );
    }
}
