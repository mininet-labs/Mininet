from __future__ import annotations

import importlib.util
import json
import sys
import unittest
from dataclasses import replace
from itertools import permutations
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("f5_phase2_model.py")
SPEC = importlib.util.spec_from_file_location("f5_phase2_model", MODULE_PATH)
assert SPEC and SPEC.loader
MODEL = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODEL
SPEC.loader.exec_module(MODEL)


class PolicyTests(unittest.TestCase):
    def test_requester_funded_policy_cannot_require_gatekeepers(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.REQUESTER_FUNDED,
            "market",
            budget=0,
        )
        with self.assertRaisesRegex(ValueError, "issuer/auditor"):
            replace(policy, issuer_threshold=1)
        with self.assertRaisesRegex(ValueError, "collusion-limit"):
            replace(
                policy,
                rate_limit_domain=f"{MODEL.RATE_LIMIT_DOMAIN_PREFIX}market",
            )

    def test_sponsor_policy_requires_finite_budget_and_scoped_rate_domain(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "sponsor",
            budget=100,
        )
        with self.assertRaisesRegex(ValueError, "finite budget"):
            replace(policy, program_budget_units=0)
        with self.assertRaisesRegex(ValueError, "settlement-specific"):
            replace(policy, rate_limit_domain="mininet/personhood/nullifier/v1")

    def test_authority_bearing_policy_cannot_be_constructed(self) -> None:
        with self.assertRaisesRegex(ValueError, "forbidden"):
            MODEL.SettlementPolicy(
                version=MODEL.MODEL_VERSION,
                settlement_class=MODEL.SettlementClass.AUTHORITY_BEARING,
                service_class=MODEL.ServiceClass.BYTE_DELIVERY,
                policy_name="authority",
                funding_source_commitment="authority-budget",
                epoch=1,
                starts_at_ms=0,
                expires_at_ms=10,
                program_budget_units=10,
                max_claim_units=1,
                duplicate_domain=f"{MODEL.DUPLICATE_DOMAIN_PREFIX}authority",
                rate_limit_domain=f"{MODEL.RATE_LIMIT_DOMAIN_PREFIX}authority",
                challenge_required=True,
                issuer_threshold=1,
                auditor_threshold=1,
                audit_sample_bps=500,
                max_retained_keys=10,
                max_claim_proof_wire_bytes=16_384,
                max_abstract_verification_ops=10_000,
                privacy=MODEL.PrivacyDeclaration.default(),
            )


class TranscriptTests(unittest.TestCase):
    def setUp(self) -> None:
        self.policy = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "transcript",
            budget=100,
        )
        self.claim, self.transcript = MODEL.make_claim(
            self.policy,
            event="transcript-event",
            requester="requester",
            funder="sponsor-budget",
            provider="provider",
            amount=10,
            rate_tag="rate-1",
        )

    def test_transcript_binds_policy_parties_event_and_challenge(self) -> None:
        self.assertTrue(
            self.transcript.verify_for(self.policy, self.claim, now_ms=1_000)
        )
        for mutated in (
            replace(self.transcript, provider_scope="attacker"),
            replace(self.transcript, requester_scope="attacker"),
            replace(self.transcript, challenge="other"),
            replace(self.transcript, response_commitment="other"),
        ):
            self.assertFalse(mutated.verify_for(self.policy, self.claim, now_ms=1_000))

    def test_transcript_expiry_is_enforced(self) -> None:
        self.assertFalse(
            self.transcript.verify_for(self.policy, self.claim, now_ms=2_001)
        )

    def test_claim_and_transcript_have_explicit_bounds(self) -> None:
        wire = self.claim.wire_size_bytes + self.transcript.wire_size_bytes
        work = (
            self.claim.abstract_verification_ops
            + self.transcript.abstract_verification_ops
        )
        self.assertLessEqual(wire, self.policy.max_claim_proof_wire_bytes)
        self.assertLessEqual(work, self.policy.max_abstract_verification_ops)


