//! The single source of truth. The UI and the HTTP API both drive the app by sending
//! `Command`s to `Core::exec`, and both see the same `state()`.

use crate::edit::{Edit, Range, Rect};
use crate::debuglog::{self, DebugLog};
use crate::export::{self, Mode};
use crate::ffmpeg::{self, MediaInfo};
use crate::record::{self, RecordOptions, Recording};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Returns the full state.
    State,
    Open { path: String },
    Close,
    /// `rect: null` removes the crop.
    SetCrop { rect: Option<Rect> },
    DeleteRange { start: f64, end: f64 },
    RestoreRange { start: f64, end: f64 },
    /// Removes the crop and all cuts (undoable).
    Reset,
    Undo,
    Redo,
    /// Returns `{ "image": "data:image/jpeg;base64,..." }`.
    Thumbnails { count: Option<u32>, height: Option<u32> },
    /// Starts a save job; returns `{ "job": id, "output": path }`.
    Export { output: Option<String>, mode: Option<Mode> },
    Cancel { job: u64 },
    /// Blocks until the job finishes (or `timeout` seconds pass); returns the job.
    Wait { job: u64, timeout: Option<f64> },
    Sources,
    RecordStart {
        #[serde(flatten)]
        options: RecordOptions,
    },
    /// Stops recording and opens the result in the editor.
    RecordStop,
    // Player controls, carried out by the UI. Fail when there's no UI (headless).
    Play,
    Pause,
    Seek { time: f64 },
    Select { start: f64, end: f64 },
    ClearSelection,
    /// The UI reports its player state here (playhead, playing, selection...), shown in `state().ui`.
    UiReport { ui: Value },
}

