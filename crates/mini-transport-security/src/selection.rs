//! Locally seeded, bounded, prefix-diverse peer selection.
//!
//! This raises the cost of an eclipse without pretending IP diversity proves
//! independent ownership. Selection is independent of advertisement input order
//! and uses a caller-local seed, so no discovery peer controls first position.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;

use mini_crypto::HashAlgorithm;

use crate::{Result, TransportEndpointId, TransportSecurityError, VerifiedPeerAdvertisement};

/// Maximum verified records accepted by one selection call before any
/// allocation/sort. Larger local pools must be sampled or processed in bounded
/// batches by the caller.
pub const MAX_SELECTION_CANDIDATES: usize = 1_024;
pub const MAX_SELECTED_PEERS: usize = 64;
pub const MIN_DIAL_TIMEOUT_MS: u64 = 100;
pub const MAX_DIAL_TIMEOUT_MS: u64 = 60_000;
const SELECTION_DOMAIN: &[u8] = b"mini-transport-security/peer-selection/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerSelectionPolicy {
    pub max_peers: usize,
    pub max_per_network_prefix: usize,
    pub dial_timeout_ms: u64,
}

impl Default for PeerSelectionPolicy {
    fn default() -> Self {
        Self {
            max_peers: 8,
            max_per_network_prefix: 2,
            dial_timeout_ms: 5_000,
        }
    }
}

impl PeerSelectionPolicy {
    pub fn validate(self) -> Result<Self> {
        if self.max_peers == 0
            || self.max_peers > MAX_SELECTED_PEERS
            || self.max_per_network_prefix == 0
            || self.max_per_network_prefix > self.max_peers
            || !(MIN_DIAL_TIMEOUT_MS..=MAX_DIAL_TIMEOUT_MS).contains(&self.dial_timeout_ms)
        {
            return Err(TransportSecurityError::InvalidSelectionPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialAttempt {
    pub endpoint_id: TransportEndpointId,
    pub address: std::net::SocketAddr,
    pub routing_key: mini_crypto::AgreementPublicKey,
    pub timeout_ms: u64,
}

/// Build a bounded dial order. Records must already have passed signature and
/// KEL verification. Duplicate endpoint ids and concentrated network prefixes
/// are skipped; no majority or peer vote is consulted.
pub fn diverse_dial_plan(
    records: &[VerifiedPeerAdvertisement],
    local_seed: [u8; 32],
    policy: PeerSelectionPolicy,
) -> Result<Vec<DialAttempt>> {
    let policy = policy.validate()?;
    if records.len() > MAX_SELECTION_CANDIDATES {
        return Err(TransportSecurityError::LimitExceeded);
    }
    let mut candidates: Vec<_> = records
        .iter()
        .map(|record| (selection_score(record, local_seed), record))
        .collect();
    candidates.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.endpoint_id().cmp(&right.1.endpoint_id()))
            .then_with(|| left.1.address().cmp(&right.1.address()))
    });

    let mut selected = Vec::with_capacity(policy.max_peers);
    let mut endpoints = HashSet::new();
    let mut routing_keys = HashSet::new();
    let mut prefix_counts: HashMap<NetworkPrefix, usize> = HashMap::new();
    for (_, record) in candidates {
        if selected.len() >= policy.max_peers {
            break;
        }
        if !endpoints.insert(record.endpoint_id()) || !routing_keys.insert(record.routing_key()) {
            continue;
        }
        let prefix = NetworkPrefix::from_ip(record.address().ip());
        let count = prefix_counts.entry(prefix).or_default();
        if *count >= policy.max_per_network_prefix {
            continue;
        }
        *count += 1;
        selected.push(DialAttempt {
            endpoint_id: record.endpoint_id(),
            address: record.address(),
            routing_key: record.routing_key(),
            timeout_ms: policy.dial_timeout_ms,
        });
    }
    Ok(selected)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NetworkPrefix {
    V4([u8; 3]),
    V6([u8; 6]),
}

impl NetworkPrefix {
    fn from_ip(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(value) => {
                let octets = value.octets();
                Self::V4([octets[0], octets[1], octets[2]])
            }
            IpAddr::V6(value) => {
                let octets = value.octets();
                Self::V6([
                    octets[0], octets[1], octets[2], octets[3], octets[4], octets[5],
                ])
            }
        }
    }
}

fn selection_score(record: &VerifiedPeerAdvertisement, local_seed: [u8; 32]) -> [u8; 32] {
    let mut transcript = Vec::with_capacity(SELECTION_DOMAIN.len() + 32 + 32 + 18);
    transcript.extend_from_slice(SELECTION_DOMAIN);
    transcript.extend_from_slice(&local_seed);
    transcript.extend_from_slice(&record.endpoint_id().to_bytes());
    match record.address().ip() {
        IpAddr::V4(value) => {
            transcript.push(4);
            transcript.extend_from_slice(&value.octets());
        }
        IpAddr::V6(value) => {
            transcript.push(6);
            transcript.extend_from_slice(&value.octets());
        }
    }
    transcript.extend_from_slice(&record.address().port().to_be_bytes());
    HashAlgorithm::Blake3.digest(&transcript)
}

#[cfg(test)]
mod tests {
    use did_mini::{Capabilities, Controller, FreshnessPins};
    use mini_crypto::AgreementSecretKey;

