# mini-desktop

Windows-first egui reference client shell for Mininet.

The connected desktop beta (D-0523) opens into an X-style black shell:
navigation rail, central timeline, and a discovery column on wide windows.
Timelines are loaded by a worker (50 cards), show real author names and
author-claimed relative times, and offer *Following* and *Everyone* scopes,
received-post search (Explore), a media filter with inline images, and
"Who to follow" from received signed profiles. Communities open into
Reddit-style threaded discussion (title + body threads, nested replies,
upvotes, Top/New) built entirely from existing comment and reaction
objects, so discussions replicate like everything else. **Library** holds
files and movies of any size (one manifest up to 256 MiB, larger files as an
ordered collection of manifests) with seeding progress and chunk-by-chunk
export; **Earnings** shows the service tickets peers signed for what you
served, priced at your rate as unsettled micro-MINI credit, redeemable only
by your DID once the audited settlement layer admits the request.
**Media** is a catalog: search by title, author or type, filter by kind,
sort, and browse a grid of poster cards; author names open a **Channel**
page; **Search my peers** asks saved peers and **Fetch** retrieves one post's
verified closure. **Shorts** plays media posts one at a time and **Watch** shows one with its
comments and what is up next; audio plays in-app through pure-Rust decoders
and animated GIF/WebP loop; H.264 video without B-frames plays in-process
(OpenH264 + AAC via symphonia), while B-frame H.264, H.265/VP9/AV1 and
WebM/MKV show an explanation with export. Connections can ask the router
to forward the hosting port (UPnP) and reports whether the result is a
public address. Ticket rates are agreed per exchange (provider ask capped by
the receiver's ceiling) and written into the ticket.

Connections is where networking starts, and only there:

- **Accept connections (host)** binds one port and serves many connections
  until you stop it. Peers reach you only if that port is reachable
  (port-forward, VPS, or the same LAN) — no relay or NAT traversal yet.
- **Sessions** dial every saved peer every 30 s (backing off to 2 min after
  failures) for 15 min, 1 h, or while the app is open, exchanging public
  posts, profiles, follows and reactions.
- **Connection cards** (`mininet-peer-v1;endpoint=…;did=…;name=…`) let a
  friend save your endpoint and follow your DID in one paste. A card grants
  nothing; names are trusted only once the signed profile arrives.
- **Private conversations** are included in sessions/hosting only when you
  enable it; otherwise a private request is refused before any route check.
- **On launch** nothing connects unless you enabled "session on launch" or
  "accept connections on launch" here. Both default off.

Under the hood every desktop path is one link: anonymous CH1 handshake over
TCP, one sealed intent frame (public `MINI/SYNC1` or one private route),
then that bounded protocol. Verified by crate tests over real TCP and by two
live instances on one Windows machine; two machines behind different NATs
have not been demonstrated. See
[`connected-mininet-client.md`](../../docs/proposals/connected-mininet-client.md)
for the full product specification and what remains.

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
2. Open People in Bob's window, allow nearby discovery, and choose **Be visible
   nearby for 60 seconds**. This is opt-in and temporarily reveals only Bob's
   chosen display name, DID, and listening endpoint on the LAN. The same
   bounded window can accept multiple sync connections.
3. Open People in Alice's window, allow nearby discovery, and choose **Find
   nearby for 3 seconds**. Select **Sync signed profile** on Bob's unverified
   announcement. Bob then appears as a signed profile card with the public
   photo, name, location, age, and custom details Bob chose to publish.
4. Alice chooses **Add friend** on Bob's profile. The button performs the
   explicit signed action, using the Windows user vault for a just-in-time
   unlock and restoring the previous locked state afterward. Because Bob's
   exact DID still has a current nearby endpoint, the same button immediately
   reconnects and delivers the signed follow; failure leaves the request
   safely stored locally and reports that delivery still needs a retry.
5. Bob's visible window receives Alice's signed profile and follow. For a
   mutual friendship, let that window end, make Alice visible, and have Bob
   scan nearby. Bob can then choose **Add friend** on Alice's already-verified
   profile and its signed follow is delivered the same way. The UI shows
   **Friends** only when both independently signed follow edges are present.

Accounts created by an earlier desktop beta show a one-time upgrade card in
People. That beta signed directly with the human root, so strict peer ingest
correctly rejected its social objects. The explicit upgrade creates a separate
DPAPI-protected delegated-device vault and re-signs the same public profile;
the human DID and owner-selected public details do not change. Older posts and
other legacy beta objects remain local and are not silently rewritten.

People search matches display names and `did:mini` identifiers among signed
profiles already on the device. Names are intentionally non-unique labels;
the DID is always shown as the stable identity anchor. Nearby announcements
are treated as spoofable connection hints and never as verified identity.

Direct peer sync is not limited to a LAN: enter a reachable public hostname or
IP plus port. The listener must be reachable through the firewall/NAT, or the
operators must deploy a relay; automatic NAT traversal and a hosted relay are
not silently assumed by the client.

Internet-wide name discovery is not yet implemented. It requires a deployable,
privacy-preserving index/relay design with signed profile provenance, abuse
controls, namespace ambiguity handling, and resistance to enumeration and
poisoning; LAN multicast is not presented as that production service.

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

The public-profile editor supports an optional signed media photo, location,
age, and up to 16 owner-defined `Label: Value` fields. Every optional field can
be omitted or removed, and a photo may be selected by dropping it onto the
Creator view. These are public claims selected by the profile owner, not
platform-verified attributes.

The human-root and delegated-device seed envelopes are separately protected
with the Windows-user DPAPI boundary by `mini-windows-vault`. Day-to-day social
objects use the scoped delegated device; sync distributes both self-certifying
KELs and rejects objects without valid device provenance.

The UI has an explicit identity lock and starts every session locked. A locked
client can inspect local data but cannot publish signed objects. Each publish
form also requires an explicit signing confirmation, and privacy settings are
stored through DPAPI.

The home view now reads the local feed, lets the user choose chronological or
most-supported ordering, publishes signed replies, and records signed likes.
These controls still remain local until a user enables a transport path.

Inbox beta provides a complete manual two-instance test path: create a
conversation for a peer DID, transfer the checksummed invitation code through
a trusted channel, import it into the other Windows profile, write signed
encrypted messages, and foreground-sync only that selected opaque route over
the encrypted TCP bearer. Conversation capabilities are stored through DPAPI.
Invitation codes contain the conversation key and therefore grant message
access; they are not usernames or public friend codes. This beta has no
prekey/ratchet protocol, mailbox relay, automatic retry, multi-device fanout,
or authenticated endpoint discovery and does not claim production secure chat.

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

The Connections view also exposes a one-shot direct-peer public sync using the
real encrypted TCP bearer and verified `MINI/SYNC1` ingest. It is
foreground-user initiated, runs off the UI thread, and accepts no silent
background network activity. People adds a separate opt-in three-second LAN
scan and 60-second visible window; announcements are unverified hints, each
socket has bounded I/O. Hosting and sessions in Connections are the only
repeating network activity and both are owner-started or owner-enabled.

It does not start networking on launch, open external URLs, collect telemetry, execute
updates, or embed a browser. Private signing material is not stored as
plaintext.

The Mininet system view reports actual local object counts and distinguishes
desktop-integrated features from repository foundations that do not yet have a
safe end-user workflow, including forge administration, keystone encounters,
reward accounting, and update adoption. It does not present placeholder
buttons for those unfinished surfaces.

Diagnostics runs the real protocol code on this device and shows what
happened: identity signing and pre-rotation, content addressing, the social
feed, chunked media, encrypted messaging, two stores converging over a real
loopback socket, a governed two-approval merge, erasure-coding recovery, and
storage proofs. Roughly a third of the checks establish a *refusal* --- one
approval does not reach the two-approval floor, an approval bound to one
commit does not carry to another, a second conversation's key reads none of
this one's messages --- because those are what show the guarantees are
load-bearing rather than merely that the happy path runs. Every check builds
its own throwaway state under the OS temp directory and deletes it, so a
diagnostics run cannot touch identities or posts. The same suite is
`mini selftest`, and CI runs it on every change.

Version & install reads the same state `mininet-setup.exe` writes: the
installed version, its package digest, what a rollback would return to, and
where program files and user data each live. It can re-hash every installed
file against its manifest, and it can roll back to a version already on disk
--- after verifying that version first. It cannot check for updates, download
a release, or install one. There is no background task and no timer; updating
means running the setup program with a package you obtained however you chose.

Install the client with `mininet-setup.exe` (see `packaging/windows/`), which
installs per-user with no administrator rights, verifies every file against a
manifest carrying both BLAKE3 and SHA-256 digests, registers in Apps &
features, and keeps identities when uninstalling unless explicitly told
otherwise.

Before distribution, the Windows client still needs hardware-backed key
storage, Authenticode code signing (builds are unsigned today, so SmartScreen
warns on first run and is right to), reproducible compiler output, store-level
cross-process coordination, and an independent security review. See
`docs/WINDOWS_CLIENT_SECURITY.md`.
