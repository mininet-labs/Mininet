//! The tier 0-3 privacy request/achieved-result policy object (founder
//! research §2/§3): a caller states what it wants as a [`PrivacyRequest`],
//! a transport reports what it actually bought as an [`AchievedPrivacy`] —
//! both are plain data, serializable and auditable, never a claim of
//! absolute anonymity.
//!
//! **No transport reads or acts on this crate yet.** `mini-bearer`/
//! `mini-net` today move bytes point-to-point with no relay, mix, or
//! erasure-replication mechanism behind them. Every [`ResourceCost`] here
//! is the founder research's own cost-doctrine estimate for a tier
//! (research doc §2), not a benchmark of running code — the honest
//! "design-only" line in `docs/STATUS.md` for this crate says so
//! explicitly. Building the mechanisms these numbers assume is later
//! phase work (`docs/design/`), not this crate's job.

use crate::error::{PrivacyPolicyError, Result};
use crate::vocabulary::{Mechanism, ProtectionProperty, ResidualFloor, RESIDUAL_FLOORS};

/// Hard cap on a [`PrivacyRequest`]'s property list.
pub const MAX_PROPERTIES: usize = 32;
/// Hard cap on an [`AchievedPrivacy`]'s mechanism list.
pub const MAX_MECHANISMS: usize = 16;
/// Hard cap on an [`AchievedPrivacy`]'s residual-floor list (headroom above
/// the fixed five in [`RESIDUAL_FLOORS`], never expected to be reached).
pub const MAX_FLOORS: usize = 8;

const DOMAIN: &[u8] = b"mini-privacy-policy/tier/v1";
const TAG_REQUEST: u8 = 0x01;
const TAG_ACHIEVED: u8 = 0x02;

/// The four privacy tiers the founder research names (research doc §2).
/// Higher tiers cost strictly more resources and never claim to remove any
/// [`ResidualFloor`] — see [`RESIDUAL_FLOORS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrivacyTier {
    /// Tier 0 — Direct/Economy. No relay; a counterparty sees the real
    /// endpoint address. Roughly 1x resource cost.
    Direct = 0,
    /// Tier 1 — Relayed/Private. One or more relay hops hide the
    /// counterparty's address. Roughly 2-4x bandwidth, 100-500ms added
    /// latency.
    Relayed = 1,
    /// Tier 2 — Mixed/High-risk. Mix-network batching and cover traffic.
    /// Roughly 5-50x bandwidth, seconds-to-minutes added latency.
    Mixed = 2,
    /// Tier 3 — Burst/Suppression-resistant. Tier 2 plus multi-region
    /// erasure-coded replication so content survives coordinated takedown.
    Burst = 3,
}

impl PrivacyTier {
    pub(crate) fn to_byte(self) -> u8 {
        self as u8
    }

    pub(crate) fn from_byte(b: u8) -> Result<Self> {
        Ok(match b {
            0 => PrivacyTier::Direct,
            1 => PrivacyTier::Relayed,
            2 => PrivacyTier::Mixed,
            3 => PrivacyTier::Burst,
            _ => return Err(PrivacyPolicyError::Malformed),
        })
    }
}

/// A resource-cost range, expressed as fixed-point multipliers in
/// thousandths (`1000` = 1.000x) so no float ever enters a wire message or
/// a determinism-sensitive comparison. Every field is a `min..=max` range
/// because the founder research states these as ranges, never single
/// numbers — collapsing a range to one figure would itself be an
/// overclaim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceCost {
    /// Bandwidth multiplier vs. Tier 0, in thousandths.
    pub bandwidth_multiplier_millix_min: u32,
    pub bandwidth_multiplier_millix_max: u32,
    /// Added one-way latency, in milliseconds.
    pub added_latency_ms_min: u32,
    pub added_latency_ms_max: u32,
    /// Storage multiplier vs. an unreplicated object, in thousandths.
    pub storage_multiplier_millix_min: u32,
    pub storage_multiplier_millix_max: u32,
    /// Whether this tier assumes a paid resource market funds the extra
    /// bandwidth/storage (research doc §8) rather than volunteers alone.
    pub requires_payment: bool,
}

