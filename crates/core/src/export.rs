//! Turns an `Edit` into an ffmpeg run. See specs/export.md.

use crate::edit::{Edit, Range};
use crate::ffmpeg::{self, MediaInfo};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Frame-accurate, re-encoded. Required when cropping.
    #[default]
    Exact,
    /// Lossless stream copy; cuts snap to keyframes. Time cuts only.
    Fast,
}

/// Everything needed to run ffmpeg: arguments plus any helper file it reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub args: Vec<String>,
    /// (path, contents) of a filter graph or concat list to write before running.
    pub helper: Option<(PathBuf, String)>,
    pub kept_duration: f64,
}

/// `<folder>/<name>-cropped.<ext>`, adding `-2`, `-3`... if taken.
pub fn default_output(source: &str, mode: Mode) -> PathBuf {
    let src = Path::new(source);
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
    let ext = match mode {
        Mode::Exact => "mp4".to_string(),
        Mode::Fast => src.extension().and_then(|s| s.to_str()).unwrap_or("mp4").to_lowercase(),
    };
    let dir = src.parent().unwrap_or(Path::new("."));
    (1..)
        .map(|n| {
            let suffix = if n == 1 { String::new() } else { format!("-{n}") };
            dir.join(format!("{stem}-cropped{suffix}.{ext}"))
        })
        .find(|p| !p.exists())
        .unwrap()
}

fn partial_path(out: &Path) -> PathBuf {
    let ext = out.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    out.with_extension(format!("part.{ext}"))
}

fn t(v: f64) -> String {
    format!("{v:.6}")
}

fn video_encoder_args() -> Vec<String> {
    let args: &[&str] = if cfg!(target_os = "macos") && ffmpeg::has_encoder("h264_videotoolbox") {
        &["-c:v", "h264_videotoolbox", "-q:v", "65", "-pix_fmt", "yuv420p"]
    } else {
        &["-c:v", "libx264", "-crf", "18", "-preset", "medium", "-pix_fmt", "yuv420p"]
    };
    args.iter().map(|s| s.to_string()).collect()
}

pub fn plan(edit: &Edit, info: &MediaInfo, out: &Path, mode: Mode, work_dir: &Path) -> Result<Plan, String> {
    let kept = edit.kept(info.duration);
    if kept.is_empty() {
        return Err("everything is deleted, there's nothing left to save".into());
    }
    let kept_duration = kept.iter().map(Range::len).sum();
    // No -nostdin: cancelling works by sending "q" on stdin.
    let mut args: Vec<String> = ["-hide_banner", "-y"].iter().map(|s| s.to_string()).collect();
    let partial = partial_path(out).to_string_lossy().into_owned();
    let tail = |args: &mut Vec<String>| {
        args.extend(["-progress", "pipe:1", "-nostats"].map(String::from));
        args.push(partial.clone());
    };

    match mode {
        Mode::Fast => {
            if edit.crop.is_some() {
                return Err("fast save can't crop; use exact save".into());
            }
            let mut list = String::from("ffconcat version 1.0\n");
            let src = edit.source.replace('\'', r"'\''");
            for r in &kept {
                list += &format!("file '{src}'\ninpoint {}\noutpoint {}\n", t(r.start), t(r.end));
            }
            let list_path = work_dir.join("concat.txt");
            args.extend(["-f", "concat", "-safe", "0", "-i"].map(String::from));
            args.push(list_path.to_string_lossy().into_owned());
            args.extend(["-map", "0:v:0", "-map", "0:a:0?", "-c", "copy", "-avoid_negative_ts", "make_zero"].map(String::from));
            tail(&mut args);
            return Ok(Plan { args, helper: Some((list_path, list)), kept_duration });
        }
        Mode::Exact => {}
    }

    let crop = edit.crop.map(|c| format!("crop={}:{}:{}:{}", c.w, c.h, c.x, c.y));
    let mut helper = None;
    if kept.len() == 1 {
        // One range: seek on the input instead of decoding from the start.
        let r = kept[0];
        if r.start > 0.0 {
            args.extend(["-ss".into(), t(r.start)]);
        }
        if r.end < info.duration {
            args.extend(["-t".into(), t(r.len())]);
        }
        args.extend(["-i".into(), edit.source.clone()]);
        if let Some(c) = &crop {
            args.extend(["-vf".into(), c.clone()]);
        }
        args.extend(["-map", "0:v:0", "-map", "0:a:0?"].map(String::from));
    } else {
        let mut g = String::new();
        let mut pads = String::new();
        for (i, r) in kept.iter().enumerate() {
            g += &format!("[0:v]trim=start={}:end={},setpts=PTS-STARTPTS[v{i}];\n", t(r.start), t(r.end));
            pads += &format!("[v{i}]");
            if info.has_audio {
                g += &format!("[0:a]atrim=start={}:end={},asetpts=PTS-STARTPTS[a{i}];\n", t(r.start), t(r.end));
                pads += &format!("[a{i}]");
            }
        }
        let a = if info.has_audio { 1 } else { 0 };
        let vc = if crop.is_some() { "[vc]" } else { "[vout]" };
        g += &format!("{pads}concat=n={}:v=1:a={a}{vc}{}", kept.len(), if a == 1 { "[aout]" } else { "" });
        if let Some(c) = &crop {
            g += &format!(";\n[vc]{c}[vout]");
        }
        g += "\n";
        let graph_path = work_dir.join("graph.txt");
        args.extend(["-i".into(), edit.source.clone()]);
        args.extend(["-/filter_complex".into(), graph_path.to_string_lossy().into_owned()]);
        args.extend(["-map", "[vout]"].map(String::from));
        if info.has_audio {
            args.extend(["-map", "[aout]"].map(String::from));
        }
        helper = Some((graph_path, g));
    }
    args.extend(video_encoder_args());
    if info.has_audio {
        args.extend(["-c:a", "aac", "-b:a", "192k"].map(String::from));
    }
    args.extend(["-fps_mode", "passthrough", "-movflags", "+faststart"].map(String::from));
    tail(&mut args);
    Ok(Plan { args, helper, kept_duration })
}

