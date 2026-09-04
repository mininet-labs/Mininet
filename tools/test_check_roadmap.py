from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("check_roadmap.py")
SPEC = importlib.util.spec_from_file_location("check_roadmap", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
REPO_ROOT = Path(__file__).resolve().parents[1]


SUMMARY = """<!-- ROADMAP-SUMMARY-BEGIN -->
R1 R2
**2 items: 0 done, 1 active, 1 ready, 0 blocked, 0 outside.**
<!-- ROADMAP-SUMMARY-END -->
"""

ROADMAP = """# The road to release

### R1 — First thing · `active`
Body.

### R2 — Second thing · `ready`
Body.
"""


class RoadmapCheckerTests(unittest.TestCase):
    def check(self, roadmap: str, readme: str, decisions: str = ""):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "docs").mkdir()
            (root / "docs/ROADMAP_TO_RELEASE.md").write_text(roadmap, encoding="utf-8")
            (root / "docs/DECISION_LOG.md").write_text(decisions, encoding="utf-8")
            (root / "README.md").write_text(readme, encoding="utf-8")
            errors: list[str] = []
            warnings: list[str] = []
            CHECKER.check(root, errors, warnings)
            return errors, warnings

    def test_a_consistent_roadmap_passes(self) -> None:
        errors, _ = self.check(ROADMAP, SUMMARY)
        self.assertEqual([], errors)

    def test_counts_that_disagree_with_the_readme_fail(self) -> None:
        # The whole point: the front page cannot quietly describe a different
        # roadmap than the one in docs/.
        drifted = SUMMARY.replace("1 active, 1 ready", "2 active, 0 ready")
        errors, _ = self.check(ROADMAP, drifted)
        self.assertTrue(any("README says" in error for error in errors), errors)

    def test_done_without_a_decision_fails(self) -> None:
        # "done" is the one claim this file must never be able to make on its
        # own word.
        roadmap = ROADMAP.replace("· `active`", "· `done`")
        readme = SUMMARY.replace(
            "**2 items: 0 done, 1 active, 1 ready, 0 blocked, 0 outside.**",
            "**2 items: 1 done, 0 active, 1 ready, 0 blocked, 0 outside.**",
        )
        errors, _ = self.check(roadmap, readme)
        self.assertTrue(any("cites no D-number" in error for error in errors), errors)

    def test_done_citing_a_nonexistent_decision_fails(self) -> None:
        # A reference to a decision that was never written is the same empty
        # claim wearing a citation.
        roadmap = ROADMAP.replace(
            "### R1 — First thing · `active`\nBody.",
            "### R1 — First thing · `done`\n**Closed by:** D-9999.",
        )
        readme = SUMMARY.replace(
            "**2 items: 0 done, 1 active, 1 ready, 0 blocked, 0 outside.**",
            "**2 items: 1 done, 0 active, 1 ready, 0 blocked, 0 outside.**",
        )
        errors, _ = self.check(roadmap, readme, decisions="### D-0001 — real\n")
        self.assertTrue(any("D-9999" in error for error in errors), errors)

    def test_done_citing_a_real_decision_passes(self) -> None:
        roadmap = ROADMAP.replace(
            "### R1 — First thing · `active`\nBody.",
            "### R1 — First thing · `done`\n**Closed by:** D-0001.",
        )
        readme = SUMMARY.replace(
            "**2 items: 0 done, 1 active, 1 ready, 0 blocked, 0 outside.**",
            "**2 items: 1 done, 0 active, 1 ready, 0 blocked, 0 outside.**",
        )
        errors, _ = self.check(
            roadmap, readme, decisions="### D-0001 — a real entry\n"
        )
        self.assertEqual([], errors)

    def test_blocked_without_a_named_blocker_fails(self) -> None:
        roadmap = ROADMAP.replace("### R2 — Second thing · `ready`", "### R2 — Second thing · `blocked`")
        readme = SUMMARY.replace("1 active, 1 ready, 0 blocked", "1 active, 0 ready, 1 blocked")
        errors, _ = self.check(roadmap, readme)
        self.assertTrue(any("names no blocker" in error for error in errors), errors)

    def test_blocked_by_a_nonexistent_item_fails(self) -> None:
        roadmap = ROADMAP.replace(
            "### R2 — Second thing · `ready`\nBody.",
            "### R2 — Second thing · `blocked`\n**Blocked by:** R99.",
        )
        readme = SUMMARY.replace("1 active, 1 ready, 0 blocked", "1 active, 0 ready, 1 blocked")
        errors, _ = self.check(roadmap, readme)
        self.assertTrue(any("R99" in error for error in errors), errors)

    def test_an_item_cannot_block_itself(self) -> None:
        roadmap = ROADMAP.replace(
            "### R2 — Second thing · `ready`\nBody.",
            "### R2 — Second thing · `blocked`\n**Blocked by:** R2.",
        )
        readme = SUMMARY.replace("1 active, 1 ready, 0 blocked", "1 active, 0 ready, 1 blocked")
        errors, _ = self.check(roadmap, readme)
        self.assertTrue(any("blocked by itself" in error for error in errors), errors)

    def test_an_unknown_status_fails(self) -> None:
        roadmap = ROADMAP.replace("· `ready`", "· `nearly`")
        errors, _ = self.check(roadmap, SUMMARY)
        self.assertTrue(any("expected one of" in error for error in errors), errors)

    def test_duplicate_ids_fail(self) -> None:
        roadmap = ROADMAP.replace("### R2 — Second thing", "### R1 — Second thing")
        errors, _ = self.check(roadmap, SUMMARY)
        self.assertTrue(any("defined twice" in error for error in errors), errors)

    def test_a_readme_without_the_summary_block_fails(self) -> None:
        errors, _ = self.check(ROADMAP, "# Mininet\n\nNo roadmap here.\n")
        self.assertTrue(
            any("ROADMAP-SUMMARY block" in error for error in errors), errors
        )

    def test_a_done_count_that_disagrees_with_the_readme_fails(self) -> None:
        # Completed work is counted on the front page now (D-0453's named
        # follow-up). Before this, `done` items silently left the summary and
        # the totals still validated -- so finishing an item made the front
        # page quietly less accurate.
        roadmap = ROADMAP.replace(
            "### R1 — First thing · `active`\nBody.",
            "### R1 — First thing · `done`\n**Closed by:** D-0001.",
        )
        errors, _ = self.check(
            roadmap, SUMMARY, decisions="### D-0001 — a real entry\n"
        )
        self.assertTrue(any("done" in error for error in errors), errors)

    def test_the_real_repository_roadmap_is_consistent(self) -> None:
        # The check that actually matters day to day: this repository's own
        # roadmap and README agree right now.
        errors: list[str] = []
        warnings: list[str] = []
        CHECKER.check(REPO_ROOT, errors, warnings)
        self.assertEqual([], errors)


if __name__ == "__main__":
    unittest.main()
