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
use mini_beta::{BetaAccountId, BetaEpochId, BetaMiniPolicy};
use mini_beta_exec::{
    create_account_registration, create_transfer, derive_account_id,
    parse_account_registration_object, resolve_snapshot, BetaExecError, BetaOutputRef,
    BetaTransferOutput, EpochIssuancePolicy, BETA_ACCOUNT_TYPE,
};
use mini_crypto::random_32;
use mini_objects::{ObjectId, ObjectType};

use crate::{now_ms, storage_error, Core, IdentityVault};

const MAX_TRANSFER_INPUTS: usize = 256;

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

    pub(crate) fn beta_register_account(&mut self, epoch: [u8; 32]) -> Result<Reply, ServiceError> {
        let epoch = epoch_id(epoch)?;
        self.ensure_signing_available()?;
        let nonce = random_32()
            .map_err(|_| ServiceError::new(ErrorCode::Internal, "entropy source unavailable"))?;
        let sequence = self.reserve_sequence()?;
        let human = self.signing_identity()?.0.clone();
        let account = derive_account_id(epoch, &human, nonce).map_err(exec_error)?;
        let object = create_account_registration(
            &mut self.store,
            &human,
            &self.identity.as_ref().expect("checked above").device,
            epoch,
            nonce,
            now_ms(),
            sequence,
        )
        .map_err(exec_error)?;
        Ok(Reply::BetaAccount {
            account: *account.as_bytes(),
            registration_id: object.id().as_str().to_string(),
        })
    }

    pub(crate) fn beta_balances(
        &self,
        epoch: [u8; 32],
        campaigns: &[String],
    ) -> Result<Reply, ServiceError> {
        let epoch = epoch_id(epoch)?;
        let policy = issuance_policy(epoch, campaigns)?;
        let human = self.human.as_ref().ok_or_else(|| {
            ServiceError::new(ErrorCode::RootMissing, "create a Mininet root first")
        })?;
        let mut accounts: Vec<[u8; 32]> = self
            .beta_registrations(epoch)?
            .into_iter()
            .filter(|(_, owner, _)| owner == human)
            .map(|(_, _, account)| *account.as_bytes())
            .collect();
        accounts.sort();
        if accounts.len() > usize::from(MAX_BETA_ACCOUNTS) {
            return Err(bad("too many BETA accounts for one view"));
        }
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

    pub(crate) fn beta_transfer(
        &mut self,
        epoch: [u8; 32],
        campaigns: &[String],
        from: [u8; 32],
        to: [u8; 32],
        amount: u64,
        memo: &str,
    ) -> Result<Reply, ServiceError> {
        let epoch = epoch_id(epoch)?;
        let policy = issuance_policy(epoch, campaigns)?;
        let from = account_id(from)?;
        let to = account_id(to)?;
        if amount == 0 {
            return Err(bad("amount must be positive"));
        }
        self.ensure_signing_available()?;
        let human = self.signing_identity()?.0.clone();
        let registrations = self.beta_registrations(epoch)?;
        if !registrations
            .iter()
            .any(|(_, owner, account)| owner == &human && *account == from)
        {
            return Err(bad("sender is not a BETA account owned by this identity"));
        }
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
        let sequence = self.reserve_sequence()?;
        let object = create_transfer(
            &mut self.store,
            &human,
            &self.identity.as_ref().expect("checked above").device,
            epoch,
            from,
            &inputs,
            &outputs,
            memo,
            now_ms(),
            sequence,
        )
        .map_err(exec_error)?;
        Ok(Reply::BetaTransfer {
            transfer_id: object.id().as_str().to_string(),
            amount_micro: amount,
            change_micro: change,
        })
    }
}
