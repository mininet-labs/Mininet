//! The Go-Live transition is one-way, ordered, tamper-evident and identity-free.

use mini_bootstrap_state::{
    replay, BootstrapState, FounderIntegrationEnded, Kind, Log, StateError, RECORD_BYTES,
};

const OUTAGE_PROOF: [u8; 32] = [7; 32];
const CLOSURE: [u8; 32] = [9; 32];

fn full_log() -> Log {
    let mut log = Log::new();
    log.append(Kind::ForgeCanonical, OUTAGE_PROOF).unwrap();
    log.append(Kind::GoLive, CLOSURE).unwrap();
    log
}

#[test]
fn it_starts_pre_go_live_with_the_pause_and_founder_path_on() {
    let state = Log::new().state();
    assert_eq!(state, BootstrapState::pre_go_live());
    assert!(!state.forge_canonical() && !state.go_live());
    assert!(state.governance_pause_active());
    assert!(state.require_founder_integration().is_ok());
}

#[test]
fn go_live_needs_forge_canonical_first_and_each_happens_once() {
    let mut log = Log::new();
    assert_eq!(
        log.append(Kind::GoLive, CLOSURE),
        Err(StateError::GoLiveBeforeForgeCanonical)
    );
    log.append(Kind::ForgeCanonical, OUTAGE_PROOF).unwrap();
    assert!(log.state().forge_canonical() && !log.state().go_live());
    assert!(log.state().require_founder_integration().is_ok());
    assert_eq!(
        log.append(Kind::ForgeCanonical, OUTAGE_PROOF),
        Err(StateError::AlreadyDone)
    );
    log.append(Kind::GoLive, CLOSURE).unwrap();
    assert!(log.state().go_live());
}

#[test]
fn after_go_live_the_founder_path_and_pause_are_gone_for_good() {
    let mut log = full_log();
    let state = log.state();
    assert!(!state.governance_pause_active());
    assert!(!state.founder_integration_enabled());
    assert_eq!(
        state.require_founder_integration(),
        Err(FounderIntegrationEnded)
    );
    // Nothing can follow Go-Live: no re-enable, no undo, no repeat.
    for kind in [Kind::ForgeCanonical, Kind::GoLive] {
        assert_eq!(log.append(kind, [3; 32]), Err(StateError::AfterGoLive));
    }
    assert!(log.state().go_live(), "state unchanged by the refusals");
}

#[test]
fn empty_evidence_is_refused() {
    let mut log = Log::new();
    assert_eq!(
        log.append(Kind::ForgeCanonical, [0; 32]),
        Err(StateError::EmptyEvidence)
    );
    assert!(!log.state().forge_canonical());
}

#[test]
fn a_log_round_trips_and_state_is_recomputed_not_read() {
    let log = full_log();
    let bytes = log.encode();
    assert_eq!(bytes.len(), 2 * RECORD_BYTES);
    let again = replay(&bytes).unwrap();
    assert_eq!(again.state(), log.state());
    assert_eq!(again.encode(), bytes);
    // The empty log is the pre-Go-Live log.
    assert_eq!(replay(&[]).unwrap().state(), BootstrapState::pre_go_live());
}

#[test]
fn every_tampering_of_a_log_is_refused() {
    let bytes = full_log().encode();

    // Truncation, padding, unknown kind.
    assert_eq!(
        replay(&bytes[..bytes.len() - 1]).unwrap_err(),
        StateError::Malformed
    );
    let mut padded = bytes.clone();
    padded.push(0);
    assert_eq!(replay(&padded).unwrap_err(), StateError::Malformed);
    let mut unknown = bytes.clone();
    unknown[0] = 9;
    assert_eq!(replay(&unknown).unwrap_err(), StateError::Malformed);

    // Dropping the first record leaves a Go-Live with no Forge canonicality.
    assert_eq!(
        replay(&bytes[RECORD_BYTES..]).unwrap_err(),
        StateError::GoLiveBeforeForgeCanonical
    );
    // Reordering is refused.
    let mut swapped = bytes[RECORD_BYTES..].to_vec();
    swapped.extend_from_slice(&bytes[..RECORD_BYTES]);
    assert!(replay(&swapped).is_err());
    // Editing the first record's evidence breaks the second record's link.
    let mut edited = bytes.clone();
    edited[1] ^= 1;
    assert_eq!(replay(&edited).unwrap_err(), StateError::BrokenChain);
    // Editing the link itself is caught too.
    let mut relinked = bytes.clone();
    relinked[RECORD_BYTES + 40] ^= 1;
    assert_eq!(replay(&relinked).unwrap_err(), StateError::BrokenChain);
    // A longer history than the two legal records, or a repeat, is refused.
    let mut repeated = bytes.clone();
    repeated.extend_from_slice(&bytes[..RECORD_BYTES]);
    assert_eq!(replay(&repeated).unwrap_err(), StateError::Malformed);
    let mut twice = bytes[..RECORD_BYTES].to_vec();
    twice.extend_from_slice(&bytes[..RECORD_BYTES]);
    assert!(replay(&twice).is_err());
}

#[test]
fn records_carry_no_identity_fields() {
    // A record is exactly kind + evidence digest + previous digest.
    assert_eq!(RECORD_BYTES, 1 + 32 + 32);
    let manifest = include_str!("../Cargo.toml");
    let deps = manifest.split("[dependencies]").nth(1).unwrap();
    for forbidden in [
        "did-mini",
        "mini-objects",
        "mini-forge",
        "mini-value",
        "mini-treasury",
        "mini-beta",
        "mini-chain",
        "mini-uniqueness",
        "mini-crypto",
    ] {
        assert!(
            !deps.contains(forbidden),
            "{forbidden} must not be a dependency"
        );
    }
}
