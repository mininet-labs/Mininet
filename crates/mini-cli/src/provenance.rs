//! `mini provenance ...` — record and verify build-provenance claims.
//! Thin wrapper over `mini_provenance::record_provenance`/`list_provenance`/
//! `independent_agreement`, matching `crate::release`'s pattern of parsed
//! arguments in, formatted report out.

use std::path::Path;

use mini_provenance::{independent_agreement, record_provenance, BuildProvenance};

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};
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

/// `mini provenance record <subject-id> --environment-digest <hex>
/// --commands-digest <hex> --output <hex>... --group <label>
/// [--network-enabled] --started-ms <n> --finished-ms <n>`
#[allow(clippy::too_many_arguments)]
pub fn record(
    home: &Path,
    store_path: &Path,
    subject_ref: &str,
    environment_digest_hex: &str,
    commands_digest_hex: &str,
    output_digest_hexes: &[String],
    reproducibility_group: &str,
    network_enabled: bool,
    started_ms: u64,
    finished_ms: u64,
) -> Result<CommandResult> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;

    let subject =
        mini_objects::ObjectId::parse(subject_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let environment_digest = parse_hex32(environment_digest_hex, "--environment-digest")?;
    let commands_digest = parse_hex32(commands_digest_hex, "--commands-digest")?;
    if output_digest_hexes.is_empty() {
        return Err(CliError::Usage(
            "at least one --output <hex> is required".to_string(),
        ));
    }
    let mut output_digests = Vec::with_capacity(output_digest_hexes.len());
    for h in output_digest_hexes {
        output_digests.push(parse_hex32(h, "--output")?);
    }

    let provenance = BuildProvenance {
        environment_digest,
        commands_digest,
        output_digests: output_digests.clone(),
        reproducibility_group: reproducibility_group.to_string(),
        network_enabled,
        started_ms,
        finished_ms,
    };

    let seq = sequence::next(home)?;
    let obj = record_provenance(
        &mut store,
        &identity.human_did(),
        &identity.device,
        &subject,
        &provenance,
        sequence::now_ms(),
        seq,
    )
    .map_err(|e| CliError::Provenance(e.to_string()))?;

    Ok(CommandResult::new(format!(
        "provenance recorded: {}\n  subject: {}\n  {} output digest(s)",
        obj.id().as_str(),
        subject.as_str(),
        output_digests.len()
    ))
    .field("provenance_id", JsonValue::str(obj.id().as_str()))
    .field("subject_id", JsonValue::str(subject.as_str()))
    .field("output_count", JsonValue::num(output_digests.len() as u64)))
}

/// `mini provenance verify <subject-id> --output <hex> [--min-agreement N]`
pub fn verify(
    home: &Path,
    store_path: &Path,
    subject_ref: &str,
    output_digest_hex: &str,
    min_agreement: u32,
) -> Result<CommandResult> {
    let identity = crate::identity::load_or_init(home)?;
    let store = open_store(store_path)?;
    let oracle = crate::store::build_oracle(home, &identity)?;

    let subject =
        mini_objects::ObjectId::parse(subject_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let expected_output = parse_hex32(output_digest_hex, "--output")?;

    let agreement = independent_agreement(&store, &oracle, &subject, expected_output)
        .map_err(|e| CliError::Provenance(e.to_string()))?;

    if agreement < min_agreement {
        return Err(CliError::Provenance(format!(
            "only {agreement} independent identity root(s) agree on output {}, need {min_agreement}",
            hex(&expected_output)
        )));
    }

    Ok(CommandResult::new(format!(
        "verified: {agreement} independent identity root(s) agree on output {} for subject {}",
        hex(&expected_output),
        subject.as_str()
    ))
    .field("subject_id", JsonValue::str(subject.as_str()))
    .field("output_digest", JsonValue::str(hex(&expected_output)))
    .field("agreement", JsonValue::num(agreement as u64))
    .field("min_agreement", JsonValue::num(min_agreement as u64)))
}
