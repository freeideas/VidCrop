//! A running log of everything that happens in the app, for debugging: every command (from the
//! page or the API) with its outcome, and every click, key and player event the page reports.
//! It lives in a temporary folder named for when the app started, which is deleted when the app
//! quits. Look at `events.jsonl` there while the app runs, or copy the folder to keep it.

use serde_json::{json, Value};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PREFIX: &str = "vidcrop-debug-";

pub struct DebugLog {
    dir: PathBuf,
    file: Mutex<File>,
    t0: Instant,
}

impl DebugLog {
    /// Creates `<temp>/vidcrop-debug-YYYYMMDD-HHMMSS-<pid>/events.jsonl`.
    pub fn create() -> Result<DebugLog, String> {
        let temp = std::env::temp_dir();
        remove_stale(&temp);
        let dir = temp.join(format!("{PREFIX}{}-{}", utc_stamp(SystemTime::now()), std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = File::create(dir.join("events.jsonl")).map_err(|e| e.to_string())?;
        let log = DebugLog { dir, file: Mutex::new(file), t0: Instant::now() };
        log.write("app", json!({ "event": "start", "pid": std::process::id(), "version": env!("CARGO_PKG_VERSION") }));
        Ok(log)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Appends one line: `{"t": seconds since start, "src": ..., ...fields}`.
    pub fn write(&self, src: &str, fields: Value) {
        let mut line = json!({ "t": (self.t0.elapsed().as_secs_f64() * 1000.0).round() / 1000.0, "src": src });
        if let (Some(l), Value::Object(f)) = (line.as_object_mut(), fields) {
            l.extend(f);
        }
        let mut f = self.file.lock().unwrap();
        let _ = writeln!(f, "{line}");
        let _ = f.flush();
    }

    /// Deletes the folder. Called when the app quits.
    pub fn remove(self) {
        drop(self.file);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Shortens long strings (thumbnail images, big states) so the log stays readable.
pub fn trim(v: &Value) -> Value {
    match v {
        Value::String(s) if s.len() > 300 => Value::String(format!("{}… ({} chars)", &s[..s.floor_char_boundary(200)], s.len())),
        Value::Array(a) if a.len() > 50 => json!(format!("[{} items]", a.len())),
        Value::Array(a) => Value::Array(a.iter().map(trim).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), trim(v))).collect()),
        _ => v.clone(),
    }
}

/// Folders left behind by a crash are kept a day for a look, then cleared out.
fn remove_stale(temp: &Path) {
    let Ok(entries) = std::fs::read_dir(temp) else { return };
    for e in entries.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok()).is_some_and(|age| age > Duration::from_secs(86400));
        if old && e.file_name().to_string_lossy().starts_with(PREFIX) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// `YYYYMMDD-HHMMSS` in UTC.
fn utc_stamp(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    // Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_utc() {
        assert_eq!(utc_stamp(UNIX_EPOCH + Duration::from_secs(1_790_506_517)), "20260927-105517");
        assert_eq!(utc_stamp(UNIX_EPOCH), "19700101-000000");
    }

    #[test]
    fn trims_big_values() {
        let v = trim(&json!({ "image": "x".repeat(1000), "n": 1 }));
        assert!(v["image"].as_str().unwrap().ends_with("(1000 chars)"));
        assert_eq!(v["n"], 1);
    }
}
