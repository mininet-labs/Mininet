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

use std::collections::VecDeque;

use mini_objects::{ObjectId, ObjectType};
use mini_store::{Backend, Store};

use crate::{ForgeError, Result, CHAIN_TYPE, PROJECT_TYPE, PR_TYPE};

/// Maximum number of objects in one release retrieval closure.
///
/// This is deliberately lower than the general governance chain walk limit:
/// retrieval is a one-shot network operation and must not turn a peer's
/// historical store into an unbounded response. A long-lived project can use
/// ordinary `mini sync` or a later paginated retrieval lane.
pub const MAX_RELEASE_RETRIEVAL_OBJECTS: usize = 4096;

/// Select the bounded object closure needed to verify `release_id` on another
/// peer.
///
/// Forward links are followed for every object. The reverse link index is
/// followed only for relations that the governed-release verifier reads:
/// attestations (`release`), governance successors (`prev`), and review
/// evidence (`pr`). The returned ids are sorted for deterministic framing.
///
/// This function does not decide whether a release is valid and does not
/// grant any authority. It only answers which already-stored objects a serving
/// peer may offer for the exact requested release.
pub fn release_retrieval_ids<B: Backend>(
    store: &Store<B>,
    release_id: &ObjectId,
) -> Result<Vec<ObjectId>> {
    let release = store.get(release_id)?;
    if release.object_type != ObjectType::RELEASE {
        return Err(ForgeError::BadObject);
    }

    let mut selected = Vec::new();
    let mut pending = VecDeque::from([release_id.clone()]);

    while let Some(id) = pending.pop_front() {
        if selected.iter().any(|existing| existing == &id) {
            continue;
        }
        if selected.len() >= MAX_RELEASE_RETRIEVAL_OBJECTS {
            return Err(ForgeError::FieldTooLarge);
        }

        let object = store.get(&id)?;
        selected.push(id.clone());

        // The content-addressed object graph carries repository and artifact
        // closure in its ordinary typed links. No release-specific wire
        // format is needed to walk these edges.
        for link in &object.links {
            enqueue(&mut pending, &selected, link.target.clone());
        }

        // Governance and attestations are indexed by reverse links because
        // their objects point *at* the release/chain/PR. Only the relation
        // names consumed by release verification are admitted here; unrelated
        // social or discussion objects cannot expand this retrieval merely by
        // mentioning a target id under another relation.
        let reverse_relations = reverse_relations(&object.object_type);
        if !reverse_relations.is_empty() {
            for candidate_id in store.linking_to(&id)? {
                let candidate = store.get(&candidate_id)?;
                if candidate
                    .links
                    .iter()
                    .any(|link| link.target == id && reverse_relations.contains(&link.rel.as_str()))
                {
                    enqueue(&mut pending, &selected, candidate_id);
                }
            }
        }
    }

    selected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(selected)
}

fn enqueue(pending: &mut VecDeque<ObjectId>, selected: &[ObjectId], id: ObjectId) {
    if !selected.iter().any(|existing| existing == &id)
        && !pending.iter().any(|existing| existing == &id)
    {
        pending.push_back(id);
    }
}

fn reverse_relations(object_type: &ObjectType) -> &'static [&'static str] {
    if *object_type == ObjectType::RELEASE {
        return &["release"];
    }
    match object_type {
        ObjectType::Custom(name) if name == PROJECT_TYPE => &["prev"],
        ObjectType::Custom(name) if name == CHAIN_TYPE => &["prev", "pr"],
        ObjectType::Custom(name) if name == PR_TYPE => &["pr"],
        _ => &[],
    }
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
        let tree = object(
            ObjectType::Custom("mini/tree".to_string()),
            &human.did(),
            &device,
            &[],
        );
        store.insert(&tree).unwrap();
        let commit = object(
            ObjectType::COMMIT,
            &human.did(),
            &device,
            &[("tree", tree.id())],
        );
        store.insert(&commit).unwrap();
        let chunk = object(
            ObjectType::Custom("mini/media-chunk".to_string()),
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
                ("base", project.id()),
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
}
