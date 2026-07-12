# Governance Pack integration (v1.0)

**Status:** living record of what a founder-supplied "Mininet Governance
Pack" is, how it was integrated, and exactly what changed as a result.
Update this file in the same PR whenever a new pack version lands.

## What arrived

The founder supplied `mininet-governance-pack-v1.0.zip` (83 files: ~50
governance specification documents, five RFCs, five JSON Schemas with
worked examples for a future Forge-native signed governance-object
encoding, and a `repository-template/` of GitHub-facing artifacts). Its
own `README.md` states the combined scope: "v0.5 Forge-native governance
and anonymous compensation; v0.6 working-group governance and maintainer
rotation," built on an earlier v0.4 this repository had not previously
received. A v1.1 was flagged by the founder as forthcoming.

## v1.1 delta and bootstrap activation

Version 1.1 adds the model-neutral Primary AI Engineer Charter
(`GOV-AI-050`), a repository-root `AGENTS.md` session adapter, an external
activation record, phase and structured-Decision schemas, trust-before-load
runtime validation, and adversarial validator tests. D-0084 activates the
charter for the founder-guarded phase using exact content digests and a final
structured Decision. The role remains non-authorizing: file presence alone
does not activate a changed copy and grants no AI authority.

The accepted D-0082 deployment, not the original ZIP bytes, is the v1.1
integration baseline. An exact comparison found that all 65 governance and
Forge files landed byte-for-byte; nine template artifacts were activated
unchanged; and three were retained unchanged under `repository-template/`.
Five live paths intentionally differ from the ZIP and are preserved:

- `.github/ISSUE_TEMPLATE/config.yml` keeps free-form issues enabled;
- the implementation and research forms retain two YAML quoting fixes;
- the live concise pull-request template is not replaced by the staged
  13-heading template; and
- the live candidate governance workflow remains the adapted, blocking baseline-only
  rollout rather than enabling proposal-metadata enforcement.

The v1.1 workflow adaptation pins its checkout action, runs the validator's
unit tests, and supplies separate canonical checkouts. D-0083 promotes the
baseline job to a blocking check and defines a temporary Founder-operated
GitHub integration profile. A live CODEOWNERS routes to the Founder during
D-0083; the staged team-routed template remains deferred until those humans
exist. `governance-canonical.yml` becomes the trusted base-branch evaluator
after D-0084 is on `main`.

## Precedence — never inverted

The pack is explicit about its own subordination, and this integration
does not change that: its own `docs/governance/00_GOVERNANCE_INDEX.md`
states "This pack does not replace `SPEC-00`, `docs/INVARIANTS.md`, or
accepted entries in `docs/DECISION_LOG.md`," and `01_DEVELOPMENT_CONSTITUTION.md`
opens with "This document does not create a second Constitution... If it
conflicts with `SPEC-00` or a frozen invariant, this document is wrong and
must be amended." That matches this repository's own existing hierarchy
(`CLAUDE.md`'s "five canonical documents," read order: Founder Directives
→ Invariants → Decision Log → Failure Book → Threat Model), and this PR
changes none of those five documents' authority. `docs/governance/`,
`forge-native/`, and `governance/` (the new top-level policy-config
directory) are all **subordinate, supplementary** material:
specification and process, not constitution.

Note the repo does not have a literal `SPEC-00` file — `docs/INVARIANTS.md`
already cites "SPEC-00 §12" throughout as the whitepaper/constitutional
register those rows mirror. The pack's references to `SPEC-00` resolve to
that same pre-existing citation convention; no new document was invented
to satisfy it.

## Truth boundary (quoting the pack's own words)

> "The object and policy specifications in this pack are normative design
> proposals unless the current Mininet source explicitly implements them.
> Existing `mini-forge`, `mini-bounty`, identity, provenance, release, and
> consensus primitives are treated as foundations — not as proof that
> every object, privacy mechanism, dispute path, or working-group rule
> described here is already enforced."

This integration preserves that boundary. Nothing in this PR claims any
pack specification is implemented, audited, or enforced merely because it
now has a file in this repository. Where a pack document maps to real
code, the compatibility matrix below says so explicitly and names the
crate/test; where it doesn't, it says "specified only."

