//! Publication profile dimensions (D-0364, Track D1, founder research
//! `docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §27: "Implement visibility, attribution, transport, and persistence as
//! independent choices.")
//!
//! [`PublicationProfile`] deliberately does not cross-validate its four
//! fields against each other. A caller can construct any of the 3 x 2 x 4
//! x 3 = 72 combinations -- including ones that look unwise, like
//! `Visibility::Public` paired with `Attribution::Anonymous` at
//! `PrivacyTier::Direct` -- because the research doctrine's own framing is
//! that these are *independent* publication-time choices, not a coupled
//! state machine this crate should silently constrain on a caller's
//! behalf. Whether a chosen combination actually achieves what a caller
//! wants is a separate question [`crate::achieved_result_receipt_for`]
//! (Track D2) answers, by routing through `mini-transport-policy`'s own
//! fail-closed property check -- it is not baked into construction here.

use mini_privacy_policy::PrivacyTier;

/// Who can discover and read a published object. Independent of
/// [`Attribution`]: a `Public` object can still be `Anonymous`, and a
/// `Private` object can still be `Attributed` (attribution is about
/// whether the publisher's identity root is disclosed, not about who can
/// see the content).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Discoverable and readable by anyone.
    Public,
    /// Readable by anyone with the object's address, but not indexed or
    /// surfaced through discovery paths.
    Unlisted,
    /// Readable only by parties the publisher has explicitly granted
    /// access to.
    Private,
}

/// Whether the publishing identity root is disclosed alongside the
/// object. Independent of [`Visibility`] and [`PublicationProfile::
/// transport`] -- see this module's own doc comment for why this crate
/// does not couple them structurally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribution {
    /// The publisher's identity root (or a pairwise pseudonym derived
    /// from it) is disclosed alongside the object.
    Attributed,
    /// No publisher identity is disclosed alongside the object. This is
    /// a publication-time *declaration of intent*, not a proof: whether
    /// it is actually achievable depends on the chosen transport tier,
    /// which [`crate::achieved_result_receipt_for`] checks.
    Anonymous,
}

/// How long a published object is expected to remain retrievable, and
/// under what replication posture. Independent of [`Visibility`] and
/// [`Attribution`] -- an `Ephemeral` object can be `Public`, and a
/// `Replicated` object can be `Private`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persistence {
    /// No durability guarantee beyond the publisher's own online window.
    Ephemeral,
    /// Committed to durable local storage, single copy.
    Durable,
    /// Committed with erasure-coded replication across multiple
    /// providers (`mini-erasure`/`mini-porep`'s domain -- this crate
    /// declares the *choice*, not the replication mechanism itself).
    Replicated,
}

/// The four independent publication-time choices Track D1 asks for.
/// `transport` reuses [`mini_privacy_policy::PrivacyTier`] directly
/// rather than inventing a parallel vocabulary -- it is exactly the same
/// choice a `TransportRequest` already makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicationProfile {
    pub visibility: Visibility,
    pub attribution: Attribution,
    pub transport: PrivacyTier,
    pub persistence: Persistence,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_visibilities() -> [Visibility; 3] {
        [
            Visibility::Public,
            Visibility::Unlisted,
            Visibility::Private,
        ]
    }

    fn all_attributions() -> [Attribution; 2] {
        [Attribution::Attributed, Attribution::Anonymous]
    }

    fn all_transports() -> [PrivacyTier; 4] {
        [
            PrivacyTier::Direct,
            PrivacyTier::Relayed,
            PrivacyTier::Mixed,
            PrivacyTier::Burst,
        ]
    }

    fn all_persistences() -> [Persistence; 3] {
        [
            Persistence::Ephemeral,
            Persistence::Durable,
            Persistence::Replicated,
        ]
    }

    #[test]
    fn every_combination_of_the_four_dimensions_constructs_with_no_cross_validation() {
        let mut count = 0;
        for &visibility in &all_visibilities() {
            for &attribution in &all_attributions() {
                for &transport in &all_transports() {
                    for &persistence in &all_persistences() {
                        let _profile = PublicationProfile {
                            visibility,
                            attribution,
                            transport,
                            persistence,
                        };
                        count += 1;
                    }
                }
            }
        }
        assert_eq!(count, 3 * 2 * 4 * 3);
    }

    #[test]
    fn public_visibility_combines_with_anonymous_attribution() {
        let profile = PublicationProfile {
            visibility: Visibility::Public,
            attribution: Attribution::Anonymous,
            transport: PrivacyTier::Direct,
            persistence: Persistence::Ephemeral,
        };
        assert_eq!(profile.visibility, Visibility::Public);
        assert_eq!(profile.attribution, Attribution::Anonymous);
    }

    #[test]
    fn private_visibility_combines_with_attributed_attribution() {
        let profile = PublicationProfile {
            visibility: Visibility::Private,
            attribution: Attribution::Attributed,
            transport: PrivacyTier::Burst,
            persistence: Persistence::Replicated,
        };
        assert_eq!(profile.visibility, Visibility::Private);
        assert_eq!(profile.attribution, Attribution::Attributed);
    }

    #[test]
    fn ephemeral_persistence_combines_with_replicated_grade_transport() {
        // Persistence and transport are independent: a caller can ask for
        // Burst-tier transport privacy without asking for durable storage.
        let profile = PublicationProfile {
            visibility: Visibility::Unlisted,
            attribution: Attribution::Anonymous,
            transport: PrivacyTier::Burst,
            persistence: Persistence::Ephemeral,
        };
        assert_eq!(profile.transport, PrivacyTier::Burst);
        assert_eq!(profile.persistence, Persistence::Ephemeral);
    }
}
