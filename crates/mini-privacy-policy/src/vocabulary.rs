//! Cost-doctrine vocabulary (founder research, `docs/research/
//! MININET_RESEARCH_V2_20260713.md` §3): every privacy/availability/
//! integrity property is named, tied to a mechanism, and never claimed for
//! free. [`ProtectionProperty`] is what a caller wants; [`Mechanism`] is
//! what actually buys it; [`ResidualFloor`] is what stays true no matter how
//! much is spent.

use crate::error::{PrivacyPolicyError, Result};

/// A property a caller may want protected. Extensible: add a variant here
/// exactly when a real mechanism in this workspace can produce it — this
/// list is not a promise, it is the current honest inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProtectionProperty {
    /// Content is unreadable to anyone but the intended recipient(s).
    ContentSecrecy,
    /// A counterparty cannot learn the other side's network address.
    CounterpartyIpHiding,
    /// An observer cannot tell who is talking to whom.
    WhoTalksToWhomHiding,
    /// The path cannot be blocked by a single chokepoint operator.
    CensorshipResistance,
    /// A payment cannot be linked back to the payer's other activity.
    PaymentUnlinkability,
    /// Repeated requests/queries by the same principal cannot be linked
    /// to each other (e.g. private retrieval).
    RequestUnlinkability,
    /// Stored data survives the loss of some holders.
    StorageAvailability,
    /// Stored data cannot be silently corrupted without detection.
    StorageIntegrity,
    /// Only routing/storage necessities are visible; application metadata
    /// is not.
    MetadataMinimization,
    /// Traffic timing/size alone cannot correlate two endpoints.
    TimingCorrelationResistance,
    /// A signal that *some* live human is behind an action right now.
    HumanLivenessSignal,
    /// A signal (never a proof — see [`ResidualFloor::GlobalUniquenessOfPersons`])
    /// that a pseudonym is not obviously duplicated.
    HumanUniquenessSignal,
    /// Content stays reachable under large-scale coordinated takedown
    /// pressure (Tier 3's distinguishing property).
    SuppressionResistance,
}

/// A mechanism that buys some [`ProtectionProperty`] at a real resource
/// cost. Extensible for the same reason [`ProtectionProperty`] is: this is
/// an inventory of what exists or is concretely planned, not a wishlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Mechanism {
    /// Symmetric AEAD over the payload (`mini-crypto`).
    AeadEncryption,
    /// Single or chained onion-style relay hop(s).
    OnionRelay,
    /// Sphinx-style mix network with cover traffic and batching.
    MixNetwork,
    /// Reed-Solomon erasure coding replicated across failure domains
    /// (`mini-erasure`).
    ErasureCodedReplication,
    /// A blinded, pre-purchased resource credential redeemed without
    /// revealing the purchase.
    BlindedPrepaidToken,
    /// Linkable ring signature (`mini-value`).
    RingSignature,
    /// Stealth one-time address (`mini-value`).
    StealthAddress,
    /// Bulletproofs confidential-amount range proof (`mini-value`).
    BulletproofRangeProof,
    /// A private, device-local log of repeated interaction over time.
    ContinuityJournal,
    /// Attestations from a diverse set of independent social contacts.
    SocialAttestationDiversity,
    /// Continuity of the same physical device/hardware root.
    DeviceHardwareAttestation,
    /// A history of real, verifiable contribution (code, storage, review).
    EconomicContributionHistory,
    /// Coarse, privacy-preserving on-device co-presence detection
    /// (`mini-presence`).
    CoarseOnDeviceCoPresence,
    /// An externally issued uniqueness credential this network did not
    /// itself mint (still duplicable/coercible — see
    /// [`ResidualFloor::GlobalUniquenessOfPersons`]).
    ExternalUniquenessCredential,
    /// Padding messages to a fixed size class.
    TrafficPadding,
    /// Independent cover traffic unrelated to any real message.
    CoverTraffic,
    /// A randomized, bounded delay before forwarding.
    BoundedRandomDelay,
}

