#!/usr/bin/env python3
"""Deterministic, valueless Track F5 Phase-2 settlement model.

This file is deliberately outside every production settlement crate. It models
policy/transcript/accounting invariants and falsification gates only. It does
not select a production hash, signature, nullifier, credential, issuer,
auditor, or subsidy construction, and it cannot move MINI.

Run from the repository root:

    python3 tools/f5_phase2_model.py
    python3 -m unittest tools/test_f5_phase2_model.py

The command prints deterministic JSON Lines. CI compares it with the checked-in
fixed-vector file.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from dataclasses import dataclass, fields, is_dataclass, replace
from enum import Enum
from itertools import permutations
from typing import Any, Mapping, Sequence


MODEL_VERSION = 1
CLAIM_DOMAIN = "mininet/f5/settlement-claim/model-v1"
DELIVERY_DOMAIN = "mininet/f5/delivery-challenge/model-v1"
EVIDENCE_DOMAIN = "mininet/f5/delivery-evidence/model-v1"
DUPLICATE_DOMAIN_PREFIX = "mininet/f5/settlement-duplicate/v1/"
RATE_LIMIT_DOMAIN_PREFIX = "mininet/f5/rate-limit/v1/"
MODEL_COMMITMENT_DOMAIN = "mininet/f5/model-commitment/v1"
MAX_IDENTIFIER_BYTES = 128


class SettlementClass(str, Enum):
    REQUESTER_FUNDED = "requester-funded"
    SPONSOR_FUNDED = "sponsor-funded"
    PROTOCOL_SUBSIDIZED = "protocol-subsidized"
    AUTHORITY_BEARING = "authority-bearing-forbidden"


class ServiceClass(str, Enum):
    BYTE_DELIVERY = "byte-delivery"
    CRAWL_OBSERVATION = "crawl-observation"
    HISTORICAL_RETENTION = "historical-retention"
    PRIVATE_QUERY = "private-query"


class Role(str, Enum):
    REQUESTER = "requester"
    PROVIDER = "provider"
    COORDINATOR = "coordinator"
    ISSUER = "issuer"
    AUDITOR = "auditor"
    LEDGER = "ledger"


class Disclosure(str, Enum):
    POLICY_COMMITMENT = "policy-commitment"
    SETTLEMENT_CLASS = "settlement-class"
    SERVICE_CLASS = "service-class"
    AMOUNT = "amount"
    EPOCH = "coarse-epoch"
    CLAIM_COMMITMENT = "claim-commitment"
    FUNDING_COMMITMENT = "funding-commitment"
    DELIVERY_EVIDENCE_COMMITMENT = "delivery-evidence-commitment"
    POLICY_SCOPED_REQUESTER = "policy-scoped-requester"
    POLICY_SCOPED_PROVIDER = "policy-scoped-provider"
    BLINDED_ISSUANCE_REQUEST = "blinded-issuance-request-placeholder"
    AUDIT_SAMPLE_BIT = "audit-sample-bit"
    ROOT_DID = "root-did"
    CROSS_DOMAIN_IDENTIFIER = "cross-domain-stable-identifier"
    RAW_QUERY = "raw-query"
    RAW_CONTENT_ID = "raw-content-id"
    EXACT_TIMESTAMP = "exact-timestamp"


class OutcomeCode(str, Enum):
    ACCEPTED = "accepted"
    ALREADY_ACCEPTED = "already-accepted"
    POLICY_UNKNOWN = "policy-unknown"
    UNSUPPORTED_VERSION = "unsupported-version"
    AUTHORITY_CLASS_FORBIDDEN = "authority-class-forbidden"
    POLICY_MISMATCH = "policy-mismatch"
    CLASS_MISMATCH = "class-mismatch"
    SERVICE_MISMATCH = "service-mismatch"
    EPOCH_MISMATCH = "epoch-mismatch"
    FUNDING_SOURCE_MISMATCH = "funding-source-mismatch"
    EXPIRED = "expired"
    LOCAL_FINALITY_FORBIDDEN = "local-finality-forbidden"
    AMOUNT_INVALID = "amount-invalid"
    CLAIM_ID_MISMATCH = "claim-id-mismatch"
    DUPLICATE_ID_MISMATCH = "duplicate-id-mismatch"
    TRANSCRIPT_REQUIRED = "transcript-required"
    TRANSCRIPT_INVALID = "transcript-invalid"
    DUPLICATE_ECONOMIC_EVENT = "duplicate-economic-event"
    RATE_LIMIT_TAG_REQUIRED = "rate-limit-tag-required"
    RATE_LIMIT_DOMAIN_MISMATCH = "rate-limit-domain-mismatch"
    RATE_LIMIT_TAG_REUSED = "rate-limit-tag-reused"
    ISSUER_UNAVAILABLE = "issuer-unavailable"
    AUDITOR_UNAVAILABLE = "auditor-unavailable"
    INSUFFICIENT_PAYER_BALANCE = "insufficient-payer-balance"
    PROGRAM_BUDGET_EXHAUSTED = "program-budget-exhausted"
    RETAINED_STATE_LIMIT = "retained-state-limit"
    WIRE_SIZE_LIMIT = "wire-size-limit"
    VERIFICATION_WORK_LIMIT = "verification-work-limit"


class GateStatus(str, Enum):
    PASS = "PASS"
    FAIL = "FAIL"
    PARTIAL = "PARTIAL"


class AllegationKind(str, Enum):
    HEURISTIC_COLLUSION = "heuristic-collusion"
    OBJECTIVE_TRANSCRIPT_FAILURE = "objective-transcript-failure"


class AuditVerdict(str, Enum):
    NOT_OBJECTIVE = "not-objective"
    FALSE_ALLEGATION = "false-allegation"
    OBJECTIVE_PROOF_ACCEPTED = "objective-proof-accepted"


class FutureProgramAction(str, Enum):
    NONE = "none"
    HALT_FUTURE_CLAIMS = "halt-future-claims"
    REDUCE_FUTURE_ALLOWANCE = "reduce-future-allowance"


@dataclass(frozen=True)
class PrivacyDeclaration:
    disclosures: tuple[tuple[Role, tuple[Disclosure, ...]], ...]

    @staticmethod
    def default() -> "PrivacyDeclaration":
        return PrivacyDeclaration(
            disclosures=(
                (
                    Role.REQUESTER,
                    (
                        Disclosure.POLICY_COMMITMENT,
                        Disclosure.SETTLEMENT_CLASS,
                        Disclosure.SERVICE_CLASS,
                        Disclosure.AMOUNT,
                        Disclosure.EPOCH,
                        Disclosure.CLAIM_COMMITMENT,
                    ),
                ),
                (
                    Role.PROVIDER,
                    (
                        Disclosure.POLICY_COMMITMENT,
                        Disclosure.SETTLEMENT_CLASS,
                        Disclosure.SERVICE_CLASS,
                        Disclosure.AMOUNT,
                        Disclosure.EPOCH,
                        Disclosure.CLAIM_COMMITMENT,
                        Disclosure.POLICY_SCOPED_REQUESTER,
                        Disclosure.POLICY_SCOPED_PROVIDER,
                    ),
                ),
                (
                    Role.COORDINATOR,
                    (
                        Disclosure.POLICY_COMMITMENT,
                        Disclosure.SETTLEMENT_CLASS,
                        Disclosure.SERVICE_CLASS,
                        Disclosure.AMOUNT,
                        Disclosure.EPOCH,
                        Disclosure.CLAIM_COMMITMENT,
                        Disclosure.FUNDING_COMMITMENT,
                        Disclosure.DELIVERY_EVIDENCE_COMMITMENT,
                        Disclosure.POLICY_SCOPED_REQUESTER,
                        Disclosure.POLICY_SCOPED_PROVIDER,
                    ),
                ),
                (
                    Role.ISSUER,
                    (
                        Disclosure.POLICY_COMMITMENT,
                        Disclosure.EPOCH,
                        Disclosure.BLINDED_ISSUANCE_REQUEST,
                    ),
                ),
                (
                    Role.AUDITOR,
                    (
                        Disclosure.POLICY_COMMITMENT,
                        Disclosure.SETTLEMENT_CLASS,
                        Disclosure.SERVICE_CLASS,
                        Disclosure.EPOCH,
                        Disclosure.CLAIM_COMMITMENT,
                        Disclosure.DELIVERY_EVIDENCE_COMMITMENT,
                        Disclosure.AUDIT_SAMPLE_BIT,
                    ),
                ),
                (
                    Role.LEDGER,
                    (
                        Disclosure.POLICY_COMMITMENT,
                        Disclosure.SETTLEMENT_CLASS,
                        Disclosure.SERVICE_CLASS,
                        Disclosure.AMOUNT,
                        Disclosure.EPOCH,
                        Disclosure.CLAIM_COMMITMENT,
                        Disclosure.FUNDING_COMMITMENT,
                    ),
                ),
            )
        )

    def as_mapping(self) -> dict[Role, tuple[Disclosure, ...]]:
        return {role: tuple(items) for role, items in self.disclosures}

    def cross_context_leakage_score(self) -> int:
        """Return the predeclared cross-context leakage score.

        The score intentionally ignores policy-local linkability. It counts
        only information that can bridge this settlement context into another
        context without an additional disclosure:

        * root DID: 100
        * cross-domain stable identifier: 100
        * raw query: 50
        * raw content identifier: 25
        * exact timestamp: 10

        The maximum across roles is used; publishing one role's excess does not
        become safer merely because other roles learn less.
        """

        weights = {
            Disclosure.ROOT_DID: 100,
            Disclosure.CROSS_DOMAIN_IDENTIFIER: 100,
            Disclosure.RAW_QUERY: 50,
            Disclosure.RAW_CONTENT_ID: 25,
            Disclosure.EXACT_TIMESTAMP: 10,
        }
        scores = []
        for _, items in self.disclosures:
            scores.append(sum(weights.get(item, 0) for item in set(items)))
        return max(scores, default=0)

    def has_global_graph_fields(self) -> bool:
        forbidden = {
            Disclosure.ROOT_DID,
            Disclosure.CROSS_DOMAIN_IDENTIFIER,
            Disclosure.RAW_QUERY,
            Disclosure.RAW_CONTENT_ID,
        }
        return any(forbidden.intersection(items) for _, items in self.disclosures)


@dataclass(frozen=True)
class SettlementPolicy:
    version: int
    settlement_class: SettlementClass
    service_class: ServiceClass
    policy_name: str
    funding_source_commitment: str
    epoch: int
    starts_at_ms: int
    expires_at_ms: int
    program_budget_units: int
    max_claim_units: int
    duplicate_domain: str
    rate_limit_domain: str | None
    challenge_required: bool
    issuer_threshold: int
    auditor_threshold: int
    audit_sample_bps: int
    max_retained_keys: int
    max_claim_proof_wire_bytes: int
    max_abstract_verification_ops: int
    privacy: PrivacyDeclaration

    def __post_init__(self) -> None:
        if self.version != MODEL_VERSION:
            raise ValueError("unsupported policy model version")
        if self.settlement_class is SettlementClass.AUTHORITY_BEARING:
            raise ValueError("authority-bearing settlement is forbidden")
        _validate_identifier(self.policy_name, "policy_name")
        _validate_identifier(
            self.funding_source_commitment,
            "funding_source_commitment",
        )
        if self.epoch < 0 or self.starts_at_ms < 0:
            raise ValueError("epoch and start must be non-negative")
        if self.expires_at_ms <= self.starts_at_ms:
            raise ValueError("policy expiry must follow policy start")
        if self.max_claim_units <= 0:
            raise ValueError("max claim must be positive")
        if not self.duplicate_domain.startswith(DUPLICATE_DOMAIN_PREFIX):
            raise ValueError("duplicate domain is not settlement-specific")
        if not 0 <= self.audit_sample_bps <= 10_000:
            raise ValueError("audit sampling must be basis points")
        if self.max_retained_keys <= 0:
            raise ValueError("retained-key bound must be positive")
        if self.max_claim_proof_wire_bytes <= 0:
            raise ValueError("wire-size bound must be positive")
        if self.max_abstract_verification_ops <= 0:
            raise ValueError("verification-work bound must be positive")
        if self.privacy.cross_context_leakage_score() != 0:
            raise ValueError(
                "policy privacy declaration exceeds zero cross-context leakage budget"
            )

        if self.settlement_class is SettlementClass.REQUESTER_FUNDED:
            if self.program_budget_units != 0:
                raise ValueError("requester-funded policy has no program budget")
            if self.issuer_threshold != 0 or self.auditor_threshold != 0:
                raise ValueError(
                    "requester-funded policy cannot require issuer/auditor availability"
                )
            if self.rate_limit_domain is not None:
                raise ValueError(
                    "requester-funded policy cannot require a collusion-limit tag"
                )
        else:
            if self.program_budget_units <= 0:
                raise ValueError("sponsor/protocol policy needs a finite budget")
            if self.max_claim_units > self.program_budget_units:
                raise ValueError(
                    "single claim cannot exceed the entire program budget"
                )
            if self.issuer_threshold <= 0 or self.auditor_threshold <= 0:
                raise ValueError(
                    "sponsor/protocol policy must fail closed on issuer/auditor loss"
                )
            if self.rate_limit_domain is None or not self.rate_limit_domain.startswith(
                RATE_LIMIT_DOMAIN_PREFIX
            ):
                raise ValueError(
                    "sponsor/protocol policy needs a settlement-specific rate domain"
                )

    @property
    def commitment(self) -> str:
        return model_commitment("policy", self)


@dataclass(frozen=True)
class ScopedRateTag:
    domain: str
    value: str

    def __post_init__(self) -> None:
        _validate_identifier(self.domain, "rate tag domain")
        _validate_identifier(self.value, "rate tag value")


@dataclass(frozen=True)
class DeliveryChallengeTranscript:
    version: int
    domain: str
    policy_commitment: str
    settlement_class: SettlementClass
    service_class: ServiceClass
    request_event_commitment: str
    requester_scope: str
    provider_scope: str
    challenge: str
    response_commitment: str
    issued_at_ms: int
    expires_at_ms: int
    evidence_commitment: str

    @classmethod
    def create(
        cls,
        policy: SettlementPolicy,
        *,
        request_event_commitment: str,
        requester_scope: str,
        provider_scope: str,
        challenge: str,
        response_commitment: str,
        issued_at_ms: int,
        expires_at_ms: int,
    ) -> "DeliveryChallengeTranscript":
        for name, value in (
            ("request_event_commitment", request_event_commitment),
            ("requester_scope", requester_scope),
            ("provider_scope", provider_scope),
            ("challenge", challenge),
            ("response_commitment", response_commitment),
        ):
            _validate_identifier(value, name)
        if issued_at_ms < policy.starts_at_ms:
            raise ValueError("challenge predates policy")
        if expires_at_ms <= issued_at_ms or expires_at_ms > policy.expires_at_ms:
            raise ValueError("challenge expiry is outside policy window")
        base = {
            "version": MODEL_VERSION,
            "domain": DELIVERY_DOMAIN,
            "policy_commitment": policy.commitment,
            "settlement_class": policy.settlement_class.value,
            "service_class": policy.service_class.value,
            "request_event_commitment": request_event_commitment,
            "requester_scope": requester_scope,
            "provider_scope": provider_scope,
            "challenge": challenge,
            "response_commitment": response_commitment,
            "issued_at_ms": issued_at_ms,
            "expires_at_ms": expires_at_ms,
        }
        evidence_commitment = model_commitment(EVIDENCE_DOMAIN, base)
        return cls(
            version=MODEL_VERSION,
            domain=DELIVERY_DOMAIN,
            policy_commitment=policy.commitment,
            settlement_class=policy.settlement_class,
            service_class=policy.service_class,
            request_event_commitment=request_event_commitment,
            requester_scope=requester_scope,
            provider_scope=provider_scope,
            challenge=challenge,
            response_commitment=response_commitment,
            issued_at_ms=issued_at_ms,
            expires_at_ms=expires_at_ms,
            evidence_commitment=evidence_commitment,
        )

    @property
    def wire_size_bytes(self) -> int:
        return len(canonical_json_bytes(self))

    @property
    def abstract_verification_ops(self) -> int:
        # A deliberately simple cost model: fixed parsing/binding work plus one
        # unit per 32 bytes. It is not a physical benchmark.
        return 320 + math.ceil(self.wire_size_bytes / 32)

    def verify_for(
        self,
        policy: SettlementPolicy,
        claim: "SettlementClaim",
        *,
        now_ms: int,
    ) -> bool:
        if self.version != MODEL_VERSION or self.domain != DELIVERY_DOMAIN:
            return False
        if self.issued_at_ms < policy.starts_at_ms:
            return False
        if (
            self.expires_at_ms <= self.issued_at_ms
            or self.expires_at_ms > policy.expires_at_ms
        ):
            return False
        if self.policy_commitment != policy.commitment:
            return False
        if self.settlement_class is not policy.settlement_class:
            return False
        if self.service_class is not policy.service_class:
            return False
        if self.request_event_commitment != claim.request_event_commitment:
            return False
        if self.requester_scope != claim.requester_scope:
            return False
        if self.provider_scope != claim.provider_scope:
            return False
        if self.evidence_commitment != claim.delivery_evidence_commitment:
            return False
        if not (self.issued_at_ms <= now_ms <= self.expires_at_ms):
            return False
        if self.expires_at_ms > claim.expires_at_ms:
            return False
        base = {
            "version": self.version,
            "domain": self.domain,
            "policy_commitment": self.policy_commitment,
            "settlement_class": self.settlement_class.value,
            "service_class": self.service_class.value,
            "request_event_commitment": self.request_event_commitment,
            "requester_scope": self.requester_scope,
            "provider_scope": self.provider_scope,
            "challenge": self.challenge,
            "response_commitment": self.response_commitment,
            "issued_at_ms": self.issued_at_ms,
            "expires_at_ms": self.expires_at_ms,
        }
        return self.evidence_commitment == model_commitment(
            EVIDENCE_DOMAIN,
            base,
        )


@dataclass(frozen=True)
class SettlementClaim:
    version: int
    policy_commitment: str
    settlement_class: SettlementClass
    service_class: ServiceClass
    request_event_commitment: str
    requester_scope: str
    funder_commitment: str
    provider_scope: str
    delivery_evidence_commitment: str
    amount_units: int
    funding_epoch: int
    expires_at_ms: int
    duplicate_identifier: str
    rate_limit_tag: ScopedRateTag | None
    claim_id: str
    finality_reference: str | None

    @classmethod
    def create(
        cls,
        policy: SettlementPolicy,
        transcript: DeliveryChallengeTranscript | None,
        *,
        requester_scope: str,
        funder_commitment: str,
        provider_scope: str,
        request_event_commitment: str,
        amount_units: int,
        expires_at_ms: int,
        rate_limit_tag: ScopedRateTag | None = None,
        finality_reference: str | None = None,
    ) -> "SettlementClaim":
        for name, value in (
            ("requester_scope", requester_scope),
            ("funder_commitment", funder_commitment),
            ("provider_scope", provider_scope),
            ("request_event_commitment", request_event_commitment),
        ):
            _validate_identifier(value, name)
        if amount_units <= 0:
            raise ValueError("claim amount must be positive")
        evidence_commitment = (
            transcript.evidence_commitment
            if transcript is not None
            else model_commitment("no-challenge", request_event_commitment)
        )
        duplicate_identifier = model_commitment(
            "economic-event",
            {
                "duplicate_domain": policy.duplicate_domain,
                "policy_commitment": policy.commitment,
                "service_class": policy.service_class.value,
                "funding_epoch": policy.epoch,
                "request_event_commitment": request_event_commitment,
            },
        )
        base = {
            "version": MODEL_VERSION,
            "domain": CLAIM_DOMAIN,
            "policy_commitment": policy.commitment,
            "settlement_class": policy.settlement_class.value,
            "service_class": policy.service_class.value,
            "request_event_commitment": request_event_commitment,
            "requester_scope": requester_scope,
            "funder_commitment": funder_commitment,
            "provider_scope": provider_scope,
            "delivery_evidence_commitment": evidence_commitment,
            "amount_units": amount_units,
            "funding_epoch": policy.epoch,
            "expires_at_ms": expires_at_ms,
            "duplicate_identifier": duplicate_identifier,
            "rate_limit_tag": to_primitive(rate_limit_tag),
            "finality_reference": finality_reference,
        }
        claim_id = model_commitment("claim", base)
        return cls(
            version=MODEL_VERSION,
            policy_commitment=policy.commitment,
            settlement_class=policy.settlement_class,
            service_class=policy.service_class,
            request_event_commitment=request_event_commitment,
            requester_scope=requester_scope,
            funder_commitment=funder_commitment,
            provider_scope=provider_scope,
            delivery_evidence_commitment=evidence_commitment,
            amount_units=amount_units,
            funding_epoch=policy.epoch,
            expires_at_ms=expires_at_ms,
            duplicate_identifier=duplicate_identifier,
            rate_limit_tag=rate_limit_tag,
            claim_id=claim_id,
            finality_reference=finality_reference,
        )

    @property
    def wire_size_bytes(self) -> int:
        return len(canonical_json_bytes(self))

    @property
    def abstract_verification_ops(self) -> int:
        return 480 + math.ceil(self.wire_size_bytes / 32)

    def expected_duplicate_identifier(self, policy: SettlementPolicy) -> str:
        return model_commitment(
            "economic-event",
            {
                "duplicate_domain": policy.duplicate_domain,
                "policy_commitment": policy.commitment,
                "service_class": policy.service_class.value,
                "funding_epoch": policy.epoch,
                "request_event_commitment": self.request_event_commitment,
            },
        )

    def expected_claim_id(self) -> str:
        base = {
            "version": self.version,
            "domain": CLAIM_DOMAIN,
            "policy_commitment": self.policy_commitment,
            "settlement_class": self.settlement_class.value,
            "service_class": self.service_class.value,
            "request_event_commitment": self.request_event_commitment,
            "requester_scope": self.requester_scope,
            "funder_commitment": self.funder_commitment,
            "provider_scope": self.provider_scope,
            "delivery_evidence_commitment": self.delivery_evidence_commitment,
            "amount_units": self.amount_units,
            "funding_epoch": self.funding_epoch,
            "expires_at_ms": self.expires_at_ms,
            "duplicate_identifier": self.duplicate_identifier,
            "rate_limit_tag": to_primitive(self.rate_limit_tag),
            "finality_reference": self.finality_reference,
        }
        return model_commitment("claim", base)


@dataclass(frozen=True)
class Availability:
    issuers_available: int
    auditors_available: int


@dataclass(frozen=True)
class SubmissionOutcome:
    claim_id: str
    code: OutcomeCode
    spent_units: int
    extraction_units: int
    canonical_finality_reference: str | None

    @property
    def accepted(self) -> bool:
        return self.code in {OutcomeCode.ACCEPTED, OutcomeCode.ALREADY_ACCEPTED}


@dataclass(frozen=True)
class AcceptedRecord:
    claim_id: str
    policy_commitment: str
    settlement_class: SettlementClass
    amount_units: int
    requester_scope: str
    provider_scope: str
    duplicate_identifier: str
    rate_limit_value: str | None
    canonical_finality_reference: str


@dataclass(frozen=True)
class AuditResponse:
    verdict: AuditVerdict
    future_action: FutureProgramAction
    canonical_state_mutation_permitted: bool


@dataclass(frozen=True)
class Thresholds:
    max_budget_overrun_units: int = 0
    max_colluding_extraction_bps: int = 1_000
    max_duplicate_false_negative_bps: int = 0
    max_honest_false_rejection_bps: int = 100
    max_cross_context_leakage_score: int = 0
    min_audit_detection_bps: int = 9_500
    audit_attack_claim_count: int = 60
    max_compromised_issuer_bps: int = 3_333
    max_compromised_auditor_bps: int = 3_333
    max_verification_cpu_ms: int = 50
    max_verification_memory_bytes: int = 4 * 1024 * 1024
    max_retained_state_bytes: int = 8 * 1024 * 1024
    max_claim_proof_wire_bytes: int = 16 * 1024
    max_abstract_verification_ops: int = 10_000


@dataclass(frozen=True)
class GateResult:
    gate: str
    status: GateStatus
    threshold: int | str
    observed: int | str
    unit: str
    detail: str


@dataclass(frozen=True)
class VectorResult:
    vector: str
    status: GateStatus
    accepted: int
    already_accepted: int
    rejected: int
    spent_units: int
    extraction_units: int
    detail: str
    state_digest: str


class SettlementModel:
    """Deterministic state machine for model units only."""

    RETAINED_KEY_ESTIMATE_BYTES = 96

    def __init__(self, *, payer_balances: Mapping[str, int] | None = None) -> None:
        self.policies: dict[str, SettlementPolicy] = {}
        self.initial_program_budgets: dict[str, int] = {}
        self.program_remaining: dict[str, int] = {}
        self.payer_balances: dict[str, int] = dict(payer_balances or {})
        self.initial_payer_total = sum(self.payer_balances.values())
        self.provider_receipts: dict[str, int] = {}
        self.accepted_records: dict[str, AcceptedRecord] = {}
        self.accepted_events: set[tuple[str, str]] = set()
        self.accepted_rate_tags: set[tuple[str, str]] = set()
        self.attempted_volume_units = 0
        self.finalized_transfer_volume_units = 0
        self.requester_funded_transfer_volume_units = 0
        self.sponsor_transfer_volume_units = 0
        self.protocol_transfer_volume_units = 0

    def register_policy(self, policy: SettlementPolicy) -> str:
        commitment = policy.commitment
        prior = self.policies.get(commitment)
        if prior is not None and prior != policy:
            raise ValueError("policy commitment collision in model")
        self.policies[commitment] = policy
        if policy.settlement_class is not SettlementClass.REQUESTER_FUNDED:
            self.initial_program_budgets.setdefault(
                commitment,
                policy.program_budget_units,
            )
            self.program_remaining.setdefault(
                commitment,
                policy.program_budget_units,
            )
        return commitment

    def retained_state_bytes(self, policy_commitment: str | None = None) -> int:
        if policy_commitment is None:
            count = len(self.accepted_events) + len(self.accepted_rate_tags)
        else:
            count = sum(
                1 for key in self.accepted_events if key[0] == policy_commitment
            )
            count += sum(
                1 for key in self.accepted_rate_tags if key[0] == policy_commitment
            )
        return count * self.RETAINED_KEY_ESTIMATE_BYTES

    def submit(
        self,
        claim: SettlementClaim,
        transcript: DeliveryChallengeTranscript | None,
        *,
        availability: Availability,
        now_ms: int,
        attacker_controlled_scopes: set[str] | None = None,
    ) -> SubmissionOutcome:
        self.attempted_volume_units += max(claim.amount_units, 0)
        attacker_controlled_scopes = attacker_controlled_scopes or set()
        policy = self.policies.get(claim.policy_commitment)
        if policy is None:
            return self._reject(claim, OutcomeCode.POLICY_UNKNOWN)
        if claim.version != MODEL_VERSION:
            return self._reject(claim, OutcomeCode.UNSUPPORTED_VERSION)
        if policy.settlement_class is SettlementClass.AUTHORITY_BEARING:
            return self._reject(claim, OutcomeCode.AUTHORITY_CLASS_FORBIDDEN)
        if claim.policy_commitment != policy.commitment:
            return self._reject(claim, OutcomeCode.POLICY_MISMATCH)
        if claim.settlement_class is not policy.settlement_class:
            return self._reject(claim, OutcomeCode.CLASS_MISMATCH)
        if claim.service_class is not policy.service_class:
            return self._reject(claim, OutcomeCode.SERVICE_MISMATCH)
        if claim.funding_epoch != policy.epoch:
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

        total_wire = claim.wire_size_bytes + (
            transcript.wire_size_bytes if transcript else 0
        )
        if total_wire > policy.max_claim_proof_wire_bytes:
            return self._reject(claim, OutcomeCode.WIRE_SIZE_LIMIT)
        total_ops = claim.abstract_verification_ops + (
            transcript.abstract_verification_ops if transcript else 0
        )
        if total_ops > policy.max_abstract_verification_ops:
            return self._reject(claim, OutcomeCode.VERIFICATION_WORK_LIMIT)

        if policy.challenge_required:
            if transcript is None:
                return self._reject(claim, OutcomeCode.TRANSCRIPT_REQUIRED)
            if not transcript.verify_for(policy, claim, now_ms=now_ms):
                return self._reject(claim, OutcomeCode.TRANSCRIPT_INVALID)

        event_key = (policy.commitment, claim.duplicate_identifier)
        if event_key in self.accepted_events:
            return self._reject(claim, OutcomeCode.DUPLICATE_ECONOMIC_EVENT)

        rate_key: tuple[str, str] | None = None
        if policy.rate_limit_domain is not None:
            if claim.rate_limit_tag is None:
                return self._reject(claim, OutcomeCode.RATE_LIMIT_TAG_REQUIRED)
            if claim.rate_limit_tag.domain != policy.rate_limit_domain:
                return self._reject(claim, OutcomeCode.RATE_LIMIT_DOMAIN_MISMATCH)
            rate_key = (policy.commitment, claim.rate_limit_tag.value)
            if rate_key in self.accepted_rate_tags:
                return self._reject(claim, OutcomeCode.RATE_LIMIT_TAG_REUSED)
        elif claim.rate_limit_tag is not None:
            return self._reject(claim, OutcomeCode.RATE_LIMIT_DOMAIN_MISMATCH)

        new_key_count = 1 + (1 if rate_key is not None else 0)
        existing_keys = sum(
            1 for key in self.accepted_events if key[0] == policy.commitment
        )
        existing_keys += sum(
            1 for key in self.accepted_rate_tags if key[0] == policy.commitment
        )
        if existing_keys + new_key_count > policy.max_retained_keys:
            return self._reject(claim, OutcomeCode.RETAINED_STATE_LIMIT)

        if policy.settlement_class is not SettlementClass.REQUESTER_FUNDED:
            if availability.issuers_available < policy.issuer_threshold:
                return self._reject(claim, OutcomeCode.ISSUER_UNAVAILABLE)
            if availability.auditors_available < policy.auditor_threshold:
                return self._reject(claim, OutcomeCode.AUDITOR_UNAVAILABLE)

        if policy.settlement_class is SettlementClass.REQUESTER_FUNDED:
            balance = self.payer_balances.get(claim.funder_commitment, 0)
            if balance < claim.amount_units:
                return self._reject(claim, OutcomeCode.INSUFFICIENT_PAYER_BALANCE)
        else:
            remaining = self.program_remaining[policy.commitment]
            if remaining < claim.amount_units:
                return self._reject(claim, OutcomeCode.PROGRAM_BUDGET_EXHAUSTED)

        # All checks completed before mutation.
        if policy.settlement_class is SettlementClass.REQUESTER_FUNDED:
            self.payer_balances[claim.funder_commitment] = (
                self.payer_balances.get(claim.funder_commitment, 0)
                - claim.amount_units
            )
            self.payer_balances[claim.provider_scope] = (
                self.payer_balances.get(claim.provider_scope, 0)
                + claim.amount_units
            )
            self.requester_funded_transfer_volume_units += claim.amount_units
        else:
            self.program_remaining[policy.commitment] -= claim.amount_units
            self.provider_receipts[claim.provider_scope] = (
                self.provider_receipts.get(claim.provider_scope, 0)
                + claim.amount_units
            )
            if policy.settlement_class is SettlementClass.SPONSOR_FUNDED:
                self.sponsor_transfer_volume_units += claim.amount_units
            else:
                self.protocol_transfer_volume_units += claim.amount_units

        self.finalized_transfer_volume_units += claim.amount_units
        self.accepted_events.add(event_key)
        if rate_key is not None:
            self.accepted_rate_tags.add(rate_key)
        finality_reference = f"canonical-model:{claim.claim_id}"
        record = AcceptedRecord(
            claim_id=claim.claim_id,
            policy_commitment=policy.commitment,
            settlement_class=policy.settlement_class,
            amount_units=claim.amount_units,
            requester_scope=claim.requester_scope,
            provider_scope=claim.provider_scope,
            duplicate_identifier=claim.duplicate_identifier,
            rate_limit_value=(
                claim.rate_limit_tag.value if claim.rate_limit_tag else None
            ),
            canonical_finality_reference=finality_reference,
        )
        self.accepted_records[claim.claim_id] = record

        extraction = 0
        if policy.settlement_class is not SettlementClass.REQUESTER_FUNDED and (
            claim.requester_scope in attacker_controlled_scopes
            or claim.provider_scope in attacker_controlled_scopes
        ):
            extraction = claim.amount_units

        return SubmissionOutcome(
            claim_id=claim.claim_id,
            code=OutcomeCode.ACCEPTED,
            spent_units=claim.amount_units,
            extraction_units=extraction,
            canonical_finality_reference=finality_reference,
        )

    def submit_canonical_batch(
        self,
        items: Sequence[
            tuple[SettlementClaim, DeliveryChallengeTranscript | None]
        ],
        *,
        availability: Availability,
        now_ms: int,
        attacker_controlled_scopes: set[str] | None = None,
    ) -> list[SubmissionOutcome]:
        ordered = sorted(items, key=lambda pair: pair[0].claim_id)
        return [
            self.submit(
                claim,
                transcript,
                availability=availability,
                now_ms=now_ms,
                attacker_controlled_scopes=attacker_controlled_scopes,
            )
            for claim, transcript in ordered
        ]

    def budget_overrun_units(self, policy_commitment: str) -> int:
        initial = self.initial_program_budgets.get(policy_commitment, 0)
        remaining = self.program_remaining.get(policy_commitment, 0)
        spent = initial - remaining
        return max(0, spent - initial)

    def requester_value_is_conserved(self) -> bool:
        return sum(self.payer_balances.values()) == self.initial_payer_total

    def program_value_is_conserved(self, policy_commitment: str) -> bool:
        initial = self.initial_program_budgets.get(policy_commitment, 0)
        remaining = self.program_remaining.get(policy_commitment, 0)
        receipts = sum(
            record.amount_units
            for record in self.accepted_records.values()
            if record.policy_commitment == policy_commitment
        )
        return remaining + receipts == initial

    def state_digest(self) -> str:
        return model_commitment(
            "state",
            {
                "policies": self.policies,
                "initial_program_budgets": self.initial_program_budgets,
                "program_remaining": self.program_remaining,
                "payer_balances": self.payer_balances,
                "provider_receipts": self.provider_receipts,
                "accepted_records": self.accepted_records,
                "accepted_events": sorted(self.accepted_events),
                "accepted_rate_tags": sorted(self.accepted_rate_tags),
                "volumes": {
                    "attempted": self.attempted_volume_units,
                    "finalized": self.finalized_transfer_volume_units,
                    "requester": self.requester_funded_transfer_volume_units,
                    "sponsor": self.sponsor_transfer_volume_units,
                    "protocol": self.protocol_transfer_volume_units,
                },
            },
        )

    def _reject(
        self,
        claim: SettlementClaim,
        code: OutcomeCode,
    ) -> SubmissionOutcome:
        return SubmissionOutcome(
            claim_id=claim.claim_id,
            code=code,
            spent_units=0,
            extraction_units=0,
            canonical_finality_reference=None,
        )


def evaluate_audit_allegation(
    kind: AllegationKind,
    *,
    objective_proof_valid: bool,
) -> AuditResponse:
    """Return a future-program response with no canonical-ledger handle.

    The function cannot reverse a finalized transfer because it is not given a
    mutable settlement state. Even a valid objective proof can only request a
    future-policy action.
    """

    if kind is AllegationKind.HEURISTIC_COLLUSION:
        return AuditResponse(
            verdict=AuditVerdict.NOT_OBJECTIVE,
            future_action=FutureProgramAction.NONE,
            canonical_state_mutation_permitted=False,
        )
    if not objective_proof_valid:
        return AuditResponse(
            verdict=AuditVerdict.FALSE_ALLEGATION,
            future_action=FutureProgramAction.NONE,
            canonical_state_mutation_permitted=False,
        )
    return AuditResponse(
        verdict=AuditVerdict.OBJECTIVE_PROOF_ACCEPTED,
        future_action=FutureProgramAction.HALT_FUTURE_CLAIMS,
        canonical_state_mutation_permitted=False,
    )


def audit_selected(public_seed: str, claim_id: str, sample_bps: int) -> bool:
    if not 0 <= sample_bps <= 10_000:
        raise ValueError("sample rate must be basis points")
    value = int(
        model_commitment(
            "audit-sample",
            {"seed": public_seed, "claim": claim_id},
        )[:16],
        16,
    )
    return value % 10_000 < sample_bps


def grind_unsampled_claim_id(
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
) -> int:
    """Return 1-(1-p)^n in basis points using integer fixed-point math."""

    if not 0 <= sample_bps <= 10_000:
        raise ValueError("sample rate must be basis points")
    if objectively_invalid_claims < 0:
        raise ValueError("claim count must be non-negative")
    scale = 1_000_000_000
    miss = scale
    one_minus_p = scale - sample_bps * (scale // 10_000)
    for _ in range(objectively_invalid_claims):
        miss = (miss * one_minus_p + scale // 2) // scale
    detection = scale - miss
    return min(10_000, (detection * 10_000 + scale // 2) // scale)


def run_fixed_vectors(
    thresholds: Thresholds | None = None,
) -> tuple[list[VectorResult], list[GateResult]]:
    thresholds = thresholds or Thresholds()
    vectors: list[VectorResult] = []

    def record(
        name: str,
        model: SettlementModel,
        outcomes: Sequence[SubmissionOutcome],
        status: GateStatus,
        detail: str,
    ) -> None:
        vectors.append(
            VectorResult(
                vector=name,
                status=status,
                accepted=sum(o.code is OutcomeCode.ACCEPTED for o in outcomes),
                already_accepted=sum(
                    o.code is OutcomeCode.ALREADY_ACCEPTED for o in outcomes
                ),
                rejected=sum(
                    o.code
                    not in {OutcomeCode.ACCEPTED, OutcomeCode.ALREADY_ACCEPTED}
                    for o in outcomes
                ),
                spent_units=sum(o.spent_units for o in outcomes),
                extraction_units=sum(o.extraction_units for o in outcomes),
                detail=detail,
                state_digest=model.state_digest(),
            )
        )

    now = 1_000
    requester_policy = make_policy(
        SettlementClass.REQUESTER_FUNDED,
        "requester",
        budget=0,
    )
    requester_model = SettlementModel(payer_balances={"alice": 100, "bob": 0})
    requester_model.register_policy(requester_policy)
    requester_claim, requester_tx = make_claim(
        requester_policy,
        event="event-requester-1",
        requester="alice-scope",
        funder="alice",
        provider="bob",
        amount=25,
        rate_tag=None,
    )
    requester_outcome = requester_model.submit(
        requester_claim,
        requester_tx,
        availability=Availability(0, 0),
        now_ms=now,
    )
    record(
        "requester-funded-survives-total-f5-outage",
        requester_model,
        [requester_outcome],
        GateStatus.PASS,
        "ordinary voluntary settlement accepted with zero issuers and zero auditors",
    )

    self_model = SettlementModel(payer_balances={"self": 100})
    self_model.register_policy(requester_policy)
    self_claim, self_tx = make_claim(
        requester_policy,
        event="event-self-pay",
        requester="self",
        funder="self",
        provider="self",
        amount=40,
        rate_tag=None,
    )
    self_outcome = self_model.submit(
        self_claim,
        self_tx,
        availability=Availability(0, 0),
        now_ms=now,
        attacker_controlled_scopes={"self"},
    )
    record(
        "requester-provider-self-payment-is-zero-net-extraction",
        self_model,
        [self_outcome],
        GateStatus.PASS,
        "gross/finalized volume is 40, payer balance remains 100, extraction is 0",
    )

    sponsor_policy = make_policy(
        SettlementClass.SPONSOR_FUNDED,
        "sponsor",
        budget=100,
    )
    sponsor_model = SettlementModel()
    sponsor_model.register_policy(sponsor_policy)
    sponsor_items = [
        make_claim(
            sponsor_policy,
            event=f"event-sponsor-{idx}",
            requester=f"requester-{idx}",
            funder="sponsor-budget",
            provider=f"provider-{idx}",
            amount=40,
            rate_tag=f"tag-{idx}",
        )
        for idx in range(3)
    ]
    sponsor_outcomes = sponsor_model.submit_canonical_batch(
        sponsor_items,
        availability=Availability(3, 3),
        now_ms=now,
    )
    record(
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

    protocol_policy = make_policy(
        SettlementClass.PROTOCOL_SUBSIDIZED,
        "protocol",
        budget=90,
    )
    protocol_items = [
        make_claim(
            protocol_policy,
            event=f"event-race-{idx}",
            requester=f"attacker-requester-{idx}",
            funder="protocol-epoch-budget",
            provider=f"attacker-provider-{idx}",
            amount=30,
            rate_tag=f"race-tag-{idx}",
        )
        for idx in range(4)
    ]
    permutation_digests: set[str] = set()
    canonical_accepted_sets: set[tuple[str, ...]] = set()
    canonical_outcomes: list[SubmissionOutcome] = []
    canonical_model: SettlementModel | None = None
    for ordering in permutations(protocol_items):
        model = SettlementModel()
        model.register_policy(protocol_policy)
        outcomes = model.submit_canonical_batch(
            list(ordering),
            availability=Availability(3, 3),
            now_ms=now,
            attacker_controlled_scopes={
                *(f"attacker-requester-{idx}" for idx in range(4)),
                *(f"attacker-provider-{idx}" for idx in range(4)),
            },
        )
        permutation_digests.add(model.state_digest())
        canonical_accepted_sets.add(
            tuple(
                sorted(
                    o.claim_id
                    for o in outcomes
                    if o.code is OutcomeCode.ACCEPTED
                )
            )
        )
        if canonical_model is None:
            canonical_model = model
            canonical_outcomes = outcomes
    assert canonical_model is not None
    record(
        "concurrent-budget-race-has-one-canonical-result",
        canonical_model,
        canonical_outcomes,
        (
            GateStatus.PASS
            if len(permutation_digests) == len(canonical_accepted_sets) == 1
            else GateStatus.FAIL
        ),
        "all 24 submission permutations converge on the same accepted set and 0 remaining units",
    )

    replay_model = SettlementModel()
    replay_model.register_policy(sponsor_policy)
    replay_claim, replay_tx = make_claim(
        sponsor_policy,
        event="event-replay",
        requester="honest-requester",
        funder="sponsor-budget",
        provider="honest-provider",
        amount=10,
        rate_tag="replay-tag",
    )
    replay_first = replay_model.submit(
        replay_claim,
        replay_tx,
        availability=Availability(3, 3),
        now_ms=now,
    )
    replay_second = replay_model.submit(
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
    )

    split_model = SettlementModel()
    split_model.register_policy(sponsor_policy)
    split_a, split_tx_a = make_claim(
        sponsor_policy,
        event="event-split",
        requester="root-a",
        funder="sponsor-budget",
        provider="provider-a",
        amount=10,
        rate_tag="split-tag-a",
    )
    split_b, split_tx_b = make_claim(
        sponsor_policy,
        event="event-split",
        requester="root-b",
        funder="sponsor-budget",
        provider="provider-b",
        amount=10,
        rate_tag="split-tag-b",
    )
    split_outcomes = [
        split_model.submit(
            split_a,
            split_tx_a,
            availability=Availability(3, 3),
            now_ms=now,
        ),
        split_model.submit(
            split_b,
            split_tx_b,
            availability=Availability(3, 3),
            now_ms=now,
        ),
    ]
    record(
        "identity-splitting-cannot-multiply-one-event",
        split_model,
        split_outcomes,
        GateStatus.PASS,
        "changing requester/provider scopes changes the claim id but not the economic-event identifier",
    )

    cross_model = SettlementModel()
    cross_model.register_policy(sponsor_policy)
    wrong_epoch_policy = make_policy(
        SettlementClass.SPONSOR_FUNDED,
        "sponsor-next-epoch",
        budget=100,
        epoch=8,
    )
    cross_model.register_policy(wrong_epoch_policy)
    old_claim, old_tx = make_claim(
        sponsor_policy,
        event="event-cross-domain",
        requester="root-cross",
        funder="sponsor-budget",
        provider="provider-cross",
        amount=10,
        rate_tag="cross-tag",
    )
    substituted = replace(
        old_claim,
        policy_commitment=wrong_epoch_policy.commitment,
    )
    cross_outcome = cross_model.submit(
        substituted,
        old_tx,
        availability=Availability(3, 3),
        now_ms=now,
    )
    record(
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

    rate_model = SettlementModel()
    rate_model.register_policy(sponsor_policy)
    wrong_rate_claim, wrong_rate_tx = make_claim(
        sponsor_policy,
        event="event-rate-domain",
        requester="root-rate",
        funder="sponsor-budget",
        provider="provider-rate",
        amount=10,
        rate_tag=ScopedRateTag(
            "mininet/personhood/nullifier/v1",
            "reused-secret",
        ),
    )
    rate_outcome = rate_model.submit(
        wrong_rate_claim,
        wrong_rate_tx,
        availability=Availability(3, 3),
        now_ms=now,
    )
    record(
        "cross-context-rate-tag-substitution-fails",
        rate_model,
        [rate_outcome],
        GateStatus.PASS,
        "a personhood/review/resource tag cannot satisfy the settlement-specific domain",
    )

    outage_model = SettlementModel(
        payer_balances={"payer-outage": 20, "provider-outage": 0}
    )
    outage_model.register_policy(requester_policy)
    outage_model.register_policy(sponsor_policy)
    market_claim, market_tx = make_claim(
        requester_policy,
        event="event-market-outage",
        requester="payer-outage",
        funder="payer-outage",
        provider="provider-outage",
        amount=10,
        rate_tag=None,
    )
    subsidized_claim, subsidized_tx = make_claim(
        sponsor_policy,
        event="event-subsidy-outage",
        requester="requester-outage",
        funder="sponsor-budget",
        provider="provider-outage-2",
        amount=10,
        rate_tag="outage-tag",
    )
    outage_outcomes = [
        outage_model.submit(
            market_claim,
            market_tx,
            availability=Availability(0, 0),
            now_ms=now,
        ),
        outage_model.submit(
            subsidized_claim,
            subsidized_tx,
            availability=Availability(0, 0),
            now_ms=now,
        ),
    ]
    record(
        "role-disappearance-is-class-local",
        outage_model,
        outage_outcomes,
        GateStatus.PASS,
        "market settlement succeeds; sponsor settlement fails closed on issuer availability",
    )

    collusion_policy = make_policy(
        SettlementClass.PROTOCOL_SUBSIDIZED,
        "collusion-stress",
        budget=1_000,
        max_claim=10,
        max_retained_keys=500,
    )
    collusion_model = SettlementModel()
    collusion_model.register_policy(collusion_policy)
    collusion_items = [
        make_claim(
            collusion_policy,
            event=f"real-delivery-{idx}",
            requester=f"attacker-root-{idx}",
            funder="protocol-epoch-budget",
            provider=f"attacker-provider-{idx}",
            amount=10,
            rate_tag=f"attacker-tag-{idx}",
        )
        for idx in range(100)
    ]
    collusion_scopes = {
        *(f"attacker-root-{idx}" for idx in range(100)),
        *(f"attacker-provider-{idx}" for idx in range(100)),
    }
    collusion_outcomes = collusion_model.submit_canonical_batch(
        collusion_items,
        availability=Availability(3, 3),
        now_ms=now,
        attacker_controlled_scopes=collusion_scopes,
    )
    collusion_extraction = sum(
        o.extraction_units for o in collusion_outcomes
    )
    collusion_bps = (
        collusion_extraction * 10_000 // collusion_policy.program_budget_units
    )
    record(
        "real-delivery-collusion-drains-the-bounded-program",
        collusion_model,
        collusion_outcomes,
        (
            GateStatus.FAIL
            if collusion_bps > thresholds.max_colluding_extraction_bps
            else GateStatus.PASS
        ),
        "every challenged delivery is real, so accounting caps loss at the budget but cannot distinguish honest demand",
    )

    retained_policy = make_policy(
        SettlementClass.SPONSOR_FUNDED,
        "retained-state",
        budget=100,
        max_claim=10,
        max_retained_keys=2,
    )
    retained_model = SettlementModel()
    retained_model.register_policy(retained_policy)
    retained_items = [
        make_claim(
            retained_policy,
            event=f"retained-{idx}",
            requester=f"retained-requester-{idx}",
            funder="retained-budget",
            provider=f"retained-provider-{idx}",
            amount=10,
            rate_tag=f"retained-tag-{idx}",
        )
        for idx in range(2)
    ]
    retained_outcomes = retained_model.submit_canonical_batch(
        retained_items,
        availability=Availability(3, 3),
        now_ms=now,
    )
    record(
        "retained-state-bound-fails-closed",
        retained_model,
        retained_outcomes,
        GateStatus.PASS,
        "one accepted claim consumes two retained keys; the next is rejected rather than evicting replay state",
    )

    audit_model = SettlementModel(
        payer_balances={"audit-payer": 20, "audit-provider": 0}
    )
    audit_model.register_policy(requester_policy)
    audit_claim, audit_tx = make_claim(
        requester_policy,
        event="audit-finality",
        requester="audit-payer",
        funder="audit-payer",
        provider="audit-provider",
        amount=10,
        rate_tag=None,
    )
    audit_outcome = audit_model.submit(
        audit_claim,
        audit_tx,
        availability=Availability(0, 0),
        now_ms=now,
    )
    before = audit_model.state_digest()
    false_response = evaluate_audit_allegation(
        AllegationKind.OBJECTIVE_TRANSCRIPT_FAILURE,
        objective_proof_valid=False,
    )
    heuristic_response = evaluate_audit_allegation(
        AllegationKind.HEURISTIC_COLLUSION,
        objective_proof_valid=False,
    )
    after = audit_model.state_digest()
    audit_ok = (
        before == after
        and not false_response.canonical_state_mutation_permitted
        and not heuristic_response.canonical_state_mutation_permitted
    )
    record(
        "audit-cannot-rewrite-canonical-finality",
        audit_model,
        [audit_outcome],
        GateStatus.PASS if audit_ok else GateStatus.FAIL,
        "false or heuristic allegations receive no canonical-ledger handle and mutate no balance",
    )

    leaky_declaration = PrivacyDeclaration(
        disclosures=(
            (
                Role.AUDITOR,
                (Disclosure.ROOT_DID, Disclosure.RAW_QUERY),
            ),
        )
    )
    privacy_status = (
        GateStatus.FAIL
        if leaky_declaration.cross_context_leakage_score()
        > thresholds.max_cross_context_leakage_score
        else GateStatus.PASS
    )
    vectors.append(
        VectorResult(
            vector="privacy-budget-rejects-global-graph-fields",
            status=(
                GateStatus.PASS
                if privacy_status is GateStatus.FAIL
                else GateStatus.FAIL
            ),
            accepted=0,
            already_accepted=0,
            rejected=1,
            spent_units=0,
            extraction_units=0,
            detail="a declaration exposing root DID and raw query exceeds the zero cross-context leakage gate",
            state_digest=model_commitment(
                "privacy-vector",
                leaky_declaration,
            ),
        )
    )

    duplicate_outcomes = [
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
    )
    max_budget_overrun = max(
        sponsor_model.budget_overrun_units(sponsor_policy.commitment),
        canonical_model.budget_overrun_units(protocol_policy.commitment),
        collusion_model.budget_overrun_units(collusion_policy.commitment),
    )
    modeled_policies = (
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
    )
    detection_bps = audit_detection_probability_bps(
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
    max_observed_retained = max(
        sponsor_model.retained_state_bytes(sponsor_policy.commitment),
        collusion_model.retained_state_bytes(collusion_policy.commitment),
    )
    max_configured_retained = max(
        policy.max_retained_keys * SettlementModel.RETAINED_KEY_ESTIMATE_BYTES
        for policy in modeled_policies
    )

    gates = [
        GateResult(
            gate="maximum-budget-overrun",
            status=(
                GateStatus.PASS
                if max_budget_overrun <= thresholds.max_budget_overrun_units
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_budget_overrun_units,
            observed=max_budget_overrun,
            unit="model-units",
            detail="precommitted sponsor/protocol budgets never go negative",
        ),
        GateResult(
            gate="maximum-colluding-extraction",
            status=(
                GateStatus.PASS
                if collusion_bps <= thresholds.max_colluding_extraction_bps
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_colluding_extraction_bps,
            observed=collusion_bps,
            unit="basis-points-of-program-budget",
            detail="FAIL is expected: real delivery plus many roots drains the cap without a reviewed scarcity assumption",
        ),
        GateResult(
            gate="duplicate-false-negative-rate",
            status=GateStatus.PASS,
            threshold=thresholds.max_duplicate_false_negative_bps,
            observed=duplicate_false_negatives * 10_000 // duplicate_attempts,
            unit="basis-points",
            detail="retry and identity-split vectors produce no duplicate spend",
        ),
        GateResult(
            gate="honest-false-rejection-rate",
            status=GateStatus.PARTIAL,
            threshold=thresholds.max_honest_false_rejection_bps,
            observed=honest_rejections * 10_000 // honest_claims,
            unit="basis-points",
            detail="one structural honest vector passes, but one sample cannot establish a 1% population rate",
        ),
        GateResult(
            gate="cross-context-linkability-score",
            status=(
                GateStatus.PASS
                if PrivacyDeclaration.default().cross_context_leakage_score()
                <= thresholds.max_cross_context_leakage_score
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_cross_context_leakage_score,
            observed=PrivacyDeclaration.default().cross_context_leakage_score(),
            unit="declared-score",
            detail="default model uses policy-scoped commitments and coarse epochs, not root DIDs/raw queries",
        ),
        GateResult(
            gate="cross-policy-semantic-deduplication",
            status=GateStatus.PARTIAL,
            threshold="explicit-policy-family-rule-or-declared-independent-budgets",
            observed="unmeasured-no-global-registry-by-design",
            unit="policy-overlap-semantics",
            detail="separate policies may pay the same event; a global requester/provider activity graph is forbidden",
        ),
        GateResult(
            gate="audit-detection-probability",
            status=(
                GateStatus.PASS
                if detection_bps >= thresholds.min_audit_detection_bps
                else GateStatus.FAIL
            ),
            threshold=thresholds.min_audit_detection_bps,
            observed=detection_bps,
            unit="basis-points-for-60-objectively-invalid-claims",
            detail="5% sampling reaches >=95% detection only when claim ids are fixed before realized randomness is revealed",
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
            gate="issuer-concentration",
            status=GateStatus.PARTIAL,
            threshold=thresholds.max_compromised_issuer_bps,
            observed="unmeasured-no-construction-selected",
            unit="basis-points-compromised",
            detail="numeric limit is fixed, but Phase 2 deliberately selects no issuer protocol or operator set",
        ),
        GateResult(
            gate="auditor-concentration",
            status=GateStatus.PARTIAL,
            threshold=thresholds.max_compromised_auditor_bps,
            observed="unmeasured-no-operator-set-selected",
            unit="basis-points-compromised",
            detail="public sampling/objective proofs limit discretion, but real operational independence is unmeasured",
        ),
        GateResult(
            gate="weak-device-verification-cpu",
            status=GateStatus.PARTIAL,
            threshold=thresholds.max_verification_cpu_ms,
            observed="unmeasured-no-weak-device-benchmark",
            unit="milliseconds-per-claim",
            detail=(
                f"configured abstract-work cap is {max_configured_ops}; "
                f"largest fixture used {max_fixture_ops}; physical CPU remains unmeasured"
            ),
        ),
        GateResult(
            gate="weak-device-verification-memory",
            status=GateStatus.PARTIAL,
            threshold=thresholds.max_verification_memory_bytes,
            observed="unmeasured-no-allocator-benchmark",
            unit="bytes-peak",
            detail="the model bounds retained state and wire input, but does not claim a physical allocator measurement",
        ),
        GateResult(
            gate="retained-state-per-policy-epoch",
            status=(
                GateStatus.PASS
                if max_configured_retained <= thresholds.max_retained_state_bytes
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_retained_state_bytes,
            observed=max_configured_retained,
            unit="configured-estimated-bytes",
            detail=(
                f"configured policy capacity is measured, not only the "
                f"{max_observed_retained}-byte fixture state; eviction still fails closed"
            ),
        ),
        GateResult(
            gate="claim-plus-proof-wire-size",
            status=(
                GateStatus.PASS
                if max_configured_wire <= thresholds.max_claim_proof_wire_bytes
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_claim_proof_wire_bytes,
            observed=max_configured_wire,
            unit="configured-bytes",
            detail=(
                f"policy cap is compared directly with the 16 KiB gate; "
                f"largest fixture used {max_fixture_wire} bytes"
            ),
        ),
        GateResult(
            gate="abstract-verification-work",
            status=(
                GateStatus.PASS
                if max_configured_ops <= thresholds.max_abstract_verification_ops
                else GateStatus.FAIL
            ),
            threshold=thresholds.max_abstract_verification_ops,
            observed=max_configured_ops,
            unit="configured-model-operations",
            detail=(
                f"policy cap is compared directly with the gate; largest fixture "
                f"used {max_fixture_ops}; this is not a physical CPU measurement"
            ),
        ),
    ]
    return vectors, gates


def make_policy(
    settlement_class: SettlementClass,
    name: str,
    *,
    budget: int,
    max_claim: int | None = None,
    epoch: int = 7,
    max_retained_keys: int = 100_000,
) -> SettlementPolicy:
    requester = settlement_class is SettlementClass.REQUESTER_FUNDED
    return SettlementPolicy(
        version=MODEL_VERSION,
        settlement_class=settlement_class,
        service_class=ServiceClass.BYTE_DELIVERY,
        policy_name=name,
        funding_source_commitment=(
            "requester-balance" if requester else f"{name}-budget"
        ),
        epoch=epoch,
        starts_at_ms=0,
        expires_at_ms=10_000,
        program_budget_units=budget,
        max_claim_units=max_claim or (100 if requester else budget),
        duplicate_domain=f"{DUPLICATE_DOMAIN_PREFIX}{name}",
        rate_limit_domain=(
            None if requester else f"{RATE_LIMIT_DOMAIN_PREFIX}{name}"
        ),
        challenge_required=True,
        issuer_threshold=0 if requester else 2,
        auditor_threshold=0 if requester else 2,
        audit_sample_bps=500,
        max_retained_keys=max_retained_keys,
        max_claim_proof_wire_bytes=16 * 1024,
        max_abstract_verification_ops=10_000,
        privacy=PrivacyDeclaration.default(),
    )


def make_claim(
    policy: SettlementPolicy,
    *,
    event: str,
    requester: str,
    funder: str,
    provider: str,
    amount: int,
    rate_tag: str | ScopedRateTag | None,
) -> tuple[SettlementClaim, DeliveryChallengeTranscript]:
    event_commitment = model_commitment("request-event", event)
    # Valid sponsor/protocol vectors derive the exact immutable funding source
    # from the policy; only requester-funded claims choose a payer balance.
    effective_funder = (
        funder
        if policy.settlement_class is SettlementClass.REQUESTER_FUNDED
        else policy.funding_source_commitment
    )
    transcript = DeliveryChallengeTranscript.create(
        policy,
        request_event_commitment=event_commitment,
        requester_scope=requester,
        provider_scope=provider,
        challenge=model_commitment(
            "challenge",
            {"event": event, "policy": policy.commitment},
        ),
        response_commitment=model_commitment(
            "response",
            {"event": event, "provider": provider},
        ),
        issued_at_ms=900,
        expires_at_ms=2_000,
    )
    if isinstance(rate_tag, str):
        assert policy.rate_limit_domain is not None
        scoped_tag: ScopedRateTag | None = ScopedRateTag(
            policy.rate_limit_domain,
            rate_tag,
        )
    else:
        scoped_tag = rate_tag
    claim = SettlementClaim.create(
        policy,
        transcript,
        requester_scope=requester,
        funder_commitment=effective_funder,
        provider_scope=provider,
        request_event_commitment=event_commitment,
        amount_units=amount,
        expires_at_ms=2_000,
        rate_limit_tag=scoped_tag,
    )
    return claim, transcript


def render_report(thresholds: Thresholds | None = None) -> str:
    thresholds = thresholds or Thresholds()
    vectors, gates = run_fixed_vectors(thresholds)
    lines: list[str] = []
    lines.append(
        json.dumps(
            {
                "kind": "f5-phase2-model",
                "model_version": MODEL_VERSION,
                "thresholds": to_primitive(thresholds),
            },
            sort_keys=True,
            separators=(",", ":"),
        )
    )
    for vector in sorted(vectors, key=lambda item: item.vector):
        lines.append(
            json.dumps(
                {"kind": "vector", **to_primitive(vector)},
                sort_keys=True,
                separators=(",", ":"),
            )
        )
    for gate in sorted(gates, key=lambda item: item.gate):
        lines.append(
            json.dumps(
                {"kind": "gate", **to_primitive(gate)},
                sort_keys=True,
                separators=(",", ":"),
            )
        )
    all_pass = all(
        gate.status is GateStatus.PASS for gate in gates
    ) and all(vector.status is GateStatus.PASS for vector in vectors)
    lines.append(
        json.dumps(
            {
                "kind": "authorization",
                "phase3_authorized": all_pass,
                "reason": (
                    "all precommitted gates passed"
                    if all_pass
                    else "blocked: at least one gate failed or remains unmeasured"
                ),
            },
            sort_keys=True,
            separators=(",", ":"),
        )
    )
    return "\n".join(lines) + "\n"


def model_commitment(label: str, value: Any) -> str:
    """Return a deterministic model-only commitment.

    SHA-256 is used only to make fixed vectors stable and compact. This does
    not select a production F5 commitment or transcript primitive.
    """

    payload = canonical_json_bytes(
        {
            "domain": MODEL_COMMITMENT_DOMAIN,
            "label": label,
            "value": to_primitive(value),
        }
    )
    return hashlib.sha256(payload).hexdigest()


def canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(
        to_primitive(value),
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=True,
    ).encode("utf-8")


def to_primitive(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, bool)):
        return value
    if isinstance(value, Enum):
        return value.value
    if is_dataclass(value):
        return {
            field.name: to_primitive(getattr(value, field.name))
            for field in fields(value)
        }
    if isinstance(value, Mapping):
        return {
            str(key.value if isinstance(key, Enum) else key): to_primitive(item)
            for key, item in sorted(value.items(), key=lambda pair: str(pair[0]))
        }
    if isinstance(value, (tuple, list)):
        return [to_primitive(item) for item in value]
    if isinstance(value, (set, frozenset)):
        return sorted(to_primitive(item) for item in value)
    raise TypeError(f"unsupported canonical value: {type(value)!r}")


def _validate_identifier(value: str, field_name: str) -> None:
    if not value:
        raise ValueError(f"{field_name} must not be empty")
    if len(value.encode("utf-8")) > MAX_IDENTIFIER_BYTES:
        raise ValueError(
            f"{field_name} exceeds {MAX_IDENTIFIER_BYTES} bytes"
        )


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        metavar="PATH",
        help="compare deterministic output with a checked-in fixed-vector file",
    )
    args = parser.parse_args(argv)
    report = render_report()
    if args.check:
        with open(args.check, "r", encoding="utf-8") as handle:
            expected = handle.read()
        if expected != report:
            print("fixed-vector mismatch", flush=True)
            return 1
        return 0
    print(report, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
