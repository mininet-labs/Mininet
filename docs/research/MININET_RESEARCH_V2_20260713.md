> **Provenance note (added on commit, not part of the original document):**
> founder-supplied research, uploaded 14 July 2026 and adopted as direction
> by D-0094 (`docs/DECISION_LOG.md`). Per `CLAUDE.md`'s canonical-source
> ordering this is supporting research, **subordinate** to
> `docs/FOUNDER_DIRECTIVES.md`, `docs/INVARIANTS.md`,
> `docs/DECISION_LOG.md`, and `docs/FAILURE_BOOK.md` — it may refine
> sequencing and scope; it does not itself weaken any frozen invariant, and
> nothing below should be read as claiming any protocol property already
> exists until D-0094 or a later decision confirms what actually shipped.
> See `docs/STATUS.md` §6 (Privacy) for implementation truth.

# MiniNet — Privacy, Distribution, Suppression Resistance, and Human Evidence

**A resource-cost architecture: every property has a price, and every price has a floor**

Version 2.0 · 13 July 2026 · Research document — not an implementation claim

---

## How this version differs from the previous draft

The previous draft was already sound. Its structure — one shared substrate, four planes, named privacy tiers, honest personhood — is kept. This version rebuilds it around a single organizing principle and makes that principle quantitative:

> **The cost doctrine.** Every privacy, availability, and integrity property MiniNet can offer is *purchasable*. You buy it with measurable network resources — bandwidth, latency, storage, redundancy, compute, jurisdictional diversity, and unlinkable money. Spend more, and the residual risk shrinks. This is true for nearly everything the earlier draft described as "impossible" — those properties were not impossible, they were *unpriced*.
>
> **The floor.** A short, explicitly enumerated set of residuals do **not** reach zero at any finite price. They are not failures of the design; they are properties of physics, economics, and human beings. The honest move is not to hide them behind a premium label but to *name each one and state what it costs to push it as low as it will go.*

So the earlier document's central caution — "MiniNet cannot guarantee that nobody can ever identify a user" — is not softened here. It is made **operational**: for each way a user could be identified, this document gives the mechanism that suppresses it, the resource cost of that mechanism, and the residual that survives an unlimited budget. That is the difference between "we can't promise anonymity" (true but useless) and "here is the anonymity you get per dollar, and here is the part no dollar buys" (true and buildable).

Everything below is written to answer the request: *there is always a way for everything, at a cost in network resources.* The answer is **yes, with a named floor** — and the floor is small.

---

## 0. Executive summary

MiniNet supports file distribution, short-video feeds, profiles, social posts, private messages, and email-style addresses on **one substrate**:

```
encrypted, signed, content-addressed objects
  + capability-based access
  + threat-policy transport selection (direct → relay → mix → burst)
  + store-and-forward mailboxes
  + erasure-coded distributed storage
  + unlinkable prepaid resource credentials
```

The modules do not each build a network. They share an object, transport, privacy, storage, payment, and policy layer, and add application schemas on top.

**The pricing rule that makes the whole thing coherent:** a premium tier must purchase *real scarce resources* — cover bytes, relay bandwidth, mixing delay, redundant storage, independent jurisdictions, repair traffic, anonymous-credential issuance. It must never sell a cosmetic "secure" label. If two users pay the same price, they must be buying the same physical resources and therefore hiding in the same crowd. **A privacy feature that only the buyer uses is not privacy; it is a fingerprint.**

**Four privacy tiers**, each a defined point on a cost curve:

| Tier | Name | What you buy | Cost multiplier* | Residual |
|---|---|---|---|---|
| 0 | Direct / Economy | Content + integrity, ordinary routing | ~1× | Peers see IP, timing, volume |
| 1 | Relayed / Private | No direct P2P; entry+rendezvous separation; padded envelopes; rotating IDs | ~2–4× bandwidth, +100–500 ms | Relay collusion; global timing correlation |
| 2 | Mixed / High-risk | Fixed packets, layered mix hops, batching, delays, cover traffic, anonymous payment, private retrieval | ~5–50× bandwidth, +seconds to minutes | Long-session intersection; endpoint compromise |
| 3 | Burst / Suppression-resistant | Tier 2 transport + multi-region erasure replication, decoys, delayed/threshold reveal, repair | Tier 2 + storage byte-time × redundancy × jurisdictions | Global observer + endpoint seizure combined; coercion of release agents |

\* Order-of-magnitude, relative to sending the same bytes in the clear. Real figures are set by the parameters in §5 and §8 and must be measured, not assumed.

**For humanity**, MiniNet does not claim to prove one-person-one-identity today. It accumulates a **Human Evidence Credential** from optional independent signals over time, and expresses confidence as classes (`Unassessed → ActiveParticipant → HumanEvidenceQualified → StrongHumanEvidence → ExternalUniquenessBacked`). Confidence is *bought with accumulated cost* — continuity time, live-challenge effort, attestation diversity, optional external credentials — and even the top class admits the two residuals below.

**The floor (enumerated in full in §9).** No finite resource budget removes these:

