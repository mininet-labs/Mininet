<!--
Received deliverable, reproduced verbatim below the scope header. Do not edit
the body: corrections and responses belong in a new dated document, the same
append-only discipline the decision log uses.
-->

**Audit scope** (required header, D-0441)

| Field | Value |
|---|---|
| Reviewed at | `main` @ `e60191c4a0fdc8be42995cc2fb21b9a56e910f44`, plus PR #297 at `7a48a878b9007364a7779f3e03ceec5066e34d61` |
| Workspace size at that commit | 71 crates |
| Method | Documentation, repository map, CI configuration, and representative implementations. **Not** a local build, test run, or automated static-analysis pass — the auditor states this explicitly in §2 |
| Tool versions | None; no tooling was run |
| Revalidation trigger | Any change to the vertical slices named in §3.1, or the next external review |
| Response | `docs/audits/external-full-codebase-audit-2026-08-05-response.md` |

---

# Mininet Full-Codebase Improvement Audit

**Repository:** `mininet-labs/Mininet`  
**Audit date:** 2026-08-05  
**Primary branch reviewed:** `main`  
**Additional focused review:** PR #297 (`mini-storage-fraud`)  
**Repository scale observed:** approximately 840 indexed files, 71 crates, and 7,823 indexed Rust symbols.

---

## 1. Executive assessment

Mininet is unusually strong in architectural intent, explicit authority boundaries, typed APIs, adversarial documentation, and honest maturity labeling. The repository is not a shallow prototype: it already contains substantial implementations for identity, KELs, storage, messaging foundations, BFT verification, settlement, proof-of-replication research, forge/release/provenance pipelines, update and rollback, search components, desktop integration, Windows vault support, Android/UniFFI boundaries, and governance machinery.

The dominant risk is now **breadth outrunning integration and assurance**. The project has accumulated more than seventy crates and a very large decision/governance surface, while many critical guarantees still stop at library boundaries. The most important next step is not adding more isolated mechanisms. It is making a smaller set of end-to-end vertical paths demonstrably correct under real persistence, process restarts, key rotation, hostile inputs, multiple devices, lossy networks, stale state, and compromised peers.

### Overall verdict

- **Architecture:** ambitious, coherent, and often carefully bounded.
- **Implementation quality:** generally disciplined Rust, strong type modeling, good negative documentation, and many tests.
- **Production readiness:** not yet suitable for value-bearing, identity-critical, privacy-critical, or governance-critical deployment without external review and deeper integration testing.
- **Main technical danger:** individually plausible components may create false confidence when composed without authenticated state, freshness, replay, registration, and consequence boundaries.
- **Main organizational danger:** a growing decision-log and crate count can make the repository harder to reason about than the protocol itself.

### Top ten priorities

1. Freeze broad feature expansion temporarily and establish a small number of end-to-end reference workflows.
2. Build a unified state/freshness/replay model instead of separate per-crate in-memory variants.
3. Complete historical KEL signature verification and witnessed freshness before relying on long-lived evidence.
4. Replace self-asserted cryptographic claims with authenticated registration records and verifiable state transitions.
5. Establish a canonical wire-format framework shared across crates.
6. Add workspace-wide fuzzing, property testing, state-machine testing, and cross-platform golden vectors.
7. Repair dependency-vulnerability CI so it actually gates rather than silently failing under `continue-on-error`.
8. Reduce crate fragmentation and duplicated policy/codec/error/replay abstractions.
9. Make the network daemon, persistence layer, and multi-device lifecycle the center of development.
10. Treat every value, identity, update, personhood, and cryptographic prototype as externally gated until independent review is complete.

---

## 2. Audit scope and limitations

This report examines the repository as a whole through:

- the generated repository map;
- root workspace configuration;
- current product/architecture and beta gap documents;
- CI configuration;
- memory-safety audit material;
- representative core implementations previously inspected in identity, storage, proof-of-replication, consensus evidence, and PR #297;
- documented status, design boundaries, and known follow-ups across the repository.

The execution environment could not directly clone GitHub, so this was not a local full-workspace build, compiler run, or automated static-analysis pass over every source file. Findings are therefore classified implicitly as:

- **verified code finding** where a concrete implementation was inspected;
- **verified repository/process finding** where current configuration or status documentation states the condition;
- **systemic recommendation** where repeated architectural patterns indicate a likely improvement area requiring repository-side confirmation.

A future follow-up should run this report’s proposed automated checks in a local checkout and append machine-generated file/line findings.

---

## 3. Cross-cutting architectural findings

### 3.1 Breadth has exceeded the project’s current integration capacity

The repository contains approximately 71 crates spanning identity, transport, storage, economics, consensus, treasury, social, search, crawling, desktop, Android-facing FFI, forge, provenance, installation, and policy. This modularity is conceptually clean, but the number of boundaries creates a large integration tax:

