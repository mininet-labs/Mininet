# RFC-0001: Mininet Protocol Governance

**Status:** Draft for bootstrap adoption  
**Category:** Governance protocol

## Abstract

This RFC defines a platform-independent protocol by which proposals, evidence, reviews, merge decisions, releases, compensation claims, and constitutional amendments may be represented and legitimized. It is intended to operate first through GitHub-backed procedures and later through Mininet Forge signed objects.

## Goals

- permit anonymous, pseudonymous, and public contribution;
- keep AI useful but non-sovereign;
- bind review and approval to immutable artifacts;
- scale from founder bootstrap to verified-human governance;
- separate compensation from political power;
- preserve owner-controlled adoption;
- survive loss of GitHub and current maintainers.

## Non-goals

- proving human uniqueness in this RFC;
- defining monetary policy;
- forcing legal identity;
- replacing technical specifications;
- granting AI governance rights;
- making every development artifact public when security requires confidentiality.

## Core objects

### ChangeProposal

Contains proposal ID, predecessor/canonical base, proposed state digest, author key, identity mode declaration, scope, risk class, affected directives/invariants, compensation reference, and signature.

### EvidenceRecord

Contains evidence type, subject proposal and exact digest, producer identity or AI role, reproducibility information, result, limitations, and signature/attestation.

### ReviewRecord

Contains reviewer authority, exact reviewed digest, decision, findings, conflicts, independence declaration, and signature.

### HumanResponsibilityAcceptance

Binds a persistent human or governance identity to the exact final proposal and states the accepted scope. This identity may be pseudonymous.

### MergeDecision

Contains proposal, final integrated digest, policy ID, qualifying approvals, canonical predecessor, resulting canonical head, and authorization.

### BuildAttestation

Binds canonical source, pipeline, runner, capabilities, environment, outputs, and builder identity.

### ReleaseDecision

Binds canonical source and artifact set to release policy, signer threshold, freshness, rollback sequence, transparency checkpoint, and limitations.

### BountyClaim

Binds accepted work to a claim commitment or privacy-preserving payment destination without requiring legal identity.

## Policy evaluation

Policy evaluation is deterministic over signed objects. It verifies:

- exact-digest binding;
- reviewer independence and authority;
- risk-specific quorum;
- required evidence and external gates;
- invariant coverage;
- AI non-quorum rule;
- compensation/governance separation;
- canonical predecessor continuity.

## Bootstrap authority

During Phase 0, founder authorization may satisfy the final merge decision only when all non-waivable evidence and invariant checks pass. Founder authorization is explicitly marked bootstrap-only.

## Governance transition

A phase-transition decision identifies the old policy, new policy, evidence that exit criteria are satisfied, activation checkpoint, recovery mechanism, and rollback conditions. No transition is inferred from time or popularity.

## Privacy

Object formats should avoid legal names, IP addresses, device fingerprints, and unrelated metadata. Persistent pseudonyms are keys or lineages, not mandatory public profiles. Selective disclosure may prove role eligibility without revealing unnecessary identity attributes.

## Security considerations

Threats include sockpuppet reviewers, compromised keys, AI credential escalation, stale approvals, build-provenance forgery, GitHub account seizure, bounty double claims, governance capture, reviewer bribery, and metadata deanonymization.

Mitigations rely on scoped keys, exact-digest binding, threshold authorization, independent builders, key rotation, privacy-preserving uniqueness mechanisms, public reasoning records, optional relays, and the permanent right to fork and refuse adoption.

## Adoption

Adopting this RFC does not make the described Forge object implementation complete. It establishes the target governance semantics and requires current GitHub procedures to approximate them honestly until native enforcement exists.

## Conformance language

Implementations claiming conformance MUST distinguish proposal, evidence, review, approval, canonicalization, release, and adoption as separate state transitions. They MUST NOT count AI output as human quorum, MUST NOT require public legal identity for ordinary contribution, MUST bind approvals to exact content, and MUST preserve explicit owner choice over activation.
