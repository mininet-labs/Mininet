# Forge-native Founding Parliament objects

These schemas pre-stage the proposed Founding Parliament in the same artifact vocabulary Forge will eventually use. They are **non-authorizing** until an exact-state activation decision and canonical Forge state-resolution code say otherwise.

Object payload schemas in this PR:

- `parliament-invitation.schema.json` — single-use candidacy invitation capability; invitation is not a seat or vote.
- `parliament-seat-term.schema.json` — rolling active-duty seat term and committee assignment.
- `parliament-motion.schema.json` — exact-state plenary motion and evidence/report references.
- `parliament-vote.schema.json` — one ballot bound to one active seat/term and exact motion state.
- `parliament-emergency-order.schema.json` — P0/P1 warning, release quarantine, or workaround only.
- `parliament-duty-receipt.schema.json` — activity evidence for duty compensation, with no vote-direction field.
- `h0-guardian-stay.schema.json` — H0's bounded temporary constitutional stay evidence.
- `parliament-transition.schema.json` — monotonic seat/public-eligibility/H0-authority transition record.

All eventual signed instances SHOULD be wrapped in the existing `governance-object.schema.json` envelope or its canonical successor.

## Missing before activation

Schemas are vocabulary, not authorization. Production activation still requires:

- canonical object-type registration;
- strict Rust parsers/builders and object-link validation;
- active-seat resolution and duplicate-vote rejection across replicated state;
- invitation rolling-window accounting and single-use consumption;
- richer signed expansion/public-transition evidence rather than boolean claims;
- recusal/excused-duty semantics;
- conflict/equivocation handling;
- deterministic parliamentary result resolution under network forks;
- integration with release eligibility without creating forced adoption;
- independently replicated duty-compensation authorization;
- one-way public-transition state enforcement.

Until those exist, GitHub examples or schema-conforming JSON do not create a legitimate Parliament act.
