//! `mini beta grant-policy|grant|grant-approve|grant-status`: decentralised,
//! threshold-approved Beta grants (`mini-beta-grants`). Test value only:
//! every amount is in micro-BETA units, has no conversion to anything, and a
//! grant authorization has no issuance effect until its threshold is met.
//!
//! `--at-ms T` (optional on every creating command) overrides the object
//! timestamp, for deterministic offline/replay use; default is the wall clock.

use std::path::Path;

use did_mini::Did;
use mini_beta::{create_grant_authorization, BetaAccountId, BetaEpochId, GrantClass};
use mini_beta_grants::{
    create_grant_approval, create_grant_policy, detect_authorizer_equivocations,
    validate_grant_acceptance, GrantAcceptanceError,
};

use crate::beta::{
    beta_err, extract_flag, extract_flag_multi, next, parse_hex32, parse_id, reject_remaining,
    required_flag, required_items,
};
use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};
use crate::sequence;
use crate::store::open_store;

const MAX_MEMBERS: usize = 64;
const MAX_BANDS: usize = 64;
const MAX_APPROVALS: usize = 128;

fn grants_err(error: GrantAcceptanceError) -> CliError {
    CliError::Object(format!("beta grants: {error}"))
}

fn parse_u64(value: &str, what: &str) -> Result<u64> {
    value
        .trim()
        .parse::<u64>()
        .map_err(|_| CliError::Usage(format!("{what} must be an unsigned integer")))
}

fn parse_u16(value: &str, what: &str) -> Result<u16> {
    value
        .trim()
        .parse::<u16>()
        .map_err(|_| CliError::Usage(format!("{what} must be an integer 0..=65535")))
}

fn now_or(at_ms: Option<String>) -> Result<u64> {
    match at_ms {
        Some(value) => parse_u64(&value, "--at-ms"),
        None => Ok(sequence::now_ms()),
    }
}

/// Route `mini beta <noun> <sub>`; returns the JSON `kind` and the outcome.
pub fn dispatch(
    home: &Path,
    store: &Path,
    noun: &str,
    sub: &str,
    args: Vec<String>,
) -> (&'static str, Result<CommandResult>) {
    match (noun, sub) {
        ("grant-policy", "create") => {
            ("beta.grant-policy-create", policy_create(home, store, args))
        }
        ("grant", "create") => ("beta.grant-create", grant_create(home, store, args)),
        ("grant-approve", _) => (
            "beta.grant-approve",
            // `grant-approve <policy-id> <grant-id>`: `sub` is the policy id.
            grant_approve(home, store, sub, args),
        ),
        ("grant-status", _) => (
            "beta.grant-status",
            // `grant-status <policy-id> <grant-id> ...`: `sub` is the policy id.
            grant_status(store, sub, args),
        ),
        _ => (
            "beta.unimplemented",
            Err(CliError::Usage(format!(
                "unknown `beta` command: {noun:?} {sub:?}"
            ))),
        ),
    }
}