1. A **compromised endpoint** defeats every transport tier. Encryption protects the wire, not a phone with malware or a screen being photographed.
2. A **global passive observer plus enough time** can correlate a sufficiently long, distinctive, high-volume session. Cost pushes the required observation window and volume *up*; it does not make it infinite.
3. **Global uniqueness of persons** cannot be proven from behaviour. It requires an external uniqueness root, and every such root can be duplicated across jurisdictions or coerced.
4. **The user themselves** can leak identity through content, writing style, EXIF, payment reuse, or cross-account correlation regardless of a correct transport.
5. **Coercion and legal compulsion** act on people and keys, below the protocol.

The design's job is to make items 1–5 *as expensive as possible for the adversary* and *as cheap as possible for the user*, and to state honestly where each floor sits.

---

## 1. The cost doctrine, formalized

### 1.1 Every property is a purchase

For any property `P` MiniNet offers (hide source IP, hide who-talks-to-whom, survive takedown, resist correlation, prove liveness…), there is:

- a **mechanism** `M(P)` that provides it;
- a **resource cost** `C(P)` = a vector of (bandwidth, latency, storage byte-time, compute, jurisdictional diversity, money) that `M(P)` consumes;
- a **risk curve** `R(P, spend)` that is monotonically non-increasing in spend;
- a **residual floor** `R_min(P) = lim R(P, spend)` as spend → ∞.

The design contribution is not "make `R = 0`." It is: **publish `M`, `C`, `R(spend)`, and `R_min` for every `P`, and let the owner choose a point on the curve.** The UI shows the chosen point and its residual. This is why MiniNet never displays an absolute "anonymous" badge — it displays a coordinate on a named curve.

### 1.2 Why "just encrypt everything and use P2P" fails the doctrine

Encryption sets `R(content) ≈ R_min(content)` cheaply — good. But direct P2P leaves `R(source-IP)` and `R(counterpart-IP)` at maximum: both endpoints are exposed, and no amount of *encryption* spend moves that curve, because the leak is in the *routing*, not the *payload*. You must spend on a **different resource** — relay/mix bandwidth — to move it. The doctrine forces you to notice that content-cost and metadata-cost are separate budgets. This is the single most common architectural mistake and the earlier draft was right to reject it.

### 1.3 Why one maximum tier for everything fails the doctrine

Fixed-rate cover traffic and mixing buy strong metadata protection at 5–50× bandwidth and seconds-to-minutes of latency. Applying that to a 4K video feed or a public software mirror is paying mix prices for a property (source anonymity) the publisher of public content does not want. Worse, it is *anti-privacy*: if only high-risk users can afford the maximum tier, the maximum tier's traffic pattern identifies them. **Security must be tiered so that each user buys exactly the property they need and hides in the largest possible crowd buying the same thing.**

### 1.4 The anonymity-set corollary

Every privacy purchase has a hidden second cost: **it only works if enough other people make the same purchase.** A mix packet is anonymous because it is indistinguishable from `k−1` others in its batch; the "anonymity set" is `k`. Doubling your own cover traffic while alone doubles your bill and buys nothing. Therefore MiniNet must **pool and subsidize** high tiers (§8.4) so that the crowd exists. Privacy is a *club good*: its price per unit of protection falls as membership rises. This reframes subsidy from charity to engineering necessity.

### 1.5 Decisions carried forward from the earlier draft (kept)

- Reject convergent/deduplicating encryption for private content — equality leakage enables confirmation attacks.
- Reject public-DHT lookups for sensitive content — the query reveals the interest.
- Reject transparent on-chain payment for high-risk publication — payment correlation undoes routing anonymity.
- Reject "prove humanity from browsing history" — invasive, forgeable, discriminatory, and a surveillance asset.
- Reject "one score = one unique human" — activity proves liveness and persistence, never global uniqueness.
- Keep Tor/I2P as *compatibility bearers and bootstrap*, not as the whole architecture.

---

## 2. Threat model — what "nobody can listen" costs to approach

### 2.1 Properties MiniNet prices (the purchase menu)

Contents · sender/recipient network address · who-talks-to-whom · who-follows-whom · who-published-what · what-a-user-reads · when-online · device inventory and linkage · payment-to-action link · cross-module linkability · social graph · geographic movement · moderation preferences · real-vs-cover status of a packet.

Each of these is a separate `P` with its own curve in §3.

### 2.2 Adversary classes and the resource that defeats each

| Class | Adversary | Property purchased to counter it | Primary resource spent |
|---|---|---|---|
| A0 | Curious counterparty | Counterpart-IP hiding | Relay bandwidth (Tier 1) |
| A1 | Malicious relay / storage node | Role separation, opaque shards, no global ID | Multiple independent relays; encryption |
| A2 | Local observer (ISP, Wi-Fi, carrier) | Entry obfuscation, bridges, padding | Obfuscation compute; cover bytes |
| A3 | Partial network observer (some relays/links) | Route diversity across operators/ASNs | Diversity premium (§8) |
| A4 | **Global passive observer** | Mixing, delay, batching, cover, delayed publication | Latency + heavy cover bytes — **floor lives here** |
| A5 | Active adversary (drop/inject/tag/replay) | Authenticated layered packets, replay caches, path redundancy | Redundant sends; per-hop MAC compute |
| A6 | **Endpoint compromise** | *Not a network property* — endpoint hygiene only | Separate devices; hardware — **floor lives here** |
| A7 | Social/economic correlation | Anonymous payment, style tooling, metadata stripping | e-cash issuance; user discipline |

