MN-208 Research Report: Private Lookup, DHT Restrictions, and Interest-Hiding Retrieval

Research target

Repository: mininet-labs/mininet
Work item: MN-208 — no sensitive lookups to a public DHT / private lookup
Research date: 14 July 2026

⸻

Executive conclusion

MN-208 should not begin by building a general-purpose Kademlia-style value DHT and then attempting to add privacy around it.

The repository currently has no provider-record or value-storage DHT to restrict. Its networking code has peer routing, gossip deduplication, and peer exchange, but no public PUT, GET, provider-advertisement, or value-resolution layer. Issue #144 therefore correctly deferred MN-208 rather than creating a privacy rule for functionality that does not yet exist.

The correct MN-208 outcome is a design doctrine and narrowly scoped private-index protocol:

Public network routing may use public peer-discovery data, but private object discovery must never require broadcasting a sensitive object identifier, capability, mailbox address, subscription, search term, or content interest to a public DHT.

The recommended architecture separates four questions:

1. Peer routing:
   Which nodes can carry traffic?
2. Public content discovery:
   Which nodes provide deliberately public content?
3. Private capability resolution:
   Which authorised service currently stores an opaque private object?
4. Private retrieval:
   How can a user fetch the object without revealing the exact requested item?

Only the first two belong in an ordinary public DHT.

For private objects, Mininet should use a layered design:

Capability
   │
   ├─ derives a rotating, unlinkable lookup label
   │
   ▼
Private index replicas
   │  queried through relay / OHTTP-like role separation
   │
   ├─ return encrypted provider or mailbox descriptor bundles
   │
   ▼
Retrieval path
   │  relay/mix + fixed-size bundles + cache/decoys
   ▼
Opaque object or shard

The first production design should combine:

* capability-derived rotating lookup labels;
* encrypted signed index records;
* short epochs;
* several independently operated index replicas;
* proxied or oblivious requests so one service does not learn both client IP and query;
* fixed-size query and response classes;
* bundled responses containing multiple candidates;
* local caching and subscription prefetch;
* optional decoy queries;
* exact capability checks at retrieval;
* no plaintext object ID or user identity;
* no globally stable private provider advertisement;
* no claim that lookup privacy is equivalent to full PIR.

True Private Information Retrieval should remain an optional later tier.

PIR is valuable because it can hide which database row a client requests. It is also costly, operationally complex, and dependent on either computational assumptions or non-colluding replicated servers. It should be deployed only after benchmarking and external cryptographic review.

The strongest initial doctrine is therefore:

1. Do not put sensitive names in a public DHT.
2. Hide client identity from the private index through role separation.
3. Hide exact interest partially through rotating labels, batches, bundles, caching, and decoys.
4. Offer genuine PIR later where its cost is justified.
5. State the remaining leakage honestly.

⸻

1. Current repository state

1.1 There is no value DHT to restrict yet

The repository review recorded in issue #144 found that mini-net currently provides:

* peer-bucket routing;
* gossip deduplication;
* peer exchange.

It does not provide:

* arbitrary key-value publication;
* object provider records;
* value retrieval;
* public content lookup;
* sensitive namespace resolution.

This matters because MN-208 should not accidentally expand into:

“Build Mininet’s complete DHT and private search system in one PR.”

The lane should first freeze the privacy boundary that any future discovery system must obey.

1.2 Founder research direction

The research document states:

No sensitive lookups to a public DHT.

Its proposed alternatives are:

* encrypted rendezvous descriptors;
* capability-keyed private namespaces;
* proxied queries;
* batching and decoys;
* replicated private indexes;
* PIR where practical;
* short-lived provider advertisements that do not identify the publisher.

For private content retrieval, it also recommends:

* relay-based fetches;
* desired-plus-decoy bundles;
* opaque capability-derived IDs;
* fixed-size shard groups;
* PIR against index servers;
* popular-bundle caching;
* subscription prefetch.

MN-208 should convert those principles into one coherent, implementable sequence.

⸻

2. Why public DHT lookup leaks private interests

A traditional content-routing flow might look like:

client asks DHT:
    “Who provides object X?”

Even when the object itself is encrypted, the query may reveal:

* the exact object or ciphertext identifier;
* that the client possesses or seeks a capability;
* mailbox membership;
* subscription relationships;
* which private feed the user follows;
* which shard belongs to the same manifest;
* timing of new-content interest;
* repeated access to one topic;
* correlation across devices;
* approximate social graph;
* publisher popularity;
* geographical clusters of readers.

2.1 Ciphertext IDs are not automatically private

A ciphertext content ID is safer than a plaintext hash because an outsider cannot necessarily predict it.

However, it can still become sensitive when:

* the ID is shared with several people;
* a storage node sees the upload;
* a capability leaks;
* a recipient publishes it accidentally;
* the same ID is queried repeatedly;
* a censor learns the ID from one endpoint;
* the ID remains stable for a long period.

Once known, it becomes a global tracking handle.

2.2 Provider records reveal publishers and storage placement

A public provider record can reveal:

* which node first announced content;
* which nodes replicate it;
* when replication began;
* how quickly interest grew;
* which operator stores related shards;
* whether a private mailbox is active;
* where a high-risk publication is prepositioned.

