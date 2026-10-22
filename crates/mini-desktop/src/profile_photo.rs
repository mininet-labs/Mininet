//! Prepare a bounded, metadata-free avatar for explicit review before signing.
use eframe::egui;
use std::io::Read;
const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) struct Prepared {
    pub source_path: String,
    pub bytes: Vec<u8>,
    preview: egui::ColorImage,
    texture: Option<egui::TextureHandle>,
}
impl Prepared {
    pub fn matches(&self, source_path: &str, bytes: &[u8]) -> bool {
        self.source_path == source_path.trim() && self.bytes == bytes
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let texture = self.texture.get_or_insert_with(|| {
            ui.ctx().load_texture(
                "reviewed-profile-photo",
                self.preview.clone(),
                egui::TextureOptions::LINEAR,
            )
        });
        ui.image((texture.id(), texture.size_vec2()));
        ui.label(format!(
            "This JPEG preview will be published ({} bytes). The original file is unchanged.",
            self.bytes.len()
        ));
    }
}
pub(crate) fn prepare(path: &str) -> Result<Prepared, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("Could not inspect photo: {error}"))?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_BYTES {
        return Err("Select a regular image file no larger than 8 MiB.".into());
    }
    let file =
        std::fs::File::open(path).map_err(|error| format!("Could not open photo: {error}"))?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Select a regular image file.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read photo: {error}"))?;
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err("Profile photo exceeds 8 MiB.".into());
    }
    prepare_bytes(path, &bytes)
}
fn prepare_bytes(path: &str, bytes: &[u8]) -> Result<Prepared, String> {
    let image = crate::decode_profile_image(bytes)?
        .0
        .thumbnail(256, 256)
        .to_rgba8();
    let (width, height) = image.dimensions();
    let mut rgb = image::RgbImage::new(width, height);
    for (source, target) in image.pixels().zip(rgb.pixels_mut()) {
        let alpha = u16::from(source[3]);
        for channel in 0..3 {
            target[channel] =
                ((u16::from(source[channel]) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        }
    }
    let mut encoded = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 85)
        .encode_image(&rgb)
        .map_err(|error| format!("Could not prepare avatar: {error}"))?;
    if encoded.len() > mini_app_protocol::MAX_PROFILE_PHOTO_BYTES {
        return Err("The optimized photo is too large; choose a simpler or smaller image.".into());
    }
    // Decode the actual encoded bytes: the displayed preview is what gets signed.
    let decoded = image::load_from_memory_with_format(&encoded, image::ImageFormat::Jpeg)
        .map_err(|error| error.to_string())?
        .to_rgba8();
    Ok(Prepared {
        source_path: path.into(),
        bytes: encoded,
        preview: egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            decoded.as_raw(),
        ),
        texture: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png(image: image::DynamicImage) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }
    #[test]
    fn reviewed_photo_is_the_encoded_bounded_jpeg_with_transparency_on_white() {
        let bytes = png(image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_pixel(512, 256, image::Rgba([10, 20, 30, 0])),
        ));
        let prepared = prepare_bytes("image.png", &bytes).unwrap();
        assert!(prepared.bytes.len() <= mini_app_protocol::MAX_PROFILE_PHOTO_BYTES);
        assert_eq!(prepared.source_path, "image.png");
        assert!(prepared.matches(" image.png ", &prepared.bytes));
        assert!(!prepared.matches("other.png", &prepared.bytes));
        let mut replacement = prepared.bytes.clone();
        replacement[0] ^= 1;
        assert!(!prepared.matches("image.png", &replacement));

        assert_eq!(
            image::guess_format(&prepared.bytes).unwrap(),
            image::ImageFormat::Jpeg
        );
        let actual = image::load_from_memory(&prepared.bytes).unwrap().to_rgba8();
        assert_eq!(actual.dimensions(), (256, 128));
        assert_eq!(prepared.preview.size, [256, 128]);
        assert_eq!(
            prepared.preview.pixels[0].to_array(),
            actual.get_pixel(0, 0).0
        );
        assert!(actual.get_pixel(0, 0)[0] >= 250);
    }
    #[test]
    fn invalid_oversized_dimension_and_oversized_source_are_refused() {
        assert!(prepare_bytes("bad", b"invalid").is_err());
        let wide = png(image::DynamicImage::ImageRgb8(image::RgbImage::new(
            4097, 1,
        )));
        assert!(prepare_bytes("wide.png", &wide).is_err());
        let path =
            std::env::temp_dir().join(format!("mininet-large-avatar-{}", std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_SOURCE_BYTES + 1).unwrap();
        drop(file);
        assert!(prepare(path.to_str().unwrap()).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
