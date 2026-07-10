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
use crate::project as project_alias;
use crate::sequence;
use crate::store::{build_oracle, open_store};

/// Resolve the chain position a new PR/merge should build against: the
/// project's current governed tip if resolvable, else the project id
/// itself (the genesis, for the very first PR).
fn default_base(
    store: &mini_store::Store<mini_store::FsBackend>,
    home: &Path,
    identity: &crate::identity::Identity,
    project_id: &ObjectId,
) -> ObjectId {
    build_oracle(home, identity)
        .ok()
        .and_then(|oracle| resolve_project(store, &oracle, project_id).ok())
        .map(|s| s.tip)
        .unwrap_or_else(|| project_id.clone())
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
) -> Result<String> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let project_id = project_alias::resolve(home, project_ref)?;
    let head = ObjectId::parse(head_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let base = match base_ref {
        Some(b) => ObjectId::parse(b).map_err(|e| CliError::Object(e.to_string()))?,
        None => default_base(&store, home, &identity, &project_id),
    };

    let seq = sequence::next(home)?;
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
    Ok(format!("PR proposed: {}", obj.id().as_str()))
}

/// `mini pr approve <project> --pr <id> --head <commit-id> [--reject] [--findings <text>]`
pub fn approve_pr(
    home: &Path,
    store_path: &Path,
    pr_ref: &str,
    head_ref: &str,
    approve_it: bool,
    findings_text: Option<&str>,
) -> Result<String> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let pr_id = ObjectId::parse(pr_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let head = ObjectId::parse(head_ref).map_err(|e| CliError::Object(e.to_string()))?;

    let seq = sequence::next(home)?;
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
        let seq2 = sequence::next(home)?;
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
        out.push_str(&format!(
            "\nfindings recorded: {}",
            findings_obj.id().as_str()
        ));
    }

    Ok(out)
}

/// `mini pr merge <project> --pr <id> [--prev <chain-entry-id>]`
pub fn merge_pr(
    home: &Path,
    store_path: &Path,
    project_ref: &str,
    pr_ref: &str,
    prev_ref: Option<&str>,
) -> Result<String> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let project_id = project_alias::resolve(home, project_ref)?;
    let pr_id = ObjectId::parse(pr_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let prev = match prev_ref {
        Some(p) => ObjectId::parse(p).map_err(|e| CliError::Object(e.to_string()))?,
        None => default_base(&store, home, &identity, &project_id),
    };

    let seq = sequence::next(home)?;
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
    Ok(format!(
        "merge entry recorded: {} (applied: {applied} -- resolve_project sees {} entries)",
        obj.id().as_str(),
        state.entries
    ))
}

/// `mini pr ai-assisted <pr-id> --owner <did>`
pub fn set_ai_assisted(home: &Path, store_path: &Path, pr_ref: &str, owner: Did) -> Result<String> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let pr_id = ObjectId::parse(pr_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let seq = sequence::next(home)?;
    declare_ai_assistance(
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
    Ok(format!("declared AI-assisted, owner {}", owner.as_str()))
}

/// `mini pr findings <pr-id>`
pub fn show_findings(home: &Path, store_path: &Path, pr_ref: &str) -> Result<String> {
    let identity = crate::identity::load_or_init(home)?;
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
    Ok(out)
}
