# Resident AI

Resident AI is the owner's assistant inside Mininet: help using a node, explain
received material, inspect source, propose changes, and organize evidence for
human review. The name identifies a feature, not a standing office, privileged
identity, mandatory model, or central service. Owners may disable it or replace
the model. Directive 12 governs: AI may propose; people decide.

## How it fits the ecosystem

| Plane | Existing Mininet role | Resident AI's role |
|---|---|---|
| Content | Signed, content-addressed objects and chunked media move through owner-controlled stores and peer synchronization. This supplies the torrent-like hosting foundation. | Explain sharing, inspect explicitly selected source and calculate a file/chunk/replica plan. It does not start hosting or prove that copies are independently available. |
| Value | Existing value, settlement and fee mechanisms remain their own protocol and audit domain. | Explain costs and compare owner-supplied arithmetic estimates with a budget. It cannot authorize payments, custody keys or mint receipts. |
| Assistance | An owner-selected local OpenAI-compatible model server produces text. | Coding, checks, local source search, governance analysis, hosting and fee guidance, with explicit AI labeling and context grants. |
| Governance | Human decisions and exact-state review/merge/release rules establish authority. | Advisory analysis and suggested findings only. Resident AI has zero vote or approval weight, and no merge or release permission. |

This change implements the assistance interface and safe local tools. It does
not claim to implement a decentralized paid inference marketplace. A later
peer-compute design can reuse signed content/model manifests, scoped jobs,
bounded resource offers, exact result hashes, owner budgets and existing value
protocols. Such offers and model outputs must remain evidence, never personhood,
governance authority, or an automatic payment instruction. No new fee schedule,
model registry, mandatory AI provider or economic legitimacy is adopted here.

## Desktop use

Open **Resident AI** from the Mininet desktop navigation. Model setup asks for a
numeric loopback HTTP endpoint and model name. Save configuration, enter a
question and explicitly press **Send to local model**. No request starts on
application launch or simply opening this view.

The owner must install/start a compatible model server separately; Resident AI
does not bundle weights, install dependencies, start a model or pretend an
unavailable provider answered. Examples of the endpoint shape are
`http://127.0.0.1:11434/v1/chat/completions` and
`http://127.0.0.1:8080/v1/chat/completions`. Model names come from the owner's
actual server; the examples do not assert that a particular model is installed.

Optional context is a previewed source file from an explicitly chosen workspace.
The checkbox grants only the exact preview to the next request. Changing the
path clears that grant. Local inspection/search results stay local and are not
automatically attached. Model responses run on a worker rather than blocking
the renderer; one model request is active at a time. Cancel/clear discards late
output, without starting another request until the bounded worker exits.

Eight turns are kept in panel memory, not published or automatically added to
the next prompt. Responses display the model's reported name and an AI advice
label. The endpoint's model name is not a cryptographic proof of model weights.

## CLI use

The desktop and CLI share `<MININET_HOME>/resident.json`, defaulting to
`~/.mininet/resident.json`. The desktop displays this configuration path. It is
separate from the object-store directory and contains no API/signing key.

```text
mini --json resident status
mini resident configure --endpoint http://127.0.0.1:11434/v1/chat/completions --model <installed-model>
mini resident ask "Explain this module and suggest a focused check" --workspace <source-root> --context src/lib.rs
mini --json resident inspect --workspace <source-root>
mini --json resident search --workspace <source-root> --query "verify_release"
mini --json resident check --area forge
mini --json resident hosting --workspace <source-root> --file assets/sample.bin --replicas 3
mini --json resident fees --bytes 1048577 --units-per-mib 3 --copies 2 --max-units 10
mini --json resident capabilities
```

`status` distinguishes saved configuration from actual provider availability.
Inspection, search, diagnostics, hosting plans and fee arithmetic work without
a model. `check` invokes only the existing shipped diagnostics in throwaway
state; it cannot run a model-selected shell command or untrusted Cargo build
script. Its result is check evidence, not an external audit or formal proof.

Hosting returns a streamed BLAKE3 digest, size and chunk/copy arithmetic. No
peer connection, data upload or hosting window starts. Requested copies do not
prove independent replicas. Fee inputs are owner-supplied abstract units, not
an authenticated market quote or a declared MINI price; checked arithmetic
fails on overflow and never authorizes a transaction.

## Code changes remain explicit

Model output is plain advisory text. It never dispatches CLI commands, writes
files, installs software, publishes objects, changes governance or spends funds.
To apply a reviewed draft, prepare an edits JSON array:

```json
[{"path":"src/lib.rs","before_hash":"<BLAKE3 of exact old UTF-8 bytes>","replacement":"reviewed replacement text"}]
```

`before_hash:null` creates a new source file, only when it does not exist and
its parent already exists. Then:

```text
mini --json resident patch-preview --workspace <source-root> --edits edits.json --output preview.json
mini --json resident patch-apply --workspace <source-root> --preview preview.json --confirm <exact-preview-id>
```

