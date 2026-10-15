# mini-beta

`mini-beta` is the deliberately isolated Open-Beta coordination and test-value
crate for Mininet's final pre-Go-Live phase (PR #334).

It has two jobs:

1. define Forge-compatible, signed, content-addressed objects for beta campaigns,
   structured findings, dispositions, accepted contribution receipts, and Beta
   MINI grant authorizations; and
2. provide a resettable in-memory/reference ledger for **Beta MINI**, so product
   flows can exercise grants, balances, transfers, failure cases, and contributor
   participation before production value is allowed.

## Hard separation from production value

This crate intentionally depends only on `did-mini`, `mini-objects`, and
`mini-store`. It does **not** depend on `mini-value`, `mini-private-payment`,
`mini-settlement`, `mini-treasury`, `mini-chain`, `mini-consensus`, or Forge
governance. That dependency wall is part of the security model: Beta MINI is a
test-domain instrument, not a pre-mint of production MINI.

Beta MINI is:

- bound to an explicit 32-byte beta epoch;
- resettable by starting a new epoch;
- denominated in micro-BETA-MINI for deterministic integer accounting;
- capped per grant and per epoch by local policy;
- incapable of producing governance/review/release/personhood weight because no
  such API or dependency exists here; and
- **not automatically convertible to production MINI**. A contribution receipt
  may remain historical evidence after Go-Live, but it creates no protocol debt,
  token allocation, investment promise, or guaranteed future entitlement.

## Anonymous bootstrap participation

Before Go-Live, `docs/governance/52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md` requires
Mininet-level participation records to remain anonymous. Accordingly, beta
objects contain no contributor DID, username, legal name, reputation handle, or
persistent contributor profile.

The signed object's author is the **record/intake signer**, not the contributor.
A finding uses a fresh 32-byte `SubmissionTag`; an accepted contribution uses a
fresh `ClaimTag`; a Beta MINI account is a fresh 32-byte `BetaAccountId`. These
handles are artifact-scoped and MUST NOT be reused to construct cross-submission
identity continuity.

GitHub usernames, email addresses, transport accounts, payment destinations, and
network metadata remain transport metadata only and must never become Mininet
governance credentials.

## Forge-native object types

The crate writes ordinary `mini-objects` custom objects into `mini-store`, so the
same records can replicate through Mininet storage/sync and later be surfaced by
Forge without a GitHub API dependency:

- `mininet.beta/campaign/v1`
- `mininet.beta/finding/v1`
- `mininet.beta/finding-disposition/v1`
- `mininet.beta/contribution/v1`
- `mininet.beta/grant/v1`

GitHub issue forms are a temporary adapter onto these fields, not the long-term
source of truth.

## Authority boundary

A grant authorization object is evidence that some accepted beta process wants a
grant applied. `BetaMiniLedger::apply_grant` deliberately does not decide which
signer is politically authorized; callers must select the authorization objects
accepted by their current beta/Forge policy. This keeps accounting separate from
governance. Balances never feed back into that selection.

## Build

```sh
cargo test -p mini-beta
```

License: CC0-1.0 (public domain).
