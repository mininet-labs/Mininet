//! Forge-native Open Beta workflow surfaces.
//!
//! Participant-authored findings and work claims deliberately do **not** use
//! the persistent identity stored under `MININET_HOME`.  Each such artifact is
//! signed by a fresh root + delegated device pair, its KEL carriers are stored
//! beside the artifact for ordinary `mini sync`, and the secret controllers are
//! dropped at the end of the command.  This gives artifact-scoped author DIDs,
//! not a contributor directory.
//!
//! Operational records (disposition, task creation, contribution acceptance)
//! still use the local bootstrap/operator identity.  That is temporary
//! workflow authority, not a contributor identity and not a governance weight.

use std::path::Path;

use did_mini::{Capabilities, Controller};
use mini_beta::{
    create_contribution_receipt, create_finding, create_finding_disposition, read_campaign,
    read_contribution_receipt, read_finding, read_finding_disposition, BetaAccountId, ClaimTag,
    ContributionKind, EvidenceClass, FindingSeverity, FindingState, SubmissionTag,
    BETA_CAMPAIGN_TYPE, BETA_CONTRIBUTION_TYPE, BETA_FINDING_DISPOSITION_TYPE, BETA_FINDING_TYPE,
};
use mini_crypto::{random_32, SigningKey};
use mini_forge::{create_task_brief, create_work_claim};
use mini_objects::{ObjectId, ObjectType};
use mini_store::{Backend, Store};
use mini_sync::kel_carrier;

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};
use crate::project as project_alias;
use crate::sequence;
use crate::store::open_store;

const CLAIM_DOMAIN: &[u8] = b"mininet/beta/claim-secret/v1";

fn extract_flag(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let pos = args.iter().position(|value| value == flag)?;
    if pos + 1 >= args.len() {
        return None;
    }
    args.remove(pos);
    Some(args.remove(pos))
}

fn extract_flag_multi(args: &mut Vec<String>, flag: &str) -> Vec<String> {
    let mut values = Vec::new();
    while let Some(value) = extract_flag(args, flag) {
        values.push(value);
    }
    values
}

fn extract_bool_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(pos) = args.iter().position(|value| value == flag) {
        args.remove(pos);
        true
    } else {
        false
    }
}

fn required_flag(args: &mut Vec<String>, flag: &str, context: &str) -> Result<String> {
    extract_flag(args, flag).ok_or_else(|| CliError::Usage(format!("{context}: {flag} required")))
}

fn required_items(args: &mut Vec<String>, flag: &str, context: &str) -> Result<Vec<String>> {
    let values = extract_flag_multi(args, flag);
    if values.is_empty() {
        return Err(CliError::Usage(format!(
            "{context}: {flag} must be supplied at least once"
        )));
    }
    Ok(values)
}

fn reject_remaining(args: Vec<String>, context: &str) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(CliError::Usage(format!(
            "{context}: unexpected arguments: {}",
            args.join(" ")
        )))
    }
}

fn next(args: &mut Vec<String>, context: &str) -> Result<String> {
    if args.is_empty() {
        Err(CliError::Usage(format!("{context}: missing argument")))
    } else {
        Ok(args.remove(0))
    }
}

fn parse_id(value: &str) -> Result<ObjectId> {
    ObjectId::parse(value).map_err(|error| CliError::Object(error.to_string()))
}

fn beta_err(error: mini_beta::BetaError) -> CliError {
    CliError::Object(format!("beta: {error}"))
}

fn forge_err(error: mini_forge::ForgeError) -> CliError {
    CliError::Forge(error.to_string())
}

fn crypto_err(error: mini_crypto::CryptoError) -> CliError {
    CliError::Identity(format!("entropy: {error}"))
}

