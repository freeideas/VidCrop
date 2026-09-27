//! Screen recording: ScreenCaptureKit on macOS 15+ (record_mac.rs), ffmpeg capture devices elsewhere.
//! See specs/screen-recording.md.

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
    /// Also record the sound the computer plays (only where `can_record_system_audio`).
    #[serde(default)]
    pub system_audio: bool,
    pub fps: Option<u32>,
}

pub struct Recording {
    how: How,
    pub started: Instant,
}

enum How {
    Ffmpeg { child: Child, mkv: PathBuf },
    #[cfg(target_os = "macos")]
    Native(crate::record_mac::Native),
}

/// ScreenCaptureKit on macOS 15+; ffmpeg's capture devices everywhere else.
fn native() -> bool {
    #[cfg(target_os = "macos")]
    return crate::record_mac::available();
    #[allow(unreachable_code)]
    false
}

/// Whether `RecordOptions::system_audio` works here.
pub fn can_record_system_audio() -> bool {
    native()
}

pub fn list_sources() -> Result<Vec<Source>, String> {
    #[cfg(target_os = "macos")]
    if native() {
        return crate::record_mac::list_sources();
    }
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
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // Named like macOS names its own, since this becomes the saved file's name too.
    let stamp = chrono::Local::now().format("Screen recording %Y-%m-%d at %H.%M.%S");
    let fps = opts.fps.unwrap_or(30).clamp(1, 60);
    #[cfg(target_os = "macos")]
    if native() {
        let raw = dir.join(format!("{stamp}.mov"));
        let rec = crate::record_mac::start(opts, &raw, fps)?;
        return Ok(Recording { how: How::Native(rec), started: Instant::now() });
    }
    if opts.system_audio {
        return Err("recording the computer's own sound isn't supported on this system yet".into());
    }
    let sources = list_sources()?;
    let screen = match &opts.screen {
        Some(s) => s.clone(),
        None => sources.iter().find(|s| s.kind == "screen").map(|s| s.id.clone()).ok_or("no screen found to record")?,
    };
    let fps = fps.to_string();
    let mkv = dir.join(format!("{stamp}.mkv"));

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
    // If ffmpeg dies right away (bad device) or sits there writing nothing (macOS without
    // permission hangs rather than failing), stop it and say why.
    let t0 = Instant::now();
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            let out = child.wait_with_output().map_err(|e| e.to_string())?;
            return Err(permission_hint(&String::from_utf8_lossy(&out.stderr)));
        }
        if std::fs::metadata(&mkv).map(|m| m.len() > 0).unwrap_or(false) {
            break;
        }
        if t0.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let out = child.wait_with_output().map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&mkv);
            return Err(permission_hint(&String::from_utf8_lossy(&out.stderr)));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(Recording { how: How::Ffmpeg { child, mkv }, started: Instant::now() })
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
    pub fn stop(self) -> Result<PathBuf, String> {
        let raw = match self.how {
            #[cfg(target_os = "macos")]
            How::Native(rec) => rec.stop()?,
            How::Ffmpeg { mut child, mkv } => {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(b"q");
                    let _ = stdin.flush();
                }
                let t0 = Instant::now();
                while child.try_wait().map_err(|e| e.to_string())?.is_none() {
                    if t0.elapsed() > Duration::from_secs(10) {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if !mkv.is_file() {
                    return Err("the recording produced no file".into());
                }
                mkv
            }
        };
        finish(&raw, &crate::paths::desktop_dir())
    }
}

/// Rewraps the raw recording as an MP4 the web view can play (MKV isn't), without re-encoding
/// the picture. Mic and computer sound arrive as separate tracks; they're mixed into one,
/// since the editor only keeps the first audio track. The result goes in `out_dir` (the Desktop),
/// so recordings are never left in a hidden folder.
fn finish(raw: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    let stem = raw.file_stem().and_then(|s| s.to_str()).unwrap_or("Screen recording");
    let mp4 = (1..)
        .map(|n| out_dir.join(if n == 1 { format!("{stem}.mp4") } else { format!("{stem} {n}.mp4") }))
        .find(|p| !p.exists())
        .unwrap();
    let audio_tracks = ffmpeg::command("ffprobe")
        .args(["-v", "error", "-select_streams", "a", "-show_entries", "stream=index", "-of", "csv=p=0"])
        .arg(raw)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0);
    let mut cmd = ffmpeg::command("ffmpeg");
    cmd.args(["-hide_banner", "-y", "-v", "error", "-i"]).arg(raw);
    if audio_tracks > 1 {
        let inputs: String = (0..audio_tracks).map(|i| format!("[0:a:{i}]")).collect();
        cmd.args(["-filter_complex", &format!("{inputs}amix=inputs={audio_tracks}:duration=longest:normalize=0[a]")]);
        cmd.args(["-map", "0:v:0", "-map", "[a]", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k"]);
    } else {
        cmd.args(["-map", "0:v:0", "-map", "0:a:0?", "-c", "copy"]);
    }
    let out = cmd.args(["-movflags", "+faststart"]).arg(&mp4).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("couldn't finish the recording: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let _ = std::fs::remove_file(raw);
    Ok(mp4)
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

    /// Mic and computer sound come in as two tracks; the result must have exactly one. Needs ffmpeg.
    #[test]
    fn finish_mixes_audio_tracks() {
        let dir = std::env::temp_dir().join(format!("vidcrop-rec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("raw.mov");
        let ok = ffmpeg::command("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=30:duration=2"])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-f", "lavfi", "-i", "sine=frequency=880:duration=2"])
            .args(["-map", "0", "-map", "1", "-map", "2", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"])
            .arg(&raw)
            .status()
            .unwrap()
            .success();
        assert!(ok, "couldn't make the test clip");
        let mp4 = finish(&raw, &dir).unwrap();
        let out = ffmpeg::command("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0"])
            .arg(&mp4)
            .output()
            .unwrap();
        let kinds = String::from_utf8_lossy(&out.stdout);
        assert_eq!(kinds.split_whitespace().collect::<Vec<_>>(), ["video", "audio"]);
        assert!(!raw.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
