//! Bounded evidence selection for native release retrieval.
//!
//! A full object-store reconciliation is useful for replicas, but it is too
//! broad for a device that wants one release. This module selects the
//! content-addressed closure that the existing governed-release verifier may
//! need: the release's forward links, attestations, governance-chain evidence,
//! source tree, and artifact chunks.
//!
//! Selection is not trust. The serving peer's result is only a transfer plan;
//! the receiving peer still verifies object integrity/provenance through
//! `mini-sync` and then runs [`crate::verify_governed_release`]. A missing or
//! oversized closure fails closed rather than being silently shortened.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};

use mini_objects::{ObjectId, ObjectType};
use mini_store::{Backend, Store};

use crate::{
    ForgeError, Result, APPROVE_TYPE, ATTEST_TYPE, CHAIN_TYPE, EXECUTABLE_FILE_TYPE, FILE_TYPE,
    PROJECT_TYPE, PR_TYPE, TREE_TYPE,
};

/// Maximum number of objects in one release retrieval closure.
///
/// This is deliberately lower than the general governance chain walk limit:
/// retrieval is a one-shot network operation and must not turn a peer's
/// historical store into an unbounded response. A long-lived project can use
/// ordinary `mini sync` or a later paginated retrieval lane.
pub const MAX_RELEASE_RETRIEVAL_OBJECTS: usize = 4096;

/// Compute the release closure from objects a peer actually sent, using the
/// same selector as the serving side. The receiver uses this to refuse a
/// responder-advertised closure that contains anything the release does not
/// reach. The objects are consumed into an in-memory index rather than copied
/// into a second store. Fails closed if any referenced object is absent or an
/// object does not match its id.
pub fn release_closure_from_objects(
    objects: Vec<mini_objects::Object>,
    release_id: &ObjectId,
) -> Result<Vec<ObjectId>> {
    let mut by_id: HashMap<ObjectId, mini_objects::Object> = HashMap::with_capacity(objects.len());
    let mut incoming: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    for object in objects {
        object
            .verify_integrity(object.id())
            .map_err(|_| ForgeError::BadObject)?;
        for link in &object.links {
            incoming
                .entry(link.target.clone())
                .or_default()
                .push(object.id().clone());
        }
        by_id.insert(object.id().clone(), object);
    }
    select_closure(&Received { by_id, incoming }, release_id)
}

/// Where the selector reads objects and the reverse-link index from: a real
/// store when serving, or the objects a peer sent when checking.
trait ObjectSource {
    fn object(&self, id: &ObjectId) -> Result<Cow<'_, mini_objects::Object>>;
    fn linking(&self, id: &ObjectId) -> Result<Vec<ObjectId>>;
}

impl<B: Backend> ObjectSource for Store<B> {
    fn object(&self, id: &ObjectId) -> Result<Cow<'_, mini_objects::Object>> {
        Ok(Cow::Owned(self.get(id)?))
    }
    fn linking(&self, id: &ObjectId) -> Result<Vec<ObjectId>> {
        Ok(self.linking_to(id)?)
    }
}

struct Received {
    by_id: HashMap<ObjectId, mini_objects::Object>,
    incoming: HashMap<ObjectId, Vec<ObjectId>>,
}

impl ObjectSource for Received {
    fn object(&self, id: &ObjectId) -> Result<Cow<'_, mini_objects::Object>> {
        self.by_id
            .get(id)
            .map(Cow::Borrowed)
            .ok_or(ForgeError::BadObject)
    }
    fn linking(&self, id: &ObjectId) -> Result<Vec<ObjectId>> {
        Ok(self.incoming.get(id).cloned().unwrap_or_default())
    }
}