- repeated codecs;
- repeated replay guards;
- repeated registry/oracle traits;
- repeated timestamp/freshness handling;
- separate in-memory state machines with no shared durable backend;
- many designs that are correct only if callers preserve unstated sequencing requirements.

**Improvement:** define 5–8 protocol-critical vertical slices and require each to have one complete integration crate or harness covering persistence, process restart, wire encoding, network transfer, key rotation, replay, and adversarial failure.

Suggested slices:

1. Identity creation → device delegation → rotation → revocation → recovery → witnessed freshness.
2. Public object creation → storage → sync → indexing → deletion/tombstone/moderation visibility.
3. Private session setup → asynchronous delivery → ratchet → device removal → backup/recovery.
4. Storage registration → PoRep audit → continuing challenge → reward eligibility → conflict handling.
5. Transaction admission → settlement → execution → reorg/finality → recovery from crash.
6. Forge proposal → review → governed merge → reproducible build → release → install → rollback.
7. Search crawl → extraction sandbox → immutable index → federation → ranking explanation.

### 3.2 Too many security properties remain caller responsibilities

Multiple APIs document that callers must provide fresh KELs, use the correct replica ID, preserve ordering, persist replay state, or validate registration. This is honest, but unsafe as a long-term API model.

**Rule:** when violating a requirement destroys soundness, encode it in a constructor, verified type, registry transition, or state machine. Do not leave it as documentation.

Examples:

- `VerifiedKelAtCheckpoint`, not raw `Kel`, for high-value verification.
- `RegisteredSealCommitment`, not raw `StorageCommitment`.
- `FreshChallenge`, not arbitrary timestamps/nonces.
- `PersistedReplayGuard`, not a trait whose default path is in-memory.
- `AuthorizedDevice<CAPABILITY>`, not a DID plus later manual capability checking.

### 3.3 Separate libraries are often ahead of a canonical state model

Many components produce or verify objects but there is no single authenticated view of:

- current identity state;
- current delegation/revocation state;
- accepted storage registrations;
- accepted releases;
- current governance state;
- final settlement state;
- current network membership and peer reputation.

This can create “locally valid, globally stale” behavior.

**Improvement:** define canonical state-view traits with explicit checkpoint identity:

```rust
trait CanonicalView {
    type Checkpoint: Clone + Eq + Hash;
    fn checkpoint(&self) -> &Self::Checkpoint;
}
```

Every high-value verification result should record the checkpoint against which it was valid.

### 3.4 The repository needs an explicit complexity budget

Every new crate, decision, policy document, registry, and wire format expands the protocol’s permanent maintenance burden.

Add a mandatory PR section:

- Why this cannot live in an existing crate.
- Which duplicate abstraction it removes.
- Long-term migration cost.
- State/wire compatibility commitment.
- What can be deleted after this lands.

Prefer merging narrow policy crates where they contain only a few data types and no independent trust boundary.

---

## 4. Identity, KEL, delegation, and recovery

### 4.1 Historical detached signatures are not first-class

`Kel::verify_message` verifies against the **current** key state. Long-lived claims signed before rotation can stop verifying after an ordinary rotation. This affects storage claims and may affect other detached payloads.

**Required improvement:**

- add `verify_message_at_sequence` or `verify_message_at_event_digest`;
- include KEL sequence and event digest in every long-lived signed object;
- support witnessed checkpoint validation;
- distinguish “valid when issued” from “authorized now.”

### 4.2 Freshness remains a critical unresolved boundary

A verifier checks the KEL it is handed, not necessarily the newest KEL. Revoked devices may remain valid against stale copies.

**Required improvement:**

- make monotonic freshness pins persistent and mandatory for high-value operations;
- integrate witness receipts or chain anchoring;
- detect KEL forks and duplicity before accepting authority;
- define stale-state failure behavior for offline operation;
- create explicit freshness classes: `Unpinned`, `LocallyPinned`, `Witnessed`, `Finalized`.

### 4.3 Device authority is not consistently modeled across domains

The repository has generic capabilities such as SIGN, PAY, POST, ATTEST, VOTE, and MANAGE_DEVICES. New domains may reuse SIGN because no domain-specific bit exists.

**Improvement:** avoid an ever-growing global bitset. Use typed capability grants or namespaced capability identifiers with conservative unknown handling.

Examples:

- `StorageProviderOperate`
- `StorageProviderRegisterReplica`
- `ForgeBuildWorker`
- `RelayOperate`
- `CrawlerFetch`

### 4.4 Root keys risk being pulled into online workflows

Where APIs accept only a root `Controller`, developers may keep sensitive root keys online rather than use delegated devices.

**Improvement:** all routine online actions should accept an authorized delegated signer and separately name the root. Root participation should be limited to recovery, delegation, revocation, and exceptional governance operations.

### 4.5 Pairwise identity recovery and rotation require deeper lifecycle tests

