//! In-app playback with no external process, browser or platform codec:
//! audio through pure-Rust decoders (`symphonia` via `rodio`: MP3, FLAC,
//! Ogg Vorbis, WAV, AAC/ALAC in MP4 containers) and animated GIF/WebP
//! through the `image` crate already used for photos.
//!
//! What is deliberately not here: H.264/VP9/AV1 video decoding. No
//! pure-Rust decoder for those exists, and this shell will not embed a
//! browser or launch another program to play a file. Video posts show a
//! poster card and can be exported from the Library; that limit is stated
//! in the UI rather than hidden behind a broken play button.
//!
//! The audio device is opened lazily on the first play and never on
//! launch.

use eframe::egui;
use image::codecs::gif::GifDecoder;
use image::codecs::webp::WebPDecoder;
use image::AnimationDecoder;
use mini_objects::ObjectId;
#[cfg(windows)]
use rodio::{Decoder, MixerDeviceSink, Player, Source};
use std::io::Cursor;
use std::time::{Duration, Instant};

/// Largest payload decoded for in-memory playback.
pub const MAX_AUDIO_BYTES: u64 = 96 * 1024 * 1024;
pub const MAX_ANIMATION_BYTES: u64 = 24 * 1024 * 1024;
/// Frames kept for one animation; longer clips are truncated.
pub const MAX_FRAMES: usize = 600;
const MAX_FRAME_EDGE: u32 = 720;

/// What kind of playback a content type gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    Audio,
    Animation,
    Image,
    /// Real video: shown as a poster; export to watch.
    VideoUnsupported,
    Other,
}

pub fn playback_for(content_type: &str) -> Playback {
    let ct = content_type.to_ascii_lowercase();
    if ct.starts_with("audio/") || ct == "application/ogg" {
        Playback::Audio
    } else if ct == "image/gif" || ct == "image/webp" {
        Playback::Animation
    } else if ct.starts_with("image/") {
        Playback::Image
    } else if ct.starts_with("video/") {
        Playback::VideoUnsupported
    } else {
        Playback::Other
    }
}

/// What is loaded in the audio player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlaying {
    pub media: ObjectId,
    pub title: String,
    pub author: String,
    pub duration: Option<Duration>,
}

#[cfg(windows)]
pub struct AudioPlayer {
    _device: MixerDeviceSink,
    player: Player,
    now: Option<NowPlaying>,
}

#[cfg(windows)]
impl AudioPlayer {
    /// Open the default output device. Called on first play only.
    pub fn open() -> Result<Self, String> {
        let device = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|error| format!("no audio output: {error}"))?;
        let player = Player::connect_new(device.mixer());
        Ok(Self {
            _device: device,
            player,
            now: None,
        })
    }

    /// Decode `bytes` and start playing, replacing whatever was loaded.
    pub fn play_bytes(
        &mut self,
        bytes: Vec<u8>,
        media: ObjectId,
        title: String,
        author: String,
    ) -> Result<(), String> {
        if bytes.len() as u64 > MAX_AUDIO_BYTES {
            return Err(format!(
                "audio larger than {} MB is not decoded in memory",
                MAX_AUDIO_BYTES / (1024 * 1024)
            ));
        }
        let source = Decoder::new(Cursor::new(bytes))
            .map_err(|error| format!("could not decode audio: {error}"))?;
        let duration = source.total_duration();
        self.player.stop();
        self.player.append(source);
        self.player.play();
        self.now = Some(NowPlaying {
            media,
            title,
            author,
            duration,
        });
        Ok(())
    }

    pub fn now(&self) -> Option<&NowPlaying> {
        self.now.as_ref()
    }

    pub fn is_playing(&self) -> bool {
        self.now.is_some() && !self.player.is_paused() && !self.player.empty()
    }

    pub fn toggle(&self) {
        if self.player.is_paused() {
            self.player.play();
        } else {
            self.player.pause();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.player.is_paused()
    }

    pub fn stop(&mut self) {
        self.player.stop();
        self.now = None;
    }

    pub fn position(&self) -> Duration {
        self.player.get_pos()
    }

    pub fn seek(&self, to: Duration) {
        let _ = self.player.try_seek(to);
    }

    pub fn volume(&self) -> f32 {
        self.player.volume()
    }

    pub fn set_volume(&self, volume: f32) {
        self.player.set_volume(volume.clamp(0.0, 1.5));
    }
}

/// On non-Windows targets audio output is not compiled in (see Cargo.toml).
#[cfg(not(windows))]
pub struct AudioPlayer {
    now: Option<NowPlaying>,
}

#[cfg(not(windows))]
impl AudioPlayer {
    pub fn open() -> Result<Self, String> {
        Err("audio playback is built for Windows in this client".into())
    }
    pub fn play_bytes(
        &mut self,
        _bytes: Vec<u8>,
        _media: ObjectId,
        _title: String,
        _author: String,
    ) -> Result<(), String> {
        Err("audio playback is built for Windows in this client".into())
    }
    pub fn now(&self) -> Option<&NowPlaying> {
        self.now.as_ref()
    }
    pub fn is_playing(&self) -> bool {
        false
    }
    pub fn toggle(&self) {}
    pub fn is_paused(&self) -> bool {
        true
    }
    pub fn stop(&mut self) {
        self.now = None;
    }
    pub fn position(&self) -> Duration {
        Duration::ZERO
    }
    pub fn seek(&self, _to: Duration) {}
    pub fn volume(&self) -> f32 {
        1.0
    }
    pub fn set_volume(&self, _volume: f32) {}
}