/// Kind of a typed edge target, matched against the stored object's type.
#[derive(Clone, Copy)]
enum Kind {
    Release,
    Commit,
    Manifest,
    Named(&'static str),
}

impl Kind {
    fn matches(self, ty: &ObjectType) -> bool {
        match self {
            Kind::Release => *ty == ObjectType::RELEASE,
            Kind::Commit => *ty == ObjectType::COMMIT,
            Kind::Manifest => *ty == ObjectType::MEDIA_MANIFEST,
            Kind::Named(n) => matches!(ty, ObjectType::Custom(name) if name == n),
        }
    }
}

/// How many of a relation's links are followed. The verifiers disagree on this
/// by relation, and the selector mirrors each: `Single` relations that repeat
/// are malformed and skipped; `First` follows only the first link (what the
/// verifier or resolver reads); `Many` follows every link.
#[derive(Clone, Copy)]
enum Mode {
    Single,
    First,
    Many,
}

/// One typed forward edge: relation name, the object types the target may be,
/// and how its links are followed.
struct Forward {
    rel: &'static str,
    kinds: &'static [Kind],
    mode: Mode,
}

const PROJECT: Kind = Kind::Named(PROJECT_TYPE);
const PR: Kind = Kind::Named(PR_TYPE);
const CHAIN: Kind = Kind::Named(CHAIN_TYPE);
const TREE: Kind = Kind::Named(TREE_TYPE);

fn owner_is(ty: &ObjectType, kind: Kind) -> bool {
    kind.matches(ty)
}

const fn one(rel: &'static str, kinds: &'static [Kind]) -> Forward {
    Forward {
        rel,
        kinds,
        mode: Mode::Single,
    }
}
const fn first(rel: &'static str, kinds: &'static [Kind]) -> Forward {
    Forward {
        rel,
        kinds,
        mode: Mode::First,
    }
}
const fn many(rel: &'static str, kinds: &'static [Kind]) -> Forward {
    Forward {
        rel,
        kinds,
        mode: Mode::Many,
    }
}

// Release verification reads the first `project`, `commit` and `artifact`
// link, so only those are followed.
const RELEASE_EDGES: &[Forward] = &[
    first("project", &[PROJECT]),
    first("commit", &[Kind::Commit]),
    first("artifact", &[Kind::Manifest]),
];
// Only the tree is needed: release verification validates a commit's `parent`
// ids structurally but never dereferences them, so parents are not offered.
const COMMIT_EDGES: &[Forward] = &[one("tree", &[TREE])];
// `part` belongs to superblocks; a manifest's verifier reads only `chunk`.
const MANIFEST_EDGES: &[Forward] = &[many("chunk", &[Kind::Named("mini/chunk")])];
const TREE_ENTRY_KINDS: &[Kind] = &[
    TREE,
    Kind::Named(FILE_TYPE),
    Kind::Named(EXECUTABLE_FILE_TYPE),
];
// The verifier and resolver read a PR's first `project`/`head`/`base` link.
const PR_EDGES: &[Forward] = &[
    first("project", &[PROJECT]),
    first("head", &[Kind::Commit]),
    first("base", &[Kind::Commit]),
];
// The resolver reads a chain entry's `project` and `prev` with any-link
// semantics (so a resolver-valid entry is never omitted) but consumes only the
// first `pr` link, so a second PR is never traversed. Targets stay typed and
// project-scoped.
const CHAIN_EDGES: &[Forward] = &[
    many("project", &[PROJECT]),
    many("prev", &[CHAIN, PROJECT]),
    first("pr", &[PR]),
];
// Approvals are terminal evidence, like attestations: they are only ever
// reached from the PR or chain entry they approve, which is already selected.
const APPROVE_EDGES: &[Forward] = &[];
// An attestation is terminal evidence: its `release` link is not traversed, or
// an attestation reached for release A could pull in release B.
const ATTEST_EDGES: &[Forward] = &[];

/// Forward edges the governed-release verifier reads for this object type.
fn forward_edges(ty: &ObjectType) -> &'static [Forward] {
    if owner_is(ty, Kind::Release) {
        RELEASE_EDGES
    } else if owner_is(ty, Kind::Commit) {
        COMMIT_EDGES
    } else if owner_is(ty, Kind::Manifest) {
        MANIFEST_EDGES
    } else if owner_is(ty, PR) {
        PR_EDGES
    } else if owner_is(ty, CHAIN) {
        CHAIN_EDGES
    } else if owner_is(ty, Kind::Named(APPROVE_TYPE)) {
        APPROVE_EDGES
    } else if owner_is(ty, Kind::Named(ATTEST_TYPE)) {
        ATTEST_EDGES
    } else {
        &[]
    }
}

/// One typed reverse edge: an object of `owner` kind may be reached from
/// objects of `candidate` kind that link to it. The governed verifiers count
/// any stored attestation or approval that links to the target (they do not
/// check the relation name), so those are admitted by type alone; the
/// resolver does require a chain entry's `prev` link, so chain candidates
/// must link the owner under `prev`. The candidate's own forward edges stay
/// typed and project-scoped, and attestations and approvals are terminal.
struct Reverse {
    owner: Kind,
    candidate: Kind,
    rel: Option<&'static str>,
}

