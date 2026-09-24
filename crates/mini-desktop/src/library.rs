//! Files and movies as seedable, resumable, content-addressed objects.
//!
//! `mini-media` already splits any payload into 1 MiB chunk objects plus a
//! manifest, capped at 256 MiB per manifest. A larger file is published as
//! an ordered *collection*: one small signed object whose `part` links name
//! up to 256 manifests (≈64 GiB), with the file name, content type and
//! total length in its payload. Every part is an ordinary manifest, so
//! chunks replicate, resume and verify exactly as today — a peer that holds
//! half the parts seeds half the movie. Nothing here is a player or a
//! streaming protocol; export reassembles to disk chunk by chunk without
//! holding the file in memory.

use did_mini::{Controller, Did};
use mini_media::{
    assemble_to_writer, missing_chunks, publish_media, read_manifest, Manifest, CHUNK_SIZE,
    MAX_TOTAL_LEN,
};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};
use std::io::{Read, Write};
use std::path::Path;

pub const COLLECTION_TYPE: &str = "mininet/media-collection/v1";
const COLLECTION_VERSION: u8 = 1;
pub const MAX_PARTS: usize = 256;
pub const MAX_NAME_BYTES: usize = 255;
pub const MAX_CONTENT_TYPE_BYTES: usize = 128;
/// Bytes per part; the largest a single manifest may carry.
pub const PART_BYTES: u64 = MAX_TOTAL_LEN;

/// What a library entry is backed by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Manifest,
    Collection,
}

/// One file this device knows about, fully or partly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: ObjectId,
    pub kind: Kind,
    pub name: String,
    pub content_type: String,
    pub total_len: u64,
    pub author: Did,
    pub timestamp_ms: u64,
    /// Chunks present / chunks total across every part.
    pub chunks_present: usize,
    pub chunks_total: usize,
    /// Parts whose manifest object itself has not arrived yet.
    pub parts_missing: usize,
}

impl Item {
    pub fn complete(&self) -> bool {
        self.parts_missing == 0 && self.chunks_present == self.chunks_total
    }

