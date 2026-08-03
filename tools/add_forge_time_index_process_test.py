#!/usr/bin/env python3
"""Add real subprocess coverage for the ordered time-index lock."""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TESTS = ROOT / "crates/mini-store/tests/time_pages.rs"
PLANNING = ROOT / "docs/planning/forge-bounded-fs-index-pages.md"
DECISIONS = ROOT / "docs/DECISION_LOG.md"
STATUS = ROOT / "docs/STATUS.md"
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/forge-time-index-process-test.yml"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one target, found {count}: {old[:120]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


text = TESTS.read_text(encoding="utf-8")
anchor = "#[cfg(unix)]\n#[test]\nfn a_symlinked_ordered_index_is_refused()"
process_tests = r'''const PROCESS_WRITER_ROOT: &str = "MININET_TIME_PAGE_PROCESS_ROOT";
const PROCESS_WRITER_SEED: &str = "MININET_TIME_PAGE_PROCESS_SEED";
const PROCESS_WRITER_TIMESTAMP: &str = "MININET_TIME_PAGE_PROCESS_TIMESTAMP";
const PROCESS_WRITER_SEQUENCE: &str = "MININET_TIME_PAGE_PROCESS_SEQUENCE";

fn process_object(seed: u8, timestamp_ms: u64, sequence: u64) -> Object {
    let controller = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(
            format!("process-post-{seed}-{sequence}").into_bytes(),
        ))
        .sign(&controller.did(), &controller)
        .unwrap()
}

#[test]
fn process_writer_child() {
    let Some(root) = std::env::var_os(PROCESS_WRITER_ROOT) else {
        return;
    };
    let seed: u8 = std::env::var(PROCESS_WRITER_SEED)
        .unwrap()
        .parse()
        .unwrap();
    let timestamp_ms: u64 = std::env::var(PROCESS_WRITER_TIMESTAMP)
        .unwrap()
        .parse()
        .unwrap();
    let sequence: u64 = std::env::var(PROCESS_WRITER_SEQUENCE)
        .unwrap()
        .parse()
        .unwrap();

    let object = process_object(seed, timestamp_ms, sequence);
    let mut store = Store::new(FsBackend::open(std::path::Path::new(&root)).unwrap());
    store.insert(&object).unwrap();
}

#[test]
fn independent_process_writers_preserve_every_time_row() {
    let root = temp_root("process-writers");
    let executable = std::env::current_exe().unwrap();
    let specs: Vec<(u8, u64, u64)> = (0u8..8)
        .map(|index| {
            (
                80 + index,
                [40u64, 10, 30, 20, 20, 50, 5, 30][index as usize],
                u64::from(index) + 1,
            )
        })
        .collect();

    let mut children = Vec::new();
    for (seed, timestamp_ms, sequence) in &specs {
        children.push(
            std::process::Command::new(&executable)
                .arg("--exact")
                .arg("process_writer_child")
                .arg("--nocapture")
                .env(PROCESS_WRITER_ROOT, &root)
                .env(PROCESS_WRITER_SEED, seed.to_string())
                .env(PROCESS_WRITER_TIMESTAMP, timestamp_ms.to_string())
                .env(PROCESS_WRITER_SEQUENCE, sequence.to_string())
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }

    let store = Store::new(FsBackend::open(&root).unwrap());
    let actual = collect_pages(&store, 0, 2);
    let mut expected: Vec<(u64, String)> = specs
        .into_iter()
        .map(|(seed, timestamp_ms, sequence)| {
            (
                timestamp_ms,
                process_object(seed, timestamp_ms, sequence)
                    .id()
                    .as_str()
                    .to_string(),
            )
        })
        .collect();
    expected.sort();
    assert_eq!(
        actual,
        expected.into_iter().map(|(_, id)| id).collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn a_symlinked_ordered_index_is_refused()'''
if text.count(anchor) != 1:
    raise SystemExit(f"expected one symlink-test anchor, found {text.count(anchor)}")
TESTS.write_text(text.replace(anchor, process_tests, 1), encoding="utf-8")

replace_once(
    PLANNING,
    "- Independent filesystem writers converge under the OS lock.\n",
    "- Eight independently spawned test processes write one shared filesystem store\n"
    "  concurrently and preserve every canonical time row under the OS lock.\n",
)
replace_once(
    DECISIONS,
    "out-of-order writes, reopen, concurrent writers, legacy rebuild, partial tails,\n",
    "out-of-order writes, reopen, real cross-process writers, legacy rebuild, partial tails,\n",
)
replace_once(
    STATUS,
    "  writers, rebuild, partial tails, missing bases, journal recovery, compaction,\n",
    "  threads and independently spawned process writers, rebuild, partial tails,\n"
    "  missing bases, journal recovery, compaction,\n",
)

SELF.unlink()
WORKFLOW.unlink()
run("cargo", "fmt", "--all")
run("cargo", "test", "-p", "mini-store", "--all-features")
run(
    "cargo",
    "clippy",
    "-p",
    "mini-store",
    "--all-targets",
    "--all-features",
    "--",
    "-D",
    "warnings",
)
run("python3", "tools/mininet_nav.py", "build")
