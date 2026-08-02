# Cryptographic architecture: composition over invention, and the flagship research protocol (D-0421)

**Status:** Canonical doctrine/research-roadmap synthesis. No cryptographic primitive, protocol activation, authority, or implementation is created by this document. D-0427 now supplies the dedicated Phase-0 doctrine for the anti-collusion settlement gap originally named here; implementation remains unstarted.

## Why this document exists

`CLAUDE.md` and D-0063 already define the fence: Mininet does not invent proprietary hashes, ciphers, signature schemes, RNGs, password hashing, TLS, or general-purpose ZK curves/proving systems. What was missing was the positive rule inside that fence:

> Mininet may compose Mininet-specific protocols from established, independently analyzed primitives when no off-the-shelf protocol expresses the required participant sovereignty, privacy, and authority boundaries. A new composition remains experimental until its transcript, assumptions, non-goals, abuse model, migration/shutdown path, weakest-device cost, and external-review gate are explicit.

This is not permission to solve social questions with cryptographic vocabulary. A proof can bind a transcript or hide a witness. It cannot make one root one human, prove genuine attention, prove two organizations are independent, or prove social value.

**Refs:** Directive 14; D-0063; D-0068; D-0070; D-0095/D-0322; D-0098; D-0099; D-0427; `docs/INVARIANTS.md` hard limitations.

## Decision

Use established primitives, narrow typed protocols, staged maturity, and external review. Prefer composition over duplicated implementations and prefer removal over sophistication. “More true to Mininet” means:

- fewer trusted intermediaries;
- no new central authority;
- less metadata learned by the network;
- deterministic verification of a bounded claim;
- participant-owned keys and local choice;
- explicit degradation when an edge service disappears; and
- no conversion of money, service history, or cryptographic credentials into political authority.

Every research track below keeps its own acceptance gate. Shared plumbing or a shared program name does not make several incomplete tracks collectively production-ready.

## The six tracks

### 1. Private proof of useful contribution

**Current status:** doctrine/research preparation, not activated.  
**Primary source:** `docs/design/mn602-mn603-anonymous-resource-payment-preparation.md` (D-0099).

D-0099 defines online-spend, issuer-backed, fixed-denomination blind resource credits as a research path for relay/mix/storage/bridge/private-index service, with strict role separation and a phased path from transparent valueless tokens to a separately reviewed limited real-value pilot. Privacy Pass, GNU Taler, Coconut/BBS-style credentials, and related systems are prior-art references, not silently selected dependencies.

`mini-resource-pricing` remains quoting only. No blind token, redemption, or wallet crate is implied by this synthesis.

### 2. Anti-collusion settlement

**Current status:** Phase-0 doctrine exists (D-0427); no implementation phase is started.  
**Primary source:** `docs/design/anti-collusion-content-settlement-preparation.md`.

D-0417's `mini-contribution` is a requester-funded, linkable baseline. A signed receipt can prove that typed parties signed a transcript and that a verified delivery verdict was bound into a claim. It cannot prove requester/provider independence or genuine demand.

D-0427 supplies the distinction this document's original short problem statement lacked:

- an operator paying itself from its own finalized balance can fabricate unlimited **claim volume**, but cannot create unlimited net protocol value;
- the commons-loss problem begins when a sponsor, treasury, emission/subsidy budget, or farmable privilege pays the claim.

Therefore requester-funded voluntary settlement must remain independent of anti-collusion issuers/auditors. Sponsor-funded and protocol-subsidized settlement require finite precommitted budgets, typed policy, duplicate/rate-limit rules, objective challenge/audit behavior, and a shutdown path that affects only that program.

Delivery challenges prove freshness/delivery/replay resistance only. They do not prove collusion resistance, human demand, usefulness, or one-human-one-claim.

### 3. Unlinkable personhood membership

**Current status:** research proposal only; one-person uniqueness remains unresolved.  
**Primary source:** `docs/design/frontier-personhood-governance-and-consensus-proposals.md`.

The proposed architecture separates evidence, policy, aggregate proof, credential issuance, scoped nullifiers, and recovery. `mini-uniqueness` remains a narrower signal-fusion prototype and is not an anonymous unique-human credential.

The strongest honest future statement is policy- and epoch-bound risk-limited eligibility, not metaphysical proof of biological uniqueness. Identity-root count must not be described as human count.

### 4. Private and federated search

**Current status:** mixed.

- F1/F2 signed observation/segment exchange format, F3 deterministic local federation, F4 local re-ranking, and F7 bounded local observation history are implemented in `mini-search-federation` (D-0422/D-0423/D-0424/D-0426).
- F5 has Phase-0 anti-collusion settlement doctrine only (D-0427).
- F6 private query transport remains undesigned.
- PIR remains research/review preparation under `docs/design/mn208-pir-research-and-review-preparation.md` (D-0098).

No implemented Track F function performs a private remote query or pays a provider. Local plurality and signed data exchange are foundations, not deployment.

### 5. Recoverable post-quantum identities

**Current status:** follow `docs/STATUS.md` and `docs/design/post-quantum-identity-migration.md`; production migration remains externally gated.

Mininet uses standardized ML-DSA-65 through reviewed external implementation code rather than implementing lattice mathematics in-house. Verify/sign/provisioning slices do not by themselves migrate a KEL, preserve recovery semantics, or activate PQ authority. A live-break recovery path must distinguish identities with pre-break anchors from those without them.

