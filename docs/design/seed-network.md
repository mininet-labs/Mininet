# Owner-controlled seed network

Mininet's shared-content direction is a network of participating nodes and
servers: signed public content can outlive its original publisher, selected
history remains available for an owner-chosen duration, and providers collect
verifiable delivery evidence and payment promises. Resident AI can help explain
and operate this infrastructure; it does not own it or authorize spending.

This draft implements a developer CLI runtime over real sockets. It is not yet
a globally discovered, anonymous Internet, an independent-replica guarantee,
a paid-ranking scheme or a production payout marketplace. The narrow supported
profile is paired, explicitly trusted nodes with reachable numeric TCP endpoints.

## Working implementation

- `mini-seed` maintains a **separate** persistent pool of exact public signed
  object bytes. The CLI verifies signature/delegation/provenance before admission
  or receipt acknowledgement; the pool itself verifies integrity, not authorship.
- The owner opts in with a byte quota and each content ID's retention period.
  Expired objects disappear from inventory and new reads immediately, even before
  physical pruning. Reads never extend leases. Renewing is another explicit add.
- A seed-only mode of the existing four-worker service offers this pool, never
  the original object store. It reuses encrypted sessions with transcript-bound
  mutual endpoint authentication, pre-trusted KELs and explicit peer allowlists.
- Inventory, quotes and fetches are real network exchanges. Content travels in
  bounded chunks; receivers verify its exact ID, public payload, signature and
  BLAKE3 digest before signing a `mini-storage` serve receipt. Both devices must
  be delegated with ATTEST, and their identity roots must differ.
- Explicit reseeding allows a receiver to become another provider. `follow`
  polls one chosen peer for new seeded objects within a bounded duration/count,
  prunes expired pool copies, and fetches/reseeds within quota. It spends nothing
  automatically and remembers already-held/fetched IDs for that run, so polling
  does not silently reset their retention after expiry.
- Paid offers price **delivery of serialized object bytes**, rounded up per MiB.
  The owner supplies a wallet-signed `mini-settlement::PaymentClaim`. A cap,
  exact amount, payee, settlement network, signature and validity must match.
  Only a digest commitment travels before delivery. The signed claim is released
  after verification, with the witness acknowledgement. The provider persists a
  per-network/payer/sequence reservation and a mutually signed delivery receipt.
  Export retains the original canonical claim wire bytes.

A retention lease is a local commitment, not proof of continuous possession or
independent replicas. An inactive provider earns no payment from this feature.
A receipt proves a witnessed serve, not future uptime or honest demand. Existing
storage-proof, anti-farming and canonical monetary rules remain separate.

## Node setup and use

Use the existing identity/KEL pairing workflow from the native Forge runbook.
Both endpoints must already trust the other's current root and device KELs.
Receivers also need the content author's verified KELs; seeding does not turn a
provider into authority over the author's identity. Run these commands on each
node's own home; do not share identity seed files.

```text
mini --home <node-home> seed configure --max-bytes 268435456
mini --home <publisher-home> --store <source-store> seed add --id <signed-object-id> --days 7 --allow-public-seeding
mini --home <publisher-home> seed serve --addr 0.0.0.0:7401 --allow-peer <reader-root-did>
mini --home <reader-home> seed peers --addr <publisher-ip>:7401 --peer <publisher-root-did>
mini --home <reader-home> --store <reader-store> seed fetch --addr <publisher-ip>:7401 --peer <publisher-root-did> --id <signed-object-id> --reseed-days 3
```

`add` copies exactly the selected object. It does not recursively export links,
Git history, attached media blobs or author KELs. Select the additional signed
history objects explicitly. Private/encrypted objects and private/cold cache tiers
are refused. Original source objects are neither moved nor pruned. Quota defaults
to zero; configuring it alone neither selects content nor starts networking.

Start a seed service on the reader, allowing another paired node, to retrieve the
same content from that second provider. Seed and whole-store Forge services share
one per-home listener lock; use separate node homes for simultaneous services.
`service status` reports `scope:seed-pool` or `scope:whole-store`.

```text
mini --home <reader-home> seed follow --addr <publisher-ip>:7401 --peer <publisher-root-did> --days 3 --rounds 3600 --interval-ms 1000 --max-objects 100
mini --home <reader-home> seed follow-stop
mini --home <node-home> seed status
mini --home <node-home> seed inventory
mini --home <node-home> seed revoke --id <signed-object-id>
mini --home <node-home> seed prune
mini --home <node-home> service status
mini --home <node-home> service stop
```

`follow-stop` persists until an explicit subsequent `follow --resume`. Stop is
checked between rounds/requests; an in-flight bounded transfer may finish. Only
one follow runs per home. A paid peer is refused by its zero automatic budget.
Following is explicit peer polling, not global discovery or a search index.
Optional reseeding failure is reported separately from an already completed
verified delivery. `follow` stops on that failure instead of silently continuing.

