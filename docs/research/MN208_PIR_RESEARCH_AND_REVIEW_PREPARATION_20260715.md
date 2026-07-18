PIR Research and External-Review Preparation Report

mini-private-index — MN-208 Phase 9

Repository: mininet-labs/mininet
Track: MN-208 private lookup
Phase: 9 — Private Information Retrieval research and review preparation
Research date: 15 July 2026
Status: Technology-selection and external-review brief; not a production protocol specification

⸻

Executive conclusion

Mininet should not select or implement a PIR protocol yet.

The correct next step is a bounded benchmark and external cryptographic-review programme over a frozen mini-private-index database model.

PIR can hide which database row a client retrieves. It does not automatically hide:

* the client's network address;
* when a query occurred;
* which database or epoch was queried;
* the size or frequency of requests;
* later retrieval from an object provider;
* whether multiple PIR servers collude;
* malicious or inconsistent server responses;
* the fact that the user is using a high-privacy mode.

The recommended first PIR target is therefore deliberately narrow:

Retrieve one fixed-size encrypted mailbox or private-provider descriptor from one immutable, epoch-versioned database containing equal-length records.

Do not begin with:

* arbitrary object retrieval;
* full-text private search;
* dynamic key-value operations;
* variable-size results;
* private writes;
* subscriptions;
* general oblivious RAM;
* arbitrary application queries.

The recommended candidate families for external evaluation are:

Candidate	Trust model	Main advantage	Main weakness	Mininet status
Whole-database download	No PIR trust assumption	Simplest and strongest selection privacy	Bandwidth scales with database	Baseline; recommended for small indexes
Two-server information-theoretic PIR	At least one replica does not collude	Simple security model; no heavy client cryptography	Requires independent full replicas	Preferred first true-PIR research candidate
SimplePIR / HintlessPIR family	One computationally bounded server	Very high server throughput	Client hints, preprocessing, database-update complexity	Benchmark candidate
Spiral	One computationally bounded server	Low communication and practical lattice-based design	Heavy server computation and cryptographic complexity	Benchmark candidate
SealPIR	One computationally bounded server	Mature research implementation and Microsoft SEAL foundation	Older efficiency profile; C++/HE dependency	Compatibility baseline, not preferred endpoint
ZipPIR	One computationally bounded server	High throughput without large client-stored hints	Very new 2026 research; insufficient deployment history	Research watchlist only
ORAM / searchable encryption	Different threat model	Supports richer private access patterns	Much larger complexity and leakage surface	Explicitly out of scope

The preferred Mininet research sequence is:

1. freeze a fixed-record database API;
2. measure whole-index download;
3. benchmark a two-server PIR;
4. benchmark one mature single-server lattice PIR;
5. simulate database updates and mobile clients;
6. define malicious-server and collusion assumptions;
7. obtain an external cryptographic review;
8. only then select a protocol for an experimental implementation.

If Mininet cannot operate genuinely independent replicas, it should not claim information-theoretic PIR.

If single-server PIR is too expensive for old mobile devices or small community operators, Mininet should prefer:

* whole-index download;
* fixed bundles;
* caching;
* prefetch;
* role-separated relay lookup;

rather than deploying a complicated construction whose assumptions the network cannot meet.

The likely long-term architecture is a portfolio:

Small private namespace
    → download the complete encrypted epoch index
Moderate namespace with independent operators
    → two-server PIR
Large namespace or no credible non-collusion
    → reviewed single-server computational PIR
Low-resource fallback
    → relay-separated fixed-size bundled lookup

No mode should be labelled simply "private."

The returned assurance should name the actual property:

FullIndexDownloaded
TwoServerPir { assumed_non_collusion: 1 }
SingleServerComputationalPir { scheme, parameters }
ProxiedBundledLookup

⸻

1. Why Phase 9 is correctly gated

PIR is not a normal data-access optimisation.

It is a cryptographic protocol whose privacy depends on details including:

* database layout;
* record size;
* query generation;
* random-number generation;
* cryptographic parameters;
* preprocessing;
* response computation;
* replica independence;
* update procedure;
* malicious-server behaviour;
* query reuse;
* client state;
* transport metadata;
* implementation side channels.

Selecting a scheme before freezing the database model would reverse the correct design order.

A PIR protocol optimised for:

one million fixed 256-byte records

may be unsuitable for:

ten thousand variable 8-KiB records updated continuously

Similarly, a scheme efficient for a large cloud server may be unacceptable for:

* a volunteer node;
* a home server;
* a phone;
* an intermittent community replica.

Phase 9 should therefore produce:

1. a formal workload specification;
2. an implementation shortlist;
3. reproducible benchmarks;
4. a threat-model comparison;
5. a review packet for external cryptographers.

It should not produce protocol code that applications can rely on.

⸻

2. Exact privacy goal

The client knows a private logical label:

L = capability-derived lookup label

The epoch database maps canonical row positions to encrypted records:

row i → fixed-size encrypted descriptor

The client wants record i.

The basic PIR goal is:

The PIR server should not learn i from the cryptographic query and response computation.

This is narrower than:

Nobody learns what the client is interested in.

The latter additionally requires protection of:

* network source;
* timing;
* database selection;
* epoch selection;
* result use;
* fetch correlation;
* client state;
* server collusion.

⸻

3. Recommended first database

3.1 Database purpose

Use PIR first for:

private mailbox descriptor resolution

or:

private provider-descriptor resolution

Each row should contain an encrypted bundle such as:

pub struct PirDescriptorRecord {
    pub version: RecordVersion,
    pub sequence: u64,
    pub valid_until_epoch: u64,
    pub descriptor_ciphertext: FixedBytes<RECORD_PAYLOAD>,
    pub padding: FixedBytes<PADDING>,
}

The server sees only fixed-length opaque rows.

3.2 Properties

The database should be:

* immutable within one epoch;
* deterministically ordered;
* equal-length by row;
* replicated bit-for-bit;
* content-addressed;
* committed by a signed epoch root;
* bounded in total size;
* replaced atomically at epoch transition.

3.3 Why immutable epochs

Many fast PIR protocols benefit from preprocessing tied to one exact database.

Continuous record updates can require:

* hint regeneration;
* server preprocessing;
* client-state refresh;
* replica resynchronisation;
* database-root changes;
* additional leakage.

Immutable epochs turn dynamic updates into:

build new database
→ publish new signed root
→ activate at epoch boundary

This is far easier to analyse.

3.4 Equal-size records

Variable record sizes leak information and complicate PIR layout.

All first-phase records should use one fixed size.

If a descriptor exceeds the limit, it should reference another encrypted object rather than expand the PIR record.

⸻

4. Record addressing

The capability-derived label should not directly become an array position through a simple truncated hash without collision handling.

Recommended database build:

1. derive epoch lookup label;
2. map it into a deterministic authenticated dictionary;
3. assign a canonical row;
4. include enough encrypted metadata to confirm the correct record after retrieval;
5. handle collisions during database construction;
6. commit to the entire ordered database.

Possible structures to evaluate:

* sorted labels with fixed-size rows;
* cuckoo hashing;
* two-choice hashing;
* minimal perfect hashing built per epoch;
* bucketed dictionaries.

The PIR reviewer must evaluate whether the addressing structure leaks:

* label popularity;
* collision class;
* insertion history;
* record type;
* namespace size.

The client should verify after decryption:

record.subject_label == expected_label

inside the encrypted record.

⸻

5. Whole-index download baseline

Before implementing PIR, Mininet must benchmark the trivial privacy solution:

download every encrypted row

This provides perfect item-selection privacy from the server because the client requests the entire database.

It is often dismissed too quickly.

5.1 Example scale

Suppose:

100,000 records
512 bytes each

The raw index is approximately:

51.2 MB

For a daily or multi-hour epoch, that may be acceptable for:

* desktop users;
* community caches;
* local relays;
* Wi-Fi clients;
* subscriptions with incremental epoch updates.

It may be unacceptable for:

* mobile data;
* old phones;
* frequent epochs;
* millions of rows.

5.2 Advantages

* minimal cryptographic risk;
* easy audit;
* offline queries;
* no server computation;
* no collusion assumption;
* good caching;
* excellent anonymity among all rows;
* simple malicious-server detection through signed database roots.

5.3 Disadvantages

* bandwidth proportional to database;
* local storage;
* epoch-update cost;
* database membership disclosure;
* large initial bootstrap.

5.4 Recommendation

Whole-index download must remain a supported privacy mode.

For small community or application-specific databases, it may be the correct final design—not merely a benchmark.

⸻

6. Multi-server information-theoretic PIR

6.1 Security model

The database is replicated across multiple servers.

The client sends correlated queries constructed so that:

* each individual server learns nothing about the target row;
* responses combine to reconstruct the desired record.

Classical information-theoretic PIR assumes the servers do not communicate about the query. The capacity of replicated multi-server PIR depends on the number of servers and messages; the standard model protects the requested index from each individual non-colluding database. (⁠arXiv)

6.2 Why it fits Mininet

Mininet already prefers role separation and independently operated infrastructure.

Two-server PIR can align with:

* separate operators;
* separate ASNs;
* separate jurisdictions;
* identical epoch database;
* no single server learning the target.

6.3 Advantages

* no computational privacy assumption for query selection;
* simple conceptual claim;
* potentially lightweight clients;
* good fit for fixed-size replicated databases;
* future extensibility to more replicas.

6.4 Weaknesses

* privacy collapses when required replicas collude;
* replicas must store identical database state;
* query routing must not reveal target through metadata;
* malicious responses need detection;
* availability requires enough online servers;
* operator "independence" may be fake;
* network observation can correlate paired queries.

6.5 Required deployment rule

The same legal entity or infrastructure operator must not operate both members of a purported two-server PIR pair.

At minimum, require diversity across:

* operator;
* administrative credentials;
* hosting provider;
* ASN where practical;
* logging system;
* funding control.

6.6 Pair selection

The client should not use one universal fixed pair forever.

It may select from approved replica pairs while ensuring both servers hold the exact signed epoch database.

However, pair selection itself can fingerprint a client.

Initial research should compare:

* globally standard pair per epoch;
* random pair from a replica set;
* community-selected pair;
* one local plus one public replica.

6.7 Malicious server

Information-theoretic privacy does not guarantee response correctness.

A replica may:

* return malformed data;
* selectively fail;
* encode identifiers in errors;
* return stale database results;
* attempt to fingerprint queries;
* collude after the fact.

Clients need:

* signed epoch root;
* row-level authenticated encryption;
* deterministic database commitment;
* response validation;
* retry without revealing target;
* blame evidence where practical.

⸻

7. Single-server computational PIR

7.1 Security model

One server stores the database.

Cryptography prevents a computationally bounded server from learning the selected row.