### 2.3 Honest security statement, priced

- E2E encryption drives content risk to its floor **cheaply**.
- Relaying removes counterpart-IP exposure at **~2–4× bandwidth**.
- Onion routing separates source/destination knowledge across hops at **+hundreds of ms and per-hop overhead**.
- Mixing + padding + delay + cover raise traffic-analysis cost **super-linearly for the adversary** while costing the user **latency and multiplied bandwidth**.
- **No network spend protects a compromised endpoint** (A6 floor).
- Low-latency anonymity remains correlatable by a capable A4 given a long enough, distinctive enough session (A4 floor). Cost raises the required window; it does not close it.
- The user can self-deanonymize through content/timing/EXIF/style/payment/reuse (A7 residual) — mitigated by tooling, never removed.

**Therefore the UI shows a threat-model label and a residual-risk line, never an absolute "anonymous."** This is unchanged from the earlier draft and is non-negotiable.

---

## 3. The master cost table — every property, its price, and its floor

This is the centre of the document. For each protected property: the mechanism, the dominant resource you spend, how far spending pushes the risk, and the residual that spending cannot remove.

| Property `P` | Mechanism `M(P)` | Dominant cost `C(P)` | Risk falls with spend? | Residual floor `R_min` |
|---|---|---|---|---|
| **Content secrecy** | AEAD, forward-secret ratchets, per-object keys | Negligible compute | To ~0 | Endpoint compromise (A6); key coercion |
| **Content integrity / authenticity** | Signatures or anonymous authorization, Merkle DAGs | Negligible | To ~0 | Signing-key theft |
| **Counterpart IP hiding** | Relay + rendezvous, no direct P2P (Tier 1) | 2–4× bandwidth, +100–500 ms | To ~0 vs A0 | Global observer correlation (A4) |
| **Source hiding from storage** | Upload via relay/mix, opaque shards, courier ingest | Tier cost + upload spreading | Deep | A4 timing on upload bursts |
| **Who-talks-to-whom** | Sealed sender, per-contact rotating queues, separated send/receive routes | Extra relay round-trips | Deep vs A1–A3 | A4 intersection over time |
| **What-a-user-reads** | Ciphertext CIDs, decoy fetches, PIR, cached bundles | PIR compute / decoy bandwidth | Deep | Perfect PIR is expensive; approximations leak a little |
| **When-online** | Always-on mailbox/replication agents, scheduled cover independent of activity | Continuous cover bytes + a hosted agent | Deep | Cost of 24/7 cover; agent trust |
| **Resistance to traffic correlation** | Fixed packets, batching, delay, cover, route diversity (Tier 2) | 5–50× bandwidth, seconds–minutes latency | Raises adversary cost super-linearly | **A4 + long distinctive session (floor)** |
| **Censorship / takedown resistance** | Erasure shards across operators/jurisdictions, repair, preposition/reveal (Tier 3) | Storage byte-time × redundancy × diversity | To arbitrarily low per added region | Simultaneous global legal action; reveal-agent coercion |
| **Payment unlinkability** | Blinded prepaid tokens, e-cash, pooled redemption | Issuance + denomination overhead | Deep | Issuer compromise; exact-value timing |
| **Cross-module unlinkability** | Per-scope pseudonyms, scope nullifiers, no global DID in transport | Key management overhead | To ~0 by construction | User reuse / style (A7) |
| **Social-graph privacy** | Capability-derived feed/queue addresses; no plaintext "Alice follows Bob" at nodes | Extra indirection | Deep | Endpoint seizure of contact store |
| **Location privacy** | Coarse on-device co-presence; raw GPS/Wi-Fi/BT never leaves device | Local compute only | To ~0 for raw data | Coarse commitments still leak entropy |
| **Real-vs-cover indistinguishability** | Uniform packet format, pooled cover schedules | Cover bytes (pooled) | To ~0 *within the pool* | Pool must be large enough (§1.4) |
| **Liveness of a human** | Unpredictable live challenge + continuity + attestations | User time + issuer cost | High confidence | Human farms, AI assist, coercion |
| **Uniqueness of a person** | External uniqueness credential (biometric/eID) via ZK | Issuer + audit cost | Per-issuer only | **Cross-issuer duplication; coercion (floor)** |

**How to read this table:** every row's risk column says "to ~0" or "deep" — meaning *yes, there is a way, at a resource cost.* Only four rows have a hard floor (bolded), and §9 addresses each. This table *is* the answer to "is there always a way?" — the answer is yes for everything except the four named residuals, and even those are pushed down, not left untouched.

---

## 4. One common substrate (condensed)

### 4.1 Object envelope

Every deliverable artifact is an immutable encrypted `ObjectEnvelope` exposing **only** what routing and storage need:

```
ObjectEnvelope {
  protocol_version, object_class, privacy_tier,
  content_locator,                       // ciphertext-addressed in private modes
  ciphertext_digest,
  encrypted_content_key_or_capability,
  author_proof_policy,                   // signature OR anonymous authorization
  retention_policy, replication_policy,
  reply_or_thread_capability,
  optional_moderation_labels,            // signed opinions, not protocol truth
  optional_payment_policy,
  signature_or_anonymous_authorization
}
```

Application metadata (title, author, tags, thread structure) lives **inside** the ciphertext.

