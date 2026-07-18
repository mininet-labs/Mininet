#!/usr/bin/env python3
"""Manage Mininet bootstrap work claims.

This is a local coordination helper. It edits governance/work-claims.json and
then reuses check_governance.py for validation. It grants no authority.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import time
from pathlib import Path

import check_governance


REGISTRY = check_governance.WORK_CLAIMS_PATH
LOCK = Path("governance/work-claims.lock")


class ClaimLock:
    def __init__(self, root: Path) -> None:
        self.path = root / LOCK
        self.fd: int | None = None

    def __enter__(self) -> "ClaimLock":
        self.path.parent.mkdir(parents=True, exist_ok=True)
        deadline = time.monotonic() + 20
        while True:
            try:
                self.fd = os.open(self.path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
                os.write(self.fd, str(os.getpid()).encode("ascii"))
                return self
            except FileExistsError:
                if time.monotonic() > deadline:
                    raise SystemExit(f"work-claim lock is busy: {self.path}")
                time.sleep(0.2)

    def __exit__(self, *_exc: object) -> None:
        if self.fd is not None:
            os.close(self.fd)
        try:
            self.path.unlink()
        except FileNotFoundError:
            pass


def load_registry(root: Path) -> dict:
    path = root / REGISTRY
    if not path.is_file():
        return {
            "$schema": "./work-claims.schema.json",
            "schema_version": 1,
            "registry_id": "mininet-bootstrap-work-claims",
            "updated_at": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
            "claims": [],
        }
    return json.loads(path.read_text(encoding="utf-8"))


def save_registry(root: Path, registry: dict) -> None:
    registry["updated_at"] = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    path = root / REGISTRY
    path.write_text(json.dumps(registry, indent=2) + "\n", encoding="utf-8")


def validate(root: Path) -> int:
    errors: list[str] = []
    warnings: list[str] = []
    check_governance.validate_work_claims(root, errors, warnings)
    for warning in warnings:
        print(f"warning: {warning}")
    for error in errors:
        print(f"error: {error}")
    return 1 if errors else 0


def claim(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    with ClaimLock(root):
        registry = load_registry(root)
        claims = registry.setdefault("claims", [])
        claims[:] = [
            existing for existing in claims
            if not (
                existing.get("issue") == args.issue
                and existing.get("status") in check_governance.WORK_CLAIM_ACTIVE_STATUSES
            )
        ]
        expiry = (
            dt.date.today() + dt.timedelta(days=args.days)
            if args.lease_expires is None
            else dt.date.fromisoformat(args.lease_expires)
        )
        claims.append({
            "issue": args.issue,
            "status": args.status,
            "contributor": args.contributor,
            "branch": args.branch,
            "pull_request": args.pull_request,
            "lease_expires": expiry.isoformat(),
            "paths": args.path,
            "decision_ids": args.decision_id,
            "notes": args.notes,
        })
        save_registry(root, registry)
    return validate(root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    sub = parser.add_subparsers(dest="command", required=True)

    validate_parser = sub.add_parser("validate")
    validate_parser.set_defaults(func=lambda args: validate(Path(args.root).resolve()))

    claim_parser = sub.add_parser("claim")
    claim_parser.add_argument("--issue", type=int, required=True)
    claim_parser.add_argument("--contributor", required=True)
    claim_parser.add_argument("--branch", required=True)
    claim_parser.add_argument("--path", action="append", required=True)
    claim_parser.add_argument("--decision-id", action="append", default=[])
    claim_parser.add_argument("--pull-request", type=int)
    claim_parser.add_argument("--lease-expires")
    claim_parser.add_argument("--days", type=int, default=7)
    claim_parser.add_argument("--status", default="active", choices=sorted(check_governance.WORK_CLAIM_ACTIVE_STATUSES))
    claim_parser.add_argument("--notes", default="")
    claim_parser.set_defaults(func=claim)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
