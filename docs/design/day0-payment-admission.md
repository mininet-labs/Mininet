# Day-0 bounded payment admission

**Status:** proposed implementation
**Decision:** proposed D-0416
**Roadmap:** next payment portion of #61; follows merged PR #273

## Outcome

This slice creates the bounded wallet-to-proposer seam missing after D-0415:

1. a standalone canonical `PaymentClaim` wire frame;
2. strict decode bounds before allocation;
3. a node-local admission pool checked against finalized ledger state;
4. aggregate pending-balance reservation;
5. deterministic candidate ordering; and
6. revalidation after finality or local expiry.

Admission is not consensus. It does not move, reserve, lock, or finalize MINI.
A proposer may omit any admitted claim, and every selected claim must still
pass D-0415 execution in a quorum-finalized block.

## Standalone claim wire

`PaymentClaim::to_wire_bytes` and `from_wire_bytes` use
`mini-settlement/payment-claim-wire/v1`, followed by the signed network ID,
bounded length-prefixed payer/payee/head-hint fields, fixed-width amount,
sequence and validity values, signature-suite tag, and exact signature bytes.

Each opaque field is limited to 4,096 bytes and a whole claim to 16 KiB.
Decoding rejects wrong domain, unknown suite, truncation at every position,
trailing bytes, length overflow, and oversized input. Decoding is structural;
admission separately verifies the signature and policy.

This is intentionally standalone. A wallet need not construct a validator
proposal or consensus envelope merely to submit one signed promise.

## Admission policy

The default local policy allows at most:

- 4,096 pending claims;
- 8 MiB of conservatively estimated encoded claim memory; and
- 64 claims from one payer.

Custom policies must remain nonzero, keep the per-payer limit within the
global limit, and never exceed the consensus block claim cap.

Admission rejects:

- malformed wire or invalid signature;
- another settlement network;
- unsupported payee account;
- locally expired claim;
- exact duplicate or same-payer/same-sequence conflict;
- already finalized or canonically rejected claim;
- aggregate locally pending spend above finalized balance; and
- any global, per-payer, or byte limit breach.

The aggregate balance check is local DoS/user-feedback protection only. It is
not an on-chain reservation. Another finalized payment may change the payer's
balance before proposal.

## Determinism and revalidation

Candidate claims are sorted by `(payer bytes, sequence, claim digest)`, not
arrival time. Two pools containing the same set therefore produce the same
candidate order. This removes accidental host-map and arrival-order
nondeterminism without claiming fairness: proposers still have censorship
power until a wider inclusion design exists.

After canonical state advances, `revalidate` deterministically removes exact
canonical outcomes, stale sequences, local expiry, wrong-network claims, and
claims no longer covered by the payer's current balance. Removed claims return
an explicit reason suitable for logs and wallet feedback.

## Security boundary

The pool holds public payment metadata in memory. It provides no anonymity,
encrypted routing, persistence, peer authentication, fee market, spam-cost
mechanism, guaranteed inclusion, or censorship resistance. Operators must not
expose it as an unauthenticated internet service without a separately bounded
transport/rate limiter.

The local clock affects only admission and wallet expiry labels. Canonical
execution deliberately remains independent of an untrusted device clock; a
claim that later finalizes reports final even if a local validity window has
elapsed.

## Remaining work

- authenticated submission transport and re-gossip;
- peer/rate limits and Sybil-resistant resource pricing;
- proposer inclusion/censorship accountability;
- governed fee accounting;
- durable crash-safe pool persistence, if desired;
- compact state and rejection proofs;
- bounded canonical rejection-history pruning;
- private transactions and metadata-resistant routing; and
- marketplace order, escrow, refund, and dispute objects.