2.3 DHT paths expand observation

In many DHTs, a lookup is sent to several progressively closer peers.

This means a sensitive key may be revealed not only to one resolver but to multiple unrelated routing participants.

2.4 Encryption does not hide the lookup key

Encrypting the response or the transport does not prevent the DHT node receiving the lookup from seeing the queried key unless the protocol is specifically designed to obscure it.

⸻

3. Taxonomy of Mininet lookup classes

MN-208 should freeze a typed classification rather than rely on caller judgment.

Conceptually:

pub enum LookupPrivacyClass {
    Public,
    CapabilityScoped,
    PrivateProxied,
    PrivateBundled,
    PrivatePIR,
}

The exact enum can differ, but the policy distinction should exist.

3.1 Public

Suitable for:

* public releases;
* public software mirrors;
* public profile objects deliberately advertised;
* public media;
* public bootstrap data;
* public network node records.

May use:

* public DHT;
* stable CID;
* provider advertisement;
* deduplication;
* ordinary caching.

Residual:

* interest in public content may still be sensitive.

Public content does not mean every user’s reading behaviour should be public.

3.2 Capability-scoped

Suitable for:

* private object lookup where possession of an unguessable capability is the main protection;
* small communities;
* early implementation;
* low-cost private retrieval.

Uses:

* secret-derived lookup label;
* encrypted record;
* direct or relayed private index.

Residual:

* index sees the label;
* repeated labels are linkable;
* compromise of the capability permits lookup.

3.3 Private proxied

Adds role separation:

* relay sees client IP, not plaintext query;
* gateway/index sees query, not client IP.

This follows the privacy-partitioning pattern standardised in Oblivious HTTP and Oblivious DoH. OHTTP separates a relay from a gateway so the gateway does not learn the client IP while the relay cannot read the encrypted request. (⁠rfc-editor.org)

Residual:

* relay and gateway collusion;
* query itself visible to gateway after decryption;
* timing and size correlation.

3.4 Private bundled

Adds:

* fixed-size query classes;
* several lookup labels per request;
* desired plus decoy items;
* response bundles;
* cacheable epochs;
* subscription prefetch.

Residual:

* statistical inference;
* decoy-quality failures;
* larger bandwidth;
* index still sees a set containing the target.

3.5 Private PIR

Uses cryptographic PIR so the database operator cannot learn which row was selected.

Residual:

* access timing;
* database version;
* client network path unless separately hidden;
* multi-server collusion assumptions;
* computational cost;
* response-size patterns;
* correctness or malicious-server attacks;
* sparse anonymity sets.

⸻

4. Recommended architecture

4.1 Public routing plane

A public routing plane may contain:

* peer contact data;
* transport endpoints;
* network capabilities;
* topology epochs;
* public object providers;
* explicitly public namespaces.

It must not contain:

* private mailbox names;
* private feed IDs;
* capability secrets;
* private object CIDs;
* social subscriptions;
* private provider records;
* recipient DID mappings.

4.2 Private index plane

A separate mini-private-index service should store encrypted opaque records indexed by capability-derived labels.

Conceptually:

pub struct PrivateIndexRecord {
    version: PrivateIndexVersion,
    epoch: IndexEpoch,
    lookup_label: LookupLabel,
    encrypted_descriptor: Vec<u8>,
    expires_at: u64,
    publisher_authorization: IndexAuthorization,
    signature_or_mac: RecordAuthenticator,
}

The index service need not know:

* human-readable object name;
* object type;
* publisher DID;
* recipient DID;
* plaintext provider list;
* application semantics.

4.3 Retrieval plane

The returned descriptor may identify:

* one or more mailbox relays;
* shard providers;
* a rendezvous capability;
* an opaque retrieval route;
* a signed storage receipt set;
* a next rotating label.

It should be encrypted to capability holders.

4.4 Object plane

Possession of the index result must not by itself grant content access.

Retrieval still requires:

* object capability;
* mailbox capability;
* shard capability;
* decryption key;
* token or holder proof.

This preserves defence in depth.

⸻

5. Capability-derived lookup labels

5.1 Goal

The lookup label should be:

* unguessable without the capability;
* unlinkable across scopes;
* rotated over time;
* different for distinct index replicas where appropriate;
* not equal to the object ID;
* not reusable as a decryption key;
* safe to reveal to the intended index replica under the selected privacy class.

5.2 Conceptual derivation

Using existing HKDF-SHA256 tooling:

lookup_label =
    HKDF-Expand(
        capability_secret,
        info =
            "mininet/private-index/lookup-label/v1" ||
            index_scope ||
            replica_id ||
            epoch ||
            record_purpose,
        L = 32
    )

Do not derive it from:

* public DID;
* plaintext object ID;
* stable profile name;
* raw mailbox name;
* transport pseudonym.

5.3 Separate derivation domains

Use distinct labels for:

provider lookup
mailbox lookup
feed-head lookup
subscription lookup
shard lookup
bridge lookup
next-label rotation
record encryption
record authentication

A lookup label must never also serve as:

* content key;
* capability token;
* signature key;
* route tag;
* payment identifier.

5.4 Epoch rotation

