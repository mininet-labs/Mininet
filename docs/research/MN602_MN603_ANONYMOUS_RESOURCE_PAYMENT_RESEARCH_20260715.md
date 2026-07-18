Anonymous Resource Payment and Redemption Research Report

MN-602 / MN-603 — Privacy-Tier Payments Without Identity or Governance Leakage

Repository: mininet-labs/mininet
Predecessor: MN-601 — mini-resource-pricing quote engine
Research date: 15 July 2026
Status: Protocol-selection and external-review preparation; no production payment implementation

⸻

Executive conclusion

Mininet should not pay relay, bridge, mix, storage, cover-traffic, or private-index providers by attaching an ordinary MINI transfer to each service request.

That design would make the payment graph a second metadata graph:

payer identity
    → service provider
    → privacy tier
    → time
    → payload size
    → storage duration
    → likely application action

A user purchasing stronger privacy could become easier to identify precisely because the payment reveals that purchase.

The recommended MN-602/MN-603 architecture is a two-stage anonymous resource-credit system:

MINI or subsidised entitlement
        │
        │ issuance / withdrawal
        ▼
unlinkable fixed-denomination resource tokens
        │
        │ anonymous presentation
        ▼
relay / bridge / mix / storage / index service
        │
        │ redemption
        ▼
provider settlement

The issuer must not be able to link a redeemed token to the withdrawal that created it.

The service provider should learn only:

* the token class;
* denomination;
* validity epoch;
* service scope;
* proof that the token is genuine and unspent;
* any narrowly required anti-abuse data.

It should not learn:

* the payer's global DID;
* wallet history;
* source transaction;
* unrelated token holdings;
* governance role;
* personhood status;
* application object;
* destination identity.

The preferred first protocol is:

Online-spend, issuer-backed, fixed-denomination blind-signature tokens with immediate spent-token checking and batched provider redemption.

This is much narrower than creating a new anonymous currency.

It resembles the privacy structure used by Privacy Pass and Chaumian electronic cash:

1. a client blinds a token request;
2. an issuer authorises and signs it;
3. the client unblinds the signature;
4. the token is later presented unlinkably;
5. redemption checks authenticity and reuse.

Privacy Pass is an IETF-standardised architecture for unlinkable tokens issued in one interaction and redeemed in another; the architecture deliberately separates issuance from redemption and defines origins, issuers, and clients as distinct roles.

GNU Taler is the strongest deployed prior-art family for actual anonymous payment rather than anti-abuse tokens. It uses blind-signature-based coins so the exchange cannot link withdrawal to payment, while supporting merchant deposit and double-spending prevention. Its design deliberately keeps payers private while making merchants accountable. (Wikipedia)

Mininet should borrow these properties, but it should not copy either system wholesale:

* Privacy Pass tokens ordinarily represent authorisation or anti-abuse credit, not monetary value with provider settlement.
* GNU Taler is a complete payment system with exchange, merchant, refund, tax, and banking assumptions much broader than Mininet's internal resource market.
* Coconut provides threshold-issued, rerandomisable credentials and supports anonymous-payment applications, but it introduces pairings, threshold issuance, and a larger cryptographic surface than Mininet needs for the first online-spend version. (arxiv.org)

The staged recommendation is:

Stage	Mechanism	Purpose
MN-602A	Non-monetary test credits	Prove issuance, spend, replay rejection, and provider accounting
MN-602B	Single-issuer blind resource tokens	Private online payment for test services
MN-603A	Batched provider redemption	Convert accepted tokens into provider balances
MN-603B	Threshold issuance or distributed mint	Remove one issuer as the sole availability/trust point
Later	Offline e-cash or accountable double-spend tracing	Only if disconnected spending is genuinely required

Do not begin with offline anonymous cash.

Online spent-token checking is substantially simpler:

provider receives token
→ issuer checks serial has not been spent
→ issuer atomically marks it spent
→ provider receives signed acceptance

Offline spending would require a provider to accept a token without consulting the issuer and later detect reuse. That raises harder questions about:

* double-spend identity exposure;
* merchant risk;
* conflict ordering;
* offline limits;
* payer tracing;
* false accusation;
* disconnected settlement.

Classic offline e-cash research can reveal a double spender after repeated use, but such constructions add identity-escrow and cut-and-choose complexity. They should remain outside the initial Mininet resource-payment protocol.

The strongest initial doctrine is therefore:

1. Quotes are public policy; payments are unlinkable instruments.
2. Tokens buy resources, not voice, reputation, personhood, or validator weight.
3. One payment token is scoped to one service family and epoch.
4. Providers redeem in batches to reduce timing correlation.
5. The issuer cannot see the private service request.
6. The provider cannot see the withdrawal or wallet identity.
7. Subsidies use the same token format as paid credits so subsidised users are not fingerprinted.
8. Every implementation remains externally reviewed before real value.

⸻

1. The problem created by ordinary payments

MN-601 can quote a declared cost for:

