//! [`IntakeEnvelope`]: the top-level record tying a source to its
//! derived representations, provenance, warnings, review state,
//! authority class, and links. Structurally enforces this crate's core
//! rule (research report §3.2): *derived text is not the source,
//! automated classification is not judgment, and imported material
//! receives no project authority merely because Mininet can parse it.*
//!
//! Every envelope is constructed at [`ReviewState::Unreviewed`] and
//! [`AuthorityClass::UntrustedExternal`] — there is no constructor or
//! setter that starts anywhere else. The only way forward is through
//! [`IntakeEnvelope::advance_review_state`] and
//! [`IntakeEnvelope::promote_authority`], which enforce legal
//! transitions and the review-before-authority gate respectively.

use crate::authority::AuthorityClass;
use crate::codec::{Reader, Writer};
use crate::error::{IntakeError, Result};
use crate::ids::IntakeId;
use crate::link::IntakeLink;
use crate::representation::{DerivationRecord, DerivedRepresentation};
use crate::review::ReviewState;
use crate::source::SourceRecord;
use crate::warning::IntakeWarning;

/// This crate's envelope wire-format version.
pub const ENVELOPE_VERSION: u16 = 1;

const MAX_REPRESENTATIONS: usize = 64;
const MAX_WARNINGS: usize = 256;
const MAX_LINKS: usize = 256;

/// The full record of one piece of external material's journey through
/// Mininet Intake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntakeEnvelope {
    version: u16,
    pub intake_id: IntakeId,
    pub source: SourceRecord,
    representations: Vec<DerivedRepresentation>,
    provenance: Vec<DerivationRecord>,
    warnings: Vec<IntakeWarning>,
    review_state: ReviewState,
    authority: AuthorityClass,
    links: Vec<IntakeLink>,
}

impl IntakeEnvelope {
    /// Construct a new envelope. Always starts `Unreviewed` and
    /// `UntrustedExternal` — by construction, not convention.
    pub fn new(intake_id: IntakeId, source: SourceRecord) -> Self {
        IntakeEnvelope {
            version: ENVELOPE_VERSION,
            intake_id,
            source,
            representations: Vec::new(),
            provenance: Vec::new(),
            warnings: Vec::new(),
            review_state: ReviewState::Unreviewed,
            authority: AuthorityClass::UntrustedExternal,
            links: Vec::new(),
        }
    }

    pub fn review_state(&self) -> ReviewState {
        self.review_state
    }

    pub fn authority(&self) -> AuthorityClass {
        self.authority
    }

    pub fn representations(&self) -> &[DerivedRepresentation] {
        &self.representations
    }

    pub fn provenance(&self) -> &[DerivationRecord] {
        &self.provenance
    }

    pub fn warnings(&self) -> &[IntakeWarning] {
        &self.warnings
    }

    pub fn links(&self) -> &[IntakeLink] {
        &self.links
    }

    /// Attach a newly produced representation and its provenance record
    /// together — there is no way to add one without the other, so a
    /// representation can never appear without naming who produced it.
    pub fn add_representation(
        &mut self,
        representation: DerivedRepresentation,
        derivation: DerivationRecord,
    ) {
        self.representations.push(representation);
        self.provenance.push(derivation);
    }

    pub fn add_warning(&mut self, warning: IntakeWarning) {
        self.warnings.push(warning);
    }

    pub fn add_link(&mut self, link: IntakeLink) {
        self.links.push(link);
    }

    /// Move to `next` review state. Fails if `next` is not a legal
    /// transition from the current state (see
    /// [`ReviewState::allows_transition_to`]) — a caller cannot jump
    /// straight to `Accepted`, and cannot leave a terminal `Rejected`/
    /// `Superseded` state.
    pub fn advance_review_state(&mut self, next: ReviewState) -> Result<()> {
        if !self.review_state.allows_transition_to(next) {
            return Err(IntakeError::InvalidReviewTransition);
        }
        self.review_state = next;
        Ok(())
    }

