//! `mini pr ...` — propose/approve/merge, over `mini_forge::governance`'s
//! already-real proposal/review/merge object model (predates the audit
//! this crate responds to; see D-0066's correction in `docs/DECISION_LOG.md`).

use std::path::{Path, PathBuf};

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

struct ProposalContext {
    home: PathBuf,
    store_path: PathBuf,
    identity: crate::identity::Identity,
    store: mini_store::Store<mini_store::FsBackend>,
    project: ObjectId,
    branch: String,
    title: String,
    head: ObjectId,
    base: ObjectId,
    fields: Vec<(&'static str, JsonValue)>,
    plan_id: String,
}
#[allow(clippy::too_many_arguments)]
fn capture_proposal(
    home: &Path,
    store_path: &Path,
    project_ref: &str,
    branch: &str,
    title: &str,
    head_ref: &str,
    base_ref: Option<&str>,
) -> Result<ProposalContext> {
    // Match the protocol's valid_name and payload limits before any allocation
    // of sequence numbers or signed objects.
    if branch.is_empty()
        || branch.len() > 64
        || branch.len() > mini_forge::MAX_VERSION_BYTES
        || branch == "."
        || branch == ".."
        || !branch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err(CliError::Usage("invalid proposal branch name".into()));
    }
    if title.len() > mini_forge::MAX_TITLE_BYTES {
        return Err(CliError::Usage(
            "proposal title exceeds protocol limit".into(),
        ));
    }
    let home = crate::repo::reviewed_directory(home)?;
    let store_path = crate::repo::reviewed_directory(store_path)?;
    crate::repo::reviewed_directory(&store_path.join("blobs"))?;
    crate::repo::reviewed_directory(&store_path.join("meta"))?;
    let identity = crate::identity::load(&home)?;
    let store = open_store(&store_path)?;
    let project = project_alias::resolve(&home, project_ref)?;
    let oracle = build_oracle(&home, &identity)?;
    let state =
        resolve_project(&store, &oracle, &project).map_err(|e| CliError::Forge(e.to_string()))?;
    let base = match base_ref {
        Some(id) => ObjectId::parse(id).map_err(|e| CliError::Object(e.to_string()))?,
        None => state.tip.clone(),
    };
    if base != state.tip {
        return Err(CliError::Forge(
            "planned proposal base must equal the current verified governance tip".into(),
        ));
    }
    let head = ObjectId::parse(head_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let commit = store
        .get(&head)
        .map_err(|e| CliError::Store(e.to_string()))?;
    if commit.object_type != mini_objects::ObjectType::COMMIT {
        return Err(CliError::Forge("proposal head must be a commit".into()));
    }
    use mini_forge::IdentityOracle;
    let root_kel = oracle
        .kel(&commit.author_human)
        .ok_or_else(|| CliError::Forge("proposal head author root KEL is not trusted".into()))?;
    let device_kel = oracle
        .kel(&commit.author_device)
        .ok_or_else(|| CliError::Forge("proposal head author device KEL is not trusted".into()))?;
    mini_objects::verify_provenance(&commit, root_kel, device_kel)
        .map_err(|e| CliError::Forge(e.to_string()))?;
    // The existing checkout parser enforces exact commit/tree/closure shape,
    // mode support and resource bounds. It does NOT certify every child's author.
    let files = mini_forge::checkout(&store, &head).map_err(|e| CliError::Forge(e.to_string()))?;
    let file_count = files.len() as u64;
    let byte_count: u64 = files.iter().map(|(_, bytes)| bytes.len() as u64).sum();
    drop(files);
    let tree = commit
        .links
        .iter()
        .find(|link| link.rel == "tree")
        .ok_or_else(|| CliError::Forge("proposal head has no canonical tree".into()))?
        .target
        .clone();
    let state_json = JsonValue::Object(vec![
        ("tip".into(), JsonValue::str(state.tip.as_str())),
        ("entries".into(), JsonValue::num(state.entries as u64)),
        (
            "forks_detected".into(),
            JsonValue::Bool(state.forks_detected),
        ),
        (
            "min_approvals".into(),
            JsonValue::num(state.policy.min_approvals as u64),
        ),
        (
            "maintainers".into(),
            JsonValue::strs(state.policy.maintainers.iter().map(|did| did.as_str())),
        ),
        (
            "branches".into(),
            JsonValue::Array(
                state
                    .branches
                    .iter()
                    .map(|(name, head)| {
                        JsonValue::Object(vec![
                            ("branch".into(), JsonValue::str(name)),
                            ("head".into(), JsonValue::str(head.as_str())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
    .render();
    let mut state_hash = blake3::Hasher::new();
    state_hash.update(b"MINI/PROPOSAL-GOVERNANCE-STATE/v1\0");
    state_hash.update(state_json.as_bytes());
    let fields = vec![
        (
            "home",
            JsonValue::str(home.to_str().expect("reviewed UTF-8 home")),
        ),
        (
            "store",
            JsonValue::str(store_path.to_str().expect("reviewed UTF-8 store")),
        ),
        ("human_did", JsonValue::str(identity.human_did().as_str())),
        ("device_did", JsonValue::str(identity.device_did().as_str())),
        ("project_id", JsonValue::str(project.as_str())),
        ("branch", JsonValue::str(branch)),
        ("title", JsonValue::str(title)),
        ("head", JsonValue::str(head.as_str())),
        ("head_tree", JsonValue::str(tree.as_str())),
        (
            "head_author_human",
            JsonValue::str(commit.author_human.as_str()),
        ),
        (
            "head_author_device",
            JsonValue::str(commit.author_device.as_str()),
        ),
        ("base", JsonValue::str(base.as_str())),
        ("governance_tip", JsonValue::str(state.tip.as_str())),
        (
            "governance_state_digest",
            JsonValue::str(state_hash.finalize().to_hex().to_string()),
        ),
        ("forks_detected", JsonValue::Bool(state.forks_detected)),
        ("head_files", JsonValue::num(file_count)),
        ("head_bytes", JsonValue::num(byte_count)),
    ];
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"MINI/PROPOSAL-PLAN/v1\0");
    hasher.update(
        JsonValue::Object(
            fields
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect(),
        )
        .render()
        .as_bytes(),
    );
    let plan_id = hasher.finalize().to_hex().to_string();
    Ok(ProposalContext {
        home,
        store_path,
        identity,
        store,
        project,
        branch: branch.into(),
        title: title.into(),
        head,
        base,
        fields,
        plan_id,
    })
}
/// Review an exact verified head and current proposal metadata without creating
/// objects or sequence state. Existing identity lock metadata is not authority.
#[allow(clippy::too_many_arguments)]
pub fn preview_proposal(
    home: &Path,
    store: &Path,
    project: &str,
    branch: &str,
    title: &str,
    head: &str,
    base: Option<&str>,
) -> Result<CommandResult> {
    let context = capture_proposal(home, store, project, branch, title, head, base)?;
    let human = format!(
        "proposal plan: {}\n{}",
        context.plan_id,
        context
            .fields
            .iter()
            .map(|(key, value)| format!("{key}: {}\n", value.render()))
            .collect::<String>()
    );
    let mut result = CommandResult::new(human).field("plan_id", JsonValue::str(context.plan_id));
    result.fields.extend(context.fields);
    Ok(result)
}
/// Confirmation binds one locally captured proposal context; no global CAS,
/// approval, merge, canonicalization or child-authority claim is introduced.
#[allow(clippy::too_many_arguments)]
pub fn propose_planned(
    home: &Path,
    store: &Path,
    project: &str,
    branch: &str,
    title: &str,
    head: &str,
    base: Option<&str>,
    confirmation: &str,
) -> Result<CommandResult> {
    let mut context = capture_proposal(home, store, project, branch, title, head, base)?;
    if context.plan_id != confirmation {
        return Err(CliError::Usage(
            "proposal plan changed or confirmation does not match; review a fresh proposal plan"
                .into(),
        ));
    }
    record_planned_proposal(&mut context)
}
fn record_planned_proposal(context: &mut ProposalContext) -> Result<CommandResult> {
    let sequence = sequence::next(&context.home, &context.store_path)?;
    let object = propose(
        &mut context.store,
        &context.identity.human_did(),
        &context.identity.device,
        &context.project,
        &context.branch,
        &context.title,
        &context.head,
        &context.base,
        sequence::now_ms(),
        sequence,
    )
    .map_err(|e| CliError::Forge(e.to_string()))?;
    let mut result = CommandResult::new(format!("PR proposed: {}", object.id().as_str()))
        .field("pr_id", JsonValue::str(object.id().as_str()))
        .field("plan_id", JsonValue::str(&context.plan_id));
    result.fields.extend(context.fields.clone());
    Ok(result)
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

#[cfg(test)]
mod proposal_plan_tests {
    use super::*;
    struct World {
        _dir: tempfile::TempDir,
        home: PathBuf,
        store: PathBuf,
        first: ObjectId,
        second: ObjectId,
    }
    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let store = dir.path().join("store");
        crate::repo::init(&home, &store, "demo", vec![], 1).unwrap();
        crate::repo::init(&home, &store, "other", vec![], 1).unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("code.rs"), "first").unwrap();
        let one =
            crate::repo::commit_workspace(&home, &store, "demo", "main", "one", &source).unwrap();
        std::fs::write(source.join("code.rs"), "second").unwrap();
        let two =
            crate::repo::commit_workspace(&home, &store, "demo", "main", "two", &source).unwrap();
        let first = ObjectId::parse(one.human.strip_prefix("commit created: ").unwrap()).unwrap();
        let second = ObjectId::parse(two.human.strip_prefix("commit created: ").unwrap()).unwrap();
        World {
            _dir: dir,
            home,
            store,
            first,
            second,
        }
    }
    fn id(result: &CommandResult, name: &str) -> String {
        match &result
            .fields
            .iter()
            .find(|(key, _)| *key == name)
            .unwrap()
            .1
        {
            JsonValue::String(value) => value.clone(),
            _ => panic!("expected string field"),
        }
    }
    #[test]
    fn proposal_plan_changes_require_fresh_confirmation_before_any_writes() {
        let w = world();
        let preview = preview_proposal(
            &w.home,
            &w.store,
            "demo",
            "main",
            "title",
            w.first.as_str(),
            None,
        )
        .unwrap();
        let plan = id(&preview, "plan_id");
        let sequence = std::fs::read(w.home.join("sequence")).unwrap();
        let count = open_store(&w.store).unwrap().all_ids().unwrap().len();
        for (project, branch, title, head) in [
            ("demo", "feature", "title", &w.first),
            ("demo", "main", "new title", &w.first),
            ("other", "main", "title", &w.first),
            ("demo", "main", "title", &w.second),
        ] {
            assert!(propose_planned(
                &w.home,
                &w.store,
                project,
                branch,
                title,
                head.as_str(),
                None,
                &plan
            )
            .is_err());
        }
        assert!(propose_planned(
            &w.home,
            &w.store,
            "demo",
            "main",
            "title",
            w.first.as_str(),
            Some(w.second.as_str()),
            &plan
        )
        .is_err());
        assert!(propose_planned(
            &w.home,
            &w.store,
            "demo",
            "main",
            "title",
            w.first.as_str(),
            None,
            "yes"
        )
        .is_err());
        assert!(preview_proposal(
            &w.home,
            &w.store,
            "demo",
            "bad/branch",
            "title",
            w.first.as_str(),
            None
        )
        .is_err());
        assert!(preview_proposal(
            &w.home,
            &w.store,
            "demo",
            "main",
            &"x".repeat(mini_forge::MAX_TITLE_BYTES + 1),
            w.first.as_str(),
            None
        )
        .is_err());
        let project = project_alias::resolve(&w.home, "demo").unwrap();
        assert!(preview_proposal(
            &w.home,
            &w.store,
            "demo",
            "main",
            "title",
            project.as_str(),
            None
        )
        .is_err());
        assert_eq!(
            open_store(&w.store).unwrap().all_ids().unwrap().len(),
            count
        );
        assert_eq!(std::fs::read(w.home.join("sequence")).unwrap(), sequence);
        let proposal = propose_planned(
            &w.home,
            &w.store,
            "demo",
            "main",
            "title",
            w.first.as_str(),
            None,
            &plan,
        )
        .unwrap();
        assert_eq!(id(&proposal, "plan_id"), plan);
        let identity = crate::identity::load(&w.home).unwrap();
        let view = mini_forge::inspect_proposal(
            &open_store(&w.store).unwrap(),
            &build_oracle(&w.home, &identity).unwrap(),
            &ObjectId::parse(&id(&proposal, "pr_id")).unwrap(),
        )
        .unwrap();
        assert_eq!(view.head, w.first);
        assert_eq!(view.base, project);
        assert_eq!(view.title, "title");
    }
    fn outsider() -> (did_mini::Controller, did_mini::Controller) {
        let mut root =
            did_mini::Controller::incept_single_from_seeds(&[71; 32], &[72; 32]).unwrap();
        let device = did_mini::Controller::incept_device_single_from_seeds(
            &root.did(),
            &[73; 32],
            &[74; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), did_mini::Capabilities::primary())
            .unwrap();
        (root, device)
    }
    #[test]
    fn proposal_preview_requires_trusted_head_provenance_and_existing_backend() {
        let w = world();
        let (root, device) = outsider();
        let mut store = open_store(&w.store).unwrap();
        let original = store.get(&w.first).unwrap();
        let tree = original
            .links
            .iter()
            .find(|link| link.rel == "tree")
            .unwrap()
            .target
            .clone();
        let untrusted = mini_forge::commit(
            &mut store,
            &root.did(),
            &device,
            "outsider",
            &tree,
            &[],
            0,
            0,
        )
        .unwrap();
        let count = store.all_ids().unwrap().len();
        drop(store);
        let sequence = std::fs::read(w.home.join("sequence")).unwrap();
        assert!(preview_proposal(
            &w.home,
            &w.store,
            "demo",
            "main",
            "title",
            untrusted.id().as_str(),
            None
        )
        .is_err());
        assert_eq!(
            open_store(&w.store).unwrap().all_ids().unwrap().len(),
            count
        );
        assert_eq!(std::fs::read(w.home.join("sequence")).unwrap(), sequence);
        crate::store::trust_kel(&w.home, root.kel()).unwrap();
        crate::store::trust_kel(&w.home, device.kel()).unwrap();
        let preview = preview_proposal(
            &w.home,
            &w.store,
            "demo",
            "main",
            "title",
            untrusted.id().as_str(),
            None,
        )
        .unwrap();
        assert_eq!(id(&preview, "head_author_human"), root.did().as_str());
        let missing = w._dir.path().join("missing-store");
        assert!(preview_proposal(
            &w.home,
            &missing,
            "demo",
            "main",
            "title",
            w.first.as_str(),
            None
        )
        .is_err());
        assert!(!missing.exists());
        let partial = w._dir.path().join("partial-store");
        std::fs::create_dir(&partial).unwrap();
        assert!(preview_proposal(
            &w.home,
            &partial,
            "demo",
            "main",
            "title",
            w.first.as_str(),
            None
        )
        .is_err());
        assert!(!partial.join("blobs").exists() && !partial.join("meta").exists());
    }
    #[test]
    fn governed_base_changes_invalidate_plan_but_captured_proposal_is_not_re_resolved() {
        let w = world();
        let (root, device) = outsider();
        crate::store::trust_kel(&w.home, root.kel()).unwrap();
        crate::store::trust_kel(&w.home, device.kel()).unwrap();
        let preview = preview_proposal(
            &w.home,
            &w.store,
            "demo",
            "main",
            "title",
            w.first.as_str(),
            None,
        )
        .unwrap();
        let mut captured = capture_proposal(
            &w.home,
            &w.store,
            "demo",
            "main",
            "captured title",
            w.first.as_str(),
            None,
        )
        .unwrap();
        let project = captured.project.clone();
        let old_base = captured.base.clone();
        let owner = crate::identity::load(&w.home).unwrap();
        let mut store = open_store(&w.store).unwrap();
        let proposed = propose(
            &mut store,
            &root.did(),
            &device,
            &project,
            "main",
            "upstream",
            &w.second,
            &old_base,
            0,
            0,
        )
        .unwrap();
        approve(
            &mut store,
            &owner.human_did(),
            &owner.device,
            proposed.id(),
            &w.second,
            true,
            0,
            0,
        )
        .unwrap();
        let entry = merge(
            &mut store,
            &owner.human_did(),
            &owner.device,
            &project,
            &old_base,
            proposed.id(),
            0,
            0,
        )
        .unwrap();
        assert_eq!(
            resolve_project(&store, &build_oracle(&w.home, &owner).unwrap(), &project)
                .unwrap()
                .tip,
            *entry.id()
        );
        let count = store.all_ids().unwrap().len();
        let sequence = std::fs::read(w.home.join("sequence")).unwrap();
        drop(store);
        assert!(propose_planned(
            &w.home,
            &w.store,
            "demo",
            "main",
            "title",
            w.first.as_str(),
            None,
            &id(&preview, "plan_id")
        )
        .is_err());
        assert!(preview_proposal(
            &w.home,
            &w.store,
            "demo",
            "main",
            "title",
            w.first.as_str(),
            Some(old_base.as_str())
        )
        .is_err());
        assert_eq!(
            open_store(&w.store).unwrap().all_ids().unwrap().len(),
            count
        );
        assert_eq!(std::fs::read(w.home.join("sequence")).unwrap(), sequence);
        // This simulates a separate writer racing after confirmation: the
        // signing helper must retain the reviewed base/title/head, not adopt
        // newly resolved metadata behind the owner's plan.
        let result = record_planned_proposal(&mut captured).unwrap();
        let view = mini_forge::inspect_proposal(
            &open_store(&w.store).unwrap(),
            &build_oracle(&w.home, &owner).unwrap(),
            &ObjectId::parse(&id(&result, "pr_id")).unwrap(),
        )
        .unwrap();
        assert_eq!(view.base, old_base);
        assert_eq!(view.title, "captured title");
        assert_eq!(view.head, w.first);
        assert!(view.stale);
    }
}
