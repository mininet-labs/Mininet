# Governance Consistency Matrix

**Status:** Normative cross-document review aid  
**Version:** 1.1

This matrix identifies where each cross-cutting rule is defined, operationalized, and ultimately enforced. A document reference is not enforcement; the final column must point to repository policy or code before the rule may be described as implemented.

| Rule | Primary governance source | Operational source | Current enforcement expectation |
|---|---|---|---|
| Canonical precedence | Doc 00 | Docs 03, RFC-0001 | `SPEC-00`, invariants, decision log, protected history |
| No forced updates | Docs 01, 07 | Docs 06, 13 | `mini-update`, explicit `OwnerApproval`, installer re-verification |
| Anonymous/pseudonymous contribution | Docs 01, 12 | Docs 05, 06 | Open proposal path; no compulsory legal identity |
| Compensation does not create voice | Docs 01, 12 | Docs 11, 13 | Treasury policy separated from governance membership |
| AI cannot satisfy human quorum | Docs 01, 04 | Docs 05, 06, 13, 50 | Branch rules plus Forge governance validation; `AGENTS.md` is procedural guidance only |
| AI engineering coordination is non-authorizing | Doc 50 | Doc 04; `AGENTS.md` adapter | Protected review routing and proposal evidence; specified and packaged, not activated |
| AI/model replacement preserves authority | Docs 43, 48, 50 | Docs 20, 50; `AGENTS.md` adapter | `GOV-AI-050-05` continuity scenario; not yet demonstrated |
| Exact-state approvals | Docs 02, 04 | Docs 05, 06 | Stale approval dismissal; digest-bound Forge approvals |
| Author not independent reviewer | Docs 02, 04 | Docs 05, 13 | Required independent approval checks |
| Combined integration evidence | Docs 02, 05 | Docs 06, 13 | Integration branch or merge queue plus full combined CI |
| Tests are not external audit | Docs 01, 03, 07 | Docs 05, 13 | Production gate for sensitive cryptography |
| GitHub is temporary | Docs 00, 01, 09 | Docs 06, 10, 13 | Forge outage drills and canonical-history parity |
| Founder authority is temporary | Docs 01, 08, 09, 43, 50 | Docs 10, 13 | Phase-gated removal of bypass and release authority; no AI inheritance |
| Free fork, no counterfeit continuity | Docs 01, 02 | Docs 06, RFC-0001 | Future fork/registry representation; currently a frozen requirement |
| Identity root is not personhood | Docs 00, 01, 14 | Docs 03, 11 | Product/docs checks; future personhood implementation |
| Release provenance required | Docs 02, 07 | Docs 06, 13 | Signed provenance bound to execution and release policy |
| Owner adoption remains local | Docs 01, 07 | Docs 10, 13 | No remote activation path; explicit local approval |
| Optional safety/moderation layers | Docs 01, 07 | Docs 06, 12 | Subscription/local-policy architecture, constrained by protocol truth |

## Review procedure

For every governance-affecting proposal:

1. Identify the matrix rows touched.
2. Cite the relevant frozen invariant or accepted decision.
3. State whether enforcement is structural, validation-based, procedural, or still pending.
4. Update this matrix when responsibility moves between GitHub, Forge, governance, or code.
5. Never mark a procedural rule as cryptographically enforced.
