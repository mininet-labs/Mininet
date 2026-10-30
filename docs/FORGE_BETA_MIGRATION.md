# Open Beta -> Forge migration matrix

PR #334 treats GitHub as a temporary adapter, not the system Mininet is trying
to perfect. This matrix makes the remaining dependency explicit.

## Rule

A workflow is not Forge-ready merely because a document says Forge can replace
GitHub. It is Forge-ready only when the necessary object/state exists on the
Mininet object/store/sync substrate and a user can perform the workflow without
a GitHub-specific identity or API.

## Current matrix

| Workflow | GitHub/bootstrap surface today | Forge / Mininet-native representation | State after PR #334 core | Remaining cutover work |
|---|---|---|---|---|
| Publish a beta mission | PR/issue/docs | `mininet.beta/campaign/v1` (`mini-beta::BetaCampaign`) | **coded** | CLI/app creation + discovery UI + sync demo |
| Submit beta evidence | `beta-test-report.yml` | `mininet.beta/finding/v1` (`BetaFinding`) | **coded** | privacy-safe ingestion UI/CLI; attachments through Mininet media |
| Triage/disposition a finding | issue comments/labels | `mininet.beta/finding-disposition/v1` | **coded** | Forge surface to append/view dispositions and duplicates |
| Turn a finding into work | issue + work claim | existing `mini-forge::TaskBrief` + `WorkClaim` | **coded before #334** | direct finding -> task link in Forge UX/CLI |
| Claim a task | GitHub discussion + `governance/work-claims.json` | `mini-forge::WorkClaim` | **coded before #334** | make native claims the primary view; mirror to GitHub while needed |
| Technical review handoff | PR review/comments | `mini-forge::TechnicalReview` | **coded before #334** | native review UI/CLI and exact-state handoff walkthrough |
| Record accepted useful work | PR/issue closure text | `mininet.beta/contribution/v1` | **coded** | Forge acceptance surface + policy for who may record acceptance during beta |
| Give test currency | manual/bootstrap process | `mininet.beta/grant/v1` + `BetaMiniLedger` | **coded reference core** | wallet/app surface + durable replicated beta ledger/execution path |
| Reward accepted testing/code/docs/research | ad-hoc bounty discussion | contribution receipt -> participation grant | **coded evidence chain** | private one-time claim UX and durable execution; no public GitHub handle binding |
| Discover suggested tasks | GitHub issue list | `mini-forge::suggest_tasks` | **coded before #334** | app/CLI surfacing and campaign-aware filters |
| Share exact code/release | GitHub repository/releases | Forge repo/release objects + retrieval | **coded before #334** | make native retrieval routine for beta builds |
| Mirror Git <-> Forge | GitHub/git | `git_import` + `git_export` | **coded before #334** | continuous/operator-friendly bridge with conflict/evidence reporting |
| Work during GitHub outage | impossible for GitHub-only intake | store/sync + Forge objects | **partially coded** | end-to-end beta campaign -> finding -> task -> review -> contribution -> grant outage demo |
| Make Forge canonical | GitHub `main` bootstrap | one-way `forge_canonical=true` transition | **not complete** | machine state, transition proof, bootstrap authority shutdown |
| Go-Live | bootstrap decision | one-way `go_live=true` after Forge canonical | **not complete** | satisfy substantive safety gates and execute irreversible transition |

## What must be proven before `forge_canonical = true`

The following is an engineering acceptance list, not a ceremonial declaration.

- [ ] A fresh node can discover the current beta campaign without GitHub.
- [ ] A tester can submit a structured finding without a GitHub account.
- [ ] Finding payload/attachments replicate to another independently operated node.
- [ ] Triage can append a disposition without mutating or deleting the original report.
- [ ] An accepted finding can produce a Forge task brief.
- [ ] A contributor can discover and claim that task with an expiring native claim.
- [ ] Exact-state implementation/reproduction evidence can be handed to a reviewer.
- [ ] Review evidence remains distinct from merge/release/governance approval.
- [ ] Accepted useful work can produce a contribution receipt without creating a persistent contributor identity.
- [ ] A Beta MINI testing/participation grant can be authorized and applied in the matching beta epoch.
- [ ] The same grant cannot be applied twice.
- [ ] Old-epoch Beta MINI cannot cross a reset boundary.
- [ ] No balance/reward input exists in task suggestion, technical review, merge, release, personhood, or governance authority calculations.
- [ ] A GitHub outage does not stop the complete beta contribution loop.
- [ ] GitHub can be restored as a **mirror** without becoming the authority that decides which Forge state is canonical.
- [ ] Pre-Go-Live transport metadata cannot be retroactively promoted into Mininet contributor identities.
- [ ] The bootstrap custodian's canonical-integration power has a machine-enforced one-way shutdown when Forge canonicality/Go-Live activates.

## Beta MINI durability path

`mini-beta::BetaMiniLedger` is intentionally a small reference accounting core,
not a production chain. The safe progression is:

1. **reference core now** — bounded grants, duplicate prevention, transfers,
   explicit epochs, reset semantics;
2. **durable beta execution** — append-only grant/transfer events persisted and
   replicated on the beta network, still in the isolated beta domain;
3. **wallet/app surface** — users can request a testing grant, see the word
   `BETA`, transfer/spend test value, and see the current epoch/reset warning;
4. **participation claim surface** — accepted contribution receipt -> fresh
   private claim tag -> fresh beta account, without public GitHub identity
   binding;
5. **outage proof** — the whole authorization/accounting flow works with GitHub
   unavailable;
6. **retirement** — Beta MINI can be reset or ended without any production
   conversion promise;
7. **production value remains separate** — if/when production MINI activates,
   it does so only through the audited production value/custody/settlement path,
   not by upgrading the beta ledger in place.

This prevents the common testnet failure mode where a supposedly disposable
test balance becomes a political or financial promise because early holders
expect automatic recognition later.

## Contribution reward doctrine

Useful work should be easy to recognize across code and non-code routes, but the
reward mechanism must remain evidence-first:

- the artifact/evidence is what is accepted;
- the contributor may remain Mininet-anonymous before Go-Live;
- a contribution receipt is not a reputation score;
- a grant is not an appointment;
- amount does not affect review or governance weight;
- repeated contributions do not silently create an identity profile;
- AI may help inspect or draft evidence but cannot self-authorize a grant or governance result;
- production compensation, if later offered, is a separate governed mechanism and is never promised by Beta MINI.

## Exact failure point

The transition fails Mininet's purpose if GitHub remains the place where a
finding becomes "real," a task becomes official, a contribution becomes
recognized, or a reward becomes valid. In that state Forge is only a backup of
a centralized authority surface.

The long-term fix is one complete GitHub-independent contribution loop, followed
by the one-way Forge-canonical transition and removal of bootstrap canonical
control. GitHub may remain a useful mirror indefinitely; it must not remain a
required authority.