/// The founder research's own cost-doctrine estimate for `tier` (research
/// doc §2). Not measured from running code — see this module's own
/// honesty note.
pub fn expected_cost(tier: PrivacyTier) -> ResourceCost {
    match tier {
        PrivacyTier::Direct => ResourceCost {
            bandwidth_multiplier_millix_min: 1000,
            bandwidth_multiplier_millix_max: 1000,
            added_latency_ms_min: 0,
            added_latency_ms_max: 0,
            storage_multiplier_millix_min: 1000,
            storage_multiplier_millix_max: 1000,
            requires_payment: false,
        },
        PrivacyTier::Relayed => ResourceCost {
            bandwidth_multiplier_millix_min: 2000,
            bandwidth_multiplier_millix_max: 4000,
            added_latency_ms_min: 100,
            added_latency_ms_max: 500,
            storage_multiplier_millix_min: 1000,
            storage_multiplier_millix_max: 1000,
            requires_payment: true,
        },
        PrivacyTier::Mixed => ResourceCost {
            bandwidth_multiplier_millix_min: 5000,
            bandwidth_multiplier_millix_max: 50_000,
            added_latency_ms_min: 1_000,
            added_latency_ms_max: 60_000,
            storage_multiplier_millix_min: 1000,
            storage_multiplier_millix_max: 1000,
            requires_payment: true,
        },
        PrivacyTier::Burst => ResourceCost {
            bandwidth_multiplier_millix_min: 5000,
            bandwidth_multiplier_millix_max: 50_000,
            added_latency_ms_min: 1_000,
            added_latency_ms_max: 60_000,
            // Mixed's cost plus multi-region erasure replication overhead
            // (research doc §6.2's "resilient"/"high-risk" bands).
            storage_multiplier_millix_min: 2000,
            storage_multiplier_millix_max: 3000,
            requires_payment: true,
        },
    }
}

/// What a caller wants: at least `tier`, specifically covering
/// `properties`. Plain data — a router (later phase work) decides how to
/// satisfy it; nothing here dials a socket or picks a mechanism.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyRequest {
    pub tier: PrivacyTier,
    pub properties: Vec<ProtectionProperty>,
}

/// What was actually bought: the tier reached, the mechanisms used, and
/// the residual floors that still apply — always all five, on every
/// tier (see [`RESIDUAL_FLOORS`]). This is the auditable record a caller
/// can log or display; it is never itself a proof of anything claimed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AchievedPrivacy {
    pub tier: PrivacyTier,
    pub mechanisms: Vec<Mechanism>,
    pub cost: ResourceCost,
    pub residual_floors: Vec<ResidualFloor>,
}

impl AchievedPrivacy {
    /// Build a result carrying the fixed five residual floors, so a caller
    /// cannot construct an `AchievedPrivacy` that silently omits one.
    pub fn new(tier: PrivacyTier, mechanisms: Vec<Mechanism>, cost: ResourceCost) -> Self {
        AchievedPrivacy {
            tier,
            mechanisms,
            cost,
            residual_floors: RESIDUAL_FLOORS.to_vec(),
        }
    }
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(rest: &'a [u8]) -> Self {
        Reader { rest }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.rest.len() < n {
            return Err(PrivacyPolicyError::Malformed);
        }
        let (head, tail) = self.rest.split_at(n);
        self.rest = tail;
        Ok(head)
    }

    fn take_u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn take_u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn finish(self) -> Result<()> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(PrivacyPolicyError::Malformed)
        }
    }
}

fn encode_cost(out: &mut Vec<u8>, cost: &ResourceCost) {
    out.extend_from_slice(&cost.bandwidth_multiplier_millix_min.to_be_bytes());
    out.extend_from_slice(&cost.bandwidth_multiplier_millix_max.to_be_bytes());
    out.extend_from_slice(&cost.added_latency_ms_min.to_be_bytes());
    out.extend_from_slice(&cost.added_latency_ms_max.to_be_bytes());
    out.extend_from_slice(&cost.storage_multiplier_millix_min.to_be_bytes());
    out.extend_from_slice(&cost.storage_multiplier_millix_max.to_be_bytes());
    out.push(u8::from(cost.requires_payment));
}

fn decode_cost(r: &mut Reader) -> Result<ResourceCost> {
    Ok(ResourceCost {
        bandwidth_multiplier_millix_min: r.take_u32()?,
        bandwidth_multiplier_millix_max: r.take_u32()?,
        added_latency_ms_min: r.take_u32()?,
        added_latency_ms_max: r.take_u32()?,
        storage_multiplier_millix_min: r.take_u32()?,
        storage_multiplier_millix_max: r.take_u32()?,
        requires_payment: match r.take_u8()? {
            0 => false,
            1 => true,
            _ => return Err(PrivacyPolicyError::Malformed),
        },
    })
}

