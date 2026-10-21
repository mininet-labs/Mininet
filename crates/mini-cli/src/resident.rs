//! Resident AI's owner-invoked CLI adapter. Model output never reaches dispatch.
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use mini_resident::{Domain, PatchEdit, PatchPreview, ProviderConfig, ResidentRequest, Workspace};
use serde_json::{json, Value};

use crate::error::{CliError, Result};

const MAX_DOCUMENT: usize = 2 * 1024 * 1024;
fn usage(message: impl Into<String>) -> CliError {
    CliError::Usage(message.into())
}
fn error(e: impl std::fmt::Display) -> CliError {
    usage(format!("Resident AI: {e}"))
}
fn take(args: &mut Vec<String>, flag: &str) -> Result<Option<String>> {
    let Some(position) = args.iter().position(|a| a == flag) else {
        return Ok(None);
    };
    if position + 1 >= args.len() || args[position + 1].starts_with("--") {
        return Err(usage(format!("missing {flag} value")));
    }
    args.remove(position);
    Ok(Some(args.remove(position)))
}
fn required(args: &mut Vec<String>, flag: &str) -> Result<String> {
    take(args, flag)?.ok_or_else(|| usage(format!("{flag} required")))
}
fn no_more(args: &[String]) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(usage("unexpected or duplicate Resident AI arguments"))
    }
}
fn workspace(args: &mut Vec<String>) -> Result<Workspace> {
    Workspace::open(Path::new(&required(args, "--workspace")?)).map_err(error)
}
fn read_document(path: &Path) -> Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path).map_err(error)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(usage("document must be a regular non-symlink file"));
    }
    let mut data = Vec::new();
    File::open(path)
        .map_err(error)?
        .take(MAX_DOCUMENT as u64 + 1)
        .read_to_end(&mut data)
        .map_err(error)?;
    if data.len() > MAX_DOCUMENT {
        return Err(usage("Resident AI document exceeds limit"));
    }
    Ok(data)
}
fn write_new(path: &Path, data: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(error)?;
    file.write_all(data)
        .and_then(|_| file.sync_all())
        .map_err(error)?;
    mini_durable::sync_parent(path).map_err(error)
}
fn number(args: &mut Vec<String>, flag: &str) -> Result<u64> {
    required(args, flag)?
        .parse()
        .map_err(|_| usage(format!("invalid {flag}")))
}

