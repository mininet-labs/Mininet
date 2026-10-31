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
    build_account_registration, build_transfer, derive_account_id,
    parse_account_registration_object, resolve_snapshot, BetaExecError, BetaOutputRef,
    BetaTransferOutput, EpochIssuancePolicy, BETA_ACCOUNT_TYPE, MAX_TRANSFER_INPUTS,
};
use mini_crypto::KdfSuite;
use mini_objects::{ObjectId, ObjectType};
use mini_windows_vault::SeedPair;

use crate::beta_journal::{self, Record, KIND_REGISTER, KIND_TRANSFER};
use crate::{
    identity_from_seeds, next_object_sequence, now_ms, storage_error, validate_operation, Core,
    CoreIdentity, IdentityVault,
};

/// HKDF domain separating per-account BETA signing identities from every other
/// use of the root seed.
const ACCOUNT_IDENTITY_INFO: &[u8] = b"mininet/beta/account-identity/v1";

/// One BETA account's isolated signing identity. It is derived, never stored,
/// from the unlocked root seed, so the author DID on public BETA objects is
/// unrelated to the primary Mininet DID and unlinkable without the seed.
pub(crate) struct AccountSlot {
    pub(crate) identity: CoreIdentity,
    pub(crate) nonce: [u8; 32],
    pub(crate) account: BetaAccountId,
}

fn derive_slot(
    root: &SeedPair,
    epoch: BetaEpochId,
    index: u16,
) -> Result<AccountSlot, ServiceError> {
    let mut info = Vec::with_capacity(ACCOUNT_IDENTITY_INFO.len() + 34);
    info.extend_from_slice(ACCOUNT_IDENTITY_INFO);
    info.extend_from_slice(epoch.as_bytes());
    info.extend_from_slice(&index.to_be_bytes());
    let mut okm = KdfSuite::HkdfSha256
        .derive_bytes(None, &root.current, &info, 160)
        .map_err(|_| ServiceError::new(ErrorCode::Identity, "BETA identity derivation failed"))?;
    let seed = |at: usize| -> [u8; 32] { okm[at..at + 32].try_into().expect("160 bytes") };
    let root_seeds = SeedPair {
        current: seed(0),
        next: seed(32),
    };
    let device_seeds = SeedPair {
        current: seed(64),
        next: seed(96),
    };
    let nonce = seed(128);
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

    /// This identity's registered accounts in `epoch`, in index order, each with
    /// its derived signing identity, plus the next free slot (if any remain).
    pub(crate) fn beta_own_slots(
        &mut self,
        epoch: BetaEpochId,
    ) -> Result<(Vec<AccountSlot>, Option<AccountSlot>), ServiceError> {
        let root = self.vault.load_root()?;
        let registered: std::collections::HashMap<[u8; 32], did_mini::Did> = self
            .beta_registrations(epoch)?
            .into_iter()
            .map(|(_, owner, account)| (*account.as_bytes(), owner))
            .collect();
        let mut owned = Vec::new();
        for index in 0..MAX_BETA_ACCOUNTS {
            let slot = derive_slot(&root, epoch, index)?;
            match registered.get(slot.account.as_bytes()) {
                Some(owner) if *owner == slot.identity.root.did() => owned.push(slot),
                _ => return Ok((owned, Some(slot))),
            }
        }
        Ok((owned, None))
    }

    pub(crate) fn beta_reply(record: &Record) -> Result<Reply, ServiceError> {
        let object = mini_objects::Object::from_bytes(&record.object).map_err(storage_error)?;
        let id = object.id().as_str().to_string();
        Ok(if record.kind == KIND_REGISTER {
            Reply::BetaAccount {
                account: record.account,
                registration_id: id,
            }
        } else {
            Reply::BetaTransfer {
                transfer_id: id,
                amount_micro: record.amount,
                change_micro: record.change,
            }
        })
    }

    /// Replay an existing operation (finishing a half-committed one) or `None`.
    fn beta_replay(
        &mut self,
        operation_id: &str,
        kind: u8,
        digest: [u8; 32],
    ) -> Result<Option<Reply>, ServiceError> {
        let Some(record) = beta_journal::read(&self.beta_ops, operation_id)? else {
            return Ok(None);
        };
        if record.kind != kind || record.digest != digest {
            return Err(bad("operation id was already used with different content"));
        }
        let record =
            beta_journal::complete(&self.beta_ops, &mut self.store, operation_id, &record)?;
        Self::beta_reply(&record).map(Some)
    }

    fn beta_journal_and_commit(
        &mut self,
        operation_id: &str,
        record: Record,
    ) -> Result<Reply, ServiceError> {
        beta_journal::write(&self.beta_ops, operation_id, &record)?;
        let record =
            beta_journal::complete(&self.beta_ops, &mut self.store, operation_id, &record)?;
        Self::beta_reply(&record)
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
        let (_, next) = self.beta_own_slots(epoch)?;
        let slot = next.ok_or_else(|| bad("BETA account limit reached for this epoch"))?;
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
        self.beta_journal_and_commit(
            operation_id,
            Record {
                kind: KIND_REGISTER,
                complete: false,
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
        let (owned, _) = self.beta_own_slots(epoch)?;
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
        let (owned, _) = self.beta_own_slots(epoch)?;
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
        self.beta_journal_and_commit(
            operation_id,
            Record {
                kind: KIND_TRANSFER,
                complete: false,
                digest,
                account: *from.as_bytes(),
                amount,
                change,
                object: object.to_bytes(),
            },
        )
    }
}
