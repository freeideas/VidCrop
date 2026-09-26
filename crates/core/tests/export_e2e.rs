//! End to end: make a real test clip with ffmpeg, drive `Core` with the same JSON the API
//! takes, and check the saved file with ffprobe. Needs ffmpeg installed.

use serde_json::json;
use std::path::PathBuf;
use vidcrop_core::ffmpeg;
use vidcrop_core::session::{Core, Headless};

fn test_clip(dir: &PathBuf) -> PathBuf {
    let p = dir.join("in.mp4");
    let ok = ffmpeg::command("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30:duration=6"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=6", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .args(["-c:a", "aac", "-shortest"])
        .arg(&p)
        .status()
        .unwrap()
        .success();
    assert!(ok, "couldn't make the test clip");
    p
}

#[test]
fn crop_cut_save_through_commands() {
    let dir = std::env::temp_dir().join(format!("vidcrop-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("VIDCROP_CACHE_DIR", dir.join("cache"));
    let src = test_clip(&dir);
    let out = dir.join("out.mp4");

    let core = Core::new(Box::new(Headless));
    let run = |v: serde_json::Value| core.exec_json(v).unwrap();
    run(json!({ "cmd": "open", "path": src }));
    run(json!({ "cmd": "set_crop", "rect": { "x": 101, "y": 50, "w": 321, "h": 200 } }));
    run(json!({ "cmd": "delete_range", "start": 1.0, "end": 2.0 }));
    run(json!({ "cmd": "delete_range", "start": 4.0, "end": 5.0 }));
    run(json!({ "cmd": "delete_range", "start": 0.0, "end": 0.5 }));
    run(json!({ "cmd": "undo" }));
    assert!(core.exec_json(json!({ "cmd": "play" })).is_err(), "headless has no player");

    let st = run(json!({ "cmd": "state" }));
    assert_eq!(st["output_size"], json!({ "w": 320, "h": 200 }));
    assert_eq!(st["kept_duration"], json!(4.0));

    let job = run(json!({ "cmd": "export", "output": out }));
    let done = run(json!({ "cmd": "wait", "job": job["job"] }));
    assert_eq!(done["status"], "done", "{done}");

    let info = ffmpeg::probe(out.to_str().unwrap()).unwrap();
    assert_eq!((info.width, info.height, info.has_audio), (320, 200, true));
    assert!((info.duration - 4.0).abs() < 0.1, "duration {}", info.duration);
    let _ = std::fs::remove_dir_all(&dir);
}
