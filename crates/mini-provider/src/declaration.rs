//! The declaration: a provider's own signed claim about what it offers.
//!
//! The protocol never validates its content; humans and reviewers do
//! (FD-18 Part I, T2). This crate checks only structural well-formedness
//! (§[`ProviderDeclaration::check_wellformed`]) -- never truthfulness.

use did_mini::Did;

use crate::error::{ProviderError, Result};

/// Hard limit on [`ProviderDeclaration::description`], the same
/// defensive-decoding discipline every untrusted-input type in this
/// workspace applies (ID5).
pub const MAX_DESCRIPTION_BYTES: usize = 4096;
/// Hard limit on [`ProviderDeclaration::jurisdictions`].
pub const MAX_JURISDICTION_CLAIMS: usize = 32;
/// Hard limit on [`ProviderDeclaration::data_required`] and
/// [`ExitTerms::retained_data`].
pub const MAX_DATA_REQUIREMENTS: usize = 64;

/// A provider's own signed claim about what it offers. Never verified by
/// the protocol -- rendered to humans, judged by humans.
///
/// Binding this to a real signed, content-addressed
/// `mini_objects::Object` (so a declaration is tamper-evident and
/// discoverable the way every other object in this workspace is) is
/// deferred -- Wave 1 of FD-18's confirmed sequencing is pure data only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDeclaration {
    /// The declaring identity. MAY be an `OrgRoot` (a future, separate
    /// non-human-root type, FD-18 Part II.5) or a human `did:mini`.
    /// Carries no vote either way (INV-18-08).
    pub declarant: Did,

    pub service: ServiceClass,

    /// Free-form, human-language, no protocol meaning.
    pub description: String,

    /// Jurisdictions the provider asserts it operates under. A CLAIM, not
    /// a credential. Empty is legal and honest.
    pub jurisdictions: Vec<JurisdictionClaim>,

    /// Exactly what the provider needs from the human. Clients render this
    /// before any grant is signed. Anything not listed here that the
    /// provider later requests is a protocol-visible broken promise.
    pub data_required: Vec<DataRequirement>,

    // ---- The four mandatory-honesty fields (non-Option by design) ----
    pub custody: CustodyPosture,
    pub freeze_powers: FreezePowers,
    pub death_disposition: DeathDisposition,
    pub exit: ExitTerms,

    /// Non-optional, like `mini-bridge`'s `BridgeDescriptor::expires_at_ms`
    /// (D-0309). A stale declaration is not a live offer.
    pub expires_at_ms: u64,
}

impl ProviderDeclaration {
    /// Structural well-formedness ONLY. This never judges whether a
    /// provider is good, honest, licensed, or safe -- the protocol has no
    /// opinion and must never acquire one (T2, non-negotiable #5).
    pub fn check_wellformed(&self, now_ms: u64) -> Result<()> {
        if self.expires_at_ms <= now_ms {
            return Err(ProviderError::DeclarationExpired(self.expires_at_ms));
        }
        if let CustodyPosture::JustInTime { max_hold_ms } = &self.custody {
            if *max_hold_ms == 0 {
                return Err(ProviderError::UnboundedHold);
            }
        }
        if self.description.len() > MAX_DESCRIPTION_BYTES {
            return Err(ProviderError::DescriptionTooLong);
        }
        if self.jurisdictions.len() > MAX_JURISDICTION_CLAIMS {
            return Err(ProviderError::TooManyJurisdictionClaims);
        }
        if self.data_required.len() > MAX_DATA_REQUIREMENTS
            || self.exit.retained_data.len() > MAX_DATA_REQUIREMENTS
        {
            return Err(ProviderError::TooManyDataRequirements);
        }
        Ok(())
    }
}

/// What kind of service a provider offers. Deliberately open
/// (`#[non_exhaustive]` and an [`ServiceClass::Other`] catch-all): the
/// protocol has no fixed catalog of legitimate edge services, and adding
/// one would itself be a step toward a canonical registry (INV-18-04).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServiceClass {
    /// MINI <-> other-asset conversion. Card Architecture B.
    Conversion,
    /// Card issuance or equivalent legacy-rail spend. Card Architecture A.
    LegacySpendRail,
    Custody,
    PhysicalDelivery,
    Connectivity,
    ProfessionalService,
    HardwareSupply,
    /// Anything not yet imagined. Clients render the string and warn that
    /// it has no known semantics.
    Other(String),
}

