//! The public-commons policy object (research doctrine §8): a fixed set of
//! protocol entitlements every identity root holds, independent of wallet
//! balance, governance weight, or any paid tier.

use crate::error::{CommonsPolicyError, Result};

const DOMAIN: &[u8] = b"mini-commons-policy/policy/v1";
const TAG_POLICY: u8 = 0x01;

/// Whether a public-commons action is a free protocol right today, or a
/// surface this workspace has not yet built. Never a price: there is no
/// third "paid" variant, because paying for a commons action would
/// contradict the doctrine this crate encodes (research doc §7: "Do not
/// represent these as a zero-price commercial purchase").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Entitlement {
    /// Granted to every identity root, unconditionally, at protocol level.
    FreeProtocolRight,
    /// Not yet built in this workspace. Distinct from a denial: this is an
    /// honest "not implemented" marker, never a paywall.
    Unsupported,
}

impl Entitlement {
    fn to_byte(self) -> u8 {
        match self {
            Entitlement::FreeProtocolRight => 1,
            Entitlement::Unsupported => 2,
        }
    }

    fn from_byte(b: u8) -> Result<Self> {
        Ok(match b {
            1 => Entitlement::FreeProtocolRight,
            2 => Entitlement::Unsupported,
            _ => return Err(CommonsPolicyError::Malformed),
        })
    }
}

/// The public-commons entitlement policy (research doc §8). Every field is
/// an [`Entitlement`], never a price or a balance threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicCommonsPolicy {
    pub view_public_profiles: Entitlement,
    pub view_public_objects: Entitlement,
    pub create_public_profile: Entitlement,
    pub publish_public_object: Entitlement,
    pub reply_publicly: Entitlement,
    pub comment_publicly: Entitlement,
    pub react_publicly: Entitlement,
    pub search_public_index: Entitlement,
}

impl PublicCommonsPolicy {
    /// The commons policy every identity root holds today. `search_public_index`
    /// is [`Entitlement::Unsupported`] because no search index exists yet in
    /// this workspace (Track E backlog); every other action is already
    /// modeled by an existing crate (`mini-social`, `mini-intake-types`) and
    /// is a [`Entitlement::FreeProtocolRight`].
    pub const fn free_commons() -> Self {
        PublicCommonsPolicy {
            view_public_profiles: Entitlement::FreeProtocolRight,
            view_public_objects: Entitlement::FreeProtocolRight,
            create_public_profile: Entitlement::FreeProtocolRight,
            publish_public_object: Entitlement::FreeProtocolRight,
            reply_publicly: Entitlement::FreeProtocolRight,
            comment_publicly: Entitlement::FreeProtocolRight,
            react_publicly: Entitlement::FreeProtocolRight,
            search_public_index: Entitlement::Unsupported,
        }
    }

    /// Serialize to bytes, so a policy can be logged, sent, or replayed in
    /// a test.
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(DOMAIN.len() as u32).to_be_bytes());
        out.extend_from_slice(DOMAIN);
        out.push(TAG_POLICY);
        out.push(self.view_public_profiles.to_byte());
        out.push(self.view_public_objects.to_byte());
        out.push(self.create_public_profile.to_byte());
        out.push(self.publish_public_object.to_byte());
        out.push(self.reply_publicly.to_byte());
        out.push(self.comment_publicly.to_byte());
        out.push(self.react_publicly.to_byte());
        out.push(self.search_public_index.to_byte());
        out
    }

    /// Parse bytes back into a policy. Rejects truncation, an unknown
    /// domain/tag/entitlement byte, and any trailing bytes.
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 4 {
            return Err(CommonsPolicyError::Malformed);
        }
        let domain_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let mut offset = 4;
        if bytes.len() < offset + domain_len {
            return Err(CommonsPolicyError::Malformed);
        }
        if &bytes[offset..offset + domain_len] != DOMAIN {
            return Err(CommonsPolicyError::Malformed);
        }
        offset += domain_len;

        let expected_len = offset + 1 + 8;
        if bytes.len() != expected_len {
            return Err(CommonsPolicyError::Malformed);
        }
        if bytes[offset] != TAG_POLICY {
            return Err(CommonsPolicyError::Malformed);
        }
        offset += 1;

        let mut next = || {
            let b = bytes[offset];
            offset += 1;
            Entitlement::from_byte(b)
        };
        Ok(PublicCommonsPolicy {
            view_public_profiles: next()?,
            view_public_objects: next()?,
            create_public_profile: next()?,
            publish_public_object: next()?,
            reply_publicly: next()?,
            comment_publicly: next()?,
            react_publicly: next()?,
            search_public_index: next()?,
        })
    }
}

/// A minimal view of an account's wallet/governance standing. This crate
/// takes it only so its own tests can prove, at the type level, that
/// [`commons_policy_for`] never consults it: the parameter is bound and
/// immediately discarded, not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalletStanding {
    pub balance_micro: u64,
    pub governance_weight: u64,
}