    pub fn percent(&self) -> u8 {
        if self.chunks_total == 0 {
            return if self.complete() { 100 } else { 0 };
        }
        ((self.chunks_present as u128 * 100) / self.chunks_total as u128) as u8
    }
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn get_str(bytes: &[u8], pos: &mut usize, max: usize) -> Option<String> {
    let len = u32::from_be_bytes(bytes.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
    *pos += 4;
    if len > max {
        return None;
    }
    let s = String::from_utf8(bytes.get(*pos..*pos + len)?.to_vec()).ok()?;
    *pos += len;
    Some(s)
}

/// Decoded collection header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    pub id: ObjectId,
    pub name: String,
    pub content_type: String,
    pub total_len: u64,
    pub parts: Vec<ObjectId>,
    pub author: Did,
    pub timestamp_ms: u64,
}

pub fn read_collection(object: &Object) -> Result<Collection, String> {
    if object.object_type != ObjectType::Custom(COLLECTION_TYPE.into()) {
        return Err("not a media collection".into());
    }
    let Payload::Public(bytes) = &object.payload else {
        return Err("collection payload is not public".into());
    };
    let mut pos = 0;
    if bytes.first() != Some(&COLLECTION_VERSION) {
        return Err("unknown collection version".into());
    }
    pos += 1;
    let name = get_str(bytes, &mut pos, MAX_NAME_BYTES).ok_or("malformed collection name")?;
    let content_type =
        get_str(bytes, &mut pos, MAX_CONTENT_TYPE_BYTES).ok_or("malformed content type")?;
    let total_len = u64::from_be_bytes(
        bytes
            .get(pos..pos + 8)
            .ok_or("malformed length")?
            .try_into()
            .map_err(|_| "malformed length")?,
    );
    pos += 8;
    if pos != bytes.len() {
        return Err("trailing bytes in collection".into());
    }
    let parts: Vec<ObjectId> = object
        .links
        .iter()
        .filter(|link| link.rel == "part")
        .map(|link| link.target.clone())
        .collect();
    if parts.is_empty() || parts.len() > MAX_PARTS {
        return Err("collection has no parts or too many".into());
    }
    if total_len > parts.len() as u64 * PART_BYTES {
        return Err("collection length exceeds its parts".into());
    }
    Ok(Collection {
        id: object.id().clone(),
        name,
        content_type,
        total_len,
        parts,
        author: object.author_human.clone(),
        timestamp_ms: object.timestamp_ms,
    })
}

/// Guess a content type from a file extension; unknown becomes
/// `application/octet-stream`, which still seeds and exports fine.
pub fn content_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp4" | "m4v") => "video/mp4",
        Some("mkv") => "video/x-matroska",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("avi") => "video/x-msvideo",
        Some("mp3") => "audio/mpeg",
        Some("flac") => "audio/flac",
        Some("ogg" | "oga") => "audio/ogg",
        Some("wav") => "audio/wav",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("pdf") => "application/pdf",
        Some("txt" | "md") => "text/plain",
        Some("zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

/// Result of publishing a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    /// The object a post should link: the manifest, or the collection.
    pub id: ObjectId,
    pub parts: usize,
    pub bytes: u64,
    /// Objects written (chunks + manifests + collection).
    pub objects: u64,
}

/// Publish `path` as chunks + manifest(s) (+ collection when larger than
/// one part). `sequence` is the author's next sequence; the returned
/// `objects` count is how far it advanced. Reads one part at a time.
#[allow(clippy::too_many_arguments)]
pub fn publish_file<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    path: &Path,
    name: &str,
    content_type: &str,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Published, String> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_NAME_BYTES {
        return Err(format!("name must be 1..={MAX_NAME_BYTES} bytes"));
    }
    if content_type.is_empty() || content_type.len() > MAX_CONTENT_TYPE_BYTES {
        return Err("content type is empty or too long".into());
    }
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("could not open file: {error}"))?;
    let total_len = file.metadata().map_err(|error| error.to_string())?.len();
    if total_len == 0 {
        return Err("the file is empty".into());
    }
    let parts_needed = total_len.div_ceil(PART_BYTES) as usize;
    if parts_needed > MAX_PARTS {
        return Err(format!(
            "file is larger than {} GiB, the most one collection can hold",
            (MAX_PARTS as u64 * PART_BYTES) >> 30
        ));
    }
    let mut manifests: Vec<Manifest> = Vec::with_capacity(parts_needed);
    let mut next_sequence = sequence;
    let mut objects: u64 = 0;
    let mut remaining = total_len;
    let mut buffer = vec![0u8; PART_BYTES.min(remaining) as usize];
    while remaining > 0 {
        let take = PART_BYTES.min(remaining) as usize;
        buffer.resize(take, 0);
        file.read_exact(&mut buffer)
            .map_err(|error| format!("could not read file: {error}"))?;
        let manifest = publish_media(
            store,
            human,
            device,
            content_type,
            &buffer,
            timestamp_ms,
            next_sequence,
        )
        .map_err(|error| error.to_string())?;
        let written = manifest.chunks.len() as u64 + 1;
        next_sequence = next_sequence.saturating_add(written);
        objects = objects.saturating_add(written);
        manifests.push(manifest);
        remaining -= take as u64;
    }
    // Even a one-part file gets a collection header: a manifest has no name,
    // and peers should see "movie.mp4", not a content id.
    let mut payload = vec![COLLECTION_VERSION];
    put_str(&mut payload, name);
    put_str(&mut payload, content_type);
    payload.extend_from_slice(&total_len.to_be_bytes());
    let mut builder = ObjectBuilder::new(ObjectType::Custom(COLLECTION_TYPE.into()))
        .timestamp_ms(timestamp_ms)
        .sequence(next_sequence)
        .payload(Payload::Public(payload));
    for manifest in &manifests {
        builder = builder.link("part", manifest.id.clone());
    }
    let collection = builder
        .sign(human, device)
        .map_err(|error| error.to_string())?;
    store
        .insert(&collection)
        .map_err(|error| error.to_string())?;
    Ok(Published {
        id: collection.id().clone(),
        parts: manifests.len(),
        bytes: total_len,
        objects: objects + 1,
    })
}