Retention in this slice controls **pool** storage. A fetched object's ordinary
source-store copy remains until that store's own cleanup policy removes it.
Expiry is not a remote deletion command and cannot erase copies held by others.
A transfer that started with a valid lease may finish after expiry or revocation;
new reads cannot start. Multiple owners deliberately choose different durations.

## Paid delivery and settlement

By default serving is free. To opt into paid delivery, configure the provider's
wallet payee bytes, an exact 32-byte settlement network ID and a micro-MINI/MiB
rate. These are service terms, not a new monetary issuance or protocol fee rule.

```text
mini --home <provider-home> seed price --micro-per-mib 7 --payee-hex <wallet-payee-hex> --network-hex <32-byte-network-hex>
mini --home <reader-home> seed quote --addr <provider-ip>:7401 --peer <provider-root-did> --id <object-id>
mini --home <reader-home> seed fetch --addr <provider-ip>:7401 --peer <provider-root-did> --id <object-id> --max-micro 7 --payment-claim <wallet-signed-claim-file>
mini --home <provider-home> seed earnings
mini --home <provider-home> seed payment-export --receipt <receipt-id> --output <new-claim-file>
```

Create the matching payment claim through an owner-controlled wallet using the
existing settlement API. This command set neither generates a wallet nor signs
payments. Payment keys are not identity seeds. The quote's payee is an opaque
wallet address selected by that provider; verify its terms before authorizing a
claim. Claims/amounts and this paired transport are not confidential payments.

**AcceptedLocal means accepted locally and still awaiting settlement; it is never
a spendable balance or proof of getting paid.** The
exported claim must enter the existing canonical settlement path. This runtime
does not submit it, query finalization, reserve a canonical balance or implement
escrow. Cross-provider conflicting claims are resolved canonically, not by local
journals. A provider gives bytes before receiving the signed claim, and a malicious
receiver can withhold payment/acknowledgement. A connection failure after the claim
is sent can leave acknowledgement uncertain; local evidence survives. Owners must
accept that credit risk to enable paid serving. Real fair exchange and automated
canonical payout reconciliation remain unfinished work, not implied guarantees.

Revenue, storage or replicas never grant governance votes or human legitimacy.
No earned amount feeds organic ranking. Free protocol participation stays free;
this optional provider price is a quote for an additional external resource.

## Bounds, persistence and validation

- Pool: explicit quota up to 1 GiB, 4,096 objects, 64 MiB per serialized object,
  1..3,650 retention days, bounded atomic manifest and OS-locked mutations.
  Quota accounts for stored object bytes, not all filesystem metadata overhead.
- Pool object then manifest admission and manifest-before-unlink cleanup recover
  from orphan files. Unknown files, malformed state, missing blobs and symlinks
  fail closed. The pool does not follow links into original source storage.
- Transfer: 64 KiB content chunks, 128 KiB application records, at most 4,096
  inventory objects, four service workers, inherited bounded server deadlines
  and a 120-second absolute client-session deadline. The underlying bearer has
  its own larger bounded frame allocation before application rejection.
- Evidence/claim journals: 4,096 records each, explicit archival required when
  full. Nonce replay checks cover both device roles; payment reservations reject
  reuse of the same network/payer/sequence at that provider. Local journal removal
  or restoring all state together is outside those replay guarantees.
- Paths assume an owner-controlled filesystem and cooperating processes. This
  is not a sandbox against a malicious concurrent local administrator. There is
  no source-store-wide pruning or globally atomic content/payment transaction.

Run focused software checks:

```text
cargo test -p mini-seed --locked
python3 tools/check_native_forge.py --tests seed_network native_service --offline
```

Twelve core tests cover restart, expiry/renewal, quota, concurrent writers,
corruption, encryption, symlinks, orphan recovery, signed catalogues, rollback,
withdrawal and bounded batch reads. Eight real TCP seed tests
cover three-node propagation and following, unselected/private/untrusted content,
peer authorization, revocation without source loss, pending paid delivery/export,
price caps, claim replay, optional reseed failure, signed discovery/relay/import,
fallback mesh following, owner opt-out and verified paged search. The existing service test
covers authentication, stop and restart regression. No hardware run, actual wallet
funding, finalized payout, independent audit or Internet-scale benchmark is implied.

## Signed discovery and a live multi-provider mesh

Provider discovery now has an owner-local signed catalogue, rather than a
mandatory registry. An explicitly enabled provider advertises a complete snapshot
of its active seeded IDs, byte sizes and lease expiries, with its public endpoint,
root/device signature, issue time and bounded expiry. Empty snapshots withdraw
previous entries. A consumer verifies the exact author, current delegation and
signature, rejects rollback or conflicting equal-time snapshots, and retains
high-water marks across restart and explicit catalogue removal.

```text
mini --home <provider-home> seed announce --addr <reachable-ip>:7401 --ttl-ms 60000
mini --home <reader-home> seed contact add --name first --addr <reachable-ip>:7401 --peer <trusted-provider-root>
mini --home <reader-home> seed catalog refresh
mini --home <reader-home> seed locate --id <object-id>
mini --home <reader-home> seed retrieve --id <object-id> --reseed-days 3
mini --home <reader-home> seed mesh-follow --days 3 --rounds 3600 --interval-ms 1000 --max-objects 100
```

