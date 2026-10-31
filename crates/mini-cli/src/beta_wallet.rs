//! `mini beta account|wallet|transfer`: BETA-denominated test-value wallet over
//! the durable signed-object execution core (`mini-beta-exec`).
//!
//! Everything printed here is Beta test value in an explicit, resettable epoch.
//! Balances are provisional derived state (late replicated conflict evidence can
//! change them), carry no production-MINI conversion right, and have no
//! governance, review, release or personhood weight.

use std::path::Path;

use did_mini::Did;
use mini_beta::{BetaAccountId, BetaEpochId, BetaMiniPolicy, MICRO_BETA_MINI_PER_BETA_MINI};
use mini_beta_exec::{
    create_account_registration, create_transfer, parse_account_registration_object,
    resolve_snapshot, BetaOutputRef, BetaSnapshot, BetaTransferOutput, EpochIssuancePolicy,
    BETA_ACCOUNT_TYPE, MAX_TRANSFER_INPUTS,
};
use mini_crypto::random_32;
use mini_objects::{ObjectId, ObjectType};
use mini_store::{Backend, Store};

use crate::beta::{
    extract_flag, extract_flag_multi, hex, parse_hex32, parse_id, reject_remaining, required_flag,
};
use crate::beta_identity;
use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};
use crate::store::open_store;

const MAX_AUTHORIZED_CAMPAIGNS: usize = 64;
const LABEL: &str = "BETA test value (not production MINI; no conversion; provisional)";

/// Route `mini beta <noun> <sub>`; returns the JSON `kind` and the outcome.
pub fn dispatch(
    home: &Path,
    store: &Path,
    noun: &str,
    sub: &str,
    args: Vec<String>,
) -> (&'static str, Result<CommandResult>) {
    match (noun, sub) {
        ("account", "new") => ("beta.account-new", account_new(home, store, args)),
        ("account", "list") => ("beta.account-list", account_list(home, store, args)),
        ("wallet", "balance") => ("beta.wallet-balance", wallet_balance(home, store, args)),
        ("transfer", "send") => ("beta.transfer-send", transfer_send(home, store, args)),
        _ => (
            "beta.unknown",
            Err(CliError::Usage(format!(
                "unknown `beta` command: {noun:?} {sub:?}; use account new|list, wallet balance, transfer send"
            ))),
        ),
    }
}

fn exec_err(error: mini_beta_exec::BetaExecError) -> CliError {
    CliError::Object(format!("beta wallet: {error}"))
}

fn beta_core_err(error: mini_beta::BetaError) -> CliError {
    CliError::Object(format!("beta: {error}"))
}

fn store_err(error: impl std::fmt::Display) -> CliError {
    CliError::Store(error.to_string())
}

/// Format integer micro-units without floating point.
fn format_beta(micro: u64) -> String {
    format!(
        "{}.{:06} BETA",
        micro / MICRO_BETA_MINI_PER_BETA_MINI,
        micro % MICRO_BETA_MINI_PER_BETA_MINI
    )
}

fn epoch_arg(args: &mut Vec<String>, context: &str) -> Result<BetaEpochId> {
    let bytes = parse_hex32(&required_flag(args, "--epoch", context)?, "--epoch")?;
    BetaEpochId::new(bytes).map_err(beta_core_err)
}

fn account_arg(args: &mut Vec<String>, flag: &str, context: &str) -> Result<BetaAccountId> {
    let bytes = parse_hex32(&required_flag(args, flag, context)?, flag)?;
    BetaAccountId::new(bytes).map_err(beta_core_err)
}

/// The issuance trust anchor is always caller-selected and explicit: an empty
/// set authorizes no issuance, and the store is never scanned for campaigns.
fn issuance_policy(
    epoch: BetaEpochId,
    args: &mut Vec<String>,
    context: &str,
) -> Result<EpochIssuancePolicy> {
    let campaigns = extract_flag_multi(args, "--campaign");
    if campaigns.is_empty() {
        return Err(CliError::Usage(format!(
            "{context}: --campaign <id> must be supplied at least once (the authorized campaigns that may issue BETA in this epoch)"
        )));
    }
    if campaigns.len() > MAX_AUTHORIZED_CAMPAIGNS {
        return Err(CliError::Usage(format!(
            "{context}: at most {MAX_AUTHORIZED_CAMPAIGNS} --campaign values"
        )));
    }
    let ids = campaigns
        .iter()
        .map(|value| parse_id(value))
        .collect::<Result<Vec<ObjectId>>>()?;
    Ok(EpochIssuancePolicy::new(epoch, &ids))
}