/// Runs a plan. `progress` gets 0.0..=1.0. Setting `cancel` stops ffmpeg and removes the partial file.
pub fn run(
    plan: &Plan,
    out: &Path,
    cancel: Arc<AtomicBool>,
    mut progress: impl FnMut(f64) + Send + 'static,
) -> Result<PathBuf, String> {
    if let Some((p, text)) = &plan.helper {
        std::fs::write(p, text).map_err(|e| format!("couldn't write {}: {e}", p.display()))?;
    }
    let partial = partial_path(out);
    let mut child = ffmpeg::command("ffmpeg")
        .args(&plan.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;

    let total = plan.kept_duration.max(0.001);
    let stdout = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<f64>().ok()) {
                progress((us / 1e6 / total).clamp(0.0, 1.0));
            } else if line.trim() == "progress=end" {
                progress(1.0);
            }
        }
    });
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let stderr = child.stderr.take().unwrap();
    let log2 = log.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let mut l = log2.lock().unwrap();
            l.push(line);
            if l.len() > 30 {
                l.remove(0);
            }
        }
    });

    let mut cancel_at: Option<Instant> = None;
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if cancel.load(Ordering::Relaxed) {
            match cancel_at {
                None => {
                    if let Some(stdin) = child.stdin.as_mut() {
                        let _ = stdin.write_all(b"q");
                        let _ = stdin.flush();
                    }
                    cancel_at = Some(Instant::now());
                }
                Some(t0) if t0.elapsed() > Duration::from_secs(2) => {
                    let _ = child.kill();
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    std::thread::sleep(Duration::from_millis(50)); // let the log thread catch up
    if let Some((p, _)) = &plan.helper {
        let _ = std::fs::remove_file(p);
    }
    if cancel.load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(&partial);
        return Err("cancelled".into());
    }
    if !status.success() {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("ffmpeg failed:\n{}", log.lock().unwrap().join("\n")));
    }
    std::fs::rename(&partial, out).map_err(|e| format!("couldn't move the finished file into place: {e}"))?;
    Ok(out.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Rect;

    fn info(has_audio: bool) -> MediaInfo {
        MediaInfo {
            path: "/v/in.mp4".into(),
            duration: 10.0,
            width: 1920,
            height: 1080,
            rotation: 0,
            fps: 30.0,
            video_codec: "h264".into(),
            has_audio,
            size_bytes: 0,
        }
    }

    #[test]
    fn single_range_seeks_on_input() {
        let mut e = Edit::new("/v/in.mp4");
        e.delete_range(0.0, 2.0, 10.0);
        e.set_crop(Some(Rect { x: 10, y: 10, w: 100, h: 100 }), 1920, 1080);
        let p = plan(&e, &info(true), Path::new("/v/out.mp4"), Mode::Exact, Path::new("/tmp")).unwrap();
        let a = p.args.join(" ");
        assert!(a.contains("-ss 2.000000 -i /v/in.mp4 -vf crop=100:100:10:10"), "{a}");
        assert!(p.helper.is_none());
        assert_eq!(p.kept_duration, 8.0);
        assert!(a.ends_with("/v/out.part.mp4"));
    }

    #[test]
    fn many_ranges_use_graph() {
        let mut e = Edit::new("/v/in.mp4");
        e.delete_range(2.0, 3.0, 10.0);
        e.delete_range(5.0, 6.0, 10.0);
        let p = plan(&e, &info(false), Path::new("/v/out.mp4"), Mode::Exact, Path::new("/tmp")).unwrap();
        let (_, g) = p.helper.unwrap();
        assert!(g.contains("[v0][v1][v2]concat=n=3:v=1:a=0[vout]"), "{g}");
        assert!(!g.contains("atrim"));
    }

    #[test]
    fn fast_mode_refuses_crop_and_writes_concat_list() {
        let mut e = Edit::new("/v/it's.mov");
        e.delete_range(2.0, 3.0, 10.0);
        let p = plan(&e, &info(true), Path::new("/v/out.mov"), Mode::Fast, Path::new("/tmp")).unwrap();
        let (_, list) = p.helper.unwrap();
        assert!(list.contains(r"file '/v/it'\''s.mov'"), "{list}");
        e.set_crop(Some(Rect { x: 10, y: 10, w: 100, h: 100 }), 1920, 1080);
        assert!(plan(&e, &info(true), Path::new("/v/o.mov"), Mode::Fast, Path::new("/tmp")).is_err());
    }
}
