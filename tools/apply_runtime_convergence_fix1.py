#!/usr/bin/env python3
from pathlib import Path


def replace(path: str, old: str, new: str, expected: int = 1) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{path}: expected {expected}, found {count}: {old!r}")
    target.write_text(text.replace(old, new), encoding="utf-8")


runtime = "crates/mini-transport-security/src/runtime.rs"
replace(
    runtime,
    """#[derive(Debug, Clone, Copy)]
pub struct LocalSessionIdentity<'a> {
    pub root: &'a Did,
    pub device: &'a Controller,
    pub routing_key: AgreementPublicKey,
}

impl<'a> LocalSessionIdentity<'a> {
    pub const fn new(
        root: &'a Did,
        device: &'a Controller,
        routing_key: AgreementPublicKey,
    ) -> Self {
""",
    """#[derive(Debug, Clone)]
pub struct LocalSessionIdentity<'a> {
    pub root: Did,
    pub device: &'a Controller,
    pub routing_key: AgreementPublicKey,
}

impl<'a> LocalSessionIdentity<'a> {
    pub const fn new(
        root: Did,
        device: &'a Controller,
        routing_key: AgreementPublicKey,
    ) -> Self {
""",
)
replace(runtime, "        local.root,\n", "        &local.root,\n", expected=2)
replace(
    runtime,
    """            local,
            purpose,
""",
    """            local.clone(),
            purpose,
""",
)
for path in [
    "crates/mini-transport-security/tests/runtime_tcp.rs",
    "crates/mini-search-federation-net/tests/authenticated_query_over_tcp.rs",
]:
    replace(
        path,
        "LocalSessionIdentity::new(&self.root.did(), &self.device, self.routing)",
        "LocalSessionIdentity::new(self.root.did(), &self.device, self.routing)",
    )

print("runtime identity ownership fix applied")
