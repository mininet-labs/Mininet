Post-Quantum Migration Research Report

Research target

Repository: mininet-labs/mininet
Issue: #15 — [Phase 1.4] Post-quantum migration path for identity
Research date: 15 July 2026

⸻

Executive conclusion

Mininet should migrate identity signatures to ML-DSA-65, but it should not do so by merely adding SignatureSuite::MlDsa65 and changing the default.

The correct migration is a staged protocol transition in which:

1. existing Ed25519 KEL history remains permanently valid;
2. post-quantum-capable software learns to parse and verify ML-DSA before any identity is expected to use it;
3. an existing identity pre-commits to an ML-DSA public key through the ordinary KEL pre-rotation mechanism;
4. the subsequent rotation event is authorised under the identity's currently valid rules and activates the ML-DSA key;
5. a bounded hybrid transition period protects the migration event against both classical and post-quantum uncertainty;
6. after activation, new KEL events are signed with ML-DSA;
7. verifiers continue to validate historical Ed25519 events according to the suite tags embedded in those events;
8. protocol capability negotiation prevents old nodes from misinterpreting or silently discarding PQ identities;
9. recovery, delegation, witnesses, object signatures, release signatures, and network handshakes migrate through separately tracked paths rather than being assumed safe because identity rotation succeeded.

Issue #15 already states the essential invariant: a live KEL should rotate to a PQ key through an ordinary rotation event without a flag day or invalidating its existing history.

The repository has already reserved wire tag 0x02 for ML-DSA-65 and designed every key and signature to carry its suite tag.

That is an excellent starting seam, but it is not the complete migration design.

The strongest recommendation is:

Use ordinary KEL rotation for algorithm transition, but require a hybrid-authorised transition event during the migration epoch: the event is valid only when both the currently authorised Ed25519 key and the pre-committed ML-DSA key approve the same canonical rotation event.

After the hybrid transition event is accepted:

* ML-DSA becomes the active signature suite for that identity;
* Ed25519 remains historically verifiable;
* the old Ed25519 key cannot authorise later events;
* downgrade back to a classical-only suite requires an explicit governed recovery rule and should not be permitted as an ordinary rotation;
* new identities may eventually default directly to ML-DSA-65 after network readiness and implementation review.

The report recommends ML-DSA-65, not ML-DSA-87, as Mininet's general identity suite because it balances long-term security with mobile, BLE, object-size, and KEL-growth constraints. ML-DSA is now standardised in FIPS 204 and is intended for post-quantum digital signatures. (csrc.nist.gov)

SLH-DSA should be retained as an algorithm-diversity and emergency-recovery research option, not as Mininet's routine identity signature, because its signatures are substantially larger and slower. NIST standardised SLH-DSA in FIPS 205 as a stateless hash-based alternative based on SPHINCS+. (csrc.nist.gov)

ML-KEM-768 should later be added for post-quantum session establishment, but it is not a substitute for issue #15's identity-signature migration. NIST standardised ML-KEM in FIPS 203 with three parameter sets: ML-KEM-512, ML-KEM-768, and ML-KEM-1024. (csrc.nist.gov)

⸻

1. What issue #15 actually asks

Issue #15 is narrower than a complete post-quantum conversion of Mininet.

Its immediate question is:

How does an existing did:mini identity move from Ed25519 to a post-quantum signature suite without creating a second identity, invalidating historical KEL events, or requiring every participant to upgrade simultaneously?

The issue explicitly identifies the intended mechanism:

* mini-crypto::SignatureSuite is the agility seam;
* a KEL's existing history must remain valid;
* migration should appear as a normal rotation event;
* the migration path must be proven end to end on a live KEL.

This report therefore focuses primarily on:

* signature-suite migration;
* KEL semantics;
* transition authorisation;
* downgrade prevention;
* interoperability;
* key lifecycle;
* testing;
* phased rollout.

It also maps the adjacent work that cannot be ignored:

* PQ key agreement;
* delegated device keys;
* recovery;
* witnesses;
* object signatures;
* software and release signatures;
* consensus signatures;
* secure hardware support.

⸻

2. Current repository position

2.1 Crypto-agility is already a frozen invariant

mini-crypto states that no single signature algorithm may remain hard-wired for the life of the system.

Every key and signature carries a stable suite tag, and the comments specifically reserve ML-DSA-65 as the intended PQ migration target.

The current enum is:

pub enum SignatureSuite {
    Ed25519,
    // MlDsa65 reserved at wire tag 0x02
}

The current implementation already defines:

* Ed25519 tag 0x01;
* reserved ML-DSA tag 0x02;
* suite-specific key length;
* suite-specific signature length;
* unknown-suite rejection.

2.2 Existing strengths

Mininet already has the right high-level foundations:

* suite-tagged keys;
* suite-tagged signatures;
* canonical KEL events;
* pre-rotation commitments;
* hash-chained identity history;
* ordinary key-rotation events;
* no central identity registry;
* offline verification;
* key material intended to remain device-local.

2.3 Missing pieces

The present seam does not yet answer:

* how an Ed25519 event commits to a much larger ML-DSA key;
* whether the first PQ rotation is signed classically, post-quantumly, or both;
* how old clients react to an unknown suite;
* how recovery keys migrate;
* how delegated device keys migrate;
* whether witnesses must support PQ before identity rotation;
* how mixed-suite threshold signatures are counted;
* how to prevent algorithm downgrade;
* how large signatures affect BLE, object size, KEL synchronisation, and storage;
* which implementation is trusted;
* how deterministic and hedged ML-DSA signing modes are handled;
* how malformed large keys and signatures are bounded before allocation;
* how release and firmware signatures migrate;
* whether historical Ed25519 events remain trustworthy after a cryptographically relevant quantum computer exists.

⸻

3. Standards landscape

3.1 ML-DSA

NIST finalised FIPS 204 on 13 August 2024.