### 4.2 Addressing without interest leakage

Public content: stable public CIDs (cheap, dedup-friendly). Private/high-risk content: **never** a globally predictable plaintext hash — otherwise anyone holding the plaintext can confirm you stored or fetched it. Use CID-over-ciphertext with random per-object keys, capability-derived IDs, per-recipient manifests, and salted/keyed chunk boundaries. **No convergent encryption for sensitive material** (confirmation-attack floor).

### 4.3 Content DAG for arbitrary files

One generic encrypted DAG (`FileManifest`) normalizes documents, images, audio, video, software, archives, databases, and future types. Content-defined chunking (dedup) **only** in economy mode; keyed/random chunk boundaries in private modes to defeat fingerprinting — this is a deliberate spend of dedup savings to buy unlinkability.

### 4.4 Capabilities

Possession of an unguessable, scope-limited, optionally single-use/time-limited capability grants retrieval + decryption **without a global identity**. Rights separate into read / append / reply / moderate / administer. Followers, contacts, senders, and paid subscribers get different capabilities and are never joined through one account ID at the node layer.

---

## 5. Transport — the cost curve made concrete

Application code requests **properties**, not bearers:

```
TransportRequest {
  hide_counterparty_ip, hide_source_from_storage,
  resist_local_observer, resist_broad_correlation,
  maximum_latency, maximum_cost, delivery_deadline,
  packet_size_class, cover_traffic_budget
}
```

The router picks an eligible bearer (direct TCP/QUIC · local Wi-Fi/BT · relay circuit · Tor/I2P compat · MiniNet onion · MiniNet mix · delay-tolerant store-and-forward · offline courier) and records the actual protection class achieved.

### 5.1 Tier 0 — direct (≈1×)
Authenticated encrypted channels with forward secrecy. Local peers, bulk public replication, devices that accept IP exposure. **Not** for hiding an IP from a counterparty.

### 5.2 Tier 1 — relay + rendezvous (≈2–4× bw, +100–500 ms)
Three separable roles: entry relay (knows client IP, not destination) · rendezvous/mailbox relay (knows destination capability, not client IP) · optional delivery relay. **No direct user-to-user connection.** Rules: connection-scoped ephemeral IDs; rotate relays and queues; **never** a global DID in transport headers; separate upload/download routes; independent relays per direction; prevent one provider owning all roles for one delivery; pad to a few size classes; bounded random delay. **This is the recommended default for messaging and MiniMail** — it buys the property most users actually need (counterpart-IP hiding) at the lowest price that provides it.

### 5.3 Tier 2 — layered mix (≈5–50× bw, +seconds–minutes)
Sphinx-style layered packets over independently operated mixes. Required: fixed-size packets; real/cover indistinguishable; one encryption layer per hop; per-hop replay tags + caches; delays sampled from a **public** distribution; batching + reordering; routes across independent operators/jurisdictions; **single-use reply blocks** so recipients don't expose return routes; no 1:1 acknowledgment timing; epoch-based topology; defense against malicious route selection. Interactive voice/video **cannot** claim mix-grade metadata protection — it runs Tier 1 with an explicit lower label; high-risk text and manifests run Tier 2.

### 5.4 Cover traffic — the club good
Schedules: none · opportunistic · low-rate continuous · fixed-rate session · high-risk burst around publication · **community-pooled**. **Pooled is mandatory for the top tiers:** a rare "maximum privacy" pattern is itself a fingerprint (§1.4). You are not just buying your own cover; you are buying membership in a crowd, and the crowd must be subsidized to exist.

### 5.5 Entry privacy / bridges (for A2 censored environments)
Unpublished/invitation bridges · rotating bridge identities · pluggable-transport obfuscation · local Wi-Fi/BT bridge forwarding · multiple bootstrap sources · **no single public relay directory dependency.**

### 5.6 DHT restrictions
No sensitive lookups to a public DHT. Use encrypted rendezvous descriptors · capability-keyed private namespaces · proxied queries · batching + decoys · replicated private indexes · PIR where practical · short-lived provider ads that don't identify the publisher.

---

## 6. Distribution and storage — buying availability with redundancy

### 6.1 Separate publication, storage, retrieval
The publisher never serves readers. Sequence: encrypt+chunk locally → erasure-code → upload shards over independent anonymous routes → nodes issue signed custody receipts (no plaintext) → wait for replication threshold across independent failure domains → publish manifest over separate routes → **go offline.** A storage node knows only: opaque shard ID, shard bytes, expiry/lease class, repair authorization, payment credential. It never learns title, author, follower graph, or full-object mapping.

### 6.2 Erasure coding — the availability price list
Use an externally reviewed MDS code (systematic Reed–Solomon/Cauchy). Availability is bought directly with storage overhead and operator diversity:

| Policy | Data+Parity | Storage overhead | Survives loss of | Buys |
|---|---|---|---|---|
| Ordinary | 8 + 4 | 1.5× | any 4 shards | routine node churn |
| Resilient | 12 + 12 | 2× | any 12 | operator outage |
| High-risk | 16 + 32 | 3× | any 32, many operators | targeted takedown |
| Archival | nested local + geographic | 3–5× | region loss | long-term preservation |

