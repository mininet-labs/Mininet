# Pre-Go-Live operating profile enforcement

This implementation follows canonical
`docs/governance/52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md` sections 2, 8–10.
It replaces D-0083's calendar/maintainer/production-candidate sunset in
the active GitHub operating profile. It does not declare Go-Live, close
an audit gate, change release checks, or make AI evidence canonical authority.

## State and transition

The v2 state explicitly records `go_live = false`. Dates, maintainer counts,
and production-candidate status cannot expire the Pre-Go-Live profile.
`forge_canonical = true` alone is not Go-Live.

Go-Live requires both `forge_canonical = true` and `go_live = true`, with
the bootstrap profile marked `sunset`. The GitHub adapter then fails closed:
the Founder integration path is disabled and subsequent canonical work
must use live Forge governance. Against an independently verified canonical
checkpoint, validation rejects clearing either activation bit after Go-Live,
restoring a v1 profile, or downgrading the canonical v2 operating profile.
This repository checker is an adapter, not the live Forge consensus engine.

D-0083 v1 records and their tests remain historical fixtures. The dependency
exception DEP-EX-0001 keeps its existing expiry and mitigation.

## Activation and evidence limits

Candidate-side CI tests the new checker. The existing canonical-governance
workflow intentionally executes the base branch's checker, which still
requires D-0083's v1 profile and expired calendar window. Therefore the first
migration proposal cannot repair that base evaluator from its own branch.
This is an explicit canonical integration dependency, not permission to run
candidate code as trusted canonical code or disable technical checks.
Human bootstrap integration under canonical decision 52 is needed to install
the updated adapter; future proposals then use it from their canonical base.

Pass the independently verified current `--canonical-root` in production
checks to enforce transition history; a candidate cannot be its own trust
anchor. Checks without a canonical checkpoint validate local state only
and do not establish historical irreversibility.

This slice does not finish section 10's audit schema/template migration,
gate indexes, reward identity hygiene, or the live Forge handoff workflow.
Substantive cryptography audit evidence and all ordinary technical/security
checks remain required. Anonymous participation does not assert verified
biography or institutional independence.