pub fn dispatch(
    home: &Path,
    _store: &Path,
    mut args: Vec<String>,
    as_json: bool,
) -> Result<String> {
    if args.is_empty() {
        return Err(usage("resident status | configure | ask | inspect | search | check | hosting | fees | patch-preview | patch-apply | draft-save | draft-inspect | draft-review | draft-resume | draft-clear"));
    }
    let command = args.remove(0);
    let (human, value): (String, Value) = match command.as_str() {
        "status" => {
            no_more(&args)?;
            let config = mini_resident::load_config(home).map_err(error)?;
            let configured = config.is_some();
            (
                if configured {
                    "Resident AI configured; availability is checked only when you send a request."
                        .into()
                } else {
                    "Resident AI needs an owner-selected local model. Configure a loopback OpenAI-compatible chat endpoint; no model is bundled or downloaded.".into()
                },
                json!({"configured":configured,"provider_available":null,"config":config,"automatic_execution":false,"config_path":home.join("resident.json")}),
            )
        }
        "configure" => {
            let config = ProviderConfig {
                endpoint: required(&mut args, "--endpoint")?,
                model: required(&mut args, "--model")?,
            };
            no_more(&args)?;
            mini_resident::save_config(home, &config).map_err(error)?;
            (
                "Resident AI local model configuration saved. No connection or download started."
                    .into(),
                json!({"config":config,"connected":false}),
            )
        }
        "ask" => {
            if args.is_empty() {
                return Err(usage(
                    "resident ask <question> [--workspace ROOT --context RELATIVE_FILE]",
                ));
            }
            let question = args.remove(0);
            let root = take(&mut args, "--workspace")?;
            let mut paths = Vec::new();
            while let Some(path) = take(&mut args, "--context")? {
                paths.push(path);
            }
            no_more(&args)?;
            let context = if paths.is_empty() {
                Vec::new()
            } else {
                let root = root.ok_or_else(|| usage("--workspace required for context grants"))?;
                Workspace::open(Path::new(&root))
                    .map_err(error)?
                    .context(&paths)
                    .map_err(error)?
            };
            let request = ResidentRequest { question, context };
            mini_resident::validate_request(&request).map_err(error)?;
            let config = mini_resident::load_config(home).map_err(error)?.ok_or_else(|| usage("Resident AI is not configured; choose a local model with resident configure"))?;
            let reply = mini_resident::chat(&config, &request).map_err(error)?;
            let human = format!(
                "Resident AI · AI-generated advice · {}\n\n{}",
                reply.model, reply.text
            );
            (
                human,
                json!({"reply":reply,"context_files":request.context.iter().map(|i|json!({"path":i.path,"hash":i.hash})).collect::<Vec<_>>(),"actions_executed":0}),
            )
        }
        "inspect" => {
            let workspace = workspace(&mut args)?;
            no_more(&args)?;
            let report = workspace.inspect().map_err(error)?;
            (
                serde_json::to_string_pretty(&report).map_err(error)?,
                json!({"inspection":report,"model_used":false}),
            )
        }
        "search" => {
            let workspace = workspace(&mut args)?;
            let query = required(&mut args, "--query")?;
            no_more(&args)?;
            let hits = workspace.search(&query).map_err(error)?;
            (
                serde_json::to_string_pretty(&hits).map_err(error)?,
                json!({"hits":hits,"scope":"explicit local workspace","model_used":false}),
            )
        }
        "check" => {
            let area = required(&mut args, "--area")?;
            no_more(&args)?;
            // Only shipped diagnostic implementations, never a model-selected
            // command, Cargo build script or arbitrary source executable.
            let scratch = std::env::temp_dir().join(format!(
                "mininet-resident-check-{}-{}",
                std::process::id(),
                crate::sequence::now_ms()
            ));
            std::fs::create_dir(&scratch).map_err(error)?;
            let (result, clean) = crate::selftest::run(Some(&scratch), Some(&area))?;
            let value: Value = serde_json::from_str(&result.clone().render(true, "resident.check"))
                .map_err(error)?;
            if !clean {
                return Err(CliError::SelfTest(result.human));
            }
            (
                result.human,
                json!({"diagnostics":value,"model_used":false,"audit_or_formal_proof":false}),
            )
        }
        "hosting" => {
            let workspace = workspace(&mut args)?;
            let path = required(&mut args, "--file")?;
            let replicas = take(&mut args, "--replicas")?
                .unwrap_or_else(|| "3".into())
                .parse::<u64>()
                .map_err(|_| usage("invalid replicas"))?;
            no_more(&args)?;
            if !(1..=64).contains(&replicas) {
                return Err(usage("replicas must be 1..64"));
            }
            let summary = workspace.file_summary(&path).map_err(error)?;
            let chunks = summary.bytes.div_ceil(mini_media::CHUNK_SIZE as u64);
            let replicated = summary
                .bytes
                .checked_mul(replicas)
                .ok_or_else(|| usage("hosting size overflow"))?;
            let value = json!({"file":summary,"chunks":chunks,"chunk_bytes":mini_media::CHUNK_SIZE,"requested_copies":replicas,"requested_storage_bytes":replicated,"availability_verified":false,"hosting_started":false,"fees_spent":0});
            (serde_json::to_string_pretty(&value).map_err(error)?, value)
        }
        "fees" => {
            let bytes = number(&mut args, "--bytes")?;
            let rate = number(&mut args, "--units-per-mib")?;
            let copies = number(&mut args, "--copies")?;
            let cap = number(&mut args, "--max-units")?;
            no_more(&args)?;
            if !(1..=64).contains(&copies) {
                return Err(usage("copies must be 1..64"));
            }
            let units = bytes
                .div_ceil(1024 * 1024)
                .checked_mul(rate)
                .and_then(|n| n.checked_mul(copies))
                .ok_or_else(|| usage("fee estimate overflow"))?;
            let value = json!({"estimated_units":units,"within_owner_budget":units<=cap,"max_units":cap,"price_source":"owner supplied arithmetic inputs","network_quote_verified":false,"currency_authorized":false,"funds_spent":0});
            (serde_json::to_string_pretty(&value).map_err(error)?, value)
        }
        "draft-save" => {
            let workspace = workspace(&mut args)?;
            let edits_path = PathBuf::from(required(&mut args, "--edits")?);
            let expected = take(&mut args, "--expect")?;
            let create_only =
                if let Some(index) = args.iter().position(|arg| arg == "--create-only") {
                    args.remove(index);
                    true
                } else {
                    false
                };
            if create_only && expected.is_some() {
                return Err(usage("choose --create-only or --expect, not both"));
            }
            no_more(&args)?;
            let edits: Vec<PatchEdit> =
                serde_json::from_slice(&read_document(&edits_path)?).map_err(error)?;
            let saved = if create_only || expected.is_some() {
                workspace.save_draft_if(&edits, expected.as_deref())
            } else {
                workspace.save_draft(&edits)
            }
            .map_err(error)?;
            (format!("Saved staged draft {} privately in .resident-draft.json. No source changed; confirmation was not saved.",saved.preview.id),json!({"draft_id":saved.preview.id,"paths":saved.preview.edits.iter().map(|e|&e.path).collect::<Vec<_>>(),"saved":true,"source_changed":false,"confirmation_saved":false}))
        }
        "draft-review" => {
            let workspace = workspace(&mut args)?;
            no_more(&args)?;
            let review = workspace.review_saved_draft().map_err(error)?;
            let files=review.files.iter().map(|file|json!({"edit":file.edit,"current":file.current,"matches_baseline":file.matches_baseline})).collect::<Vec<_>>();
            let value = json!({"draft_id":review.id,"files":files,"source_changed":false,"rebased":false,"applied":false});
            (serde_json::to_string_pretty(&value).map_err(error)?, value)
        }
        "draft-inspect" => {
            let workspace = workspace(&mut args)?;
            no_more(&args)?;
            let info = workspace.draft_info().map_err(error)?;
            (
                format!(
                    "Stored draft {}: {}. Current source has not been validated.",
                    info.id,
                    info.paths.join(", ")
                ),
                json!({"draft_id":info.id,"paths":info.paths,"source_validated":false,"applied":false}),
            )
        }
        "draft-resume" => {
            let workspace = workspace(&mut args)?;
            let output = PathBuf::from(required(&mut args, "--output")?);
            no_more(&args)?;
            let saved = workspace.load_draft().map_err(error)?;
            write_new(
                &output,
                &serde_json::to_vec_pretty(&saved.preview).map_err(error)?,
            )?;
            (format!("Saved draft revalidated and exported to {}. Review the full preview and type its exact ID to apply; no source changed.",output.display()),json!({"preview":saved.preview,"output":output,"source_validated":true,"applied":false}))
        }
        "draft-clear" => {
            let workspace = workspace(&mut args)?;
            let confirmation = required(&mut args, "--confirm")?;
            no_more(&args)?;
            workspace.clear_draft(&confirmation).map_err(error)?;
            (
                format!("Removed stored draft {confirmation}. Source files unchanged."),
                json!({"draft_id":confirmation,"removed":true,"source_changed":false}),
            )
        }
        "patch-preview" => {
            let workspace = workspace(&mut args)?;
            let edits_path = PathBuf::from(required(&mut args, "--edits")?);
            let output = PathBuf::from(required(&mut args, "--output")?);
            no_more(&args)?;
            let edits: Vec<PatchEdit> =
                serde_json::from_slice(&read_document(&edits_path)?).map_err(error)?;
            let preview = workspace.preview_patch(&edits).map_err(error)?;
            write_new(
                &output,
                &serde_json::to_vec_pretty(&preview).map_err(error)?,
            )?;
            (format!("Patch preview saved at {}. Inspect it before confirming its exact id. No source changed.",output.display()),json!({"preview":preview,"output":output,"applied":false}))
        }
        "patch-apply" => {
            let workspace = workspace(&mut args)?;
            let path = PathBuf::from(required(&mut args, "--preview")?);
            let confirmation = required(&mut args, "--confirm")?;
            no_more(&args)?;
            let preview: PatchPreview =
                serde_json::from_slice(&read_document(&path)?).map_err(error)?;
            let receipt = workspace
                .apply_patch(&preview, &confirmation)
                .map_err(error)?;
            (
                "Owner-confirmed patch applied. Review and diagnostics remain separate steps."
                    .into(),
                json!({"receipt":receipt,"human_review_completed":false,"merged":false}),
            )
        }
        "capabilities" => {
            no_more(&args)?;
            let policies = [
                Domain::Coding,
                Domain::Checks,
                Domain::Search,
                Domain::Governance,
                Domain::Hosting,
                Domain::Fees,
            ]
            .into_iter()
            .map(mini_resident::domain_policy)
            .collect::<Vec<_>>();
            ("Resident AI helps with code, checks, local search, governance analysis, hosting plans and fee estimates. Model output is advice; owner-invoked tools remain separate.".into(),json!({"policies":policies,"bundled_model":false,"peer_inference_marketplace":false}))
        }
        _ => return Err(usage("unknown Resident AI command")),
    };
    if as_json {
        let mut value = value;
        let object = value
            .as_object_mut()
            .ok_or_else(|| usage("invalid Resident AI result"))?;
        object.insert("ok".into(), json!(true));
        object.insert("kind".into(), json!(format!("resident.{command}")));
        serde_json::to_string(&value).map_err(error)
    } else {
        Ok(human
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect())
    }
}
