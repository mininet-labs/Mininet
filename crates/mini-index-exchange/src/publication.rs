//! A provider's signed publication of an index segment, and the
//! verification that lets any node accept it without trusting the provider.
//!
//! The trust argument has two independent legs, and a publication is only
//! accepted if **both** hold:
//!
//! 1. **Content address.** An [`mini_lexical_index::IndexSegment`] has a
//!    BLAKE3 `segment_id` over its canonical bytes. A receiver re-derives
//!    that id from the bytes it was given and checks it equals the id in the
//!    publication. So the provider cannot publish an id for content it did
//!    not actually produce — the bytes are their own proof.
//! 2. **Signature.** The provider signs the manifest with their key. A
//!    receiver verifies the signature and derives the provider's
//!    [`ProviderPseudonym`] from the verifying key. So a third party cannot
//!    forge a publication in a provider's name.
//!
//! Together: "provider P published exactly this segment" is verifiable from
//! bytes alone. This is the mechanism behind D-0312's plurality — many
//! providers publish index segments built from the same observations, and
//! anyone caches, replicates, and compares them by id without trusting who
//! sent them.
//!
//! A publication is a *provenance attestation*. It is not a ranking,
//! authority, or payment signal, and carries no balance, stake, or weight
//! (Directive 16). Which of several published segments a searcher uses is a
//! ranking/selection choice made elsewhere, not something a publication can
//! buy.

use mini_crypto::{HashAlgorithm, Multihash, Signature, SigningKey, VerifyingKey};
use mini_lexical_index::{IndexManifest, IndexSegment, IndexSegmentId};
use mini_web_types::ProviderPseudonym;

use crate::codec::{Reader, Writer};
use crate::error::{ExchangeError, Result};

/// Domain-separation prefix for the signed message. Prevents a signature
/// made here from being replayed as a signature in any other protocol, and
/// vice versa.
const SIGNING_DOMAIN: &[u8] = b"mini-index-exchange/v1/segment-publication";

const MAX_KEY_BYTES: usize = 4096;
const MAX_SIG_BYTES: usize = 8192;
const MAX_MULTIHASH_BYTES: usize = 128;

/// A signed statement: "I, this provider, publish the index segment
/// described by this manifest."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentPublication {
    /// The published segment's manifest — its content address plus shape.
    pub manifest: IndexManifest,
    /// The publisher's verifying key. Its hash is the provider pseudonym.
    pub publisher: VerifyingKey,
    /// Signature over the domain-separated canonical manifest.
    pub signature: Signature,
}

/// The result of verifying a publication: who published it, and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPublication {
    /// The provider's pseudonym, derived as the BLAKE3 digest of the
    /// verifying key. A provider is exactly the party who holds the key.
    pub provider: ProviderPseudonym,
    pub manifest: IndexManifest,
}

/// Canonically encode a manifest for signing. Byte-identical on both sides,
/// so a signature made by `publish` verifies in `verify`.
fn manifest_bytes(manifest: &IndexManifest) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(manifest.format_version);
    w.u32(manifest.document_count);
    w.u32(manifest.term_count);
    w.bytes(&manifest.segment_id.0.to_bytes());
    w.into_bytes()
}

/// The exact message that is signed: the domain prefix followed by the
/// canonical manifest bytes.
fn signed_message(manifest: &IndexManifest) -> Vec<u8> {
    let mut msg = Vec::with_capacity(SIGNING_DOMAIN.len() + 32);
    msg.extend_from_slice(SIGNING_DOMAIN);
    msg.extend_from_slice(&manifest_bytes(manifest));
    msg
}

/// Derive a provider pseudonym from a verifying key: the BLAKE3 digest of
/// the key's bytes. Deterministic and unlinkable to any governance identity
/// (a pseudonym is not an account or a vote).
pub fn provider_pseudonym(key: &VerifyingKey) -> ProviderPseudonym {
    ProviderPseudonym(Multihash::of(HashAlgorithm::Blake3, &key.to_bytes()))
}

impl SegmentPublication {
    /// Sign and publish a segment's manifest.
    pub fn publish(manifest: IndexManifest, key: &SigningKey) -> Self {
        let signature = key.sign(&signed_message(&manifest));
        SegmentPublication {
            manifest,
            publisher: key.verifying_key(),
            signature,
        }
    }

