# mini-update

Local, explicit update-adoption state machine (`docs/BOOTSTRAP_AND_UPDATE.md`
"Update adoption rule" + "No forced updates"): wraps `mini-forge`'s release
verification gates (`verify_governed_release`) with a tiny local record of
what a device is running and what it has chosen to do about a candidate.

**No forced updates \[FREEZE\].** Nothing here executes, fetches, or installs
anything. `AdoptionState::evaluate` is a pure query — "could this be adopted
right now" — and never mutates state. `AdoptionState::adopt` is the device
owner's explicit local act, and it always re-verifies from scratch rather
than trusting a stale decision, so nothing can "arm" an adoption ahead of
time and have it fire later unchecked. `AdoptionState::refuse` records a
refusal, which is a normal, first-class outcome — refusing never blocks
ordinary operation, and a refused release can still be adopted later by
simply calling `adopt` directly (it re-checks everything fresh; there is
deliberately no separate "unrefuse" step).

`AdoptionDecision` distinguishes **"not yet"** (timelock still running, too
few attestations so far — keep watching, no new facts needed to change the
answer) from **"rejected"** (a hard gate failed: governance fork, non-
canonical source commit, malformed object — re-evaluating won't help without
new facts) from **"refused"** (the device owner already said no).

```sh
cargo test -p mini-update
```

License: CC0-1.0 (public domain).