**Placement invariant:** never enough shards to reconstruct under one operator, ASN, cloud, jurisdiction, or payment recipient. *This is where "takedown resistance at a cost" becomes literal:* each added independent jurisdiction is a line item, and each one lowers the probability that any single legal action removes the object.

### 6.3 Swarming without revealing interest
Economy mode: Bitswap-style want-have. Private mode: fetch via relays · request bundles of desired **+ decoy** chunks · opaque capability-derived IDs · fixed-size shard groups · PIR against index servers · cache popular bundles so timing is less unique · prefetch along subscriptions.

### 6.4 Suppression-resistant burst (Tier 3) — preposition then reveal
A `ReplicationBurstPolicy` buys: minimum independent operators, minimum jurisdictions, erasure parameters, initial-replication deadline, repair duration, cover class, delayed-publication option, release-key policy, budget credential. **Two-phase release:**

- **PREPOSITION** — encrypted shards spread widely; nodes cannot decrypt or discover meaning.
- **REVEAL** — decryption capability / public manifest released after the replication threshold is proven.

For extreme cases, split the reveal capability via **threshold secret sharing** among independent release agents, or **time-lock** it — protecting against seizure of the publisher before release. This buys pre-seizure resilience at the cost of **new coercion and governance risk** on the release agents (a floor item, §9).

### 6.5 Repair and persistence — paying to stay alive
Receipts don't prove ongoing retrievability. Buy durability with: random shard audits · proof-of-retrievability (externally reviewed) · periodic reconstruction tests by independent repair agents · automatic repair when diversity drops below policy · retention bonds / delayed payment · penalties **only** where evidence is independently verifiable. No custom PoRep is called secure without review.

### 6.6 Source hiding — beyond encrypting shards
Different routes/times per shard · mix routes for high-risk material · avoid unique burst timing from one link · cover before and after upload · optional offline courier ingest · local EXIF/document stripping · optional transcode to remove device fingerprints · **separate author-signing, transport, and payment identities** · anonymous credentials instead of a persistent key where authorship need not be proven.

---

## 7. Application modules (schemas over the substrate)

All four are schemas + delivery policies over §4–§6. None may open direct sockets that bypass the privacy-policy layer.

- **Tiks (short video).** `TikObject` with encrypted/public manifest, creator capability or anonymous authorization, reply/stitch/duet capabilities, visibility + retention + optional bounty, safety-label commitments. ABR variants are separate encrypted DAGs. Creator IP never exposed to viewers. Views/recommendations computed locally; global counters via privacy-preserving aggregation, not raw viewer logs. **Metadata warning:** video length and segment access patterns are fingerprintable — high-security mode pads segments, uses common sizes, fetches decoys, and avoids a single distinctive sequence (a bandwidth spend to buy unlinkability).
- **Profile / social feed.** Append-only capability namespace (`ProfileRoot`): rotating presentation key, encrypted private sections, feed-head capabilities, contact-invite capabilities, delegated device keys. No globally searchable identifier required; users hold public pseudonyms, per-community pseudonyms, per-contact IDs, and one-time publishing credentials. Followers subscribe via capability-derived feed addresses; nodes never see a plaintext follow relation. Feeds use encrypted append-only logs / Merkle trees / per-audience group keys / **MLS** for dynamic private groups.
- **Private messages.** Async prekeys + forward secrecy + post-compromise security · per-contact/per-device unidirectional queues · sealed/anonymous sender · relay-separated send/receive · rotating queue addresses · optional mix delivery · local encrypted contact store · **no server-visible global account ID.** SimpleX-style delivery; **MLS** (not a custom ratchet) for groups.
- **MiniMail.** Separate presentation address from delivery capability: `public-name@namespace → privacy-preserving resolver → rotating mailbox capability → recipient-controlled queues`. Modes: public (postage/PoW spam control) · contact-only (revocable) · one-time · community · high-risk dropbox (mix-routed, no reply unless sender includes a reply block). Mailbox servers hold **only opaque encrypted envelopes**. Spam control is recipient-chosen (postage token, PoW, invitation, community credential, ZK reputation proof, refundable deposit, local filters). No mandatory global spam/safety authority above protocol integrity.

---

## 8. Economics — pricing the resources, and the adversary's bill

### 8.1 Price is computed from measurable resources
```
Price =  relay_bytes
       + mix_bytes_and_delay
       + cover_bytes
       + storage_byte_time
       + diversity_premium          (independent operators / ASNs / jurisdictions)
       + repair_reserve
       + audit_proof_cost
       + congestion_premium
```
The client shows, before purchase: expected anonymity-set / pool size · expected delay · redundancy target · retention · estimated cost · **residual threat statement.** The user is buying a coordinate on a curve, and the coordinate is disclosed.

### 8.2 The adversary also has a bill — design to invert the ratio
Good privacy engineering makes the **defender's marginal cost low and the attacker's marginal cost high.** Examples:

- **Erasure diversity:** one added jurisdiction costs the publisher ~one shard's storage; it may cost the adversary an entire additional legal action. Favourable ratio → spend here.
- **Cover pooling:** a shared cover schedule costs each member a fraction of a fixed pool; it forces the observer to correlate against the *whole pool*, raising their analysis cost roughly with pool size. Favourable ratio → subsidize the pool.
- **Mixing latency:** delay is cheap for the user (patience) but can force the observer into exponentially larger candidate sets per batch. Favourable ratio for non-interactive traffic → default high-risk text to Tier 2.
- **Live challenges:** cheap for a real human, linearly costly for a human farm at scale. Favourable ratio → use for liveness, not uniqueness.

