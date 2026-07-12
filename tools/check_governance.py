#!/usr/bin/env python3
"""Reference Mininet bootstrap governance validator.

Standard-library only by design. It validates repository policy artifacts and,
when provided, a proposal body and changed-path list. It does not infer human
identity, reviewer competence, or constitutional legitimacy. Active charter
validation requires a separately supplied canonical checkpoint whose
provenance the caller has independently established; this tool compares state
but cannot make that checkpoint legitimate.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import sys
from pathlib import Path

REQUIRED_HEADINGS = [
    "Change class", "Exact state", "Summary", "Founder directives",
    "Invariants", "Decision log", "Evidence and tests", "AI assistance",
    "Security and privacy", "Dependencies", "Release and adoption",
    "Compensation", "Reviewer attestations",
]
CHECKED_CHANGE_CLASS = re.compile(
    r"^\s*-\s*\[[xX]\]\s*("
    r"Documentation only|Ordinary implementation|Protocol-critical|"
    r"Cryptography-sensitive|Constitutional / Tier-F|"
    r"Emergency security correction"
    r")\s*$",
    re.M,
)
SENSITIVE_CHANGE_CLASSES = {
    "Protocol-critical",
    "Cryptography-sensitive",
    "Constitutional / Tier-F",
    "Emergency security correction",
}
PROTECTED_PREFIXES = (
    "AGENTS.md", "CLAUDE.md", "GEMINI.md", ".cursorrules", ".cursor/rules/",
    ".github/copilot-instructions.md",
    "docs/governance/", "governance/",
    "docs/FOUNDER_DIRECTIVES.md", "docs/INVARIANTS.md", "docs/DECISION_LOG.md",
    "crates/mini-crypto/", "crates/mini-value/", "crates/mini-treasury/",
    "crates/mini-identity/", "crates/mini-consensus/", "crates/mini-chain/",
    "crates/mini-forge/", "crates/mini-provenance/", "crates/mini-update/",
    "crates/mini-installer/", ".github/workflows/", "deny.toml",
)
TIER_F_PREFIXES = (
    "docs/FOUNDER_DIRECTIVES.md", "docs/INVARIANTS.md",
)
PROHIBITED_CLAIMS = {
    "forced update": re.compile(r"\b(force(?:d)? update|mandatory activation)\b", re.I),
    "permanent owner/admin": re.compile(r"\b(permanent (?:owner|admin)|admin key|owner key)\b", re.I),
    "money buys governance": re.compile(r"\b(balance|stake|payment|wealth).{0,40}\b(vote|quorum|governance weight)\b", re.I | re.S),
}
CHARTER_PATH = Path("docs/governance/50_PRIMARY_AI_ENGINEER_CHARTER.md")
CHARTER_SUMMARY_PATH = Path("docs/governance/50_PRIMARY_AI_ENGINEER_CHARTER.summary.json")
CHARTER_ID = "GOV-AI-050"
CHARTER_VERSION = "1.1"
ACTIVATION_RECORD_PATH = Path("governance/ai-charter-activation.json")
ACTIVATION_SCHEMA_PATH = Path("governance/ai-charter-activation.schema.json")
ACTIVATION_DECISION_SCHEMA_PATH = Path("governance/ai-charter-activation-decision.schema.json")
PHASE_RECORD_PATH = Path("governance/current-phase.json")
PHASE_SCHEMA_PATH = Path("governance/current-phase.schema.json")
MODEL_SPECIFIC_LOADERS = (
    Path("CLAUDE.md"),
    Path(".github/copilot-instructions.md"),
    Path("GEMINI.md"),
    Path(".cursorrules"),
)
INSTRUCTION_EXCLUDED_PARTS = {".git", "target", ".canonical-checkpoint"}
SESSION_BOUNDARY_MARKERS = (
    "Session authority boundary",
    "repository-root `AGENTS.md`",
    "grants no Mininet Authority",
)
MODEL_LOADER_CONFLICTS = {
    "unbounded full-corpus startup rule": re.compile(
        r"five canonical documents\s*[—-]\s*read order for any non-trivial task",
        re.I,
    ),
    "unbounded historical Founder scope": re.compile(
        r"Founder-approved[\s\S]{0,160}design it and implement how you see fit",
        re.I,
    ),
    "premature GitHub mirror claim": re.compile(r"GitHub is the UAT/mirror", re.I),
}
AUTHORITY_GRANT_PATTERNS = {
    "AI authorizing power": re.compile(
        r"\b(?:AI|Primary AI Engineer|The Engineer)\s+"
        r"(?:may|can|shall|is authorized to)\s+(?:independently\s+)?"
        r"(?:approve|canonicalize|merge\s+(?:into\s+)?canonical|"
        r"sign\s+(?:a\s+)?release|publish\s+(?:a\s+)?release|"
        r"satisfy\s+(?:a\s+)?(?:human\s+|governance\s+)?quorum|"
        r"cast\s+(?:a\s+)?(?:human\s+|governance\s+)?vote|"
        r"administer\s+(?:secrets|treasury|repository|governance))\b",
        re.I,
    ),
    "unilateral Founder power": re.compile(
        r"\bFounder\s+(?:alone|solely|unilaterally)\s+"
        r"(?:may|can|shall|decides|controls|authorizes|canonicalizes|publishes)\b",
        re.I,
    ),
}


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def read_optional(path: str | None) -> str:
    if not path:
        return ""
    return Path(path).read_text(encoding="utf-8")


def resolve_charter_root(root: Path) -> Path:
    """Support both an installed repository and this package's template layout."""
    if (root / CHARTER_PATH).is_file():
        return root
    if (root.parent / CHARTER_PATH).is_file():
        return root.parent
    return root