    /// Verify the publisher's signature over the manifest and return who
    /// published it. This proves authorship of the *claim*; it does not by
    /// itself prove the claim's `segment_id` matches any particular bytes —
    /// use [`SegmentPublication::verify_segment`] for that.
    pub fn verify(&self) -> Result<VerifiedPublication> {
        self.publisher
            .verify(&signed_message(&self.manifest), &self.signature)
            .map_err(|_| ExchangeError::BadSignature)?;
        Ok(VerifiedPublication {
            provider: provider_pseudonym(&self.publisher),
            manifest: self.manifest.clone(),
        })
    }

    /// Verify the signature **and** that `segment` is exactly the published
    /// segment: its re-derived content address must equal the published
    /// `segment_id`, and its actual shape must equal the published manifest.
    /// This is the full check a receiver runs before trusting the segment.
    pub fn verify_segment(&self, segment: &IndexSegment) -> Result<VerifiedPublication> {
        let verified = self.verify()?;
        if segment.segment_id() != self.manifest.segment_id {
            return Err(ExchangeError::SegmentIdMismatch);
        }
        if segment.manifest() != self.manifest {
            return Err(ExchangeError::ManifestMismatch);
        }
        Ok(verified)
    }

    /// Canonical wire encoding.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        // Manifest.
        w.u8(self.manifest.format_version);
        w.u32(self.manifest.document_count);
        w.u32(self.manifest.term_count);
        w.bytes(&self.manifest.segment_id.0.to_bytes());
        // Publisher key: suite tag + bytes.
        w.u8(self.publisher.suite().tag());
        w.bytes(&self.publisher.to_bytes());
        // Signature: suite tag + bytes.
        w.u8(self.signature.suite().tag());
        w.bytes(&self.signature.to_bytes());
        w.into_bytes()
    }

    /// Decode a publication from untrusted bytes. Bounded before allocation;
    /// key and signature suites are parsed through `mini-crypto`, which
    /// rejects unknown suites and wrong-length material.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        use mini_crypto::SignatureSuite;

        let mut r = Reader::new(bytes);
        let format_version = r.u8()?;
        let document_count = r.u32()?;
        let term_count = r.u32()?;
        let segment_id = IndexSegmentId(Multihash::from_bytes(
            &r.bytes_limited(MAX_MULTIHASH_BYTES)?,
        )?);
        let manifest = IndexManifest {
            format_version,
            document_count,
            term_count,
            segment_id,
        };

        let key_suite = SignatureSuite::from_tag(r.u8()?)?;
        let key_bytes = r.bytes_limited(MAX_KEY_BYTES)?;
        let publisher = VerifyingKey::from_suite_bytes(key_suite, &key_bytes)?;

        let sig_suite = SignatureSuite::from_tag(r.u8()?)?;
        let sig_bytes = r.bytes_limited(MAX_SIG_BYTES)?;
        let signature = Signature::from_suite_bytes(sig_suite, &sig_bytes)?;

        if !r.finished() {
            return Err(ExchangeError::TrailingBytes);
        }
        Ok(SegmentPublication {
            manifest,
            publisher,
            signature,
        })
    }
}