* direct transport;
* relayed transport;
* mixed transport;
* burst/high-risk transport;
* storage;
* bandwidth;
* cover traffic;
* computation.

It does not execute payment.

That separation is correct.

A naive next step would be:

Transfer {
    payer,
    provider,
    amount,
    quote_id,
    service_type
}

before opening the service.

This creates several failures.

1.1 Privacy-tier fingerprinting

A user paying for the highest privacy tier identifies themselves as someone needing high privacy.

1.2 Timing linkage

Payment immediately followed by a relay connection or private-index query makes the two actions easy to correlate.

1.3 Provider graph

Repeated payments reveal:

* favourite entry bridges;
* chosen mix providers;
* storage locations;
* mailbox operators;
* recurring applications.

1.4 Global identity leakage

If the payer signs with a did:mini root, the service provider receives a durable cross-service identity.

1.5 Governance contamination

If service access, discounts, or provider status depend on balances that are also visible to governance systems, the voice/value wall may erode indirectly.

1.6 Subsidy fingerprinting

A special "free user" request format identifies subsidised or low-balance users.

Anonymous credits should make paid and subsidised access indistinguishable at presentation.

⸻

2. Required separation of roles

The architecture should distinguish at least five roles.

2.1 Funding source

Supplies MINI or a policy-backed subsidy.

It may know:

* the funding account;
* amount withdrawn;
* withdrawal epoch.

It should not know:

* which service consumes each token;
* which provider receives it;
* exact spend time.

2.2 Token issuer

Verifies withdrawal authorisation and blindly signs token material.

It may be:

* a treasury-backed mint;
* a federation;
* a threshold committee;
* a service-specific issuer.

It should not learn the unblinded token serial.

2.3 Client wallet

Generates blinded token requests, stores unspent tokens, selects denominations, and presents them.

2.4 Service provider

Accepts tokens in exchange for one typed resource.

It should not learn the payer's withdrawal or root identity.

2.5 Redemption service

Checks tokens, marks them spent, and credits providers.

The issuer and redemption service may initially be one operational service, but their logs and protocol roles should remain separable.

⸻

3. What the token represents

The first token must not be a general bearer MINI coin.

It should represent one fixed unit of resource credit.

Examples:

pub enum ResourceCreditClass {
    RelayByte,
    MixPacket,
    StorageByteDay,
    PrivateIndexQuery,
    BridgeSession,
    CoverPacketContribution,
}

A token may be:

1 MiB relayed transport
1 fixed-size mix packet
1 private-index query
1 MiB-day storage

The token is therefore a prepaid service voucher.

This makes the first protocol simpler because:

* denominations are discrete;
* service scope is explicit;
* exchange-rate complexity stays outside spending;
* provider redemption can use the MN-601 quote table;
* tokens cannot silently become a general parallel currency.

⸻

4. Token denomination

4.1 Fixed denominations

Use a small standard set, for example:

1
2
4
8
16
64
256 resource units

Advantages:

* efficient payment composition;
* no arbitrary amount field;
* reduced fingerprinting;
* straightforward accounting;
* no change protocol in the first version.

4.2 Exact-payment leakage

A rare denomination combination may reveal request size.

Mitigations:

* coarse service classes;
* minimum billing buckets;
* standard bundles;
* overpayment with no refund;
* wallet pre-selection independent of the application object.

4.3 No token-specific price

A token should state a resource denomination, not a live MINI conversion rate.

The issuer charges for credits under the current pricing policy during withdrawal.

The service provider redeems according to the issuance epoch or agreed settlement policy.

⸻

5. Blind issuance

5.1 Conceptual flow

The client generates:

serial
token class
denomination
validity epoch
issuer identifier
protocol version

It blinds the token message and sends:

BlindIssuanceRequest {
    blinded_message,
    requested_class,
    requested_denomination,
    count,
    payment_authorisation
}

The issuer:

1. verifies the funding or subsidy authorisation;
2. verifies quantity and class limits;
3. signs the blinded messages;
4. records only aggregate issuance data;
5. returns blind signatures.

The client unblinds them into spendable tokens.

5.2 Issuer-visible fields

The issuer must know enough to charge correctly:

* class;
* denomination;
* count;
* epoch.

These public attributes must be cryptographically bound to the hidden token serial.

5.3 Hidden fields

At minimum:

* unique serial;
* wallet randomness;
* any spend nonce.

5.4 Linkability risk

Issuing one unusual bundle immediately before spending it can defeat blind-signature unlinkability through timing and amount correlation.

Clients should therefore:

* withdraw standard bundles;
* withdraw before immediate need;
* mix paid and subsidised credits locally;
* avoid unique quantity patterns;
* maintain a buffer of tokens.

⸻

6. Token format

Conceptual form:

