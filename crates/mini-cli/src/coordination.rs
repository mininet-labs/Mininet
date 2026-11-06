//! `mini team` and `mini task` -- the first Forge-native contributor flow.
//!
//! These commands are intentionally explicit. They create and inspect signed
//! coordination evidence, while `mini pr approve`, governed merge, release,
//! and owner adoption remain separate commands with their existing rules.

use std::path::Path;

use mini_forge::{
    create_task_brief, create_technical_review, create_work_claim, create_working_group_charter,
    list_task_briefs, list_working_group_charters, read_working_group_charter, suggest_tasks,
    task_snapshot, ReviewDisposition, ReviewKind, WorkingGroupLifecycle,
};
use mini_objects::ObjectId;

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};
use crate::project as project_alias;
use crate::sequence;
use crate::store::{add_carrier_kels, build_oracle, open_store, MAX_CARRIER_AUTHORS};

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

fn required_flag(args: &mut Vec<String>, flag: &str, context: &str) -> Result<String> {
    extract_flag(args, flag).ok_or_else(|| CliError::Usage(format!("{context}: {flag} required")))
}

fn required_u64(args: &mut Vec<String>, flag: &str, context: &str) -> Result<u64> {
    let value = required_flag(args, flag, context)?;
    value
        .parse()
        .map_err(|_| CliError::Usage(format!("{context}: bad {flag}")))
}

fn parse_id(value: &str) -> Result<ObjectId> {
    ObjectId::parse(value).map_err(|error| CliError::Object(error.to_string()))
}

fn parse_items(values: Vec<String>, flag: &str) -> Result<Vec<String>> {
    if values.is_empty() {
        return Err(CliError::Usage(format!(
            "{flag} must be supplied at least once"
        )));
    }
    Ok(values)
}

fn parse_lifecycle(value: &str) -> Result<WorkingGroupLifecycle> {
    WorkingGroupLifecycle::parse(value).ok_or_else(|| {
        CliError::Usage(format!(
            "unknown lifecycle {value:?}; use proposed, incubating, active, mature, suspended, splitting, merging, or retired"
        ))
    })
}

fn parse_review_kind(value: &str) -> Result<ReviewKind> {
    ReviewKind::parse(value).ok_or_else(|| {
        CliError::Usage(format!(
            "unknown review kind {value:?}; use peer, external, or ai"
        ))
    })
}

fn parse_review_disposition(value: &str) -> Result<ReviewDisposition> {
    ReviewDisposition::parse(value).ok_or_else(|| {
        CliError::Usage(format!(
            "unknown review disposition {value:?}; use observations, needs-changes, or blocked"
        ))
    })
}

/// `mini team propose <project> --group-id <id> --name <name> ...`
#[allow(clippy::too_many_lines)]
pub fn team_propose(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let project_ref = next(&mut args, "team propose")?;
    let group_id = required_flag(&mut args, "--group-id", "team propose")?;
    let name = required_flag(&mut args, "--name", "team propose")?;
    let purpose = required_flag(&mut args, "--purpose", "team propose")?;
    let domain_paths = parse_items(extract_flag_multi(&mut args, "--path"), "--path")?;
    let autonomous_actions = extract_flag_multi(&mut args, "--autonomous");
    let reserved_actions = extract_flag_multi(&mut args, "--reserved");
    let term_policy = required_flag(&mut args, "--term-policy", "team propose")?;
    let appeal_policy = required_flag(&mut args, "--appeal-policy", "team propose")?;
    let lifecycle = extract_flag(&mut args, "--lifecycle")
        .map(|value| parse_lifecycle(&value))
        .transpose()?
        .unwrap_or(WorkingGroupLifecycle::Proposed);
    if !matches!(
        lifecycle,
        WorkingGroupLifecycle::Proposed | WorkingGroupLifecycle::Incubating
    ) {
        return Err(CliError::Usage(
            "team propose accepts only proposed or incubating lifecycle labels; active/mature state must be established by a later governance transition"
                .to_string(),
        ));
    }
    let dependencies = extract_flag_multi(&mut args, "--dependency");
    reject_remaining(args, "team propose")?;

    let identity = crate::identity::load_or_init(home)?;
    let project_id = project_alias::resolve(home, &project_ref)?;
    let mut store = open_store(store_path)?;
    let object = create_working_group_charter(
        &mut store,
        &identity.human_did(),
        &identity.device,
        Some(&project_id),
        &group_id,
        &name,
        &purpose,
        &domain_paths,
        &autonomous_actions,
        &reserved_actions,
        &term_policy,
        &appeal_policy,
        lifecycle,
        &dependencies,
        sequence::now_ms(),
        sequence::next(home, store_path)?,
    )
    .map_err(|error| CliError::Forge(error.to_string()))?;
    Ok(CommandResult::new(format!(
        "working-group charter proposed: {} (lifecycle: {})",
        object.id().as_str(),
        lifecycle.as_str()
    ))
    .field("charter_id", JsonValue::str(object.id().as_str()))
    .field("lifecycle", JsonValue::str(lifecycle.as_str())))
}

