#!/usr/bin/env python3
"""One-shot PR #285 truth sync.

This helper exists only to update large canonical documentation files from the
checked-out repository without round-tripping or truncating them through an API.
The automation removes this file and its temporary workflow before committing.
"""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DECISION_LOG = ROOT / "docs" / "DECISION_LOG.md"
STATUS = ROOT / "docs" / "STATUS.md"
DOCTRINE = ROOT / "docs" / "design" / "anti-collusion-content-settlement-preparation.md"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement target, found {count}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def append_decision() -> None:
    text = DECISION_LOG.read_text(encoding="utf-8")
    if "### D-0428 —" in text:
        return
    entry = r'''

### D-0428 — F5 Phase-2 settlement transcript, adversary/economic model, and falsification gates  ·  *Proposed*

**Date:** 2026-08-02 · **Refs:** D-0421 §7; D-0427; D-0417
(`mini-contribution`); D-0404 (`mini-attest` Tier 0); D-0047; roadmap
issues #18, #175, #228, and #229; `docs/design/
f5-phase2-settlement-model.md`; `tools/f5_phase2_model.py`.

**Decision:** adopt the deterministic, valueless F5 Phase-2 model and its
checked-in fixed report as the exact falsification baseline before any F5
production implementation. The model separates requester-funded,
sponsor-funded, and protocol-subsidized settlement; makes authority-bearing
settlement unconstructable; binds policy, service, epoch, event, transcript,
duplicate, and rate-limit domains; conserves payer/program value; and gives
audit evaluation no path to rewrite canonical finality. It deliberately records
rather than hides its decisive negative result: genuine-delivery colluders using
many roots and unique placeholder rate tags consume 100% of the bounded protocol
budget, exceeding the precommitted 10% loss gate. The generated authorization
therefore remains `false`.

**Reason:** Phase 0 required an executable model that distinguishes accounting
safety from anti-collusion. The model proves that finite budgets, replay checks,
and delivery challenges prevent unbounded issuance and fake non-delivery, but
also proves that those mechanisms cannot manufacture independent demand. Merging
a reproducible failed gate is safer and more scientifically honest than
weakening the gate or shipping a mechanism that calls real delivery
"collusion-resistant."

**Constitutional impact:** no frozen invariant is amended and no authority is
granted. This strengthens Directives 2, 4, 5, 9, 14, 16, and 18: ordinary
requester-funded settlement has no issuer/auditor gate; budgets are finite;
finality is not reversible; model outputs confer no ranking, personhood,
governance, validator, moderation, reviewer, or constitutional authority; and
no production cryptographic construction is selected.

**Implementation status:** Phase 2 only. One deterministic Python model, one
adversarial test suite, one exact-output test, and one checked-in JSONL vector.
No production crate, external dependency, wallet/chain behavior, credential,
nullifier, issuer set, auditor network, sponsor fund, protocol subsidy, or real
MINI activation exists. Accounting/replay/finality tests pass; the
colluding-extraction gate fails; issuer/auditor independence, unlinkability, and
physical weakest-device cost remain unmeasured.

**Failure point:** the model's `request_event_commitment` and `ScopedRateTag` are
scenario inputs. Different commitments can describe semantically identical work,
and fresh tag strings are not proof of scarce entitlement, unique humans, or
independent issuers. Threshold key counts can also be one operator behind several
keys. A production implementation that copies those placeholders, treats claim-ID
ordering as fair allocation, or equates challenge-valid delivery with independent
demand would fail the doctrine while appearing mechanically correct.

**Required follow-up:** Phase 3 is not automatically authorized. Any later PR
must be separately scoped and exact-state reviewed. A narrowly valueless
delivery-integrity prototype may proceed only if it says genuine colluders pass
and makes no anti-collusion or real-value claim. Any sponsor/protocol anti-
collusion or activation proposal must first define and externally review an
explicit scarcity assumption, preserve requester-funded permissionlessness,
meet D-0047, demonstrate operational independence rather than key count, rerun
the declared colluding set below the 10% gate, and benchmark the exact verifier
on the weakest supported device. F6 remains separate and unstarted.

**Supersedes / superseded by:** fulfills and supersedes D-0427's Phase-2
required follow-up only. It does not supersede D-0427's doctrine, D-0417's
requester-funded baseline, D-0404's linkable Tier-0 reviews, or D-0099. Any later
Phase-3-or-higher decision must cite this fixed report and its failed gate.
'''
    DECISION_LOG.write_text(text.rstrip() + entry + "\n", encoding="utf-8")


