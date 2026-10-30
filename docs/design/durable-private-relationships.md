# Durable private relationship pairing — implementation boundary

This branch adds a Windows desktop relationship vault and a typed authenticated
pairing seam. It composes existing did-mini controllers, Windows DPAPI,
mini-transport-security authenticated CH1 connections and mini-sync verified
ingest. No new cryptography or authority selector is introduced.

## Identity lifecycle

An explicit pairing operation creates one pairwise pseudonym inception and one
random POST device for a selected counterpart. Their full KEL histories and
current/next signing keys are committed together to relationships.dpapi before
either network disclosure or FOLLOW publication. Reload uses Controller::restore,
including the delegation event, rather than re-deriving inception and inventing
a fresh device history. An exclusive file lease rejects simultaneous creators.
The vault is bound to its owner's real DID, bounded to 32 relationships, and
scrubs temporary plaintext/seed buffers. Current storage supports the desktop's
single Ed25519 controllers; unsupported key shapes return an error before export.

## Authenticated confidential target exchange

Workspace::pair_private accepts an existing AuthenticatedConnection and an
independently selected expected counterpart DID. The existing transport must
prove that DID on the live channel for PeerExchange. The caller remains responsible
for initially confirming whose DID it intends to trust; a discovered endpoint,
ordinary connection card or the peer itself does not select that trust input.

Both parties exchange pseudonym root/device KELs and signed root-to-pseudonym
linkage only inside CH1 encrypted application frames. The application frame
binds the exact live channel. Verification checks the peer's real-root linkage,
recipient, pseudonym, delegated POST device and bounded issue time. The target
and consumed channel binding are durably saved before responder acknowledgment.
A previously recorded target may only extend its original root/device KELs;
replacement or forked histories fail closed. Interrupted exchange can leave one
side paired; retry on a new channel reuses its persisted identity.

CH1 rejects ciphertext tampering/counter replay. Application binding rejects
cross-channel replay, and persisted consumed binding rejects repeated acceptance.
This is not ratcheted messaging, proof of humanness, global revocation freshness
or resistance to traffic analysis. Global root proofs remain visible to the
intended counterpart, and transport-level authentication discloses its own
identity before application exchange. Existing transport freshness limitations
remain; owner pinning does not invent unseen revocation evidence.

## Publication and recovery

Workspace::follow_privately only accepts a target already in that protected
authenticated state. It builds and verifies the FOLLOW, then durably reserves
the next pseudonym sequence before inserting pseudonym KEL carriers and FOLLOW.
A failed state save causes no public store writes. A subsequent backend write
failure can leave carriers or a reserved sequence gap, but cannot reuse a
sequence or fork a KEL. Only pseudonym identity material reaches the public store;
real-root linkage remains in the encrypted exchange/vault.

The ordinary root-card private-follow button stays disabled: it cannot perform
this authentication. This slice supplies the Workspace entry point and real
authenticated TCP adapter coverage; a dedicated interactive pairing screen is
not implemented. No launch, transport anonymity or production activation claim.

## Regression evidence and validation

Regression source covers restart preserving the same delegated pseudonym device,
replay after reload, channel mismatch, tampered linkage, wrong identity,
save-before-publish failure, exclusive creation lease, and a real two-peer TCP
authenticated exchange followed by two-store mini-sync Ingest verification and
restart/sequence continuation. The in-memory test vault is a test adapter,
not an encrypted storage substitute. Windows DPAPI behavior retains the existing
platform boundary and needs Windows validation.

Hosted validation at source head `902a883e754e51b1d10448a78f4968547cfa557b`:
[workflow 38860259911](https://github.com/mininet-labs/Mininet/actions/runs/38860259911),
job `116840687511`, ran `cargo fmt --all` and
`cargo test --locked -p mini-desktop private_relationship -- --nocapture`.
All four targeted tests passed (0 failed): exclusive creator lease;
persistence failure without target/sequence/public writes; identity continuity
after restart with replay/tamper/wrong-identity rejection; and real authenticated
TCP pairing followed by verified two-store ingestion and restart.

The hosted formatter's exact corrections are committed and its temporary
read-only workflow is removed. Standard Windows Clippy reported only that
`File::try_lock` and `File::unlock` require Rust 1.89 while the desktop
manifest declared 1.83; the desktop now declares its actual 1.89 API floor.
The already pinned workspace toolchain satisfies it. No lint was suppressed.

Final corrected-head checks, Windows DPAPI/platform validation and full
workspace checks still need to pass. The separate old Wasmtime dependency-deny
baseline failure is not evidence against or validation of this implementation.