fn parse_evidence_class(value: &str) -> Result<EvidenceClass> {
    match value {
        "physical-device" => Ok(EvidenceClass::PhysicalDevice),
        "emulator" => Ok(EvidenceClass::Emulator),
        "rust-toolchain" => Ok(EvidenceClass::RustToolchain),
        "research" => Ok(EvidenceClass::Research),
        "external-review" => Ok(EvidenceClass::ExternalReview),
        "accessibility" => Ok(EvidenceClass::Accessibility),
        "other" => Ok(EvidenceClass::Other),
        _ => Err(CliError::Usage(format!(
            "unknown evidence class {value:?}; use physical-device, emulator, rust-toolchain, research, external-review, accessibility, or other"
        ))),
    }
}

fn parse_severity(value: &str) -> Result<FindingSeverity> {
    match value {
        "observation" => Ok(FindingSeverity::Observation),
        "low" => Ok(FindingSeverity::Low),
        "medium" => Ok(FindingSeverity::Medium),
        "high" => Ok(FindingSeverity::High),
        "critical" => Ok(FindingSeverity::Critical),
        _ => Err(CliError::Usage(format!(
            "unknown severity {value:?}; use observation, low, medium, high, or critical"
        ))),
    }
}

fn parse_finding_state(value: &str) -> Result<FindingState> {
    match value {
        "accepted" => Ok(FindingState::Accepted),
        "duplicate" => Ok(FindingState::Duplicate),
        "needs-information" => Ok(FindingState::NeedsInformation),
        "fixed" => Ok(FindingState::Fixed),
        "cannot-reproduce" => Ok(FindingState::CannotReproduce),
        "rejected" => Ok(FindingState::Rejected),
        _ => Err(CliError::Usage(format!(
            "unknown finding state {value:?}; use accepted, duplicate, needs-information, fixed, cannot-reproduce, or rejected"
        ))),
    }
}

fn parse_contribution_kind(value: &str) -> Result<ContributionKind> {
    match value {
        "testing" => Ok(ContributionKind::Testing),
        "reproduction" => Ok(ContributionKind::Reproduction),
        "hardware" => Ok(ContributionKind::Hardware),
        "accessibility" => Ok(ContributionKind::Accessibility),
        "documentation" => Ok(ContributionKind::Documentation),
        "security" => Ok(ContributionKind::Security),
        "research" => Ok(ContributionKind::Research),
        "code" => Ok(ContributionKind::Code),
        "review" => Ok(ContributionKind::Review),
        "reproducibility" => Ok(ContributionKind::Reproducibility),
        "operations" => Ok(ContributionKind::Operations),
        _ => Err(CliError::Usage(format!(
            "unknown contribution kind {value:?}"
        ))),
    }
}

