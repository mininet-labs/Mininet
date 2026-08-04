#!/usr/bin/env python3
"""Make unknown F6 availability and personalization wire tags fail closed."""
from pathlib import Path

path = Path("crates/mini-search-federation-net/src/query.rs")
text = path.read_text(encoding="utf-8")
replacements = [
    (
        '''        1 => AvailabilityState::Unavailable(match r.u8()? {
            0 => UnavailabilityReason::NotFetched,
            1 => UnavailabilityReason::FetchFailed,
            2 => UnavailabilityReason::Gone,
            3 => UnavailabilityReason::UnsupportedContent,
            // Future `UnavailabilityReason` variants encoded by a newer
            // peer remain a generic unavailable state to this version.
            _ => UnavailabilityReason::UnsupportedContent,
        }),
''',
        '''        1 => AvailabilityState::Unavailable(match r.u8()? {
            0 => UnavailabilityReason::NotFetched,
            1 => UnavailabilityReason::FetchFailed,
            2 => UnavailabilityReason::Gone,
            3 => UnavailabilityReason::UnsupportedContent,
            _ => return Err(NetError::Protocol),
        }),
''',
    ),
    (
        '''            // Future `RestrictionReason` variants encoded by a newer
            // peer remain an explicit generic safety restriction to this
            // version, never silently becoming available.
            _ => RestrictionReason::SafetyWarning,
''',
        '''            _ => return Err(NetError::Protocol),
''',
    ),
    (
        '''    let personalization = match r.u8()? {
        0 => PersonalizationPolicy::None,
        1 => PersonalizationPolicy::LocalUserControlled,
        // Future personalization modes remain non-personalized to this
        // version rather than causing the whole response to fail.
        _ => PersonalizationPolicy::None,
    };
''',
        '''    let personalization = match r.u8()? {
        0 => PersonalizationPolicy::None,
        1 => PersonalizationPolicy::LocalUserControlled,
        _ => return Err(NetError::Protocol),
    };
''',
    ),
]
for old, new in replacements:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one decoder-tag match, found {count}: {old[:100]!r}")
    text = text.replace(old, new)
path.write_text(text, encoding="utf-8")
print("PR 296 unknown F6 decoder tags now fail closed")