ML-DSA is the standardised form of CRYSTALS-Dilithium and provides post-quantum digital signatures. NIST states that it is believed secure even against an adversary possessing a large-scale quantum computer. (csrc.nist.gov)

FIPS 204 defines three parameter sets:

* ML-DSA-44;
* ML-DSA-65;
* ML-DSA-87.

The trade-off is conventional:

* higher parameter sets provide greater security strength;
* keys and signatures become larger;
* signing and verification become more expensive.

3.2 ML-KEM

FIPS 203 standardises ML-KEM for establishing shared secrets over public channels.

The standard defines:

* ML-KEM-512;
* ML-KEM-768;
* ML-KEM-1024.

NIST describes ML-KEM as believed secure against quantum-capable attackers and identifies ML-KEM-768 as the middle security/performance parameter set. (csrc.nist.gov)

ML-KEM matters to Mininet because migrating signatures alone does not protect:

* bearer handshakes;
* encrypted object-key establishment;
* relay links;
* mailbox setup;
* "harvest now, decrypt later" exposure.

But ML-KEM belongs to a parallel migration track.

3.3 SLH-DSA

FIPS 205 standardises SLH-DSA, based on SPHINCS+, as a stateless hash-based signature scheme. (csrc.nist.gov)

Its strategic value is algorithmic diversity:

* ML-DSA depends on module-lattice assumptions;
* SLH-DSA relies primarily on hash-function security.

Its disadvantages include:

* much larger signatures;
* slower operations;
* higher bandwidth and storage costs;
* poor fit for frequent mobile KEL events.

SLH-DSA is therefore valuable as:

* a root recovery option;
* release or archival signature option;
* emergency migration fallback;
* cross-family assurance mechanism.

It is not the recommended default for every identity event.

3.4 Standards are final, migration engineering is not

NIST's first three PQ standards were finalised in 2024, but transition systems still need to decide:

* hybrid composition;
* key representation;
* protocol negotiation;
* backwards compatibility;
* downgrade rules;
* certificate or identity continuity;
* implementation selection;
* operational rollout.

Mininet's problem is therefore no longer algorithm selection alone. It is protocol migration.

⸻

4. Quantum threat model

4.1 What quantum computing threatens

A sufficiently capable fault-tolerant quantum computer running Shor's algorithm would threaten the mathematical assumptions behind:

* Ed25519;
* X25519;
* ECDSA;
* RSA;
* classical Diffie–Hellman.

Symmetric cryptography and hash functions are affected differently. Grover's algorithm provides a quadratic search speedup rather than the dramatic structural break Shor provides against RSA and elliptic curves.

Mininet's existing 256-bit symmetric keys and strong hash functions therefore remain much better positioned than its public-key signatures and key agreement.

4.2 Identity forgery risk

When Ed25519 becomes breakable, an attacker could potentially:

* forge KEL rotations;
* create fake delegated-device events;
* forge object signatures;
* impersonate validators;
* forge release metadata;
* counterfeit witness receipts;
* manufacture recovery events.

4.3 Historical-signature risk

A key distinction is required:

Historical verification

A verifier can still mathematically check that an old Ed25519 signature matches an old public key.

Historical trust

After Ed25519 is considered breakable, an attacker might be able to generate a forged old-style event.

The hash-chain ordering and previously witnessed or anchored history become critical.

A past Ed25519 event is safer when:

* it was observed before the quantum break;
* its digest was anchored in later PQ-authenticated history;
* witnesses timestamped it;
* transparency logs committed to it;
* canonical chain state included it;
* many independent holders retained the original sequence.

The migration cannot retroactively make every unwitnessed historical signature post-quantum secure.

4.4 Harvest-now risk

Signatures do not have the same "decrypt later" property as encrypted sessions, but captured identity material can still be valuable for:

* future forgery;
* constructing counterfeit histories;
* targeting keys that remain active too long;
* exploiting delayed migrations.

For encrypted data, the urgency is greater because ciphertext collected today may later be decrypted if its session secret was established only with X25519 or another quantum-vulnerable mechanism.

⸻

5. Migration invariants

The migration design should freeze the following invariants.

PQ1 — identity continuity

An identity's self-certifying identifier does not change merely because its active signature suite changes.

PQ2 — historical validity

Every historical event is verified using the suite tag carried by its key and signature.

PQ3 — ordinary rotation semantics

A suite change is represented through the normal KEL rotation mechanism, not a separate registry migration.

PQ4 — explicit suite commitment

The prior event must commit to the exact next suite and exact next public key.

PQ5 — no silent downgrade

After an identity activates a PQ-required policy, later classical-only events are invalid unless a separately defined emergency recovery policy explicitly permits them.

PQ6 — hybrid transition

During the migration window, the suite-changing rotation must satisfy both the old and new trust domains.

PQ7 — no flag day

Old Ed25519 identities continue operating while the network gains PQ support.

PQ8 — no unknown-suite reinterpretation

A client that does not understand ML-DSA must reject or mark the identity unsupported. It must never reinterpret PQ bytes as Ed25519 or skip verification.

PQ9 — bounded decoding

Large PQ keys and signatures must be length-checked before allocation and cryptographic processing.

PQ10 — suite agility continues

ML-DSA must not become the final permanently hard-coded algorithm.

The framework must still support future suites.

⸻

6. The central migration question

Suppose the current KEL state has:

active key: Ed25519 K0
next commitment: ?

The owner wants the next active key to be:

ML-DSA-65 K1

What signatures should authorise the rotation?

⸻

7. Migration options

Option A — current Ed25519 key alone authorises PQ rotation

The Ed25519 key signs a rotation event that activates ML-DSA.

Advantages

* simplest;
* fits ordinary rotation;
* no special event format;
* old active key is the current authority.

Weaknesses

