//! Local private-account projection. No public account ledger and no new money.
//!
//! Signed offline receipts are pending credit, never available funds. Only an
//! atomic canonical inclusion plus the exact output commitment can unlock credit.
//! This is a bounded projection over caller-retained claims, not a durable wallet,
//! complete-history proof, personhood verifier, lender or consensus implementation.
//! Callers must supply one coherent, independently verified, network-bound ledger
//! snapshot and retain every broadcast outgoing claim until canonically resolved.

use std::collections::{BTreeMap, BTreeSet};

use mini_settlement::SettlementState;
use mini_value::{MininetStealthAddress, StealthAddressScheme, StealthKeypair};
use zeroize::{Zeroize, Zeroizing};

use crate::{reconcile, PrivateLedgerView, PrivatePaymentError, VerifiedPrivateClaim};

/// Work bound per projection, including duplicate supplied claims.
pub const MAX_ACCOUNT_CLAIMS: usize = 1_024;

/// Canonical output membership is required in addition to spent-image lookup.
/// Implementations must bind both lookups to the same validated chain snapshot.
pub trait AccountLedgerView: PrivateLedgerView {
    /// Commitment recorded for this one-time key; never a caller's claimed amount.
    fn output_commitment(&self, one_time_key: &[u8]) -> Option<Vec<u8>>;
}

/// Local status of one received output; never published to the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditStatus {
    /// Valid signed receipt with locally known input backing, but no finality.
    Pending,
    /// Conflicting signed receipts are known locally; none is spendable.
    ConflictingPending,
    /// Input membership or final output evidence is missing/inconsistent.
    Unverified,
    /// Canonically created output, not spent or reserved in the supplied view.
    Available,
    /// Canonically created output reserved by an unresolved local spend.
    Reserved,
    /// This received output has itself been spent canonically.
    Spent,
    /// The incoming claim lost canonical ordering or was rejected.
    Rejected,
    /// Local device-clock expiry; not proof that a broadcast claim cannot settle.
    Expired,
}

/// Non-secret local receipt reference and its opened amount, if known.
/// Debug redacts amounts and transaction references to avoid incidental logs.
#[derive(Clone, PartialEq, Eq)]
pub struct AccountCredit {
    pub claim_digest: [u8; 32],
    pub output_index: usize,
    pub amount_micro: Option<u64>,
    pub status: CreditStatus,
}

impl core::fmt::Debug for AccountCredit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AccountCredit")
            .field("status", &self.status)
            .field("amount_known", &self.amount_micro.is_some())
            .finish_non_exhaustive()
    }
}

/// Totals cover only the supplied history and coherent ledger checkpoint.
/// Pending, unknown, rejected and expired receipts are excluded from available.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub available_micro: u128,
    pub reserved_micro: u128,
    pub pending_micro: u128,
    pub conflicting_pending_micro: u128,
    pub spent_micro: u128,
    pub unknown_amount_outputs: usize,
    pub credits: Vec<AccountCredit>,
}

impl core::fmt::Debug for AccountSnapshot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AccountSnapshot")
            .field("outputs", &self.credits.len())
            .field("unknown_amount_outputs", &self.unknown_amount_outputs)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountError {
    LimitExceeded,
    NetworkMismatch,
    Overflow,
    InvalidKey,
    /// Two canonically credited receipts name the same output key.
    InconsistentLedger,
}

impl core::fmt::Display for AccountError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "private account projection failed: {self:?}")
    }
}
impl std::error::Error for AccountError {}

