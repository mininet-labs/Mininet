# GitHub Rulesets Blueprint

This file is a configuration blueprint, not an importable claim that the settings are active.

## `main`

- Require pull request.
- Required approvals: 2 for the current protocol repository.
- Dismiss stale approvals.
- Require approval of most recent push.
- Require Code Owner review.
- Require conversation resolution.
- Require checks: existing Rust check job, dependency audit, dependency deny, reproducibility and governance-policy jobs after their baselines are clean.
- Block force pushes and deletion.
- Restrict bypass to founder emergency account initially; log every use.
- Require signed commits after active maintainers have signing configured.

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
