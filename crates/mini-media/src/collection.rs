//! Shared decoder for the existing named media-collection v1 wire format.
//! Parsing grants neither provenance nor permission to fetch referenced parts.
use did_mini::Did;
use mini_objects::{Object, ObjectId, ObjectType, Payload};
/// Existing desktop collection wire tag.
pub const COLLECTION_TYPE: &str = "mininet/media-collection/v1";
/// Existing desktop collection wire version.
pub const COLLECTION_VERSION: u8 = 1;
/// Maximum manifests in one named collection.
pub const MAX_COLLECTION_PARTS: usize = 256;
/// Maximum UTF-8 file-name bytes.
pub const MAX_COLLECTION_NAME_BYTES: usize = 255;
/// Maximum UTF-8 MIME-type bytes.
pub const MAX_COLLECTION_CONTENT_TYPE_BYTES: usize = 128;
/// Maximum bytes per collection part.
pub const COLLECTION_PART_BYTES: u64 = crate::MAX_TOTAL_LEN;
use {
    COLLECTION_PART_BYTES as PART_BYTES,
    MAX_COLLECTION_CONTENT_TYPE_BYTES as MAX_CONTENT_TYPE_BYTES,
    MAX_COLLECTION_NAME_BYTES as MAX_NAME_BYTES, MAX_COLLECTION_PARTS as MAX_PARTS,
};
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
pub struct MediaCollection {
    pub id: ObjectId,
    pub name: String,
    pub content_type: String,
    pub total_len: u64,
    pub parts: Vec<ObjectId>,
    pub author: Did,
    pub timestamp_ms: u64,
}

/// Decode a canonical public collection header without accessing its parts.
pub fn read_collection(object: &Object) -> Result<MediaCollection, String> {
    if object.object_type != ObjectType::Custom(COLLECTION_TYPE.into()) {
        return Err("not a media collection".into());
    }
    let Payload::Public(bytes) = &object.payload else {
        return Err("collection payload is not public".into());
    };
    if bytes.len() > 1 + 4 + MAX_NAME_BYTES + 4 + MAX_CONTENT_TYPE_BYTES + 8
        || object.links.is_empty()
        || object.links.len() > MAX_PARTS
        || object.links.iter().any(|link| link.rel != "part")
    {
        return Err("noncanonical collection shape".into());
    }
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
    Ok(MediaCollection {
        id: object.id().clone(),
        name,
        content_type,
        total_len,
        parts,
        author: object.author_human.clone(),
        timestamp_ms: object.timestamp_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;
    use mini_objects::ObjectBuilder;
    fn field(out: &mut Vec<u8>, value: &str) {
        out.extend_from_slice(&(value.len() as u32).to_be_bytes());
        out.extend_from_slice(value.as_bytes());
    }
    #[test]
    fn legacy_collection_header_is_bounded_exact_and_does_not_need_its_parts() {
        let root = Controller::incept_single_from_seeds(&[21; 32], &[22; 32]).unwrap();
        let target = ObjectBuilder::new(ObjectType::POST)
            .payload(Payload::Public(vec![]))
            .sign(&root.did(), &root)
            .unwrap();
        let mut payload = vec![COLLECTION_VERSION];
        field(&mut payload, "movie.mp4");
        field(&mut payload, "video/mp4");
        payload.extend_from_slice(&123u64.to_be_bytes());
        let build = |body: Vec<u8>, extra: bool| {
            let mut builder = ObjectBuilder::new(ObjectType::Custom(COLLECTION_TYPE.into()))
                .payload(Payload::Public(body))
                .link("part", target.id().clone());
            if extra {
                builder = builder.link("unexpected", target.id().clone());
            }
            builder.sign(&root.did(), &root).unwrap()
        };
        let object = build(payload.clone(), false);
        let collection = read_collection(&object).unwrap();
        assert_eq!(collection.name, "movie.mp4");
        assert_eq!(collection.total_len, 123);
        assert_eq!(collection.parts, vec![target.id().clone()]);
        assert_eq!(collection.author, root.did());
        assert!(read_collection(&build(payload.clone(), true)).is_err());
        let mut trailing = payload.clone();
        trailing.push(0);
        assert!(read_collection(&build(trailing, false)).is_err());
        let mut huge = payload.clone();
        huge[1..5].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(read_collection(&build(huge, false)).is_err());
        let mut over = vec![COLLECTION_VERSION];
        field(&mut over, &"x".repeat(MAX_NAME_BYTES + 1));
        field(&mut over, "video/mp4");
        over.extend_from_slice(&123u64.to_be_bytes());
        assert!(read_collection(&build(over, false)).is_err());
        let mut length = payload;
        let n = length.len();
        length[n - 8..].copy_from_slice(&(PART_BYTES + 1).to_be_bytes());
        assert!(read_collection(&build(length, false)).is_err());
    }
}
