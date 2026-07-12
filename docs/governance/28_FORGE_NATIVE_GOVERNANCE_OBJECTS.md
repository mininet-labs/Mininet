# Forge-Native Governance Objects

**Status:** Normative design specification

## 1. Purpose

This document maps Mininet's governance lifecycle onto signed, content-addressed objects that do not depend on GitHub accounts, pull requests, platform administrators, or legal identity.

The central rule is:

> A mutable web page may display governance, but only immutable signed objects may constitute governance evidence.

## 2. Common envelope

Every governance object MUST be encoded canonically and placed inside a common envelope containing:

- `object_type` and version;
- `network_id` and project identifier;
- payload digest;
- author or authority key reference;
- key-event-log position or equivalent lineage proof;
- creation time or logical sequence where available;
- signature suite and signature;
- optional privacy-preserving authorization proof;
- optional supersedes/revokes references.

Object identifiers MUST be derived from canonical bytes. Display metadata MUST NOT alter the identifier.

## 3. Core object families

### 3.1 ChangeProposal

A request to modify an immutable target state.

Required fields:

- repository/project identifier;
- base canonical head;
- proposed head;
- author pseudonym or anonymous submission key;
- description digest;
- affected domains and claimed risk class;
- referenced issues, directives, invariants, and decisions;
- evidence plan;
- compensation claim commitment, if any.

A proposal MUST NOT imply acceptance, authority, or payment eligibility.

### 3.2 EvidenceBundle

Binds evidence to an exact proposal state.

Evidence MAY include:

- tests and test logs;
- reproducible build attestations;
- benchmarks;
- model checking or proofs;
- threat analysis;
- interoperability results;
- external audit reports;
- AI-generated findings;
- human findings.

Each item MUST identify its producer, method, inputs, and limitations. An AI-produced item is evidence, not approval.

### 3.3 TechnicalReview

A signed evaluation of one exact proposal digest.

Required fields:

- proposal digest;
- reviewed head;
- reviewer authority reference or advisory classification;
- decision: approve, request changes, reject, abstain;
- findings and severity;
- evidence inspected;
- conflicts disclosed or privacy-preserving no-conflict proof;
- expiry or invalidation rule.

Any change to the reviewed head invalidates approval unless the review explicitly covers a deterministic transformation.

### 3.4 HumanResponsibilityAcceptance

Where policy requires a human or persistent accountable pseudonym to accept responsibility for AI-assisted work, this object binds that acceptance to the exact proposal state and declared scope.

It does not require legal identity.

### 3.5 Approval

An authorization object distinct from technical review.

An approval MUST reference:

- the exact proposal and head;
- the authority delegation used;
- domain and risk scope;
- policy version;
- expiry or one-use semantics;
- any conditions.

Possessing a key does not make an approval legitimate unless the referenced delegation was valid at decision time.

### 3.6 IntegrationResult

Records the deterministic result of combining accepted proposals against a named base.

It MUST bind:

- all included proposal heads;
- conflict resolutions;
- combined tests and evidence;
- resulting tree/commit digest;
- integration actor or automation;
- deviations from individual proposal evidence.

Feature-level success does not substitute for integration-level evidence.

### 3.7 CanonicalizationDecision

The governance act that advances canonical history.

It MUST include:

- previous canonical head;
- new canonical head;
- policy and quorum proof;
- approvals counted and exclusions applied;
- timelock/cooling-off data where required;
- dissent or minority report references;
- rollback/revocation conditions.

Canonicalization MUST be monotonic within one history lineage. Forks create a new lineage rather than rewriting signed history.

### 3.8 ReleaseProposal and ReleaseDecision

These objects bind canonical source, build provenance, artifact digests, release policy, timelock, transparency checkpoint, rollback sequence, and governance authorization.

A release decision makes software eligible for voluntary adoption. It MUST NOT create remote execution authority.

### 3.9 Delegation and Revocation

Delegations MUST specify:

- delegator authority;
- recipient key lineage;
- permitted object types/actions;
- domain and repository scope;
- maximum risk class;
- start, expiry, and revocation conditions;
- whether subdelegation is allowed.

Revocation MUST be independently discoverable and effective according to a deterministic sequence rule.

### 3.10 Bounty and Compensation objects

Defined in `29_ANONYMOUS_BOUNTY_LIFECYCLE.md` and `30_COMPENSATION_PRIVACY_AND_SETTLEMENT.md`.

## 4. Object separation rules

The following MUST remain separate:

- authorship and review;
- review and approval;
- approval and canonicalization;
- canonicalization and release;
- release and owner adoption;
- acceptance and compensation;
- compensation and governance authority;
- reputation and personhood;
- identity continuity and public identity.

Combining these distinctions into one "trusted maintainer" flag would violate Mininet's least-authority direction.

## 5. Privacy

Objects SHOULD reveal only what is needed to verify the action. A proposal author MAY use a fresh key. Review and governance roles MAY require persistent pseudonymous continuity. Selective-disclosure proofs MAY establish role eligibility without revealing unrelated identity data.

Network metadata privacy is not solved merely by pseudonymous object signatures. Forge transport SHOULD support relays, delayed publication, local-first creation, and metadata minimization.

## 6. Replay, equivocation, and stale authority

Verification MUST reject:

- cross-network replay;
- cross-project replay;
- approvals for a different head;
- expired or revoked delegations;
- duplicate approval counting from one root/lineage where independence is required;
- conflicting canonicalization decisions by the same authority set at the same sequence;
- superseded policy versions when the newer policy is mandatory.

Conflicting signed objects MUST be preserved as evidence rather than silently discarded.

## 7. GitHub bootstrap mapping

During bootstrap:

- a pull request approximates `ChangeProposal`;
- CI artifacts approximate `EvidenceBundle`;
- reviews approximate `TechnicalReview` and sometimes `Approval`;
- a protected merge approximates `CanonicalizationDecision`;
- GitHub Actions and release pages approximate transport/display layers.

This mapping is imperfect because GitHub records are platform-controlled and not all actions are independently signed. The migration plan MUST close those gaps before Forge becomes authoritative.

## 8. Acceptance tests

A conforming implementation MUST demonstrate:

1. exact-head review invalidation after mutation;
2. delegation expiry and revocation;
3. AI evidence excluded from human/governance quorum;
4. duplicate root/lineage approvals not double-counted;
5. cross-network and cross-project replay rejection;
6. preservation of equivocation evidence;
7. canonicalization from combined integration evidence;
8. release eligibility without forced adoption;
9. anonymous proposal acceptance without legal identity;
10. compensation without authority escalation.
