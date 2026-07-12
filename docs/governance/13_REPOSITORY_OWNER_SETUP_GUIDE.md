# Current Repository Owner Setup Guide

**Status:** Practical bootstrap checklist  
**Version:** 1.1  
**Audience:** Current owner of `mininet-labs/mininet`  
**Scope:** GitHub and operational setup only; it intentionally excludes committing or pushing these documentation files.

## 1. Secure the organization owner account

1. Enable a hardware security key or passkey for two-factor authentication.
2. Store recovery codes offline in two separate secure locations.
3. Create a second emergency organization owner account controlled through a separate credential path; do not use it for daily work.
4. Review organization owners and remove any unnecessary owner.
5. Do not give AI agents owner credentials, personal access tokens with administration rights, or release secrets.

## 2. Set organization-wide security policy

In GitHub organization settings:

1. Require two-factor authentication for members.
2. Restrict repository creation, deletion, visibility changes, and outside-collaborator invitations to owners initially.
3. Set the base permission to **Read** rather than Write.
4. Disable members' ability to change repository visibility or delete repositories.
5. Review installed GitHub Apps and OAuth apps; remove unused integrations.
6. Prefer GitHub Apps or fine-grained tokens over classic personal access tokens.
7. Set short token expirations and minimum scopes.

## 3. Create teams before granting direct access

Create teams such as:

- `founder-guardians`
- `core-maintainers`
- `reviewers-identity`
- `reviewers-consensus`
- `reviewers-forge-release`
- `reviewers-storage`
- `reviewers-value-crypto`
- `security-stewards`
- `release-signers`

Grant permissions to teams, not individuals, wherever possible. Start conservatively and expand when real contributors arrive.

## 4. Protect `main` with a ruleset

Go to **Repository -> Settings -> Rules -> Rulesets -> New branch ruleset**.

Target `main` and enable:

1. Restrict deletions.
2. Block force pushes.
3. Require a pull request before merging.
4. While D-0083 is active, set required approvals to 0. The Founder performs
   the merge; AI review is evidence with zero approval weight.
5. Require all conversations to be resolved.
6. Require `check`, `reproducibility`, and `governance-baseline` to pass.
7. Require the branch to be up to date.
8. Restrict deletion and force push; add no bypass actor.
9. After D-0084 is on `main`, require `canonical-governance`, which executes
   the base branch's checker against the proposal as untrusted data.

At D-0083 sunset, replace steps 4–9 with the normal D-0033 profile: two
independent human approvals, stale-approval dismissal, approval of the latest
reviewable push, Code Owner review by real scoped teams, canonical checks, and
no ordinary bypass. Require signed commits once all active maintainers have
signing configured.

Do not permit direct pushes to `main` for engineers or AI accounts.

## 5. Protect integration branches

Create a ruleset targeting `integration/*`:

- require pull requests;
- require one independent approval;
- require CI;
- dismiss stale approvals;
- block force pushes and deletion while active;
- allow the integration maintainer to merge only after checks pass.

For a two-engineer batch, create `integration/<batch-name>` from current `main`. Both engineers branch from it and review each other's feature proposals. The combined integration proposal then targets `main`.

## 6. Protect release branches and tags

Create rules for `release/*` and version tags such as `v*`:

- restrict creation to the release-signers team or release automation;
- block modification and deletion;
- require protected environment approval before publishing;
- require release artifacts to match canonical source and provenance evidence;
- do not allow an AI account to be the sole release approver.

## 7. Configure required CI checks

Open a successful Actions run and copy the exact check names into the `main` ruleset. Require at minimum:

- formatting;
- Clippy with warnings denied;
- complete workspace tests;
- dependency advisory audit;
- `cargo-deny` policy;
- adversarial/integration harnesses;
- reproducibility or provenance checks currently implemented;
- documentation/link checks when available.

Change advisory and dependency-policy jobs from informational to blocking. Any exception should identify the advisory, owner, justification, mitigation, and expiry date.

Pin third-party GitHub Actions to full commit SHAs rather than mutable major tags.

## 8. Configure security features

In **Settings -> Security and analysis**:

1. Enable dependency graph.
2. Enable Dependabot alerts.
3. Enable Dependabot security updates, but do not auto-merge security-critical dependency changes without review.
4. Enable secret scanning and push protection when available.
5. Enable private vulnerability reporting.
6. Configure security advisories and add `security-stewards`.
7. Create an internal response checklist: acknowledge, triage, reproduce, patch privately, request CVE if appropriate, coordinate release, publish advisory.

## 9. Configure environments and secrets

Create protected environments:

- `staging`
- `release`
- `production-mirror` if needed

For `release`:

- require approval from at least two authorized humans/pseudonymous signers;
- limit which branches/tags can deploy;
- use short-lived credentials where possible;
- keep signing keys outside ordinary CI if practical;
- prevent pull-request code from reading release secrets.

## 10. Set merge methods

Enable squash merge for ordinary proposals. Keep merge commits available for stacked or signature-sensitive work if needed. Disable rebase merge if it would destroy reviewed signed commit identity.

Require the final merge message to reference issues, proposals, AI-assistance record, and affected directives/invariants for protocol-critical changes.

## 11. Prepare CODEOWNERS

Create domain ownership rules mapping critical paths to teams. Do not assign a single person as the only owner of a critical area. Require cross-domain ownership when a proposal touches multiple subsystems.

