# mini-media

Chunked, content-addressed media: ≤1 MiB chunk objects + one ordered manifest
carrying the content type, total length, and the BLAKE3 digest of the whole
payload. Assembly re-verifies that digest, so **a manifest cannot lie** about
what its chunks compose into.

Chunks are ordinary objects, so they ride `mini-sync` in any order across many
short encounters — progressive and interruption-proof by construction;
`missing_chunks` says exactly what a player or updater still needs. These same
manifests carry the forge's release artifacts (the app distributes through the
network itself, D-0020).

Honest limits: one manifest ≈ 256 MiB (nesting later); nearby-first,
relay-accelerated — not a CDN.

License: CC0-1.0 (public domain).
