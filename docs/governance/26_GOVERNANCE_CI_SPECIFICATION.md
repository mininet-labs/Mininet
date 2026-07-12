# Governance CI Specification

**Status:** Operational bootstrap specification  
**Version:** 0.4

## 1. Purpose

Governance CI checks whether repository proposals contain the minimum evidence required for legitimate review. It cannot replace human or governed authorization.

## 2. Checks

The reference validator in `repository-template/tools/check_governance.py` performs:

- policy and schema presence checks;
- pull-request body heading checks when supplied through an environment variable or file;
- path classification against protected path rules;
- invariant and decision-reference requirements for Tier-F paths;
- prohibited claim detection for forced update, permanent owner/admin authority and money-to-vote mappings;
- machine-readable governance-summary validation;
- dependency-exception expiry validation;
- CODEOWNERS coverage warnings.

## 3. Modes

- `baseline`: validates repository policy files and summaries.
- `proposal`: additionally validates changed paths and proposal body.
- `strict`: treats declared warnings as failures and is intended for `main` protection after a baseline cleanup.

## 4. Security

The workflow MUST use least-privilege permissions. It MUST NOT expose release secrets to pull-request code. Third-party actions SHOULD be pinned to immutable commit SHAs before activation.

## 5. Adoption sequence

1. Run in warning mode on existing proposals.
2. Correct false positives and document exceptions.
3. Make baseline/schema checks blocking.
4. Make proposal metadata blocking.
5. Enable protected-path review requirements.
6. Move equivalent validation into Forge policy execution.
