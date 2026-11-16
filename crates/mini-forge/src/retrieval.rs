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

use std::collections::{HashSet, VecDeque};

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

/// One typed forward edge: relation name, the object types the target may be,
/// and whether the relation may repeat. A single-valued relation that appears
/// more than once is malformed and is not followed at all.
struct Forward {
    rel: &'static str,
    kinds: &'static [Kind],
    multi: bool,
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
        multi: false,
    }
}
const fn many(rel: &'static str, kinds: &'static [Kind]) -> Forward {
    Forward {
        rel,
        kinds,
        multi: true,
    }
}

const RELEASE_EDGES: &[Forward] = &[
    one("project", &[PROJECT]),
    one("commit", &[Kind::Commit]),
    one("artifact", &[Kind::Manifest]),
];
const COMMIT_EDGES: &[Forward] = &[one("tree", &[TREE]), many("parent", &[Kind::Commit])];
const MANIFEST_EDGES: &[Forward] = &[
    many("chunk", &[Kind::Named("mini/chunk")]),
    many(
        "part",
        &[Kind::Named("mini/superblock"), Kind::Named("mini/chunk")],
    ),
];
const TREE_ENTRY_KINDS: &[Kind] = &[
    TREE,
    Kind::Named(FILE_TYPE),
    Kind::Named(EXECUTABLE_FILE_TYPE),
];
const PR_EDGES: &[Forward] = &[
    one("project", &[PROJECT]),
    one("head", &[Kind::Commit]),
    one("base", &[Kind::Commit]),
];
const CHAIN_EDGES: &[Forward] = &[
    one("project", &[PROJECT]),
    one("prev", &[CHAIN, PROJECT]),
    one("pr", &[PR]),
];
const APPROVE_EDGES: &[Forward] = &[one("pr", &[PR])];
const ATTEST_EDGES: &[Forward] = &[one("release", &[Kind::Release])];

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
/// objects of `candidate` kind that link to it under `rel`.
struct Reverse {
    owner: Kind,
    candidate: Kind,
    rel: &'static str,
}

const REVERSE_EDGES: &[Reverse] = &[
    Reverse {
        owner: Kind::Release,
        candidate: Kind::Named(ATTEST_TYPE),
        rel: "release",
    },
    Reverse {
        owner: PROJECT,
        candidate: CHAIN,
        rel: "prev",
    },
    Reverse {
        owner: CHAIN,
        candidate: CHAIN,
        rel: "prev",
    },
    // Amendment quorum: approvals whose `pr` link targets a chain entry.
    Reverse {
        owner: CHAIN,
        candidate: Kind::Named(APPROVE_TYPE),
        rel: "pr",
    },
    Reverse {
        owner: PR,
        candidate: Kind::Named(APPROVE_TYPE),
        rel: "pr",
    },
    Reverse {
        owner: PR,
        candidate: CHAIN,
        rel: "pr",
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
    let release = store.get(release_id)?;
    if release.object_type != ObjectType::RELEASE {
        return Err(ForgeError::BadObject);
    }
    let project_id = single_link(&release, "project").ok_or(ForgeError::BadObject)?;

    let mut selected = Vec::new();
    let mut pending = VecDeque::from([release_id.clone()]);
    let mut seen: HashSet<ObjectId> = HashSet::from([release_id.clone()]);

    while let Some(id) = pending.pop_front() {
        let object = store.get(&id)?;
        selected.push(id.clone());

        // A tree's children are the targets its canonical payload names (what
        // `checkout` reads), not whatever its `entry` links claim.
        if TREE.matches(&object.object_type) {
            for entry in crate::read_tree(&object)? {
                let target_object = store.get(&entry.target)?;
                if TREE_ENTRY_KINDS
                    .iter()
                    .any(|k| k.matches(&target_object.object_type))
                {
                    enqueue(&mut pending, &mut seen, &entry.target)?;
                }
            }
        }

        for edge in forward_edges(&object.object_type) {
            let targets: Vec<&ObjectId> = object
                .links
                .iter()
                .filter(|link| link.rel == edge.rel)
                .map(|link| &link.target)
                .collect();
            if targets.len() > 1 && !edge.multi {
                continue;
            }
            for target in targets {
                let target_object = store.get(target)?;
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
            for candidate_id in store.linking_to(&id)? {
                let candidate = store.get(&candidate_id)?;
                if !edge.candidate.matches(&candidate.object_type)
                    || single_link(&candidate, edge.rel).as_ref() != Some(&id)
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

/// The single target of `rel`, or `None` when absent or repeated.
fn single_link(object: &mini_objects::Object, rel: &str) -> Option<ObjectId> {
    let mut it = object.links.iter().filter(|l| l.rel == rel);
    let first = it.next()?;
    if it.next().is_some() {
        return None;
    }
    Some(first.target.clone())
}

/// Governance objects must belong to the release's own project. Other kinds
/// carry no project link and are bounded by their typed edge instead.
fn in_project(object: &mini_objects::Object, id: &ObjectId, project: &ObjectId) -> bool {
    let ty = &object.object_type;
    if PROJECT.matches(ty) {
        return id == project;
    }
    if PR.matches(ty) || CHAIN.matches(ty) {
        return single_link(object, "project").as_ref() == Some(project);
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
    fn reverse_candidates_are_admitted_by_type_and_relation_together() {
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
        for excluded in [
            crafted.id(),
            doubled.id(),
            foreign_pr.id(),
            other_project.id(),
        ] {
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
}