### 6. Proof-carrying Forge contributions

**Current status:** the most mature composition track.

`mini-provenance`, `mini-build-runner-wasmtime`, and `mini-forge::release` compose signed build provenance, isolated execution, independent-builder agreement, a release transparency log, rollback protection, and equivocation detection. This is the repository's proof that “compose established primitives, do not invent them” can reach a real end-to-end workflow.

It does not turn AI evidence into approval, repository access into authority, or a release into owner adoption.

## What Mininet does not invent

The hard rule remains in `CLAUDE.md`; this document does not create a second drifting list. In summary, Mininet does not invent:

- cryptographic hash functions;
- symmetric ciphers or AEADs;
- signature algorithms;
- RNGs or password hashing;
- TLS;
- general-purpose proving curves/systems; or
- mathematical constructions merely to avoid an external dependency.

Naming a prior-art construction as a candidate is not adopting, vendoring, implementing, or auditing it.

## Required maturity gate

Before any research composition here backs real MINI, real authority, or real personal data, require at minimum:

1. exact typed claim and protocol transcript;
2. explicit adversary, assumptions, privacy budget, and non-goals;
3. canonical encoding and domain-separation registry entry;
4. replay, downgrade, cross-domain, recovery, issuer/auditor/provider collusion, and metadata analysis;
5. bounded malformed-input and denial-of-service behavior;
6. deterministic test vectors and adversarial simulation;
7. weakest-supported-device benchmarks;
8. at least one independent verifier implementation for a proof-critical path;
9. migration, shutdown, and role-disappearance behavior;
10. external cryptographic/privacy review under D-0047;
11. independent economic/mechanism review where value or subsidy is involved; and
12. a separate exact-state activation decision for the reviewed artifact and parameters.

Tests passing, founder review, multiple agreeing AI systems, or several keys under one organization are not substitutes for independent review or operational independence.

## 7. Anti-collusion settlement gap — now owned by D-0427

This section originally named the gap but deliberately refused to design it in the same PR. The required dedicated Phase-0 document now exists at `docs/design/anti-collusion-content-settlement-preparation.md` (D-0427).

The canonical problem statement is now:

> A signed service receipt proves a signed transcript, not economic independence. Ordinary voluntary requester-funded transfers may remain permissionless. Any third-party or commons-funded reward program must ensure that colluding identities cannot consume more than a finite precommitted budget or multiply a capped entitlement, while avoiding a central permission authority and without publishing a surveillance graph.

A resolution requires, according to the exact funded class:

- immutable funding/policy class and budget commitment;
- no unbounded per-claim issuance;
- fresh typed delivery challenges, honestly scoped to delivery integrity;
- cross-claim replay/duplicate resistance;
- a settlement-domain rate-limit credential only where the funded policy requires one;
- no one-human claim until personhood is independently accepted and audited;
- delayed precommitted random sampling and objective fraud proofs;
- no reversal of canonical finality or confiscation of unrelated balances;
- no effect on personhood, ranking, moderation, governance, validation, or review quorum; and
- issuer/auditor disappearance that halts only the affected subsidy program.

The next phase is a transcript, settlement-class schema, adversary/economic model, simulator, privacy budget, and numeric falsification thresholds. It is not a nullifier crate.

## 8. Flagship synthesis: Unlinkable Proof of Useful Contribution

This is a label for eventual convergence, not a new cipher or an authorization to start implementation:

> A provider proves entitlement to a bounded payment or subsidy for a typed useful service, while the protocol minimizes linkage among requester, provider, content/query, funding source, and other activity; replay and capped-entitlement multiplication are rejected; and no proof or payment creates political authority.

The synthesis requires three independently mature tracks:

- private resource/payment credentials;
- anti-collusion/cap integrity for third-party or commons-funded claims; and
- an accepted scoped membership/rate-limit assumption where policy requires one.

Requester-funded market settlement does not wait for this synthesis and must not be placed behind its issuers. A protocol-subsidized real-value pilot does wait for the applicable reviewed pieces and a separate activation.

A coherent future implementation may share reviewed credential/nullifier tooling, but secrets and domains must remain distinct across settlement, personhood, review, resource payment, governance, and search.

## Constitutional impact

None. This document and D-0427 add doctrine, not authority. They strengthen the practical application of Directives 2, 4, 5, 9, 14, 16, and 18: remove central dependencies, preserve canonical ownership, learn less data, keep complexity bounded, keep money separate from voice, and ensure edge services can disappear without taking the core with them.

## Failure point

This synthesis fails if future work:

- calls receipt validity proof of genuine human demand;
- calls a delivery challenge collusion resistance;
- makes a credential issuer necessary for ordinary requester-funded payments;
- hides an uncapped subsidy behind privacy terminology;
- treats multiple keys as organizational independence;
- shares nullifiers/secrets across authority domains;
- lets audit heuristics seize balances or lower humanness;
- converts provider revenue into ranking/governance authority; or
- skips external review because primitives were individually standardized.

## Required follow-up

- D-0427 Phase 2: transcript/schema/threat/economic model and precommitted numeric gates.
- Continue each other track only through its own documented maturity process.
- Whoever changes a track's real status updates this synthesis and `docs/STATUS.md` in the same proposal.

## Supersedes / superseded by

Supersedes nothing. It cross-references and organizes D-0063, D-0068, D-0070, D-0095/D-0322, D-0098, D-0099, and D-0427 without modifying their authority or activation status.