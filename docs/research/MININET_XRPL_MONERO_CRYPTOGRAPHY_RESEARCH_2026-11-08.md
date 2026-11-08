# MININET: Cryptography Reuse and Interoperability Research

## What to learn from XRP Ledger and Monero, what to implement, and what not to copy

**Technical design proposal | 8 November 2026 | Research-only / not a release authorization**  
**Repository reviewed:** `mininet-labs/Mininet`, default branch `main`, observed head `d307c2752fec14479ca2db8ec1ea83446f182754`.  
**Audience:** founder, Rust maintainers, independent cryptographers, protocol reviewers, wallet/consensus engineers.  
**Scope:** cryptographic primitives, privacy composition, transaction authorization, custody, canonical state transitions, replay protection, and optional asset-rail integration.  
**Decision status:** recommendations for a governed design review; no source changes, deployment, external audit, or security certification are claimed.

## Executive decision

**Yes: reuse, specialize, and verify established cryptography. No: invent a new cipher, change proof mathematics casually, or assume a famous cryptocurrency's security automatically transfers to Mininet.** A published protocol can be implemented with Mininet-specific serialization, domain separation, network IDs, recipient scanning, wallet UX, and consensus hooks. Those adaptations must be specified, audited as a whole, and compatibility-tested. Studying and clean-room-reimplementing open prior art is a legitimate engineering method; reusing maintained libraries usually creates a smaller and safer audit surface.

The review's central discovery is that Mininet is not starting from zero. It has already implemented CryptoNote-style one-time destinations, Ristretto-based MLSAG, Pedersen commitments, Bulletproof range proofs, shielded claims, an experimental `PrivatePaymentV3`, and a replacement FROST library custody path. Therefore the near-term priority is **a single, consensus-enforced, audited V3 private-value path**, not a new parallel payment engine. The hard question is no longer simply whether Mininet can hide an amount; it is whether a *malicious proposer, wallet, observer, or peer* can cause an invalid hidden transfer to be finalized, identify a spender, or make an owner believe an unfinalized offline promise is money. [M2-M8]

The recommended program is:

1. **P0 / Canonical shielded validity:** Make `PrivatePaymentClaimV3` fully verified by the actual validator/consensus transition, with canonical claim bytes and digest, membership, unique key images/nullifiers, range and conservation proofs, deterministic fees, and atomic state application. V3 is currently additive and not wired into the consensus verifier. [M3-M5]
2. **P0 / Migrate, then retire legacy:** Coordinate an explicit, governed version/migration decision; keep old formats from silently crossing generator bases; retire the ordinary transparent path before real private-value production, subject to approved migration. [M2, M3, M9]
3. **P0 / Independent applied cryptographic audit:** Commission a specialist review of V3 as a *complete composition*, plus FROST DKG/signing, wallet scanning, serialization, secret nonces, weak-device resource limits, and actual release revision. Existing reports and tests do not collectively close the release gate. [M6, M7, M9]
4. **P1 / Wallet privacy and recovery:** Harden view-only scanning, selective disclosure, decoy availability, transaction fingerprint reduction, session privacy, and never-equate-offline-with-final state. Do not compromise Mininet's identity root or introduce issuer-controlled universal decryption. [M1, M2, M8]
5. **P2 / Optional XRPL/XMR interoperability:** Start read-only (strict canonical RPC verification, headers/receipts, event monitoring) at a separately bounded edge. Cross-chain custody, bridge-minting, and externally meaningful settlement claims require an independent trust and economic analysis, not an import of XRPL's witness/door-account pattern. [X5]

### Decision scorecard

| Capability or candidate | Present Mininet state | Recommendation | Why |
|---|---|---|---|
| Ristretto one-time destinations | Implemented, V3 variant exists; composition unaudited | Keep and review | Monero-like receiver unlinkability without adopting Monero wire format |
| MLSAG two-column private spends | V2 and V3 versions present | Secure V3 now; evaluate CLSAG later | Changing signature families before completing chain validity increases migration risk |
| Bulletproof range proofs | Hand-written older path; `bulletproofs = 5.0.0` V2 library path | Finish migration to reviewed library-backed V3 | Generator basis and transcript are consensus-critical |
| Public fees, visible transaction shape | Present residual metadata | Quantify and mitigate | Cryptographic privacy is not anonymity against traffic analysis |
| FROST signer/DKG | `mini-custody` wraps `frost-ristretto255 = 3.0.0`; legacy hand-written functions feature-gated | Prefer maintained library + durable nonce journal | An ordinary FROST signature is not XRPL multi-signing or Monero signing |
| XRPL multi-signing and tickets | Not appropriate to copy literally into private ledger | Borrow typed authorization/reservation ideas | Public account sequences and signers would link Mininet spenders |
| XRPL XLS-96 confidential MPT | Final specification updated 22 Oct 2026; activation not established by this review | Research reference only | Account-linked, issuer/auditor-mirrored, optional confidentiality, different group |
| Monero CLSAG/Bulletproofs+ | Mature Monero examples, not Mininet-compatible bytes | Benchmark and evaluate through versioned RFC | Reduce proof costs only with reviewed migration |
| Monero FCMP++ / CARROT | Monero upstream milestone still has open work | Watch only | Do not describe as a currently deployed and proven replacement |
| Cryptographic proof of unique humans | Not achieved by XRP, Monero or Mininet | Separate research gate | Cryptographic pseudonyms/nullifiers alone do not prove distinct people |

---

## 1. Research method, evidence, and authority boundary

**Source hierarchy.** (a) Current Mininet crate and policy files read from its connected GitHub repository; (b) protocol-owner technical specifications for XRPL, Monero and FROST; (c) peer-reviewed or maintained cryptographic implementation documentation. Source links are indexed in section 14. The observed `main` SHA is a source-review reference, not an independently reproduced build. No `cargo` execution, code fuzzing, real hardware testing, formal proof, live-ledger signing, or outside specialist audit was performed for this document. Statements of implementation status are evidence-based but not a security certification. A few repository documents are historical and disagree in emphasis; `docs/STATUS.md`, current code, and explicit release gates take precedence over old optimistic decisions. [M2, M8, M9]

**Authority.** `AGENTS.md` and the canonical governance documents define who may authorize changes. This report does not activate a policy, close an audit, establish a new consensus rule, grant treasury access, or substitute an AI review for human approval. Any code PR must follow approved founder/governance process, record a new decision, preserve append-only history, and check frozen invariants. [M1, M10]

**Constitutional constraints with direct cryptographic consequences:**

- **P1 / Directive 16:** wealth, deposits, fees, treasury shares, validators' balances, and token amounts must never buy voting or constitution-changing power. Keep the value-to-governance dependency boundary intact.
- **P2 / personhood:** a `did:mini` root is not a scientifically verified unique human. Neither a ring key image nor an XRPL account is a personhood credential.
- **M1 / M2 / M3:** money has exactly one canonical outcome; CRDT merges are forbidden for spendable value; offline transfers are signed pending claims, not final settlement; conflicting spends resolve only through canonical ordering.
- **A1 and release checklist:** outside applied-cryptography review is a hard go/no-go requirement for real value, custody, personhood and relevant consensus paths.
- **Identity and privacy:** private keys remain device-controlled, no forced raw biometric sharing, no compulsory universal disclosure, no essential hosted identity authority; weakest real device and long-term recoverability are design constraints. [M1, M9]

### The safe boundary for reverse engineering

| Work type | Allowed approach | Risk classification |
|---|---|---|
| Study Monero's open algorithms, read XRPL specs, port test vectors | Cite version/commit/spec and reimplement or invoke independently reviewed protocol as written | Acceptable research/implementation route |
| Change domain labels, suite IDs, wire encoding, application policy | Specify complete bytes/transcripts, version every rule, audit anti-replay and cross-protocol use | High review burden, but not a novel primitive |
| Select a maintained published proof/signing library | Pin audited dependency, review API usage, RNG, generators, serialization and supply chain | Preferred where compatible |
| Re-create a new ring proof by splicing MLSAG, CLSAG and novel challenge equations | Do not do so | Prohibited new cryptographic construction |
| Import Monero point encodings into Ristretto or XRPL secp256k1 by changing a type alias | Do not do so | Mathematically/wire incompatible and unsafe |
| Expose a universal decrypt key, mandatory issuer backdoor or single bridge operator | Reject unless already constitutionally legitimate, which these designs are not | Conflicts with privacy/sovereignty constraints |

Licensing is a separate gate from mathematical permission. Researching open protocols does not automatically grant the right to copy every project's source code into CC0 Mininet. Prefer documented API/library use or clean-room implementation with source attribution; review each dependency's current license, patents, notices, export/regulatory duties, and source provenance before merging.

## 2. XRP Ledger: exactly which ideas transfer

XRPL is fundamentally an account-based, publicly addressable settlement system, not a Monero-equivalent privacy system. Its standard signing algorithms are secp256k1 ECDSA and Ed25519 EdDSA; an XRPL Ed25519 public key carries a leading `0xED` byte in the ledger's 33-byte identifier representation. XRPL seed derivation, address checksums, transaction serialization, signing prefixes and validation rules are **not** the same as ordinary Mininet Ed25519/Ristretto calls. If Mininet ever signs a genuine XRP Ledger transaction, it needs an isolated, XRP-format transaction signer/verifier using the exact XRPL standard; it must not reinterpret a Mininet FROST-Ristretto signature as valid XRP signature material. [X1]

