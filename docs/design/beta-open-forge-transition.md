# Open Beta and Forge Transition Milestone

**Status:** PR #334 scope contract; implementation in this branch must stay inside this boundary.

## Purpose

This milestone opens Mininet's final pre-Go-Live phase to broad public testing and contribution while making the transition away from GitHub a first-class engineering goal rather than a future cleanup task.

The target is not merely "collect beta feedback." The target is one coherent path:

> test an exact build -> submit reproducible evidence -> turn accepted findings into scoped work -> record contribution evidence -> issue safe Beta MINI for test/use and participation -> mirror the same workflow in Mininet Forge -> make Forge capable of becoming canonical -> Go-Live removes the bootstrap custodian.

GitHub remains a temporary mirror and intake adapter during this milestone. Every new workflow introduced here must have a Forge-native representation or an explicit migration mapping.

## Non-negotiable safety boundary

### Beta MINI is not production MINI

Pre-Go-Live Beta MINI MUST be unmistakably domain-separated test value:

- bound to a beta network/epoch identifier;
- resettable and explicitly non-production;
- never accepted by production settlement, treasury, governance, personhood, release authority, or custody paths;
- never convertible by protocol promise into production MINI;
- never represented as an investment, sale, debt, deposit, or guaranteed future entitlement;
- never used as vote, reviewer, maintainer, release, personhood, or reputation weight;
- suitable for exercising balances, transfers, rewards, fees, local provider flows, and failure/recovery UX before real value is allowed.

A future community may choose to recognize early work through an independently governed production mechanism after Go-Live, but this milestone MUST NOT encode an automatic conversion right. That would silently create a pre-allocation and a centralized issuer promise before the real value path has passed its audit gates.

### Participation reward evidence is separate from identity and authority

Before Go-Live the canonical Mininet participant classification remains `anonymous` under `docs/governance/52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md`.

Contribution records therefore bind to artifacts and one-time claim handles, not a stable contributor profile. A GitHub account is transport metadata only. A reward destination or claim key MUST NOT become a persistent Mininet identity, pseudonym, governance credential, or reputation score.

Rewards recognize useful work; they never purchase authority.

## Milestone tracks

### A. Open public Beta

Create one obvious entry point for a person with:

- one Android phone;
- two or more Android phones;
- emulator/toolchain access;
- Rust/dev skills;
- security/research skills; or
- no coding background but willingness to reproduce and report behavior.

The entry point must say exactly what can be tested today, what is unsafe/not audited, how to protect private information, how to submit a finding, and how to see whether the same finding is already known.

### B. Findings as structured evidence

Beta reports need a machine-readable core that can survive GitHub migration. A finding record must bind at minimum:

- exact revision/release/build identifier;
- evidence class (physical device, emulator, Rust/toolchain, research, external review);
- affected component/surface;
- reproduction steps or test procedure;
- expected and observed result;
- severity/impact claim;
- privacy-redaction statement;
- attachments/digests where applicable;
- disposition state and links to follow-up work.

GitHub issue forms are an adapter for this schema, not the canonical long-term model.

### C. Beta MINI test currency

Implement a small, auditable Beta MINI ledger/model that is incapable of being confused with production value. It must support the user-visible behaviors beta testers need to exercise: grants, balances, transfers/spends in the beta domain, reset/epoch rollover, and deterministic accounting evidence.

Two grant classes are required:

1. **testing grants** — enough Beta MINI to exercise product flows;
2. **participation grants** — issued for accepted testing, reproduction, documentation, review, coding, or other useful contribution evidence.

Grant policy must be explicit, bounded, reviewable, and independent of governance power. Sybil resistance for free test currency is an abuse-control problem, not a reason to invent premature personhood claims.

### D. Contribution and reward flow

The contribution flow must work for more than code. It should accept evidence for:

- bug discovery and high-quality reproduction;
- hardware/device matrix testing;
- regression verification;
- accessibility/usability findings;
- documentation corrections;
- threat modeling/security findings;
- research and protocol analysis;
- coding and tests;
- release/reproducibility work; and
- operational help that produces inspectable evidence.

An accepted contribution produces a content-addressed contribution/reward record. The record may authorize Beta MINI now and may be usable as evidence by a future, separately governed production bounty mechanism, but it cannot itself promise production value.

### E. Forge-first coordination

Mininet Forge is the main engineering objective of this milestone. Forge must be able to represent the full beta loop without GitHub-specific identities or APIs:

- publish beta campaigns/test missions;
- submit finding reports;
- deduplicate/link related findings;
- triage findings into task briefs;
- claim work with expiring claims;
- submit review handoffs and evidence;
- record accepted contributions;
- attach Beta MINI grant authorization evidence;
- publish milestone/readiness state;
- replicate all of the above through Mininet storage/sync;
- export/mirror to GitHub while GitHub remains useful;
- later run with GitHub unavailable.

Forge coordination objects provide evidence and workflow state. They do not create political authority or make a merge/release canonical by themselves.

### F. Go-Live transition readiness

PR #334 must leave an explicit checklist for the one-way transition required by the bootstrap governance decision:

- `forge_canonical = true` can be evidenced and represented;
- beta findings/tasks/reward evidence no longer depend on GitHub;
- a GitHub outage does not stop contribution intake or coordination;
- bootstrap contribution identity remains anonymous and is not retroactively linked;
- Beta MINI remains distinguishable from production value and can be retired/reset;
- production-value activation remains gated by its substantive cryptography/custody/settlement review requirements;
- Founder/bootstrap integration authority can end rather than merely become ceremonial text.

## Initial implementation slices in PR #334

This PR should land the largest coherent safe slice now, not merely prose:

1. Forge-native beta campaign, finding, disposition, contribution, and beta-grant authorization object types with strict validation and content IDs.
2. A domain-separated Beta MINI accounting primitive suitable for test grants and participation grants, with epoch reset semantics and no production-conversion path.
3. Tests proving money/balance cannot affect Forge approval/governance/review authority.
4. GitHub beta-test and contributor intake updates that map directly onto the Forge-native fields.
5. A public `docs/BETA_OPEN.md` entry point and test itinerary covering one-phone, two-phone, multi-hop/offline, lifecycle, recovery, privacy, and malformed/adversarial cases.
6. A migration matrix showing which current GitHub workflows already have Forge equivalents, which this PR adds, and what remains before `forge_canonical = true`.

## Explicit non-goals

PR #334 does **not**:

- activate real-money MINI;
- mint or promise future production tokens;
- waive the substantive external cryptography/value safety gates;
- solve personhood;
- make GitHub accounts Mininet identities;
- create contributor reputation as governance weight;
- make automated/AI triage authoritative;
- declare Forge canonical before the required outage/evidence path actually works; or
- declare Go-Live.

## Success condition

This milestone succeeds when a new person can arrive with no prior project context, choose a useful beta task, test an exact state, submit a privacy-safe structured finding, see it become scoped work, contribute a fix or verification, receive a safe Beta MINI participation grant where appropriate, and have the entire evidence chain representable inside Forge without creating a central issuer, identity authority, or pay-to-govern path.
