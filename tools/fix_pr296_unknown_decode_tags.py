#!/usr/bin/env python3
"""Make unknown F6 wire enum tags fail closed and add regressions."""
from pathlib import Path

path = Path("crates/mini-search-federation-net/src/query.rs")
text = path.read_text(encoding="utf-8")
replacements = [
    (
        "            255 => UnavailabilityReason::FetchFailed,\n",
        "            255 => return Err(NetError::Protocol),\n",
    ),
    (
        "            255 => RestrictionReason::UserFilter,\n",
        "            255 => return Err(NetError::Protocol),\n",
    ),
]
for old, new in replacements:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one decoder-tag match, found {count}: {old!r}")
    text = text.replace(old, new)

marker = '''    #[test]
    fn outbound_and_inbound_codecs_enforce_the_same_field_bounds() {
'''
regression = '''    #[test]
    fn unknown_wire_enum_tags_fail_closed() {
        let mut unavailable = Reader::new(&[1, 255]);
        assert!(matches!(
            decode_availability(&mut unavailable),
            Err(NetError::Protocol)
        ));

        let mut restricted = Reader::new(&[2, 255]);
        assert!(matches!(
            decode_availability(&mut restricted),
            Err(NetError::Protocol)
        ));

        let (_, _, _, _, profile) = fixture();
        let mut profile_writer = Writer::new();
        encode_profile(&mut profile_writer, &profile);
        let mut profile_bytes = profile_writer.finish();
        *profile_bytes.last_mut().unwrap() = 255;
        assert!(matches!(
            decode_profile(&mut Reader::new(&profile_bytes)),
            Err(NetError::Protocol)
        ));

        let mut url_writer = Writer::new();
        encode_url(&mut url_writer, &url("example.org", "/"));
        let mut url_bytes = url_writer.finish();
        url_bytes[0] = 255;
        assert!(matches!(
            decode_url(&mut Reader::new(&url_bytes)),
            Err(NetError::Protocol)
        ));
    }

'''
if text.count(marker) != 1:
    raise SystemExit(f"expected one codec-regression insertion point, found {text.count(marker)}")
text = text.replace(marker, regression + marker)
path.write_text(text, encoding="utf-8")
print("PR 296 unknown F6 decoder tags now fail closed")
