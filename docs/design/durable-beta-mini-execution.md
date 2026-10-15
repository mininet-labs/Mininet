# Durable Beta MINI execution and authenticated wallet accounts

**Issue:** #337  
**Parent milestone:** #334  
**Depends on:** #339 / stacked PR #341 for multi-party grant acceptance  
**Status:** implementation scope for a stacked draft PR; Beta-only, not production money

## Exact failure being closed

`mini-beta::BetaMiniLedger` is intentionally an in-memory reference accounting core. It proves local arithmetic and grant invariants, but it does not provide authenticated wallet ownership, durable transfer evidence, restart recovery, replicated conflict handling, or a product-safe distinction between provisional beta state and final production value.

Persisting that mutable `HashMap` would not solve the problem. Two offline devices could spend the same balance and later replicate incompatible histories. Choosing the first object seen, the smallest hash, the newest timestamp, or a server answer would manufacture hidden authority.

This work therefore derives Beta balances from **immutable signed account/transfer objects plus threshold-accepted grant objects**, and fails closed on conflicting spends instead of selecting a local winner.

## Core model: authenticated accounts + UTXO-style Beta outputs

### Beta account registration

Add `mininet.beta/account/v1`.

Each Beta account uses a fresh account-scoped `did:mini` root/device pair. It is not the user's normal Mininet identity and must not be reused across unrelated contribution rewards merely to create reputation continuity.

The account id is derived, not arbitrary:

```text
BetaAccountId = BLAKE3("mininet/beta/account/v1" || owner_root_did || random_nonce)
```

The registration object contains:

- beta epoch;
- random nonce;
- derived `BetaAccountId`;
- optional non-identifying local display hint only if it is not part of canonical payload.

The object author root is the account owner. Validation re-derives the id from author root + nonce. Another DID therefore cannot register the same account id merely after observing it.

A user may have many Beta accounts. Account continuity exists only for spending that account; it creates no personhood, governance, contributor reputation, or future production entitlement.

### Durable outputs

Accepted value exists as immutable outputs:

1. **Grant output** — output 0 of one threshold-accepted Beta grant; or
2. **Transfer output** — indexed output of one valid Beta transfer object.

An output identifies:

- exact source object id;
- output index;
- beta epoch;
- recipient BetaAccountId;
- integer micro-BETA-MINI amount.

There are no fees in v1. Transfers conserve value exactly.

### Signed transfer

Add `mininet.beta/transfer/v1`.

A v1 transfer:

- is authored by the fresh account root/device that owns every input;
- identifies the exact epoch and sender account;
- consumes bounded explicit output references;
- creates bounded explicit recipient outputs;
- requires all inputs to belong to the sender;
- requires every input/output amount to be non-zero;
- requires exact `sum(inputs) == sum(outputs)` with checked arithmetic;
- may include a short non-sensitive memo;
- cannot consume outputs from another epoch; and
- cannot mint, burn into an implicit fee, or carry production-value semantics.

Recipient accounts must already have valid registrations for the same epoch.

## Deterministic replicated resolver

Add a separate `mini-beta-exec` crate rather than making `mini-beta` depend on storage/finality/governance systems.

The resolver derives one epoch snapshot from a **complete verified object set**. Remote objects are assumed to have crossed `mini-sync`'s signature/KEL/provenance ingest boundary before reaching the store.

### Phase A — registrations

Strictly parse all Beta account registrations for the epoch.

- one derived account id -> one registration object;
- multiple registrations for the same derived account id fail that account closed;
- malformed or wrong-epoch registrations do not create spend authority.

### Phase B — accepted grants

For each Beta campaign in the epoch:

1. require exactly one valid grant policy through #341's fail-closed policy resolver;
2. collect exact signed approval evidence;
3. require threshold acceptance of each candidate grant;
4. require the grant recipient account to have one valid registration;
5. detect more than one threshold-accepted participation grant for the same contribution and invalidate the conflicting reward set instead of choosing a local winner; and
6. checked-sum all otherwise accepted grants against the epoch supply bound.

If accepted issuance exceeds the epoch supply cap, the epoch enters an **issuance-conflict** state. Do not sort grant ids and mint the first ones until the cap is reached; that would turn a hash ordering into monetary authority.

### Phase C — transfer candidates

Strictly validate every transfer's encoding, epoch, sender registration/author, referenced outputs and amount conservation.

Build the potential output graph before choosing validity. This prevents arrival order from deciding conflicts.

### Phase D — double-spend conflict rule

If two otherwise structurally valid transfers consume the same output, **all conflicting consumers are invalid**.

No timestamp/hash/arrival-order winner.

Because only the account authority can validly sign those spends, a conflict is owner equivocation/self-DoS rather than a third-party way to steal the output.

### Phase E — dependency resolution

After removing structurally invalid/conflicting transfers:

- grant outputs are roots;
- a transfer is valid only if every producer transfer for its inputs is valid;
- cycles/unresolved references fail closed;
- invalid producer outputs cannot be resurrected downstream; and
- balances are the sum of valid unspent outputs owned by each account.

The result must be independent of object insertion order.

## Late-arriving conflicts and provisional status

This resolver gives deterministic eventual convergence, **not instant finality**.

A node may initially see transfer A and later learn of an equally signed conflicting transfer B. Once both are present, the derived state invalidates both. Therefore:

- wallet UX must label locally derived balances/transfers as **BETA / provisional**;
- no irreversible real-world settlement may rely on this layer;
- providers must understand that beta receipts are test evidence, not production finality; and
- #338 remains responsible for canonical Forge state/finality before any stronger claim.

