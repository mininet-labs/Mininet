//! Network-wide search and targeted fetch over the desktop link.
//!
//! *Search*: the dialer sends a query; the peer answers from its own media
//! catalog (bounded, words over title/description/author/type) with small
//! result records — ids, title, author, kind, size, time. Results are
//! hints from that peer, not verified objects.
//!
//! *Fetch*: the dialer names a post; the peer serves the exact closure the
//! post needs — the post, the author's identity carriers, the manifest or
//! collection with its part manifests, and as many chunks as the retrieval
//! budget allows — through `mini-sync`'s existing bounded retrieval, which
//! verifies every object on ingest. Chunks past the budget arrive through
//! later fetches or ordinary sessions; the store resumes by id.
//!
//! Both are owner-triggered from the Media view and fan out only to saved
//! peers. This is search over the peers you chose, not an index of the
//! whole network.

use crate::catalog;
use crate::library;
use did_mini::Did;
use mini_media::read_manifest;
use mini_objects::{ObjectId, ObjectType};
use mini_store::{Backend, Store};
use mini_sync::KEL_CARRIER;
use std::collections::BTreeSet;

pub const MAX_QUERY_BYTES: usize = 200;
pub const MAX_RESULTS: usize = 50;
/// Objects one fetch may select (mini-sync caps retrieval at 4096).
pub const MAX_FETCH_OBJECTS: usize = 4000;
/// Bytes one fetch may select, under mini-sync's 512 MiB retrieval budget.
pub const MAX_FETCH_BYTES: usize = 480 * 1024 * 1024;

/// One search hit as a peer reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteResult {
    pub post: ObjectId,
    pub media: ObjectId,
    pub title: String,
    pub description: String,
    pub author_name: String,
    pub author_did: String,
    pub kind: String,
    pub bytes: u64,
    pub timestamp_ms: u64,
    /// Endpoint of the peer that actually holds it when the answering peer
    /// forwarded the search; empty when the answering peer holds it.
    pub via: String,
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    out.extend_from_slice(&(bytes.len().min(u16::MAX as usize) as u16).to_be_bytes());
    out.extend_from_slice(&bytes[..bytes.len().min(u16::MAX as usize)]);
}

fn get_str(bytes: &[u8], pos: &mut usize) -> Result<String, String> {
    let len = u16::from_be_bytes(
        bytes
            .get(*pos..*pos + 2)
            .ok_or("truncated")?
            .try_into()
            .map_err(|_| "truncated")?,
    ) as usize;
    *pos += 2;
    let s = String::from_utf8(bytes.get(*pos..*pos + len).ok_or("truncated")?.to_vec())
        .map_err(|_| "not UTF-8")?;
    *pos += len;
    Ok(s)
}

fn get_u64(bytes: &[u8], pos: &mut usize) -> Result<u64, String> {
    let v = u64::from_be_bytes(
        bytes
            .get(*pos..*pos + 8)
            .ok_or("truncated")?
            .try_into()
            .map_err(|_| "truncated")?,
    );
    *pos += 8;
    Ok(v)
}

pub fn encode_results(results: &[RemoteResult]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(results.len().min(MAX_RESULTS) as u16).to_be_bytes());
    for r in results.iter().take(MAX_RESULTS) {
        put_str(&mut out, r.post.as_str());
        put_str(&mut out, r.media.as_str());
        put_str(&mut out, &r.title);
        put_str(
            &mut out,
            &r.description.chars().take(240).collect::<String>(),
        );
        put_str(&mut out, &r.author_name);
        put_str(&mut out, &r.author_did);
        put_str(&mut out, &r.kind);
        out.extend_from_slice(&r.bytes.to_be_bytes());
        out.extend_from_slice(&r.timestamp_ms.to_be_bytes());
        put_str(&mut out, &r.via);
    }
    out
}

