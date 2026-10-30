//! Bounded owner-reviewed JPEG publication. No paths or network access.
use crate::{profile_details_values, validate_profile_details_pair, ErrorCode, ServiceError};
use did_mini::{Controller, Did};
use mini_app_protocol::{PublicProfileDetails, MAX_PROFILE_PHOTO_BYTES};
use mini_objects::{Object, ObjectType, Payload};
use mini_store::{MemoryBackend, Store};
fn bad(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::new(ErrorCode::BadRequest, error.to_string())
}
pub(crate) fn digest(photo: &[u8]) -> [u8; 32] {
    mini_crypto::HashAlgorithm::Blake3.digest(photo)
}
pub(crate) fn validate(photo: &[u8]) -> Result<(), ServiceError> {
    if photo.is_empty()
        || photo.len() > MAX_PROFILE_PHOTO_BYTES
        || !photo.starts_with(&[0xff, 0xd8])
    {
        return Err(bad("photo must be a bounded JPEG"));
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(photo), image::ImageFormat::Jpeg);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(256);
    limits.max_image_height = Some(256);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(bad)?;
    if image.width() == 0 || image.height() == 0 || image.width() > 256 || image.height() > 256 {
        return Err(bad("photo dimensions exceed 256 pixels"));
    }
    Ok(())
}
pub(crate) fn build(
    human: &Did,
    device: &Controller,
    draft: &PublicProfileDetails,
    photo: &[u8],
    base: u64,
    time: u64,
) -> Result<Vec<Vec<u8>>, ServiceError> {
    let profile_sequence = base
        .checked_add(1)
        .ok_or_else(|| bad("photo sequence overflow"))?;
    let mut memory = Store::new(MemoryBackend::new());
    let manifest =
        mini_media::publish_media(&mut memory, human, device, "image/jpeg", photo, time, base)
            .map_err(bad)?;
    if manifest.chunks.len() != 1 {
        return Err(bad("photo must fit one chunk"));
    }
    let mut effective = draft.clone();
    effective.avatar = Some(manifest.id.as_str().into());
    let (avatar, fields) = profile_details_values(&effective)?;
    let public = mini_social::PublicProfileDraft {
        display_name: &effective.display_name,
        bio: &effective.bio,
        avatar: avatar.as_ref(),
        location: effective.location.as_deref(),
        age: effective.age,
        fields: &fields,
    };
    let (profile, head) =
        mini_social::build_profile_details(human, device, &public, time, profile_sequence)
            .map_err(bad)?;
    Ok(vec![
        memory.get(&manifest.chunks[0]).map_err(bad)?.to_bytes(),
        memory.get(&manifest.id).map_err(bad)?.to_bytes(),
        profile.to_bytes(),
        head.to_bytes(),
    ])
}
pub(crate) fn validate_record(
    draft: &PublicProfileDetails,
    expected_digest: &[u8; 32],
    expected_len: u32,
    bytes: &[Vec<u8>],
) -> Result<Vec<Object>, ServiceError> {
    draft.validate().map_err(bad)?;
    if draft.avatar.is_some()
        || expected_len == 0
        || expected_len as usize > MAX_PROFILE_PHOTO_BYTES
        || bytes.len() != 4
    {
        return Err(bad("invalid photo profile journal shape"));
    }
    let mut objects = Vec::with_capacity(4);
    for bytes in bytes {
        let object = Object::from_bytes(bytes).map_err(bad)?;
        if object.to_bytes() != *bytes || object.signatures().is_empty() {
            return Err(bad("noncanonical photo journal object"));
        }
        object.verify_integrity(object.id()).map_err(bad)?;
        objects.push(object);
    }
    let (chunk, manifest, profile, head) = (&objects[0], &objects[1], &objects[2], &objects[3]);
    let Payload::Public(photo) = &chunk.payload else {
        return Err(bad("photo chunk must be public"));
    };
    if chunk.object_type != ObjectType::Custom(mini_media::CHUNK_TYPE.into())
        || !chunk.links.is_empty()
        || photo.len() != expected_len as usize
        || &digest(photo) != expected_digest
    {
        return Err(bad("photo journal differs from reviewed bytes"));
    }
    validate(photo)?;
    let media = mini_media::read_manifest(manifest).map_err(bad)?;
    if media.content_type != "image/jpeg"
        || media.total_len != u64::from(expected_len)
        || &media.digest != expected_digest
        || media.chunks != vec![chunk.id().clone()]
        || manifest.links.len() != 1
        || manifest.links[0].rel != "chunk"
        || manifest.links[0].target != *chunk.id()
    {
        return Err(bad("photo manifest does not bind the exact reviewed chunk"));
    }
    let next = chunk
        .sequence
        .checked_add(1)
        .ok_or_else(|| bad("photo journal sequence overflow"))?;
    for (index, object) in objects.iter().enumerate() {
        if object.author_human != chunk.author_human
            || object.author_device != chunk.author_device
            || object.timestamp_ms != chunk.timestamp_ms
            || object.sequence != if index < 2 { chunk.sequence } else { next }
        {
            return Err(bad("photo journal identities, time or sequences disagree"));
        }
    }
    let mut effective = draft.clone();
    effective.avatar = Some(manifest.id().as_str().into());
    validate_profile_details_pair(profile, head, &effective)?;
    // The existing assembler checks both length and digest against stored public
    // chunk bytes, entirely in memory before any filesystem store mutation.
    let mut memory = Store::new(MemoryBackend::new());
    memory.insert(chunk).map_err(bad)?;
    memory.insert(manifest).map_err(bad)?;
    if mini_media::assemble(&memory, &media).map_err(bad)? != *photo {
        return Err(bad("photo assembly differs from reviewed bytes"));
    }
    Ok(objects)
}

#[cfg(test)]
pub(crate) fn test_jpeg(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([120, 160, 200]));
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85)
        .encode_image(&image)
        .unwrap();
    bytes
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jpeg_validation_enforces_byte_dimension_and_format_limits() {
        assert!(validate(&test_jpeg(2, 2)).is_ok());
        assert!(validate(&test_jpeg(257, 1)).is_err());
        assert!(validate(b"not a JPEG").is_err());
        assert!(validate(&vec![0; MAX_PROFILE_PHOTO_BYTES + 1]).is_err());
    }
}