### 2.1 Transaction authorization and key isolation

XRPL accounts can use master keys, regular keys, and multi-signing signer lists with quorum/weights. The useful lesson is **separate who can authorize an exact operation from who can own an identity or govern the network**. Mininet already has scoped DID delegations and `mini-custody` FROST; an improved integration can present the human with an exact typed proposal, make signers verify network/asset/amount ceiling/recipient commitment/expiry and canonical request hash, and durably reserve nonces before signing. Signature shares or a completed signing session must not, by themselves, create governance status. [X2, M5]

**Do not copy XRPL signer weights into Mininet governance.** A signer list is an account-authorization mechanism, not the project's equal-person legitimacy mechanism. In the money layer, a policy may require 3 of 5 independent custody keys without making those five parties holders of Mininet voting power.

### 2.2 Tickets and stale-state prevention

XRPL Tickets reserve sequence slots so an account can prepare or multi-sign an operation while other transactions continue. In Mininet, a *public per-payer sequence* would undo much of the shielded transaction model; the legacy transparent settlement format already demonstrates that leak. Import the abstract operational requirements instead: unique request identifiers, exact-claim authorization, crash-safe local reservations, duplicate-detection, bounded pending lifetimes, and canonical execution receipts. Use V3 key images/nullifiers as the spend-once primitive, not an XRPL-style public account sequence. A local nonce journal is not canonical finality. [X3, M3]

### 2.3 Confidential MPTs (XLS-0096, updated 22 Oct 2026)

XRPL now documents an XLS-96 **final specification** for optional confidential transfers of multi-purpose tokens using secp256k1 EC-ElGamal ciphertexts, compact sigma proofs, Bulletproofs range proofs, mirrored ciphertexts for issuer/optional auditor, and an inbox/spending balance split with a version counter. A final specification is not evidence that the `ConfidentialTransfer` amendment is currently active on the specific ledger used for integration; activation must be checked against amendment/voting status at implementation time. [X4]

This provides valuable **verification design lessons**: proof-of-plaintext consistency between encrypted representations, explicit post-transfer range constraints, avoiding proofs becoming stale when incoming credits arrive, and rejecting state transitions where the issuer/global supply accounting diverges. Its model differs from Mininet's shielded UTXO-style notes and RingCT construction: sender and receiver account linkage remains visible, confidentiality is elective, and issuers/auditors hold decrypt-capable mirrored keys. Its issuer clawback/freeze policy would be unacceptable as a universal MINI-currency primitive. Mininet should study these mechanisms as comparator tests, not import their trust model.

### 2.4 Escrow, bridges and witnesses

XRPL's hash/time-conditioned escrow suggests bounded, typed, deterministic *conditions* for future Mininet transfers. It does **not** authorize offline finality or additional administrative signing authority. XRPL cross-chain bridge designs use door accounts and independently operated witness servers to attest commits and releases between otherwise separate ledgers; witnesses are a trust and censorship/collusion surface, not a mathematical proof of another chain's finality. If Mininet eventually supports external value, the first release should observe settlement and attach independently checked evidence, not mint irredeemable MINI or claim trustless XRP/XMR deposits. [X5, X6]

### 2.5 XRP-specific recommendations

| XRPL mechanism | Mininet adaptation | Reject/avoid |
|---|---|---|
| Local sign + canonical transaction encoding | Dedicated optional `xrpl-observer` and `xrpl-signer` edge services with validated main/test network IDs | Key/seed format conflation, signing arbitrary bytes |
| Master/regular/multi-sign role distinctions | DID capabilities and FROST ceremony policy bound to exact typed actions | Signer weights as governance power |
| Tickets and sequence | Durable exact-request journals, fee policy IDs, claim IDs, replay negative tests | Visible account sequence attached to shielded MINI spends |
| Confidential MPT proofs | Test patterns for proof/ledger atomicity, staleness, global supply consistency | Universal issuer/auditor decrypt access; compulsory clawbacks |
| Bridge attestations | Independent evidence formats + separate watcher quorum risk analysis | Single trusted bridge/door account as protocol core |

## 3. Monero: exactly which ideas transfer

Monero is the closer comparator for Mininet's private payments. The deployed architectural family is CryptoNote-style recipient stealth addresses, RingCT Pedersen commitments, linkable ring spend proofs, decoy ring members, and range proofs, plus network-layer relay controls. Monero moved from MLSAG to CLSAG for cheaper signatures and adopted Bulletproofs+ range proofs. Mininet already uses related ideas, but **not Monero's exact proof suite or wire format**: its group is Ristretto255; Monero's on-chain curve encoding, hash functions, transcript labels, transaction format and consensus upgrades are distinct. Shared words such as "ring", "key image" and "Bulletproof" do not imply interoperable bytes. [R1-R3, M3]

### 3.1 Recipient privacy: one-time addresses and view/spend separation

A typical CryptoNote-style construction gives the recipient a public view key `A = aG` and spend key `B = bG`. A sender chooses a secret `r`, publishes ephemeral `R = rG`, and forms a one-time output key of the approximate form `P = H_s(rA || output_index)G + B`. Recipient scanning uses `aR = rA` to recognize outputs without broadcasting the spend secret `b`. The spender later derives the one-time private key. This is *mathematical intuition*, not the wire formula for Mininet V3: its actual Ristretto scheme has versioned domains, tag rules and canonical encoding that auditors must review from source. [R2, M3]

Mininet should retain separate spend and view authority, make view keys expressly non-signing, test view tag rejection and false positives, keep memo openings encrypted to the intended output, and avoid leaking recipient addresses through metadata or services. The disclosure interface must never imply a view key identifies every account controlled by a person.

### 3.2 Hidden amounts and conservation

A Pedersen commitment over an appropriately defined prime-order group looks like `C(v,r) = v*H + r*G`, where the amount `v` is hidden by secret blinding `r`. The verifier checks that `sum(C_input_pseudo) - sum(C_output) - fee*H` equals the group identity, provided the input/output blinding terms have been made to cancel. This is **not sufficient by itself**. The proof must also show inputs actually authorized real prior outputs, output amounts are bounded nonnegative integers (range proofs), key images/nullifiers are unused, every referenced ring member/commitment is canonical, and the entire transaction is bound to the exact chain and fee policy. A malicious prover who can introduce negative or unbounded amounts, select inconsistent commitment generators, or reuse a key image can create money or steal value behind otherwise opaque ciphertexts. [R1, M3]

### 3.3 Ring signatures and honest anonymity statements

MLSAG and CLSAG provide related anonymous ownership proof semantics, but CLSAG is a distinct protocol with distinct transcripts and security analysis. Ring size alone does not equal practical privacy: recognizable output-age patterns, known decoys, fees, multiple inputs, timing and wallet fingerprints can sharply shrink an effective anonymity set. Mininet's V3 has a fixed 16-member ring and an OSPEAD-inspired distribution; the implementation itself admits calibration/units questions and the roadmap lacks real spending-age data. A statistically plausible ring is not a proof of anonymity. [R3, M3]

**Recommendation:** finish the audited V3 MLSAG implementation first. Build reproducible benchmark vectors that compare the current 16-member V3 output/verification cost with independently reviewed CLSAG candidates. Consider a CLSAG-v4 proposal only if an independent security review demonstrates lower weak-device expense and a safe whole-chain version transition. Never silently change a signature scheme, generator or challenge hash under an existing suite ID.

### 3.4 Bulletproofs versus Bulletproofs+

Monero's deployed Bulletproofs+ proof system is an optimization of an established family; Mininet's new library-backed `bp_range_v2` uses the published Bulletproofs design through `bulletproofs = 5.0.0`, not Monero's Bulletproofs+ wire proof. The Mininet source explicitly warns that the older hand-written generator basis and the library-backed basis are incompatible and that V3 is not yet wired into consensus. Keep each group's generator definitions and proof transcripts identical across prover, verifier, wallet, historical replay and state snapshots. [R1, M3]

A Bulletproofs+-style optimization should be a **future migration research item**, not a reason to defer the more immediate V3 chain validity gate. The same applies to Monero's proposed FCMP++ / CARROT evolution: the official upstream FCMP++ milestone still contains open integration/review work as observed in November 2026. Do not represent it as already deployed or audited for Mininet. [R4]

### 3.5 Privacy beyond mathematics

Monero's own privacy model still needs transaction relay techniques and cautious remote-node behavior. A shielded payment sent immediately after a public post from a stable IP can be correlated even if its ring proof is perfect. Mininet already has relay/transport privacy work; implementation claims should distinguish: **cryptographic amount confidentiality**, **recipient privacy**, **sender set-membership anonymity**, **network-origin privacy**, and **inference resistance**. Only the capabilities actually measured and audited should be advertised.

---
## 4. Current Mininet implementation: evidence inventory and failure surfaces

The table below separates existence of code from security of the *end-to-end path*. "Implemented" means a source module was inspected, not that its mathematical correctness or release readiness has been established. Path names are canonical repository paths as observed at the reviewed `main` state. [M2-M8]