/// The public-commons policy for any account, regardless of its wallet or
/// governance standing. There is deliberately no other constructor that
/// takes a balance or weight and returns a *different* policy: this
/// function's signature is the proof that no such code path exists.
pub fn commons_policy_for(_standing: WalletStanding) -> PublicCommonsPolicy {
    PublicCommonsPolicy::free_commons()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_balance_account_receives_the_full_free_commons_policy() {
        let standing = WalletStanding {
            balance_micro: 0,
            governance_weight: 0,
        };
        let policy = commons_policy_for(standing);
        assert_eq!(policy.publish_public_object, Entitlement::FreeProtocolRight);
        assert_eq!(policy.reply_publicly, Entitlement::FreeProtocolRight);
        assert_eq!(policy.comment_publicly, Entitlement::FreeProtocolRight);
        assert_eq!(policy.react_publicly, Entitlement::FreeProtocolRight);
        assert_eq!(policy.create_public_profile, Entitlement::FreeProtocolRight);
    }

    #[test]
    fn a_large_balance_grants_no_greater_protocol_authority_than_zero() {
        let poor = commons_policy_for(WalletStanding {
            balance_micro: 0,
            governance_weight: 0,
        });
        let rich = commons_policy_for(WalletStanding {
            balance_micro: u64::MAX,
            governance_weight: 0,
        });
        assert_eq!(
            poor, rich,
            "a wealthy account must receive exactly the same commons policy as a penniless one"
        );
    }

    #[test]
    fn payment_cannot_alter_governance_weight_in_the_returned_policy() {
        let low_weight = commons_policy_for(WalletStanding {
            balance_micro: u64::MAX,
            governance_weight: 1,
        });
        let high_weight = commons_policy_for(WalletStanding {
            balance_micro: u64::MAX,
            governance_weight: u64::MAX,
        });
        assert_eq!(
            low_weight, high_weight,
            "governance weight must never change the commons policy a payment could buy"
        );
    }

    #[test]
    fn every_standing_across_a_balance_and_weight_sweep_yields_the_identical_policy() {
        let reference = PublicCommonsPolicy::free_commons();
        for balance_micro in [0, 1, 1_000, u64::MAX / 2, u64::MAX] {
            for governance_weight in [0, 1, 1_000, u64::MAX / 2, u64::MAX] {
                let policy = commons_policy_for(WalletStanding {
                    balance_micro,
                    governance_weight,
                });
                assert_eq!(policy, reference);
            }
        }
    }

    #[test]
    fn a_policy_round_trips_through_wire_bytes() {
        let policy = PublicCommonsPolicy::free_commons();
        assert_eq!(
            PublicCommonsPolicy::from_wire_bytes(&policy.to_wire_bytes()).unwrap(),
            policy
        );
    }

    #[test]
    fn a_policy_with_an_unsupported_entitlement_round_trips() {
        let policy = PublicCommonsPolicy {
            search_public_index: Entitlement::Unsupported,
            ..PublicCommonsPolicy::free_commons()
        };
        assert_eq!(
            PublicCommonsPolicy::from_wire_bytes(&policy.to_wire_bytes()).unwrap(),
            policy
        );
    }

    #[test]
    fn a_truncated_policy_is_rejected_at_every_length() {
        let full = PublicCommonsPolicy::free_commons().to_wire_bytes();
        for cut in 0..full.len() {
            assert!(
                PublicCommonsPolicy::from_wire_bytes(&full[..cut]).is_err(),
                "truncating a policy to {cut} bytes must be rejected"
            );
        }
    }

    #[test]
    fn trailing_bytes_after_a_well_formed_policy_are_rejected() {
        let mut bytes = PublicCommonsPolicy::free_commons().to_wire_bytes();
        bytes.push(0xff);
        assert!(PublicCommonsPolicy::from_wire_bytes(&bytes).is_err());
    }

    #[test]
    fn a_wrong_domain_is_rejected() {
        let mut out = Vec::new();
        let wrong_domain: &[u8] = b"not-the-right-domain";
        out.extend_from_slice(&(wrong_domain.len() as u32).to_be_bytes());
        out.extend_from_slice(wrong_domain);
        out.push(TAG_POLICY);
        out.extend_from_slice(&[1u8; 8]);
        assert!(PublicCommonsPolicy::from_wire_bytes(&out).is_err());
    }

    #[test]
    fn an_unknown_tag_is_rejected() {
        let mut out = Vec::new();
        out.extend_from_slice(&(DOMAIN.len() as u32).to_be_bytes());
        out.extend_from_slice(DOMAIN);
        out.push(0xee);
        out.extend_from_slice(&[1u8; 8]);
        assert!(PublicCommonsPolicy::from_wire_bytes(&out).is_err());
    }

    #[test]
    fn an_unknown_entitlement_byte_is_rejected() {
        let mut out = Vec::new();
        out.extend_from_slice(&(DOMAIN.len() as u32).to_be_bytes());
        out.extend_from_slice(DOMAIN);
        out.push(TAG_POLICY);
        out.extend_from_slice(&[0xee; 8]);
        assert_eq!(
            PublicCommonsPolicy::from_wire_bytes(&out),
            Err(CommonsPolicyError::Malformed)
        );
    }

    #[test]
    fn every_entitlement_round_trips_through_its_byte() {
        for e in [Entitlement::FreeProtocolRight, Entitlement::Unsupported] {
            assert_eq!(Entitlement::from_byte(e.to_byte()).unwrap(), e);
        }
    }
}
