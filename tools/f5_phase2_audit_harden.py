#!/usr/bin/env python3
"""One-shot adaptive-audit hardening for PR #285.

This script patches the Phase-2 model and truth-sync documents, regenerates the
fixed report, runs the focused tests, removes itself and its temporary workflow,
and rebuilds navigation against the exact resulting tree.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MODEL = ROOT / "tools" / "f5_phase2_model.py"
TESTS = ROOT / "tools" / "test_f5_phase2_model.py"
FIXTURE = ROOT / "tools" / "fixtures" / "f5_phase2_report.jsonl"
MODEL_DOC = ROOT / "docs" / "design" / "f5-phase2-settlement-model.md"
DOCTRINE = ROOT / "docs" / "design" / "anti-collusion-content-settlement-preparation.md"
STATUS = ROOT / "docs" / "STATUS.md"
DECISIONS = ROOT / "docs" / "DECISION_LOG.md"
WORKFLOW = ROOT / ".github" / "workflows" / "f5-phase2-audit-harden.yml"
SELF = Path(__file__)


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one target, found {count}: {old[:80]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def replace_count(path: Path, old: str, new: str, expected: int) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != expected:
        raise SystemExit(
            f"{path}: expected {expected} targets, found {count}: {old[:80]!r}"
        )
    path.write_text(text.replace(old, new), encoding="utf-8")


def patch_model() -> None:
    replace_count(
        MODEL,
        'model_commitment("delivery-evidence", base)',
        'model_commitment(EVIDENCE_DOMAIN, base)',
        2,
    )
    replace_once(
        MODEL,
        "        self.attempted_volume_units += claim.amount_units\n",
        "        self.attempted_volume_units += max(claim.amount_units, 0)\n",
    )
    replace_once(
        MODEL,
        '''def audit_detection_probability_bps(
    sample_bps: int,
    objectively_invalid_claims: int,
) -> int:''',
        '''def grind_unsampled_claim_id(
    public_seed: str,
    candidate_prefix: str,
    sample_bps: int,
    *,
    max_attempts: int = 100_000,
) -> tuple[str, int]:
    """Find a claim-id candidate excluded by a known deterministic sample.

    This is an attack model, not a production API. If the realized sampling
    seed is known while a claimant can vary claim-committed data, deterministic
    public sampling is grindable. The rule/source may be precommitted, but the
    realized entropy must remain unpredictable until claims are immutable.
    """

    _validate_identifier(candidate_prefix, "audit grinding prefix")
    if max_attempts <= 0:
        raise ValueError("audit grinding attempt bound must be positive")
    for nonce in range(max_attempts):
        candidate = model_commitment(
            "audit-grind-candidate",
            {"prefix": candidate_prefix, "nonce": nonce},
        )
        if not audit_selected(public_seed, candidate, sample_bps):
            return candidate, nonce + 1
    raise RuntimeError("unable to find an unsampled candidate within the bound")


def audit_detection_probability_bps(
    sample_bps: int,
    objectively_invalid_claims: int,
) -> int:''',
    )
    replace_once(
        MODEL,
        '''    detection_bps = audit_detection_probability_bps(
        collusion_policy.audit_sample_bps,
        thresholds.audit_attack_claim_count,
    )
    max_retained = max(''',
        '''    detection_bps = audit_detection_probability_bps(
        collusion_policy.audit_sample_bps,
        thresholds.audit_attack_claim_count,
    )
    known_audit_seed = "known-before-claim-construction"
    grinding_results = [
        grind_unsampled_claim_id(
            known_audit_seed,
            f"adaptive-claim-{index}",
            collusion_policy.audit_sample_bps,
        )
        for index in range(thresholds.audit_attack_claim_count)
    ]
    grinding_selected = sum(
        audit_selected(
            known_audit_seed,
            claim_id,
            collusion_policy.audit_sample_bps,
        )
        for claim_id, _ in grinding_results
    )
    grinding_detection_bps = (
        10_000 if grinding_selected > 0 else 0
    )
    grinding_max_attempts = max(attempts for _, attempts in grinding_results)
    vectors.append(
        VectorResult(
            vector="known-audit-randomness-can-be-ground-away",
            status=(
                GateStatus.PASS
                if grinding_detection_bps >= thresholds.min_audit_detection_bps
                else GateStatus.FAIL
            ),
            accepted=len(grinding_results),
            already_accepted=0,
            rejected=0,
            spent_units=0,
            extraction_units=0,
            detail=(
                "with the realized seed known before claim construction, "
                f"all {len(grinding_results)} submitted claim ids avoid the "
                f"5% sample; maximum search was {grinding_max_attempts} candidates"
            ),
            state_digest=model_commitment(
                "audit-grinding-vector",
                grinding_results,
            ),
        )
    )
    max_retained = max(''',
    )
    replace_once(
        MODEL,
        '''        GateResult(
            gate="honest-false-rejection-rate",
            status=GateStatus.PASS,
            threshold=thresholds.max_honest_false_rejection_bps,
            observed=honest_rejections * 10_000 // honest_claims,
            unit="basis-points",
            detail="the declared honest requester-funded vector is accepted",
        ),''',
        '''        GateResult(
            gate="honest-false-rejection-rate",
            status=GateStatus.PARTIAL,
            threshold=thresholds.max_honest_false_rejection_bps,
            observed=honest_rejections * 10_000 // honest_claims,
            unit="basis-points",
            detail="one structural honest vector passes, but one sample cannot establish a 1% population rate",
        ),''',
    )
    replace_once(
        MODEL,
        '''            detail="5% precommitted sampling detects at least one of 60 invalid claims with >=95% probability",
        ),
        GateResult(
            gate="issuer-concentration",''',
        '''            detail="5% sampling reaches >=95% detection only when claim ids are fixed before realized randomness is revealed",
        ),
        GateResult(
            gate="audit-randomness-grinding-resistance",
            status=(
                GateStatus.PASS
                if grinding_detection_bps >= thresholds.min_audit_detection_bps
                else GateStatus.FAIL
            ),
            threshold=thresholds.min_audit_detection_bps,
            observed=grinding_detection_bps,
            unit="basis-points-observed-adaptive-campaign-detection",
            detail=(
                "FAIL: a known realized seed plus claimant-controlled ids lets "
                "selective submission avoid every sampled target"
            ),
        ),
        GateResult(
            gate="issuer-concentration",''',
    )


def patch_tests() -> None:
    replace_once(
        TESTS,
        '''    def test_transcript_expiry_is_enforced(self) -> None:
        self.assertFalse(
            self.transcript.verify_for(self.policy, self.claim, now_ms=2_001)
        )
''',
        '''    def test_evidence_commitment_uses_the_declared_evidence_domain(self) -> None:
        base = {
            "version": self.transcript.version,
            "domain": self.transcript.domain,
            "policy_commitment": self.transcript.policy_commitment,
            "settlement_class": self.transcript.settlement_class.value,
            "service_class": self.transcript.service_class.value,
            "request_event_commitment": self.transcript.request_event_commitment,
            "requester_scope": self.transcript.requester_scope,
            "provider_scope": self.transcript.provider_scope,
            "challenge": self.transcript.challenge,
            "response_commitment": self.transcript.response_commitment,
            "issued_at_ms": self.transcript.issued_at_ms,
            "expires_at_ms": self.transcript.expires_at_ms,
        }
        self.assertEqual(
            self.transcript.evidence_commitment,
            MODEL.model_commitment(MODEL.EVIDENCE_DOMAIN, base),
        )
        self.assertNotEqual(
            self.transcript.evidence_commitment,
            MODEL.model_commitment("delivery-evidence", base),
        )

    def test_transcript_expiry_is_enforced(self) -> None:
        self.assertFalse(
            self.transcript.verify_for(self.policy, self.claim, now_ms=2_001)
        )
''',
    )
    replace_once(
        TESTS,
        '''    def test_sponsor_budget_cannot_go_negative(self) -> None:
        policy = MODEL.make_policy(''',
        '''    def test_invalid_negative_amount_cannot_reduce_attempted_volume(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.REQUESTER_FUNDED,
            "negative-attempt",
            budget=0,
        )
        model = MODEL.SettlementModel(payer_balances={"payer": 10})
        model.register_policy(policy)
        claim, transcript = MODEL.make_claim(
            policy,
            event="negative-attempt-event",
            requester="payer",
            funder="payer",
            provider="provider",
            amount=1,
            rate_tag=None,
        )
        malformed = replace(claim, amount_units=-10)
        malformed = replace(malformed, claim_id=malformed.expected_claim_id())
        outcome = model.submit(
            malformed,
            transcript,
            availability=MODEL.Availability(0, 0),
            now_ms=1_000,
        )
        self.assertEqual(outcome.code, MODEL.OutcomeCode.AMOUNT_INVALID)
        self.assertEqual(model.attempted_volume_units, 0)
        self.assertEqual(model.payer_balances["payer"], 10)

    def test_sponsor_budget_cannot_go_negative(self) -> None:
        policy = MODEL.make_policy(''',
    )
    replace_once(
        TESTS,
        '''    def test_audit_sampling_is_public_and_deterministic(self) -> None:
        first = [''',
        '''    def test_known_realized_audit_seed_is_grindable(self) -> None:
        seed = "known-before-claim-construction"
        results = [
            MODEL.grind_unsampled_claim_id(seed, f"claim-{index}", 500)
            for index in range(60)
        ]
        self.assertTrue(
            all(not MODEL.audit_selected(seed, claim_id, 500) for claim_id, _ in results)
        )
        self.assertLess(max(attempts for _, attempts in results), 100)

    def test_audit_sampling_is_public_and_deterministic(self) -> None:
        first = [''',
    )
    replace_once(
        TESTS,
        '''        self.assertEqual(gates["maximum-colluding-extraction"]["status"], "FAIL")
        self.assertEqual(gates["issuer-concentration"]["status"], "PARTIAL")
        self.assertEqual(gates["weak-device-verification-cpu"]["status"], "PARTIAL")''',
        '''        self.assertEqual(gates["maximum-colluding-extraction"]["status"], "FAIL")
        self.assertEqual(gates["audit-randomness-grinding-resistance"]["status"], "FAIL")
        self.assertEqual(gates["honest-false-rejection-rate"]["status"], "PARTIAL")
        self.assertEqual(gates["issuer-concentration"]["status"], "PARTIAL")
        self.assertEqual(gates["weak-device-verification-cpu"]["status"], "PARTIAL")''',
    )


def patch_model_doc() -> None:
    replace_once(
        MODEL_DOC,
        '''| Collusion resistance | **FAIL** | One hundred attacker-controlled requester/provider pairs with unique roots/tags perform real delivery and drain 100% of a bounded protocol budget. The gate permits at most 10%. |
| Issuer/auditor independence | **PARTIAL** |''',
        '''| Collusion resistance | **FAIL** | One hundred attacker-controlled requester/provider pairs with unique roots/tags perform real delivery and drain 100% of a bounded protocol budget. The gate permits at most 10%. |
| Adaptive audit sampling | **FAIL** | If the realized sampling seed is known while claim-committed inputs remain variable, attackers grind 60 claim IDs so every submitted claim avoids the 5% sample. |
| Issuer/auditor independence | **PARTIAL** |''',
    )
    replace_once(
        MODEL_DOC,
        '''replay, and central permission over voluntary payments, but the design does not
yet resist economically valid collusion. Therefore the generated report sets''',
        '''replay, and central permission over voluntary payments, but the design does not
yet resist economically valid collusion or adaptive sampling grind. Therefore
the generated report sets''',
    )
    replace_once(
        MODEL_DOC,
        '''- `mininet/f5/delivery-evidence/model-v1`;
- `mininet/f5/settlement-duplicate/v1/<policy-name>`;''',
        '''- `mininet/f5/delivery-evidence/model-v1` (the actual evidence-commitment label, not a dead documentation constant);
- `mininet/f5/settlement-duplicate/v1/<policy-name>`;''',
    )
    replace_once(
        MODEL_DOC,
        '''3. `Availability` is a count. Several available keys may still be controlled by
   one organization or one machine.

Those assumptions are''',
        '''3. `Availability` is a count. Several available keys may still be controlled by
   one organization or one machine.
4. Ideal audit probability applies only when claim IDs are immutable before the
   realized sampling entropy is knowable. A known seed plus variable claim data
   is explicitly modeled as grindable.

Those assumptions are''',
    )
    old_audit = '''## 11. Audit semantics

Audit sampling is deterministic from a precommitted public seed, claim ID, and
policy sample rate. The model uses a 5% rate. For 60 independently sampleable,
objectively invalid claims, the modeled probability of sampling at least one is
9,539 basis points, above the 9,500-basis-point gate.

That PASS does **not** apply to genuine-delivery collusion. A colluding claim
that satisfies every objective transcript/accounting predicate is not an
objectively invalid claim, so more sampling does not reveal its social intent.
This is why the audit gate passes while the colluding-extraction gate fails.

`evaluate_audit_allegation` accepts either a heuristic allegation or an
objective transcript-failure allegation. It has no mutable settlement state.
A heuristic or false allegation causes no action. A valid objective proof may
request a future-program halt, but cannot reverse canonical ownership,
confiscate unrelated funds, alter personhood, or create a blacklist.
'''
    new_audit = '''## 11. Audit semantics

The sampling **rule and randomness source commitment** must be public before
claims. The realized entropy must not be knowable or selectively biasable until
the claim set is immutable. No auditor may privately choose targets.

The model uses a 5% rate. If 60 objectively invalid claim IDs are already fixed
before entropy is revealed and samples behave independently, the modeled
probability of sampling at least one is 9,539 basis points, above the
9,500-basis-point gate.

The adaptive vector then gives the attacker the realized seed while allowing it
to vary claim-committed inputs. `grind_unsampled_claim_id` finds an unsampled ID
for every one of 60 submitted claims. Observed campaign detection is therefore
0 basis points: **FAIL**. A preannounced static seed is not delayed random audit.
A future design needs a decentralized, independently verifiable source whose
realized value appears only after claims are irrevocably committed, plus explicit
bias, withholding, fork, and liveness analysis. No single beacon operator may
become the target-selection authority.

Even an ungrindable sample does **not** detect genuine-delivery collusion. A
colluding claim that satisfies every objective transcript/accounting predicate
is not objectively invalid, so more sampling does not reveal its social intent.

`evaluate_audit_allegation` accepts either a heuristic allegation or an
objective transcript-failure allegation. It has no mutable settlement state.
A heuristic or false allegation causes no action. A valid objective proof may
request a future-program halt, but cannot reverse canonical ownership,
confiscate unrelated funds, alter personhood, or create a blacklist.
'''
    replace_once(MODEL_DOC, old_audit, new_audit)
    replace_once(
        MODEL_DOC,
        '''| Honest false rejection | at most 100 bps | A future measured system must not protect a subsidy by broadly excluding honest participants. The current fixed honest set is small and therefore only a structural check. |
| Cross-context leakage score | 0 |''',
        '''| Honest false rejection | at most 100 bps | A future measured system must not protect a subsidy by broadly excluding honest participants. One fixed honest vector is only a **PARTIAL** structural check, not a population estimate. |
| Cross-context leakage score | 0 |''',
    )
    replace_once(
        MODEL_DOC,
        '''| Audit detection | at least 9,500 bps for 60 objectively invalid claims | A large repeated objective fraud campaign should be detected with at least 95% probability under the declared sample rule. |
| Compromised issuers |''',
        '''| Ideal fixed-ID audit detection | at least 9,500 bps for 60 objectively invalid claims | A large repeated objective fraud campaign should be detected with at least 95% probability only when IDs are fixed before entropy is revealed. |
| Adaptive audit-grinding resistance | at least 9,500 bps campaign detection | The same floor must hold when claimants can vary claim-committed inputs; a known realized seed fails this gate. |
| Compromised issuers |''',
    )
    replace_once(
        MODEL_DOC,
        '''| False or heuristic audit allegation | **PASS** | No canonical state mutation. |
| Root-DID/raw-query privacy declaration |''',
        '''| False or heuristic audit allegation | **PASS** | No canonical state mutation. |
| Known realized audit seed with variable claim IDs | **FAIL** | All 60 submitted claims are ground until they avoid the 5% sample. |
| Root-DID/raw-query privacy declaration |''',
    )
    replace_once(
        MODEL_DOC,
        '''- honest false rejection in the declared vector: **PASS**, observed `0 bps`;
- cross-context leakage score:''',
        '''- honest false rejection in the declared vector: **PARTIAL**, observed `0 bps`
  from only one structural honest case;
- cross-context leakage score:''',
    )
    replace_once(
        MODEL_DOC,
        '''- audit detection for 60 objectively invalid claims: **PASS**, observed
  `9,539 bps`;
- issuer concentration:''',
        '''- ideal fixed-ID audit detection for 60 objectively invalid claims: **PASS**,
  observed `9,539 bps`;
- adaptive audit-grinding resistance: **FAIL**, observed `0 bps` when the
  realized seed is known during claim construction;
- issuer concentration:''',
    )
    replace_once(
        MODEL_DOC,
        '''### 14.2 Economic-event equivalence is caller supplied — **PARTIAL**''',
        '''### 14.2 Known audit randomness is grindable — **FAIL**

**Location:** `audit_selected`, `grind_unsampled_claim_id`, and fixed vector
`known-audit-randomness-can-be-ground-away`.

**Failure:** a deterministic public sample is not manipulation-resistant if its
realized seed is known while claimants can vary event, nonce, or other
claim-committed inputs. The attacker submits only IDs that hash outside the
sample; all 60 modeled claims evade audit.

**Long-term solution:** precommit the rule and source, not the realized value.
Reveal decentralized, independently verifiable entropy only after the claim set
is immutable. Analyze contributor bias, withholding, forks, fallback, and
liveness; never give one beacon operator or auditor target-selection power.

### 14.3 Economic-event equivalence is caller supplied — **PARTIAL**''',
    )
    for old, new in (
        ("### 14.3 Distinct policies", "### 14.4 Distinct policies"),
        ("### 14.4 Rate-limit scarcity", "### 14.5 Rate-limit scarcity"),
        ("### 14.5 Deterministic claim-ID ordering", "### 14.6 Deterministic claim-ID ordering"),
        ("### 14.6 Threshold counts", "### 14.7 Threshold counts"),
        ("### 14.7 Privacy is declared", "### 14.8 Privacy is declared"),
        ("### 14.8 Retained-state exhaustion", "### 14.9 Retained-state exhaustion"),
        ("### 14.9 Physical weak-device cost", "### 14.10 Physical weak-device cost"),
    ):
        replace_once(MODEL_DOC, old, new)
    replace_once(
        MODEL_DOC,
        '''- audit evaluation cannot rewrite finality; and
- the full report is reproducible byte-for-byte.''',
        '''- audit evaluation cannot rewrite finality; and
- the full report is reproducible byte-for-byte.''',
    )
    replace_once(
        MODEL_DOC,
        '''- fair scarce-budget allocation;
- physical performance;''',
        '''- fair scarce-budget allocation;
- unpredictable, unbiasable, and live audit randomness;
- physical performance;''',
    )
    replace_once(
        MODEL_DOC,
        '''4. state that real colluders can pass;
5. introduce no requester-funded permission gate;''',
        '''4. state that real colluders can pass;
5. treat audit sampling as unsafe unless claims are immutable before realized
   entropy is known;
6. introduce no requester-funded permission gate;''',
    )
    replace_once(
        MODEL_DOC,
        '''6. select no real-value activation path; and
7. receive independent exact-head human review.''',
        '''7. select no real-value activation path; and
8. receive independent exact-head human review.''',
    )
    replace_once(
        MODEL_DOC,
        '''drain is acceptable merely because no overrun occurred.

It also fails if''',
        '''drain is acceptable merely because no overrun occurred, or that a known
sampling seed remains safe while claim IDs are attacker-variable.

It also fails if''',
    )


def patch_doctrine() -> None:
    replace_once(
        DOCTRINE,
        '''   sampling and objective fraud-proof rules. Sampling randomness comes from a
   precommitted public source; no auditor chooses targets privately. Heuristic''',
        '''   sampling and objective fraud-proof rules. The rule and randomness-source
   commitment are fixed publicly before claims, but realized entropy must remain
   unpredictable and unbiasable until claims are immutable; no auditor or beacon
   operator chooses targets privately. Heuristic''',
    )
    replace_once(
        DOCTRINE,
        '''- **Delayed, randomized, privacy-bounded audit.** Sampling rules and randomness
  are public and fixed before the sampled claims exist. Audit proves only
  declared transcript/accounting predicates.''',
        '''- **Delayed, randomized, privacy-bounded audit.** Sampling rules and the
  randomness-source commitment are public before claims, while realized entropy
  is unavailable until the claim set is immutable. A known seed with variable
  claim inputs is grindable and fails closed. Audit proves only declared
  transcript/accounting predicates.''',
    )
    replace_once(
        DOCTRINE,
        '''protocol budget against a precommitted 10% loss gate, and cross-policy semantic
overlap remains unmeasured without a global activity registry, so the report
sets `phase3_authorized` to `false`.''',
        '''protocol budget against a precommitted 10% loss gate. A second attack gives
claimants the realized audit seed before claim construction; all 60 submitted
claims grind outside the 5% sample. Cross-policy semantic overlap also remains
unmeasured without a global activity registry, so the report sets
`phase3_authorized` to `false`.''',
    )
    replace_once(
        DOCTRINE,
        '''- continues operating a subsidy program after its objective cap/audit safety
  assumptions fail.

A doctrine-only file can also drift. The first Phase-2 proposal must re-check
all factual references against then-current code and update this file and
`docs/STATUS.md` in the same proposal.''',
        '''- continues operating a subsidy program after its objective cap/audit safety
  assumptions fail; or
- reveals target-selection entropy while claimants can still grind claim IDs.

This doctrine can drift. Every later F5 proposal must re-check its factual
references against then-current code and truth-sync this file and
`docs/STATUS.md` in the same proposal.''',
    )
    replace_once(
        DOCTRINE,
        '''research an established scarcity construction and privacy-preserving policy-
family allocation rule capable of bringing the declared colluding set below the
precommitted 10% loss ceiling.''',
        '''research an established scarcity construction, privacy-preserving policy-family
allocation rule, and delayed decentralized randomness construction capable of
bringing the declared colluding set below the precommitted 10% loss ceiling and
preventing adaptive sample grinding.''',
    )


def patch_status() -> None:
    replace_once(
        STATUS,
        '''  of a bounded protocol budget against a precommitted 10% ceiling. A
  second vector remains **partial** by design: two independently committed''',
        '''  of a bounded protocol budget against a precommitted 10% ceiling. A
  second gate **fails**: when the realized audit seed is known while claim
  inputs remain variable, all 60 submitted IDs grind outside the 5% sample.
  A separate vector remains **partial** by design: two independently committed''',
    )


def patch_decisions() -> None:
    replace_once(
        DECISIONS,
        '''colluders using many roots and unique placeholder rate tags consume 100% of the
bounded protocol budget, exceeding the precommitted 10% loss gate. The generated
authorization therefore remains `false`.''',
        '''colluders using many roots and unique placeholder rate tags consume 100% of the
bounded protocol budget, exceeding the precommitted 10% loss gate. Separately,
a known realized audit seed lets adaptive claimants grind all 60 submitted IDs
outside the 5% sample. The generated authorization therefore remains `false`.''',
    )
    replace_once(
        DECISIONS,
        '''colluding-extraction gate fails; cross-policy semantic overlap,
issuer/auditor independence, unlinkability, and physical weakest-device cost
remain unmeasured.''',
        '''colluding-extraction and adaptive-audit-grinding gates fail; cross-policy
semantic overlap, issuer/auditor independence, unlinkability, and physical
weakest-device cost remain unmeasured.''',
    )
    replace_once(
        DECISIONS,
        '''precommit a privacy-preserving policy-family overlap rule. Threshold key counts
can still be one operator behind several keys.''',
        '''precommit a privacy-preserving policy-family overlap rule. A deterministic
sample is also grindable if its realized seed is known before claims are
immutable. Threshold key counts can still be one operator behind several keys.''',
    )
    replace_once(
        DECISIONS,
        '''explicit scarcity assumption and any policy-family overlap rule, preserve
requester-funded permissionlessness, meet D-0047, demonstrate operational
independence rather than key count, rerun the declared colluding set below the
10% gate, and benchmark the exact verifier on the weakest supported device.''',
        '''explicit scarcity assumption, policy-family overlap rule, and decentralized
delayed-randomness construction, preserve requester-funded permissionlessness,
meet D-0047, demonstrate operational independence rather than key count, rerun
the declared colluding set below the 10% gate, prevent adaptive sample grinding,
and benchmark the exact verifier on the weakest supported device.''',
    )


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def main() -> None:
    patch_model()
    patch_tests()
    patch_model_doc()
    patch_doctrine()
    patch_status()
    patch_decisions()

    report = subprocess.run(
        [sys.executable, str(MODEL)],
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout
    FIXTURE.write_text(report, encoding="utf-8")

    run(sys.executable, "-m", "unittest", "tools/test_f5_phase2_model.py")
    run(sys.executable, "-m", "unittest", "tools/test_f5_phase2_vectors.py")

    WORKFLOW.unlink()
    SELF.unlink()
    run(sys.executable, "tools/mininet_nav.py", "build")


if __name__ == "__main__":
    main()
