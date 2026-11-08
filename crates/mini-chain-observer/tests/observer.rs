//! Adversarial tests for the read-only observer (research T23-T25, T18).

use std::collections::BTreeSet;

use mini_chain_observer::{
    Assessment, Chain, FinalityProfile, Observer, ProfileError, Receipt, ReceiptJournal, Refusal,
    Seen, SourceReport, Status, Transfer,
};

const NOW: u64 = 100_000_000;
const TX: [u8; 32] = [7; 32];

fn src(n: u8) -> [u8; 32] {
    [n; 32]
}

fn sources(ids: &[u8]) -> BTreeSet<[u8; 32]> {
    ids.iter().map(|n| src(*n)).collect()
}

fn xrpl() -> Observer {
    Observer::new(FinalityProfile::xrpl(0, sources(&[1, 2, 3]))).unwrap()
}

fn monero() -> Observer {
    Observer::new(FinalityProfile::monero(0, sources(&[1, 2, 3]))).unwrap()
}

fn transfer(chain: Chain, amount: u64) -> Transfer {
    Transfer {
        chain,
        network: 0,
        tx_hash: TX,
        asset: [0; 32],
        destination_hash: [9; 32],
        amount_minor: amount,
    }
}

fn included(
    source: u8,
    tip: u64,
    height: u64,
    block: u8,
    validated: bool,
    at: u64,
) -> SourceReport {
    SourceReport {
        source: src(source),
        transfer_hash: TX,
        tip_height: tip,
        seen: Seen::Included {
            height,
            block_hash: [block; 32],
            validated,
        },
        observed_at_ms: at,
    }
}

#[test]
fn t23_one_source_alone_can_never_finalize_a_fake_deposit() {
    let mut o = xrpl();
    o.record(
        transfer(Chain::Xrpl, 5),
        included(1, 110, 100, 1, true, NOW),
        NOW,
    )
    .unwrap();
    let a = o.assess(&TX, NOW);
    assert_eq!(a.status, Status::Included);
    assert!(!a.status.is_final_evidence());
}

#[test]
fn two_independent_agreeing_validated_sources_finalize_xrpl() {
    let mut o = xrpl();
    for s in [1, 2] {
        o.record(
            transfer(Chain::Xrpl, 5),
            included(s, 110, 100, 1, true, NOW),
            NOW,
        )
        .unwrap();
    }
    let a = o.assess(&TX, NOW);
    assert_eq!(a.status, Status::Finalized);
    assert_eq!(a.agreeing_sources, 2);
    assert_eq!(a.inclusion, Some((100, [1; 32])));
}

#[test]
fn an_unvalidated_xrpl_ledger_is_not_final_however_many_sources_agree() {
    let mut o = xrpl();
    for s in [1, 2, 3] {
        o.record(
            transfer(Chain::Xrpl, 5),
            included(s, 110, 100, 1, false, NOW),
            NOW,
        )
        .unwrap();
    }
    assert_eq!(o.assess(&TX, NOW).status, Status::Included);
}

#[test]
fn a_duplicate_source_counts_once() {
    let mut o = xrpl();
    for at in [NOW - 2, NOW - 1, NOW] {
        o.record(
            transfer(Chain::Xrpl, 5),
            included(1, 110, 100, 1, true, at),
            NOW,
        )
        .unwrap();
    }
    assert_eq!(o.assess(&TX, NOW).status, Status::Included);
}

#[test]
fn unknown_sources_and_other_networks_are_refused() {
    let mut o = xrpl();
    assert_eq!(
        o.record(
            transfer(Chain::Xrpl, 5),
            included(9, 110, 100, 1, true, NOW),
            NOW
        ),
        Err(Refusal::UnknownSource)
    );
    let mut other = transfer(Chain::Xrpl, 5);
    other.network = 1; // testnet reported to a mainnet observer
    assert_eq!(
        o.record(other, included(1, 110, 100, 1, true, NOW), NOW),
        Err(Refusal::WrongNetwork)
    );
    assert_eq!(
        o.record(
            transfer(Chain::Monero, 5),
            included(1, 110, 100, 1, true, NOW),
            NOW
        ),
        Err(Refusal::WrongNetwork)
    );
    assert_eq!(o.assess(&TX, NOW).status, Status::Unobserved);
}

