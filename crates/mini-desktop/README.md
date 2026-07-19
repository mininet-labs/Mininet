# mini-desktop

Windows-first egui reference client shell for Mininet.

Run it locally:

```powershell
cargo run -p mini-desktop
```

The default home is `%LOCALAPPDATA%\Mininet`. To run two independent local
instances, launch each one with a different `MININET_HOME`, such as
`%LOCALAPPDATA%\Mininet\profiles\alice` and
`%LOCALAPPDATA%\Mininet\profiles\bob`. Each profile then has its own DPAPI
identity vault, object store, settings, and sequence space.

Two-instance friend workflow:

1. Start Alice and Bob with different `MININET_HOME` values and complete root
   plus public-account onboarding in each window.
2. In each window, open Connections and copy the displayed DID through a
   trusted channel.
3. In Creator studio, enter the other DID under People and follows, confirm
   signing, and choose Follow locally.
4. In both windows enable local-network discovery in Privacy & safety. Have
   Bob choose Listen once on port `46000`, then have Alice choose Connect once
   to Bob's address.
5. Repeat the signed follow in the opposite direction if both people want a
   mutual friend relationship. The UI counts it as mutual only after both
   signed objects are present locally.

Direct peer sync is not limited to a LAN: enter a reachable public hostname or
IP plus port. The listener must be reachable through the firewall/NAT, or the
operators must deploy a relay; automatic NAT traversal and a hosted relay are
not silently assumed by the client.

The shell starts with a first-run flow: create a local Mininet root, then fill
out and publish a signed public account profile. It then exposes the
information architecture and hardened defaults: local feed composition,
communities, creator space, connections, system/storage inspection, and the
privacy center. Posts, profiles, and community cards now go through
the real `mini-store`/`mini-social` APIs and are written to
`%LOCALAPPDATA%\\Mininet\\objects`.

Creator studio also exposes the shipped public-wall protocol: a wall can be
published as public or unlisted, with optional opaque links, without implying
that the wall is linked to another identity. The Mininet system view includes
the actual local object inventory and a conservative production-readiness
matrix for features whose protocol foundations are ahead of their desktop or
deployment workflows.

The identity seed envelope is protected with the Windows-user DPAPI boundary
by `mini-windows-vault`.

The UI has an explicit identity lock and starts every session locked. A locked
client can inspect local data but cannot publish signed objects. Each publish
form also requires an explicit signing confirmation, and privacy settings are
stored through DPAPI.

The home view now reads the local feed, lets the user choose chronological or
most-supported ordering, publishes signed replies, and records signed likes.
These controls still remain local until a user enables a transport path.

Community cards expose signed join/leave controls and locally known member
counts. Membership is not inferred from a server response; it is derived from
the same convergent objects used by every other social surface.

Creator studio can publish a local file through `mini-media` and create a
linked signed post. Chunking is resumable/content-addressed, and the UI never
opens a browser or uploads the file to a third-party service.

Connections also provides bounded offline object-bundle export/import. This
supports USB, shared-folder, and trusted peer handoff when a relay or domain
is blocked. Bundles carry signed objects only; the DPAPI identity vault is
never exported, and portable bundles should be placed inside an encrypted
container when their contents are sensitive.

The Connections view also exposes a one-shot direct-peer sync using the real
encrypted TCP bearer and verified `MINI/SYNC1` ingest. It is foreground-user
initiated, runs off the UI thread, and requires local-network discovery to be
enabled. There is no automatic discovery, retry loop, or always-on listener.

It does not start networking, open external URLs, collect telemetry, execute
updates, or embed a browser. Private signing material is not stored as
plaintext.

The Mininet system view reports actual local object counts and distinguishes
desktop-integrated features from repository foundations that do not yet have a
safe end-user workflow, including forge administration, keystone encounters,
reward accounting, and update adoption. It does not present placeholder
buttons for those unfinished surfaces.

Before distribution, the Windows client still needs hardware-backed key
storage, local export/import, Windows packaging, reproducible release
verification, and an independent security review. See
`docs/WINDOWS_CLIENT_SECURITY.md`.