pub struct ResourceToken {
    pub version: ResourceTokenVersion,
    pub issuer: ResourceIssuerId,
    pub class: ResourceCreditClass,
    pub denomination: ResourceDenomination,
    pub issuance_epoch: u64,
    pub expiry_epoch: u64,
    pub serial: TokenSerial,
    pub signature: BlindTokenSignature,
}

The serial must be:

* random;
* fixed-length;
* globally unique with overwhelming probability;
* unavailable to the issuer during blind issuance;
* included in redemption;
* unsuitable as a cross-protocol identifier.

6.1 Typed signing domain

The signed token transcript must include:

"mininet/resource-token/v1"
protocol version
issuer
class
denomination
issuance epoch
expiry epoch
serial

No generic blind signature over caller-provided bytes should enter the application API.

⸻

7. Spend protocol

The client presents tokens through an already protected Mininet channel.

Conceptual request:

pub struct ResourceSpendRequest {
    pub version: SpendVersion,
    pub service_request_digest: Digest,
    pub provider: ProviderId,
    pub spend_epoch: u64,
    pub tokens: Vec<ResourceToken>,
    pub client_nonce: SpendNonce,
}

The service request itself may remain encrypted or opaque.

7.1 Binding token to service session

A stolen presentation should not be replayable to another provider.

The client should sign or MAC a spend commitment using token-derived material:

spend_commitment =
    H(
        token serial
        provider id
        service request digest
        spend epoch
        client nonce
    )

The exact construction needs external review.

The token itself must remain redeemable only once.

7.2 Provider verification

The provider checks:

* token encoding;
* issuer;
* class;
* denomination;
* epoch;
* signature;
* service-class compatibility;
* total required value.

It then requests online redemption or a reservation.

7.3 Atomicity

The provider must not deliver expensive service before receiving reliable acceptance unless it intentionally assumes payment risk.

Recommended sequence:

client presents tokens
→ provider submits redemption batch/reservation
→ redemption service atomically marks serials spent
→ provider receives signed acceptance
→ service begins

For latency-sensitive services, the provider may maintain a short-lived redemption channel or pre-authorised reservation pool.

⸻

8. Spent-token database

The online issuer or redemption service needs a spent set:

issuer
token serial
spent epoch
provider settlement reference

It must not store:

* payer identity;
* application request;
* destination;
* object ID;
* transport route.

8.1 Atomic check-and-mark

The operation must be:

if serial absent:
    insert serial
    accept
else:
    reject double spend

Concurrent redemptions of the same serial must produce one acceptance.

8.2 Database privacy

A public spent-token list would reveal spend volume and enable service correlation.

The spent set should remain within the redemption service, with public aggregate commitments or audit proofs later if required.

8.3 Retention

Serials must remain retained for at least:

token lifetime + redemption grace + dispute period

Expired serials may later be compacted through committed epoch sets.

⸻

9. Provider redemption

Providers should not redeem every token in a separate externally visible transaction.

9.1 Batched redemption

A provider collects accepted tokens and submits fixed-size or bounded batches.

Benefits:

* weaker timing correlation;
* lower settlement overhead;
* less public graph detail;
* simpler accounting.

9.2 Provider identification

Unlike payers, providers may need accountable identities for:

* settlement;
* service quality;
* tax or legal obligations;
* dispute resolution;
* rate limits.

GNU Taler's architecture deliberately protects the payer while merchants remain identifiable, illustrating that payer anonymity does not require anonymous providers. (Wikipedia)

Mininet may choose either:

* publicly identified providers;
* scoped provider pseudonyms;
* provider committees.

The provider identity must remain separate from governance voting weight.

9.3 Redemption receipt

pub struct RedemptionReceipt {
    pub provider: ProviderId,
    pub batch_digest: Digest,
    pub accepted_value: ResourceValue,
    pub rejected_count: u32,
    pub settlement_epoch: u64,
    pub issuer_signature: Signature,
}

Do not include payer identifiers.

⸻

10. Subsidies

Subsidy is essential because privacy must not be available only to wealthy users.

10.1 Same token format

Subsidised and paid tokens must be indistinguishable at spend time.

10.2 Subsidy issuance

Possible sources:

* universal baseline allotment per identity root;
* community grant;
* emergency censorship allocation;
* application-funded access;
* treasury-approved public-good budget;
* provider promotional credit.

10.3 Sybil risk

A universal allocation per identity root is farmable while personhood remains unsolved.

Therefore early subsidies should use:

* low-value limits;
* application-specific grants;
* community budgets;
* proof-of-work contribution;
* service contribution;
* optional human-evidence policy only where explicitly allowed.

No subsidy mechanism may be represented as one-human-one-share.

10.4 No special presentation bit

The provider should not know whether the token was:

* purchased;
* earned;
* subsidised;
* granted.

⸻

11. Refunds

Refunds are difficult because the service provider should not learn the payer identity and the issuer should not link the spend to the withdrawal.

First-version recommendation

Avoid arbitrary refunds.

Use:

