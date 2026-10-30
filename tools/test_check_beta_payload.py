import unittest

from jsonschema import ValidationError
from check_beta_payload import canonical_did, validate_payload

OBJECT_ID = "zgVwfgAcvNjFe1YPYJrkQ6ocqJkt983ic4uoSL4pL9cQDZg"  # canonical BLAKE3 multihash, base58btc


class BetaInterchangeTests(unittest.TestCase):
    def test_zero_registration_and_grant_fields_are_rejected(self):
        account = {"beta_epoch": "01" * 32, "nonce": "02" * 32, "account_id": "03" * 32}
        validate_payload("account", account)
        for key in account:
            with self.assertRaises(ValidationError):
                validate_payload("account", {**account, key: "0" * 64})
        grant = {"campaign_id": OBJECT_ID, "beta_epoch": "01" * 32, "account_id": "03" * 32,
                 "grant_class": "testing", "amount_micro": 1, "memo": "test units"}
        validate_payload("grant", grant)
        for key in ["beta_epoch", "account_id"]:
            with self.assertRaises(ValidationError):
                validate_payload("grant", {**grant, key: "0" * 64})

    def test_unicode_uses_utf8_byte_limits(self):
        finding = {"campaign_id": OBJECT_ID, "submission_tag": "01" * 32, "evidence_class": "other",
                   "severity": "low", "component": "😀" * 128, "summary": "summary",
                   "environment": "test", "steps": "one", "expected": "expected", "observed": "observed",
                   "evidence": ["digest:test"], "limitations": "fixture", "privacy_redacted": True}
        validate_payload("finding", finding)
        with self.assertRaises(ValidationError):
            validate_payload("finding", {**finding, "component": "😀" * 129})

    def test_campaign_requires_forward_window(self):
        campaign = {"target_id": OBJECT_ID, "beta_epoch": "01" * 32, "title": "title",
                    "instructions": "instructions", "routes": ["cli"], "starts_ms": 100,
                    "ends_ms": 101, "default_testing_grant_micro": 1}
        validate_payload("campaign", campaign)
        for end in [1, 100]:
            with self.assertRaises(ValidationError):
                validate_payload("campaign", {**campaign, "ends_ms": end})

    def test_authorizers_require_supported_canonical_multihashes(self):
        for code in ["12", "1e"]:
            self.assertTrue(canonical_did("did:mini:f" + code + "20" + "01" * 32))
        for bad in ["did:mini:not-a-multihash", "did:mini:f1120" + "01" * 32,
                    "did:mini:f922000" + "01" * 32, "did:mini:f1220" + "01" * 31]:
            self.assertFalse(canonical_did(bad))
        members = ["did:mini:f1220" + f"{n:02x}" * 32 for n in range(1, 4)]
        policy = {"campaign_id": OBJECT_ID, "beta_epoch": "01" * 32, "authorizers": members,
                  "testing_threshold": 2, "participation_threshold": 2,
                  "testing_grant_amount_micro": 1, "participation_reward_bands_micro": [1],
                  "valid_from_ms": 1, "valid_until_ms": 2}
        validate_payload("grant-policy", policy)
        with self.assertRaises(ValidationError):
            validate_payload("grant-policy", {**policy, "authorizers": [*members[:2], "did:mini:invalid"]})

    def test_non_canonical_object_ids_are_rejected(self):
        campaign = {"target_id": OBJECT_ID, "beta_epoch": "01" * 32, "title": "title",
                    "instructions": "do", "routes": ["a"], "starts_ms": 1, "ends_ms": 2,
                    "default_testing_grant_micro": 1}
        validate_payload("campaign", campaign)
        for bad in ["x", "id", OBJECT_ID[1:], "f" + OBJECT_ID[1:], OBJECT_ID[:-1], OBJECT_ID + "1"]:
            with self.assertRaises(ValidationError):
                validate_payload("campaign", {**campaign, "target_id": bad})


if __name__ == "__main__":
    unittest.main()
