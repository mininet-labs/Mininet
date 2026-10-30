# Parliament Activity and Compensation

**Status:** Proposed operational policy — not active merely by file presence

## 1. Principle

A Founding Parliament seat is a duty lease, not a pension, title, or passive salary claim.

Compensation exists because Stewards carry availability, review, emergency, key-custody, conflict-management, and governance burdens. Payment is for demonstrated service. It MUST NOT depend on vote direction, ideological alignment, invitation ancestry, popularity, balance, or seniority.

Council/Parliament service is intentionally not Mininet's highest-return economic path. A contributor who focuses entirely on engineering, cryptography, research, testing, storage, bandwidth, accessibility, or other productive work may earn substantially more than a Steward.

## 2. Duty period

Default duty period: **30 days**.

Each active seat receives an explicit duty plan at the start of the period. The plan may include:

- assigned committee work;
- dossiers requiring substantive review;
- eligible plenary votes;
- emergency/on-call assignments;
- security or continuity exercises;
- key-rotation/recovery exercises;
- conflict disclosures or recusal duties.

A duty plan MUST NOT require a particular vote result.

## 3. Duty Receipt

A payment claim requires a signed/verified `DutyReceipt` bound to the seat term and duty period.

Minimum default evidence:

- at least 70% of assigned committee duties completed;
- participation in at least 70% of eligible plenary votes;
- 100% response to specifically assigned emergency calls, unless a pre-recorded legitimate unavailability/recusal rule applies;
- at least one substantive review/dossier contribution in the period.

The executable reference kernel in `crates/mini-forge/src/parliament_policy.rs` implements the strict baseline without any field for vote direction.

## 4. Abstention and recusal

A properly recorded abstention counts as parliamentary participation.

A conflict recusal must not be punished as inactivity when the conflict record proves that non-participation was required. The production Forge object model therefore needs a bounded `recusal` or `excused_duty` evidence path before duty compensation becomes real value.

The reference kernel in this PR does not yet implement excused-duty objects; until that exists, the module is proposal/test code only.

## 5. Payment character

The Steward Duty Allowance SHOULD be:

- fixed or bounded per duty period;
- small relative to high-value technical contribution rewards;
- published before the period begins;
- identical for equivalent duty classes;
- independent of treasury voting outcomes;
- independent of how many invitations the Steward issues.

No percentage of treasury, protocol fees, invitation rewards, political campaign rewards, or vote bonuses are allowed.

## 6. Technical work by Stewards

A Steward may also contribute technical or research work.

That work is compensated under the same contribution rules as work by a non-Steward. Steward status provides no multiplier and no priority claim.

This separation is mandatory:

`duty allowance != contribution reward != human commons dividend`

## 7. Inactivity and vacancy

A Steward who fails the duty threshold:

1. receives no duty allowance for that period;
2. receives a public/private duty-failure record appropriate to the security context;
3. may enter a remediation period for transient failure;
4. loses the active seat if the term/absence rules require vacancy.

Loss of a political seat never confiscates already earned contribution rewards or lawful balances.

## 8. Anti-gaming

The system MUST reject:

- voting on meaningless motions solely to farm participation;
- self-assigned trivial reviews counted as substantive work;
- duplicate evidence reused across incompatible duty periods;
- emergency calls fabricated solely to trigger payment;
- inviter/invitee referral compensation;
- paying for YES/NO outcomes;
- retroactively changing the duty formula after observing votes.

Duty requirements should be set before the period and bind to exact Forge objects.

## 9. Treasury separation

No active Steward may unilaterally authorize their own payment.

The durable implementation SHOULD use deterministic duty rules plus an independently verifiable settlement path. Parliament may set future policy prospectively, but one active period must not rewrite its own already-earned threshold after the fact.

## 10. Values verdict

**PASS** when active service is required and payment is outcome-neutral.

**FAIL** if office possession itself becomes a salary entitlement or if payment can reward a particular political vote.

The long-term public Parliament should preserve the same duty-proof model so governance remains a service burden, not a professional entitlement class.
