#!/usr/bin/env python3
"""Apply the shared F6 validator at the public legacy merge boundary."""
from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:140]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


replace_once(
    "crates/mini-search-federation-net/src/query.rs",
    """fn validate_wire_result(result: &WireResult) -> Result<()> {
""",
    """pub(crate) fn validate_wire_result(result: &WireResult) -> Result<()> {
""",
)
replace_once(
    "crates/mini-search-federation-net/src/remote_merge.rs",
    """use crate::query::{AuthenticatedQueryResults, WireResult};
""",
    """use crate::query::{validate_wire_result, AuthenticatedQueryResults, WireResult};
""",
)
replace_once(
    "crates/mini-search-federation-net/src/remote_merge.rs",
    """pub fn federated_result_from_wire(
    wire: WireResult,
    provider: ProviderPseudonym,
) -> Result<FederatedResult> {
    let bps = |v: u16| WeightBps::new(v).map_err(|_| NetError::Protocol);
""",
    """pub fn federated_result_from_wire(
    wire: WireResult,
    provider: ProviderPseudonym,
) -> Result<FederatedResult> {
    // `WireResult` is public and can be constructed locally or supplied by a
    // legacy caller without traversing the F6 decoder. Reuse the exact same
    // canonical URL, field, multihash, score, and displayability validator here
    // before the value enters the typed federated merge.
    validate_wire_result(&wire)?;
    let bps = |v: u16| WeightBps::new(v).map_err(|_| NetError::Protocol);
""",
)
replace_once(
    "crates/mini-search-federation-net/src/remote_merge.rs",
    """    #[test]
    fn an_out_of_range_relevance_score_is_rejected() {
""",
    """    #[test]
    fn a_locally_constructed_filtered_result_cannot_bypass_f6_validation() {
        let mut wire = wire_result("/a", 100);
        wire.availability =
            AvailabilityState::Restricted(mini_web_types::RestrictionReason::UserFilter);
        assert_eq!(
            federated_result_from_wire(wire, provider(b"p1")),
            Err(NetError::Protocol)
        );
    }

    #[test]
    fn an_out_of_range_relevance_score_is_rejected() {
""",
)
replace_once(
    "crates/mini-search-federation-net/src/remote_merge.rs",
    """/// only ever emits validated [`WeightBps`]. The F6 wire decoder also
/// rejects these values; this conversion deliberately repeats the check because
/// [`WireResult`] is public and can be constructed locally without passing
/// through the decoder. Invalid local or legacy inputs therefore still fail
/// closed before entering the typed federated merge.
""",
    """/// only ever emits validated [`WeightBps`]. This conversion invokes the
/// same shared validator as the F6 wire codec because [`WireResult`] is public
/// and can be constructed locally without passing through the decoder. Invalid,
/// noncanonical, oversized, or non-displayable local/legacy inputs therefore
/// fail closed before entering the typed federated merge; the typed `WeightBps`
/// conversion below repeats the score check as defense in depth.
""",
)
replace_once(
    "docs/design/f6-private-query-transport.md",
    """The F6 decoder now rejects the same values; the conversion repeats the check because public `WireResult` values may also be constructed locally or arrive through legacy code without traversing that decoder.
""",
    """The conversion now invokes the same shared canonical-field, score, multihash, URL, and displayability validator as the F6 decoder because public `WireResult` values may also be constructed locally or arrive through legacy code without traversing that decoder; typed score conversion then repeats the range check as defense in depth.
""",
)
replace_once(
    "docs/THREAT_MODEL.md",
    """| **F6 outbound/decode asymmetry, profile substitution, mutated URL/profile fields, or filtered-result reinsertion** | Every request/response is validated before encoding and after decoding; canonical URLs and profile versions are reconstructed/validated, response fields, score components, result counts, and multihashes share one bound set, clients require the requested profile, and ranked responses reject every non-displayable state. | **Closed for F6 framing, profile attribution, and ranker-filter preservation.** Provider honesty and query-content privacy remain unsolved. |
""",
    """| **F6 outbound/decode/legacy-merge asymmetry, profile substitution, mutated public fields, or filtered-result reinsertion** | Requests/responses validate before encoding and after decoding; the public legacy merge reuses the same result validator; canonical URLs/profile versions, fields, scores, counts, multihashes, requested-profile attribution, and displayability all fail closed. | **Closed for F6 framing, public merge input, profile attribution, and ranker-filter preservation.** Provider honesty and query-content privacy remain unsolved. |
""",
)
replace_once(
    "docs/planning/privacy-transport-runtime-convergence.md",
    """  symmetric outbound/decode bounds, requested-profile enforcement, and rejection
  of non-displayable ranked results.
""",
    """  symmetric outbound/decode bounds, requested-profile enforcement, rejection
  of non-displayable ranked results, and the same validation at the public
  local/legacy merge boundary.
""",
)
replace_once(
    "docs/DECISION_LOG.md",
    """values constructed locally or received through legacy code) and
`merge_remote_results(local:
""",
    """values constructed locally or received through legacy code; the conversion
now invokes the complete shared validator before its typed score conversion) and
`merge_remote_results(local:
""",
)
print("PR 296 F6 merge-boundary validation applied")