class SettlementInvariantTests(unittest.TestCase):
    def test_requester_funded_settlement_survives_total_f5_outage(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.REQUESTER_FUNDED,
            "market-outage",
            budget=0,
        )
        model = MODEL.SettlementModel(
            payer_balances={"payer": 50, "provider": 0}
        )
        model.register_policy(policy)
        claim, transcript = MODEL.make_claim(
            policy,
            event="market-outage-event",
            requester="payer-scope",
            funder="payer",
            provider="provider",
            amount=20,
            rate_tag=None,
        )
        outcome = model.submit(
            claim,
            transcript,
            availability=MODEL.Availability(0, 0),
            now_ms=1_000,
        )
        self.assertEqual(outcome.code, MODEL.OutcomeCode.ACCEPTED)
        self.assertEqual(model.payer_balances, {"payer": 30, "provider": 20})
        self.assertTrue(model.requester_value_is_conserved())

    def test_requester_self_payment_has_volume_but_zero_net_extraction(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.REQUESTER_FUNDED,
            "self-pay",
            budget=0,
        )
        model = MODEL.SettlementModel(payer_balances={"same-party": 50})
        model.register_policy(policy)
        claim, transcript = MODEL.make_claim(
            policy,
            event="self-pay-event",
            requester="same-party",
            funder="same-party",
            provider="same-party",
            amount=20,
            rate_tag=None,
        )
        outcome = model.submit(
            claim,
            transcript,
            availability=MODEL.Availability(0, 0),
            now_ms=1_000,
            attacker_controlled_scopes={"same-party"},
        )
        self.assertEqual(outcome.code, MODEL.OutcomeCode.ACCEPTED)
        self.assertEqual(outcome.extraction_units, 0)
        self.assertEqual(model.payer_balances["same-party"], 50)
        self.assertEqual(model.finalized_transfer_volume_units, 20)
        self.assertEqual(model.requester_funded_transfer_volume_units, 20)

    def test_sponsor_budget_cannot_go_negative(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "bounded-sponsor",
            budget=100,
            max_claim=40,
        )
        model = MODEL.SettlementModel()
        model.register_policy(policy)
        items = [
            MODEL.make_claim(
                policy,
                event=f"event-{index}",
                requester=f"requester-{index}",
                funder="sponsor",
                provider=f"provider-{index}",
                amount=40,
                rate_tag=f"tag-{index}",
            )
            for index in range(3)
        ]
        outcomes = model.submit_canonical_batch(
            items,
            availability=MODEL.Availability(3, 3),
            now_ms=1_000,
        )
        self.assertEqual(
            sum(outcome.code is MODEL.OutcomeCode.ACCEPTED for outcome in outcomes),
            2,
        )
        self.assertEqual(model.program_remaining[policy.commitment], 20)
        self.assertEqual(model.budget_overrun_units(policy.commitment), 0)
        self.assertTrue(model.program_value_is_conserved(policy.commitment))

    def test_canonical_batch_is_permutation_independent(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.PROTOCOL_SUBSIDIZED,
            "race",
            budget=90,
            max_claim=30,
        )
        items = [
            MODEL.make_claim(
                policy,
                event=f"race-{index}",
                requester=f"requester-{index}",
                funder="epoch-budget",
                provider=f"provider-{index}",
                amount=30,
                rate_tag=f"race-tag-{index}",
            )
            for index in range(4)
        ]
        digests: set[str] = set()
        accepted_sets: set[tuple[str, ...]] = set()
        for ordering in permutations(items):
            model = MODEL.SettlementModel()
            model.register_policy(policy)
            outcomes = model.submit_canonical_batch(
                list(ordering),
                availability=MODEL.Availability(3, 3),
                now_ms=1_000,
            )
            digests.add(model.state_digest())
            accepted_sets.add(
                tuple(
                    sorted(
                        outcome.claim_id
                        for outcome in outcomes
                        if outcome.code is MODEL.OutcomeCode.ACCEPTED
                    )
                )
            )
        self.assertEqual(len(digests), 1)
        self.assertEqual(len(accepted_sets), 1)
        self.assertEqual(len(next(iter(accepted_sets))), 3)

    def test_network_retry_is_idempotent(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "retry",
            budget=100,
        )
        model = MODEL.SettlementModel()
        model.register_policy(policy)
        claim, transcript = MODEL.make_claim(
            policy,
            event="retry-event",
            requester="requester",
            funder="sponsor",
            provider="provider",
            amount=10,
            rate_tag="retry-tag",
        )
        first = model.submit(
            claim,
            transcript,
            availability=MODEL.Availability(3, 3),
            now_ms=1_000,
        )
        second = model.submit(
            claim,
            transcript,
            availability=MODEL.Availability(3, 3),
            now_ms=1_000,
        )
        self.assertEqual(first.code, MODEL.OutcomeCode.ACCEPTED)
        self.assertEqual(second.code, MODEL.OutcomeCode.ALREADY_ACCEPTED)
        self.assertEqual(second.spent_units, 0)
        self.assertEqual(model.program_remaining[policy.commitment], 90)

    def test_identity_splitting_cannot_multiply_one_economic_event(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "split",
            budget=100,
        )
        model = MODEL.SettlementModel()
        model.register_policy(policy)
        first, first_transcript = MODEL.make_claim(
            policy,
            event="same-economic-event",
            requester="root-a",
            funder="sponsor",
            provider="provider-a",
            amount=10,
            rate_tag="tag-a",
        )
        second, second_transcript = MODEL.make_claim(
            policy,
            event="same-economic-event",
            requester="root-b",
            funder="sponsor",
            provider="provider-b",
            amount=10,
            rate_tag="tag-b",
        )
        self.assertNotEqual(first.claim_id, second.claim_id)
        self.assertEqual(first.duplicate_identifier, second.duplicate_identifier)
        first_outcome = model.submit(
            first,
            first_transcript,
            availability=MODEL.Availability(3, 3),
            now_ms=1_000,
        )
        second_outcome = model.submit(
            second,
            second_transcript,
            availability=MODEL.Availability(3, 3),
            now_ms=1_000,
        )
        self.assertEqual(first_outcome.code, MODEL.OutcomeCode.ACCEPTED)
        self.assertEqual(
            second_outcome.code,
            MODEL.OutcomeCode.DUPLICATE_ECONOMIC_EVENT,
        )

    def test_cross_epoch_policy_substitution_is_rejected_before_spend(self) -> None:
        policy_a = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "policy-a",
            budget=100,
            epoch=7,
        )
        policy_b = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "policy-b",
            budget=100,
            epoch=8,
        )
        model = MODEL.SettlementModel()
        model.register_policy(policy_a)
        model.register_policy(policy_b)
        claim, transcript = MODEL.make_claim(
            policy_a,
            event="cross-policy-event",
            requester="requester",
            funder="sponsor",
            provider="provider",
            amount=10,
            rate_tag="tag",
        )
        substituted = replace(claim, policy_commitment=policy_b.commitment)
        outcome = model.submit(
            substituted,
            transcript,
            availability=MODEL.Availability(3, 3),
            now_ms=1_000,
        )
        self.assertEqual(outcome.code, MODEL.OutcomeCode.EPOCH_MISMATCH)
        self.assertEqual(model.program_remaining[policy_a.commitment], 100)
        self.assertEqual(model.program_remaining[policy_b.commitment], 100)

    def test_same_epoch_policy_substitution_invalidates_claim_id(self) -> None:
        policy_a = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "same-epoch-policy-a",
            budget=100,
            epoch=7,
        )
        policy_b = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "same-epoch-policy-b",
            budget=100,
            epoch=7,
        )
        model = MODEL.SettlementModel()
        model.register_policy(policy_a)
        model.register_policy(policy_b)
        claim, transcript = MODEL.make_claim(
            policy_a,
            event="same-epoch-cross-policy-event",
            requester="requester",
            funder="sponsor",
            provider="provider",
            amount=10,
            rate_tag="tag",
        )
        substituted = replace(claim, policy_commitment=policy_b.commitment)
        outcome = model.submit(
            substituted,
            transcript,
            availability=MODEL.Availability(3, 3),
            now_ms=1_000,
        )
        self.assertEqual(outcome.code, MODEL.OutcomeCode.CLAIM_ID_MISMATCH)
        self.assertEqual(model.program_remaining[policy_a.commitment], 100)
        self.assertEqual(model.program_remaining[policy_b.commitment], 100)

    def test_non_settlement_rate_tag_cannot_be_substituted(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "rate-domain",
            budget=100,
        )
        model = MODEL.SettlementModel()
        model.register_policy(policy)
        claim, transcript = MODEL.make_claim(
            policy,
            event="rate-domain-event",
            requester="requester",
            funder="sponsor",
            provider="provider",
            amount=10,
            rate_tag=MODEL.ScopedRateTag(
                "mininet/personhood/nullifier/v1",
                "cross-context-secret",
            ),
        )
        outcome = model.submit(
            claim,
            transcript,
            availability=MODEL.Availability(3, 3),
            now_ms=1_000,
        )
        self.assertEqual(
            outcome.code,
            MODEL.OutcomeCode.RATE_LIMIT_DOMAIN_MISMATCH,
        )

    def test_role_disappearance_is_local_to_subsidized_class(self) -> None:
        market_policy = MODEL.make_policy(
            MODEL.SettlementClass.REQUESTER_FUNDED,
            "outage-market",
            budget=0,
        )
        sponsor_policy = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "outage-sponsor",
            budget=100,
        )
        model = MODEL.SettlementModel(
            payer_balances={"payer": 20, "market-provider": 0}
        )
        model.register_policy(market_policy)
        model.register_policy(sponsor_policy)
        market_claim, market_transcript = MODEL.make_claim(
            market_policy,
            event="market-event",
            requester="payer",
            funder="payer",
            provider="market-provider",
            amount=10,
            rate_tag=None,
        )
        sponsor_claim, sponsor_transcript = MODEL.make_claim(
            sponsor_policy,
            event="sponsor-event",
            requester="requester",
            funder="sponsor",
            provider="sponsor-provider",
            amount=10,
            rate_tag="sponsor-tag",
        )
        market_outcome = model.submit(
            market_claim,
            market_transcript,
            availability=MODEL.Availability(0, 0),
            now_ms=1_000,
        )
        sponsor_outcome = model.submit(
            sponsor_claim,
            sponsor_transcript,
            availability=MODEL.Availability(0, 0),
            now_ms=1_000,
        )
        self.assertEqual(market_outcome.code, MODEL.OutcomeCode.ACCEPTED)
        self.assertEqual(
            sponsor_outcome.code,
            MODEL.OutcomeCode.ISSUER_UNAVAILABLE,
        )
        self.assertEqual(model.program_remaining[sponsor_policy.commitment], 100)

    def test_retained_state_limit_fails_closed_without_eviction(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.SPONSOR_FUNDED,
            "retained",
            budget=100,
            max_claim=10,
            max_retained_keys=2,
        )
        model = MODEL.SettlementModel()
        model.register_policy(policy)
        first, first_transcript = MODEL.make_claim(
            policy,
            event="retained-1",
            requester="requester-1",
            funder="sponsor",
            provider="provider-1",
            amount=10,
            rate_tag="retained-tag-1",
        )
        second, second_transcript = MODEL.make_claim(
            policy,
            event="retained-2",
            requester="requester-2",
            funder="sponsor",
            provider="provider-2",
            amount=10,
            rate_tag="retained-tag-2",
        )
        self.assertEqual(
            model.submit(
                first,
                first_transcript,
                availability=MODEL.Availability(3, 3),
                now_ms=1_000,
            ).code,
            MODEL.OutcomeCode.ACCEPTED,
        )
        self.assertEqual(
            model.submit(
                second,
                second_transcript,
                availability=MODEL.Availability(3, 3),
                now_ms=1_000,
            ).code,
            MODEL.OutcomeCode.RETAINED_STATE_LIMIT,
        )
        self.assertEqual(model.program_remaining[policy.commitment], 90)