fn policy_create(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<CommandResult> {
    const CTX: &str = "beta grant-policy create";
    let campaign_id = parse_id(&next(&mut args, CTX)?)?;
    let members = required_items(&mut args, "--member", CTX)?;
    let testing_threshold = parse_u16(
        &required_flag(&mut args, "--testing-threshold", CTX)?,
        "--testing-threshold",
    )?;
    let participation_threshold = parse_u16(
        &required_flag(&mut args, "--participation-threshold", CTX)?,
        "--participation-threshold",
    )?;
    let testing_amount = parse_u64(
        &required_flag(&mut args, "--testing-amount", CTX)?,
        "--testing-amount",
    )?;
    let bands = required_items(&mut args, "--reward-band", CTX)?;
    let valid_from = parse_u64(
        &required_flag(&mut args, "--valid-from-ms", CTX)?,
        "--valid-from-ms",
    )?;
    let valid_until = parse_u64(
        &required_flag(&mut args, "--valid-until-ms", CTX)?,
        "--valid-until-ms",
    )?;
    let at_ms = extract_flag(&mut args, "--at-ms");
    reject_remaining(args, CTX)?;

    if members.len() > MAX_MEMBERS {
        return Err(CliError::Usage(format!(
            "{CTX}: at most {MAX_MEMBERS} --member values"
        )));
    }
    if bands.len() > MAX_BANDS {
        return Err(CliError::Usage(format!(
            "{CTX}: at most {MAX_BANDS} --reward-band values"
        )));
    }
    let members = members
        .iter()
        .map(|m| Did::parse(m).map_err(|e| CliError::Usage(format!("{CTX}: bad --member: {e}"))))
        .collect::<Result<Vec<_>>>()?;
    let bands = bands
        .iter()
        .map(|b| parse_u64(b, "--reward-band"))
        .collect::<Result<Vec<_>>>()?;
    let timestamp = now_or(at_ms)?;

    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let object = create_grant_policy(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &campaign_id,
        &members,
        testing_threshold,
        participation_threshold,
        testing_amount,
        &bands,
        valid_from,
        valid_until,
        timestamp,
        sequence::next(home, store_path)?,
    )
    .map_err(grants_err)?;
    Ok(CommandResult::new(format!(
        "grant policy created: {}\n  members: {}; testing threshold {}; participation threshold {}\n  testing grant: {} micro-BETA-MINI (test value only)",
        object.id().as_str(),
        members.len(),
        testing_threshold,
        participation_threshold,
        testing_amount
    ))
    .field("policy_id", JsonValue::str(object.id().as_str()))
    .field("members", JsonValue::num(members.len() as u64))
    .field("testing_threshold", JsonValue::num(u64::from(testing_threshold)))
    .field(
        "participation_threshold",
        JsonValue::num(u64::from(participation_threshold)),
    ))
}

fn grant_create(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<CommandResult> {
    const CTX: &str = "beta grant create";
    let campaign_id = parse_id(&next(&mut args, CTX)?)?;
    let epoch = BetaEpochId::new(parse_hex32(
        &required_flag(&mut args, "--epoch", CTX)?,
        "--epoch",
    )?)
    .map_err(beta_err)?;
    let account = BetaAccountId::new(parse_hex32(
        &required_flag(&mut args, "--account", CTX)?,
        "--account",
    )?)
    .map_err(beta_err)?;
    let class = match required_flag(&mut args, "--class", CTX)?.as_str() {
        "testing" => GrantClass::Testing,
        "participation" => GrantClass::Participation,
        other => {
            return Err(CliError::Usage(format!(
                "{CTX}: unknown class {other:?}; use testing or participation"
            )))
        }
    };
    let contribution = extract_flag(&mut args, "--contribution")
        .map(|v| parse_id(&v))
        .transpose()?;
    let amount = parse_u64(&required_flag(&mut args, "--amount", CTX)?, "--amount")?;
    let memo = required_flag(&mut args, "--memo", CTX)?;
    let at_ms = extract_flag(&mut args, "--at-ms");
    reject_remaining(args, CTX)?;
    let timestamp = now_or(at_ms)?;

    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let object = create_grant_authorization(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &campaign_id,
        contribution.as_ref(),
        epoch,
        account,
        class,
        amount,
        &memo,
        timestamp,
        sequence::next(home, store_path)?,
    )
    .map_err(beta_err)?;
    Ok(CommandResult::new(format!(
        "grant authorization created: {}\n  {} grant of {} micro-BETA-MINI (test value only); no issuance effect until the policy threshold is met",
        object.id().as_str(),
        class.as_str(),
        amount
    ))
    .field("grant_id", JsonValue::str(object.id().as_str()))
    .field("class", JsonValue::str(class.as_str()))
    .field("amount_micro_beta", JsonValue::num(amount)))
}

fn grant_approve(
    home: &Path,
    store_path: &Path,
    policy: &str,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    const CTX: &str = "beta grant-approve";
    let policy_id = parse_id(policy)?;
    let grant_id = parse_id(&next(&mut args, CTX)?)?;
    let at_ms = extract_flag(&mut args, "--at-ms");
    reject_remaining(args, CTX)?;
    let timestamp = now_or(at_ms)?;

    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let object = create_grant_approval(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &policy_id,
        &grant_id,
        timestamp,
        sequence::next(home, store_path)?,
    )
    .map_err(grants_err)?;
    Ok(
        CommandResult::new(format!("grant approval created: {}", object.id().as_str()))
            .field("approval_id", JsonValue::str(object.id().as_str()))
            .field("policy_id", JsonValue::str(policy_id.as_str()))
            .field("grant_id", JsonValue::str(grant_id.as_str())),
    )
}

fn grant_status(store_path: &Path, policy: &str, mut args: Vec<String>) -> Result<CommandResult> {
    const CTX: &str = "beta grant-status";
    let policy_id = parse_id(policy)?;
    let grant_id = parse_id(&next(&mut args, CTX)?)?;
    let approvals = extract_flag_multi(&mut args, "--approval");
    reject_remaining(args, CTX)?;
    if approvals.len() > MAX_APPROVALS {
        return Err(CliError::Usage(format!(
            "{CTX}: at most {MAX_APPROVALS} --approval values"
        )));
    }
    let approval_ids = approvals
        .iter()
        .map(|a| parse_id(a))
        .collect::<Result<Vec<_>>>()?;

    let store = open_store(store_path)?;
    let (accepted, counted, threshold, reason) =
        match validate_grant_acceptance(&store, &policy_id, &grant_id, &approval_ids) {
            Ok(acc) => (
                true,
                u64::from(acc.distinct_approvers),
                u64::from(acc.required_approvals),
                String::new(),
            ),
            Err(GrantAcceptanceError::ThresholdNotMet { required, observed }) => (
                false,
                u64::from(observed),
                u64::from(required),
                format!("threshold not met: {observed} of {required} distinct approvers"),
            ),
            Err(other) => return Err(grants_err(other)),
        };
    let equivocations =
        detect_authorizer_equivocations(&store, &policy_id, &approval_ids).map_err(grants_err)?;

    let mut human = if accepted {
        format!("grant ACCEPTED: {counted} distinct approvals (threshold {threshold})")
    } else {
        format!("grant NOT accepted: {reason}")
    };
    human.push_str(&format!(
        "\n  authorizer equivocations detected: {}",
        equivocations.len()
    ));
    for e in &equivocations {
        human.push_str(&format!(
            "\n  equivocation: {} approved both {} and {}",
            e.authorizer.as_str(),
            e.first_grant_id.as_str(),
            e.second_grant_id.as_str()
        ));
    }
    human.push_str("\n  (test value only; an accepted grant is evidence, not a conversion)");

    Ok(CommandResult::new(human)
        .field("accepted", JsonValue::Bool(accepted))
        .field("approvals_counted", JsonValue::num(counted))
        .field("threshold", JsonValue::num(threshold))
        .field("equivocations", JsonValue::num(equivocations.len() as u64)))
}
