# mini-treasury

Community-governed BTC/XMR-to-MINI contribution bookkeeping and threshold-
signature custody (whitepaper §8.2 "how the rich contribute" / §10 treasury
custody).

## Safe to build now (ordinary bookkeeping and arithmetic)

- `rate` — a governed exchange-rate history and the multiplication that
  turns a contribution into a minted amount at whatever rate was in effect.
- `receipt::ContributionReceipt` — the bookkeeping record of a claimed
  contribution (asset, amount, rate, minted MINI).
- `signers` — **who** is authorized to approve treasury actions and whether
  enough of them agreed (`TreasurySignerSet`, `meets_threshold`), mirroring
  `mini-forge`'s governance approval-counting pattern: distinct-identity
  counting only, no weight field, no path to extra voting power for being a
  signer (P1 unchanged). This is identity-level authorization ("is this
  person on the committee"), a separate question from `frost_sign`'s
  cryptographic signing ("here is a valid signature the committee actually
  produced").

## Founder-overridden (D-0037), AI-authored prototype: FROST threshold custody

- `frost_keygen` — trusted-dealer Feldman VSS keygen: splits one group
  secret key into `n` shares, any `threshold` of which can later sign,
  with each share individually verifiable against the dealer's published
  commitments (`s_i*G == sum_k A_k * i^k`).
- `frost_sign` — FROST (Flexible Round-Optimized Schnorr Threshold
  signatures, Komlo & Goldberg): two rounds — nonce commitment, then a
  binding-factor-weighted response — produce one ordinary Schnorr
  signature under the group public key. No participant, and no single
  point in the protocol, ever reconstructs the group secret key. Per-share
  verification (`verify_signature_share`) catches a bad or malicious
  signer's contribution *before* aggregation, with attribution, instead of
  only learning the final aggregate doesn't verify.

Both load-bearing algebraic identities (individual-share verification,
`z_i*G == R_i + c*lambda_i*Y_i`; and aggregate validity,
`z*G == R + c*Y`, via Shamir reconstruction-in-the-exponent) were
hand-derived and checked term-by-term before implementation — documented
in `frost_sign`'s module docs, same discipline `mini_value::bp_range` used
for Bulletproofs.

### Real distributed key generation and resharing (D-0059/D-0060, closes D-0048's DKG gap)

- `frost_dkg` — Pedersen DKG (RFC 9591 §4): every participant runs an
  independent Feldman VSS of their own random value; the group secret is
  the *sum* of every non-excluded participant's contribution. No dealer,
  because there is no single party who ever holds the whole secret, not
  even briefly. A misbehaving participant is handled via a self-verifying
  complaint/rebuttal mechanism (`DkgComplaint`/`DkgRebuttal`/
  `dkg_resolve`) — the honest majority can exclude them and finish key
  generation without any new consensus/voting machinery, and a false
  accuser cannot frame an honest participant who correctly rebuts. A
  Schnorr proof of knowledge on every round-1 package prevents rogue-key
  attacks (a participant choosing their commitment as a function of
  others' to bias the group key).
- `frost_reshare` — committee rotation: an active old-committee subset
  redistributes the *same* group secret to a new committee via
  Lagrange-weighted sub-sharing, reusing `frost_dkg`'s commitment/
  complaint machinery unchanged. `reshare_finalize` independently
  recomputes and checks the resulting group public key against the old
  one rather than only trusting the algebra.

Both DKG and resharing produce ordinary `KeyPackage`/`PublicKeyPackage`
values — identical to what `trusted_dealer_keygen` produces — so
`frost_sign` needs no DKG-specific code path at all; a full round-trip
(DKG or resharing, then an ordinary FROST signature) is directly tested.

### Live multi-device signing demo

```sh
cargo run -p mini-treasury --example frost_live_demo
```

Five separate OS threads, each holding only its own key share (never
shared with any other thread), talk to a coordinator exclusively through
`std::sync::mpsc` channels — the same request/response shape a real
network transport would carry. The demo runs two sessions live: a 3-of-5
payout signing with two devices offline, and an adversarial session where
one device's reported share is tampered with in transit — the coordinator
catches and attributes it before any signature is produced, rather than
emitting a bad aggregate. See the example's own doc comment for exactly
what "live" does and doesn't mean here (real threads and real channels and
real cryptography; not separate physical hardware, not a real network
transport, not DKG keygen).

### Honest limits — read before trusting this with anything real

- **`trusted_dealer_keygen` still briefly holds the whole secret while
  splitting it** — kept for tests/demos, not for production. `frost_dkg`
  is the production path now, but see the next point.
- **DKG and resharing are real, tested, and architecturally sound — and
  still unaudited.** Every call site must explicitly pass an
  `AcknowledgedPrototypeOnly` or `AcknowledgedUnauditedDkg` (issue #93) so
  none of these paths is reachable by accident — the type system, not
  just a comment, marks them prototype-only pending
  `docs/gates/dkg-audit-scope.md`'s external review.
- **Resharing does not revoke the old committee's shares.** Old
  participants still physically hold working key material after a
  reshare; only actually deleting it completes a real rotation. This
  module has no way to enforce or verify that deletion — see
  `frost_reshare`'s own module docs.
- **Nonces (signing) and coefficients (DKG/reshare) are zeroized on drop,
  but this is still a prototype.** `SigningNonces` and `DkgRound1Secret`
  scrub their secret scalars when dropped and redact them from `Debug`
  output (issue #93) — real hardening, not just documentation — but
  neither has been reviewed for compiler-reordering/copy risk the way an
  externally audited implementation would be.
- **No network, no transport, no session/replay layer.** Round-1/round-2
  data (signing nonces, commitments, shares, DKG packages) are plain
  values the caller must transport over its own confidential,
  authenticated channel — the demo's channels stand in for what
  `mini-net`/`mini-bearer` would carry in a deployed system; that wiring
  does not exist yet.

## Deliberately not built here

Whitepaper §11: "bridge and treasury custody is a permanent honeypot by
nature." D-0035 point 5's external-audit requirement stands even under
D-0037's authorship-policy change for this specific gap:

- `receipt::ExternalReceiptOracle` — verifying a Bitcoin or Monero
  transaction actually paid the treasury is real cross-chain engineering
  (confirmation depth, reorg safety, Monero's view-key/output-scanning
  machinery). `NoExternalReceiptOracle` is the correct, permanent stand-in.
  This is a separate integration surface entirely, not something FROST or
  any signing scheme closes.

**[FREEZE reminder — D-0037]** Every primitive above — signing, DKG, and
resharing alike — is founder-reviewed, not externally audited. Nothing in
this crate should be read as "custody solved" for real funds until that
audit happens (`docs/gates/dkg-audit-scope.md` for the DKG-specific scope).

This crate is bookkeeping, governance-membership data, and a threshold-
signature prototype — not a deployable treasury.

## Build & test

```sh
cargo test -p mini-treasury
```

License: CC0-1.0 (public domain).
