# Git import bridge

**Status:** doctrine design doc, no code shipped yet. Companion code lands
as a follow-up PR under this same decision.

**Refs:** D-0004 (SHA-256 retained for the git-object interop path);
`docs/design/self-hosted-forge-spine.md` (names the export bridge shipped
and import "genuinely unstarted"); `crates/mini-forge/src/git_export.rs`
(no dedicated decision-log entry of its own — shipped as a Batch 1
deferred item, per that design doc); roadmap #65/#102 (self-hosted forge
spine); Directive 4, Directive 13, Directive 16.

## Why

`docs/design/self-hosted-forge-spine.md` and `CLAUDE.md` both name "GitHub
import/export mirror automation" as a remaining Batch 5/6 gap. The export
half already exists and is genuinely verified against real git (`crates/
mini-forge/tests/git_export.rs` hashes the same inputs with the real `git`
binary and asserts byte-identical object ids) — but `git_export.rs`'s own
module doc is explicit that it is "genuinely one-directional" and that
import is a **materially different problem**, not a missing symmetric
half: "verifying and re-signing untrusted git history against this tree's
identity model is a materially different problem than emitting bytes from
objects this store already trusts." This doc resolves that problem
honestly before any code is written against it.

## The core tension

Every object `mini-forge` stores requires a real `did:mini` signature —
`put_file`, `put_tree`, and `commit` all take `human: &Did, device:
&Controller` and refuse to construct an object without signing it
(`ObjectBuilder::sign`). A real git commit, by contrast, carries no
`did:mini` signature at all — only a free-text author name/email git never
authenticates.

This means a git import **cannot** claim the imported content was
authored by the original git commit's author under this tree's identity
model, because nobody holds that authority: the importer does not have the
original author's signing key, and inventing a signature would be forgery,
not import. The only construction that is both possible and honest is:
**the importer signs an object attesting "I imported this content, and it
verifiably reproduces real git commit `<sha256>`, whose git-visible author/
committer metadata was X"** — never "I am X."

## Decision

