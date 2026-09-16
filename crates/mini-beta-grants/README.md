# mini-beta-grants

Campaign-scoped multi-party acceptance evidence for **Beta MINI only**.

This crate exists because `mini-beta::BetaMiniLedger` deliberately does not decide who may issue a shared-network grant. Filling that gap with one Founder key, GitHub bot, faucet server, or treasury operator would create a trusted issuer. `mini-beta-grants` instead requires a short-lived campaign policy and multiple distinct operational authorizer DIDs before the underlying Beta ledger may apply a grant.

## What it proves

Given one exact campaign, policy, grant and immutable approval set, independent nodes can deterministically check:

- policy author/campaign/epoch/time consistency;
- a minimum three-member authorization set;
- testing and participation thresholds of at least two;
- unique DID counting (one DID cannot multiply its own weight);
- an exact testing-grant amount;
- explicit participation reward bands;
- same-campaign contribution evidence for participation rewards; and
- exact policy/grant binding for every approval.

The wrapper `SharedBetaLedger` calls `BetaMiniLedger::apply_grant` only after threshold acceptance succeeds, preserving the accounting core's separate supply, campaign-cap, wrong-epoch and one-contribution/one-award checks.

## Policy-fork and outsider-noise boundary

A campaign may have **exactly one valid policy** at this layer. If the campaign record authority publishes two independently valid policies, `resolve_unique_campaign_policy` fails closed with `PolicyConflict`; it never chooses by timestamp, object id, arrival order, repository state, or wealth.

The uniqueness rule does not hand a denial-of-service veto to arbitrary publishers. Objects with the policy type are ignored as policy candidates when they are malformed, target a different campaign, are authored by somebody other than that campaign's record authority, or otherwise fail policy validation. A third party cannot stop legitimate Beta grant validation merely by publishing policy-shaped noise into the shared type index.

This still is not distributed finality. A valid competing policy from the actual temporary campaign authority is intentionally a stop condition until #337/#338 provide canonical conflict resolution.

## Authenticity boundary

`mini-store` is deliberately persistence, **not** the signature/provenance trust boundary. Remote policy, grant, contribution and approval objects must therefore enter a normal node through `mini-sync`'s strict verified-ingest path (KEL resolution, signature verification, delegation/revocation and capability checks) before this crate evaluates their semantic threshold rules.

Direct `Store::insert` is used in local unit/integration tests for objects just created by known in-process controllers. It is not an acceptable network ingest shortcut. A product path that decodes arbitrary network objects, inserts them directly into `mini-store`, and then calls `validate_grant_acceptance` would be insecure even if the threshold mathematics passed.

This separation is intentional: the domain crate must not duplicate identity/KEL validation with a second subtly different implementation. Before #341 can be wired into a shared product surface, the call path must be traced to and tested through the existing `mini-sync::Ingest` boundary. #341 does not claim that product integration yet.

## What it does **not** prove

A `did:mini` is not proof of a unique human. The current campaign record authority chooses the temporary policy membership, so several listed DIDs could still be controlled by one actor. This crate removes **unilateral grant issuance**, not the remaining Pre-Go-Live bootstrap selection dependency.

That distinction is intentional. The long-term fix is Forge-native governance/personhood and the one-way bootstrap-authority shutdown tracked by #338, not pretending multiple keys automatically mean multiple independent humans.

This crate also provides no rollback-free distributed finality. It can detect signed authorizer equivocation and produces deterministic validation once peers hold the same objects, but canonical conflict resolution for durable shared execution belongs to #337/#338.

## Hard walls

This crate must not depend on production value, settlement, treasury, chain/consensus, personhood/economy, Forge governance, or GitHub APIs. Its dependency-wall test enforces the expected runtime dependency list.

Beta MINI accepted here:

- is resettable test value;
- has no production-MINI conversion promise;
- does not increase governance/review/release/personhood authority;
- does not make authorizer membership a political role; and
- cannot be made valid by wealth, balance, employer, GitHub account, reward history or contribution count.

See `docs/design/decentralized-beta-grant-acceptance.md` for the full threat model and acceptance plan.
