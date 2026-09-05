# The road to release

**Status: no release date is set, and none should be inferred from this
document.** Most of what remains is not code — it is external audit, legal
review, real hardware, and one open research question nobody has solved. Those
close when an outside party actually does the work, not when engineering
finishes preparing for them. A roadmap that promised a date would be the first
dishonest thing in this repository.

What this document *is*: the single ordered account of what stands between
today and a public release that real people can trust with real money, real
identity, and real speech. Every item says what it is, why it blocks release,
what would actually close it, and who can close it. Nothing here is closed by
writing more Rust unless the "Closed by" line says so.

This file is the detail. The [README](../README.md#the-road-to-release) carries
the summary, and the two are kept consistent by `tools/check_roadmap.py`, which
fails CI if they drift.

## How to read the status marks

| Mark | Meaning |
|---|---|
| `done` | Closed. Cites the D-number or merged PR that closed it. |
| `active` | Being worked now. |
| `ready` | Nothing blocks starting it; nobody has. |
| `blocked` | Waits on another row, named in `Blocked by`. |
| `outside` | **Cannot be closed by this repository.** Needs an auditor, counsel, a researcher, real hardware, or a founder decision. |

`outside` is the important one. Six of the rows below are `outside`, and they
include the largest gate of all. Engineering's job for those is to prepare the
scope package under [`docs/gates/`](gates/) and then stop — never to quietly
downgrade a gate into something code can satisfy.

## Rule for keeping this current

A pull request that advances or closes a roadmap item **updates its row in the
same pull request**, with the D-number or PR that justifies the change. Not
afterwards, not in a follow-up. A roadmap updated in arrears is a roadmap that
describes the past.

`tools/check_roadmap.py` enforces what it can mechanically: every row has a
valid status, every `done` row cites a decision that actually exists in
`docs/DECISION_LOG.md`, every `blocked` row names a row that exists, and the
README's summary counts match this file. It cannot check that a row is
*honest*; that stays a review question.

---

# Phase 1 — Get the tree trustworthy again

Small, unglamorous, and first, because everything downstream is measured by
CI and by the governance validators. While these are red, no other claim in
this repository can be checked cheaply.

### R1 — Governance validator reads data, not commentary · `done`
The exceptions scan counted a commented-out template example as a live
governance exception, turning `governance-policy` on `main` red and
`canonical-governance` red on every open pull request. Also gave that check
its first tests in either direction.
**Closed by:** D-0452 / PR #309, merged 2026-09-04. `governance-policy` on
`main` is green again on the two merge commits since, and the validator suite
passes there (111 tests).

### R2 — A green, meaningful CI baseline on `main` · `done`
A **scheduled** run has now passed end to end on `main`, which is what this
row actually named — push-triggered green was never sufficient, because the
scheduled run is what caught both prior failures days after a merge.
**Closed by:** D-0461. Scheduled `ci` run 860 (2026-09-05 08:22 UTC) passed on
head `01ad0ce`, and every other workflow is green on that same head:
`governance-policy` 734, `reproducibility` 636, `android-ci` 505,
`android-reproducibility` 496, and CodeQL's default-setup "Push on main" 904.
`governance-canonical` has no run on that head **by design** — it triggers on
`pull_request_target`, so it only ever runs against pull requests; it was green
on #313, #314 and #316.
**The surviving exception, recorded rather than tolerated:** `dependency-audit`
reports advisories without gating (D-0441/D-0450). That is a deliberate,
already-decided split — the scanner failing to *run* is a hard failure, while
an advisory against a pinned dependency is a loud warning — and it stays a
warning until this workspace has a triage process for one. R2 is closed with
that exception named, not with it hidden.

---

# Phase 2 — Finish the money layer so it can be audited as one thing

The external cryptography audit (R12) is the single largest gate before any
real value moves. It is worth much more if what it reviews is a *complete*
path rather than a promising set of pieces. These rows exist to make the audit
scope a whole system.

### R3 — Retire the transparent payment path · `ready`
`mini-contribution`, `mini-engagement`, `mini-bounty`, `mini-execution` and
`mini-chain` still settle through `mini_settlement::PaymentClaim`, with a
stable payer key, a cleartext amount, and a per-payer `sequence` counter — a
complete public transaction graph.
**Why it blocks:** D-0451 established that if both a transparent and a private
format exist, *choosing* the private one is itself a signal. Until the
transparent path is gone, privacy is opt-in, and opt-in privacy is privacy for
nobody.
**No longer blocked.** R4 shipped the change and fee model these crates
needed and R5 shipped the finality they settle against, so the shielded path
can now do everything the transparent one does. What remains is a migration
decision rather than a missing capability: whether the transparent path is
deleted outright or deprecated first, and what happens to claims already
settled under it. That is a founder call with a D-number, not something to
pick unilaterally while touching five crates' public APIs.
**Closed by:** the five crates settling only through `mini-private-payment`,
with a decision recording what each lost or kept.

### R4 — Fees, change, and multi-output claims · `done`
A private claim now spends up to 16 inputs and creates up to 16 outputs, and
proves `Σ inputs = Σ outputs + fee` without opening a single amount. Change is
an output paying yourself, built by the same code path as any other, so
nothing on the wire says which output was the payment.
**Why it blocked:** R3 cannot happen without it, and a transparent fee attached
to a shielded payment reintroduces exactly the linkable value the shielded
path removes.
**Closed by:** D-0455 — a two-column MLSAG in `mini-value` binding each
pseudo-output commitment to the ring member the signer actually controls, the
balance check in `mini_private_payment::verify`, and the commitment opening
carried in the sealed memo so a received payment is immediately spendable.
Claim format v2; v1 claims do not decode, because a v1 claim proved no
balance.
**What it did not close:** the fee is public, and so are a claim's input and
output counts — each is a fingerprint, stated in the crate docs rather than
papered over. What a fee *should be*, and who collects it, is an economics
decision this makes possible and does not make. The construction is standard
RingCT; this implementation of it is still unaudited and still gated behind
R12.

### R5 — Chain-backed private ledger view · `done`
`PrivateLedgerView` had one implementation — a test double that finalized
whatever it was told to — so no private payment could reach `Finalized` on
anything but its say-so. The chain now finalizes shielded spends and
`reconcile` reads real canonical state.
**Closed by:** D-0457. `mini-execution` finalizes opaque
`(key_image, claim_digest)` records with the transparent path's own
first-wins/M3 discipline, all-or-nothing per claim; `mini-private-payment`'s
`ChainBackedPrivateLedger` adapts any key-image lookup into a
`PrivateLedgerView`. The two crates deliberately do **not** depend on each
other: that edge would have been the first path in this tree from a value
crate to the crate that counts votes (P1, Directive 16).
**What it did not close:** the chain finalizes a key image on a proposer's
say-so. It cannot check that a valid claim produced one — that is the
cryptography it deliberately cannot see — so a Byzantine proposer can burn
an output that is not theirs. The *ordering* is real; the ledger's
*contents* are not yet trustworthy. That validity rule is R8's.

### R6 — Amount disclosure for accounts that choose it · `done`
A view key made income enumerable and left it un-addable. An account can now
publish `(amount, blinding)` openings for chosen outputs, and anyone
recomputes the commitment to check them.
**Closed by:** D-0458. `AmountDisclosure` plus `audit_amounts`, which returns
`AuditedIncome` — the opened total **beside** the count of recognized
payments left unopened — rather than a bare number. Openings are chosen and
no cryptography can force the choice, so a sum reported without that count
would quietly mean "the part they decided to show". Recognition comes from
the view key, so the auditor learns the payment count from cryptography
rather than from the discloser's cooperation, which is what makes a withheld
opening visible.
**What it did not close:** completeness in the direction that matters. A
disclosure covers one account and nothing proves it is the only one its
holder controls. An audit still sees income only — never a balance.

### R7 — Decoy weights fitted to real traffic · `blocked`
`AGE_WEIGHTS` (D-0449) is a legible starting shape, not a distribution fitted
to measured spend ages, because no such traffic exists yet. `MIN_RING_SIZE`
should be revisited on the same evidence.
**Blocked by:** R17 — there is no traffic to fit until something real runs.

---

# Phase 3 — Make the network a network

### R8 — Consensus production gaps · `active`
The consensus slices are tested protocol work, not a production network. Four
named gaps: no state sync for a node that missed a whole height; no slashing
layer; peers are supplied rather than discovered; and `mini_bearer::Channel`'s
handshake is anonymous, so it proves nothing about *which* validator is on the
other end.
**Why it blocks:** each is a liveness or accountability hole that only appears
under real adversarial load.
**Progress:** the accountability gap is closed by **D-0460**. A validator that
signs two conflicting votes at one `(phase, height, round)` now convicts
itself: `EquivocationProof` carries both votes, anyone can check it, and
`ValidatorSet::excluding` computes the set without the offender. The sanction
is exclusion, never an economic penalty — Mininet has no stake, and a penalty
denominated in value would make validator behaviour a function of wealth in
exactly the direction P1 and Directive 16 forbid.
**What that did not close:** nothing *detects* equivocation without vote
gossip, nothing ejects automatically (adopting an exclusion is a governance
action, or fabricating a removal becomes the attack), and only double-voting
is covered — silence, censorship and invalid proposals are not self-proving
in the same way.
**More progress:** **D-0462** closes the peer-discovery gap.
`mini-consensus::discovery::pex_over_tcp`/`serve_pex_over_tcp` carry
`mini-net`'s already-tested PEX logic over the same anonymous, encrypted
`mini_bearer::Channel` handshake `catch_up_over_tcp`/`state_sync_over_tcp`
already use, so a node can learn peers it was never handed, with no
directory server. Not wired into `TcpMesh::establish` itself — that
constructor's deadlock-free convention still needs one address list every
node agrees on up front, so turning a discovered address book into a mesh
topology stays a host decision.
**Closed by:** the two remaining gaps — state sync (also substantially
closed by D-0207's catch-up/state-sync primitives, though not wired into
`TcpMesh::establish` either) and a validator-authenticated bearer
handshake — with tests that fail without the fix, plus the honest limits
restated for whatever remains. The shielded-spend validity rule named in
D-0457 also lands here: today the chain finalizes a key image on a
proposer's say-so.

### R9 — KEL freshness and witnesses (M3) · `ready`
The stale-KEL revocation gap, audit #12 finding F4. A device whose delegation
was revoked can still be accepted by a peer holding an old key event log.
**Why it blocks:** it is a live identity-security hole, not a design question —
revocation that does not propagate is revocation in name only.
**Closed by:** witness/freshness bounds enforced where KELs are accepted.

### R10 — BLE and local-radio transport · `outside`
Needs real phone hardware and Kotlin radio wiring. The BLE-first bootstrap
story is central to the "works without infrastructure" claim and is currently
unproven on any physical device.
**Closed by:** the T1–T6 matrix in `docs/gates/hardware-test-protocol.md` run
on real hardware (#97).

### R11 — A client application people can actually use · `ready`
`mini-keystone`'s two-device demo exists, but two physical devices have not
completed the full acceptance path, and the encrypted keystone/range/reward
path is still separate.
**Why it blocks:** a protocol nobody can run is not a released network.
**Closed by:** the acceptance path completed on two real devices, end to end.

---

# Phase 4 — The gates code cannot close

These are the reason there is no date. Each is tracked in
[#99](../../issues/99) with a scope package under [`docs/gates/`](gates/).
**A passing test suite, a founder review, and an AI review do not close any of
them.**

### R12 — External cryptography audit · `outside`
Scope: `mini-value`, `mini-treasury`, `mini-settlement`, `mini-bounty`, and now
`mini-private-payment`. Three founder-overridden AI-authored prototype
constructions (stealth addresses, ring signatures, Bulletproofs) compose into
one object, plus a protocol decoy-sampling rule and a disclosure path.
**Why it blocks:** it gates everything value-bearing (D-0047). A privacy
failure does not announce itself — it produces payments that look private and
are not.
**Closed by:** a real applied-cryptography auditor engaged and signed off
(#72, `docs/gates/crypto-audit-scope.md`). Founder action, at day 0.

### R13 — FROST DKG audit · `outside`
The DKG and resharing are implemented and tested (D-0059/D-0060); the audit is
not done. A DKG bug can permanently expose treasury and bridge keys — not one
transaction, the keys themselves.
**Closed by:** #93, `docs/gates/dkg-audit-scope.md`. Founder action, at day 0.

### R14 — Legal counsel review · `outside`
Contribution rails and treasury, before either touches real funds.
**Closed by:** #96, `docs/gates/legal-review-brief.md`. Founder action, at day 0.

### R15 — Sybil resistance and personhood · `outside`
**The sharpest open question in the project** (#18, #21). Everything today
counts identity roots. An identity root is not a verified human, and nothing
in this repository may ever be described as "one human, one vote" until
SPEC-02 personhood actually lands.
**Why it blocks:** equal-weight-per-root governance is only meaningful if roots
are scarce. Today they are not.
**Closed by:** funded research on the tracks named in D-0075 —
private TLS predicates, sensor provenance, blind uniqueness credentials,
private co-presence diversity, coercion modeling. This is open research, not
engineering debt.

### R16 — Tokenomics validation · `outside`
D-0073/D-0074 are decided and a 576-scenario simulation harness has been run,
but calibration authority is not something this repository can hold.
**Closed by:** a mechanism-design specialist validating the calibration
(#47, #50).

---

# Phase 5 — Release engineering

### R17 — Adversarial testing at real-world scale · `blocked`
**Blocked by:** R8, R11.
Everything above is tested against fixtures and local processes. Nothing has
faced a hostile internet.
**Closed by:** a sustained public test network with results published, however
unflattering.

### R18 — Genesis, validator set, and activation · `blocked`
**Blocked by:** R8, R12.
Nobody has chosen genesis, the initial validator set, activation time, or
release state. D-0443 deliberately left all four out of scope.
**Closed by:** a founder decision recorded as a D-number, with the reasoning
public.

### R19 — Release, install, and rollback on real targets · `ready`
The spine exists end to end (D-0066 Batches 1–5) and
`tools/no_github_outage_demo.sh` proves the lifecycle with GitHub never named.
What has not happened is that path running on real target machines, repeatedly,
including a deliberately broken release rolling itself back.
**Closed by:** the Debian appliance profile (D-0446) exercised on real
hardware, with the rollback path proven under failure.

### R20 — The honest launch statement · `blocked`
**Blocked by:** R12, R13, R14, R15, R16, R17, R18 — every gate whose outcome
the statement has to report. It is written last on purpose: it is an account
of what actually happened, not a plan.
Before release, one document states plainly what is proven, what is audited by
whom, what is still assumed, and what a user is trusting when they install
this. Every "not built", "not audited", "not anonymous" statement in the tree
gets re-checked against reality on that day.
**Why it matters:** the entire project is a bet that honesty is a feature. The
launch statement is where that bet is settled.

---

## What is deliberately *not* on this list

- **Extreme-environment and DTN operation** (#28) — real domain expertise
  needed, explicitly not launch-blocking.
- **Local Wi-Fi data bearer** (#98) — connectivity, not a security signal;
  lower urgency than R10.
- **Automatic recruitment, contributor directories, working-group
  delegations** — coordination conveniences, not release requirements.
- **Anything that would weaken a frozen invariant to ship sooner.** There is
  no row for that and there never will be. If a gate cannot be closed
  honestly, the answer is that the network is not ready, not that the
  invariant was too strict.
