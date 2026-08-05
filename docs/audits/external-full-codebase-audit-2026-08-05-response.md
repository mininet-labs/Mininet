# Response to the full-codebase improvement audit of 2026-08-05

Follows `docs/gates/EXTERNAL_REVIEW_RESPONSE_TEMPLATE.md`. Records what this
batch actually changed, what it deliberately did not, and what the audit
establishes and does not establish. This is a disposition record, not an audit.

## 1. Review identity and exact state

- **Review ID:** `ER-2026-08-05-full-codebase`
- **Gate issue:** none — this was an unsolicited full-repository review, not a
  gate deliverable under #72 or #262. It does **not** discharge any external
  gate.
- **Scope package:** `docs/audits/external-full-codebase-audit-2026-08-05.md`
  (received deliverable, reproduced verbatim)
- **Exact state reviewed:** `main` @ `e60191c4a0fdc8be42995cc2fb21b9a56e910f44`
  and PR #297 @ `7a48a878b9007364a7779f3e03ceec5066e34d61`
- **Response publication:** public, in-repository

## 2. Reviewer competence and independence

Not established. The reviewer is unidentified to this repository, and no
competence, method, standard, or conflict-of-interest evidence was supplied.
**Nothing here may be cited as independent review satisfying D-0047 or #72.**
The findings are weighed on their reproducibility alone — several were
confirmed against the code and are fixed below; the rest are recorded as
recommendations, not as adjudicated defects.

## 3. Scope and exclusions

### Included

Documentation, the generated repository map, workspace configuration, CI
configuration, and representative implementations in identity, storage,
proof-of-replication, consensus evidence, and PR #297.

### Excluded

The auditor states in §2 that the environment could not clone the repository,
so this was **not** a local build, test run, or automated static-analysis pass.
Most findings are therefore classified by the auditor as systemic
recommendations rather than verified code findings, and the report asks for a
follow-up run of its own proposed automated checks in a local checkout.

### Claims examined

That breadth has outrun integration and assurance; that too many security
properties remain caller responsibilities; that PR #297 was unsound; and that
several shared concerns (codecs, freshness, replay, state) are duplicated per
crate rather than owned once.

## 4. Review questions

Answered by this batch: is PR #297's storage-fraud claim actually bound to a
`mini-porep` seal (§7.1)? Are historical detached signatures first-class
(§4.1)? Does the dependency-audit job gate (§19.1)? Does Merkle verification
constrain proof shape (§7.5)? Do audit documents state their own scope
(§20.1)?

Not answered here: everything requiring an operated service, a durable state
model, or an externally reviewed protocol.

## 5. Findings

Confirmed against the code, and fixed in this batch:

| Audit § | Finding | Confirmed how |
|---|---|---|
| 7.1 | PR #297 authenticates a DID's statement about a generic `StorageCommitment` with no binding to a seal or accepted audit; an attacker can copy an honest provider's root and frame them | Reproduced as a test before the rewrite |
| 4.1 | `Kel::verify_message` verifies against current key state only, so long-lived claims stop verifying after ordinary rotation | Reproduced; now `verify_message_at` |
| 6.4 | Canonical ordering is not enforced, so equivalent objects have multiple content IDs | Found in seven merged decoders |
| 6.3/6.2 | Encoders can produce values their own decoders reject | Found: a >16-key identity could not decode its own object |
| 7.5 | Merkle verification checks the root but not the proof's shape against the declared leaf count | Reproduced: a proof padded with `None` siblings verified identically |
| 19.1 | `continue-on-error: true` masks a hard scanner failure, so the job reports a pass when the scanner never ran | Confirmed from the workflow and its own comments |
| 20.1 | The memory-safety audit states 22 crates against a workspace that now has 72 | Confirmed: 72 crate directories today |

One further defect was found by this repository while remediating §7.1, and is
not in the audit: `mini_porep::sample_challenges` drew challenge layers
uniformly, but the audit binds `SealCommitment::replica_root` only at
final-layer challenges — so an audit could succeed having never constrained
`replica_root`, letting a forged replica root pass (about one audit in
twenty-six for a 2-layer seal with 8 challenges).

## 6. Response and disposition

**Fixed in this batch** — D-0439, D-0440, D-0441. See those entries for the
full reasoning; in summary: the storage-fraud crate is rebuilt around audited
registration with unattributed conflict evidence; `did-mini` gains historical
key-state verification, shared wire limits, a canonical signature-order rule,
and a `STORE` capability; seven merged decoders are corrected against those
shared rules; `mini-porep`'s audit reserves final-layer challenges;
`MerkleProof::verify` validates proof shape; the dependency-audit job now
gates on the scanner running and distinguishes that from advisory findings; and
audit documents now carry a scope header.

**Accepted and tracked, not done here.** The great majority of the report.
Named explicitly so they are not quietly lost: the vertical-slice programme
(§3.1), verified-wrapper types (§3.2, §22.3), canonical state views (§3.3), a
complexity budget (§3.4), witnessed freshness classes (§4.2), namespaced
capabilities (§4.3), pairwise lifecycle tests (§4.5), cross-language identity
vectors (§4.6), maturity labels (§5.1), a shared transcript framework (§5.3),
misuse tests (§5.4), post-quantum migration of stored signatures (§5.5), a
`mini-codec` crate (§6.1), version migration policy (§6.5), decoder fuzzing
(§6.6), the storage lifecycle state machine (§7.3), audit security-parameter
calculations (§7.4), erasure corruption tests (§7.6), capacity accounting
(§7.7), the whole networking section (§8), messaging (§9), consensus
operations (§10), treasury ceremonies (§11), personhood typing (§12), forge
reproducibility and installer power-loss testing (§13), search (§14), platform
integration (§15), the persistence/replay/clock programme (§16), the testing
strategy (§18), SBOM and generated-file freshness (§19.3, §19.5), and the
documentation/governance items beyond §20.1.

**Partially disputed.** §20.3 ("Accepted status should not precede legitimate
adoption") is correct as a rule and was already the practice for unmerged
proposals; but D-0431 records the repository's contrary position for *merged*
work, that merged code is the operative decision rather than a pending
proposal. This response does not reopen that. All three decisions in this
batch are *Proposed*.

**Not adopted here.** §20.1's request for a scope header on every audit is
implemented **forward-only**: the header is now required for new audits, and
the memory-safety audit carries a retrospective note. The other twelve
historical audits are left without one rather than back-filling commit SHAs and
tool versions that nobody recorded at the time — inventing that metadata would
make the documents look more rigorous than they were, which is the opposite of
what §20.1 asks for.

## 7. Closure recommendation

**Do not close any gate on this review.** It establishes no independent
competence, ran no tooling, and explicitly excludes building or testing the
workspace. #72 and D-0047 remain open and untouched. The appropriate closure is
narrower: the seven confirmed findings above are addressed, and the remainder
stands as a recommendation backlog for founder prioritisation.

## 8. Attestation

Prepared by the AI engineering session that produced the accompanying batch.
AI work is evidence, not approval; this response carries no quorum,
canonicalization, or gate-closure authority. The confirmed findings were
reproduced as failing tests before being fixed, and those tests are in the
batch.

## 9. What this response does not prove

- That the fixed findings were the only ones present. Six sections of the
  report were confirmed against code; the rest were not independently verified
  either way.
- That any component is now production-safe, audited, or Sybil-resistant.
- That the auditor is competent, independent, or acting in good faith — none of
  that was established, and the findings are accepted on reproducibility alone.
- That the recommendation backlog in §6 is complete or correctly prioritised.