pub fn decode_results(bytes: &[u8]) -> Result<Vec<RemoteResult>, String> {
    let mut pos = 0;
    let count = u16::from_be_bytes(
        bytes
            .get(0..2)
            .ok_or("truncated")?
            .try_into()
            .map_err(|_| "truncated")?,
    ) as usize;
    pos += 2;
    if count > MAX_RESULTS {
        return Err("too many results".into());
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let post = ObjectId::parse(&get_str(bytes, &mut pos)?).map_err(|e| e.to_string())?;
        let media = ObjectId::parse(&get_str(bytes, &mut pos)?).map_err(|e| e.to_string())?;
        let title = get_str(bytes, &mut pos)?;
        let description = get_str(bytes, &mut pos)?;
        let author_name = get_str(bytes, &mut pos)?;
        let author_did = get_str(bytes, &mut pos)?;
        Did::parse(&author_did).map_err(|e| e.to_string())?;
        let kind = get_str(bytes, &mut pos)?;
        let bytes_len = get_u64(bytes, &mut pos)?;
        let timestamp_ms = get_u64(bytes, &mut pos)?;
        let via = get_str(bytes, &mut pos)?;
        out.push(RemoteResult {
            post,
            media,
            title,
            description,
            author_name,
            author_did,
            kind,
            bytes: bytes_len,
            timestamp_ms,
            via,
        });
    }
    if pos != bytes.len() {
        return Err("trailing bytes".into());
    }
    Ok(out)
}

/// Answer a query from this device's catalog.
pub fn local_results<B: Backend>(
    store: &Store<B>,
    viewer: &Did,
    query: &str,
) -> Result<Vec<RemoteResult>, String> {
    let query: String = query.chars().take(MAX_QUERY_BYTES).collect();
    let mut entries = catalog::build(store, viewer)?;
    entries.retain(|entry| entry.matches(&query));
    catalog::sort(&mut entries, catalog::Sort::Newest);
    Ok(entries
        .into_iter()
        .take(MAX_RESULTS)
        .map(|entry| RemoteResult {
            post: entry.post,
            media: entry.media,
            title: entry.title,
            description: entry.description,
            author_name: entry.author_name,
            author_did: entry.author_did,
            kind: entry.kind.label().to_string(),
            bytes: entry.bytes,
            timestamp_ms: entry.timestamp_ms,
            via: String::new(),
        })
        .collect())
}