const REVERSE_EDGES: &[Reverse] = &[
    Reverse {
        owner: Kind::Release,
        candidate: Kind::Named(ATTEST_TYPE),
        rel: None,
    },
    Reverse {
        owner: PROJECT,
        candidate: CHAIN,
        rel: Some("prev"),
    },
    Reverse {
        owner: CHAIN,
        candidate: CHAIN,
        rel: Some("prev"),
    },
    // Amendment quorum: approvals linking a chain entry.
    Reverse {
        owner: CHAIN,
        candidate: Kind::Named(APPROVE_TYPE),
        rel: None,
    },
    Reverse {
        owner: PR,
        candidate: Kind::Named(APPROVE_TYPE),
        rel: None,
    },
];

/// Select the bounded object closure needed to verify `release_id` on another
/// peer.
///
/// Every edge is typed: a forward link is followed only under a relation the
/// governed-release verifier reads *and* only when its target is an object of
/// the type that relation means; a reverse (indexed) candidate is admitted
/// only as the specific candidate type for the specific relation that reached
/// it. Single-valued relations that repeat are malformed and not followed.
/// Governance objects (project, PR, chain entry) must belong to the release's
/// own project. The returned ids are sorted for deterministic framing.
///
/// This function does not decide whether a release is valid and does not
/// grant any authority. It only answers which already-stored objects a serving
/// peer may offer for the exact requested release. A crafted object that
/// passes these structural checks (right type, right relation, right project)
/// can still be offered; it is then rejected by the receiver's governed
/// verification. That bounded disclosure is the residual.
pub fn release_retrieval_ids<B: Backend>(
    store: &Store<B>,
    release_id: &ObjectId,
) -> Result<Vec<ObjectId>> {
    select_closure(store, release_id)
}

fn select_closure<S: ObjectSource>(store: &S, release_id: &ObjectId) -> Result<Vec<ObjectId>> {
    let release = store.object(release_id)?;
    if release.object_type != ObjectType::RELEASE {
        return Err(ForgeError::BadObject);
    }
    let project_id = release
        .links
        .iter()
        .find(|l| l.rel == "project")
        .map(|l| l.target.clone())
        .ok_or(ForgeError::BadObject)?;

    let mut selected = Vec::new();
    let mut pending = VecDeque::from([release_id.clone()]);
    let mut seen: HashSet<ObjectId> = HashSet::from([release_id.clone()]);

    while let Some(id) = pending.pop_front() {
        let object = store.object(&id)?;
        selected.push(id.clone());

        // A tree's children are the targets its canonical payload names (what
        // `checkout` reads), not whatever its `entry` links claim.
        if TREE.matches(&object.object_type) {
            for entry in crate::read_tree(&object)? {
                let target_object = store.object(&entry.target)?;
                if TREE_ENTRY_KINDS
                    .iter()
                    .any(|k| k.matches(&target_object.object_type))
                {
                    enqueue(&mut pending, &mut seen, &entry.target)?;
                }
            }
        }

        for edge in forward_edges(&object.object_type) {
            let mut targets: Vec<&ObjectId> = object
                .links
                .iter()
                .filter(|link| link.rel == edge.rel)
                .map(|link| &link.target)
                .collect();
            match edge.mode {
                Mode::Single if targets.len() > 1 => continue,
                Mode::First => targets.truncate(1),
                _ => {}
            }
            for target in targets {
                let target_object = store.object(target)?;
                if edge
                    .kinds
                    .iter()
                    .any(|k| k.matches(&target_object.object_type))
                    && in_project(&target_object, target, &project_id)
                {
                    enqueue(&mut pending, &mut seen, target)?;
                }
            }
        }

        for edge in REVERSE_EDGES
            .iter()
            .filter(|e| e.owner.matches(&object.object_type))
        {
            for candidate_id in store.linking(&id)? {
                let candidate = store.object(&candidate_id)?;
                let links_owner = candidate
                    .links
                    .iter()
                    .any(|l| l.target == id && edge.rel.is_none_or(|rel| l.rel == rel));
                if !edge.candidate.matches(&candidate.object_type)
                    || !links_owner
                    || !in_project(&candidate, &candidate_id, &project_id)
                {
                    continue;
                }
                enqueue(&mut pending, &mut seen, &candidate_id)?;
            }
        }
    }

    selected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(selected)
}

