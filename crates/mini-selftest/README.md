# mini-selftest

Diagnostics that run the real Mininet stack and report what happened.

A protocol whose guarantees can only be confirmed by reading its test suite is
a protocol most of its users cannot confirm at all. This crate exists so a
person holding the client can press a button, watch identity, storage, social
objects, chunked media, encrypted messaging, peer sync over a real socket,
governed review, erasure coding, storage proofs, and the Windows install path
actually execute, and read the result.

Two front ends, one suite:

```sh
mini selftest          # every check
mini selftest forge    # one area
mini selftest list     # what would run, without running it
```

and the client's **Diagnostics** view, which runs the same checks off the UI
thread.

## Refusal checks carry most of the weight

Roughly a third of the checks establish that something is **refused**:

- one approval does not reach the two-approval protocol floor;
- an approval bound to one commit does not carry to a substituted one;
- a second conversation's key reads none of the first one's messages;
- a signature over different bytes does not verify;
- losing one shard more than parity fails instead of returning wrong bytes;
- a storage proof does not verify for a block it was not made from;
- a package with one flipped byte is refused rather than installed.

A suite of happy paths tells you the code *can* succeed. It is the refusals
that tell you the guarantees are load-bearing, and those are exactly the
properties a user is being asked to trust.

## Safety

Every check builds its own throwaway state under a scratch directory and the
runner deletes it afterwards, so a diagnostics run cannot see or damage
identities, objects, or settings. Two checks read an existing Windows
installation; they only read it. A check that cannot run here reports
`Skipped` with a reason — never a pass.

## The wall

This crate has a governance edge (`mini-forge`), so under P1 / Directive 16 it
must never gain an edge to `mini-value`, `mini-bounty`, or `mini-treasury`.
There is deliberately no shielded-payment or bounty check here; adding one
would be a voice/value wall violation, not a missing feature. It must also
never be depended upon by a protocol crate — only by front ends — or its
breadth would leak into the graph it is meant to observe from outside.
