# GitHub Decommission Plan

**Status:** Future operational plan

## Objective

Remove GitHub as a dependency without losing history, discoverability, contributor access, evidence, or recovery capability.

## Modes

1. **Primary:** GitHub is canonical bootstrap surface.
2. **Hybrid:** GitHub and Forge synchronize; Forge verifies all canonical events.
3. **Mirror:** Forge is canonical; GitHub is read-only or delayed.
4. **Archive:** GitHub preserves historical snapshots only.
5. **Disabled:** No active GitHub dependency remains.

## Preconditions for Forge primacy

- complete source, issues, proposals, reviews, decisions, releases, and security records are replicated;
- Git-to-Forge object mappings are deterministic and auditable;
- contributors can submit, review, build, and receive compensation without GitHub;
- Forge operation survives loss of any one operator;
- governance and release keys are independent of GitHub accounts;
- public documentation explains how to verify the canonical Forge state;
- disaster recovery has been tested;
- the community has completed the required vote.

## Migration procedure

1. Freeze a documented GitHub checkpoint.
2. Import and verify all repository objects and governance records.
3. Run parallel canonicalization for a defined observation period.
4. Reconcile every mismatch publicly.
5. Change GitHub branch rules to read-only except mirror automation.
6. Publish canonical Forge identifiers and verification instructions.
7. Rotate any secrets or keys previously tied to GitHub automation.
8. Test contribution and release during a deliberate GitHub outage.
9. Hold the community decision on mirror, archive, or shutdown mode.
10. Preserve reversible recovery for a limited, governed period.

## What must not be lost

- authorship and pseudonymous attribution;
- issue and proposal history;
- review and rejection reasoning;
- security advisories with appropriate confidentiality;
- release and provenance records;
- decision log continuity;
- compensation claims;
- licences and public-domain dedication evidence.

## Return path

A future community may choose to operate a GitHub mirror again. Doing so must not restore GitHub as constitutional authority or require participants to use it.

## No-stranding rule

GitHub may not be disabled while contributors, maintainers, security reporters, release consumers, or archival users lack a tested alternative path. Decommissioning must improve sovereignty without silently reducing participation, disclosure safety, discoverability, or recoverability.