* pre-service redemption reservation;
* provider charges only after it can supply the service;
* partial token consumption avoided;
* provider-issued replacement token on failed service;
* short expiry for reservation.

A replacement token must not permit provider tracing across later spends.

GNU Taler includes a fuller refund protocol, but importing its entire payment model would expand Mininet's scope significantly.

⸻

12. Privacy Pass comparison

Privacy Pass standardises unlinkable token issuance and redemption for authorisation and anti-abuse scenarios.

Its architecture supports the core separation:

issuance context
!=
redemption context

This makes it a good basis for:

* free service credits;
* rate-limit tokens;
* anti-abuse tickets;
* subsidised bridge access;
* private-index query tickets.

It is less directly suited to:

* monetary denominations;
* merchant/provider settlement;
* refunds;
* value accounting.

Recommendation

Use Privacy Pass concepts and, where possible, standard token machinery for non-monetary or issuer-backed service credits.

Do not claim ordinary Privacy Pass tokens are electronic cash.

⸻

13. GNU Taler comparison

GNU Taler is the closest mature model for anonymous payer-visible-provider payments.

Relevant properties include:

* blind withdrawal;
* unlinkable payment;
* deposit/redemption;
* double-spend prevention;
* refunds;
* merchant accountability;
* no blockchain requirement.

It has a significantly larger architecture than Mininet needs:

* exchange operations;
* bank integration;
* merchant contracts;
* taxability;
* reserve management;
* denomination-key management;
* wire transfers;
* refund protocol.

Recommendation

Study and possibly reuse Taler as an external payment rail later.

For Mininet's first internal resource-credit system, adopt only the narrow architectural principles rather than embedding or forking Taler.

⸻

14. Coconut comparison

Coconut supports:

* threshold credential issuance;
* public and private attributes;
* rerandomisation;
* selective disclosure;
* unlinkable multiple presentations;
* tolerance of some malicious or unavailable authorities.

Its authors demonstrate applications including anonymous payments and censorship-resistance proxy distribution. (arxiv.org)

This makes Coconut conceptually attractive for distributed resource credits.

Advantages

* threshold issuer;
* no single mint;
* unlinkable presentations;
* expressive private attributes;
* short credentials.

Weaknesses

* pairing-based cryptography;
* larger audit surface;
* credential semantics broader than one-use tokens;
* double-spend handling still needs application-specific nullifiers or ledgers;
* not part of Mininet's current primitive set;
* no obvious weak-device fit without measurement.

Recommendation

Reserve Coconut or a comparable threshold anonymous-credential system for MN-603B research.

Do not use it for the first token implementation.

⸻

15. Threshold issuance

A single issuer creates:

* censorship risk;
* availability risk;
* monetary-control concentration;
* key-compromise risk;
* issuance surveillance risk.

The long-term system should consider threshold issuance:

t of n issuers jointly authorise token issuance

No one issuer should see both:

* funding identity;
* complete unblinded token;
* spend destination.

Staged design

Phase 1

One test issuer.

Phase 2

Several independent issuers behind one logical mint policy.

Phase 3

Threshold blind signing or threshold credential issuance.

The threshold protocol must be externally reviewed.

⸻

16. Issuer-key epochs

Token-signing keys should rotate by denomination and epoch.

Advantages:

* bounded spent-set retention;
* key-compromise containment;
* explicit expiry;
* pricing-policy transition;
* easier revocation.

Public denomination metadata might include:

pub struct TokenDenomination {
    pub issuer: ResourceIssuerId,
    pub key_epoch: u64,
    pub class: ResourceCreditClass,
    pub value: ResourceDenomination,
    pub valid_from: u64,
    pub withdraw_until: u64,
    pub spend_until: u64,
    pub redeem_until: u64,
    pub public_key: BlindSignaturePublicKey,
}

Clients must reject unknown or downgraded denomination parameters.

⸻

17. Quote binding

A quote should be separate from the payment token.

pub struct ResourceQuote {
    pub quote_id: QuoteId,
    pub provider: ProviderId,
    pub class: ResourceCreditClass,
    pub required_units: u64,
    pub valid_until: u64,
    pub policy_version: PricingPolicyVersion,
    pub signature: Signature,
}

The quote may be public or transmitted privately.

The spend binds to:

quote digest
provider
token total

A provider cannot:

* reuse the spend for another quote;
* charge a different service class;
* silently change the price after token presentation.

⸻

18. Change

A wallet often cannot match a quote exactly with fixed denominations.

Option A — overpay

Simple, but wasteful.

Option B — provider returns anonymous change token

More efficient but creates:

* issuer diversity issues;
* token authenticity;
* linkability;
* provider-minted money risk.

Option C — issuer-mediated change

Provider redeems the full amount and requests a blinded change token for the client.

More protocol complexity.

Recommendation

Version 1 should:

* use granular standard denominations;
* permit bounded overpayment;
* avoid change;
* disclose maximum overpayment before acceptance.

⸻

19. Offline spending

Why it is attractive