## Three numbering systems now coexist — by design, not collision

- `D-00xx` / `D-02xx` — this repo's own append-only decision log
  (`docs/DECISION_LOG.md`), unaffected.
- `SPEC-xx` — the pre-existing whitepaper/constitutional citation
  convention (`SPEC-00`, `SPEC-11`, ...), unaffected.
- `RFC-000x` — new with this pack (`RFC-0001` through `RFC-0005`), a
  governance-pack-internal numbering series. No prior RFC series existed
  in this repository, so there is no collision, but a future contributor
  should not confuse an `RFC-000x` reference with a `D-00xx` decision or
  a `SPEC-xx` citation — they are three different registers with
  different authority (RFCs here are proposals *within* the pack, not
  accepted decisions).

## What is active, staged, and still owner-dependent

**Deployed by D-0082 and updated by D-0083/D-0084:**
- `docs/governance/*.md` (50 docs + `CHANGELOG.md` + `RFC-0001`–`RFC-0005`) — copied verbatim, purely additive.
- `forge-native/schemas/*.json` + `forge-native/examples/*.json` — copied verbatim; all five schemas and three examples validated as parseable JSON.
- `governance/policy.yml`, `governance/exceptions.yml`, `governance/bootstrap-operating-state.json` and schemas — machine-readable policy and fail-closed D-0083 trigger state at repo root.
- `tools/check_governance.py` — the pack's reference validator, standard-library only. Run locally: `python3 tools/check_governance.py --mode baseline` (currently passes clean, 0 errors, 0 warnings).
- `.github/workflows/governance-policy.yml` — the baseline job is now blocking under D-0083; proposal-metadata remains staged until the expanded PR template is adopted.
- `.github/workflows/governance-canonical.yml` — after first canonicalization, runs the base branch's checker against proposal bytes without executing candidate code.
- `.github/ISSUE_TEMPLATE/*.yml` (bug/design/research/implementation/audit/bounty forms) — purely additive; no issue forms existed before.
- `.github/CODEOWNERS` — temporary D-0083 routing to the verified Founder user `@mininet-labs`; review routing creates no approval weight.
- `.github/CODEOWNERS.template` — future scoped-team routing, still inert until the named human teams exist.

