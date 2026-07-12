# Working-Group Charter and Lifecycle

**Status:** Normative scaling specification

## 1. Purpose

Working groups distribute technical responsibility without creating permanent owners of protocol domains.

## 2. Charter object

Every group MUST have a signed charter defining:

- name and immutable group identifier;
- purpose and domain boundaries;
- dependencies and neighboring groups;
- decisions it may make autonomously;
- decisions requiring cross-group or governance approval;
- reviewer and maintainer roles;
- selection, term, inactivity, removal, and appeal rules;
- conflict policy;
- security/private-information handling;
- reporting and evidence requirements;
- dissolution, split, and merge rules;
- charter version and amendment threshold.

## 3. Lifecycle

```text
Proposed -> Incubating -> Active -> Mature
                         -> Suspended
                         -> Splitting / Merging
                         -> Retired
```

### Proposed

A charter and initial scope exist, but no delegated authority.

### Incubating

The group may organize work and provide advisory review. Canonical authority remains with bootstrap maintainers or governance.

### Active

The group has defined reviewer/maintainer delegations and measurable responsibilities.

### Mature

The group demonstrates continuity, rotation, cross-group integration, and independence from one employer/person.

### Suspended

Authority is temporarily frozen because quorum, security, capture, or continuity requirements failed. Contribution continues; canonical decisions are rerouted.

### Retired

Responsibilities and open objects are transferred. Historical objects remain verifiable.

## 4. Formation gates

Formation requires:

- coherent technical scope;
- at least two independent persistent participants before receiving autonomous authority;
- a named integration boundary;
- no conflict with existing constitutional or domain authority;
- initial test/evidence obligations;
- a sunset review date.

A one-person group may exist for organization but MUST NOT be represented as independent governance.

## 5. Autonomy

Groups may decide ordinary implementation details within accepted specifications. They may not unilaterally alter frozen invariants, constitutional semantics, money/governance separation, owner adoption, personhood claims, or cross-domain interfaces without the required broader process.

## 6. Capture resistance

A mature group SHOULD avoid decisive control by one employer, one funding source, one key lineage, or one AI operator. Where diversity is not yet possible, the limitation MUST be explicit and authority narrower.

## 7. Split and merge

Splits and merges require responsibility mapping, open-proposal transfer, delegation replacement, and interface ownership decisions. No proposal or security obligation may become ownerless during reorganization.

## 8. Reporting

Groups SHOULD publish periodic machine-readable reports covering active authority, expiring delegations, unresolved disputes, security gates, integration debt, and contributor capacity. Reports are evidence, not self-certification.