#[test]
fn a_lying_source_that_changes_the_amount_makes_the_transfer_ambiguous_never_final() {
    let mut o = xrpl();
    o.record(
        transfer(Chain::Xrpl, 5),
        included(1, 110, 100, 1, true, NOW),
        NOW,
    )
    .unwrap();
    o.record(
        transfer(Chain::Xrpl, 5),
        included(2, 110, 100, 1, true, NOW),
        NOW,
    )
    .unwrap();
    o.record(
        transfer(Chain::Xrpl, 5_000),
        included(3, 110, 100, 1, true, NOW),
        NOW,
    )
    .unwrap();
    assert_eq!(o.assess(&TX, NOW).status, Status::Ambiguous);
}

#[test]
fn t24_sources_disagreeing_about_the_block_are_ambiguous() {
    let mut o = monero();
    o.record(
        transfer(Chain::Monero, 5),
        included(1, 200, 100, 1, true, NOW),
        NOW,
    )
    .unwrap();
    o.record(
        transfer(Chain::Monero, 5),
        included(2, 200, 100, 2, true, NOW),
        NOW,
    )
    .unwrap();
    assert_eq!(o.assess(&TX, NOW).status, Status::Ambiguous);
}

#[test]
fn monero_needs_its_confirmation_depth() {
    let mut o = monero();
    for s in [1, 2] {
        o.record(
            transfer(Chain::Monero, 5),
            included(s, 105, 100, 1, true, NOW),
            NOW,
        )
        .unwrap();
    }
    assert_eq!(o.assess(&TX, NOW).status, Status::Included);
    for s in [1, 2] {
        o.record(
            transfer(Chain::Monero, 5),
            included(s, 110, 100, 1, true, NOW + 1),
            NOW + 1,
        )
        .unwrap();
    }
    assert_eq!(o.assess(&TX, NOW + 1).status, Status::Finalized);
}

#[test]
fn a_failed_transaction_is_never_promoted_and_mixed_reports_are_ambiguous() {
    let mut o = xrpl();
    let failed = |s: u8| SourceReport {
        source: src(s),
        transfer_hash: TX,
        tip_height: 110,
        seen: Seen::Failed,
        observed_at_ms: NOW,
    };
    for s in [1, 2] {
        o.record(transfer(Chain::Xrpl, 5), failed(s), NOW).unwrap();
    }
    assert_eq!(o.assess(&TX, NOW).status, Status::Failed);
    o.record(
        transfer(Chain::Xrpl, 5),
        included(3, 110, 100, 1, true, NOW),
        NOW,
    )
    .unwrap();
    assert_eq!(o.assess(&TX, NOW).status, Status::Ambiguous);
}

#[test]
fn clock_drift_cannot_hold_a_report_open_or_fake_freshness() {
    let mut o = xrpl();
    assert_eq!(
        o.record(
            transfer(Chain::Xrpl, 5),
            included(1, 110, 100, 1, true, NOW + 10_000_000),
            NOW
        ),
        Err(Refusal::FromTheFuture)
    );
    assert_eq!(
        o.record(
            transfer(Chain::Xrpl, 5),
            included(1, 110, 100, 1, true, NOW - 10_000_000),
            NOW
        ),
        Err(Refusal::Stale)
    );
    // A report that was fine goes stale and stops counting.
    o.record(
        transfer(Chain::Xrpl, 5),
        included(1, 110, 100, 1, true, NOW),
        NOW,
    )
    .unwrap();
    o.record(
        transfer(Chain::Xrpl, 5),
        included(2, 110, 100, 1, true, NOW),
        NOW,
    )
    .unwrap();
    assert_eq!(o.assess(&TX, NOW).status, Status::Finalized);
    assert_eq!(o.assess(&TX, NOW + 10_000_000).status, Status::Unobserved);
}

fn finalized() -> Assessment {
    Assessment {
        status: Status::Finalized,
        agreeing_sources: 2,
        inclusion: Some((100, [1; 32])),
    }
}

#[test]
fn the_journal_is_idempotent_durable_and_survives_restart() {
    let dir = tempdir("journal-restart");
    let path = dir.join("receipts");
    let mut journal = ReceiptJournal::open(&path).unwrap();
    journal.record(Chain::Xrpl, TX, &finalized()).unwrap();
    journal.record(Chain::Xrpl, TX, &finalized()).unwrap();
    drop(journal);
    let reopened = ReceiptJournal::open(&path).unwrap();
    let receipt: &Receipt = reopened.get(&TX).unwrap();
    assert_eq!(receipt.status, Status::Finalized);
    assert_eq!(receipt.inclusion, Some((100, [1; 32])));
    assert!(!receipt.contradicted_after_finality);
}

