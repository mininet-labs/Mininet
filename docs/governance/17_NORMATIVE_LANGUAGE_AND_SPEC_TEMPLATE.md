# Normative Language and Governance Specification Template

**Status:** Normative drafting standard  
**Version:** 0.3

## 1. Normative terms

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** indicate requirement strength.

- **MUST / SHALL** — required for conformance.
- **MUST NOT / SHALL NOT** — prohibited for conformance.
- **SHOULD / RECOMMENDED** — expected unless a documented, reviewable reason justifies deviation.
- **SHOULD NOT** — normally prohibited unless a documented exception is justified.
- **MAY / OPTIONAL** — permitted but not required.

A document MUST identify whether it is constitutional, normative, operational, explanatory, draft, or historical.

## 2. Mininet reserved status terms

- **Verified** — checked against explicit criteria with recorded Evidence.
- **Authorized** — performed by an Actor holding the required Authority and Scope.
- **Legitimate** — accepted through the applicable constitutional process.
- **Canonical** — entered into Canonical History.
- **Governed** — accepted through the applicable governance Decision process.
- **Available** — retrievable or selectable, but not necessarily activated.
- **Adopted** — locally activated through Owner Approval.
- **Experimental** — implemented or investigated without production-grade assurance.
- **Externally Audited** — examined by an independent qualified party against a declared scope.

These terms MUST NOT be used as marketing synonyms. Their stated criteria must be demonstrable.

## 3. Governance specification template

Every new normative governance specification SHOULD use the following structure.

### Identifier and status

- Document ID
- Version
- Status
- Authority class
- Authors or proposing identities
- Supersedes / superseded by

### Purpose

What problem or state transition this specification governs.

### Scope

What is included and explicitly excluded.

### Constitutional traceability

List relevant constitutional clauses, frozen invariants, and accepted decisions.

### Definitions

Reference the Protocol Ontology and define only narrower local terms.

### Actors

List each Actor and whether it may be anonymous, pseudonymous, public, organizational, or automated.

### Authorities

For every Action, identify required Authority, Scope, quorum, independence, and conflict rules.

### Inputs

Identify all required content-bound objects and external state.

### Preconditions

Conditions that MUST hold before the transition is attempted.

### State transition

Define source state, action, destination state, and atomicity requirements.

### Required evidence

Define the minimum Evidence and how it binds to the Exact Proposal State.

### Failure conditions

List invalid, rejected, expired, conflicted, unavailable, and unsafe conditions.

### Recovery and rollback

Define how partial failure is detected and how a safe state is restored.

### Audit events

Define durable records produced by success and failure.

### Privacy effects

State what information is revealed, to whom, for how long, and whether disclosure is optional.

### Compensation effects

State whether the transition creates, modifies, or settles a Bounty or payment.

### Implementation mappings

Describe current GitHub bootstrap and target Forge implementation separately.

### Test scenarios

List positive, negative, adversarial, continuity, and recovery tests.

### Machine-readable summary

Provide or reference a structured summary following `22_MACHINE_READABLE_SUMMARIES.md`.

## 4. Exception rule

A deviation from a SHOULD requirement MUST record:

- the requirement;
- the reason;
- the approving Authority;
- affected Scope;
- start and expiry time;
- risks introduced;
- removal plan.

A MUST requirement cannot be waived by an operational exception unless the governing higher-level document explicitly defines a waiver process.

## 5. Honest-status rule

Documentation MUST distinguish:

- designed;
- specified;
- implemented;
- tested;
- independently reproduced;
- externally audited;
- production approved.

No lower state may be presented as a higher state.