/// Project a private account from verified incoming claims and retained outgoing
/// authorizations. Both view and spend keys stay local: the spend key is needed
/// to identify spent outputs, something a view-only income scan cannot prove.
///
/// This function does not reserve inputs atomically for a new payment. A wallet
/// must serialize selection/signing and durably journal the outgoing claim before
/// broadcast. Do not release reservations merely because a local clock expired;
/// existing private-claim expiry is not a canonical cancellation mechanism.
pub fn account_snapshot(
    network_id: &[u8; 32],
    keys: &StealthKeypair,
    incoming: &[VerifiedPrivateClaim],
    outgoing: &[VerifiedPrivateClaim],
    ledger: &impl AccountLedgerView,
    now_ms: u64,
) -> Result<AccountSnapshot, AccountError> {
    if incoming.len().saturating_add(outgoing.len()) > MAX_ACCOUNT_CLAIMS {
        return Err(AccountError::LimitExceeded);
    }
    if incoming
        .iter()
        .chain(outgoing)
        .any(|c| &c.claim().network_id != network_id)
    {
        return Err(AccountError::NetworkMismatch);
    }
    let mut reservations = BTreeSet::new();
    for claim in outgoing {
        // Terminal canonical failures release a reservation. Local expiry and
        // partial evidence deliberately do not: it may still settle elsewhere.
        if !matches!(
            reconcile(claim, ledger, now_ms),
            Ok(SettlementState::Finalized
                | SettlementState::RejectedConflict
                | SettlementState::RejectedCanonical(_))
        ) {
            reservations.extend(claim.key_images().map(<[u8]>::to_vec));
        }
    }
    let unique: BTreeMap<_, _> = incoming
        .iter()
        .map(|claim| (*claim.transcript_digest(), claim))
        .collect();
    let mut image_users: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    for claim in unique.values() {
        for image in claim.key_images() {
            *image_users.entry(image.to_vec()).or_default() += 1;
        }
    }
    let view = Zeroizing::new(keys.view_secret_bytes());
    let spend = Zeroizing::new(keys.spend_secret_bytes());
    let mut result = AccountSnapshot::default();
    let mut credited_keys = BTreeSet::new();
    for (digest, claim) in unique {
        let state = reconcile(claim, ledger, now_ms);
        let backed = claim.claim().inputs.iter().all(|input| {
            input
                .ring
                .iter()
                .zip(&input.ring_commitments)
                .all(|(key, commitment)| ledger.output_commitment(key).as_ref() == Some(commitment))
        });
        for (index, output) in claim.claim().outputs.iter().enumerate() {
            if !MininetStealthAddress.recognizes(&*view, &keys.spend_public_bytes(), &output.output)
            {
                continue;
            }
            let amount = mini_value::recover_shared_secret(&*view, &output.output.tx_public_key)
                .and_then(|shared| claim.open_memo(index, &shared).ok())
                .filter(|note| {
                    mini_value::pedersen_commitment(note.amount_micro, &note.blinding)
                        .is_some_and(|commitment| commitment.as_slice() == output.amount_commitment)
                })
                .map(|note| note.amount_micro);
            let mut status = match state {
                Ok(SettlementState::Finalized) => {
                    if ledger
                        .output_commitment(&output.output.one_time_address)
                        .as_ref()
                        != Some(&output.amount_commitment)
                    {
                        CreditStatus::Unverified
                    } else {
                        if !credited_keys.insert(output.output.one_time_address.clone()) {
                            return Err(AccountError::InconsistentLedger);
                        }
                        let mut scalar =
                            mini_value::derive_spend_scalar(&*view, &*spend, &output.output)
                                .ok_or(AccountError::InvalidKey)?;
                        let secret = Zeroizing::new(scalar.to_bytes());
                        scalar.zeroize();
                        let image =
                            mini_value::spend_key_image(&secret).ok_or(AccountError::InvalidKey)?;
                        if ledger.finalized_claim(&image).is_some() {
                            CreditStatus::Spent
                        } else if reservations.contains(image.as_slice()) {
                            CreditStatus::Reserved
                        } else {
                            CreditStatus::Available
                        }
                    }
                }
                Ok(SettlementState::RejectedConflict | SettlementState::RejectedCanonical(_)) => {
                    CreditStatus::Rejected
                }
                Ok(SettlementState::Expired) => CreditStatus::Expired,
                Err(PrivatePaymentError::IncompleteFinality) => CreditStatus::Unverified,
                Err(_) => CreditStatus::Unverified,
                _ if !backed => CreditStatus::Unverified,
                _ if claim.key_images().any(|image| image_users[image] > 1) => {
                    CreditStatus::ConflictingPending
                }
                _ => CreditStatus::Pending,
            };
            if amount.is_none() {
                result.unknown_amount_outputs += 1;
                // A recognized output with an invalid opening cannot be selected
                // for spending even if its creation is canonically recorded.
                if status == CreditStatus::Available {
                    status = CreditStatus::Unverified;
                }
            }
            if let Some(amount) = amount {
                let total = match status {
                    CreditStatus::Available => Some(&mut result.available_micro),
                    CreditStatus::Reserved => Some(&mut result.reserved_micro),
                    CreditStatus::Pending => Some(&mut result.pending_micro),
                    CreditStatus::ConflictingPending => Some(&mut result.conflicting_pending_micro),
                    CreditStatus::Spent => Some(&mut result.spent_micro),
                    _ => None,
                };
                if let Some(total) = total {
                    *total = total
                        .checked_add(u128::from(amount))
                        .ok_or(AccountError::Overflow)?;
                }
            }
            result.credits.push(AccountCredit {
                claim_digest: digest,
                output_index: index,
                amount_micro: amount,
                status,
            });
        }
    }
    Ok(result)
}