| Existing component and evidence | What it currently does | What remains for a release-grade guarantee |
|---|---|---|
| `mini-crypto`: `crates/mini-crypto/Cargo.toml` | Ed25519/X25519, ChaCha20-Poly1305, HKDF-SHA256, BLAKE3/SHA-256, OS randomness, ML-DSA-65 support | Version/suite-aware key separation, audited integration, compatibility and secure-storage validation |
| `mini-value`: `stealth_impl.rs`, `stealth_v3.rs` | Receiver one-time destinations, view secrets and tags, versioned V3 derivation | Formal/cross-implementation privacy review, scanner/tag timing and false-positive tests |
| `mini-value`: `mlsag.rs`, `mlsag_v3.rs` | Anonymous input authorization, pseudo-output commitment relation and network-bound key images | Independent ring proof and challenge transcript audit, attack-resistant ring member selection |
| `mini-value`: `bp_range.rs`, `bp_ipa.rs` | Older bespoke Bulletproof/inner-product proof path | Decommission from newly accepted claims after governed cutover; preserve historical replay vectors |
| `mini-value`: `bp_range_v2.rs` | Library-backed 64-bit range proof, exact 672-byte encoded proof in current single-output configuration | Integrate into canonical V3 path; verify generator derivation against review; benchmark weak devices |
| `mini-private-payment`: `claim.rs`, `claim_v3.rs` | V2 live consumers plus V3 model with fixed ring, canonical bytes, separate memo/signing/claim-ID digests | V3 validator path and state migration; deterministic fee schedule finalization; replace V2 callers |
| `mini-private-payment`: `decoy.rs`, `scan.rs`, `memo.rs`, `memo_v3.rs` | Decoy selection, receiver scanning and encrypted output notes | Test calibration, full-output-set availability, key handling, failure/noise/metadata profiling |
| `mini-shielded-verify`: `lib.rs` | Separates cryptographic verification adapter from consensus and buffers structurally decoded **V2** evidence | V3 verifier dispatch, durable evidence gossip/retention, anti-replacement safeguards, DoS model |
| `mini-execution` / `mini-consensus` | Canonical chain application and finality; existing nullifier/claim-validity remediation | Independent full V3 proof acceptance before every state mutation, replay/snapshot test with hostile proposers |
| `mini-custody`: `Cargo.toml`, `signing.rs`, `session.rs` | `frost-ristretto255`-backed dealerless DKG/signing wrapper with crash-safe nonce journal path | Integration of audited ceremony policy, rotation, share recovery, hostile/coerced signer casework |
| `mini-treasury`: `Cargo.toml` | Historical hand-derived FROST signing/DKG retained behind explicit legacy feature flags | Guarantee legacy signing cannot be reached by production builders; negative compatibility tests |
| `did-mini` / `mini-uniqueness` / `mini-pq-anchor` | Device-root identity, revocation and personhood-signal work; PQ anchoring | Unsolved unique-person verification; robust key-era and pre-break PQ transition; independent review |

**Important repository-specific nuances:**

1. **V3 exists; it is not the live consensus payment path.** `claim_v3.rs` explicitly says V3 is not wired into `mini-shielded-verify`, `mini-execution`, or consensus. The V2 pool's `insert` currently decodes `PrivatePaymentClaim`, not `PrivatePaymentClaimV3`; mere evidence admission is structural and not proof verification. No amount of confidence in V3 algorithms can make it enforce canonical state if validators never execute their verifier. [M3, M4]
2. **There are different commitment bases.** The V2-named Bulletproofs library module and the previous `bp_range` module do not share a generator basis. The names "V2" range and "V3" claim describe *different versioning layers*. A proof under one basis cannot balance with points committed under the other. You must migrate *proofs, signatures, balances, one-time outputs, memos and canonical state* together. [M3]
3. **Evidence storage is not automatic archival.** `ClaimEvidencePool` budgets are per-node operational guards, not consensus rules. The source describes no durable gossip/retention policy; the same digest can be replaced with different admitted bytes before complete verification. Hash addresses and canonical record identity must be bound to complete exact bytes, not merely a subset of fields. [M4]
4. **FROST migration is underway, not a blanket audit pass.** Production-target library wrappers and nonce burn records materially reduce hand-rolled risk; nevertheless the signing ceremony, transport, persisted shares, hostile-party behavior, crash boundaries, guardian independence and final app call sites still need outside examination. [M5, M6]
5. **Past audit evidence is mixed.** `PR332_REMEDIATION_STATUS.md` marks shielded verification F-07 **PARTIAL**, and the binary release checklist remains **NO-GO**. No local build or external source analysis is claimed here. Historical statements saying "complete" should not be allowed to override a later explicit gate. [M7-M9]

### Gap severity

- **Critical before any real settlement:** proof authenticity enforced at consensus; spend-before-existence rejection; uniqueness of key image; fee and supply correctness; all-or-nothing application; independent external audit.
- **High before broad privacy claims:** V2/V3 migration consistency; decoy and view-tag leakage; durable evidence availability; timestamp/shape/IP linkability; wallet secret recovery.
- **High before custody:** signer policy, FROST nonce uniqueness across crashes and backups, share rotation, DKG complaint handling, irreversible withdrawal confirmation.
- **Separate unsolved scientific gate:** unique-human/personhood verification; no payment cryptography is a shortcut.

## 5. Cryptographic walkthroughs: why the pieces work together

The following equations illustrate established prior art. Their exact hash-to-point definitions, transcript encodings and subgroup choices must come from the *versioned Mininet spec* and selected published libraries, never from these compact formulas alone.

### 5.1 Pedersen hidden-value conservation

Let `G` and `H` be independently derived generators of a prime-order group, with no party knowing a discrete-log relation between them. Commit to value `v` with random blinding `r`:

`C = v*H + r*G`

For a private transfer with two hidden inputs worth 70 and 30 MINI units, outputs worth 63 and 35, and fee 2:

`70 + 30 = 63 + 35 + 2`

The verifier does not see the amounts. It checks the point equation:

`C_in1' + C_in2' - C_out1 - C_out2 - C_fee = identity`

where the input pseudo-commitments use fresh blindings, and the blinding sums have been balanced. Without an authenticated relation from each pseudo-input back to a genuine spendable ring member, a stranger could invent the point `C_in1'`. Without output range proofs, an invalid negative or overflowing amount could satisfy modular arithmetic. Without a canonical-output-membership check, the ring proof might cover made-up history. Without no-reuse key images, the same valid note could be spent twice. **These conditions must all be true at the state transition, not just in a UI test.** [R1, M3, M4]

### 5.2 Anonymous ownership and spent-once key images

With secret one-time key `x` and its public one-time key `P = xG`, a linkable-ring system derives a key image of the general form `I = x*H_p(P)`. The ring proof reveals neither which member's secret key is known nor the amount, but repeated spends of the same key yield the same image under the specified domain. Mininet V3 specifically domain-separates the key-image base by suite ID, network ID and the one-time key; this reduces cross-network/cross-suite replay. A ledger maintains the set of accepted key images and rejects a second use. [M3]

A proof verifier must simultaneously validate the **ownership column** and **commitment-difference column** of Mininet's two-column MLSAG. Checking only the key-image uniqueness is inadequate; otherwise a proposer could burn another owner's apparent nullifier without possessing a valid signature. This is why complete signed claim bytes, not a proposer-supplied list of images, must be in the canonical consensus acceptance path.

### 5.3 View-key recognition

A sender's fresh ephemeral `R` lets the recipient's scan key derive a shared secret and recognize a one-time output without exposing the long-term address. A cheap view tag may filter likely outputs; it is not a proof that the output belongs to the recipient. Full one-time-key recognition, memo authentication and commitment-opening checks must follow. Scanning from a remote peer risks revealing the real output's membership/interest pattern through access requests. Local scanning or privacy-preserving retrieval is preferable, measured against weak-phone constraints. [R2, M3]

### 5.4 XRP Ledger's EC-ElGamal confidentiality is a different model

In additive ElGamal intuition, with public key `Y = xG`, a plaintext `m` represented as a group point `mH` can be encrypted as the pair `(rG, mH + rY)`. Zero-knowledge equality/consistency and range proofs must prove the ciphertexts, account balances and all mirrored holder/issuer/auditor copies correspond to the same valid amount. On the XRPL confidential-MPT design, validators update encrypted **account** balances and version numbers; on Mininet, validators accept and consume **one-time notes**. Translating one model into the other without a complete new, audited statement would be a new protocol design, not a harmless parameter change. [X4]

### 5.5 FROST quorum is not a new signing scheme

FROST builds one ordinary group signature using secret shares from a threshold of participants, with fresh hiding and binding nonces. The final verifier need not learn who supplied the shares. That is excellent for protecting a treasury against compromise of a single device; however a signature alone does not prove that the signers were independent people, that the transaction was legitimate under policy, or that an on-chain ledger finalized a payment. The *ceremony and policy journal* must bind every share to the exact typed action and current signer manifest. Never reuse a FROST signing nonce pair, including after reboot, restored snapshot, cancellation or failover. [F1, M5]

### 5.6 Simple threat example: why V3's `claim_id` differs from `signing_digest`

Suppose a cache is keyed only by the unsigned payload hash `D`. Two encodings of the same unsigned transfer have different signature bytes `S1` and `S2`. They can share `D`, allowing cache replacement or disagreement over which exact bytes were included. Mininet V3 has separate domains for `memo_context_digest(output_index)`, `signing_digest()` and `claim_id()`, with `claim_id` hashing the complete encoded claim including signatures. This is a sound structural design direction; the remaining work is to use it *everywhere* that claims are stored, gossiped, ordered, replayed and verified. [M3]

