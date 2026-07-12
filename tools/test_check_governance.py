from __future__ import annotations

import datetime as dt
import hashlib
import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("check_governance.py")
SPEC = importlib.util.spec_from_file_location("check_governance", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
TEMPLATE_ROOT = Path(__file__).resolve().parents[1]
FIXTURE_SOURCE = CHECKER.resolve_charter_root(TEMPLATE_ROOT)
PACKAGE_LAYOUT = FIXTURE_SOURCE != TEMPLATE_ROOT


def copy_fixture(destination: Path) -> Path:
    shutil.copytree(
        FIXTURE_SOURCE,
        destination,
        ignore=shutil.ignore_patterns(".git", "target", ".canonical-checkpoint", "_generated"),
    )
    return destination / "repository-template" if PACKAGE_LAYOUT else destination


CLASSES = (
    "Documentation only",
    "Ordinary implementation",
    "Protocol-critical",
    "Cryptography-sensitive",
    "Constitutional / Tier-F",
    "Emergency security correction",
)


def proposal(*selected: str, extra: str = "") -> str:
    boxes = "\n".join(
        f"- [{'x' if item in selected else ' '}] {item}" for item in CLASSES
    )
    sections = {
        "Change class": boxes,
        "Exact state": "0123456789abcdef",
        "Summary": "Validator test proposal.",
        "Founder directives": "None — explanation: validator-only test.",
        "Invariants": "None — explanation: validator-only test.",
        "Decision log": "None — explanation: validator-only test.",
        "Evidence and tests": "Unit test evidence.",
        "AI assistance": "No material AI assistance.",
        "Security and privacy": "No new authority or disclosure.",
        "Dependencies": "No dependency changes.",
        "Release and adoption": "No effect.",
        "Compensation": "None.",
        "Reviewer attestations": "Pending authorized review.",
    }
    body = "\n\n".join(f"## {heading}\n\n{text}" for heading, text in sections.items())
    return f"{body}\n{extra}\n"


def validate(body: str, changed: list[str]) -> tuple[list[str], list[str]]:
    errors: list[str] = []
    warnings: list[str] = []
    CHECKER.validate_proposal(body, changed, errors, warnings)
    return errors, warnings


def activate_candidate(temp_root: Path) -> tuple[Path, Path, dict[str, Path]]:
    candidate = temp_root / "candidate"
    root = copy_fixture(candidate)
    source_root = CHECKER.resolve_charter_root(root)
    decision = "D-9999"

    charter = source_root / CHECKER.CHARTER_PATH
    charter_text = charter.read_text(encoding="utf-8")
    replacements = {
        "**Status:** Proposed operational bootstrap charter":
            "**Status:** Operational bootstrap charter",
        "**Authority class:** Unclassified pending independent review; proposed as operational and non-authorizing":
            "**Authority class:** Operational; non-authorizing; independently classified",
        "**Proposing identity:** To be bound by the adopting proposal":
            "**Proposing identity:** did:mini:test-proposer",
        "**Activation decision:** None in this candidate":
            f"**Activation decision:** {decision}",
        "**Activation decision registry:** None in this candidate":
            "**Activation decision registry:** docs/DECISION_LOG.md",
    }
    for old, new in replacements.items():
        charter_text = charter_text.replace(old, new)
    charter.write_text(charter_text, encoding="utf-8")
    charter_digest = hashlib.sha256(charter.read_bytes()).hexdigest()

    adapter = root / "AGENTS.md"
    adapter_text = adapter.read_text(encoding="utf-8")
    adapter_text = adapter_text.replace(
        "**Adapter status:** Proposed operational loader; non-authorizing",
        "**Adapter status:** Operational loader; non-authorizing",
    ).replace(
        "**Activation decision:** None in this template",
        f"**Activation decision:** {decision}",
    ).replace(
        "**Activated charter digest:** None in this template",
        f"**Activated charter digest:** {charter_digest}",
    )
    adapter.write_text(adapter_text, encoding="utf-8")
    adapter_digest = hashlib.sha256(adapter.read_bytes()).hexdigest()

    summary = source_root / CHECKER.CHARTER_SUMMARY_PATH
    summary_data = json.loads(summary.read_text(encoding="utf-8"))
    summary_data["document"]["status"] = "operational"
    summary_data["document"]["authority_class"] = "operational-non-authorizing"
    summary_data["document"]["source_sha256"] = charter_digest
    summary_data["traceability"]["decisions"] = [decision]
    summary.write_text(json.dumps(summary_data, indent=2) + "\n", encoding="utf-8")
    summary_digest = hashlib.sha256(summary.read_bytes()).hexdigest()

    activation = root / CHECKER.ACTIVATION_RECORD_PATH
    activation_data = json.loads(activation.read_text(encoding="utf-8"))
    activation_data.update({
        "status": "active",
        "decision_ref": decision,
        "decision_record": "governance/decisions/D-9999.json",
        "phase": "founder-guarded",
        "effective_at": "2020-01-01T00:00:00Z",
    })
    activation_data["charter"]["sha256"] = charter_digest
    activation_data["summary"]["sha256"] = summary_digest
    activation_data["adapter"]["sha256"] = adapter_digest
    activation.write_text(json.dumps(activation_data, indent=2) + "\n", encoding="utf-8")

    phase = root / CHECKER.PHASE_RECORD_PATH
    phase_data = json.loads(phase.read_text(encoding="utf-8"))
    phase_data.update({
        "status": "active",
        "phase": "founder-guarded",
        "decision_ref": "D-PHASE-0001",
        "effective_at": "2020-01-01T00:00:00Z",
    })
    phase.write_text(json.dumps(phase_data, indent=2) + "\n", encoding="utf-8")

    activation_digest = hashlib.sha256(activation.read_bytes()).hexdigest()
    decision_path = root / "governance/decisions/D-9999.json"
    decision_path.parent.mkdir(parents=True, exist_ok=True)
    decision_data = {
        "$schema": "../ai-charter-activation-decision.schema.json",
        "schema_version": 1,
        "object_type": "ai-charter-activation-decision",
        "decision_ref": decision,
        "status": "final",
        "classification": "operational",
        "activation_record_sha256": activation_digest,
        "activation_artifacts_sha256": CHECKER.activation_artifacts_digest(
            activation_digest, charter_digest, adapter_digest, summary_digest
        ),
        "charter_sha256": charter_digest,
        "summary_sha256": summary_digest,
        "adapter_sha256": adapter_digest,
        "phase": "founder-guarded",
        "effective_at": "2020-01-01T00:00:00Z",
        "cooling_required": False,
        "cooling_completed_at": None,
        "cooling_basis": "No cooling required by the test policy.",
        "superseded_by": None,
    }
    decision_path.write_text(json.dumps(decision_data, indent=2) + "\n", encoding="utf-8")

    registry = source_root / "docs/DECISION_LOG.md"
    registry.write_text(
        f"# Test decision registry\n\n## {decision}\n\n"
        "governance/decisions/D-9999.json\n",
        encoding="utf-8",
    )

    shutil.copy2(root / ".github/CODEOWNERS.template", root / ".github/CODEOWNERS")
    canonical = temp_root / "canonical"
    shutil.copytree(candidate, canonical)
    canonical_root = canonical / "repository-template" if PACKAGE_LAYOUT else canonical
    canonical_source_root = CHECKER.resolve_charter_root(canonical_root)
    return root, canonical_root, {
        "root": root,
        "canonical_root": canonical_root,
        "adapter": adapter,
        "charter": charter,
        "summary": summary,
        "registry": registry,
        "activation": activation,
        "phase": phase,
        "decision": decision_path,
        "canonical_registry": canonical_source_root / "docs/DECISION_LOG.md",
        "canonical_activation": canonical_root / CHECKER.ACTIVATION_RECORD_PATH,
        "canonical_phase": canonical_root / CHECKER.PHASE_RECORD_PATH,
        "canonical_decision": canonical_root / "governance/decisions/D-9999.json",
        "codeowners": root / ".github/CODEOWNERS",
    }


class ProposalClassificationTests(unittest.TestCase):
    def test_requires_one_selected_class(self) -> None:
        errors, _ = validate(proposal(), [])
        self.assertIn("proposal must select exactly one change class checkbox", errors)

        errors, _ = validate(
            proposal("Documentation only", "Protocol-critical"), []
        )
        self.assertIn("proposal must select exactly one change class checkbox", errors)

    def test_agents_requires_sensitive_class(self) -> None:
        errors, _ = validate(proposal("Documentation only"), ["AGENTS.md"])
        self.assertIn(
            "protected paths changed without a sensitive change classification", errors
        )

    def test_charter_requires_sensitive_class(self) -> None:
        errors, _ = validate(
            proposal("Ordinary implementation"),
            ["docs/governance/50_PRIMARY_AI_ENGINEER_CHARTER.md"],
        )
        self.assertIn(
            "protected paths changed without a sensitive change classification", errors
        )

    def test_protocol_critical_satisfies_path_floor(self) -> None:
        errors, _ = validate(proposal("Protocol-critical"), ["AGENTS.md"])
        self.assertNotIn(
            "protected paths changed without a sensitive change classification", errors
        )

    def test_model_specific_loaders_require_sensitive_class(self) -> None:
        for path in ("CLAUDE.md", ".github/copilot-instructions.md"):
            with self.subTest(path=path):
                errors, _ = validate(proposal("Documentation only"), [path])
                self.assertIn(
                    "protected paths changed without a sensitive change classification",
                    errors,
                )

    def test_tier_f_still_requires_traceability(self) -> None:
        errors, _ = validate(
            proposal("Constitutional / Tier-F"), ["docs/INVARIANTS.md"]
        )
        self.assertIn("Tier-F path changed without an invariant identifier", errors)
        self.assertIn("Tier-F path changed without a decision identifier", errors)

        errors, _ = validate(
            proposal(
                "Constitutional / Tier-F",
                extra="Affected records: INV-U1 and D-0001.",
            ),
            ["docs/INVARIANTS.md"],
        )
        self.assertNotIn("Tier-F path changed without an invariant identifier", errors)
        self.assertNotIn("Tier-F path changed without a decision identifier", errors)


class PackagedBaselineTests(unittest.TestCase):
    def test_candidate_template_baseline(self) -> None:
        errors: list[str] = []
        warnings: list[str] = []
        CHECKER.validate_baseline(TEMPLATE_ROOT, errors, warnings)
        self.assertEqual([], errors)

    def test_known_claude_conflicts_fail(self) -> None:
        errors: list[str] = []
        warnings: list[str] = []
        CHECKER.validate_model_specific_loader(
            "CLAUDE.md",
            """Read AGENTS.md.
## The five canonical documents — read order for any non-trivial task
Founder-approved
(2026-07-08: \"design it and implement how you see fit\").
GitHub is the UAT/mirror.
""",
            errors,
            warnings,
        )
        self.assertEqual(3, len(errors))


class ActivatedBaselineTests(unittest.TestCase):
    def validate_active(
        self, mutate=None, canonical_override=None, candidate_activation=False, now=None
    ) -> tuple[list[str], list[str]]:
        with tempfile.TemporaryDirectory() as temp:
            root, canonical_root, paths = activate_candidate(Path(temp))
            if mutate:
                mutate(paths)
            if canonical_override:
                canonical_root = canonical_override(Path(temp), paths)
            errors: list[str] = []
            warnings: list[str] = []
            CHECKER.validate_baseline(
                root,
                errors,
                warnings,
                canonical_root,
                now=now,
                candidate_activation=candidate_activation,
            )
            return errors, warnings

    def test_valid_activated_state(self) -> None:
        errors, _ = self.validate_active()
        self.assertEqual([], errors)

    def test_charter_digest_drift_fails(self) -> None:
        def mutate(paths):
            paths["charter"].write_text(
                paths["charter"].read_text(encoding="utf-8") + "\nDrift.\n",
                encoding="utf-8",
            )

        self.assertIn(
            "activated charter digest does not match the charter file",
            self.validate_active(mutate)[0],
        )

    def test_adapter_digest_drift_fails(self) -> None:
        def mutate(paths):
            paths["adapter"].write_text(
                paths["adapter"].read_text(encoding="utf-8") + "\nDrift.\n",
                encoding="utf-8",
            )

        self.assertIn(
            "activated adapter digest does not match AGENTS.md",
            self.validate_active(mutate)[0],
        )

    def test_decision_and_registry_mismatch_fail(self) -> None:
        def mutate(paths):
            paths["canonical_registry"].write_text(
                "# Missing activation decision\n", encoding="utf-8"
            )

        self.assertIn(
            "activation registry does not index the structured final Decision",
            self.validate_active(mutate)[0],
        )

    def test_adapter_and_record_decisions_must_match(self) -> None:
        def mutate(paths):
            text = paths["adapter"].read_text(encoding="utf-8")
            paths["adapter"].write_text(
                text.replace("**Activation decision:** D-9999", "**Activation decision:** D-9998"),
                encoding="utf-8",
            )

        self.assertIn(
            "AGENTS.md and activation record decisions do not match",
            self.validate_active(mutate)[0],
        )

    def test_summary_must_be_operational_and_cite_decision(self) -> None:
        def mutate(paths):
            data = json.loads(paths["summary"].read_text(encoding="utf-8"))
            data["document"]["status"] = "draft"
            data["traceability"]["decisions"] = []
            paths["summary"].write_text(json.dumps(data), encoding="utf-8")

        errors, _ = self.validate_active(mutate)
        self.assertIn(
            "activated AGENTS.md requires an operational or normative charter summary",
            errors,
        )
        self.assertIn(
            "activated charter summary does not cite its activation decision", errors
        )

    def test_activated_state_requires_installed_codeowners(self) -> None:
        def mutate(paths):
            paths["codeowners"].unlink()

        self.assertIn(
            "activated AI charter requires an installed .github/CODEOWNERS",
            self.validate_active(mutate)[0],
        )

    def test_activated_model_loader_requires_agents_reference(self) -> None:
        def mutate(paths):
            (paths["root"] / "CLAUDE.md").write_text(
                "# Tool-specific context only\n", encoding="utf-8"
            )

        self.assertIn(
            "model-specific loader lacks the reviewed Session authority boundary: CLAUDE.md",
            self.validate_active(mutate)[0],
        )

    def test_model_loader_authority_grant_fails_below_valid_boundary(self) -> None:
        def mutate(paths):
            text = (
                "> **Session authority boundary:** Read repository-root `AGENTS.md`.\n"
                "> This file grants no Mininet Authority.\n\n"
                "The Primary AI Engineer may canonicalize this proposal.\n"
            )
            (paths["root"] / "CLAUDE.md").write_text(text, encoding="utf-8")
            (paths["canonical_root"] / "CLAUDE.md").write_text(text, encoding="utf-8")

        self.assertIn(
            "CLAUDE.md contains prohibited explicit authority grant: AI authorizing power",
            self.validate_active(mutate)[0],
        )

    def test_candidate_loader_drift_from_canonical_fails(self) -> None:
        def mutate(paths):
            canonical_text = (
                "> **Session authority boundary:** Read repository-root `AGENTS.md`.\n"
                "> This file grants no Mininet Authority.\n"
            )
            (paths["canonical_root"] / "CLAUDE.md").write_text(
                canonical_text, encoding="utf-8"
            )
            (paths["root"] / "CLAUDE.md").write_text(
                canonical_text + "\nChanged candidate context.\n", encoding="utf-8"
            )

        self.assertIn(
            "active worktree instruction surface differs from canonical state: CLAUDE.md",
            self.validate_active(mutate)[0],
        )

    def test_self_consistent_noncanonical_branch_cannot_activate(self) -> None:
        def proposed_canonical(temp_root, _paths):
            proposed = temp_root / "proposed-canonical"
            return copy_fixture(proposed)

        self.assertIn(
            "worktree activation record does not match the canonical checkpoint",
            self.validate_active(canonical_override=proposed_canonical)[0],
        )

    def test_future_effective_time_fails(self) -> None:
        self.assertIn(
            "AI charter activation effective time has not arrived",
            self.validate_active(
                now=dt.datetime(2019, 1, 1, tzinfo=dt.timezone.utc)
            )[0],
        )

    def test_phase_mismatch_fails(self) -> None:
        def mutate(paths):
            phase = json.loads(paths["canonical_phase"].read_text(encoding="utf-8"))
            phase["phase"] = "working-group"
            paths["canonical_phase"].write_text(
                json.dumps(phase, indent=2) + "\n", encoding="utf-8"
            )

        self.assertIn(
            "canonical phase does not match charter activation phase",
            self.validate_active(mutate)[0],
        )

    def test_future_cooling_completion_fails(self) -> None:
        def mutate(paths):
            decision = json.loads(paths["canonical_decision"].read_text(encoding="utf-8"))
            decision["cooling_required"] = True
            decision["cooling_completed_at"] = "2999-01-01T00:00:00Z"
            paths["canonical_decision"].write_text(
                json.dumps(decision, indent=2) + "\n", encoding="utf-8"
            )

        self.assertIn(
            "structured activation Decision cooling period is incomplete",
            self.validate_active(mutate)[0],
        )

    def test_no_cooling_decision_requires_null_completion(self) -> None:
        def mutate(paths):
            decision = json.loads(paths["canonical_decision"].read_text(encoding="utf-8"))
            decision["cooling_completed_at"] = "2020-01-01T00:00:00Z"
            paths["canonical_decision"].write_text(
                json.dumps(decision, indent=2) + "\n", encoding="utf-8"
            )

        self.assertIn(
            "no-cooling Decision must set cooling_completed_at to null",
            self.validate_active(mutate)[0],
        )

    def test_decision_classification_must_match_charter_metadata(self) -> None:
        def mutate(paths):
            decision = json.loads(paths["canonical_decision"].read_text(encoding="utf-8"))
            decision["classification"] = "constitutional"
            paths["canonical_decision"].write_text(
                json.dumps(decision, indent=2) + "\n", encoding="utf-8"
            )

        errors = self.validate_active(mutate)[0]
        self.assertIn(
            "charter authority class does not match the activation Decision classification",
            errors,
        )
        self.assertIn(
            "charter summary authority class does not match the activation Decision classification",
            errors,
        )

    def test_explicit_authority_grant_fails_even_with_denial_marker(self) -> None:
        def mutate(paths):
            paths["adapter"].write_text(
                paths["adapter"].read_text(encoding="utf-8")
                + "\nThe Primary AI Engineer may canonicalize this proposal.\n",
                encoding="utf-8",
            )

        self.assertIn(
            "AGENTS.md contains prohibited explicit authority grant: AI authorizing power",
            self.validate_active(mutate)[0],
        )

    def test_removed_activation_gate_fails_digest_check(self) -> None:
        def mutate(paths):
            text = paths["adapter"].read_text(encoding="utf-8")
            paths["adapter"].write_text(
                text.replace("## Activation gate", "## Gate removed by proposal"),
                encoding="utf-8",
            )

        self.assertIn(
            "activated adapter digest does not match AGENTS.md",
            self.validate_active(mutate)[0],
        )

    def test_append_only_supersession_marker_disables_core(self) -> None:
        def mutate(paths):
            with paths["canonical_registry"].open("a", encoding="utf-8") as registry:
                registry.write("\nAI-Charter-Activation-Superseded: D-9999 -> D-10000\n")

        self.assertIn(
            "activation Decision is superseded by D-10000",
            self.validate_active(mutate)[0],
        )

    def test_noncanonical_activation_can_be_reviewed_only_as_candidate(self) -> None:
        def proposed_canonical(temp_root, _paths):
            proposed = temp_root / "proposed-canonical"
            return copy_fixture(proposed)

        errors, warnings = self.validate_active(
            canonical_override=proposed_canonical,
            candidate_activation=True,
        )
        self.assertEqual([], errors)
        self.assertTrue(any("proposal data only" in warning for warning in warnings))


class RuntimeInstructionSurfaceTests(unittest.TestCase):
    def validate_runtime(self, mutate=None) -> list[str]:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            canonical = root / "canonical"
            candidate = root / "candidate"
            for checkout in (canonical, candidate):
                (checkout / "tools").mkdir(parents=True)
                (checkout / "AGENTS.md").write_text("canonical instructions\n", encoding="utf-8")
                (checkout / "tools/check_governance.py").write_text("# checker\n", encoding="utf-8")
            if mutate:
                mutate(candidate, canonical)
            errors: list[str] = []
            CHECKER.validate_runtime_instruction_surfaces(
                candidate,
                canonical,
                errors,
                checker_path=canonical / "tools/check_governance.py",
            )
            return errors

    def test_identical_instruction_surfaces_pass(self) -> None:
        self.assertEqual([], self.validate_runtime())

    def test_new_nested_agents_is_rejected_before_load(self) -> None:
        def mutate(candidate, _canonical):
            nested = candidate / "crates/example/AGENTS.md"
            nested.parent.mkdir(parents=True)
            nested.write_text("untrusted nested instructions\n", encoding="utf-8")

        errors = self.validate_runtime(mutate)
        self.assertTrue(any("candidate adds untrusted instruction surfaces" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
