//! A screen recording lives in the cache until saved. Saving it untouched, with no path given,
//! copies it to the Desktop under its own name. Needs ffmpeg installed.
//! (Its own file because it sets process-wide env vars.)

use serde_json::json;
use vidcrop_core::ffmpeg;
use vidcrop_core::session::{Core, Headless};

#[test]
fn unedited_recording_saves_to_desktop() {
    let dir = std::env::temp_dir().join(format!("vidcrop-recsave-{}", std::process::id()));
    let (cache, desktop) = (dir.join("cache"), dir.join("Desktop"));
    std::fs::create_dir_all(cache.join("recordings")).unwrap();
    std::fs::create_dir_all(&desktop).unwrap();
    std::env::set_var("VIDCROP_CACHE_DIR", &cache);
    std::env::set_var("VIDCROP_DESKTOP_DIR", &desktop);

    let rec = cache.join("recordings/Screen recording 2026-09-27 at 12.00.00.mp4");
    let ok = ffmpeg::command("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=30:duration=2"])
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&rec)
        .status()
        .unwrap()
        .success();
    assert!(ok, "couldn't make the test clip");

    let core = Core::new(Box::new(Headless));
    let run = |v: serde_json::Value| core.exec_json(v).unwrap();
    run(json!({ "cmd": "open", "path": rec }));
    assert_eq!(run(json!({ "cmd": "state" }))["from_recording"], true);
    let job = run(json!({ "cmd": "export" }));
    let done = run(json!({ "cmd": "wait", "job": job["job"] }));
    assert_eq!(done["status"], "done", "{done}");

    let out = desktop.join("Screen recording 2026-09-27 at 12.00.00.mp4");
    assert_eq!(job["output"], json!(out.to_string_lossy()));
    let info = ffmpeg::probe(out.to_str().unwrap()).unwrap();
    assert_eq!((info.width, info.height), (320, 240));
    let _ = std::fs::remove_dir_all(&dir);
}