**Deviated from the pack as shipped (and why):**
- `.github/ISSUE_TEMPLATE/config.yml` shipped with `blank_issues_enabled: false`. Changed to `true`. Disabling free-form issues would have silently changed how the founder has been filing issues (#8–#93) with no warning — "never break existing onboarding" overrides adopting this one field verbatim. Revisit once the issue-form set is actually in regular use.
- Fixed `mininet-labs/mininet` → `mininet-labs/Mininet` (repo slug casing) in `config.yml`'s contact links, to match the casing this repo's own `README.md` uses.
- The pack's `governance-policy.yml` ships a second job, `proposal-metadata`, that hard-requires a PR body to contain 13 specific section headings (`Change class`, `Exact state`, `Founder directives`, ...). This repo's live `.github/pull_request_template.md` does not produce those headings, so wiring that job in now would fail on literally every PR by construction — noise, not signal, and arguably "silently enabling breaking enforcement" by another name even with `continue-on-error: true`. It stays out of the live workflow.

**Staged (present in the repo, inert, not wired into any live path):**
- `repository-template/.github/pull_request_template.md` — the pack's expanded proposal template (13 required sections). Adopting it live would change what every future PR looks like; that's a founder call (Phase B/C of the pack's own plan), not something to activate unilaterally. Kept verbatim for review.
- `repository-template/.github/workflows/governance-policy.yml` — the two-job v1.1-safe template (baseline + proposal-metadata), with immutable action pins, a separate canonical checkpoint, candidate-activation checks, and validator tests. It may be adapted when the expanded template is adopted; it is not a verbatim v1.0 copy target.
- `repository-template/GITHUB_RULESETS_BLUEPRINT.md` — branch-protection/ruleset configuration guidance; entirely owner-privileged GitHub settings, see below.

**Still owner-dependent:**
- Creating the GitHub teams (`core-maintainers`, `reviewers-constitution`, `reviewers-identity`, `reviewers-consensus`, `reviewers-forge-release`, `reviewers-value-crypto`, `reviewers-storage`, `security-stewards`) and replacing the temporary Founder routes with the scoped template at D-0083 sunset.
- Replacing D-0083's temporary Founder-operated `main` ruleset at its sunset with D-0033's normal two-human approval floor.
- Deciding whether/when to adopt the expanded PR template and the `proposal-metadata` CI job (Phase B+).
- Secrets, deploy keys, release credentials, protected release environments, and other sensitive owner-held material. Private vulnerability reporting, dependency graph, Dependabot alerts/security grouping, branch-update suggestions, and merged-branch cleanup are active.
- Any decision to treat a `docs/governance/` document as promoted to constitutional status — that is an amendment (`docs/governance/39_CONSTITUTIONAL_AMENDMENT_PROTOCOL.md`), never a documentation PR.

## Compatibility matrix

Format: does an equivalent already exist in this repo, and what's the
relationship. "Specified only" means the pack document describes
something with no corresponding implementation in this tree today (stated
plainly, matching the pack's own truth-boundary language above).

| Pack document | Existing repo equivalent | Relationship |
|---|---|---|
| `000_GOVERNANCE_ARCHITECTURE.md` | *(none)* | Supplements — new layered-authority framing, explicitly subordinate |
| `01_DEVELOPMENT_CONSTITUTION.md` | `docs/FOUNDER_DIRECTIVES.md` (17 directives) | Supplements — explicitly "does not create a second Constitution"; a development-governance translation layer, not a rival |
| `02_LEGITIMACY_MODEL.md` | `mini-forge::governance` (propose/approve/merge/amend) | Supplements — formalizes states the crate already implements informally |
| `03_DIRECTIVE_TRACEABILITY.md` | `docs/DECISION_LOG.md`'s 7-field template; `CONTRIBUTING.md` checklist | Overlaps, supplements — a superset traceability block, doesn't contradict the existing template |
| `04_AI_HUMAN_COLLABORATION_WORKBOOK.md` | `CLAUDE.md` workflow ritual; D-0067 AI-assistance declarations | Supplements — generalizes practice already in force here |
| `05_SCALABLE_DEVELOPMENT_WORKFLOW.md` | `CONTRIBUTING.md` | Supplements — `CONTRIBUTING.md` is today's 2-person reality; this is the scaling roadmap |
| `06_REPOSITORY_AND_FORGE_OPERATIONS.md` | Branch-restart ritual (`CLAUDE.md`); `mini-forge`/`mini-sync` | Supplements |
| `07_RELEASE_AND_OWNER_ADOPTION.md` | `mini-forge::release` (D-0070), `mini-installer` (D-0071), `mini-update::AdoptionState` | Supplements — prose description of what these crates already enforce in code; no conflict found |
| `08_FOUNDER_BOOTSTRAP_AND_HANDOFF.md` | "Founder merges PRs himself" convention (`CLAUDE.md`) | Supplements |
| `09_TRANSITION_TO_SELF_GOVERNANCE.md` | `docs/gates/` (external legitimacy gates) | Adjacent — different axis (governance-authority transfer vs. external-audit gates); supplements |
| `10_GITHUB_DECOMMISSION_PLAN.md` | README's "temporary public mirror" framing; `tools/no_github_outage_demo.sh` (D-0081) | Supplements — D-0081 is a runnable proof of this doc's premise |
| `11_WORKING_GROUPS_AND_MAINTAINERS.md` | *(none)* | Net-new, forward-looking; specified only |
| `12_ANONYMOUS_CONTRIBUTION_AND_COMPENSATION.md` | `mini-bounty`; `docs/design/bounty-and-review.md` | Overlaps, supplements — formalizes rules the crate's code already encodes |
| `13_REPOSITORY_OWNER_SETUP_GUIDE.md` | *(none)* | Net-new; maps directly to "Founder must complete" above |
| `14_CANONICAL_VOCABULARY.md` | `CLAUDE.md`'s established terms ("identity root," never "verified human") | Overlaps, no contradiction found; supplements |
| `15_CONSISTENCY_MATRIX.md` | *(none — this file's own cross-document review aid)* | Supplements |
| `16_PROTOCOL_ONTOLOGY.md` | *(none)* | Net-new; supplements |
| `17_NORMATIVE_LANGUAGE_AND_SPEC_TEMPLATE.md` | *(none)* | Net-new; supplements |
| `18_GOVERNANCE_STATE_MACHINES.md` | Real type-state pipelines in `mini-forge`/`mini-installer` (Rust enums, not prose) | Supplements — this repo already builds enforced type-state machines; this is the prose spec |
| `19_GOVERNANCE_DECISION_TABLE.md` | `CONTRIBUTING.md`'s "author not independent reviewer"; D-0033 2-approval floor | Overlaps, supplements |
| `20_FAILURE_MODES_AND_CONTINUITY.md` | `docs/THREAT_MODEL.md` | Adjacent — THREAT_MODEL.md covers protocol/civilization threats; this covers governance-process threats specifically; no conflict |
| `21_GOVERNANCE_TEST_SUITE.md` | `mini-forge`'s existing adversarial tests (e.g. `author_never_counts_and_one_identity_root_counts_once`) | Supplements — names scenarios, some of which already have real Rust tests |
| `22_MACHINE_READABLE_SUMMARIES.md` | `docs/_generated/*` (`mininet_nav.py`'s index) | Adjacent — different purpose (doc metadata vs. code/symbol index); supplements |
| `23_REPOSITORY_ENFORCEMENT_ARCHITECTURE.md` | `ci.yml`'s existing staged-advisory pattern | Overlaps — this repo independently arrived at the same "observe before block" pattern; confirms, doesn't conflict |
| `24_PROPOSAL_METADATA_SPECIFICATION.md` | `.github/pull_request_template.md` (shorter, different field set) | Differs — kept staged, not activated live (see Deviations) |
| `25_CODEOWNERS_AND_REVIEW_ROUTING.md` | D-0083 live Founder route plus future team template | Net-new; `.github/CODEOWNERS` routes to the Founder with zero required approval, while `.github/CODEOWNERS.template` stages the later scoped-team model |
| `26_GOVERNANCE_CI_SPECIFICATION.md` | `.github/workflows/governance-policy.yml` | Activated (baseline job only) |
| `27_REPOSITORY_INTEGRATION_PLAN.md` | This integration | Phase A steps 2/3/5 done this PR; step 1 (teams) and step 4 (CI-baseline recording) plus Phases B–E remain founder decisions |
| `28_FORGE_NATIVE_GOVERNANCE_OBJECTS.md` | `mini-forge`'s existing (unsigned) governance objects; `forge-native/schemas/*.json` | Specified only — schemas describe a future signed-object encoding nothing in this repo implements or consumes yet |
| `29_ANONYMOUS_BOUNTY_LIFECYCLE.md` | `mini-bounty` | Supplements |
| `30_COMPENSATION_PRIVACY_AND_SETTLEMENT.md` | `mini-settlement`, `mini-value` stealth addresses | Supplements |
| `31_DISPUTES_APPEALS_AND_RESTITUTION.md` | *(none)* | Net-new; specified only |
| `32_PSEUDONYMOUS_REPUTATION_AND_KEY_CONTINUITY.md` | `did-mini` (pre-rotation, recovery, pairwise pseudonyms) | Supplements — did-mini already implements the crypto continuity this doc assumes |
| `33_WORKING_GROUP_CHARTER_AND_LIFECYCLE.md` | *(none)* | Net-new; not relevant at today's 2-contributor scale |
| `34_MAINTAINER_DELEGATION_AND_ROTATION.md` | `mini-forge`'s per-root approvals | Supplements |
| `35_CROSS_GROUP_INTEGRATION_COUNCIL.md` | *(none)* | Net-new, forward-looking |
| `36_SCALING_FROM_TWO_TO_THOUSANDS.md` | *(none)* | Net-new |
| `37_GITHUB_TO_FORGE_AUTHORITY_MAPPING.md` | `docs/design/self-hosted-forge-spine.md` | Overlaps in theme, different granularity (governance-authority mapping vs. technical-batch plan); supplements |
| `38_V05_V06_IMPLEMENTATION_BACKLOG.md` | GitHub roadmap issues #8–#93, hub #92 | Adjacent — a parallel backlog not yet merged into the GitHub roadmap; flagged as follow-up, not merged automatically |
| `39_CONSTITUTIONAL_AMENDMENT_PROTOCOL.md` | `CLAUDE.md`'s "frozen invariants are frozen... without an explicit founder decision recorded as a D-number" | Overlaps, formalizes an existing informal rule |
| `40_GOVERNANCE_SIMULATION_AND_STRESS_TESTING.md` | Tokenomics sim harness (different domain) | Adjacent; net-new for governance specifically |
| `41_EXTERNAL_REVIEW_AND_PUBLIC_CHALLENGE.md` | `docs/gates/`, `docs/audits/` | Overlaps, supplements |
| `42_GOVERNANCE_V1_CONFORMANCE_STANDARD.md` | *(none)* | Net-new |
| `43_SUCCESSION_AND_FOUNDER_DISAPPEARANCE.md` | *(none — a genuine, previously-undocumented gap)* | Net-new; specified only, closes a real documentation gap |
| `44_RIGHT_TO_FORK_EXIT_AND_RESTART.md` | `docs/design/fork-legitimacy.md`; README "forking is always free" | Overlaps, no conflict found |
| `45_GOVERNANCE_SECURITY_AND_PRIVACY_MODEL.md` | `docs/THREAT_MODEL.md` | Adjacent, same relationship as `20_` above |
| `46_IMPLEMENTATION_CONFORMANCE_MAP.md` | `docs/STATUS.md` | Overlaps in purpose — `docs/STATUS.md` remains this repo's authoritative living status doc per `CLAUDE.md`; this pack document is its own self-audit, cross-linked rather than merged |
| `47_ACTIVATION_DEPLOYMENT_AND_MIGRATION.md` | This PR | Activated — this PR is exactly this document's Phase A |
| `48_POST_V1_EVOLUTION_AND_OPEN_RESEARCH.md` | `docs/FAILURE_BOOK.md` (opposite direction — rejected paths, not future ones) | Adjacent; net-new |
| `49_V1_RELEASE_AUDIT_AND_SIGNOFF.md` | *(none — describes the pack's own authoring process)* | Informational only; no repo action needed |
| `50_PRIMARY_AI_ENGINEER_CHARTER.md` | `AGENTS.md`, `governance/ai-charter-activation.json`, D-0084 | Activated as an operational, model-neutral, non-authorizing bootstrap charter; exact digests and phase are externally bound |
| `CHANGELOG.md` | *(none — pack's own version history)* | Informational |
| `RFC-0001`–`RFC-0005` | *(none — no RFC series existed)* | Net-new numbering series; specified only |

## Verification performed

- All five `forge-native/schemas/*.json` and three `forge-native/examples/*.json` parsed with `python3 -m json.load` — valid.
- `python3 tools/check_governance.py --mode baseline` — exit 0, no errors, no warnings, against this repo's actual current tree (not the pack's own isolated copy).
- `docs/_generated/*` regenerated via `python3 tools/mininet_nav.py build` to index the new files.
- D-0082's original deployment changed no prior constitutional meaning. D-0083 is a later, explicit, temporary procedural weakening of D-0033 for GitHub `main` integration only; D-0084 separately activates the non-authorizing charter. The exact activation procedure and rollback commands are recorded in `docs/governance/ACTIVATION_CHECKLIST_v1.1.md`.

## Next time a pack version lands (v1.2+)

1. Diff the new zip's `MANIFEST.json` against this repo's `docs/governance/CHANGELOG.md` to find what actually changed.
2. Re-run this compatibility matrix exercise for new/changed documents only — don't re-derive rows that haven't changed.
3. Re-run `check_governance.py --mode baseline` before committing.
4. Update this file's "What arrived" section and re-run `mininet_nav.py build`.
