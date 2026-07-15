KEL Witness Receipts and Duplicity-Gossip Research Report

Research target

Repository: mininet-labs/mininet
Problem: KEL witness/duplicity gossip for the "never seen a fresher log" case
Relevant sources: SPEC-01 §7, invariant M3, audit #12 finding F4
Research date: 15 July 2026

⸻

Executive conclusion

Mininet's current pin-based protection is necessary but incomplete.

Pinning protects a verifier that has already seen a particular KEL head:

previously pinned head H5
newly presented fork H4'
→ reject because it conflicts with known history

It does not protect a verifier that has never seen the identity before:

new verifier receives valid-looking branch A
attacker gives everyone else valid-looking branch B
new verifier has no prior head to compare

Both branches may:

* begin from the same self-certifying inception event;
* contain valid controller signatures;
* satisfy local hash chaining;
* appear internally complete;
* use acceptable sequence numbers;
* pass ordinary KEL verification in isolation.

The harder problem is therefore not merely stale-log detection.

It is:

How can independently operating Mininet peers obtain durable, transferable evidence that a particular key event was observed by enough independent witnesses, and how can conflicting observations become compact, gossipable duplicity proofs?

The recommended design is a KERI-inspired witness receipt layer combined with transparency-style gossip, but not a central global identity ledger.

The core protocol should introduce:

1. Witness configuration in KEL establishment events
    * exact witness identifiers;
    * witness threshold;
    * witness-set generation;
    * rotation rules.
2. Typed witness receipts
    * a witness signs a statement binding one identity, sequence number, event digest, prior digest, witness-set generation, and receipt epoch;
    * a receipt means "I observed and accepted this event under the stated witness policy," not "this identity belongs to a human" or "this event is globally canonical."
3. Witnessed event certificates
    * an event plus enough valid receipts to meet the configured threshold;
    * independently verifiable without contacting witnesses.
4. First-seen monotonic witness state
    * an honest witness must not receipt two different event digests for the same identity and sequence;
    * it must reject events that do not extend its accepted chain, subject to explicit recovery rules.
5. Typed duplicity proofs
    * two conflicting controller-signed events at the same sequence; or
    * two incompatible witnessed continuations from the same prior event; or
    * one witness signing conflicting receipts;
    * all represented as small, self-contained evidence objects.
6. Gossip summaries
    * peers exchange compact KEL-head summaries and known duplicity-proof identifiers during normal sync;
    * disagreements trigger targeted receipt/event retrieval rather than full-log flooding.
7. Independent witness diversity
    * witness threshold must not be interpreted as meaningful if all witnesses share one operator, machine, ASN, jurisdiction, or administrative root.
8. No global freshness claim
    * a witness certificate proves that a threshold of configured witnesses observed one branch;
    * it does not prove that no other event exists anywhere;
    * freshness remains bounded by witness availability, gossip reach, and the verifier's policy.

The preferred acceptance rule for a verifier that has never seen the identity is:

Accept current authority only when:
1. the full KEL verifies from inception;
2. each establishment transition satisfies ordinary KEL rules;
3. the current establishment state names a witness policy;
4. the presented head is accompanied by a valid witness certificate;
5. the certificate satisfies the verifier's minimum threshold/diversity policy;
6. no known duplicity proof conflicts with the chain;
7. the certificate is recent enough for the requested authority class.

For low-risk contexts, the verifier may still accept an unwitnessed direct-mode KEL with an explicit lower assurance class.

For governance, treasury, release, recovery, validator, or constitutional authority, witness certification should be mandatory.

The design should use ordinary independent signatures first. It should not begin with threshold aggregation, BLS, CoSi, or a bespoke consensus protocol.

The simplest secure first version is more valuable than a compact but novel cryptographic construction.

⸻

1. Repository context

1.1 Mininet already uses KERI-style identity

The repository describes did-mini as a KERI-style identity layer supporting:

* self-certifying identifiers;
* key event logs;
* pre-rotation;
* delegation;
* recovery;
* pairwise pseudonyms.

Everything else in the system ultimately roots in this identity layer.

1.2 Existing project rules matter here

The repository imposes several constraints that directly shape the solution:

* no novel unreviewed cryptography;
* prefer simpler, well-established constructions;
* security-sensitive authority must use typed request types rather than generic signatures;
* honesty about unbuilt or unaudited guarantees is mandatory;
* append-only history must not be rewritten.

The proposed witness protocol therefore must not expose an API such as:

witness.sign(bytes)

It should expose a typed operation such as:

witness.sign_event_receipt(WitnessReceiptStatement)

⸻

2. The precise security gap

2.1 What pinning solves

Suppose Alice previously verified identity D through sequence 12 and pinned:

D → event digest E12

An attacker later presents another chain ending at:

D → event digest E10'

or a conflicting sequence-12 event:

D → event digest E12'

Alice can reject it because it contradicts her retained state.

This protects returning verifiers.

2.2 What pinning cannot solve

Bob has never seen D.

The attacker presents:

inception E0
rotation E1
rotation E2A

while the legitimate network sees:

inception E0
rotation E1
rotation E2B

If both E2A and E2B are signed with keys authorised by E1, both may be locally valid.

Bob cannot determine which branch:

* appeared first;
* was broadly witnessed;
* was later revoked;
* was shown only to him;
* is an isolated attacker-created fork.

The phrase "never seen a fresher log" captures the problem:

The verifier has no trusted local comparison point, so absence of a fresher event in its own storage proves nothing.

2.3 The stronger attack

A controller or stolen controller key deliberately equivocates:

same identity
same prior event
same sequence
different next keys
different audiences

This is not merely an attacker replaying stale data.

It is a valid authority producing conflicting authority histories.

Cryptographic signatures prove that the authorised key signed each branch.

They do not select one branch as globally canonical.

⸻

3. Security goals

G1 — transferable observation evidence

A verifier should be able to confirm that independent witnesses observed a particular event without contacting those witnesses in real time.

G2 — fork accountability

If a controller or witness signs incompatible statements, anyone holding both should be able to construct a compact proof.

G3 — first-contact protection

A new verifier should gain stronger evidence than "this chain is internally valid."

G4 — no central identity ledger

The solution must not require every Mininet identity event to enter one globally ordered blockchain or central transparency service.

G5 — offline verifiability

Receipts and duplicity proofs must be independently verifiable from local bytes and public keys.

G6 — bounded state