impl Command {
    fn is_ui(&self) -> bool {
        matches!(self, Command::Play | Command::Pause | Command::Seek { .. } | Command::Select { .. } | Command::ClearSelection)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: u64,
    pub kind: String,
    /// "running", "done", "failed" or "cancelled".
    pub status: String,
    pub progress: f64,
    pub output: String,
    pub error: Option<String>,
    #[serde(skip)]
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct Session {
    info: Option<MediaInfo>,
    edit: Option<Edit>,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    jobs: BTreeMap<u64, Job>,
    next_job: u64,
    recording: Option<Recording>,
    ui: Value,
    /// The debug log folder, while there is one.
    debug_log: Option<String>,
}

/// How the core talks back to whoever hosts it (the Tauri app, or nothing when headless).
pub trait Host: Send + Sync {
    /// Called after every state change.
    fn changed(&self, _state: &Value) {}
    /// Carries out a player command. The default says there's no UI.
    fn ui(&self, _cmd: &str, _args: &Value) -> Result<(), String> {
        Err("there's no UI in headless mode".into())
    }
}

pub struct Headless;
impl Host for Headless {}

pub struct Core {
    s: Mutex<Session>,
    host: Box<dyn Host>,
    work_dir: PathBuf,
    log: Mutex<Option<DebugLog>>,
}

impl Core {
    pub fn new(host: Box<dyn Host>) -> Arc<Core> {
        let work_dir = crate::paths::cache_dir();
        let _ = std::fs::create_dir_all(&work_dir);
        Arc::new(Core { s: Mutex::new(Session { ui: json!({}), ..Default::default() }), host, work_dir, log: Mutex::new(None) })
    }

    /// Starts logging every command and UI event to a temporary folder (see `debuglog`); returns it.
    pub fn start_debug_log(&self) -> Result<PathBuf, String> {
        let log = DebugLog::create()?;
        let dir = log.dir().to_path_buf();
        self.s.lock().unwrap().debug_log = Some(dir.to_string_lossy().into_owned());
        *self.log.lock().unwrap() = Some(log);
        Ok(dir)
    }

    /// Deletes the log folder. Call when the app quits.
    pub fn stop_debug_log(&self) {
        self.s.lock().unwrap().debug_log = None;
        if let Some(log) = self.log.lock().unwrap().take() {
            log.remove();
        }
    }

    pub fn debug_log_dir(&self) -> Option<PathBuf> {
        self.log.lock().unwrap().as_ref().map(|l| l.dir().to_path_buf())
    }

    fn log(&self, src: &str, fields: Value) {
        if let Some(log) = self.log.lock().unwrap().as_ref() {
            log.write(src, fields);
        }
    }

    pub fn state(&self) -> Value {
        snapshot(&self.s.lock().unwrap())
    }

    fn notify(&self) {
        let st = self.state();
        self.host.changed(&st);
    }

    pub fn exec_json(self: &Arc<Self>, v: Value) -> Result<Value, String> {
        if v["cmd"] == "log" {
            // Events the page reports go straight into the debug log, not logged as a command.
            for e in v["events"].as_array().into_iter().flatten() {
                self.log("ui", debuglog::trim(e));
            }
            return Ok(json!({ "ok": true }));
        }
        let t0 = Instant::now();
        let result = serde_json::from_value::<Command>(v.clone()).map_err(|e| format!("bad command: {e}")).and_then(|cmd| self.exec(cmd));
        let ms = t0.elapsed().as_millis() as u64;
        match &result {
            // `state` answers are big and frequent; the log only needs to know it was asked.
            Ok(_) if v["cmd"] == "state" => self.log("cmd", json!({ "cmd": v, "ms": ms })),
            Ok(r) => self.log("cmd", json!({ "cmd": debuglog::trim(&v), "ms": ms, "result": debuglog::trim(r) })),
            Err(e) => self.log("cmd", json!({ "cmd": debuglog::trim(&v), "ms": ms, "error": e })),
        }
        result
    }

    pub fn exec(self: &Arc<Self>, cmd: Command) -> Result<Value, String> {
        if cmd.is_ui() {
            let v = ui_args(&cmd);
            let name = v["cmd"].as_str().unwrap_or("").to_string();
            self.host.ui(&name, &v)?;
            return Ok(json!({ "ok": true }));
        }
        let result = match cmd {
            Command::State => return Ok(self.state()),
            Command::Open { path } => self.open(&path),
            Command::Close => {
                let mut s = self.s.lock().unwrap();
                (s.info, s.edit) = (None, None);
                s.undo.clear();
                s.redo.clear();
                Ok(json!({ "ok": true }))
            }
            Command::SetCrop { rect } => self.change(|e, i| e.set_crop(rect, i.width, i.height)),
            Command::DeleteRange { start, end } => self.change(|e, i| e.delete_range(start, end, i.duration)),
            Command::RestoreRange { start, end } => self.change(|e, _| e.restore_range(start, end)),
            Command::Reset => self.change(|e, _| {
                e.crop = None;
                e.deleted.clear();
            }),
            Command::Undo => self.history(true),
            Command::Redo => self.history(false),
            Command::Thumbnails { count, height } => {
                let info = self.s.lock().unwrap().info.clone().ok_or("no video is open")?;
                let image = ffmpeg::thumbnail_strip(&info, count.unwrap_or(40), height.unwrap_or(90))?;
                return Ok(json!({ "image": image }));
            }
            Command::Export { output, mode } => self.export(output, mode.unwrap_or_default()),
            Command::Cancel { job } => {
                let s = self.s.lock().unwrap();
                let j = s.jobs.get(&job).ok_or("no such job")?;
                j.cancel.store(true, Ordering::Relaxed);
                return Ok(json!({ "ok": true }));
            }
            Command::Wait { job, timeout } => return self.wait(job, timeout.unwrap_or(600.0)),
            Command::Sources => {
                return Ok(json!({ "sources": record::list_sources()?, "system_audio": record::can_record_system_audio() }))
            }
            Command::RecordStart { options } => {
                if self.s.lock().unwrap().recording.is_some() {
                    return Err("already recording".into());
                }
                let rec = record::start(&options, &self.work_dir.join("recordings"))?;
                self.s.lock().unwrap().recording = Some(rec);
                Ok(json!({ "ok": true }))
            }
            Command::RecordStop => {
                let rec = self.s.lock().unwrap().recording.take().ok_or("not recording")?;
                let path = rec.stop()?;
                self.open(&path.to_string_lossy())
            }
            Command::UiReport { ui } => {
                // Frequent and cosmetic: stored, but doesn't trigger a change notification.
                self.s.lock().unwrap().ui = ui;
                return Ok(json!({ "ok": true }));
            }
            Command::Play | Command::Pause | Command::Seek { .. } | Command::Select { .. } | Command::ClearSelection => {
                unreachable!()
            }
        };
        if result.is_ok() {
            self.notify();
        }
        result
    }

    fn open(&self, path: &str) -> Result<Value, String> {
        let abs = std::fs::canonicalize(path).map_err(|e| format!("can't open {path}: {e}"))?;
        let path = &*abs.to_string_lossy();
        let info = ffmpeg::probe(path)?;
        let mut s = self.s.lock().unwrap();
        s.edit = Some(Edit::new(path));
        s.info = Some(info.clone());
        s.undo.clear();
        s.redo.clear();
        Ok(json!({ "file": info }))
    }

    /// Applies an undoable change to the edit.
    fn change(&self, f: impl FnOnce(&mut Edit, &MediaInfo)) -> Result<Value, String> {
        let mut s = self.s.lock().unwrap();
        let info = s.info.clone().ok_or("no video is open")?;
        let before = s.edit.clone().unwrap();
        let mut after = before.clone();
        f(&mut after, &info);
        if after != before {
            s.undo.push(before);
            s.redo.clear();
            s.edit = Some(after.clone());
        }
        Ok(json!({ "edit": after }))
    }

    fn history(&self, undo: bool) -> Result<Value, String> {
        let mut s = self.s.lock().unwrap();
        let cur = s.edit.clone().ok_or("no video is open")?;
        let prev = if undo { s.undo.pop() } else { s.redo.pop() }.ok_or("nothing to undo or redo")?;
        if undo { s.redo.push(cur) } else { s.undo.push(cur) }
        s.edit = Some(prev.clone());
        Ok(json!({ "edit": prev }))
    }

    fn export(self: &Arc<Self>, output: Option<String>, mode: Mode) -> Result<Value, String> {
        let (edit, info, id) = {
            let mut s = self.s.lock().unwrap();
            s.next_job += 1;
            (s.edit.clone().ok_or("no video is open")?, s.info.clone().unwrap(), s.next_job)
        };
        let out = output.map(PathBuf::from).unwrap_or_else(|| export::default_output(&edit.source, mode));
        if Path::new(&edit.source) == out {
            return Err("won't overwrite the original video".into());
        }
        let job_dir = self.work_dir.join(format!("job-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&job_dir).map_err(|e| e.to_string())?;
        let plan = export::plan(&edit, &info, &out, mode, &job_dir)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job {
            id,
            kind: "export".into(),
            status: "running".into(),
            progress: 0.0,
            output: out.to_string_lossy().into_owned(),
            error: None,
            cancel: cancel.clone(),
        };
        self.s.lock().unwrap().jobs.insert(id, job);

        let core = self.clone();
        std::thread::spawn(move || {
            let c2 = core.clone();
            let mut last = 0.0;
            let result = export::run(&plan, &out, cancel, move |p| {
                if p - last >= 0.01 || p >= 1.0 {
                    last = p;
                    if let Some(j) = c2.s.lock().unwrap().jobs.get_mut(&id) {
                        j.progress = p;
                    }
                    c2.notify();
                }
            });
            let _ = std::fs::remove_dir_all(&job_dir);
            if let Some(j) = core.s.lock().unwrap().jobs.get_mut(&id) {
                match result {
                    Ok(_) => (j.status, j.progress) = ("done".into(), 1.0),
                    Err(e) if e == "cancelled" => j.status = "cancelled".into(),
                    Err(e) => (j.status, j.error) = ("failed".into(), Some(e)),
                }
            }
            core.notify();
        });
        Ok(json!({ "job": id, "output": self.s.lock().unwrap().jobs[&id].output }))
    }

    fn wait(&self, id: u64, timeout: f64) -> Result<Value, String> {
        let t0 = Instant::now();
        loop {
            let j = self.s.lock().unwrap().jobs.get(&id).cloned().ok_or("no such job")?;
            if j.status != "running" || t0.elapsed().as_secs_f64() > timeout {
                return Ok(serde_json::to_value(j).unwrap());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// The player command as JSON for the UI (same shape as the API sent it).
fn ui_args(cmd: &Command) -> Value {
    match cmd {
        Command::Play => json!({ "cmd": "play" }),
        Command::Pause => json!({ "cmd": "pause" }),
        Command::Seek { time } => json!({ "cmd": "seek", "time": time }),
        Command::Select { start, end } => json!({ "cmd": "select", "start": start, "end": end }),
        Command::ClearSelection => json!({ "cmd": "clear_selection" }),
        _ => Value::Null,
    }
}

fn snapshot(s: &Session) -> Value {
    let (kept, kept_duration, output_size) = match (&s.edit, &s.info) {
        (Some(e), Some(i)) => {
            let size = e.crop.map(|c| (c.w, c.h)).unwrap_or((i.width, i.height));
            (e.kept(i.duration), e.kept_duration(i.duration), Some(json!({ "w": size.0, "h": size.1 })))
        }
        _ => (Vec::<Range>::new(), 0.0, None),
    };
    json!({
        "file": s.info,
        "edit": s.edit,
        "kept": kept,
        "kept_duration": kept_duration,
        "output_size": output_size,
        "can_undo": !s.undo.is_empty(),
        "can_redo": !s.redo.is_empty(),
        "jobs": s.jobs.values().collect::<Vec<_>>(),
        "recording": s.recording.as_ref().map(|r| json!({ "seconds": r.started.elapsed().as_secs_f64() })),
        "ui": s.ui,
        "debug_log": s.debug_log,
    })
}