Labels should rotate by epoch:

label_e = PRF(capability_secret, scope || epoch)

Benefits:

* limits long-term correlation;
* bounds stale records;
* supports revocation;
* prevents one leaked label from tracking indefinitely.

Risks:

* offline users may miss rotations;
* clock disagreement;
* record duplication during grace periods;
* rollback attacks.

Recommended:

* current epoch;
* previous epoch grace;
* encrypted next-label pointer;
* monotonic record sequence;
* expiry enforcement.

⸻

6. Index record design

6.1 Encrypted descriptor

The index stores:

lookup label → encrypted fixed-size descriptor bundle

The ciphertext contains:

* descriptor version;
* provider/mailbox candidates;
* object or shard routing data;
* validity;
* sequence number;
* next rotation information;
* integrity data;
* optional replication threshold information.

6.2 Fixed-size record classes

Use a small number of record sizes, for example:

Small
Medium
Large

A unique record size can reveal:

* object popularity;
* provider count;
* content type;
* high-risk replication policy.

Private records should pad to class boundaries.

6.3 Signed or capability-authenticated writes

An attacker must not overwrite another capability namespace.

Possible write authorization:

* scoped pseudonym signature;
* capability-token MAC;
* holder-bound capability signature;
* one-time update token;
* monotonic sequence proof.

The primitive must be typed.

Avoid a generic:

put(key, bytes, signature)

without domain-specific validation.

6.4 Conflict rules

Private index records should not silently merge arbitrary writer updates.

Recommended:

* one active writer authority per record generation;
* monotonic sequence;
* exact signed replacement;
* explicit multi-writer log if required;
* reject same-sequence conflicts;
* preserve conflicting signed records for equivocation detection where safe.

⸻

7. Role-separated query path

7.1 OHTTP-style pattern

A client forms an encrypted query for a private-index gateway and sends it through a relay:

Client
  → relay
  → private-index gateway
  → relay
  → client

The relay sees:

* client IP;
* gateway identity;
* request size and timing.

It does not see:

* lookup labels;
* response contents.

The gateway sees:

* lookup labels;
* query class;
* record epoch.

It does not directly see:

* client IP;
* transport identity.

OHTTP standardises the same fundamental partition: encrypted application requests pass through a relay, preventing one party from seeing both client identity and request contents. (⁠rfc-editor.org)

ODoH applies the model to DNS queries, separating the proxy that sees the client from the target resolver that sees the query. (⁠rfc-editor.org)

7.2 Why not copy OHTTP blindly

OHTTP uses HPKE and HTTP-specific formats.

MN-208 should not introduce a new cryptographic dependency solely to claim OHTTP compliance if mini-crypto does not already expose the required construction.

The design should reuse the role separation and either:

* adopt OHTTP through a reviewed compatibility adapter; or
* use the existing Mininet encrypted relay channel and private-index gateway key.

7.3 Stateful versus stateless lookup

OHTTP is most appropriate for unlinkable discrete transactions and warns that application state can defeat unlinkability. (⁠rfc-editor.org)

Mininet private queries should avoid:

* cookies;
* persistent gateway sessions;
* global authentication tokens;
* stable request IDs;
* client-specific response formatting;
* personalised errors.

⸻

8. Query batching and decoys

8.1 Batched labels

Instead of requesting one label, request a fixed number:

N labels per query

One may be real; others may be:

* locally selected decoys;
* subscription prefetch;
* previously cached popular labels;
* cover labels published for this purpose.

8.2 Problems with naive decoys

Random labels are poor decoys because the gateway can immediately see they do not exist.

Decoys should be drawn from a public or privately distributed cover catalogue of live labels or cover records.

8.3 Fixed query cardinality

All requests in one class should contain the same number of labels.

Otherwise:

* one-label requests identify real-time reads;
* large requests identify high-risk mode;
* count correlates with subscriptions.

8.4 Fixed response shape

The server should return one padded slot per requested label, including indistinguishable negative responses.

Do not return:

* variable response count;
* plaintext “not found” flags;
* distinct timing for missing records;
* record-specific HTTP status.

8.5 Decoy cost

Decoys spend:

* bandwidth;
* index compute;
* cache space;
* relay capacity.

They provide only partial privacy.

They should be reported as a measured privacy class, not called PIR.

⸻

9. Caching and prefetch

9.1 Local cache

A client should retain encrypted descriptors and objects to avoid repeated lookups.

Benefits:

* less interest leakage;
* lower latency;
* lower index load;
* better offline behaviour.

Risks:

* seized device reveals interests;
* stale descriptors;
* rollback;
* storage pressure.

9.2 Subscription prefetch

For private feeds or mailboxes, clients can fetch a fixed bundle periodically regardless of whether new content exists.

This hides:

* exact read time;
* whether a notification caused the request;
* individual object selection.

9.3 Community cache

A trusted local or relay cache may fetch bundles for many users.

The cache should store opaque encrypted bundles and not receive individual capability secrets.

9.4 Popular bundle caching

Popular public or semi-private bundles can be cached broadly so many clients request the same ciphertext set.

This creates a crowd.

However, a private group with only one member online has a very small anonymity set regardless of caching architecture.

⸻

10. Private Information Retrieval