* if Ed25519 is already compromised, the attacker can redirect the identity to an attacker-controlled PQ key;
* transition security depends entirely on the old algorithm;
* no evidence that the claimed new-key holder actually possesses the ML-DSA secret key;
* permits malicious commitment to unusable or malformed PQ keys unless carefully validated.

Decision

Insufficient as the preferred migration mode.

It may be accepted only during an early pre-quantum migration phase when the old suite remains trusted and policy explicitly permits unilateral classical migration.

⸻

Option B — new ML-DSA key alone signs its activation

Advantages

* proves possession of the PQ private key;
* does not depend on an Ed25519 signature for the transition event itself.

Weaknesses

* a not-yet-authorised key authorises itself;
* breaks ordinary KEL authority semantics;
* an attacker could propose a new PQ key without current-key approval;
* complicates recovery and conflict resolution.

Decision

Reject.

⸻

Option C — dual-sign the transition event

The same canonical rotation event is signed by:

* the currently authorised Ed25519 key;
* the new pre-committed ML-DSA key.

Advantages

* current authority approves the change;
* new-key possession is proven;
* attacker must defeat both schemes to forge the transition before either is broken;
* maintains a clear continuity bridge;
* supports hybrid risk reduction;
* easy to test conceptually.

Weaknesses

* event is larger;
* KEL verification must support a transition policy;
* old software cannot verify the ML-DSA half;
* transition event requires both key implementations;
* policy must define whether both signatures are mandatory or whether one is advisory.

Decision

Recommend.

⸻

Option D — two-step activation

Event R1 adds an ML-DSA key as pending while Ed25519 stays active.

After a waiting period, event R2 signed with both keys activates ML-DSA and retires Ed25519.

Advantages

* observation and challenge window;
* witnesses can verify both keys;
* compromised devices may be detected before activation;
* supports staged software rollout;
* clearer operational recovery.

Weaknesses

* more protocol state;
* two events;
* more edge cases;
* delay can block urgent migration;
* an attacker controlling the current key may still interfere.

Decision

Recommended for high-authority roots, optional for ordinary identities.

⸻

Option E — hybrid compound signature suite forever

Every future event is signed by Ed25519 and ML-DSA.

Advantages

* security survives failure of one algorithm;
* gradual confidence;
* no abrupt dependency on ML-DSA alone.

Weaknesses

* permanent signature-size overhead;
* continued dependence on Ed25519 implementation;
* device and bandwidth cost;
* unclear recovery when only one component rotates;
* encourages treating a hybrid tuple as one opaque suite;
* does not simplify after migration.

Decision

Use only during the transition epoch or for specific high-authority roles. Do not make it the permanent default for every identity.

⸻

8. Recommended KEL migration protocol

8.1 Stage 0 — software-readiness event

Before identities rotate, software releases must support:

* parsing suite 0x02;
* ML-DSA-65 public keys;
* ML-DSA-65 signatures;
* mixed-suite KEL verification;
* strict unknown-suite failure;
* transition-policy evaluation;
* larger event sizes;
* PQ test vectors.

No default changes yet.

8.2 Stage 1 — PQ pre-commitment

The active Ed25519 identity creates a normal KEL event committing to:

next suite = ML-DSA-65
next public key = K1
transition policy = ClassicalAndNextSuite
minimum activation time or sequence

The commitment should bind:

* suite tag;
* exact public-key bytes;
* key role;
* threshold position;
* transition policy;
* optional activation delay.

The commitment must not be merely:

hash(public_key)

without also binding the suite and role.

Recommended commitment domain:

next_key_commitment =
    BLAKE3(
        "did-mini/next-key-commitment/v2" ||
        suite_tag ||
        key_role ||
        canonical_public_key
    )

If the current commitment format already includes suite-tagged key bytes canonically, preserve it rather than inventing a parallel form.

8.3 Stage 2 — hybrid activation rotation

The identity emits one canonical rotation event containing:

* prior event digest;
* new sequence;
* current key threshold;
* activated ML-DSA key;
* next-key commitments;
* transition-policy marker;
* Ed25519 signature from the current active key;
* ML-DSA signature from the activated pre-committed key.

Validation requires:

1. the event chains to the current KEL head;
2. the activated suite is known;
3. the activated public key matches the prior commitment;
4. the Ed25519 signature satisfies the current authority threshold;
5. the ML-DSA signature proves possession of the activated key;
6. both signatures cover identical canonical event bytes;
7. transition policy allows this suite change;
8. no downgrade rule is violated;
9. event and signature lengths are canonical;
10. witness or anchoring policy is satisfied where required.

8.4 Stage 3 — PQ-active state

After acceptance:

* ML-DSA is the current active suite;
* Ed25519 current keys are retired;
* subsequent ordinary events require ML-DSA;
* new pre-rotation commitments should normally be PQ;
* delegations use PQ-capable root authorisation;
* verifiers retain the full mixed-suite history.

8.5 Stage 4 — PQ-required state

After governance activates the network-wide PQ-required epoch:

* new identities default to ML-DSA-65;
* security-critical roles cannot remain classical-only;
* old Ed25519 identities may still be read;
* classical-only identities may have reduced capabilities or explicit legacy status;
* no automatic identity deletion occurs;
* migration remains owner-controlled where constitutional policy permits, but high-authority participation can require a stronger suite.

⸻

9. Proposed KEL policy types

Conceptually:

pub enum TransitionPolicy {
    CurrentSuiteOnly,
    CurrentAndNextSuite,
    ThresholdCurrentAndNextSuite,
}
pub enum SuiteSecurityState {
    Supported,
    Deprecated,
    VerificationOnly,
    ForbiddenForNewAuthority,
}
pub struct SuitePolicy {
    pub suite: SignatureSuite,
    pub state: SuiteSecurityState,
    pub valid_for_new_identities: bool,
    pub valid_for_rotation_targets: bool,
    pub valid_for_current_authority: bool,
}

