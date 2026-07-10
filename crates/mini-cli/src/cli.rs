//! Argument parsing and dispatch. Deliberately hand-rolled rather than
//! pulling in a CLI-parsing dependency: the command surface is small and
//! fixed, and this workspace's own convention (see `mini-spacetime`,
//! `mini-porep`) is to avoid a dependency where a few dozen lines of plain
//! Rust do the job — the same reasoning, applied to tooling instead of
//! cryptography.

use std::path::{Path, PathBuf};

use did_mini::Did;

use crate::error::{CliError, Result};
use crate::{identity, pr, repo, store, sync};

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
/// name. Returns the human-readable output to print on success.
pub fn run(raw_args: &[String]) -> Result<String> {
    let mut args: Vec<String> = raw_args.to_vec();
    let home = extract_flag(&mut args, "--home")
        .map(PathBuf::from)
        .unwrap_or_else(default_home);
    let store_path = extract_flag(&mut args, "--store")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("store"));

    dispatch(&home, &store_path, args)
}

fn dispatch(home: &Path, store_path: &Path, mut args: Vec<String>) -> Result<String> {
    if args.is_empty() {
        return Err(CliError::Usage("no command given".to_string()));
    }
    let verb = args.remove(0);
    match verb.as_str() {
        "identity" => dispatch_identity(home, args),
        "kel" => dispatch_kel(home, args),
        "repo" => dispatch_repo(home, store_path, args),
        "pr" => dispatch_pr(home, store_path, args),
        "sync" => dispatch_sync(home, store_path, args),
        other => Err(CliError::Usage(format!("unknown command: {other:?}"))),
    }
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

fn next(args: &mut Vec<String>, context: &str) -> Result<String> {
    if args.is_empty() {
        return Err(CliError::Usage(format!("{context}: missing argument")));
    }
    Ok(args.remove(0))
}