10.1 What PIR provides

PIR lets a client retrieve a database item without revealing the selected item index to the server.

Two broad families matter:

Information-theoretic PIR

Requires multiple replicated servers that do not collude.

Computational PIR

Can operate with one server but relies on computational assumptions and usually significant server-side work.

The fundamental multi-server PIR model assumes non-communicating replicated databases; the privacy guarantee fails if enough replicas collude. (⁠arXiv)

10.2 Why PIR is attractive for Mininet

It can protect:

* mailbox index selection;
* provider-record lookup;
* private feed-head lookup;
* capability namespace resolution;
* shard-map lookup;
* bridge-descriptor retrieval.

The index server learns less than with batching and decoys.

10.3 Why PIR should not be the first implementation

Costs include:

* database replication;
* server compute;
* synchronisation;
* malicious response handling;
* key and parameter management;
* query-size and response-size overhead;
* difficult mobile benchmarking;
* non-collusion governance;
* cryptographic audit requirements.

10.4 Recommended PIR posture

Phase it:

1. capability-derived labels;
2. role-separated proxied queries;
3. batches, fixed response classes, decoys;
4. replicated indexes;
5. benchmark established PIR libraries;
6. external review;
7. pilot for specific fixed-size datasets.

10.5 Suitable first PIR use case

A good first candidate is a bounded, fixed-record database such as:

current mailbox descriptors for one epoch

It is easier than:

* arbitrary full-text search;
* unbounded object provider discovery;
* variable-size shard manifests.

10.6 PIR must be combined with transport privacy

PIR hides the selected row from the database.

It does not automatically hide:

* client IP;
* query timing;
* session continuity;
* database choice;
* epoch;
* response size;
* whether the user queried at all.

Use relay or mix transport around PIR.

⸻

11. Search is not lookup

MN-208 should distinguish exact private lookup from private search.

11.1 Exact lookup

The client already has an unguessable capability or label.

Goal:

resolve this known opaque name

This is in scope.

11.2 Search

The client asks:

find private objects matching terms or attributes

This is much harder because the index may learn:

* search terms;
* result patterns;
* repeated topic interests;
* vocabulary;
* social context.

Searchable encryption, oblivious RAM, and private set intersection introduce larger cryptographic and leakage surfaces.

11.3 Recommendation

MN-208 version 1 should support exact capability-based resolution only.

Defer:

* free-text private search;
* fuzzy search;
* private recommendation;
* encrypted range queries;
* global private discovery.

⸻

12. Provider advertisements

12.1 Public provider ads

Allowed only for deliberately public content.

May include:

* public CID;
* provider peer ID;
* expiry;
* capacity;
* signature.

12.2 Private provider descriptors

Must be encrypted under the capability namespace.

A storage node should not publicly announce:

I provide private object X

Instead, an authorised publisher or repair process writes an encrypted descriptor containing opaque retrieval candidates.

12.3 Short lifetimes

Private provider descriptors should expire quickly enough to limit tracking but not so quickly that offline clients cannot retrieve.

12.4 Provider privacy

Even encrypted index records can expose a provider if the ciphertext is written immediately after upload and queried immediately after publication.

Mitigations:

* delayed advertisement;
* multiple routes;
* batching;
* independent writer relay;
* descriptor prepositioning;
* common publication windows;
* several providers per bundle.

⸻

13. Negative lookups

“Not found” is itself sensitive.

It can reveal:

* label validity;
* capability guessing success;
* record expiry;
* mailbox activity;
* revocation;
* whether a user has transitioned to a new epoch.

Recommended behaviour

* constant response class;
* encrypted negative records;
* indistinguishable processing time within policy bounds;
* no detailed unauthenticated error;
* rate limits on arbitrary labels;
* optional cover records.

⸻

14. Replication and non-collusion

14.1 Replica roles

Private indexes should be operated by independent parties.

Diversity dimensions:

* operator;
* cloud;
* ASN;
* jurisdiction;
* funding source;
* software build;
* governance relationship.

14.2 Replication modes

Same encrypted records at every replica

Simple and compatible with multi-server PIR.

Sharded records

Reduces one-server knowledge but complicates availability and retrieval.

Threshold or secret-shared index

Stronger privacy but much more cryptographic complexity.

Recommendation

Begin with identical encrypted records replicated across independent services.

This supports:

* availability;
* equivocation comparison;
* future multi-server PIR;
* simple consistency.

14.3 Collusion statement

If the relay and gateway collude, proxied query privacy collapses.

If the required PIR replicas collude, information-theoretic query privacy collapses.

Operator diversity is therefore a security assumption, not a cosmetic deployment preference.

RFC 9614 formalises privacy partitioning as an architectural approach: divide information across parties so no single participant has the complete view. (⁠rfc-editor.org)

⸻

15. Consistency and equivocation

A malicious index may return different records to different clients.

Threats

* isolate a target;
* return attacker-controlled providers;
* suppress updates;
* selectively claim “not found”;
* track users with unique descriptor variants;
* force rollback.

Defences

* signed records;
* monotonic sequence;
* record commitments;
* epoch roots;
* cross-replica comparison;
* gossiped consistency proofs;
* transparency log;
* client caching of highest sequence;
* reject unsigned negative claims where applicable.

