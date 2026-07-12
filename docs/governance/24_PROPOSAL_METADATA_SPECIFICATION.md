# Proposal Metadata Specification

**Status:** Normative bootstrap specification  
**Version:** 0.4

## 1. Purpose

Proposal metadata makes review requirements explicit and machine-checkable while preserving the right to contribute anonymously or pseudonymously. It describes the proposal and its evidence; it does not demand legal identity.

## 2. Required fields

A proposal MUST include the following headings or equivalent structured fields:

- `Change class`
- `Exact state`
- `Summary`
- `Founder directives`
- `Invariants`
- `Decision log`
- `Evidence and tests`
- `AI assistance`
- `Security and privacy`
- `Dependencies`
- `Release and adoption`
- `Compensation`
- `Reviewer attestations`

## 3. Exact-state binding

Reviews and approvals apply only to the exact final commit or Forge object digest named by the proposal. A substantive update invalidates prior approval unless the reviewer explicitly re-attests to the new state.

On GitHub, the exact state is the pull request head SHA. In Forge, it is the signed proposal-state object digest.

## 4. AI disclosure

Material AI assistance MUST state the roles performed, such as author, researcher, adversary, optimizer or documentation reviewer. AI output MAY count as evidence. It MUST NOT count as authorization, human uniqueness, governance quorum or independent release approval.

The proposal MUST identify an authorized persistent identity accepting responsibility for submitting the final state. That identity MAY be pseudonymous and need not disclose a legal name.

## 5. Invariant declarations

The proposal MUST either list every affected invariant or state why none are affected. A Tier-F domain change MUST identify the enforcing code/test update and the associated decision entry. Ambiguity on a frozen domain resolves to rejection until clarified.

## 6. Tests

Evidence SHOULD include:

- positive behavior test;
- negative or adversarial test;
- integration test when crossing crate boundaries;
- regression test for a corrected defect;
- reproducibility evidence where build or release behavior changes.

Passing tests establish tested behavior, not cryptographic security or production readiness.

## 7. Compensation separation

A proposal MAY reference a bounty or payment destination. That reference MUST NOT alter reviewer count, merge authority, governance weight or release authority. Technical acceptance and compensation authorization are separate decisions.

## 8. Privacy

A contributor MAY omit legal identity, employer, location and other unnecessary personal data. Security reports MAY use private disclosure. Public attribution is voluntary unless a narrowly scoped role explicitly requires a persistent public identity under accepted governance.
