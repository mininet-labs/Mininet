# Mininet social network product contract

This document defines what “one place” means for Mininet. Mininet should feel
like one coherent client, not six cloned platforms. The same signed,
content-addressed objects power every surface, while the client presents
focused views for different intents.

## Product surfaces

| User intent | Familiar surface | Mininet view | Core objects |
| --- | --- | --- | --- |
| Share a thought, photo, link, or clip | Facebook / Instagram / X | Home feed and profile | `POST`, `PROFILE`, `MEDIA_MANIFEST` |
| Watch short and long media | TikTok / YouTube | Discover and creator pages | `POST` + media manifest |
| Discuss a topic | Reddit | Communities and threaded topics | `COMMUNITY`, `COMMENT`, membership |
| Follow people and creators | Every platform | Following, followers, public walls | `FOLLOW`, `WALL`, `PROFILE` |
| Save and react | Instagram / Reddit / YouTube | Reactions and personal saves | `REACTION` |
| Publish a project or release | GitHub | Forge portal embedded in the same client | forge objects |
| Watch external catalogs | Stremio / Torrentio | Optional source adapters | external-source metadata, never core trust |

The key integration rule is composability: a post may link a media manifest
and a community; a comment may reply to that post; a reaction may target either
the post or comment; a forge discussion may appear in the same feed. The
objects are not copied into platform-specific databases.

## Feed contract

The feed is a local view over objects the device has received. It must expose:

- why an item is present (`Own` or `Followed` today, with community/media
  reasons added as those surfaces land);
- which filter was selected;
- deterministic tie breaking;
- support counts when support ordering is selected;
- a finite page/limit rather than an unbounded engagement loop.

Initial filters are `Chronological` and `MostSupported`. Filters reorder the
eligible set; they do not silently erase followed speech. Personal mutes,
blocklists, community labels, and age/safety settings are explicit local
filters and must be inspectable separately from ranking.

## Community contract

Communities are portable cards with a name, charter, and declared admission
mode. A join or leave is a signed member-authored object and resolves with
last-write-wins. This provides Reddit-like communities without making a
directory or server the owner of the community graph.

The next community milestones are:

1. topic objects and concurrent comment edits using the existing CRDT layer;
2. label-based moderation where the author's object remains retrievable;
3. moderator policy objects and appeals;
4. local/gossiped discovery and community-specific feed filters.

## Media and creator contract

`mini-media` already provides chunked, resumable, content-addressed payloads.
The product layer should add media metadata (caption, preview, duration,
aspect ratio, language, subtitles, and content warnings) as signed links or a
versioned post payload. The player must support progressive nearby-first
fetching and clearly show when a source is incomplete.

Creators need profile pages, pinned collections, subscriptions, support
events, and analytics computed locally from received objects. Server-side
audience guarantees are not part of the sovereign core.

## External catalog and torrent adapters

Stremio/Torrentio-style discovery is useful, but it is an adapter boundary:

- adapters may import catalog metadata and source descriptors;
- playback must be opt-in and legal in the user's jurisdiction;
- source availability, copyright status, safety, and uptime must not be
  represented as Mininet governance facts;
- adapters must be sandboxed, version-pinned, and removable without breaking
  native social objects;
- Mininet should not silently proxy, seed, or download external content.

The native network remains the durable source for Mininet-authored media.

## Safety and trust

Identity, personhood signals, reputation, moderation labels, and popularity
are different concepts. None should silently gate a person's ability to speak.
The client should provide:

- block and mute controls local to the user;
- subscribed community labelers;
- “why am I seeing this?” and “why is this hidden?” inspectors;
- report/appeal objects rather than central deletion;
- clear provenance and incomplete-sync indicators;
- separate pseudonymous walls that are not linked unless the owner chooses it.

## Delivery sequence

The implemented foundation is profiles, follows, public walls, chronological
feeds, threaded comments, typed reactions, community cards, membership, and
support-aware ordering. The next vertical slice should connect these APIs to a
desktop reference client, then add media metadata/playback, notifications,
labels, and external-source adapters behind explicit feature flags.

This contract is intentionally honest: it describes the complete product
direction while distinguishing shipped protocol primitives from future UI,
transport, playback, governance, and adapter work.
