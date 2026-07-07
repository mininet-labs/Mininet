//! Mininet's op-log CRDT (SPEC-09 §3): multi-author mutable state as
//! **append-only logs of signed operations** that merge conflict-free and
//! offline-first — the same machinery for forum threads, conversations, shared
//! docs, and forge PR discussions.
//!
//! ## Ops are ordinary objects
//!
//! Every operation is a signed [`Object`] of type [`ObjectType::CRDT_OP`], so
//! signing, provenance, content-addressing, storage, and sync all come from the
//! existing layers. An op links its document root (`"doc"`) and carries a small
//! payload:
//!
//! - **Add** — create a node (comment/entry) under a parent (the doc root or
//!   another Add).
//! - **Edit** — replace the body of a node.
//! - **Tombstone** — retract a node (a tombstone, not an erasure: bytes that
//!   left your device may persist elsewhere; the protocol never pretends
//!   otherwise).
//!
//! ## Why we own this CRDT: one-human authorship
//!
//! Edit/Tombstone authority belongs to the node's **human** (`author_human`),
//! not the device that happened to sign the Add — your phone may edit what your
//! laptop wrote, and nobody else's device may (community-governed moderation
//! acts through filters/labels, SPEC-10 — never by rewriting someone's words).
//! That rule is the reason this is our own ~300 lines instead of a dependency.
//!
//! ## Convergence by construction
//!
//! [`replay`] computes state as a fold over the op **set** with
//! order-independent rules — Adds are set membership, Edits are per-node
//! last-write-wins by `(sequence, op id)`, Tombstones are monotone — so every
//! replica that has seen the same ops derives the identical state **in any
//! arrival order**, with no coordination. Invalid ops are deterministically
//! *excluded* (and reported), never fatal: one hostile op can't poison a
//! thread. Ops whose parent hasn't arrived yet are `pending` and join the tree
//! the moment it does.
//!
//! Display ordering of siblings is by `(timestamp_ms, op id)` — timestamps are
//! author-claimed hints; the id tiebreak keeps ordering identical everywhere.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use std::collections::BTreeMap;

use did_mini::{Controller, Did};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};

/// Maximum body bytes for an Add/Edit.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

const OP_ADD: u8 = 1;
const OP_EDIT: u8 = 2;
const OP_TOMBSTONE: u8 = 3;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, CrdtError>;

/// Why an op could not be *built*. (Replay never fails on op content — invalid
/// ops are excluded deterministically instead.)
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CrdtError {
    /// Body exceeds [`MAX_BODY_BYTES`].
    BodyTooLarge,
    /// The underlying object could not be built/signed.
    Object(mini_objects::ObjectError),
}

impl core::fmt::Display for CrdtError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CrdtError::BodyTooLarge => write!(f, "op body too large"),
            CrdtError::Object(e) => write!(f, "object: {e}"),
        }
    }
}
impl std::error::Error for CrdtError {}
impl From<mini_objects::ObjectError> for CrdtError {
    fn from(e: mini_objects::ObjectError) -> Self {
        CrdtError::Object(e)
    }
}

/// Build an **Add** op: a new node with `body`, under `parent` (the doc root id
/// or another Add's id), in document `doc`.
pub fn op_add(
    doc: &ObjectId,
    parent: &ObjectId,
    body: &[u8],
    timestamp_ms: u64,
    sequence: u64,
    human: &Did,
    device: &Controller,
) -> Result<Object> {
    if body.len() > MAX_BODY_BYTES {
        return Err(CrdtError::BodyTooLarge);
    }
    let mut payload = Vec::with_capacity(1 + body.len());
    payload.push(OP_ADD);
    payload.extend_from_slice(body);
    Ok(ObjectBuilder::new(ObjectType::CRDT_OP)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .link("doc", doc.clone())
        .link("parent", parent.clone())
        .payload(Payload::Public(payload))
        .sign(human, device)?)
}

