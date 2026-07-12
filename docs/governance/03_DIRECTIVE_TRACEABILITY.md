# Founder Directive and Governance Traceability

**Status:** Normative process requirement

## Objective

Mininet preserves reasoning as infrastructure. Every material change must be traceable from human purpose through constitutional rule, implementation, test, and release evidence.

The required chain is:

`Founder Directive -> Constitutional Article -> Frozen Invariant -> Decision/RFC -> Specification -> Code -> Test -> Build Evidence -> Governed Release`

Not every editorial change touches every link, but every security- or protocol-relevant proposal must identify all applicable links.

## Proposal traceability block

Every material Change Proposal should include:

```text
Directive impact: D1, D2, ...
Constitutional articles:
Frozen invariants:
Decision-log entries:
Specifications:
Threat-model delta:
Code owners / working groups:
Tests added or changed:
External gate required: yes/no
Owner-adoption impact:
```

## Required reasoning

A proposal must state not only which directive supports it, but what trade-off it introduces. For example, adding a large sandbox dependency may be consistent with simplicity only when isolated behind a narrow boundary and safer than custom sandboxing.

## Frozen invariant rule

A frozen invariant must be encoded as a check rather than a convention wherever technically possible. Documentation saying “do not do X” is insufficient when the type system, dependency graph, verifier, or CI can make X impossible.

## Status honesty

Traceability records must distinguish:

- implemented and enforced;
- implemented but not integrated;
- tested internally but unaudited;
- specified only;
- open research;
- externally blocked.

A green test suite must not change an unaudited construction into audited cryptography.

## Review duties

Reviewers must verify that:

1. cited directives genuinely support the proposal;
2. no affected invariant is omitted;
3. tests exercise the claimed property rather than merely a happy path;
4. documentation does not overstate implementation;
5. new authority or data collection is explicit;
6. owner choice remains preserved;
7. any compensation mechanism remains separated from governance weight.

## Machine-readable future form

Mininet Forge should eventually encode these fields in signed proposal objects. CI should generate an invariant coverage report and reject protocol-critical proposals with missing traceability.
