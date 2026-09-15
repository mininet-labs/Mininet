# Mininet desktop beta — setup and test guide

For testers of the Windows client on the `claude/connected-desktop-beta`
line (D-0523–D-0526). Everything below was exercised on Windows 11; other
platforms are not part of this beta. Nothing here moves money and nothing
here is anonymous: read "What this beta is not" before inviting anyone.

## 1. Get the client

**Option A — the built installer (recommended).** Every CI run on the branch
uploads a signed-by-manifest installer as a workflow artifact named
`mininet-windows-client-<commit>` (files: `mininet-setup-<version>-x86_64-pc-windows-msvc.exe`,
`Mininet-<version>.msi`, `*.manifest.txt`, `SHA256SUMS.txt`). Download it from
the **Actions → ci → windows-packaging-pipeline** run for the commit you are
testing, check `SHA256SUMS.txt`, run `mininet-setup.exe` and follow the wizard.
The installer re-verifies its own manifest and refuses downgrades.

**Option B — build from source.**

```powershell
git clone https://github.com/mininet-labs/Mininet.git
cd Mininet
git switch claude/connected-desktop-beta
cargo run --release -p mini-desktop
```

Requirements: the pinned Rust toolchain in `rust-toolchain.toml` (rustup
installs it), MSVC Build Tools (C++ — the in-process video decoder compiles
OpenH264 from source), and about 10 minutes for the first build.

**Data location.** Each identity lives in `%LOCALAPPDATA%\Mininet`. To run
two testers on one machine, start each with its own `MININET_HOME`:

```powershell
$env:MININET_HOME="$env:LOCALAPPDATA\Mininet\profiles\alice"; .\mininet-desktop.exe
```

## 2. First launch: identity and public account

1. **Create local root.** A signing root protected by the Windows user vault
   (DPAPI). Nothing is uploaded; there is no account server.
2. **Create your public account**: display name, optional bio; confirm the
   signing checkbox. You can add a photo, location, age and custom fields
   later in **Creator studio**.
3. You land on **Home**. Top bar: connection state pill (Offline right now),
   **Unlock identity** (needed to sign posts; follows, likes, replies and
   library uploads unlock just-in-time and re-lock), **Privacy**.

Nothing has touched the network yet. The app never starts networking on
launch unless you enable that yourself in Connections (step 4).

## 3. Windows Firewall

The first time you **host** (accept connections), Windows asks whether to
allow `mininet-desktop` on private/public networks. Click **Allow** —
hosting does not work otherwise. Sessions (outgoing) need no prompt.

## 4. Connect two machines

Pick one tester to **host**; the other **dials**. Both can host if both are
reachable.

**On the host (A):**

1. **Connections → Accept connections (host)**: port defaults to 46000.
   Press **Start hosting**. The pill turns **Hosting**.
2. **Reach me from the internet → Open port on router (UPnP)**. Outcomes:
   - **MAPPED** with a public address: your card now carries it. Done.
   - **MAPPED · NO PUBLIC ADDRESS**: your router is itself behind another
     NAT or carrier-grade NAT. Forward the port on the upstream router, host
     from a machine with a public address, or test on the same LAN
     (use **Detect LAN address**).
   - **Router mapping failed**: UPnP is off on the router. Forward TCP 46000
     to this machine by hand and type your public IP into *Reachable host*.
3. **Your connection card → Copy card** and send the text to B over any
   channel you trust (it carries your address and public DID; it grants
   nothing by itself).

**On the dialer (B):**

1. **Connections → Saved peers → Add from a connection card**: paste, press
   **Add peer and follow**. That saves A's endpoint and signs a follow of A's
   DID.
2. Choose a **session length** (15 min / 1 h / while open) and press
   **Start session**. The pill goes **Connecting → Connected** within ~30 s.
   A's Activity shows "public exchange: received …"; B's shows "Peer sync
   complete …". Both sides show **tickets: attested … KB**.
3. Optional, both sides: **Connection policy** — "Start a session with my
   saved peers when Mininet opens" / "Accept connections when Mininet opens"
   / "Renew the router mapping…". All default off.

**What to check:** A's profile appears in B's **People** and in **Who to
follow**; B appears on A's side after A follows back (People → Follow, or
the discovery column). A follow is mutual only after both signed follows
have crossed.

## 5. Post, read, react

- **Home → What is happening? → confirm → Post.** It shares on the next
  exchange (≤30 s in a session). The other side sees **Home (1 new)** in the
  rail.
- **Following** shows you + people you follow; **Everyone** shows all
  received posts. **Explore** searches the last 50 received posts.
- **♥** likes, **💬** opens the thread (replies are threaded, nested), the
  **ℹ** menu shows the author DID, lets you copy it or **mute** the author on
  this device (hides them from timelines, suggestions and directory; publishes
  nothing).

## 6. Files, movies and music (Library)