The exact form may be different, but Mininet needs to distinguish:

* parsing support;
* historical verification;
* authority for new events;
* default selection;
* forbidden use.

A single DEFAULT constant is not enough for a century-scale migration.

⸻

10. Why ML-DSA-65 is the recommended identity suite

10.1 ML-DSA-44

Advantages

* smallest ML-DSA keys and signatures;
* fastest;
* lowest KEL and BLE overhead.

Weaknesses

* lower security category;
* less conservative for a century-scale identity root;
* may require earlier migration if security margins change.

Decision

Possible for ephemeral delegated devices, but not recommended as the root identity default without further threat and performance study.

10.2 ML-DSA-65

Advantages

* balanced security and size;
* natural migration target already named in Mininet's source;
* suitable general-purpose level;
* less costly than ML-DSA-87;
* better fit for mobile and local transport;
* widely expected to be the practical middle parameter choice.

Weaknesses

* substantially larger than Ed25519;
* lattice-family concentration;
* implementation maturity and side-channel review are essential.

Decision

Recommend for ordinary root identities and delegated devices, subject to benchmark and audit.

10.3 ML-DSA-87

Advantages

* highest ML-DSA security category;
* conservative for governance and release roots.

Weaknesses

* largest keys and signatures;
* higher mobile and storage cost;
* unnecessary for every social object or device event;
* increases denial-of-service pressure.

Decision

Consider for:

* release authority;
* constitution or genesis signatures;
* high-value long-lived governance roots.

Do not make it the universal default.

⸻

11. SLH-DSA role

SLH-DSA provides a different security family based on hash constructions rather than module lattices. NIST standardised it as FIPS 205. (csrc.nist.gov)

Recommended uses:

* emergency recovery authority;
* release-signing checkpoint;
* archival attestation;
* periodic KEL checkpoint;
* second-family hybrid for exceptionally critical roots;
* migration fallback if ML-DSA suffers a major break.

Do not use it for:

* every post;
* frequent device events;
* BLE-constrained routine traffic;
* every consensus vote.

A useful model is:

ML-DSA-65:
    routine PQ identity operation
SLH-DSA:
    sparse high-assurance checkpoints and recovery

⸻

12. Historical KEL preservation

12.1 Existing Ed25519 events remain part of the identity

The verifier processes each event with the suite active at that event.

Example:

Event 0: Ed25519
Event 1: Ed25519
Event 2: Ed25519 → commits ML-DSA
Event 3: hybrid transition
Event 4: ML-DSA
Event 5: ML-DSA

There is no need to:

* re-sign every historical event;
* create a new DID;
* rewrite the SCID;
* duplicate the KEL;
* wrap old events inside new ones.

12.2 PQ checkpointing

The first ML-DSA event should commit to the entire prior KEL head.

Because the KEL is hash chained, signing the prior event digest under ML-DSA provides a post-quantum authenticated checkpoint of the exact historical chain the owner recognised at migration time.

This does not make old Ed25519 signatures quantum-resistant in isolation.

It establishes:

The holder of the valid migrated PQ identity accepted this exact historical chain as its predecessor.

Witnesses, anchors, replicated copies, and transparency records strengthen that statement by showing that the history existed before a later attacker attempted to forge it.

12.3 Witness anchoring

The hybrid transition should receive stronger-than-normal witness treatment:

* several witness receipts;
* gossip;
* canonical-chain anchoring;
* transparency log inclusion;
* durable replication.

This reduces the risk of later counterfeit migration histories.

⸻

13. Old-client behaviour

13.1 Before PQ activation

Old clients can continue to verify Ed25519 histories.

13.2 At a PQ event

An old client sees unknown suite 0x02.

It must:

* reject the event as unsupported;
* preserve the prior verified state;
* mark the identity as having an unsupported continuation if that fact is available safely;
* avoid treating the previous Ed25519 head as permanently current;
* avoid accepting later data under stale authority.

13.3 The stale-head danger

An old verifier might validate the KEL only up to the last Ed25519 event and continue trusting that key.

That creates a downgrade fork:

* upgraded nodes know the identity moved to ML-DSA;
* old nodes continue accepting the retired Ed25519 key.

Therefore an identity migration needs a suite-transition tombstone or capability signal visible before activation.

The prior Ed25519 event should announce:

next event requires suite support unavailable to legacy clients

An old client that understands the generic transition marker but not ML-DSA can stop safely.

Clients too old even to understand the marker remain a residual risk and must be retired from security-critical use.

⸻

14. Downgrade prevention

14.1 Ordinary downgrade must be invalid

After PQ activation, this must fail:

ML-DSA → Ed25519

unless an explicit recovery policy approved before compromise allows it.

14.2 Why downgrade is dangerous

An attacker may try to:

* exploit old software;
* replay an earlier Ed25519 head;
* fork from the pre-migration state;
* claim PQ support is unavailable;
* induce an owner to rotate back for compatibility.

14.3 Security-level monotonicity

Each suite should have a governed security rank or transition relation.

Avoid a naive numeric rank alone because algorithm diversity cannot always be represented on one axis.

Recommended rule:

allowed_transition(from, to, policy, epoch)

rather than:

to.rank >= from.rank

14.4 Emergency rollback

If ML-DSA suffers a catastrophic break, the system may need to move to:

* SLH-DSA;
* another future suite;
* a hybrid recovery suite.

This is not a "downgrade" if the governed transition table identifies the target as the current emergency-safe suite.

⸻

15. Threshold and multi-key identities

If an identity uses multiple current keys, transition validation must define:

* how many old-suite signatures are required;
* how many new-suite possession signatures are required;
* whether every future key must be PQ;
* whether mixed Ed25519/ML-DSA thresholds are temporary;
* whether device keys may migrate independently.

Recommended:

old authority threshold:
    existing KEL threshold
new possession threshold:
    threshold over every activated new-suite key