impl ServiceClass {
    /// A small `Copy` tag for use as a `LocalProviderPolicy` disable-class
    /// key -- every `Other(..)` string maps to the same tag, on purpose:
    /// disabling "the unknown-service-class bucket" is a coarser, honestly
    /// weaker control than disabling one named provider, and this type
    /// makes that limitation visible rather than pretending per-string
    /// granularity exists.
    pub fn tag(&self) -> ServiceClassTag {
        match self {
            ServiceClass::Conversion => ServiceClassTag::Conversion,
            ServiceClass::LegacySpendRail => ServiceClassTag::LegacySpendRail,
            ServiceClass::Custody => ServiceClassTag::Custody,
            ServiceClass::PhysicalDelivery => ServiceClassTag::PhysicalDelivery,
            ServiceClass::Connectivity => ServiceClassTag::Connectivity,
            ServiceClass::ProfessionalService => ServiceClassTag::ProfessionalService,
            ServiceClass::HardwareSupply => ServiceClassTag::HardwareSupply,
            ServiceClass::Other(_) => ServiceClassTag::Other,
        }
    }
}

/// [`ServiceClass`] without its payload, so it can key a `HashSet` in
/// [`crate::LocalProviderPolicy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceClassTag {
    Conversion,
    LegacySpendRail,
    Custody,
    PhysicalDelivery,
    Connectivity,
    ProfessionalService,
    HardwareSupply,
    Other,
}

/// What the provider can do to a human's funds. There is no "unspecified"
/// variant, on purpose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustodyPosture {
    /// Provider never holds user value. Strongly preferred.
    NoneHeld,
    /// Value held only for the duration of a single engagement.
    JustInTime { max_hold_ms: u64 },
    /// Provider parks balances. Clients MUST render this as a warning.
    ParkedBalance { insured: bool },
}

/// Whether, and under what claimed authority, the provider can halt a
/// user's funds or service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreezePowers {
    pub can_freeze_user: bool,
    /// Under what asserted authority. Honest answers include "court
    /// order", "internal risk policy", "any time, no reason".
    pub grounds: Vec<String>,
    /// Does the user get told? Providers who answer `false` are entitled
    /// to say so; clients are entitled to shout about it.
    pub notifies_user: bool,
}

/// FD-02: assume every provider eventually disappears. A provider that has
/// not thought about its own death has not thought about the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeathDisposition {
    /// Nothing is held, so nothing is lost.
    NothingHeld,
    /// Funds recoverable by the user alone, without the provider.
    UserRecoverableUnilaterally { method: String },
    /// Recoverable only with a third party's cooperation.
    RequiresThirdParty { who: String },
    /// The honest bad answer. Permitted. Clients render it in red.
    LostToInsolvency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitTerms {
    pub notice_required_ms: Option<u64>,
    pub exit_fee_micromini: u64,
    /// Data the provider retains after the human leaves.
    pub retained_data: Vec<DataRequirement>,
}

/// One piece of data a provider needs from (or retains about) a human.
/// Deliberately a bare label, not a closed taxonomy: the protocol has no
/// opinion on what data means, only that every requirement a provider will
/// ever ask for must be listed here before a grant is signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataRequirement {
    pub label: String,
    /// True if this specific item survives the human's exit (see
    /// [`ExitTerms::retained_data`], which reuses this same type).
    pub retained_after_exit: bool,
}

