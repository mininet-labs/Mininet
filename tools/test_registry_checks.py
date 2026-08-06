"""Tests for the decision-registry and wire-limit checks.

Each test reconstructs the shape of a real incident rather than an invented
one, so a future change that breaks the check fails against the thing the
check exists to catch.
"""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

import check_decisions
import check_wire_limits


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def entry(identifier: str, body: str = "body", status: str = "Accepted") -> str:
    return f"### {identifier} — title  ·  *{status}*\n\n{body}\n\n"


class DecisionLogRepo:
    """A throwaway git repo with a decision log and a claims registry."""

    def __init__(self, stack: tempfile.TemporaryDirectory) -> None:
        self.root = Path(stack.name)
        self._git("init", "-q")
        self._git("config", "user.email", "test@example.invalid")
        self._git("config", "user.name", "test")

    def _git(self, *args: str) -> None:
        subprocess.run(["git", "-C", str(self.root), *args], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def commit(self, log_body: str, claims: dict | None = None, message: str = "c") -> str:
        write(self.root / check_decisions.DECISION_LOG, log_body)
        write(
            self.root / check_decisions.WORK_CLAIMS,
            json.dumps(claims or {"claims": []}, indent=2) + "\n",
        )
        self._git("add", "-A")
        self._git("commit", "-q", "-m", message)
        return subprocess.check_output(
            ["git", "-C", str(self.root), "rev-parse", "HEAD"], text=True
        ).strip()

    def run(self, canonical: str | None = None) -> int:
        return check_decisions.run(self.root, canonical, report_existing=False)


class DecisionRegistryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.stack = tempfile.TemporaryDirectory()
        self.addCleanup(self.stack.cleanup)
        self.repo = DecisionLogRepo(self.stack)

    def test_deleting_a_merged_decision_fails(self):
        """PR #296's actual failure: a branch predating D-0437 merged cleanly
        and took the entry with it. Nothing else caught this."""
        base = self.repo.commit(entry("D-0001") + entry("D-0002"))
        self.repo.commit(entry("D-0001"))  # D-0002 dropped
        self.assertEqual(self.repo.run(canonical=base), 1)

    def test_superseding_without_deleting_passes(self):
        """The legitimate shape: keep the old entry, add a new one."""
        base = self.repo.commit(entry("D-0001") + entry("D-0002"))
        self.repo.commit(entry("D-0001") + entry("D-0002") + entry("D-0003"))
        self.assertEqual(self.repo.run(canonical=base), 0)

    def test_status_truth_sync_warns_but_does_not_fail(self):
        """Real edits like 'complete in draft PR #292' -> 'merged through PR
        #292' must not block, or the check gets routinely bypassed."""
        base = self.repo.commit(entry("D-0001", body="status: draft"))
        self.repo.commit(entry("D-0001", body="status: merged"))
        self.assertEqual(self.repo.run(canonical=base), 0)

    def test_duplicate_heading_fails(self):
        base = self.repo.commit(entry("D-0001"))
        self.repo.commit(entry("D-0001") + entry("D-0002") + entry("D-0002"))
        self.assertEqual(self.repo.run(canonical=base), 1)

    def test_pre_existing_duplicate_does_not_fail_unrelated_work(self):
        """D-0372 is already duplicated upstream. Unrelated branches must not
        inherit a permanent red build for it."""
        doubled = entry("D-0001") + entry("D-0001")
        base = self.repo.commit(doubled)
        self.repo.commit(doubled + entry("D-0002"))
        self.assertEqual(self.repo.run(canonical=base), 0)

    def test_two_open_claims_on_one_number_fails(self):
        """The D-0438 race, caught at claim time instead of at review."""
        claims = {
            "claims": [
                {"status": "in_review", "branch": "a", "decision_ids": ["D-0009"]},
                {"status": "in_review", "branch": "b", "decision_ids": ["D-0009"]},
            ]
        }
        self.repo.commit(entry("D-0009"), claims)
        self.assertEqual(self.repo.run(), 1)

    def test_claiming_an_already_merged_number_fails(self):
        """Either the claim is stale or the number was taken first; both need
        action before review."""
        base = self.repo.commit(entry("D-0001"))
        claims = {
            "claims": [{"status": "in_review", "branch": "b", "decision_ids": ["D-0001"]}]
        }
        self.repo.commit(entry("D-0001", body="different body"), claims)
        self.assertEqual(self.repo.run(canonical=base), 1)

    def test_the_branch_that_merged_its_own_number_is_not_flagged(self):
        """A branch whose entry is byte-identical to the baseline merged it;
        that is not a collision."""
        body = entry("D-0001")
        base = self.repo.commit(body)
        claims = {
            "claims": [{"status": "in_review", "branch": "b", "decision_ids": ["D-0001"]}]
        }
        self.repo.commit(body, claims)
        self.assertEqual(self.repo.run(canonical=base), 0)

    def test_closed_claims_are_ignored(self):
        base = self.repo.commit(entry("D-0001"))
        claims = {
            "claims": [{"status": "closed", "branch": "b", "decision_ids": ["D-0001"]}]
        }
        self.repo.commit(entry("D-0001", body="changed"), claims)
        self.assertEqual(self.repo.run(canonical=base), 0)


class WireLimitTests(unittest.TestCase):
    def setUp(self) -> None:
        self.stack = tempfile.TemporaryDirectory()
        self.addCleanup(self.stack.cleanup)
        self.root = Path(self.stack.name)

    def test_a_cap_below_did_minis_fails(self):
        """The seven-crate defect: a decoder that cannot read what did-mini
        will happily sign."""
        write(self.root / "crates/mini-thing/src/wire.rs",
              "const MAX_SIGNATURES: usize = 16;\n")
        self.assertEqual(check_wire_limits.check(self.root), 1)

    def test_referencing_did_mini_passes(self):
        write(self.root / "crates/mini-thing/src/wire.rs",
              "const MAX_SIGNATURES: usize = did_mini::MAX_SIGNATURES;\n")
        self.assertEqual(check_wire_limits.check(self.root), 0)

    def test_a_larger_literal_passes(self):
        """Being more permissive than did-mini is safe; the danger is one-sided."""
        write(self.root / "crates/mini-thing/src/wire.rs",
              "const MAX_SIGNATURES: usize = 128;\n")
        self.assertEqual(check_wire_limits.check(self.root), 0)

    def test_did_mini_itself_is_not_checked_against_itself(self):
        write(self.root / "crates/did-mini/src/limits.rs",
              "const MAX_SIGNATURES: usize = 64;\n")
        write(self.root / "crates/other/src/lib.rs", "// nothing\n")
        self.assertEqual(check_wire_limits.check(self.root), 0)

    def test_non_literal_expressions_are_left_to_humans(self):
        write(self.root / "crates/mini-thing/src/wire.rs",
              "const MAX_SIGNATURES: usize = SOME_OTHER_CONST * 2;\n")
        self.assertEqual(check_wire_limits.check(self.root), 0)


if __name__ == "__main__":
    unittest.main()
