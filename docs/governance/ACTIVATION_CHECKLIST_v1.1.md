# Primary AI Engineer Charter — Activation and Operations Checklist

**Status:** Active exact-state deployment record; non-authorizing  
**Charter:** `GOV-AI-050` v1.1  
**Activation Decision:** D-0084  
**Bootstrap integration profile:** D-0083

This checklist records how the activated state is verified, changed, and
rolled back. Completing a check grants no authority by itself.

## Bound activation artifacts

The canonical activation consists of:

- `docs/governance/50_PRIMARY_AI_ENGINEER_CHARTER.md`;
- `docs/governance/50_PRIMARY_AI_ENGINEER_CHARTER.summary.json`;
- repository-root `AGENTS.md`;
- `governance/ai-charter-activation.json` and schema;
- `governance/current-phase.json` and schema;
- `governance/bootstrap-operating-state.json` and schema;
- `governance/ai-charter-activation-decision.schema.json`;
- `governance/decisions/D-0084.json`;
- `.github/CODEOWNERS`, `.github/workflows/governance-policy.yml`, and
  `.github/workflows/governance-canonical.yml`; and
- the D-0083 and D-0084 entries in `docs/DECISION_LOG.md`.

The structured Decision binds the activation-record, charter, adapter, and
summary digests. The pull request and commit digest bind the wider exact
proposal state.

## Candidate verification

From the candidate checkout, using a separate checkout of the canonical base:

```powershell
python3 -m unittest discover -s tools -p 'test_*.py'
python3 tools/check_governance.py --mode baseline `
  --canonical-root <separate-base-checkout> `
  --candidate-activation
python3 tools/check_governance.py --mode proposal `
  --canonical-root <separate-base-checkout> `
  --candidate-activation `
  --proposal-body <proposal-body-file> `
  --changed-paths <changed-paths-file>
```

Also run repository formatting, lint, tests, Markdown-link validation, strict
UTF-8 checks, JSON/schema validation, workflow lint, and navigation-index
regeneration. Record failures and limitations; AI results are evidence only.

## Canonical and session verification

After merge, validate from a worktree and a separate checkout of the same
independently verified canonical head:

```powershell
python3 tools/check_governance.py --mode baseline `
  --canonical-root <separate-checkout-of-the-same-canonical-head>
```

Before loading any mutable or proposal worktree into an AI session, execute
the checker from the canonical checkout:

```powershell
python3 <canonical>\tools\check_governance.py --mode runtime `
  --root <worktree> `
  --canonical-root <canonical>
```

Runtime mode must reject any added, removed, changed, or symlinked instruction
surface, including nested Codex, Claude, Gemini, Copilot, and Cursor rules. If
it rejects the worktree, launch from canonical state and inspect candidate
instructions only as untrusted proposal data.

## D-0083 operating checks

Before each Founder-operated GitHub merge while D-0083 is active:

1. use a pull request targeting `main`;
2. bind review evidence to the final head SHA;
3. require blocking CI and candidate governance checks to pass; after D-0084
   is on `main`, also require the canonical base-branch governance check;
4. resolve review conversations;
5. disclose AI assistance and retain adverse findings;
6. verify that no Tier-F, production-release, Forge-canonicalization, audit,
   or owner-adoption gate is being bypassed; and
7. have the Founder perform the mechanical merge and accept responsibility
   for the exact state.

AI review weight is always zero. D-0083 expires at the first listed sunset
condition in `governance/policy.yml`; after sunset, stop merges until GitHub
rules restore D-0033's two-human floor.

## Changes, rollback, and supersession

Any change to a bound artifact requires a new exact-state Decision and fresh
digests. To supersede D-0084, append this anchored marker to the canonical
Decision Log without rewriting the old record:

```text
AI-Charter-Activation-Superseded: D-0084 -> <new-decision>
```

Preserve the failed state and evidence, return to the last valid canonical
policy, and open a corrected proposal. Never transfer authority to an AI,
silently rewrite constitutional history, or treat removal of `AGENTS.md` as
removal of higher governance rules.