/// Build an **Edit** op replacing the body of `node` (an Add's id).
pub fn op_edit(
    doc: &ObjectId,
    node: &ObjectId,
    body: &[u8],
    timestamp_ms: u64,
    sequence: u64,
    human: &Did,
    device: &Controller,
) -> Result<Object> {
    if body.len() > MAX_BODY_BYTES {
        return Err(CrdtError::BodyTooLarge);
    }
    let mut payload = Vec::with_capacity(1 + body.len());
    payload.push(OP_EDIT);
    payload.extend_from_slice(body);
    Ok(ObjectBuilder::new(ObjectType::CRDT_OP)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .link("doc", doc.clone())
        .link("node", node.clone())
        .payload(Payload::Public(payload))
        .sign(human, device)?)
}

/// Build a **Tombstone** op retracting `node`.
pub fn op_tombstone(
    doc: &ObjectId,
    node: &ObjectId,
    timestamp_ms: u64,
    sequence: u64,
    human: &Did,
    device: &Controller,
) -> Result<Object> {
    Ok(ObjectBuilder::new(ObjectType::CRDT_OP)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .link("doc", doc.clone())
        .link("node", node.clone())
        .payload(Payload::Public(vec![OP_TOMBSTONE]))
        .sign(human, device)?)
}

/// One live node of a replayed document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The node's id (its Add op's object id).
    pub id: ObjectId,
    /// The parent (doc root id or another node's id).
    pub parent: ObjectId,
    /// The authoring human.
    pub author: Did,
    /// Current body (latest valid Edit wins; otherwise the Add body).
    pub body: Vec<u8>,
    /// Author-claimed creation time (display hint).
    pub timestamp_ms: u64,
    /// Whether the node has been retracted by its author.
    pub tombstoned: bool,
}

/// The replayed state of one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocState {
    /// The document root this state belongs to.
    pub doc: ObjectId,
    /// All attached nodes, keyed by node id.
    nodes: BTreeMap<String, Node>,
    /// Ops that were structurally valid but whose parent is not (yet) attached.
    pub pending: Vec<ObjectId>,
    /// Ops deterministically excluded (wrong doc, bad shape, wrong author…).
    pub rejected: Vec<ObjectId>,
}

impl DocState {
    /// A node by id, if attached.
    pub fn node(&self, id: &ObjectId) -> Option<&Node> {
        self.nodes.get(id.as_str())
    }

    /// Live (non-tombstoned) children of `parent`, in deterministic display
    /// order: `(timestamp_ms, id)`.
    pub fn children(&self, parent: &ObjectId) -> Vec<&Node> {
        let mut out: Vec<&Node> = self
            .nodes
            .values()
            .filter(|n| n.parent == *parent && !n.tombstoned)
            .collect();
        out.sort_by(|a, b| {
            a.timestamp_ms
                .cmp(&b.timestamp_ms)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        out
    }

    /// Total attached nodes (including tombstoned).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether no nodes are attached.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Parsed view of one op.
struct ParsedOp<'a> {
    obj: &'a Object,
    kind: u8,
    target: ObjectId, // Add: parent · Edit/Tombstone: node
    body: Vec<u8>,
}

fn parse_op<'a>(doc: &ObjectId, obj: &'a Object) -> Option<ParsedOp<'a>> {
    if obj.object_type != ObjectType::CRDT_OP {
        return None;
    }
    // Must name this document.
    let doc_link = obj.links.iter().find(|l| l.rel == "doc")?;
    if doc_link.target != *doc {
        return None;
    }
    let bytes = match &obj.payload {
        Payload::Public(b) if !b.is_empty() => b,
        _ => return None,
    };
    let kind = bytes[0];
    let body = bytes[1..].to_vec();
    let target_rel = match kind {
        OP_ADD => "parent",
        OP_EDIT | OP_TOMBSTONE => "node",
        _ => return None,
    };
    if kind == OP_TOMBSTONE && !body.is_empty() {
        return None;
    }
    if body.len() > MAX_BODY_BYTES {
        return None;
    }
    let target = obj
        .links
        .iter()
        .find(|l| l.rel == target_rel)?
        .target
        .clone();
    Some(ParsedOp {
        obj,
        kind,
        target,
        body,
    })
}