def update_status() -> None:
    old = '''F5/F6 (provider payments,
  private query transport) remain undesigned and unbuilt. F5's Phase-0
  doctrine now exists — `docs/design/
  anti-collusion-content-settlement-preparation.md` (D-0427) — but
  Phases 1-9 are not started; F6 has no doctrine document yet at all.'''
    new = '''F6 (private query transport) remains undesigned and unbuilt. F5 now
  has its Phase-0 doctrine (D-0427) plus a completed **deterministic,
  valueless Phase-2 falsification model** (D-0428) in `tools/
  f5_phase2_model.py`, with adversarial tests and a byte-exact checked-in
  JSONL result vector. Phase 1 is only the already-existing linkable
  D-0417/D-0404 baseline, not anti-collusion code. The Phase-2 accounting
  shell passes finite-budget conservation, exact retry/same-event replay,
  requester-funded issuer/auditor independence, cross-domain rejection,
  bounded state/input, and finality-isolation checks. Its decisive attack
  gate **fails**: 100 colluding requester/provider pairs with distinct
  roots/tags and genuine delivery drain 100% of a bounded protocol budget
  against a precommitted 10% ceiling. Issuer/auditor operational
  independence, cryptographic unlinkability, semantic event uniqueness,
  and physical weakest-device CPU/memory remain unmeasured. The generated
  report therefore sets `phase3_authorized` to `false`; no production
  settlement-integrity/delivery-challenge/audit crate, credential,
  nullifier, subsidy, or real-value activation is authorized. Phases 3-9
  remain unstarted; F6 still has no doctrine document. See `docs/design/
  f5-phase2-settlement-model.md` and `docs/design/
  federated-search-exchange-f1-f2.md`.'''
    replace_once(STATUS, old, new)


def update_doctrine() -> None:
    replace_once(
        DOCTRINE,
        "**Status:** Phase-0 doctrine and research preparation only. No\n",
        "**Status:** Phase-0 doctrine plus a completed, valueless Phase-2 falsification model (D-0428). No\n",
    )
    replace_once(
        DOCTRINE,
        '''No implementation phase is authorized by this document. Phase 1 predates it;
Phases 2-9 are unstarted.''',
        '''No implementation phase is authorized by this document. Phase 1 predates it.
Phase 2 is now complete under D-0428 as a deterministic Python model with a
checked-in fixed report; it moves no value and selects no production
construction. Its genuine-delivery collusion vector drains 100% of the bounded
protocol budget against a precommitted 10% loss gate, so the report sets
`phase3_authorized` to `false`. Phases 3-9 remain unstarted and unauthorized.''',
    )
    replace_once(
        DOCTRINE,
        '''Doctrine only. Zero implementation lines, zero new dependencies, zero new
crates, zero activation.''',
        '''Doctrine plus D-0428's non-production Phase-2 model: deterministic Python
accounting/transcript state, adversarial tests, and an exact JSONL result vector.
There are still zero production F5 crate lines, zero new dependencies, zero
selected cryptographic constructions, and zero activation.''',
    )
    replace_once(
        DOCTRINE,
        '''The next work is **not a nullifier crate**. It is Phase 2: a precise transcript,
settlement-class schema, adversary model, deterministic budget/attack simulator,
privacy budget, and numeric falsification thresholds. Coordinate with roadmap
#228/#229 so review- and settlement-context derivations cannot collide, and
with #18 without pretending #18 is solved. No F5 implementation PR should be
accepted before that Phase-2 package is reviewed.''',
        '''D-0428 completes Phase 2 and preserves its failed colluding-extraction gate.
The next work is still **not a nullifier crate** and not real-value activation.
A later, separately reviewed proposal may either narrow Phase 3 to a valueless
delivery-integrity prototype that explicitly admits genuine colluders pass, or
research an established scarcity construction/ allocation rule capable of
bringing the declared colluding set below the precommitted 10% loss ceiling.
Coordinate with roadmap #228/#229 so review- and settlement-context derivations
cannot collide, and with #18 without pretending #18 is solved. No production
anti-collusion or sponsor/protocol activation PR should be accepted while the
D-0428 authorization result remains false.''',
    )


def main() -> None:
    append_decision()
    update_status()
    update_doctrine()
    subprocess.run(
        ["python3", str(ROOT / "tools" / "mininet_nav.py"), "build"],
        cwd=ROOT,
        check=True,
    )


if __name__ == "__main__":
    main()
