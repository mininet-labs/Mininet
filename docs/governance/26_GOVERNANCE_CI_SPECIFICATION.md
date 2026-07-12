# Governance CI Specification

**Status:** Operational bootstrap specification  
**Version:** 1.1

## 1. Purpose

Governance CI checks whether repository proposals contain the minimum evidence required for legitimate review. It cannot replace human or governed authorization.

## 2. Checks

The reference validator in `repository-template/tools/check_governance.py` performs:

- policy and schema presence checks;
- pull-request body heading checks when supplied through an environment variable or file;
- path classification against protected path rules;
- invariant and decision-reference requirements for Tier-F paths;
- prohibited claim detection for forced update, permanent owner/admin authority and money-to-vote mappings;
- JSON parseability for the document-summary schema and exact identity checks for the Document 50 summary;
- dependency-exception expiry validation;
- presence of CODEOWNERS and explicit constitutional routing for `AGENTS.md`;
- session-adapter path, ID, version, activation-state and activated-digest consistency;
- external activation-record binding of charter, adapter, and summary SHA-256 digests;
- comparison with a separately supplied canonical checkpoint for active worktrees;
- structured final activation-Decision binding, active phase equality, cooling-basis, effective-time, and append-only supersession checks;
- required non-authorizing markers in the charter and adapter; and
- conservative rejection of explicit AI or unilateral-Founder Authority-grant patterns;
- blocking detection of known conflicting current-`CLAUDE.md` instructions;
- blocking `AGENTS.md` references in present model-specific loaders after activation; and
- pre-activation warnings when a present model-specific loader does not reference `AGENTS.md`.

The validator does not establish that a supplied checkpoint is legitimate; the caller must obtain it independently from current canonical infrastructure. It also does not perform general JSON Schema validation for every summary instance, prove path-by-path CODEOWNERS coverage beyond the standing AI surfaces, semantically detect every subtle or paraphrased Authority grant, validate exception ownership, enforce stable metadata across the legacy governance corpus, or convert every prohibited-claim warning into a blocking result. These gaps MUST NOT be described as implemented.

## 3. Modes

- `baseline`: validates required policy artifacts, schema parseability, exception expiry, standing-charter structure, external activation-record and digest consistency, model-loader reconciliation, and adapter review routing.
- `runtime`: MUST execute from a separately verified canonical checkout and compares all known canonical and candidate instruction surfaces byte-for-byte before candidate instructions are parsed.
- `proposal`: additionally validates changed paths and proposal body.
- `strict`: runs proposal validation and treats declared warnings as failures; it requires a proposal body and is intended for blocking proposal enforcement after baseline cleanup.

An active worktree requires `--canonical-root` pointing to a separate, independently verified canonical checkout or checkpoint. Pull-request CI may add `--candidate-activation` to validate a proposed activation structurally; that mode explicitly does not make its Session Core active. A mutable worktree session should first execute `<canonical>/tools/check_governance.py --mode runtime --root <worktree> --canonical-root <canonical>`; an in-worktree checker cannot establish trust-before-load.

## 4. Security

The workflow MUST use least-privilege permissions. It MUST NOT expose release secrets to pull-request code. Third-party actions SHOULD be pinned to immutable commit SHAs before activation.

## 5. Adoption sequence

1. Run in warning mode on existing proposals.
2. Correct false positives and document exceptions.
3. Make baseline/schema checks blocking.
4. Make proposal metadata blocking.
5. Enable protected-path review requirements.
6. Move equivalent validation into Forge policy execution.