Preview creates a new document and changes no source. Its ID binds the selected
workspace, paths, before hashes and replacement bytes. Apply recomputes the
preview, requires that exact owner confirmation and rejects stale source.
Writes are staged, executable permissions are retained, and publication errors
trigger rollback. Multiple files are not a crash-atomic filesystem transaction.
The owner must control the workspace; this is not a sandbox against another
local process racing filesystem paths. Neither operation commits or merges.

Follow with the native Forge's exact-head proposal/review process. Sensitive
AI-assisted changes still require the existing human review floor. Local JSON
advice is not a signed human object or an approval. Future publication of AI
evidence must use the existing separate `AiObject` envelope and AI-disclosure
capability, rather than laundering replies into human-authored objects.

## Bounds and privacy

- Questions: 16 KiB; context: 32 files / 256 KiB total; replies: 128 KiB.
- Provider: 2 MiB encoded request, 1 MiB response, 16 KiB HTTP headers,
  2,048 requested output tokens, 10-second connect/write timeouts and a
  60-second absolute response deadline (including CPU inference wait). Truncated/tool-call completions are rejected.
- Search: literal UTF-8 source, 256 KiB per file, 16 MiB scanned content,
  100 hits; binary/large files are excluded and budget exhaustion is explicit.
- Patches: at most 16 files / 256 KiB replacement text; explicit confirmation
  and before-content hashes. Hosting summaries stream at most 64 MiB.

Only numeric loopback endpoints are accepted; no DNS, redirects, inherited
proxy/API credentials, remote fallback or auto-download occurs. A loopback
address is a transport boundary, not proof that the chosen server itself does
not forward data elsewhere. The owner controls that server and its policies.

Metadata/key directories, known secret filename patterns and symlink paths are
refused. This is not a complete secret detector: an ordinary source file can
still contain sensitive text. The owner reviews attachments before granting
them. Workspace inspection/search does not upload results. Resident AI never
opens wallet or identity seeds and has no dependency on financial or governance
authority crates. Existing hardware, cryptographic audit, economic research and
release-adoption gates remain separate from these software checks.

## Validation

Core tests cover grant integrity, context/patch bounds, stale edits, exact owner
confirmation, symlinks and binary hashing. Real loopback HTTP fixtures cover
requests, chunked/closed responses, malformed data, remote endpoint rejection,
truncation/tool metadata and unavailable servers. CLI tests exercise an actual
provider exchange, local tools, fees, diagnostics and confirmed patches; desktop
state tests cover attachment snapshots and cancelled late output. These tests
verify implementation behavior, not a particular model's capability or output
truth. No real model inference or hardware benchmark is implied.

## Desktop reviewed edit composer

**Reviewed local code edits** now provides an owner-operated path from a draft
to local file changes. Select a source workspace in Resident AI, enter a relative
UTF-8 source path and load its baseline. Choose creation explicitly for a new
path; its parent must already exist. Edit the complete replacement and stage it.
Up to 16 files and 256 KiB replacement bytes may be staged. An empty replacement
writes an empty file; this composer does not delete paths or create directories.

**Use answer as unapplied replacement draft** copies the model answer literally
into the editor. It does not parse code fences, commands, tool calls or JSON.
Remove explanation/fences yourself and inspect the exact replacement. Drafts and
file reads stay local unless separately selected as model request context.

Create a fresh preview and compare every original with its complete replacement.
The ID binds the backend's canonical workspace root, paths, original content
hashes and replacement bytes. Applying requires typing that exact ID. Changes to
the workspace, composer or staged list clear preview confirmation. The backend
revalidates source at apply time and refuses stale edits. Success reports exact
paths and the preview ID; it does not imply a commit, tests, review approval,
merge or publication. Model text never authorizes a filesystem write.

Reads/preview/apply run in a worker. Source-root editing is disabled while the
worker runs, results are polled even when the owner navigates elsewhere, and
ordinary window-close requests are deferred during an apply. Writes cannot be
cancelled halfway through. OS termination, power loss and another local writer
are outside this UI guard: keep a clean owner-controlled workspace and backups.

Cooperating Resident writers take a workspace lock. Replacements and independent
rollback copies are staged and synced before publication. Existing files use
rename replacement; unsupported filesystem/platform replacement fails without a
truncate/delete fallback. New files use no-overwrite hard links and therefore
require hard-link support. Existing portable permissions are retained. Detected
publication errors trigger best-effort restoration from staged originals, while
intervening changed destinations are left for manual recovery. Failed recovery
reports retained original paths. Individual files are published atomically;
multiple files are not one crash-atomic transaction and ordinary editors do not
participate in the Resident lock. Inspect the workspace after any uncertain
result rather than assuming all-or-nothing completion.

`.resident*` lock/staging/recovery artifacts are excluded from Resident context,
inspection/search and native Forge workspace snapshots. Stale recovery files
are not automatically imported, sent to a model or swept away; inspect and
recover them before an owner removes them. Add `.resident-patch*` to `.gitignore`
in any Git-backed workspace before mirroring it (this repository includes the
rule); native Forge snapshots exclude these artifacts independently. No shell/test execution, automatic
commit, native proposal submission, model-generated multi-step agent loop or
production readiness of the wider ecosystem is claimed by this feature.
