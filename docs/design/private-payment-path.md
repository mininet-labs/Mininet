# The private payment path: shielded settlement, and the social vertical it serves

**Decisions:** D-0447 (see `docs/DECISION_LOG.md`)
**Status:** **Proposed.** Prototype cryptography, unaudited, gated behind D-0047/[#72](../../issues/72). Nothing here may carry real value until an external cryptographic audit says otherwise.
**Refs:** D-0036/D-0040 (`mini-value`'s stealth addresses, ring signatures, Bulletproofs); D-0045/D-0055 (M1/M2/M3, `mini-settlement`); D-0061 (`mini-execution::LedgerChain`); D-0417 (`mini-contribution`); D-0402 (`mini-engagement`); D-0033 (`Store::note_view`, PR2); Directives 1, 9, 11, 13, 14, 16, 17; invariants P1, P5, PR2, V2, V3, M1, M2, M3.

---

## 1. The gap, stated precisely

`mini-value` has had real stealth addresses, real ring signatures, and real Bulletproofs since D-0036/D-0040. **Nothing composed them into a payment.**

Every payment this tree can actually make goes through `mini_settlement::PaymentClaim`:

```rust
pub struct PaymentClaim {
    pub network_id: [u8; 32],
    pub payer: Vec<u8>,          // a stable public key
    pub payee: Vec<u8>,          // a stable address
    pub amount_micro: u64,       // cleartext
    pub sequence: u64,           // a per-payer counter
    pub valid_until_ms: u64,
    pub last_known_chain: Vec<u8>,
    pub signature: Signature,
}
```

Every crate that pays anyone settles through it: `mini-contribution` for creator and seeder payouts (D-0417), `mini-engagement` for escrowed work (D-0402), `mini-bounty` for development bounties. So the complete transaction graph is public by construction — who paid whom, how much, and in what order.

The `sequence` field deserves particular attention. It is a counter *per payer key*, so an observer who sees two claims from the same payer immediately has their ordering, and by collecting claims gets that payer's entire ordered payment history. No cryptanalysis is required; the format hands it over. This is strictly worse than Bitcoin's privacy, which at least uses a fresh address per output.

That is not a gap in a privacy feature. It is the absence of one, in a project whose Directive 9 says privacy is *architecture* rather than a promise, whose P5 forbids protocol-level personal data, and whose stated purpose is returning ownership of data to the people it describes.

There is a second, sharper way to see it. `mini_store::Store::note_view` was built deliberately to take **no viewer identity** — the doc comment says so explicitly, and PR2 freezes the behavior. The storage layer refuses to record who read what. Then the payment layer publishes who paid whom for what. Protecting the view and leaking the payment protects nothing.

## 2. What was built

`mini-private-payment` is the composition that was missing. Its `PrivatePaymentClaim` is the shielded counterpart of `PaymentClaim`:

| transparent | private |
|---|---|
| `payer: Vec<u8>` — a stable key | **absent.** A ring signature proves *some* member of an anonymity set authorized this. |
| `payee: Vec<u8>` — a stable address | a fresh `mini_value::StealthOutput`, one per payment. |
| `amount_micro: u64` — cleartext | a Pedersen commitment plus a Bulletproof range proof. |
| `sequence: u64` — per-payer counter | **absent.** The key image is the conflict key. |
| (nothing — a purpose field would leak) | `SealedMemo`, AEAD-sealed to the recipient. |

Nothing cryptographic is invented. All three primitives already existed and are already governed by D-0036/D-0040; this crate composes them and adds the transcript, codec, nullifier, and reconciliation discipline around them.

### 2.1 The split transcript, and why

> The layout below is the **v1** claim: one input, one output, no fee. §12
> replaces it with the v2 conservation format. The split-transcript
> *argument* is unchanged and is why v2 keeps the same two-transcript shape,
> so it is left standing here rather than rewritten.

The memo is sealed with the claim as AEAD additional data, so it cannot be lifted off one payment and stapled onto another. But the ring signature must also cover the memo, or the memo could be swapped or stripped.

Doing both naively is circular: the memo is bound to a transcript that contains the memo. The resolution is two transcripts:

```text
binding_transcript = domain ‖ version ‖ network_id ‖ tx_public_key ‖ one_time_address
                   ‖ amount_commitment ‖ range_proof ‖ valid_until_ms
                   ‖ last_known_chain ‖ ring
transcript         = binding_transcript ‖ memo
```

- the **memo** is sealed with `BLAKE3(binding_transcript)` as AAD — so it cannot move to a claim paying a different address or committing a different amount;
- the **signature** covers `transcript`, which includes the memo — so the memo cannot be swapped or stripped.

Both bindings hold, neither is circular. This was found by a `debug_assert` firing during development, not by reasoning; the assertion stayed.

### 2.2 The key image, and the honest limit

`RingSignature::key_image` is deterministic in the one-time secret being spent. Spending the same output twice produces the same key image, which is what makes double-spend detection possible without a public payer. It replaces `(payer, sequence)` as the conflict key throughout.

**It is linkable, by design, and that cost is real.** Two spends of the same output are linkable to each other. They are not linkable to a person, an identity root, or any other payment — but "unlinkable" is too strong a word for what this achieves, and the crate never uses it unqualified. This is the standard CryptoNote trade-off, adopted deliberately rather than stumbled into.

### 2.3 M1/M2/M3, unchanged

The invariants do not relax because a payment is shielded. `reconcile` returns `mini_settlement`'s own `SettlementState` rather than a parallel enum — two enums meaning almost the same thing is how a wallet ends up rendering a private payment as final under rules the transparent path would have called pending.

- **M1** — there is no function anywhere in the crate that combines two claims. A conflicting claim is `RejectedConflict`, full stop. `KeyImageSet` keeps the first and refuses the second; it does not net, sum, or prefer the larger.
- **M2** — `PendingCanonical` until a `PrivateLedgerView` says otherwise. `is_final()` is true only for `Finalized`.
- **M3** — conflicts resolve by asking the ledger, never by arrival order, local preference, or amount. `canonical_ordering_alone_resolves_a_conflict` proves exactly one of two conflicting claims finalizes, and that the larger amount does not win.

### 2.4 Two gaps closed in `mini-value` along the way

Both were blocking, and both are the kind of gap that only appears when someone tries to use the code rather than read it:

- **A `RangeProof` could not be serialized.** Its fields were private with no encode/decode, so a confidential amount could never cross a wire — the entire `ConfidentialAmountScheme` was usable only within one process. Added `to_bytes`/`from_bytes` with a fixed 672-byte width (no length prefixes: a 64-bit range proof has exactly one size, so nothing in the encoding is attacker-steerable).
- **The stealth shared secret was computed and discarded.** `derive_output` computes `r*B` to derive the one-time address and throws it away, so a sender had no way to attach anything only the recipient could read. `derive_output_with_secret`/`recover_shared_secret` return it. This introduces no new cryptography — it exposes a value the existing derivation already produces — and the type is `Debug`-redacted and zeroized on drop.
- A third, smaller one: `MininetRingSignature` could not be constructed without a secret key, so every verifying call site had to invent one. `verifier()` is fail-closed — its index is out of range for any ring, so it cannot sign.

## 3. The social vertical

The motivating use is a reader paying a creator for a post. `crates/mini-private-payment/tests/unity.rs` runs it end to end with real primitives at every step:

```text
mini-social       publish a post          -> ObjectId, needs POST and never VOTE (V3)
mini-store        a reader views it       -> cache tier, no viewer identity (PR2)
mini-private-payment  the reader pays     -> stealth output + ring signature + range proof,
                                             with the ObjectId sealed in the memo
mini-settlement   shared vocabulary       -> Pending / AcceptedLocal / Finalized
(pending)         canonical consensus     -> the only thing that makes it final
```

The tests assert the properties rather than narrating them: the amount never appears in the wire bytes in either endianness; no eight-byte run of the post id appears anywhere in the claim; the creator's `did:mini` root and its bare SCID both appear nowhere; five readers paying one creator produce five distinct addresses and five distinct commitments, so no public income ledger exists; and a billion-micro-MINI finalized payment leaves the post byte-identical and grants the payer nothing capability-shaped.

**No permanent dependency edge was created.** `mini-private-payment` does not depend on `mini-social`; `PaymentPurpose` carries opaque caller bytes. A payment layer that knew what a post was would be a payment layer that could be made to treat some posts differently. The social dependency exists only in `dev-dependencies`, where it demonstrates the composition without making it structural.

## 4. The voice/value wall

This is a **value** crate. It depends on `mini-value`, `mini-crypto`, and `mini-settlement`, and on nothing governance-shaped — no `mini-forge`, no `mini-chain` voting, in either direction (P1, Directive 16).

Directive 16's list is explicit that money may buy attention. This crate is a mechanism for exactly that and no more: a paid post is byte-identical to an unpaid one, and a finalized payment yields the payer no capability, no weight, and no quorum standing. `paying_for_a_post_grants_the_payer_no_capability_over_it` is the test that keeps it that way.

Private money buying quiet governance would be the worst version of the failure Directive 16 exists to prevent, precisely because it would be unobservable. The wall matters *more* here, not less.

## 5. What this deliberately does not do

- **Decoy selection.** `MIN_RING_SIZE` bounds the ring's size; nothing can judge whether the decoys are plausible. A ring of eight whose other seven members are visibly long-spent outputs hides nobody, and every signature check still passes. §7.1 records this as an open protocol question.
- **Network-level privacy.** Timing, IP, and traffic analysis belong to `mini-relay` and `mini-transport-security`. A cryptographically private payment broadcast from a fixed IP immediately after viewing one post is not private in practice, and this crate cannot help with that.
- **Amount aggregation.** Individual amounts are hidden; the *number* of payments and their timing are not.
- **Multi-output transactions.** One claim, one output. Change outputs, fee outputs, and the balance check across them (`verify_balance` exists in `mini-value` and is unused here) are a real design step, not an oversight — see §8.
- **Sybil resistance.** Nothing here counts humans, and nothing here may ever be cited as if it did ([#18](../../issues/18)).
- **Consensus.** `PrivateLedgerView` has no chain-backed implementation, exactly as `mini_settlement::CanonicalLedgerView` waited for `mini_execution::LedgerChain` (D-0061). Until one exists, no private payment can reach `Finalized` in production.

## 6. Why not just add fields to `PaymentClaim`?

Considered and rejected. The transparent claim is depended on by `mini-contribution`, `mini-engagement`, and `mini-bounty`, all of which reasonably read `amount_micro` and `payer`. Making those fields optional would give every existing consumer a silent "privacy off" path and a new `None` case to forget, and Directive 14 says the smaller, well-trodden construction wins. A separate object with its own verification path means a caller chooses transparency or privacy explicitly, and a crate that has not opted in cannot accidentally handle a shielded claim as if it were transparent.

## 7. Open questions for founder/governance review

1. ~~**How are decoys chosen?**~~ **Answered by founder direction 2026-08-07; implemented in D-0449 — see §10.** Enforced by protocol for everyone, via the ring signature that already exists, not via a mixer service.
2. ~~**Is `MIN_RING_SIZE = 8` defensible?**~~ **Answered: raised to 16, with 8 frozen as an absolute floor (D-0449).** The remaining question — what the *right* number is, from measured traffic rather than judgement — stays open and cannot close until traffic exists.
3. ~~**Should the transparent path remain available at all?**~~ **Answered by founder direction 2026-08-07 — "invert or no transparency at all is good no real need for anything to be public". Implemented as D-0451; see §11.** No. Auditability becomes a disclosure a party makes about itself, not a property of the format. The five-crate migration off the transparent path is required follow-up, not done here.
4. **What is the fee model?** A private payment with no fee output cannot pay for its own inclusion, and a transparent fee attached to a shielded payment reintroduces a linkable value.
5. **Where does the anonymity set come from on a weak device?** Directive 11: a phone that must hold the whole output set to pick decoys is a phone that cannot make private payments at all. D-0449 settles the *policy* — the set is local, because asking a peer for decoys hands that peer your ring — but not the engineering. A one-time key is 32 bytes, so a million outputs is 32 MB and prunable, which is more tractable than it first sounds; below that threshold the honest answer is that the device does not make private payments, not that it makes weaker ones.

## 10. Decoy selection (D-0449)

`PaymentRequest` no longer carries a `ring`. It carries a `ring_size`, a `real_output_index` into the caller's local [`OutputSet`], and fresh `decoy_entropy`; `build` calls `select_ring` and the protocol picks the members.

### Why this could not be left to wallets

Two distinct failures, and only the first is obvious:

1. **Bad decoys break the payment that uses them.** A ring of sixteen whose other fifteen members are visibly long-spent outputs hides nobody, and every signature check still passes. Silent failure.
2. **Different decoys break payments that don't.** If two wallets sample differently, an observer can tell *which wallet* made a payment from the shape of its ring. That harms users who did nothing wrong — including users of the *better* wallet, because "unusual" is what stands out, and the smaller population is the more identifiable one.

(2) is the reason this cannot be an implementation choice even in principle. A per-wallet policy makes every wallet's users a smaller anonymity set than the network's.

### Why not a mixer

A mixer is a service — value in, different value out — and that reintroduces a coordinator, a pool, an operator, and something seizable or simply gone tomorrow. Directive 2 says assume exactly that. It also requires trusting the mixer's logging policy, which is a promise rather than mathematics (Directive 9).

**The ring signature already is the mixing.** Every spend mixes with N decoys, locally, with no pool and nothing to shut down. Nothing needed adding; only the rule for choosing who to mix with was missing.

### Why the set must be local

A peer that serves decoy keys learns your ring, and your ring contains your real output. There is no phrasing of that request which does not hand over the answer. So `OutputSet` is a local view, and a device that cannot hold one does not make private payments from that device.

### The rule

Real spends skew recent. Uniform decoy selection therefore fails immediately: where one ring member is far newer than the rest, the newest is the real one with high probability — the attack that forced the wider field off uniform selection. Decoys are drawn with the same recency skew, from `AGE_WEIGHTS`, a frozen table over logarithmic age buckets.

**No floating point anywhere.** A protocol rule computed in `f64` is a rule two platforms can disagree about, and a wallet that samples differently from its peers is a wallet whose users are identifiable — the same harm as (2), arriving through a numerical back door. Every step is integer arithmetic over a fixed table, including rejection sampling rather than modulo, since biased indices are a distribution an observer can fit against.

### What it does not fix

Age-weighted selection **reduces** the statistical attacks on ring anonymity; it does not eliminate them. The weights are a legible starting shape, not a distribution fitted to measured traffic, because no traffic exists to fit. A wallet that deliberately constructs a poor ring still can — but that only harms its own user, which is the correct place for the remaining freedom to sit.

## 11. Auditability without a transparent format (D-0451)

### The trap

The obvious way to keep treasury disbursements checkable is to keep `mini_settlement::PaymentClaim` alongside this path and use it for them. It is a trap, for one reason that generalizes: **if both formats exist, choosing the private one is itself a signal.** Every private payment then carries an implicit "why did this person need privacy?", and privacy that must be opted into is privacy for nobody — the people who most need it are exactly the ones whose opting-in is most visible.

This is §10's argument (2) with the volume turned up. There, a per-wallet decoy policy made each wallet's users a smaller anonymity set than the network's, and that only had to be *inferred* from ring shape. A format choice does not need inferring; it is on the wire for everyone.

### The inversion

Auditability stops being a property of the format and becomes a disclosure a party makes about itself. Nothing is public by default. An account that wants to be auditable publishes its view key (`ViewKeyDisclosure`); anyone can then `audit` its income. The treasury can be answerable to everyone without a single counterparty being exposed who did not choose it.

### `audit` is `scan`

Deliberately the same function with the same return type. Disclosure grants the public the account holder's *reading* ability — no more, no less. A richer, audit-specific result type would have been the first step toward "audited" quietly meaning "more exposed than the owner is", so there is not one.

### The acknowledgement is a type

`ViewKeyDisclosure::create` takes an `AcknowledgedIrreversibleDisclosure`, which is constructible only by writing out a long phrase verbatim — the typed-domains rule applied to an authority that destroys privacy rather than moves money, the same shape as `mini_installer::OwnerApproval`. Publishing a view key is:

- **retroactive** — it decrypts payments received *before* the disclosure; nobody can offer "disclosed from now on";
- **irrevocable** — a key cannot be unpublished, and rotating limits only the future;
- **other people's exposure** — every memo a sender wrote becomes readable, and they were never asked.

The third is why a `bool` was not acceptable. A flag gets set by someone skimming; a phrase that names the third-party harm is harder to paste past without reading.

### What an audit still cannot see

- **Spending.** A view key recognizes incoming payments only. An account that received one payment and spent it is indistinguishable, to an auditor, from one that received it and still holds it.
- **Amounts.** A Pedersen commitment is not opened by a view key. An audit sees which payments arrived and what they were for, never how much they were worth — a real gap between this and what most people hear in "audit".
- **Completeness.** A disclosure covers one account, and no cryptography can prove it is the only one its holder controls.
- **Noise.** Anyone can pay a published address, so a non-empty `ScanOutcome::unreadable` is not evidence the discloser hid anything.

### A defect this found in `scan`

Working out what an audit does when a memo will not open exposed a live availability bug in D-0447's already-merged `scan`, which returned `Result` and propagated a single claim's failure. Since an account's public keys are published, **any** stranger could derive a valid stealth output paying it and seal the memo under a key the recipient cannot derive — a claim that verifies, is recognized, and does not open. One of those made the whole scan return `Err`, erasing every payment the wallet had ever received. Cost to the attacker: one payment. `scan` is now total, and unreadable-but-recognized claims are reported separately rather than dropped or fatal.

## 12. Value conservation, fees, and change (D-0455)

### The hole §2 left

Everything §2 describes hides amounts. **Nothing checked them.** A v1 claim
carried exactly one input, exactly one output, and a range proof over the
output's commitment — and a range proof says only "this is a number in
`[0, 2^64)`". It says nothing about the number the payer actually spent. A
payer could commit to any amount at all, prove it was in range, and the
verifier had no equation to fail.

Hiding a number nobody checks is not privacy. It is minting, with the
privacy as the thing that stops you noticing. Every value invariant in this
tree — M1 above all — was resting on a claim shape that could create money
from nothing.

It also made the path unusable for real payments in a second, more ordinary
way: with one input and one output, you can only pay someone an amount you
happen to hold *exactly*. No change. No fee. That is why D-0451's follow-up
list said wiring `mini-contribution` and `mini-engagement` to this path
would ship "a half-private path that looks finished".

### What closes it

The standard construction, and deliberately nothing more inventive:
Noether and Mackenzie's *Ring Confidential Transactions* — published,
peer-reviewed, and running in production for years. Composition of vetted
prior art, implemented in-house (D-0063), not a new design.

**The balance equation.** Pedersen commitments are additively homomorphic,
so a verifier can sum commitments it cannot open:

```text
Σ pseudo_commitments  −  ( Σ output_commitments  +  Commit(fee, 0) )  ==  Commit(0, 0)
```

The fee enters as a commitment to a publicly known amount under a **zero**
blinding factor, which is exactly what makes it checkable: a verifier
recomputes it from the cleartext `fee_micro` and would reject any other
value.

**Pseudo-output commitments, and why the real one cannot appear.** The
naive version puts each spent output's own commitment in the sum. That
publishes which ring member was real, and the ring stops hiding anyone —
the anonymity would be destroyed by the very check meant to make the
amounts safe. So each input carries a *re-blinded* commitment to the same
value under a fresh blinding factor, and the builder chooses those
factors so the differences cancel across the whole claim.

**The MLSAG.** A one-column ring signature proves only "I control some
member". It does not tie the pseudo-commitment to the member the signer
actually controls, so a signer could re-blind to a *different* value and
balance the claim against money that was never there. `mini_value::mlsag`
is therefore two-column, per ring member `j`:

| column | statement | generator |
|---|---|---|
| 0 | I hold the one-time secret for `ring_keys[j]` | `basepoint()` |
| 1 | I know the discrete log of `ring_commitments[j] − pseudo_commitment` | `blinding_generator()` |

Column 1 verifies as zero **only** for the member whose commitment hides
the same value as the pseudo-commitment — a commitment-to-zero proof.
Both columns share one challenge chain, so one signature proves both
statements about the *same* index without revealing it.

**Column 1 carries no key image, on purpose.** Column 0's key image is the
double-spend nullifier and must exist. A key image on column 1 would be
deterministic in the *blinding difference*, so two spends that happened to
share a blinding difference would link — a linkage that buys nothing, since
double-spend detection already has what it needs from column 0.

**Range proofs run first.** Without them a "negative" output balances the
equation while minting value, so `verify` checks every Bulletproof before
it checks the sum. The order is the security property, not an optimization.

### Change is not a field

A claim may spend up to `MAX_INPUTS` outputs and create up to
`MAX_OUTPUTS`. Change is an output paying yourself, built by the same code
path as any other output, sealed the same way, with the same padded memo
and the same range proof. Nothing on the wire says which output was the
payment.

The alternative — a `change` field, or a distinguishable change output —
would have let an observer read the payment out of every claim by
discarding the change, which is most of what hiding the amount was for.
`a_reader_pays_a_creator_keeps_the_change_and_the_network_takes_a_fee` in
`tests/unity.rs` asserts the two outputs are structurally identical and
that each party reads only their own.

### The commitment opening travels in the memo

To spend an output you need its value *and* its blinding factor. Neither
is on the wire, and there is nobody to ask. So `PaymentNote` — the memo's
plaintext — carries the purpose, the amount, and the blinding factor, all
sealed to the recipient. Receiving a payment and being able to spend it
are then the same event, which is the property that makes the path
transitive: `a_recipient_can_spend_what_they_received` in
`tests/conservation.rs` receives a payment, opens the note, and spends it
onward with no side channel.

The cost is stated rather than hidden: `MAX_MEMO_BYTES` drops from 252 to
212, because the note overhead is real and the padded memo size must not
change (a memo whose length varied would split the anonymity set).

### What this costs in privacy, stated plainly

- **The fee is public.** A verifier must be able to check the fee charged
  is the fee declared, and a hidden fee would need its own range proof and
  still leave the network unable to prioritize. An unusual fee narrows
  which claims could be yours.
- **A claim's shape is public** — how many inputs, how many outputs. Also a
  fingerprint. Both are why a wallet should prefer ordinary shapes.
- **A claim's inputs are linked to each other.** Spending several outputs
  together says they share an owner, without saying who. The alternative,
  one claim per input, leaks through timing instead.

### The format moved, and old claims do not decode

`CLAIM_VERSION` is 2 and the domain is `…/claim/v2`. A v1 claim proved no
balance, so accepting one would accept a payment that could mint value —
there is no compatibility to preserve and none is offered. The golden
vectors moved with it, deliberately to a two-input, two-output fixture: a
one-of-each vector would pin the format for exactly the shape that existed
before conservation did.

### What is still not proven

The MLSAG, the Bulletproofs, and the stealth derivation remain unaudited
prototypes gated behind D-0047/#72. Conservation now *holds* under the
construction; whether the construction is implemented correctly is what an
external audit is for, and nothing here changes that gate.

## 8. Required follow-up

- **Retiring the transparent path** in `mini-contribution`, `mini-engagement`, `mini-bounty`, `mini-execution` and `mini-chain`, which §11 is the prerequisite for and does not itself do.
- An **amount-disclosure** mechanism — opening a commitment to a named auditor — if "auditable" is ever to include sums rather than only the set of payments.
- Binding a disclosure to a `did:mini` root, left to callers in D-0451 to keep an identity dependency out of a value crate.
- Fitting `AGE_WEIGHTS` to real spend-age data once any exists, and revisiting `MIN_RING_SIZE` on the same evidence.
- A chain-backed `PrivateLedgerView`, the private analogue of D-0061.
- Wiring `mini-contribution` and `mini-engagement` to offer the private path. The fee and change model that blocked this now exists (§12); what remains is a chain-backed `PrivateLedgerView`, without which no private payout could reach `Finalized`. **Still deliberately not done here.**
- A **fee policy**: §12 makes fees possible and checkable, and says nothing about what a fee should be or who collects it. That is an economics decision, not a cryptographic one.
- **Output selection and consolidation** — which of a wallet's outputs to spend, and when to consolidate. §12 makes multi-input claims possible; choosing badly is its own fingerprint, and no policy exists yet.
- **External cryptographic audit ([#72](../../issues/72), D-0047)** before anything here gates value. Three prototype constructions compose into one object; the composition needs review as much as the pieces, and a privacy failure does not announce itself.

## 9. Supersedes / superseded by

New ground; supersedes nothing. Extends `mini-value` (D-0036/D-0040) with serialization and shared-secret accessors it lacked, and reuses `mini-settlement`'s M1/M2/M3 vocabulary (D-0045/D-0055) rather than restating it. No `mini-forge` or `mini-chain` voting edge in either direction, so no voice/value wall edge exists (P1, Directive 16).
