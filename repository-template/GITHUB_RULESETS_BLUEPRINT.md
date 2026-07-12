# GitHub Rulesets Blueprint

This file is a configuration blueprint, not an importable claim that the settings are active.

## `main`

### Active founder-only profile (D-0083)

- Target the default branch (`main`) and set enforcement to Active.
- Require pull request; required approvals: 0.
- Founder is the mechanical merge operator; AI review weight is 0.
- Require conversation resolution.
- Require the `check`, `reproducibility`, and `governance-baseline` checks.
- Require branches to be up to date.
- Block force pushes and deletion; configure no bypass actor.
- After D-0084 is canonical and `governance-canonical.yml` runs from the base
  branch, add `canonical-governance` as a required check.

### Sunset target (D-0033)

- Require pull request.
- Required approvals: 2 for the current protocol repository.
- Dismiss stale approvals.
- Require approval of most recent push.
- Require Code Owner review.
- Require conversation resolution.
- Require checks: Rust `check`, reproducibility, canonical governance, and any
  additional dependency checks after their baselines are clean.
- Block force pushes and deletion.
- Restrict bypass to founder emergency account initially; log every use.
- Require signed commits after active maintainers have signing configured.

The active-state record at `governance/bootstrap-operating-state.json` fails
closed when D-0083 expires or another sunset trigger is recorded. The owner
must then replace the live ruleset with the D-0033 target before merges resume.

## `integration/*`

- Require pull request.
- Required approvals: 1 independent reviewer.
- Require Rust and governance checks.
- Dismiss stale approvals.
- Block force pushes and deletion while active.

## `release/*` and `v*`

- Restrict creation to release-signers or approved automation.
- Block deletion and modification.
- Require protected `release` environment.
- Require independent provenance and governance evidence before publication.
- Never make availability equivalent to forced adoption.