/// A decoded animation: frames as textures with their delays.
pub struct Animation {
    frames: Vec<(egui::TextureHandle, Duration)>,
    total: Duration,
    started: Instant,
    pub size: egui::Vec2,
}

impl Animation {
    /// Decode a GIF or WebP into textures, bounded in frame count and size.
    pub fn decode(
        ctx: &egui::Context,
        key: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_ANIMATION_BYTES {
            return Err("animation is too large to decode in memory".into());
        }
        let frames: Vec<image::Frame> = match content_type.to_ascii_lowercase().as_str() {
            "image/gif" => GifDecoder::new(Cursor::new(bytes))
                .map_err(|error| error.to_string())?
                .into_frames()
                .take(MAX_FRAMES)
                .collect::<Result<_, _>>()
                .map_err(|error| error.to_string())?,
            "image/webp" => WebPDecoder::new(Cursor::new(bytes))
                .map_err(|error| error.to_string())?
                .into_frames()
                .take(MAX_FRAMES)
                .collect::<Result<_, _>>()
                .map_err(|error| error.to_string())?,
            other => return Err(format!("{other} is not an animation")),
        };
        if frames.is_empty() {
            return Err("animation has no frames".into());
        }
        let mut textures = Vec::with_capacity(frames.len());
        let mut total = Duration::ZERO;
        let mut size = egui::Vec2::ZERO;
        for (index, frame) in frames.into_iter().enumerate() {
            let (num, den) = frame.delay().numer_denom_ms();
            let delay = Duration::from_millis(u64::from(num / den.max(1)).max(20));
            let mut rgba = frame.into_buffer();
            if rgba.width() > MAX_FRAME_EDGE || rgba.height() > MAX_FRAME_EDGE {
                rgba = image::imageops::thumbnail(
                    &rgba,
                    rgba.width().min(MAX_FRAME_EDGE),
                    rgba.height().min(MAX_FRAME_EDGE),
                );
            }
            let dims = [rgba.width() as usize, rgba.height() as usize];
            size = egui::vec2(dims[0] as f32, dims[1] as f32);
            let color = egui::ColorImage::from_rgba_unmultiplied(dims, rgba.as_raw());
            let texture = ctx.load_texture(
                format!("anim:{key}:{index}"),
                color,
                egui::TextureOptions::LINEAR,
            );
            total += delay;
            textures.push((texture, delay));
        }
        Ok(Self {
            frames: textures,
            total,
            started: Instant::now(),
            size,
        })
    }

    /// The frame to show now, looping.
    pub fn current(&self) -> &egui::TextureHandle {
        let elapsed = self.started.elapsed();
        let mut t = if self.total.is_zero() {
            Duration::ZERO
        } else {
            Duration::from_nanos((elapsed.as_nanos() % self.total.as_nanos()) as u64)
        };
        for (texture, delay) in &self.frames {
            if t < *delay {
                return texture;
            }
            t -= *delay;
        }
        &self.frames[self.frames.len() - 1].0
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
}

pub fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            (seconds % 3600) / 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_types_map_to_the_right_playback() {
        assert_eq!(playback_for("audio/mpeg"), Playback::Audio);
        assert_eq!(playback_for("audio/flac"), Playback::Audio);
        assert_eq!(playback_for("image/gif"), Playback::Animation);
        assert_eq!(playback_for("image/webp"), Playback::Animation);
        assert_eq!(playback_for("image/png"), Playback::Image);
        assert_eq!(playback_for("video/mp4"), Playback::VideoUnsupported);
        assert_eq!(playback_for("application/pdf"), Playback::Other);
        assert_eq!(format_duration(Duration::from_secs(65)), "1:05");
        assert_eq!(format_duration(Duration::from_secs(3725)), "1:02:05");
    }

    #[cfg(windows)]
    #[test]
    fn a_wav_decodes_without_an_output_device() {
        // 0.1 s of silence, 8 kHz mono 16-bit: a real decode path, no device.
        let samples = 800u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(samples * 2).to_le_bytes());
        wav.extend(std::iter::repeat_n(0u8, (samples * 2) as usize));
        let source = Decoder::new(Cursor::new(wav)).unwrap();
        assert_eq!(source.total_duration(), Some(Duration::from_millis(100)));
    }

    #[test]
    fn a_two_frame_gif_becomes_an_animation() {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame, RgbaImage};
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            for shade in [0u8, 255u8] {
                let img = RgbaImage::from_pixel(4, 4, image::Rgba([shade, shade, shade, 255]));
                encoder
                    .encode_frame(Frame::from_parts(
                        img,
                        0,
                        0,
                        Delay::from_numer_denom_ms(100, 1),
                    ))
                    .unwrap();
            }
        }
        let ctx = egui::Context::default();
        let animation = Animation::decode(&ctx, "t", "image/gif", &bytes).unwrap();
        assert_eq!(animation.frame_count(), 2);
        assert_eq!(animation.total, Duration::from_millis(200));
        assert_eq!(animation.size, egui::vec2(4.0, 4.0));
        assert!(Animation::decode(&ctx, "t", "image/png", &bytes).is_err());
    }
}