This avoids non-collusion assumptions but introduces:

* hardness assumptions;
* larger implementation complexity;
* heavier computation;
* parameter selection;
* side-channel concerns.

7.2 Why Mininet may need it

In many deployments, credible non-colluding replicas may not exist.

Examples:

* one community operator;
* one home server;
* one regional bridge;
* one organisation with several nominal servers;
* emergency operation.

A reviewed single-server scheme may provide a stronger practical guarantee than falsely claiming that commonly controlled servers are independent.

⸻

8. SealPIR

SealPIR is a research implementation of computational PIR using Microsoft SEAL and homomorphic encryption. It was introduced as a fast PIR system with compressed queries and amortised query processing.

Advantages

* recognised research implementation;
* built on an established homomorphic-encryption library;
* useful baseline;
* public code and literature;
* clear single-server model.

Weaknesses

* C++ and Microsoft SEAL dependency;
* substantial cryptographic parameter surface;
* heavier build and audit boundary;
* older than several newer high-throughput protocols;
* likely not optimal for a new mobile-first system;
* database update and preprocessing costs require measurement.

Recommendation

Use SealPIR as:

* a compatibility baseline;
* a correctness comparison;
* a benchmark reference.

Do not assume it should become the production Mininet dependency.

⸻

9. Spiral

Spiral is a lattice-based single-server PIR design aimed at low communication while retaining practical performance.

Potential advantages

* low online communication;
* modern lattice-based construction;
* attractive for bandwidth-limited clients;
* single-server privacy;
* likely post-quantum-oriented assumptions.

Risks

* complex implementation;
* nontrivial preprocessing;
* server computation;
* parameter sensitivity;
* implementation maturity;
* side-channel and malformed-query handling;
* difficult Rust integration unless a suitable reviewed implementation exists.

Recommendation

Spiral should be on the external-review shortlist.

It should be compared with a high-throughput hint-based design and a multi-server design rather than selected in isolation.

⸻

10. SimplePIR and hint-based PIR

SimplePIR demonstrated very high server throughput by using client-specific or reusable preprocessing hints.

This family is attractive when:

* the database remains stable;
* preprocessing is amortised;
* clients can store or obtain the required hint;
* server throughput matters more than setup cost.

Advantages

* extremely high online throughput;
* simpler online server work than many HE-based designs;
* attractive for popular large databases.

Weaknesses

* hint generation;
* client hint storage;
* distribution cost;
* database-update invalidation;
* privacy implications of client-specific state;
* difficult first query;
* weak-device storage constraints.

Mininet concern

A mobile client that must download or retain a large hint may lose the apparent advantage.

The benchmark must include:

initial hint acquisition
database epoch change
hint storage
query latency
server preprocessing
total bytes over one month

not only online queries per second.

Recommendation

Benchmark SimplePIR or a current successor as the high-throughput single-server candidate.

Do not judge it using server throughput alone.

⸻

11. HintlessPIR and related work

Newer PIR research attempts to reduce or eliminate large client hints while preserving high throughput.

This direction is highly relevant to Mininet because:

* clients may be old phones;
* epochs may change;
* private indexes may be dynamic across epochs;
* long-lived client-specific server state is undesirable.

However, each candidate must be assessed for:

* publication maturity;
* independent implementations;
* proof review;
* parameter security;
* update costs;
* mobile performance.

No scheme should enter production because its benchmark table is attractive.

⸻

12. ZipPIR

ZipPIR was published in 2026 as a high-throughput single-server PIR design without large client-side storage. The authors report throughput above 2 GB/s and less than 200 KB of server-side storage per client for a 1 GB database, while avoiding the large client-stored hints of some prior designs. (⁠arXiv)

This is promising for Mininet's constraints.

It is also very new.

Advantages

* high reported throughput;
* low client storage;
* update-friendly offline design claims;
* relevant to resource-limited clients;
* single-server model.

Risks

* insufficient independent review history;
* no long operational deployment;
* complexity combining LWE and Paillier-style components;
* implementation availability and audit status uncertain;
* 2026 publication means ecosystem evidence is immature.

Recommendation

Place ZipPIR on the research watchlist, not the initial implementation shortlist.

Reassess after:

* independent cryptanalysis;
* public implementation maturity;
* reproducible benchmarks;
* external expert opinion.

⸻

13. Symmetric PIR and database privacy

Ordinary PIR protects the client's selected index.

It may permit a malicious client to learn more than one record, depending on protocol and implementation.

For mini-private-index, records are already encrypted under capability-derived secrets.

Therefore database privacy may primarily come from:

encrypted records
+
capability possession

rather than symmetric PIR.

The external review should still ask:

* Can malformed queries extract linear combinations of many records?
* Does this reveal ciphertext useful for offline attacks?
* Can a client amplify server work?
* Can one query retrieve a large fraction of the database?
* Does the server need proof of well-formed query?

Mininet should not add expensive SPIR machinery unless the encrypted-record model is insufficient.

⸻

14. PIR does not hide the database

A client may reveal its interest by selecting a particular database:

political group index
medical-support index
private community index
regional dissident mailbox epoch

Therefore databases should avoid overly narrow public names.

Possible strategies:

* combine several namespaces into one anonymity domain;
* use standard epoch classes;
* route through relays;
* hide gateway destination through mix transport;
* cache common databases locally;
* download several related database roots;
* use broad community index groups.

This creates a tension:

* larger database gives a larger anonymity set;
* larger database makes PIR more expensive.