/// Governance objects must belong to the release's own project, judged the
/// way the resolver judges them: a chain entry has *a* `project` link to it,
/// a PR's first `project` link is it. Other kinds carry no project link and
/// are bounded by their typed edge instead.
fn in_project(object: &mini_objects::Object, id: &ObjectId, project: &ObjectId) -> bool {
    let ty = &object.object_type;
    if PROJECT.matches(ty) {
        return id == project;
    }
    if CHAIN.matches(ty) {
        return object
            .links
            .iter()
            .any(|l| l.rel == "project" && l.target == *project);
    }
    if PR.matches(ty) {
        return object
            .links
            .iter()
            .find(|l| l.rel == "project")
            .map(|l| &l.target)
            == Some(project);
    }
    true
}

/// Queue `id` once. The closure limit is enforced here, over everything
/// selected or pending, so a hostile fan-out stops at the bound instead of
/// being materialized first.
fn enqueue(
    pending: &mut VecDeque<ObjectId>,
    seen: &mut HashSet<ObjectId>,
    id: &ObjectId,
) -> Result<()> {
    if seen.contains(id) {
        return Ok(());
    }
    if seen.len() >= MAX_RELEASE_RETRIEVAL_OBJECTS {
        return Err(ForgeError::FieldTooLarge);
    }
    seen.insert(id.clone());
    pending.push_back(id.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use did_mini::{Capabilities, Controller};
    use mini_objects::{ObjectBuilder, Payload};
    use mini_store::{MemoryBackend, Store};

    use super::*;

    fn identity(seed: u8) -> (Controller, Controller) {
        let mut root =
            Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed.wrapping_add(2); 32],
            &[seed.wrapping_add(3); 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    fn object(
        object_type: ObjectType,
        human: &did_mini::Did,
        device: &Controller,
        links: &[(&str, &ObjectId)],
    ) -> mini_objects::Object {
        let mut builder = ObjectBuilder::new(object_type).payload(Payload::Public(Vec::new()));
        for (rel, target) in links {
            builder = builder.link(rel, (*target).clone());
        }
        builder.sign(human, device).unwrap()
    }

    #[test]
    fn closure_includes_forward_and_verifier_reverse_edges_but_not_noise() {
        let (human, device) = identity(7);
        let mut store = Store::new(MemoryBackend::new());

        let project = object(
            ObjectType::Custom(PROJECT_TYPE.to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&project).unwrap();
        let tree_id = crate::put_tree(&mut store, &human.did(), &device, &[]).unwrap();
        let tree = store.get(&tree_id).unwrap();
        let commit = object(
            ObjectType::COMMIT,
            &human.did(),
            &device,
            &[("tree", tree.id())],
        );
        store.insert(&commit).unwrap();
        let chunk = object(
            ObjectType::Custom("mini/chunk".to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&chunk).unwrap();
        let manifest = object(
            ObjectType::MEDIA_MANIFEST,
            &human.did(),
            &device,
            &[("chunk", chunk.id())],
        );
        store.insert(&manifest).unwrap();
        let pr = object(
            ObjectType::Custom(PR_TYPE.to_string()),
            &human.did(),
            &device,
            &[
                ("project", project.id()),
                ("head", commit.id()),
                ("base", commit.id()),
            ],
        );
        store.insert(&pr).unwrap();
        let chain = object(
            ObjectType::Custom(CHAIN_TYPE.to_string()),
            &human.did(),
            &device,
            &[
                ("project", project.id()),
                ("prev", project.id()),
                ("pr", pr.id()),
            ],
        );
        store.insert(&chain).unwrap();
        let approval = object(
            ObjectType::Custom("mini/approve".to_string()),
            &human.did(),
            &device,
            &[("pr", pr.id())],
        );
        store.insert(&approval).unwrap();
        let attestation = object(
            ObjectType::Custom("mini/attest".to_string()),
            &human.did(),
            &device,
            &[("release", project.id())],
        );
        // This is intentionally not a valid release attestation target. It
        // should be excluded because the release itself is the seed below.
        store.insert(&attestation).unwrap();
        let release = object(
            ObjectType::RELEASE,
            &human.did(),
            &device,
            &[
                ("project", project.id()),
                ("commit", commit.id()),
                ("artifact", manifest.id()),
            ],
        );
        store.insert(&release).unwrap();
        let real_attestation = object(
            ObjectType::Custom("mini/attest".to_string()),
            &human.did(),
            &device,
            &[("release", release.id())],
        );
        store.insert(&real_attestation).unwrap();
        let noise = object(
            ObjectType::Custom("mini/comment".to_string()),
            &human.did(),
            &device,
            &[("topic", release.id())],
        );
        store.insert(&noise).unwrap();

        let ids = release_retrieval_ids(&store, release.id()).unwrap();
        for required in [
            release.id(),
            project.id(),
            commit.id(),
            tree.id(),
            manifest.id(),
            chunk.id(),
            pr.id(),
            chain.id(),
            approval.id(),
            real_attestation.id(),
        ] {
            assert!(ids.contains(required), "missing {}", required.as_str());
        }
        assert!(!ids.contains(noise.id()));
        assert!(!ids.contains(&attestation.id().clone()));
        assert!(ids.windows(2).all(|w| w[0].as_str() < w[1].as_str()));
    }

    #[test]
    fn approval_extra_reviewed_link_does_not_widen_closure() {
        let (human, device) = identity(9);
        let mut store = Store::new(MemoryBackend::new());
        let project = object(
            ObjectType::Custom(PROJECT_TYPE.to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&project).unwrap();
        let unrelated = object(
            ObjectType::Custom("mini/comment".to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&unrelated).unwrap();
        let pr = object(
            ObjectType::Custom(PR_TYPE.to_string()),
            &human.did(),
            &device,
            &[("project", project.id())],
        );
        store.insert(&pr).unwrap();
        let approval = object(
            ObjectType::Custom(APPROVE_TYPE.to_string()),
            &human.did(),
            &device,
            &[("pr", pr.id()), ("reviewed", unrelated.id())],
        );
        store.insert(&approval).unwrap();
        let release = object(
            ObjectType::RELEASE,
            &human.did(),
            &device,
            &[("project", project.id())],
        );
        store.insert(&release).unwrap();
        let ids = release_retrieval_ids(&store, release.id()).unwrap();
        assert!(!ids.contains(unrelated.id()));
    }

    #[test]
    fn forward_targets_must_be_the_type_the_relation_means() {
        let (human, device) = identity(11);
        let mut store = Store::new(MemoryBackend::new());
        let project = object(
            ObjectType::Custom(PROJECT_TYPE.to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&project).unwrap();
        let post = object(ObjectType::POST, &human.did(), &device, &[]);
        store.insert(&post).unwrap();
        let release = object(
            ObjectType::RELEASE,
            &human.did(),
            &device,
            &[("project", project.id()), ("artifact", post.id())],
        );
        store.insert(&release).unwrap();
        let ids = release_retrieval_ids(&store, release.id()).unwrap();
        assert!(!ids.contains(post.id()), "wrong-type artifact disclosed");
        assert!(ids.contains(project.id()));
    }

    #[test]
    fn reverse_candidates_are_admitted_by_type_but_their_forward_edges_stay_scoped() {
        let (human, device) = identity(13);
        let mut store = Store::new(MemoryBackend::new());
        let project = object(
            ObjectType::Custom(PROJECT_TYPE.to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&project).unwrap();
        let other_project = object(
            ObjectType::Custom(PROJECT_TYPE.to_string()),
            &human.did(),
            &device,
            &[("note", project.id())],
        );
        store.insert(&other_project).unwrap();
        let pr = object(
            ObjectType::Custom(PR_TYPE.to_string()),
            &human.did(),
            &device,
            &[("project", project.id())],
        );
        store.insert(&pr).unwrap();
        let chain = object(
            ObjectType::Custom(CHAIN_TYPE.to_string()),
            &human.did(),
            &device,
            &[
                ("project", project.id()),
                ("prev", project.id()),
                ("pr", pr.id()),
            ],
        );
        store.insert(&chain).unwrap();
        // Unrelated PR in a different project.
        let foreign_pr = object(
            ObjectType::Custom(PR_TYPE.to_string()),
            &human.did(),
            &device,
            &[("project", other_project.id())],
        );
        store.insert(&foreign_pr).unwrap();
        // Approval reaching the chain via `prev` while pointing `pr` elsewhere.
        let crafted = object(
            ObjectType::Custom(APPROVE_TYPE.to_string()),
            &human.did(),
            &device,
            &[("prev", chain.id()), ("pr", foreign_pr.id())],
        );
        store.insert(&crafted).unwrap();
        // Approval repeating the single-valued `pr` relation.
        let doubled = object(
            ObjectType::Custom(APPROVE_TYPE.to_string()),
            &human.did(),
            &device,
            &[("pr", pr.id()), ("pr", foreign_pr.id())],
        );
        store.insert(&doubled).unwrap();
        let release = object(
            ObjectType::RELEASE,
            &human.did(),
            &device,
            &[("project", project.id())],
        );
        store.insert(&release).unwrap();
        let ids = release_retrieval_ids(&store, release.id()).unwrap();
        // The verifiers count any approval linking to the target, so those
        // approvals are offered; what they point at elsewhere is not.
        assert!(ids.contains(crafted.id()) && ids.contains(doubled.id()));
        for excluded in [foreign_pr.id(), other_project.id()] {
            assert!(!ids.contains(excluded), "{} admitted", excluded.as_str());
        }
        assert!(ids.contains(chain.id()) && ids.contains(pr.id()));
    }

    #[test]
    fn amendment_approvals_and_payload_tree_targets_are_in_the_closure() {
        let (human, device) = identity(15);
        let mut store = Store::new(MemoryBackend::new());
        let project = object(
            ObjectType::Custom(PROJECT_TYPE.to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&project).unwrap();
        // Amendment-style chain entry with an approval targeting it directly.
        let amend = object(
            ObjectType::Custom(CHAIN_TYPE.to_string()),
            &human.did(),
            &device,
            &[("project", project.id()), ("prev", project.id())],
        );
        store.insert(&amend).unwrap();
        let quorum = object(
            ObjectType::Custom(APPROVE_TYPE.to_string()),
            &human.did(),
            &device,
            &[("pr", amend.id())],
        );
        store.insert(&quorum).unwrap();

        // A tree whose canonical payload names file A but whose `entry` link
        // names file B: the payload target is what checkout reads.
        let file_a = object(
            ObjectType::Custom(FILE_TYPE.to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&file_a).unwrap();
        let file_b = ObjectBuilder::new(ObjectType::Custom(FILE_TYPE.to_string()))
            .payload(Payload::Public(vec![1]))
            .sign(&human.did(), &device)
            .unwrap();
        store.insert(&file_b).unwrap();
        let mut payload = 1u32.to_be_bytes().to_vec();
        payload.push(0);
        payload.extend_from_slice(&1u32.to_be_bytes());
        payload.push(b'a');
        payload.extend_from_slice(&(file_a.id().as_str().len() as u32).to_be_bytes());
        payload.extend_from_slice(file_a.id().as_str().as_bytes());
        let tree = ObjectBuilder::new(ObjectType::Custom(TREE_TYPE.to_string()))
            .link("entry", file_b.id().clone())
            .payload(Payload::Public(payload))
            .sign(&human.did(), &device)
            .unwrap();
        store.insert(&tree).unwrap();
        let commit = object(
            ObjectType::COMMIT,
            &human.did(),
            &device,
            &[("tree", tree.id())],
        );
        store.insert(&commit).unwrap();
        let release = object(
            ObjectType::RELEASE,
            &human.did(),
            &device,
            &[("project", project.id()), ("commit", commit.id())],
        );
        store.insert(&release).unwrap();

        let ids = release_retrieval_ids(&store, release.id()).unwrap();
        assert!(ids.contains(quorum.id()), "amendment approval missing");
        assert!(ids.contains(file_a.id()), "payload tree target missing");
        assert!(!ids.contains(file_b.id()), "link-only tree target offered");
    }

    #[test]
    fn manifest_part_links_and_odd_relation_names_follow_the_verifier() {
        let (human, device) = identity(17);
        let mut store = Store::new(MemoryBackend::new());
        let project = object(
            ObjectType::Custom(PROJECT_TYPE.to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&project).unwrap();
        let chunk = object(
            ObjectType::Custom("mini/chunk".to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&chunk).unwrap();
        let stray = ObjectBuilder::new(ObjectType::Custom("mini/chunk".to_string()))
            .payload(Payload::Public(vec![9]))
            .sign(&human.did(), &device)
            .unwrap();
        store.insert(&stray).unwrap();
        let manifest = object(
            ObjectType::MEDIA_MANIFEST,
            &human.did(),
            &device,
            &[("chunk", chunk.id()), ("part", stray.id())],
        );
        store.insert(&manifest).unwrap();
        let release = object(
            ObjectType::RELEASE,
            &human.did(),
            &device,
            &[("project", project.id()), ("artifact", manifest.id())],
        );
        store.insert(&release).unwrap();
        // An attestation that links the release under a non-conventional
        // relation is still counted by the verifier, so it is offered.
        let odd = object(
            ObjectType::Custom(ATTEST_TYPE.to_string()),
            &human.did(),
            &device,
            &[("attests", release.id())],
        );
        store.insert(&odd).unwrap();
        let ids = release_retrieval_ids(&store, release.id()).unwrap();
        assert!(ids.contains(chunk.id()));
        assert!(!ids.contains(stray.id()), "manifest part link followed");
        assert!(ids.contains(odd.id()), "odd-relation attestation missing");
    }

    #[test]
    fn closure_from_received_objects_matches_and_rejects_unrelated_extras() {
        let (human, device) = identity(19);
        let mut store = Store::new(MemoryBackend::new());
        let project = object(
            ObjectType::Custom(PROJECT_TYPE.to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&project).unwrap();
        let release = object(
            ObjectType::RELEASE,
            &human.did(),
            &device,
            &[("project", project.id())],
        );
        store.insert(&release).unwrap();
        let unrelated = ObjectBuilder::new(ObjectType::POST)
            .payload(Payload::Public(vec![5]))
            .sign(&human.did(), &device)
            .unwrap();
        let derived = release_closure_from_objects(
            vec![project.clone(), release.clone(), unrelated.clone()],
            release.id(),
        )
        .unwrap();
        assert!(derived.contains(release.id()) && derived.contains(project.id()));
        assert!(!derived.contains(unrelated.id()));
        // A missing referenced object fails closed.
        assert!(release_closure_from_objects(vec![release.clone()], release.id()).is_err());
    }

    #[test]
    fn a_resolver_valid_chain_entry_with_a_repeated_project_link_is_offered() {
        let (human, device) = identity(21);
        let mut store = Store::new(MemoryBackend::new());
        let project = object(
            ObjectType::Custom(PROJECT_TYPE.to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&project).unwrap();
        let pr = object(
            ObjectType::Custom(PR_TYPE.to_string()),
            &human.did(),
            &device,
            &[("project", project.id())],
        );
        store.insert(&pr).unwrap();
        // The resolver accepts this entry (it only needs *a* project link).
        let fork = object(
            ObjectType::Custom(CHAIN_TYPE.to_string()),
            &human.did(),
            &device,
            &[
                ("project", project.id()),
                ("project", project.id()),
                ("prev", project.id()),
                ("pr", pr.id()),
            ],
        );
        store.insert(&fork).unwrap();
        let release = object(
            ObjectType::RELEASE,
            &human.did(),
            &device,
            &[("project", project.id())],
        );
        store.insert(&release).unwrap();
        let ids = release_retrieval_ids(&store, release.id()).unwrap();
        assert!(ids.contains(fork.id()), "resolver-valid entry omitted");
        assert!(ids.contains(pr.id()));
    }

    #[test]
    fn first_link_semantics_and_terminal_attestations_match_the_verifiers() {
        let (human, device) = identity(23);
        let mut store = Store::new(MemoryBackend::new());
        let project = object(
            ObjectType::Custom(PROJECT_TYPE.to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&project).unwrap();
        let tree_id = crate::put_tree(&mut store, &human.did(), &device, &[]).unwrap();
        let commit = object(
            ObjectType::COMMIT,
            &human.did(),
            &device,
            &[("tree", &tree_id)],
        );
        store.insert(&commit).unwrap();
        let other_commit = ObjectBuilder::new(ObjectType::COMMIT)
            .link("tree", tree_id.clone())
            .payload(Payload::Public(vec![7]))
            .sign(&human.did(), &device)
            .unwrap();
        store.insert(&other_commit).unwrap();
        // The verifier reads the first `commit` link; the second is ignored.
        let release = object(
            ObjectType::RELEASE,
            &human.did(),
            &device,
            &[
                ("project", project.id()),
                ("project", project.id()),
                ("commit", commit.id()),
                ("commit", other_commit.id()),
            ],
        );
        store.insert(&release).unwrap();
        // A second release, and an attestation of the first that also carries a
        // `release` link to the second.
        let second = object(
            ObjectType::RELEASE,
            &human.did(),
            &device,
            &[("project", project.id()), ("commit", other_commit.id())],
        );
        store.insert(&second).unwrap();
        let attest = object(
            ObjectType::Custom(ATTEST_TYPE.to_string()),
            &human.did(),
            &device,
            &[("attests", release.id()), ("release", second.id())],
        );
        store.insert(&attest).unwrap();
        // A merge entry with two `pr` links: the resolver consumes the first.
        let pr = object(
            ObjectType::Custom(PR_TYPE.to_string()),
            &human.did(),
            &device,
            &[("project", project.id())],
        );
        store.insert(&pr).unwrap();
        let extra_pr = ObjectBuilder::new(ObjectType::Custom(PR_TYPE.to_string()))
            .link("project", project.id().clone())
            .payload(Payload::Public(vec![3]))
            .sign(&human.did(), &device)
            .unwrap();
        store.insert(&extra_pr).unwrap();
        let chain = object(
            ObjectType::Custom(CHAIN_TYPE.to_string()),
            &human.did(),
            &device,
            &[
                ("project", project.id()),
                ("prev", project.id()),
                ("pr", pr.id()),
                ("pr", extra_pr.id()),
            ],
        );
        store.insert(&chain).unwrap();

        let ids = release_retrieval_ids(&store, release.id()).unwrap();
        assert!(ids.contains(commit.id()), "first commit link dropped");
        assert!(!ids.contains(other_commit.id()), "second commit followed");
        assert!(ids.contains(attest.id()), "attestation not offered");
        assert!(!ids.contains(second.id()), "attestation traversed to B");
        assert!(ids.contains(pr.id()), "first pr missing");
        assert!(!ids.contains(extra_pr.id()), "second pr followed");
    }

    #[test]
    fn verifier_unused_edges_and_non_resolver_chain_candidates_are_not_offered() {
        let (human, device) = identity(25);
        let mut store = Store::new(MemoryBackend::new());
        let project = object(
            ObjectType::Custom(PROJECT_TYPE.to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&project).unwrap();
        let tree_id = crate::put_tree(&mut store, &human.did(), &device, &[]).unwrap();
        // A commit's `parent` is validated structurally but never dereferenced.
        let parent = ObjectBuilder::new(ObjectType::COMMIT)
            .link("tree", tree_id.clone())
            .payload(Payload::Public(vec![1]))
            .sign(&human.did(), &device)
            .unwrap();
        store.insert(&parent).unwrap();
        let commit = ObjectBuilder::new(ObjectType::COMMIT)
            .link("tree", tree_id.clone())
            .link("parent", parent.id().clone())
            .payload(Payload::Public(vec![2]))
            .sign(&human.did(), &device)
            .unwrap();
        store.insert(&commit).unwrap();
        // A PR with a second `project`/`head`: only the first is read, and an
        // absent later target must not abort selection.
        let pr = ObjectBuilder::new(ObjectType::Custom(PR_TYPE.to_string()))
            .link("project", project.id().clone())
            .link("head", commit.id().clone())
            .link("head", parent.id().clone())
            .link("base", commit.id().clone())
            .payload(Payload::Public(vec![4]))
            .sign(&human.did(), &device)
            .unwrap();
        store.insert(&pr).unwrap();
        // A chain object with a `project` link but no `prev` link to the
        // project: the resolver requires `prev`, so it is not evidence.
        let loose_pr = ObjectBuilder::new(ObjectType::Custom(PR_TYPE.to_string()))
            .link("project", project.id().clone())
            .payload(Payload::Public(vec![5]))
            .sign(&human.did(), &device)
            .unwrap();
        store.insert(&loose_pr).unwrap();
        let loose_chain = object(
            ObjectType::Custom(CHAIN_TYPE.to_string()),
            &human.did(),
            &device,
            &[
                ("project", project.id()),
                ("next", project.id()),
                ("pr", loose_pr.id()),
            ],
        );
        store.insert(&loose_chain).unwrap();
        // An approval reached as reverse evidence is terminal.
        let other_pr = ObjectBuilder::new(ObjectType::Custom(PR_TYPE.to_string()))
            .link("project", project.id().clone())
            .payload(Payload::Public(vec![6]))
            .sign(&human.did(), &device)
            .unwrap();
        store.insert(&other_pr).unwrap();
        let approval = object(
            ObjectType::Custom(APPROVE_TYPE.to_string()),
            &human.did(),
            &device,
            &[("other", pr.id()), ("pr", other_pr.id())],
        );
        store.insert(&approval).unwrap();
        let chain = object(
            ObjectType::Custom(CHAIN_TYPE.to_string()),
            &human.did(),
            &device,
            &[
                ("project", project.id()),
                ("prev", project.id()),
                ("pr", pr.id()),
            ],
        );
        store.insert(&chain).unwrap();
        let release = object(
            ObjectType::RELEASE,
            &human.did(),
            &device,
            &[("project", project.id()), ("commit", commit.id())],
        );
        store.insert(&release).unwrap();

        let ids = release_retrieval_ids(&store, release.id()).unwrap();
        assert!(!ids.contains(parent.id()), "commit parent offered");
        assert!(ids.contains(pr.id()) && ids.contains(chain.id()));
        assert!(
            !ids.contains(loose_chain.id()),
            "chain without prev offered"
        );
        assert!(!ids.contains(loose_pr.id()), "pr of loose chain offered");
        assert!(ids.contains(approval.id()), "approval of the PR missing");
        assert!(
            !ids.contains(other_pr.id()),
            "approval traversed its pr link"
        );
    }
}