def metadata_value(text: str, label: str) -> str | None:
    match = re.search(rf"^\*\*{re.escape(label)}:\*\*\s*(.+?)\s*$", text, re.M)
    return match.group(1).strip() if match else None


def read_json_object(path: Path, label: str, errors: list[str]) -> dict | None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(errors, f"invalid {label} JSON: {exc}")
        return None
    if not isinstance(value, dict):
        fail(errors, f"{label} must be a JSON object")
        return None
    return value


def parse_instant(value: object, label: str, errors: list[str]) -> dt.datetime | None:
    if not isinstance(value, str) or not value:
        fail(errors, f"{label} is missing")
        return None
    try:
        instant = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        fail(errors, f"{label} is invalid")
        return None
    if instant.tzinfo is None:
        fail(errors, f"{label} must include a timezone")
        return None
    return instant.astimezone(dt.timezone.utc)


def safe_repo_path(base: Path, value: object, label: str, errors: list[str]) -> Path | None:
    if not isinstance(value, str) or not value:
        fail(errors, f"{label} is missing")
        return None
    relative = Path(value.strip("`"))
    if relative.is_absolute() or ".." in relative.parts:
        fail(errors, f"{label} must be a safe repository-relative path")
        return None
    resolved_base = base.resolve()
    resolved_path = (resolved_base / relative).resolve()
    if not resolved_path.is_relative_to(resolved_base):
        fail(errors, f"{label} escapes the repository through a symbolic link")
        return None
    return resolved_path


def activation_artifacts_digest(
    activation_record_digest: str,
    charter_digest: str,
    adapter_digest: str,
    summary_digest: str,
) -> str:
    payload = (
        "mininet-ai-charter-activation-artifacts-v1\n"
        f"activation-record:{activation_record_digest}\n"
        f"charter:{charter_digest}\n"
        f"adapter:{adapter_digest}\n"
        f"summary:{summary_digest}\n"
    ).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def instruction_surfaces(root: Path) -> dict[str, Path]:
    """Return every known auto-loaded instruction surface below root."""
    found: dict[str, Path] = {}

    def add(path: Path) -> None:
        if not path.is_file():
            return
        relative = path.relative_to(root)
        if any(part in INSTRUCTION_EXCLUDED_PARTS for part in relative.parts):
            return
        if any(part.startswith(".canonical-checkpoint") for part in relative.parts):
            return
        found[relative.as_posix()] = path

    for relative in MODEL_SPECIFIC_LOADERS:
        add(root / relative)
    for path in root.rglob("AGENTS.md"):
        add(path)
    cursor_rules = root / ".cursor/rules"
    if cursor_rules.is_dir():
        for path in cursor_rules.rglob("*.md"):
            add(path)
    return dict(sorted(found.items()))