/// Create a fresh artifact-scoped root + POST-only delegated device and place
/// their KEL carriers in the same store.  The caller owns the controllers only
/// until it has signed its one artifact; dropping them destroys local signing
/// continuity by design.
fn ephemeral_artifact_signer<B: Backend>(store: &mut Store<B>) -> Result<(Controller, Controller)> {
    let mut human = Controller::incept_single().map_err(|e| CliError::Identity(e.to_string()))?;
    let current = SigningKey::generate().map_err(crypto_err)?;
    let next = SigningKey::generate().map_err(crypto_err)?;
    let device = Controller::incept_device(&human.did(), vec![current], 1, vec![next], 1)
        .map_err(|e| CliError::Identity(e.to_string()))?;
    human
        .delegate_device(&device.did(), Capabilities::POST)
        .map_err(|e| CliError::Identity(e.to_string()))?;

    // Device carrier can self-authenticate its embedded KEL.  The root carrier
    // is signed by the delegated device and becomes fully provenance-verifiable
    // once both carriers are present at the receiving ingest boundary.
    let device_carrier = kel_carrier(&device.kel(), &human.did(), &device)
        .map_err(|e| CliError::Object(e.to_string()))?;
    let root_carrier = kel_carrier(&human.kel(), &human.did(), &device)
        .map_err(|e| CliError::Object(e.to_string()))?;
    store
        .insert(&device_carrier)
        .map_err(|e| CliError::Store(e.to_string()))?;
    store
        .insert(&root_carrier)
        .map_err(|e| CliError::Store(e.to_string()))?;
    Ok((human, device))
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            use std::fmt::Write;
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Generate a secret and publishable commitment.  Only the commitment goes in
/// the contribution receipt.  A future private-claim path can require the
/// preimage; publishing the preimage would destroy bearer privacy.
fn fresh_claim_secret() -> Result<([u8; 32], ClaimTag)> {
    let secret = random_32().map_err(crypto_err)?;
    let mut material = Vec::with_capacity(CLAIM_DOMAIN.len() + secret.len());
    material.extend_from_slice(CLAIM_DOMAIN);
    material.extend_from_slice(&secret);
    let digest = blake3::hash(&material);
    let mut commitment = [0u8; 32];
    commitment.copy_from_slice(digest.as_bytes());
    let tag = ClaimTag::new(commitment).map_err(beta_err)?;
    Ok((secret, tag))
}

pub fn campaign_list(_home: &Path, store_path: &Path, args: Vec<String>) -> Result<CommandResult> {
    reject_remaining(args, "beta campaign list")?;
    let store = open_store(store_path)?;
    let ids = store
        .by_type(&ObjectType::Custom(BETA_CAMPAIGN_TYPE.to_string()))
        .map_err(|e| CliError::Store(e.to_string()))?;
    let mut campaigns = Vec::new();
    for id in ids {
        let campaign = read_campaign(&store, &id).map_err(beta_err)?;
        campaigns.push(campaign);
    }
    campaigns.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    let human = if campaigns.is_empty() {
        "no beta campaigns".to_string()
    } else {
        campaigns
            .iter()
            .map(|campaign| {
                format!(
                    "{} [{}..{}] {}",
                    campaign.id.as_str(),
                    campaign.starts_ms,
                    campaign.ends_ms,
                    campaign.title
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(CommandResult::new(human).field(
        "campaigns",
        JsonValue::Array(campaigns.iter().map(campaign_json).collect()),
    ))
}

pub fn campaign_show(
    _home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let id = parse_id(&next(&mut args, "beta campaign show")?)?;
    reject_remaining(args, "beta campaign show")?;
    let store = open_store(store_path)?;
    let campaign = read_campaign(&store, &id).map_err(beta_err)?;
    let human = format!(
        "campaign {}\ntitle: {}\ntarget: {}\nepoch: {}\nwindow: {}..{}\nroutes: {}\ninstructions: {}",
        campaign.id.as_str(),
        campaign.title,
        campaign.target_id.as_str(),
        hex(campaign.epoch.as_bytes()),
        campaign.starts_ms,
        campaign.ends_ms,
        campaign.routes.join(", "),
        campaign.instructions
    );
    Ok(CommandResult::new(human).field("campaign", campaign_json(&campaign)))
}

#[allow(clippy::too_many_lines)]
pub fn finding_submit(
    _home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    // The privacy gate is checked before any other argument is parsed or
    // validated, deliberately. A caller who forgets --privacy-redacted must
    // see the privacy warning first, every time -- not whichever other
    // validation error happens to trigger first depending on what else is
    // malformed in the command line. Surfacing the redaction requirement
    // only when the rest of the input is otherwise well-formed would make
    // it easy to miss on exactly the submissions most likely to be
    // hurried and least likely to have been re-read for leftover secrets
    // or device identifiers.
    let redacted = extract_bool_flag(&mut args, "--privacy-redacted");
    if !redacted {
        return Err(CliError::Usage(
            "beta finding submit requires --privacy-redacted after removing secrets, stable device identifiers, private location/content, and unnecessary personal data"
                .to_string(),
        ));
    }
    let campaign_id = parse_id(&next(&mut args, "beta finding submit")?)?;
    let evidence_class = parse_evidence_class(&required_flag(
        &mut args,
        "--evidence-class",
        "beta finding submit",
    )?)?;
    let severity = parse_severity(&required_flag(
        &mut args,
        "--severity",
        "beta finding submit",
    )?)?;
    let component = required_flag(&mut args, "--component", "beta finding submit")?;
    let summary = required_flag(&mut args, "--summary", "beta finding submit")?;
    let environment = required_flag(&mut args, "--environment", "beta finding submit")?;
    let steps = required_flag(&mut args, "--steps", "beta finding submit")?;
    let expected = required_flag(&mut args, "--expected", "beta finding submit")?;
    let observed = required_flag(&mut args, "--observed", "beta finding submit")?;
    let evidence = required_items(&mut args, "--evidence", "beta finding submit")?;
    let limitations = required_flag(&mut args, "--limitations", "beta finding submit")?;
    reject_remaining(args, "beta finding submit")?;

    let tag = SubmissionTag::new(random_32().map_err(crypto_err)?).map_err(beta_err)?;
    let mut store = open_store(store_path)?;
    let (human, device) = ephemeral_artifact_signer(&mut store)?;
    let object = create_finding(
        &mut store,
        &human.did(),
        &device,
        &campaign_id,
        tag,
        evidence_class,
        severity,
        &component,
        &summary,
        &environment,
        &steps,
        &expected,
        &observed,
        &evidence,
        &limitations,
        true,
        sequence::now_ms(),
        1,
    )
    .map_err(beta_err)?;
    Ok(CommandResult::new(format!(
        "anonymous beta finding recorded: {}\nartifact submission tag: {}\nThe signing identity was fresh for this artifact and is not saved.",
        object.id().as_str(),
        hex(tag.as_bytes())
    ))
    .field("finding_id", JsonValue::str(object.id().as_str()))
    .field("submission_tag", JsonValue::str(hex(tag.as_bytes())))
    .field("artifact_scoped_author", JsonValue::Bool(true)))
}

pub fn finding_list(
    _home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let campaign_filter = extract_flag(&mut args, "--campaign")
        .map(|value| parse_id(&value))
        .transpose()?;
    reject_remaining(args, "beta finding list")?;
    let store = open_store(store_path)?;
    let ids = store
        .by_type(&ObjectType::Custom(BETA_FINDING_TYPE.to_string()))
        .map_err(|e| CliError::Store(e.to_string()))?;
    let mut findings = Vec::new();
    for id in ids {
        let finding = read_finding(&store, &id).map_err(beta_err)?;
        if campaign_filter
            .as_ref()
            .map(|campaign| &finding.campaign_id == campaign)
            .unwrap_or(true)
        {
            findings.push(finding);
        }
    }
    findings.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    let human = if findings.is_empty() {
        "no beta findings".to_string()
    } else {
        findings
            .iter()
            .map(|finding| {
                format!(
                    "{} [{}] {}: {}",
                    finding.id.as_str(),
                    finding.severity.as_str(),
                    finding.component,
                    finding.summary
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(CommandResult::new(human).field(
        "findings",
        JsonValue::Array(findings.iter().map(finding_json).collect()),
    ))
}

pub fn finding_show(
    _home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let id = parse_id(&next(&mut args, "beta finding show")?)?;
    reject_remaining(args, "beta finding show")?;
    let store = open_store(store_path)?;
    let finding = read_finding(&store, &id).map_err(beta_err)?;
    let human = format!(
        "finding {}\ncampaign: {}\nclass: {}\nseverity: {}\ncomponent: {}\nsummary: {}\nenvironment: {}\nsteps: {}\nexpected: {}\nobserved: {}\nlimitations: {}",
        finding.id.as_str(),
        finding.campaign_id.as_str(),
        finding.evidence_class.as_str(),
        finding.severity.as_str(),
        finding.component,
        finding.summary,
        finding.environment,
        finding.steps,
        finding.expected,
        finding.observed,
        finding.limitations
    );
    Ok(CommandResult::new(human).field("finding", finding_json(&finding)))
}

pub fn finding_disposition(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let finding_id = parse_id(&next(&mut args, "beta finding disposition")?)?;
    let state = parse_finding_state(&required_flag(
        &mut args,
        "--state",
        "beta finding disposition",
    )?)?;
    let rationale = required_flag(&mut args, "--rationale", "beta finding disposition")?;
    let task_id = extract_flag(&mut args, "--task")
        .map(|value| parse_id(&value))
        .transpose()?;
    let resolved_in = extract_flag(&mut args, "--resolved-in")
        .map(|value| parse_id(&value))
        .transpose()?;
    reject_remaining(args, "beta finding disposition")?;

    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let object = create_finding_disposition(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &finding_id,
        task_id.as_ref(),
        resolved_in.as_ref(),
        state,
        &rationale,
        sequence::now_ms(),
        sequence::next(home, store_path)?,
    )
    .map_err(beta_err)?;
    Ok(CommandResult::new(format!(
        "finding disposition recorded: {} ({})",
        object.id().as_str(),
        state.as_str()
    ))
    .field("disposition_id", JsonValue::str(object.id().as_str()))
    .field("state", JsonValue::str(state.as_str())))
}

#[allow(clippy::too_many_lines)]
pub fn finding_to_task(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let project_ref = next(&mut args, "beta finding to-task")?;
    let finding_id = parse_id(&next(&mut args, "beta finding to-task")?)?;
    let route = required_flag(&mut args, "--route", "beta finding to-task")?;
    let risk = required_flag(&mut args, "--risk", "beta finding to-task")?;
    let title = required_flag(&mut args, "--title", "beta finding to-task")?;
    let description = required_flag(&mut args, "--description", "beta finding to-task")?;
    let paths = required_items(&mut args, "--path", "beta finding to-task")?;
    let acceptance = required_flag(&mut args, "--acceptance", "beta finding to-task")?;
    let non_goals = required_items(&mut args, "--non-goal", "beta finding to-task")?;
    let team = extract_flag(&mut args, "--team")
        .map(|value| parse_id(&value))
        .transpose()?;
    let mut extra_evidence = extract_flag_multi(&mut args, "--evidence");
    reject_remaining(args, "beta finding to-task")?;

    let identity = crate::identity::load_or_init(home)?;
    let project_id = project_alias::resolve(home, &project_ref)?;
    let mut store = open_store(store_path)?;
    let finding = read_finding(&store, &finding_id).map_err(beta_err)?;
    // Preserve the exact source finding regardless of what additional evidence
    // the operator chooses to attach.
    extra_evidence.insert(0, finding.id.as_str().to_string());
    let object = create_task_brief(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &project_id,
        team.as_ref(),
        &route,
        &risk,
        &title,
        &description,
        &paths,
        &extra_evidence,
        &acceptance,
        &non_goals,
        sequence::now_ms(),
        sequence::next(home, store_path)?,
    )
    .map_err(forge_err)?;
    Ok(CommandResult::new(format!(
        "Forge task created from finding {}: {}",
        finding.id.as_str(),
        object.id().as_str()
    ))
    .field("finding_id", JsonValue::str(finding.id.as_str()))
    .field("task_id", JsonValue::str(object.id().as_str())))
}

pub fn task_claim(_home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<CommandResult> {
    let task_id = parse_id(&next(&mut args, "beta task claim")?)?;
    let role = required_flag(&mut args, "--role", "beta task claim")?;
    let paths = required_items(&mut args, "--path", "beta task claim")?;
    let expires_ms: u64 = required_flag(&mut args, "--expires-ms", "beta task claim")?
        .parse()
        .map_err(|_| CliError::Usage("beta task claim: bad --expires-ms".to_string()))?;
    let base = extract_flag(&mut args, "--base")
        .map(|value| parse_id(&value))
        .transpose()?;
    let notes = extract_flag(&mut args, "--notes").unwrap_or_default();
    reject_remaining(args, "beta task claim")?;

    let mut store = open_store(store_path)?;
    let (human, device) = ephemeral_artifact_signer(&mut store)?;
    let object = create_work_claim(
        &mut store,
        &human.did(),
        &device,
        &task_id,
        &role,
        &paths,
        base.as_ref(),
        expires_ms,
        &notes,
        sequence::now_ms(),
        1,
    )
    .map_err(forge_err)?;
    Ok(CommandResult::new(format!(
        "anonymous Forge work claim recorded: {}\nThe signing identity was fresh for this claim and is not saved.",
        object.id().as_str()
    ))
    .field("claim_id", JsonValue::str(object.id().as_str()))
    .field("artifact_scoped_author", JsonValue::Bool(true))
    .field("lease_expires_ms", JsonValue::num(expires_ms)))
}

pub fn contribution_accept(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let source_id = parse_id(&next(&mut args, "beta contribution accept")?)?;
    let campaign_id = extract_flag(&mut args, "--campaign")
        .map(|value| parse_id(&value))
        .transpose()?;
    let kind = parse_contribution_kind(&required_flag(
        &mut args,
        "--kind",
        "beta contribution accept",
    )?)?;
    let summary = required_flag(&mut args, "--summary", "beta contribution accept")?;
    let evidence = required_items(&mut args, "--evidence", "beta contribution accept")?;
    reject_remaining(args, "beta contribution accept")?;

    let (claim_secret, claim_tag) = fresh_claim_secret()?;
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let object = create_contribution_receipt(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &source_id,
        campaign_id.as_ref(),
        claim_tag,
        kind,
        &summary,
        &evidence,
        sequence::now_ms(),
        sequence::next(home, store_path)?,
    )
    .map_err(beta_err)?;

    Ok(CommandResult::new(format!(
        "accepted contribution recorded: {}\nPRIVATE claim secret: {}\npublic claim commitment: {}\nDo not post the claim secret in GitHub, logs, or public evidence.",
        object.id().as_str(),
        hex(&claim_secret),
        hex(claim_tag.as_bytes())
    ))
    .field("contribution_id", JsonValue::str(object.id().as_str()))
    .field("claim_secret", JsonValue::str(hex(&claim_secret)))
    .field("claim_commitment", JsonValue::str(hex(claim_tag.as_bytes())))
    .field("sensitive_output", JsonValue::Bool(true)))
}

pub fn contribution_show(
    _home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let id = parse_id(&next(&mut args, "beta contribution show")?)?;
    reject_remaining(args, "beta contribution show")?;
    let store = open_store(store_path)?;
    let contribution = read_contribution_receipt(&store, &id).map_err(beta_err)?;
    let human = format!(
        "contribution {}\nsource: {}\ncampaign: {}\nkind: {}\nsummary: {}\nclaim commitment: {}",
        contribution.id.as_str(),
        contribution.source_id.as_str(),
        contribution
            .campaign_id
            .as_ref()
            .map(ObjectId::as_str)
            .unwrap_or("none"),
        contribution.kind.as_str(),
        contribution.summary,
        hex(contribution.claim_tag.as_bytes())
    );
    Ok(CommandResult::new(human).field("contribution", contribution_json(&contribution)))
}

pub fn claim_new(_home: &Path, _store_path: &Path, args: Vec<String>) -> Result<CommandResult> {
    reject_remaining(args, "beta claim new")?;
    let (secret, tag) = fresh_claim_secret()?;
    let account = BetaAccountId::new(random_32().map_err(crypto_err)?).map_err(beta_err)?;
    Ok(CommandResult::new(format!(
        "PRIVATE claim secret: {}\npublic claim commitment: {}\nopaque beta account: {}\nThe account is a test handle only; authenticated wallet ownership belongs to #337.",
        hex(&secret),
        hex(tag.as_bytes()),
        hex(account.as_bytes())
    ))
    .field("claim_secret", JsonValue::str(hex(&secret)))
    .field("claim_commitment", JsonValue::str(hex(tag.as_bytes())))
    .field("beta_account", JsonValue::str(hex(account.as_bytes())))
    .field("sensitive_output", JsonValue::Bool(true)))
}

pub fn disposition_show(
    _home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let id = parse_id(&next(&mut args, "beta disposition show")?)?;
    reject_remaining(args, "beta disposition show")?;
    let store = open_store(store_path)?;
    let disposition = read_finding_disposition(&store, &id).map_err(beta_err)?;
    Ok(CommandResult::new(format!(
        "disposition {}\nfinding: {}\nstate: {}\nrationale: {}",
        disposition.id.as_str(),
        disposition.finding_id.as_str(),
        disposition.state.as_str(),
        disposition.rationale
    ))
    .field(
        "disposition",
        JsonValue::Object(vec![
            ("id".to_string(), JsonValue::str(disposition.id.as_str())),
            (
                "finding_id".to_string(),
                JsonValue::str(disposition.finding_id.as_str()),
            ),
            (
                "state".to_string(),
                JsonValue::str(disposition.state.as_str()),
            ),
            (
                "rationale".to_string(),
                JsonValue::str(disposition.rationale),
            ),
            (
                "task_id".to_string(),
                JsonValue::opt_str(disposition.task_id.as_ref().map(ObjectId::as_str)),
            ),
            (
                "resolved_in".to_string(),
                JsonValue::opt_str(disposition.resolved_in.as_ref().map(ObjectId::as_str)),
            ),
        ]),
    ))
}

fn campaign_json(campaign: &mini_beta::BetaCampaign) -> JsonValue {
    JsonValue::Object(vec![
        ("id".to_string(), JsonValue::str(campaign.id.as_str())),
        (
            "target_id".to_string(),
            JsonValue::str(campaign.target_id.as_str()),
        ),
        (
            "beta_epoch".to_string(),
            JsonValue::str(hex(campaign.epoch.as_bytes())),
        ),
        ("title".to_string(), JsonValue::str(&campaign.title)),
        (
            "instructions".to_string(),
            JsonValue::str(&campaign.instructions),
        ),
        (
            "routes".to_string(),
            JsonValue::strs(campaign.routes.iter().map(String::as_str)),
        ),
        ("starts_ms".to_string(), JsonValue::num(campaign.starts_ms)),
        ("ends_ms".to_string(), JsonValue::num(campaign.ends_ms)),
        (
            "default_testing_grant".to_string(),
            JsonValue::num(campaign.default_testing_grant),
        ),
    ])
}

fn finding_json(finding: &mini_beta::BetaFinding) -> JsonValue {
    JsonValue::Object(vec![
        ("id".to_string(), JsonValue::str(finding.id.as_str())),
        (
            "campaign_id".to_string(),
            JsonValue::str(finding.campaign_id.as_str()),
        ),
        (
            "submission_tag".to_string(),
            JsonValue::str(hex(finding.submission_tag.as_bytes())),
        ),
        (
            "evidence_class".to_string(),
            JsonValue::str(finding.evidence_class.as_str()),
        ),
        (
            "severity".to_string(),
            JsonValue::str(finding.severity.as_str()),
        ),
        ("component".to_string(), JsonValue::str(&finding.component)),
        ("summary".to_string(), JsonValue::str(&finding.summary)),
        (
            "environment".to_string(),
            JsonValue::str(&finding.environment),
        ),
        ("steps".to_string(), JsonValue::str(&finding.steps)),
        ("expected".to_string(), JsonValue::str(&finding.expected)),
        ("observed".to_string(), JsonValue::str(&finding.observed)),
        (
            "evidence".to_string(),
            JsonValue::strs(finding.evidence.iter().map(String::as_str)),
        ),
        (
            "limitations".to_string(),
            JsonValue::str(&finding.limitations),
        ),
        (
            "privacy_redacted".to_string(),
            JsonValue::Bool(finding.privacy_redacted),
        ),
    ])
}

fn contribution_json(contribution: &mini_beta::ContributionReceipt) -> JsonValue {
    JsonValue::Object(vec![
        ("id".to_string(), JsonValue::str(contribution.id.as_str())),
        (
            "source_id".to_string(),
            JsonValue::str(contribution.source_id.as_str()),
        ),
        (
            "campaign_id".to_string(),
            JsonValue::opt_str(contribution.campaign_id.as_ref().map(ObjectId::as_str)),
        ),
        (
            "claim_commitment".to_string(),
            JsonValue::str(hex(contribution.claim_tag.as_bytes())),
        ),
        (
            "kind".to_string(),
            JsonValue::str(contribution.kind.as_str()),
        ),
        ("summary".to_string(), JsonValue::str(&contribution.summary)),
        (
            "evidence".to_string(),
            JsonValue::strs(contribution.evidence.iter().map(String::as_str)),
        ),
    ])
}

/// List dispositions without creating a second mutable state system.
pub fn disposition_list(
    _home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let finding_filter = extract_flag(&mut args, "--finding")
        .map(|value| parse_id(&value))
        .transpose()?;
    reject_remaining(args, "beta disposition list")?;
    let store = open_store(store_path)?;
    let ids = store
        .by_type(&ObjectType::Custom(
            BETA_FINDING_DISPOSITION_TYPE.to_string(),
        ))
        .map_err(|e| CliError::Store(e.to_string()))?;
    let mut dispositions = Vec::new();
    for id in ids {
        let disposition = read_finding_disposition(&store, &id).map_err(beta_err)?;
        if finding_filter
            .as_ref()
            .map(|finding| &disposition.finding_id == finding)
            .unwrap_or(true)
        {
            dispositions.push(disposition);
        }
    }
    dispositions.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    let human = if dispositions.is_empty() {
        "no beta finding dispositions".to_string()
    } else {
        dispositions
            .iter()
            .map(|item| {
                format!(
                    "{} [{}] finding {}",
                    item.id.as_str(),
                    item.state.as_str(),
                    item.finding_id.as_str()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(CommandResult::new(human).field(
        "dispositions",
        JsonValue::Array(
            dispositions
                .iter()
                .map(|item| {
                    JsonValue::Object(vec![
                        ("id".to_string(), JsonValue::str(item.id.as_str())),
                        (
                            "finding_id".to_string(),
                            JsonValue::str(item.finding_id.as_str()),
                        ),
                        ("state".to_string(), JsonValue::str(item.state.as_str())),
                    ])
                })
                .collect(),
        ),
    ))
}

/// List contribution receipts.  No balance/reward amount is part of this view.
pub fn contribution_list(
    _home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let campaign_filter = extract_flag(&mut args, "--campaign")
        .map(|value| parse_id(&value))
        .transpose()?;
    reject_remaining(args, "beta contribution list")?;
    let store = open_store(store_path)?;
    let ids = store
        .by_type(&ObjectType::Custom(BETA_CONTRIBUTION_TYPE.to_string()))
        .map_err(|e| CliError::Store(e.to_string()))?;
    let mut contributions = Vec::new();
    for id in ids {
        let contribution = read_contribution_receipt(&store, &id).map_err(beta_err)?;
        if campaign_filter
            .as_ref()
            .map(|campaign| contribution.campaign_id.as_ref() == Some(campaign))
            .unwrap_or(true)
        {
            contributions.push(contribution);
        }
    }
    contributions.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    let human = if contributions.is_empty() {
        "no accepted beta contributions".to_string()
    } else {
        contributions
            .iter()
            .map(|item| {
                format!(
                    "{} [{}] source {}",
                    item.id.as_str(),
                    item.kind.as_str(),
                    item.source_id.as_str()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(CommandResult::new(human).field(
        "contributions",
        JsonValue::Array(contributions.iter().map(contribution_json).collect()),
    ))
}