/// The full receive path: given untrusted segment bytes and untrusted
/// publication bytes (e.g. from a peer), decode both, verify the signature,
/// and verify the segment matches the published content address. Returns the
/// validated segment and who published it, or an error naming the exact
/// failure — never a partially trusted result.
pub fn accept_published_segment(
    segment_bytes: &[u8],
    publication_bytes: &[u8],
) -> Result<(IndexSegment, VerifiedPublication)> {
    // Decoding the segment already enforces its canonical form.
    let segment = IndexSegment::from_bytes(segment_bytes)?;
    let publication = SegmentPublication::from_bytes(publication_bytes)?;
    let verified = publication.verify_segment(&segment)?;
    Ok((segment, verified))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_crypto::SigningKey;
    use mini_lexical_index::{Field, IndexBuilder, UrlId};

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_seed(&[seed; 32])
    }

    fn url(seed: &[u8]) -> UrlId {
        UrlId(Multihash::of(HashAlgorithm::Blake3, seed))
    }

    fn sample_segment() -> IndexSegment {
        let mut b = IndexBuilder::new();
        b.add_document(
            url(b"https://example.org/a"),
            &[(Field::Title, "alpha beta"), (Field::Body, "beta gamma")],
        );
        b.add_document(
            url(b"https://example.org/b"),
            &[(Field::Body, "gamma delta epsilon")],
        );
        b.build()
    }

    #[test]
    fn a_published_segment_round_trips_and_verifies() {
        let seg = sample_segment();
        let pubn = SegmentPublication::publish(seg.manifest(), &key(1));

        // Signature verifies and names the right provider.
        let verified = pubn.verify().unwrap();
        assert_eq!(
            verified.provider,
            provider_pseudonym(&key(1).verifying_key())
        );

        // Full segment check passes for the real bytes.
        pubn.verify_segment(&seg).unwrap();

        // Wire round trip preserves everything.
        let decoded = SegmentPublication::from_bytes(&pubn.to_bytes()).unwrap();
        assert_eq!(decoded, pubn);
        decoded.verify_segment(&seg).unwrap();
    }

    #[test]
    fn the_full_accept_path_verifies_untrusted_bytes() {
        let seg = sample_segment();
        let pubn = SegmentPublication::publish(seg.manifest(), &key(7));
        let (got_seg, verified) =
            accept_published_segment(&seg.to_bytes(), &pubn.to_bytes()).unwrap();
        assert_eq!(got_seg.segment_id(), seg.segment_id());
        assert_eq!(
            verified.provider,
            provider_pseudonym(&key(7).verifying_key())
        );
    }

    #[test]
    fn a_forged_signature_is_rejected() {
        // Publisher key 1 signs, but we swap in key 2's identity: verifying
        // against the wrong key must fail.
        let seg = sample_segment();
        let mut pubn = SegmentPublication::publish(seg.manifest(), &key(1));
        pubn.publisher = key(2).verifying_key();
        assert_eq!(pubn.verify(), Err(ExchangeError::BadSignature));
    }

    #[test]
    fn tampered_manifest_breaks_the_signature() {
        let seg = sample_segment();
        let mut pubn = SegmentPublication::publish(seg.manifest(), &key(1));
        // Alter the signed content: the signature no longer matches.
        pubn.manifest.document_count += 1;
        assert_eq!(pubn.verify(), Err(ExchangeError::BadSignature));
    }

    #[test]
    fn segment_bytes_not_matching_the_published_id_are_rejected() {
        // Publish segment A's manifest, then try to pass off segment B's
        // bytes under it. The content address will not match.
        let seg_a = sample_segment();
        let mut b = IndexBuilder::new();
        b.add_document(
            url(b"https://other/x"),
            &[(Field::Body, "totally different")],
        );
        let seg_b = b.build();

        let pubn = SegmentPublication::publish(seg_a.manifest(), &key(1));
        assert_eq!(
            pubn.verify_segment(&seg_b),
            Err(ExchangeError::SegmentIdMismatch)
        );
        // And through the full accept path with mismatched bytes.
        assert_eq!(
            accept_published_segment(&seg_b.to_bytes(), &pubn.to_bytes()),
            Err(ExchangeError::SegmentIdMismatch)
        );
    }

    #[test]
    fn two_providers_publishing_the_same_segment_are_distinct_but_both_valid() {
        // The plurality property: the same content-addressed segment can be
        // published by different providers; both publications verify, and
        // they name different providers over the identical segment id.
        let seg = sample_segment();
        let p1 = SegmentPublication::publish(seg.manifest(), &key(1));
        let p2 = SegmentPublication::publish(seg.manifest(), &key(2));

        let v1 = p1.verify_segment(&seg).unwrap();
        let v2 = p2.verify_segment(&seg).unwrap();
        assert_eq!(v1.manifest.segment_id, v2.manifest.segment_id);
        assert_ne!(v1.provider, v2.provider);
    }

    #[test]
    fn trailing_and_truncated_publication_bytes_are_rejected() {
        let seg = sample_segment();
        let pubn = SegmentPublication::publish(seg.manifest(), &key(1));
        let bytes = pubn.to_bytes();

        let mut extra = bytes.clone();
        extra.push(0);
        assert_eq!(
            SegmentPublication::from_bytes(&extra),
            Err(ExchangeError::TrailingBytes)
        );

        for cut in 0..bytes.len() {
            assert!(SegmentPublication::from_bytes(&bytes[..cut]).is_err());
        }
    }

    #[test]
    fn a_manifest_with_the_right_id_but_wrong_shape_is_rejected() {
        // Same segment id, but a manifest claiming a different document
        // count. verify_segment must catch the shape disagreement even
        // though the id matches and the signature is valid.
        let seg = sample_segment();
        let mut manifest = seg.manifest();
        let real_count = manifest.document_count;
        manifest.document_count = real_count + 5;
        let pubn = SegmentPublication::publish(manifest, &key(1));
        // Signature verifies (it signed the tampered manifest)...
        pubn.verify().unwrap();
        // ...but the segment's real shape does not match the manifest.
        assert_eq!(
            pubn.verify_segment(&seg),
            Err(ExchangeError::ManifestMismatch)
        );
    }
}
