<!--
Mininet has no owner and no admin key (P3) — this template exists to make
review fast and consistent while the on-chain forge isn't live yet
(SPEC-11). See CONTRIBUTING.md for the full checklist, docs/INVARIANTS.md
for the frozen/tunable register this PR is checked against, and
docs/FOUNDER_DIRECTIVES.md for the reasoning behind both when a change
falls outside anything either document anticipated. Delete this comment
before submitting.
-->

## Summary

<!-- 1-3 sentences: what changed and why. Link an issue/decision if one exists. -->

## Does this touch a frozen domain?

<!--
Tier F (docs/INVARIANTS.md) covers things like: vote weight, personhood,
admin/owner keys, forced updates, data sovereignty, unmasking a user. If
this PR is Tier O (app surface, client software, plugin, new bearer/storage
client) or a Tier T tunable change within existing bounds, say so and skip
the rest of this section.
-->

- [ ] This PR does **not** touch a Tier-F frozen invariant.
- [ ] This PR **does** touch a Tier-F domain — the relevant `docs/INVARIANTS.md`
      row's **Enforced by** cell is updated, and a `D-00xx` entry is added to
      `docs/DECISION_LOG.md` explaining the choice.

## Test plan

<!-- What did you run, and what should a reviewer run to verify? -->

- [ ] `cargo fmt --all`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --all --all-features`
- [ ] `Cargo.lock` is committed if dependencies changed (D-0006)
- [ ] New/changed behavior has a test that fails without the change

## Reviewer checklist (2-approval floor, D-0033)

<!--
Protocol-critical repos require at least two distinct maintainer approvals
before merge — see mini-forge::governance::PROTOCOL_MIN_APPROVALS. If this
PR was AI-assisted on crypto-sensitive code, say so explicitly; those also
need two human approvals, no exceptions.
-->

- [ ] I reviewed this as a maintainer, not just skimmed the diff
- [ ] No balance, stake, or payment appears anywhere in a vote/quorum/access rule
- [ ] No new path lets any single key or party unmask a user, force an update, or act as an owner/admin
- [ ] For any judgment call not covered by a spec or invariant: it holds up against `docs/FOUNDER_DIRECTIVES.md` (name the directive if it's non-obvious)

🤖 If this PR (or parts of it) were AI-drafted: note which parts, and confirm a human reviewed the crypto/identity/governance-sensitive portions line by line.