Deterministic pairwise pseudonyms derived from current key material may have lifecycle complexity after rotation. Confirm whether the same context remains recoverable across root rotation and device replacement.

**Tests needed:**

- derive before and after root rotation;
- restore from backup and reproduce pseudonym;
- migrate pseudonym control without linking contexts;
- compromised-old-key behavior;
- multi-key roots and future derivation policy.

### 4.6 Identity data structures need cross-language vectors

Android, desktop, FFI, and future clients must produce exactly the same SCIDs, KEL encodings, signature bytes, and delegation checks.

Create immutable vectors for:

- inception;
- delegated inception;
- rotation;
- revocation;
- threshold signatures;
- malformed and noncanonical encodings;
- KEL fork detection;
- recovery.

---

## 5. Cryptography and cryptographic protocol boundaries

### 5.1 “Composed from reviewed primitives” is not equivalent to reviewed protocol

The repository is careful to say this in places, but some design language still moves too quickly from a unit test to strong protocol claims.

**Improvement:** standardize maturity labels:

- `Primitive-backed prototype`
- `Internally adversarially tested`
- `Externally reviewed design`
- `Externally audited implementation`
- `Deployment validated`

No value-bearing path should depend on anything below externally audited implementation plus deployment validation.

### 5.2 Eliminate absolute cryptographic wording

Replace “always,” “never collides,” and “proves” where the guarantee depends on hash assumptions, probabilistic audits, network assumptions, trusted clocks, or honest setup.

Use explicit assumption statements and failure probabilities.

### 5.3 Protocol transcripts need one canonical framework

Many crates hand-build domain-separated signing bytes. This is better than generic signing, but repeated bespoke encoding increases divergence risk.

Create a shared transcript API:

```rust
trait CanonicalTranscript {
    const DOMAIN: &'static [u8];
    const VERSION: u16;
    fn encode_body(&self, out: &mut CanonicalWriter) -> Result<()>;
}
```

The framework should enforce:

- version;
- domain;
- network/genesis identifier;
- length-delimited fields;
- canonical ordering;
- bounded lengths;
- checked integer conversions;
- test-vector generation.

### 5.4 Add cryptographic misuse tests

Workspace-wide tests should attempt:

- cross-domain signature replay;
- cross-network replay;
- version downgrade;
- alternative encoding of same semantic object;
- duplicate signature indices;
- threshold inflation using repeated keys;
- stale KEL acceptance;
- signing with a delegated device lacking capability;
- transcript field omission.

### 5.5 Post-quantum migration must include stored historical signatures

A PQ anchor or migration design must specify what happens to:

- old KEL events;
- old release attestations;
- old settlement claims;
- long-lived content signatures;
- recovery records;
- historical evidence.

Do not treat PQ migration as merely adding a new signature suite tag.

---

## 6. Wire formats, codecs, serialization, and compatibility

### 6.1 Codec duplication is now a systemic risk

Multiple crates implement small custom readers/writers, limits, trailing-byte rejection, and signature encoding. The approach is disciplined but duplicated.

**Improvement:** build one audited `mini-codec` crate or a small set of profiles:

- canonical object codec;
- bounded network codec;
- secret-state codec;
- human-readable diagnostic encoding.

### 6.2 Use checked conversions everywhere

Any `u64 as usize`, `usize as u32`, or vector length cast can truncate across platforms or silently create malformed output.

Enforce Clippy lints or custom checks for:

- `as usize` from larger integer types;
- `len() as u32`;
- multiplication before bounds checking;
- timestamp arithmetic overflow;
- capacity calculations.

### 6.3 Encoders should not produce undecodable values

If public fields can exceed decoder limits, `to_bytes()` should return `Result<Vec<u8>>`, or constructors should guarantee all invariants.

### 6.4 Canonical ordering must be explicit

Maps, sets, signatures, approvals, attestations, and evidence pairs need deterministic ordering. Equivalent objects should not have multiple content IDs.

### 6.5 Versioning needs migration policy, not only version tags

Each durable wire type should define:

- whether unknown versions are stored, relayed, or rejected;
- downgrade behavior;
- migration ownership;
- canonical re-encoding rules;
- how old signatures remain verifiable;
- maximum support horizon.

### 6.6 Add fuzzing for every public decoder

The repository should maintain a cargo-fuzz or libFuzzer target per decoder family, with a CI smoke corpus and scheduled deeper runs.

Required properties:

- no panic;
- no excessive allocation;
- no quadratic behavior;
- decode(encode(x)) = x;
- encode(decode(bytes)) is canonical;
- trailing bytes rejected;
- malformed counts fail before allocation.

---

## 7. Storage, PoRep, spacetime, erasure, and fraud evidence

### 7.1 PR #297 has blocking soundness issues

