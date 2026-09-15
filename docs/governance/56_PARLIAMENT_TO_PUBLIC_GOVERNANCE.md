# Parliament to Public Governance Transition

**Status:** Proposed transition specification — not active merely by file presence

## 1. Goal

Mininet should not build one government for the founding phase and another unrelated government for mature humanity. The Founding Parliament is therefore designed as the first operating form of the public governance system.

The transition changes who may legitimately occupy seats and participate in public selection. It should not discard the useful machinery of committees, dossiers, minority reports, votes, recalls, emergency warnings, duty receipts, and exact-state Forge history.

## 2. Phases

### Phase F0 — Founding Parliament

- small chamber, beginning at 7 active seats;
- candidate entry is invitation-gated;
- H0 invitation allowance and bounded Guardian Stay may exist;
- persistent Steward identity begins only prospectively under the activation decision;
- no Pre-Go-Live anonymous history is retroactively linked.

### Phase F1 — Expanding Parliament

- seat capacity may grow through `2n + 1` steps when evidence permits;
- mature public-personhood candidates may begin entering alongside invitation-qualified candidates;
- public eligibility may only increase;
- invitation ancestry must not determine vote weight or seat priority;
- Forge must progressively replace GitHub as the authoritative substrate.

### Phase F2 — Public Parliament

- invitation is no longer a prerequisite for political eligibility;
- all mature verified humans participate under equal public rules;
- H0 exceptional Guardian Stay is permanently disabled;
- no founder recovery dependency remains;
- historical Founding Stewards retain history and earned rewards but no permanent political privilege.

## 3. Expansion gate

A chamber expansion is valid only when all required evidence is recorded for the exact transition state:

- enough qualified candidates to fill the proposed capacity;
- standing committees can be staffed;
- parliamentary and committee key rotation/recovery has been exercised;
- an adversarial governance/capture exercise has passed;
- Forge can operate without GitHub as a hard dependency;
- invitation concentration/capture has been reviewed and found within the then-canonical safety bounds.

Calendar time, token price, number of invitations, or popularity is insufficient.

The reference policy kernel enforces the exact capacity progression and requires evidence booleans plus candidate capacity. Production activation will need richer signed evidence objects rather than trusting booleans.

## 4. Public eligibility must be monotonic

The transition state carries a public-eligibility measure. Its exact production representation may change, but the invariant is fixed:

> once political access has been opened to a class of mature verified humans, insiders cannot later close that access merely to regain control.

The reference kernel models this as `public_eligibility_bps` and rejects any backward transition.

At Public Parliament activation the value is 100%, and the phase cannot return to Founding or Expanding.

## 5. Public-transition evidence

Final transition additionally requires:

- mature personhood honestly proving one-human participation at the level claimed;
- a public ballot/selection mechanism demonstrated under adversarial conditions;
- no Founder/H0 recovery dependency;
- Forge operation without GitHub;
- successful governance adversarial exercise;
- explicit one-way disabling of H0 exceptional authority.

If personhood still counts only `did:mini` identity roots, this gate is not satisfied.

## 6. Founder/H0 sunset

The Public Transition atomically requires:

`h0_guardian_active = false`

That bit or its canonical successor is one-way. No emergency motion, repository setting, H0 request, or ordinary vote may restore it.

H0 remains free to stand, serve, vote, contribute, or retire under the same public rules as any other mature human.

## 7. Representation after public maturity

This proposal deliberately does not freeze one century-long electoral formula before mature personhood and real-world governance simulation exist.

The public system SHOULD preserve:

- equal-human political voice;
- rolling active terms rather than permanent office;
- committee competence;
- public recall/removal mechanisms;
- capture resistance against wealth, celebrity, employer, party, and campaign machinery;
- meaningful paths for ordinary humans to serve without becoming career politicians.

A hybrid of equal-human election and qualified sortition is a candidate for later simulation, not a claim implemented by this PR.

## 8. No self-preserving council

The Founding Parliament may not make its own continuity the criterion for readiness.

Once the objective Public Transition evidence is complete and the applicable threshold approves the exact transition, Founding-only privileges end.

H0's Guardian Stay cannot target the Public Transition, its own sunset, or a valid Guardian Stay override.

## 9. GitHub and Forge

GitHub may host this proposal and bootstrap its implementation, but it must not become the permanent electorate, identity registry, seat registry, or source of parliamentary legitimacy.

Before Public Parliament, the complete governance loop must operate using Mininet/Forge-native objects and independently operated infrastructure.

## 10. Exact failure point

**FAIL** if the expanding Parliament can grow numerically while all access still depends on a small insider invitation tree, if public eligibility can be rolled back, if `did:mini` roots are falsely called humans, or if H0 remains a necessary recovery/authorization path after public maturity.

**PASS** only when the same useful parliamentary machinery becomes genuinely open to mature verified humanity and all founding-only political privilege expires.