1. **Library → Add a file or movie**: type a path or **drop a file onto the
   window**; name and type are guessed; confirm; **Add to library**. Files up
   to 256 MiB are one manifest; larger ones (to 64 GiB) become a collection of
   parts. Progress shows chunks present / total.
2. **Share as post** with a caption. The first caption line is the title
   people search for; the rest is the description.
3. The other side sees it in **Media** (grid with posters), **Shorts**, and
   **Watch**. Seeding: everything complete on a hosting machine is served;
   a machine holding some parts seeds those parts.
4. **Export to disk** writes a complete file chunk by chunk.

**Playback:**
- Music (MP3/FLAC/Ogg/WAV/AAC-in-MP4): **Play**, **Play next**, **Play all**
  in Library; now-playing bar with seek, pause, volume, skip.
- GIF/WebP: loops in Shorts and Watch.
- Video: **H.264 in MP4/M4V/MOV without B-frames** plays in-process with
  AAC. A file **with B-frames** (common for phone recordings and x264
  defaults) shows an explanation instead — re-encode with
  `ffmpeg -i in.mp4 -c:v libx264 -bf 0 -c:a aac out.mp4`, or export to
  watch. H.265/VP9/AV1/WebM/MKV do not decode in-app.

## 7. Find things (Media catalog, channels)

**Media**: search box (words over title, description, author, DID, type),
filters (Video / Music / Images / GIFs / Files), sort (Newest / Most liked /
Most discussed), grid of poster cards. Click a card → **Watch** (big stage,
comments, "Up next"). Click an author name or avatar → **Channel** (profile,
follow, message, their media and posts). The search box covers what your
device holds; **🌐 Search my peers** asks every saved peer for matches and
lists them under "On your peers" with a **Fetch** button that retrieves
exactly that post (identity, manifest, chunks) verified on arrival, after
which it plays like anything local. Search reaches your saved peers, one
hop — not the whole network.

## 8. Private messages

1. **People → Message** on a profile (or **Messages → Create an invitation**
   with the peer's DID). Create the invite and send the code to the peer over
   a trusted channel — **it contains the conversation key**.
2. The peer imports it in **Messages → Import an invitation**.
3. Both sides enable **Connections → Connection policy → Include my private
   conversations in sessions and hosting**. Messages then deliver on the
   next exchange; otherwise use the manual "Deliver selected conversation"
   controls. The Messages list shows peer, last message and counts.

## 9. Communities (Reddit-style)

**Communities → Create** (name, charter). **Open** it → **Start a
discussion** (title + body) → threads with **⬆ upvotes**, nested **Reply**,
collapse, mute, Top/New ordering. Discussions are ordinary signed comments
and replicate like posts. **Join** publishes a signed membership.

## 10. Earnings (service tickets)

Every exchange ends with each side signing a **service ticket** for what it
received, naming the other side's DID. **Earnings** shows credit as host and
as creator (media you authored that completed on someone else's device),
what you owe, the agreed rate per ticket, and lets you build a
**redemption request** only your DID can build or verify. Credit is
**unsettled**: nothing is paid until the audited settlement layer admits a
request. Rates: **Your rate** (your ask) and **most I pay per MB** (your
ceiling); each exchange agrees `min(provider ask, receiver ceiling)`.

## 11. What to test and report

Please report each item as pass/fail with the two `Activity` texts:

1. Two machines on **different networks** (one with UPnP or a port-forward):
   session connects, posts and profiles cross both ways, tickets appear.
2. **Restart both apps**: with launch policies on, hosting/session resume;
   with them off, nothing connects until you press a button.
3. Follow, like, reply, mute, unmute; Home (N new) appears on the other side.
4. Upload a 5–50 MB file; it appears on the other side with a poster and
   plays (no-B-frame H.264) or explains why not.
5. A file **>256 MiB** (collection): progress across parts, export matches
   the original (compare SHA-256).
6. Private conversation with "include private conversations" on both sides:
   messages deliver without manual sync.
7. Community: thread, reply, upvote cross to the other side.
8. Earnings: host credit on the serving side, creator credit for your own
   media completed on the other side, redemption request verifies.
9. Stop a session mid-exchange; the app stays responsive; the next exchange
   resumes chunks rather than starting over.
10. Media → type a title word → **Search my peers** → **Fetch** a hit you do
    not hold; it should flip to "on this device" and play/export.
11. Diagnostics → run the suite; note any failed check.

Include: Windows build, both `Activity` panels, the connection card host
part (redact the DID if you like), and whether UPnP reported a public
address.

## 12. What this beta is not

- Not anonymous: peers see your IP. Not a relay: behind CGNAT you need a
  reachable partner. Search reaches saved peers one hop deep, not an index
  of the whole network.
- Not money: tickets are signed evidence with unsettled credit.
- Not production messaging: invites carry the conversation key; there is no
  ratchet or mailbox yet.
- Not moderation: muting is device-local.
- Ticket objects replicate to everyone and are not pruned yet.
