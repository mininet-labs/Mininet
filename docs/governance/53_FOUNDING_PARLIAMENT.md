# Founding Parliament

**Status:** Proposed constitutional bootstrap mechanism — not active merely by file presence
**Scope:** Transition from Founder-guarded bootstrap to an expanding, duty-bearing Parliament and finally to public human governance

## 1. Purpose

Mininet needs a governance mechanism that can operate before mature, privacy-preserving one-human-one-vote personhood exists without turning temporary custodians into a permanent ruling class. This proposal creates a small Founding Parliament whose members carry real temporary responsibility, whose seats roll through active service, and whose machinery can grow into public governance without being replaced by a different institution.

The Parliament is a bridge, not an aristocracy. Founding status, contribution volume, MINI balance, employer, fame, invitation ancestry, or compensation MUST NOT create additional vote weight.

## 2. Relationship to current bootstrap policy

This document is a proposal. It does not by itself supersede `52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md`, activate persistent Pre-Go-Live contributor identities, change the current Founder integration role, or make Forge canonical.

Activation requires an exact-state Founder bootstrap decision while Document 52 is active, plus machine-readable activation state. Pre-Go-Live anonymous contribution history MUST remain unlinkable unless a contributor later chooses to prove a specific private claim. GitHub usernames, emails, commit authors, payment destinations, and other transport metadata MUST NOT be promoted into Parliament identity.

The first persistent Steward identities begin prospectively when the Parliament activation decision says they may. No activation may retroactively assign a DID or reputation profile to anonymous Beta work.

## 3. H0

`H0` is the historical Founding Guardian designation for Mininet's Founder. It is not a superior human class.

While the Founding Parliament is active:

- H0 holds one active parliamentary seat if H0 satisfies the same active-duty requirements as other Stewards;
- H0 has exactly one ordinary parliamentary vote;
- H0 has the elevated invitation allowance defined below;
- H0 may use only the narrowly bounded Guardian Stay in Document 54;
- H0 has no unilateral treasury, balance, release-installation, identity-unmasking, forced-update, or permanent constitutional authority.

After public governance maturity, H0 may remain a historical designation but carries no exceptional protocol authority.

## 4. Candidate invitation

The founding candidate pool is invitation-gated while public personhood governance is immature.

Invitation allowances:

- H0: at most 100 invitations per rolling 30 days;
- an active Steward: at most 10 invitations per rolling 60 days, only after at least 90 days of active Steward service and one completed duty period.

An invitation means only: `eligible to volunteer for qualification`.

It does NOT mean:

- automatic seat;
- automatic vote;
- automatic MINI;
- automatic working-group role;
- guaranteed future governance authority;
- control by the inviter.

Inviters receive no referral payment, vote delegation, percentage of an invitee's rewards, or continuing authority over an invitee.

Invitations SHOULD be privacy-preserving bearer-or-commitment capabilities that can be accepted without exposing prior Beta identity. They MUST be single-use, expiring, and consumed on successful candidacy activation.

## 5. Rolling active seats

Parliament begins with **7 active seats**.

Founding-phase seat capacity grows by the deterministic series:

`7 -> 15 -> 31 -> 63 -> 127 -> 255 -> ...`

The next capacity is `2n + 1`.

Growth is never automatic by calendar, token price, popularity, or invitation count. Expansion requires evidence that the next chamber can operate safely: enough qualified candidates, staffed committees, tested key rotation/recovery, adversarial-governance exercises, Forge operation without GitHub as a hard dependency, and an explicit invitation-capture review.

A seat is a temporary duty lease, not property.

Default seat term: **90 days**.

A Steward may serve at most **4 consecutive ordinary terms** before a rest term when qualified replacements exist. A recorded shortage may temporarily waive the rest requirement; shortage must not become a permanent excuse for incumbency.

A seat becomes vacant on expiration, resignation, death/unavailability, verified key loss without successful recovery, removal under due process, or material duty failure.

## 6. Standing committees

The Parliament maintains standing committees. Initial committees are:

1. Security & Emergency Response
2. Privacy, Identity & Personhood
3. Consensus & Protocol
4. MINI, Treasury & Human Commons
5. Networking & Storage
6. Forge, Build & Provenance
7. Release & Owner Safety
8. Applications, Accessibility & Human Experience
9. Constitution, Rights & Appeals

Every Steward has at least one primary committee. Committee membership changes workload and responsibility, never vote weight.

Committees investigate, develop evidence, prepare dossiers, record majority and minority reports, and recommend action. The plenary Parliament retains enduring political authority.

## 7. Immediate-effect authority

Only the Security & Emergency Response committee may exercise the immediate-effect class defined by this proposal.

Allowed immediate actions are deliberately defensive and reversible:

- P0/P1 warning;
- temporary official-release quarantine/recommendation withdrawal;
- critical workaround notice.

