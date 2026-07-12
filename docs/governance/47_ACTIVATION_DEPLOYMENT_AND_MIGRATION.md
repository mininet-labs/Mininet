# Governance Activation, Deployment, and Migration

**Status:** Normative operational specification

**Version:** 1.0

## Normative interpretation

This document is subordinate to `docs/FOUNDER_DIRECTIVES.md`, frozen invariants, and accepted decisions in the canonical Mininet repository. Where this package differs from canonical project authority, the canonical repository prevails until a governed amendment explicitly adopts the new rule.

Identity disclosure is never presumed. Anonymous, pseudonymous, and public participation remain valid unless a narrowly scoped role requires continuity or delegated authority. Persistent cryptographic accountability does not imply legal-name disclosure.

## Activation is separate from publication

Publishing v1.0 documentation does not activate all rules. Activation requires a canonical decision identifying:

- exact governance package digest;
- conformance profile;
- effective time or event;
- existing exceptions;
- responsible bootstrap authorities;
- migration steps;
- rollback or suspension conditions.

## Deployment phases

### Phase A — Documentation alignment

Repository guidance references one hierarchy and vocabulary. Contradictory legacy rules are removed or explicitly superseded.

### Phase B — Observe-only enforcement

Governance CI reports missing metadata, review routes, expired exceptions, and invalid summaries without blocking ordinary development.

### Phase C — Bootstrap blocking

Canonical branch protection, required CI, exact-state review, exception expiry, and sensitive-domain routing become blocking.

### Phase D — Hybrid Forge

GitHub and Forge object histories are dual-recorded. Divergence alarms block canonicalization.

### Phase E — Forge-primary

Forge decisions become canonical; GitHub receives verified mirrors.

### Phase F — Protocol-sovereign

Founder and platform dependencies are removed after conformance evidence and a governed transition.

## Migration safety

No phase transition may erase the ability to:

- reconstruct prior canonical history;
- identify the authority that approved the transition;
- export data;
- reject a release;
- fork from a known state;
- inspect unresolved exceptions.

## Rollback

Operational deployment MAY roll back when enforcement causes unexpected failure. Constitutional history MUST record the rollback; it must not be silently rewritten.
