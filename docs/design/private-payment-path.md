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

1. **How are decoys chosen?** The anonymity of every payment rests on this, and it is entirely the caller's problem today. The literature is clear that naive decoy selection is breakable; a real answer needs a sampling rule over the output set, and probably a policy the protocol states rather than each wallet inventing.
2. **Is `MIN_RING_SIZE = 8` defensible?** It is a legible floor, not a figure derived from a deanonymization analysis of real traffic — that analysis has not been done and cannot be until traffic exists.
3. **Should the transparent path remain available at all?** If both exist, the choice to use the private one is itself a signal, and a payment that *must* be transparent (an audited treasury disbursement, say) coexists awkwardly with one that must not be. This is a protocol-policy question, not an implementation detail.
4. **What is the fee model?** A private payment with no fee output cannot pay for its own inclusion, and a transparent fee attached to a shielded payment reintroduces a linkable value.
5. **Where does the anonymity set come from on a weak device?** Directive 11: a phone that must download the whole output set to pick decoys is a phone that cannot make private payments at all.

## 8. Required follow-up

- Multi-output claims with `verify_balance`, so change and fees can exist without leaking.
- A chain-backed `PrivateLedgerView`, the private analogue of D-0061.
- A decoy-selection policy answering §7.1, with the analysis §7.2 needs behind it.
- Wiring `mini-contribution` and `mini-engagement` to offer the private path, once the above exist. **Deliberately not done here** — those crates' payouts are one of the strongest arguments for this work, and changing them before the fee and change model exist would ship a half-private path that looks finished.
- **External cryptographic audit ([#72](../../issues/72), D-0047)** before anything here gates value. Three prototype constructions compose into one object; the composition needs review as much as the pieces, and a privacy failure does not announce itself.

## 9. Supersedes / superseded by

New ground; supersedes nothing. Extends `mini-value` (D-0036/D-0040) with serialization and shared-secret accessors it lacked, and reuses `mini-settlement`'s M1/M2/M3 vocabulary (D-0045/D-0055) rather than restating it. No `mini-forge` or `mini-chain` voting edge in either direction, so no voice/value wall edge exists (P1, Directive 16).
