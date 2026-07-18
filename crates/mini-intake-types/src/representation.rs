//! Derived representations and the provenance of who produced them.
//! Carries the crate's core rule structurally: a [`DerivedRepresentation`]
//! is always paired with a [`DerivationRecord`] naming the exact
//! generator that produced it — there is no way to attach a
//! representation to an [`crate::IntakeEnvelope`] anonymously.

use crate::codec::{Reader, Writer};
use crate::error::{IntakeError, Result};
use crate::ids::{read_multihash, write_multihash};
use mini_crypto::Multihash;

const MAX_EXTRACTOR_ID_BYTES: usize = 256;
const MAX_EXTRACTOR_VERSION_BYTES: usize = 64;

/// What kind of derived output a representation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RepresentationKind {
    ExtractedText,
    Metadata,
    Preview,
    Thumbnail,
    StructuralMap,
}

impl RepresentationKind {
    fn tag(self) -> u8 {
        match self {
            RepresentationKind::ExtractedText => 1,
            RepresentationKind::Metadata => 2,
            RepresentationKind::Preview => 3,
            RepresentationKind::Thumbnail => 4,
            RepresentationKind::StructuralMap => 5,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            1 => Ok(RepresentationKind::ExtractedText),
            2 => Ok(RepresentationKind::Metadata),
            3 => Ok(RepresentationKind::Preview),
            4 => Ok(RepresentationKind::Thumbnail),
            5 => Ok(RepresentationKind::StructuralMap),
            _ => Err(IntakeError::BadRepresentationKind),
        }
    }
}

/// Which extractor produced a representation, and which version of it —
/// the identity behind [`crate::IntakeEnvelope`]'s "core rule" that a
/// parser's output always names its own producer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GeneratorIdentity {
    pub extractor_id: String,
    pub extractor_version: String,
}

/// One derived output of a source: its kind, content address, size, and
/// producer. The bytes themselves live in `mini-intake`'s storage layer
/// (Track B2) — this crate only carries the content address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedRepresentation {
    pub kind: RepresentationKind,
    pub digest: Multihash,
    pub byte_length: u64,
    pub generator: GeneratorIdentity,
    /// Whether re-running the same generator on the same source is
    /// expected to reproduce the same digest. `false` for extractors
    /// that are explicitly nondeterministic (e.g. OCR, per the research
    /// report's Phase B note) so callers never assume reproducibility
    /// that was never promised.
    pub deterministic: bool,
}

/// A provenance-log entry: when a representation was produced and by
/// whom. Kept separate from [`DerivedRepresentation`] itself because an
/// [`crate::IntakeEnvelope`]'s `provenance` list is the append-only
/// history, while `representations` is the current set — a
/// representation can be superseded (re-extracted with a newer
/// extractor version) without losing the record that the old one
/// existed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationRecord {
    pub representation: RepresentationKind,
    pub generator: GeneratorIdentity,
    pub produced_at_ms: u64,
}

fn encode_generator(w: &mut Writer, g: &GeneratorIdentity) {
    w.str(&g.extractor_id);
    w.str(&g.extractor_version);
}

fn decode_generator(r: &mut Reader) -> Result<GeneratorIdentity> {
    let extractor_id = r.str_limited(MAX_EXTRACTOR_ID_BYTES)?;
    let extractor_version = r.str_limited(MAX_EXTRACTOR_VERSION_BYTES)?;
    Ok(GeneratorIdentity {
        extractor_id,
        extractor_version,
    })
}

impl DerivedRepresentation {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.u8(self.kind.tag());
        write_multihash(w, &self.digest);
        w.u64(self.byte_length);
        encode_generator(w, &self.generator);
        w.bool(self.deterministic);
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self> {
        let kind = RepresentationKind::from_tag(r.u8()?)?;
        let digest = read_multihash(r)?;
        let byte_length = r.u64()?;
        let generator = decode_generator(r)?;
        let deterministic = r.bool()?;
        Ok(DerivedRepresentation {
            kind,
            digest,
            byte_length,
            generator,
            deterministic,
        })
    }
}

impl DerivationRecord {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.u8(self.representation.tag());
        encode_generator(w, &self.generator);
        w.u64(self.produced_at_ms);
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self> {
        let representation = RepresentationKind::from_tag(r.u8()?)?;
        let generator = decode_generator(r)?;
        let produced_at_ms = r.u64()?;
        Ok(DerivationRecord {
            representation,
            generator,
            produced_at_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_crypto::HashAlgorithm;

    fn sample_generator() -> GeneratorIdentity {
        GeneratorIdentity {
            extractor_id: "mini-extractor-text".to_string(),
            extractor_version: "0.0.1".to_string(),
        }
    }

    #[test]
    fn a_derived_representation_round_trips() {
        let rep = DerivedRepresentation {
            kind: RepresentationKind::ExtractedText,
            digest: Multihash::of(HashAlgorithm::Blake3, b"extracted text"),
            byte_length: 128,
            generator: sample_generator(),
            deterministic: true,
        };
        let mut w = Writer::new();
        rep.encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let decoded = DerivedRepresentation::decode(&mut r).unwrap();
        assert!(r.finished());
        assert_eq!(decoded, rep);
    }

    #[test]
    fn a_derivation_record_round_trips() {
        let rec = DerivationRecord {
            representation: RepresentationKind::Preview,
            generator: sample_generator(),
            produced_at_ms: 42,
        };
        let mut w = Writer::new();
        rec.encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let decoded = DerivationRecord::decode(&mut r).unwrap();
        assert!(r.finished());
        assert_eq!(decoded, rec);
    }

    #[test]
    fn every_representation_kind_round_trips_through_its_tag() {
        for kind in [
            RepresentationKind::ExtractedText,
            RepresentationKind::Metadata,
            RepresentationKind::Preview,
            RepresentationKind::Thumbnail,
            RepresentationKind::StructuralMap,
        ] {
            assert_eq!(RepresentationKind::from_tag(kind.tag()).unwrap(), kind);
        }
    }

    #[test]
    fn an_unrecognized_representation_kind_tag_is_rejected() {
        assert_eq!(
            RepresentationKind::from_tag(200),
            Err(IntakeError::BadRepresentationKind)
        );
    }
}