When the ratio is *unfavourable* (e.g., perfect PIR, fixed-rate cover for a lone user), the doctrine says: **don't buy it retail — pool it, subsidize it, or drop to a tier whose ratio is favourable.**

### 8.3 Anonymous payment (or payment re-links everything)
Transparent on-chain payment from a known wallet to a high-risk publication reveals the source. Use blinded prepaid bandwidth/storage tokens · anonymous credentials with unlinkable spends · offline/externally purchased vouchers · threshold-issued e-cash · one-show credentials with double-spend detection that reveals the *token*, not necessarily identity · separate payment and publishing devices where the threat model requires. **Issuers must not learn the service destination; service nodes must not learn the purchase account.**

### 8.4 Subsidy and solidarity — not charity, but crowd-manufacture
Because privacy is a club good (§1.4), strong tiers are worthless without a crowd. Fund the crowd: community cover traffic · donated storage/relay credits · governance-approved public-interest pools · anonymous sponsorship credentials · equal-size pooled traffic windows · emergency grants without public identity. **This keeps strong privacy from becoming a luxury and, simultaneously, keeps the top tiers effective at all.**

---

## 9. The floor — what no budget removes, and what it costs to approach it

This section is the honest core. For each floor item: why cost cannot reach zero, and what spend pushes it as low as it goes.

**F1 — Endpoint compromise (A6).** Malware, OS telemetry, keylogging, screenshots, or physical seizure reveals plaintext before encryption or after decryption. *No transport spend touches this.* **Push it down with:** separate high-risk profiles/devices, local malware isolation, no cloud keyboard/backup in high-risk mode, secure deletion where hardware permits, hardware-backed keys. Residual: a fully owned endpoint sees everything its user sees. This is a property of the device, not the network.

**F2 — Global observer + long distinctive session (A4).** Low-latency traffic is correlatable given enough observation of a long, high-volume, distinctive flow. **Push it down with:** mixing, delay, batching, pooled cover, delayed/scheduled publication independent of activity, always-on agents so "online = active" stops holding, and by *not* generating long distinctive sessions. Cost raises the required observation window and volume — potentially beyond an adversary's patience or reach — but does not make correlation impossible in the limit. State the residual per session length in the UI.

**F3 — Intersection over time.** A publisher who appears only when content appears is identifiable across enough rounds. **Push it down with:** delegated always-on mailbox/replication agents, third-party prepositioning, offline ingestion, and multiple publishers sharing release windows (a shared crowd again). Residual shrinks with crowd size and decouples with agent delegation, but a unique publication schedule is inherently linkable.

**F4 — Global uniqueness of persons.** Behaviour proves liveness and persistence, never that one human controls only one identity. **Push it down with:** external uniqueness credentials (biometric/eID) consumed as ZK proofs, reconciled under a governance-approved federation. Residual: any single issuer can be duplicated across jurisdictions, and any credential can be transferred or coerced. Hence `ExternalUniquenessBacked` is per-issuer, and `FederatedUniquenessQualified` is explicitly future research (§11), never claimed early.

**F5 — The user and coercion (A7 + legal/physical).** Content, writing style, EXIF, payment reuse, cross-account linkage, and rubber-hose/legal compulsion all act *outside* the transport. **Push it down with:** EXIF/document stripping, transcode normalization, style warnings, separate identities per scope, unlinkable payment, threshold/time-locked reveal for pre-seizure resilience, and duress/decoy provisions where lawful. Residual: a user can always choose to reveal themselves, and a person can be compelled. Protocols bind bytes, not people.

**The doctrine restated against the floor:** for F1–F5 the answer to "is there a way?" is *"there is a way to make it very expensive for the adversary and very cheap for you, and here is exactly how expensive."* That is the maximum any honest network can offer, and it is a great deal more than "we can't promise anonymity."

---

## 10. Human evidence — buying confidence with accumulated cost

### 10.1 Three questions that must never be conflated
- **A. Is a live human doing this now?** — strong evidence achievable.
- **B. Has this pseudonym behaved like a human over time?** — strong evidence achievable.
- **C. Is this the only identity of this physical human?** — F4 floor; needs an external uniqueness root.

### 10.2 Output: credentials, not identity revelation
The device proves statements like "a live-human challenge was completed within 30 days," "this credential kept independent device continuity for 12 months," "≥5 established participants from 3 unrelated communities attested co-presence," "a recognized issuer provided a unique-person credential," "this credential has not been used in this scope this epoch." The relying app learns the **statement**, not the raw evidence.

### 10.3 Signal families (each an optional purchase of confidence)

