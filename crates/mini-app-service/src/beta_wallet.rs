//! BETA wallet commands: register account, balances, transfer.
//!
//! BETA is provisional, resettable test value in an explicit epoch. It is not
//! production MINI and carries no governance, review, release or personhood
//! weight. The issuance trust anchor is only the caller-supplied campaign
//! list; the store is never scanned for campaigns. Error text and logs never
//! include account or object ids.

use mini_app_protocol::{
    BetaAccountBalance, BetaBalancesView, ErrorCode, Reply, ServiceError, MAX_BETA_ACCOUNTS,
};
use mini_beta::{BetaAccountId, BetaEpochId, BetaError, BetaMiniPolicy};
use mini_beta_exec::{
    build_account_registration, build_transfer, derive_account_id,
    parse_account_registration_object, resolve_snapshot, BetaExecError, BetaOutputRef,
    BetaTransferOutput, EpochIssuancePolicy, BETA_ACCOUNT_TYPE, MAX_TRANSFER_INPUTS,
};
use mini_beta_grants::{
    build_grant_approval, detect_authorizer_equivocations, parse_grant_approval_object,
    validate_grant_acceptance, validate_policy, GrantAcceptanceError, BETA_GRANT_APPROVAL_TYPE,
};
use mini_crypto::random_32;
use mini_crypto::KdfSuite;
use mini_objects::{ObjectId, ObjectType};
use mini_store::StoreError;
use mini_windows_vault::SeedPair;

use crate::beta_journal::{self, Record, KIND_APPROVAL, KIND_REGISTER, KIND_TRANSFER};
use crate::{
    identity_from_seeds, next_object_sequence, now_ms, storage_error, validate_operation, Core,
    CoreIdentity, IdentityVault,
};

/// HKDF domain separating per-account BETA signing identities from every other
/// use of the root seed.
/// Bound on objects examined when discovering a policy's approvals.
const MAX_STATUS_SCAN: usize = 4096;

const ACCOUNT_IDENTITY_INFO: &[u8] = b"mininet/beta/account-identity/v1";

/// One BETA account's isolated signing identity. It is derived, never stored,
/// from the unlocked root seed and the account's random registration nonce, so
/// the author DID on public BETA objects is unrelated to the primary Mininet
/// DID and unlinkable without the seed. A fresh random nonce per registration
/// also means two devices restored from one seed never share an identity,
/// account id, or author sequence.
pub(crate) struct AccountSlot {
    pub(crate) identity: CoreIdentity,
    pub(crate) nonce: [u8; 32],
    pub(crate) account: BetaAccountId,
}

fn derive_slot(
    root: &SeedPair,
    epoch: BetaEpochId,
    nonce: [u8; 32],
) -> Result<AccountSlot, ServiceError> {
    let mut info = Vec::with_capacity(ACCOUNT_IDENTITY_INFO.len() + 64);
    info.extend_from_slice(ACCOUNT_IDENTITY_INFO);
    info.extend_from_slice(epoch.as_bytes());
    info.extend_from_slice(&nonce);
    let mut okm = KdfSuite::HkdfSha256
        .derive_bytes(None, &root.current, &info, 128)
        .map_err(|_| ServiceError::new(ErrorCode::Identity, "BETA identity derivation failed"))?;
    let seed = |at: usize| -> [u8; 32] { okm[at..at + 32].try_into().expect("128 bytes") };
    let root_seeds = SeedPair {
        current: seed(0),
        next: seed(32),
    };
    let device_seeds = SeedPair {
        current: seed(64),
        next: seed(96),
    };
    okm.fill(0);
    let identity = identity_from_seeds(&root_seeds, &device_seeds)?;
    let owner = identity.root.did();
    let account = derive_account_id(epoch, &owner, nonce).map_err(exec_error)?;
    Ok(AccountSlot {
        identity,
        nonce,
        account,
    })
}