Witnesses, clients, and gossip peers require explicit storage, retention, and message-size bounds.

G7 — witness rotation

An identity must be able to change its witness set through normal establishment events.

G8 — recovery compatibility

Witness rules must not permanently prevent lawful recovery when old witnesses disappear.

G9 — no hidden authority escalation

Witnesses attest observation and consistency.

They do not become owners of the identity and must not be able to rotate it themselves.

G10 — assurance classes

Direct-mode identities may remain valid for low-risk use, while high-authority operations require stronger witness evidence.

⸻

4. Non-goals

The design does not prove:

* one-human-one-identity;
* that a witnessed event is morally or legally correct;
* that a controller was not coerced;
* that witnesses are independent merely because they use different DIDs;
* that no hidden branch exists anywhere;
* global real-time consensus;
* exact wall-clock event ordering;
* permanent witness availability;
* protection after endpoint compromise;
* protection when the controller and sufficient witnesses collude;
* that the newest reachable branch is necessarily the lawful branch.

⸻

5. Prior art

5.1 KERI witness model

KERI uses append-only key event logs and distinguishes direct verification from an indirect mode supported by witnessed key event receipt logs.

Its indirect mode relies on witnesses observing key events and issuing receipts, giving later verifiers additional evidence about which events were accepted by the configured witness set. (arXiv)

This is the closest conceptual fit for Mininet because Mininet already uses:

* self-certifying roots;
* pre-rotation;
* append-only KELs;
* controller-selected witnesses;
* transferable event history.

What Mininet should adopt

* witness lists embedded in establishment state;
* receipt thresholds;
* witness receipts over event digests;
* witness-set rotation;
* duplicity accountability.

What Mininet should not claim automatically

* full KERI protocol equivalence;
* KERI security proofs applying unchanged to Mininet;
* global canonicality;
* witness independence without deployment policy.

⸻

5.2 Certificate Transparency

Certificate Transparency uses append-only Merkle logs, signed tree heads, inclusion proofs, and consistency proofs to make certificate issuance publicly auditable.

Its central lesson for Mininet is:

A signed log view is useful only if inconsistent views can be compared.

Research on CT gossip specifically addresses split-view attacks in which different clients receive inconsistent but individually valid log states. Efficient gossip proposals exchange signed log summaries and request consistency evidence when views differ. (arXiv)

Useful concepts

* compact signed summaries;
* append-only consistency;
* gossip comparison;
* split-view evidence;
* independent monitors.

Poor fit if copied directly

A global CT-style Merkle log for every Mininet identity would:

* centralise a core identity function;
* expose identity activity patterns;
* impose global availability requirements;
* create a censorship target;
* make private or pairwise identities publicly enumerable.

Mininet needs per-identity witnessed histories plus decentralised gossip, not one universal public identity log.

⸻

5.3 Key Transparency and verifiable registries

Key-transparency systems combine append-only logs with verifiable maps so users can audit both:

* global consistency;
* the current key associated with a particular identity.

Research on client-auditable verifiable data structures shows that users can participate in auditing through local checks and gossip rather than trusting one universal monitor. (arXiv)

Useful concepts

* local client auditing;
* compact consistency summaries;
* verifiable state transitions;
* detecting personalised views.

Limitation for Mininet

Many key-transparency systems assume:

* one service namespace;
* a central directory operator;
* globally meaningful account names;
* a server-maintained key map.

Mininet's identity root is self-certifying and should not depend on one directory operator.

⸻

5.4 Witness cosigning

CoSi proposes proactive witness cosigning, where authoritative statements are accepted only after a diverse witness group collectively observes them.

Its prototype demonstrated scalable collective signing with thousands of witnesses, though the construction depends on multisignature coordination and aggregation machinery. (arXiv)

Useful lesson

High-authority statements can be required to cross a witness-observation threshold before acceptance.

Why not use CoSi first

Mininet should not initially require:

* interactive witness rounds;
* a leader-driven signing tree;
* signature aggregation;
* new multisignature assumptions;
* all witnesses online at once.

Independent receipts are:

* simpler;
* asynchronous;
* compatible with offline operation;
* easier to audit;
* already aligned with KEL semantics.

Aggregation may be added later.

⸻

6. Recommended trust model

The protocol should distinguish four modes.

6.1 Direct mode

The verifier validates controller signatures and KEL chaining only.

Suitable for:

* local testing;
* first-device bootstrap;
* low-risk pseudonyms;
* pairwise identities with direct key confirmation.

Claim:

This KEL is internally valid under its controller keys.

It does not claim broad observation.

6.2 Pinned mode

The verifier additionally retains the highest accepted state.

Suitable for:

* repeated relationships;
* known contacts;
* device-local continuity.

Claim:

This presented history is consistent with what this verifier accepted earlier.

6.3 Witnessed mode

The presented head includes enough witness receipts to satisfy the identity's witness threshold.

Claim:

A threshold of the configured witnesses signed observation of this event.

6.4 Witnessed-and-gossiped mode

The verifier also has:

* recent cross-peer head summaries;
* no known duplicity proof;
* sufficient receipt freshness;
* witness-diversity policy satisfaction.

Claim:

This branch is internally valid, sufficiently witnessed, and no conflicting evidence is known within the verifier's gossip horizon.

This is the strongest honest claim available without global consensus.

⸻

7. Witness configuration

7.1 Witness policy belongs in establishment state

Every inception or rotation event that changes witness policy should include:

pub struct WitnessPolicy {
    pub generation: u64,
    pub witnesses: Vec<WitnessId>,
    pub threshold: u16,
}

The exact structure may use existing KEL types, but it must bind:

* ordered or canonically sorted witness identifiers;
* threshold;
* policy generation;
* activation sequence;
* optional receipt-validity policy.

7.2 Why generation is necessary

The same witness identifier may appear across several policy versions.

A receipt must prove which witness set it belongs to.

Without a generation field, an old receipt could be misapplied after:

* witness removal;
* witness addition;
* threshold change;
* recovery;
* policy reset.

7.3 Threshold constraints

Require:

1 <= threshold <= witness_count

For high-authority use, project policy should normally require:

threshold >= 2

and preferably a Byzantine-style threshold appropriate to the assumed number of faulty witnesses.

However, Mininet should not hard-code 2f + 1 unless the witness protocol is actually claiming Byzantine consensus.

Receipts are observation evidence, not necessarily a BFT agreement round.

