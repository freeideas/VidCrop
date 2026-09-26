//! Screen recording through ffmpeg's capture devices. See specs/screen-recording.md.

use crate::ffmpeg;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Source {
    /// What to pass back in `RecordOptions`.
    pub id: String,
    pub name: String,
    /// "screen" or "mic".
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecordOptions {
    /// A screen `Source::id`; the first screen when missing.
    pub screen: Option<String>,
    /// A mic `Source::id`; no sound when missing.
    pub mic: Option<String>,
    pub fps: Option<u32>,
}

pub struct Recording {
    child: Child,
    mkv: PathBuf,
    pub started: Instant,
}

pub fn list_sources() -> Result<Vec<Source>, String> {
    if cfg!(target_os = "macos") {
        let out = ffmpeg::command("ffmpeg")
            .args(["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""])
            .output()
            .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
        Ok(parse_avfoundation(&String::from_utf8_lossy(&out.stderr)))
    } else if cfg!(windows) {
        Ok(vec![Source { id: "0".into(), name: "Screen 1".into(), kind: "screen".into() }])
    } else {
        let display = std::env::var("DISPLAY").map_err(|_| "no X11 display (Wayland isn't supported yet)")?;
        Ok(vec![
            Source { id: display, name: "Screen".into(), kind: "screen".into() },
            Source { id: "default".into(), name: "Default microphone".into(), kind: "mic".into() },
        ])
    }
}

fn parse_avfoundation(text: &str) -> Vec<Source> {
    let mut out = Vec::new();
    let mut audio = false;
    for line in text.lines() {
        if line.contains("AVFoundation video devices") {
            audio = false;
        } else if line.contains("AVFoundation audio devices") {
            audio = true;
        } else if let Some(rest) = line.split("] [").nth(1) {
            let Some((idx, name)) = rest.split_once("] ") else { continue };
            let name = name.trim().to_string();
            if audio {
                out.push(Source { id: idx.into(), name, kind: "mic".into() });
            } else if name.starts_with("Capture screen") {
                out.push(Source { id: idx.into(), name, kind: "screen".into() });
            }
        }
    }
    out
}

pub fn start(opts: &RecordOptions, dir: &Path) -> Result<Recording, String> {
    let sources = list_sources()?;
    let screen = match &opts.screen {
        Some(s) => s.clone(),
        None => sources.iter().find(|s| s.kind == "screen").map(|s| s.id.clone()).ok_or("no screen found to record")?,
    };
    let fps = opts.fps.unwrap_or(30).clamp(1, 60).to_string();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let mkv = dir.join(format!("recording-{stamp}.mkv"));

    let mut args: Vec<String> = vec!["-hide_banner".into(), "-y".into()];
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    if cfg!(target_os = "macos") {
        let input = match &opts.mic {
            Some(m) => format!("{screen}:{m}"),
            None => format!("{screen}:none"),
        };
        args.extend(s(&["-thread_queue_size", "1024", "-f", "avfoundation", "-capture_cursor", "1"]));
        args.extend(["-framerate".into(), fps.clone(), "-i".into(), input]);
        args.extend(s(&["-c:v", "h264_videotoolbox", "-b:v", "20M", "-pix_fmt", "yuv420p"]));
    } else if cfg!(windows) {
        args.extend(["-f".into(), "lavfi".into(), "-i".into()]);
        args.push(format!("ddagrab=output_idx={screen}:framerate={fps},hwdownload,format=bgra"));
        if let Some(m) = &opts.mic {
            args.extend(["-f".into(), "dshow".into(), "-i".into(), format!("audio={m}")]);
        }
        args.extend(s(&["-c:v", "libx264", "-preset", "ultrafast", "-crf", "18", "-pix_fmt", "yuv420p"]));
    } else {
        args.extend(["-f".into(), "x11grab".into(), "-framerate".into(), fps.clone(), "-i".into(), screen]);
        if let Some(m) = &opts.mic {
            args.extend(["-f".into(), "pulse".into(), "-i".into(), m.clone()]);
        }
        args.extend(s(&["-c:v", "libx264", "-preset", "ultrafast", "-crf", "18", "-pix_fmt", "yuv420p"]));
    }
    if opts.mic.is_some() {
        args.extend(s(&["-c:a", "aac", "-b:a", "160k"]));
    }
    args.push(mkv.to_string_lossy().into_owned());

    let mut child = ffmpeg::command("ffmpeg")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
    // If ffmpeg dies right away (no permission, bad device), report why.
    std::thread::sleep(Duration::from_millis(700));
    if let Ok(Some(_)) = child.try_wait() {
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        return Err(permission_hint(&String::from_utf8_lossy(&out.stderr)));
    }
    Ok(Recording { child, mkv, started: Instant::now() })
}

fn permission_hint(stderr: &str) -> String {
    let tail: Vec<&str> = stderr.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect();
    let mut msg = format!("screen recording didn't start:\n{}", tail.join("\n"));
    if cfg!(target_os = "macos") {
        msg += "\n\nmacOS may be blocking it: allow VidCrop in System Settings > Privacy & Security > Screen Recording, then restart VidCrop.";
    }
    msg
}

impl Recording {
    /// Stops cleanly and returns an MP4 the editor can play.
    pub fn stop(mut self) -> Result<PathBuf, String> {
        if let Some(stdin) = self.child.stdin.as_mut() {
            let _ = stdin.write_all(b"q");
            let _ = stdin.flush();
        }
        let t0 = Instant::now();
        loop {
            if self.child.try_wait().map_err(|e| e.to_string())?.is_some() {
                break;
            }
            if t0.elapsed() > Duration::from_secs(10) {
                let _ = self.child.kill();
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if !self.mkv.is_file() {
            return Err("the recording produced no file".into());
        }
        // Web views can't play MKV: rewrap as MP4 without re-encoding.
        let mp4 = self.mkv.with_extension("mp4");
        let out = ffmpeg::command("ffmpeg")
            .args(["-hide_banner", "-y", "-v", "error", "-i"])
            .arg(&self.mkv)
            .args(["-c", "copy", "-movflags", "+faststart"])
            .arg(&mp4)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("couldn't finish the recording: {}", String::from_utf8_lossy(&out.stderr)));
        }
        let _ = std::fs::remove_file(&self.mkv);
        Ok(mp4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_device_list() {
        let text = "[AVFoundation indev @ 0x1] AVFoundation video devices:\n\
            [AVFoundation indev @ 0x1] [0] FaceTime HD Camera\n\
            [AVFoundation indev @ 0x1] [1] Capture screen 0\n\
            [AVFoundation indev @ 0x1] AVFoundation audio devices:\n\
            [AVFoundation indev @ 0x1] [0] MacBook Pro Microphone\n";
        let s = parse_avfoundation(text);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].id.as_str(), s[0].kind.as_str()), ("1", "screen"));
        assert_eq!((s[1].id.as_str(), s[1].name.as_str()), ("0", "MacBook Pro Microphone"));
    }
}