pub(crate) fn input_digest(kind: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"mininet/beta/operation-input/v1");
    hasher.update(kind);
    for part in parts {
        hasher.update(&(part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

fn bad(message: &str) -> ServiceError {
    ServiceError::new(ErrorCode::BadRequest, message)
}

fn exec_error(error: BetaExecError) -> ServiceError {
    match error {
        BetaExecError::Store(_) => ServiceError::new(ErrorCode::Storage, "BETA store error"),
        BetaExecError::InvalidObject
        | BetaExecError::InvalidAccount
        | BetaExecError::InvalidTransfer
        | BetaExecError::WrongEpoch
        | BetaExecError::Beta(_)
        | BetaExecError::Object(_) => bad("BETA request rejected"),
        _ => ServiceError::new(ErrorCode::Internal, "BETA execution error"),
    }
}

/// Map a grant-library error to fixed text. Never includes object ids,
/// account ids or DIDs.
fn grants_error(error: GrantAcceptanceError) -> ServiceError {
    match error {
        GrantAcceptanceError::Store(StoreError::NotFound)
        | GrantAcceptanceError::Beta(BetaError::Store(StoreError::NotFound)) => {
            bad("grant policy or grant not found in this store")
        }
        GrantAcceptanceError::Store(_) | GrantAcceptanceError::Beta(BetaError::Store(_)) => {
            ServiceError::new(ErrorCode::Storage, "BETA store error")
        }
        GrantAcceptanceError::InvalidApproval => bad(
            "this identity cannot approve this grant (not a policy member, or outside the policy window)",
        ),
        GrantAcceptanceError::EvidenceLimit => bad("too many approvals to evaluate here"),
        GrantAcceptanceError::PolicyConflict => {
            bad("more than one grant policy exists for this campaign")
        }
        GrantAcceptanceError::GrantRuleViolation => bad("the grant violates its policy"),
        GrantAcceptanceError::InvalidObject
        | GrantAcceptanceError::InvalidPolicy
        | GrantAcceptanceError::PolicyAuthorMismatch
        | GrantAcceptanceError::Beta(_)
        | GrantAcceptanceError::Object(_) => bad("grant or policy is not valid"),
        _ => ServiceError::new(ErrorCode::Internal, "BETA grant error"),
    }
}

fn object_id(value: &str) -> Result<ObjectId, ServiceError> {
    ObjectId::parse(value).map_err(|_| bad("invalid object id"))
}

fn epoch_id(bytes: [u8; 32]) -> Result<BetaEpochId, ServiceError> {
    BetaEpochId::new(bytes).map_err(|_| bad("invalid BETA epoch"))
}

fn account_id(bytes: [u8; 32]) -> Result<BetaAccountId, ServiceError> {
    BetaAccountId::new(bytes).map_err(|_| bad("invalid BETA account"))
}

fn issuance_policy(
    epoch: BetaEpochId,
    campaigns: &[String],
) -> Result<EpochIssuancePolicy, ServiceError> {
    let ids = campaigns
        .iter()
        .map(|c| ObjectId::parse(c).map_err(|_| bad("invalid BETA campaign id")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EpochIssuancePolicy::new(epoch, &ids))
}

impl<V: IdentityVault> Core<V> {
    /// Registrations in `epoch` as `(registration id, owner, account)`.
    fn beta_registrations(
        &self,
        epoch: BetaEpochId,
    ) -> Result<Vec<(ObjectId, did_mini::Did, BetaAccountId)>, ServiceError> {
        let mut found = Vec::new();
        for id in self
            .store
            .by_type(&ObjectType::Custom(BETA_ACCOUNT_TYPE.to_string()))
            .map_err(storage_error)?
        {
            let object = self.store.get(&id).map_err(storage_error)?;
            let Ok(registration) = parse_account_registration_object(&object) else {
                continue;
            };
            if registration.epoch == epoch {
                found.push((id, registration.owner, registration.account));
            }
        }
        found.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        Ok(found)
    }

    /// This profile's registered accounts in `epoch` (from its local registration
    /// receipts), each with its derived signing identity.
    pub(crate) fn beta_own_slots(
        &mut self,
        epoch: BetaEpochId,
    ) -> Result<Vec<AccountSlot>, ServiceError> {
        let nonces = beta_journal::owned_nonces(&self.beta_ops, epoch.as_bytes())?;
        if nonces.is_empty() {
            return Ok(Vec::new());
        }
        let root = self.vault.load_root()?;
        nonces
            .into_iter()
            .map(|nonce| derive_slot(&root, epoch, nonce))
            .collect()
    }

    /// Publish the derived identity's KEL carriers so peers can verify its
    /// objects through ordinary verified sync. Deterministic and idempotent.
    fn beta_publish_kels(&mut self, slot: &AccountSlot) -> Result<(), ServiceError> {
        let owner = slot.identity.root.did();
        for kel in [slot.identity.root.kel(), slot.identity.device.kel()] {
            let carrier = mini_sync::kel_carrier(&kel, &owner, &slot.identity.device)
                .map_err(storage_error)?;
            self.store.insert(&carrier).map_err(storage_error)?;
        }
        Ok(())
    }

    pub(crate) fn beta_reply_from(receipt: &beta_journal::Receipt) -> Reply {
        if receipt.kind == KIND_REGISTER {
            Reply::BetaAccount {
                account: receipt.account,
                registration_id: receipt.object_id.clone(),
            }
        } else if receipt.kind == KIND_APPROVAL {
            Reply::BetaGrantApproval {
                approval_id: receipt.object_id.clone(),
            }
        } else {
            Reply::BetaTransfer {
                transfer_id: receipt.object_id.clone(),
                amount_micro: receipt.amount,
                change_micro: receipt.change,
            }
        }
    }

    /// Replay an existing operation (finishing a half-committed one) or `None`.
    fn beta_replay(
        &mut self,
        operation_id: &str,
        kind: u8,
        digest: [u8; 32],
    ) -> Result<Option<Reply>, ServiceError> {
        let receipt = match beta_journal::lookup(&self.beta_ops, operation_id)? {
            None => return Ok(None),
            Some(beta_journal::Entry::Done(receipt)) => receipt,
            Some(beta_journal::Entry::Pending(record)) => {
                if record.kind != kind || record.digest != digest {
                    return Err(bad("operation id was already used with different content"));
                }
                beta_journal::complete(&self.beta_ops, &mut self.store, operation_id, &record)?
            }
        };
        if receipt.kind != kind || receipt.digest != digest {
            return Err(bad("operation id was already used with different content"));
        }
        Ok(Some(Self::beta_reply_from(&receipt)))
    }

    fn beta_journal_and_commit(
        &mut self,
        operation_id: &str,
        record: Record,
    ) -> Result<Reply, ServiceError> {
        beta_journal::write_pending(&self.beta_ops, operation_id, &record)?;
        let receipt =
            beta_journal::complete(&self.beta_ops, &mut self.store, operation_id, &record)?;
        Ok(Self::beta_reply_from(&receipt))
    }

    pub(crate) fn beta_register_account(
        &mut self,
        operation_id: &str,
        epoch: [u8; 32],
    ) -> Result<Reply, ServiceError> {
        validate_operation(operation_id)?;
        let epoch = epoch_id(epoch)?;
        let digest = input_digest(b"register", &[epoch.as_bytes()]);
        if let Some(reply) = self.beta_replay(operation_id, KIND_REGISTER, digest)? {
            return Ok(reply);
        }
        self.ensure_signing_available()?;
        beta_journal::commit_pending(&self.beta_ops, &mut self.store)?;
        if self.beta_own_slots(epoch)?.len() >= usize::from(MAX_BETA_ACCOUNTS) {
            return Err(bad("BETA account limit reached for this epoch"));
        }
        let nonce = random_32()
            .map_err(|_| ServiceError::new(ErrorCode::Internal, "entropy source unavailable"))?;
        let slot = derive_slot(&self.vault.load_root()?, epoch, nonce)?;
        let owner = slot.identity.root.did();
        let sequence = next_object_sequence(&self.store, Some(&owner))?;
        let object = build_account_registration(
            &owner,
            &slot.identity.device,
            epoch,
            slot.nonce,
            now_ms(),
            sequence,
        )
        .map_err(exec_error)?;
        self.beta_publish_kels(&slot)?;
        self.beta_journal_and_commit(
            operation_id,
            Record {
                kind: KIND_REGISTER,
                digest,
                account: *slot.account.as_bytes(),
                amount: 0,
                change: 0,
                object: object.to_bytes(),
            },
        )
    }

    pub(crate) fn beta_balances(
        &mut self,
        epoch: [u8; 32],
        campaigns: &[String],
    ) -> Result<Reply, ServiceError> {
        let epoch = epoch_id(epoch)?;
        let policy = issuance_policy(epoch, campaigns)?;
        if self.human.is_none() {
            return Err(ServiceError::new(
                ErrorCode::RootMissing,
                "create a Mininet root first",
            ));
        }
        let owned = self.beta_own_slots(epoch)?;
        let mut accounts: Vec<[u8; 32]> = owned.iter().map(|s| *s.account.as_bytes()).collect();
        accounts.sort();
        let snapshot = resolve_snapshot(
            &self.store,
            epoch,
            BetaMiniPolicy::open_beta_default(),
            &policy,
        )
        .map_err(exec_error)?;
        let conflicts = snapshot
            .account_conflicts
            .len()
            .saturating_add(snapshot.transfer_conflicts.len())
            .saturating_add(snapshot.invalid_transfers.len())
            .saturating_add(usize::from(snapshot.issuance_conflict));
        let mut balances = Vec::with_capacity(accounts.len());
        for bytes in accounts {
            let account = account_id(bytes)?;
            balances.push(BetaAccountBalance {
                account: bytes,
                amount_micro: snapshot.balance(&account),
            });
        }
        Ok(Reply::BetaBalances(BetaBalancesView {
            epoch: *epoch.as_bytes(),
            accounts: balances,
            total_issued_micro: snapshot.total_issued,
            conflicts: u32::try_from(conflicts).unwrap_or(u32::MAX),
            snapshot_digest: snapshot.digest,
        }))
    }

    /// Approve one exact grant under one policy.
    ///
    /// Signed by the user's PRIMARY identity, deliberately NOT the
    /// per-account derived identity: grant-policy members are public DIDs by
    /// design (the threshold is over named authorizers), so the approval must
    /// be attributable to the member's real DID. It therefore links the
    /// approver's public DID to the approved grant.
    pub(crate) fn beta_grant_approve(
        &mut self,
        operation_id: &str,
        policy_id: &str,
        grant_id: &str,
    ) -> Result<Reply, ServiceError> {
        validate_operation(operation_id)?;
        let policy = object_id(policy_id)?;
        let grant = object_id(grant_id)?;
        let digest = input_digest(
            b"grant-approve",
            &[policy.as_str().as_bytes(), grant.as_str().as_bytes()],
        );
        if let Some(reply) = self.beta_replay(operation_id, KIND_APPROVAL, digest)? {
            return Ok(reply);
        }
        self.ensure_signing_available()?;
        beta_journal::commit_pending(&self.beta_ops, &mut self.store)?;
        let sequence = self.reserve_sequence()?;
        let (human, device) = self.signing_identity()?;
        let object = build_grant_approval(
            &self.store,
            human,
            device,
            &policy,
            &grant,
            now_ms(),
            sequence,
        )
        .map_err(grants_error)?;
        self.beta_journal_and_commit(
            operation_id,
            Record {
                kind: KIND_APPROVAL,
                digest,
                account: [0; 32],
                amount: 0,
                change: 0,
                object: object.to_bytes(),
            },
        )
    }

    /// Read-only: how many distinct policy members approved `grant_id`, and
    /// whether the policy threshold is met. Approvals are discovered by
    /// scanning objects that link to the policy; invalid or foreign ones are
    /// ignored. An accepted grant is evidence only, never a conversion.
    pub(crate) fn beta_grant_status(
        &self,
        policy_id: &str,
        grant_id: &str,
    ) -> Result<Reply, ServiceError> {
        let policy = object_id(policy_id)?;
        let grant = object_id(grant_id)?;
        validate_policy(&self.store, &policy).map_err(grants_error)?;
        let approval_type = ObjectType::Custom(BETA_GRANT_APPROVAL_TYPE.to_string());
        let mut scanned = 0usize;
        // One approval per (author, grant): repeats count once anyway.
        let mut unique: std::collections::BTreeMap<(String, String), ObjectId> =
            std::collections::BTreeMap::new();
        for id in self.store.linking_to(&policy).map_err(storage_error)? {
            scanned += 1;
            if scanned > MAX_STATUS_SCAN {
                return Err(bad("too many objects reference this policy to evaluate"));
            }
            let object = self.store.get(&id).map_err(storage_error)?;
            if object.object_type != approval_type {
                continue;
            }
            let Ok(approval) = parse_grant_approval_object(&object) else {
                continue;
            };
            if approval.policy_id != policy {
                continue;
            }
            match validate_grant_acceptance(
                &self.store,
                &policy,
                &approval.grant_id,
                std::slice::from_ref(&id),
            ) {
                Ok(_) | Err(GrantAcceptanceError::ThresholdNotMet { .. }) => {}
                Err(GrantAcceptanceError::Store(error)) => return Err(storage_error(error)),
                Err(_) => continue,
            }
            let key = (
                approval.record_author.as_str().to_string(),
                approval.grant_id.as_str().to_string(),
            );
            let slot = unique.entry(key).or_insert_with(|| id.clone());
            if id.as_str() < slot.as_str() {
                *slot = id;
            }
        }
        let mut for_grant = Vec::new();
        let mut all = Vec::new();
        for ((_, approved), id) in &unique {
            if approved.as_str() == grant.as_str() {
                for_grant.push(id.clone());
            }
            all.push(id.clone());
        }
        let (accepted, counted, threshold) =
            match validate_grant_acceptance(&self.store, &policy, &grant, &for_grant) {
                Ok(acceptance) => (
                    true,
                    acceptance.distinct_approvers,
                    acceptance.required_approvals,
                ),
                Err(GrantAcceptanceError::ThresholdNotMet { required, observed }) => {
                    (false, observed, required)
                }
                Err(other) => return Err(grants_error(other)),
            };
        let equivocations =
            detect_authorizer_equivocations(&self.store, &policy, &all).map_err(grants_error)?;
        Ok(Reply::BetaGrantStatus {
            accepted,
            approvals_counted: counted,
            threshold,
            equivocations: u16::try_from(equivocations.len()).unwrap_or(u16::MAX),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn beta_transfer(
        &mut self,
        operation_id: &str,
        epoch: [u8; 32],
        campaigns: &[String],
        from: [u8; 32],
        to: [u8; 32],
        amount: u64,
        memo: &str,
    ) -> Result<Reply, ServiceError> {
        validate_operation(operation_id)?;
        let epoch = epoch_id(epoch)?;
        let policy = issuance_policy(epoch, campaigns)?;
        let from = account_id(from)?;
        let to = account_id(to)?;
        if amount == 0 {
            return Err(bad("amount must be positive"));
        }
        let campaign_list = campaigns.join("\n");
        let digest = input_digest(
            b"transfer",
            &[
                epoch.as_bytes(),
                campaign_list.as_bytes(),
                from.as_bytes(),
                to.as_bytes(),
                &amount.to_be_bytes(),
                memo.as_bytes(),
            ],
        );
        if let Some(reply) = self.beta_replay(operation_id, KIND_TRANSFER, digest)? {
            return Ok(reply);
        }
        self.ensure_signing_available()?;
        beta_journal::commit_pending(&self.beta_ops, &mut self.store)?;
        let owned = self.beta_own_slots(epoch)?;
        let slot = owned
            .into_iter()
            .find(|slot| slot.account == from)
            .ok_or_else(|| bad("sender is not a BETA account owned by this identity"))?;
        let registrations = self.beta_registrations(epoch)?;
        if !registrations.iter().any(|(_, _, account)| *account == to) {
            return Err(bad("recipient is not a registered BETA account"));
        }
        let snapshot = resolve_snapshot(
            &self.store,
            epoch,
            BetaMiniPolicy::open_beta_default(),
            &policy,
        )
        .map_err(exec_error)?;
        if snapshot.account_conflicts.contains(&to) || snapshot.account_conflicts.contains(&from) {
            return Err(bad("BETA account registration is conflicted"));
        }
        if snapshot.balance(&from) < amount {
            return Err(bad("insufficient BETA balance"));
        }
        let mut spendable: Vec<_> = snapshot
            .outputs
            .iter()
            .filter(|output| !output.spent && output.account == from)
            .collect();
        spendable.sort_by(|a, b| {
            b.amount
                .cmp(&a.amount)
                .then_with(|| {
                    a.output_ref
                        .source_id
                        .as_str()
                        .cmp(b.output_ref.source_id.as_str())
                })
                .then_with(|| a.output_ref.index.cmp(&b.output_ref.index))
        });
        let mut inputs: Vec<BetaOutputRef> = Vec::new();
        let mut gathered: u64 = 0;
        for output in spendable {
            if gathered >= amount {
                break;
            }
            if inputs.len() == MAX_TRANSFER_INPUTS {
                return Err(bad("amount needs too many inputs; send a smaller amount"));
            }
            gathered = gathered.saturating_add(output.amount);
            inputs.push(output.output_ref.clone());
        }
        if gathered < amount {
            return Err(bad("insufficient spendable BETA outputs"));
        }
        let change = gathered - amount;
        let mut outputs = vec![BetaTransferOutput {
            account: to,
            amount,
        }];
        if change > 0 {
            outputs.push(BetaTransferOutput {
                account: from,
                amount: change,
            });
        }
        let owner = slot.identity.root.did();
        let sequence = next_object_sequence(&self.store, Some(&owner))?;
        let object = build_transfer(
            &owner,
            &slot.identity.device,
            epoch,
            from,
            &inputs,
            &outputs,
            memo,
            now_ms(),
            sequence,
        )
        .map_err(exec_error)?;
        self.beta_publish_kels(&slot)?;
        self.beta_journal_and_commit(
            operation_id,
            Record {
                kind: KIND_TRANSFER,
                digest,
                account: *from.as_bytes(),
                amount,
                change,
                object: object.to_bytes(),
            },
        )
    }
}

#[cfg(test)]
pub(crate) fn derive_slot_for_test<V: IdentityVault>(
    core: &mut Core<V>,
    epoch: BetaEpochId,
    nonce: [u8; 32],
) -> AccountSlot {
    derive_slot(&core.vault.load_root().unwrap(), epoch, nonce).unwrap()
}