This is preferable to pretending an eventually replicated store has consensus.

## Durable restart model

The immutable objects themselves are the durable ledger. A restart:

1. reopens the `mini-store` backend;
2. rehydrates verified KEL state through existing sync rules;
3. reruns the deterministic resolver; and
4. obtains the same snapshot from the same object set.

Do not persist a mutable balance file as canonical state. A balance cache may exist later only if it is disposable and bound to a digest of the exact resolved object set.

## Wallet secret storage

Product/CLI wallet code may persist fresh account root/device seed material under a Beta-specific wallet directory with owner-only filesystem permissions, following the existing CLI identity seed-file discipline.

Hard rules:

- never derive a Beta wallet key from GitHub username, public profile, contribution id, legal identity, main Mininet identity, MINI balance, or a reusable reward handle;
- one contribution may choose a fresh account;
- account seed loss means loss of that test account unless an explicit Beta recovery mechanism is later designed;
- no Founder/server recovery master key;
- wallet backup is user-controlled test key material, not a protocol administrator function.

## Claim handoff

PR #342's `ClaimTag` becomes a **public commitment** to private claim material, not the bearer secret itself. #337 must preserve that distinction.

A participation-grant request may prove knowledge of the private claim preimage to the grant workflow and designate a fresh registered Beta account. The public contribution object must never contain that preimage.

This PR must not make the public claim commitment itself sufficient to steal a reward.

## Epoch reset / retirement

Every account, grant output and transfer is bound to one exact non-zero `BetaEpochId`.

Starting another epoch creates an entirely separate output graph:

- old outputs are not valid inputs;
- balances do not carry over;
- account ids may be recreated only through fresh same-epoch registrations;
- no conversion ratio exists; and
- no production MINI right is created.

A user-facing current/retired epoch marker is operational metadata. Until #338 defines canonical epoch authority, a single server/founder-signed retirement flag must not be treated as universal monetary truth.

## Product surface

Initial CLI/product surfaces should expose:

```text
mini beta wallet new
mini beta wallet list
mini beta wallet balance <account>
mini beta wallet outputs <account>
mini beta wallet transfer <account> --input ... --to ... --amount ...
mini beta wallet status
```

Every balance/transfer view MUST display:

- `BETA MINI` (never bare `MINI`);
- exact beta epoch;
- provisional/conflicted status;
- no production conversion promise.

A transfer error should distinguish wrong epoch, unknown/unregistered account, missing output, insufficient selected value, double-spend conflict, invalid producer, and issuance conflict.

## Replication acceptance

A two-node test uses independent stores and normal `mini sync`:

1. create fresh account registrations;
2. sync registrations/KEL carriers;
3. sync campaign/policy/grant/approvals;
4. both nodes derive the same grant output/balance;
5. create and sync a transfer;
6. both derive the same outputs/balances;
7. create an offline double spend on one node;
8. before sync the nodes may show different **provisional** snapshots;
9. after full object replication both identify the same conflict and converge to the same fail-closed snapshot;
10. restart both stores and prove the snapshot digest is unchanged.

No GitHub API, balance server, trusted sequencer, or hidden reconciliation endpoint may participate.

## Crate boundary

Proposed `mini-beta-exec` runtime dependencies:

- `did-mini`
- `mini-beta`
- `mini-beta-grants`
- `mini-objects`
- `mini-store`

No production value, settlement, treasury, chain, consensus, Forge governance, personhood/economy, airdrop, HTTP, or GitHub dependencies.

`mini-beta` remains the small reference object/accounting crate rather than absorbing durable execution policy.

## Adversarial tests

Required before this slice can leave draft:

- account id cannot be claimed by another author;
- duplicate account registrations fail closed;
- grant to unregistered account is not spendable;
- one valid accepted grant becomes exactly one output;
- duplicate participation reward conflict creates no winner by object ordering;
- epoch-supply oversubscription fails closed;
- transfer without account-owner author is rejected;
- input from another owner is rejected;
- wrong epoch is rejected;
- zero amount/overflow/non-conserving transfer is rejected;
- missing input is rejected;
- same input spent twice invalidates all conflicting consumers;
- downstream outputs of an invalid/conflicted transfer are invalid;
- input/order insertion permutations derive identical snapshot digest;
- independent stores converge after sync;
- restart from `FsBackend` derives identical state;
- new epoch starts with zero usable prior outputs; and
- no Beta event can be interpreted as production MINI conversion evidence.

## Values verdict

- **Authenticated ownership — PASS when transfers require a fresh account-scoped delegated signer and derived account id.**
- **Durability — PASS when restart derives the same state from immutable objects rather than a privileged mutable balance database.**
- **Double-spend safety — PARTIAL.** Conflicts fail closed and converge; instant finality is not solved. Exact remaining fix is #338 canonical replicated state/finality.
- **No hidden sequencer — PASS if conflict handling never picks first-seen/hash/timestamp/server order.**
- **Anonymous contribution separation — PASS at the account-object layer if accounts use fresh roots and private claim preimages never enter public receipts. Network/timing privacy remains PARTIAL.**
- **Voice/value wall — PASS if this crate has no path into governance/reviewer/release/personhood weight.**
- **Production-money readiness — FAIL by design.** This is resettable Beta execution and cannot substitute for production cryptography/custody/settlement audits.

## Exit condition

The #337 durable-execution core is complete when authenticated account/transfer objects, deterministic fail-closed resolver, restart/two-node convergence tests, dependency wall, and unmistakable Beta/provisional wallet surfaces are green on exact-head CI. Canonical irreversible finality and bootstrap-authority retirement remain #338 rather than being hidden behind a local tie-break.