The proposed storage-fraud claim authenticates a DID’s statement about a generic `StorageCommitment`, but does not authenticate that the commitment came from a correctly identity-bound `mini-porep::SealCommitment` or that a registration audit passed.

An attacker can copy an honest provider’s public root, sign it under the attacker’s own DID, and create accepted “collision evidence.” This can frame an honest provider.

**Required redesign:**

- claim the full `SealCommitment` or its authenticated registration digest;
- require `replica_id == derive_replica_id(root, device, assignment)`;
- require `replica_root` and `node_count` to match the ongoing PDP commitment;
- require proof of accepted registration audit;
- classify duplicates as ambiguous conflict evidence, not proof that both parties colluded;
- trigger fresh individual audits;
- never automatically slash both parties.

### 7.2 Storage identity binding needs a founder-level decision

Specify whether a replica ID binds to:

- provider root;
- storage device;
- physical disk;
- piece/assignment;
- replica ordinal;
- epoch or sealing policy.

Binding only to root + context may cause two devices under one root to produce the same replica.

### 7.3 Registration and ongoing possession must be one authenticated lifecycle

A real storage state machine should be:

```text
ProposedSeal
→ CommittedBeforeChallenge
→ RegistrationChallengesIssued
→ RegistrationAuditAccepted
→ ActiveReplica
→ OngoingProofWindow
→ Degraded
→ Suspended
→ Recovered or Retired
```

Every transition should be authenticated, checkpointed, and replay resistant.

### 7.4 Probabilistic audit parameters need security calculations

The PoRep audit should expose a documented relationship among:

- fraction of work skipped;
- number of challenges;
- false acceptance probability;
- layer count;
- node count;
- challenge independence;
- adversarial precomputation.

Tests that sample 30 challenges are not a security parameter specification.

### 7.5 Merkle proof verification should validate proof shape

Where Merkle proofs contain sibling lists, verification should ensure the proof depth/shape is consistent with the declared leaf count, not only that the final root matches. Otherwise unusual promoted-node paths may permit noncanonical proofs.

### 7.6 Erasure coding needs end-to-end corruption and repair tests

Beyond matrix correctness, test:

- adversarial shard substitution;
- inconsistent shard metadata;
- mixed generations;
- partial repair interruption;
- duplicate shard IDs;
- malicious repair peers;
- content ID verification after reconstruction;
- huge-object streaming without full-memory buffering.

### 7.7 Capacity accounting must bind bytes, pieces, and proofs

Ensure a provider cannot claim capacity units unrelated to actual node count or byte size. `capacity_units` should be derived from accepted commitments, not caller supplied.

---

## 8. Networking, transport, relay, bridge, and sync

### 8.1 Networking remains more demo-like than service-like

The architecture documentation itself identifies the absence of a durable local daemon, background scheduler, authenticated discovery, NAT traversal, relay fallback, reconnect/backoff, and multi-host loss testing.

**Priority:** make one operated network service before adding more protocol types.

### 8.2 Unify transport security and application authentication

Transport encryption alone is insufficient. Every session should bind:

- peer DID/device;
- negotiated protocol version;
- network/genesis ID;
- route/mailbox capability;
- channel transcript;
- replay window;
- session expiration.

### 8.3 Add a deterministic network simulator

Build a test harness for:

- latency;
- packet loss;
- duplication;
- reordering;
- partitions;
- asymmetric connectivity;
- relay compromise;
- clock skew;
- peer churn;
- bandwidth caps.

Use it across sync, consensus, messaging, relay, bridge, and storage challenges.

### 8.4 Peer scoring must remain behavior-scoped

Do not allow routing/reliability scores to become universal identity reputation. Make score type and scope impossible to reuse across governance, personhood, or economic authority.

### 8.5 Metadata privacy needs traffic-level testing

Opaque envelopes do not prevent timing, size, route, and polling-pattern leakage. Add padding/batching profiles, mailbox cover behavior where justified, and explicit observable-metadata tests.

---

## 9. Messaging and private communication

### 9.1 Current private messaging is not yet a secure-chat protocol

The product architecture correctly lists missing authenticated prekeys, ratchets, asynchronous mailboxes, forward secrecy, post-compromise recovery, multi-device fanout, group epochs, and replay windows.

**Improvement:** do not expand UI claims until a reviewed session protocol exists.

### 9.2 Ratchet implementation should reuse reviewed prior art

Do not invent a new ratchet. Implement or carefully adapt a well-reviewed Signal-style double ratchet with explicit licensing and audit plan, or isolate a reviewed implementation behind Mininet-owned interfaces.

### 9.3 Multi-device semantics need an explicit model

Define:

- whether each device has independent sessions;
- fanout ordering;
- device addition/removal effects;
- history visibility;
- sender keys for groups;
- message deduplication;
- backup recovery and deleted-device behavior.

### 9.4 Mailbox abuse and privacy are inseparable