7.4 Witness diversity metadata

Protocol validity should depend only on cryptographic identifiers and thresholds.

Verifier assurance may additionally evaluate:

* operator;
* ASN;
* cloud provider;
* jurisdiction;
* software implementation;
* governance relationship;
* funding source;
* physical region.

This metadata is imperfect and may be unavailable.

It should affect an assurance label, not basic signature validity.

⸻

8. Witness receipt format

8.1 Typed statement

Recommended conceptual type:

pub struct WitnessReceiptStatement {
    pub version: WitnessReceiptVersion,
    pub identity: Did,
    pub sequence: u64,
    pub event_digest: EventDigest,
    pub prior_event_digest: Option<EventDigest>,
    pub event_kind: KeyEventKind,
    pub witness_policy_generation: u64,
    pub witness_id: WitnessId,
    pub observed_epoch: u64,
}

A witness signs only this typed statement through:

sign_witness_receipt(statement)

8.2 Why bind the identity

An event digest may be globally unique, but explicit identity binding prevents cross-protocol or cross-identity ambiguity.

8.3 Why bind the sequence

The receipt must prove the event's place in the identity's log.

8.4 Why bind the prior digest

This makes the witness state transition explicit and simplifies proof that the witnessed event extends a particular branch.

8.5 Why bind event kind

An inception, ordinary rotation, recovery, delegation, and interaction event may have different witness policies.

8.6 Why bind policy generation

A witness removed in generation 4 must not have its generation-3 receipt counted under generation 4.

8.7 Why use a coarse observed epoch

The receipt may need freshness evaluation, but an exact timestamp:

* increases clock dependency;
* leaks witness timing;
* may create false ordering claims;
* can become a tracking surface.

A coarse network epoch is preferable to claiming authoritative wall-clock time.

8.8 Receipt output

pub struct WitnessReceipt {
    pub statement: WitnessReceiptStatement,
    pub signature: Signature,
}

A receipt does not contain arbitrary notes or extensible untyped metadata.

⸻

9. Witness state machine

For each witnessed identity, a witness retains at least:

pub struct WitnessIdentityState {
    pub identity: Did,
    pub accepted_sequence: u64,
    pub accepted_event_digest: EventDigest,
    pub witness_policy_generation: u64,
}

Potentially it also retains:

* recent event digests;
* issued receipt digests;
* detected duplicity proofs;
* policy-transition state;
* retention metadata.

9.1 Receive event

A witness receives a complete candidate KEL or a candidate event plus enough prior state.

9.2 Verify chain

It verifies:

* self-certifying inception;
* signatures;
* sequence;
* prior digest;
* pre-rotation;
* witness-policy transition;
* recovery rules;
* event canonicality.

9.3 Compare local state

Exact duplicate

If the event matches the already accepted digest:

* return the existing receipt;
* do not issue a semantically different receipt.

Valid direct successor

If it extends the accepted event correctly:

* accept;
* update state;
* issue receipt.

Stale ancestor

If it is older than accepted state:

* do not issue a new receipt;
* optionally return a head hint or existing receipt;
* avoid revealing excessive history to unauthenticated requesters.

Conflicting same-sequence event

If the witness already accepted another digest at the same sequence:

* do not receipt it;
* construct or record a duplicity proof;
* gossip the proof according to policy.

Conflicting descendant

If the event builds on a branch inconsistent with local accepted state:

* reject;
* request missing intermediate evidence if ambiguity remains;
* produce a fork proof when sufficient signed material is present.

Recovery event

Evaluate under explicit recovery rules.

A recovery event must not bypass witness consistency merely because it uses a special event kind.

⸻

10. Witnessed event certificate

A verifier should not need to process one loose receipt at a time from arbitrary sources.

Recommended type:

pub struct WitnessedEventCertificate {
    pub version: WitnessCertificateVersion,
    pub identity: Did,
    pub sequence: u64,
    pub event_digest: EventDigest,
    pub witness_policy_generation: u64,
    pub receipts: Vec<WitnessReceipt>,
}

Validation requires:

1. every receipt statement matches the same event;
2. every witness belongs to the active policy;
3. no witness is counted twice;
4. each signature verifies;
5. the threshold is met;
6. receipt epochs satisfy freshness policy;
7. all encodings are canonical;
8. event digest matches the presented KEL event.

10.1 Do not require ordered receipt arrival

Receipts should be canonically sorted in the certificate for deterministic encoding.

10.2 Do not aggregate signatures initially

Individual signatures increase size but preserve:

* simple verification;
* precise accountability;
* easy identification of equivocating witnesses;
* algorithm agility;
* mixed-suite witness migration.

⸻

11. Duplicity proof taxonomy

11.1 Controller same-sequence duplicity

Two different valid controller-signed events:

identity D
sequence n
digest A != digest B

Recommended proof:

pub struct ControllerDuplicityProof {
    pub identity: Did,
    pub sequence: u64,
    pub event_a: KeyEvent,
    pub event_b: KeyEvent,
}

Validation:

* same identity;
* same sequence;
* different canonical digest;
* both controller-authorised under the same prior state or otherwise demonstrably incompatible.

11.2 Conflicting successor proof

Two events may have different sequence numbers yet form incompatible branches from one prior event.

The proof includes the minimum branch events needed to show divergence.

11.3 Witness equivocation proof

One witness signs receipts for conflicting event digests at the same identity, sequence, and policy generation.

pub struct WitnessEquivocationProof {
    pub witness_id: WitnessId,
    pub receipt_a: WitnessReceipt,
    pub receipt_b: WitnessReceipt,
}

11.4 Conflicting threshold certificates

Two different event digests at the same sequence both have valid threshold certificates.

This proves one of:

* controller duplicity;
* overlapping witness dishonesty;
* witness-policy ambiguity;
* catastrophic state inconsistency.

The proof should retain both certificates.

11.5 Stale presentation evidence

A stale KEL alone is not duplicity.

A presenter may be offline or unaware.

A stale presentation becomes stronger evidence only when the presenter claims current authority despite a newer sufficiently witnessed head.

Mininet should not punish ordinary offline peers as equivocators.

⸻

12. Duplicity proof properties

A valid duplicity proof must be:

* self-contained;
* deterministic;
* minimal;
* bounded;
* independently verifiable;
* impossible to create from one honest branch alone;
* content-addressed;
* gossipable;
* persistable.

It should not require access to:

* one witness's private database;
* wall-clock logs;
* a central adjudicator;
* private user metadata.

⸻

13. Gossip protocol

13.1 Gossip should be ordinary network hygiene

KEL gossip should piggyback on:

* peer sync;
* identity resolution;
* forge interactions;
* governance messages;
* witness communication;
* release verification;
* mailbox or relay sessions where appropriate.

A separate always-on global gossip service is not necessary initially.

13.2 Head summary

Recommended compact summary:

pub struct KelHeadSummary {
    pub version: KelHeadSummaryVersion,
    pub identity: Did,
    pub sequence: u64,
    pub event_digest: EventDigest,
    pub witness_policy_generation: u64,
    pub certificate_digest: Option<CertificateDigest>,
    pub known_duplicity_digest: Option<DuplicitySetDigest>,
}

13.3 Comparison outcomes

Same sequence and digest

No action.

Local sequence lower

Request:

* missing KEL suffix;
* head witness certificate;
* relevant duplicity proofs.

Local sequence higher

Offer a bounded head hint.

Same sequence, different digest

Request both branch events and certificates immediately.

This is the highest-priority duplicity condition.

Different sequence but incompatible ancestry

Request consistency evidence or the missing branch.

13.4 Gossip privacy

A global "which identities do you know?" exchange would leak social and application relationships.

Therefore:

* gossip an identity only in contexts where it is already relevant;
* use bounded interest sets;
* avoid global enumeration;
* permit public high-authority identities to be widely monitored;
* treat pairwise pseudonyms and private identities more narrowly.

13.5 Duplicity-proof propagation

Once a valid proof is known, its content ID may be announced broadly.

For public authority roots, full proof propagation should be aggressive.

For private or pairwise identities, propagation must be scoped to avoid unnecessary identity exposure.

⸻

14. Gossip horizon and honest freshness

The protocol must not say:

This is the globally freshest KEL.

It may say:

This is the freshest sufficiently witnessed KEL known within this verifier's configured gossip horizon.

The horizon should be described using:

* certificate receipt epoch;
* last gossip comparison;
* number of independent peers;
* witness threshold;
* diversity class;
* maximum accepted staleness.

Example:

Assurance:
  witnessed threshold: 3 of 5
  certificate age: 2 epochs
  independent gossip peers: 4
  no known duplicity proof

This is more honest than one boolean is_fresh.

⸻

15. First-contact verification

For a first-time verifier, recommended procedure:

Step 1 — verify the full KEL

Check all ordinary KEL rules from inception.

Step 2 — identify the active witness policy

Read the latest valid establishment event.

Step 3 — verify the head certificate

Require enough receipts for the active threshold.

Step 4 — verify policy continuity

Ensure witness-set changes were themselves authorised and sufficiently witnessed under the previous policy.

Step 5 — check known duplicity evidence

Consult local proof store and relevant gossip peers.

Step 6 — apply assurance policy

Example classes:

pub enum KelAssurance {
    Direct,
    Pinned,
    Witnessed,
    WitnessedRecent,
    WitnessedRecentAndGossiped,
    DuplicityDetected,
}

Step 7 — gate authority

Examples:

* public post: Direct may be enough;
* contact continuity: Pinned;
* repository merge approval: WitnessedRecent;
* validator admission: WitnessedRecentAndGossiped;
* treasury recovery: stronger bespoke policy.

⸻

16. Witness selection

16.1 Controller-selected witnesses

The identity should choose its witness set.

This preserves self-sovereignty.

16.2 Verifier minimums

A verifier is not required to consider every configured witness set equally trustworthy.

It may require:

* minimum threshold;
* minimum number of operators;
* no single-provider majority;
* acceptable freshness;
* recognised witness capabilities.

16.3 Public witness services

Some witnesses may operate as public infrastructure.

Risks:

* concentration;
* surveillance;
* censorship;
* legal coercion;
* correlated failure.

16.4 Community witnesses

Communities may witness member identities.

Risks:

* social collusion;
* exclusion;
* small anonymity;
* common administration.

16.5 Personal witnesses

A user may choose independent friends, devices, organisations, or hosted services.

Risks:

* availability;
* low operational discipline;
* common recovery compromise.

16.6 Recommended default

For important roots, choose witnesses across at least:

* three operators;
* two infrastructure providers or networks;
* two jurisdictions where practical;
* one locally controlled or community-controlled witness;
* one independently monitored public witness.

These are deployment recommendations, not cryptographic facts.

⸻

17. Witness rotation

17.1 Rotation event

A valid establishment event may:

* add witnesses;
* remove witnesses;
* change threshold;
* increment policy generation.

17.2 Old-policy authorisation

The witness-policy-changing event should require certification under the old active policy.

Otherwise, a compromised controller could remove honest witnesses before presenting a fork.

17.3 New-policy acknowledgement

For high-assurance transitions, also require receipts from enough new witnesses to prove they accepted responsibility.

This yields:

old witness threshold
AND
new witness readiness threshold

17.4 Dead witness recovery

A witness set may become unavailable.

The protocol needs a recovery path that cannot be triggered casually.

Possible requirements:

* controller recovery threshold;
* waiting period;
* retained evidence of witness unavailability;
* higher replacement threshold;
* broad gossip announcement;
* stronger monitoring.

No witness set should be able to hold an identity permanently hostage.

⸻

18. Recovery and duplicity

Recovery is the hardest special case because it intentionally overrides ordinary current-key continuity.

18.1 Recovery must identify the displaced branch

A recovery event should bind:

* the last accepted establishment event;
* the recovery authority;
* the reason code class;
* replacement keys;
* replacement witness policy;
* recovery epoch.

18.2 Recovery does not erase duplicity

If the old controller signed conflicting events, the proofs remain valid after recovery.

18.3 Witness response to recovery

Witnesses should:

1. verify the recovery threshold;
2. verify permitted recovery sequence;
3. mark old current authority retired;
4. receipt the recovery event;
5. retain conflicting-branch evidence;
6. gossip the recovery head and relevant proof identifiers.

18.4 Competing recovery events

Two valid-looking recovery events are a severe conflict.

The protocol must not resolve this through "highest timestamp wins."

It requires the recovery rules already established in the prior KEL state.

⸻

19. Transparency log options

19.1 One global log

Benefit

Easy monitoring and global consistency.

Problems

* identity enumeration;
* centralisation;
* censorship;
* availability dependency;
* private pseudonym leakage;
* global metadata history.

