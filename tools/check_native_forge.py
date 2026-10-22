#!/usr/bin/env python3
"""Focused native Forge software acceptance without building the Wasm runner.

Compile the real CLI sources with their production dependencies and the exact
repository lockfile versions. The only omitted dev-dependency is the unrelated
Wasmtime runner, which none of these selected tests use. The production binary
and Clippy still use the unmodified workspace manifest. No network/service is
published; socket tests use loopback and temporary owner homes.
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

CLI_TESTS = [
    "git_bridge", "git_history", "native_forge_review", "native_remotes",
    "native_service", "two_developers", "network_sync", "cli_json_output",
    "network_sync_release", "resident_commands", "seed_network",
]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true", help="also build the optimized CLI")
    parser.add_argument("--offline", action="store_true", help="require cached Cargo dependencies")
    parser.add_argument("--tests", nargs="+", choices=CLI_TESTS, help="run selected CLI integration suites")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    cargo = shutil.which("cargo")
    if cargo is None:
        candidate = Path.home() / ".cargo/bin/cargo"
        cargo = str(candidate) if candidate.is_file() else None
    if cargo is None or shutil.which("git") is None:
        parser.error("Rust/Cargo and Git must be installed")
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(root / "target")
    network = ["--offline"] if args.offline else []

    def run(command: list[str]) -> None:
        print("+ " + " ".join(command), flush=True)
        subprocess.run(command, cwd=root, env=env, check=True)

    # Avoid changing the workspace's dependency model just to run CLI tests.
    # Preserve each real dependency declaration; only resolve relative paths
    # against the manifest that owns them.
    source = root / "crates/mini-cli/Cargo.toml"
    data = tomllib.loads(source.read_text())
    section = source.read_text().split("[dependencies]", 1)[1].split("[dev-dependencies]", 1)[0]
    for dependency in data["dependencies"].values():
        if isinstance(dependency, dict) and "path" in dependency:
            relative = dependency["path"]
            absolute = (source.parent / relative).resolve().as_posix()
            section = section.replace(f'path = "{relative}"', f'path = "{absolute}"')
    tests = args.tests or CLI_TESTS
    with tempfile.TemporaryDirectory(prefix="mininet-forge-acceptance-") as directory:
        harness = Path(directory)
        manifest = (
            '[package]\nname = "mini-cli-acceptance"\nversion = "0.0.0"\nedition = "2021"\n'
            '[lib]\nname = "mini_cli"\n'
            f'path = "{(source.parent / "src/lib.rs").as_posix()}"\n'
            '[[bin]]\nname = "mini"\n'
            f'path = "{(source.parent / "src/main.rs").as_posix()}"\n'
            '[dependencies]\n' + section + '\n[dev-dependencies]\ntempfile = "3"\n'
        )
        for name in tests:
            test = source.parent / f"tests/{name}.rs"
            if not test.is_file():
                raise SystemExit(f"required acceptance test is missing: {test}")
            manifest += f'\n[[test]]\nname = "{name}"\npath = "{test.as_posix()}"\n'
        (harness / "Cargo.toml").write_text(manifest)
        shutil.copyfile(root / "Cargo.lock", harness / "Cargo.lock")
        run([cargo, "build", "--locked", "-p", "mini-cli", "--bin", "mini", *network])
        run([cargo, "test", "--manifest-path", str(harness / "Cargo.toml"), *network,
             *([] if args.tests else ["--lib"]),
             *[part for name in tests for part in ["--test", name]]])
    if not args.tests:
        run([cargo, "test", "--locked", "-p", "mini-forge", *network,
             "--test", "forge", "--test", "governance", "--test", "git_import", "--test", "git_export"])
        run([cargo, "test", "--locked", "-p", "mini-sync", *network, "--lib", "--test", "sync"])
    run([cargo, "clippy", "--locked", "-p", "mini-cli", "-p", "mini-forge", "-p", "mini-sync",
         "--lib", "--bins", *network, "--", "-D", "warnings"])
    run([cargo, "fmt", "--all", "--check"])
    if args.release:
        run([cargo, "build", "--locked", "--release", "-p", "mini-cli", "--bin", "mini", *network])
    label = "Selected native Forge checks" if args.tests else "Native Forge software acceptance"
    print(f"{label} passed. Hardware, research and external audit gates remain separate.")


if __name__ == "__main__":
    main()