That trade-off must be measured.

⸻

15. Transport architecture

The recommended query path is:

Client
  → Tier 1 relay or Tier 2 mix
  → PIR replica(s)
  → relay or mix
  → Client

For two-server PIR:

Client
  ├─ independent route → Replica A
  └─ independent route → Replica B

Do not send both queries over one connection to one common front proxy that logs them together.

Transport requirements

* fixed query-size class;
* fixed response-size class;
* no client identifier;
* no cookie;
* no persistent application session;
* no global DID;
* no capability label outside encrypted PIR query;
* bounded retries;
* database root explicitly authenticated;
* separate query and later object-fetch route.

⸻

16. Query timing

Even perfect PIR leaks that a query occurred.

A client querying immediately after receiving a notification reveals likely interest.

Mitigations:

* periodic query schedule;
* subscription prefetch;
* query batches;
* cover PIR queries;
* cache;
* epoch-wide bulk retrieval;
* random delay;
* separate lookup and fetch timing.

Cover PIR is expensive because the server performs real cryptographic work.

The cost policy must distinguish:

real query
cover query
prefetch query
retry

without making them distinguishable on the wire.

⸻

17. Later object fetch

PIR may privately return:

provider descriptor for object X

If the client immediately connects directly to that provider and requests X, the overall interest becomes visible.

Therefore the full path requires:

1. PIR lookup;
2. delayed or scheduled fetch;
3. private relay or mix transport;
4. opaque object request;
5. fixed-size or bundled retrieval;
6. local cache.

PIR is one component of private retrieval, not the entire solution.

⸻

18. Database integrity

Every PIR server must serve the same committed epoch database.

Recommended metadata:

pub struct PrivateIndexEpoch {
    pub version: EpochVersion,
    pub epoch: u64,
    pub row_count: u64,
    pub row_size: u32,
    pub database_root: Digest,
    pub layout_id: LayoutId,
    pub pir_parameter_set: PirParameterSetId,
    pub valid_from: u64,
    pub valid_until: u64,
    pub signatures: Vec<EpochSignature>,
}

The client obtains this metadata outside the PIR query or from a broadly cached source.

Required protections

* rollback rejection;
* root signature verification;
* exact row count;
* exact row size;
* layout binding;
* parameter binding;
* replica agreement;
* previous-epoch grace;
* no server-selected silent parameter downgrade.

⸻

19. Malicious-server threat model

A server may:

* return random responses;
* return a valid response for another row;
* use a stale database;
* vary timing by inferred query;
* craft malformed responses targeting the client parser;
* attempt lattice decryption-failure or side-channel attacks;
* retain queries indefinitely;
* collude with other replicas;
* encode a client identifier into a response;
* selectively deny service to particular query classes.

The first Mininet PIR implementation must not assume honest-but-curious servers unless the product claim says so clearly.

Minimum malicious-server defence

* authenticated encrypted records;
* committed database root;
* canonical parameters;
* response size bounds;
* constant error class;
* client verification;
* no parsing of unauthenticated variable-length plaintext;
* redundant query option for high assurance;
* replay-safe client state;
* fuzzing.

Formal verifiable PIR may be deferred, but incorrect responses must not become accepted descriptors.

⸻

20. Malicious-client threat model

A client may:

* submit malformed queries;
* force extreme server computation;
* send oversized ciphertexts;
* exploit cryptographic parser bugs;
* retrieve multiple rows;
* flood preprocessing;
* create many client identities;
* abuse cover-query subsidies.

Controls:

* exact query size;
* fixed parameter sets;
* no client-selected cryptographic dimensions;
* request quotas;
* anonymous rate credentials;
* service-chosen proof-of-work;
* bounded concurrency;
* timeout;
* sandboxed cryptographic worker;
* pre-authentication byte limits.

⸻

21. Post-quantum posture

Many modern single-server PIR schemes use lattice assumptions and may be designed with post-quantum security in mind.

That does not automatically make the whole system post-quantum secure.

Review questions include:

* exact LWE/RLWE parameters;
* concrete security estimates;
* quantum attack model;
* homomorphic-encryption dependency;
* database encryption keys;
* transport handshake;
* signatures on epoch metadata;
* long-term retention of queries.

A lattice PIR plus X25519 transport and Ed25519 epoch signatures is not a fully PQ-private system.

The PIR choice should align with Mininet's broader PQ migration but remain a separate reviewed decision.

⸻

22. Implementation language and dependency boundary

Mininet should not reimplement a PIR paper from scratch as its first prototype.

Preferred stages:

1. benchmark upstream/reference implementations unchanged;
2. reproduce published results;
3. wrap behind a process boundary;
4. freeze protocol parameters;
5. obtain review;
6. decide whether a Rust implementation is justified.

Why process isolation first

PIR implementations may contain:

* C++;
* unsafe vectorised code;
* heavy HE libraries;
* assembly;
* large memory allocations;
* attacker-controlled ciphertext parsers.

A separate cryptographic worker reduces the damage of memory-safety bugs.

Conceptual boundary:

mini-private-index
  → typed fixed-size PIR request
  → sandboxed PIR worker
  → fixed-size response

The worker must not access:

* identity keys;
* capabilities;
* application plaintext;
* governance;
* wallet;
* arbitrary files.

⸻

23. Candidate evaluation criteria

Every candidate should be scored on:

Security

* privacy assumption;
* post-quantum posture;
* proof maturity;
* malicious-server model;
* malformed-query resistance;
* collusion model;
* query reuse safety;
* side-channel review.