    /// Promote to a higher [`AuthorityClass`]. Fails if `next` is not
    /// strictly higher than the current class, and — the structural
    /// enforcement of "a parser cannot assign authority" — fails for
    /// any class at or above [`AuthorityClass::ReviewedEvidence`] unless
    /// the envelope's review state is already [`ReviewState::Accepted`].
    pub fn promote_authority(&mut self, next: AuthorityClass) -> Result<()> {
        if next <= self.authority {
            return Err(IntakeError::InvalidAuthorityPromotion);
        }
        if next >= AuthorityClass::ReviewedEvidence && self.review_state != ReviewState::Accepted {
            return Err(IntakeError::InvalidAuthorityPromotion);
        }
        self.authority = next;
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u16(self.version);
        self.intake_id.encode(&mut w);
        self.source.encode(&mut w);
        w.u32(self.representations.len() as u32);
        for rep in &self.representations {
            rep.encode(&mut w);
        }
        w.u32(self.provenance.len() as u32);
        for prov in &self.provenance {
            prov.encode(&mut w);
        }
        w.u32(self.warnings.len() as u32);
        for warning in &self.warnings {
            warning.encode(&mut w);
        }
        w.u8(self.review_state.tag());
        w.u8(self.authority.tag());
        w.u32(self.links.len() as u32);
        for link in &self.links {
            link.encode(&mut w);
        }
        w.into_bytes()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let version = r.u16()?;
        if version != ENVELOPE_VERSION {
            return Err(IntakeError::UnsupportedVersion);
        }
        let intake_id = IntakeId::decode(&mut r)?;

        let source = SourceRecord::decode(&mut r)?;

        let n_reps = r.u32()? as usize;
        if n_reps > MAX_REPRESENTATIONS {
            return Err(IntakeError::LimitExceeded);
        }
        let mut representations = Vec::with_capacity(n_reps);
        for _ in 0..n_reps {
            representations.push(DerivedRepresentation::decode(&mut r)?);
        }

        let n_prov = r.u32()? as usize;
        if n_prov > MAX_REPRESENTATIONS {
            return Err(IntakeError::LimitExceeded);
        }
        let mut provenance = Vec::with_capacity(n_prov);
        for _ in 0..n_prov {
            provenance.push(DerivationRecord::decode(&mut r)?);
        }

        let n_warn = r.u32()? as usize;
        if n_warn > MAX_WARNINGS {
            return Err(IntakeError::LimitExceeded);
        }
        let mut warnings = Vec::with_capacity(n_warn);
        for _ in 0..n_warn {
            warnings.push(IntakeWarning::decode(&mut r)?);
        }

        let review_state = ReviewState::from_tag(r.u8()?)?;
        let authority = AuthorityClass::from_tag(r.u8()?).ok_or(IntakeError::BadAuthorityClass)?;

        let n_links = r.u32()? as usize;
        if n_links > MAX_LINKS {
            return Err(IntakeError::LimitExceeded);
        }
        let mut links = Vec::with_capacity(n_links);
        for _ in 0..n_links {
            links.push(IntakeLink::decode(&mut r)?);
        }

        if !r.finished() {
            return Err(IntakeError::TrailingBytes);
        }

        Ok(IntakeEnvelope {
            version,
            intake_id,
            source,
            representations,
            provenance,
            warnings,
            review_state,
            authority,
            links,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaType;
    use crate::representation::{GeneratorIdentity, RepresentationKind};
    use mini_crypto::{HashAlgorithm, Multihash};

    fn sample_source() -> SourceRecord {
        SourceRecord {
            digest: Multihash::of(HashAlgorithm::Blake3, b"source bytes"),
            media_type: MediaType::Pdf,
            byte_length: 4096,
            received_at_ms: 1_752_800_000_000,
            declared_name: Some("report.pdf".to_string()),
        }
    }

    fn sample_id() -> IntakeId {
        IntakeId(Multihash::of(HashAlgorithm::Blake3, b"intake-id-seed"))
    }

    #[test]
    fn a_new_envelope_starts_unreviewed_and_untrusted() {
        let envelope = IntakeEnvelope::new(sample_id(), sample_source());
        assert_eq!(envelope.review_state(), ReviewState::Unreviewed);
        assert_eq!(envelope.authority(), AuthorityClass::UntrustedExternal);
        assert!(envelope.representations().is_empty());
        assert!(envelope.warnings().is_empty());
        assert!(envelope.links().is_empty());
    }

    #[test]
    fn adding_a_representation_also_records_its_provenance() {
        let mut envelope = IntakeEnvelope::new(sample_id(), sample_source());
        let generator = GeneratorIdentity {
            extractor_id: "mini-extractor-text".to_string(),
            extractor_version: "0.0.1".to_string(),
        };
        envelope.add_representation(
            DerivedRepresentation {
                kind: RepresentationKind::ExtractedText,
                digest: Multihash::of(HashAlgorithm::Blake3, b"extracted"),
                byte_length: 64,
                generator: generator.clone(),
                deterministic: true,
            },
            DerivationRecord {
                representation: RepresentationKind::ExtractedText,
                generator,
                produced_at_ms: 1,
            },
        );
        assert_eq!(envelope.representations().len(), 1);
        assert_eq!(envelope.provenance().len(), 1);
    }

    #[test]
    fn review_state_cannot_jump_straight_to_accepted() {
        let mut envelope = IntakeEnvelope::new(sample_id(), sample_source());
        assert_eq!(
            envelope.advance_review_state(ReviewState::Accepted),
            Err(IntakeError::InvalidReviewTransition)
        );
    }

    #[test]
    fn review_state_follows_the_ordinary_happy_path() {
        let mut envelope = IntakeEnvelope::new(sample_id(), sample_source());
        envelope
            .advance_review_state(ReviewState::UnderReview)
            .unwrap();
        envelope
            .advance_review_state(ReviewState::Accepted)
            .unwrap();
        assert_eq!(envelope.review_state(), ReviewState::Accepted);
    }

    #[test]
    fn rejected_review_state_cannot_transition_further() {
        let mut envelope = IntakeEnvelope::new(sample_id(), sample_source());
        envelope
            .advance_review_state(ReviewState::UnderReview)
            .unwrap();
        envelope
            .advance_review_state(ReviewState::Rejected)
            .unwrap();
        assert_eq!(
            envelope.advance_review_state(ReviewState::UnderReview),
            Err(IntakeError::InvalidReviewTransition)
        );
    }

    #[test]
    fn authority_cannot_reach_reviewed_evidence_without_an_accepted_review() {
        let mut envelope = IntakeEnvelope::new(sample_id(), sample_source());
        assert_eq!(
            envelope.promote_authority(AuthorityClass::ReviewedEvidence),
            Err(IntakeError::InvalidAuthorityPromotion)
        );
    }

    #[test]
    fn authority_can_reach_public_reference_without_any_review() {
        let mut envelope = IntakeEnvelope::new(sample_id(), sample_source());
        envelope
            .promote_authority(AuthorityClass::PublicReference)
            .unwrap();
        assert_eq!(envelope.authority(), AuthorityClass::PublicReference);
    }

    #[test]
    fn authority_reaches_reviewed_evidence_after_an_accepted_review() {
        let mut envelope = IntakeEnvelope::new(sample_id(), sample_source());
        envelope
            .advance_review_state(ReviewState::UnderReview)
            .unwrap();
        envelope
            .advance_review_state(ReviewState::Accepted)
            .unwrap();
        envelope
            .promote_authority(AuthorityClass::ReviewedEvidence)
            .unwrap();
        assert_eq!(envelope.authority(), AuthorityClass::ReviewedEvidence);
    }

    #[test]
    fn authority_cannot_be_demoted_or_repeated() {
        let mut envelope = IntakeEnvelope::new(sample_id(), sample_source());
        envelope
            .promote_authority(AuthorityClass::PublicReference)
            .unwrap();
        assert_eq!(
            envelope.promote_authority(AuthorityClass::PersonalReference),
            Err(IntakeError::InvalidAuthorityPromotion)
        );
        assert_eq!(
            envelope.promote_authority(AuthorityClass::PublicReference),
            Err(IntakeError::InvalidAuthorityPromotion)
        );
    }

    #[test]
    fn an_envelope_round_trips_through_bytes_with_representations_warnings_and_links() {
        let mut envelope = IntakeEnvelope::new(sample_id(), sample_source());
        let generator = GeneratorIdentity {
            extractor_id: "mini-extractor-text".to_string(),
            extractor_version: "0.0.1".to_string(),
        };
        envelope.add_representation(
            DerivedRepresentation {
                kind: RepresentationKind::ExtractedText,
                digest: Multihash::of(HashAlgorithm::Blake3, b"extracted"),
                byte_length: 64,
                generator: generator.clone(),
                deterministic: true,
            },
            DerivationRecord {
                representation: RepresentationKind::ExtractedText,
                generator,
                produced_at_ms: 1,
            },
        );
        envelope.add_warning(IntakeWarning {
            code: "malformed-pdf-xref".to_string(),
            message: "recovered via linear scan".to_string(),
        });
        envelope.add_link(IntakeLink::Issue(152));
        envelope
            .advance_review_state(ReviewState::UnderReview)
            .unwrap();

        let bytes = envelope.to_bytes();
        let decoded = IntakeEnvelope::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn an_unsupported_version_is_rejected() {
        let envelope = IntakeEnvelope::new(sample_id(), sample_source());
        let mut bytes = envelope.to_bytes();
        bytes[0] = 0xFF;
        bytes[1] = 0xFF;
        assert_eq!(
            IntakeEnvelope::from_bytes(&bytes),
            Err(IntakeError::UnsupportedVersion)
        );
    }

    #[test]
    fn trailing_bytes_after_a_complete_decode_are_rejected() {
        let envelope = IntakeEnvelope::new(sample_id(), sample_source());
        let mut bytes = envelope.to_bytes();
        bytes.push(0xFF);
        assert_eq!(
            IntakeEnvelope::from_bytes(&bytes),
            Err(IntakeError::TrailingBytes)
        );
    }

    #[test]
    fn truncated_bytes_are_rejected_not_panicking() {
        let envelope = IntakeEnvelope::new(sample_id(), sample_source());
        let bytes = envelope.to_bytes();
        for cut in 0..bytes.len() {
            let result = IntakeEnvelope::from_bytes(&bytes[..cut]);
            assert!(result.is_err(), "expected an error truncating at {cut}");
        }
    }

    #[test]
    fn an_oversized_representation_count_is_rejected_before_allocating() {
        let mut w = Writer::new();
        w.u16(ENVELOPE_VERSION);
        sample_id().encode(&mut w);
        sample_source().encode(&mut w);
        w.u32((MAX_REPRESENTATIONS + 1) as u32);
        let bytes = w.into_bytes();
        assert_eq!(
            IntakeEnvelope::from_bytes(&bytes),
            Err(IntakeError::LimitExceeded)
        );
    }
}