Decision

Reject as mandatory infrastructure.

19.2 Per-witness append-only logs

Each witness maintains an append-only Merkle log of receipts.

Benefits

* compact signed roots;
* inclusion proofs;
* witness accountability;
* efficient monitoring;
* split-view evidence.

Problems

* more implementation;
* log consistency gossip;
* witness can show split views;
* private identity activity leakage if entries are public.

Decision

Recommended as a later witness-service enhancement, not required for v1.

19.3 Public log only for high-authority roots

Governance, release, treasury, and validator roots may opt into public transparency logs.

Decision

Strongly recommended.

These identities already exercise public authority, so transparency leakage is less problematic.

19.4 Private receipt log

Private witnesses may provide inclusion and consistency proofs only to authorised peers.

This preserves privacy but weakens public monitoring.

⸻

20. Merkle log design for witnesses

A future witness log may append:

receipt digest
identity digest or scoped label
sequence
event digest
policy generation

The signed tree head contains:

pub struct WitnessLogHead {
    pub witness_id: WitnessId,
    pub tree_size: u64,
    pub root_hash: Digest,
    pub log_epoch: u64,
    pub signature: Signature,
}

Clients may request:

* inclusion proof for a receipt;
* consistency proof between two heads;
* compact log range;
* known equivocation proofs.

A witness that signs two inconsistent log heads becomes accountable.

Certificate Transparency demonstrates the general value of signed log summaries and consistency comparison, while gossip research shows that clients must actually compare views to detect split-view behaviour. (arXiv)

⸻

21. Why receipts alone are not enough

Suppose a malicious witness signs:

receipt for branch A to Alice
receipt for branch B to Bob

If Alice and Bob never compare evidence, both accept their view.

Therefore:

receipts provide evidence
gossip turns conflicting evidence into detection

This is the central lesson from transparency systems.

A receipt protocol without gossip provides accountability only after accidental discovery.

A gossip protocol without signed receipts exchanges claims that may not be provable.

Mininet needs both.

⸻

22. Why gossip alone is not enough

Unsigned peers may claim:

* "I saw a newer log";
* "that witness equivocated";
* "this branch is stale."

Without signed events and receipts, these are denial-of-service opportunities.

Every severe gossip claim must resolve to verifiable evidence.

The network may gossip hints, but authority decisions rely on proofs.

⸻

23. Receipt collection

23.1 Controller push

After creating an event, the controller submits it to configured witnesses.

23.2 Witness fetch

Witnesses may fetch missing prior events from:

* controller;
* peers;
* other witnesses;
* KEL mirrors.

23.3 Asynchronous threshold

The event becomes sufficiently witnessed when enough receipts are collected.

Witnesses need not coordinate with each other.

23.4 Certificate assembly

The controller, relay, mirror, or any peer may assemble the certificate from valid receipts.

Certificate assembly conveys no authority.

23.5 Partial certificates

Partial receipts may be propagated, but should not be presented as threshold certification.

⸻

24. Event finality semantics

Mininet should avoid calling witnessed KEL events "final" without qualification.

Recommended terms:

Locally valid
Pinned
Threshold witnessed
Gossip corroborated
Duplicity detected
Recovered

A threshold-witnessed event may later be followed by a valid rotation or recovery.

It is not immutable finality in the consensus sense.

⸻

25. Denial-of-service risks

Witnesses may face:

* fake identity floods;
* giant KEL submissions;
* invalid-signature floods;
* repeated receipt requests;
* long-history replay;
* proof gossip floods;
* fork explosions;
* storage exhaustion.

Controls

* strict event-size caps;
* bounded KEL suffix requests;
* signature verification ordering;
* receipt caching;
* per-identity quotas;
* anonymous rate credentials;
* service-chosen proof-of-work;
* capability-based witness admission;
* paid or community-subsidised witness service;
* proof deduplication by content ID;
* bounded proof-store retention.

A witness should not need to retain every arbitrary invalid submission.

⸻

26. Privacy considerations

26.1 Witnesses learn identity activity

A witness sees:

* that the identity exists;
* event timing;
* rotation frequency;
* recovery;
* witness-set changes.

26.2 Pairwise identities

A pairwise pseudonym should not automatically use the same public witness set as the root identity.

That would create correlation.

26.3 Scoped witnesses

An identity may derive or select witness relationships per scope.

26.4 Receipt distribution

A receipt contains an identity and event digest.

Broadly publishing it can make a private identity enumerable.

Therefore:

* public authority roots: broad publication;
* public social identities: policy choice;
* private pairwise roots: scoped distribution;
* contact-only identities: direct exchange.

26.5 Witness logs

Public Merkle logs should not automatically expose every private identity.

Use:

* scoped logs;
* blinded or keyed identity labels;
* private inclusion proofs;
* opt-in public transparency.

⸻

27. Cost model

Witnessing purchases stronger first-contact assurance with:

* witness bandwidth;
* signature verification;
* receipt signatures;
* storage;
* gossip traffic;
* availability;
* operator diversity;
* monitoring.

Low-cost mode

* no witnesses;
* local pinning only.

Standard witnessed mode

* 3 witnesses;
* threshold 2;
* receipts on establishment events only.

Strong mode

* 5–7 witnesses;
* threshold 3–5;
* establishment and recovery receipts;
* periodic gossip;
* independent operators.

High-authority mode

* stronger threshold;
* public receipt log;
* external monitors;
* aggressive gossip;
* multiple jurisdictions;
* receipt freshness requirement.

Not every interaction event needs witness receipts.

A practical first version should witness:

* inception;
* rotation;
* recovery;
* witness-policy change;
* high-authority delegation.

Routine low-authority interaction events can remain controller-signed unless application policy requires otherwise.

⸻

28. Alternatives considered

A. Pinning only

Advantage

Simple and already useful.

Failure

No first-contact protection.

Decision

Keep as baseline, not complete solution.

⸻

B. Highest sequence wins

Advantage

Simple conflict rule.

Failure

An attacker can manufacture a longer fork with a compromised key.

Sequence proves position inside a branch, not legitimacy across branches.

Decision

Reject.

⸻

C. Latest timestamp wins

Advantage

Appears intuitive.

Failure

Clocks are forgeable, inconsistent, and not authority.

Decision

Reject.

⸻

D. Global blockchain ordering

Advantage

One canonical order.

Failure

