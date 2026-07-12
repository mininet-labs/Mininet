# GitHub-to-Forge Authority Mapping

**Status:** Migration specification

## 1. Purpose

This document prevents accidental transfer of GitHub platform assumptions into Mininet's constitutional model.

## 2. Mapping table

| GitHub bootstrap concept | Forge-native concept | Migration warning |
|---|---|---|
| Account | Key lineage / optional pseudonym | GitHub account ownership is platform-dependent and not personhood |
| Pull request | ChangeProposal | PR text and head can mutate; Forge approvals bind to immutable state |
| Review comment | Advisory finding or TechnicalReview | Ordinary comments are not necessarily signed authority objects |
| Approval | TechnicalReview + Approval | GitHub conflates review UI and branch policy |
| Branch protection | Canonicalization policy | Platform admins may bypass rules; Forge policy must be independently verifiable |
| Merge commit | IntegrationResult + CanonicalizationDecision | A commit alone does not contain legitimacy proof |
| CODEOWNERS | Domain delegation registry | CODEOWNERS routes review but does not establish constitutional authority |
| Actions run | EvidenceBundle / BuildProvenance | Runner and workflow identity must be cryptographically bound |
| Release page | ReleaseDecision display | Platform publication does not equal governed release |
| Issue | Problem/Research/Bounty object | Issue mutability and deletion require archival mapping |
| Team | WorkingGroup membership/delegation | Platform membership is not governance by itself |
| Admin | Bootstrap operator | Must not become a permanent constitutional role |

## 3. Dual-running phase

During dual running:

- every canonical GitHub event SHOULD generate or reference a Forge object;
- object digests SHOULD be written back to the GitHub proposal;
- discrepancies MUST fail closed for sensitive actions;
- authority MUST be derived from the declared canonical system, not whichever UI is convenient;
- the mirror direction and recovery source MUST be explicit.

## 4. Cutover gates

Forge may become canonical only after demonstrating:

1. proposal and exact-state review;
2. delegation and revocation;
3. independent approval counting;
4. integration result production;
5. provenance-bound release decisions;
6. transparency/equivocation detection;
7. anonymous/pseudonymous contribution;
8. bounty claims and disputes;
9. backup, replication, and recovery;
10. operation during a complete GitHub outage;
11. founder/platform admin inability to rewrite accepted history unilaterally;
12. clear user-visible declaration of the source of truth.

## 5. Decommission

Disabling GitHub requires a governed decision and a tested recovery plan. The repository may remain a read-only mirror, archival transport, or external contributor gateway if that does not restore platform authority.