| # | Signal | Buys | Cost | Limit |
|---|---|---|---|---|
| 1 | Long-term cryptographic continuity | Sybil cost floor-raising | time | bots keep keys too |
| 2 | Unpredictable live interaction | "live human now" | user seconds | AI assist, farms, deepfakes, coercion; must not become discriminatory CAPTCHA |
| 3 | Device/home-hardware continuity | real-world footprint | hardware + uptime | wealthy attacker runs many; never require one vendor |
| 4 | Social attestations (graph *diversity*, not count) | independent corroboration | attester liability/reputation | not uniqueness; can exclude isolated users |
| 5 | Optional institutional credential (eID/employer/bank/carrier/NGO) via ZK | "a qualified issuer attested a natural person" | issuer cost | exclusion, surveillance, cross-country duplicates; never sole path |
| 6 | Optional biometric uniqueness provider (credential only, **no raw biometrics ever enter MiniNet**) | per-domain uniqueness | audit + hardware | biometrics unchangeable if leaked; can centralize; needs competing issuers |
| 7 | Co-presence / location-entropy (coarse, on-device; raw GPS/Wi-Fi/BT never leaves) | anti-remote-Sybil | local compute | spoofing, coercion, unequal mobility; coarse only |
| 8 | Economic / contribution history | reputation, discard cost | real work | **proves reputation, not humanity — and AI can do valuable work; do not exclude AI contributors from rewards** |

### 10.4 Privacy-preserving aggregation and nullifiers
Each issuer provides a credential commitment; the device proves a policy without exposing the signal set. Example:
```
StrongHumanEvidence if:
  live_challenge_recent
  AND continuity_age >= 180 days
  AND at least 2 of {diverse_social_attestations, home_device_continuity,
                     institution_credential, biometric_uniqueness_credential,
                     diverse_copresence_credential}
```
The proof reveals: resulting class, expiry epoch, a scope-specific nullifier if one-per-scope use is required — and **no raw evidence, no globally reusable identifier.** Scope nullifier: `nullifier = PRF(secret, application_scope || epoch)` — one use per scope/epoch without linking across applications. This enforces uniqueness only relative to one issuer's guarantee (F4).

### 10.5 Decay, recovery, taxonomy
Liveness decays fast; continuity ages slowly; attestations are revocable; institutional/biometric credentials expire or need periodic (not repeated-capture) integrity confirmation. Losing a device must not destroy personhood — support pre-rotated recovery keys, threshold guardians, unlinkable-continuity reissuance, cooling periods for high-authority roles, and detection of simultaneous old/new use.

Classes: `Unassessed` · `ActiveParticipant` · `HumanEvidenceQualified` · `StrongHumanEvidence` · `ExternalUniquenessBacked` · (`FederatedUniquenessQualified` — future research, not claimed early).

### 10.6 What MiniNet must never do
No raw browsing-history upload · no central behavior dossier · no required precise location · no required government identity · no stored raw biometrics · never treat one phone/SIM/IP/wallet/device as one human · never treat social attestations alone as global uniqueness · never let a score silently decide constitutional rights without appeal and fork path · **never call evidence "proof" where the assumptions don't justify it.** Keep human-evidence, unique-person, reputation, role-authority, and contribution-value as **five separate concepts.**

---

## 11. Components and phased implementation

### 11.1 Crate/module split
`mini-object` · `mini-chunker` · `mini-erasure` · `mini-storage-contract` · `mini-relay` · `mini-mix` · `mini-private-index` · `mini-credential` · `mini-ecash` · `mini-mailbox` · `mini-message` · `mini-profile` · `mini-tiks` · `mini-minimail` · `mini-privacy-policy` · `mini-human-evidence`. Responsibilities stay narrow; each maps to a section above.

### 11.2 Sequence (build the cheap, high-value properties first)

- **Phase A — IP privacy before any mixnet claim.** Relay circuits + rendezvous; no direct P2P in private mode; remove global DIDs from transport headers; rotating mailbox capabilities; padded envelope classes; Tor as optional bearer; network-observer integration tests. *Exit:* counterparties, mailbox servers, and ordinary storage nodes cannot directly learn both endpoints.
- **Phase B — common encrypted object + arbitrary files.** Ciphertext-addressed manifests; private chunking; externally defensible erasure coding; parallel shard retrieval; diversity-aware placement; receipts + repair; arbitrary MIME above manifests. *Exit:* a publisher can upload, disappear, and later reconstruct from independent nodes without exposing plaintext or source IP.
- **Phase C — messaging + MiniMail.** Async key establishment; per-contact unidirectional queues; rotation; separate send/retrieve relays; reply capabilities; optional anonymous postage; MLS adapter. *Exit:* relays need no global account ID and cannot read content or trivially derive a full social graph.
- **Phase D — mixnet (research implementation).** Formal packet spec; external crypto review; simulator for latency/anonymity-sets/attacks; replay-safe layered packets; batching + delays; cover schedules; topology epochs + directory consistency; adversarial lab. *Exit:* measurable timing-correlation resistance in a published threat model. **Do not market as globally anonymous before this evidence exists.**
- **Phase E — suppression-resistant replication.** Preposition/reveal lifecycle; burst contracts; diversity constraints; anonymous payment; auto-repair; threshold/delayed reveal; source-hygiene tooling. *Exit:* a controlled exercise can remove the publisher and several operators after prepositioning while the object stays reconstructable and the source stays hidden from recipients and nodes.
- **Phase F — application modules.** Tiks, Profile, Messages, MiniMail over one substrate; no direct-socket bypass of the privacy-policy layer.

---

## 12. Attacks → required defenses (reference)