    use super::*;
    use crate::{PeerAdvertisement, ReplayCache};

    fn verified(seed: u8, address: &str) -> VerifiedPeerAdvertisement {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        let routing = AgreementSecretKey::from_seed(&[seed + 4; 32]).public_key();
        let advertisement = PeerAdvertisement::issue(
            [7; 32],
            &root.did(),
            &device,
            routing,
            address.parse().unwrap(),
            1_000,
            2_000,
        )
        .unwrap();
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(8).unwrap();
        advertisement
            .verify(
                [7; 32],
                1_500,
                &root.kel(),
                &device.kel(),
                &mut freshness,
                &mut replay,
            )
            .unwrap()
    }

    #[test]
    fn selection_is_input_order_independent_and_prefix_bounded() {
        let a = verified(10, "10.0.0.1:9000");
        let b = verified(20, "10.0.0.2:9000");
        let c = verified(30, "10.0.1.1:9000");
        let d = verified(40, "10.0.2.1:9000");
        let policy = PeerSelectionPolicy {
            max_peers: 3,
            max_per_network_prefix: 1,
            dial_timeout_ms: 1_000,
        };
        let forward = diverse_dial_plan(
            &[a.clone(), b.clone(), c.clone(), d.clone()],
            [9; 32],
            policy,
        )
        .unwrap();
        let reverse = diverse_dial_plan(&[d, c, b, a], [9; 32], policy).unwrap();
        assert_eq!(forward, reverse);
        assert_eq!(forward.len(), 3);
        let same_prefix = forward
            .iter()
            .filter(|attempt| match attempt.address.ip() {
                IpAddr::V4(ip) => ip.octets()[..3] == [10, 0, 0],
                IpAddr::V6(_) => false,
            })
            .count();
        assert!(same_prefix <= 1);
    }

    #[test]
    fn candidate_input_is_bounded_before_sorting() {
        let record = verified(10, "10.0.0.1:9000");
        let oversized = vec![record; MAX_SELECTION_CANDIDATES + 1];
        assert_eq!(
            diverse_dial_plan(&oversized, [1; 32], PeerSelectionPolicy::default()),
            Err(TransportSecurityError::LimitExceeded)
        );
    }

    #[test]
    fn local_seed_is_part_of_the_selection_score() {
        let record = verified(10, "10.0.0.1:9000");
        assert_ne!(
            selection_score(&record, [1; 32]),
            selection_score(&record, [2; 32])
        );
    }
}
