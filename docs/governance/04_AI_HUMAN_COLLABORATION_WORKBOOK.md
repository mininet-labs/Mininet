# AI and Human Collaboration Workbook

**Status:** Normative operating model
**Version:** 1.1

## Core rule

AI may generate work and evidence. Humans or legitimate governance authorize canonical history. Anonymous or pseudonymous humans may perform that role where policy permits; public legal identity is not required by default.

## Standing engineering coordinator

When `GOV-AI-050` has been validly activated for the current bootstrap phase, the Primary AI Engineer coordinates the engineering-preparation lifecycle across the roles below. “Primary” identifies responsibility for producing a coherent proposal; it grants no review independence, Approval, quorum, Canonicalization, release, administrative, treasury, secret, or Owner Adoption Authority.

The standing coordinator may combine AI Author, Adversary, Simplifier, and Integration Reviewer outputs. Each output retains its role and adverse findings. Coordination does not convert several AI outputs into an independent human Review or governance Decision.

Before activation, Document 50 and its session adapter remain proposals and this workbook continues to operate without that named coordination role.

## Standard roles

### AI Author

Produces an implementation, specification, test plan, or analysis. Must disclose assumptions, unresolved questions, and generated files.

### AI Adversary

Attempts to break the author's work. It seeks invariant violations, mathematical counterexamples, parser failures, race conditions, hidden authority, privacy leakage, downgrade paths, and unsupported claims.

### AI Simplifier

Looks for unnecessary code, dependencies, protocol states, and configuration. It checks Directive 14 without weakening security.

### AI Integration Reviewer

Compares the proposal against adjacent crates, APIs, specifications, and current canonical state. It searches for contracts that compile separately but fail together.

### Human Technical Reviewer

Evaluates the exact final digest, checks evidence, resolves disagreements, and accepts review responsibility under a persistent authorized identity. The identity may be pseudonymous.

### Human/Governance Legitimizer

Applies the applicable policy and authorizes canonicalization. This role must be independent of payment and cannot be filled by AI alone.

## Workbook for every material proposal

### Section A — Proposal

- Problem being solved:
- Why it belongs in Mininet:
- Applicable directives and invariants:
- What authority, data, dependency, or complexity is added:
- What simpler alternatives were rejected and why:
- Identity mode of submitter: anonymous / pseudonymous / public / organization:
- AI assistance used:

### Section B — Claims

For every important claim, write:

- Claim:
- Evidence:
- What the evidence does not prove:
- Falsification test:
- Required external review:

### Section C — AI author self-check

- Did I inspect current code rather than rely on issue text?
- Did I bind approvals and evidence to exact digests?
- Did I add negative and adversarial tests?
- Did I preserve owner choice?
- Did I introduce a route from money to governance?
- Did I overstate cryptographic or security maturity?
- Did I add an unnecessary dependency or authority?

### Section D — Independent AI attack pass

A different model, context, or agent should attempt:

1. malformed input and boundary attacks;
2. privilege/capability escalation;
3. replay, rollback, equivocation, and stale-state attacks;
4. identity-root duplication or quorum inflation;
5. cross-crate contract mismatch;
6. crash consistency and partial failure;
7. weakest-device denial of service;
8. privacy metadata leakage;
9. documentation/implementation mismatch;
10. mathematical counterexample where applicable.

The attack pass publishes findings and reproduction steps. It does not approve the proposal.

### Section E — Simplification pass

- Can a state, dependency, feature, or public API be removed?
- Can a security condition be enforced by type or construction?
- Can the same property be expressed with less ambient authority?
- Is complexity isolated to machines that need it?

### Section F — Human review

The human reviewer records:

- exact digest reviewed;
- evidence personally inspected;
- findings accepted or rejected with reasons;
- uncertainty remaining;
- whether external review is still required;
- approval, request changes, or reject.

A human must not approve merely because multiple AIs agree. Agreement is evidence, not legitimacy.

## Two-contributor pattern

- Contributor A and their AI author work on proposal A.
- Contributor B's AI adversary attacks proposal A; B performs human review.
- Contributor B and their AI author work on proposal B.
- A's AI adversary attacks proposal B; A performs human review.
- The combined integration state receives a new cross-change AI and human review.
- Founder or a third authorized reviewer supplies any additional required approval for protocol-critical mainline changes.

## Hundred-contributor pattern

Each working group operates author, adversary, simplifier, and human-review lanes. Cross-domain proposals require reviewers from every affected group. The Integration Council reviews only boundary and canonicalization evidence rather than re-reviewing every line.

## AI attribution

Attribution should record model/tool family, date, role, relevant prompt or task summary, generated scope, and the human or governance identity accepting the submission. Sensitive prompts need not be public if they contain vulnerabilities or private information, but the existence and scope of AI assistance must not be concealed.

## AI permissions

AI-operated credentials should have the minimum permissions needed to create branches, proposals, comments, and test artifacts. AI credentials must not have direct canonical-branch push, release-signing, treasury, constitutional-vote, or secret-administration authority.

## Required final-record fields

For material AI-assisted work, preserve: exact proposal digest, AI roles used, material claims made, tests and attacks attempted, unresolved uncertainty, authorized reviewer identities, conflicts of interest, and final responsibility acceptance. The accepting identity may be persistent and pseudonymous. A model name or transcript is useful provenance but never substitutes for review of the final exact state.
