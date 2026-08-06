# Day-0 release code hardening audit — 2026-08-05

**Status:** engineering audit and patch evidence; not release approval or an external cryptographic audit

**Audited base:** `af45860914edfe16e0fbc483b927bb2b274666d8`; reconciled onto
`a9f9c96d3bb3dd9419ffce54b9b9abcbe05c3a11`

**Patch branch:** `codex/release-security-audit`

**Decision:** proposed D-0442

**Audit scope**

| Field | Value |
|---|---|
| Reviewed at | `main` @ `af45860914edfe16e0fbc483b927bb2b274666d8`, reconciled and extended at `a9f9c96d3bb3dd9419ffce54b9b9abcbe05c3a11` |
| Workspace size | 72 crates at the reviewed base |
| Method | Manual cross-module review of the listed release-sensitive PRs; regression test for every fixed finding; targeted all-feature tests and workspace Clippy |
| Tool versions | Repository-pinned Rust toolchain; Python version recorded by governance CI |
| Revalidation trigger | Any change to block/header/body hashing, consensus wire/archive formats, issuance plans, payment admission, owner sealing/cache policy, or crawler address admission |

## Scope

This pass reviewed release-sensitive code introduced or materially changed by
merged PRs #271–#275, #283–#294, and #297, concentrating on:

- monetary issuance, payment claims, admission, and deterministic execution;
- consensus proposal authentication, finality restoration, snapshots, and
  persistent recovery;
- owner-only encryption, cache privacy, object storage, proof-of-storage, and
  collision evidence;
- crawler network isolation and SSRF defenses; and
- cryptographic suite boundaries and malformed-input behavior.

Draft PR #296 owns transport/authenticated-query runtime convergence and edits
the same transport/query files. This audit deliberately does not duplicate or
silently supersede that active work.

Merged PR #299 replaced D-0437's original storage-fraud claim format with
audited replica registration and separately fixed its portability/canonical
encoding boundaries under D-0439–D-0441. That newer implementation is
preserved; the superseded storage-fraud patch from this audit is not carried
onto the reconciled branch.

## Fixed in this branch

### Consensus and value

1. **Proposal signatures did not bind the settlement body.** The signed
   transcript authenticated only the header hash. Because execution drops
   invalidly signed claims, an intermediary could alter those body entries
   without changing `state_root`, while the proposal signature still verified.
   The v2 proposal-signing transcript now includes
   `SettlementBlockBody::hash()`, with an adversarial mutation test.

2. **One proposal could demand excessive signature verification.** The
   consensus ceiling was 100,000 claims while the operational admission ceiling
   was 4,096. Both now use 4,096, bounding Byzantine-proposer CPU amplification
   before voting.

3. **Monetary epoch rollover could repeat issuance.** `saturating_add(1)` made
   `u64::MAX` its own successor. Epoch progression now uses checked addition and
   rejects a terminal ledger instead of accepting the terminal epoch again.

4. **A retryable payment was blocked by admission.** Canonical execution
   explicitly permits an exact `InsufficientFunds` claim to be retried after the
   payer receives funds, but admission treated every rejection as terminal.
   Admission now permits only that retryable reason, then rechecks the current
   balance and all ordinary policy.

5. **Payment signing could panic on a post-quantum key.** The payment format is
   currently Ed25519-only, but accepted the suite-polymorphic `SigningKey` and
   called its Ed25519-only panic path. It now returns
   `UnsupportedSignatureSuite` before signing.

6. **Admission cleanup had a codec-dependent panic.** Accepted wire sizes are
   now retained with pending claims, so removal is exact and cannot panic or
   re-encode under a later codec.

### Storage and privacy

7. **Owner sealing accepted plaintext that could not fit in an object.** The
   old limit allowed an 8 MiB plaintext and then added a 60-byte sealed-box
   overhead, producing bytes rejected by `Payload::Encrypted`. The exported
   plaintext ceiling now reserves the full ephemeral-key, nonce, and AEAD-tag
   overhead; the largest accepted plaintext produces exactly one legal maximum
   payload.

8. **Encrypted objects could be assigned advertising cache tiers.** Both direct
   tier changes and predeclared tiers could bypass the documented
   `PrivateOnly` ceiling. Advertising assignments now fail closed once payload
   privacy is known, and insertion rechecks predeclared policy before writing
   the object. Cache metadata also rejects non-canonical multi-byte values.

### Crawler security