Privacy trade-off

Public transparency of every private lookup label would recreate the leakage MN-208 is trying to avoid.

Therefore transparency should cover:

* index software releases;
* epoch configuration;
* replica keys;
* aggregate signed roots;
* consistency proofs.

It should not publish raw private labels.

⸻

16. Revocation

Revocation must avoid turning one stable public list into a catalogue of private relationships.

16.1 Capability revocation

Possible approaches:

* rotate capability secret;
* advance record epoch;
* stop publishing old label;
* publish encrypted revocation state under old capability;
* use short record expiry;
* maintain holder-specific rekey.

16.2 Reader removal

For groups:

* issue new group capability;
* update record under new lookup label;
* distribute the new capability to remaining members;
* expire old record;
* re-encrypt future content.

Revocation cannot make a removed reader forget content or descriptors already obtained.

16.3 Provider removal

Publish a higher-sequence descriptor excluding the provider.

Clients reject rollback.

⸻

17. Abuse resistance

Private indexes can be attacked by:

* random-label flooding;
* write spam;
* giant records;
* expensive PIR queries;
* replay;
* hot-key attacks;
* storage exhaustion;
* malicious decoy amplification;
* enumeration.

Controls

* fixed record sizes;
* bounded labels;
* short expiry;
* authenticated writes;
* anonymous rate credentials;
* capability-bound quotas;
* service-chosen proof-of-work;
* prepaid resource tokens;
* per-epoch limits;
* bounded PIR parameters;
* no unbounded search;
* admission control.

Do not require global legal identity.

⸻

18. Technologies considered

Solution A — ordinary public Kademlia DHT for all content

Advantages

* simple;
* decentralised;
* familiar;
* scalable;
* easy provider discovery.

Problems

* exposes sensitive keys to routing peers;
* creates global tracking handles;
* leaks publisher and reader interests;
* difficult to revoke;
* private provider records become observable;
* query path multiplies exposure.

Decision

Reject for private content.

⸻

Solution B — hash private object IDs and use the public DHT

Advantages

* easy;
* no new service.

Problems

* the ID is already a hash or opaque name;
* once known, it remains trackable;
* repeated queries link users;
* no client-IP separation;
* provider ads leak storage placement.

Decision

Reject.

⸻

Solution C — encrypt DHT values but leave keys public

Advantages

* hides descriptor contents.

Problems

* query interest and stable key remain exposed;
* provider and access timing remain linkable;
* values may be copied and replayed.

Decision

Insufficient.

⸻

Solution D — capability-derived labels on the public DHT

Advantages

* unguessable to outsiders;
* low implementation cost;
* no plaintext object ID.

Problems

* DHT nodes receiving the label can track repeated use;
* capability compromise reveals historical interest;
* global provider records remain;
* no client identity separation.

Decision

Accept only as a temporary low-privacy mode, not the recommended private default.

⸻

Solution E — central private index over TLS

Advantages

* simple;
* fast;
* easy to operate;
* exact access control.

Problems

* central service sees client IP and every interest;
* central censorship and compromise target;
* complete social/read graph;
* single point of failure.

Decision

Reject as the sole architecture.

⸻

Solution F — private index through relay separation

Advantages

* practical;
* standards-supported architectural precedent;
* gateway does not see client IP;
* relay cannot read query;
* works with fixed-size requests;
* lower cost than PIR.

Problems

* gateway sees query labels;
* relay/gateway collusion;
* timing correlation;
* operator diversity requirement.

Decision

Recommend as the initial production baseline.

⸻

Solution G — batches and decoys

Advantages

* no new advanced cryptography;
* partial interest hiding;
* configurable cost;
* easy to combine with caches and subscriptions.

Problems

* weak decoys are detectable;
* bandwidth cost;
* statistical leakage;
* not cryptographic PIR.

Decision

Recommend as an optional stronger class, with honest naming.

⸻

Solution H — single-server computational PIR immediately

Advantages

* strong query-index privacy from one database;
* no non-collusion requirement.

Problems

* heavy computation;
* advanced cryptography;
* difficult implementation and audit;
* database-update challenges;
* possible mobile cost;
* malicious-server considerations.

Decision

Defer pending benchmarking and external review.

⸻

Solution I — multi-server information-theoretic PIR

Advantages

* strong privacy under non-collusion;
* attractive for independent Mininet operators;
* replicated index already useful for availability.

Problems

* every server needs consistent database state;
* collusion breaks assumptions;
* more bandwidth;
* operational coordination;
* malicious response handling.

Decision

Best future high-privacy option for bounded fixed-record indexes.

⸻

Solution J — download the entire index

Advantages

* strongest simple interest privacy;
* no per-item query leakage;
* offline search.

Problems

* only practical for small databases;
* large bandwidth;
* update leakage;
* device storage;
* index membership itself may be sensitive.

Decision

Recommend for small epoch catalogues and community indexes.

This is an underrated option.

For a small private group, downloading the complete encrypted epoch index may be cheaper and safer than deploying PIR.

⸻

19. Recommended implementation sequence

Phase 0 — doctrine only

Before building a value DHT, add a design document:

