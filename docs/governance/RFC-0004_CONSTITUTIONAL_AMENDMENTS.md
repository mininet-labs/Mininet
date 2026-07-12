# RFC-0004: Constitutional Amendments

**Status:** Proposed normative RFC

**Version:** 1.0

## Normative interpretation

This document is subordinate to `docs/FOUNDER_DIRECTIVES.md`, frozen invariants, and accepted decisions in the canonical Mininet repository. Where this package differs from canonical project authority, the canonical repository prevails until a governed amendment explicitly adopts the new rule.

Identity disclosure is never presumed. Anonymous, pseudonymous, and public participation remain valid unless a narrowly scoped role requires continuity or delegated authority. Persistent cryptographic accountability does not imply legal-name disclosure.

## Abstract

This RFC defines content-addressed objects and validation rules for constitutional amendments.

## Objects

`AmendmentProposal` binds parent digest, proposed digest, classification, impact map, evidence set, migration plan, activation rule, and author signature.

`AmendmentReview` binds proposal digest, reviewer identity, review class, findings, conflicts, and signature.

`AmendmentDecision` binds the exact proposal, eligible authority set, approvals, rejections, threshold rule, decision time, and governance finality reference.

`AmendmentActivation` binds the decision, cooling-period proof, activation condition, resulting constitution digest, and migration checkpoint.

## Validation

A validator MUST reject:

- approval of a different proposal digest;
- activation before the cooling period;
- classification below the proposal's actual effect;
- missing impact analysis for frozen invariants;
- emergency activation of a constitutional change;
- authority counted outside its scope or validity period;
- AI advisory records counted as human or governance authorization.

## Privacy

Reviewers and proposers MAY use pseudonymous identities. Constitutional authority requires persistent authorization, not legal-name disclosure.
