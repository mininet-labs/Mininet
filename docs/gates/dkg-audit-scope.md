# FROST distributed key generation — audit scope package

Gates [roadmap #93](https://github.com/britak420/Mininet/issues/93), **P0**
per D-0048. **Founder action required: same reviewer pool as
`crypto-audit-scope.md`, but treat this as a separate audit scope** — a
DKG bug is categorically worse than a signing bug: it can permanently
expose treasury/bridge keys the moment key generation happens, not just
compromise one transaction.

## Why this is separate from ordinary signing review

`mini-treasury`'s existing FROST signing (D-0041) uses **trusted-dealer
keygen** — one party briefly holds the whole secret before splitting it.
That's acceptable for the live demo; it is not acceptable for real
custody, because that one party is a single point of catastrophic failure
for however long it holds the secret, however careful the code around it
is. Real DKG means no party — not even briefly, not even in memory — ever
holds the full key.

## Scope

Implementing the DKG protocol itself (Pedersen DKG or equivalent) is
ordinary engineering work this repository can do; **trusting an
in-house DKG implementation without external review is not acceptable**
for anything backing real value, which is why this is a named gate rather
than just another roadmap issue.

## The Rust shape the implementation should take (for the auditor's context)

```rust
pub enum KeygenMode {
    TestOnlyTrustedDealer,
    PedersenDkg,
    ReshareFromPreviousEpoch,
}

pub struct DkgTranscript {
    pub session_id: SessionId,
    pub participants: Vec<SignerId>,
    pub commitments: Vec<Commitment>,
    pub complaints: Vec<Complaint>,
    pub final_group_key: GroupPublicKey,
}
```

with a hard, code-enforced rule that a mainnet/real-value build cannot
even compile a path through `TestOnlyTrustedDealer`:

```rust
if network == Network::Mainnet && mode == KeygenMode::TestOnlyTrustedDealer {
    return Err(CustodyError::TrustedDealerForbidden);
}
```

## The questions an auditor must answer

- Does the commitment scheme actually bind each participant to their
  share before secrets are revealed (no participant can choose their
  share *after* seeing others')?
- Is the complaint/dispute mechanism sound — can a malicious participant
  falsely accuse an honest one, or can a genuinely malicious share evade
  detection?
- What happens under each of these adversarial conditions, and is the
  behavior tested for every one:
  - a rogue-key participant (adaptively choosing a key to cancel out others')
  - a missing/non-responsive share
  - a malformed commitment
  - an equivocating participant (different values to different peers)
  - an aborting participant mid-ceremony
  - a resharing ceremony with a removed signer
  - an old transcript replayed into a new session
  - a transcript claiming the wrong threshold
  - nonce reuse across sessions
- Does resharing (`ReshareFromPreviousEpoch`) actually rotate the key
  material, or does it leave the old shares still capable of
  reconstructing the group key?

## Hard constraints on the review

Same as `crypto-audit-scope.md`: no admin key, no backdoor, no bypass of
the `TrustedDealerForbidden` guard for anything the founder intends to
call "production." If the auditor's only path to a finding's fix is
weakening that guard, the finding stands and the guard stays.

## Recommended architecture note for whoever builds this

Per the founder's own framing (and consistent with this repo's typed-
domain hard rule in `CLAUDE.md`): treasury, bridge, and any future
emergency-pause signing should share **one** custody engine
(`mini-custody`, not yet built) rather than each growing its own signer
stack — one sealed authority boundary, not several ad hoc ones that could
drift apart under audit.

## What closes this gate

A written audit report on the DKG implementation specifically (separate
document from the general crypto audit, even if the same auditor does
both), triaged findings, and — before any real-value custody ceremony
runs — a signed-off `PedersenDkg` (or equivalent) implementation with
`TestOnlyTrustedDealer` provably unreachable in the production build.
