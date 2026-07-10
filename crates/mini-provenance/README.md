# mini-provenance

SLSA/in-toto-style build provenance as real, signed, content-addressed
objects — self-hosted forge spine Batch 2a (D-0068,
[`docs/design/self-hosted-forge-spine.md`](../../docs/design/self-hosted-forge-spine.md)).

## What this closes

The founder-adopted external audit named a specific, real gap: this
repository's CI runs a same-runner clean-rebuild comparison and its own
workflow already says so honestly — but nothing turned "builder X got
digest D" into a queryable, signed, independently-countable claim the way
`mini_forge::release`'s artifact attestations already are for a *cut*
release. This crate generalizes that exact pattern to the *build* stage,
before a release is even proposed:

- `record_provenance()` signs a builder's environment digest, the exact
  commands/pipeline recipe run (as a digest, not raw logs), every output
  digest produced, whether the build had network access, and a
  self-declared reproducibility group, tied to a subject (a commit or
  artifact `ObjectId`).
- `list_provenance()` reads back every author-verified claim against a
  subject.
- `independent_agreement()` counts how many **distinct identity roots** —
  excluding the subject's own author, the exact exclusion the audit asked
  for ("do not count... the release author's own build") — agree on a
  given output digest.

## Honest limit

Code can verify *distinct identity roots* agree on a digest. It cannot
verify *administratively independent infrastructure* — three containers on
one host, signed by three keys the same person controls, are
indistinguishable from three real builders to anything in this crate.
That's a policy/process fact about who controls which signing key, not a
code gap — the same caveat `mini_forge::release`'s own docs already carry
for release attestations, unchanged here.

## What this does not do

Nothing here *runs* a build. Sandboxed execution (WASI/Wasmtime) is Batch
2b — a separate, deliberately deferred decision given the size of the
Wasmtime dependency versus this workspace's consistent minimal-dependency
pattern. This crate only makes the *result* of a build (wherever and
however it ran) into a real, verifiable, independently-countable claim.

```sh
cargo test -p mini-provenance
```

License: CC0-1.0 (public domain).