Blind mailboxes require:

- quotas;
- proof-of-work or bounded admission;
- request inbox separation;
- capability rotation;
- deletion receipts;
- anti-enumeration;
- relay-independent recovery.

---

## 10. Consensus, chain, settlement, execution, and value

### 10.1 Verification libraries are ahead of a real networked consensus deployment

A BFT verifier is not equivalent to an operated consensus system. Missing or high-risk areas include:

- validator networking;
- durable WAL;
- crash recovery;
- proposer scheduling;
- mempool behavior;
- state sync;
- reconfiguration;
- fork-choice integration;
- evidence gossip;
- denial-of-service resistance.

### 10.2 Add formal state-machine tests

Use model-based/property testing for rounds, locks, votes, timeouts, reconfiguration, and crash recovery.

Check invariants such as:

- no two finalized blocks at one height;
- monotonic finalized height;
- no vote counted twice per identity root;
- no stale validator set accepted;
- recovery reproduces pre-crash state.

### 10.3 Time and finality must not rely on local wall clocks

Any expiry, cooling period, admission window, or settlement timeout should define the trusted time source and behavior under skew.

### 10.4 Economic parameters need simulation tied to executable code

The repository includes economic modeling crates and design documents. Ensure simulations import the exact production formulas and constants, rather than reimplementing approximations.

### 10.5 Separate evidence from automatic penalties

This is correctly emphasized in several places. Preserve it universally. Evidence should identify:

- what is cryptographically proven;
- what is inferred;
- possible innocent explanations;
- attribution confidence;
- required due process.

### 10.6 Value code requires a stronger audit and release gate

Any code involving balances, treasury custody, anonymous value, ring signatures, Bulletproofs, DKG, FROST, settlement, or reward issuance should be impossible to activate without an externally signed audit-gate artifact naming the exact commit.

---

## 11. Treasury, DKG, threshold signing, and custody

### 11.1 Threshold custody needs operational ceremonies, not only algorithms

Document and test:

- participant authentication;
- complaint handling;
- abort/retry;
- transcript retention;
- lost participant recovery;
- resharing under churn;
- malicious coordinator;
- device compromise;
- backup destruction;
- emergency succession.

### 11.2 Secret material lifecycle needs hardware-backed integration

The project has Windows vault and Android signer designs, but treasury-grade keys require:

- hardware-backed non-exportable keys where possible;
- explicit export prohibition;
- secure display/confirmation;
- independent-device quorum;
- signed ceremony transcripts;
- disaster recovery drills.

### 11.3 DKG and FROST need interoperable vectors

Generate test vectors against independent implementations or formal reference vectors. Internal round trips are insufficient.

---

## 12. Personhood, uniqueness, presence, and human evidence

### 12.1 Identity-root counting must never be presented as human uniqueness

The repository is explicit about this; keep enforcing it in types and UI.

Suggested type separation:

- `IdentityRoot`
- `HumanEvidenceBundle`
- `ProvisionalHumanCredential`
- `GovernanceEligibleHuman`

No implicit conversions.

### 12.2 Presence is not distance bounding

Application-layer RTT is useful evidence but not a formal proof of physical proximity. UI and downstream algorithms must preserve uncertainty.

### 12.3 Signal fusion needs adversarial calibration

For every personhood signal, define:

- cost to honest user;
- cost to Sybil attacker;
- false positive/negative rate;
- accessibility impact;
- geographic bias;
- coercion risk;
- privacy leakage;
- revocation and appeal.

### 12.4 Avoid irreversible global reputation

Human evidence should remain purpose-bound and privacy-preserving. Do not create a universal score that becomes social credit.

### 12.5 Government or institutional attestations must remain optional

They can be one signal, never the sole root of personhood or protocol participation.

---

## 13. Forge, provenance, builds, releases, updates, and installer

### 13.1 The forge spine is one of the strongest areas

The distinction among review, approval, merge, release, and owner adoption is excellent. Preserve this separation in UI and APIs.

### 13.2 Reproducibility must include environment capture

Reproducible codegen flags are not sufficient. Record:

- compiler and target;
- dependency lock;
- system libraries;
- build image digest;
- locale/timezone;
- environment variables;
- filesystem ordering;
- generated assets;
- signing separation.

### 13.3 Native tool execution remains a major sandbox gap

Any raw/native pipeline step should remain ineligible for trusted provenance until it runs inside a separately reviewed OS-level sandbox.

### 13.4 Installer rollback needs power-loss testing

Test failure at every filesystem step:

- mid-download;
- mid-stage;
- mid-rename;
- mid-symlink switch;
- after activation but before health confirmation;
- during rollback;
- disk full;
- antivirus lock;
- permission changes.

### 13.5 Release transparency should be independently replicated

A local append-only log is not enough if one operator can rewrite all copies. Add gossip, consistency proofs, checkpoint signing, and independent witnesses.