/// The five residual floors the founder research names as un-removable by
/// any budget (research doc §3, "residual floor"). Deliberately fixed at
/// five, not extensible: adding a sixth "floor" here without founder
/// research backing it would be exactly the kind of unearned confidence
/// this vocabulary exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResidualFloor {
    /// F1 — a compromised endpoint defeats any protection bought in
    /// transit or storage.
    EndpointCompromise,
    /// F2 — a global observer correlating a long, distinctive session
    /// against timing/volume alone.
    GlobalObserverLongSessionCorrelation,
    /// F3 — intersection attacks across repeated observation over time.
    IntersectionOverTime,
    /// F4 — global uniqueness of persons cannot be proven from behavior
    /// alone; it needs an external uniqueness root, which can always be
    /// duplicated or coerced. Never claim "one human, one identity" past
    /// this floor (see `docs/INVARIANTS.md`'s Sybil-unsolved limitation).
    GlobalUniquenessOfPersons,
    /// F5 — the user or their coercion: content/style/EXIF/payment-reuse
    /// leaks and legal compulsion are outside any protocol's reach.
    UserOrCoercion,
}

/// All five residual floors, in the fixed order the research names them.
/// Every [`crate::AchievedPrivacy`] carries this exact list regardless of
/// tier — no mechanism removes any of them.
pub const RESIDUAL_FLOORS: [ResidualFloor; 5] = [
    ResidualFloor::EndpointCompromise,
    ResidualFloor::GlobalObserverLongSessionCorrelation,
    ResidualFloor::IntersectionOverTime,
    ResidualFloor::GlobalUniquenessOfPersons,
    ResidualFloor::UserOrCoercion,
];

impl ProtectionProperty {
    pub(crate) fn to_byte(self) -> u8 {
        match self {
            ProtectionProperty::ContentSecrecy => 1,
            ProtectionProperty::CounterpartyIpHiding => 2,
            ProtectionProperty::WhoTalksToWhomHiding => 3,
            ProtectionProperty::CensorshipResistance => 4,
            ProtectionProperty::PaymentUnlinkability => 5,
            ProtectionProperty::RequestUnlinkability => 6,
            ProtectionProperty::StorageAvailability => 7,
            ProtectionProperty::StorageIntegrity => 8,
            ProtectionProperty::MetadataMinimization => 9,
            ProtectionProperty::TimingCorrelationResistance => 10,
            ProtectionProperty::HumanLivenessSignal => 11,
            ProtectionProperty::HumanUniquenessSignal => 12,
            ProtectionProperty::SuppressionResistance => 13,
        }
    }

    pub(crate) fn from_byte(b: u8) -> Result<Self> {
        Ok(match b {
            1 => ProtectionProperty::ContentSecrecy,
            2 => ProtectionProperty::CounterpartyIpHiding,
            3 => ProtectionProperty::WhoTalksToWhomHiding,
            4 => ProtectionProperty::CensorshipResistance,
            5 => ProtectionProperty::PaymentUnlinkability,
            6 => ProtectionProperty::RequestUnlinkability,
            7 => ProtectionProperty::StorageAvailability,
            8 => ProtectionProperty::StorageIntegrity,
            9 => ProtectionProperty::MetadataMinimization,
            10 => ProtectionProperty::TimingCorrelationResistance,
            11 => ProtectionProperty::HumanLivenessSignal,
            12 => ProtectionProperty::HumanUniquenessSignal,
            13 => ProtectionProperty::SuppressionResistance,
            _ => return Err(PrivacyPolicyError::Malformed),
        })
    }
}

impl Mechanism {
    pub(crate) fn to_byte(self) -> u8 {
        match self {
            Mechanism::AeadEncryption => 1,
            Mechanism::OnionRelay => 2,
            Mechanism::MixNetwork => 3,
            Mechanism::ErasureCodedReplication => 4,
            Mechanism::BlindedPrepaidToken => 5,
            Mechanism::RingSignature => 6,
            Mechanism::StealthAddress => 7,
            Mechanism::BulletproofRangeProof => 8,
            Mechanism::ContinuityJournal => 9,
            Mechanism::SocialAttestationDiversity => 10,
            Mechanism::DeviceHardwareAttestation => 11,
            Mechanism::EconomicContributionHistory => 12,
            Mechanism::CoarseOnDeviceCoPresence => 13,
            Mechanism::ExternalUniquenessCredential => 14,
            Mechanism::TrafficPadding => 15,
            Mechanism::CoverTraffic => 16,
            Mechanism::BoundedRandomDelay => 17,
        }
    }