impl PrivacyRequest {
    /// Serialize to bytes, so a request can be logged, sent, or replayed
    /// in a test.
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put_bytes(&mut out, DOMAIN);
        out.push(TAG_REQUEST);
        out.push(self.tier.to_byte());
        out.extend_from_slice(&(self.properties.len() as u32).to_be_bytes());
        for p in &self.properties {
            out.push(p.to_byte());
        }
        out
    }

    /// Parse bytes back into a request. Rejects truncation, an unknown
    /// domain/tag/tier/property byte, an over-cap property count, and any
    /// trailing bytes.
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let domain_len = r.take_u32()? as usize;
        let domain = r.take(domain_len)?;
        if domain != DOMAIN {
            return Err(PrivacyPolicyError::Malformed);
        }
        if r.take_u8()? != TAG_REQUEST {
            return Err(PrivacyPolicyError::Malformed);
        }
        let tier = PrivacyTier::from_byte(r.take_u8()?)?;
        let count = r.take_u32()? as usize;
        if count > MAX_PROPERTIES {
            return Err(PrivacyPolicyError::TooManyProperties);
        }
        let mut properties = Vec::with_capacity(count);
        for _ in 0..count {
            properties.push(ProtectionProperty::from_byte(r.take_u8()?)?);
        }
        r.finish()?;
        Ok(PrivacyRequest { tier, properties })
    }
}