/// The objects a requester needs to hold `seeds` (posts) usefully, in an
/// order that lets partial delivery still verify: identity carriers, the
/// post, manifests, then chunks. Bounded by object count and bytes.
pub fn fetch_closure<B: Backend>(
    store: &Store<B>,
    seeds: &[ObjectId],
) -> Result<Vec<ObjectId>, String> {
    let mut selected: Vec<ObjectId> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut bytes_left = MAX_FETCH_BYTES;
    let mut push = |id: &ObjectId, selected: &mut Vec<ObjectId>, bytes_left: &mut usize| -> bool {
        if selected.len() >= MAX_FETCH_OBJECTS || !seen.insert(id.as_str().to_owned()) {
            return false;
        }
        let Ok(object) = store.get(id) else {
            return false;
        };
        let size = object.to_bytes().len();
        if size > *bytes_left {
            return false;
        }
        *bytes_left -= size;
        selected.push(id.clone());
        true
    };
    for seed in seeds {
        let Ok(post) = store.get(seed) else {
            continue;
        };
        // Identity first: without the author's KELs nothing else verifies.
        if let Ok(ids) = store.by_author(&post.author_human) {
            for id in ids {
                if let Ok(object) = store.get(&id) {
                    if object.object_type == ObjectType::Custom(KEL_CARRIER.to_string()) {
                        push(&id, &mut selected, &mut bytes_left);
                    }
                }
            }
        }
        // The author's current profile so the requester can show a name:
        // the profile object plus the signed HEAD object that names it as
        // current (resolution goes through the head, not the type index).
        if let Ok(Some(target)) = store.resolve_head(&post.author_human, "profile") {
            push(&target, &mut selected, &mut bytes_left);
            if let Ok(ids) = store.by_author(&post.author_human) {
                for id in ids {
                    if let Ok(object) = store.get(&id) {
                        if object.object_type == ObjectType::HEAD
                            && object.links.iter().any(|link| link.target == target)
                        {
                            push(&id, &mut selected, &mut bytes_left);
                        }
                    }
                }
            }
        }
        push(seed, &mut selected, &mut bytes_left);
        let Ok(resolved) = mini_social::resolve_post(store, seed) else {
            continue;
        };
        let mini_social::PostKind::Media { media } = resolved.kind else {
            continue;
        };
        let Ok(media_object) = store.get(&media) else {
            continue;
        };
        let mut manifests = Vec::new();
        if read_manifest(&media_object).is_ok() {
            push(&media, &mut selected, &mut bytes_left);
            manifests.push(media.clone());
        } else if let Ok(collection) = library::read_collection(&media_object) {
            push(&media, &mut selected, &mut bytes_left);
            for part in collection.parts {
                push(&part, &mut selected, &mut bytes_left);
                manifests.push(part);
            }
        }
        for manifest_id in manifests {
            let Ok(object) = store.get(&manifest_id) else {
                continue;
            };
            let Ok(manifest) = read_manifest(&object) else {
                continue;
            };
            for chunk in &manifest.chunks {
                if !push(chunk, &mut selected, &mut bytes_left)
                    && selected.len() >= MAX_FETCH_OBJECTS
                {
                    return Ok(selected);
                }
            }
        }
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    use mini_media::publish_media;
    use mini_social::{publish_media_post, publish_profile};
    use mini_store::MemoryBackend;
    use mini_sync::kel_carrier;

    fn person(seed: u8) -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    #[test]
    fn results_round_trip_and_reject_garbage() {
        let (alice, dev) = person(10);
        let mut store = Store::new(MemoryBackend::new());
        let media =
            publish_media(&mut store, &alice.did(), &dev, "video/mp4", &[1; 5], 1, 1).unwrap();
        let post = publish_media_post(
            &mut store,
            &alice.did(),
            &dev,
            media.id.clone(),
            "Title\nDesc",
            2,
            2,
        )
        .unwrap();
        let results = local_results(&store, &alice.did(), "title").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].post, *post.id());
        assert_eq!(results[0].kind, "Video");
        let decoded = decode_results(&encode_results(&results)).unwrap();
        assert_eq!(decoded, results);
        assert!(decode_results(&[0, 9, 1]).is_err());
        assert!(local_results(&store, &alice.did(), "nomatch")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn fetch_closure_carries_identity_post_manifest_and_chunks() {
        let (alice, dev) = person(20);
        let mut store = Store::new(MemoryBackend::new());
        for kel in [alice.kel(), dev.kel()] {
            store
                .insert(&kel_carrier(&kel, &alice.did(), &dev).unwrap())
                .unwrap();
        }
        publish_profile(&mut store, &alice.did(), &dev, "Alice", "", None, 1, 3).unwrap();
        let media = publish_media(
            &mut store,
            &alice.did(),
            &dev,
            "video/mp4",
            &vec![7u8; 2 * mini_media::CHUNK_SIZE + 1],
            2,
            4,
        )
        .unwrap();
        let post = publish_media_post(
            &mut store,
            &alice.did(),
            &dev,
            media.id.clone(),
            "Clip",
            3,
            10,
        )
        .unwrap();
        let closure = fetch_closure(&store, &[post.id().clone()]).unwrap();
        // 2 carriers + profile + its head + post + manifest + 3 chunks.
        assert_eq!(closure.len(), 2 + 2 + 1 + 1 + 3, "{closure:?}");
        assert_eq!(&closure[4], post.id());
        assert!(closure.contains(&media.id));
        for chunk in &media.chunks {
            assert!(closure.contains(chunk));
        }
        // A non-post seed still gets identity + profile + head + itself.
        assert_eq!(
            fetch_closure(&store, std::slice::from_ref(&media.chunks[0]))
                .unwrap()
                .len(),
            5
        );
    }
}