---

## 6. Proposed implementation architecture

Keep the existing constitutional and dependency boundary. The diagram below is a *proposed flow*, not a claim that all arrows are currently production-wired.

```text
User / device wallet
  |  derive one-time destination; local output scan; typed payment intent
  |  V3 build (ring members + pseudo-commitments + fee policy + range proofs)
  v
Canonical PrivatePaymentClaimV3 bytes
  |  exact-byte claim_id; bounded admission; authenticated evidence gossip
  v
mini-shielded-verify (V3 version dispatch)
  |  cryptographic verify: range, MLSAG, memo/transcript, fee
  |  referential verify: every ring member/commitment in canonical UTXO set
  v
mini-execution validated transition
  |  reject duplicate nullifiers/images; enforce global fee/supply invariants
  |  atomic consume + create; canonical sorted deterministic digest
  v
mini-consensus / canonical finalized block and QC
  |  independently verify accepted evidence; snapshot + historical replay
  v
Wallet reconciles Finalized only from canonical chain evidence

Separate optional external edge (never the governing chain):
XRPL watcher / Monero watcher --> attested observations --> non-authoritative
reconciliation receipts --> optional separately audited custody/bridge service
```

**Architectural invariant:** consensus must agree on a pure validity predicate derived from exact block bytes and canonical state. The validator's local evidence pool may accelerate receipt of full claim bytes, but lack of a required claim cannot be interpreted as proof of validity. Consensus must refuse a transition unless all required evidence is available and verified; a node behind on archival state must catch up and revalidate, never accept a proposer's summary as authority. The value/voice wall can still be enforced with a dedicated adapter crate; *independently checking* a block's value-bearing facts does not mean governance vote weights depend on value.

### 6.1 Proposed V3 acceptance pipeline: explicit ordering

1. **Decode and canonicalize:** bound raw length; reject unknown version/suite, malformed scalar/point, duplicate ring member, unsorted ring, duplicate input, duplicate output encoding, trailing bytes and noncanonical reserialization.
2. **Bind context:** reject wrong `network_id`, fee policy ID, algorithm suite, chain era, unsupported version or impossible resource charge. Verify `claim_id == H(domain || exact_claim_bytes)` and that all signatures bind the correct `signing_digest`.
3. **Check membership:** for every ring member and amount commitment pair, require canonical membership under the correct historical snapshot/UTXO commitment set. Invalid/missing decoys are not acceptable alternatives.
4. **Verify proofs:** each MLSAG V3 proof, unique key image, output range proof, value conservation including deterministic fee and memo binding if memo integrity is consensus-critical.
5. **Check current state:** ensure none of the key images has already been consumed; reject inflation and unknown/unspendable issuance state; enforce atomicity for all inputs and outputs.
6. **Apply atomically:** compute exact transition, state root and accounting updates only if every check succeeds; finalize only behind valid BFT QC. Reorg and catch-up rules must revalidate the same predicate with the same bytes.
7. **Persist and reconcile:** durable evidence, canonical claim bytes, nullifier indexes and state roots survive crash and replay; wallet marks pending/accepted/final based on canonical chain, not transport receipts.

### 6.2 Consensus compatibility and migration

Do not silently make V3 mean V2. Write a new normative migration decision with a **bounded activation height or explicitly prelaunch coordinated state reset**, strict old-format policy, full state snapshot/UTXO migration rules, and prohibitions on ambiguous mixed-generator claims. If no live monetary state exists, the safest option may be a clean prelaunch version cutover after preserving audit vectors. If historical V2 claims have been accepted anywhere, the procedure must preserve all outstanding ownership and old claim verification under its original exact suite, even if creation of new V2 claims is disabled. An owner-visible wallet migration and recoverability drill are separate acceptance gates.

A migration PR must name: first accepted V3 version/height; which historical V2 bytes remain verifiable; whether old rings can include old-basis outputs; whether withdrawal/reissue is required; how canonical snapshots and claim evidence are transformed; how wallets detect unsupported chains; how deterministic replay before/after the cutover is proven. A reviewer should be able to answer *for every byte format*: can it be created, included, spent, scanned and replayed? [M3, M9]

### 6.3 Prevent unbounded data and work amplification

Before expensive cryptographic verification, enforce hard structural byte/count and per-peer operational budgets. V3 sets a `65,536`-byte upper claim limit and up to 16 inputs/16 outputs with fixed 16-member rings, but those alone do not bind total CPU, memory copies, concurrent proof requests, output-set snapshot access or historical evidence retention. Introduce per-peer backpressure, bounded verification workers, invariant-safe duplicate handling, deterministic consensus validity separate from local admission, and measured worst-case verification on a target low-memory Android device. A hostile peer should never be able to force thousands of expensive failed range proofs for free. Limits that can change the consensus verdict must be agreed protocol rules; local budgets must only delay or refuse local admission, not turn a valid state transition into permanent disagreement.

---
## 7. Rust code examples and cryptographic integration patterns

