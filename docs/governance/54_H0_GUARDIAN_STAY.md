# H0 Guardian Stay

**Status:** Proposed temporary constitutional safeguard — not active merely by file presence

## 1. Purpose

The Founding Parliament is intentionally small and therefore vulnerable to coordinated capture, panic, bribery, institutional pressure, or a sudden majority willing to violate Mininet's founding human-freedom protections. H0 may therefore receive one exceptional, temporary power: a public Guardian Stay.

This is not a permanent veto. It is a time-bounded brake that forces reconsideration and can be overridden by an overwhelming Parliament.

## 2. H0 has one ordinary vote

H0's normal parliamentary vote has exactly the same weight as any other active Steward.

The Guardian Stay is separate from ordinary voting and may be used only for the narrow catastrophic-risk reasons below.

## 3. Permitted reasons

A stay may be issued only when an exact proposed action plausibly creates one of these failures:

1. money, balance, stake, employer power, contribution volume, or hardware ownership gaining political weight;
2. a permanent owner, administrator, recovery master, Founder key, platform key, or other unilateral authority;
3. forced software adoption, kill switch, or remote disabling authority;
4. hidden or unilateral identity unmasking/decryption authority;
5. confiscation or hidden value-control backdoor;
6. destruction of lawful fork, exit, or owner-sovereignty rights.

A disagreement about ordinary policy, aesthetics, staffing, technical taste, reward amount, committee preference, or political popularity is not a valid reason.

## 4. Effect

A valid Guardian Stay pauses canonicalization of one exact motion/state for at most **14 days**.

It does not:

- delete the proposal;
- rewrite votes;
- secretly modify the proposal;
- freeze user balances;
- stop the network;
- force an update;
- remove a Steward;
- extend H0's term;
- create a new law by itself.

The stay MUST publish:

- exact target digest;
- one enumerated reason from Section 3;
- constitutional/directive references;
- concrete failure theory;
- expiration timestamp;
- any evidence safe to disclose.

## 5. Mandatory reconsideration

After a stay:

1. Constitution, Rights & Appeals produces a review;
2. affected technical committees produce evidence;
3. at least one adversarial case against the stay is preserved;
4. Parliament reconsiders the exact motion/state;
5. Parliament may override with at least **75% of all active seats**.

If the override passes, the same exact stayed state may proceed subject to every other applicable technical/release/adoption rule.

## 6. No serial veto

H0 may not issue a second Guardian Stay against the same exact motion digest after the first stay expires or is overridden.

A materially equivalent proposal must preserve enough predecessor/history information for reviewers to identify an attempted cosmetic re-packaging of the same stayed action. The initial code in this PR enforces one stay per exact target; semantic-equivalence detection remains a governance/state-engine requirement before production activation.

## 7. Matters H0 may never stay

H0 may not use the Guardian Stay to block:

- a valid motion that ends H0's exceptional authority;
- the evidence-complete Public Transition motion defined by Document 56;
- a valid override of H0's own stay;
- H0's lawful removal from an active seat for inactivity, key failure, or due-process misconduct;
- an owner's individual decision to refuse an update or fork.

## 8. Sunset

The Guardian Stay exists only while the Founding/Expanding Parliament state explicitly records `h0_guardian_active = true`.

When the Public Transition becomes canonical:

`h0_guardian_active: true -> false`

This transition is one-way. No H0 instruction, ordinary parliamentary motion, emergency declaration, repository setting, or software release may restore the power.

H0 may retain the historical title and participate as an ordinary human under the public governance rules.

## 9. Key compromise and disappearance

The Guardian Stay MUST NOT depend forever on one irreplaceable device key. During its temporary life, H0 key rotation/recovery must be explicit and observable. Recovery must not create a second simultaneous H0.

If H0 disappears, dies, permanently loses the authorized lineage, or voluntarily renounces the role, the Guardian Stay lapses; it MUST NOT silently pass to heirs, a company, a foundation, an AI, an employer, or a private nominee.

Ordinary Parliament continues.

## 10. Value judgment

**PASS only as a temporary, public, overridable stay.**

A permanent Founder veto would make one person the ultimate owner of Mininet and would fail the anti-centralization purpose. The Guardian Stay is acceptable only because its scope is enumerated, its duration is capped, Parliament can override it, it cannot block its own sunset, and public maturation permanently destroys the exceptional power.
