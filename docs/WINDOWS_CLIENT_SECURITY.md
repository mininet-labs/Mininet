# Windows client security boundary

The Windows client is a reference renderer over Mininet's Rust core. It is not
a security product that can make a compromised Windows installation safe. Its
job is to make Mininet itself a small, inspectable, non-tracking participant.

## Defaults enforced by the shell

- no analytics, advertising SDK, crash uploader, remote configuration, or
  background network loop;
- no embedded browser and no automatic opening of external URLs;
- no external-source adapter, LAN discovery, or relay use until the user
  enables each separately;
- no silent update executor and no forced update path;
- explicit identity lock/unlock control; every session starts locked, and
  locked clients remain readable but cannot sign;
- explicit per-action signing confirmation before publishing objects;
- privacy settings are DPAPI-protected rather than stored as plaintext config;
- no protocol secrets in URLs, window titles, analytics labels, or log output;
- local composition and drafts are usable before any sync path is enabled;
- all network paths should carry opaque encrypted protocol frames and remain
  replaceable.

## Censorship and blocking resilience

The client should preserve a local object store and offer several independent
ways to move data: offline export/import, direct local Wi-Fi, peer handoff,
self-hosted relays, and later bridge transports. No DNS name, app store,
single relay, or hosted API is an update authority or a required social
database. A blocked route should reduce availability, not destroy local data
or identity.

The Windows client exposes a one-shot direct-peer sync over the existing
encrypted TCP bearer and verified sync ingest. It requires a user-supplied
address or an explicit, opt-in LAN discovery action. People can announce a
chosen public name, DID, and endpoint for 60 seconds and can scan multicast for
three seconds; announcements are labeled unverified until signed objects and
KELs pass ingest. A visible window can serve multiple sequential connections,
each accepted socket has a ten-second read/write timeout, and malformed peers
do not end the remaining window. There is no retry loop or always-on socket.
The peer can learn what the user chooses to replicate, which is inherent to
synchronization and is disclosed in the UI.

The desktop offline-transfer controls use a bounded, versioned object bundle.
Import parses each object and inserts it through the content-addressed store;
the identity DPAPI vault is deliberately excluded. Portable bundles are not
encrypted by the application, so sensitive exports require a user-selected
encrypted destination such as BitLocker.

This is not a claim of perfect censorship resistance. A censor can block a
particular transport, observe endpoint traffic, seize a device, or pressure
distribution channels. The architecture is intended to make those failures
non-fatal and visible.

## Keylogger and endpoint boundary

An application cannot reliably defeat a kernel-level keylogger, malicious
administrator, compromised accessibility provider, hostile input driver,
screen capture, or malware with access to the desktop. Mininet must therefore:

1. keep signing keys in the identity/keystore layer rather than text fields;
2. use OS-protected key storage and explicit confirmation for signing;
3. never log passwords, recovery phrases, drafts, or clipboard contents;
4. prefer device-side signing over exporting private material to the UI;
5. show a clear high-risk warning before entering recovery material;
6. document a trusted-device threat model instead of promising “keylogger
   proof” behavior.

## Release and supply-chain requirements

Windows binaries should be reproducibly built, signed through the governed
release process, hash-verifiable, and installable from a local file or peer.
The client may display a release proposal, but adoption remains a user choice.
Dependencies should be pinned and audited; the UI shell must not grow an
unreviewed plugin or arbitrary script execution mechanism.

The current `mini-desktop` crate has local social-object integration and
separate Windows-user DPAPI seed vaults for the human root and a scoped primary
device. Social objects are signed by that delegated device, and peer sync
ships both KEL carriers so strict provenance verification does not need to
trust a display name or endpoint. Hardware-backed key storage, Windows
packaging, code signing, sandboxing, store-level cross-process coordination,
and independent security review remain required before making a high-assurance
Windows distribution claim. DPAPI protects against offline file theft for the
current user profile; it does not protect against malware or an administrator
already running as that user.
