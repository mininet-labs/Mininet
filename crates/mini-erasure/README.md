# mini-erasure

Systematic Reed-Solomon erasure coding over `GF(2^8)`, plus a self-healing
repair layer — closes roadmap [#30](../../issues/30) (erasure coding &
replication strategy) and [#32](../../issues/32) (self-healing storage
design).

## Why erasure coding, not just replication

Plain replication (`N` full copies) tolerates `N - 1` losses at `N x`
storage cost. Systematic Reed-Solomon splits a file into `data_shards`
pieces and computes `parity_shards` additional pieces, tolerating up to
`parity_shards` losses at only `(data_shards + parity_shards) /
data_shards x` cost — for typical parameters (e.g. 10 data + 4 parity)
dramatically cheaper than replication for the same loss tolerance. This is
the same reason RAID6, Backblaze, Ceph, and IPFS's own optional erasure
coding all use it instead of naive copies.

## The construction

- `gf256.rs` — arithmetic in `GF(2^8)`: multiplication via the standard
  carry-less multiply-and-reduce against the primitive polynomial
  `x^8+x^4+x^3+x^2+1` (the same field QR codes, PDF417, and RAID6 use);
  inversion by brute-force search over the 255 nonzero elements (this
  crate only ever inverts small matrices, never a per-byte hot path, so
  simplicity wins over log/antilog tables).
- `matrix.rs` — dense `GF(2^8)` matrices, Gauss-Jordan inversion, and the
  systematic Vandermonde generator matrix: a `(k+m) x k` matrix whose top
  `k` rows are the identity (so the first `k` output shards are the
  original data, unencoded) and whose bottom `m` rows are Vandermonde
  coefficients. Every `k`-row subset of a Vandermonde matrix is invertible
  (the maximum-distance-separable property), which is exactly what lets
  reconstruction use *any* `k` of the `k+m` shards, not just the first
  `k`.
- `code.rs` — `encode()`/`reconstruct()`: split data into shards, multiply
  by the generator matrix, and invert the submatrix of whichever `k`
  shards survive to recover the original.
- `health.rs` — self-healing: `plan_repair()` reports which shard indices
  can't currently be trusted (missing, or present but failing a BLAKE3
  integrity check — corruption is treated the same as absence, never
  silently trusted), and `repair()` reconstructs the original data and
  regenerates exactly the missing shards, ready for a caller to
  redistribute to fresh holders.

Coded in-house rather than depended on as a library, for the same reason
D-0063 gives for `mini-porep`'s cryptography: composing an already-
published, real-world-deployed construction ourselves keeps it inside this
repo's own governance boundary. Erasure coding is coding theory, not
cryptography, so CLAUDE.md's crypto-invention rule doesn't technically
apply — but the same Directive-14 "prefer the well-trodden construction"
reasoning does, and is followed the same way.

## Scope boundary

This crate proves the erasure-coding and repair *logic* is correct — 27
tests including an end-to-end two-outage healing cycle, exhaustive
reconstruction from every valid shard subset for small `(k, m)`, corrupted
shards being caught and healed identically to missing ones, and
losing more than `parity_shards` holders being reported unreconstructable
rather than silently wrong. Deciding which peer should hold a regenerated
shard, and actually transferring it, is `mini-net`/`mini-store`'s job — a
distribution problem, not a coding-theory one — and is not attempted here.

```sh
cargo test -p mini-erasure
```

License: CC0-1.0 (public domain).