High cost, central dependency, privacy leakage, consensus coupling, and unnecessary globalisation of identity events.

Decision

Reject as mandatory identity infrastructure.

⸻

E. One trusted witness

Advantage

Simple.

Failure

Single point of compromise, censorship, and equivocation.

Decision

Permit only as low-assurance deployment.

⸻

F. Threshold independent receipts

Advantage

Simple, asynchronous, auditable, KERI-aligned.

Failure

Larger certificates and no automatic split-view discovery.

Decision

Recommend for v1.

⸻

G. CoSi-style collective signature

Advantage

Compact client verification and proactive witness exposure.

Failure

Interactive coordination and new aggregation complexity.

Decision

Future optimisation only.

⸻

H. Per-witness Merkle logs

Advantage

Strong witness accountability and efficient monitoring.

Failure

More protocol and privacy complexity.

Decision

Recommended Phase 2 enhancement.

⸻

I. Global CT-style identity log

Advantage

Strong global monitoring.

Failure

Centralises and enumerates identity activity.

Decision

Use only optionally for public authority roots.

⸻

J. Gossip unsigned head claims

Advantage

Cheap.

Failure

Easy denial of service and no transferable proof.

Decision

Use only as retrieval hints.

⸻

29. Recommended protocol phases

Phase 0 — design and state audit

Document:

* current pin semantics;
* KEL event types;
* establishment state;
* recovery;
* witness placeholders;
* all call sites that treat a KEL head as current.

Produce:

docs/design/kel-witness-receipts-and-duplicity-gossip.md

Phase 1 — receipt types

Implement:

* WitnessPolicy;
* WitnessReceiptStatement;
* WitnessReceipt;
* WitnessedEventCertificate;
* strict canonical encoding;
* signature verification;
* no network service.

Phase 2 — in-memory witness state machine

Implement:

* first-seen event acceptance;
* direct successor verification;
* duplicate idempotence;
* stale rejection;
* conflict detection;
* receipt issuance;
* controller duplicity proof;
* witness equivocation proof.

Phase 3 — KEL verification integration

Add assurance output:

Direct
Pinned
Witnessed
DuplicityDetected

Do not replace ordinary KEL validity with one boolean.

Phase 4 — receipt collection protocol

Add typed messages:

SubmitEventForWitnessing
WitnessReceiptResponse
WitnessConflictResponse
FetchKelSuffix
FetchWitnessCertificate

Phase 5 — gossip summaries

Piggyback:

* head summaries;
* certificate digests;
* duplicity-proof digests.

Implement targeted fetch on disagreement.

Phase 6 — persistent witness service

Add:

* durable state;
* crash recovery;
* receipt index;
* bounded retention;
* identity quotas;
* transport authentication;
* monitoring.

Phase 7 — witness rotation and recovery

Implement:

* policy generation;
* old-threshold certification;
* new-witness acknowledgement;
* unavailable-witness recovery;
* recovery-event certification.

Phase 8 — public-authority transparency

For governance, release, validator, and treasury roots:

* per-witness append-only receipt logs;
* signed Merkle heads;
* inclusion proofs;
* consistency proofs;
* independent monitors.

Phase 9 — adversarial network simulation

Simulate:

* controller forks;
* witness collusion;
* network partitions;
* eclipse attacks;
* delayed gossip;
* witness churn;
* recovery conflicts;
* stale first-contact verifiers;
* selective proof suppression.

Phase 10 — external review

Review:

* trust claims;
* witness state machine;
* fork proof completeness;
* recovery;
* gossip privacy;
* denial of service;
* threshold semantics;
* witness-log consistency.

⸻

30. Required adversarial tests

Receipt tests

1. Valid receipt verifies.
2. Wrong identity fails.
3. Wrong sequence fails.
4. Wrong event digest fails.
5. Wrong prior digest fails.
6. Wrong event kind fails.
7. Wrong witness-policy generation fails.
8. Non-member witness does not count.
9. Duplicate witness receipt counts once.
10. Unknown receipt version fails.
11. Trailing bytes fail.
12. Oversized receipt fails before allocation.

Witness-state tests

1. First valid event is accepted according to bootstrap rules.
2. Exact repeated event is idempotent.
3. Direct valid successor is receipted.
4. Stale ancestor receives no new receipt.
5. Same-sequence conflict is detected.
6. Divergent descendant is rejected.
7. Missing intermediate event triggers bounded fetch.
8. Invalid controller signature fails before state mutation.
9. Witness restart preserves accepted head.
10. Concurrent submissions cannot produce conflicting honest receipts.

Certificate tests

1. Threshold certificate verifies.
2. Below-threshold certificate fails assurance.
3. Receipt from old policy generation does not count.
4. Receipt for another event does not count.
5. Duplicate receipts do not inflate threshold.
6. Certificate receipt order does not affect digest.
7. Unknown witness suite fails safely.
8. Stale certificate receives lower assurance.
9. Certificate cannot be moved to another identity.
10. Event mutation invalidates the certificate.

Duplicity tests

1. Two same-sequence conflicting events produce a valid controller proof.
2. Identical events do not produce a proof.
3. Two conflicting witness receipts produce a witness equivocation proof.
4. Two compatible sequential events are not duplicity.
5. Two conflicting threshold certificates are retained.
6. A stale branch alone is not labelled duplicity.
7. Recovery does not delete prior duplicity evidence.
8. Proof encoding is canonical.
9. Invalid proof cannot trigger authority revocation.
10. Proof content ID deduplicates gossip.

Gossip tests

1. Equal heads cause no fetch.
2. Lower local sequence fetches suffix.
3. Higher local sequence offers bounded hint.
4. Same sequence/different digest triggers conflict retrieval.
5. Incompatible ancestry is detected.
6. Malicious unsigned hint cannot change authority.
7. Proof is verified before storage.
8. Gossip is scoped to relevant identities.
9. Private identity lists are not globally enumerated.
10. Repeated gossip remains bounded.

Witness-rotation tests

1. New witness set cannot activate without old-policy authority.
2. Removed witness receipts stop counting.
3. New policy generation is monotonic.
4. Threshold cannot exceed witness count.
5. High-assurance mode requires new-witness acknowledgement.
6. Unavailable-witness recovery follows explicit rules.
7. Controller cannot silently reduce threshold through an interaction event.
8. Conflicting witness-policy rotations produce duplicity evidence.

Partition tests

