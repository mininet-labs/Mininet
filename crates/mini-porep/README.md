# mini-porep

Real proof-of-replication (PoRep): Filecoin-style Stacked Depth-Robust Graph
(SDR) sealing, coded here in-house from the published, peer-reviewed,
real-world-deployed construction rather than depended on as a library
(D-0063) — closes roadmap [#31](../../issues/31).

## The gap this closes

`mini-spacetime::storage_proof` proves *possession*: answering a Merkle/PDP
challenge requires genuinely holding the claimed bytes. What that scheme's
own docs name explicitly as an open gap is **replication uniqueness** — it
cannot distinguish a thousand honest small devices, each holding their own
copy, from one well-resourced warehouse machine holding a single copy and
answering every challenge on behalf of many claimed identities. That is
exactly the attack the whitepaper's "a thousand cheap, scattered machines
outcompete one warehouse" thesis depends on resisting.

This crate closes the gap by making the thing being proven *expensive and
sequential to produce*: `seal()` transforms data into a replica through work
that provably cannot be shortcut, so producing `k` replicas costs
approximately `k` times the sealing work — there is no way for one machine
to cheaply fake holding many independent sealed copies.

## The construction

1. **DRG parent selection** (`drg.rs`): each node in a layer has up to 6
   parents drawn from strictly earlier same-layer nodes — the sequential
   predecessor (forcing full in-layer sequentiality) plus ~5 pseudorandom
   long-range back-edges (defeating "store a thin spine, recompute the
   rest" shortcuts). Deterministic given `(replica_id, layer_index,
   node_index)` — sealer and verifier always agree on the graph with no
   shared randomness.
2. **Stacked layered labeling** (`seal.rs`): layer 0 seeds from the raw
   data (`label(0, i) = H(replica_id, 0, i, D_i)`); each layer `L >= 1`
   hashes that layer's DRG parent labels together with the *previous*
   layer's label at the same index (`label(L, i) = H(replica_id, L, i,
   [parent labels], label(L-1, i))`) — the cross-layer identity edge that
   gives stacking its depth. Shortcutting layer `L` requires already having
   computed all of layer `L-1`, transitively down to layer 0.
3. **Final encoding**: `R_i = label(num_layers, i) XOR D_i` — the replica
   is the same size as the original data, but unrecoverable without the
   full sequential labeling work.
4. **Registration-time probabilistic audit** (`audit.rs`): the honest
   substitute for a zk-SNARK sealing circuit (building one from scratch was
   judged far too large and too risky to get right in this pass). A
   verifier samples random `(layer, node)` challenges; the prover reveals
   each challenged label plus everything its formula depends on (DRG parent
   labels, the previous layer's label, the original data node), each with a
   Merkle proof against a root published *before* the challenge; the
   verifier recomputes the hash directly. Real "spot-check a random
   subgraph" methodology, not zero-knowledge — it reveals plaintext
   intermediate labels for challenged indices, an accepted tradeoff since
   sealing isn't trying to keep data confidential, only to prove genuine
   sequential work was performed once.
5. **Ongoing challenge-response** (`challenge.rs`): reuses
   `mini_spacetime`'s existing PDP Merkle challenge machinery directly
   against the sealed replica's own root, rather than duplicating it — the
   same storage-risk problem `mini_spacetime::storage_proof` already
   solves, now applied to a replica whose *origin* has been separately
   proven. `PorepStorageProof` implements
   `mini_spacetime::ProofOfSpaceTimeSource`, so
   `mini_spacetime::proposer_weight` needs zero changes to consume proof
   sourced from real replication instead of mere possession.

## Honest limits

- **Simplified DRG, not Filecoin's exact `BucketGraph`.** Structurally
  similar (sequential + long-range edges), not parameter-identical —
  reproducing Filecoin's specific probability-weighted bucket-sampling
  distribution from memory was judged too much precision risk to get right
  from scratch.
- **Probabilistic audit, not a succinct proof.** Sampling enough challenges
  makes skipping a meaningful fraction of the sealing work exponentially
  unlikely to go undetected, but unlike a SNARK this is not one small
  universally-checkable proof, and it reveals plaintext intermediate labels
  for every challenged index.
- **Unaudited.** Real, tested, founder-reviewed AI-authored cryptography
  prototype code — not audit-equivalent. Gated behind D-0047 before any
  real value depends on it, same posture as every other `mini-value`/
  `mini-treasury` prototype in this tree.
- No GPU/hardware acceleration attempted — sealing large data through many
  stacked layers is real CPU work by design; that's the point, not a bug.

```sh
cargo test -p mini-porep
```

License: CC0-1.0 (public domain).
