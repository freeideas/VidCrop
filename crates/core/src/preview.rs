//! Preview copies. Big videos (full-resolution screen recordings are 3456x2234 on a Retina Mac)
//! overwhelm the web view's decoder: after a seek it shows half-decoded green blotches or a
//! frozen frame. So the player shows a smaller copy made in the background: at most 1920x1080,
//! a keyframe every half second and no B-frames, so any moment decodes quickly. Crop and cuts
//! are stored in the original's pixels and seconds, and saving always reads the original, so the
//! copy only affects what's on screen. See specs/architecture.md.

use crate::export::Plan;
use crate::ffmpeg::MediaInfo;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Whether the player should get a preview copy instead of the original.
pub fn needed(info: &MediaInfo) -> bool {
    let playable = matches!(info.video_codec.as_str(), "h264" | "hevc");
    !playable || u64::from(info.width) * u64::from(info.height) > 1920 * 1088
}

/// `<cache>/previews/<hash of path, size and date>.mp4`, so an unchanged file reuses its copy.
pub fn path_for(info: &MediaInfo, cache: &Path) -> PathBuf {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    info.path.hash(&mut h);
    info.size_bytes.hash(&mut h);
    let modified = std::fs::metadata(&info.path).and_then(|m| m.modified()).ok();
    modified.hash(&mut h);
    cache.join("previews").join(format!("{:016x}.mp4", h.finish()))
}

pub fn plan(info: &MediaInfo, out: &Path) -> Plan {
    let partial = crate::export::partial_path(out).to_string_lossy().into_owned();
    let mut args: Vec<String> = ["-hide_banner", "-y", "-v", "error", "-i"].map(String::from).to_vec();
    args.push(info.path.clone());
    args.extend(["-map", "0:v:0", "-map", "0:a:0?"].map(String::from));
    args.extend(["-vf", "scale=w='min(1920,iw)':h='min(1080,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"].map(String::from));
    if cfg!(target_os = "macos") {
        args.extend(["-c:v", "h264_videotoolbox", "-b:v", "8M"].map(String::from));
    } else {
        args.extend(["-c:v", "libx264", "-preset", "veryfast", "-crf", "23"].map(String::from));
    }
    // A keyframe every half second and no B-frames: any moment can be shown after decoding a few frames.
    args.extend(["-bf", "0", "-g", "15", "-force_key_frames", "expr:gte(t,n_forced*0.5)", "-pix_fmt", "yuv420p"].map(String::from));
    args.extend(["-fps_mode", "passthrough", "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart"].map(String::from));
    args.extend(["-progress", "pipe:1", "-nostats"].map(String::from));
    args.push(partial);
    Plan { args, helper: None, kept_duration: info.duration }
}

/// Removes preview copies not used for a week.
pub fn clean_old(cache: &Path) {
    let Ok(entries) = std::fs::read_dir(cache.join("previews")) else { return };
    for e in entries.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok());
        if old.is_some_and(|age| age.as_secs() > 7 * 86400) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(w: u32, h: u32, codec: &str) -> MediaInfo {
        MediaInfo {
            path: "/v/in.mp4".into(),
            duration: 10.0,
            width: w,
            height: h,
            rotation: 0,
            fps: 30.0,
            video_codec: codec.into(),
            has_audio: true,
            size_bytes: 0,
        }
    }

    #[test]
    fn big_or_unusual_videos_get_a_preview() {
        assert!(!needed(&info(1920, 1080, "h264")));
        assert!(needed(&info(3456, 2234, "h264")));
        assert!(needed(&info(1280, 720, "prores")));
    }
}