/// Replay a set of ops into the state of document `doc`.
///
/// Pure and order-independent: any permutation of `ops` yields the identical
/// `DocState`. Duplicate ops (same id) are harmless. Signature/provenance
/// verification happens at ingest (before ops reach a store), as everywhere.
pub fn replay(doc: &ObjectId, ops: &[Object]) -> DocState {
    // Deduplicate by op id; classify.
    let mut adds: BTreeMap<String, ParsedOp> = BTreeMap::new();
    let mut edits: Vec<ParsedOp> = Vec::new();
    let mut tombs: Vec<ParsedOp> = Vec::new();
    let mut rejected: Vec<ObjectId> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();

    for obj in ops {
        if seen.contains(&obj.id().as_str()) {
            continue;
        }
        seen.push(obj.id().as_str());
        match parse_op(doc, obj) {
            None => rejected.push(obj.id().clone()),
            Some(p) => match p.kind {
                OP_ADD => {
                    adds.insert(obj.id().as_str().to_string(), p);
                }
                OP_EDIT => edits.push(p),
                _ => tombs.push(p),
            },
        }
    }

    // Attach adds: a node attaches if its parent is the doc root or an attached
    // add. Iterate to fixpoint (bounded by node count) so arrival order of the
    // input never matters.
    let mut attached: BTreeMap<String, Node> = BTreeMap::new();
    loop {
        let mut progressed = false;
        for (id, p) in &adds {
            if attached.contains_key(id) {
                continue;
            }
            let parent_ok = p.target == *doc || attached.contains_key(p.target.as_str());
            if parent_ok {
                attached.insert(
                    id.clone(),
                    Node {
                        id: p.obj.id().clone(),
                        parent: p.target.clone(),
                        author: p.obj.author_human.clone(),
                        body: p.body.clone(),
                        timestamp_ms: p.obj.timestamp_ms,
                        tombstoned: false,
                    },
                );
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    let pending: Vec<ObjectId> = adds
        .iter()
        .filter(|(id, _)| !attached.contains_key(*id))
        .map(|(_, p)| p.obj.id().clone())
        .collect();

    // Edits: only the node's human may edit; per-node LWW by (sequence, op id).
    let mut best_edit: BTreeMap<String, (u64, String, Vec<u8>)> = BTreeMap::new();
    for e in &edits {
        let node = match attached.get(e.target.as_str()) {
            Some(n) => n,
            None => {
                rejected.push(e.obj.id().clone());
                continue;
            }
        };
        if node.author.as_str() != e.obj.author_human.as_str() {
            rejected.push(e.obj.id().clone());
            continue;
        }
        let cand = (
            e.obj.sequence,
            e.obj.id().as_str().to_string(),
            e.body.clone(),
        );
        match best_edit.get(e.target.as_str()) {
            Some((s, i, _)) if (cand.0, cand.1.as_str()) <= (*s, i.as_str()) => {}
            _ => {
                best_edit.insert(e.target.as_str().to_string(), cand);
            }
        }
    }
    for (node_id, (_, _, body)) in best_edit {
        if let Some(n) = attached.get_mut(&node_id) {
            n.body = body;
        }
    }

    // Tombstones: only the node's human; monotone.
    for t in &tombs {
        match attached.get_mut(t.target.as_str()) {
            Some(n) if n.author.as_str() == t.obj.author_human.as_str() => {
                n.tombstoned = true;
            }
            _ => rejected.push(t.obj.id().clone()),
        }
    }

    DocState {
        doc: doc.clone(),
        nodes: attached,
        pending,
        rejected,
    }
}