### 13.6 Git import/export needs ambiguity rules

Define handling for:

- SHA-1 and SHA-256 repositories;
- submodules;
- LFS;
- signed commits/tags;
- rewritten history;
- case-insensitive filesystems;
- symlinks;
- executable bits;
- malicious paths.

---

## 14. Search, crawling, extraction, ranking, and federation

### 14.1 Search components need an end-to-end service

The repository has intake, crawler, fetch, extraction, indexing, ranking, query, and federation crates. The next priority is one durable observation-to-result pipeline.

### 14.2 Extraction must remain out of process

HTML, documents, media metadata, and third-party adapters are hostile inputs. Keep parsers outside the trusted process with strict CPU, memory, output, and filesystem caps.

### 14.3 Robots and politeness are production blockers

Implement a durable robots cache, per-origin scheduling, redirect-aware policy, retry/backoff, and crawl budget accounting.

### 14.4 Federated query privacy is not PIR

Transport encryption does not hide the query from the queried provider. Preserve this statement in UI and APIs.

### 14.5 Ranking needs provenance and manipulation resistance

Every result should be able to explain:

- source segment/provider;
- ranking profile;
- applied user filters;
- trust/quality annotations;
- sponsored or conflict-of-interest status;
- missing/blocked reasons.

Avoid one global rank or reputation oracle.

### 14.6 Index exchange needs poisoning defenses

Add:

- signed segment manifests;
- origin observations;
- duplicate/canonical URL handling;
- malware/spam labels;
- resource accounting;
- quarantine before merge;
- independent corroboration.

---

## 15. Desktop, Windows vault, Android, FFI, and platform integration

### 15.1 FFI boundaries need panic containment

No Rust panic should unwind across FFI. Every exported function should return typed errors and catch internal panics where appropriate.

### 15.2 Platform keystores need capability-focused APIs

Do not expose generic signing through FFI. Export narrowly typed operations such as signing a KEL rotation, pairing response, release adoption, or message prekey bundle.

### 15.3 Vault formats need migration and corruption recovery

Define:

- versioned secret-state schema;
- atomic writes;
- backup strategy;
- rollback protection;
- partial corruption detection;
- device migration;
- user-visible recovery paths.

### 15.4 Real-device tests are mandatory

Android and Windows CI cannot replace:

- two physical Android devices;
- BLE GATT behavior across vendors;
- sleep/background restrictions;
- process death;
- permission revocation;
- clock changes;
- network handover;
- Windows antivirus/file locking.

### 15.5 Accessibility and safe failure need test plans

Cryptographic and recovery interfaces must work with screen readers, keyboard-only use, low vision, cognitive accessibility, and interrupted workflows.

---

## 16. Persistence, replay protection, clocks, and crash recovery

### 16.1 Replay protection is fragmented

The beta status notes separate replay-guard-shaped traits in several crates, with durable storage implemented only in some paths.

**Improvement:** create one persistent replay/freshness service with namespaced keys and transactional updates.

### 16.2 In-memory defaults are dangerous

For security-critical production constructors, require a durable backend explicitly. Keep in-memory variants under test or clearly named `Ephemeral...` types.

### 16.3 File-backed stores need locking and atomicity

Every file-backed state implementation should specify:

- concurrent process behavior;
- lock strategy;
- fsync semantics;
- temp-file/rename pattern;
- corruption checksum;
- permissions;
- rollback detection;
- recovery after partial write.

### 16.4 Time sources need abstraction

Use injected monotonic and wall-clock sources. Tests should simulate jumps, rollback, skew, and overflow.

---

## 17. Error handling and API design

### 17.1 Avoid collapsing distinct errors

Malformed encoding, unsupported algorithm, bad signature, stale authority, wrong provider, and truncated input should remain distinguishable where callers need different responses.

### 17.2 Verified and unverified types should be separate

Do not expose getters on unverified evidence as if values were trusted. Prefer consuming verification functions returning private-field verified wrappers.

### 17.3 Public mutable fields weaken invariants

Use constructors and accessors for protocol objects. Public fields make verify-then-mutate bugs easy.

### 17.4 Boolean verification loses diagnostics

Security-sensitive verification should return typed errors internally. A final convenience `is_valid()` can wrap it.

### 17.5 Avoid panics in library paths

Review every `unwrap`, `expect`, indexing operation, subtraction, and modulo for attacker-controlled values. Assertions are acceptable in tests, not public decoders or network handlers.

---

## 18. Testing strategy improvements

### 18.1 Unit-test count is not assurance

Prioritize tests that cross trust boundaries and process boundaries.

### 18.2 Establish a workspace-wide adversarial test matrix

For each protocol object:

- honest round trip;
- tampered field;
- wrong signer;
- stale signer;
- revoked signer;
- cross-domain replay;
- cross-network replay;
- duplicate input;
- out-of-order input;
- oversized input;
- truncated input;
- unknown version;
- future timestamp;
- clock rollback;
- process restart.