Client

* query generation time;
* peak memory;
* persistent storage;
* query size;
* decoding time;
* battery;
* old-device support;
* WASM feasibility.

Server

* preprocessing time;
* online CPU;
* memory;
* database expansion;
* per-client state;
* concurrent throughput;
* update cost;
* SIMD dependence;
* GPU dependence.

Network

* query bytes;
* response bytes;
* setup bytes;
* hint bytes;
* epoch-update bytes;
* retry cost.

Operations

* replica synchronisation;
* immutable epoch support;
* reproducibility;
* implementation licence;
* maintenance;
* language;
* audit history;
* deterministic test vectors.

⸻

24. Required benchmark matrix

Database sizes

At minimum:

10 MiB
100 MiB
1 GiB
10 GiB

Record sizes

256 B
512 B
1 KiB
4 KiB

Clients

* old low-end Android class;
* current mid-range phone;
* laptop;
* home ARM node;
* server-class client baseline.

Servers

* low-cost VPS;
* 8-core commodity server;
* 32-core server;
* home node;
* optional GPU only as a separate experiment.

Network profiles

* local LAN;
* 50 Mbps broadband;
* 10 Mbps mobile;
* 1 Mbps constrained;
* 200 ms latency;
* 2% loss;
* Tor path;
* three-hop mix path.

Update profiles

* immutable 24-hour epoch;
* six-hour epoch;
* one-hour epoch;
* 1% record churn;
* 10% record churn;
* complete replacement.

⸻

25. Metrics

Measure:

client setup bytes
client persistent state
client peak memory
query-generation time
query bytes
server preprocessing
server online CPU
server peak memory
response bytes
client decode time
total latency
queries per second
energy estimate
epoch update cost
replica sync cost
failure recovery

Also measure privacy-related operational metrics:

number of replicas
operator diversity
query batching
cover-query overhead
database anonymity-set size
database selection leakage
fetch-correlation window

⸻

26. External-review questions

The external cryptographer should answer:

Protocol selection

1. Which candidate best fits immutable fixed-record epochs?
2. Is two-server PIR preferable under Mininet's realistic operator model?
3. Which single-server scheme has sufficient implementation and proof maturity?
4. Are the selected parameter sets conservative?
5. What post-quantum claim is supportable?

Database model

6. Does the row-addressing mechanism leak label structure?
7. Are equal-size records sufficient?
8. Does epoch rebuilding create identifiable client-state updates?
9. Can database roots be authenticated without weakening query privacy?

Query safety

10. Are queries reusable?
11. Does client randomness failure expose the target?
12. Can malformed queries extract several rows?
13. Can the server fingerprint clients through parameter variation?
14. Does batching weaken formal privacy?

Malicious servers

15. Can a server return a targeted wrong row?
16. Can responses act as tracking tags?
17. What correctness mechanism is required?
18. Does the protocol assume honest-but-curious behaviour?

Multi-server trust

19. What collusion threshold is assumed?
20. Can one common front end destroy non-collusion?
21. How should paired query timing be separated?
22. What operator-independence standard is meaningful?

Side channels

23. Are client decode failures query-dependent?
24. Are server response times query-dependent?
25. Are memory accesses data-dependent?
26. Can compressed queries create parsing or timing oracles?

Implementation

27. Is the reference implementation production-suitable?
28. Should it remain out-of-process?
29. Which test vectors are required?
30. Which fuzzing and side-channel tools are appropriate?

⸻

27. Recommended research candidates

Candidate 0 — complete download

Mandatory baseline.

Candidate 1 — two-server information-theoretic PIR

Preferred first cryptographic candidate because:

* security property is easy to explain;
* clients may remain lightweight;
* Mininet already values independent operators;
* fixed replicated epochs fit naturally.

The research must validate performance and realistic non-collusion.

Candidate 2 — one mature single-server lattice PIR

Select one of:

* Spiral;
* SimplePIR successor;
* another externally recommended current candidate.

Do not benchmark ten schemes superficially.

Candidate 3 — SealPIR baseline

Useful for comparison and interoperability.

Watchlist — ZipPIR

Reassess after external review and implementation maturity. Its 2026 results are promising but too new for the first trusted selection. (⁠arXiv)

⸻

28. Selection logic

Use this decision sequence:

Is complete epoch download within the target budget?
    yes → use complete download
    no  ↓
Are genuinely independent replicas available?
    yes → benchmark two-server PIR
    no  ↓
Is reviewed single-server PIR within client/server budget?
    yes → use selected CPIR experimentally
    no  ↓
Use proxied bundled lookup with explicit weaker assurance.

This prevents cryptographic novelty from becoming mandatory infrastructure.

⸻

29. PIR API boundary

Applications should not select a scheme directly.

Conceptual request:

pub struct PrivateLookupRequest {
    pub database: PrivateIndexDatabaseId,
    pub epoch: u64,
    pub lookup_secret: LookupSecret,
    pub required_privacy: LookupPrivacyRequirement,
    pub maximum_cost: LookupCostBudget,
}

Result:

pub struct PrivateLookupResult {
    pub record: EncryptedDescriptorRecord,
    pub achieved_privacy: AchievedLookupPrivacy,
    pub database_root: Digest,
}

Privacy enum:

pub enum AchievedLookupPrivacy {
    CompleteDatabaseDownload,
    TwoServerInformationTheoretic {
        replicas: [ReplicaId; 2],
        independence_policy: IndependencePolicyId,
    },
    SingleServerComputational {
        scheme: PirSchemeId,
        parameter_set: PirParameterSetId,
    },
    ProxiedBundled {
        bundle_size: u16,
    },
}

Do not return:

Private

as one undifferentiated boolean.

⸻

30. Parameter governance

PIR parameter sets are security-critical.

They should be:

* named;
* versioned;
* externally reviewed;
* immutable within an epoch;
* bound to the database root;
* rejected when unknown;
* changed through a governed decision.

The server must not negotiate weaker parameters dynamically.

Example:

pub enum PirParameterSetId {
    TwoServerV1Record512,
    SpiralV1Database1GiB,
}

The exact scheme names should be added only after selection.

⸻

31. Client-state privacy

Some protocols use persistent hints or preprocessing.

Client state can reveal:

* databases followed;
* epoch history;
* subscription timing;
* application membership.

Requirements:

* encrypted at rest;
* scoped separately from identity keys;
* deletable;
* not synced by default;
* not globally identifiable;
* bounded;
* invalidated safely;
* never reused across unrelated databases.

A hint must not become a stable client identifier sent to the server.

⸻

32. Replica independence

Mininet should define an operational independence policy.

Possible levels:

Level 0:
    Separate processes only
Level 1:
    Separate machines
Level 2:
    Separate administrative operators
Level 3:
    Separate operators and hosting providers
Level 4:
    Separate operators, ASNs, and jurisdictions

Only Levels 2–4 should count toward a non-collusion privacy claim.

Even then, independence is an assumption.

The UI and protocol result must say:

assumed non-collusion between Replica A and Replica B

not:

mathematically impossible to link query

⸻

33. Database publication

The epoch database should be reproducible from authorised private-index records.

Possible flow:

collect authorised records
→ canonicalise
→ pad
→ sort/layout
→ build immutable database
→ compute root
→ replicas independently reproduce
→ sign matching root
→ activate epoch

A client should prefer an epoch root signed by multiple independent builders or operators.

This prevents one replica from privately constructing a database layout targeted at one client.

⸻

34. Query retries

Retries can leak.

If one server fails, querying another server immediately with a related query may enable correlation.

The policy should define:

* maximum retries;
* fresh query randomness;
* route changes;
* delay;
* whether a failed two-server query can fall back to one-server mode;
* downgrade approval.

A request requiring two-server information-theoretic PIR must not silently fall back to direct single-server lookup.

⸻

35. Cover queries

Cover PIR queries may hide whether the user had a real interest.

They also consume real server resources.

Potential models:

* client-generated cover;
* subscription schedule;
* community-funded cover;
* relay-generated aggregate cover;
* cached whole-index epochs.

Cover policy must consider:

* cost;
* fairness;
* abuse;
* low-income users;
* server capacity;
* distinguishability.

A high-risk mode used only by a few users may itself be a fingerprint.

⸻

36. Negative records

Every valid row should decrypt to either:

present descriptor

or:

authenticated empty descriptor

The outer PIR response must not reveal whether the row was empty.

The client may need to distinguish:

* valid empty;
* wrong key;
* stale epoch;
* malicious response.

These distinctions should remain local and not trigger unique retry behaviour visible to the server.

⸻

37. Test plan

Database tests

1. Same records produce the same canonical epoch database.
2. All rows have identical length.
3. Different record ordering changes the root and is rejected.
4. Replica roots must match.
5. Unknown layout IDs fail.
6. Epoch rollback fails.
7. Empty records are indistinguishable before decryption.
8. Label collisions resolve deterministically.
9. Retrieved record confirms the intended label internally.
10. Database size is bounded.

PIR correctness tests

1. Every row can be retrieved.
2. Wrong database root fails.
3. Wrong epoch fails.
4. Malformed query fails safely.
5. Malformed response fails safely.
6. Server cannot select the returned row.
7. Client cannot silently accept another row.
8. Query randomness is fresh.
9. Concurrent queries remain independent.
10. Retry uses fresh cryptographic state.

Two-server tests

1. Each individual query distribution is independent of target in test vectors.
2. One response alone cannot reconstruct the target record.
3. Both responses reconstruct correctly.
4. Different database roots fail.
5. One malicious response is detected where the selected protocol supports it.
6. Colluding-server simulation demonstrates privacy loss honestly.
7. Both queries use independent transport paths.
8. Common front-proxy use is rejected under strong mode.
9. Replica identities are authenticated.
10. Independence policy is included in achieved assurance.

Single-server tests

1. Server-side logs contain no target index.
2. Query size is fixed.
3. Response size is fixed.
4. Timing variation across rows is measured.
5. Malformed ciphertexts remain bounded.
6. Parameter downgrade fails.
7. Reference vectors match.
8. Cross-implementation vectors match where available.
9. Worker crash does not expose client secrets.
10. Worker is sandboxed.

Correlation tests

1. PIR lookup and object fetch use separate routes.
2. Fetch delay policy is applied.
3. Database choice does not expose application name where avoidable.
4. Repeated queries are scheduled or cached.
5. Cover and real queries have the same shape.
6. Error handling does not create target-specific retries.

⸻

38. Benchmark acceptance gates

No prototype should progress until it meets explicit budgets.

Illustrative—not yet final—budgets:

Mobile client

* query generation under several seconds;
* peak memory within old-device limits;
* no multi-gigabyte hint;
* bounded battery impact;
* response decode practical over mobile data.