1. Honest witnesses on opposite partitions may temporarily receipt different forks only if controller equivocation reaches them.
2. After partition healing, gossip constructs proofs.
3. One isolated verifier does not claim global freshness.
4. Conflicting certificates remain detectable.
5. Network delay does not turn ordinary staleness into false duplicity.

⸻

31. Assurance policy recommendations

Public posts and low-risk objects

Minimum:

Direct or Pinned

Contact identity continuity

Minimum:

Pinned

Preferred:

Witnessed

Forge approvals

Minimum:

WitnessedRecent

Release authority

Minimum:

WitnessedRecentAndGossiped

plus release transparency requirements.

Validator identity

Minimum:

WitnessedRecentAndGossiped

Treasury and constitutional recovery

Require:

* strong witness threshold;
* recovery threshold;
* public proof monitoring;
* multiple operators;
* no known duplicity;
* stricter freshness.

⸻

32. Failure handling

32.1 Duplicity detected

Do not automatically choose one branch.

Instead:

* mark authority disputed;
* reject new high-authority actions;
* preserve both branches;
* distribute proof;
* invoke recovery or governance policy;
* permit low-risk read-only historical inspection.

32.2 Insufficient receipts

Return:

locally valid but insufficiently witnessed

Do not call the KEL invalid unless application policy requires witnesses.

32.3 Witness unavailable

Try alternative configured witnesses.

Do not silently reduce threshold.

32.4 Gossip unavailable

Reduce assurance.

Do not claim no duplicity exists.

32.5 Conflicting certificates

Freeze high-authority acceptance and require recovery/adjudication.

⸻

33. Witness accountability

A witness-equivocation proof may justify:

* removal from future policies;
* reputation reduction;
* exclusion from high-assurance verifier sets;
* governance action;
* loss of service bond where independently verifiable economic rules exist.

It must not automatically justify:

* confiscation without due process;
* identity deanonymisation;
* unrelated censorship;
* punishment based on unsigned accusations.

⸻

34. Proposed Rust boundaries

Conceptual module structure:

did-mini/
  witness/
    policy.rs
    receipt.rs
    certificate.rs
    state.rs
    duplicity.rs
    gossip.rs

Typed operations:

WitnessState::observe_event(...)
WitnessState::issue_receipt(...)
WitnessedEventCertificate::verify(...)
ControllerDuplicityProof::verify(...)
WitnessEquivocationProof::verify(...)
KelHeadSummary::compare(...)

Avoid:

sign(bytes)
accept_latest(event)
resolve_fork_by_timestamp(...)

⸻

35. Decision-log recommendation

Decision

Mininet extends KEL verification with asynchronous witness receipts and proof-carrying duplicity gossip.

An establishment event defines a versioned witness policy containing a witness set and threshold. Each witness maintains monotonic first-seen state per identity and may sign a typed receipt binding the identity, sequence, event digest, prior digest, event kind, witness-policy generation, and observation epoch.

A KEL event is threshold witnessed when accompanied by enough valid receipts from the configured witness set.

Conflicting controller events or conflicting witness receipts produce compact, independently verifiable duplicity proofs. Peers gossip KEL-head summaries and proof identifiers during ordinary relevant interactions; disagreements trigger targeted evidence retrieval.

Reason

Local pinning protects returning verifiers but cannot protect a verifier with no prior head against a valid-looking personalised fork. Witness receipts provide transferable evidence of observation, and gossip makes conflicting views discoverable without introducing a mandatory global identity ledger.

Constitutional impact

* strengthens identity continuity and M3-style anti-duplicity enforcement;
* preserves self-certifying ownership;
* preserves offline verification;
* does not make witnesses identity owners;
* does not create one central registry;
* does not claim global real-time consensus;
* preserves direct-mode identities at lower assurance;
* adds stronger requirements only where authority policy demands them.

Failure point

The design fails if witnesses can sign conflicting receipts without detectable evidence, if old witness receipts count under new policy generations, if private identity gossip becomes globally enumerable, if recovery silently bypasses witness consistency, or if "threshold witnessed" is marketed as globally freshest.

Required follow-up

* exact SPEC-01 reconciliation;
* receipt type implementation;
* witness state machine;
* duplicity-proof corpus;
* gossip protocol;
* recovery integration;
* witness diversity policy;
* public-authority transparency logs;
* adversarial simulation;
* external review.

⸻

36. Final recommendations

Adopt now

1. Keep pinning.
2. Add typed witness policies to establishment state.
3. Add witness-policy generations.
4. Add typed independent witness receipts.
5. Bind receipts to identity, sequence, current digest, prior digest, event kind, policy generation, and epoch.
6. Add threshold-witnessed event certificates.
7. Make receipt issuance idempotent.
8. Enforce first-seen monotonic witness state.
9. Add controller duplicity proofs.
10. Add witness equivocation proofs.
11. Add conflicting-certificate proofs.
12. Gossip compact head summaries.
13. Retrieve full evidence only on disagreement.
14. Keep private-identity gossip scoped.
15. Require witness certification for high-authority roots.
16. Preserve direct and pinned lower-assurance modes.
17. Treat gossip horizon and freshness explicitly.
18. Require old-policy certification for witness-set changes.
19. Integrate lawful recovery without erasing proof history.
20. Use independent ordinary signatures first.
21. Add durable witness state and crash recovery.
22. Simulate network partitions and eclipse attacks.
23. Commission external review before high-value enforcement.

Adopt later

1. Per-witness append-only Merkle receipt logs.
2. Signed witness-log heads.
3. Inclusion and consistency proofs.
4. Independent witness monitors.
5. Public transparency for governance, release, validator, and treasury roots.
6. Signature aggregation after the simple protocol is stable.
7. Automated witness-diversity scoring.
8. Economic witness bonds only with independently verifiable evidence.
9. Cross-network gossip relays.
10. Post-quantum witness receipts.

Defer

1. One global identity transparency log.
2. Universal publication of private identity receipts.
3. Interactive collective signing for every event.
4. Witness receipts for every low-value interaction event.
5. Fully anonymous witness service.
6. Complex witness reputation economics.
7. BFT consensus among witnesses.
8. Global wall-clock ordering.
9. Automatic branch selection after duplicity.
10. Threshold signature aggregation.

Reject

