# Beta contributor guide

This is the practical door for people who want to help Mininet before a
public Beta is published. It is an orientation and evidence guide, not a
grant of authority. The canonical documents and accepted decisions linked
from the [README](../README.md) control if this page is ever stale.

## Current Beta boundary

The target is a real, honest two-device path. The repository contains a
substantial Rust core, an Android foundation, and Rust-side LAN/QR pairing
work, but the end-to-end Beta is not yet validated.

Still outstanding for a genuine device Beta include real Android CI and
two-device acceptance for the LAN/QR path ([#200](../../issues/200)), the
Android BLE implementation and hardware test ([#201](../../issues/201)),
background lifecycle behavior ([#202](../../issues/202)), dependency/build
verification ([#203](../../issues/203)), Android CI assembly ([#204](../../issues/204)),
and reproducible APK evidence ([#205](../../issues/205)). The external gates
and research questions are tracked separately in [#262](../../issues/262).

This project is not ready for real value, treasury custody, production
personhood claims, or production cryptography. A passing test suite is not an
external audit. Current governance and consensus counts must be described in
terms of identity roots, not as proof of unique humans.

## Choose a route

| You can help with | Safe first route | Evidence to return |
|---|---|---|
| Try a Beta flow or reproduce a bug | Use the [Beta test report](../../issues/new?template=beta-test-report.yml) | Exact revision, device/toolchain, steps, expected/observed result, logs, and limits |
| Rust implementation or tests | Pick an open implementation issue, read the five canon docs, then claim the issue in the work registry | Focused tests plus the full PR ritual and a clear non-goals section |
| Android, LAN, BLE, or device testing | Start with #200-#205 and state whether you have an emulator, one device, or two physical devices | Device-specific evidence; do not generalize emulator/Rust results to hardware |
| Documentation, research, or threat modeling | Use #262 or #263, or open a scoped design/research issue | Sources, alternatives, falsification conditions, and unresolved questions |
| Security or external review coordination | Read [`docs/gates/`](gates/README.md) and use the [review response template](gates/EXTERNAL_REVIEW_RESPONSE_TEMPLATE.md) | Scope, independence, findings, disposition, residual risk, and review date |
| Governance/process or contributor routing | Start with #263 and the [preliminary report](design/mininet-teams-and-contributor-routing.md) | A proposal, not an activated team or authority claim |
| Domain consultation or employee-sponsored work | Declare the relevant conflict/relationship privately where needed, then contribute through a scoped issue | Technical evidence; employment or funding does not confer authority |

If you are unsure, choose the smallest documentation, reproduction, or test
task and ask in the issue. You do not need to infer policy from a GitHub team
name, a job title, a bounty, or an AI-generated suggestion.

## The contribution path

1. **Discover** a route and read its linked scope.
2. **Choose** a privacy mode: anonymous, pseudonymous, or public. Legal-name
   disclosure is not the default.
3. **Claim** one open issue and record the branch, paths, lease, and planned
   Decision identifier in `governance/work-claims.json`. A claim prevents
   collisions; it is not approval or legitimacy.
4. **Build or test** against the exact revision. Keep Rust-only, emulator,
   physical-device, external-review, and research evidence distinct.
5. **Peer review** the exact state and record adverse findings. Peer review is
   not the same event as authorized approval.
6. **Handoff** the evidence for the applicable human or governance authority.
   AI reviews are evidence and carry zero approval weight.

Use [`CONTRIBUTOR_TASK_BRIEF_TEMPLATE.md`](CONTRIBUTOR_TASK_BRIEF_TEMPLATE.md)
when an issue does not already give enough structure. Use
[`CONTRIBUTING.md`](../CONTRIBUTING.md) for the branch and validation ritual.

## Teams, employees, and consultants

Mininet's intended long-term model is a set of bounded working groups, not a
permanent maintainer class. The existing Governance Pack describes
contributors, reviewers, maintainers, integration representatives, and
security stewards, as well as expiring terms, appeals, conflicts, and group
lifecycles. The current Forge-native charter schema is a design artifact; no
runtime recruitment or delegation service is active yet.

An employee or consultant is welcome as a contributor. The employment or
client relationship may matter for conflict disclosure or compensation, but
it is not a governance credential. Organization roots are not governance-
eligible. A group may organize ordinary implementation work inside accepted
specifications, but it cannot amend frozen invariants, make a personhood
claim, authorize value, or force an update.

The intended automatic experience is guided matching: a client suggests a
task from declared skills, domain, dependencies, risk, and evidence needs;
the person accepts it. Suggestions do not silently assign work, disclose
private employment data, appoint a reviewer, or create an authority
delegation. The design questions for a Mininet-native implementation are in
[#263](../../issues/263), not decided by this page.

## Releases and public confirmation

Peer findings, external challenge, release evidence, installation, and owner
adoption are separate stages. A public release page or a group recommendation
can make an exact release eligible for voluntary adoption; it cannot install
software on another device. Each owner retains an explicit typed choice to
stage, activate, defer, reject, fork, or roll back according to local policy.

## What to say in a report

Always state:

- the exact commit, release, or issue state;
- what you tested and what you did not test;
- the environment and reproducible steps;
- whether the result is Rust-only, emulator-only, physical-device, external,
  or research evidence;
- privacy, safety, and recovery limitations;
- what is not built, not audited, not anonymous, or not enforced.

Never upload secrets, private keys, unnecessary personal data, or a claim that
AI agreement is approval. For security-sensitive material, use the private
security route linked in the issue configuration.

## Further reading

- [Founder Directives](FOUNDER_DIRECTIVES.md)
- [Invariants](INVARIANTS.md)
- [Failure Book](FAILURE_BOOK.md)
- [Threat Model](THREAT_MODEL.md)
- [Decision Log](DECISION_LOG.md)
- [Governance Index](governance/00_GOVERNANCE_INDEX.md)
- [Preliminary teams/routing report](design/mininet-teams-and-contributor-routing.md)
- [External-review gate index](gates/README.md)