impl AchievedPrivacy {
    /// Serialize to bytes for logging/auditing.
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put_bytes(&mut out, DOMAIN);
        out.push(TAG_ACHIEVED);
        out.push(self.tier.to_byte());
        out.extend_from_slice(&(self.mechanisms.len() as u32).to_be_bytes());
        for m in &self.mechanisms {
            out.push(m.to_byte());
        }
        encode_cost(&mut out, &self.cost);
        out.extend_from_slice(&(self.residual_floors.len() as u32).to_be_bytes());
        for f in &self.residual_floors {
            out.push(f.to_byte());
        }
        out
    }

    /// Parse bytes back into an achieved-privacy record. Rejects
    /// truncation, an unknown domain/tag/tier/mechanism/floor byte,
    /// over-cap counts, and any trailing bytes.
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let domain_len = r.take_u32()? as usize;
        let domain = r.take(domain_len)?;
        if domain != DOMAIN {
            return Err(PrivacyPolicyError::Malformed);
        }
        if r.take_u8()? != TAG_ACHIEVED {
            return Err(PrivacyPolicyError::Malformed);
        }
        let tier = PrivacyTier::from_byte(r.take_u8()?)?;
        let mech_count = r.take_u32()? as usize;
        if mech_count > MAX_MECHANISMS {
            return Err(PrivacyPolicyError::TooManyMechanisms);
        }
        let mut mechanisms = Vec::with_capacity(mech_count);
        for _ in 0..mech_count {
            mechanisms.push(Mechanism::from_byte(r.take_u8()?)?);
        }
        let cost = decode_cost(&mut r)?;
        let floor_count = r.take_u32()? as usize;
        if floor_count > MAX_FLOORS {
            return Err(PrivacyPolicyError::TooManyFloors);
        }
        let mut residual_floors = Vec::with_capacity(floor_count);
        for _ in 0..floor_count {
            residual_floors.push(ResidualFloor::from_byte(r.take_u8()?)?);
        }
        r.finish()?;
        Ok(AchievedPrivacy {
            tier,
            mechanisms,
            cost,
            residual_floors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> PrivacyRequest {
        PrivacyRequest {
            tier: PrivacyTier::Relayed,
            properties: vec![
                ProtectionProperty::CounterpartyIpHiding,
                ProtectionProperty::ContentSecrecy,
            ],
        }
    }

    fn sample_achieved() -> AchievedPrivacy {
        AchievedPrivacy::new(
            PrivacyTier::Mixed,
            vec![Mechanism::MixNetwork, Mechanism::CoverTraffic],
            expected_cost(PrivacyTier::Mixed),
        )
    }

    #[test]
    fn a_request_round_trips_through_wire_bytes() {
        let req = sample_request();
        assert_eq!(
            PrivacyRequest::from_wire_bytes(&req.to_wire_bytes()).unwrap(),
            req
        );
    }

    #[test]
    fn an_empty_property_list_round_trips() {
        let req = PrivacyRequest {
            tier: PrivacyTier::Direct,
            properties: Vec::new(),
        };
        assert_eq!(
            PrivacyRequest::from_wire_bytes(&req.to_wire_bytes()).unwrap(),
            req
        );
    }

    #[test]
    fn an_achieved_record_round_trips_through_wire_bytes() {
        let ach = sample_achieved();
        assert_eq!(
            AchievedPrivacy::from_wire_bytes(&ach.to_wire_bytes()).unwrap(),
            ach
        );
    }

    #[test]
    fn achieved_new_always_carries_all_five_residual_floors() {
        let ach = sample_achieved();
        assert_eq!(ach.residual_floors, RESIDUAL_FLOORS.to_vec());
    }

    #[test]
    fn a_truncated_request_is_rejected_at_every_length() {
        let full = sample_request().to_wire_bytes();
        for cut in 0..full.len() {
            assert!(
                PrivacyRequest::from_wire_bytes(&full[..cut]).is_err(),
                "truncating a request to {cut} bytes must be rejected"
            );
        }
    }

    #[test]
    fn a_truncated_achieved_record_is_rejected_at_every_length() {
        let full = sample_achieved().to_wire_bytes();
        for cut in 0..full.len() {
            assert!(
                AchievedPrivacy::from_wire_bytes(&full[..cut]).is_err(),
                "truncating an achieved record to {cut} bytes must be rejected"
            );
        }
    }

    #[test]
    fn trailing_bytes_after_a_well_formed_request_are_rejected() {
        let mut bytes = sample_request().to_wire_bytes();
        bytes.push(0xff);
        assert!(PrivacyRequest::from_wire_bytes(&bytes).is_err());
    }

    #[test]
    fn trailing_bytes_after_a_well_formed_achieved_record_are_rejected() {
        let mut bytes = sample_achieved().to_wire_bytes();
        bytes.push(0xff);
        assert!(AchievedPrivacy::from_wire_bytes(&bytes).is_err());
    }

    #[test]
    fn a_wrong_domain_tag_is_rejected() {
        let mut out = Vec::new();
        put_bytes(&mut out, b"not-the-right-domain");
        out.push(TAG_REQUEST);
        out.push(PrivacyTier::Direct.to_byte());
        out.extend_from_slice(&0u32.to_be_bytes());
        assert!(PrivacyRequest::from_wire_bytes(&out).is_err());
    }

    #[test]
    fn a_property_count_over_the_cap_is_rejected_before_allocating() {
        let mut out = Vec::new();
        put_bytes(&mut out, DOMAIN);
        out.push(TAG_REQUEST);
        out.push(PrivacyTier::Direct.to_byte());
        out.extend_from_slice(&((MAX_PROPERTIES as u32) + 1).to_be_bytes());
        assert_eq!(
            PrivacyRequest::from_wire_bytes(&out),
            Err(PrivacyPolicyError::TooManyProperties)
        );
    }

    #[test]
    fn a_mechanism_count_over_the_cap_is_rejected_before_allocating() {
        let mut out = Vec::new();
        put_bytes(&mut out, DOMAIN);
        out.push(TAG_ACHIEVED);
        out.push(PrivacyTier::Direct.to_byte());
        out.extend_from_slice(&((MAX_MECHANISMS as u32) + 1).to_be_bytes());
        assert_eq!(
            AchievedPrivacy::from_wire_bytes(&out),
            Err(PrivacyPolicyError::TooManyMechanisms)
        );
    }

    #[test]
    fn a_floor_count_over_the_cap_is_rejected_before_allocating() {
        let mut out = Vec::new();
        put_bytes(&mut out, DOMAIN);
        out.push(TAG_ACHIEVED);
        out.push(PrivacyTier::Direct.to_byte());
        out.extend_from_slice(&0u32.to_be_bytes()); // zero mechanisms
        encode_cost(&mut out, &expected_cost(PrivacyTier::Direct));
        out.extend_from_slice(&((MAX_FLOORS as u32) + 1).to_be_bytes());
        assert_eq!(
            AchievedPrivacy::from_wire_bytes(&out),
            Err(PrivacyPolicyError::TooManyFloors)
        );
    }

    #[test]
    fn an_unknown_tag_is_rejected() {
        let mut out = Vec::new();
        put_bytes(&mut out, DOMAIN);
        out.push(0xee);
        assert!(PrivacyRequest::from_wire_bytes(&out).is_err());
    }

    #[test]
    fn expected_cost_is_monotonically_non_decreasing_by_tier() {
        let tiers = [
            PrivacyTier::Direct,
            PrivacyTier::Relayed,
            PrivacyTier::Mixed,
            PrivacyTier::Burst,
        ];
        for pair in tiers.windows(2) {
            let lower = expected_cost(pair[0]);
            let higher = expected_cost(pair[1]);
            assert!(
                higher.bandwidth_multiplier_millix_max >= lower.bandwidth_multiplier_millix_max,
                "{:?} must not cost less bandwidth than {:?}",
                pair[1],
                pair[0]
            );
        }
    }

    #[test]
    fn direct_tier_requires_no_payment_every_other_tier_does() {
        assert!(!expected_cost(PrivacyTier::Direct).requires_payment);
        assert!(expected_cost(PrivacyTier::Relayed).requires_payment);
        assert!(expected_cost(PrivacyTier::Mixed).requires_payment);
        assert!(expected_cost(PrivacyTier::Burst).requires_payment);
    }
}