transition valid:
    old threshold satisfied
    AND
    new threshold satisfied

Do not combine signature counts from unrelated suites into one undifferentiated threshold.

⸻

16. Delegated devices

16.1 Root-first migration

The root should support PQ before devices are required to migrate.

16.2 Device migration options

Immediate all-device migration

Strongest but operationally difficult.

Gradual device migration

Root is ML-DSA; some existing delegated devices remain Ed25519 temporarily.

This can be acceptable if:

* root delegation records explicitly authorise the device suite;
* legacy device capabilities are reduced;
* migration deadline exists;
* new device delegations require PQ;
* root can revoke legacy devices.

16.3 Recommendation

Use staged migration:

1. root becomes hybrid/PQ;
2. new devices require ML-DSA;
3. old Ed25519 devices remain under time-limited delegation;
4. sensitive capabilities require PQ devices earlier;
5. legacy devices expire or are revoked.

⸻

17. Recovery migration

Recovery is one of the most dangerous gaps.

A PQ-active identity remains classically vulnerable if its recovery path still uses only Ed25519.

The migration must include:

* root active key;
* next key;
* recovery keys;
* guardian keys;
* witness-authorisation keys;
* delegated high-authority devices;
* offline backup material.

Recommended recovery policy:

normal recovery:
    threshold ML-DSA guardians
high-assurance recovery:
    ML-DSA threshold
    plus optional SLH-DSA offline recovery key
legacy Ed25519 recovery:
    expires after migration grace

⸻

18. Key agreement is a separate urgent track

Issue #15 concerns signatures, but Mininet's X25519 channels remain quantum-vulnerable.

NIST's ML-KEM standard is specifically intended to establish shared secrets over public channels. (csrc.nist.gov)

Recommended future handshake:

hybrid shared secret =
    HKDF(
        X25519_shared_secret ||
        ML-KEM-768_shared_secret,
        transcript,
        protocol domain
    )

The combiner must be externally reviewed.

Security goal:

* confidentiality holds if either the classical or PQ component remains secure;
* transcript binds both algorithms;
* downgrade removing one component fails;
* all-zero and decapsulation failures are handled safely.

Do not assume adding ML-DSA signatures protects recorded X25519 ciphertext.

⸻

19. Objects, releases, consensus, and witnesses

Identity migration is necessary but insufficient.

Object signatures

Private and public objects signed by device keys must support variable signature sizes and PQ suites.

Release signatures

Software releases are long-lived and high-value.

Consider:

* ML-DSA-87;
* SLH-DSA;
* hybrid release signatures;
* offline signing.

Consensus

Consensus votes are frequent and bandwidth-sensitive.

ML-DSA signature sizes may materially affect:

* block size;
* vote propagation;
* quorum certificates;
* BLE or constrained links;
* validator CPU.

Consensus PQ migration requires separate benchmarks.

Witness receipts

Witness signatures must migrate or quantum attackers may forge duplicity evidence, receipts, or historical observations.

KEL checkpoints

A sparse SLH-DSA or ML-DSA checkpoint may provide long-term assurance without making every old event carry a huge signature.

⸻

20. Implementation strategy

20.1 Do not write ML-DSA from scratch

Use an externally reviewed implementation.

Selection criteria:

* FIPS 204 conformance;
* constant-time design where applicable;
* Rust memory-safety boundary;
* no unsafe code in Mininet-facing wrapper where avoidable;
* deterministic test vectors;
* mobile targets;
* WASM considerations;
* no hidden global RNG;
* explicit randomness interface;
* maintained upstream;
* reproducible build;
* license compatible with CC0 project usage and distribution;
* fuzzing and side-channel review.

20.2 Candidate integration forms

Pure Rust crate

Advantages:

* easier cross-compilation;
* no C FFI;
* stronger memory-safety story;
* simpler WASM.

Risks:

* implementation maturity;
* performance;
* constant-time review.

Vendored audited C implementation behind FFI

Advantages:

* mature optimised code;
* established test vectors;
* possible FIPS validation path.

Risks:

* unsafe boundary;
* build complexity;
* mobile packaging;
* reproducibility;
* platform variance.

liboqs

Advantages:

* broad algorithm support;
* experimentation;
* easy benchmarks.

Risks:

* large dependency;
* not ideal as permanent consensus-critical substrate;
* algorithm breadth exceeds Mininet needs;
* API and build surface.

Recommendation

Use a two-stage approach:

1. benchmark and test through a recognised reference or liboqs-compatible implementation;
2. choose one narrowly scoped audited production implementation for ML-DSA-65;
3. wrap it behind Mininet's existing SignatureSuite API;
4. retain cross-implementation test vectors.

⸻

21. API changes

The current API assumes key and signature lengths can be returned as constants per suite, which is compatible with ML-DSA.

However, the entire codebase must be audited for assumptions such as:

public key <= 32 bytes
signature <= 64 bytes
one signature fits in small stack buffer

Recommended types:

pub enum PublicKey {
    Ed25519(Ed25519PublicKey),
    MlDsa65(MlDsa65PublicKey),
}
pub enum Signature {
    Ed25519(Ed25519Signature),
    MlDsa65(MlDsa65Signature),
}

or equivalent suite-tagged fixed-bound byte containers.

Requirements:

* strict exact lengths;
* no unbounded Vec<u8> accepted without suite validation;
* redacted private-key debug;
* zeroisation where supported;
* canonical encodings only;
* no ambiguous parsing;
* no generic fallback verifier.

⸻

22. Wire format

22.1 Existing suite tag

Retain:

0x01 = Ed25519
0x02 = ML-DSA-65

22.2 Future tags

Reserve tags through a governed registry.

Do not assign:

* "experimental" production tags casually;
* the same algorithm under multiple tags without a version reason;
* parameter sets by implicit key length.

Each tag must identify:

* algorithm;
* parameter set;
* encoding version;
* signing mode where relevant.

