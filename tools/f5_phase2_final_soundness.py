#!/usr/bin/env python3
"""One-shot final soundness correction for PR #285.

The script closes three review findings before the merge candidate is exposed:

* claim schema versions must be rejected explicitly;
* a recomputed transcript issued before policy start must fail verification; and
* bounded-work gates must measure configured policy capacity, not only the
  small fixture state that happened to be reached.

It regenerates the exact vector, runs focused tests, truth-syncs doctrine/status,
removes itself and its temporary workflow, and rebuilds navigation.
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
WORKFLOW = ROOT / ".github" / "workflows" / "f5-phase2-final-soundness.yml"
SELF = Path(__file__)


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one target, found {count}: {old[:100]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def patch_model() -> None:
    replace_once(
        MODEL,
        '    POLICY_UNKNOWN = "policy-unknown"\n    AUTHORITY_CLASS_FORBIDDEN',
        '    POLICY_UNKNOWN = "policy-unknown"\n'
        '    UNSUPPORTED_VERSION = "unsupported-version"\n'
        '    AUTHORITY_CLASS_FORBIDDEN',
    )
    replace_once(
        MODEL,
        '''        if self.version != MODEL_VERSION or self.domain != DELIVERY_DOMAIN:
            return False
        if self.policy_commitment != policy.commitment:''',
        '''        if self.version != MODEL_VERSION or self.domain != DELIVERY_DOMAIN:
            return False
        if self.issued_at_ms < policy.starts_at_ms:
            return False
        if (
            self.expires_at_ms <= self.issued_at_ms
            or self.expires_at_ms > policy.expires_at_ms
        ):
            return False
        if self.policy_commitment != policy.commitment:''',
    )
    replace_once(
        MODEL,
        '''        policy = self.policies.get(claim.policy_commitment)
        if policy is None:
            return self._reject(claim, OutcomeCode.POLICY_UNKNOWN)
        if policy.settlement_class is SettlementClass.AUTHORITY_BEARING:''',
        '''        policy = self.policies.get(claim.policy_commitment)
        if policy is None:
            return self._reject(claim, OutcomeCode.POLICY_UNKNOWN)
        if claim.version != MODEL_VERSION:
            return self._reject(claim, OutcomeCode.UNSUPPORTED_VERSION)
        if policy.settlement_class is SettlementClass.AUTHORITY_BEARING:''',
    )
    old_metrics = '''    max_wire = max(
        claim.wire_size_bytes + transcript.wire_size_bytes
        for claim, transcript in (
            (requester_claim, requester_tx),
            (sponsor_items[0][0], sponsor_items[0][1]),
            (collusion_items[0][0], collusion_items[0][1]),
        )
    )
    max_ops = max(
        claim.abstract_verification_ops + transcript.abstract_verification_ops
        for claim, transcript in (
            (requester_claim, requester_tx),
            (sponsor_items[0][0], sponsor_items[0][1]),
            (collusion_items[0][0], collusion_items[0][1]),
        )
    )'''
    new_metrics = '''    modeled_policies = (
        requester_policy,
        sponsor_policy,
        protocol_policy,
        wrong_epoch_policy,
        overlap_policy_a,
        overlap_policy_b,
        collusion_policy,
        retained_policy,
    )
    max_fixture_wire = max(
        claim.wire_size_bytes + transcript.wire_size_bytes
        for claim, transcript in (
            (requester_claim, requester_tx),
            (sponsor_items[0][0], sponsor_items[0][1]),
            (collusion_items[0][0], collusion_items[0][1]),
        )
    )
    max_configured_wire = max(
        policy.max_claim_proof_wire_bytes for policy in modeled_policies
    )
    max_fixture_ops = max(
        claim.abstract_verification_ops + transcript.abstract_verification_ops
        for claim, transcript in (
            (requester_claim, requester_tx),
            (sponsor_items[0][0], sponsor_items[0][1]),
            (collusion_items[0][0], collusion_items[0][1]),
        )
    )
    max_configured_ops = max(
        policy.max_abstract_verification_ops for policy in modeled_policies
    )'''
    replace_once(MODEL, old_metrics, new_metrics)
    replace_once(
        MODEL,
        '''    max_retained = max(
        sponsor_model.retained_state_bytes(sponsor_policy.commitment),
        collusion_model.retained_state_bytes(collusion_policy.commitment),
    )''',
        '''    max_observed_retained = max(
        sponsor_model.retained_state_bytes(sponsor_policy.commitment),
        collusion_model.retained_state_bytes(collusion_policy.commitment),
    )
    max_configured_retained = max(
        policy.max_retained_keys * SettlementModel.RETAINED_KEY_ESTIMATE_BYTES
        for policy in modeled_policies
    )''',
    )
    replace_once(
        MODEL,
        '''            detail=f"abstract model work is bounded at {max_ops} operations; physical CPU remains a later benchmark gate",''',
        '''            detail=(
                f"configured abstract-work cap is {max_configured_ops}; "
                f"largest fixture used {max_fixture_ops}; physical CPU remains unmeasured"
            ),''',
    )
    replace_once(
        MODEL,
        '''                if max_retained <= thresholds.max_retained_state_bytes
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_retained_state_bytes,
            observed=max_retained,
            unit="estimated-bytes",
            detail="the model retains only bounded event/rate keys and fails closed before eviction",''',
        '''                if max_configured_retained <= thresholds.max_retained_state_bytes
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_retained_state_bytes,
            observed=max_configured_retained,
            unit="configured-estimated-bytes",
            detail=(
                f"configured policy capacity is measured, not only the "
                f"{max_observed_retained}-byte fixture state; eviction still fails closed"
            ),''',
    )
    replace_once(
        MODEL,
        '''                if max_wire <= thresholds.max_claim_proof_wire_bytes
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_claim_proof_wire_bytes,
            observed=max_wire,
            unit="bytes",
            detail="canonical model claim plus transcript stays under the precommitted 16 KiB cap",''',
        '''                if max_configured_wire <= thresholds.max_claim_proof_wire_bytes
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_claim_proof_wire_bytes,
            observed=max_configured_wire,
            unit="configured-bytes",
            detail=(
                f"policy cap is compared directly with the 16 KiB gate; "
                f"largest fixture used {max_fixture_wire} bytes"
            ),''',
    )
    replace_once(
        MODEL,
        '''                if max_ops <= thresholds.max_abstract_verification_ops
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_abstract_verification_ops,
            observed=max_ops,
            unit="model-operations",
            detail="bounded parse/binding accounting; not a substitute for the physical CPU gate",''',
        '''                if max_configured_ops <= thresholds.max_abstract_verification_ops
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_abstract_verification_ops,
            observed=max_configured_ops,
            unit="configured-model-operations",
            detail=(
                f"policy cap is compared directly with the gate; largest fixture "
                f"used {max_fixture_ops}; this is not a physical CPU measurement"
            ),''',
    )
    replace_once(
        MODEL,
        '    all_pass = all(gate.status is GateStatus.PASS for gate in gates)',
        '''    all_pass = all(
        gate.status is GateStatus.PASS for gate in gates
    ) and all(vector.status is GateStatus.PASS for vector in vectors)''',
    )


def patch_tests() -> None:
    replace_once(
        TESTS,
        '''    def test_transcript_expiry_is_enforced(self) -> None:
        self.assertFalse(
            self.transcript.verify_for(self.policy, self.claim, now_ms=2_001)
        )
''',
        '''    def test_recomputed_pre_policy_transcript_is_rejected(self) -> None:
        mutated = replace(self.transcript, issued_at_ms=-1)
        base = {
            "version": mutated.version,
            "domain": mutated.domain,
            "policy_commitment": mutated.policy_commitment,
            "settlement_class": mutated.settlement_class.value,
            "service_class": mutated.service_class.value,
            "request_event_commitment": mutated.request_event_commitment,
            "requester_scope": mutated.requester_scope,
            "provider_scope": mutated.provider_scope,
            "challenge": mutated.challenge,
            "response_commitment": mutated.response_commitment,
            "issued_at_ms": mutated.issued_at_ms,
            "expires_at_ms": mutated.expires_at_ms,
        }
        mutated = replace(
            mutated,
            evidence_commitment=MODEL.model_commitment(MODEL.EVIDENCE_DOMAIN, base),
        )
        mutated_claim = replace(
            self.claim,
            delivery_evidence_commitment=mutated.evidence_commitment,
        )
        mutated_claim = replace(
            mutated_claim,
            claim_id=mutated_claim.expected_claim_id(),
        )
        self.assertFalse(mutated.verify_for(self.policy, mutated_claim, now_ms=1_000))

    def test_transcript_expiry_is_enforced(self) -> None:
        self.assertFalse(
            self.transcript.verify_for(self.policy, self.claim, now_ms=2_001)
        )
''',
    )
    replace_once(
        TESTS,
        '''    def test_program_claim_must_match_policy_funding_source(self) -> None:
        policy = MODEL.make_policy(''',
        '''    def test_unknown_claim_schema_version_is_rejected(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.REQUESTER_FUNDED,
            "claim-version",
            budget=0,
        )
        model = MODEL.SettlementModel(payer_balances={"payer": 20})
        model.register_policy(policy)
        claim, transcript = MODEL.make_claim(
            policy,
            event="claim-version-event",
            requester="payer",
            funder="payer",
            provider="provider",
            amount=10,
            rate_tag=None,
        )
        unknown = replace(claim, version=MODEL.MODEL_VERSION + 1)
        unknown = replace(unknown, claim_id=unknown.expected_claim_id())
        outcome = model.submit(
            unknown,
            transcript,
            availability=MODEL.Availability(0, 0),
            now_ms=1_000,
        )
        self.assertEqual(outcome.code, MODEL.OutcomeCode.UNSUPPORTED_VERSION)
        self.assertEqual(model.payer_balances, {"payer": 20})

    def test_program_claim_must_match_policy_funding_source(self) -> None:
        policy = MODEL.make_policy(''',
    )
    replace_once(
        TESTS,
        '''        self.assertEqual(gates["audit-randomness-grinding-resistance"]["status"], "FAIL")
        self.assertEqual(gates["honest-false-rejection-rate"]["status"], "PARTIAL")''',
        '''        self.assertEqual(gates["audit-randomness-grinding-resistance"]["status"], "FAIL")
        self.assertEqual(gates["retained-state-per-policy-epoch"]["status"], "FAIL")
        self.assertEqual(
            gates["retained-state-per-policy-epoch"]["observed"],
            9_600_000,
        )
        self.assertEqual(gates["claim-plus-proof-wire-size"]["observed"], 16_384)
        self.assertEqual(gates["abstract-verification-work"]["observed"], 10_000)
        self.assertEqual(gates["honest-false-rejection-rate"]["status"], "PARTIAL")''',
    )


def patch_model_doc() -> None:
    replace_once(
        MODEL_DOC,
        '''| Adaptive audit sampling | **FAIL** | If the realized sampling seed is known while claim-committed inputs remain variable, attackers grind 60 claim IDs so every submitted claim avoids the 5% sample. |
| Issuer/auditor independence |''',
        '''| Adaptive audit sampling | **FAIL** | If the realized sampling seed is known while claim-committed inputs remain variable, attackers grind 60 claim IDs so every submitted claim avoids the 5% sample. |
| Configured retained-state capacity | **FAIL** | The default modeled policy allows 100,000 replay keys, estimated at 9,600,000 bytes, above the precommitted 8 MiB ceiling. Fixture state was smaller, but the gate must measure what policy permits. |
| Issuer/auditor independence |''',
    )
    replace_once(
        MODEL_DOC,
        '''yet resist economically valid collusion or adaptive sampling grind. Therefore
the generated report sets''',
        '''yet resist economically valid collusion or adaptive sampling grind, and its
default configured replay-state capacity exceeds the precommitted memory ceiling.
Therefore the generated report sets''',
    )
    replace_once(
        MODEL_DOC,
        '''The verifier checks all bindings, checks the current model time is inside the
challenge window, and checks the transcript expires no later than the claim.''',
        '''The verifier checks all bindings, rejects an unsupported transcript version,
rejects issue time before policy start, rejects malformed or post-policy windows,
checks the current model time is inside the challenge window, and checks the
transcript expires no later than the claim.''',
    )
    replace_once(
        MODEL_DOC,
        '''1. locate the exact registered policy;
2. reject the forbidden authority class;
3. match policy commitment, settlement class, and service class;
4. match funding epoch and, for sponsor/protocol classes, the exact committed
   funding source;
5. enforce positive amount and the per-claim cap;
6. recompute and verify claim ID;
7. recompute and verify the economic-event duplicate identifier;
8. return `AlreadyAccepted` for an exact finalized retry, including a retry
   after the original claim window closes;
9. enforce policy and new-claim expiry windows;
10. reject caller-supplied finality;
11. enforce combined wire-size and abstract-work caps;
12. require and verify the delivery transcript when policy requires it;
13. reject a previously accepted economic event in the policy domain;
14. require, domain-check, and deduplicate the rate-limit tag where required;
15. fail closed before exceeding retained replay-state capacity;
16. check issuer/auditor availability only for sponsor/protocol classes;
17. check payer balance or program budget; and
18. atomically debit one source, credit the modeled recipient, record replay
    keys, and create a canonical model finality reference.

No rejection after step 17 mutates value or replay state.''',
        '''1. locate the exact registered policy;
2. reject an unsupported claim schema version;
3. reject the forbidden authority class;
4. match policy commitment, settlement class, and service class;
5. match funding epoch and, for sponsor/protocol classes, the exact committed
   funding source;
6. enforce positive amount and the per-claim cap;
7. recompute and verify claim ID;
8. recompute and verify the economic-event duplicate identifier;
9. return `AlreadyAccepted` for an exact finalized retry, including a retry
   after the original claim window closes;
10. enforce policy and new-claim expiry windows;
11. reject caller-supplied finality;
12. enforce combined wire-size and abstract-work caps;
13. require and verify the delivery transcript when policy requires it;
14. reject a previously accepted economic event in the policy domain;
15. require, domain-check, and deduplicate the rate-limit tag where required;
16. fail closed before exceeding retained replay-state capacity;
17. check issuer/auditor availability only for sponsor/protocol classes;
18. check payer balance or program budget; and
19. atomically debit one source, credit the modeled recipient, record replay
    keys, and create a canonical model finality reference.

No rejection after step 18 mutates value or replay state.''',
    )
    replace_once(
        MODEL_DOC,
        '''- retained state: **PASS**, observed estimate `19,200 bytes`;
- claim plus transcript: **PASS**, observed maximum `1,523 bytes`; and
- abstract work: **PASS**, observed maximum `849` operations.''',
        '''- retained-state configured capacity: **FAIL**, observed estimate
  `9,600,000 bytes` versus an 8 MiB ceiling; the largest fixture state was
  `19,200 bytes`;
- claim plus transcript policy cap: **PASS**, configured at `16,384 bytes`;
  the largest fixture used `1,523 bytes`; and
- abstract-work policy cap: **PASS**, configured at `10,000 operations`;
  the largest fixture used `849` operations.''',
    )
    replace_once(
        MODEL_DOC,
        '''### 14.10 Physical weak-device cost is unmeasured — **PARTIAL**''',
        '''### 14.10 Configured replay-state capacity exceeds the gate — **FAIL**

**Location:** `make_policy(max_retained_keys=100_000)`,
`SettlementModel.RETAINED_KEY_ESTIMATE_BYTES`, and gate
`retained-state-per-policy-epoch`.

**Failure:** the earlier report measured only 19,200 bytes reached by the fixed
fixtures. The policy permits 100,000 retained keys, estimated at 9,600,000
bytes—above the precommitted 8 MiB threshold. Measuring a friendly fixture
instead of the allowed adversarial capacity falsely reported PASS.

**Long-term solution:** do not weaken the 8 MiB threshold after seeing the
failure. A later proposal must lower the policy cap, define authenticated epoch
compaction/checkpointing, or justify another bounded representation and then
measure its configured worst case. Evicting replay state silently is forbidden.

### 14.11 Physical weak-device cost is unmeasured — **PARTIAL**''',
    )
    replace_once(
        MODEL_DOC,
        '''- bounded input/state checks fail closed;
- audit evaluation cannot rewrite finality; and''',
        '''- bounded input/state checks fail closed;
- unknown claim versions and pre-policy transcripts are rejected;
- audit evaluation cannot rewrite finality; and''',
    )
    replace_once(
        MODEL_DOC,
        '''- physical performance;
- network partition liveness;''',
        '''- compliance with the 8 MiB configured retained-state gate;
- physical performance;
- network partition liveness;''',
    )
    replace_once(
        MODEL_DOC,
        '''drain is acceptable merely because no overrun occurred, or that a known
sampling seed remains safe while claim IDs are attacker-variable.''',
        '''drain is acceptable merely because no overrun occurred, that a known
sampling seed remains safe while claim IDs are attacker-variable, or that a
small fixture state proves a larger configured capacity meets the memory gate.''',
    )


def patch_doctrine() -> None:
    replace_once(
        DOCTRINE,
        '''claimants the realized audit seed before claim construction; all 60 submitted
claims grind outside the 5% sample. Cross-policy semantic overlap also remains
unmeasured without a global activity registry, so the report sets''',
        '''claimants the realized audit seed before claim construction; all 60 submitted
claims grind outside the 5% sample. The configured replay-state capacity is
9,600,000 estimated bytes against an 8 MiB ceiling, so that gate also fails.
Cross-policy semantic overlap remains unmeasured without a global activity
registry, so the report sets''',
    )
    replace_once(
        DOCTRINE,
        '''- reveals target-selection entropy while claimants can still grind claim IDs.

This doctrine can drift.''',
        '''- reveals target-selection entropy while claimants can still grind claim IDs;
  or
- reports a bounded-state PASS from friendly observed fixtures while the
  configured adversarial capacity exceeds the threshold.

This doctrine can drift.''',
    )


def patch_status() -> None:
    replace_once(
        STATUS,
        '''  second gate **fails**: when the realized audit seed is known while claim
  inputs remain variable, all 60 submitted IDs grind outside the 5% sample.
  A separate vector remains **partial** by design:''',
        '''  second gate **fails**: when the realized audit seed is known while claim
  inputs remain variable, all 60 submitted IDs grind outside the 5% sample.
  A third gate **fails** after measuring configured rather than friendly
  observed capacity: 100,000 replay keys estimate to 9,600,000 bytes,
  above the 8 MiB ceiling. A separate vector remains **partial** by design:''',
    )


def patch_decisions() -> None:
    replace_once(
        DECISIONS,
        '''a known realized audit seed lets adaptive claimants grind all 60 submitted IDs
outside the 5% sample. The generated authorization therefore remains `false`.''',
        '''a known realized audit seed lets adaptive claimants grind all 60 submitted IDs
outside the 5% sample. The configured 100,000-key replay-state allowance is
estimated at 9,600,000 bytes, above the precommitted 8 MiB gate. The generated
authorization therefore remains `false`.''',
    )
    replace_once(
        DECISIONS,
        '''colluding-extraction and adaptive-audit-grinding gates fail; cross-policy
semantic overlap, issuer/auditor independence, unlinkability, and physical
weakest-device cost remain unmeasured.''',
        '''colluding-extraction, adaptive-audit-grinding, and configured retained-state
gates fail; cross-policy semantic overlap, issuer/auditor independence,
unlinkability, and physical weakest-device cost remain unmeasured.''',
    )
    replace_once(
        DECISIONS,
        '''immutable. Threshold key counts can still be one operator behind several keys. A production implementation that''',
        '''immutable. The default replay-key capacity also exceeds the precommitted
8 MiB estimate even though friendly fixtures stay below it. Threshold key
counts can still be one operator behind several keys. A production implementation that''',
    )
    replace_once(
        DECISIONS,
        '''and benchmark the exact verifier on the weakest supported device. F6
remains separate and unstarted.''',
        '''meet the configured 8 MiB retained-state ceiling, and benchmark the exact
verifier on the weakest supported device. F6 remains separate and unstarted.''',
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