Until the file is committed, keep an equivalent owner map in repository settings or a private setup checklist so team permissions can be prepared.

## 12. Open contributor intake

1. Allow public issue creation unless active abuse requires temporary restriction.
2. Use issue forms for bug, implementation, research, security, design, external audit, and bounty proposals.
3. Enable Discussions for broad ideas so issues remain actionable.
4. Publish a clear distinction between ordinary issues and private security reporting.
5. State that GitHub account identity is a temporary platform requirement, not Mininet's constitutional identity policy.

## 13. Configure AI-assisted development rules

1. AI accounts receive only permission to create branches, proposals, comments, and CI artifacts.
2. AI cannot push to `main`, approve reviews, satisfy quorum, access release secrets, sign releases, control treasury, or administer rulesets.
3. Require an AI-assistance disclosure for material proposals.
4. Require a named persistent human or governance identity to accept ownership of the final proposal; the identity may be pseudonymous.
5. Encourage separate AI author, adversary, and simplifier passes.
6. Require human inspection of final exact digest for security-critical work.
7. Install the root `AGENTS.md` adapter only in the exact proposal that activates its named charter digest, or after that charter is already canonical.
8. Require model-specific session loaders to reference `AGENTS.md` and remove contradictory or authority-expanding instructions.
9. Route changes to `AGENTS.md`, model-specific loaders, and `docs/governance/**` through constitutional reviewers with at least a protocol-critical path floor.
10. Keep activation in an external record that binds the charter, adapter, and summary digests; never put the adapter's expected digest inside the adapter itself.

## 14. Establish the two-engineer integration workflow

For each coupled batch:

1. Founder creates `integration/<batch>` from `main`.
2. Engineer A creates `feature/a/<scope>` from integration.
3. Engineer B creates `feature/b/<scope>` from integration.
4. B reviews A; A reviews B.
5. Each proposal runs required checks.
6. Merge both into integration.
7. Run full combined CI and adversarial tests.
8. Open integration-to-main proposal.
9. Obtain non-author engineer approval plus founder/third reviewer approval.
10. Canonicalize only the final tested integration digest.
11. Delete temporary branches after records are preserved.

## 15. Prepare for ten to one hundred contributors

1. Create domain teams and assign at least two reviewers per critical domain.
2. Adopt CODEOWNERS review routing.
3. Define maintainer nomination, inactivity, conflict, and removal procedures.
4. Introduce a rotating integration maintainer.
5. Enable a merge queue for independent changes after CI supports `merge_group` events.
6. Keep explicit integration branches for tightly coupled batches.
7. Require cross-domain reviewers for boundary changes.
8. Review permissions quarterly.
9. Publish maintainer and release-authority rosters by cryptographic identity; public legal names remain optional.

## 16. Set up compensation governance

1. Define bounties with objective acceptance criteria before assignment where possible.
2. Separate technical acceptance from payment destination.
3. Permit pseudonymous or anonymous claims.
4. Require treasury approval according to the current bootstrap policy.
5. Record payment evidence without collecting unnecessary identity information.
6. Ensure compensation never grants votes, merge rights, or release authority.
7. Obtain jurisdiction-specific legal advice before operating a live treasury; do not invent blanket identity collection in advance.

## 17. Founder emergency access

Keep one documented emergency path capable of restoring rules or stopping a compromised automation. Use hardware-backed credentials, offline recovery, and explicit logging.

Every use must produce a retrospective explaining:

- trigger;
- actions taken;
- affected history or secrets;
- independent review;
- whether the access should be narrowed.

Emergency access must never become a forced-update, treasury-seizure, or constitutional-amendment path.

## 18. Audit cadence

Monthly during active growth:

- review members, teams, outside collaborators, apps, deploy keys, tokens, environments, and ruleset bypass lists;
- verify required checks still correspond to real CI jobs;
- inspect failed security checks and expired exceptions;
- test recovery account access without exposing credentials.

Quarterly:

- rotate sensitive credentials;
- review maintainer activity and conflicts;
- exercise a compromised-maintainer scenario;
- verify GitHub export and Forge import paths;
- update the transition-readiness scorecard.

## 19. Begin hybrid Forge operation when ready

Do not wait until Forge is perfect to dogfood it. Start by importing signed commits, reviews, build results, and release objects while GitHub remains canonical.

Promote Forge only after it can:

- accept anonymous/pseudonymous proposals;
- enforce review and governance policy;
- reproduce canonical history;
- continue during GitHub outage;
- compensate accepted work;
- distribute verified releases without forced adoption.

## 20. Reduce founder authority deliberately

Do not remove founder protection based only on contributor count or pressure. Use the phase gates in Documents 08 and 09.

When the community and infrastructure can preserve legitimacy independently:

1. remove routine founder bypass;
2. transfer domain authority to working groups;
3. transfer release authority to threshold signers;
4. transfer constitutional authority to governance only after the applicable personhood mechanism is implemented and accurately described;
5. retain no hidden admin or forced-update key;
6. let the community vote on GitHub mirror or shutdown status.

## v0.2 owner verification checkpoint

After completing this guide, export or screenshot the active rulesets, team permissions, required checks, environment reviewers, bypass lists, security settings, and release permissions. Store the evidence privately and record a non-secret digest or review date in project operations. Repeat after every material GitHub settings change. Repository settings are part of the present security boundary even though they are not stored in Git.
