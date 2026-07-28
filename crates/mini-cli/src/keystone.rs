//! `mini keystone run` — the CLI entry point for the identity → channel →
//! presence → reward proof point (D-0369; `docs/BETA_STATUS.md` item 4,
//! "standalone CLI harness"). Before this, that flow was only reachable via
//! `cargo run -p mini-keystone --example keystone`, not the actual `mini`
//! binary a developer or tester runs; `mini repo`/`mini pr`/`mini build`/
//! `mini release`/`mini installer` already cover the rest of item 4's
//! named chain (forge PR → merge → release → verify) as their own
//! subcommands — see `tools/no_github_outage_demo.sh` (D-0081) for a
//! script that already drives that half end to end.
//!
//! Two local identities are involved: this invocation's own `--home` and a
//! second `--peer-home` (also a real, persisted `mini` home — created with
//! `identity::load_or_init` exactly like every other command here). Both
//! run in this one process for the same reason `mini-keystone`'s own
//! example does — see that crate's doc comment on why it deliberately
//! reports two separate views rather than one combined dashboard, which
//! this command mirrors in its own human-readable output below.
//!
//! Each home also gets its own durable [`FileReplayGuard`] (D-0366/D-0367),
//! opened at `<home>/replay-guard.log` — so running this command twice
//! against the same two homes exercises the actual persistence property,
//! not just an in-memory guard that forgets everything when the process
//! exits.

use std::path::Path;

use mini_bearer::pair;
use mini_keystone::{run_demo, DemoReport, Participant};
use mini_presence::{FileReplayGuard, TransportKind};

use crate::identity;
use crate::json::{CommandResult, JsonValue};
use crate::CliError;

/// Entries older than this are dropped from each home's replay guard on
/// open (D-0366's `retention_ms`) — one day, matching
/// `mini_presence::RangePolicy::ble_default`'s own `max_age_ms`.
const REPLAY_RETENTION_MS: u64 = 86_400_000;

fn participant_from(identity: identity::Identity) -> Participant {
    Participant {
        root: identity.human,
        device: identity.device,
    }
}

/// `mini keystone run --peer-home <path> [--now-ms N]`
pub fn run(home: &Path, peer_home: &Path, now_ms: u64) -> Result<CommandResult, CliError> {
    if home == peer_home {
        return Err(CliError::Usage(
            "--peer-home must be a different path from --home (two distinct identities)"
                .to_string(),
        ));
    }

    let a = participant_from(identity::load_or_init(home)?);
    let b = participant_from(identity::load_or_init(peer_home)?);

    let (mut bearer_a, mut bearer_b) = pair();

    let mut guard_a = FileReplayGuard::open(home.join("replay-guard.log"), REPLAY_RETENTION_MS)
        .map_err(|e| CliError::Keystone(e.to_string()))?;
    let mut guard_b =
        FileReplayGuard::open(peer_home.join("replay-guard.log"), REPLAY_RETENTION_MS)
            .map_err(|e| CliError::Keystone(e.to_string()))?;

    let report: DemoReport = run_demo(
        &a,
        &b,
        &mut bearer_a,
        &mut bearer_b,
        TransportKind::InProcess,
        now_ms,
        &mut guard_a,
        &mut guard_b,
    )
    .map_err(|e| CliError::Keystone(e.to_string()))?;

    Ok(render(&report))
}

fn render(report: &DemoReport) -> CommandResult {
    let binding_hex = hex_encode(&report.channel_binding);
    let human = format!(
        "keystone demo complete -- identity verified offline, presence range-bound & mutually signed\n\
         --- what the initiator's own device shows its owner ---\n\
         identity root   : {}\n\
         accrued reward  : {} points ({} vested -- value matures slowly, P4)\n\
         --- what the responder's own device shows its owner ---\n\
         identity root   : {}\n\
         accrued reward  : {} points ({} vested)\n\
         channel binding (shared, not identity) : {}",
        report.initiator_root,
        report.initiator_account.accrued_points,
        report.initiator_account.vested_points,
        report.responder_root,
        report.responder_account.accrued_points,
        report.responder_account.vested_points,
        binding_hex,
    );
    CommandResult::new(human)
        .field(
            "initiator_root",
            JsonValue::str(report.initiator_root.clone()),
        )
        .field(
            "responder_root",
            JsonValue::str(report.responder_root.clone()),
        )
        .field("channel_binding", JsonValue::str(binding_hex))
        .field(
            "initiator_accrued_points",
            JsonValue::num(report.initiator_account.accrued_points),
        )
        .field(
            "initiator_vested_points",
            JsonValue::num(report.initiator_account.vested_points),
        )
        .field(
            "responder_accrued_points",
            JsonValue::num(report.responder_account.accrued_points),
        )
        .field(
            "responder_vested_points",
            JsonValue::num(report.responder_account.vested_points),
        )
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            use std::fmt::Write;
            let _ = write!(out, "{b:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tempdir(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "mini-cli-keystone-test-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    #[test]
    fn run_creates_both_identities_and_reports_accrual() {
        let home = tempdir("a");
        let peer = tempdir("b");

        let result = run(&home, &peer, 1_000_000).unwrap();
        assert!(result.human.contains("keystone demo complete"));

        // Both identities are real, persisted homes now.
        assert!(identity::load(&home).is_ok());
        assert!(identity::load(&peer).is_ok());

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&peer);
    }

    #[test]
    fn running_twice_against_the_same_homes_reuses_identities() {
        let home = tempdir("c");
        let peer = tempdir("d");

        let first = run(&home, &peer, 1_000_000).unwrap();
        let second = run(&home, &peer, 1_000_000).unwrap();
        // Same identity roots both times (persisted, not re-created).
        let root_of = |r: &CommandResult| {
            r.fields
                .iter()
                .find(|(k, _)| *k == "initiator_root")
                .cloned()
        };
        assert_eq!(root_of(&first), root_of(&second));

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&peer);
    }

    #[test]
    fn same_home_for_both_sides_is_rejected() {
        let home = tempdir("e");
        let err = run(&home, &home, 1_000_000).unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
        let _ = std::fs::remove_dir_all(&home);
    }
}