These actions may propagate immediately through Forge/Mininet warning surfaces because delay may itself cause harm.

Immediate-effect authority MUST NOT:

- seize, freeze, transfer, mint, or burn user balances;
- force an update or remotely disable an owner's software;
- rewrite canonical settlement history;
- amend the Constitution;
- change issuance or personhood rules;
- appoint or extend Parliament;
- suppress a lawful fork;
- unmask a user;
- create a kill switch.

An immediate order expires after at most **72 hours** unless the plenary Parliament ratifies the continuing action through the applicable motion class.

## 8. Plenary voting

Each active Steward has one vote.

No balance, reward, employer, contribution count, inviter, committee role, or H0 designation changes that weight.

Motion classes:

### Ordinary

- participation quorum: at least 2/3 of active seats;
- passes when YES exceeds NO among participating votes; abstentions count for quorum but not toward either side.

### Major

Cross-domain policy, important operational appointments/removals, substantial economic implementation, or similarly consequential decisions:

- participation quorum: at least 2/3 of active seats;
- YES must be a majority of **all active seats**.

### Emergency Fix

An exact-state P0/P1 remediation package:

- YES must be a majority of **all active seats**;
- the package MUST bind the vulnerability, affected state, exact fix digest, tests, residual risk, and any dissenting technical analysis;
- emergency speed does not waive owner adoption rights or technical evidence.

### Constitutional

Changes to constitutional/governance architecture or comparable foundational rules:

- YES must be at least 2/3 of all active seats;
- adversarial review and exact-state binding remain required where applicable.

### Public Transition

Transition of invitation-based authority toward mature public governance:

- YES must be at least 2/3 of all active seats;
- the evidence requirements of Document 56 must also pass;
- H0's Guardian Stay may not block this motion class.

### Guardian Stay Override

- YES must be at least 3/4 of all active seats.

## 9. Parliamentary dossiers

Enduring decisions SHOULD use a Forge-native dossier flow:

`Proposal -> Committee assignment -> Rapporteur/evidence lead -> Technical evidence -> Adversarial review -> Majority report + minority report -> Plenary debate -> exact final text/state -> vote -> canonical decision object`

A committee chair cannot suppress a minority report. Materially changing the exact proposed state invalidates stale approvals and votes.

## 10. Selection and rotation

Invitation is only the first filter. Seat allocation SHOULD combine objective qualification with a capture-resistant selection mechanism among qualified candidates. The long-term target is verifiable public selection, including a mixture of equal-human election and qualified sortition if the community validates that model.

The same inviter MUST NOT gain continuing control over invitees. Expansion evidence must explicitly measure invitation concentration and reject chambers whose effective control still collapses to one person or coordinated source.

## 11. Parliament is not a reward tier

A Steward is paid for active duty under Document 55. Council/Parliament service is intentionally not the highest-return economic path.

A contributor may decline all governance duty and continue to earn contribution, engineering, research, storage, bandwidth, or other rewards. A technically exceptional contributor may earn far more MINI than a Steward. A Steward doing technical work earns the same contribution reward the work would earn if performed by a non-Steward; there is no governance multiplier.

## 12. Public-governance destination

The institution SHOULD evolve rather than be replaced.

The same Forge machinery — terms, committees, dossiers, votes, emergency orders, duty receipts, conflict records, recalls, and public reasoning — remains useful as access opens.

What changes is the source of legitimacy:

`invited qualified candidates -> mixed invited/public candidates -> all mature verified humans eligible under equal rules`

Public eligibility may only increase. It may never be reduced to restore insider control.

At full public transition:

- invitations cease to be a prerequisite for political eligibility;
- every mature verified human has equal public governance eligibility/ballot weight under the then-canonical public model;
- H0 exceptional Guardian Stay authority irreversibly ends;
- Founding Steward history remains historical evidence, not permanent political privilege.

## 13. Anti-capture invariants

Any implementation of this proposal MUST prove:

- one active Steward seat counts once;
- H0 ordinary vote counts once;
- invitation does not directly create a vote;
- inviter ancestry creates no vote multiplier;
- money/reward/balance creates no vote multiplier;
- duty compensation is independent of vote direction;
- emergency committees cannot modify balances or force adoption;
- immediate orders expire;
- H0 cannot veto the H0 sunset/public transition;
- public eligibility is monotonic;
- public activation disables exceptional H0 authority one-way;
- anonymous Pre-Go-Live contribution history is not retroactively linked.

## 14. Exact failure point

This mechanism FAILS Mininet's values if invitation ancestry becomes permanent political access, if H0 can perpetually block public transition, if inactive officeholders are paid merely for possessing a title, if committee emergency power can alter ownership or force software, or if the Founding Parliament can indefinitely refuse evidence-complete public maturation merely to preserve its own authority.

The long-term solution is the same machine and institutional workflow with universal mature-human access, rotating active duty, equal political voice, and no exceptional Founder authority.
