# Native Open Beta proof status

This file records what PR #342 actually proves so later documentation does not silently promote a focused integration test into Forge canonicality.

## Proven in the branch

`crates/mini-cli/tests/beta_native_flow.rs` exercises two independent Mininet homes/stores with no GitHub API or account in the test path.

The test proves:

- Beta campaign discovery through the native `mini beta campaign` surface;
- finding submission refuses to proceed without an explicit privacy-redaction acknowledgement;
- two participant findings created by one local user use different artifact-scoped author DIDs and neither author is the user's persistent local DID;
- the artifact-scoped KEL carriers and findings cross the ordinary `mini sync` verified-ingest boundary to an independently opened peer store;
- the receiving peer can list the same immutable finding ids through the native Beta CLI;
- contribution acceptance stores only the public domain-separated claim commitment in the signed receipt; and
- the private claim preimage printed for handoff is absent from the public contribution object's bytes.

The focused proof was run under format, Clippy `-D warnings`, and the exact integration test before the temporary proof workflow removed itself.

## Not proven by that test

This is **not** evidence that Forge is canonical or that every final-phase workflow is complete. In particular:

- campaign/disposition/task/contribution acceptance is still bootstrap operational authority;
- network timing and transport metadata can still correlate anonymous artifacts;
- accepted-contribution reward execution remains #337/#339 work;
- the complete GitHub-outage contribution/review/release loop and one-way bootstrap shutdown remain #338;
- personhood is not solved; and
- an artifact-scoped `did:mini` is not evidence of a unique human.

The branch remains draft until the repository's normal exact-head CI/reproducibility/governance workflows also pass. A focused self-test is not self-authorizing release evidence.