### 18.3 Add property tests

High-value properties:

- canonical serialization;
- threshold monotonicity;
- no duplicate counting;
- state-machine transition legality;
- CRDT convergence;
- erasure reconstruction;
- consensus safety;
- balance conservation;
- rollback monotonicity.

### 18.4 Add mutation testing

Use mutation testing selectively for core verification code. A test suite that still passes after flipping a comparison or skipping a signature check is not adequate.

### 18.5 Add long-running soak tests

Run multi-process nodes for hours/days under churn, partitions, disk pressure, and restarts.

### 18.6 Add cross-architecture CI

At minimum:

- Linux x86_64;
- Linux aarch64;
- Windows;
- Android build;
- 32-bit compile checks where supported;
- big-endian compile/test if feasible through emulation.

This catches assumptions such as `u64 as usize` and filesystem behavior.

---

## 19. CI, dependency governance, and supply chain

### 19.1 Dependency audit is currently not a real gate

The CI file explicitly states that the RustSec action’s install step is failing due to toolchain incompatibility and that `continue-on-error: true` masks the hard failure.

**Immediate fix:**

- run cargo-audit in a container or toolchain independent of the repository pin;
- fail if the scanner itself fails;
- distinguish scanner failure from advisory findings;
- add a scheduled scan on `main`;
- record accepted advisories with expiry and owner.

### 19.2 Pin all third-party actions by immutable SHA

This is already done in the inspected CI. Continue it across every workflow and automate update review.

### 19.3 Add SBOM and provenance for every release

Generate CycloneDX/SPDX, dependency licenses, source commit, builder identity, and reproducibility attestations.

### 19.4 Review build scripts and proc macros

Supply-chain review must include `build.rs`, proc macros, native dependencies, downloaded tools, and test-time compilers.

### 19.5 Enforce generated-file freshness

CI should fail if repository maps, indexes, bindings, schemas, or vectors differ from regenerated output.

---

## 20. Documentation, governance, and repository maintainability

### 20.1 Some audits are stale relative to repository growth

The memory-safety audit states it checked 22 crates and 40 external dependencies, while the current repository map shows roughly 71 crates. The audit remains historically useful but should not be presented as current whole-workspace coverage.

**Improvement:** every audit document should include:

- exact commit SHA;
- crate count;
- dependency count;
- tool versions;
- expiry/revalidation trigger;
- superseding audit link.

### 20.2 Decision-log scale risks obscuring current truth

Append-only history is valuable, but thousands of lines and hundreds of decisions require generated current-state views.

Create machine-readable decision records and generate:

- active decisions by subsystem;
- superseded decisions;
- unresolved follow-ups;
- external gates;
- protocol versions;
- authority-impact map.

### 20.3 “Accepted” status should not precede legitimate adoption

AI-drafted decisions in an unmerged PR should be Proposed or Candidate, not Accepted, unless the repository’s governance process has actually adopted them.

### 20.4 Status documents need automated consistency checks

Detect contradictions such as:

- crate count mismatch;
- shipped feature with open blocker;
- issue marked complete while required follow-up remains;
- outdated test command;
- old toolchain statement;
- decision range mismatch.

### 20.5 Reduce duplicated prose

Long repeated explanations across README, STATUS, BETA_STATUS, decision log, design docs, and crate docs can drift. Keep one canonical maturity statement and link to it.

---

## 21. Legal, abuse, moderation, and operational readiness

### 21.1 Abuse handling is a protocol feature

Public social, messaging, crawling, storage, relays, and search need:

- block/mute/report;
- request isolation;
- rate limits;
- local moderation policy;
- evidence handling;
- appeal/correction;
- child-safety and illegal-content response boundaries;
- operator liability documentation.

### 21.2 Privacy claims need data-flow inventories

For each client and service, document:

- outbound destinations;
- metadata visible to peers/relays;
- local logs;
- crash reports;
- retained identifiers;
- deletion behavior;
- backups.

### 21.3 Incident response and key compromise drills are missing from most prototypes

Create executable runbooks for:

- compromised release key;
- compromised witness;
- malicious relay;
- KEL fork;
- treasury participant compromise;
- dependency vulnerability;
- malicious update;
- corrupted local state.

---

## 22. Concrete refactoring opportunities

### 22.1 Candidate shared crates/services

- `mini-codec`: canonical bounded encoding and transcript support.
- `mini-state-store`: atomic durable state, migrations, locks, checksums.
- `mini-freshness`: replay windows, monotonic pins, trusted checkpoints.
- `mini-clock`: injectable monotonic/wall-clock interfaces.
- `mini-authz`: typed root/device/capability verification.
- `mini-testkit`: deterministic identities, clocks, networks, storage, faults.

