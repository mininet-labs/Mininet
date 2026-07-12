# Governance v1 Conformance Standard

**Status:** Normative

**Version:** 1.0

## Normative interpretation

This document is subordinate to `docs/FOUNDER_DIRECTIVES.md`, frozen invariants, and accepted decisions in the canonical Mininet repository. Where this package differs from canonical project authority, the canonical repository prevails until a governed amendment explicitly adopts the new rule.

Identity disclosure is never presumed. Anonymous, pseudonymous, and public participation remain valid unless a narrowly scoped role requires continuity or delegated authority. Persistent cryptographic accountability does not imply legal-name disclosure.

## Meaning of v1.0

Version 1.0 means the governance model is constitutionally complete enough to operate without the Founder or GitHub. It does not mean every mechanism is implemented, audited, activated, or socially mature.

A deployment MUST distinguish:

- **Specified:** the rule is defined;
- **Implemented:** code or operational controls enforce it;
- **Verified:** tests or review support the enforcement claim;
- **Activated:** canonical governance has enabled it;
- **Mature:** it has survived real use and incident review.

## Conformance profiles

### Bootstrap Profile

Founder remains the final GitHub guardian. Required:

- protected canonical branch;
- review and CI gates;
- explicit AI assistance metadata;
- no forced update path;
- traceable decisions and exceptions;
- contributor privacy choices.

### Hybrid Profile

GitHub and Forge operate together. Required:

- object-level mapping between both systems;
- no silent divergence;
- exact-state approvals in both representations;
- documented authority precedence;
- reproducible recovery from either side.

### Forge-Primary Profile

Forge is canonical and GitHub is a mirror. Required:

- independent operators;
- proposal, review, merge, release, delegation, and revocation objects;
- identity/key continuity;
- governance finality and transparency;
- public export and fork capability;
- tested GitHub outage continuity.

### Protocol-Sovereign Profile

No founder or platform is required. Required:

- constitutional amendment process;
- working-group succession;
- independent release and build authority;
- treasury and governance separation;
- privacy-preserving contribution and payout paths;
- tested founder disappearance;
- tested infrastructure loss;
- owner-controlled adoption.

## Prohibited conformance claims

A project MUST NOT claim Forge-primary or protocol-sovereign conformance merely because:

- objects can be stored without GitHub;
- one CLI demo succeeds;
- one founder can recover the system;
- AI produced reviews;
- a governance document exists;
- tests pass without independent operators;
- anonymous signatures are mistaken for unlinkability.

## Evidence bundle

A conformance claim MUST cite exact versions, configuration, canonical object IDs, test results, unresolved exceptions, responsible authorities, and expiry or reassessment date.