22.3 Signature encoding

Use the exact canonical byte format required by the selected FIPS 204 implementation.

Reject:

* alternate encodings;
* trailing bytes;
* truncated signatures;
* non-canonical expanded forms;
* unknown parameter identifiers.

⸻

23. Event-size impact

PQ signatures are much larger than Ed25519 signatures.

This affects:

* KEL transfer;
* Bluetooth exchange;
* object envelopes;
* gossip;
* witness receipts;
* block inclusion;
* storage;
* QR or offline transfer;
* denial-of-service limits.

Required work:

* update maximum key lengths;
* update maximum signature lengths;
* update event-size caps;
* benchmark worst-case multi-signature rotations;
* ensure parsers reject oversized events before allocation;
* consider KEL checkpoint compression;
* preserve full verifiability;
* avoid generic compression formats that introduce parser complexity or malleability.

Do not truncate PQ signatures.

⸻

24. Rollout phases

Phase 0 — research and inventory

* enumerate every public-key use;
* classify signature versus key-agreement role;
* record wire format;
* record maximum sizes;
* record hardware dependency;
* identify long-lived confidentiality data;
* select candidate implementation.

Phase 1 — verify-only support

* add ML-DSA-65 enum variant;
* parse keys and signatures;
* verify standard test vectors;
* add malformed-input tests;
* no generation;
* no KEL activation.

Phase 2 — key generation and isolated signing

* generate ML-DSA keys;
* sign typed test messages;
* secure RNG;
* secret zeroisation;
* benchmarks;
* mobile tests;
* cross-implementation vectors.

Phase 3 — KEL hybrid migration prototype

* pre-commit ML-DSA key;
* dual-sign transition;
* activate ML-DSA;
* verify complete mixed-suite KEL;
* reject downgrade;
* test legacy clients.

Phase 4 — delegated devices and recovery

* mixed device suites;
* capability restrictions;
* recovery migration;
* guardian migration;
* witness support.

Phase 5 — network opt-in

* allow volunteer identities to migrate;
* no change to default;
* collect performance and interoperability data;
* audit logs;
* external cryptographic review.

Phase 6 — new-identity default

After:

* broad verifier support;
* mobile readiness;
* witness readiness;
* recovery readiness;
* external audit;
* stable implementation.

Change:

SignatureSuite::DEFAULT = MlDsa65

Phase 7 — classical deprecation

* prohibit new Ed25519 identities;
* prohibit rotation targets to Ed25519;
* retain historical verification;
* require PQ for high-authority roles;
* eventually expire legacy active authority.

Phase 8 — PQ-required network state

* consensus policy recognises only approved PQ-active validator roots;
* release pipeline requires PQ signatures;
* object and identity surfaces publish maturity labels;
* classical verification remains for history.

⸻

25. External review gate

No production migration should occur before external review of:

* suite wrapper;
* canonical encodings;
* random-number handling;
* side-channel posture;
* hybrid transition semantics;
* KEL pre-commitment binding;
* dual-signature verification;
* downgrade prevention;
* recovery migration;
* legacy-client stale-head behaviour;
* event-size and denial-of-service handling;
* cross-implementation vectors;
* key destruction;
* deterministic versus hedged signing mode;
* failure behaviour.

The review should distinguish:

* algorithm assurance from FIPS 204;
* implementation assurance;
* protocol-composition assurance;
* operational migration assurance.

Standardising ML-DSA does not automatically validate Mininet's KEL migration protocol.

⸻

26. Adversarial tests

Suite tests

1. 0x01 parses only as Ed25519.
2. 0x02 parses only as ML-DSA-65.
3. Unknown tags fail closed.
4. Wrong-length ML-DSA keys fail before verification.
5. Wrong-length signatures fail before verification.
6. Trailing bytes fail.
7. Ed25519 key bytes cannot be parsed as ML-DSA.
8. ML-DSA bytes cannot be parsed as Ed25519.
9. Cross-suite verification always fails.
10. Suite-tag mutation invalidates the signature.

KEL transition tests

1. Existing Ed25519-only KEL still verifies.
2. Ed25519 event can commit to an ML-DSA key.
3. Activated ML-DSA key must match the prior commitment.
4. Transition without the old Ed25519 signature fails.
5. Transition without the new ML-DSA signature fails.
6. Signatures over different canonical event bytes fail.
7. New-key possession proof cannot be replayed for another identity.
8. Wrong suite in commitment fails.
9. Wrong key role in commitment fails.
10. ML-DSA activation without prior commitment fails.
11. Post-migration Ed25519 event fails.
12. Replay of the pre-migration head cannot revive the old key.
13. Lower-sequence transition fails.
14. Parallel conflicting PQ transitions produce duplicity evidence.
15. Full mixed KEL verifies offline.

Legacy tests

1. Old client rejects unknown suite.
2. Old client does not accept retired Ed25519 authority after seeing transition intent.
3. Upgraded client marks stale legacy head as superseded.
4. Unknown-suite event cannot be skipped.
5. Legacy device capabilities expire under policy.
6. Old and new clients agree on the last mutually supported head.

Recovery tests

1. Classical-only recovery fails after grace expiry.
2. PQ recovery threshold succeeds.
3. SLH-DSA emergency recovery succeeds only under its explicit policy.
4. Recovery cannot downgrade to Ed25519.
5. Compromised old recovery key cannot replace a migrated root.
6. Guardian-suite mismatch fails.

Resource tests

1. Oversized public key is rejected without large allocation.
2. Oversized signature is rejected.
3. Maximum threshold migration event stays under defined cap.
4. BLE chunking round-trips a migration event.
5. KEL synchronisation resumes after interruption.
6. Verification cost remains bounded under malformed-event floods.
7. Batch verification is not assumed unless securely implemented.

⸻

27. Technologies considered

Ed25519 forever

Advantage