### 22.2 Candidate consolidation

Review narrow policy crates and merge those that do not represent independent trust boundaries or deployment units. A crate should usually exist because it owns one of:

- a stable protocol boundary;
- a security boundary;
- a platform boundary;
- an independently deployable component;
- a dependency-isolation boundary.

### 22.3 Introduce verified wrappers

Examples:

```rust
VerifiedDid
VerifiedKelCheckpoint
AuthorizedDevice<C>
RegisteredReplica
FinalizedSettlement
VerifiedRelease
CanonicalObject
FreshNonce
```

### 22.4 Introduce protocol IDs and network IDs everywhere

Every signed durable object should be bound to a network/genesis ID and protocol/version domain to prevent replay between testnet, forks, and future networks.

---

## 23. Recommended implementation plan

### Phase A — Stop false confidence

1. Repair dependency scanning.
2. Mark stale audits with commit scope.
3. Correct overclaims in PR #297 and other cryptographic docs.
4. Add external-gate enforcement to value/crypto activation paths.
5. Separate verified from unverified types in new code.

### Phase B — Shared correctness infrastructure

1. Build canonical codec/transcript crate.
2. Build persistent replay/freshness service.
3. Add historical KEL verification.
4. Add deterministic clock and network test kits.
5. Add golden vectors and fuzz targets.

### Phase C — Identity vertical slice

1. Root creation.
2. Device delegation.
3. Multi-device sync.
4. Rotation/revocation.
5. Witnessed freshness.
6. Recovery after process/device loss.
7. Physical Android/Windows tests.

### Phase D — Network and messaging vertical slice

1. Durable daemon.
2. Authenticated discovery.
3. Relay mailbox.
4. Reviewed session ratchet.
5. Background delivery.
6. Device fanout/removal.
7. Loss/partition/clock-skew testing.

### Phase E — Storage vertical slice

1. Typed assignment.
2. Identity/device-bound seal parameters.
3. Prechallenge commitment.
4. Registration audit.
5. Persistent registry.
6. Ongoing proof windows.
7. Conflict classification and re-audit.
8. Reward eligibility only after external review.

### Phase F — Value and consensus vertical slice

1. Durable consensus WAL.
2. Network simulator safety tests.
3. Finalized canonical state view.
4. Payment admission and settlement integration.
5. Crash/reorg recovery.
6. External cryptography and economics review.

### Phase G — Product integration

1. One desktop/mobile shell.
2. Honest feature readiness checks.
3. Accessibility.
4. Export/import and recovery.
5. Signed reproducible installers.
6. Incident-response drills.

---

## 24. Proposed automated audit commands for a local checkout

Run and preserve outputs by commit SHA:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --all-features --release
cargo deny check
cargo audit
cargo metadata --format-version 1 > audit/cargo-metadata.json
```

Static searches:

```bash
rg -n 'TODO|FIXME|HACK|XXX' crates tools .github
rg -n 'unwrap\(|expect\(|panic!\(|unreachable!\(|todo!\(|unimplemented!\(' crates
rg -n ' as usize| as u32| as u64' crates
rg -n 'SystemTime|UNIX_EPOCH|Instant::now|thread::sleep' crates
rg -n 'std::fs|File::create|OpenOptions|rename\(|remove_file' crates
rg -n 'Command::new|TcpListener|TcpStream|UdpSocket' crates
rg -n 'sign_message|verify_message|Signature|SigningKey' crates
rg -n 'InMemory|Ephemeral|Default' crates
```

Recommended tools:

- `cargo-fuzz`
- `cargo-mutants`
- `cargo-nextest`
- `cargo-llvm-cov`
- `cargo-semver-checks`
- `cargo-geiger` for dependency awareness only
- Miri on selected pure-Rust crates
- Loom for concurrent state
- proptest for codecs/state machines
- network simulation with deterministic fault injection

---

## 25. Final conclusion

Mininet’s strongest characteristic is that it understands many of its own dangers. The documentation repeatedly distinguishes prototypes from production, identity roots from humans, evidence from punishment, transport encryption from privacy, and code review from legitimate governance. That discipline should now be applied to repository growth itself.

The project does not primarily need more concepts. It needs fewer, deeper, fully integrated guarantees:

- one authoritative freshness model;
- one canonical encoding model;
- one durable state model;
- one real network service;
- one reviewed private-session protocol;
- one authenticated storage lifecycle;
- one crash-safe consensus/value path;
- one enforceable external-audit gate.

The best near-term measure of progress is not crate count, decision count, issue closure, or unit-test count. It is whether two independently installed devices can execute a complete workflow, survive hostile conditions and restarts, and produce portable evidence that another implementation can verify from the same canonical state.

Until those vertical paths exist and are independently reviewed, the repository should continue to describe itself as a sophisticated, extensively tested protocol research and integration prototype—not a production-safe autonomous network.
