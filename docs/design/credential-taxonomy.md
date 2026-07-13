# Credential taxonomy: participant, human, role, resource

Founder review (`Mininet_In_Depth_Review_20260712.md`, P0 backlog item
`credential-separation`, Value 2/Value 8/Value 9, Phase 0 exit criterion):
name and separate four distinct claims this tree makes about a `did:mini`
identity, so that a reviewer — or a future automated check — can recognize
at a glance which class of claim a given code path is actually relying on,
and catch a role-level consequence (a bad review, a missed heartbeat, an
equivocation) before it can accidentally mutate a claim it has no business
touching.

This is a **naming and mapping document, not a new abstraction layer**.
Every mechanism named below already exists, is already tested, and keeps
its current API. Nothing here introduces a new wrapper type, a new trait,
or a new dependency edge. What changes is that this tree now has one
place that states, plainly, which existing mechanism answers which
question — so the next PR that's tempted to let a role penalty quietly
touch personhood, or a resource commitment quietly touch governance
weight, has a document to be checked against.

## The four classes

### 1. `ParticipantCredential` — "a key-controlled identity exists"

**What it claims:** nothing about humanity, trustworthiness, or standing
— only that a `did:mini` identifier is internally valid (self-certifying,
hash-chained, correctly signed). Bots, scripts, and test fixtures all
qualify equally with a human's root identity.

**What answers it today:** `did_mini::Kel::verify()` returning `Ok(KeyState)`,
or equivalently possessing a `did_mini::Controller`. This is the *only*
claim required for ordinary object publication, hosting, search, and
agent activity (Value 8's "ordinary objects... must never require
[humanity]").

### 2. `HumanEvidence` — "evidence exists toward this root being one human"

**What it claims:** a derived, decaying, multi-source confidence score —
never a proof. This is explicitly **not** a credential in the review's
sense: it is the raw accumulator a future credential would be minted
from, not something itself presented as authorization.

**What answers it today:** `mini_uniqueness::HumanRecord`/`HumanStatus`
(D-0038, D-0054, D-0086). `HumanStatus::EvidenceQualifiedHuman` — renamed
from the review's own flagged `FullHuman` (D-0086) precisely because it
is evidence, not a credential — is the strongest tier this mechanism can
honestly produce today.

### 3. `RoleCredential` family — "this identity currently holds a specific, revocable authorization"

**What it claims:** narrow, scoped, revocable authority to perform one
specific function. Never humanity, never extra governance weight —
Directive 16/P1's voice/value wall and P2's one-root-one-vote rule both
still apply underneath every role.

**What answers it today**, one mechanism per role, deliberately not
unified into a single type (each already carries its own revocation,
rotation, and eligibility rules, and collapsing them would blur exactly
the separation this document exists to preserve):

- Device-scoped authority within one root: `did_mini::Capabilities`
  (`SIGN`/`PAY`/`POST`/`ATTEST`/`VOTE`/`MANAGE_DEVICES`) and
  `did_mini::BaseDeviceRole` — can only *narrow* a device, never inflate
  the root's standing (delegation.rs's own module doc).
- Validator eligibility: `mini_chain::ValidatorSet` membership.
- Forge maintainer/reviewer authority: `mini_forge::governance`'s
  maintainer set and approval protocol.
- Treasury/bridge custody signing: `mini_treasury`'s per-vault signer
  committees (D-0059/D-0060; see "Custody separation" below for the
  cellular design that keeps these committees themselves disjoint).

A validator that equivocates (D-0088's `EquivocatorRegistry`) is exactly
a `RoleCredential`-scoped finding: it can, once a real consequence
mechanism exists, cost that root its *validator* eligibility. It must
never be wired to cost the root its `HumanEvidence` or any
`HumanCredential` it might someday hold — that would be precisely the
Value-9 failure ("reverse liability") the review names as a hard
regression to prevent going forward.

### 4. `ResourceCredential` family — "this identity has committed storage/bandwidth"

**What it claims:** a measurable resource commitment, never standing.

**What answers it today:** `mini_storage`/`mini_reward`'s receipt and
accrual types. Storage/seeding reward accrual is already confirmed
(`docs/STATUS.md` §1) to create no governance weight — this class is
already correctly walled off from voice today, this document just names
the wall's other side.

## What does not exist yet, honestly

**`UniqueHumanCredential`** — an epoch-bound, unlinkable, nullifier-based
proof that a `HumanEvidence` accumulator represents a genuinely distinct
human — is **not implemented**. This is Phase 2 (personhood research
network) work, not Phase 0: it requires scope-specific nullifiers, a
multi-proposer root/challenge/fraud-proof protocol, and a public
adversarial pilot before it could honestly be called a credential rather
than a score. Nothing in this tree may claim to grant one today. Every
`HumanStatus` variant remains, and is documented as, `HumanEvidence`,
never a `HumanCredential` — see `docs/INVARIANTS.md`'s hard limitations
and `crates/mini-uniqueness/src/status.rs`'s own doc comment.

## Custody separation (`custody-separation` P0 item)

`docs/design/treasury-economic-model.md` §9-10's cellular custody design
already separates every vault "by asset, purpose, liquidity venue,
operational region, and custody committee," and already forbids any
single majority from controlling "rate-source administration, receipt
verification, custody signing, mint authorization, and accounting"
together. That document is amended alongside this one to state the
review's exact ask explicitly and unambiguously, rather than leaving it
implied by the general cellular principle: **a bridge-specific vault's
signer committee and the general treasury's signer committee are always
disjoint sets — no individual may hold a seat on both.**

## `docs-supersession` (P0 item) — an honest non-finding

The review's `docs-supersession` item asks to "mark old Cosmos, LoRa,
reverse-liability, LE-cooperation, and old economics text historical."
A full sweep of this repository's own canonical documents
(`docs/DECISION_LOG.md`, `docs/FAILURE_BOOK.md`, `docs/ROADMAP.md`,
`docs/INVARIANTS.md`) found no live contradiction to mark: the Cosmos
SDK path is already recorded as a rejected path in
`docs/FAILURE_BOOK.md`'s "Cosmos SDK / Go chain stack" entry; `LoRa`,
`reverse-liability`, and mandatory law-enforcement-cooperation language
do not appear anywhere in this repository at all. The contradictions the
review names live entirely in the founder's externally-held SPEC-00
through SPEC-11 whitepapers and sprint plans, which were never committed
to this GitHub repository and are therefore outside what this session can
read or edit. Superseding those requires either committing them here for
editing or the founder handling it in whatever system currently holds
them — this document records that finding honestly rather than silently
skipping the item or inventing repository content to "fix."

## Non-goals of this document

- Does not create a new Rust type, trait, or crate.
- Does not change any existing function's signature or behavior.
- Does not solve Sybil resistance or personhood (roadmap #18) — it names
  the boundary the unsolved problem sits behind, so future code cannot
  quietly blur it.
- Does not replace the review's own required work for
  `UniqueHumanCredential` (Phase 2, P2 backlog) — that remains entirely
  future, funded, adversarially-tested research, not a naming exercise.