Community server

* affordable commodity hardware;
* bounded memory;
* sustainable queries per second;
* no mandatory GPU;
* reproducible deployment;
* database preprocessing within epoch window.

Network

* fixed query and response classes;
* total monthly cost comparable to the privacy benefit;
* Tor/mix overhead measured;
* no hidden huge setup transfer.

External reviewers should help set the actual numbers.

⸻

39. Release gates

No production-facing PIR claim until:

1. workload is frozen;
2. database layout is specified;
3. candidate is externally reviewed;
4. security parameters are documented;
5. reference implementation is pinned;
6. benchmark is reproducible;
7. mobile tests pass;
8. malicious-query bounds pass;
9. malicious-response handling is defined;
10. collusion assumptions are displayed;
11. transport metadata limits are documented;
12. retrieval correlation is tested;
13. failure and downgrade behaviour are explicit;
14. implementation audit is complete;
15. public documentation avoids saying PIR provides total anonymity.

⸻

40. Decisions that remain blocked

Do not decide yet:

* production PIR scheme;
* lattice parameter set;
* two-server versus one-server default;
* client hint format;
* native Rust rewrite;
* global PIR replica set;
* economic subsidy model;
* cover-query rate;
* database epoch duration;
* malicious-server proof system;
* post-quantum marketing claim.

These require benchmark and external review.

⸻

41. Proposed external research package

Send reviewers:

01-threat-model.md
02-fixed-record-workload.md
03-database-layout.md
04-candidate-comparison.md
05-benchmark-methodology.md
06-replica-independence-policy.md
07-malicious-server-model.md
08-transport-correlation.md
09-mobile-results.md
10-open-questions.md

Also include:

* reproducible benchmark scripts;
* hardware descriptions;
* raw result data;
* candidate source commits;
* build recipes;
* packet captures;
* failure traces;
* parameter files;
* no private user data.

⸻

42. Proposed decision-log entry

Decision

Mininet does not select or implement a production PIR protocol for mini-private-index until a fixed-record, immutable-epoch workload is benchmarked and externally reviewed.

Phase 9 evaluates:

1. complete encrypted index download;
2. two-server information-theoretic PIR with genuinely independent replicas;
3. one mature single-server computational PIR;
4. a non-PIR proxied bundled lookup baseline.

The first PIR use case is exact retrieval of one fixed-size encrypted mailbox or provider descriptor.

PIR remains one component of private lookup and does not imply network anonymity, fetch anonymity, server non-collusion, or malicious-server correctness.

Reason

PIR performance and security depend strongly on database size, update pattern, record layout, client resources, server resources, and trust assumptions. Selecting a scheme before freezing these properties would create an unaudited load-bearing cryptographic dependency.

Constitutional impact

* strengthens private-interest protection;
* avoids public-DHT disclosure;
* preserves weak-device support;
* keeps advanced cryptography optional;
* does not create one mandatory index operator;
* does not claim stronger privacy than achieved;
* introduces no governance or value authority.

Failure point

The decision fails if nominally independent replicas share control, if database choice identifies the user's interest, if PIR lookup is immediately correlated with direct fetch, if malformed queries exhaust servers, or if an experimental implementation is marketed as production-private before review.

Required follow-up

* freeze workload;
* build whole-index baseline;
* benchmark two-server PIR;
* benchmark one single-server PIR;
* simulate updates;
* test mobile clients;
* define replica independence;
* commission cryptographic review;
* select one experimental candidate;
* implementation audit before pilot.

⸻

43. Final recommendations

Adopt now

1. Keep Phase 9 research-only.
2. Freeze one fixed-size record type.
3. Freeze immutable signed epochs.
4. Build whole-index download baseline.
5. Benchmark two-server PIR.
6. Benchmark one mature single-server PIR.
7. Use SealPIR only as a baseline unless review recommends it.
8. Place Spiral on the shortlist.
9. Place a current SimplePIR-family implementation on the shortlist.
10. Keep ZipPIR on the watchlist.
11. Use relay or mix transport around PIR.
12. Separate lookup and fetch paths.
13. Authenticate database roots.
14. Define malicious-server behaviour.
15. Define malicious-client bounds.
16. Define replica independence.
17. Return achieved privacy as a typed result.
18. Require external cryptographic review.
19. Prefer out-of-process prototype workers.
20. Publish reproducible benchmarks.

Adopt later if supported by evidence

1. Two-server PIR for independent community replicas.
2. Single-server lattice PIR for large indexes.
3. Whole-index download for small namespaces.
4. Scheduled cover queries.
5. private subscription prefetch;
6. replicated database builders;
7. malicious-response proofs;
8. post-quantum parameter alignment;
9. audited native implementation.

Defer

1. Free-text private search.
2. Variable-size records.
3. Private writes through PIR.
4. ORAM.
5. Searchable encryption.
6. Arbitrary application predicates.
7. General private DHT replacement.
8. GPU-mandatory serving.
9. Client-specific permanent server state.
10. Global PIR infrastructure.
11. Economic pricing before benchmarks.
12. Native Rust rewrite before protocol selection.

Reject

1. Selecting PIR from paper benchmarks alone.
2. Calling batching PIR.
3. Calling OHTTP PIR.
4. Claiming PIR hides client IP.
5. Claiming PIR hides later fetch.
6. Claiming two replicas are independent because they use different hostnames.
7. Silent fallback from two-server to direct lookup.
8. Client-selected cryptographic parameters.
9. Variable response sizes.
10. Unauthenticated database roots.
11. One global PIR server.
12. Production code before external review.
13. New cryptography invented inside Mininet.
14. Treating the newest protocol as automatically best.
15. Ignoring whole-index download because it looks unsophisticated.

