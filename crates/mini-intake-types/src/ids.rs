//! Content-addressed identifiers, wrapping `mini_crypto`'s existing
//! [`mini_crypto::Multihash`] rather than inventing a parallel digest
//! type — no new cryptography (Directive 14).

use mini_crypto::Multihash;

use crate::codec::{Reader, Writer};
use crate::error::Result;

const MAX_MULTIHASH_BYTES: usize = 128;

/// A source's or envelope's content-addressed identifier. Callers derive
/// this by hashing whatever canonical bytes are meaningful to them
/// (e.g. the immutable source bytes) — this crate does not perform
/// hashing itself, since deciding *what* gets hashed is an orchestration
/// concern (`mini-intake`, Track B2), not a vocabulary concern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntakeId(pub Multihash);

impl IntakeId {
    pub(crate) fn encode(&self, w: &mut Writer) {
        write_multihash(w, &self.0);
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self> {
        Ok(IntakeId(read_multihash(r)?))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.encode(&mut w);
        w.into_bytes()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let id = IntakeId::decode(&mut r)?;
        if !r.finished() {
            return Err(crate::error::IntakeError::TrailingBytes);
        }
        Ok(id)
    }
}

pub(crate) fn write_multihash(w: &mut Writer, mh: &Multihash) {
    w.bytes(&mh.to_bytes());
}

pub(crate) fn read_multihash(r: &mut Reader) -> Result<Multihash> {
    let mh_bytes = r.bytes_limited(MAX_MULTIHASH_BYTES)?;
    Ok(Multihash::from_bytes(&mh_bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_crypto::HashAlgorithm;

    #[test]
    fn an_intake_id_round_trips_through_bytes() {
        let mh = Multihash::of(HashAlgorithm::Blake3, b"hello world");
        let id = IntakeId(mh);
        let bytes = id.to_bytes();
        let decoded = IntakeId::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn trailing_bytes_after_an_intake_id_are_rejected() {
        let mh = Multihash::of(HashAlgorithm::Blake3, b"hello world");
        let id = IntakeId(mh);
        let mut bytes = id.to_bytes();
        bytes.push(0xFF);
        assert!(IntakeId::from_bytes(&bytes).is_err());
    }
}