docs/design/private-lookup-and-dht-boundary.md

Freeze:

* public versus private lookup classification;
* forbidden public-DHT fields;
* capability-derived labels;
* exact lookup only;
* residual-risk language.

Phase 1 — private index primitive

Implement:

* typed lookup labels;
* index epochs;
* encrypted fixed-size records;
* authenticated writes;
* expiry;
* monotonic sequence;
* local in-memory index;
* exhaustive malformed-input tests.

No network yet.

Phase 2 — replicated index service

Implement:

* several replicas;
* signed epoch configuration;
* record replication;
* consistency checks;
* negative-response padding;
* bounded quotas.

Phase 3 — relay-separated query

Use Tier 1 relay or OHTTP-style gateway separation:

* relay sees client connection;
* gateway sees encrypted or decrypted query as defined;
* no stable client state;
* fixed request/response classes.

Phase 4 — rotating labels

Add:

* per-epoch derivation;
* previous-epoch grace;
* next-label pointer;
* rollback prevention;
* revocation.

Phase 5 — batching and caching

Add:

* fixed query cardinality;
* cover records;
* desired-plus-decoy queries;
* local cache;
* subscription prefetch;
* whole-index download for small namespaces.

Phase 6 — retrieval integration

Resolve:

* mailbox descriptors;
* feed heads;
* shard providers;
* rendezvous routes.

Do not add free-text search.

Phase 7 — PIR research prototype

Select one bounded use case and benchmark:

* 2-server or 3-server PIR;
* fixed record count;
* fixed record size;
* mobile latency;
* server CPU;
* network overhead;
* collusion model;
* malicious response detection.

Phase 8 — external cryptographic review

Review:

* PIR choice;
* label derivation;
* record encryption;
* query unlinkability;
* malicious replicas;
* replay;
* consistency;
* traffic analysis.

Phase 9 — high-privacy pilot

Pilot only after:

* independent operators;
* measured anonymity set;
* published performance;
* failure handling;
* honest residual statements.

⸻

20. Required tests

Label tests

1. Same capability, scope, replica, and epoch derive the same label.
2. Different capabilities derive different labels.
3. Different epochs derive different labels.
4. Different replicas derive different labels if replica separation is enabled.
5. Different purposes derive different labels.
6. Public object ID cannot be recovered from the label.
7. Lookup labels cannot be used as content keys.
8. Old labels expire.
9. Previous-epoch grace is bounded.
10. Unknown derivation versions fail closed.

Record tests

1. Record ciphertext mutation fails.
2. Record label mutation fails authentication.
3. Expired records are rejected.
4. Same-sequence conflicting updates are detected.
5. Rollback to a lower sequence is rejected.
6. Oversized records fail before allocation.
7. Unknown record versions fail.
8. Negative and positive responses have the same outer size class.
9. Index node cannot parse private descriptor contents.
10. Unauthorized writers cannot replace records.

Query tests

1. Relay cannot decrypt query.
2. Gateway does not receive client network identity from protocol fields.
3. Query contains no global DID.
4. Request ID is not stable across queries.
5. Repeated application state is not stored at gateway.
6. Query and response classes are fixed.
7. Missing and present labels do not produce distinguishable outer errors.
8. One gateway cannot correlate clients through cookies or sessions.
9. Direct fallback is rejected when private-proxied lookup is required.
10. Batch order is randomised.

Decoy tests

1. Decoys point to valid cover records.
2. Real label position is random.
3. Query cardinality is fixed.
4. Response cardinality is fixed.
5. Missing decoy records do not create different timing.
6. Decoy generation cannot be influenced into exposing the real label.
7. Repeated real queries do not always reuse the same decoy set.

Replica tests

1. One replica outage does not prevent lookup.
2. Conflicting signed epoch roots are detected.
3. Stale replica responses are rejected.
4. Replica identity is authenticated.
5. One replica cannot force a lower record sequence.
6. Clients can compare roots without publishing raw labels.

Retrieval tests

1. Index result alone cannot decrypt the object.
2. Index result alone cannot exercise append or reply rights.
3. Provider candidate cannot substitute a different object undetected.
4. Private provider is never advertised through the public DHT.
5. Fetch paths can use separate relays from lookup paths.
6. Cached descriptors remain encrypted at rest.

⸻

21. Residual floors

21.1 The gateway sees lookup labels in non-PIR modes

Role separation hides who queried, not necessarily what was queried.

21.2 Relay and gateway collusion

Collusion can reconnect query content with client identity.

21.3 Timing correlation

A lookup immediately followed by a fetch can identify interest.

Mitigations:

* delay;
* prefetch;
* separate routes;
* batching;
* cover retrieval.

21.4 Small anonymity sets

A private group with one active member cannot obtain a large crowd merely by encrypting labels.

21.5 Capability compromise

A stolen capability may expose current and future lookup labels until rotation or revocation.

21.6 Endpoint compromise

A compromised device sees:

* capabilities;
* labels;
* cache;
* queries;
* decrypted results.

21.7 Provider observation

A storage provider may infer interest when the object is fetched, even if lookup was private.

21.8 PIR operator assumptions

Multi-server PIR fails if too many replicas collude.

21.9 Long-term intersection