fn manifest_progress<B: Backend>(store: &Store<B>, manifest: &Manifest) -> (usize, usize) {
    let total = manifest.chunks.len();
    let missing = missing_chunks(store, manifest)
        .map(|missing| missing.len())
        .unwrap_or(total);
    (total - missing, total)
}

/// Every manifest and collection on the device. Manifests that are parts
/// of a known collection are folded into it rather than listed twice.
pub fn list<B: Backend>(store: &Store<B>) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    let mut folded: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for id in store
        .by_type(&ObjectType::Custom(COLLECTION_TYPE.into()))
        .map_err(|error| error.to_string())?
    {
        let Ok(object) = store.get(&id) else { continue };
        let Ok(collection) = read_collection(&object) else {
            continue;
        };
        let mut present = 0;
        let mut total = 0;
        let mut parts_missing = 0;
        for part in &collection.parts {
            folded.insert(part.as_str().to_owned());
            match store.get(part).ok().and_then(|o| read_manifest(&o).ok()) {
                Some(manifest) => {
                    let (p, t) = manifest_progress(store, &manifest);
                    present += p;
                    total += t;
                }
                None => {
                    parts_missing += 1;
                    total += (PART_BYTES / CHUNK_SIZE as u64) as usize;
                }
            }
        }
        items.push(Item {
            id: collection.id,
            kind: Kind::Collection,
            name: collection.name,
            content_type: collection.content_type,
            total_len: collection.total_len,
            author: collection.author,
            timestamp_ms: collection.timestamp_ms,
            chunks_present: present,
            chunks_total: total,
            parts_missing,
        });
    }
    for id in store
        .by_type(&ObjectType::MEDIA_MANIFEST)
        .map_err(|error| error.to_string())?
    {
        if folded.contains(id.as_str()) {
            continue;
        }
        let Ok(object) = store.get(&id) else { continue };
        let Ok(manifest) = read_manifest(&object) else {
            continue;
        };
        let (present, total) = manifest_progress(store, &manifest);
        items.push(Item {
            name: format!(
                "{} {}",
                manifest.content_type,
                &id.as_str()[..12.min(id.as_str().len())]
            ),
            id,
            kind: Kind::Manifest,
            content_type: manifest.content_type,
            total_len: manifest.total_len,
            author: object.author_human,
            timestamp_ms: object.timestamp_ms,
            chunks_present: present,
            chunks_total: total,
            parts_missing: 0,
        });
    }
    items.sort_by(|a, b| {
        b.timestamp_ms
            .cmp(&a.timestamp_ms)
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    Ok(items)
}

/// Write a complete item to `writer`, part by part, chunk by chunk.
pub fn export<B: Backend, W: Write>(
    store: &Store<B>,
    item: &ObjectId,
    writer: &mut W,
) -> Result<u64, String> {
    let object = store.get(item).map_err(|error| error.to_string())?;
    if let Ok(manifest) = read_manifest(&object) {
        assemble_to_writer(store, &manifest, writer).map_err(|error| error.to_string())?;
        return Ok(manifest.total_len);
    }
    let collection = read_collection(&object)?;
    let mut written: u64 = 0;
    for (index, part) in collection.parts.iter().enumerate() {
        let part_object = store.get(part).map_err(|_| {
            format!(
                "part {} of {} has not arrived",
                index + 1,
                collection.parts.len()
            )
        })?;
        let manifest = read_manifest(&part_object).map_err(|error| error.to_string())?;
        assemble_to_writer(store, &manifest, writer).map_err(|error| {
            format!(
                "part {} of {} is incomplete: {error}",
                index + 1,
                collection.parts.len()
            )
        })?;
        written += manifest.total_len;
    }
    if written != collection.total_len {
        return Err("assembled length does not match the collection".into());
    }
    Ok(written)
}

/// Human-readable size.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Capabilities;
    use mini_store::MemoryBackend;

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

    fn temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mininet-library-{}-{}-{name}",
            std::process::id(),
            crate::now_ms()
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn small_file_is_one_named_part_and_round_trips() {
        let mut store = Store::new(MemoryBackend::new());
        let (me, dev) = person(10);
        let bytes: Vec<u8> = (0..(3 * CHUNK_SIZE + 17))
            .map(|i| (i % 251) as u8)
            .collect();
        let path = temp_file("clip.mp4", &bytes);
        let published = publish_file(
            &mut store,
            &me.did(),
            &dev,
            &path,
            "clip.mp4",
            content_type_for(&path),
            5,
            1,
        )
        .unwrap();
        assert_eq!(published.parts, 1);
        assert_eq!(published.objects, 6);
        let items = list(&store).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, Kind::Collection);
        assert_eq!(items[0].name, "clip.mp4");
        assert!(items[0].complete());
        assert_eq!(items[0].content_type, "video/mp4");
        let mut out = Vec::new();
        assert_eq!(
            export(&store, &published.id, &mut out).unwrap(),
            bytes.len() as u64
        );
        assert_eq!(out, bytes);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn large_file_becomes_a_collection_and_reports_progress() {
        // Shrink the part size for the test through a two-part file that
        // exceeds one part: build the collection by hand from two manifests
        // published the normal way, then exercise list/export on it.
        let mut store = Store::new(MemoryBackend::new());
        let (me, dev) = person(20);
        let a: Vec<u8> = vec![1; CHUNK_SIZE + 5];
        let b: Vec<u8> = vec![2; 2 * CHUNK_SIZE];
        let ma = publish_media(&mut store, &me.did(), &dev, "video/mp4", &a, 1, 1).unwrap();
        let mb = publish_media(&mut store, &me.did(), &dev, "video/mp4", &b, 1, 10).unwrap();
        let mut payload = vec![COLLECTION_VERSION];
        put_str(&mut payload, "movie.mp4");
        put_str(&mut payload, "video/mp4");
        payload.extend_from_slice(&((a.len() + b.len()) as u64).to_be_bytes());
        let collection = ObjectBuilder::new(ObjectType::Custom(COLLECTION_TYPE.into()))
            .timestamp_ms(2)
            .sequence(20)
            .payload(Payload::Public(payload))
            .link("part", ma.id.clone())
            .link("part", mb.id.clone())
            .sign(&me.did(), &dev)
            .unwrap();
        store.insert(&collection).unwrap();

        let items = list(&store).unwrap();
        assert_eq!(items.len(), 1, "parts are folded into the collection");
        let item = &items[0];
        assert_eq!(item.kind, Kind::Collection);
        assert_eq!(item.name, "movie.mp4");
        assert_eq!(item.chunks_total, 4);
        assert!(item.complete());
        assert_eq!(item.percent(), 100);
        let mut out = Vec::new();
        export(&store, collection.id(), &mut out).unwrap();
        assert_eq!(out.len(), a.len() + b.len());
        assert_eq!(&out[..a.len()], &a[..]);
        assert_eq!(&out[a.len()..], &b[..]);

        // A receiver that only has the collection header and part A.
        let mut partial = Store::new(MemoryBackend::new());
        partial.insert(&collection).unwrap();
        partial.insert(&store.get(&ma.id).unwrap()).unwrap();
        for chunk in &ma.chunks {
            partial.insert(&store.get(chunk).unwrap()).unwrap();
        }
        let items = list(&partial).unwrap();
        assert_eq!(items[0].parts_missing, 1);
        assert!(!items[0].complete());
        assert!(items[0].percent() < 100);
        let mut out = Vec::new();
        assert!(export(&partial, collection.id(), &mut out).is_err());
    }

    #[test]
    fn sizes_and_types_read_well() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(content_type_for(Path::new("x.MKV")), "video/x-matroska");
        assert_eq!(
            content_type_for(Path::new("x.bin")),
            "application/octet-stream"
        );
    }
}
