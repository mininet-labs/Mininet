#!/usr/bin/env python3
"""One-shot correction to the queued hardening transformation."""

from pathlib import Path


path = Path(__file__).with_name("f5_phase2_harden.py")
text = path.read_text(encoding="utf-8")
old = '''        if claim.funding_epoch != policy.epoch:
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

        # An exact retry'''
new = '''        if claim.funding_epoch != policy.epoch:
            return self._reject(claim, OutcomeCode.EPOCH_MISMATCH)
        if claim.amount_units <= 0 or claim.amount_units > policy.max_claim_units:
            return self._reject(claim, OutcomeCode.AMOUNT_INVALID)
        if claim.expected_claim_id() != claim.claim_id:
            return self._reject(claim, OutcomeCode.CLAIM_ID_MISMATCH)
        if claim.expected_duplicate_identifier(policy) != claim.duplicate_identifier:
            return self._reject(claim, OutcomeCode.DUPLICATE_ID_MISMATCH)
        if (
            policy.settlement_class is not SettlementClass.REQUESTER_FUNDED
            and claim.funder_commitment != policy.funding_source_commitment
        ):
            return self._reject(claim, OutcomeCode.FUNDING_SOURCE_MISMATCH)

        # An exact retry'''
if text.count(old) != 1:
    raise SystemExit(f"expected one hardening target, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
Path(__file__).unlink()