Mininet supports:

* BLE;
* local Wi-Fi;
* delay-tolerant exchange;
* intermittent connectivity.

An offline service provider may need payment without contacting a mint.

Why it is dangerous

A bearer token can be copied and spent repeatedly while disconnected.

Possible controls include:

* trusted hardware;
* local spent sets;
* small offline limits;
* identity-revealing double-spend proofs;
* delayed risk-bearing;
* recipient-specific tokens.

Recommendation

Keep offline token use outside MN-602/MN-603 version 1.

A disconnected provider may instead accept:

* unpaid community service;
* pre-established bilateral credit;
* low-risk capped IOU;
* device-bound local quota.

Do not misrepresent these as settled anonymous cash.

⸻

20. Double-spend consequences

In online mode, double spending should result in:

* rejection of the second spend;
* no automatic identity revelation;
* no global reputation penalty;
* no governance consequence;
* optional provider abuse evidence consisting only of the reused serial and signed redemption responses.

A copied token may indicate:

* wallet compromise;
* malicious wallet;
* provider replay;
* network duplication;
* issuer bug.

The protocol should not assume every duplicate proves intentional fraud.

⸻

21. Wallet design

The wallet needs:

* unspent token store;
* reserved token state;
* spent token history;
* denomination selection;
* expiry handling;
* issuer-key updates;
* crash recovery;
* encrypted backup.

21.1 Local metadata danger

Even when the network is private, the wallet database reveals:

* service categories purchased;
* spend times;
* withdrawal patterns;
* subsidy use.

Store only what is required.

21.2 Backup

Copying a wallet backup can duplicate bearer tokens.

The wallet must define:

* backup before spend;
* restore after spend;
* stale backup detection;
* spent-set reconciliation;
* device migration.

Online redemption prevents financial double redemption, but restored wallets may repeatedly attempt stale tokens.

⸻

22. Provider fraud

A provider may:

* redeem tokens but fail to provide service;
* quote one amount and charge another;
* replay a token to frame the client;
* claim redemption failed;
* sell client timing metadata.

Controls:

* signed quote;
* signed redemption acceptance;
* service receipt;
* staged delivery;
* dispute evidence;
* provider reputation separate from governance;
* optional escrow for high-cost jobs.

For small relay or query payments, disputes may cost more than the payment.

The system should favour small atomic service units.

⸻

23. Issuer fraud

An issuer may:

* issue excess credits;
* refuse withdrawals;
* selectively deny users;
* correlate timing;
* falsely reject unspent tokens;
* credit favoured providers;
* manipulate denomination keys;
* retain excessive logs.

Controls:

* public denomination keys;
* issuance accounting commitments;
* redemption receipts;
* independent audit;
* threshold operation later;
* reproducible issuer software;
* capped authority;
* transparent aggregate supply.

Blind signatures hide token serial linkage but do not make the issuer honest.

⸻

24. Supply accounting

Anonymous tokens create a tension:

* the issuer should not know which withdrawal becomes which spend;
* the system still needs to prevent unbacked issuance.

Possible public aggregates:

credits issued per class/epoch
credits redeemed per class/epoch
credits expired
provider settlements
reserve backing

Do not publish:

* individual withdrawal amounts tied to identities;
* individual token serials;
* provider-by-payer links.

A future protocol may use signed issuance and redemption commitments or zero-knowledge reserve proofs.

That is not necessary for the first test-credit implementation.

⸻

25. Voice/value wall

Resource tokens must be structurally excluded from:

* voting;
* review quorum;
* validator weight;
* personhood score;
* identity trust;
* witness selection;
* merge authority;
* constitutional amendment;
* release approval.

No crate providing anonymous payment should be imported by governance-counting code.

Providers may earn settlement value.

That must not create governance authority.

⸻

26. Recommended crate boundaries

Potential modules:

mini-resource-token
    token types
    blind issuance protocol
    spend validation
    denomination metadata
mini-resource-redemption
    atomic spent set
    provider batch redemption
    settlement receipts
mini-resource-wallet
    local token management
    withdrawal
    denomination selection
    spend reservation

mini-resource-pricing remains pure quoting logic.

It should not:

* hold keys;
* issue tokens;
* redeem tokens;
* perform transfers.

⸻

27. External implementation strategy

Do not invent a blind-signature scheme.

Candidate paths:

Path A — IETF Privacy Pass implementation

Best for:

* non-monetary credits;
* rate-limit tokens;
* bridge access;
* private queries.

Requires verifying whether the selected token type supports the required public metadata and one-use semantics.

Path B — GNU Taler integration

Best for:

* actual monetary payment;
* merchant/provider settlement;
* refunds;
* mature exchange semantics.

Requires accepting a much broader external system and licensing/deployment model.

Path C — narrowly reviewed blind RSA or VOPRF token protocol

Potentially simple, but Mininet must not design the composition casually.

Recommendation

First prototype:

Privacy-Pass-style non-monetary test credits behind an external reviewed library.