    pub(crate) fn from_byte(b: u8) -> Result<Self> {
        Ok(match b {
            1 => Mechanism::AeadEncryption,
            2 => Mechanism::OnionRelay,
            3 => Mechanism::MixNetwork,
            4 => Mechanism::ErasureCodedReplication,
            5 => Mechanism::BlindedPrepaidToken,
            6 => Mechanism::RingSignature,
            7 => Mechanism::StealthAddress,
            8 => Mechanism::BulletproofRangeProof,
            9 => Mechanism::ContinuityJournal,
            10 => Mechanism::SocialAttestationDiversity,
            11 => Mechanism::DeviceHardwareAttestation,
            12 => Mechanism::EconomicContributionHistory,
            13 => Mechanism::CoarseOnDeviceCoPresence,
            14 => Mechanism::ExternalUniquenessCredential,
            15 => Mechanism::TrafficPadding,
            16 => Mechanism::CoverTraffic,
            17 => Mechanism::BoundedRandomDelay,
            _ => return Err(PrivacyPolicyError::Malformed),
        })
    }
}

impl ResidualFloor {
    pub(crate) fn to_byte(self) -> u8 {
        match self {
            ResidualFloor::EndpointCompromise => 1,
            ResidualFloor::GlobalObserverLongSessionCorrelation => 2,
            ResidualFloor::IntersectionOverTime => 3,
            ResidualFloor::GlobalUniquenessOfPersons => 4,
            ResidualFloor::UserOrCoercion => 5,
        }
    }

    pub(crate) fn from_byte(b: u8) -> Result<Self> {
        Ok(match b {
            1 => ResidualFloor::EndpointCompromise,
            2 => ResidualFloor::GlobalObserverLongSessionCorrelation,
            3 => ResidualFloor::IntersectionOverTime,
            4 => ResidualFloor::GlobalUniquenessOfPersons,
            5 => ResidualFloor::UserOrCoercion,
            _ => return Err(PrivacyPolicyError::Malformed),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_protection_property_round_trips_through_its_byte() {
        let all = [
            ProtectionProperty::ContentSecrecy,
            ProtectionProperty::CounterpartyIpHiding,
            ProtectionProperty::WhoTalksToWhomHiding,
            ProtectionProperty::CensorshipResistance,
            ProtectionProperty::PaymentUnlinkability,
            ProtectionProperty::RequestUnlinkability,
            ProtectionProperty::StorageAvailability,
            ProtectionProperty::StorageIntegrity,
            ProtectionProperty::MetadataMinimization,
            ProtectionProperty::TimingCorrelationResistance,
            ProtectionProperty::HumanLivenessSignal,
            ProtectionProperty::HumanUniquenessSignal,
            ProtectionProperty::SuppressionResistance,
        ];
        for p in all {
            assert_eq!(ProtectionProperty::from_byte(p.to_byte()).unwrap(), p);
        }
    }

    #[test]
    fn every_mechanism_round_trips_through_its_byte() {
        let all = [
            Mechanism::AeadEncryption,
            Mechanism::OnionRelay,
            Mechanism::MixNetwork,
            Mechanism::ErasureCodedReplication,
            Mechanism::BlindedPrepaidToken,
            Mechanism::RingSignature,
            Mechanism::StealthAddress,
            Mechanism::BulletproofRangeProof,
            Mechanism::ContinuityJournal,
            Mechanism::SocialAttestationDiversity,
            Mechanism::DeviceHardwareAttestation,
            Mechanism::EconomicContributionHistory,
            Mechanism::CoarseOnDeviceCoPresence,
            Mechanism::ExternalUniquenessCredential,
            Mechanism::TrafficPadding,
            Mechanism::CoverTraffic,
            Mechanism::BoundedRandomDelay,
        ];
        for m in all {
            assert_eq!(Mechanism::from_byte(m.to_byte()).unwrap(), m);
        }
    }

    #[test]
    fn every_residual_floor_round_trips_through_its_byte() {
        for f in RESIDUAL_FLOORS {
            assert_eq!(ResidualFloor::from_byte(f.to_byte()).unwrap(), f);
        }
    }

    #[test]
    fn an_unknown_property_byte_is_rejected() {
        assert_eq!(
            ProtectionProperty::from_byte(0xee),
            Err(PrivacyPolicyError::Malformed)
        );
    }

    #[test]
    fn an_unknown_mechanism_byte_is_rejected() {
        assert_eq!(
            Mechanism::from_byte(0xee),
            Err(PrivacyPolicyError::Malformed)
        );
    }

    #[test]
    fn an_unknown_floor_byte_is_rejected() {
        assert_eq!(
            ResidualFloor::from_byte(0xee),
            Err(PrivacyPolicyError::Malformed)
        );
    }

    #[test]
    fn residual_floors_is_exactly_the_five_named_floors() {
        assert_eq!(RESIDUAL_FLOORS.len(), 5);
    }
}
