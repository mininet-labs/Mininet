//! In-process H.264 video: MP4 demuxing (`mp4`) and decoding (Cisco
//! OpenH264, built from source) on a worker thread, frames handed to the UI
//! as RGB with presentation timestamps. Audio for the same file plays
//! through the ordinary audio player, and its clock paces the video.
//!
//! Covered: H.264 (AVC) without B-frames in MP4/M4V/MOV with AAC audio.
//! Not covered and stated in the UI: H.264 with B-frames (OpenH264's
//! decoder does not support them — many phone recordings and x264
//! defaults use them; `ffmpeg -bf 0` re-encodes), H.265, VP9, AV1, and
//! WebM/MKV containers. No browser, no external program, no OS codec.

use eframe::egui;
use mp4::{MediaType, Mp4Reader, TrackType};
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Largest file decoded in memory.
pub const MAX_VIDEO_BYTES: u64 = 512 * 1024 * 1024;
/// Frames decoded ahead of the clock before the worker waits.
const LOOKAHEAD: usize = 6;
const MAX_EDGE: usize = 1920;

/// One decoded picture.
pub struct Frame {
    pub pts: Duration,
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

/// What the file contains, learned before decoding starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub width: u16,
    pub height: u16,
    pub duration: Duration,
    pub has_aac_audio: bool,
    pub video_codec: String,
}

pub enum Event {
    Probed(Probe),
    Frame(Frame),
    Ended,
    Failed(String),
}

