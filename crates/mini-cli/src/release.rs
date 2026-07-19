//! `mini release ...` — publish, attest, and verify governed releases.
//! Thin wrapper over `mini_forge::release`/`attest`/`verify_governed_release`,
//! matching `crate::repo`'s existing pattern of collapsing a multi-object
//! sequence (here: publish the artifact via `mini_media`, then create the
//! release object) into one command.

use std::path::Path;

use mini_forge::{
    attest, list_releases, release, verify_governed_release, ReleasePolicy, VerifiedRelease,
    ADOPTION_MIN_ATTESTATIONS, ADOPTION_MIN_TIMELOCK_MS,
};
use mini_media::assemble;
use mini_objects::ObjectId;

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};
use crate::project as project_alias;
use crate::sequence;
use crate::store::open_store;

fn parse_hex32(s: &str, field: &str) -> Result<[u8; 32]> {
    if s.len() != 64 {
        return Err(CliError::Usage(format!(
            "{field} must be 64 hex characters (32 bytes), got {}",
            s.len()
        )));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let byte_str = std::str::from_utf8(chunk).map_err(|_| bad_hex(field))?;
        out[i] = u8::from_str_radix(byte_str, 16).map_err(|_| bad_hex(field))?;
    }
    Ok(out)
}

fn bad_hex(field: &str) -> CliError {
    CliError::Usage(format!("{field} is not valid hex"))
}

fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// `mini release create <project> --branch <b> --version <v> --commit
/// <id> --artifact <path> --recipe-digest <hex>`
#[allow(clippy::too_many_arguments)]
pub fn create(
    home: &Path,
    store_path: &Path,
    project_ref: &str,
    branch: &str,
    version: &str,
    commit_ref: &str,
    artifact_path: &Path,
    recipe_digest_hex: &str,
) -> Result<CommandResult> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let human = identity.human_did();

    let project_id = project_alias::resolve(home, project_ref)?;
    let commit_id = ObjectId::parse(commit_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let bytes = std::fs::read(artifact_path).map_err(|e| CliError::Io(e.to_string()))?;
    let recipe_digest = parse_hex32(recipe_digest_hex, "--recipe-digest")?;

    let seq = sequence::next(home)?;
    let manifest = mini_media::publish_media(
        &mut store,
        &human,
        &identity.device,
        "application/octet-stream",
        &bytes,
        sequence::now_ms(),
        seq,
    )
    .map_err(|e| CliError::Media(e.to_string()))?;

    let seq = sequence::next(home)?;
    let obj = release(
        &mut store,
        &human,
        &identity.device,
        version,
        &project_id,
        branch,
        &commit_id,
        &manifest.id,
        manifest.digest,
        recipe_digest,
        sequence::now_ms(),
        seq,
    )
    .map_err(|e| CliError::Forge(e.to_string()))?;

    Ok(CommandResult::new(format!(
        "release {version:?} created: {}",
        obj.id().as_str()
    ))
    .field("release_id", JsonValue::str(obj.id().as_str()))
    .field("version", JsonValue::str(version))
    .field("artifact_digest", JsonValue::str(hex(&manifest.digest))))
}

/// `mini release attest <release-id> --artifact-digest <hex>`
pub fn attest_release(
    home: &Path,
    store_path: &Path,
    release_ref: &str,
    artifact_digest_hex: &str,
) -> Result<CommandResult> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let release_id = ObjectId::parse(release_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let digest = parse_hex32(artifact_digest_hex, "--artifact-digest")?;

    let seq = sequence::next(home)?;
    let obj = attest(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &release_id,
        digest,
        sequence::now_ms(),
        seq,
    )
    .map_err(|e| CliError::Forge(e.to_string()))?;

    Ok(
        CommandResult::new(format!("attestation recorded: {}", obj.id().as_str()))
            .field("attestation_id", JsonValue::str(obj.id().as_str()))
            .field("release_id", JsonValue::str(release_id.as_str())),
    )
}