/// `mini team list [--project <project>]`
pub fn team_list(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<CommandResult> {
    let project_filter = extract_flag(&mut args, "--project")
        .map(|value| project_alias::resolve(home, &value))
        .transpose()?;
    reject_remaining(args, "team list")?;
    let identity = crate::identity::load_or_init(home)?;
    let store = open_store(store_path)?;
    let oracle = build_oracle(home, &identity)?;
    let charters = list_working_group_charters(&store, &oracle)
        .map_err(|error| CliError::Forge(error.to_string()))?;
    let charters: Vec<_> = charters
        .into_iter()
        .filter(|charter| {
            project_filter
                .as_ref()
                .map(|id| charter.project_id.as_ref() == Some(id))
                .unwrap_or(true)
        })
        .collect();
    let mut human = String::new();
    for charter in &charters {
        human.push_str(&format!(
            "{} [{}] {} (identity root {})\n",
            charter.id.as_str(),
            charter.lifecycle.as_str(),
            charter.name,
            charter.author.as_str()
        ));
    }
    if human.is_empty() {
        human.push_str("no verified working-group charters");
    }
    Ok(CommandResult::new(human).field(
        "charters",
        JsonValue::Array(charters.iter().map(charter_json).collect()),
    ))
}

/// `mini team show <charter-id>`
pub fn team_show(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<CommandResult> {
    let id = parse_id(&next(&mut args, "team show")?)?;
    reject_remaining(args, "team show")?;
    let identity = crate::identity::load_or_init(home)?;
    let store = open_store(store_path)?;
    let oracle = build_oracle(home, &identity)?;
    let charter = read_working_group_charter(&store, &oracle, &id)
        .map_err(|error| CliError::Forge(error.to_string()))?;
    let human = format_charter(&charter);
    Ok(CommandResult::new(human).field("charter", charter_json(&charter)))
}

/// `mini task create <project> --route <route> --risk <class> ...`
#[allow(clippy::too_many_lines)]
pub fn task_create(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<CommandResult> {
    let project_ref = next(&mut args, "task create")?;
    let route = required_flag(&mut args, "--route", "task create")?;
    let risk_class = required_flag(&mut args, "--risk", "task create")?;
    let title = required_flag(&mut args, "--title", "task create")?;
    let description = required_flag(&mut args, "--description", "task create")?;
    let paths = parse_items(extract_flag_multi(&mut args, "--path"), "--path")?;
    let evidence = parse_items(extract_flag_multi(&mut args, "--evidence"), "--evidence")?;
    let acceptance = required_flag(&mut args, "--acceptance", "task create")?;
    let non_goals = parse_items(extract_flag_multi(&mut args, "--non-goal"), "--non-goal")?;
    let charter_id = extract_flag(&mut args, "--team")
        .map(|value| parse_id(&value))
        .transpose()?;
    reject_remaining(args, "task create")?;

    let identity = crate::identity::load_or_init(home)?;
    let project_id = project_alias::resolve(home, &project_ref)?;
    let mut store = open_store(store_path)?;
    let object = create_task_brief(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &project_id,
        charter_id.as_ref(),
        &route,
        &risk_class,
        &title,
        &description,
        &paths,
        &evidence,
        &acceptance,
        &non_goals,
        sequence::now_ms(),
        sequence::next(home, store_path)?,
    )
    .map_err(|error| CliError::Forge(error.to_string()))?;
    Ok(
        CommandResult::new(format!("task brief created: {}", object.id().as_str()))
            .field("task_id", JsonValue::str(object.id().as_str())),
    )
}

/// `mini task list [--route <route>] [--path <path>] [--limit <n>]`
pub fn task_list(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<CommandResult> {
    let route = extract_flag(&mut args, "--route");
    let path = extract_flag(&mut args, "--path");
    let limit = extract_flag(&mut args, "--limit")
        .map(|value| parse_limit(&value))
        .transpose()?
        .unwrap_or(100);
    reject_remaining(args, "task list")?;
    let identity = crate::identity::load_or_init(home)?;
    let store = open_store(store_path)?;
    let oracle = build_oracle(home, &identity)?;
    let tasks = if route.is_some() || path.is_some() {
        suggest_tasks(&store, &oracle, route.as_deref(), path.as_deref(), limit)
    } else {
        let mut all = list_task_briefs(&store, &oracle)
            .map_err(|error| CliError::Forge(error.to_string()))?;
        all.truncate(limit);
        Ok(all)
    }
    .map_err(|error| CliError::Forge(error.to_string()))?;
    Ok(task_list_result(tasks))
}

/// `mini task suggest --route <route> [--path <path>] [--limit <n>]`
pub fn task_suggest(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let route = extract_flag(&mut args, "--route");
    let path = extract_flag(&mut args, "--path");
    if route.is_none() && path.is_none() {
        return Err(CliError::Usage(
            "task suggest requires --route or --path; suggestions are explicit and local"
                .to_string(),
        ));
    }
    let limit = extract_flag(&mut args, "--limit")
        .map(|value| parse_limit(&value))
        .transpose()?
        .unwrap_or(20);
    reject_remaining(args, "task suggest")?;
    let identity = crate::identity::load_or_init(home)?;
    let store = open_store(store_path)?;
    let oracle = build_oracle(home, &identity)?;
    let tasks = suggest_tasks(&store, &oracle, route.as_deref(), path.as_deref(), limit)
        .map_err(|error| CliError::Forge(error.to_string()))?;
    let mut result = task_list_result(tasks);
    result.human = format!("advisory task suggestions\n{}", result.human);
    Ok(result)
}

/// `mini task show <task-id>`
pub fn task_show(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<CommandResult> {
    let id = parse_id(&next(&mut args, "task show")?)?;
    reject_remaining(args, "task show")?;
    let identity = crate::identity::load_or_init(home)?;
    let store = open_store(store_path)?;
    let oracle = build_oracle(home, &identity)?;
    let snapshot =
        task_snapshot(&store, &oracle, &id).map_err(|error| CliError::Forge(error.to_string()))?;
    let untrusted = untrusted_claims(&store, &snapshot)?;
    let mut human = format_snapshot(&snapshot);
    human.push_str(&format_untrusted(&untrusted));
    Ok(CommandResult::new(human)
        .field("task", task_json(&snapshot.task))
        .field(
            "claims",
            JsonValue::Array(snapshot.claims.iter().map(claim_json).collect()),
        )
        .field(
            "untrusted_claims",
            JsonValue::Array(untrusted.shown.iter().map(untrusted_claim_json).collect()),
        )
        .field(
            "untrusted_claims_not_shown",
            JsonValue::num(untrusted.not_shown as u64),
        )
        .field(
            "untrusted_claims_older_not_examined",
            JsonValue::Bool(untrusted.older_not_examined),
        )
        .field(
            "reviews",
            JsonValue::Array(snapshot.reviews.iter().map(review_json).collect()),
        ))
}

/// `mini task claim <task-id> --role <role> --path <path> --expires-ms <n>`
pub fn task_claim(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<CommandResult> {
    let task_id = parse_id(&next(&mut args, "task claim")?)?;
    let role = required_flag(&mut args, "--role", "task claim")?;
    let paths = parse_items(extract_flag_multi(&mut args, "--path"), "--path")?;
    let lease_expires_ms = required_u64(&mut args, "--expires-ms", "task claim")?;
    let base_id = extract_flag(&mut args, "--base")
        .map(|value| parse_id(&value))
        .transpose()?;
    let notes = extract_flag(&mut args, "--notes").unwrap_or_default();
    reject_remaining(args, "task claim")?;

    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let object = create_work_claim(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &task_id,
        &role,
        &paths,
        base_id.as_ref(),
        lease_expires_ms,
        &notes,
        sequence::now_ms(),
        sequence::next(home, store_path)?,
    )
    .map_err(|error| CliError::Forge(error.to_string()))?;
    Ok(CommandResult::new(format!(
        "work claim recorded (coordination only; expires {lease_expires_ms}): {}",
        object.id().as_str()
    ))
    .field("claim_id", JsonValue::str(object.id().as_str()))
    .field("lease_expires_ms", JsonValue::num(lease_expires_ms)))
}

/// `mini task review <task-id> --head <id> --kind <kind> --disposition <d> ...`
/// `mini campaign create --title <title> --description <text> --expires-ms <n>`
pub fn campaign_create(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let title = required_flag(&mut args, "--title", "campaign create")?;
    let description = required_flag(&mut args, "--description", "campaign create")?;
    let expires_ms = required_u64(&mut args, "--expires-ms", "campaign create")?;
    reject_remaining(args, "campaign create")?;

    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let object = mini_forge::create_campaign(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &title,
        &description,
        expires_ms,
        sequence::now_ms(),
        sequence::next(home, store_path)?,
    )
    .map_err(|error| CliError::Forge(error.to_string()))?;

    Ok(
        CommandResult::new(format!("campaign created: {}", object.id().as_str()))
            .field("campaign_id", JsonValue::str(object.id().as_str())),
    )
}

/// `mini finding create --target <id> [--head <id>] --findings <text>`
pub fn finding_create(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let target_id = parse_id(&required_flag(&mut args, "--target", "finding create")?)?;
    let reviewed_head = extract_flag(&mut args, "--head")
        .map(|value| parse_id(&value))
        .transpose()?;
    let findings = required_flag(&mut args, "--findings", "finding create")?;
    reject_remaining(args, "finding create")?;

    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let object = mini_forge::create_finding(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &target_id,
        reviewed_head.as_ref(),
        &findings,
        sequence::now_ms(),
        sequence::next(home, store_path)?,
    )
    .map_err(|error| CliError::Forge(error.to_string()))?;

    Ok(
        CommandResult::new(format!("finding created: {}", object.id().as_str()))
            .field("finding_id", JsonValue::str(object.id().as_str())),
    )
}

/// `mini disposition create --finding <id> --disposition <d> --notes <text>`
pub fn disposition_create(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let finding_id = parse_id(&required_flag(
        &mut args,
        "--finding",
        "disposition create",
    )?)?;
    let disposition = mini_forge::FindingDisposition::parse(&required_flag(
        &mut args,
        "--disposition",
        "disposition create",
    )?)
    .ok_or_else(|| CliError::Usage("invalid disposition".into()))?;
    let notes = required_flag(&mut args, "--notes", "disposition create")?;
    reject_remaining(args, "disposition create")?;

    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let object = mini_forge::create_disposition(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &finding_id,
        disposition,
        &notes,
        sequence::now_ms(),
        sequence::next(home, store_path)?,
    )
    .map_err(|error| CliError::Forge(error.to_string()))?;

    Ok(
        CommandResult::new(format!("disposition created: {}", object.id().as_str()))
            .field("disposition_id", JsonValue::str(object.id().as_str())),
    )
}

/// `mini campaign show <campaign-id>`
pub fn campaign_show(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
) -> Result<CommandResult> {
    let id = parse_id(&next(&mut args, "campaign show")?)?;
    reject_remaining(args, "campaign show")?;

    let identity = crate::identity::load_or_init(home)?;
    let store = open_store(store_path)?;
    let oracle = build_oracle(home, &identity)?;
    let snapshot = mini_forge::campaign_snapshot(&store, &oracle, &id)
        .map_err(|error| CliError::Forge(error.to_string()))?;

    let mut human = format!(
        "campaign {}\nidentity root: {}\ntitle: {}\ndescription: {}\nexpires_ms: {}\n",
        snapshot.campaign.id.as_str(),
        snapshot.campaign.author.as_str(),
        snapshot.campaign.title,
        snapshot.campaign.description,
        snapshot.campaign.expires_ms
    );

    human.push_str("findings:\n");
    if snapshot.findings.is_empty() {
        human.push_str("  (none)\n");
    } else {
        for finding in &snapshot.findings {
            human.push_str(&format!(
                "  {} by {}:\n    target: {}\n    head: {:?}\n    findings: {}\n",
                finding.id.as_str(),
                finding.author.as_str(),
                finding.target_id.as_str(),
                finding.reviewed_head.as_ref().map(|id| id.as_str()),
                finding.findings
            ));
        }
    }

    human.push_str("dispositions:\n");
    if snapshot.dispositions.is_empty() {
        human.push_str("  (none)\n");
    } else {
        for disposition in &snapshot.dispositions {
            human.push_str(&format!(
                "  {} by {}:\n    finding: {}\n    disposition: {}\n    notes: {}\n",
                disposition.id.as_str(),
                disposition.author.as_str(),
                disposition.finding_id.as_str(),
                disposition.disposition.as_str(),
                disposition.notes
            ));
        }
    }

    Ok(CommandResult::new(human)
        .field("campaign", campaign_json(&snapshot.campaign))
        .field(
            "findings",
            JsonValue::Array(snapshot.findings.iter().map(finding_json).collect()),
        )
        .field(
            "dispositions",
            JsonValue::Array(snapshot.dispositions.iter().map(disposition_json).collect()),
        ))
}

fn campaign_json(campaign: &mini_forge::Campaign) -> JsonValue {
    JsonValue::Object(vec![
        ("id".to_string(), JsonValue::str(campaign.id.as_str())),
        (
            "author".to_string(),
            JsonValue::str(campaign.author.as_str()),
        ),
        ("title".to_string(), JsonValue::str(campaign.title.as_str())),
        (
            "description".to_string(),
            JsonValue::str(campaign.description.as_str()),
        ),
        (
            "expires_ms".to_string(),
            JsonValue::num(campaign.expires_ms),
        ),
    ])
}

fn finding_json(finding: &mini_forge::Finding) -> JsonValue {
    JsonValue::Object(vec![
        ("id".to_string(), JsonValue::str(finding.id.as_str())),
        (
            "author".to_string(),
            JsonValue::str(finding.author.as_str()),
        ),
        (
            "target_id".to_string(),
            JsonValue::str(finding.target_id.as_str()),
        ),
        (
            "reviewed_head".to_string(),
            JsonValue::opt_str(finding.reviewed_head.as_ref().map(|id| id.as_str())),
        ),
        (
            "findings".to_string(),
            JsonValue::str(finding.findings.as_str()),
        ),
    ])
}

fn disposition_json(disposition: &mini_forge::Disposition) -> JsonValue {
    JsonValue::Object(vec![
        ("id".to_string(), JsonValue::str(disposition.id.as_str())),
        (
            "author".to_string(),
            JsonValue::str(disposition.author.as_str()),
        ),
        (
            "finding_id".to_string(),
            JsonValue::str(disposition.finding_id.as_str()),
        ),
        (
            "disposition".to_string(),
            JsonValue::str(disposition.disposition.as_str()),
        ),
        (
            "notes".to_string(),
            JsonValue::str(disposition.notes.as_str()),
        ),
    ])
}

pub fn task_review(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<CommandResult> {
    let task_id = parse_id(&next(&mut args, "task review")?)?;
    let reviewed_head = parse_id(&required_flag(&mut args, "--head", "task review")?)?;
    let kind = parse_review_kind(&required_flag(&mut args, "--kind", "task review")?)?;
    let disposition =
        parse_review_disposition(&required_flag(&mut args, "--disposition", "task review")?)?;
    let findings = required_flag(&mut args, "--findings", "task review")?;
    let evidence = parse_items(extract_flag_multi(&mut args, "--evidence"), "--evidence")?;
    let limitations = required_flag(&mut args, "--limitations", "task review")?;
    let claim_id = extract_flag(&mut args, "--claim")
        .map(|value| parse_id(&value))
        .transpose()?;
    reject_remaining(args, "task review")?;

    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let object = create_technical_review(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &task_id,
        claim_id.as_ref(),
        &reviewed_head,
        kind,
        disposition,
        &findings,
        &evidence,
        &limitations,
        sequence::now_ms(),
        sequence::next(home, store_path)?,
    )
    .map_err(|error| CliError::Forge(error.to_string()))?;
    Ok(CommandResult::new(format!(
        "technical review handoff recorded: {} ({}, {}; no approval recorded)",
        object.id().as_str(),
        kind.as_str(),
        disposition.as_str()
    ))
    .field("review_id", JsonValue::str(object.id().as_str()))
    .field("reviewed_head", JsonValue::str(reviewed_head.as_str()))
    .field("kind", JsonValue::str(kind.as_str()))
    .field("disposition", JsonValue::str(disposition.as_str())))
}

fn task_list_result(tasks: Vec<mini_forge::TaskBrief>) -> CommandResult {
    let mut human = String::new();
    for task in &tasks {
        human.push_str(&format!(
            "{} [{} / {}] {} (identity root {})\n",
            task.id.as_str(),
            task.route,
            task.risk_class,
            task.title,
            task.author.as_str()
        ));
    }
    if human.is_empty() {
        human.push_str("no verified task briefs");
    }
    CommandResult::new(human).field(
        "tasks",
        JsonValue::Array(tasks.iter().map(task_json).collect()),
    )
}

fn format_charter(charter: &mini_forge::WorkingGroupCharter) -> String {
    format!(
        "working-group charter {}\nidentity root: {}\nproject: {}\ngroup id: {}\nname: {}\npurpose: {}\nlifecycle: {}\ndomain paths: {}\nautonomous actions: {}\nreserved actions: {}\ndependencies: {}\nterm policy: {}\nappeal policy: {}\n",
        charter.id.as_str(),
        charter.author.as_str(),
        charter
            .project_id
            .as_ref()
            .map(ObjectId::as_str)
            .unwrap_or("(none)"),
        charter.group_id,
        charter.name,
        charter.purpose,
        charter.lifecycle.as_str(),
        charter.domain_paths.join(", "),
        charter.autonomous_actions.join(", "),
        charter.reserved_actions.join(", "),
        charter.dependencies.join(", "),
        charter.term_policy,
        charter.appeal_policy
    )
}

fn format_snapshot(snapshot: &mini_forge::TaskSnapshot) -> String {
    let task = &snapshot.task;
    let mut out = format!(
        "task {}\nidentity root: {}\nproject: {}\ncharter: {}\nroute: {}\nrisk: {}\ntitle: {}\ndescription: {}\npaths: {}\nevidence: {}\nacceptance: {}\nnon-goals: {}\n",
        task.id.as_str(),
        task.author.as_str(),
        task.project_id.as_str(),
        task.charter_id
            .as_ref()
            .map(ObjectId::as_str)
            .unwrap_or("(none)"),
        task.route,
        task.risk_class,
        task.title,
        task.description,
        task.paths.join(", "),
        task.evidence.join(", "),
        task.acceptance,
        task.non_goals.join(", ")
    );
    out.push_str("claims:\n");
    if snapshot.claims.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for claim in &snapshot.claims {
            out.push_str(&format!(
                "  {} identity root {} role {} expires {} paths {}\n",
                claim.id.as_str(),
                claim.author.as_str(),
                claim.role,
                claim.lease_expires_ms,
                claim.paths.join(", ")
            ));
        }
    }
    out.push_str("technical review handoffs:\n");
    if snapshot.reviews.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for review in &snapshot.reviews {
            out.push_str(&format!(
                "  {} identity root {} reviewed {} kind {} disposition {}\n",
                review.id.as_str(),
                review.author.as_str(),
                review.reviewed_head.as_str(),
                review.kind.as_str(),
                review.disposition.as_str()
            ));
        }
    }
    out
}

fn charter_json(charter: &mini_forge::WorkingGroupCharter) -> JsonValue {
    JsonValue::Object(vec![
        ("id".to_string(), JsonValue::str(charter.id.as_str())),
        (
            "author_identity_root".to_string(),
            JsonValue::str(charter.author.as_str()),
        ),
        (
            "project_id".to_string(),
            JsonValue::opt_str(charter.project_id.as_ref().map(ObjectId::as_str)),
        ),
        (
            "group_id".to_string(),
            JsonValue::str(charter.group_id.as_str()),
        ),
        ("name".to_string(), JsonValue::str(charter.name.as_str())),
        (
            "purpose".to_string(),
            JsonValue::str(charter.purpose.as_str()),
        ),
        (
            "domain_paths".to_string(),
            JsonValue::strs(charter.domain_paths.clone()),
        ),
        (
            "lifecycle".to_string(),
            JsonValue::str(charter.lifecycle.as_str()),
        ),
    ])
}

fn task_json(task: &mini_forge::TaskBrief) -> JsonValue {
    JsonValue::Object(vec![
        ("id".to_string(), JsonValue::str(task.id.as_str())),
        (
            "author_identity_root".to_string(),
            JsonValue::str(task.author.as_str()),
        ),
        (
            "project_id".to_string(),
            JsonValue::str(task.project_id.as_str()),
        ),
        (
            "charter_id".to_string(),
            JsonValue::opt_str(task.charter_id.as_ref().map(ObjectId::as_str)),
        ),
        ("route".to_string(), JsonValue::str(task.route.as_str())),
        (
            "risk_class".to_string(),
            JsonValue::str(task.risk_class.as_str()),
        ),
        ("title".to_string(), JsonValue::str(task.title.as_str())),
        ("paths".to_string(), JsonValue::strs(task.paths.clone())),
        (
            "evidence".to_string(),
            JsonValue::strs(task.evidence.clone()),
        ),
    ])
}

/// Most claims from untrusted signers one `task show` lists; the rest are only
/// counted, so a flood of throwaway identities cannot bury the view.
const MAX_UNTRUSTED_CLAIMS_SHOWN: usize = 32;
/// How many of a task's newest verified untrusted claims are held in memory at
/// once. Memory is bounded by this however many claims a peer attaches.
const UNTRUSTED_CLAIM_POOL: usize = 256;
/// Most objects linking to a task examined in one `task show`.
const MAX_TASK_LINKS_EXAMINED: usize = 100_000;

/// Claims signed by a key history the viewer has not trusted but that the store
/// holds self-certifying carriers for. Includes anonymous claims, whose signer is
/// a fresh throwaway key, and also any persistent peer the viewer has not yet
/// trusted: the carriers cannot tell the two apart.
struct UntrustedClaims {
    shown: Vec<mini_forge::WorkClaim>,
    /// Verified claims in the pool beyond the display cap.
    not_shown: usize,
    /// True when the task has more claim objects than the pool examines, so
    /// some older ones were not considered at all.
    older_not_examined: bool,
}

/// Choose which untrusted claims to display: claims whose lease has not expired
/// first, newest first, then expired ones, newest first. A cap that kept the
/// oldest would hide a fresh active claim behind expired ones.
fn select_untrusted(
    mut claims: Vec<mini_forge::WorkClaim>,
    now_ms: u64,
    cap: usize,
) -> UntrustedClaims {
    claims.sort_by(|a, b| {
        let (a_live, b_live) = (a.lease_expires_ms > now_ms, b.lease_expires_ms > now_ms);
        b_live
            .cmp(&a_live)
            .then_with(|| b.timestamp_ms.cmp(&a.timestamp_ms))
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    let not_shown = claims.len().saturating_sub(cap);
    claims.truncate(cap);
    UntrustedClaims {
        shown: claims,
        not_shown,
        older_not_examined: false,
    }
}

/// Claims that verify against the store's own key-history carriers but are not
/// already listed as trusted claims. They prove control of a key, not identity.
///
/// Claims stream through a pool of the newest `UNTRUSTED_CLAIM_POOL` verified
/// ones. A claim must beat the pool's oldest member before it is verified, and
/// is verified (against key histories loaded only for its own author) before it
/// can displace one, so a peer attaching thousands of claims, or claims that do
/// not verify, cannot grow memory or crowd out real ones.
fn untrusted_claims<B: mini_store::Backend>(
    store: &mini_store::Store<B>,
    snapshot: &mini_forge::TaskSnapshot,
) -> Result<UntrustedClaims> {
    let claim_type = mini_objects::ObjectType::Custom(mini_forge::WORK_CLAIM_TYPE.to_string());
    let mut dir = mini_forge::KelDirectory::new();
    let mut loaded = std::collections::BTreeSet::<String>::new();
    let mut pool: std::collections::BTreeMap<(u64, String), mini_forge::WorkClaim> =
        std::collections::BTreeMap::new();
    let mut seen = 0usize;
    for id in store
        .linking_to(&snapshot.task.id)
        .map_err(|e| CliError::Store(e.to_string()))?
        .into_iter()
        .take(MAX_TASK_LINKS_EXAMINED)
    {
        let Ok(object) = store.get(&id) else { continue };
        if object.object_type != claim_type || snapshot.claims.iter().any(|known| known.id == id) {
            continue;
        }
        seen += 1;
        let key = (object.timestamp_ms, id.as_str().to_owned());
        if pool.len() >= UNTRUSTED_CLAIM_POOL
            && pool.keys().next().is_some_and(|oldest| key <= *oldest)
        {
            continue;
        }
        let author = object.author_human.clone();
        if !loaded.contains(author.as_str()) {
            if loaded.len() >= MAX_CARRIER_AUTHORS {
                continue;
            }
            loaded.insert(author.as_str().to_owned());
            add_carrier_kels(store, &mut dir, &author, &object.author_device)?;
        }
        let Ok(claim) = mini_forge::read_work_claim(store, &dir, &id) else {
            continue;
        };
        if claim.task_id != snapshot.task.id {
            continue;
        }
        pool.insert(key, claim);
        if pool.len() > UNTRUSTED_CLAIM_POOL {
            pool.pop_first();
        }
    }
    let mut selected = select_untrusted(
        pool.into_values().collect(),
        sequence::now_ms(),
        MAX_UNTRUSTED_CLAIMS_SHOWN,
    );
    selected.older_not_examined = seen > UNTRUSTED_CLAIM_POOL;
    Ok(selected)
}

fn format_untrusted(claims: &UntrustedClaims) -> String {
    if claims.shown.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "claims from signers you have not trusted (this proves control of the key, not who holds it; an anonymous throwaway key looks the same as a persistent one):\n",
    );
    for claim in &claims.shown {
        out.push_str(&format!(
            "  {} key {} role {} expires {} paths {}\n",
            claim.id.as_str(),
            claim.author.as_str(),
            claim.role,
            claim.lease_expires_ms,
            claim.paths.join(", ")
        ));
    }
    if claims.not_shown > 0 {
        out.push_str(&format!(
            "  ... and {} more claim(s) from untrusted signers not shown\n",
            claims.not_shown
        ));
    }
    if claims.older_not_examined {
        out.push_str(
            "  (this task has more claims than are examined; older ones were not considered)\n",
        );
    }
    out
}

fn untrusted_claim_json(claim: &mini_forge::WorkClaim) -> JsonValue {
    JsonValue::Object(vec![
        ("id".to_string(), JsonValue::str(claim.id.as_str())),
        ("signer_trusted".to_string(), JsonValue::Bool(false)),
        (
            "controlled_key_root".to_string(),
            JsonValue::str(claim.author.as_str()),
        ),
        ("role".to_string(), JsonValue::str(claim.role.as_str())),
        ("paths".to_string(), JsonValue::strs(claim.paths.clone())),
        (
            "lease_expires_ms".to_string(),
            JsonValue::num(claim.lease_expires_ms),
        ),
    ])
}

fn claim_json(claim: &mini_forge::WorkClaim) -> JsonValue {
    JsonValue::Object(vec![
        ("id".to_string(), JsonValue::str(claim.id.as_str())),
        (
            "author_identity_root".to_string(),
            JsonValue::str(claim.author.as_str()),
        ),
        (
            "task_id".to_string(),
            JsonValue::str(claim.task_id.as_str()),
        ),
        ("role".to_string(), JsonValue::str(claim.role.as_str())),
        ("paths".to_string(), JsonValue::strs(claim.paths.clone())),
        (
            "base_id".to_string(),
            JsonValue::opt_str(claim.base_id.as_ref().map(ObjectId::as_str)),
        ),
        (
            "lease_expires_ms".to_string(),
            JsonValue::num(claim.lease_expires_ms),
        ),
    ])
}

fn review_json(review: &mini_forge::TechnicalReview) -> JsonValue {
    JsonValue::Object(vec![
        ("id".to_string(), JsonValue::str(review.id.as_str())),
        (
            "author_identity_root".to_string(),
            JsonValue::str(review.author.as_str()),
        ),
        (
            "task_id".to_string(),
            JsonValue::str(review.task_id.as_str()),
        ),
        (
            "claim_id".to_string(),
            JsonValue::opt_str(review.claim_id.as_ref().map(ObjectId::as_str)),
        ),
        (
            "reviewed_head".to_string(),
            JsonValue::str(review.reviewed_head.as_str()),
        ),
        ("kind".to_string(), JsonValue::str(review.kind.as_str())),
        (
            "disposition".to_string(),
            JsonValue::str(review.disposition.as_str()),
        ),
    ])
}

fn parse_limit(value: &str) -> Result<usize> {
    let limit: usize = value
        .parse()
        .map_err(|_| CliError::Usage("bad --limit".to_string()))?;
    if limit > 100 {
        return Err(CliError::Usage("--limit must be at most 100".to_string()));
    }
    Ok(limit)
}

fn reject_remaining(args: Vec<String>, context: &str) -> Result<()> {
    if let Some(unexpected) = args.first() {
        return Err(CliError::Usage(format!(
            "{context}: unexpected argument {unexpected:?}"
        )));
    }
    Ok(())
}

fn next(args: &mut Vec<String>, context: &str) -> Result<String> {
    if args.is_empty() {
        return Err(CliError::Usage(format!("{context}: missing argument")));
    }
    Ok(args.remove(0))
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    use mini_objects::{ObjectBuilder, ObjectType, Payload};

    fn claim(n: u8, timestamp_ms: u64, lease_expires_ms: u64) -> mini_forge::WorkClaim {
        let mut root = did_mini::Controller::incept_single().unwrap();
        let device = did_mini::Controller::incept_device(
            &root.did(),
            vec![mini_crypto::SigningKey::generate().unwrap()],
            1,
            vec![mini_crypto::SigningKey::generate().unwrap()],
            1,
        )
        .unwrap();
        root.delegate_device(&device.did(), did_mini::Capabilities::POST)
            .unwrap();
        let object = ObjectBuilder::new(ObjectType::POST)
            .sequence(u64::from(n) + 1)
            .payload(Payload::Public(vec![n]))
            .sign(&root.did(), &device)
            .unwrap();
        mini_forge::WorkClaim {
            id: object.id().clone(),
            author: root.did(),
            task_id: object.id().clone(),
            role: format!("role {n}"),
            paths: Vec::new(),
            base_id: None,
            lease_expires_ms,
            notes: String::new(),
            timestamp_ms,
            sequence: 1,
        }
    }

    #[test]
    fn the_cap_keeps_active_claims_newest_first_and_never_buries_them_under_expired_ones() {
        let now = 1_000_000;
        // Three old expired claims, then two newer active ones (the newest last).
        let claims = vec![
            claim(1, 10, 100),
            claim(2, 20, 200),
            claim(3, 30, 300),
            claim(4, 400_000, now + 5_000),
            claim(5, 500_000, now + 9_000),
        ];
        let kept = select_untrusted(claims, now, 2);
        let roles: Vec<&str> = kept.shown.iter().map(|c| c.role.as_str()).collect();
        // Both active claims survive the cap, newest first; expired ones are counted.
        assert_eq!(roles, ["role 5", "role 4"]);
        assert_eq!(kept.not_shown, 3);
        // With room to spare, expired claims follow the active ones, newest first.
        let all = select_untrusted(
            vec![
                claim(1, 10, 100),
                claim(2, 20, 200),
                claim(4, 400_000, now + 1),
            ],
            now,
            10,
        );
        let order: Vec<&str> = all.shown.iter().map(|c| c.role.as_str()).collect();
        assert_eq!(order, ["role 4", "role 2", "role 1"]);
        assert_eq!(all.not_shown, 0);
    }
}
