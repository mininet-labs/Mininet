#!/usr/bin/env python3
"""Validate Beta interchange JSON before constructing a signed Rust object.

The schemas' x-mininet keywords are mandatory adapter checks. This validator
adds byte limits and cross-field ordering to Draft 2020-12 validation. Signed
object provenance, account-ID derivation, linked-object availability and issuance
authority still require the Rust implementation and caller-selected trust policy.
"""

import argparse
import json
from pathlib import Path

from jsonschema import Draft202012Validator, FormatChecker, ValidationError, validators

SCHEMAS = Path(__file__).resolve().parents[1] / "forge-native" / "schemas"
BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
FORMATS = FormatChecker()


@FORMATS.checks("did-mini")
def canonical_did(value):
    if not isinstance(value, str):
        return True  # the schema's type keyword owns type errors
    if not value.startswith("did:mini:"):
        return False
    scid = value[len("did:mini:"):]
    try:
        if scid.startswith("f"):
            body = scid[1:]
            if len(body) != 68 or any(c not in "0123456789abcdefABCDEF" for c in body):
                return False
            raw = bytes.fromhex(body)
        elif scid.startswith("z") and 1 < len(scid) <= 48:
            body = scid[1:]
            number = 0
            for char in body:
                number = number * 58 + BASE58.index(char)
            raw = b"\x00" * (len(body) - len(body.lstrip("1")))
            raw += number.to_bytes((number.bit_length() + 7) // 8, "big")
        else:
            return False
    except (ValueError, OverflowError):
        return False
    # Did::parse accepts only canonical one-byte BLAKE3/SHA-256 multihashes
    # with exactly a 32-byte digest, encoded using supported z/f multibase.
    return len(raw) == 34 and raw[0] in (0x12, 0x1E) and raw[1] == 32


def utf8_limit(_validator, maximum, value, _schema):
    if isinstance(value, str) and len(value.encode("utf-8")) > maximum:
        yield ValidationError(f"text exceeds {maximum} UTF-8 bytes")


def ordered_fields(_validator, pairs, value, _schema):
    if not isinstance(value, dict):
        return
    for earlier, later in pairs:
        first, second = value.get(earlier), value.get(later)
        if type(first) is int and type(second) is int and second <= first:
            yield ValidationError(f"{later} must be greater than {earlier}")


BetaValidator = validators.extend(
    Draft202012Validator,
    {"x-mininet-maxUtf8Bytes": utf8_limit, "x-mininet-orderedFields": ordered_fields},
)


def validate_payload(kind, value):
    allowed = {path.name.removesuffix(".schema.json").removeprefix("beta-")
               for path in SCHEMAS.glob("beta-*.schema.json")}
    if kind not in allowed:
        raise ValueError("unknown Beta payload kind")
    schema = json.loads((SCHEMAS / f"beta-{kind}.schema.json").read_text())
    BetaValidator(schema, format_checker=FORMATS).validate(value)
    if kind == "grant-policy":
        members = value["authorizers"]
        bands = value["participation_reward_bands_micro"]
        if members != sorted(members) or bands != sorted(bands):
            raise ValidationError("policy members and reward bands must be canonically sorted")
        if not (value["testing_threshold"] <= value["participation_threshold"] <= len(members)):
            raise ValidationError("policy thresholds must be ordered within the member count")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", help="Beta schema kind, such as campaign or finding")
    parser.add_argument("payload", type=Path)
    args = parser.parse_args()
    try:
        with args.payload.open("rb") as source:
            raw = source.read(256 * 1024 + 1)
        if len(raw) > 256 * 1024:
            raise ValueError("interchange payload exceeds 256 KiB")
        validate_payload(args.kind, json.loads(raw))
    except (OSError, ValueError, ValidationError) as error:
        parser.exit(1, f"invalid Beta payload: {error}\n")
    print("Beta interchange payload valid; signed-object and authority checks still required")


if __name__ == "__main__":
    main()