Repeated lookups and retrievals across time may still identify users statistically.

21.10 Search remains harder

Exact private resolution does not solve private free-text search or recommendation.

⸻

22. Recommended design-note core

Status

Privacy boundary and staged protocol for exact private capability lookup. Not a general private search system.

Decision

Mininet public DHT functionality, if introduced, may contain only public routing and deliberately public content records.

Private object, mailbox, feed, shard, subscription, or capability resolution must use a separate private index based on:

* capability-derived rotating labels;
* encrypted fixed-size records;
* authenticated writes;
* short epochs;
* replicated independent index services;
* proxied role-separated queries;
* fixed-size response classes;
* optional batches, decoys, caching, and prefetch.

PIR is a future stronger privacy class, not a prerequisite for the initial private-index implementation.

Hard rule

No private application feature may call a public DHT provider lookup with:

* plaintext private object ID;
* private ciphertext ID used as a stable global handle;
* mailbox capability;
* feed capability;
* recipient DID;
* subscription identifier;
* search term;
* private shard manifest key.

Honest claim

The baseline private index hides client network identity from the index gateway and prevents public-DHT interest broadcast. It does not fully hide the queried label from the gateway unless PIR is used.

⸻

23. Proposed decision-log entry

Decision

Mininet separates public routing/content discovery from private capability resolution.

Public DHT records may serve peer routing and deliberately public content only. Sensitive exact lookups use capability-derived rotating labels in replicated encrypted private indexes. Queries are carried through role-separated relays, with fixed-size requests and responses; batching, caching, prefetch, and decoys provide stronger optional classes. Cryptographic PIR remains a later externally reviewed tier.

Reason

Encryption of content does not prevent a public lookup key from revealing what a user reads, follows, stores, or retrieves. A separate private-index plane prevents global interest broadcast while preserving practical low-cost resolution before advanced PIR is ready.

Constitutional impact

* strengthens P5 by preventing protocol-required disclosure of private interests;
* supports cross-scope unlinkability;
* preserves public content efficiency;
* avoids a mandatory central resolver;
* introduces no global transport identity;
* does not claim perfect reader anonymity;
* preserves user choice of privacy/cost class.

Failure point

The decision fails if capability-derived labels become stable global identifiers, the relay and gateway are commonly controlled, negative responses are distinguishable, retrieval immediately reveals the lookup, or “batched lookup” is marketed as PIR.

Required follow-up

* private-index design and implementation;
* role-separated query transport;
* replica-diversity policy;
* caching and decoy simulation;
* PIR benchmark;
* external cryptographic review;
* retrieval-correlation analysis;
* clear UI residual-risk labels.

⸻

24. Final recommendations

Adopt

1. Freeze the public/private DHT boundary before adding DHT values.
2. Keep peer routing separate from object lookup.
3. Permit public DHT provider records only for deliberately public content.
4. Build a separate private index.
5. Use capability-derived labels.
6. Rotate labels by epoch.
7. Separate lookup, encryption, capability, route, and payment key domains.
8. Encrypt provider and mailbox descriptors.
9. Use fixed record-size classes.
10. Use authenticated exact writes.
11. Use monotonic sequence and rollback protection.
12. Replicate across independent index operators.
13. Carry queries through Tier 1 relay separation.
14. Consider an OHTTP-compatible adapter.
15. Use fixed-size request and response classes.
16. Make negative responses indistinguishable.
17. Add local caching.
18. Add subscription prefetch.
19. Add desired-plus-decoy bundles.
20. Support whole-index downloads for small namespaces.
21. Keep lookup and retrieval routes separate.
22. Treat PIR as a distinct stronger class.
23. Benchmark PIR before adoption.
24. Require external review for PIR.
25. State exact remaining leakage.

Defer

1. General-purpose public value DHT.
2. Private free-text search.
3. Encrypted recommendation queries.
4. Searchable encryption.
5. ORAM.
6. Private set intersection for broad discovery.
7. Single-server CPIR production deployment.
8. Multi-server PIR before operator diversity exists.
9. Global private provider advertisements.
10. Cross-application lookup namespaces.
11. Long-lived stable private labels.
12. On-chain private index commitments containing raw labels.
13. Fully decentralised private search.
14. Anonymous payment integration until lookup primitives stabilise.

Reject

1. Private object IDs in a public DHT.
2. Public mailbox provider records.
3. Public feed capability lookups.
4. Stable private ciphertext IDs as universal query handles.
5. “It is hashed, therefore private.”
6. Encrypting DHT values while exposing the keys.
7. One central resolver that sees IP and interest.
8. Global DID authentication to perform private reads.
9. Cookies or stable sessions at private-index gateways.
10. Variable-size negative responses.
11. Direct retrieval immediately after lookup on the same route.
12. Random nonexistent labels presented as effective decoys.
13. Calling batches or decoys PIR.
14. Assuming independent replicas cannot collude.
15. Claiming PIR hides client IP.
16. Claiming lookup privacy hides endpoint compromise.
17. Building free-text private search inside MN-208.
18. Building a full DHT simply to have something to restrict.

⸻

25. Essay: The Question Can Reveal More Than the Answer

Distributed systems often focus their privacy work on stored data.

