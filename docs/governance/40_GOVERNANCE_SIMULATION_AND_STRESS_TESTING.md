# Governance Simulation and Stress Testing

**Status:** Normative testing specification

**Version:** 1.0

## Normative interpretation

This document is subordinate to `docs/FOUNDER_DIRECTIVES.md`, frozen invariants, and accepted decisions in the canonical Mininet repository. Where this package differs from canonical project authority, the canonical repository prevails until a governed amendment explicitly adopts the new rule.

Identity disclosure is never presumed. Anonymous, pseudonymous, and public participation remain valid unless a narrowly scoped role requires continuity or delegated authority. Persistent cryptographic accountability does not imply legal-name disclosure.

## Objective

Governance claims MUST be tested as system claims. Passing software unit tests does not prove resistance to capture, disappearance, coercion, inactivity, identity churn, or infrastructure loss.

## Simulation model

A simulation SHOULD model:

- contributors with anonymous, persistent-pseudonymous, public, organizational, and AI identities;
- independent and correlated operators;
- working groups and cross-group dependencies;
- proposal, review, approval, bounty, release, adoption, fork, and amendment state machines;
- communication delay, censorship, equivocation, key loss, compromise, and disappearance;
- treasury scarcity and funding concentration;
- GitHub, Forge, network, and builder outages.

## Mandatory scenarios before v1 governance activation

1. Founder becomes unavailable without warning.
2. GitHub deletes or freezes the organization.
3. Forge becomes unavailable while GitHub remains accessible.
4. Both Forge and GitHub are unavailable, but peers retain canonical objects.
5. A majority of one working group is funded by one employer.
6. AI-generated changes overwhelm human review capacity.
7. Two maintainers collude to approve a malicious exact-state proposal.
8. A reviewer key is compromised after an approval was issued.
9. A bounty contributor proves acceptance but wishes to keep payout unlinkable.
10. Treasury payment is disputed after technical acceptance.
11. A constitutional proposal gains a narrow temporary majority.
12. A release is governed but a reproducibility quorum later equivocates.
13. A device owner remains offline beyond metadata freshness limits.
14. A minority forks after losing a governance decision.
15. A personhood mechanism falsely merges or duplicates human identities.

## Pass conditions

A scenario passes only if:

- canonical state remains derivable or a deterministic recovery path exists;
- unauthorized authority does not silently become legitimate;
- owners are not forced to adopt software;
- privacy claims are not strengthened beyond observed evidence;
- minority participants retain exit and fork rights;
- compensation disputes do not rewrite technical history;
- temporary measures expire or are explicitly renewed;
- the system records uncertainty rather than fabricating consensus.

## Governance test artifacts

Every simulation run SHOULD emit:

- scenario identifier and version;
- seed and actor model;
- initial authority graph;
- ordered events;
- decisions and rejected transitions;
- resulting canonical heads;
- privacy leakage observations;
- invariant outcomes;
- unresolved ambiguity.

Simulation results are evidence, not constitutional authority.
