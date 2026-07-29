# Distributed build worker — bounded Batch 5 slice

**Maturity:** implemented proposal; locally tested; not externally audited

**Decision:** D-0409 (proposed)
**Tracking:** issue #102

## Problem

Mininet already has a deny-by-default Wasmtime runner and provenance objects,
but builds still execute on the requester's machine. Development cannot remain
independent of hosted CI until a volunteer machine can receive one exact build,
execute it without ambient authority, and return independently verifiable
artifacts.

## Design

The requester canonicalizes a regular-file workspace snapshot and sends the
exact `ExecutionRequest`, digest-matching component bytes, and bounded relative
workspace files. The message travels inside the existing encrypted
`mini-bearer::Channel`.

The worker rejects traversal, absolute or duplicate paths, symlinks,
non-files, malformed lengths, digest mismatches, and requests above policy. It
also rejects network-host and secret-read capabilities: a volunteer worker
cannot be asked to expose ambient network access or local secrets. It then
invokes the existing `mini-build-runner-wasmtime` binary as a subprocess.

The response carries the runner's `ExecutionResult` and artifact bytes sorted
by digest. Before creating an output directory, the requester verifies the
request digest, Wasmtime-isolated label, exact capabilities, output count, and
every artifact digest.

## Limits and threat boundary

The worker remains untrusted. Digest verification proves byte/result binding;
it does not prove honest hardware or truthful runner/isolation claims.
Independent builder agreement and signed `mini-provenance` records remain the
release gate. This slice grants no review, merge, release, payment, governance,
or owner-adoption authority.

Current messages use one bearer frame: 8 MiB component, 6 MiB workspace,
14 MiB artifacts, and 15 MiB total request/response bounds. There is no
chunking, resumption, discovery, endpoint identity, scheduling, queue,
concurrency, retry, reputation, payment, daemon, or process supervision.

## Alternatives considered

- Whole-store sync obscures exact job intent and exposes unrelated content.
- An HTTP job service adds another transport/authentication stack.
- Linking Wasmtime into the CLI violates D-0069's isolation boundary.
- Worker signatures identify a claimant but do not prove reproducibility.

## Next production slices

1. Chunked content-addressed transfer with interruption-safe resume.
2. Signed worker capability advertisements and explicit requester trust.
3. Bounded queues, concurrency, cancellation, retry, and supervision.
4. Multiple independent results recorded through `mini-provenance`.
5. Client-side concentration visibility without a canonical worker registry.
6. External security review before release infrastructure depends on it.
