//! `mini pr ...` — propose/approve/merge, over `mini_forge::governance`'s
//! already-real proposal/review/merge object model (predates the audit
//! this crate responds to; see D-0066's correction in `docs/DECISION_LOG.md`).

use std::path::Path;

use did_mini::Did;
use mini_forge::{
    ai_assistance, approve, declare_ai_assistance, list_findings, merge, propose, record_findings,
    resolve_project,
};
use mini_objects::ObjectId;

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};
use crate::project as project_alias;
use crate::sequence;
use crate::store::{build_oracle, open_store};

/// Resolve the chain position a new PR/merge should build against: the
/// project's verified current governed tip. Resolution errors propagate;
/// missing identity/governance evidence must never reset the base to genesis.
fn default_base(
    store: &mini_store::Store<mini_store::FsBackend>,
    home: &Path,
    identity: &crate::identity::Identity,
    project_id: &ObjectId,
) -> Result<ObjectId> {
    let oracle = build_oracle(home, identity)?;
    resolve_project(store, &oracle, project_id)
        .map(|s| s.tip)
        .map_err(|e| CliError::Forge(e.to_string()))
}

/// `mini pr propose <project> --branch <b> --title <t> --head <commit-id> [--base <id>]`
#[allow(clippy::too_many_arguments)]
pub fn propose_pr(
    home: &Path,
    store_path: &Path,
    project_ref: &str,
    branch: &str,
    title: &str,
    head_ref: &str,
    base_ref: Option<&str>,
) -> Result<CommandResult> {
    let identity = crate::identity::load(home)?;
    let mut store = open_store(store_path)?;
    let project_id = project_alias::resolve(home, project_ref)?;
    let head = ObjectId::parse(head_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let base = match base_ref {
        Some(b) => ObjectId::parse(b).map_err(|e| CliError::Object(e.to_string()))?,
        None => default_base(&store, home, &identity, &project_id)?,
    };

    // Resolve even an explicitly supplied base so missing identity/governance
    // evidence never silently falls back to project genesis.
    let _ = default_base(&store, home, &identity, &project_id)?;
    mini_forge::checkout(&store, &head).map_err(|e| CliError::Forge(e.to_string()))?;
    let seq = sequence::next(home, store_path)?;
    let obj = propose(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &project_id,
        branch,
        title,
        &head,
        &base,
        sequence::now_ms(),
        seq,
    )
    .map_err(|e| CliError::Forge(e.to_string()))?;
    Ok(
        CommandResult::new(format!("PR proposed: {}", obj.id().as_str()))
            .field("pr_id", JsonValue::str(obj.id().as_str()))
            .field("head", JsonValue::str(head.as_str()))
            .field("base", JsonValue::str(base.as_str())),
    )
}

/// `mini pr approve <project> --pr <id> --head <commit-id> [--reject] [--findings <text>]`
pub fn approve_pr(
    home: &Path,
    store_path: &Path,
    pr_ref: &str,
    head_ref: &str,
    approve_it: bool,
    findings_text: Option<&str>,
) -> Result<CommandResult> {
    let identity = crate::identity::load(home)?;
    let mut store = open_store(store_path)?;
    let pr_id = ObjectId::parse(pr_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let head = ObjectId::parse(head_ref).map_err(|e| CliError::Object(e.to_string()))?;

    let oracle = build_oracle(home, &identity)?;
    let view = mini_forge::inspect_proposal(&store, &oracle, &pr_id)
        .map_err(|e| CliError::Forge(e.to_string()))?;
    if view.head != head {
        return Err(CliError::Forge(
            "review head differs from exact proposal head".into(),
        ));
    }
    if findings_text.is_some_and(|t| t.len() > mini_forge::MAX_FINDINGS_BYTES) {
        return Err(CliError::Usage("findings exceed 4096 bytes".into()));
    }
    let seq = sequence::next(home, store_path)?;
    let obj = approve(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &pr_id,
        &head,
        approve_it,
        sequence::now_ms(),
        seq,
    )
    .map_err(|e| CliError::Forge(e.to_string()))?;

    let mut result = CommandResult::new("")
        .field("review_id", JsonValue::str(obj.id().as_str()))
        .field("pr_id", JsonValue::str(pr_id.as_str()))
        .field("head", JsonValue::str(head.as_str()))
        .field("approved", JsonValue::Bool(approve_it));
    let mut out = format!(
        "review recorded: {} ({})",
        obj.id().as_str(),
        if approve_it {
            "approve"
        } else {
            "request changes"
        }
    );

    if let Some(text) = findings_text {
        let seq2 = sequence::next(home, store_path)?;
        let findings_obj = record_findings(
            &mut store,
            &identity.human_did(),
            &identity.device,
            &pr_id,
            &head,
            text,
            sequence::now_ms(),
            seq2,
        )
        .map_err(|e| CliError::Forge(e.to_string()))?;
        result = result.field("findings_id", JsonValue::str(findings_obj.id().as_str()));
        out.push_str(&format!(
            "\nfindings recorded: {}",
            findings_obj.id().as_str()
        ));
    }

    result.human = out;
    Ok(result)
}

/// `mini pr merge <project> --pr <id> [--prev <chain-entry-id>]`
pub fn merge_pr(
    home: &Path,
    store_path: &Path,
    project_ref: &str,
    pr_ref: &str,
    prev_ref: Option<&str>,
) -> Result<CommandResult> {
    let identity = crate::identity::load(home)?;
    let mut store = open_store(store_path)?;
    let project_id = project_alias::resolve(home, project_ref)?;
    let pr_id = ObjectId::parse(pr_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let prev = match prev_ref {
        Some(p) => ObjectId::parse(p).map_err(|e| CliError::Object(e.to_string()))?,
        None => default_base(&store, home, &identity, &project_id)?,
    };

    let oracle = build_oracle(home, &identity)?;
    let view = mini_forge::inspect_proposal(&store, &oracle, &pr_id)
        .map_err(|e| CliError::Forge(e.to_string()))?;
    let state = resolve_project(&store, &oracle, &project_id)
        .map_err(|e| CliError::Forge(e.to_string()))?;
    if view.project != project_id || view.base != prev || prev != state.tip || view.stale {
        return Err(CliError::Forge(
            "proposal project/base does not match current governance tip".into(),
        ));
    }
    if view.approvals < view.required_approvals
        || !state.policy.maintainers.contains(&identity.human_did())
    {
        return Err(CliError::Forge(
            "merge requires a current maintainer and exact-head approval quorum".into(),
        ));
    }
    let seq = sequence::next(home, store_path)?;
    let obj = merge(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &project_id,
        &prev,
        &pr_id,
        sequence::now_ms(),
        seq,
    )
    .map_err(|e| CliError::Forge(e.to_string()))?;

    let oracle = build_oracle(home, &identity)?;
    let state = resolve_project(&store, &oracle, &project_id)
        .map_err(|e| CliError::Forge(e.to_string()))?;
    let applied = state.tip == *obj.id();
    Ok(CommandResult::new(format!(
        "merge entry recorded: {} (applied: {applied} -- resolve_project sees {} entries)",
        obj.id().as_str(),
        state.entries
    ))
    .field("merge_id", JsonValue::str(obj.id().as_str()))
    .field("applied", JsonValue::Bool(applied))
    .field("tip", JsonValue::str(state.tip.as_str())))
}

/// `mini pr ai-assisted <pr-id> --owner <did>`
pub fn set_ai_assisted(
    home: &Path,
    store_path: &Path,
    pr_ref: &str,
    owner: Did,
) -> Result<CommandResult> {
    let identity = crate::identity::load(home)?;
    let mut store = open_store(store_path)?;
    let pr_id = ObjectId::parse(pr_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let seq = sequence::next(home, store_path)?;
    let obj = declare_ai_assistance(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &pr_id,
        true,
        Some(&owner),
        sequence::now_ms(),
        seq,
    )
    .map_err(|e| CliError::Forge(e.to_string()))?;
    Ok(
        CommandResult::new(format!("declared AI-assisted, owner {}", owner.as_str()))
            .field("declaration_id", JsonValue::str(obj.id().as_str()))
            .field("human_owner", JsonValue::str(owner.as_str())),
    )
}

/// `mini pr findings <pr-id>`
pub fn show_findings(home: &Path, store_path: &Path, pr_ref: &str) -> Result<CommandResult> {
    let identity = crate::identity::load(home)?;
    let store = open_store(store_path)?;
    let pr_id = ObjectId::parse(pr_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let oracle = build_oracle(home, &identity)?;

    let mut out = String::new();
    if let Some(decl) =
        ai_assistance(&store, &oracle, &pr_id).map_err(|e| CliError::Forge(e.to_string()))?
    {
        out.push_str(&format!(
            "AI-assisted: {}, owner: {}\n",
            decl.ai_assisted,
            decl.human_owner
                .map(|d| d.as_str().to_string())
                .unwrap_or_default()
        ));
    }
    for f in list_findings(&store, &oracle, &pr_id).map_err(|e| CliError::Forge(e.to_string()))? {
        out.push_str(&format!("[{}] {}\n", f.reviewer.as_str(), f.text));
    }
    let structured = show_pr(home, store_path, pr_ref)?;
    Ok(CommandResult {
        human: out,
        fields: structured.fields,
    })
}

fn proposal_json(v: &mini_forge::ProposalView) -> JsonValue {
    JsonValue::Object(vec![
        ("pr_id".into(), JsonValue::str(v.id.as_str())),
        ("project_id".into(), JsonValue::str(v.project.as_str())),
        ("author".into(), JsonValue::str(v.author.as_str())),
        ("branch".into(), JsonValue::str(&v.branch)),
        ("title".into(), JsonValue::str(&v.title)),
        ("head".into(), JsonValue::str(v.head.as_str())),
        ("base".into(), JsonValue::str(v.base.as_str())),
        ("current_tip".into(), JsonValue::str(v.current_tip.as_str())),
        (
            "current_branch_head".into(),
            JsonValue::opt_str(v.current_branch_head.as_ref().map(|h| h.as_str())),
        ),
        ("approvals".into(), JsonValue::num(v.approvals as u64)),
        (
            "required_approvals".into(),
            JsonValue::num(v.required_approvals as u64),
        ),
        ("stale".into(), JsonValue::Bool(v.stale)),
        ("forks_detected".into(), JsonValue::Bool(v.forks_detected)),
        (
            "quorum_met".into(),
            JsonValue::Bool(v.approvals >= v.required_approvals),
        ),
    ])
}

pub fn list_prs(home: &Path, store_path: &Path, project_ref: &str) -> Result<CommandResult> {
    let identity = crate::identity::load(home)?;
    let store = open_store(store_path)?;
    let oracle = build_oracle(home, &identity)?;
    let project = project_alias::resolve(home, project_ref)?;
    let views = mini_forge::list_proposals(&store, &oracle, &project)
        .map_err(|e| CliError::Forge(e.to_string()))?;
    let human = views
        .iter()
        .map(|v| {
            format!(
                "{} {} [{}] approvals {}/{}{}\n",
                v.id.as_str(),
                v.title,
                v.branch,
                v.approvals,
                v.required_approvals,
                if v.stale { " (stale base)" } else { "" }
            )
        })
        .collect::<String>();
    Ok(CommandResult::new(human).field(
        "proposals",
        JsonValue::Array(views.iter().map(proposal_json).collect()),
    ))
}

pub fn show_pr(home: &Path, store_path: &Path, pr_ref: &str) -> Result<CommandResult> {
    let identity = crate::identity::load(home)?;
    let store = open_store(store_path)?;
    let oracle = build_oracle(home, &identity)?;
    let id = ObjectId::parse(pr_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let v = mini_forge::inspect_proposal(&store, &oracle, &id)
        .map_err(|e| CliError::Forge(e.to_string()))?;
    let reviews = mini_forge::proposal_reviews(&store, &oracle, &id)
        .map_err(|e| CliError::Forge(e.to_string()))?;
    let reviews_json = JsonValue::Array(
        reviews
            .into_iter()
            .map(|r| {
                JsonValue::Object(vec![
                    ("review_id".into(), JsonValue::str(r.id.as_str())),
                    ("reviewer".into(), JsonValue::str(r.reviewer.as_str())),
                    ("head".into(), JsonValue::str(r.reviewed_head.as_str())),
                    ("approved".into(), JsonValue::Bool(r.approved)),
                    (
                        "eligible_approval".into(),
                        JsonValue::Bool(r.eligible_approval),
                    ),
                ])
            })
            .collect(),
    );
    let findings =
        list_findings(&store, &oracle, &id).map_err(|e| CliError::Forge(e.to_string()))?;
    let declaration =
        ai_assistance(&store, &oracle, &id).map_err(|e| CliError::Forge(e.to_string()))?;
    let mut human = format!("{}\n{} [{}]\nhead: {}\nbase: {}\ncurrent tip: {}\napprovals: {}/{}; stale: {}; forks: {}\n",
        id.as_str(), v.title, v.branch, v.head.as_str(), v.base.as_str(), v.current_tip.as_str(),
        v.approvals, v.required_approvals, v.stale, v.forks_detected);
    let findings_json = findings
        .into_iter()
        .map(|f| {
            human.push_str(&format!(
                "[{} @ {}] {}\n",
                f.reviewer.as_str(),
                f.reviewed_head.as_str(),
                f.text
            ));
            JsonValue::Object(vec![
                ("reviewer".into(), JsonValue::str(f.reviewer.as_str())),
                ("head".into(), JsonValue::str(f.reviewed_head.as_str())),
                ("text".into(), JsonValue::str(f.text)),
            ])
        })
        .collect();
    Ok(CommandResult::new(human)
        .field("proposal", proposal_json(&v))
        .field("reviews", reviews_json)
        .field("findings", JsonValue::Array(findings_json))
        .field(
            "ai_assisted",
            declaration
                .as_ref()
                .map(|d| JsonValue::Bool(d.ai_assisted))
                .unwrap_or(JsonValue::Null),
        )
        .field(
            "human_owner",
            JsonValue::opt_str(
                declaration
                    .and_then(|d| d.human_owner)
                    .map(|d| d.as_str().to_string()),
            ),
        ))
}

/// Compare the exact proposed head against the current governed branch. File
/// bytes are returned only as bounded UTF-8; binary/large files retain sizes
/// and explicit content-omission indicators for callers to fetch separately.
pub fn diff_pr(home: &Path, store_path: &Path, pr_ref: &str) -> Result<CommandResult> {
    use std::collections::BTreeMap;
    let identity = crate::identity::load(home)?;
    let store = open_store(store_path)?;
    let oracle = build_oracle(home, &identity)?;
    let id = ObjectId::parse(pr_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let view = mini_forge::inspect_proposal(&store, &oracle, &id)
        .map_err(|e| CliError::Forge(e.to_string()))?;
    let new: BTreeMap<_, _> = mini_forge::checkout(&store, &view.head)
        .map_err(|e| CliError::Forge(e.to_string()))?
        .into_iter()
        .collect();
    let old: BTreeMap<_, _> = match &view.current_branch_head {
        Some(head) => mini_forge::checkout(&store, head)
            .map_err(|e| CliError::Forge(e.to_string()))?
            .into_iter()
            .collect(),
        None => BTreeMap::new(),
    };
    let paths: std::collections::BTreeSet<_> = old.keys().chain(new.keys()).collect();
    let mut files = Vec::new();
    let mut human = format!(
        "current governed branch -> {} (stale base: {})\n",
        view.head.as_str(),
        view.stale
    );
    let mut text_budget = 1024 * 1024usize;
    for path in paths {
        let before = old.get(path);
        let after = new.get(path);
        if before == after {
            continue;
        }
        let status = if before.is_none() {
            "added"
        } else if after.is_none() {
            "deleted"
        } else {
            "modified"
        };
        human.push_str(&format!("{status} {path}\n"));
        let mut content = |bytes: Option<&Vec<u8>>| {
            bytes.and_then(|b| {
                if b.len() > 256 * 1024 || b.len() > text_budget || b.contains(&0) {
                    return None;
                }
                let text = std::str::from_utf8(b).ok()?;
                text_budget -= b.len();
                Some(text.to_string())
            })
        };
        let before_text = content(before);
        let after_text = content(after);
        files.push(JsonValue::Object(vec![
            ("path".into(), JsonValue::str(path)),
            ("status".into(), JsonValue::str(status)),
            (
                "old_bytes".into(),
                JsonValue::num(before.map_or(0, |b| b.len()) as u64),
            ),
            (
                "new_bytes".into(),
                JsonValue::num(after.map_or(0, |b| b.len()) as u64),
            ),
            (
                "old_content_omitted".into(),
                JsonValue::Bool(before.is_some() && before_text.is_none()),
            ),
            (
                "new_content_omitted".into(),
                JsonValue::Bool(after.is_some() && after_text.is_none()),
            ),
            ("old_text".into(), JsonValue::opt_str(before_text)),
            ("new_text".into(), JsonValue::opt_str(after_text)),
        ]));
    }
    Ok(CommandResult::new(human)
        .field("proposal", proposal_json(&view))
        .field("files", JsonValue::Array(files)))
}

/// Record AI/human review evidence without emitting any approval verdict.
/// Callers must name the exact head they inspected; findings grant no quorum.
pub fn add_findings(
    home: &Path,
    store_path: &Path,
    pr_ref: &str,
    head_ref: &str,
    text: &str,
) -> Result<CommandResult> {
    let identity = crate::identity::load(home)?;
    let mut store = open_store(store_path)?;
    let oracle = build_oracle(home, &identity)?;
    let id = ObjectId::parse(pr_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let head = ObjectId::parse(head_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let view = mini_forge::inspect_proposal(&store, &oracle, &id)
        .map_err(|e| CliError::Forge(e.to_string()))?;
    if head != view.head {
        return Err(CliError::Forge(
            "findings head differs from exact proposal head".into(),
        ));
    }
    if text.is_empty() || text.len() > mini_forge::MAX_FINDINGS_BYTES {
        return Err(CliError::Usage(
            "findings must contain 1..4096 bytes".into(),
        ));
    }
    let seq = sequence::next(home, store_path)?;
    let obj = record_findings(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &id,
        &head,
        text,
        sequence::now_ms(),
        seq,
    )
    .map_err(|e| CliError::Forge(e.to_string()))?;
    Ok(CommandResult::new(format!(
        "findings recorded: {} (no approval verdict)",
        obj.id().as_str()
    ))
    .field("findings_id", JsonValue::str(obj.id().as_str()))
    .field("pr_id", JsonValue::str(id.as_str()))
    .field("head", JsonValue::str(head.as_str())))
}
