from __future__ import annotations

import datetime as dt
import json
import tempfile
import unittest
from pathlib import Path

from test_check_governance import CHECKER, copy_fixture


class PreGoLiveOperatingProfileTests(unittest.TestCase):
    def validate(self, mutate=None, canonical_mutate=None, remove_decision=False, alter_decision=False):
        with tempfile.TemporaryDirectory() as temp:
            root = copy_fixture(Path(temp) / "candidate")
            canonical = copy_fixture(Path(temp) / "canonical")
            for target, change in ((root, mutate), (canonical, canonical_mutate)):
                if change:
                    path = target / CHECKER.BOOTSTRAP_STATE_PATH
                    state = json.loads(path.read_text(encoding="utf-8"))
                    change(state)
                    path.write_text(json.dumps(state), encoding="utf-8")
            if remove_decision:
                (canonical / CHECKER.PRE_GO_LIVE_DECISION).unlink()
            if alter_decision:
                (canonical / CHECKER.PRE_GO_LIVE_DECISION).write_text("proposed replacement", encoding="utf-8")
            errors, warnings = [], []
            CHECKER.validate_bootstrap_operating_state(
                root, errors, warnings,
                now=dt.datetime(2026, 10, 14, tzinfo=dt.timezone.utc),
                canonical_root=canonical,
            )
            return errors

    def test_pre_go_live_outlives_calendar_and_maintainer_count(self):
        self.assertEqual([], self.validate(lambda s: s.update(
            independent_non_founder_human_maintainers=2,
            production_release_candidate=True,
        )))

    def test_forge_canonical_alone_is_not_go_live(self):
        self.assertEqual([], self.validate(lambda s: s.update(forge_canonical=True)))

    def test_go_live_requires_forge_and_disables_bootstrap(self):
        self.assertTrue(any("requires canonical Forge" in e for e in
                            self.validate(lambda s: s.update(go_live=True))))
        self.assertTrue(any("disables Founder" in e for e in self.validate(
            lambda s: s.update(go_live=True, forge_canonical=True, status="sunset"))))

    def test_canonical_go_live_cannot_be_rolled_back(self):
        activate = lambda s: s.update(go_live=True, forge_canonical=True, status="sunset")
        self.assertTrue(any("cannot be rolled back" in e for e in
                            self.validate(canonical_mutate=activate)))

    def test_post_go_live_forge_bit_cannot_be_cleared(self):
        activate = lambda s: s.update(go_live=True, forge_canonical=True, status="sunset")
        self.assertTrue(any("cannot be rolled back" in e for e in self.validate(
            lambda s: s.update(go_live=True, forge_canonical=False, status="sunset"),
            canonical_mutate=activate)))

    def test_canonical_profile_cannot_be_downgraded(self):
        self.assertTrue(any("cannot be downgraded" in e for e in
                            self.validate(lambda s: s.update(schema_version=1))))

    def test_go_live_is_strictly_boolean(self):
        for value in (None, 0, 1, "false", "true"):
            with self.subTest(value=value):
                self.assertTrue(any("go_live must be boolean" in e for e in
                                    self.validate(lambda s: s.update(go_live=value))))

    def test_missing_state_fields_fail_closed(self):
        self.assertTrue(self.validate(lambda s: s.pop("go_live")))

    def test_changed_canonical_decision_digest_fails_closed(self):
        self.assertTrue(any("digest mismatch" in e for e in
                            self.validate(alter_decision=True)))

    def test_candidate_cannot_invent_canonical_decision(self):
        self.assertTrue(any("requires canonical governance decision" in e for e in
                            self.validate(remove_decision=True)))


if __name__ == "__main__":
    unittest.main()
