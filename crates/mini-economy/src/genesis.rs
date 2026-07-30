use std::collections::BTreeSet;

use mini_crypto::{HashAlgorithm, Multihash};

use crate::{Amount, EconomyError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisPolicy {
    /// Equal slowly-vesting bootstrap allocation for every eligible human.
    /// The value is governance-bound input, never selected by this crate.
    pub bootstrap_per_human: Amount,
    pub vesting_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisManifest {
    pub chain_id: String,
    pub constitution_digest: [u8; 32],
    pub recipients: Vec<String>,
    pub allocation_per_human: Amount,
    pub total_locked: Amount,
    pub vesting_ms: u64,
    pub digest: Multihash,
}

pub fn build_genesis(
    chain_id: &str,
    constitution_digest: [u8; 32],
    eligible_humans: &[String],
    policy: &GenesisPolicy,
) -> Result<GenesisManifest> {
    if chain_id.is_empty()
        || chain_id.len() > 128
        || eligible_humans.is_empty()
        || policy.bootstrap_per_human == Amount::ZERO
        || policy.vesting_ms == 0
    {
        return Err(EconomyError::InvalidGenesis);
    }
    let recipients: BTreeSet<String> = eligible_humans.iter().cloned().collect();
    if recipients.len() != eligible_humans.len() || recipients.iter().any(|id| id.is_empty()) {
        return Err(EconomyError::DuplicateBeneficiary);
    }
    let recipients: Vec<String> = recipients.into_iter().collect();
    let total = policy
        .bootstrap_per_human
        .as_micro()
        .checked_mul(recipients.len() as u128)
        .map(Amount::from_micro)
        .ok_or(EconomyError::Overflow)?;

    let mut bytes = Vec::new();
    put_bytes(&mut bytes, chain_id.as_bytes());
    bytes.extend_from_slice(&constitution_digest);
    bytes.extend_from_slice(&policy.bootstrap_per_human.as_micro().to_be_bytes());
    bytes.extend_from_slice(&policy.vesting_ms.to_be_bytes());
    bytes.extend_from_slice(&(recipients.len() as u64).to_be_bytes());
    for recipient in &recipients {
        put_bytes(&mut bytes, recipient.as_bytes());
    }
    let digest = Multihash::of(HashAlgorithm::Blake3, &bytes);
    Ok(GenesisManifest {
        chain_id: chain_id.to_owned(),
        constitution_digest,
        recipients,
        allocation_per_human: policy.bootstrap_per_human,
        total_locked: total,
        vesting_ms: policy.vesting_ms,
        digest,
    })
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}
