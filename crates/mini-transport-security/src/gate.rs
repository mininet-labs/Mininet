//! Runtime privacy-tier execution gate.
//!
//! Policy vocabulary is not implementation evidence. Direct and Relayed have
//! concrete executors after #291; Mixed and Burst remain unavailable until the
//! exact Sphinx/Loopix executor receives independent review under #72/D-0305.

use mini_privacy_policy::PrivacyTier;

use crate::{Result, TransportSecurityError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutableTransport {
    /// Anonymous CH1: encrypted and forward-secret, no endpoint identity claim.
    AnonymousDirect,
    /// CH1 plus the optional transcript-bound delegated-device proof.
    AuthenticatedDirect,
    /// Exact three-role layered onion path from `mini-relay`.
    ThreeHopOnion,
}

pub fn executable_transport(
    tier: PrivacyTier,
    authenticate_direct_peer: bool,
) -> Result<ExecutableTransport> {
    match tier {
        PrivacyTier::Direct => Ok(if authenticate_direct_peer {
            ExecutableTransport::AuthenticatedDirect
        } else {
            ExecutableTransport::AnonymousDirect
        }),
        PrivacyTier::Relayed => Ok(ExecutableTransport::ThreeHopOnion),
        PrivacyTier::Mixed | PrivacyTier::Burst => {
            Err(TransportSecurityError::MixedTransportNotReviewed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implemented_tiers_map_to_real_executors() {
        assert_eq!(
            executable_transport(PrivacyTier::Direct, false).unwrap(),
            ExecutableTransport::AnonymousDirect
        );
        assert_eq!(
            executable_transport(PrivacyTier::Direct, true).unwrap(),
            ExecutableTransport::AuthenticatedDirect
        );
        assert_eq!(
            executable_transport(PrivacyTier::Relayed, false).unwrap(),
            ExecutableTransport::ThreeHopOnion
        );
    }

    #[test]
    fn mixed_and_burst_fail_closed_until_external_review() {
        for tier in [PrivacyTier::Mixed, PrivacyTier::Burst] {
            assert_eq!(
                executable_transport(tier, false),
                Err(TransportSecurityError::MixedTransportNotReviewed)
            );
        }
    }
}
