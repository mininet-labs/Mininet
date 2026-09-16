# Forge-native Open Beta workflow

PR #334 makes Open Beta a first-class Forge workload rather than a GitHub-only
process. GitHub may remain an intake/mirror adapter during bootstrap, but these
objects are the long-term semantic surface.

## Object chain

```text
beta campaign/test mission
  mininet.beta/campaign/v1
        |
        v
structured finding
  mininet.beta/finding/v1
        |
        v
append-only disposition
  mininet.beta/finding-disposition/v1
        |
        +--> ordinary Forge task brief / work claim / technical review
        |
        v
accepted contribution
  mininet.beta/contribution/v1
        |
        v
optional Beta MINI grant evidence
  mininet.beta/grant/v1
```

Semantic JSON schemas are under `forge-native/schemas/`. The executable strict
binary/object implementation is in `crates/mini-beta` on the same signed,
content-addressed `mini-objects` + `mini-store` substrate.

## Bootstrap anonymity

Before Go-Live, the canonical Mininet contribution classification is anonymous.
The beta schemas therefore contain no contributor DID, username, legal name,
persistent pseudonym, reputation id, payment destination, or reusable reward
account.

A `submission_tag` is fresh for one finding. A `claim_tag` is fresh for one
accepted contribution. A Beta MINI account should be fresh/private unless the
user explicitly chooses otherwise after Go-Live policy exists. These handles are
not identity credentials and must not be reused to manufacture cross-submission
continuity.

The signed Mininet object author is the record/intake signer that attests to the
record, not the anonymous tester/contributor being described.

## Authority boundary

These objects are evidence and workflow state. They do not create merge,
release, reviewer, personhood, or governance authority.

Beta MINI is test-domain value only. A balance or participation grant MUST NOT
be consumed by Forge approval/quorum/release/personhood logic. `mini-beta` is
kept outside the production value/authority dependency graph and has a regression
test for that wall.

The current reference ledger intentionally does not hard-code a canonical grant
issuer. A single Founder/admin/server mint key would be a central authority
failure. The decentralized grant-acceptance mechanism is tracked explicitly in
#339 and must be proven before shared Forge-native Beta MINI can be called free
of trusted issuance.

## Minimum native surfaces before Forge canonicality

A Forge/CLI/app implementation should expose:

- campaign list/show and exact target retrieval;
- finding create/list/show with privacy-redaction warnings;
- disposition/duplicate relationships without editing the original finding;
- accepted finding -> task brief handoff;
- task discovery and expiring work claim;
- exact-state technical-review handoff;
- accepted contribution receipt;
- private Beta MINI claim/account handoff;
- replication/sync status and missing-object resolution; and
- export/mirror to GitHub that never makes GitHub canonical.

The acceptance gate is not UI completeness. It is a reproducible GitHub-outage
run of the entire contribution loop on independently operated Mininet nodes, as
tracked by #338 and `docs/FORGE_BETA_MIGRATION.md`.

## No silent production upgrade

Never upgrade Beta MINI in place into production MINI. Beta epochs are
resettable and disposable. Production value, if activated, comes through the
separately audited production value/custody/settlement path. Historical beta
contribution evidence may remain useful evidence, but it is not a protocol debt
or automatic token allocation.