class FailureHonestyTests(unittest.TestCase):
    def test_real_delivery_by_colluders_passes_delivery_check(self) -> None:
        policy = MODEL.make_policy(
            MODEL.SettlementClass.PROTOCOL_SUBSIDIZED,
            "collusion",
            budget=100,
            max_claim=10,
        )
        model = MODEL.SettlementModel()
        model.register_policy(policy)
        outcomes = []
        controlled = set()
        for index in range(10):
            requester = f"attacker-requester-{index}"
            provider = f"attacker-provider-{index}"
            controlled.update((requester, provider))
            claim, transcript = MODEL.make_claim(
                policy,
                event=f"real-delivery-{index}",
                requester=requester,
                funder="protocol-budget",
                provider=provider,
                amount=10,
                rate_tag=f"attacker-tag-{index}",
            )
            outcomes.append(
                model.submit(
                    claim,
                    transcript,
                    availability=MODEL.Availability(3, 3),
                    now_ms=1_000,
                    attacker_controlled_scopes=controlled,
                )
            )
        self.assertTrue(
            all(outcome.code is MODEL.OutcomeCode.ACCEPTED for outcome in outcomes)
        )
        self.assertEqual(sum(outcome.extraction_units for outcome in outcomes), 100)
        self.assertEqual(model.program_remaining[policy.commitment], 0)
        self.assertEqual(model.budget_overrun_units(policy.commitment), 0)

    def test_audit_allegation_has_no_canonical_state_handle(self) -> None:
        for response in (
            MODEL.evaluate_audit_allegation(
                MODEL.AllegationKind.HEURISTIC_COLLUSION,
                objective_proof_valid=False,
            ),
            MODEL.evaluate_audit_allegation(
                MODEL.AllegationKind.OBJECTIVE_TRANSCRIPT_FAILURE,
                objective_proof_valid=False,
            ),
            MODEL.evaluate_audit_allegation(
                MODEL.AllegationKind.OBJECTIVE_TRANSCRIPT_FAILURE,
                objective_proof_valid=True,
            ),
        ):
            self.assertFalse(response.canonical_state_mutation_permitted)

    def test_privacy_budget_detects_global_graph_fields(self) -> None:
        safe = MODEL.PrivacyDeclaration.default()
        leaky = MODEL.PrivacyDeclaration(
            disclosures=(
                (
                    MODEL.Role.AUDITOR,
                    (MODEL.Disclosure.ROOT_DID, MODEL.Disclosure.RAW_QUERY),
                ),
            )
        )
        self.assertEqual(safe.cross_context_leakage_score(), 0)
        self.assertFalse(safe.has_global_graph_fields())
        self.assertEqual(leaky.cross_context_leakage_score(), 150)
        self.assertTrue(leaky.has_global_graph_fields())

    def test_audit_sampling_is_public_and_deterministic(self) -> None:
        first = [
            MODEL.audit_selected("public-seed", f"claim-{index}", 500)
            for index in range(200)
        ]
        second = [
            MODEL.audit_selected("public-seed", f"claim-{index}", 500)
            for index in range(200)
        ]
        self.assertEqual(first, second)
        self.assertEqual(
            MODEL.audit_detection_probability_bps(500, 60),
            MODEL.audit_detection_probability_bps(500, 60),
        )
        self.assertGreaterEqual(
            MODEL.audit_detection_probability_bps(500, 60),
            9_500,
        )


class ReportTests(unittest.TestCase):
    def test_fixed_vectors_are_deterministic_and_preserve_failed_gates(self) -> None:
        first = MODEL.render_report()
        second = MODEL.render_report()
        self.assertEqual(first, second)
        self.assertNotIn("TBD", first)

        records = [json.loads(line) for line in first.splitlines()]
        gates = {record["gate"]: record for record in records if record["kind"] == "gate"}
        authorization = next(
            record for record in records if record["kind"] == "authorization"
        )
        self.assertEqual(gates["maximum-budget-overrun"]["status"], "PASS")
        self.assertEqual(gates["maximum-colluding-extraction"]["status"], "FAIL")
        self.assertEqual(gates["issuer-concentration"]["status"], "PARTIAL")
        self.assertEqual(gates["weak-device-verification-cpu"]["status"], "PARTIAL")
        self.assertFalse(authorization["phase3_authorized"])

    def test_all_vector_names_are_unique(self) -> None:
        vectors, _ = MODEL.run_fixed_vectors()
        names = [vector.vector for vector in vectors]
        self.assertEqual(len(names), len(set(names)))


if __name__ == "__main__":
    unittest.main()
