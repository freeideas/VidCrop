//! Finding and calling ffmpeg / ffprobe: probing files and making timeline thumbnails.

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Finds a tool: `VIDCROP_<NAME>` env var, next to our own executable (bundled sidecar),
/// the PATH, then common install folders (apps started from Finder don't get the shell PATH).
pub fn tool(name: &str) -> PathBuf {
    static CACHE: OnceLock<std::sync::Mutex<std::collections::HashMap<String, PathBuf>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(p) = cache.lock().unwrap().get(name) {
        return p.clone();
    }
    let found = find_tool(name);
    cache.lock().unwrap().insert(name.to_string(), found.clone());
    found
}

fn find_tool(name: &str) -> PathBuf {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    if let Ok(p) = std::env::var(format!("VIDCROP_{}", name.to_uppercase())) {
        return PathBuf::from(p);
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        dirs.push(d);
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        dirs.push(PathBuf::from(d));
    }
    dirs.into_iter().map(|d| d.join(&exe)).find(|p| p.is_file()).unwrap_or_else(|| PathBuf::from(exe))
}

pub fn command(name: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(tool(name));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashing up
    }
    c
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MediaInfo {
    pub path: String,
    pub duration: f64,
    /// Displayed size, after applying rotation metadata.
    pub width: u32,
    pub height: u32,
    pub rotation: i32,
    pub fps: f64,
    pub video_codec: String,
    pub has_audio: bool,
    pub size_bytes: u64,
}

pub fn probe(path: &str) -> Result<MediaInfo, String> {
    let out = command("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams", path])
        .output()
        .map_err(|e| format!("couldn't run ffprobe: {e}"))?;
    if !out.status.success() {
        return Err(format!("not a video ffprobe can read: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let j: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    parse_probe(path, &j)
}

fn parse_probe(path: &str, j: &Value) -> Result<MediaInfo, String> {
    let streams = j["streams"].as_array().cloned().unwrap_or_default();
    let v = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .ok_or("the file has no video stream")?;
    let num = |x: &Value| x.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| x.as_f64());
    let duration = num(&j["format"]["duration"]).or_else(|| num(&v["duration"])).ok_or("unknown duration")?;
    let rotation = v["side_data_list"]
        .as_array()
        .and_then(|l| l.iter().find_map(|d| d["rotation"].as_f64()))
        .or_else(|| num(&v["tags"]["rotate"]))
        .unwrap_or(0.0) as i32;
    let (mut width, mut height) = (v["width"].as_u64().unwrap_or(0) as u32, v["height"].as_u64().unwrap_or(0) as u32);
    if rotation.rem_euclid(180) == 90 {
        std::mem::swap(&mut width, &mut height);
    }
    let fps = ["avg_frame_rate", "r_frame_rate"]
        .iter()
        .filter_map(|k| {
            let (n, d) = v[*k].as_str()?.split_once('/')?;
            let (n, d) = (n.parse::<f64>().ok()?, d.parse::<f64>().ok()?);
            (n > 0.0 && d > 0.0).then(|| n / d)
        })
        .next()
        .unwrap_or(30.0);
    Ok(MediaInfo {
        path: path.to_string(),
        duration,
        width,
        height,
        rotation,
        fps,
        video_codec: v["codec_name"].as_str().unwrap_or("").to_string(),
        has_audio: streams.iter().any(|s| s["codec_type"] == "audio"),
        size_bytes: num(&j["format"]["size"]).unwrap_or(0.0) as u64,
    })
}

/// A horizontal strip of `count` thumbnails, `height` px tall, as a JPEG data URL.
pub fn thumbnail_strip(info: &MediaInfo, count: u32, height: u32) -> Result<String, String> {
    use base64::Engine;
    let count = count.clamp(1, 200);
    let fps = count as f64 / info.duration.max(0.1);
    let vf = format!("fps={fps:.6},scale=-2:{height},tile={count}x1");
    let out = command("ffmpeg")
        .args(["-v", "error", "-i", &info.path, "-an", "-vf", &vf, "-frames:v", "1", "-q:v", "5"])
        .args(["-f", "image2pipe", "-c:v", "mjpeg", "-"])
        .output()
        .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(format!("thumbnails failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(out.stdout)))
}

/// Names of the available ffmpeg encoders, read once.
pub fn has_encoder(name: &str) -> bool {
    static ENCODERS: OnceLock<String> = OnceLock::new();
    let list = ENCODERS.get_or_init(|| {
        command("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    });
    list.split_whitespace().any(|w| w == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rotated_phone_video() {
        let j = serde_json::json!({
            "format": {"duration": "12.5", "size": "1000"},
            "streams": [
                {"codec_type": "video", "codec_name": "hevc", "width": 1920, "height": 1080,
                 "avg_frame_rate": "30000/1001", "side_data_list": [{"rotation": -90}]},
                {"codec_type": "audio"}
            ]
        });
        let m = parse_probe("x.mov", &j).unwrap();
        assert_eq!((m.width, m.height, m.rotation, m.has_audio), (1080, 1920, -90, true));
        assert!((m.fps - 29.97).abs() < 0.01);
        assert_eq!(m.duration, 12.5);
    }
}