/// A running decode.
pub struct VideoPlayer {
    rx: Receiver<Event>,
    stop: Arc<AtomicBool>,
    pub probe: Option<Probe>,
    pending: Option<Frame>,
    texture: Option<egui::TextureHandle>,
    pub shown_pts: Duration,
    started: Instant,
    pub ended: bool,
    pub error: Option<String>,
    pub media: mini_objects::ObjectId,
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Convert one AVCC sample (length-prefixed NALs) to Annex B.
fn avcc_to_annex_b(sample: &[u8], length_size: usize, out: &mut Vec<u8>) -> Result<(), String> {
    out.clear();
    let mut pos = 0;
    while pos + length_size <= sample.len() {
        let mut len = 0usize;
        for byte in &sample[pos..pos + length_size] {
            len = (len << 8) | usize::from(*byte);
        }
        pos += length_size;
        let end = pos.checked_add(len).ok_or("NAL length overflow")?;
        let nal = sample.get(pos..end).ok_or("NAL runs past the sample")?;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(nal);
        pos = end;
    }
    Ok(())
}

fn decode_thread(bytes: Vec<u8>, tx: SyncSender<Event>, stop: Arc<AtomicBool>) {
    let result = (|| -> Result<(), String> {
        let size = bytes.len() as u64;
        let mut reader = Mp4Reader::read_header(Cursor::new(bytes), size)
            .map_err(|error| format!("not a readable MP4: {error}"))?;
        let (track_id, sps, pps, length_size, width, height, timescale) = {
            let track = reader
                .tracks()
                .values()
                .find(|track| track.track_type().ok() == Some(TrackType::Video))
                .ok_or("the file has no video track")?;
            let media = track.media_type().map_err(|error| error.to_string())?;
            if media != MediaType::H264 {
                return Err(format!(
                    "{media} video is not decodable in-app (only H.264/AVC is)"
                ));
            }
            let has_b_frames = track.trak.mdia.minf.stbl.ctts.is_some();
            let avcc = track
                .trak
                .mdia
                .minf
                .stbl
                .stsd
                .avc1
                .as_ref()
                .map(|avc1| &avc1.avcc)
                .ok_or("H.264 track has no decoder configuration")?;
            if has_b_frames {
                return Err(
                    "this H.264 file uses B-frames, which the in-app decoder (OpenH264) does not support; re-encode with B-frames off (ffmpeg -bf 0) or export to watch"
                        .into(),
                );
            }
            (
                track.track_id(),
                track
                    .sequence_parameter_set()
                    .map_err(|error| error.to_string())?
                    .to_vec(),
                track
                    .picture_parameter_set()
                    .map_err(|error| error.to_string())?
                    .to_vec(),
                usize::from(avcc.length_size_minus_one & 0x03) + 1,
                track.width(),
                track.height(),
                track.timescale(),
            )
        };
        let has_aac_audio = reader.tracks().values().any(|track| {
            track.track_type().ok() == Some(TrackType::Audio)
                && track.media_type().ok() == Some(MediaType::AAC)
        });
        let duration = reader.duration();
        if usize::from(width) > MAX_EDGE || usize::from(height) > MAX_EDGE {
            return Err(format!(
                "{width}x{height} exceeds the {MAX_EDGE} px in-app limit"
            ));
        }
        tx.send(Event::Probed(Probe {
            width,
            height,
            duration,
            has_aac_audio,
            video_codec: "H.264".into(),
        }))
        .map_err(|_| "player closed".to_string())?;

        let mut decoder = Decoder::new().map_err(|error| format!("decoder: {error}"))?;
        let mut headers = Vec::new();
        headers.extend_from_slice(&[0, 0, 0, 1]);
        headers.extend_from_slice(&sps);
        headers.extend_from_slice(&[0, 0, 0, 1]);
        headers.extend_from_slice(&pps);
        let _ = decoder
            .decode(&headers)
            .map_err(|error| format!("headers: {error}"))?;

        let count = reader
            .sample_count(track_id)
            .map_err(|error| error.to_string())?;
        let mut annex_b = Vec::new();
        for sample_id in 1..=count {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            let Some(sample) = reader
                .read_sample(track_id, sample_id)
                .map_err(|error| error.to_string())?
            else {
                break;
            };
            avcc_to_annex_b(&sample.bytes, length_size, &mut annex_b)?;
            let pts_ticks =
                (sample.start_time as i64 + i64::from(sample.rendering_offset)).max(0) as u64;
            let pts = Duration::from_secs_f64(pts_ticks as f64 / f64::from(timescale.max(1)));
            let decoded = match decoder.decode(&annex_b) {
                Ok(Some(yuv)) => yuv,
                Ok(None) => continue,
                Err(error) => {
                    #[cfg(test)]
                    eprintln!("sample {sample_id}: {error}");
                    let _ = error;
                    continue;
                }
            };
            let (w, h) = decoded.dimensions();
            let mut rgb = vec![0u8; w * h * 3];
            decoded.write_rgb8(&mut rgb);
            let frame = Frame {
                pts,
                width: w,
                height: h,
                rgb,
            };
            // Bounded: block until the UI has consumed older frames.
            let mut event = Event::Frame(frame);
            loop {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                match tx.try_send(event) {
                    Ok(()) => break,
                    Err(TrySendError::Full(back)) => {
                        event = back;
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(TrySendError::Disconnected(_)) => return Ok(()),
                }
            }
        }
        Ok(())
    })();
    let _ = match result {
        Ok(()) => tx.send(Event::Ended),
        Err(error) => tx.send(Event::Failed(error)),
    };
}

/// Decode only the first picture of an H.264 MP4 as an RGB poster. The
/// first sample is a keyframe, so this works for files whose later
/// B-frames the decoder cannot handle.
pub fn poster(bytes: &[u8], max_edge: usize) -> Result<Frame, String> {
    let size = bytes.len() as u64;
    let mut reader = Mp4Reader::read_header(Cursor::new(bytes), size)
        .map_err(|error| format!("not a readable MP4: {error}"))?;
    let (track_id, sps, pps, length_size) = {
        let track = reader
            .tracks()
            .values()
            .find(|track| track.track_type().ok() == Some(TrackType::Video))
            .ok_or("no video track")?;
        if track.media_type().map_err(|error| error.to_string())? != MediaType::H264 {
            return Err("not H.264".into());
        }
        let avcc = track
            .trak
            .mdia
            .minf
            .stbl
            .stsd
            .avc1
            .as_ref()
            .map(|avc1| &avc1.avcc)
            .ok_or("no decoder configuration")?;
        (
            track.track_id(),
            track
                .sequence_parameter_set()
                .map_err(|error| error.to_string())?
                .to_vec(),
            track
                .picture_parameter_set()
                .map_err(|error| error.to_string())?
                .to_vec(),
            usize::from(avcc.length_size_minus_one & 0x03) + 1,
        )
    };
    let mut decoder = Decoder::new().map_err(|error| format!("decoder: {error}"))?;
    let mut headers = vec![0, 0, 0, 1];
    headers.extend_from_slice(&sps);
    headers.extend_from_slice(&[0, 0, 0, 1]);
    headers.extend_from_slice(&pps);
    let _ = decoder.decode(&headers);
    let count = reader
        .sample_count(track_id)
        .map_err(|error| error.to_string())?;
    let mut annex_b = Vec::new();
    for sample_id in 1..=count.min(30) {
        let Some(sample) = reader
            .read_sample(track_id, sample_id)
            .map_err(|error| error.to_string())?
        else {
            break;
        };
        avcc_to_annex_b(&sample.bytes, length_size, &mut annex_b)?;
        if let Ok(Some(yuv)) = decoder.decode(&annex_b) {
            let (w, h) = yuv.dimensions();
            let mut rgb = vec![0u8; w * h * 3];
            yuv.write_rgb8(&mut rgb);
            let image = image::RgbImage::from_raw(w as u32, h as u32, rgb)
                .ok_or("frame buffer size mismatch")?;
            let scale = (max_edge as f32 / w.max(h) as f32).min(1.0);
            let thumb = image::imageops::thumbnail(
                &image,
                ((w as f32 * scale) as u32).max(1),
                ((h as f32 * scale) as u32).max(1),
            );
            return Ok(Frame {
                pts: Duration::ZERO,
                width: thumb.width() as usize,
                height: thumb.height() as usize,
                rgb: thumb.into_raw(),
            });
        }
    }
    Err("no decodable picture in the first samples".into())
}

impl VideoPlayer {
    /// Start decoding `bytes` on a worker. Nothing is shown until the first
    /// frame arrives.
    pub fn start(media: mini_objects::ObjectId, bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_VIDEO_BYTES {
            return Err(format!(
                "video larger than {} MB is not decoded in memory",
                MAX_VIDEO_BYTES / (1024 * 1024)
            ));
        }
        let (tx, rx) = sync_channel(LOOKAHEAD);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        std::thread::spawn(move || decode_thread(bytes, tx, worker_stop));
        Ok(Self {
            rx,
            stop,
            probe: None,
            pending: None,
            texture: None,
            shown_pts: Duration::ZERO,
            started: Instant::now(),
            ended: false,
            error: None,
            media,
        })
    }

    /// Advance to `clock` (audio position when audio plays, else wall
    /// time) and return the texture to draw. Uploads at most one frame per
    /// call; frames older than the clock are dropped to keep up.
    pub fn frame_at(
        &mut self,
        ctx: &egui::Context,
        clock: Option<Duration>,
    ) -> Option<&egui::TextureHandle> {
        let clock = clock.unwrap_or_else(|| self.started.elapsed());
        let mut latest: Option<Frame> = None;
        loop {
            let candidate = match self.pending.take() {
                Some(frame) => frame,
                None => match self.rx.try_recv() {
                    Ok(Event::Frame(frame)) => frame,
                    Ok(Event::Probed(probe)) => {
                        self.probe = Some(probe);
                        continue;
                    }
                    Ok(Event::Ended) => {
                        self.ended = true;
                        break;
                    }
                    Ok(Event::Failed(error)) => {
                        self.error = Some(error);
                        self.ended = true;
                        break;
                    }
                    Err(_) => break,
                },
            };
            if candidate.pts <= clock {
                latest = Some(candidate);
            } else {
                self.pending = Some(candidate);
                break;
            }
        }
        if let Some(frame) = latest {
            let image = egui::ColorImage::from_rgb([frame.width, frame.height], &frame.rgb);
            match self.texture.as_mut() {
                Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
                None => {
                    self.texture = Some(ctx.load_texture(
                        format!("video:{}", self.media.as_str()),
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
            self.shown_pts = frame.pts;
        }
        self.texture.as_ref()
    }

    pub fn restart_clock(&mut self) {
        self.started = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avcc_samples_become_annex_b() {
        let sample = [0, 0, 0, 2, 0x65, 0xAA, 0, 0, 0, 1, 0x41];
        let mut out = Vec::new();
        avcc_to_annex_b(&sample, 4, &mut out).unwrap();
        assert_eq!(out, vec![0, 0, 0, 1, 0x65, 0xAA, 0, 0, 0, 1, 0x41]);
        assert!(avcc_to_annex_b(&[0, 0, 0, 9, 1], 4, &mut out).is_err());
        let two = [0, 1, 0x67];
        avcc_to_annex_b(&two, 2, &mut out).unwrap();
        assert_eq!(out, vec![0, 0, 0, 1, 0x67]);
    }

    #[test]
    fn garbage_is_reported_not_panicked() {
        let mut root = did_mini::Controller::incept_single_from_seeds(&[1; 32], &[2; 32]).unwrap();
        let device =
            did_mini::Controller::incept_device_single_from_seeds(&root.did(), &[3; 32], &[4; 32])
                .unwrap();
        root.delegate_device(&device.did(), did_mini::Capabilities::primary())
            .unwrap();
        let media = mini_objects::ObjectBuilder::new(mini_objects::ObjectType::POST)
            .payload(mini_objects::Payload::Public(b"x".to_vec()))
            .sign(&root.did(), &device)
            .unwrap()
            .id()
            .clone();
        let player = VideoPlayer::start(media, vec![0u8; 64]).unwrap();
        let ctx = egui::Context::default();
        let mut player = player;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !player.ended && Instant::now() < deadline {
            let _ = player.frame_at(&ctx, None);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(player.ended);
        assert!(player.error.is_some());
    }
}

#[cfg(test)]
mod clip_tests {
    use super::*;

    /// Runs only when a local H.264 sample exists; prints what the decoder
    /// does with it.
    #[test]
    fn local_clip_decodes_frames() {
        let Ok(bytes) = std::fs::read("C:/dev/clip720.mp4") else {
            return;
        };
        let (tx, rx) = sync_channel(4);
        let stop = Arc::new(AtomicBool::new(false));
        std::thread::spawn(move || decode_thread(bytes, tx, stop));
        let mut frames = 0;
        let mut first = None;
        loop {
            match rx.recv_timeout(Duration::from_secs(20)) {
                Ok(Event::Frame(frame)) => {
                    if first.is_none() {
                        first = Some((frame.width, frame.height, frame.pts));
                    }
                    frames += 1;
                }
                Ok(Event::Probed(probe)) => eprintln!("probe {probe:?}"),
                Ok(Event::Ended) => break,
                Ok(Event::Failed(error)) => panic!("failed: {error}"),
                Err(error) => panic!("timeout: {error}"),
            }
        }
        eprintln!("frames {frames} first {first:?}");
        assert!(frames > 200, "expected ~300 frames, got {frames}");
        let bytes = std::fs::read("C:/dev/clip720.mp4").unwrap();
        let poster = poster(&bytes, 320).unwrap();
        assert_eq!(poster.width, 320);
        assert_eq!(poster.height, 180);
    }
}