Add other paired nodes as contacts to obtain fallback retrieval and continuous
refresh/reseeding across those contacts. Retrieval tries candidate contacts in
stable name order, verifies actual content/receipts and isolates failed peers.
It never automatically pays. Paid providers require the explicit quote/fetch flow.
`mesh-follow` reads current contacts each round, respects the shared `follow-stop`
marker and `--resume`, prevents simultaneous follow jobs on one home, caps its
transfer count and preserves existing lease durations during that run. Failure
reports are bounded to 64 entries. Each round's candidate union is capped at
4,096 IDs; narrow activated contacts if that profile is exceeded.

Contacts are explicit **owner-local dial permissions**, not a provider licensing
board or identity grant. A saved endpoint must name an already trusted root;
changing it requires explicit removal and re-addition. Merely receiving a signed
advertisement never activates its endpoint. Retrieval uses the configured contact
address, even if the signed advertisement describes another address. The current
trusted KEL/delegation is rechecked before using cached hints; encrypted session
authentication and actual content verification remain independent requirements.

```text
mini --home <reader-home> seed catalog refresh --contact first
mini --home <reader-home> seed catalog pull --contact bootstrap
mini --home <reader-home> seed catalog list
mini --home <reader-home> seed catalog export --peer <provider-root> --output <new-advertisement-file>
mini --home <other-home> seed catalog import --file <advertisement-file>
mini --home <reader-home> seed catalog remove --peer <provider-root>
mini --home <reader-home> seed contact list
mini --home <reader-home> seed contact remove --name first
mini --home <provider-home> seed unannounce
```

`refresh` requests that contact's own new signed snapshot. `pull` relays the
contact's cached original provider-signed snapshots, retaining their original
attribution and requiring each signer's current trusted KELs. Partial acceptance
and rejected snapshots are reported. Offline export/import uses the same signed
bytes. Third-party discovery still requires explicit trust and endpoint activation
before dialing; a relay cannot redirect a reader to arbitrary advertised hosts.
Advertisements are public redistribution metadata once announced. Do not announce
an endpoint or selected ID inventory that must remain private.

`unannounce` shares the issuer's lock, so after it returns no new snapshot may be
signed under the removed configuration. Already-issued snapshots/in-flight bytes
may remain useful until expiry. Revoke content or stop serving to deny new actual
reads. Metadata hints do not remotely delete content and never prove independent
replication, future uptime or personhood.

Bounds: advertisement TTL at most 15 minutes, future clock skew at most 30 seconds,
4,096 unique entries and 1 MiB per signed object; the local catalogue is at most
8 MiB and tracks 64 provider high-water marks, including removed providers.
Tombstones intentionally do not free high-water slots. Contacts: 64 saved,
16 automatically queried per operation. A relayed exchange has at most 64 objects
and 8 MiB total bytes. Refresh, fallback retrieval and search each share one
120-second network deadline across their selected contacts. No DNS or automatic
third-party endpoint dialing is introduced.

## Verified search of seeded public content

```text
mini --home <reader-home> seed search --query "history"
mini --home <reader-home> seed search --query "history" --contact first --limit 4
mini --home <reader-home> seed search --query "history" --contact first --limit 4 --after <returned-object-id>
```

This is a bounded **literal UTF-8 content search**, not the separate web ranking
pipeline or a global index. Queries are explicitly disclosed to the selected
contacts. The server searches only its public seed pool, excluding unselected,
expired, private and cold source-store objects. It sends complete signed matching
source objects, not unverifiable snippets; the reader verifies author/delegation,
content ID and the literal match before generating the displayed excerpt locally.
Search results are not automatically inserted in the ordinary store or reseeded.

At most 16 selected contacts, eight hits per provider/page, 64 KiB per serialized
source object, 16 MiB scanned source bytes per provider/page and a 256-byte query
are permitted. Binary/large objects are outside this search profile. Incomplete
pages return provider-specific continuation cursors; continuing requires exactly
one selected contact. A provider may omit matches, so signatures establish returned
content authenticity, not exhaustive search or truth. Offline/bad providers are
reported without hiding other valid results. IDs are deduplicated and ordered by
object ID; payments, replica counts and provider popularity never rank results.
The cache/store verification context is loaded once per response and pool reads
are batched under one manifest load to avoid repeated full metadata scans.

Because this proof-carrying search transmits complete small source objects,
providers with nonzero delivery prices refuse it. The explicit paid fetch flow
remains necessary there; search cannot bypass their configured delivery price.
This has no effect on free public protocol participation or organic web ranking.

Still unfinished: open global peer discovery/DHT and NAT traversal, unpaired
public-reader onboarding, media-chunk and linked-history expansion, integration
with the existing web query/ranking system, desktop mesh controls and automated
canonical payout/fair exchange. This remains a bounded developer profile, not a
claim that the complete Internet replacement is ready to launch.