/// Shared core of `mini release verify` and `mini installer stage`: neither
/// re-derives governance/attestation/timelock trust from a bare release id
/// alone -- both go through the same real `verify_governed_release` check
/// and get back the same [`VerifiedRelease`].
#[allow(clippy::too_many_arguments)]
pub fn verified_release(
    home: &Path,
    store_path: &Path,
    release_ref: &str,
    project_ref: &str,
    branch: &str,
    min_attestations: Option<u32>,
    timelock_ms: Option<u64>,
    now_ms: Option<u64>,
) -> Result<VerifiedRelease> {
    let identity = crate::identity::load_or_init(home)?;
    let store = open_store(store_path)?;
    let oracle = crate::store::build_oracle(home, &identity)?;

    let release_id = ObjectId::parse(release_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let project_id = project_alias::resolve(home, project_ref)?;
    let policy = ReleasePolicy {
        min_attestations: min_attestations.unwrap_or(ADOPTION_MIN_ATTESTATIONS),
        timelock_ms: timelock_ms.unwrap_or(ADOPTION_MIN_TIMELOCK_MS),
        now_ms: now_ms.unwrap_or_else(sequence::now_ms),
    };

    verify_governed_release(&store, &oracle, &release_id, &project_id, branch, &policy)
        .map_err(|e| CliError::Forge(e.to_string()))
}

/// `mini release verify <release-id> <project> --branch <b>
/// [--min-attestations N] [--timelock-ms N] [--now-ms N]`
#[allow(clippy::too_many_arguments)]
pub fn verify(
    home: &Path,
    store_path: &Path,
    release_ref: &str,
    project_ref: &str,
    branch: &str,
    min_attestations: Option<u32>,
    timelock_ms: Option<u64>,
    now_ms: Option<u64>,
) -> Result<CommandResult> {
    let verified = verified_release(
        home,
        store_path,
        release_ref,
        project_ref,
        branch,
        min_attestations,
        timelock_ms,
        now_ms,
    )?;

    Ok(CommandResult::new(format!(
        "verified: release {} version {:?}, {} independent attester(s), artifact digest {}",
        verified.id.as_str(),
        verified.version,
        verified.attesters,
        hex(&verified.artifact.digest)
    ))
    .field("release_id", JsonValue::str(verified.id.as_str()))
    .field("version", JsonValue::str(&verified.version))
    .field("attesters", JsonValue::num(verified.attesters as u64))
    .field(
        "artifact_digest",
        JsonValue::str(hex(&verified.artifact.digest)),
    ))
}

/// `mini release fetch <release-id> <project> --branch <b> --output <path>`.
///
/// Fetch is deliberately adoption-safe: it runs the complete governed release
/// verification first, then assembles the content-addressed artifact and
/// creates the destination file without overwriting an existing path.
#[allow(clippy::too_many_arguments)]
pub fn fetch(
    home: &Path,
    store_path: &Path,
    release_ref: &str,
    project_ref: &str,
    branch: &str,
    output: &Path,
    min_attestations: Option<u32>,
    timelock_ms: Option<u64>,
    now_ms: Option<u64>,
) -> Result<CommandResult> {
    let verified = verified_release(
        home,
        store_path,
        release_ref,
        project_ref,
        branch,
        min_attestations,
        timelock_ms,
        now_ms,
    )?;
    let store = open_store(store_path)?;
    let bytes = assemble(&store, &verified.artifact).map_err(|e| CliError::Media(e.to_string()))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| CliError::Io(e.to_string()))?;
    use std::io::Write;
    file.write_all(&bytes)
        .map_err(|e| CliError::Io(e.to_string()))?;

    Ok(CommandResult::new(format!(
        "verified release {} fetched to {}",
        verified.id.as_str(),
        output.display()
    ))
    .field("release_id", JsonValue::str(verified.id.as_str()))
    .field("output", JsonValue::str(&output.to_string_lossy()))
    .field("bytes", JsonValue::num(bytes.len() as u64))
    .field(
        "artifact_digest",
        JsonValue::str(hex(&verified.artifact.digest)),
    ))
}

/// `mini release list <project> --branch <b>` -- the transparency log is
/// the object store itself (D-0070); this just prints it.
pub fn list(
    home: &Path,
    store_path: &Path,
    project_ref: &str,
    branch: &str,
) -> Result<CommandResult> {
    let store = open_store(store_path)?;
    let project_id = project_alias::resolve(home, project_ref)?;
    let releases =
        list_releases(&store, &project_id, branch).map_err(|e| CliError::Forge(e.to_string()))?;
    let ids: Vec<String> = releases
        .iter()
        .map(|o| o.id().as_str().to_string())
        .collect();
    let human = if releases.is_empty() {
        format!("no releases recorded for {project_ref:?} / {branch}")
    } else {
        let mut out = String::new();
        for id in &ids {
            out.push_str(&format!("{id}\n"));
        }
        out
    };
    Ok(CommandResult::new(human).field("release_ids", JsonValue::strs(ids)))
}