⸻

44. Essay: Privacy Sometimes Means Asking for Everything

Private Information Retrieval offers an unusually compelling promise.

A server holds a database. A client requests one record. The server computes the answer without learning which record the client selected.

For Mininet, this appears to solve the central private-index problem. A user can discover a mailbox, feed head, or provider without broadcasting the lookup label through a public DHT and without revealing the label to an ordinary index server.

But PIR is easiest to misunderstand when described only by its strongest sentence.

The server may not learn the row.

It can still see the client's network connection.

It can still see when the query happened.

It can still see which database and epoch were selected.

It can still observe that the client uses the expensive high-privacy protocol.

Another provider may see the object fetched moments later.

Two supposedly independent replicas may share an operator and compare their queries.

A malicious server may return the wrong descriptor.

PIR protects one dimension of one operation.

This does not make it weak.

It makes precise design necessary.

The first discipline is to choose the operation before choosing the cryptography.

"Private lookup" is not one workload.

Retrieving a 512-byte mailbox descriptor from a one-gigabyte immutable database is different from searching text, retrieving a video, updating a feed, or locating a variable-size object.

A protocol optimised for one may perform terribly for another.

Mininet should begin with the narrowest useful row: one fixed-size encrypted descriptor in one signed epoch database.

This creates a surface cryptographers can actually review.

The second discipline is to respect the trivial solution.

A client that downloads the entire index reveals no row selection. The server cannot know which record mattered because the client received every record.

This costs bandwidth instead of cryptographic computation.

For a ten-megabyte community index, complete download may be obviously superior to a lattice-based protocol, a server cluster, client hints, and years of maintenance.

Cryptographic sophistication should not be mistaken for privacy quality.

The third discipline is to name the trust model.

Multi-server PIR can provide information-theoretic privacy against each individual server, but only when the required servers do not collude.

Two machines in the same cloud account are not meaningfully independent.

Two services operated by one company are not meaningfully independent.

Two legal entities that share logs, administrators, and funding may not be independent.

The mathematics can protect the query only inside the assumed organisational reality.

If Mininet can recruit independent community, university, cooperative, or public-interest operators, two-server PIR becomes attractive. It gives a simple claim: neither operator alone learns the requested row.

If such independence does not exist, the honest alternative is single-server computational PIR. The privacy then rests on cryptographic hardness rather than organisational separation.

This creates a different cost.

The client generates an encrypted query. The server performs computation over a large database. Modern protocols have made this increasingly practical, but "practical" in a paper may mean a powerful server, a stable database, a large preprocessing phase, or persistent client hints.

Mininet's users may have old phones. Its operators may have home servers. Its database may change every few hours. A benchmark that reports only server gigabytes per second can hide the costs that matter most.

The benchmark must include the first query, the mobile client, the database update, the hint transfer, the Tor path, and the failed retry.

The newest protocols are exciting. ZipPIR, published in 2026, reports high throughput without the large client-side hints associated with some earlier designs. (⁠arXiv)

That is exactly the kind of result Mininet should watch.

It is not the kind of result Mininet should make load-bearing immediately.

New cryptography needs time for other researchers to examine the proof, reproduce the implementation, test the parameters, and find the assumptions hidden by benchmark conditions.

The purpose of Phase 9 is to create that distance between interest and adoption.

A serious PIR programme should make it easy for an external cryptographer to say:

* the workload is wrong;
* the parameter set is unsafe;
* the replicas are not independent;
* the client hint is too large;
* the database update leaks;
* the malformed-query defence is incomplete;
* complete download is better.

That criticism is the product.

Only after the criticism survives should code become the product.

PIR also demonstrates why privacy must be layered.

Suppose the client privately retrieves a provider descriptor, then immediately opens a direct connection to that provider and requests the corresponding object. The lookup server did not learn the row, but the provider learned the interest.

The complete system needs:

* private lookup;
* private transport;
* delayed fetch;
* opaque object identifiers;
* fixed-size retrieval;
* caching;
* application discipline.

No one cryptographic primitive can absorb all of those roles.

This should influence the user-facing language.

A client should not receive a green badge saying "private."

It should receive a precise result:

row hidden from each of two assumed non-colluding replicas;
client IP hidden through relay;
database and query time still observable;
later object retrieval separately protected.

That sentence is longer than a badge.

It is also useful.

The deepest question is not whether Mininet can implement PIR.

It can.

The question is where PIR creates more privacy than complexity.

For small indexes, asking for everything may be best.

For independent replicated indexes, splitting the question may be best.

For a large single server, encrypting the question may be best.

For a weak device under severe bandwidth limits, a bundled proxied lookup may be the only practical option.

A system designed for centuries should preserve all four choices.

PIR should not become a constitutional dependency, one universal network service, or a symbol of sophistication.

It should remain what it actually is:

A specialised way to ask one server-held database a question without revealing which answer was wanted.

Used in the right place, that is powerful.

Used without its assumptions, it is only expensive obscurity.

The strongest immediate deliverable is a research-only PR containing the fixed workload, benchmark methodology, candidate shortlist, and external-review questions. No PIR crate should be added until that package has been reviewed and the whole-index and two-server baselines have been measured.
