# v0.5–v0.6 Implementation Backlog

**Status:** Non-binding engineering plan derived from normative specifications

## P0 — Object correctness

- canonical serialization and domain-separated identifiers;
- common signed governance envelope;
- exact-head proposal/review/approval validation;
- delegation expiry and revocation;
- duplicate-lineage exclusion;
- equivocation preservation;
- integration-result object;
- policy-version binding.

## P0 — Compensation safety

- immutable funded bounty object;
- claim commitment and nullifier;
- acceptance attestation;
- payout destination separation;
- payment authorization threshold;
- double-payment prevention;
- challenge window;
- scoped dispute freeze.

## P1 — Privacy

- unlinkable or selectively linked claim proof design;
- metadata-minimizing submission transport;
- stealth/private settlement integration only after external cryptographic review;
- privacy threat model and test vectors;
- no legal-identity field in base contribution objects.

## P1 — Working-group authority

- signed charter object;
- reviewer and maintainer delegation objects;
- automatic expiry;
- suspension/revocation objects;
- group lifecycle state machine;
- integration-representative delegation;
- machine-readable authority graph.

## P1 — Cross-group integration

- affected-domain calculation;
- combined integration evidence;
- domain quorum rules;
- orphan-interface detection;
- deadlock/appeal object;
- release-readiness report.

## P2 — Reputation

- multidimensional evidence ledger;
- key-lineage continuity;
- selective disclosure;
- appealable negative findings;
- protection against one lineage counting as several independent reviewers.

## P2 — Migration

- GitHub event to Forge-object bridge;
- bidirectional object links;
- dual-running consistency checker;
- GitHub outage exercise;
- canonical-source indicator;
- mirror reconstruction test.

## Required adversarial tests

1. author attempts self-approval through rotated child keys;
2. AI-generated review is counted as human quorum;
3. revoked maintainer signs after revocation;
4. sponsor changes bounty criteria after submission;
5. same claim is paid twice;
6. treasury redirects destination;
7. reviewer and claimant are secretly one lineage;
8. one employer captures a working-group quorum;
9. group disappears with open security obligations;
10. two groups approve incompatible interface assumptions;
11. GitHub and Forge show different canonical heads;
12. founder/admin attempts unilateral canonical rewrite after cutover.

## Completion definition

v0.5 implementation is complete only when Forge objects can carry the full proposal-to-compensation chain without GitHub authority.

v0.6 implementation is complete only when at least two working groups can delegate, rotate, integrate, dispute, and recover authority without founder intervention in ordinary operations.