def validate_runtime_instruction_surfaces(
    root: Path,
    canonical_root: Path | None,
    errors: list[str],
    checker_path: Path | None = None,
) -> None:
    """Authenticate instruction bytes before candidate instructions are parsed."""
    if canonical_root is None:
        fail(errors, "runtime mode requires --canonical-root")
        return
    resolved_root = root.resolve()
    resolved_canonical = canonical_root.resolve()
    if resolved_root == resolved_canonical:
        fail(errors, "runtime mode requires a separate candidate worktree")
        return
    expected_checker = (resolved_canonical / "tools/check_governance.py").resolve()
    actual_checker = (checker_path or Path(__file__)).resolve()
    if actual_checker != expected_checker:
        fail(errors, "runtime mode must execute the checker from the canonical checkout")
        return

    candidate = instruction_surfaces(resolved_root)
    canonical = instruction_surfaces(resolved_canonical)
    candidate_paths = set(candidate)
    canonical_paths = set(canonical)
    if candidate_paths != canonical_paths:
        added = sorted(candidate_paths - canonical_paths)
        removed = sorted(canonical_paths - candidate_paths)
        if added:
            fail(errors, f"candidate adds untrusted instruction surfaces: {', '.join(added)}")
        if removed:
            fail(errors, f"candidate removes canonical instruction surfaces: {', '.join(removed)}")
    for relative in sorted(candidate_paths & canonical_paths):
        if candidate[relative].read_bytes() != canonical[relative].read_bytes():
            fail(errors, f"candidate instruction surface differs from canonical state: {relative}")


def validate_no_authority_grants(label: str, text: str, errors: list[str]) -> None:
    for name, pattern in AUTHORITY_GRANT_PATTERNS.items():
        if pattern.search(text):
            fail(errors, f"{label} contains prohibited explicit authority grant: {name}")


def validate_model_specific_loader(
    loader_name: str,
    text: str,
    errors: list[str],
    warnings: list[str],
    require_agents_reference: bool = False,
) -> None:
    if not all(marker in text for marker in SESSION_BOUNDARY_MARKERS):
        message = f"model-specific loader lacks the reviewed Session authority boundary: {loader_name}"
        if require_agents_reference:
            fail(errors, message)
        else:
            warnings.append(message)
    validate_no_authority_grants(loader_name, text, errors)
    for name, pattern in MODEL_LOADER_CONFLICTS.items():
        if pattern.search(text):
            fail(errors, f"model-specific loader retains {name}: {loader_name}")


