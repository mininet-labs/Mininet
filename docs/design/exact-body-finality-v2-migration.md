# Exact-body finality v2 and prelaunch migration

**Status:** implemented proposal; coordinated prelaunch hard fork; not a
release, canonicalization, or owner-adoption record

**Decision:** proposed D-0443

## Problem

The former block header committed to the post-execution `state_root`, but not
to the exact ordered `SettlementBlockBody`. That is insufficient as a history
commitment. Two different bodies can produce the same state: for example,
execution deliberately drops a claim whose signature is invalid. A quorum
certificate over only the old header could therefore prove a resulting state
without proving which rejected or ignored bytes were actually finalized.

The proposal-signature hardening in D-0442 authenticated the body while a live
proposal was in flight. It did not carry that proof into the durable header,
quorum certificate, snapshot, or catch-up archive. Restarted and late-joining
nodes therefore could not independently reconstruct the exact finalized
history from the QC alone.

The same review found a second consensus-boundary defect: the consensus body
codec serialized payment claims but omitted `monetary_epochs`. A monetary
transition could be present in a locally proposed body yet disappear when the
proposal crossed the wire. In addition, the scalable epoch commitment omitted
the nested Human Share epoch and vesting duration. Both omissions are fixed as
part of the same incompatible format change so there is one coordinated
prelaunch transition, not several ambiguous partial migrations.

## Protocol design

`BlockHeader` version 2 contains two independent commitments:

- `state_root`: the deterministic post-execution ledger state; and
- `body_root`: BLAKE3 over the exact ordered body bytes and all monetary epoch
  fields.

Votes and quorum certificates already commit to `BlockHeader::hash()`. Because
the v2 hash includes `body_root`, a valid QC now proves both the resulting state
and the exact historical body. `LedgerChain::apply_finalized_block` checks the
body count limits and `body_root` before performing quorum-signature work, then
recomputes and checks `state_root` before mutating live state. Proposal
verification and node voting perform the same body check. Persistent finalized
blocks and compatibility catch-up decoding reject a header/body mismatch.

### Version map

| Object | Old | New | Reason |
|---|---:|---:|---|
| Block-header hash domain / explicit version | v1 / none | v2 / `2` | Add `body_root` to every vote/QC target |
| Settlement-body hash | v1 | v2 | Commit exact monetary-plan bytes, not only prior digests |
| Consensus message / proposal transcript | v2 | v3 | Carry the larger header and monetary epochs; bind the new value |
| Finalized block archive | v1 | v2 | Persist the v2 header and exact body |
| Compatibility catch-up | v1 | v2 | Prevent mixed historical responses |
| Consensus snapshot | v1 | v2 | Bind the checkpoint QC to a v2 header |
| State-sync request/response | v1 | v2 | Prevent old peers from silently mixing formats |
| Archive install journal | v1 | v2 | Prevent replay of an old interrupted install |
| Scalable epoch commitment | v1 | v2 | Include nested Human Share epoch and vesting fields |

The standalone scalable-epoch wire codec starts at v1 because it did not exist
before this migration. It is domain separated, canonical, length-prefixed, and
bounded to one epoch per block, 1,024 optional grants, 1,024 UTF-8 bytes per
beneficiary, and 2 MiB per encoded plan. Decoding is structural only;
`MonetaryLedger::apply_epoch` still re-derives policy caps, supply binding,
totals, grant validity, and sequential epoch authority before issuance.

No compatibility decoder guesses missing fields. Every old outer domain is
rejected explicitly. This fail-closed boundary is necessary because a v1
header has no cryptographic value from which an authentic `body_root` can be
reconstructed.

## Coordinated prelaunch migration

This procedure assumes Mininet has not established a production canonical
ledger. It is a hard fork and must be scheduled as one validator-set event.

1. Select and publish the exact reviewed release commit, network identifier,
   genesis object, validator set, activation time, and rollback rule through
   the applicable human/governance process. This document does not select any
   of them.
2. Stop proposal production on every validator. Do not let mixed binaries vote
   at the same network identifier.
3. Back up the old archive and wallet export as read-only evidence. Label it
   **pre-v2, non-canonical for v2**. Never rewrite files in place.
4. Install the same reviewed binary on all validators and independently verify
   its release digest.
5. Move the old consensus archive out of the configured live archive path.
   The software will reject its domains, but quarantine makes operator intent
   explicit and preserves recovery evidence.
6. Start every validator from the agreed v2 genesis. If preserving beta-era
   balances is desired, the applicable governance process must create and
   externally audit a new explicit genesis allocation; software must not infer
   balances from uncommitted v1 body history.
7. Before opening public submission, verify on at least a quorum plus one
   independent observer that the network id, genesis hash, validator-set
   digest, first v2 header hash, `body_root`, state root, and archive reopen
   result are identical.
8. Exercise a real state-sync from a fresh node and confirm it reaches the same
   height, tip hash, and state commitment.
9. Retain the old backup under the project's evidence-retention policy. It is
   not a fallback chain and must never be served to v2 peers.

After any v2 block finalizes, rollback to a v1 binary under the same network id
is forbidden: the old binary cannot validate the new header or exact history.
A rollback requires a separately governed network/genesis decision, not a
filesystem restore.

## Failure handling

- An old message, archive row, snapshot, state-sync response, or install
  journal fails as malformed; there is no permissive fallback.
- A supplied body whose hash differs from `header.body_root` is rejected before
  quorum signature verification or state mutation.
- A monetary plan that exceeds any wire/resource bound is rejected before
  execution. A well-framed but economically forged plan is rejected when
  execution re-derives the issuance transition.
- An interrupted v2 snapshot install remains recoverable through the v2
  journal. An interrupted v1 journal is quarantined with the old archive.
- If validators disagree on genesis, first v2 header, or state commitment,
  public operation remains stopped. A majority result is evidence of a split,
  not permission to guess which history is canonical.

## Security and authority boundaries

This closes exact-body ambiguity for v2 finalized blocks and fixes loss of
monetary epochs in the consensus codec. It does not prove Sybil resistance,
choose validators, solve long-range attacks or dynamic validator transitions,
make issuance policy economically sound, provide transaction privacy, or
authorize production value. Static-set/weak-subjectivity limits and A1's
independent consensus/cryptography/issuance audit gate remain.

State snapshots are still one bounded bearer frame and rejection history is
still monotonic. Chunked authenticated state transfer and bounded historical
rejection proofs remain separate P1 work because they change storage/proof and
resumption semantics; this migration must not disguise those gaps.

## Required evidence before activation

- exact-head Linux workspace tests and Clippy;
- two independent maintainer reviews under D-0033;
- independent consensus, issuance, and cryptographic review under A1;
- a multi-process mixed-version drill proving v1 inputs fail closed;
- a fresh-node v2 state-sync/reopen drill on the intended deployment platform;
- an operator runbook naming backup, quarantine, genesis, digest verification,
  abort, and separately governed rollback procedures; and
- no claim that this proposal itself approved, merged, canonicalized, released,
  or adopted any state.