/// This home's registered accounts in `epoch`, as `(registration id, account)`:
/// the accounts whose per-account identity this home can re-derive.
fn owned_accounts<B: Backend>(
    home: &Path,
    store: &Store<B>,
    epoch: BetaEpochId,
) -> Result<Vec<(ObjectId, BetaAccountId)>> {
    let mine: std::collections::HashMap<[u8; 32], Did> = beta_identity::owned(home, epoch)?
        .into_iter()
        .map(|identity| (identity.nonce, identity.owner()))
        .collect();
    let mut owned = Vec::new();
    if mine.is_empty() {
        return Ok(owned);
    }
    for id in store
        .by_type(&ObjectType::Custom(BETA_ACCOUNT_TYPE.to_string()))
        .map_err(store_err)?
    {
        let object = store.get(&id).map_err(store_err)?;
        let Ok(registration) = parse_account_registration_object(&object) else {
            continue;
        };
        if registration.epoch == epoch && mine.get(&registration.nonce) == Some(&registration.owner)
        {
            owned.push((id, registration.account));
        }
    }
    owned.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    Ok(owned)
}

fn account_is_registered<B: Backend>(
    store: &Store<B>,
    account: &BetaAccountId,
    epoch: BetaEpochId,
) -> Result<bool> {
    for id in store
        .by_type(&ObjectType::Custom(BETA_ACCOUNT_TYPE.to_string()))
        .map_err(store_err)?
    {
        let object = store.get(&id).map_err(store_err)?;
        if let Ok(registration) = parse_account_registration_object(&object) {
            if registration.epoch == epoch && &registration.account == account {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn snapshot_fields(result: CommandResult, snapshot: &BetaSnapshot) -> CommandResult {
    result
        .field("denomination", JsonValue::str("BETA"))
        .field("label", JsonValue::str(LABEL))
        .field("provisional", JsonValue::Bool(snapshot.is_provisional()))
        .field(
            "issuance_conflict",
            JsonValue::Bool(snapshot.issuance_conflict),
        )
        .field(
            "account_conflicts",
            JsonValue::num(snapshot.account_conflicts.len() as u64),
        )
        .field(
            "transfer_conflicts",
            JsonValue::num(snapshot.transfer_conflicts.len() as u64),
        )
        .field(
            "invalid_transfers",
            JsonValue::num(snapshot.invalid_transfers.len() as u64),
        )
        .field("total_issued_micro", JsonValue::num(snapshot.total_issued))
        .field("snapshot_digest", JsonValue::str(hex(&snapshot.digest)))
}

fn warnings(snapshot: &BetaSnapshot) -> String {
    let mut notes = String::new();
    if snapshot.issuance_conflict {
        notes.push_str("\nWARNING: conflicting issuance evidence; balances may be understated.");
    }
    if !snapshot.account_conflicts.is_empty() {
        notes.push_str(&format!(
            "\nWARNING: {} account registration conflict(s) excluded.",
            snapshot.account_conflicts.len()
        ));
    }
    if !snapshot.transfer_conflicts.is_empty() || !snapshot.invalid_transfers.is_empty() {
        notes.push_str(&format!(
            "\nWARNING: {} conflicting and {} invalid transfer(s) excluded.",
            snapshot.transfer_conflicts.len(),
            snapshot.invalid_transfers.len()
        ));
    }
    notes
}

/// `beta account new --epoch <64hex>`: register a fresh opaque account.
pub fn account_new(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<CommandResult> {
    let epoch = epoch_arg(&mut args, "beta account new")?;
    reject_remaining(args, "beta account new")?;
    // Ensure the primary identity exists (it holds the seed everything derives from).
    crate::identity::load_or_init(home)?;
    let nonce = random_32().map_err(|e| CliError::Identity(format!("entropy: {e}")))?;
    let account_identity = beta_identity::derive(home, epoch, nonce)?;
    let account = account_identity.account;
    let mut store = open_store(store_path)?;
    let _guard = beta_identity::lock(home)?;
    beta_identity::record(home, epoch, &nonce)?;
    beta_identity::publish_kels(&account_identity, &mut store)?;
    let sequence = beta_identity::next_sequence(&store, &account_identity.owner())?;
    let object = create_account_registration(
        &mut store,
        &account_identity.owner(),
        &account_identity.device,
        epoch,
        nonce,
        crate::sequence::now_ms(),
        sequence,
    )
    .map_err(exec_err)?;
    Ok(CommandResult::new(format!(
        "{LABEL}\nregistered a fresh BETA account in epoch {}\naccount: {}\nregistration: {}\nThis account holds nothing until a threshold-accepted grant or a transfer credits it.",
        &hex(epoch.as_bytes())[..16],
        hex(account.as_bytes()),
        object.id().as_str()
    ))
    .field("denomination", JsonValue::str("BETA"))
    .field("epoch", JsonValue::str(hex(epoch.as_bytes())))
    .field("account", JsonValue::str(hex(account.as_bytes())))
    .field("registration_id", JsonValue::str(object.id().as_str())))
}

/// `beta account list --epoch <64hex>`: accounts registered by this identity.
pub fn account_list(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let epoch = epoch_arg(&mut args, "beta account list")?;
    reject_remaining(args, "beta account list")?;
    crate::identity::load_or_init(home)?;
    let store = open_store(store_path)?;
    let owned = owned_accounts(home, &store, epoch)?;
    let human = if owned.is_empty() {
        format!("{LABEL}\nno BETA accounts registered by this identity in this epoch")
    } else {
        let lines = owned
            .iter()
            .map(|(_, account)| hex(account.as_bytes()))
            .collect::<Vec<_>>()
            .join("\n");
        format!("{LABEL}\n{lines}")
    };
    Ok(CommandResult::new(human)
        .field("denomination", JsonValue::str("BETA"))
        .field("epoch", JsonValue::str(hex(epoch.as_bytes())))
        .field(
            "accounts",
            JsonValue::Array(
                owned
                    .iter()
                    .map(|(registration, account)| {
                        JsonValue::Object(vec![
                            (
                                "account".to_string(),
                                JsonValue::str(hex(account.as_bytes())),
                            ),
                            (
                                "registration_id".to_string(),
                                JsonValue::str(registration.as_str()),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ))
}

/// `beta wallet balance --epoch <64hex> --campaign <id>... [--account <64hex>]`
pub fn wallet_balance(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let context = "beta wallet balance";
    let epoch = epoch_arg(&mut args, context)?;
    let policy = issuance_policy(epoch, &mut args, context)?;
    let single = extract_flag(&mut args, "--account")
        .map(|value| {
            parse_hex32(&value, "--account")
                .and_then(|b| BetaAccountId::new(b).map_err(beta_core_err))
        })
        .transpose()?;
    reject_remaining(args, context)?;
    crate::identity::load_or_init(home)?;
    let store = open_store(store_path)?;
    let snapshot = resolve_snapshot(&store, epoch, BetaMiniPolicy::open_beta_default(), &policy)
        .map_err(exec_err)?;
    let accounts: Vec<BetaAccountId> = match single {
        Some(account) => vec![account],
        None => owned_accounts(home, &store, epoch)?
            .into_iter()
            .map(|(_, account)| account)
            .collect(),
    };
    let mut lines = vec![format!(
        "{LABEL}\nepoch {} (resets to zero; nothing carries over)",
        &hex(epoch.as_bytes())[..16]
    )];
    if accounts.is_empty() {
        lines.push("no BETA accounts registered by this identity in this epoch".to_string());
    }
    for account in &accounts {
        lines.push(format!(
            "{}  {}",
            hex(account.as_bytes()),
            format_beta(snapshot.balance(account))
        ));
    }
    let human = format!("{}{}", lines.join("\n"), warnings(&snapshot));
    let result = CommandResult::new(human)
        .field("epoch", JsonValue::str(hex(epoch.as_bytes())))
        .field(
            "balances",
            JsonValue::Array(
                accounts
                    .iter()
                    .map(|account| {
                        JsonValue::Object(vec![
                            (
                                "account".to_string(),
                                JsonValue::str(hex(account.as_bytes())),
                            ),
                            (
                                "amount_micro".to_string(),
                                JsonValue::num(snapshot.balance(account)),
                            ),
                        ])
                    })
                    .collect(),
            ),
        );
    Ok(snapshot_fields(result, &snapshot))
}

/// `beta transfer send --epoch <64hex> --campaign <id>... --from <64hex> --to <64hex> --amount <micro> --memo <text>`
pub fn transfer_send(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let context = "beta transfer send";
    let epoch = epoch_arg(&mut args, context)?;
    let policy = issuance_policy(epoch, &mut args, context)?;
    let from = account_arg(&mut args, "--from", context)?;
    let to = account_arg(&mut args, "--to", context)?;
    let amount: u64 = required_flag(&mut args, "--amount", context)?
        .parse()
        .map_err(|_| CliError::Usage(format!("{context}: --amount must be integer micro-BETA")))?;
    let memo = required_flag(&mut args, "--memo", context)?;
    reject_remaining(args, context)?;
    if amount == 0 {
        return Err(CliError::Usage(format!(
            "{context}: --amount must be positive"
        )));
    }
    crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let sender = beta_identity::owned(home, epoch)?
        .into_iter()
        .find(|identity| identity.account == from)
        .ok_or_else(|| {
            CliError::Usage(format!(
                "{context}: --from is not a BETA account registered by this identity in this epoch"
            ))
        })?;
    if !account_is_registered(&store, &to, epoch)? {
        return Err(CliError::Usage(format!(
            "{context}: --to is not a registered BETA account in this epoch (the recipient must register first and share the account id)"
        )));
    }
    let snapshot = resolve_snapshot(&store, epoch, BetaMiniPolicy::open_beta_default(), &policy)
        .map_err(exec_err)?;
    let available = snapshot.balance(&from);
    if available < amount {
        return Err(CliError::Usage(format!(
            "{context}: insufficient BETA balance: have {}, need {}",
            format_beta(available),
            format_beta(amount)
        )));
    }
    // Greedy selection over unspent outputs, largest first (fewest inputs).
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
            return Err(CliError::Usage(format!(
                "{context}: this amount needs more than {MAX_TRANSFER_INPUTS} inputs; send a smaller amount first"
            )));
        }
        gathered = gathered.saturating_add(output.amount);
        inputs.push(output.output_ref.clone());
    }
    if gathered < amount {
        return Err(CliError::Usage(format!(
            "{context}: insufficient spendable BETA outputs (some value may be in conflict)"
        )));
    }
    let mut outputs = vec![BetaTransferOutput {
        account: to,
        amount,
    }];
    let change = gathered - amount;
    if change > 0 {
        outputs.push(BetaTransferOutput {
            account: from,
            amount: change,
        });
    }
    let _guard = beta_identity::lock(home)?;
    beta_identity::publish_kels(&sender, &mut store)?;
    let sequence = beta_identity::next_sequence(&store, &sender.owner())?;
    let object = create_transfer(
        &mut store,
        &sender.owner(),
        &sender.device,
        epoch,
        from,
        &inputs,
        &outputs,
        &memo,
        crate::sequence::now_ms(),
        sequence,
    )
    .map_err(exec_err)?;
    Ok(CommandResult::new(format!(
        "{LABEL}\nsent {} from {} to {} (change {})\ntransfer: {}\nA provisional transfer: late conflicting evidence can still invalidate it.",
        format_beta(amount),
        &hex(from.as_bytes())[..16],
        &hex(to.as_bytes())[..16],
        format_beta(change),
        object.id().as_str()
    ))
    .field("denomination", JsonValue::str("BETA"))
    .field("transfer_id", JsonValue::str(object.id().as_str()))
    .field("amount_micro", JsonValue::num(amount))
    .field("change_micro", JsonValue::num(change))
    .field("inputs", JsonValue::num(inputs.len() as u64))
    .field("provisional", JsonValue::Bool(true)))
}