9. **The crawler address filter omitted special/transition ranges.** It now
    blocks deprecated IPv4 6to4 relay space, uses a strict ordinary-global IPv6
    range, excludes IETF protocol, 6to4, and documentation prefixes, and handles
    the globally reachable RFC 6052 NAT64 prefix by independently validating
    its embedded IPv4 destination. This preserves public NAT64 crawling without
    letting DNS64 encode loopback/private IPv4 targets. Policy was checked
    against the current [IANA IPv4 special-purpose registry](https://www.iana.org/assignments/iana-ipv4-special-registry/iana-ipv4-special-registry.xhtml)
    and [IANA IPv6 special-purpose registry](https://www.iana.org/assignments/iana-ipv6-special-registry/iana-ipv6-special-registry.xhtml).

## Follow-on blocker resolved in D-0443

The audit originally left exact historical body finality as P1. Implementing
that migration exposed two more end-to-end consensus defects and closes all
three together:

1. **Finalized headers/QCs did not commit the exact body.** Block-header v2
   adds `body_root`; votes, QCs, execution, archive rows, snapshots, and
   catch-up now bind and verify it. An adversarial regression proves that a QC
   for an empty body cannot authorize a different invalid-claim body even when
   both produce the same `state_root`.
2. **Consensus serialization omitted monetary epoch plans.** Proposal and
   catch-up body codecs now carry the exact bounded `ScalableEpochPlan`; a
   round-trip regression proves no issuance field disappears in transit.
3. **The scalable epoch commitment omitted nested authority-bearing fields.**
   Its v2 commitment and new canonical codec include the Human Share epoch and
   vesting duration as well as every other plan field. Truncation, trailing
   bytes, count/length abuse, and oversized plans fail closed.

This is an intentionally incompatible prelaunch hard fork. The version map,
archive quarantine, fresh-genesis procedure, abort rule, and prohibition on
same-network rollback after v2 finality are specified in
`docs/design/exact-body-finality-v2-migration.md`.

## Validation evidence

- focused tests passed for `mini-consensus`, `mini-crawler-fetch`,
  `mini-economy`, `mini-execution`, `mini-settlement`, `mini-store`, and
  the original-base `mini-storage-fraud` review (since superseded by PR #299);
- after D-0443, 160 targeted tests pass across `mini-chain`, `mini-consensus`,
  `mini-contribution`, `mini-economy`, and `mini-execution`, including real TCP
  consensus and state-sync/reopen tests;
- workspace-wide Clippy with all targets/features and `-D warnings` passed,
  followed by an exact-head focused Clippy rerun for the two crates changed
  during final self-review;
- the governance runtime/baseline checks and 76 Python governance tests passed
  (two platform-specific tests skipped);
- truncation, tampering, wrong-suite, retry, maximum-size, private-tier,
  special-address, and rollover regressions have explicit
  tests; and
- the complete workspace test matrix remains required in Linux CI. An attempted
  local Windows run first exhausted the build volume and, after safely cleaning
  only this worktree's generated `target/`, Windows Application Control
  rejected a newly compiled dependency build script with OS error 4551. The
  exact-head focused tests were subsequently rerun through an existing trusted
  dependency cache; host policy was not weakened to manufacture a full pass.

## Unresolved release blockers and ownership

| Priority | Code gap | Why this PR does not guess a solution |
|---|---|---|
| P0 | External review of `mini-value` range/ring constructions, PoRep/PDP assumptions, consensus, issuance, and key lifecycle | Internal tests cannot establish cryptographic security or economic soundness. |
| P0 | Authenticated public transport/runtime convergence | Active draft PR #296 owns the overlapping implementation. It must pass adversarial internet tests and independent review. |
| P0 | Sybil-resistant personhood and validator-set formation | Identity roots are not verified humans; code cannot manufacture the missing trust primitive. |
| P1 | Canonical rejection history grows without deterministic pruning | An unbounded sequence of valid rejections can eventually prevent single-frame snapshots. Pruning changes wallet proof semantics and needs an authenticated-history design. |
| P1 | Snapshot transfer is one bounded frame (8 MiB state ceiling) | Internet-scale state needs chunked Merkle transfer with independently verified chunks, resumability, and anti-equivocation tests. |
| P1 | Payment submission lacks an authenticated, rate-limited public service | The bounded admission pool is node-local. Exposing it directly would create spam and metadata risks. |
| P1 | Owner sealing lacks KEL binding, rotation, recovery, and re-sealing | The current primitive is local confidentiality. Device loss still means permanent ciphertext loss. |
| P1 | Storage collision evidence has no governed consequence path | Detection intentionally assigns no penalty, exclusion, reward clawback, or appeal. Those are separate authority-bearing systems. |
| P1 | Crawler is not yet a complete search pipeline | It still needs robots retrieval/cache, persistent frontier scheduling, HTML extraction, crawl provenance flow, index publication, abuse controls, and distributed budget enforcement. |
| P1 | Private value is not integrated with canonical settlement | `mini-value` primitives and transparent Tier-0 account settlement remain separate; no launch claim should imply private production payments. |

## Release interpretation

This branch removes concrete code defects but does not make Mininet
production-safe by declaration. A public beta should be capability-gated around
the remaining rows, carry explicit data-loss/value-risk warnings, and require
independent cryptography, consensus, storage, transport, and application
security sign-off on the exact release commit.
