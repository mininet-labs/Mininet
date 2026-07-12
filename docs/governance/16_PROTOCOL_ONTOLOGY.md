# Mininet Governance Protocol Ontology

**Status:** Normative definitions  
**Version:** 1.1

## 1. Rule of interpretation

Capitalized reserved terms in governance specifications have the meanings below. A document MAY introduce a narrower subtype but MUST NOT silently broaden or contradict the base term.

## 2. Actors

### Participant
Any human, organization, pseudonym, anonymous session, or automated agent interacting with Mininet.

### Contributor
A Participant that submits an Artifact, Proposal, Review, Evidence item, or Claim. Contributor status grants no authority by itself.

### Anonymous Contributor
A Contributor whose action is not required to link to a persistent identity. Anonymous Contributors MAY submit work and receive privacy-preserving compensation. They cannot accumulate continuity-dependent authority without adopting a persistent cryptographic identity.

### Pseudonymous Identity
A persistent cryptographic identity that does not require disclosure of legal identity. It MAY accumulate reputation, delegation, governance history, and compensation history.

### Public Identity
A Pseudonymous Identity voluntarily associated with a public person or organization. Public status grants no correctness privilege.

### AI Agent
An automated system producing Artifacts, Proposals, Evidence, Reviews, or operational actions under a defined scope. An AI Agent does not satisfy human or governance quorum unless a future constitutional amendment explicitly creates such authority.

### Primary AI Engineer
A temporary, session-scoped subtype of AI Agent responsible for coordinating engineering preparation within an authorized task after activation of an exact standing charter. “Primary” grants no Authority, continuity right, review independence, Approval, quorum, Canonicalization, release, administration, treasury, secret, emergency power, or Owner Adoption. The role expires with its task or session and cannot inherit Founder or governance Authority.

### Owner
The sovereign controller of a local device, installation, identity, or voluntary policy. The Owner alone controls local Adoption except where control was explicitly and revocably delegated.

### Maintainer
A persistent cryptographic identity delegated bounded review, integration, or operational authority for a defined Scope.

### Governor
An identity authorized by the current governance mechanism to participate in a defined Decision. Governor does not imply public legal identity.

### Founder / Bootstrap Guardian
The temporary role protecting continuity and legitimacy before replacement governance is demonstrably operational. It is a role, not property ownership of Mininet.

### Working Group
A governed domain body with a Charter, Scope, delegated Authorities, membership process, succession rule, and conflict procedure.

## 3. Artifacts and evidence

### Artifact
A content-identifiable unit of code, specification, documentation, research, data, configuration, build output, or governance record.

### Proposal
A content-bound request to alter canonical code, policy, specification, authority, treasury state, or other governed state.

### Exact Proposal State
The immutable digest or commit evaluated by a Review or Approval. Any material change creates a new Exact Proposal State.

### Evidence
Verifiable support for a Claim, including tests, proofs, benchmarks, provenance, threat analysis, independent replication, review records, and external audit reports.

### Claim
A statement asserted about an Artifact, Actor, process, or result. Claims SHOULD identify their supporting Evidence and confidence limits.

### Review
An evaluation of an Exact Proposal State. Review MAY be human or automated and MAY produce findings, objections, suggested changes, or evidence.

### Adversarial Review
A Review deliberately attempting to falsify Claims, violate invariants, exploit assumptions, or identify integration and recovery failures.

### Approval
An authorized acceptance of an Exact Proposal State for a specified purpose and Scope. Approval is distinct from Review.

### Responsibility Acceptance
A durable statement by an authorized persistent identity that it reviewed an Exact Proposal State and accepts the defined governance responsibility. It does not require public legal identity.

### Provenance
Authenticated evidence describing how an Artifact was produced, from which inputs, by which execution environment, under which capabilities, and with which outputs.

### Audit
An independent, scoped examination against declared criteria. Passing internal tests is not an external audit.

## 4. Authority and legitimacy

### Authority
Scoped permission to perform an Action. Authority is not correctness, reputation, identity, or legitimacy.

### Scope
The bounded domain, object class, branch, working group, duration, or action set to which Authority applies.

### Delegation
A cryptographically or operationally durable grant of Authority from an authorized source to a recipient within a Scope and time boundary.