#[test]
fn a_finalized_receipt_contradicted_later_is_marked_forever_never_erased() {
    let dir = tempdir("journal-reorg");
    let path = dir.join("receipts");
    let mut journal = ReceiptJournal::open(&path).unwrap();
    journal.record(Chain::Xrpl, TX, &finalized()).unwrap();
    let reorged = Assessment {
        status: Status::Finalized,
        agreeing_sources: 2,
        inclusion: Some((101, [2; 32])),
    };
    let receipt = journal.record(Chain::Xrpl, TX, &reorged).unwrap().clone();
    assert_eq!(receipt.status, Status::Ambiguous);
    assert!(receipt.contradicted_after_finality);
    // Even a later "finalized again" cannot clear it.
    let again = journal
        .record(Chain::Xrpl, TX, &finalized())
        .unwrap()
        .clone();
    assert!(again.contradicted_after_finality);
    assert!(!again.status.is_final_evidence());
    drop(journal);
    let reopened = ReceiptJournal::open(&path).unwrap();
    assert!(reopened.get(&TX).unwrap().contradicted_after_finality);
}

#[test]
fn a_corrupt_journal_fails_closed() {
    let dir = tempdir("journal-corrupt");
    let path = dir.join("receipts");
    std::fs::write(&path, "not a receipt\n").unwrap();
    assert!(ReceiptJournal::open(&path).is_err());
}

#[test]
fn the_observer_crate_has_no_key_or_value_dependencies() {
    let manifest = include_str!("../Cargo.toml");
    let manifest = manifest.split("[dependencies]").nth(1).unwrap();
    for forbidden in [
        "mini-value",
        "mini-treasury",
        "mini-custody",
        "mini-forge",
        "mini-chain",
        "mini-settlement",
        "mini-uniqueness",
        "mini-crypto",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "an observer holds no keys and touches no value: {forbidden}"
        );
    }
}

fn tempdir(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-chain-observer-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn a_profile_that_could_finalize_on_no_evidence_or_never_is_refused() {
    let mut zero = FinalityProfile::xrpl(0, sources(&[1, 2, 3]));
    zero.min_sources = 0;
    assert_eq!(Observer::new(zero).unwrap_err(), ProfileError::NoQuorum);
    let mut too_few = FinalityProfile::xrpl(0, sources(&[1]));
    too_few.min_sources = 2;
    assert_eq!(
        Observer::new(too_few).unwrap_err(),
        ProfileError::TooFewSources
    );
}

#[test]
fn report_expiry_or_lost_quorum_does_not_contradict_a_finalized_receipt() {
    let dir = tempdir("journal-expiry");
    let mut journal = ReceiptJournal::open(dir.join("receipts")).unwrap();
    journal.record(Chain::Xrpl, TX, &finalized()).unwrap();
    for status in [Status::Unobserved, Status::Observed] {
        let quiet = Assessment {
            status,
            agreeing_sources: 0,
            inclusion: None,
        };
        let receipt = journal.record(Chain::Xrpl, TX, &quiet).unwrap();
        assert_eq!(receipt.status, Status::Finalized);
        assert!(!receipt.contradicted_after_finality);
    }
    // Fewer sources agreeing on the SAME block is also not a contradiction.
    let thinner = Assessment {
        status: Status::Included,
        agreeing_sources: 1,
        inclusion: Some((100, [1; 32])),
    };
    let receipt = journal.record(Chain::Xrpl, TX, &thinner).unwrap();
    assert_eq!(receipt.status, Status::Finalized);
    // An explicit ambiguity or failure is real contradictory evidence.
    let failed = Assessment {
        status: Status::Failed,
        agreeing_sources: 2,
        inclusion: None,
    };
    let receipt = journal.record(Chain::Xrpl, TX, &failed).unwrap();
    assert!(receipt.contradicted_after_finality);
}

#[test]
fn a_failed_journal_write_changes_nothing_and_a_retry_writes_it() {
    let dir = tempdir("journal-retry");
    let parent = dir.join("missing");
    let path = parent.join("receipts");
    let mut journal = ReceiptJournal::open(&path).unwrap();
    assert_eq!(
        journal.record(Chain::Xrpl, TX, &finalized()).unwrap_err(),
        Refusal::JournalWrite
    );
    assert!(
        journal.get(&TX).is_none(),
        "a failed write must not leave a receipt"
    );
    std::fs::create_dir_all(&parent).unwrap();
    journal.record(Chain::Xrpl, TX, &finalized()).unwrap();
    drop(journal);
    let reopened = ReceiptJournal::open(&path).unwrap();
    assert_eq!(reopened.get(&TX).unwrap().status, Status::Finalized);
}