1. **Content (blobs, trees) is re-signed by the importer, verbatim bytes
   preserved.** `import_git_blob`/`import_git_tree` parse real git SHA-256
   object framing (`"<kind> <len>\0<body>"`), **verify the caller's claimed
   object id against the actual SHA-256 digest of the bytes** (never
   trusted), and reconstruct mini-forge `FILE_TYPE`/`TREE_TYPE` objects via
   the existing `put_file`/`put_tree` — signed by the importer's own
   `(human, device)`, exactly like every other object this crate produces.
   Content integrity survives exactly (the bytes are copied verbatim, so
   the tree/blob's own hash chain is real); **authorship does not** — the
   imported tree's signed author is the importer, not whoever wrote the
   original git tree (which was never a signed claim in the first place).

2. **The commit is built with the existing, unmodified `commit()`
   function** — same shape, same strict link parsing `checkout()`/
   `validated_commit_tree` already enforce (`ObjectType::COMMIT`, exactly
   one `"tree"` link, zero-or-more `"parent"` links, plain-bytes message
   payload). The original git commit message is passed through verbatim as
   the mini-forge commit's message. This makes an imported commit
   **indistinguishable in shape** from a native one and fully
   `checkout()`-compatible — the only thing that differs is, again, the
   signed author: the importer, not the original git committer.

3. **Git-only provenance is a separate, explicitly-linked object, never a
   field smuggled onto the commit.** `checkout()`'s `validated_commit_tree`
   rejects any link relation on a commit besides `"tree"`/`"parent"` by
   design (`_ => return Err(ForgeError::BadObject)`), and this doc does not
   propose changing that frozen shape. Instead, `import_git_commit`
   produces a second signed object — `GitImportProvenance` — that links
   `"commit"` to the mini-forge commit it describes and carries the
   original git commit's SHA-256 id, and the original author/committer
   name, email, and timestamp exactly as the git object stated them
   (unauthenticated data, explicitly labeled as such — this crate makes no
   claim these strings are true, only that they are what the cited git
   commit object literally said). A verifier who wants "was this really
   imported from git commit X" reads this object, not the commit's own
   fields.

4. **Existing name/size limits are inherited, not loosened.** `put_tree`
   already restricts entry names to `valid_name` (ASCII alphanumeric plus
   `-`/`_`/`.`, ≤64 bytes) and `MAX_TREE_ENTRIES`; `commit` already caps
   message bytes at `MAX_MESSAGE_BYTES`. A git tree/commit whose real
   content violates these (unicode filenames, spaces, oversized trees or
   messages) fails import with the same error an equivalent native
   `put_tree`/`commit` call would produce — this is an existing limitation
   of `mini-forge`'s object model, not a new restriction invented for
   import, and is out of scope to relax here.

5. **Scope: canonical git commit shape only, fail closed on anything
   else.** The first slice parses exactly the canonical commit format
   `git_export.rs` itself writes (`tree`/`parent*`/`author`/`committer`/
   blank line/message, `+0000` or any parsed offset accepted but not
   re-derived). Commits carrying a `gpgsig` header, an `encoding` header,
   or any other line this parser does not recognize are **rejected**, not
   silently ignored — a parser that silently drops an unrecognized header
   could misrepresent what a commit actually said.

## Alternatives considered

- **Re-signing as the original author** — rejected outright: cryptographic
  forgery, not a design tradeoff.
- **A canonical "imported, unsigned" object class that skips signing
  entirely** — rejected: every other object in this store is signed
  (`ObjectBuilder::sign` is not optional), and an unsigned exception would
  be a new, narrower authenticity model to reason about everywhere else
  that reads objects from the store.
- **Embedding git provenance directly in the commit's own payload
  alongside the message** — rejected: it would either break
  `checkout()`'s strict payload/link shape or require loosening it, and
  mixing "the message" with "unauthenticated git metadata" in one field
  invites exactly the kind of ambiguity item 3 above avoids by using a
  separate, clearly-labeled object.
- **A full arbitrary-git-history importer (rename detection, merge
  commits, submodules, LFS, signed tags)** — rejected for this slice as
  far more scope than the named gap ("GitHub import/export mirror
  automation") requires to prove the direction works at all; each is
  real, separately-scoped follow-up work.

## Constitutional impact

None intended. No frozen invariant is amended. `checkout()`/
`validated_commit_tree`'s existing strict shape is reused unmodified, not
loosened. No new cryptography — content is re-signed with the same
`mini-crypto` Ed25519 signing every other `mini-forge` object already
uses. The typed-domain rule is respected: `import_git_commit` takes an
exact importer `(Did, &Controller)` plus parsed git object data, never a
generic `sign(&[u8])`. Nothing here grants a caller any special authority —
review/merge/release still work exactly as they do for a native commit,
including requiring the same governance approvals to actually adopt
imported history onto a governed branch.

## Failure point

An importer can import a git commit that lies about its own history (a
tree that does not actually correspond to the working state a human
expects) — this bridge only proves the imported bytes matched the claimed
git object ids, never that the *content* is good, safe, or the "real"
upstream history; that judgment remains a reviewer's, exactly as it already
is for any native commit. The `GitImportProvenance` object's author/
committer strings are copied verbatim from the git object and are not
independently verified against anything (git itself never authenticates
them) — a verifier must treat them as a claim, not a proof, same as this
crate already treats a `ProviderDeclaration`'s free-text fields elsewhere.
GPG-signed and otherwise non-canonical commits are rejected, not imported
with signature information silently dropped.

## Required follow-up

A real "clone a GitHub repo and walk its object graph" driver (this slice
takes already-parsed `GitObject`-shaped bytes, the same shape
`export_commit_chain` already produces, so it composes with export's own
test fixtures immediately, but does not itself fetch anything over the
network); merge-commit/rename/submodule/LFS/signed-tag support; a governed
-adoption ceremony deciding when imported history is allowed onto a real
branch; and external review of the provenance-object shape before it is
relied on for anything beyond "for the record."

## Supersedes / superseded by

Builds on, and does not supersede, D-0004. Does not touch
`checkout`, `validated_commit_tree`, `commit`, `put_tree`, or `put_file`'s
existing behavior.