Second research step:

Compare Taler integration against a Mininet-specific fixed-resource blind-token protocol.

No real MINI should enter the system until that comparison and external review are complete.

⸻

28. Post-quantum implications

Many traditional blind-signature and anonymous-credential systems rely on:

* RSA;
* discrete logarithms;
* elliptic curves;
* pairings.

These are quantum-vulnerable.

A century-scale Mininet design needs migration support.

However, standardised and widely deployed post-quantum blind signatures and anonymous e-cash remain less mature than ML-DSA and ML-KEM.

Therefore:

* version token suites;
* keep lifetimes short;
* avoid promising long-term PQ anonymity;
* make issuer keys agile;
* avoid permanent token history;
* track PQ blind-signature research separately.

The system should not delay all private payments until a PQ anonymous-cash standard exists, but it must state the limitation.

⸻

29. Protocol phases

Phase 0 — doctrine

Produce:

docs/design/anonymous-resource-token-payments.md

Freeze:

* role separation;
* voice/value isolation;
* online spend only;
* fixed resource classes;
* same token format for paid and subsidised credits;
* no production value before audit.

Phase 1 — test token types

Implement:

* denomination metadata;
* token format;
* mock issuance;
* wallet state;
* spent-set semantics;
* no cryptographic blindness claim.

Phase 2 — real blind issuance prototype

Use one reviewed external implementation.

Test:

* issuer cannot link unblinded token;
* token verifies;
* replay rejected;
* malformed requests bounded.

Still use valueless credits.

Phase 3 — provider service integration

Use one low-risk resource:

private-index query

or:

fixed relay byte bucket

No real settlement.

Phase 4 — provider redemption

Implement:

* fixed batches;
* signed receipts;
* atomic spent set;
* provider credit ledger;
* failure recovery.

Phase 5 — adversarial simulation

Test:

* timing correlation;
* unusual withdrawals;
* replay races;
* provider fraud;
* issuer fraud;
* wallet rollback;
* subsidy farming;
* denial of service.

Phase 6 — external cryptographic review

Review:

* blind-signature integration;
* transcript binding;
* unblinding;
* serial generation;
* spent-set logic;
* provider binding;
* token theft;
* timing leaks;
* denomination privacy.

Phase 7 — closed valueless pilot

Operate with test credits and real transport/storage services.

Phase 8 — economic and legal review

Determine whether credits are:

* prepaid service vouchers;
* transferable value;
* electronic money;
* internal accounting units.

Do not guess this in protocol code.

Phase 9 — limited MINI-backed pilot

Only after:

* cryptographic audit;
* accounting review;
* legal review;
* provider settlement tests;
* wallet recovery tests;
* explicit loss limits.

Phase 10 — threshold mint research

Evaluate:

* threshold blind signatures;
* Coconut-style threshold credentials;
* federated issuance;
* distributed redemption.

⸻

30. Adversarial tests

Issuance

1. Valid blinded request receives a valid signature.
2. Issuer cannot derive final serial from stored transcript.
3. Altering class after issuance invalidates token.
4. Altering denomination invalidates token.
5. Altering expiry invalidates token.
6. Duplicate blinded requests follow explicit policy.
7. Oversized issuance batch is rejected.
8. Funding failure issues nothing.
9. Partial failure cannot debit full value without recoverable state.
10. Issuer logs contain no unblinded token.

Spend

1. Valid token pays the matching service class.
2. Token from another class fails.
3. Expired token fails.
4. Wrong issuer fails.
5. Wrong provider/session binding fails.
6. One token cannot be accepted twice.
7. Concurrent double spends yield one acceptance.
8. Token mutation fails.
9. Token order does not change total.
10. Overpayment follows explicit policy.

Redemption

1. Accepted tokens credit the provider once.
2. Duplicate batch submission is idempotent.
3. Reordered batch has the same canonical digest.
4. Mixed valid/invalid batch follows defined atomicity.
5. Provider cannot redeem another provider's reservation.
6. Settlement receipt verifies independently.
7. Spent-set rollback is detected.
8. Crash between mark-spent and credit-provider recovers safely.
9. Expired tokens cannot be resurrected.
10. No payer information appears in receipt.

Privacy

1. Paid and subsidised tokens are indistinguishable at spend.
2. Standard withdrawal bundles do not encode provider.
3. Provider never receives funding identity.
4. Issuer never receives service request.
5. Token serial differs across every token.
6. Spending one token does not reveal wallet balance.
7. Separate providers cannot link tokens by wallet identifier.
8. Exact timing-correlation limits are documented.
9. Logs are redacted.
10. Wallet backups cannot create accepted duplicate redemption.

Governance separation

1. Resource-token balances never enter vote calculations.
2. Provider revenue never changes review quorum.
3. Subsidy eligibility never changes constitutional weight.
4. Human evidence, if used for subsidy policy, never becomes payment proof visible to provider.
5. Governance crates do not depend on resource-token crates.

