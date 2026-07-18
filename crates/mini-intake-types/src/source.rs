//! [`SourceRecord`]: the immutable description of the exact bytes Mininet
//! Intake received. This crate never stores the bytes themselves —
//! that's `mini-intake`'s job (Track B2) — only their content address
//! and declared metadata.

use mini_crypto::Multihash;

use crate::codec::{Reader, Writer};
use crate::error::Result;
use crate::ids::{read_multihash, write_multihash};
use crate::media::MediaType;

const MAX_DECLARED_NAME_BYTES: usize = 1024;

/// The immutable record of one received source's identity — never the
/// bytes themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRecord {
    pub digest: Multihash,
    pub media_type: MediaType,
    pub byte_length: u64,
    pub received_at_ms: u64,
    pub declared_name: Option<String>,
}

impl SourceRecord {
    pub(crate) fn encode(&self, w: &mut Writer) {
        write_multihash(w, &self.digest);
        self.media_type.encode(w);
        w.u64(self.byte_length);
        w.u64(self.received_at_ms);
        match &self.declared_name {
            Some(name) => {
                w.bool(true);
                w.str(name);
            }
            None => w.bool(false),
        }
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self> {
        let digest = read_multihash(r)?;
        let media_type = MediaType::decode(r)?;
        let byte_length = r.u64()?;
        let received_at_ms = r.u64()?;
        let declared_name = if r.bool()? {
            Some(r.str_limited(MAX_DECLARED_NAME_BYTES)?)
        } else {
            None
        };
        Ok(SourceRecord {
            digest,
            media_type,
            byte_length,
            received_at_ms,
            declared_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_crypto::HashAlgorithm;

    fn sample() -> SourceRecord {
        SourceRecord {
            digest: Multihash::of(HashAlgorithm::Blake3, b"source bytes"),
            media_type: MediaType::Pdf,
            byte_length: 4096,
            received_at_ms: 1_752_800_000_000,
            declared_name: Some("report.pdf".to_string()),
        }
    }

    #[test]
    fn a_source_record_round_trips_with_a_declared_name() {
        let record = sample();
        let mut w = Writer::new();
        record.encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let decoded = SourceRecord::decode(&mut r).unwrap();
        assert!(r.finished());
        assert_eq!(decoded, record);
    }

    #[test]
    fn a_source_record_round_trips_without_a_declared_name() {
        let mut record = sample();
        record.declared_name = None;
        let mut w = Writer::new();
        record.encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let decoded = SourceRecord::decode(&mut r).unwrap();
        assert!(r.finished());
        assert_eq!(decoded, record);
    }
}