/// A jurisdiction a provider asserts it operates under. A claim, not a
/// credential -- this crate never validates it against any authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JurisdictionClaim {
    pub jurisdiction: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn sample_did() -> Did {
        Controller::incept_single().unwrap().did()
    }

    fn wellformed_declaration() -> ProviderDeclaration {
        ProviderDeclaration {
            declarant: sample_did(),
            service: ServiceClass::Conversion,
            description: "MINI <-> BTC conversion".to_string(),
            jurisdictions: vec![],
            data_required: vec![],
            custody: CustodyPosture::JustInTime {
                max_hold_ms: 60_000,
            },
            freeze_powers: FreezePowers {
                can_freeze_user: false,
                grounds: vec![],
                notifies_user: true,
            },
            death_disposition: DeathDisposition::NothingHeld,
            exit: ExitTerms {
                notice_required_ms: None,
                exit_fee_micromini: 0,
                retained_data: vec![],
            },
            expires_at_ms: 10_000,
        }
    }

    #[test]
    fn a_wellformed_declaration_is_accepted() {
        assert!(wellformed_declaration().check_wellformed(5_000).is_ok());
    }

    #[test]
    fn an_expired_declaration_is_rejected() {
        let d = wellformed_declaration();
        assert_eq!(
            d.check_wellformed(10_000),
            Err(ProviderError::DeclarationExpired(10_000))
        );
        assert_eq!(
            d.check_wellformed(20_000),
            Err(ProviderError::DeclarationExpired(10_000))
        );
    }

    #[test]
    fn a_just_in_time_custody_with_zero_hold_is_rejected() {
        let mut d = wellformed_declaration();
        d.custody = CustodyPosture::JustInTime { max_hold_ms: 0 };
        assert_eq!(d.check_wellformed(5_000), Err(ProviderError::UnboundedHold));
    }

    #[test]
    fn none_held_and_parked_balance_custody_have_no_hold_bound_check() {
        let mut d = wellformed_declaration();
        d.custody = CustodyPosture::NoneHeld;
        assert!(d.check_wellformed(5_000).is_ok());
        d.custody = CustodyPosture::ParkedBalance { insured: false };
        assert!(d.check_wellformed(5_000).is_ok());
    }

    #[test]
    fn an_oversized_description_is_rejected() {
        let mut d = wellformed_declaration();
        d.description = "x".repeat(MAX_DESCRIPTION_BYTES + 1);
        assert_eq!(
            d.check_wellformed(5_000),
            Err(ProviderError::DescriptionTooLong)
        );
    }

    #[test]
    fn too_many_jurisdiction_claims_is_rejected() {
        let mut d = wellformed_declaration();
        d.jurisdictions = (0..MAX_JURISDICTION_CLAIMS + 1)
            .map(|i| JurisdictionClaim {
                jurisdiction: format!("jurisdiction-{i}"),
            })
            .collect();
        assert_eq!(
            d.check_wellformed(5_000),
            Err(ProviderError::TooManyJurisdictionClaims)
        );
    }

    #[test]
    fn too_many_data_requirements_is_rejected_on_either_field() {
        let too_many: Vec<DataRequirement> = (0..MAX_DATA_REQUIREMENTS + 1)
            .map(|i| DataRequirement {
                label: format!("field-{i}"),
                retained_after_exit: false,
            })
            .collect();

        let mut d = wellformed_declaration();
        d.data_required = too_many.clone();
        assert_eq!(
            d.check_wellformed(5_000),
            Err(ProviderError::TooManyDataRequirements)
        );

        let mut d2 = wellformed_declaration();
        d2.exit.retained_data = too_many;
        assert_eq!(
            d2.check_wellformed(5_000),
            Err(ProviderError::TooManyDataRequirements)
        );
    }

    #[test]
    fn every_other_service_class_string_collapses_to_the_same_tag() {
        assert_eq!(
            ServiceClass::Other("anything".to_string()).tag(),
            ServiceClassTag::Other
        );
        assert_eq!(
            ServiceClass::Other("something else entirely".to_string()).tag(),
            ServiceClassTag::Other
        );
    }

    #[test]
    fn named_service_classes_map_to_distinct_tags() {
        let tags = [
            ServiceClass::Conversion.tag(),
            ServiceClass::LegacySpendRail.tag(),
            ServiceClass::Custody.tag(),
            ServiceClass::PhysicalDelivery.tag(),
            ServiceClass::Connectivity.tag(),
            ServiceClass::ProfessionalService.tag(),
            ServiceClass::HardwareSupply.tag(),
        ];
        for (i, a) in tags.iter().enumerate() {
            for (j, b) in tags.iter().enumerate() {
                assert_eq!(i == j, a == b);
            }
        }
    }
}
