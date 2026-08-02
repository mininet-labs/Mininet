#!/usr/bin/env python3
"""One-shot semantic hardening for PR #285.

The queued finalizer runs this helper, regenerates the exact report, executes the
model tests, and deletes the helper before committing the resulting state.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MODEL = ROOT / "tools" / "f5_phase2_model.py"
TESTS = ROOT / "tools" / "test_f5_phase2_model.py"
FIXTURE = ROOT / "tools" / "fixtures" / "f5_phase2_report.jsonl"
DOC = ROOT / "docs" / "design" / "f5-phase2-settlement-model.md"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement target, found {count}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def patch_model() -> None:
    replace_once(
        MODEL,
        '    EPOCH_MISMATCH = "epoch-mismatch"\n    EXPIRED = "expired"',
        '    EPOCH_MISMATCH = "epoch-mismatch"\n'
        '    FUNDING_SOURCE_MISMATCH = "funding-source-mismatch"\n'
        '    EXPIRED = "expired"',
    )
    replace_once(
        MODEL,
        '''        if self.max_abstract_verification_ops <= 0:
            raise ValueError("verification-work bound must be positive")

        if self.settlement_class is SettlementClass.REQUESTER_FUNDED:''',
        '''        if self.max_abstract_verification_ops <= 0:
            raise ValueError("verification-work bound must be positive")
        if self.privacy.cross_context_leakage_score() != 0:
            raise ValueError(
                "policy privacy declaration exceeds zero cross-context leakage budget"
            )

        if self.settlement_class is SettlementClass.REQUESTER_FUNDED:''',
    )
    replace_once(
        MODEL,
        '''        if claim.funding_epoch != policy.epoch:
            return self._reject(claim, OutcomeCode.EPOCH_MISMATCH)
        if now_ms < policy.starts_at_ms or now_ms > policy.expires_at_ms:
            return self._reject(claim, OutcomeCode.EXPIRED)
        if claim.expires_at_ms < now_ms or claim.expires_at_ms > policy.expires_at_ms:
            return self._reject(claim, OutcomeCode.EXPIRED)
        if claim.finality_reference is not None:
            return self._reject(claim, OutcomeCode.LOCAL_FINALITY_FORBIDDEN)
        if claim.amount_units <= 0 or claim.amount_units > policy.max_claim_units:
            return self._reject(claim, OutcomeCode.AMOUNT_INVALID)
        if claim.expected_claim_id() != claim.claim_id:
            return self._reject(claim, OutcomeCode.CLAIM_ID_MISMATCH)
        if claim.expected_duplicate_identifier(policy) != claim.duplicate_identifier:
            return self._reject(claim, OutcomeCode.DUPLICATE_ID_MISMATCH)

        if claim.claim_id in self.accepted_records:
            prior = self.accepted_records[claim.claim_id]
            return SubmissionOutcome(
                claim_id=claim.claim_id,
                code=OutcomeCode.ALREADY_ACCEPTED,
                spent_units=0,
                extraction_units=0,
                canonical_finality_reference=prior.canonical_finality_reference,
            )
''',
        '''        if claim.funding_epoch != policy.epoch:
            return self._reject(claim, OutcomeCode.EPOCH_MISMATCH)
        if (
            policy.settlement_class is not SettlementClass.REQUESTER_FUNDED
            and claim.funder_commitment != policy.funding_source_commitment
        ):
            return self._reject(claim, OutcomeCode.FUNDING_SOURCE_MISMATCH)
        if claim.amount_units <= 0 or claim.amount_units > policy.max_claim_units:
            return self._reject(claim, OutcomeCode.AMOUNT_INVALID)
        if claim.expected_claim_id() != claim.claim_id:
            return self._reject(claim, OutcomeCode.CLAIM_ID_MISMATCH)
        if claim.expected_duplicate_identifier(policy) != claim.duplicate_identifier:
            return self._reject(claim, OutcomeCode.DUPLICATE_ID_MISMATCH)

        # An exact retry of an already-finalized claim is a read of canonical
        # state, not a new claim. It remains idempotently recognizable after
        # the original submission window closes and cannot spend again.
        if claim.claim_id in self.accepted_records:
            prior = self.accepted_records[claim.claim_id]
            return SubmissionOutcome(
                claim_id=claim.claim_id,
                code=OutcomeCode.ALREADY_ACCEPTED,
                spent_units=0,
                extraction_units=0,
                canonical_finality_reference=prior.canonical_finality_reference,
            )

        if now_ms < policy.starts_at_ms or now_ms > policy.expires_at_ms:
            return self._reject(claim, OutcomeCode.EXPIRED)
        if claim.expires_at_ms < now_ms or claim.expires_at_ms > policy.expires_at_ms:
            return self._reject(claim, OutcomeCode.EXPIRED)
        if claim.finality_reference is not None:
            return self._reject(claim, OutcomeCode.LOCAL_FINALITY_FORBIDDEN)
''',
    )
    replace_once(
        MODEL,
        '''    event_commitment = model_commitment("request-event", event)
    transcript = DeliveryChallengeTranscript.create(''',
        '''    event_commitment = model_commitment("request-event", event)
    # Valid sponsor/protocol vectors derive the exact immutable funding source
    # from the policy; only requester-funded claims choose a payer balance.
    effective_funder = (
        funder
        if policy.settlement_class is SettlementClass.REQUESTER_FUNDED
        else policy.funding_source_commitment
    )
    transcript = DeliveryChallengeTranscript.create(''',
    )
    replace_once(
        MODEL,
        '''        requester_scope=requester,
        funder_commitment=funder,
        provider_scope=provider,
        request_event_commitment=event_commitment,''',
        '''        requester_scope=requester,
        funder_commitment=effective_funder,
        provider_scope=provider,
        request_event_commitment=event_commitment,''',
    )
    replace_once(
        MODEL,
        '''    replay_second = replay_model.submit(
        replay_claim,
        replay_tx,
        availability=Availability(3, 3),
        now_ms=now,
    )
    record(
        "network-retry-is-idempotent",
        replay_model,
        [replay_first, replay_second],
        GateStatus.PASS,
        "the retry returns already-accepted and consumes no additional budget",
    )''',
        '''    replay_second = replay_model.submit(
        replay_claim,
        replay_tx,
        availability=Availability(3, 3),
        now_ms=now,
    )
    replay_after_expiry = replay_model.submit(
        replay_claim,
        replay_tx,
        availability=Availability(0, 0),
        now_ms=2_001,
    )
    record(
        "network-retry-is-idempotent",
        replay_model,
        [replay_first, replay_second, replay_after_expiry],
        GateStatus.PASS,
        "retries before and after claim expiry return already-accepted and consume no additional budget",
    )''',
    )
    replace_once(
        MODEL,
        '''    record(
        "sponsor-budget-cannot-overrun",
        sponsor_model,
        sponsor_outcomes,
        GateStatus.PASS,
        "two 40-unit claims finalize; the third is rejected and 20 units remain",
    )

    protocol_policy = make_policy(''',
        '''    record(
        "sponsor-budget-cannot-overrun",
        sponsor_model,
        sponsor_outcomes,
        GateStatus.PASS,
        "two 40-unit claims finalize; the third is rejected and 20 units remain",
    )

    funding_model = SettlementModel()
    funding_model.register_policy(sponsor_policy)
    valid_funding_claim, valid_funding_tx = make_claim(
        sponsor_policy,
        event="event-funding-source",
        requester="funding-requester",
        funder=sponsor_policy.funding_source_commitment,
        provider="funding-provider",
        amount=10,
        rate_tag="funding-tag",
    )
    wrong_funding_claim = SettlementClaim.create(
        sponsor_policy,
        valid_funding_tx,
        requester_scope=valid_funding_claim.requester_scope,
        funder_commitment="different-sponsor-budget",
        provider_scope=valid_funding_claim.provider_scope,
        request_event_commitment=valid_funding_claim.request_event_commitment,
        amount_units=valid_funding_claim.amount_units,
        expires_at_ms=valid_funding_claim.expires_at_ms,
        rate_limit_tag=valid_funding_claim.rate_limit_tag,
    )
    funding_outcome = funding_model.submit(
        wrong_funding_claim,
        valid_funding_tx,
        availability=Availability(3, 3),
        now_ms=now,
    )
    record(
        "program-claim-must-name-policy-funding-source",
        funding_model,
        [funding_outcome],
        GateStatus.PASS,
        "a claim with a self-consistent id but the wrong sponsor budget is rejected before spend",
    )

    protocol_policy = make_policy(''',
    )
    replace_once(
        MODEL,
        '''    record(
        "cross-policy-and-epoch-substitution-fails",
        cross_model,
        [cross_outcome],
        GateStatus.PASS,
        "changing the policy commitment invalidates the claim id/transcript binding",
    )

    rate_model = SettlementModel()''',
        '''    record(
        "cross-policy-and-epoch-substitution-fails",
        cross_model,
        [cross_outcome],
        GateStatus.PASS,
        "changing the policy commitment invalidates the claim id/transcript binding",
    )

    overlap_policy_a = make_policy(
        SettlementClass.SPONSOR_FUNDED,
        "overlap-a",
        budget=10,
        max_claim=10,
        epoch=7,
    )
    overlap_policy_b = make_policy(
        SettlementClass.SPONSOR_FUNDED,
        "overlap-b",
        budget=10,
        max_claim=10,
        epoch=8,
    )
    overlap_model = SettlementModel()
    overlap_model.register_policy(overlap_policy_a)
    overlap_model.register_policy(overlap_policy_b)
    overlap_a = make_claim(
        overlap_policy_a,
        event="same-semantic-work",
        requester="overlap-requester-a",
        funder=overlap_policy_a.funding_source_commitment,
        provider="overlap-provider-a",
        amount=10,
        rate_tag="overlap-tag-a",
    )
    overlap_b = make_claim(
        overlap_policy_b,
        event="same-semantic-work",
        requester="overlap-requester-b",
        funder=overlap_policy_b.funding_source_commitment,
        provider="overlap-provider-b",
        amount=10,
        rate_tag="overlap-tag-b",
    )
    overlap_outcomes = [
        overlap_model.submit(
            overlap_a[0],
            overlap_a[1],
            availability=Availability(3, 3),
            now_ms=now,
        ),
        overlap_model.submit(
            overlap_b[0],
            overlap_b[1],
            availability=Availability(3, 3),
            now_ms=now,
        ),
    ]
    record(
        "distinct-policies-do-not-create-a-global-event-registry",
        overlap_model,
        overlap_outcomes,
        GateStatus.PARTIAL,
        "the same event commitment may consume two independently committed budgets; preventing unwanted overlap needs an explicit privacy-preserving policy-family rule, not a global activity graph",
    )

    rate_model = SettlementModel()''',
    )
    replace_once(
        MODEL,
        '''    duplicate_attempts = 2
    duplicate_false_negatives = 0
    honest_claims = 1
    honest_rejections = 0''',
        '''    duplicate_outcomes = [
        replay_second,
        replay_after_expiry,
        split_outcomes[1],
    ]
    duplicate_attempts = len(duplicate_outcomes)
    duplicate_false_negatives = sum(
        outcome.spent_units > 0 for outcome in duplicate_outcomes
    )
    honest_outcomes = [requester_outcome]
    honest_claims = len(honest_outcomes)
    honest_rejections = sum(
        outcome.code is not OutcomeCode.ACCEPTED for outcome in honest_outcomes
    )''',
    )
    replace_once(
        MODEL,
        '''        GateResult(
            gate="audit-detection-probability",''',
        '''        GateResult(
            gate="cross-policy-semantic-deduplication",
            status=GateStatus.PARTIAL,
            threshold="explicit-policy-family-rule-or-declared-independent-budgets",
            observed="unmeasured-no-global-registry-by-design",
            unit="policy-overlap-semantics",
            detail="separate policies may pay the same event; a global requester/provider activity graph is forbidden",
        ),
        GateResult(
            gate="audit-detection-probability",''',
    )


def patch_tests() -> None:
    replace_once(
        TESTS,
        '''    def test_authority_bearing_policy_cannot_be_constructed(self) -> None:
        with self.assertRaisesRegex(ValueError, "forbidden"):''',
        '''    def test_privacy_leaking_policy_cannot_be_constructed(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "privacy-policy",
            budget=100,
        )
        leaky = MODEL.PrivacyDeclaration(
            disclosures=((MODEL.Role.AUDITOR, (MODEL.Disclosure.ROOT_DID,)),)
        )
        with self.assertRaisesRegex(ValueError, "privacy declaration"):
            replace(policy, privacy=leaky)

    def test_authority_bearing_policy_cannot_be_constructed(self) -> None:
        with self.assertRaisesRegex(ValueError, "forbidden"):''',
    )
    replace_once(
        TESTS,
        '''    def test_network_retry_is_idempotent(self) -> None:
        policy = MODEL.make_policy(''',
        '''    def test_program_claim_must_match_policy_funding_source(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "funding-source",
            budget=100,
        )
        model = MODEL.SettlementModel()
        model.register_policy(policy)
        valid, transcript = MODEL.make_claim(
            policy,
            event="funding-source-event",
            requester="requester",
            funder=policy.funding_source_commitment,
            provider="provider",
            amount=10,
            rate_tag="funding-source-tag",
        )
        wrong = MODEL.SettlementClaim.create(
            policy,
            transcript,
            requester_scope=valid.requester_scope,
            funder_commitment="other-budget",
            provider_scope=valid.provider_scope,
            request_event_commitment=valid.request_event_commitment,
            amount_units=valid.amount_units,
            expires_at_ms=valid.expires_at_ms,
            rate_limit_tag=valid.rate_limit_tag,
        )
        outcome = model.submit(
            wrong,
            transcript,
            availability=MODEL.Availability(3, 3),
            now_ms=1_000,
        )
        self.assertEqual(
            outcome.code,
            MODEL.OutcomeCode.FUNDING_SOURCE_MISMATCH,
        )
        self.assertEqual(model.program_remaining[policy.commitment], 100)

    def test_network_retry_is_idempotent(self) -> None:
        policy = MODEL.make_policy(''',
    )
    replace_once(
        TESTS,
        '''        second = model.submit(
            claim,
            transcript,
            availability=MODEL.Availability(3, 3),
            now_ms=1_000,
        )
        self.assertEqual(first.code, MODEL.OutcomeCode.ACCEPTED)
        self.assertEqual(second.code, MODEL.OutcomeCode.ALREADY_ACCEPTED)
        self.assertEqual(second.spent_units, 0)
        self.assertEqual(model.program_remaining[policy.commitment], 90)''',
        '''        second = model.submit(
            claim,
            transcript,
            availability=MODEL.Availability(3, 3),
            now_ms=1_000,
        )
        after_expiry = model.submit(
            claim,
            transcript,
            availability=MODEL.Availability(0, 0),
            now_ms=2_001,
        )
        self.assertEqual(first.code, MODEL.OutcomeCode.ACCEPTED)
        self.assertEqual(second.code, MODEL.OutcomeCode.ALREADY_ACCEPTED)
        self.assertEqual(after_expiry.code, MODEL.OutcomeCode.ALREADY_ACCEPTED)
        self.assertEqual(second.spent_units, 0)
        self.assertEqual(after_expiry.spent_units, 0)
        self.assertEqual(model.program_remaining[policy.commitment], 90)''',
    )


def patch_doc() -> None:
    replace_once(
        DOC,
        '''| Cross-policy/domain substitution | **PASS** | Policy, class, service, epoch, event, transcript, duplicate domain, and rate-limit domain are bound into deterministic model commitments. |''',
        '''| Cross-policy/domain substitution | **PASS** | Policy, funding source, class, service, epoch, event, transcript, duplicate domain, and rate-limit domain are bound into deterministic model commitments. |''',
    )
    replace_once(
        DOC,
        '''- claims debit only the already registered program budget; and
- an outage halts this program without affecting requester-funded settlement.''',
        '''- each claim must name the policy's exact committed funding source;
- claims debit only the already registered program budget; and
- an outage halts this program without affecting requester-funded settlement.''',
    )
    replace_once(
        DOC,
        '''| `funder_commitment` | Existing payer or registered budget source. |''',
        '''| `funder_commitment` | Existing requester payer, or the exact sponsor/protocol funding source committed by policy. |''',
    )
    replace_once(
        DOC,
        '''1. locate the exact registered policy;
2. reject the forbidden authority class;
3. match policy commitment, settlement class, and service class;
4. match funding epoch;
5. enforce policy and claim expiry windows;
6. reject caller-supplied finality;
7. enforce positive amount and per-claim cap;
8. recompute and verify claim ID;
9. recompute and verify the economic-event duplicate identifier;
10. return `AlreadyAccepted` for an exact accepted retry;
11. enforce combined wire-size and abstract-work caps;
12. require and verify the delivery transcript when policy requires it;
13. reject a previously accepted economic event in the policy domain;
14. require, domain-check, and deduplicate the rate-limit tag where required;
15. fail closed before exceeding retained replay-state capacity;
16. check issuer/auditor availability only for sponsor/protocol classes;
17. check payer balance or program budget; and
18. atomically debit one source, credit the modeled recipient, record replay
    keys, and create a canonical model finality reference.''',
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
    keys, and create a canonical model finality reference.''',
    )
    replace_once(
        DOC,
        '''| Exact network retry | **PASS** | Second submission is `AlreadyAccepted` with zero spend. |''',
        '''| Exact network retry | **PASS** | Retries before and after claim expiry are `AlreadyAccepted` with zero additional spend. |''',
    )
    replace_once(
        DOC,
        '''| Cross-policy/epoch substitution | **PASS** | Rejected before spend. |
| Personhood-domain tag substituted into settlement | **PASS** | Rejected for rate-limit domain mismatch. |''',
        '''| Wrong committed sponsor funding source | **PASS** | A self-consistent claim naming another budget is rejected before spend. |
| Cross-policy/epoch substitution | **PASS** | Rejected before spend. |
| Same event under two independently committed policies | **PARTIAL** | Both may pay; avoiding unwanted overlap needs an explicit policy-family rule without a global activity graph. |
| Personhood-domain tag substituted into settlement | **PASS** | Rejected for rate-limit domain mismatch. |''',
    )
    replace_once(
        DOC,
        '''- cross-context leakage score: **PASS**, observed `0` under the narrow declared
  metric;
- audit detection for 60 objectively invalid claims: **PASS**, observed''',
        '''- cross-context leakage score: **PASS**, observed `0` under the narrow declared
  metric;
- cross-policy semantic deduplication: **PARTIAL**, no global registry by
  design and no reviewed policy-family rule yet;
- audit detection for 60 objectively invalid claims: **PASS**, observed''',
    )
    replace_once(
        DOC,
        '''### 14.3 Rate-limit scarcity is not implemented — **PARTIAL**''',
        '''### 14.3 Distinct policies can pay the same semantic work — **PARTIAL**

**Location:** policy-scoped `duplicate_identifier` and fixed vector
`distinct-policies-do-not-create-a-global-event-registry`.

**Failure:** replay protection is intentionally policy-scoped. Two independently
committed budgets can accept the same event commitment, and different event
commitments may also describe equivalent work. A global event registry would
reduce overlap but would create a cross-program activity graph and a new
correlation authority.

**Long-term solution:** policies that must share one entitlement need an explicit,
precommitted privacy-preserving policy-family domain and overlap rule. Independent
budgets must say when repeated service is legitimate. Do not infer semantic
equivalence through a centralized requester-provider graph.

### 14.4 Rate-limit scarcity is not implemented — **PARTIAL**''',
    )
    # Renumber the subsequent headings after inserting the new 14.3 section.
    for old, new in (
        ("### 14.4 Deterministic claim-ID ordering", "### 14.5 Deterministic claim-ID ordering"),
        ("### 14.5 Threshold counts", "### 14.6 Threshold counts"),
        ("### 14.6 Privacy is declared", "### 14.7 Privacy is declared"),
        ("### 14.7 Retained-state exhaustion", "### 14.8 Retained-state exhaustion"),
        ("### 14.8 Physical weak-device cost", "### 14.9 Physical weak-device cost"),
    ):
        replace_once(DOC, old, new)


def main() -> None:
    patch_model()
    patch_tests()
    patch_doc()
    report = subprocess.run(
        [sys.executable, str(MODEL)],
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout
    FIXTURE.write_text(report, encoding="utf-8")
    subprocess.run(
        [sys.executable, "-m", "unittest", "tools/test_f5_phase2_model.py"],
        cwd=ROOT,
        check=True,
    )
    subprocess.run(
        [sys.executable, "-m", "unittest", "tools/test_f5_phase2_vectors.py"],
        cwd=ROOT,
        check=True,
    )
    Path(__file__).unlink()


if __name__ == "__main__":
    main()