⸻

31. Technology decision matrix

Ordinary MINI transfer

Privacy: poor
Complexity: low
Settlement: native
Decision: reject for private per-request payment

Privacy Pass-style token

Privacy: strong issuance/redemption unlinkability
Complexity: moderate
Settlement: requires Mininet accounting layer
Decision: preferred first credit-token prototype

GNU Taler integration

Privacy: mature payer privacy
Complexity: high
Settlement: comprehensive
Decision: strongest external-payment candidate; evaluate later

Coconut-style credential

Privacy: strong unlinkable threshold credential
Complexity: high
Settlement: application-specific
Decision: future distributed-mint research

Zcash-style shielded payment

Privacy: strong ledger privacy
Complexity: very high
Settlement: blockchain-oriented
Decision: unnecessary for resource-token v1

Monero transfer per service

Privacy: better than transparent transfer but still creates payment timing and wallet-level operational dependencies
Complexity: external-chain integration
Decision: bridge funding option, not internal resource protocol

Trusted hardware counter

Privacy: potentially good offline
Complexity: vendor-dependent
Decision: optional future offline control, never mandatory

⸻

32. Proposed decision-log entry

Decision

Mininet's priced privacy and resource services use unlinkable fixed-denomination resource tokens rather than identity-linked per-request transfers.

The first protocol is online-spend and issuer-backed:

1. clients withdraw blindly signed resource credits in standard bundles;
2. clients present tokens through an already protected transport;
3. providers verify resource class and denomination;
4. a redemption service atomically rejects reused serials;
5. providers redeem accepted tokens in batches;
6. subsidised and paid tokens use the same presentation format.

Tokens purchase resources only and cannot affect governance, personhood, reputation, validator weight, or review authority.

Reason

Direct transfers expose the relationship between payer, privacy tier, provider, timing, and service volume. Blind issuance separates funding from spending while online redemption provides a simpler double-spend boundary than offline e-cash.

Constitutional impact

* preserves the voice/value wall;
* reduces payment metadata;
* permits privacy subsidies without identifying recipients to providers;
* creates no new governance weight;
* introduces no global payer identity at the service layer;
* does not claim offline anonymous cash;
* remains gated before real value.

Failure point

The design fails if withdrawal timing and denomination uniquely identify spends, if the issuer logs unblinded tokens, if providers redeem before atomic spent checking, if subsidies use a distinguishable token, if wallet backups enable accepted duplicate spending, or if resource credits become a parallel governance currency.

Required follow-up

* external blind-signature implementation review;
* test-credit prototype;
* wallet rollback design;
* atomic redemption store;
* provider batch settlement;
* timing-correlation simulation;
* subsidy-abuse analysis;
* legal/accounting classification;
* external cryptographic audit;
* threshold-mint research.

⸻

33. Final recommendations

Adopt now

1. Keep MN-601 pure and payment-free.
2. Define typed resource-credit classes.
3. Use fixed denominations.
4. Separate funding, issuance, spending, and redemption.
5. Use blinded token issuance.
6. Use random one-use serials.
7. Use online atomic spent checks.
8. Batch provider redemption.
9. Bind spend to provider and quote.
10. Use the same token format for subsidies.
11. Withdraw standard bundles ahead of use.
12. Keep token lifetimes bounded.
13. Rotate issuer keys by epoch.
14. Keep provider settlement accountable.
15. Keep payer identity hidden from provider.
16. Keep service details hidden from issuer.
17. Return typed payment assurance.
18. Use an external reviewed cryptographic implementation.
19. Begin with valueless credits.
20. Preserve the voice/value wall structurally.

Evaluate next

1. Privacy Pass implementations for test credits.
2. GNU Taler as an external monetary rail.
3. Threshold blind-signature systems.
4. Coconut-style distributed issuance.
5. Provider reservation channels.
6. Anonymous change protocols.
7. Public aggregate supply commitments.
8. PQ migration strategy.

Defer

1. Offline anonymous spending.
2. Identity-revealing double-spend tracing.
3. General anonymous currency.
4. Refunds beyond replacement tokens.
5. Arbitrary denominations.
6. Cross-class token conversion.
7. Provider-issued money.
8. Global public spent-token lists.
9. On-chain per-token redemption.
10. Fully decentralised mint before the single-issuer protocol is audited.

Reject

1. Global DID attached to service payment.
2. Public MINI transfer for every private request.
3. Privacy-tier field on a public payment.
4. Distinguishable subsidy token.
5. Token balances affecting governance.
6. Token balances affecting personhood.
7. Provider revenue affecting validator weight.
8. Silent payment downgrade.
9. Reusable bearer token.
10. Non-atomic check-and-mark.
11. Unique withdrawal bundle per request.
12. Issuer-selected token serial.
13. Provider-visible funding source.
14. Issuer-visible service request.
15. New blind-signature cryptography invented inside Mininet.
16. Real-value launch before external audit.