Small, fast, mature.

Failure

Not resistant to a cryptographically relevant quantum computer.

Decision

Reject as the century-scale endpoint.

⸻

ML-DSA-65 direct replacement

Advantage

Simple and standardised.

Failure

Does not address migration authorisation, stale clients, recovery, downgrade, or mixed history.

Decision

Necessary primitive, insufficient protocol.

⸻

ML-DSA-87 everywhere

Advantage

Maximum ML-DSA parameter strength.

Failure

Excessive routine cost for mobile identities, KELs, and objects.

Decision

Reserve for selected high-authority roles.

⸻

SLH-DSA everywhere

Advantage

Hash-based algorithm diversity.

Failure

Large signatures and high cost.

Decision

Use for sparse checkpoints, recovery, and emergency diversity.

⸻

Permanent Ed25519 + ML-DSA hybrid

Advantage

Security if either survives.

Failure

Permanent overhead and complexity.

Decision

Use for migration events and possibly high-authority roots, not every event.

⸻

New DID for PQ identity

Advantage

Simple implementation boundary.

Failure

Breaks continuity, reputation, delegations, history, governance, and founder intent.

Decision

Reject.

⸻

Re-sign entire history under ML-DSA

Advantage

Appears to make old history PQ signed.

Failure

Changes historical artefacts, creates ambiguous canonical history, wastes space, and does not prove original chronology.

Decision

Reject.

Use one PQ checkpoint over the existing KEL head.

⸻

Classical-only transition signature

Advantage

Minimal protocol change.

Failure

Transition remains as weak as Ed25519.

Decision

Permit only during explicitly early migration; prefer dual signatures.

⸻

Dual-authorised ordinary rotation

Advantage

Continuity plus PQ possession proof.

Failure

Larger event and verifier changes.

Decision

Recommend.

⸻

28. Proposed design-note core

Suggested file:

docs/design/post-quantum-identity-migration.md

Decision

Mininet identity migration uses the existing KEL rotation and suite-tag architecture.

ML-DSA-65 is the initial PQ identity suite at tag 0x02.

An existing Ed25519 identity migrates by:

1. pre-committing to an exact suite-tagged ML-DSA-65 key;
2. emitting a canonical rotation event signed by both the currently authorised Ed25519 key and the activated ML-DSA key;
3. retiring the Ed25519 authority after acceptance;
4. continuing the same KEL and DID under ML-DSA.

Historical Ed25519 events remain verifiable and are checkpointed by the first PQ-authenticated KEL head.

Hard limitations

* historical Ed25519 events do not become quantum-secure in isolation;
* old clients cannot safely follow ML-DSA KELs;
* identity signatures do not provide PQ session confidentiality;
* recovery, witnesses, devices, objects, releases, and consensus require separate migration;
* no production switch occurs before implementation and protocol review.

Downgrade rule

Once PQ activation occurs, ordinary rotation to Ed25519 is invalid.

Default rule

Ed25519 remains the default until verifier, device, recovery, witness, and operational readiness gates pass.

⸻

29. Proposed decision-log entry

Decision

Mininet adopts ML-DSA-65 as its first post-quantum identity signature suite while retaining Ed25519 for legacy-history verification.

Existing identities migrate through ordinary KEL pre-rotation and a hybrid-authorised activation event signed by both the current Ed25519 authority and the activated pre-committed ML-DSA key.

After activation, the identity remains the same did:mini root and KEL, but later events require the PQ suite. Classical-only downgrade is forbidden outside an explicit emergency recovery transition.

Reason

Suite tagging already preserves crypto agility, but changing the default alone does not prove continuity or new-key possession. Dual-authorised ordinary rotation bridges the old and new trust domains without a flag day or identity reset.

Constitutional impact

* strengthens long-term identity integrity;
* preserves self-certifying identity continuity;
* preserves offline verification;
* preserves owner-controlled key rotation;
* does not create a registry or administrator;
* does not rewrite history;
* does not weaken suite agility;
* introduces no governance weight.

Failure point

The decision fails if the selected implementation is unsafe, if old clients continue accepting retired Ed25519 authority, if recovery remains classical-only, if event-size growth breaks constrained transports, or if migration is presented as protecting X25519-encrypted history.

Required follow-up

* implementation selection;
* benchmarks;
* ML-DSA verification and signing support;
* hybrid KEL transition;
* recovery migration;
* witness migration;
* delegated-device migration;
* ML-KEM handshake research;
* external audit;
* network readiness policy.

⸻

30. Final recommendations

Adopt now

1. Write the migration design before code.
2. Keep tag 0x02 for ML-DSA-65.
3. Preserve all historical suite tags.
4. Add suite lifecycle policy beyond one DEFAULT constant.
5. Require exact suite-tagged pre-rotation commitments.
6. Use dual-authorised transition events.
7. Require proof of possession from the new ML-DSA key.
8. Keep the same DID and KEL.
9. PQ-sign the existing KEL head as a checkpoint.
10. Forbid ordinary downgrade after PQ activation.
11. Make unknown-suite handling fail closed.
12. Design safe legacy stale-head behaviour.
13. Benchmark mobile, BLE, WASM, and desktop.
14. Audit every key/signature length assumption.
15. Migrate recovery and device keys explicitly.
16. Strengthen witness treatment for migration events.
17. Use a reviewed external ML-DSA implementation.
18. Require cross-implementation vectors.
19. Require external cryptographic review.
20. Keep ML-KEM as a parallel but separate track.

Adopt later

1. ML-DSA-65 as default for new identities.
2. ML-DSA-87 for selected release or governance roots.
3. SLH-DSA for sparse recovery or archival checkpoints.
4. Hybrid X25519 + ML-KEM-768 handshakes.
5. PQ consensus signatures after performance analysis.
6. PQ witness receipts.
7. PQ release and installer verification.
8. Hardware-backed PQ keys when platforms support them.
9. Algorithm-diverse emergency recovery.