The object is encrypted. The storage node sees ciphertext. The network uses content addressing. No central database holds the user’s files. The architecture appears private.

Then the user asks where the object is.

That question can undo the privacy of everything built before it.

A lookup is an expression of interest. It says that a device seeks a particular object, mailbox, feed, profile, shard, topic, or capability. Repeated over time, lookups reveal habits, relationships, subscriptions, work schedules, political interests, health concerns, and social communities.

The response may be encrypted. The object may be encrypted. The lookup key can still be a beacon.

Public DHTs are powerful because they spread the work of resolution. No one server must know everything. A client sends a key into the routing network, nodes forward the request toward peers responsible for that part of the keyspace, and provider records identify where the value can be found.

For public software releases, public media, and public network records, this is useful.

For private interests, the same mechanism distributes the leak.

Every routing participant that handles the key gains some evidence about the requested name. A long-lived provider record shows where content resides. A stable ciphertext ID becomes trackable once any participant learns it. The system avoids one central observer by giving partial visibility to many observers.

This is not automatically better.

MN-208 should therefore begin with a refusal:

Private interests are not public routing keys.

The refusal does not require abandoning decentralisation. It requires separating planes.

The public network still needs to discover peers. It may still route toward public services and public content. But a private mailbox or feed should live in a capability namespace known only to authorised participants.

The capability is not queried directly. It derives a rotating label. That label identifies an encrypted record for a limited epoch and purpose. The record may say where an opaque mailbox currently lives or which storage nodes hold encrypted shards. The index does not need a human-readable name, a user DID, a thread title, or an object type.

Rotation limits the damage of observation. A label seen this week need not track the same private relationship indefinitely. Scope separation prevents the mailbox label from becoming the feed label or storage label.

But secret labels are not magic.

When a client sends a label directly to an index service, the service can see both the network identity and the queried label. It may not know what the label means at first. Over time, timing, repeated queries, provider access, and compromised capabilities can give the label meaning.

The next defence is partitioning.

A relay sees the client connection but not the encrypted query. A gateway decrypts the query but does not receive the client’s network address. Neither party alone has the complete picture.

This pattern now exists in standardised systems such as Oblivious HTTP and Oblivious DNS over HTTPS. Their lesson is broader than HTTP or DNS: privacy can be improved by ensuring that no one role receives every piece of information required for surveillance. (⁠rfc-editor.org)

Partitioning has a floor. The parties may collude. A broad observer may correlate their traffic. The gateway still sees the query label.

Mininet can spend more.

A request can contain several labels. Responses can have fixed size. Decoys can be live cover records rather than obviously random misses. Clients can cache results, prefetch subscriptions, and retrieve bundles on schedules that do not correspond exactly to user actions.

These mechanisms make the target less obvious, but they do not make it cryptographically invisible. Calling them PIR would be dishonest.

Private Information Retrieval is the stronger tool. It lets a client retrieve one database item without telling the database which item was selected.

This sounds like the final answer, but it moves the cost elsewhere.

A single-server computational PIR system asks the server to perform expensive cryptographic work. A multi-server information-theoretic PIR system asks several independent servers to maintain the same database and not collude. Both require careful parameter selection, synchronisation, malicious-response handling, and external review.

A small private community may get better privacy by downloading its entire encrypted epoch index. That primitive solution reveals no item selection because the client retrieves everything. For a large global index, the bandwidth becomes impossible.

The cost doctrine applies again.

There is no one private lookup mechanism for every namespace.

A small mailbox directory may be downloaded whole. A moderate index may use batches and caches. A high-risk fixed-record service may justify PIR. Public content may use an ordinary DHT. A mobile user with a strict bandwidth budget may choose role-separated exact lookup and accept that the gateway sees a rotating opaque label.

The important part is that the choice is explicit.

The system should not silently send the same sensitive key through a public DHT because that was the easiest networking primitive available.

Private lookup must also be separated from private search.

Resolving a capability means the client already knows an unguessable name. Searching for all objects related to a topic is a different problem. Search terms are meaningful. Result counts and ranking reveal information. Fuzzy matching and recommendations create much larger leakage surfaces.

MN-208 should resist absorbing that problem. Exact private resolution is difficult enough and is immediately useful for mailboxes, feed heads, rendezvous descriptors, and shard providers.

The best first system is therefore modest:

* exact capabilities;
* rotating labels;
* encrypted fixed-size records;
* independent replicas;
* a relay between client and index;
* fixed response classes;
* caches and subscription prefetch;
* decoys where their cost is justified;
* a future PIR path.

This does not make reading activity invisible.

The private-index gateway may still see labels. The retrieval provider may see a later fetch. The timing between lookup and retrieval may connect them. A compromised device sees everything. Small communities have small crowds.

But it prevents a worse architectural error: requiring every private interest to be announced to a public routing network.

A private system is not defined only by who can read the stored answer.

It is also defined by who is allowed to hear the question.

MN-208 should therefore begin as a privacy boundary and exact private-index design, not as a DHT implementation. The recommended engineering order is: doctrine → capability-derived index records → replicated private index → relay-separated queries → rotation, caching, batches, and decoys → narrowly scoped PIR research and audit.