⸻

34. Essay: Privacy Cannot End at the Price

A privacy system may encrypt the object, hide the route, mix the packet, conceal the lookup, and still reveal the user through payment.

The payment often contains the cleanest metadata in the whole system.

A transfer says that one wallet paid one provider at one time for one amount. If the provider sells a specific service, the payment explains why.

A user who buys a mix packet announces that they used the mix network.

A user who pays for a bridge session announces that direct access was blocked or unsafe.

A user who pays for private-index queries announces that they possess private lookup capabilities.

The stronger the privacy product, the more sensitive the purchase may be.

This makes direct payment structurally incompatible with Mininet's cost doctrine.

The doctrine says that privacy consumes real resources and that those resources must be priced. It should not mean that purchasing privacy requires surrendering the privacy being purchased.

Blind tokens create a separation.

The issuer knows that a wallet acquired a standard bundle of resource credits.

The provider later knows that a valid credit was spent.

Neither side alone needs to know the complete path.

This is the same conceptual move used elsewhere in Mininet:

* an entry relay knows the client but not the destination;
* a rendezvous service knows the destination capability but not the client;
* a private-index relay knows the client connection but not the query;
* a gateway knows the query but not the client address.

Privacy comes from refusing to give one role the whole story.

Blind issuance applies that rule to value.

The issuer signs a message it cannot read. The wallet removes the blinding. The resulting token is authentic, but the issuer cannot recognise it later.

The token is then spent as a bearer instrument.

Bearer instruments are powerful because possession is enough. They are dangerous for the same reason.

A copied token can be spent twice.

The first Mininet protocol should resolve this through online redemption, not through an ambitious offline-cash construction. The provider submits the token serial, and the redemption service atomically accepts it once.

This means the system is not fully decentralised and not fully offline.

That limitation is acceptable.

A simple online-spend protocol that is correctly audited is more useful than an offline anonymous-cash protocol whose double-spend tracing, identity escrow, and conflict semantics are poorly understood.

Mininet's local and delay-tolerant ambitions may eventually justify offline payment. When they do, the design must decide who bears risk while disconnected and what evidence a double spend reveals.

Those are political and economic questions as much as cryptographic ones.

The token should also remain narrow.

It should buy a mix packet, a storage unit, a relay bucket, or a private query.

It should not become a new general currency by accident.

A fixed resource token lets the provider verify one exact right without learning the wallet's balance or identity. It also allows Mininet to subsidise privacy.

Subsidy is not optional decoration.

A system that prices every privacy layer but gives no baseline access makes surveillance resistance a luxury product.

The subsidy token must be indistinguishable from the purchased token. Otherwise the provider learns which users could not or did not pay.

The funding policy may differ. The spend protocol must not.

This creates a tension with Sybil resistance. If every identity root receives free credits, farms collect free credits. While personhood remains unresolved, subsidies must remain bounded and application-specific.

The system must prefer some abuse to a design that requires global identification before a person can privately communicate.

Resource credits must also remain separated from authority.

A provider may earn many tokens by carrying traffic. That does not make the provider wise, representative, human, or constitutionally legitimate.

A user may hold many credits. That does not purchase a vote.

The voice/value wall must extend through every dependency edge and every product decision.

This is easy to state and easy to undermine indirectly.

A service may offer better reliability to large holders. A governance client may display provider revenue as reputation. A validator-selection system may favour profitable operators. A subsidy rule may silently reward politically preferred identities.

The payment architecture must make those couplings explicit enough to reject.

Anonymous tokens cannot remove every trace.

A user withdrawing an unusual number of credits and spending the same unusual denominations minutes later may still be correlated.

A rare service class may identify the purpose.

A provider can observe network timing.

A compromised wallet sees everything.

The correct system therefore combines cryptography with ordinary privacy hygiene:

* standard bundles;
* fixed denominations;
* delayed withdrawal;
* batched redemption;
* protected transport;
* coarse billing;
* local token buffers;
* limited logs.

No one technique creates unlinkability alone.

Mininet should also resist the urge to invent.

Privacy Pass provides a standard architecture for unlinkable issued tokens.

GNU Taler provides years of engineering around anonymous payer payments.

Coconut demonstrates threshold-issued rerandomisable credentials.

Each solves a different portion of the problem.

The correct first implementation may use a Privacy-Pass-style token for valueless service credits. The correct long-term monetary rail may be Taler or a separately reviewed protocol. The correct distributed mint may eventually use threshold credentials.

There is no reason to decide all three at once.

The first milestone should be humble:

A user withdraws test credits.

The issuer cannot recognise them after unblinding.

The user spends one through a private channel.

The provider receives an atomic acceptance.

A replay fails.

The provider later redeems a batch.

No payer identity appears in the service or settlement record.

That experiment would prove the architecture's essential claim without putting real value behind unaudited cryptography.

Privacy should be something Mininet can price.

It must not become something the payment graph takes back.