Defer

1. Permanent hybrid signatures for every object.
2. PQ migration of every subsystem in issue #15.
3. Threshold aggregation of ML-DSA signatures.
4. Custom signature compression.
5. New unstandardised PQ algorithms.
6. Fully PQ anonymous credentials.
7. PQ mix packets until L3's audited design stabilises.
8. FIPS certification as a prerequisite for research prototypes.
9. Changing the network default before recovery and witnesses are ready.

Reject

1. Creating a new DID for the PQ identity.
2. Re-signing the entire KEL.
3. Letting the new key self-authorise.
4. Migrating with no proof of PQ private-key possession.
5. Treating an unknown suite as ignorable.
6. Allowing old clients to keep trusting a retired key.
7. Ordinary ML-DSA-to-Ed25519 downgrade.
8. Classical-only recovery after PQ activation.
9. Writing ML-DSA from scratch.
10. Assuming FIPS standardisation validates Mininet's composition.
11. Claiming ML-DSA signatures protect old X25519 ciphertext.
12. Selecting ML-DSA-87 universally without measurement.
13. Using SLH-DSA for every frequent identity event.
14. Reusing one tag for several parameter sets.
15. Switching the default before network-wide verification readiness.

⸻

31. Essay: An Identity Must Outlive Its First Mathematics

A long-lived identity makes a promise that most cryptographic systems avoid.

It says that the same root of authority should survive devices, software generations, key loss, attacks, political change, and algorithms that eventually become obsolete.

A normal account can be migrated by a database administrator. A certificate can be replaced by a central authority. A blockchain address can be abandoned for a new one.

Mininet's identity model does not want those dependencies. A did:mini root is intended to certify itself through its own event history. That makes algorithm migration harder, but also more meaningful.

The identity cannot simply announce:

I use a new algorithm now.

The question is who is authorised to make that announcement.

If the old Ed25519 key alone approves the migration, continuity is clear, but the transition remains only as strong as the old mathematics. If the new ML-DSA key approves itself, the new key has no established authority. If the system creates a new DID, the migration discards the continuity it was meant to protect.

The correct bridge uses both sides.

The old key says:

I, the currently authorised identity, approve this exact successor.

The new key says:

I possess the secret corresponding to the successor that was previously committed.

Both signatures cover the same rotation event. Neither can substitute a different key, identity, sequence, or history.

After that event, the old key is no longer the authority. It remains part of history, just as a retired physical passport remains evidence that a person once held it without remaining valid for future travel.

This distinction between history and authority is central.

An Ed25519 signature created in the past does not disappear when ML-DSA arrives. The verifier still checks it with Ed25519 because that was the rule at that point in the KEL. The first ML-DSA event then signs the digest of the chain that came before it.

The PQ signature does not travel backwards in time and make the old events quantum-resistant. It says something narrower and defensible:

The holder of the migrated PQ identity recognised this exact historical chain as its valid past.

Witnesses, anchors, replicated copies, and transparency records strengthen that statement by showing that the history existed before a later attacker attempted to forge it.

This is why migration must happen before the old algorithm is considered broken.

A rotation mechanism is not a time machine. If an attacker can already forge the active Ed25519 key, the attacker may race the legitimate owner to install a malicious PQ successor. Hybrid migration reduces risk only while at least one side of the bridge remains trustworthy and the network can determine which event was observed first.

The migration must also avoid a flag day.

A global switch sounds clean: at one block height, every identity uses ML-DSA. In practice, devices are offline, applications lag, mobile platforms differ, witnesses update at different times, and people may not open the software for months.

Crypto-agility exists to avoid this coordination failure.

The network should first learn to understand ML-DSA. Then identities may opt into it. New identities eventually default to it. High-authority roles adopt it earlier. Ed25519 becomes verification-only rather than vanishing.

This creates a mixed world, and mixed worlds are where downgrade attacks live.

An old client may verify the KEL up to the last Ed25519 event and believe that key is still current. An attacker can exploit that belief even though upgraded nodes know the identity migrated.

Therefore migration intent must become visible before the unknown suite appears. A legacy verifier unable to follow the next event must stop trusting the previous key for new authority. "Unsupported continuation" is safer than "nothing changed."

The same caution applies to recovery.

A root that signs routine events with ML-DSA but can be recovered by one old Ed25519 key is not genuinely migrated. The attacker ignores the strong front door and enters through the classical recovery path.

Device delegation, witnesses, object signatures, release keys, and validator votes all have similar hidden dependencies. Post-quantum security is not a badge attached to the root enum. It is the removal of classical single points of failure from every path that can create authority.

Nor does a PQ signature protect confidentiality.

ML-DSA prevents future signature forgery under its assumptions. It does not stop an adversary from recording X25519 sessions today and decrypting them later with a quantum computer. ML-KEM and hybrid key establishment must follow on their own track.

This is one reason the transition should be described as a programme rather than a patch.

The patch may be small:

SignatureSuite::MlDsa65

The programme includes:

* key generation;
* secure storage;
* event formats;
* pre-rotation;
* dual authorisation;
* device migration;
* recovery;
* witnesses;
* constrained transport;
* legacy behaviour;
* consensus;
* releases;
* cryptographic review.

Mininet is unusually well positioned because the first design already refused to hard-code Ed25519 forever. Suite tags travel with keys and signatures. The KEL already expects keys to rotate. The identity is defined by its history rather than one permanent public key.

That means the migration does not need to reinvent identity.

It needs to exercise the promise the original architecture already made.

The deeper principle is that crypto-agility is not the ability to add another enum variant. It is the ability to change the mathematics without changing who the identity is, without letting old mathematics retain hidden authority, and without asking the whole world to change on the same day.

A century-scale identity cannot know which algorithms will survive the century.

It can only ensure that no algorithm is mistaken for the identity itself.