### Revocation
An authorized termination or reduction of Delegation or Authority.

### Quorum
The minimum set of distinct eligible Authorities required for a Decision. Quorum rules MUST define identity deduplication, independence, scope, and conflict-of-interest handling.

### Decision
An authorized state transition outcome, including accept, reject, defer, revoke, release, compensate, or amend.

### Legitimacy
The constitutional property by which a Decision or state transition is accepted as part of Mininet's continuous canonical evolution.

### Canonicalization
The authorized transition by which an accepted state becomes part of Canonical History.

### Canonical History
The continuous, verifiable record of legitimate Mininet evolution. A repository may mirror it but does not own it.

### Continuity
The demonstrable relationship between a new canonical state and its legitimate predecessor, including authorized succession and preserved evidence.

### Fork
A technically free continuation from copied code or history. A Fork does not automatically inherit Canonical Legitimacy, identity continuity, treasury, release authority, or community recognition.

### Exit
A Participant's ability to cease participation, withdraw delegation, stop subscriptions, reject releases, export controlled data where technically possible, or continue through a Fork.

## 5. Development and integration

### Implementation
An Artifact intended to satisfy a Proposal or Specification. Implementation alone does not confer legitimacy.

### Integration
The combination of multiple accepted or candidate Artifacts followed by evaluation of the combined state.

### Integration Candidate
A combined state proposed for canonicalization after component-level work has passed its own gates.

### Canonical Branch
The current platform representation of Canonical History. During bootstrap this is protected `main`; later it is a governed Forge head.

### Merge
A platform-specific operation combining histories. Merge is not automatically Canonicalization.

### Working Branch
A non-canonical line of development used to prepare or integrate proposals.

## 6. Release and adoption

### Build
A process transforming source and declared inputs into outputs.

### Trusted Build
A Build satisfying the applicable isolation, reproducibility, provenance, and policy requirements. Native or shell execution MUST NOT be described as trusted merely because it completed successfully.

### Release Candidate
An identified set of Artifacts proposed for release verification.

### Governed Release
A Release Candidate that satisfies applicable source, review, build, provenance, transparency, freshness, and governance gates.

### Availability
The state in which a release, service, policy, or content item can be discovered or retrieved. Availability is not activation.

### Owner Approval
Explicit local authorization naming the exact release or policy to activate.

### Adoption
The local activation of an offered release or policy after applicable re-verification and Owner Approval.

### Refusal
A valid outcome in which an Owner does not adopt an available release or optional policy.

### Rollback
A transition from an attempted or active state to a previously valid owner-approved state under defined recovery rules.

## 7. Compensation

### Bounty
A governed offer of compensation for satisfying defined acceptance criteria.

### Claimant
A Contributor asserting eligibility for a Bounty. A Claimant MAY be anonymous or pseudonymous where the payment mechanism permits.

### Acceptance Criteria
The objective or governed conditions a contribution must satisfy to qualify for compensation.

### Compensation Decision
An authorized determination that a Claim satisfies, partially satisfies, or fails Acceptance Criteria.

### Payment Address
A destination capable of receiving compensation without requiring the protocol to know a legal identity.

### Dispute
A governed challenge to acceptance, attribution, payment, authority, or process.

## 8. Political equality and identity honesty

### Identity Root
A verified `did:mini` root. It MUST NOT be described as proof of one unique human.

### Personhood Evidence
Evidence accepted under then-current policy for treating one Participant as one unique human for a defined purpose. This remains an unresolved mechanism until genuinely implemented and examined.

### Political Voice
Eligibility or weight in governance decisions. Money, storage, hardware, and early arrival MUST NOT silently become political voice.

## 9. Reserved relation statements

The following statements are normative:

- Identity is not Authority.
- Authority is not Legitimacy.
- Reputation is not Quorum.
- Review is not Approval.
- Merge is not Canonicalization.
- Release is not Adoption.
- Availability is not Consent.
- Tests are not External Audit.
- Identity Root is not Verified Human.
- Payment is not Political Voice.
- Engineering Stewardship is not Authority.
- Session Adapter is not Delegation.
- AI Coordination is not Independent Review.