Traffic correlation → mixing/delays/padding/cover/route-diversity/guard-balancing; residual: broad observer on long/unique flows (F2). Intersection → always-on agents, scheduled cover, delayed publication, prepositioning, shared windows (F3). Sybil relays/storage → operator-diversity constraints, resource proofs as *one* signal, pseudonymous reputation, independent topology sampling, audits; "many nodes ≠ many humans." Malicious directory → threshold-signed epochs, multiple observations, gossiped consistency proofs, transparency logs, no single mandatory directory, fork/equivocation detection. Tagging/replay → authenticated layered packets, per-hop replay tags + caches, malleability-resistant format, fail-on-integrity-error. Content fingerprinting → ciphertext CIDs, random keys, padded segments, transcode normalization, decoy retrieval, capability IDs. Endpoint metadata (F1) → EXIF/document stripping, malware isolation, separate high-risk profile, no cloud keyboard/backup by default, secure deletion, style warnings. Payment correlation → blind issuance, unlinkable spends, denomination standardization, delayed pooled redemption, no unique exact-value payment before publication. DoS → anonymous rate credentials, service-chosen PoW/postage, bounded queues, per-capability quotas, relay admission control; no global-legal-identity requirement.

---

## 13. External review program (before any production claim)

Commission separate reviews of: mix packet cryptography + replay resistance · traffic-analysis simulation methodology · anonymous credential / e-cash construction · private-mailbox metadata analysis · encrypted storage + erasure correctness · payment/publication unlinkability · human-evidence privacy + bias · biometric/external federation · mobile endpoint + metadata hygiene · abuse/legal/human-rights impact across jurisdictions. **Publish:** threat models, simulation code, synthetic packet traces, formal specs, test vectors, audit reports, unresolved findings, and exact maturity labels.

---

## 14. Conclusion

MiniNet should not ship five independent systems (a torrent module, a TikTok, a Facebook, a messenger, an email). It should ship **one privacy-aware sovereign data substrate** across four planes — **Data · Privacy Transport · Service · Legitimacy & Economics** — and let those experiences become modules over it.

The immediate priority is **Tier 1**: relay/rendezvous transport, rotating mailbox capabilities, no direct counterparty connections, no global transport identifiers, ciphertext-addressed arbitrary-file storage. This delivers real privacy value at the lowest price that provides it, without prematurely claiming global-observer resistance. The **mixnet** follows as an explicitly researched, measured, externally reviewed high-cost tier. **Suppression-resistant publishing** uses prepositioned erasure shards, operator diversity, anonymous replication payment, auto-repair, and a separate reveal phase — with the premium justified by *actual redundancy and anonymity resources*, and community subsidy keeping strong privacy from becoming a luxury (and, per §1.4, keeping it effective at all).

**Humanity** is approached with the same honesty: high confidence that a persistent credential is controlled by a live human over time, via optional independent signals and zero-knowledge aggregation — but no honest guarantee, yet, that each physical human controls only one identity. Human-evidence, unique-person, reputation, role-authority, and contribution-value stay separate.

The constitutional principle:

> Privacy and safety features should be available without becoming compulsory surveillance.
> Protocol integrity must be enforced.
> Identity disclosure, moderation subscriptions, trust signals, and adoption remain owner choices wherever integrity permits.

And the thesis this document was built to answer:

> **There is always a way — at a resource cost.** For nearly every property a user could want, MiniNet can push the risk arbitrarily low by spending bandwidth, latency, storage, redundancy, diversity, compute, and unlinkable money — and it discloses the price and the coordinate. Only five residuals (F1–F5) have a floor above zero, and even those are pushed down, priced, and named rather than hidden. The strongest MiniNet is not one that promises identification is impossible. It is one that makes surveillance, correlation, suppression, and compulsory identity **progressively and measurably more expensive**, tells the owner exactly what each mode costs and protects, and never claims more anonymity or humanity certainty than the evidence and the budget support.

---

## 15. Primary sources and reference projects

**MiniNet repository and recent work** *(not fetchable in this environment — cite as reported in the source document; verify against the live repo before relying on PR specifics)*
`github.com/mininet-labs/mininet` and PRs #120 (encrypted tamper-evident consensus links), #123 (`FullHuman` → `EvidenceQualifiedHuman`), #125 (KEL freshness pinning), #127 (Founder Directives canonicalization), #128 (TCP resume + Wi-Fi multicast discovery), #129 (peer exchange over TCP).

**Distribution/storage** — IPFS Bitswap & specs; Tahoe-LAFS; libp2p.
**Anonymous routing / metadata resistance** — Tor spec & onion services; I2P; Nym; Danezis & Goldberg, *Sphinx: A Compact and Provably Secure Mix Format*; Piotrowska et al., *The Loopix Anonymity System*; Katzenpost.
**Messaging** — Briar; SimpleX; Signal protocol; MLS (RFC 9420).
**Humanity / personhood** — Adler et al., *Personhood credentials* (arXiv:2408.07892); Ford, *Identity and Personhood in Digital Democracy* (arXiv:2011.02412); Hajialikhani & Jahanara, *UniqueID: Decentralized Proof-of-Unique-Human* (arXiv:1806.07583); W3C Verifiable Credentials 2.0; Semaphore; Idena; BrightID; World ID.
**Privacy-preserving authorization / retrieval** — Privacy Pass (RFC 9576); Oblivious HTTP (RFC 9458); PIR literature (evaluate per deployment scale; no single system selected).