**Classification of examples.** Examples A and B call actual named repository APIs (subject to the checkout's exact visibility and dependencies). Example C is a self-contained application-level Ed25519 verification sketch using an existing published library. Example D is intentionally *pseudocode* because V3 has not yet been connected to consensus and inventing interface names would misrepresent current functionality. Example E is a test-pattern specification. No example in this document was compiled or audited in this research session; none is permission to move real assets. Avoid copy/pasting any sketch directly into a production signer.

### Example A - use the existing, library-backed 64-bit Bulletproof

The repository exposes `random_scalar_bytes`, `prove_range_v2_from_bytes` and `verify_range_v2` from `mini-value`; the range function returns `Option<Result<...>>` so the caller must handle **both** malformed canonical blinding material and a proving/entropy failure. This example exercises a pure proof round trip; it does *not* establish that a transfer spends a real note or balances a claim. [M3]

```rust
// In a crate depending on `mini-value` at the reviewed API revision.
use mini_value::{
    prove_range_v2_from_bytes, random_scalar_bytes, verify_range_v2,
};

fn make_and_check_range_proof() -> Result<(), Box<dyn std::error::Error>> {
    // Generate canonical Ristretto scalar bytes, not raw [u8; 32]
    // interpreted directly as a scalar.
    let blinding = random_scalar_bytes()?;
    let (commitment, proof) = prove_range_v2_from_bytes(1_000, &blinding)
        .ok_or("noncanonical blinding")??;
    assert!(verify_range_v2(commitment, &proof));

    // A different commitment must not be accepted for this proof.
    let other_blinding = random_scalar_bytes()?;
    let (other_commitment, _) =
        prove_range_v2_from_bytes(999, &other_blinding)
            .ok_or("noncanonical blinding")??;
    assert!(!verify_range_v2(other_commitment, &proof));
    Ok(())
}
```

**Review notes:** Do not log `blinding`; ensure returned keys are eventually zeroized; do not confuse this test with a verified spend. The source derives a custom Ristretto commitment basis (`value_generator_v2`, `blinding_generator_v2`) for V3. An on-chain V2 commitment made under the *old* basis must not be fed into this proof's balance equation. Ensure proof transcripts and generator bases are pinned to suite ID and protocol version, never supplied by an untrusted transaction. Inspect all library error paths and deployment RNG sources. [M3]

### Example B - exact V3 claim identity versus cryptographic validity

`PrivatePaymentClaimV3` exposes `decode`, `encode`, `signing_digest`, and `claim_id`. The first operation validates **structure**; it is not the same as verifying the ring/amount proofs. Its `claim_id` binds the complete encoded claim, including signature bytes. [M3]

```rust
// Illustrative verifier-side preflight, using actual V3 method names.
// The crate's caller must use the existing concrete error types.
use mini_private_payment::PrivatePaymentClaimV3;

fn canonical_claim_id(
    wire: &[u8],
) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let claim = PrivatePaymentClaimV3::decode(wire)?;

    // Ensure the bytes that were received are exactly canonical.
    if claim.encode() != wire {
        return Err("noncanonical claim encoding".into());
    }
    // Only the complete claim ID, not the signing digest, is an
    // unambiguous identity for bytes in the mempool or finalized block.
    Ok(claim.claim_id())
}
```

**Integration caution:** The observed crate root re-exports `PrivatePaymentClaimV3`; check the exact API again at the target commit. `canonical_claim_id` is intentionally only a **preflight**. It must never be named or used as `verify_payment`. The consensus adapter must next call V3's real proof verifier with a canonical output-membership snapshot and current spent-image set, and then atomically apply the transition.

### Example C - a typed approval envelope, modeled on XRPL authorization discipline

The following *new proposed API* is an application-level check suitable for a narrowly scoped **public administrative approval** (for example, an auditor-readable, user-approved bridge withdrawal proposal). It is **not** a new payment signature scheme and **must not** put a stable signer identity into a normal anonymous MINI spend. It uses `ed25519-dalek` already present in `mini-crypto`; its purpose is to illustrate why exact fields and domain separation matter. The project should normally expose this through its own typed `mini-crypto` wrapper rather than duplicate raw library use.

```rust
use ed25519_dalek::{Signature, VerifyingKey};

pub struct BridgeWithdrawalApprovalV1 {
    pub network_id: [u8; 32],
    pub external_chain_id: [u8; 32],
    pub exact_receipt_hash: [u8; 32],
    pub destination_hash: [u8; 32],
    pub max_amount_minor: u64,
    pub not_after_height: u64,
}

impl BridgeWithdrawalApprovalV1 {
    fn signed_bytes(&self) -> Vec<u8> {
        let mut msg = Vec::with_capacity(160);
        msg.extend_from_slice(b"mininet/bridge-withdraw-approval/v1\0");
        msg.extend_from_slice(&self.network_id);
        msg.extend_from_slice(&self.external_chain_id);
        msg.extend_from_slice(&self.exact_receipt_hash);
        msg.extend_from_slice(&self.destination_hash);
        msg.extend_from_slice(&self.max_amount_minor.to_be_bytes());
        msg.extend_from_slice(&self.not_after_height.to_be_bytes());
        msg
    }
}

pub fn verify_bridge_withdrawal_approval(
    request: &BridgeWithdrawalApprovalV1,
    verifying_key: &[u8; 32],
    signature_bytes: &[u8; 64],
) -> Result<(), ed25519_dalek::SignatureError> {
    let key = VerifyingKey::from_bytes(verifying_key)?;
    let signature = Signature::from_bytes(signature_bytes);
    key.verify_strict(&request.signed_bytes(), &signature)
}
```

Changing `destination_hash`, `external_chain_id`, `max_amount_minor` or `network_id` after signature creation invalidates the approval. The verifier still needs to check that the referenced external receipt is **authentic and sufficiently final**, the request has not expired or been executed already, the signer has exactly the relevant capability, and all relevant human/constitutional approval gates exist. A valid signature proves only that some key signed those bytes; it does not make a bridge trustworthy. If implemented, this example requires a formal protocol specification and exact test vectors to avoid evolving from an illustrative envelope into unreviewed authority.

### Example D - V3 consensus bridge that is deliberately pseudocode

`mini-shielded-verify` currently exposes an integration based around V2 `ClaimEvidencePool`. The following names `verify_v3_against_snapshot`, `apply_shielded_v3_atomically` and `read_complete_claim_by_id` are **proposed functions**, not verified repository exports. They specify the behavior a focused PR must implement and test. A validator must not accept the block when the claim or verification context is absent. [M4]

```rust
// PSEUDOCODE - DOES NOT COMPILE against current repository APIs.
fn validate_then_apply_v3(
    state: &mut CanonicalState,
    block: &Block,
    evidence: &EvidenceArchive,
) -> Result<(), InvalidBlock> {
    let mut candidate = state.clone_for_atomic_transition()?;

    for claim_id in &block.v3_claim_ids {
        let exact_wire = evidence.read_complete_claim_by_id(claim_id)?;
        let claim = PrivatePaymentClaimV3::decode(&exact_wire)?;
        require(claim.encode() == exact_wire)?;
        require(claim.claim_id() == *claim_id)?;
        require(claim.network_id == candidate.network_id())?;
        require(candidate.fee_policy_is_active(&claim.fee_policy_id))?;

        // Complete ring, range, fee, memo/transcript and membership checks.
        let proof_result = verify_v3_against_snapshot(
            &claim,
            candidate.canonical_unspent_output_snapshot(),
        )?;
        for key_image in proof_result.key_images() {
            require(!candidate.key_image_is_spent(key_image))?;
        }
        candidate.apply_shielded_v3_atomically(proof_result)?;
    }
    require(candidate.compute_state_root() == block.header.state_root)?;
    state.replace_after_valid_finality(candidate)?;
    Ok(())
}
```

The real design must avoid a state clone if it is too expensive on weak phones (use a bounded deterministic write journal or equivalent atomic staging) and must not finalize before the verified QC. It must make membership evaluation deterministic and define how the evidence archive is carried or fetched so every honest validator can independently verify the exact same full claims. Most importantly, a missing verifier/evidence item is a **failure**, never a `true`/fallback branch. Wire-size, CPU, and memory limits must be documented. A Byzantine proposer must not be able to burn someone's key image by supplying only a hash or nullifier. [M4, M7]

### Example E - FROST: use maintained rounds plus durable nonce reservations

The current library-backed custody module exposes `mini_custody::signing::{round1_commit, build_signing_package, round2_sign, aggregate_signature}`. A simplified example of the safe *logical flow* is below; real app code must use `DurableCustodySigner` rather than `round1_commit` alone because the latter does not provide crash safety. `rand_core::OsRng` should be supplied by the trusted host, never user-specified network bytes. [M5, F1]

```rust
// Illustrative API flow only - actual integration uses durable signer.
// key_package belongs to one participant from a completed, approved DKG.
let mut rng = rand_core::OsRng;
let (nonces, commitment) =
    mini_custody::signing::round1_commit(&key_package, &mut rng);

// The coordinator collects authenticated commitments from the exact
// approved signer set, building a package with the typed request bytes.
let package = mini_custody::signing::build_signing_package(
    commitments_by_signer,
    &typed_and_canonical_action_bytes,
);

// A real signer first checks the exact manifest/policy/action and
// commits the reservation durably. Once consumed, the same nonces
// must never sign again, even if aggregation fails or power is lost.
let share = mini_custody::signing::round2_sign(
    &package, nonces, &key_package,
)?;
```

**Security invariants:** one nonce pair per round-2 attempt; journal committed before round-1 public commitment escapes; reject stale participant rosters, duplicated identifiers, replayed or reordered session messages, divergent signing packages, unrecognized aggregator, expired authority and recovered/restored secrets whose nonce history is uncertain. Signer UI must display the exact human-readable action corresponding to the exact signed bytes. The coordinator is an untrusted relay, not an extra trusted party. A threshold signature being mathematically valid does not prove that the action was approved by Mininet governance.

### Example F - misuse-resistant test checklist as code

These are unit/integration test contracts, not deployable tests. The reviewer should fill them with the repository's actual factories, fixtures and canonical state objects, not mock away the exact boundary the test is meant to cover.

```rust
#[test]
fn same_claim_different_network_is_rejected() {
    // build V3 on network A; try verification under network B
    // assert validation fails *before* canonical state changes.
}

#[test]
fn two_valid_claims_spending_same_output_cannot_both_finalize() {
    // create two independently valid conflict claims with same key image;
    // test both block orders and 4 independent validators' state roots.
}

#[test]
fn proof_validity_cannot_be_replaced_by_pool_presence() {
    // structurally decodable but cryptographically invalid proof bytes
    // inserted into local evidence pool; block MUST be rejected.
}

#[test]
fn generator_basis_mixing_is_rejected_across_v2_v3() {
    // old-basis commitment + V3 range proof / pseudo-output balance
    // fails deterministically. No heuristic fallback or auto-conversion.
}

#[test]
fn reboot_or_snapshot_restore_cannot_reuse_frost_commitment() {
    // reserve nonce -> crash -> restore old local snapshot -> restart;
    // signers refuse if trusted persistent anti-rollback history absent.
}
```

**Special warning on the final test:** a purely local write-ahead log may prevent accidental nonce reuse after a normal crash, but not an adversary who restores *the whole disk and all rollback counters*. The threat model must explicitly address independent monotonic storage, hardware-protected counters or remote signed checkpoints; where such protections are unavailable, the signer may have to refuse after uncertain restore. No demo should promise rollback resistance that an ordinary file cannot supply.

---

## 8. Detailed implementation backlog, ownership and acceptance gates

The proposal deliberately separates **replacement work** from *research work* and from **external permission to deploy**. Each candidate below should become its own narrowly scoped PR (or multiple small PRs) with a decision record, frozen-invariant mapping, exact changed paths, failure-mode evidence and maintained test vectors. No PR should be presented as completing a higher-level gate unless its listed independent evidence is actually present.

### P0-1 - Canonical V3 verifier and atomic execution (highest priority)

**Owners:** value, execution and consensus engineers; independent crypto reviewer.  
**Likely touch points:** `mini-private-payment/src/claim_v3.rs`, `mini-shielded-verify`, `mini-execution`, consensus message/block payload and snapshots, fixture/test harness.  
**Build:** define a versioned canonical block/evidence envelope carrying exact V3 bytes or an availability-proof-backed content-addressed reference with reliable availability before voting; dispatch only to the V3 verifier; check complete ring/commitment membership, range, double-spend and fees; commit all effects atomically; validate and replay from genesis/known canonical snapshots. Preserve P1 dependency architecture by keeping verifier in the existing adapter rather than letting voting code make money policy.  
**Done only when:** four independent validators reject invalid spend, unauthorized key image, wrong basis, missing evidence and fabricated output even when a proposer supports them; accept a funded payment and recipient re-spend; agree on the same state root; restart and recover old/new snapshots bit-for-bit; every finalization test feeds *complete cryptographically verified V3 claims*, not a stubbed boolean.  
**Stop signal:** any path can finalize a proposer-declared image before proof verification.

### P0-2 - Governed V2/V3 cutover and transparent-path retirement

**Owners:** founder/governance decision maker, wallet/value engineers, audit lead.  
**Touch points:** `mini-value`, `mini-private-payment`, `mini-bounty`, `mini-contribution`, `mini-engagement`, settlement adapters, wallet account state, `docs/DECISION_LOG.md`, roadmap and status.  
**Build:** formal conversion/migration decision for legacy claims and basis-specific outputs; single accepted new-transaction suite; explicit history verification; disallow new transparent public payer/amount/sequence transfers in production once migration is accepted; proof of no accidental public identifiers in newly canonicalized shielded claims.  
**Done only when:** no ordinary payment workflow silently selects transparent by default; clients fail clearly on unsupported suite; both old and new historical claims replay according to one rule; no dual balance/negative-supply path and no interrupted migration causing asset loss.  
**Stop signal:** a plan requiring an implicit generator conversion or rewrite of already-finalized monetary history.

### P0-3 - FROST custody hardening and legacy signing quarantine

**Owners:** custody team + two independent ceremony reviewers.  
**Touch points:** `mini-custody/src/{signing,session,manifest,share_store,rotation,transport}.rs`, `mini-treasury/Cargo.toml`, deployment/cargo feature matrix.  
**Build:** require the maintained FROST DKG/signing API in production-target builds; bind human-approved typed operations; make nonce reservation durable and crash/rollback-fail-closed; review complaint/abort and resharing; include adversarial 1-of-N/threshold boundary, signer replacement, offline/nonresponse, network censorship and poisoned journal tests.  
**Done only when:** exact signer manifest and threshold are enforced; a production compile/feature audit proves legacy hand-written APIs unreachable; an external audit covers actual composed code and recovery; malicious coordinator cannot obtain unauthorized signatures through transcript substitution.  
**Stop signal:** any nonce can be used for two distinct challenges or any signer can sign untyped arbitrary bytes.

### P0-4 - External review and reproducible crypto evidence packet

**Owners:** independent applied cryptography auditor, build/release reviewer.  
**Touch points:** `docs/gates/crypto-audit-scope.md`, `docs/gates/dkg-audit-scope.md`, `docs/gates/REAL_VALUE_RELEASE_CHECKLIST.md`, exhaustive test/vector tables.  
**Build:** reproducible source revision / Cargo.lock / toolchain statement; exact code and threat scope; a question-by-question audit of `stealth_v3`, `mlsag_v3`, `bp_range_v2`, `claim_v3`, V3 verification integration, disclosure, key images, FROST ceremony, canonical genesis and cross-chain representations if relevant. Include confidentiality, soundness, unforgeability, timing and correlation; require external reviewer to triage findings to closure.  
**Done only when:** Critical/High audit findings resolved, re-reviewed, exact-head signed outcome is recorded, and governance gates independently authorize the use case.  
**Stop signal:** calling an internal AI report or a component library's old audit a full system audit.

### P1-1 - Wallet scanner and privacy/fingerprint benchmark

**Owners:** privacy and mobile engineers, measurement reviewer.  
**Touch points:** `mini-private-payment/src/{scan,decoy,memo_v3,account}.rs`, `mini-relay`, `mini-transport-security`, mobile wallet integration.  
**Build:** receiver scan with local output set and verified view tags; memo opening and amount commitment checking; fixed normal wallet transaction shapes where economic policy permits; decoy test corpus from measured spends (and a clear pre-traffic prior); bandwidth/memory/CPU/power tests on low-end device; unsafe remote-node inference model.  
**Done only when:** wallet correctly receives, re-spends, rescans and recovers after data loss; scan false-positive and leakage tests pass; report covers active/passive observer, malicious node and metadata, not just cryptographic tests.  
**Stop signal:** wallet asks an untrusted peer "is this exact one-time output mine?" or exposes view key as spend authority.

### P1-2 - Private, voluntary disclosure and auditability

**Owners:** value, privacy, governance/economic policy reviewers.  
**Touch points:** `mini-private-payment/src/disclosure.rs` and account view, reports and audit UX.  
**Build:** recipient-controlled, output-scoped commitment openings and count of recognized-but-undisclosed outputs; optional public report of verified disclosed sum with explicit incomplete-coverage warning. Consider an alternative independent ZK receipt design only as separate research, not a new construction embedded opportunistically.  
**Done only when:** forged opening and omitted opening are detected as designed; result never mislabels partial income as all income, a balance, or all accounts belonging to one person.  
**Stop signal:** publishing an auditor's or issuer's permanent decrypt key to make bookkeeping easier.

### P1-3 - Personhood credential construction research (no uniqueness claim)

**Owners:** independent identity researchers and accessibility/economic reviewers.  
**Touch points:** `mini-uniqueness`, `mini-attest`, proof research docs and activation policy only.  
**Build:** compare already published anonymous credentials and nullifier families (e.g. Privacy Pass, Semaphore, BBS/Coconut-like designs) *after* defining the threat model, anti-collusion assumptions, issuer independence, recovery, exclusion, offline proof and accessibility. Separate evidence authenticity from a scientific determination that accounts belong to different people.  
**Done only when:** a construction has independently evaluated soundness/privacy, no indispensable issuing authority, real-world anti-Sybil evidence and appropriate constitutional authorization.  
**Stop signal:** marketing "one human one vote" because a per-election nullifier deduplicates keys.

### P2-1 - Observe XRP and XMR at a replaceable edge

**Owners:** isolated interoperability team, wallet/custody/security reviewers.  
**Likely new components:** `crates/mini-xrpl-observer`, `crates/mini-xmr-observer` (proposed names; avoid `mini-bridge`, already a network transport adapter).  
**Build:** pinned protocol decoder; multiple independent source nodes; exact network/asset/tx hashes; observed/included/finalized/ambiguous statuses; adversarial reorg/alternative-chain policy; timeout; idempotent durable receipt journal; zero private keys in observers. Allow user-owned app to consult the receipts.  
**Done only when:** malicious RPC, forged deposit, reorg/rollback, lost peer, duplicate event and timestamp drift are rejected or flagged as nonfinal. Observers cannot mint MINI, sign treasuries or move funds.  
**Stop signal:** converting an unverified remote RPC response into spendable Mininet value.

### P2-2 - Optional externally reviewed asset bridge (separate project gate)

**Owners:** independent protocol/economic/security auditor plus approved custody authority.  
**Build only after P2-1:** distinct chain-specific finality assumptions, witness accountability (with non-collusion review), deposit-to-credit unique ID and invariant, redemption proof, reserve solvency, no replay between chains, emergency pause constraints that do not create a protocol kill switch, disaster recovery and jurisdictional legal review. Monero deposit recognition needs a precise view-key/transaction-proof model and adversarial privacy/leakage assessment; Monero does not provide the same public account-receipt semantics as XRPL.  
**Done only when:** audited proof of reserves and entitlement reconciliation are independently testable, and bridge compromise cannot change Mininet constitutional voice.  
**Stop signal:** calling a witness committee "trustless"; concentrating all value in a founder-controlled door account; promising a guaranteed 1:1 redemption without verified reserves.

### P2-3 - Long-horizon crypto agility and research watch

**Owners:** cryptography/identity migration reviewers.  
**Build:** track Monero FCMP++/CARROT actual release status and separately track XRPL XLS-96 amendment activation, audits and changes; benchmark CLSAG and Bulletproofs+ candidates on target hardware; document explicit compatible/non-compatible groups; maintain suite-version registry and ML-DSA-65 migration exercises.  
**Done only when:** a reviewed versioned migration decision demonstrates improvement without weakening present privacy or recoverability.  
**Stop signal:** silently enabling a new cryptographic proof based on a roadmap or upstream experimental branch.

---
## 9. Adversarial test and audit matrix

Each test requires a reproducible fixture, exact checked-out commit, expected result, and reviewer. "Fail closed" means invalid or unverifiable transactions cause no new monetary state; for optional non-canonical services, refusal can be a conservative "unconfirmed" status. A passing unit test is not evidence of network-wide or mathematical security.

| ID | Attack or defect | Required test / evidence | Pass criterion |
|---|---|---|---|
| T01 | Forged input among decoys | Ring includes fabricated key/commitment absent from canonical state | Invalid claim rejected by all validators |
| T02 | Proposer burns arbitrary key image | Block contains image with absent/invalid full V3 proof | Block rejected; owner output not consumed |
| T03 | Double spend | Two independently well-formed spends of one owned note | At most one finalized across any candidate order |
| T04 | Wrong-chain replay | Reuse V3 claim on another network ID, cloned genesis or suite | Cryptographic/context verification rejects |
| T05 | Wrong commitment generator | Mix legacy basis with V3 Bulletproof/MLSAG basis | Reject without fallback or conversion |
| T06 | Negative/inflating output | Attempt modular negative or >64-bit balance via proof manipulation | Range verifier and balance rule reject |
| T07 | Fee alteration | Modify `fee_policy_id`, quote, signed fee or count after signing | Verification rejects; no inflation |
| T08 | Signature substitution | Same unsigned payload with changed MLSAG bytes | Full claim ID changes; signatures reverified |
| T09 | Memo malleability | Swap encrypted memos between outputs or payment contexts | Authentication/signing digest rejects |
| T10 | Noncanonical point/scalar | High-order scalar alias, malformed point, duplicate ring rows | Strict decoder rejects before expensive crypto |
| T11 | Decoy metadata attack | Create/trace decoy selection across multiple wallets | Publish statistical effective-anonymity assessment, not a binary "private" claim |
| T12 | Remote output-set query leaks | Adversarial peer observes requests for true input/output | No direct true-member oracle; risk reported |
| T13 | Cross-input linkability | 1 vs 2 vs 16 inputs under various output shapes | Public count/fee/timing leakage quantified |
| T14 | Missing evidence | Validator asked to finalize absent or truncated claim bytes | Fail closed; no proposer-trust fallback |
| T15 | Evidence replacement | Insert different bytes under a digest/cache key | Exact bytes bound to claim ID, revalidated |
| T16 | Consensus partition | Split/rejoin validators, including Byzantine proposer | No conflicting accepted monetary truth; replay converges |
| T17 | Snapshot tampering | Change output set, spent-image set or fee policy in snapshot | Invalid snapshot/QC/commitment rejected |
| T18 | Power loss during spend | Crash every write stage in wallet/outbox and execution | No duplicate finalization or false "final" UI |
| T19 | FROST nonce reuse | Crash, retry, restored disk or stolen old journal | No second signing use of nonce; recovery fails safe |
| T20 | Malicious FROST coordinator | Different typed requests to different participants | Signers abort on package/manifests mismatch |
| T21 | DKG and share churn | Rogue share, missing commitment, reshared old member | Abort or exact authorized rotation; no silent reduction of quorum |
| T22 | Weak-device denial of service | Max V3 ring, outputs, malformed proof flood | Explicit CPU/RAM/battery/bandwidth budgets met or release blocked |
| T23 | Optional XRPL fake deposit | Malicious RPC claims nonexistent/failed XRP transfer | Not promoted to canonical paid status |
| T24 | Monero remote view leakage | Malicious daemon correlates repeated wallet requests | Privacy policy/implementation rejects oracle behavior |
| T25 | Bridge witness collusion | Quorum attests wrong source transfer or ignores withdrawals | Trust assumption explicit; no claim of trustlessness |
| T26 | Personhood Sybil | One operator creates 1,000 keys/devices and scoped nullifiers | No claim that nullifiers prove 1,000 different people |
| T27 | Long-term key migration | Classical key compromised, PQ recovery/anchor missing | Fail-closed/conditional recovery; never invent ownership |
| T28 | Supply conservation | Genesis + issuance + all shielded claims over rollback/replay | Exactly equal independent accounting totals |
| T29 | View-key scope | View key used as signing key or leaks all identities | Capability separation enforced; disclosure accurate |
| T30 | Network-origin correlation | Payment posted after identifiable action from static peer | Document and measure residual correlation, not anonymity guarantee |

**Property testing and fuzz targets:** canonical decode/encode round trip; arbitrary-length malformed inputs; byte-consensus equivalence; deterministic state root across platforms; valid/invalid proof metamorphic changes; typed digest separation; cross-network differential tests; duplicate/unsorted ring rejection; no panic on hostile byte input; memory and CPU caps. Use tools such as `cargo-fuzz` and property-test frameworks only after the trusted toolchain and build provenance are established. Record initial corpus hashes and preserve regressions as golden vectors. Link every security-relevant invariant to an executable test and to the source of the invariant.

**Reference-oracle discipline:** independently compare (1) mathematical test vectors from published papers and official implementations, (2) a separately written test implementation of the *same exact suite* where available, and (3) Mininet's verifier. Do not take Monero transaction bytes as a positive Mininet test because Monero and Mininet use incompatible formats and curves. Positive tests should share the same fully specified suite; negative interoperability tests should deliberately show cross-suite rejection.

## 10. Risk register and recommended mitigations

| Risk | Likelihood / impact pre-audit | Mitigation / accountable gate |
|---|---|---|
| Undetected proof unsoundness mints or steals value | Unknown / critical | Independent whole-path crypto audit, adversarial genesis-to-spend tests, bounded release |
| Outdated or inconsistent V2/V3 generator basis | Material / critical | Versioned cutover, exact historical replay, no implicit conversion |
| Public transaction metadata deanonymizes payer | Material / high | Shape/fee uniformity and network privacy; empirical anonymity estimates |
| Ring decoys chosen from unrealistic spend ages | Material / high | Calibrate with real distribution after privacy-safe measurement; state uncertainty |
| Custody nonce reuse after disk rollback | Nonzero / critical | Durable anti-rollback strategy and signer refusal if uncertain |
| Single wallet/bridge signer becomes de facto protocol owner | Design-dependent / critical | No bridge right to alter consensus/governance; replaceable edge and independent keys |
| XRPL/XMR chain finality misinterpreted | Material / critical for bridge | Chain-specific finality policy, independent observations and fraud/reorg drills |
| Claim evidence pruned or missing | Material / high | Deterministic availability, bounded durable archive and catch-up testing |
| Weak phones cannot verify all proofs | Plausible / high | Proof cost budgets, device profiles, avoid adding new expensive proof families prematurely |
| Post-quantum migration cannot restore old secrets | Long-term / potentially critical | Pre-break anchors, honest classification of unprotected historic outputs |
| Anonymous credentials mistaken for unique-human evidence | High / constitutional | Treat root/nullifier as proof of key/eligibility only, never personhood absent validation |
| AI-written audit text misrepresented as specialist sign-off | Material / critical governance | Signed independent report, exact scope/revision, independent governance gate |

Two residual privacy facts cannot be eliminated by cryptographic proof composition alone. First, an auditor receiving selective payment openings can verify only the outputs disclosed under that account; Mininet cannot mathematically force a person to reveal every wallet they control. Second, anonymity against powerful traffic analysts depends on the network's observable behavior and on other people's usage patterns, not only the correctness of a ring proof. Both limitations belong in the product threat model and user messaging.

## 11. Suggested execution sequence and decision gates

**Phase 0 - freeze and scope.** Record exact `main` SHA and active constitutional authority; inventory every caller of V2 and V3; freeze proof generator and suite specifications; produce dependency and transcript mapping; publish intentionally failing end-to-end tests for the known missing V3 integration. Output: a gate packet, not code that claims real value.

**Phase 1 - make validity canonical.** Implement P0-1 and the smallest evidence availability path sufficient for validators and historical replay. Include tests T01-T10, T14-T18 and T28. In parallel, document the intended migration; do not activate it without the appropriate decision. Output: proof-checked V3 blocks on development networks, not launch authorization.

**Phase 2 - remove unsafe duplications.** Land approved V3 cutover, retirement of the transparent payment default, disciplined old-history verification, finalized fee policy, production FROST library-only wiring and crash-safe signing journals. Output: one clearly specified private-value path, complete test corpus, and a package the outside auditor can meaningfully review.

**Phase 3 - adversarial and external review.** Run whole-path fuzzing, multiprocess BFT network tests, device benchmarks and the outside cryptographic/custody audits. Fix and re-review High/Critical findings; keep deployment disabled while any independent hard gate remains incomplete. Output: evidence and independent go/no-go decisions, never a guessed release date.

**Phase 4 - privacy and interoperability at the edge.** Build measurable wallet scanning/disclosure UX, then XRP/XMR read-only watchers. Any cross-chain custody, asset minting or experimental future cryptography gets a separate RFC, economic threat model and audit. Output: optional applications that cannot be mistaken for constitutional authority.

**Estimated engineering complexity (relative, not a schedule):** canonical V3 + atomic execution 5/5; V2/V3 migration 5/5; external audit and remediation 5/5; custody hardening 4/5; scanner/privacy benchmarking 4/5; read-only XRPL observation 3/5; XMR observation 4/5; cross-chain money bridge 5/5; unique-human proof 5/5 and unresolved scientific research. Parallel implementation is safe only when it does not create mismatched consensus formats or competing sources of monetary truth.

### Proposal approval questions

Before implementing, reviewers should explicitly answer these six questions in a new, properly authorized decision record:

1. Is the immediate goal **V3 as the only new shielded MINI claim**, with previous claims preserved strictly as historical state, or has the network already accepted a different live monetary history requiring an alternative migration?
2. Will a proposed canonical block contain complete V3 claims, or independently retrievable immutable references with an availability guarantee strong enough for every honest validator and archival replayer?
3. Who controls suite activation and fee-policy epochs, and how is that decision kept constitutionally separate from token balances and private custody?
4. Which oldest/weakest device is the acceptance baseline, and what are the measured worst-case V3 verification and scan costs?
5. Which independent organizations and individuals can complete the external applied cryptography, FROST/DKG, BFT, economics and legal gates?
6. Is any specific XRPL or Monero interoperability actually required for the first release, or can observers remain later optional edge services without delaying the core money audit?

The recommendation is **not** to make every cryptographic feature a launch dependency. Finish the minimal auditable cryptographic core, then add carefully bounded features only when each protects a stated user need. More algorithms are not inherently more security.

## 12. Suggested ready-to-file engineering RFCs

These titles can be copied into issue proposals without asserting that a decision has already passed:

| Proposed RFC | Scope | Depends on | Explicitly does not claim |
|---|---|---|---|
| `MINI-RFC-CRYPTO-001: Canonical V3 Shielded Spend Validity` | Exact bytes, version dispatch, output membership, atomic spend, historical replay | Current V3 code, consensus state model | Independent audit pass |
| `MINI-RFC-CRYPTO-002: Payment Basis and Format Migration` | V2/V3 commitments, legacy history, retirement of transparent claims | Founder/governance decision, RFC-001 tests | Silent automatic migration |
| `MINI-RFC-CRYPTO-003: External Applied Crypto Gate Package` | Mathematical and composition review/test vectors | RFC-001/-002 specification | Internal review equals outside review |
| `MINI-RFC-CUSTODY-001: Library FROST and Recovery Closure` | DKG/signing, nonce burn, hostile parties, legacy quarantine | `mini-custody` | Governance signatures or XRPL signer compatibility |
| `MINI-RFC-PRIVACY-001: Scanner, Decoys and Metadata Measurement` | Recipient recovery, network origin/shape, weak devices | V3 proof semantics | Unconditional anonymity |
| `MINI-RFC-INTEROP-001: Read-Only XRPL/XMR Observers` | Independent receipts, finality uncertainty, wallet status | Separate edge API boundary | Bridge mints or custody |
| `MINI-RFC-RESEARCH-001: CLSAG / Bulletproofs+ / FCMP++ Evaluation` | Comparative cost/security evidence; future suite research | Canonical V3 stable, upstream maturity | Immediate consensus upgrade |

## 13. Final decision framework

**Approve now for design and test engineering:** V3 exact-claim validator integration; strict version/serialization rules; generator-basis migration proposal; durable evidence handling; independent external-audit procurement; FROST nonce/custody adversarial tests; conservative wallet scanning and payment-finality UX.

**Approve only after evidence and a separate policy decision:** transparent-path retirement/migration, changes to canonical algorithms or fee model, controlled amount-disclosure APIs, real-value bridge custody, and post-quantum rotations affecting spend ownership.

**Do not approve merely because XRP or Monero does it:** account signer weights controlling human voting; public payer sequences inside private MINI claims; issuer/auditor universal decryption; irreversible issuer clawback of default MINI; generic key-sharing for "recovery"; FCMP++ adoption from incomplete upstream work; replacing a proof suite's transcript or generator constants without a new audited suite; claiming an AI report is an external audit.

**Binary release conclusion:** the current evidence still indicates **NO-GO for real monetary value and real-people unique-human governance** until the repository's own external and operational gates are satisfied. The best use of proven cryptography is not another collection of prototypes: it is fewer cryptographic implementations, one deterministic validation boundary, careful versioning, and independent scrutiny of everything users will actually trust. [M7-M9]

---

## 14. Primary sources and verification links

Sources marked **M** are Mininet source and governance evidence read via connected GitHub at the observed `main` revision; **X** are XRPL primary documentation/specifications; **R** are Monero primary project documents; **F** is an independent standards document. Links are included so a reviewer can verify exact claims, check newer revisions, and attach a dated audit result. A retrieved source is not a security endorsement.

### Mininet: source, code and hard gates

- **[M1]** Mininet `CLAUDE.md` ("No inventing cryptography", typed authority, P1 boundaries): https://github.com/mininet-labs/Mininet/blob/main/CLAUDE.md ; Founder Directives: https://github.com/mininet-labs/Mininet/blob/main/docs/FOUNDER_DIRECTIVES.md ; Frozen invariants: https://github.com/mininet-labs/Mininet/blob/main/docs/INVARIANTS.md
- **[M2]** Release roadmap and traceability: https://github.com/mininet-labs/Mininet/blob/main/docs/ROADMAP_TO_RELEASE.md ; status: https://github.com/mininet-labs/Mininet/blob/main/docs/STATUS.md ; failure history: https://github.com/mininet-labs/Mininet/blob/main/docs/FAILURE_BOOK.md
- **[M3]** V3 value primitives and claims: https://github.com/mininet-labs/Mininet/blob/main/crates/mini-value/src/bp_range_v2.rs ; https://github.com/mininet-labs/Mininet/blob/main/crates/mini-value/src/mlsag_v3.rs ; https://github.com/mininet-labs/Mininet/blob/main/crates/mini-value/src/stealth_v3.rs ; https://github.com/mininet-labs/Mininet/blob/main/crates/mini-private-payment/src/claim_v3.rs
- **[M4]** Validator evidence adapter: https://github.com/mininet-labs/Mininet/blob/main/crates/mini-shielded-verify/src/lib.rs ; shielded-payment overview: https://github.com/mininet-labs/Mininet/blob/main/crates/mini-private-payment/src/lib.rs
- **[M5]** Custody library source: https://github.com/mininet-labs/Mininet/blob/main/crates/mini-custody/Cargo.toml ; signer adapter: https://github.com/mininet-labs/Mininet/blob/main/crates/mini-custody/src/signing.rs ; legacy feature flags: https://github.com/mininet-labs/Mininet/blob/main/crates/mini-treasury/Cargo.toml
- **[M6]** Primitive dependencies and suite availability: https://github.com/mininet-labs/Mininet/blob/main/crates/mini-crypto/Cargo.toml ; value dependencies: https://github.com/mininet-labs/Mininet/blob/main/crates/mini-value/Cargo.toml
- **[M7]** PR #332 security-remediation status, F-07 and partial findings: https://github.com/mininet-labs/Mininet/blob/main/docs/audits/PR332_REMEDIATION_STATUS.md
- **[M8]** Current status and AI-authored audit scope caveat: https://github.com/mininet-labs/Mininet/blob/main/docs/audits/EXTERNAL_AUDIT_MASTER_REPORT.md ; 14 Oct launch review: https://github.com/mininet-labs/Mininet/blob/main/docs/audits/launch-pr-readiness-20261014.md
- **[M9]** Binary real-value release checklist: https://github.com/mininet-labs/Mininet/blob/main/docs/gates/REAL_VALUE_RELEASE_CHECKLIST.md ; crypto audit scope: https://github.com/mininet-labs/Mininet/blob/main/docs/gates/crypto-audit-scope.md ; DKG scope: https://github.com/mininet-labs/Mininet/blob/main/docs/gates/dkg-audit-scope.md
- **[M10]** Governance/agent-session boundary: https://github.com/mininet-labs/Mininet/blob/main/AGENTS.md ; code snapshot location: https://github.com/mininet-labs/Mininet/tree/d307c2752fec14479ca2db8ec1ea83446f182754

### XRP Ledger: cryptography, state and bridges

- **[X1]** XRPL cryptographic keys, secp256k1 vs Ed25519 and ledger key encoding: https://xrpl.org/docs/concepts/accounts/cryptographic-keys
- **[X2]** XRPL account multi-signing and signer-list quorum: https://xrpl.org/docs/concepts/accounts/multi-signing
- **[X3]** XRPL Tickets / account sequence reservation: https://xrpl.org/docs/concepts/accounts/tickets
- **[X4]** Official confidential-transfer concept: https://xrpl.org/docs/concepts/tokens/fungible-tokens/confidential-transfers ; XLS-0096 final specification (last reported update 2026-10-22): https://xls.xrpl.org/xls/XLS-0096-confidential-mpt.html ; verify actual amendment activation before using: https://xrpl.org/resources/known-amendments
- **[X5]** XRPL cross-chain bridge model and witnesses: https://xrpl.org/docs/concepts/xrpl-sidechains/cross-chain-bridges
- **[X6]** XRPL escrow/cryptographic conditions: https://xrpl.org/docs/concepts/payment-types/escrow

### Monero and cryptographic standards

- **[R1]** Monero technical specifications: https://docs.getmonero.org/technical-specs/ ; cryptography overview: https://docs.getmonero.org/cryptography/
- **[R2]** Monero stealth-address / view-spend concept: https://www.getmonero.org/resources/moneropedia/stealthaddress.html ; Monero core source: https://github.com/monero-project/monero
- **[R3]** Monero CLSAG explanation and performance context: https://web.getmonero.org/resources/moneropedia/clsag.html ; Monero terms index (RingCT, Bulletproofs, key images): https://www.getmonero.org/resources/moneropedia/
- **[R4]** Monero FCMP++ official upstream work milestone (open work observed November 2026): https://github.com/monero-project/monero/milestone/1
- **[F1]** IRTF CFRG FROST published protocol, RFC 9591 (2024), including nonce-use security: https://www.rfc-editor.org/rfc/rfc9591.html
- **[F2]** Bulletproofs Rust library version used by Mininet and documentation: https://docs.rs/crate/bulletproofs/5.0.0
- **[F3]** NIST FIPS 204, ML-DSA signing standard (for the separately scoped PQ transition): https://csrc.nist.gov/pubs/fips/204/final

### Glossary

**Canonical claim:** exact serialized transaction data whose full validity is established by the consensus transition. **Commitment:** a binding/hiding mathematical representation of a hidden amount. **Decoy:** other member of an anonymous spend ring. **Domain separation:** distinct fixed protocol-specific context strings/IDs used to prevent signatures and proofs from another purpose or network being replayed. **FROST:** two-round threshold Schnorr signing. **Key image/nullifier:** spend-once fingerprint that permits double-spend detection without disclosing the input owner. **MLSAG/CLSAG:** distinct families of linkable ring signatures. **Proof generator basis:** the specific independent group generators required to construct and verify commitments/proofs; part of consensus semantics. **Ristretto255:** prime-order group abstraction used in Mininet's current privacy primitives, distinct from Monero's wire group format and XRPL secp256k1. **Selective disclosure:** recipient-controlled ability to reveal specific encrypted/committed information to an auditor. **Suite/version:** fully specified algorithm, transcript, keys, generators, encodings and acceptance rules, not just a human-readable version number. **ZKP:** proof that a statement about hidden data is true without disclosing that data.

**Research disposition:** For a future RFC, re-check cited `main` code and XRPL/Monero upstream status against the exact proposed revision, run the complete Rust toolchain and adversarial suite, and obtain specialist review. Nothing in this report closes the external release gates.