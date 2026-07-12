# Mininet Legitimacy Model

**Status:** Normative  
**Purpose:** Define how an artifact becomes part of canonical Mininet history.

## Principle

Legitimacy is a property of a process and evidence chain, not of the author's identity. Anonymous work can become legitimate. Public work can be rejected. AI-produced work can become part of Mininet only through the same evidence, review, and authorization process as any other work.

## Artifact classes

This model applies to:

- code and configuration;
- protocol specifications;
- constitutional and governance text;
- cryptographic constructions;
- economic mechanisms;
- research conclusions;
- release metadata;
- security advisories and emergency changes.

## States

### 1. Draft

Private or unsubmitted work. It has no project legitimacy.

### 2. Proposed

The artifact is published as a signed or platform-authenticated Change Proposal with scope, motivation, affected components, claimed directive alignment, and conflict disclosures.

### 3. Implemented

A concrete implementation or complete text exists. Implementation alone grants no legitimacy.

### 4. Evidence Attached

The proposal carries the evidence required for its risk class: tests, proofs, threat analysis, migration plan, provenance, compatibility evidence, or external review.

### 5. Adversarially Reviewed

At least one reviewer or independent AI process has attempted to falsify the proposal's claims, break invariants, identify hidden authority, expose privacy leakage, or produce counterexamples.

AI adversarial review is evidence. It is not governance approval.

### 6. Human or Governance Reviewed

The required authorized reviewers have examined the exact final artifact. Reviews bind to an immutable digest. A changed artifact invalidates stale approvals according to policy.

Reviewers may be pseudonymous. The requirement is persistent authorization and independence, not compulsory public identity.

### 7. Integrated

The proposal has been combined with the current candidate state and all integration-level checks pass. Feature-level success is not integration success.

### 8. Governed

The applicable authority has accepted the exact integrated artifact under the required policy. For ordinary bootstrap work this may be founder plus reviewer approval; later it may be a working group or network vote.

### 9. Canonical

The governed artifact is recorded in the canonical history with its evidence, approvals, predecessor, and resulting state digest.

### 10. Released

A canonical state has produced artifacts satisfying release policy: reproducible or independently attested builds, freshness, rollback protection, transparency, required audits, and timelock.

### 11. Owner Adopted

An owner explicitly activates a specific verified release or a voluntarily selected adoption policy activates it on the owner's behalf. Adoption does not retroactively create governance legitimacy.

## Invalid transitions

The following shortcuts are forbidden:

- Draft directly to Canonical.
- AI review directly to Governed.
- Successful CI directly to Released.
- Maintainer status directly to correctness.
- Payment directly to authority.
- Release eligibility directly to forced activation.
- GitHub merge directly to constitutional legitimacy when required evidence is missing.

## Risk classes

### Class 0 — Editorial

No semantic effect. One authorized review and automated checks may suffice.

### Class 1 — Ordinary implementation

Requires tests, exact-head review, integration checks, and the current merge authority.

### Class 2 — Security or protocol-critical

Identity, consensus, forge governance, update verification, installer, privacy, storage proofs, settlement, and treasury code require independent adversarial review and stronger quorum.

### Class 3 — Cryptographic or constitutional

New cryptographic constructions, monetary semantics, frozen invariants, constitutional amendments, and human-verification mechanisms require specialist review and, where stated, external legitimacy gates before production use.

### Class 4 — Emergency

A narrowly scoped response to an active vulnerability. Emergency process may compress time but may not waive evidence preservation, retrospective review, owner consent, or constitutional constraints.

## Evidence bundle

A canonical proposal record should contain:

- proposal digest and predecessor;
- author identity mode: anonymous session, pseudonym, public identity, organization, or AI-assisted;
- exact changed-object digest;
- affected directives, invariants, decisions, and specifications;
- risk class;
- test and benchmark results;
- threat-model delta;
- AI assistance and adversarial-review records;
- human/governance approvals bound to the exact digest;
- integration result;
- compensation claim or bounty reference, if any;
- release impact and migration notes.

## Independence

Independence is evaluated by control, not labels. Two accounts controlled by one actor are one reviewer. Three build jobs on one privileged host are not three independent builders. An author's own AI agents do not satisfy independent human review.

Pseudonymity does not prevent independence; the system may use persistent roots, conflict disclosures, behavioral evidence, and governance challenges without requiring public identification.

## Rejection, withdrawal, and supersession

Rejected proposals remain part of the public reasoning record unless privacy or security requires restricted handling. Rejection does not reduce a participant's right to contribute again.

A proposal may be withdrawn by its author before canonicalization. Canonical artifacts are not deleted; they are superseded by a new legitimate artifact.

## Compensation and legitimacy

Compensation follows accepted work. Compensation does not create authority, vote weight, or guaranteed acceptance of future work. An anonymous contributor may receive a bounty once the acceptance conditions are objectively satisfied.

## Consistency requirements

- An approval binds the exact proposal digest; material modification returns the proposal to review.
- AI findings may advance evidence status but cannot advance an approval or governance-quorum state.
- Anonymous proposals may advance through technical review and compensation. Continuity-dependent authority requires a persistent key, not public legal identity.
- Canonicalization on GitHub is provisional infrastructure for the constitutional state transition; Forge replaces the mechanism, not the rule.
- Adoption is never an automatic consequence of release.