def validate_session_charter(
    root: Path,
    errors: list[str],
    warnings: list[str],
    canonical_root: Path | None = None,
    now: dt.datetime | None = None,
    candidate_activation: bool = False,
) -> None:
    source_root = resolve_charter_root(root)
    adapter_path = root / "AGENTS.md"
    charter_path = source_root / CHARTER_PATH
    summary_path = source_root / CHARTER_SUMMARY_PATH
    activation_path = root / ACTIVATION_RECORD_PATH
    activation_schema_path = root / ACTIVATION_SCHEMA_PATH
    activation_decision_schema_path = root / ACTIVATION_DECISION_SCHEMA_PATH
    phase_path = root / PHASE_RECORD_PATH
    phase_schema_path = root / PHASE_SCHEMA_PATH

    required_paths = (
        adapter_path,
        charter_path,
        summary_path,
        activation_path,
        activation_schema_path,
        activation_decision_schema_path,
        phase_path,
        phase_schema_path,
    )
    for path in required_paths:
        if not path.is_file():
            display = path.relative_to(source_root if path.is_relative_to(source_root) else root)
            fail(errors, f"missing AI charter artifact: {display}")
    if not all(path.is_file() for path in required_paths):
        return

    adapter = adapter_path.read_text(encoding="utf-8")
    charter = charter_path.read_text(encoding="utf-8")
    try:
        summary = json.loads(summary_path.read_text(encoding="utf-8"))
        activation_record = json.loads(activation_path.read_text(encoding="utf-8"))
        json.loads(activation_schema_path.read_text(encoding="utf-8"))
        json.loads(activation_decision_schema_path.read_text(encoding="utf-8"))
        phase_template = json.loads(phase_path.read_text(encoding="utf-8"))
        json.loads(phase_schema_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        fail(errors, f"invalid AI charter JSON artifact: {exc}")
        return

    if not all(isinstance(value, dict) for value in (summary, activation_record, phase_template)):
        fail(errors, "AI charter summary, activation record, and phase record must be JSON objects")
        return

    local_activation_record = activation_record
    local_record_status = local_activation_record.get("status")
    canonical_source_root: Path | None = None
    canonical_repository_root: Path | None = None
    canonical_activation_path: Path | None = None
    if local_record_status == "active":
        if canonical_root is None:
            if not candidate_activation:
                fail(errors, "active worktree charter requires a separately supplied canonical root")
                return
        else:
            canonical_repository_root = canonical_root.resolve()
            if canonical_repository_root == root.resolve():
                fail(errors, "canonical root must be separate from the proposal worktree")
                return
            canonical_source_root = resolve_charter_root(canonical_repository_root)
            canonical_activation_path = canonical_repository_root / ACTIVATION_RECORD_PATH
            canonical_record = read_json_object(
                canonical_activation_path,
                "canonical AI charter activation record",
                errors,
            )
            if canonical_record is None:
                return
            if activation_path.read_bytes() == canonical_activation_path.read_bytes():
                activation_record = canonical_record
            elif not candidate_activation:
                fail(errors, "worktree activation record does not match the canonical checkpoint")
                return
        if candidate_activation and (
            canonical_activation_path is None
            or activation_path.read_bytes() != canonical_activation_path.read_bytes()
        ):
            warnings.append(
                "activation artifacts are structurally valid proposal data only; "
                "the Session Core is not active before canonicalization"
            )
            canonical_repository_root = root
            canonical_source_root = source_root
            canonical_activation_path = activation_path
            activation_record = local_activation_record
    elif local_record_status != "proposed":
        fail(errors, "worktree AI charter activation status must be proposed or active")

    document = summary.get("document", {})
    traceability = summary.get("traceability", {})
    if not isinstance(document, dict) or not isinstance(traceability, dict):
        fail(errors, "AI charter summary document and traceability must be objects")
        return
    expected = {
        "id": CHARTER_ID,
        "version": CHARTER_VERSION,
        "source": CHARTER_PATH.as_posix(),
    }
    for field, value in expected.items():
        if document.get(field) != value:
            fail(errors, f"AI charter summary {field} must be {value!r}")

    adapter_markers = (
        CHARTER_PATH.as_posix(), CHARTER_ID, f"version {CHARTER_VERSION}",
        ACTIVATION_RECORD_PATH.as_posix(), "grants no approval",
    )
    for marker in adapter_markers:
        if marker not in adapter:
            fail(errors, f"AGENTS.md lacks required charter marker: {marker}")

    charter_markers = (
        f"**Document ID:** {CHARTER_ID}", f"**Version:** {CHARTER_VERSION}",
        "does not grant protocol authority", "AI work is evidence",
        "not a fallback authority profile",
    )
    for marker in charter_markers:
        if marker not in charter:
            fail(errors, f"AI charter lacks required authority marker: {marker}")
    validate_no_authority_grants("AGENTS.md", adapter, errors)
    validate_no_authority_grants("AI charter", charter, errors)

    activation = metadata_value(adapter, "Activation decision")
    adapter_status = metadata_value(adapter, "Adapter status")
    adapter_record = metadata_value(adapter, "Activation record")
    adapter_registry = metadata_value(adapter, "Activation decision registry")
    digest = metadata_value(adapter, "Activated charter digest")
    charter_activation = metadata_value(charter, "Activation decision")
    charter_status = metadata_value(charter, "Status")
    charter_authority = metadata_value(charter, "Authority class")
    charter_registry = metadata_value(charter, "Activation decision registry")
    inactive_values = {None, "None", "None in this template", "None in this candidate"}

    record_charter = activation_record.get("charter", {})
    record_summary = activation_record.get("summary", {})
    record_adapter = activation_record.get("adapter", {})
    if not all(isinstance(value, dict) for value in (record_charter, record_summary, record_adapter)):
        fail(errors, "activation record charter, summary, and adapter fields must be objects")
        return
    record_expected = {
        "$schema": "./ai-charter-activation.schema.json",
        "schema_version": 1,
        "record_id": "mininet-primary-ai-engineer-charter",
        "decision_registry": adapter_registry,
        "phase_record": PHASE_RECORD_PATH.as_posix(),
        "rollback_decision_required": True,
    }
    for field, value in record_expected.items():
        if activation_record.get(field) != value:
            fail(errors, f"AI charter activation record {field} must be {value!r}")
    if adapter_record != ACTIVATION_RECORD_PATH.as_posix():
        fail(errors, "AGENTS.md activation record path is not canonical")
    for field, value in {
        "id": CHARTER_ID,
        "version": CHARTER_VERSION,
        "path": CHARTER_PATH.as_posix(),
    }.items():
        if record_charter.get(field) != value:
            fail(errors, f"activation record charter {field} must be {value!r}")
    if record_adapter.get("path") != "AGENTS.md":
        fail(errors, "activation record adapter path must be 'AGENTS.md'")
    if record_summary.get("path") != CHARTER_SUMMARY_PATH.as_posix():
        fail(errors, "activation record summary path is not canonical")

    record_status = activation_record.get("status")
    if record_status == "proposed":
        if not adapter_status or not adapter_status.startswith("Proposed operational loader"):
            fail(errors, "unactivated AGENTS.md must declare proposed adapter status")
        if activation not in inactive_values:
            fail(errors, "proposed activation record conflicts with activated AGENTS.md")
        if digest not in inactive_values:
            fail(errors, "inactive AGENTS.md must not declare an activated charter digest")
        if charter_activation not in inactive_values:
            fail(errors, "inactive AGENTS.md conflicts with an activated charter declaration")
        if not charter_status or not charter_status.startswith("Proposed operational"):
            fail(errors, "unactivated charter must declare proposed status")
        if not charter_authority or "Unclassified" not in charter_authority:
            fail(errors, "unactivated charter must preserve pending effect classification")
        if document.get("status") != "draft":
            fail(errors, "unactivated AGENTS.md requires a draft charter summary")
        if document.get("source_sha256") is not None:
            fail(errors, "unactivated charter summary source_sha256 must be null")
        for field in ("decision_ref", "decision_record", "phase", "effective_at"):
            if activation_record.get(field) is not None:
                fail(errors, f"proposed activation record {field} must be null")
        if any(
            record.get("sha256") is not None
            for record in (record_charter, record_summary, record_adapter)
        ):
            fail(errors, "proposed activation record must not contain activated digests")
        if activation_record.get("superseded_by") is not None:
            fail(errors, "proposed activation record must not identify supersession")
        expected_phase_template = {
            "$schema": "./current-phase.schema.json",
            "schema_version": 1,
            "status": "unrecorded",
            "phase": None,
            "decision_ref": None,
            "effective_at": None,
            "superseded_by": None,
        }
        for field, value in expected_phase_template.items():
            if phase_template.get(field) != value:
                fail(errors, f"proposed phase record {field} must be {value!r}")
    elif record_status == "active":
        if activation in inactive_values:
            fail(errors, "bound activation record requires AGENTS.md Decision metadata")
        if adapter_status != "Operational loader; non-authorizing":
            fail(errors, "activated AGENTS.md must declare operational non-authorizing status")
        if activation_record.get("decision_ref") != activation:
            fail(errors, "AGENTS.md and activation record decisions do not match")
        if charter_activation != activation:
            fail(errors, "AGENTS.md and charter activation decisions do not match")
        if not charter_status or not charter_status.startswith("Operational"):
            fail(errors, "activated charter must declare operational status")
        if not charter_authority or re.search(r"unclassified|proposed", charter_authority, re.I):
            fail(errors, "activated charter requires a confirmed effect classification")
        if canonical_repository_root is None or canonical_source_root is None or canonical_activation_path is None:
            fail(errors, "active charter validation lacks a canonical checkpoint")
            return
        check_time = now or dt.datetime.now(dt.timezone.utc)
        if check_time.tzinfo is None:
            fail(errors, "validation time must include a timezone")
            return
        check_time = check_time.astimezone(dt.timezone.utc)
        if activation_record.get("phase") not in {"founder-guarded", "maintainer-assisted"}:
            fail(errors, "active charter record has an inapplicable or missing phase")
        if activation_record.get("superseded_by") is not None:
            fail(errors, "active charter record is superseded")
        activation_effective = parse_instant(
            activation_record.get("effective_at"),
            "activation effective time",
            errors,
        )
        if activation_effective and activation_effective > check_time:
            fail(errors, "AI charter activation effective time has not arrived")

        record_charter_digest = record_charter.get("sha256")
        record_summary_digest = record_summary.get("sha256")
        record_adapter_digest = record_adapter.get("sha256")
        if not digest or not re.fullmatch(r"[0-9a-f]{64}", digest):
            fail(errors, "activated AGENTS.md requires a lowercase SHA-256 charter digest")
        if digest != record_charter_digest:
            fail(errors, "AGENTS.md and activation record charter digests do not match")
        if not isinstance(record_charter_digest, str) or not re.fullmatch(r"[0-9a-f]{64}", record_charter_digest):
            fail(errors, "activation record requires a lowercase SHA-256 charter digest")
        elif record_charter_digest != hashlib.sha256(charter_path.read_bytes()).hexdigest():
            fail(errors, "activated charter digest does not match the charter file")
        if not isinstance(record_adapter_digest, str) or not re.fullmatch(r"[0-9a-f]{64}", record_adapter_digest):
            fail(errors, "activation record requires a lowercase SHA-256 adapter digest")
        elif record_adapter_digest != hashlib.sha256(adapter_path.read_bytes()).hexdigest():
            fail(errors, "activated adapter digest does not match AGENTS.md")
        if not isinstance(record_summary_digest, str) or not re.fullmatch(r"[0-9a-f]{64}", record_summary_digest):
            fail(errors, "activation record requires a lowercase SHA-256 summary digest")
        elif record_summary_digest != hashlib.sha256(summary_path.read_bytes()).hexdigest():
            fail(errors, "activated summary digest does not match the charter summary")
        if document.get("source_sha256") != record_charter_digest:
            fail(errors, "activated charter summary does not bind the charter digest")

        phase_record_path = safe_repo_path(
            canonical_repository_root,
            activation_record.get("phase_record"),
            "canonical phase record path",
            errors,
        )
        phase_record = (
            read_json_object(phase_record_path, "canonical phase record", errors)
            if phase_record_path else None
        )
        if phase_record is not None:
            if phase_record.get("status") != "active":
                fail(errors, "canonical phase record is not active")
            if phase_record.get("superseded_by") is not None:
                fail(errors, "canonical phase record is superseded")
            if phase_record.get("phase") != activation_record.get("phase"):
                fail(errors, "canonical phase does not match charter activation phase")
            phase_effective = parse_instant(
                phase_record.get("effective_at"),
                "canonical phase effective time",
                errors,
            )
            if phase_effective and phase_effective > check_time:
                fail(errors, "canonical governance phase is not yet effective")

        decision_record_path = safe_repo_path(
            canonical_repository_root,
            activation_record.get("decision_record"),
            "structured activation Decision path",
            errors,
        )
        decision = (
            read_json_object(decision_record_path, "structured activation Decision", errors)
            if decision_record_path else None
        )
        activation_record_digest = hashlib.sha256(canonical_activation_path.read_bytes()).hexdigest()
        if decision is not None:
            decision_expected = {
                "$schema": "../ai-charter-activation-decision.schema.json",
                "schema_version": 1,
                "object_type": "ai-charter-activation-decision",
                "decision_ref": activation,
                "status": "final",
                "activation_record_sha256": activation_record_digest,
                "charter_sha256": record_charter_digest,
                "summary_sha256": record_summary_digest,
                "adapter_sha256": record_adapter_digest,
                "phase": activation_record.get("phase"),
                "effective_at": activation_record.get("effective_at"),
                "superseded_by": None,
            }
            for field, value in decision_expected.items():
                if decision.get(field) != value:
                    fail(errors, f"structured activation Decision {field} must be {value!r}")
            classification = decision.get("classification")
            if classification not in {
                "operational", "protocol-governance", "constitutional",
            }:
                fail(errors, "structured activation Decision has an invalid classification")
            else:
                if not isinstance(charter_authority, str) or not re.search(
                    rf"\b{re.escape(classification)}\b", charter_authority, re.I
                ):
                    fail(
                        errors,
                        "charter authority class does not match the activation Decision classification",
                    )
                expected_summary_authority = f"{classification}-non-authorizing"
                if document.get("authority_class") != expected_summary_authority:
                    fail(
                        errors,
                        "charter summary authority class does not match the activation Decision classification",
                    )
            exact_digest = activation_artifacts_digest(
                activation_record_digest,
                str(record_charter_digest),
                str(record_adapter_digest),
                str(record_summary_digest),
            )
            if decision.get("activation_artifacts_sha256") != exact_digest:
                fail(errors, "structured activation Decision does not bind the activation artifacts")
            decision_effective = parse_instant(
                decision.get("effective_at"),
                "Decision effective time",
                errors,
            )
            if decision_effective and decision_effective > check_time:
                fail(errors, "structured activation Decision is not yet effective")
            cooling_required = decision.get("cooling_required")
            cooling_basis = decision.get("cooling_basis")
            if not isinstance(cooling_basis, str) or not cooling_basis.strip():
                fail(errors, "structured activation Decision requires a cooling basis")
            if not isinstance(cooling_required, bool):
                fail(errors, "structured activation Decision cooling_required must be boolean")
            elif cooling_required:
                cooling_complete = parse_instant(
                    decision.get("cooling_completed_at"),
                    "Decision cooling completion time",
                    errors,
                )
                if cooling_complete and cooling_complete > check_time:
                    fail(errors, "structured activation Decision cooling period is incomplete")
            elif decision.get("cooling_completed_at") is not None:
                fail(errors, "no-cooling Decision must set cooling_completed_at to null")

        if not adapter_registry or adapter_registry in inactive_values:
            fail(errors, "activated AGENTS.md requires an activation decision registry")
        elif charter_registry != adapter_registry or activation_record.get("decision_registry") != adapter_registry:
            fail(errors, "AGENTS.md and charter activation decision registries do not match")
        else:
            registry_path = safe_repo_path(
                canonical_source_root,
                adapter_registry,
                "activation Decision registry path",
                errors,
            )
            if registry_path and not registry_path.is_file():
                fail(errors, "activation Decision registry does not exist in canonical state")
            elif registry_path and decision_record_path:
                registry_text = registry_path.read_text(encoding="utf-8")
                decision_ref = str(activation)
                decision_rel = str(activation_record.get("decision_record"))
                if decision_ref not in registry_text or decision_rel not in registry_text:
                    fail(errors, "activation registry does not index the structured final Decision")
                supersession = re.search(
                    rf"^AI-Charter-Activation-Superseded:\s*"
                    rf"{re.escape(decision_ref)}\s*->\s*(\S+)\s*$",
                    registry_text,
                    re.M,
                )
                if supersession:
                    fail(
                        errors,
                        f"activation Decision is superseded by {supersession.group(1)}",
                    )

        if document.get("status") not in {"operational", "normative"}:
            fail(errors, "activated AGENTS.md requires an operational or normative charter summary")
        decisions = traceability.get("decisions", [])
        if activation not in decisions:
            fail(errors, "activated charter summary does not cite its activation decision")
    else:
        fail(errors, "AI charter activation record status must be proposed or active")

    codeowners = root / ".github/CODEOWNERS"
    if record_status == "active" and not codeowners.is_file():
        fail(errors, "activated AI charter requires an installed .github/CODEOWNERS")
    if not codeowners.is_file() and record_status == "proposed":
        codeowners = root / ".github/CODEOWNERS.template"
    if codeowners.is_file():
        owners_text = codeowners.read_text(encoding="utf-8")
        routes = {
            "AGENTS.md": r"^/AGENTS\.md\s+.*reviewers-constitution",
            "CLAUDE.md": r"^/CLAUDE\.md\s+.*reviewers-constitution",
            "GEMINI.md": r"^/GEMINI\.md\s+.*reviewers-constitution",
            ".cursorrules": r"^/\.cursorrules\s+.*reviewers-constitution",
            ".cursor/rules/": r"^/\.cursor/rules/\s+.*reviewers-constitution",
            "governance/": r"^/governance/\s+.*reviewers-constitution",
            "docs/governance/": r"^/docs/governance/\s+.*reviewers-constitution",
            ".github/copilot-instructions.md":
                r"^/\.github/copilot-instructions\.md\s+.*reviewers-constitution",
        }
        for routed_path, pattern in routes.items():
            if not re.search(pattern, owners_text, re.M):
                fail(errors, f"CODEOWNERS does not route {routed_path} to constitutional review")
        generic_github = owners_text.find("/.github/")
        copilot_route = owners_text.find("/.github/copilot-instructions.md")
        if generic_github >= 0 and copilot_route <= generic_github:
            fail(errors, "specific Copilot CODEOWNERS route must follow the generic .github route")

    policy = root / "governance/policy.yml"
    if policy.is_file():
        policy_text = policy.read_text(encoding="utf-8")
        for glob in (
            "AGENTS.md", "CLAUDE.md", "GEMINI.md", ".cursorrules",
            ".cursor/rules/**", ".github/copilot-instructions.md",
            "docs/governance/**", "governance/**",
        ):
            if f"glob: {glob}" not in policy_text:
                fail(errors, f"governance policy does not protect {glob}")

    surfaces = instruction_surfaces(root)
    if record_status == "active" and canonical_repository_root is not None:
        canonical_surfaces = instruction_surfaces(canonical_repository_root)
        local_paths = set(surfaces) - {"AGENTS.md"}
        canonical_paths = set(canonical_surfaces) - {"AGENTS.md"}
        if local_paths != canonical_paths:
            fail(errors, "active worktree instruction-surface set differs from canonical state")
        for loader_name in sorted(local_paths & canonical_paths):
            if surfaces[loader_name].read_bytes() != canonical_surfaces[loader_name].read_bytes():
                fail(errors, f"active worktree instruction surface differs from canonical state: {loader_name}")
    for loader_name, loader in surfaces.items():
        if loader_name != "AGENTS.md":
            validate_model_specific_loader(
                loader_name,
                loader.read_text(encoding="utf-8"),
                errors,
                warnings,
                require_agents_reference=record_status == "active",
            )


def validate_baseline(
    root: Path,
    errors: list[str],
    warnings: list[str],
    canonical_root: Path | None = None,
    now: dt.datetime | None = None,
    candidate_activation: bool = False,
) -> None:
    required = [
        root / "governance/policy.yml",
        root / "governance/exceptions.yml",
        root / "governance/document-summary.schema.json",
        root / ".github/pull_request_template.md",
    ]
    for path in required:
        if not path.is_file():
            fail(errors, f"missing required governance artifact: {path.relative_to(root)}")

    schema = root / "governance/document-summary.schema.json"
    if schema.is_file():
        try:
            json.loads(schema.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            fail(errors, f"invalid JSON schema: {exc}")

    exceptions = root / "governance/exceptions.yml"
    if exceptions.is_file():
        text = exceptions.read_text(encoding="utf-8")
        for match in re.finditer(r"expires:\s*(\d{4}-\d{2}-\d{2})", text):
            expiry = dt.date.fromisoformat(match.group(1))
            if expiry < dt.date.today():
                fail(errors, f"expired governance exception: {expiry}")
        if "exceptions:" not in text:
            fail(errors, "exceptions.yml lacks an exceptions list")

    codeowners = root / ".github/CODEOWNERS"
    template = root / ".github/CODEOWNERS.template"
    if not codeowners.exists() and not template.exists():
        warnings.append("no CODEOWNERS or CODEOWNERS.template found")

    validate_session_charter(
        root, errors, warnings, canonical_root, now, candidate_activation
    )


def validate_proposal(body: str, changed: list[str], errors: list[str], warnings: list[str]) -> None:
    if not body.strip():
        fail(errors, "proposal body is empty or unavailable")
        return
    for heading in REQUIRED_HEADINGS:
        if not re.search(rf"^##+\s+{re.escape(heading)}\s*$", body, re.I | re.M):
            fail(errors, f"missing proposal heading: {heading}")

    selected_classes = CHECKED_CHANGE_CLASS.findall(body)
    if len(selected_classes) != 1:
        fail(errors, "proposal must select exactly one change class checkbox")
    selected_class = selected_classes[0] if len(selected_classes) == 1 else None

    if "REPLACE_WITH_FINAL_DIGEST" in body:
        fail(errors, "exact state still contains the template placeholder")

    protected = any(any(path == p or path.startswith(p) for p in PROTECTED_PREFIXES) for path in changed)
    tier_f = any(any(path == p or path.startswith(p) for p in TIER_F_PREFIXES) for path in changed)
    if protected and selected_class not in SENSITIVE_CHANGE_CLASSES:
        fail(errors, "protected paths changed without a sensitive change classification")
    if tier_f:
        if not re.search(r"\bINV-[A-Z0-9-]+\b", body):
            fail(errors, "Tier-F path changed without an invariant identifier")
        if not re.search(r"\bD-\d{4}\b", body):
            fail(errors, "Tier-F path changed without a decision identifier")

    for name, pattern in PROHIBITED_CLAIMS.items():
        if pattern.search(body) and not re.search(r"reject|forbid|must not|no path|does not", body, re.I):
            warnings.append(f"proposal may contain prohibited constitutional claim: {name}")

    if re.search(r"AI assisted", body, re.I) and not re.search(r"persistent.*identity|human|pseudonymous", body, re.I | re.S):
        fail(errors, "AI-assisted proposal lacks a persistent submitting/authorizing identity declaration")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument(
        "--mode",
        choices=("baseline", "proposal", "runtime", "strict"),
        default="baseline",
    )
    parser.add_argument("--proposal-body")
    parser.add_argument("--changed-paths", help="newline-delimited changed-path file")
    parser.add_argument(
        "--canonical-root",
        help=(
            "separate, independently verified canonical repository checkpoint; "
            "required when the worktree activation record is active"
        ),
    )
    parser.add_argument(
        "--candidate-activation",
        action="store_true",
        help=(
            "validate a noncanonical activation proposal structurally without "
            "treating its Session Core as active"
        ),
    )
    args = parser.parse_args()

    root = Path(args.root).resolve()
    errors: list[str] = []
    warnings: list[str] = []
    canonical_root = Path(args.canonical_root).resolve() if args.canonical_root else None
    if args.mode == "runtime":
        validate_runtime_instruction_surfaces(root, canonical_root, errors)
        if errors:
            for message in errors:
                print(f"error: {message}", file=sys.stderr)
            return 1
    validate_baseline(
        root,
        errors,
        warnings,
        canonical_root,
        candidate_activation=args.candidate_activation,
    )

    if args.mode in {"proposal", "strict"}:
        body = read_optional(args.proposal_body) or os.getenv("PR_BODY", "")
        changed = read_optional(args.changed_paths).splitlines() if args.changed_paths else []
        validate_proposal(body, [p.strip() for p in changed if p.strip()], errors, warnings)

    for message in warnings:
        print(f"warning: {message}")
    for message in errors:
        print(f"error: {message}", file=sys.stderr)
    if args.mode == "strict" and warnings:
        return 1
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
