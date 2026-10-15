//! Argument parsing and dispatch. Deliberately hand-rolled rather than
//! pulling in a CLI-parsing dependency: the command surface is small and
//! fixed, and this workspace's own convention (see `mini-spacetime`,
//! `mini-porep`) is to avoid a dependency where a few dozen lines of plain
//! Rust do the job — the same reasoning, applied to tooling instead of
//! cryptography.

use std::path::{Path, PathBuf};

use did_mini::Did;

use crate::error::{CliError, Result};
use crate::json::CommandResult;
use crate::{
    beta, build, coordination, identity, installer, intake, keystone, pr, provenance, release,
    repo, store, sync,
};

fn extract_flag(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == flag)?;
    if pos + 1 >= args.len() {
        return None;
    }
    args.remove(pos); // the flag itself
    Some(args.remove(pos)) // its value, now at the same position
}

fn extract_flag_multi(args: &mut Vec<String>, flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(v) = extract_flag(args, flag) {
        out.push(v);
    }
    out
}

fn extract_bool_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(pos) = args.iter().position(|a| a == flag) {
        args.remove(pos);
        true
    } else {
        false
    }
}

fn default_home() -> PathBuf {
    std::env::var_os("MININET_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs_home().map(|h| h.join(".mininet")))
        .unwrap_or_else(|| PathBuf::from(".mininet"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Run one CLI invocation. `raw_args` is everything after the program
/// name. Returns the human-readable output to print on success -- or, if
/// `--json` is present, a single-line JSON document for the commands that
/// support it (`build`/`release`/`provenance`/`installer`; see
/// `crate::json`'s module docs). `--json` on any other command is a clean
/// usage error rather than a silently-ignored flag, since a scripting
/// caller trusting `--json` to always produce parseable output must never
/// get human text back without being told.
pub fn run(raw_args: &[String]) -> Result<String> {
    let mut args: Vec<String> = raw_args.to_vec();
    let json = extract_bool_flag(&mut args, "--json");
    let home = extract_flag(&mut args, "--home")
        .map(PathBuf::from)
        .unwrap_or_else(default_home);
    let store_path = extract_flag(&mut args, "--store")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("store"));

    let _maintenance_lease = crate::maintenance::lease(&home)?;
    dispatch(&home, &store_path, args, json)
}

fn reject_json(json: bool, verb: &str) -> Result<()> {
    if json {
        Err(CliError::Usage(format!(
            "--json is not yet supported for `mini {verb}` commands"
        )))
    } else {
        Ok(())
    }
}

fn dispatch(home: &Path, store_path: &Path, mut args: Vec<String>, json: bool) -> Result<String> {
    if args.is_empty() {
        return Err(CliError::Usage("no command given".to_string()));
    }
    let verb = args.remove(0);
    match verb.as_str() {
        "identity" => {
            reject_json(json, "identity")?;
            dispatch_identity(home, args)
        }
        "kel" => {
            reject_json(json, "kel")?;
            dispatch_kel(home, args)
        }
        "repo" => {
            reject_json(json, "repo")?;
            dispatch_repo(home, store_path, args)
        }
        "pr" => {
            reject_json(json, "pr")?;
            dispatch_pr(home, store_path, args)
        }
        "team" => dispatch_team(home, store_path, args, json),
        "task" => dispatch_task(home, store_path, args, json),
        "beta" => dispatch_beta(home, store_path, args, json),
        "sync" => {
            reject_json(json, "sync")?;
            dispatch_sync(home, store_path, args)
        }
        "keystone" => dispatch_keystone(home, args, json),
        "intake" => {
            reject_json(json, "intake")?;
            dispatch_intake(home, store_path, args)
        }
        "build" => dispatch_build(args, json),
        "release" => dispatch_release(home, store_path, args, json),
        "provenance" => dispatch_provenance(home, store_path, args, json),
        "installer" => dispatch_installer(home, store_path, args, json),
        "windows" => dispatch_windows(args, json),
        "selftest" => dispatch_selftest(args, json),
        other => Err(CliError::Usage(format!("unknown command: {other:?}"))),
    }
}

fn dispatch_beta(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
    json: bool,
) -> Result<String> {
    let noun = next(&mut args, "beta")?;
    let sub = next(&mut args, &format!("beta {noun}"))?;
    let (kind, result) = match (noun.as_str(), sub.as_str()) {
        ("campaign", "list") => (
            "beta.campaign-list",
            beta::campaign_list(home, store_path, args),
        ),
        ("campaign", "show") => (
            "beta.campaign-show",
            beta::campaign_show(home, store_path, args),
        ),
        ("finding", "submit") => (
            "beta.finding-submit",
            beta::finding_submit(home, store_path, args),
        ),
        ("finding", "list") => (
            "beta.finding-list",
            beta::finding_list(home, store_path, args),
        ),
        ("finding", "show") => (
            "beta.finding-show",
            beta::finding_show(home, store_path, args),
        ),
        ("finding", "disposition") => (
            "beta.finding-disposition",
            beta::finding_disposition(home, store_path, args),
        ),
        ("finding", "to-task") => (
            "beta.finding-to-task",
            beta::finding_to_task(home, store_path, args),
        ),
        ("disposition", "list") => (
            "beta.disposition-list",
            beta::disposition_list(home, store_path, args),
        ),
        ("disposition", "show") => (
            "beta.disposition-show",
            beta::disposition_show(home, store_path, args),
        ),
        ("task", "claim") => ("beta.task-claim", beta::task_claim(home, store_path, args)),
        ("contribution", "accept") => (
            "beta.contribution-accept",
            beta::contribution_accept(home, store_path, args),
        ),
        ("contribution", "list") => (
            "beta.contribution-list",
            beta::contribution_list(home, store_path, args),
        ),
        ("contribution", "show") => (
            "beta.contribution-show",
            beta::contribution_show(home, store_path, args),
        ),
        ("claim", "new") => ("beta.claim-new", beta::claim_new(home, store_path, args)),
        _ => {
            return Err(CliError::Usage(format!(
                "unknown `beta` command: {noun:?} {sub:?}"
            )))
        }
    };
    result.map(|value| value.render(json, kind))
}

fn dispatch_team(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
    json: bool,
) -> Result<String> {
    let noun = next(&mut args, "team")?;
    let result = match noun.as_str() {
        "propose" => coordination::team_propose(home, store_path, args)
            .map(|r| r.render(json, "team.propose")),
        "list" => {
            coordination::team_list(home, store_path, args).map(|r| r.render(json, "team.list"))
        }
        "show" => {
            coordination::team_show(home, store_path, args).map(|r| r.render(json, "team.show"))
        }
        other => Err(CliError::Usage(format!(
            "unknown `team` subcommand: {other:?}"
        ))),
    }?;
    Ok(result)
}

fn dispatch_task(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
    json: bool,
) -> Result<String> {
    let noun = next(&mut args, "task")?;
    let result =
        match noun.as_str() {
            "create" => coordination::task_create(home, store_path, args)
                .map(|r| r.render(json, "task.create")),
            "list" => {
                coordination::task_list(home, store_path, args).map(|r| r.render(json, "task.list"))
            }
            "suggest" => coordination::task_suggest(home, store_path, args)
                .map(|r| r.render(json, "task.suggest")),
            "show" => {
                coordination::task_show(home, store_path, args).map(|r| r.render(json, "task.show"))
            }
            "claim" => coordination::task_claim(home, store_path, args)
                .map(|r| r.render(json, "task.claim")),
            "review" => coordination::task_review(home, store_path, args)
                .map(|r| r.render(json, "task.review")),
            other => Err(CliError::Usage(format!(
                "unknown `task` subcommand: {other:?}"
            ))),
        }?;
    Ok(result)
}

fn dispatch_identity(home: &Path, mut args: Vec<String>) -> Result<String> {
    let noun = next(&mut args, "identity")?;
    match noun.as_str() {
        "init" => identity::cmd_init(home),
        "show" => identity::cmd_show(home),
        other => Err(CliError::Usage(format!(
            "unknown `identity` subcommand: {other:?}"
        ))),
    }
}

fn dispatch_intake(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<String> {
    let noun = next(&mut args, "intake")?;
    match noun.as_str() {
        "add" => {
            let path = next(&mut args, "intake add")?;
            intake::cmd_add(home, &path)
        }
        "show" => {
            let id = next(&mut args, "intake show")?;
            intake::cmd_show(home, &id)
        }
        "advance" => {
            let id = next(&mut args, "intake advance")?;
            let state = next(&mut args, "intake advance")?;
            intake::cmd_advance(home, &id, &state)
        }
        "publish-post" => {
            let id = next(&mut args, "intake publish-post")?;
            intake::cmd_publish_post(home, store_path, &id)
        }
        other => Err(CliError::Usage(format!(
            "unknown `intake` subcommand: {other:?}"
        ))),
    }
}

fn dispatch_kel(home: &Path, mut args: Vec<String>) -> Result<String> {
    let noun = next(&mut args, "kel")?;
    match noun.as_str() {
        "export" => identity::cmd_export_kel(home),
        "trust" => {
            let hex = next(&mut args, "kel trust")?;
            store::cmd_trust_kel(home, &hex)
        }
        other => Err(CliError::Usage(format!(
            "unknown `kel` subcommand: {other:?}"
        ))),
    }
}

fn dispatch_repo(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<String> {
    let noun = next(&mut args, "repo")?;
    match noun.as_str() {
        "init" => {
            let name = next(&mut args, "repo init")?;
            let maintainers: Result<Vec<Did>> = extract_flag_multi(&mut args, "--maintainer")
                .into_iter()
                .map(|s| Did::parse(&s).map_err(|e| CliError::Identity(e.to_string())))
                .collect();
            let min_approvals: u32 = extract_flag(&mut args, "--min-approvals")
                .map(|s| {
                    s.parse()
                        .map_err(|_| CliError::Usage("bad --min-approvals".to_string()))
                })
                .transpose()?
                .unwrap_or(0);
            repo::init(home, store_path, &name, maintainers?, min_approvals)
        }
        "track" => {
            let name = next(&mut args, "repo track")?;
            let id = next(&mut args, "repo track")?;
            repo::track(home, &name, &id)
        }
        "commit" => {
            let project = next(&mut args, "repo commit")?;
            let branch = extract_flag(&mut args, "--branch")
                .ok_or_else(|| CliError::Usage("--branch required".to_string()))?;
            let message = extract_flag(&mut args, "--message")
                .ok_or_else(|| CliError::Usage("--message required".to_string()))?;
            let paths: Vec<PathBuf> = args.into_iter().map(PathBuf::from).collect();
            repo::commit_paths(home, store_path, &project, &branch, &message, &paths)
        }
        "checkout" => {
            let commit_id = next(&mut args, "repo checkout")?;
            let dest = next(&mut args, "repo checkout")?;
            repo::checkout_commit(home, store_path, &commit_id, Path::new(&dest))
        }
        "branch" => {
            let project = next(&mut args, "repo branch")?;
            let branch_name = next(&mut args, "repo branch")?;
            let set_to = extract_flag(&mut args, "--set");
            repo::branch(home, store_path, &project, &branch_name, set_to.as_deref())
        }
        "status" => {
            let project = next(&mut args, "repo status")?;
            repo::status(home, store_path, &project)
        }
        other => Err(CliError::Usage(format!(
            "unknown `repo` subcommand: {other:?}"
        ))),
    }
}

fn dispatch_pr(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<String> {
    let noun = next(&mut args, "pr")?;
    match noun.as_str() {
        "propose" => {
            let project = next(&mut args, "pr propose")?;
            let branch = extract_flag(&mut args, "--branch")
                .ok_or_else(|| CliError::Usage("--branch required".to_string()))?;
            let title = extract_flag(&mut args, "--title")
                .ok_or_else(|| CliError::Usage("--title required".to_string()))?;
            let head = extract_flag(&mut args, "--head")
                .ok_or_else(|| CliError::Usage("--head required".to_string()))?;
            let base = extract_flag(&mut args, "--base");
            pr::propose_pr(
                home,
                store_path,
                &project,
                &branch,
                &title,
                &head,
                base.as_deref(),
            )
        }
        "approve" => {
            let pr_id = next(&mut args, "pr approve")?;
            let head = extract_flag(&mut args, "--head")
                .ok_or_else(|| CliError::Usage("--head required".to_string()))?;
            let reject = extract_bool_flag(&mut args, "--reject");
            let findings = extract_flag(&mut args, "--findings");
            pr::approve_pr(
                home,
                store_path,
                &pr_id,
                &head,
                !reject,
                findings.as_deref(),
            )
        }
        "merge" => {
            let project = next(&mut args, "pr merge")?;
            let pr_id = next(&mut args, "pr merge")?;
            let prev = extract_flag(&mut args, "--prev");
            pr::merge_pr(home, store_path, &project, &pr_id, prev.as_deref())
        }
        "ai-assisted" => {
            let pr_id = next(&mut args, "pr ai-assisted")?;
            let owner = extract_flag(&mut args, "--owner")
                .ok_or_else(|| CliError::Usage("--owner required".to_string()))?;
            let owner = Did::parse(&owner).map_err(|e| CliError::Identity(e.to_string()))?;
            pr::set_ai_assisted(home, store_path, &pr_id, owner)
        }
        "findings" => {
            let pr_id = next(&mut args, "pr findings")?;
            pr::show_findings(home, store_path, &pr_id)
        }
        other => Err(CliError::Usage(format!(
            "unknown `pr` subcommand: {other:?}"
        ))),
    }
}

fn dispatch_sync(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<String> {
    let noun = next(&mut args, "sync")?;
    match noun.as_str() {
        "listen" => {
            let addr = extract_flag(&mut args, "--addr")
                .ok_or_else(|| CliError::Usage("--addr required".to_string()))?;
            let repeat: u32 = extract_flag(&mut args, "--repeat")
                .map(|s| {
                    s.parse()
                        .map_err(|_| CliError::Usage("bad --repeat".to_string()))
                })
                .transpose()?
                .unwrap_or(1);
            sync::listen(home, store_path, &addr, repeat)
        }
        "connect" => {
            let addr = next(&mut args, "sync connect")?;
            sync::connect(home, store_path, &addr)
        }
        other => Err(CliError::Usage(format!(
            "unknown `sync` subcommand: {other:?}"
        ))),
    }
}

fn dispatch_keystone(home: &Path, mut args: Vec<String>, json: bool) -> Result<String> {
    let noun = next(&mut args, "keystone")?;
    match noun.as_str() {
        "run" => {
            let kind = "keystone.run";
            let peer_home = required_path_flag(&mut args, "--peer-home")?;
            let now_ms = extract_u64_flag(&mut args, "--now-ms")?.unwrap_or_else(now_ms_wall_clock);
            keystone::run(home, &peer_home, now_ms).map(|r: CommandResult| r.render(json, kind))
        }
        other => Err(CliError::Usage(format!(
            "unknown `keystone` subcommand: {other:?}"
        ))),
    }
}

fn now_ms_wall_clock() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn extract_u64_flag(args: &mut Vec<String>, flag: &str) -> Result<Option<u64>> {
    extract_flag(args, flag)
        .map(|s| {
            s.parse()
                .map_err(|_| CliError::Usage(format!("bad {flag}")))
        })
        .transpose()
}

fn extract_u32_flag(args: &mut Vec<String>, flag: &str) -> Result<Option<u32>> {
    extract_flag(args, flag)
        .map(|s| {
            s.parse()
                .map_err(|_| CliError::Usage(format!("bad {flag}")))
        })
        .transpose()
}

fn required_u64_flag(args: &mut Vec<String>, flag: &str) -> Result<u64> {
    extract_u64_flag(args, flag)?.ok_or_else(|| CliError::Usage(format!("{flag} required")))
}

fn required_path_flag(args: &mut Vec<String>, flag: &str) -> Result<PathBuf> {
    extract_flag(args, flag)
        .map(PathBuf::from)
        .ok_or_else(|| CliError::Usage(format!("{flag} required")))
}

fn dispatch_build(mut args: Vec<String>, json: bool) -> Result<String> {
    let noun = next(&mut args, "build")?;
    match noun.as_str() {
        "run" => {
            let kind = "build.run";
            let component = required_path_flag(&mut args, "--component")?;
            let store_dir = required_path_flag(&mut args, "--store-dir")?;
            let scratch_dir = required_path_flag(&mut args, "--scratch-dir")?;
            let artifacts_dir = required_path_flag(&mut args, "--artifacts-dir")?;
            let capabilities =
                build::parse_capabilities(extract_flag_multi(&mut args, "--capability"))?;

            let mut limits = build::default_limits();
            if let Some(v) = extract_u64_flag(&mut args, "--max-fuel")? {
                limits.max_fuel = v;
            }
            if let Some(v) = extract_u64_flag(&mut args, "--max-memory-bytes")? {
                limits.max_memory_bytes = v;
            }
            if let Some(v) = extract_u64_flag(&mut args, "--max-wall-clock-ms")? {
                limits.max_wall_clock_ms = v;
            }
            if let Some(v) = extract_u64_flag(&mut args, "--max-output-bytes")? {
                limits.max_output_bytes = v;
            }
            if let Some(v) = extract_u64_flag(&mut args, "--max-stdout-bytes")? {
                limits.max_stdout_bytes = v;
            }
            if let Some(v) = extract_u64_flag(&mut args, "--max-stderr-bytes")? {
                limits.max_stderr_bytes = v;
            }
            if let Some(v) = extract_u32_flag(&mut args, "--max-open-files")? {
                limits.max_open_files = v;
            }

            build::run(
                &component,
                &store_dir,
                &scratch_dir,
                &artifacts_dir,
                capabilities,
                limits,
            )
            .map(|r: CommandResult| r.render(json, kind))
        }
        "serve" => {
            let addr = extract_flag(&mut args, "--addr")
                .ok_or_else(|| CliError::Usage("--addr required".to_string()))?;
            let work_dir = required_path_flag(&mut args, "--work-dir")?;
            build::serve(&addr, &work_dir).map(|r: CommandResult| r.render(json, "build.serve"))
        }
        "dispatch" => {
            let peer = extract_flag(&mut args, "--peer")
                .ok_or_else(|| CliError::Usage("--peer required".to_string()))?;
            let component = required_path_flag(&mut args, "--component")?;
            let workspace = required_path_flag(&mut args, "--workspace")?;
            let artifacts_dir = required_path_flag(&mut args, "--artifacts-dir")?;
            let capabilities =
                build::parse_capabilities(extract_flag_multi(&mut args, "--capability"))?;
            let mut limits = build::default_limits();
            if let Some(v) = extract_u64_flag(&mut args, "--max-fuel")? {
                limits.max_fuel = v;
            }
            if let Some(v) = extract_u64_flag(&mut args, "--max-memory-bytes")? {
                limits.max_memory_bytes = v;
            }
            if let Some(v) = extract_u64_flag(&mut args, "--max-wall-clock-ms")? {
                limits.max_wall_clock_ms = v;
            }
            if let Some(v) = extract_u64_flag(&mut args, "--max-output-bytes")? {
                limits.max_output_bytes = v;
            }
            build::dispatch(
                &peer,
                &component,
                &workspace,
                &artifacts_dir,
                capabilities,
                limits,
            )
            .map(|r: CommandResult| r.render(json, "build.dispatch"))
        }
        other => Err(CliError::Usage(format!(
            "unknown `build` subcommand: {other:?}"
        ))),
    }
}

fn dispatch_release(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
    json: bool,
) -> Result<String> {
    let noun = next(&mut args, "release")?;
    match noun.as_str() {
        "create" => {
            let project = next(&mut args, "release create")?;
            let branch = extract_flag(&mut args, "--branch")
                .ok_or_else(|| CliError::Usage("--branch required".to_string()))?;
            let version = extract_flag(&mut args, "--version")
                .ok_or_else(|| CliError::Usage("--version required".to_string()))?;
            let commit = extract_flag(&mut args, "--commit")
                .ok_or_else(|| CliError::Usage("--commit required".to_string()))?;
            let artifact = required_path_flag(&mut args, "--artifact")?;
            let recipe_digest = extract_flag(&mut args, "--recipe-digest")
                .ok_or_else(|| CliError::Usage("--recipe-digest required".to_string()))?;
            release::create(
                home,
                store_path,
                &project,
                &branch,
                &version,
                &commit,
                &artifact,
                &recipe_digest,
            )
            .map(|r: CommandResult| r.render(json, "release.create"))
        }
        "attest" => {
            let release_id = next(&mut args, "release attest")?;
            let artifact_digest = extract_flag(&mut args, "--artifact-digest")
                .ok_or_else(|| CliError::Usage("--artifact-digest required".to_string()))?;
            release::attest_release(home, store_path, &release_id, &artifact_digest)
                .map(|r: CommandResult| r.render(json, "release.attest"))
        }
        "verify" => {
            let release_id = next(&mut args, "release verify")?;
            let project = next(&mut args, "release verify")?;
            let branch = extract_flag(&mut args, "--branch")
                .ok_or_else(|| CliError::Usage("--branch required".to_string()))?;
            let min_attestations = extract_u32_flag(&mut args, "--min-attestations")?;
            let timelock_ms = extract_u64_flag(&mut args, "--timelock-ms")?;
            let now_ms = extract_u64_flag(&mut args, "--now-ms")?;
            release::verify(
                home,
                store_path,
                &release_id,
                &project,
                &branch,
                min_attestations,
                timelock_ms,
                now_ms,
            )
            .map(|r: CommandResult| r.render(json, "release.verify"))
        }
        "fetch" => {
            let release_id = next(&mut args, "release fetch")?;
            let project = next(&mut args, "release fetch")?;
            let branch = extract_flag(&mut args, "--branch")
                .ok_or_else(|| CliError::Usage("--branch required".to_string()))?;
            let output = required_path_flag(&mut args, "--output")?;
            let min_attestations = extract_u32_flag(&mut args, "--min-attestations")?;
            let timelock_ms = extract_u64_flag(&mut args, "--timelock-ms")?;
            let now_ms = extract_u64_flag(&mut args, "--now-ms")?;
            release::fetch(
                home,
                store_path,
                &release_id,
                &project,
                &branch,
                &output,
                min_attestations,
                timelock_ms,
                now_ms,
            )
            .map(|r: CommandResult| r.render(json, "release.fetch"))
        }
        "retrieve" => {
            let release_id = next(&mut args, "release retrieve")?;
            let project = next(&mut args, "release retrieve")?;
            let branch = extract_flag(&mut args, "--branch")
                .ok_or_else(|| CliError::Usage("--branch required".to_string()))?;
            let peer = extract_flag(&mut args, "--peer")
                .ok_or_else(|| CliError::Usage("--peer required".to_string()))?;
            let output = required_path_flag(&mut args, "--output")?;
            let min_attestations = extract_u32_flag(&mut args, "--min-attestations")?;
            let timelock_ms = extract_u64_flag(&mut args, "--timelock-ms")?;
            let now_ms = extract_u64_flag(&mut args, "--now-ms")?;
            release::retrieve(
                home,
                store_path,
                &release_id,
                &project,
                &branch,
                &peer,
                &output,
                min_attestations,
                timelock_ms,
                now_ms,
            )
            .map(|r: CommandResult| r.render(json, "release.retrieve"))
        }
        "serve" => {
            let addr = extract_flag(&mut args, "--addr")
                .ok_or_else(|| CliError::Usage("--addr required".to_string()))?;
            release::serve(store_path, &addr)
                .map(|r: CommandResult| r.render(json, "release.serve"))
        }
        "list" => {
            let project = next(&mut args, "release list")?;
            let branch = extract_flag(&mut args, "--branch")
                .ok_or_else(|| CliError::Usage("--branch required".to_string()))?;
            release::list(home, store_path, &project, &branch)
                .map(|r: CommandResult| r.render(json, "release.list"))
        }
        other => Err(CliError::Usage(format!(
            "unknown `release` subcommand: {other:?}"
        ))),
    }
}

fn dispatch_provenance(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
    json: bool,
) -> Result<String> {
    let noun = next(&mut args, "provenance")?;
    match noun.as_str() {
        "record" => {
            let subject = next(&mut args, "provenance record")?;
            let environment_digest = extract_flag(&mut args, "--environment-digest")
                .ok_or_else(|| CliError::Usage("--environment-digest required".to_string()))?;
            let commands_digest = extract_flag(&mut args, "--commands-digest")
                .ok_or_else(|| CliError::Usage("--commands-digest required".to_string()))?;
            let outputs = extract_flag_multi(&mut args, "--output");
            let group = extract_flag(&mut args, "--group")
                .ok_or_else(|| CliError::Usage("--group required".to_string()))?;
            let network_enabled = extract_bool_flag(&mut args, "--network-enabled");
            let started_ms = required_u64_flag(&mut args, "--started-ms")?;
            let finished_ms = required_u64_flag(&mut args, "--finished-ms")?;
            provenance::record(
                home,
                store_path,
                &subject,
                &environment_digest,
                &commands_digest,
                &outputs,
                &group,
                network_enabled,
                started_ms,
                finished_ms,
            )
            .map(|r: CommandResult| r.render(json, "provenance.record"))
        }
        "verify" => {
            let subject = next(&mut args, "provenance verify")?;
            let output = extract_flag(&mut args, "--output")
                .ok_or_else(|| CliError::Usage("--output required".to_string()))?;
            let min_agreement = extract_u32_flag(&mut args, "--min-agreement")?.unwrap_or(1);
            provenance::verify(home, store_path, &subject, &output, min_agreement)
                .map(|r: CommandResult| r.render(json, "provenance.verify"))
        }
        other => Err(CliError::Usage(format!(
            "unknown `provenance` subcommand: {other:?}"
        ))),
    }
}

/// `mini selftest [list|<area>]` --- run the diagnostics suite.
///
/// The same checks the client's Diagnostics page runs. A failed check makes
/// `main` exit non-zero, so this works as a post-install smoke test in a
/// script rather than only as something a person reads.
fn dispatch_selftest(mut args: Vec<String>, json: bool) -> Result<String> {
    let scratch = extract_flag(&mut args, "--scratch").map(PathBuf::from);
    let noun = if args.is_empty() {
        None
    } else {
        Some(next(&mut args, "selftest")?)
    };
    let area = match noun.as_deref() {
        Some("list") => return Ok(crate::selftest::list().render(json, "selftest.list")),
        Some("coverage") => {
            return Ok(crate::selftest::coverage().render(json, "selftest.coverage"))
        }
        Some(area) => Some(area.to_string()),
        None => None,
    };
    let (result, clean) = crate::selftest::run(scratch.as_deref(), area.as_deref())?;
    let rendered = result.render(json, "selftest.run");
    if clean {
        return Ok(rendered);
    }
    // Failing checks exit non-zero either way. The report itself --
    // human text or, under --json, the full `{"clean":false,"passed":...,
    // "failures":[...],"skips":[...]}` envelope -- is carried as the error's
    // payload rather than replaced with a generic sentence: a caller reading
    // `--json` output needs `error_code` to know the command failed *and*
    // the report's fields to know which checks did, without a second
    // invocation in a different mode to find out.
    Err(CliError::SelfTest(rendered))
}

/// `mini windows ...` --- build and inspect Windows client packages.
///
/// Read and build verbs only. Installing stays with `mininet-setup.exe`,
/// which ships inside the package and is therefore still present when Apps &
/// features later needs to run its uninstall command (`crate::windows`'s
/// module docs).
fn dispatch_windows(mut args: Vec<String>, json: bool) -> Result<String> {
    let noun = next(&mut args, "windows")?;
    let result = dispatch_windows_noun(&noun, &mut args, json)?;
    // Anything left over is a flag that was never consumed, which almost
    // always means a typo. Silently ignoring `--targte` would emit a
    // plausibly named package built for the default target instead of
    // reporting the mistake.
    reject_leftover(&args, &format!("windows {noun}"))?;
    Ok(result)
}

/// Reject arguments no option consumed.
fn reject_leftover(args: &[String], context: &str) -> Result<()> {
    if args.is_empty() {
        return Ok(());
    }
    Err(CliError::Usage(format!(
        "`mini {context}`: unrecognized argument(s): {}",
        args.join(" ")
    )))
}

fn dispatch_windows_noun(noun: &str, args: &mut Vec<String>, json: bool) -> Result<String> {
    match noun {
        "pack" => {
            let source = required_path_flag(args, "--source")?;
            let out = required_path_flag(args, "--out")?;
            let version = extract_flag(args, "--version")
                .ok_or_else(|| CliError::Usage("--version required".to_string()))?;
            // Required rather than defaulted to the clock: two runs over the
            // same bytes must produce the same container, or an independent
            // builder cannot confirm they built the same package.
            let built_at_ms = required_u64_flag(args, "--built-at-ms")?;
            let request = crate::windows::PackRequest {
                source,
                out,
                version,
                package: extract_flag(args, "--package")
                    .unwrap_or_else(|| crate::windows::DEFAULT_PACKAGE.to_string()),
                product: extract_flag(args, "--product")
                    .unwrap_or_else(|| crate::windows::DEFAULT_PRODUCT.to_string()),
                launch: extract_flag(args, "--launch")
                    .unwrap_or_else(|| crate::windows::DEFAULT_LAUNCH.to_string()),
                target: extract_flag(args, "--target")
                    .unwrap_or_else(|| crate::windows::DEFAULT_TARGET.to_string()),
                built_at_ms,
                include: extract_flag_multi(args, "--include"),
                shortcuts: extract_flag_multi(args, "--shortcut"),
            };
            crate::windows::pack(&request).map(|r: CommandResult| r.render(json, "windows.pack"))
        }
        "inspect" => {
            let path = next(args, "windows inspect")?;
            crate::windows::inspect(Path::new(&path))
                .map(|r: CommandResult| r.render(json, "windows.inspect"))
        }
        "plan" => {
            let package = next(args, "windows plan")?;
            let install_root = extract_flag(args, "--install-root").map(PathBuf::from);
            let user_data_root = extract_flag(args, "--user-data-root").map(PathBuf::from);
            let options = windows_install_options(args);
            crate::windows::plan(
                Path::new(&package),
                install_root.as_deref(),
                user_data_root.as_deref(),
                &options,
            )
            .map(|r: CommandResult| r.render(json, "windows.plan"))
        }
        "verify" => {
            let install_root = extract_flag(args, "--install-root").map(PathBuf::from);
            let user_data_root = extract_flag(args, "--user-data-root").map(PathBuf::from);
            let version = extract_flag(args, "--version");
            crate::windows::verify(
                install_root.as_deref(),
                user_data_root.as_deref(),
                version.as_deref(),
            )
            .map(|r: CommandResult| r.render(json, "windows.verify"))
        }
        "status" => {
            let install_root = extract_flag(args, "--install-root").map(PathBuf::from);
            let user_data_root = extract_flag(args, "--user-data-root").map(PathBuf::from);
            crate::windows::status(install_root.as_deref(), user_data_root.as_deref())
                .map(|r: CommandResult| r.render(json, "windows.status"))
        }
        other => Err(CliError::Usage(format!(
            "unknown `windows` subcommand: {other:?}"
        ))),
    }
}

/// Install choices shared by `windows plan`.
///
/// The defaults match `mininet-setup`'s, so a plan printed here describes
/// what that program would actually do rather than a differently-configured
/// hypothetical.
fn windows_install_options(args: &mut Vec<String>) -> mini_windows_setup::InstallOptions {
    let mut options = mini_windows_setup::InstallOptions::default();
    if extract_bool_flag(args, "--no-start-menu") {
        options.start_menu_shortcut = false;
    }
    if extract_bool_flag(args, "--desktop-shortcut") {
        options.desktop_shortcut = true;
    }
    if extract_bool_flag(args, "--no-register") {
        options.register_uninstall = false;
    }
    if extract_bool_flag(args, "--allow-downgrade") {
        options.allow_downgrade = true;
    }
    options.start_menu_dir = extract_flag(args, "--start-menu-dir").map(PathBuf::from);
    options.desktop_dir = extract_flag(args, "--desktop-dir").map(PathBuf::from);
    options
}

fn dispatch_installer(
    home: &Path,
    store_path: &Path,
    mut args: Vec<String>,
    json: bool,
) -> Result<String> {
    let noun = next(&mut args, "installer")?;
    let device_root = required_path_flag(&mut args, "--device-root")?;
    match noun.as_str() {
        "stage" => {
            let release_id = next(&mut args, "installer stage")?;
            let project = next(&mut args, "installer stage")?;
            let branch = extract_flag(&mut args, "--branch")
                .ok_or_else(|| CliError::Usage("--branch required".to_string()))?;
            let min_attestations = extract_u32_flag(&mut args, "--min-attestations")?;
            let timelock_ms = extract_u64_flag(&mut args, "--timelock-ms")?;
            let now_ms = extract_u64_flag(&mut args, "--now-ms")?;
            let timestamp_ms = required_u64_flag(&mut args, "--timestamp-ms")?;
            installer::stage(
                home,
                store_path,
                &device_root,
                &release_id,
                &project,
                &branch,
                min_attestations,
                timelock_ms,
                now_ms,
                timestamp_ms,
            )
            .map(|r: CommandResult| r.render(json, "installer.stage"))
        }
        "preflight" => {
            let release_id = next(&mut args, "installer preflight")?;
            let timestamp_ms = required_u64_flag(&mut args, "--timestamp-ms")?;
            installer::preflight(&device_root, &release_id, timestamp_ms)
                .map(|r: CommandResult| r.render(json, "installer.preflight"))
        }
        "activate" => {
            let release_id = next(&mut args, "installer activate")?;
            let approved_at_ms = required_u64_flag(&mut args, "--approved-at-ms")?;
            installer::activate(&device_root, &release_id, approved_at_ms)
                .map(|r: CommandResult| r.render(json, "installer.activate"))
        }
        "health-check" => {
            let release_id = next(&mut args, "installer health-check")?;
            let healthy = extract_bool_flag(&mut args, "--healthy");
            let unhealthy = extract_bool_flag(&mut args, "--unhealthy");
            if healthy == unhealthy {
                return Err(CliError::Usage(
                    "exactly one of --healthy or --unhealthy is required".to_string(),
                ));
            }
            let timestamp_ms = required_u64_flag(&mut args, "--timestamp-ms")?;
            installer::health_check(&device_root, &release_id, healthy, timestamp_ms)
                .map(|r: CommandResult| r.render(json, "installer.health-check"))
        }
        "rollback" => {
            let timestamp_ms = required_u64_flag(&mut args, "--timestamp-ms")?;
            installer::rollback(&device_root, timestamp_ms)
                .map(|r: CommandResult| r.render(json, "installer.rollback"))
        }
        "status" => installer::status(&device_root)
            .map(|r: CommandResult| r.render(json, "installer.status")),
        "history" => {
            let release_id = extract_flag(&mut args, "--release");
            installer::history(&device_root, release_id.as_deref())
                .map(|r: CommandResult| r.render(json, "installer.history"))
        }
        "verify-log" => installer::verify_log(&device_root)
            .map(|r: CommandResult| r.render(json, "installer.verify-log")),
        other => Err(CliError::Usage(format!(
            "unknown `installer` subcommand: {other:?}"
        ))),
    }
}

fn next(args: &mut Vec<String>, context: &str) -> Result<String> {
    if args.is_empty() {
        return Err(CliError::Usage(format!("{context}: missing argument")));
    }
    Ok(args.remove(0))
}
