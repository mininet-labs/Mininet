//! [`AuthorityClass`]: how much project authority an intake object has
//! earned, ordered from none to canonical. This is the type-level
//! enforcement of the research report's core rule — "imported material
//! receives no project authority merely because Mininet can parse it"
//! (`docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_
//! SEARCH_20260718.md` §3.2/§34.6) — every [`crate::IntakeEnvelope`] is
//! constructed at [`AuthorityClass::UntrustedExternal`] and can only
//! reach a higher class through [`crate::IntakeEnvelope::promote_authority`],
//! never by direct field assignment.

/// Ordered from least to most authoritative. `#[non_exhaustive]` and
/// `Ord`: policy code can compare classes
/// (`class >= AuthorityClass::ReviewedEvidence`) without this crate
/// closing off future additions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum AuthorityClass {
    /// The default for every newly constructed envelope. A parser or
    /// extractor result stays here no matter how confidently it parsed.
    UntrustedExternal,
    /// A human has kept this for their own private reference. Not
    /// reviewed, not published, not project-linked.
    PersonalReference,
    /// Published where others can see it, still unreviewed as evidence
    /// or project input.
    PublicReference,
    /// A human or governed process has reviewed this material and
    /// accepted it as genuine evidence for some claim.
    ReviewedEvidence,
    /// Accepted as input to an actual project decision, issue, or
    /// deliverable.
    AcceptedProjectInput,
    /// Treated as canonical project material — the same weight as a
    /// document this repository authored directly. The highest class;
    /// reaching it requires an explicit governed promotion, never an
    /// automatic one.
    CanonicalProjectMaterial,
}

impl AuthorityClass {
    pub(crate) fn tag(self) -> u8 {
        match self {
            AuthorityClass::UntrustedExternal => 1,
            AuthorityClass::PersonalReference => 2,
            AuthorityClass::PublicReference => 3,
            AuthorityClass::ReviewedEvidence => 4,
            AuthorityClass::AcceptedProjectInput => 5,
            AuthorityClass::CanonicalProjectMaterial => 6,
        }
    }

    pub(crate) fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(AuthorityClass::UntrustedExternal),
            2 => Some(AuthorityClass::PersonalReference),
            3 => Some(AuthorityClass::PublicReference),
            4 => Some(AuthorityClass::ReviewedEvidence),
            5 => Some(AuthorityClass::AcceptedProjectInput),
            6 => Some(AuthorityClass::CanonicalProjectMaterial),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_classes_are_ordered_from_least_to_most_authoritative() {
        assert!(AuthorityClass::UntrustedExternal < AuthorityClass::PersonalReference);
        assert!(AuthorityClass::PersonalReference < AuthorityClass::PublicReference);
        assert!(AuthorityClass::PublicReference < AuthorityClass::ReviewedEvidence);
        assert!(AuthorityClass::ReviewedEvidence < AuthorityClass::AcceptedProjectInput);
        assert!(AuthorityClass::AcceptedProjectInput < AuthorityClass::CanonicalProjectMaterial);
    }

    #[test]
    fn every_class_round_trips_through_its_tag() {
        for class in [
            AuthorityClass::UntrustedExternal,
            AuthorityClass::PersonalReference,
            AuthorityClass::PublicReference,
            AuthorityClass::ReviewedEvidence,
            AuthorityClass::AcceptedProjectInput,
            AuthorityClass::CanonicalProjectMaterial,
        ] {
            assert_eq!(AuthorityClass::from_tag(class.tag()), Some(class));
        }
    }
}
