# Implementation Conformance Map

**Status:** Living normative mapping

**Version:** 1.1

## Normative interpretation

This document is subordinate to `docs/FOUNDER_DIRECTIVES.md`, frozen invariants, and accepted decisions in the canonical Mininet repository. Where this package differs from canonical project authority, the canonical repository prevails until a governed amendment explicitly adopts the new rule.

Identity disclosure is never presumed. Anonymous, pseudonymous, and public participation remain valid unless a narrowly scoped role requires continuity or delegated authority. Persistent cryptographic accountability does not imply legal-name disclosure.

## Purpose

This map prevents governance prose from being mistaken for enforcement. Every high-impact rule receives an implementation location, evidence requirement, and current maturity label.

| Governance requirement | Expected enforcement | Evidence | v1 package state |
|---|---|---|---|
| exact-state proposals and reviews | `mini-forge` object validation | negative tests for stale approvals | specified; existing foundation |
| AI cannot satisfy human/governance quorum | merge policy and identity-role validation | mixed human/AI quorum tests | specified |
| AI session charter and adapter | protected root `AGENTS.md`, canonical trust-before-load comparison, external three-content-digest activation record, structured final Decision, separate canonical checkpoint, current-phase/time gates, proposal policy, and tool-permission boundaries | `GOV-AI-050-01` through `GOV-AI-050-06` | charter, adapters, schemas, and structural/adversarial tests packaged; not activated |
| voluntary adoption | `mini-update` and `mini-installer` typed owner approval | rejection without exact owner approval | existing foundation |
| trusted Wasm provenance | isolated runner + signed execution provenance | adversarial runner tests | partial foundation |
| rollback/freshness/transparency | release verifier | stale, equivocation, rollback tests | existing foundation |
| anonymous bounty claims | `mini-bounty` plus settlement policy | double-claim and unlinkability review | experimental |
| constitutional amendment | Forge governance objects and policy engine | amendment-class and cooling tests | specified |
| working-group rotation | delegation/revocation objects | expiration and capture simulations | specified |
| founder disappearance | independent authority and recovery records | continuity exercise | specified, not activated |
| Forge cutover | dual-running and recovery proofs | GitHub outage exercise | specified |
| one human, one vote | privacy-preserving personhood mechanism | false-positive/false-negative and Sybil evidence | unresolved research |
| confidential value/treasury | audited cryptographic implementation | external audit and remediation | experimental |

## Update rule

This document MUST be updated when a rule changes maturity. A status increase MUST cite evidence; a status decrease MAY be made immediately when a gap is discovered.

## Honesty rule

Documentation SHALL prefer “not implemented,” “partial,” or “unverified” over an unsupported assurance claim.