1. Pinning as the complete solution.
2. Highest sequence wins.
3. Latest timestamp wins.
4. Longest KEL wins.
5. One mandatory trusted witness.
6. Unsigned duplicity accusations.
7. Silent threshold reduction.
8. Counting old-generation receipts under a new policy.
9. Treating staleness alone as malicious duplicity.
10. Letting witnesses rotate the identity.
11. Erasing conflicting history after recovery.
12. Globally gossiping every pairwise identity.
13. Calling witnessed events consensus-final.
14. Claiming absence of known proof means no hidden fork exists.
15. Building novel signature aggregation before the receipt semantics are correct.

⸻

37. Essay: A Signature Proves Who Spoke, Not What Everyone Heard

A digital signature answers a precise question:

Did the holder of this private key authorise these bytes?

That is an extraordinarily useful answer.

It is not the answer to every question an identity system must ask.

Suppose one authorised key signs two different rotation events. One event appoints key A. The other appoints key B. Both signatures verify. Both events extend the same prior history. Both are cryptographically authentic.

The problem is not forgery.

The problem is that the authentic authority spoke differently to different audiences.

A new verifier who sees only one branch cannot detect the contradiction. Its cryptography works perfectly and still leaves it deceived.

This is the limit of local verification.

A key event log provides ordered continuity inside one presented history. Each event names its predecessor. Pre-rotation restricts which keys may legitimately take control next. Hash chaining prevents invisible edits.

But no structure inside one branch can prove that another branch was not shown somewhere else.

To detect equivocation, observations must meet.

Pinning is the smallest form of meeting. A device remembers what it saw yesterday and rejects a contradiction today. This is powerful because it turns continuity into local memory.

Yet the first meeting has no yesterday.

A person installing Mininet on a new phone, joining a community for the first time, verifying a maintainer's identity, or evaluating a validator root has no pinned history. They may receive a perfectly valid branch constructed specifically for them.

Witnesses address this gap by turning observation into signed evidence.

A witness does not certify that the controller is honest. It does not own the identity. It does not decide what the event should say. It says only:

Under this witness policy, I observed and accepted this event as the next valid point in the log I track.

One receipt is evidence from one observer. A threshold certificate shows that several configured observers reached the same event.

This changes the attacker's task.

Without witnesses, a stolen or malicious controller key can create a private fork for one victim.

With independent witnesses, the attacker must either:

* convince enough witnesses to follow the private fork;
* isolate the victim from honest receipt evidence;
* corrupt the witness set;
* exploit witness-policy rotation;
* or risk producing conflicting signed evidence.

The last outcome is especially important. A system cannot always prevent dishonesty, but it can make dishonesty leave a durable proof.

If a controller signs two events at the same sequence, the two events are the proof.

If a witness signs receipts for both events, the two receipts are the proof.

No administrator needs to interpret server logs. No witness needs to confess. No central court needs privileged database access.

The evidence consists of the conflicting authority statements themselves.

But signed evidence is useful only when it travels.

Certificate Transparency demonstrated the same underlying problem at a different scale. An append-only log can show one valid tree to Alice and another valid tree to Bob. Each view may verify locally. Split-view detection requires clients, monitors, or network observers to compare signed summaries. (arXiv)

Witness receipts without gossip have the same weakness. A malicious witness may sign branch A for one community and branch B for another. If the communities never compare receipts, the accountability mechanism remains dormant.

Gossip is therefore not incidental transport overhead. It is the process by which isolated truths become shared evidence.

Mininet should keep that gossip compact.

Peers do not need to exchange every identity log they know. Doing so would leak social relationships and make pairwise identities enumerable. They can exchange head summaries only for identities already relevant to their interaction. If summaries agree, no further traffic is needed. If they disagree, the peers request the exact events, receipts, or proofs required to explain the divergence.

The protocol should also resist the temptation to overstate what gossip proves.

A verifier that has asked five peers and checked three witnesses has a stronger view than one that has checked nothing. It still does not know that no hidden branch exists on a disconnected island.

The honest statement is not:

This is the globally freshest log.

It is:

This is the freshest sufficiently witnessed log known within my current gossip horizon, and I know of no conflicting proof.

The wording matters because distributed systems are full of absence claims that are impossible to prove absolutely.

"No newer event exists" is one of them.

A verifier can prove that a particular event exists. It can prove that witnesses signed it. It can prove that two events conflict. It can prove that one chain extends another.

It cannot prove that no isolated machine holds another valid event unless the system introduces global consensus—and Mininet should not globalise every identity action merely to obtain that statement.

Witnesses instead create priced assurance.

A low-risk pseudonym may rely only on signatures. A long-term contact may rely on local pinning. A repository maintainer may require a recent witness certificate. A validator or treasury recovery root may require a diverse threshold, public receipt logs, and broad gossip.

The stronger authority buys more independent observation.

This also prevents witnesses from becoming a hidden identity government.

The controller still creates events. The controller still rotates keys. The controller still chooses a witness policy. Witnesses cannot create authority events by themselves.

They can refuse to receipt an event, which creates an availability risk. Recovery rules must therefore permit replacement of unavailable witnesses without allowing the controller to silently remove honest witnesses before equivocating.

That transition is one of the most sensitive parts of the design. The old witness policy should certify the event that replaces it, while the new witnesses acknowledge their role. Otherwise, witness rotation becomes an escape hatch from accountability.

Recovery deserves the same care.

A lawful recovery may need to override a compromised active key. It must not delete evidence that the old key equivocated. History remains append-only even when authority changes.

The distinction between authority and evidence must hold throughout:

* controller signatures establish authority;
* witness receipts establish observation;
* gossip establishes evidence circulation;
* duplicity proofs establish contradiction;
* recovery establishes lawful continuation;
* none of these alone establishes humanity, morality, or universal truth.

This separation is what allows Mininet to strengthen its identity layer without creating one central ledger or one mandatory trusted witness.

The deepest lesson is that cryptographic identity is not only a chain of authorised statements.

It is also a social problem of who had the opportunity to see those statements, who can prove what they saw, and whether conflicting audiences can eventually discover one another.

A signature proves who spoke.

A witness receipt proves someone else heard.

Gossip is how the network discovers that the speaker told two stories.

The best next engineering deliverable is a design-only PR, followed by a small receipt/proof type PR, then an in-memory witness state-machine PR, and only afterward network gossip. The dangerous mistake would be starting with a witness daemon or Merkle log before freezing exactly what a receipt means, what constitutes duplicity, and what a first-contact verifier is allowed to claim.
