//! Screen recording on macOS 15+ through ScreenCaptureKit, which (unlike ffmpeg's avfoundation input)
//! asks for permission properly, can record the sound the computer plays, and leaves VidCrop out of
//! the picture. It writes the file itself. See specs/screen-recording.md.

use crate::record::{RecordOptions, Source};
use screencapturekit::prelude::*;
use screencapturekit::recording_output::{
    RecordingCallbacks, SCRecordingOutput, SCRecordingOutputCodec, SCRecordingOutputConfiguration,
    SCRecordingOutputFileType,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub fn available() -> bool {
    SCRecordingOutput::is_available()
}

pub struct Native {
    stream: SCStream,
    output: SCRecordingOutput,
    file: PathBuf,
    failed: Arc<Mutex<Option<String>>>,
}

fn content() -> Result<SCShareableContent, String> {
    SCShareableContent::get().map_err(|e| {
        format!(
            "macOS won't let VidCrop see the screen ({e}).\n\nAllow VidCrop in System Settings > Privacy & Security > Screen & System Audio Recording, then quit and reopen VidCrop."
        )
    })
}

pub fn list_sources() -> Result<Vec<Source>, String> {
    let mut out: Vec<Source> = content()?
        .displays()
        .iter()
        .enumerate()
        .map(|(i, d)| Source {
            id: d.display_id().to_string(),
            name: format!("Screen {} ({}x{})", i + 1, d.width(), d.height()),
            kind: "screen".into(),
        })
        .collect();
    out.extend(
        AudioInputDevice::list()
            .into_iter()
            .map(|m| Source { id: m.id, name: m.name, kind: "mic".into() }),
    );
    Ok(out)
}

pub fn start(opts: &RecordOptions, file: &Path, fps: u32) -> Result<Native, String> {
    let content = content()?;
    let displays = content.displays();
    let display = match &opts.screen {
        Some(id) => displays.iter().find(|d| d.display_id().to_string() == *id).ok_or("that screen isn't connected")?,
        None => displays.first().ok_or("no screen found to record")?,
    };
    // Leave our own windows (the REC indicator, a dialog) out of the recording.
    let apps = content.applications();
    let me: Vec<&SCRunningApplication> = apps.iter().filter(|a| a.process_id() == std::process::id() as i32).collect();
    let filter = SCContentFilter::create()
        .with_display(display)
        .with_excluding_windows(&[])
        .with_excluding_applications(&me, &[])
        .build()
        .map_err(|e| e.to_string())?;
    let scale = filter.point_pixel_scale().max(1.0);
    let even = |v: f64| ((v as u32) / 2 * 2).max(2);
    let mut config = SCStreamConfiguration::new()
        .with_width(even(display.width() as f64 * scale as f64))
        .with_height(even(display.height() as f64 * scale as f64))
        .with_minimum_frame_interval(&CMTime::new(1, fps as i32))
        .with_shows_cursor(true)
        .with_captures_audio(opts.system_audio)
        .with_excludes_current_process_audio(true)
        .with_sample_rate(48000)
        .with_channel_count(2);
    if let Some(mic) = &opts.mic {
        config = config.with_captures_microphone(true).map_err(|e| e.to_string())?;
        config = config.with_microphone_capture_device_id(mic).map_err(|e| e.to_string())?;
    }

    let rec_config = SCRecordingOutputConfiguration::new()
        .map_err(|e| e.to_string())?
        .with_output_url(file)
        .map_err(|_| "bad recording path")?
        .with_video_codec(SCRecordingOutputCodec::H264)
        .with_output_file_type(SCRecordingOutputFileType::MOV);
    let failed = Arc::new(Mutex::new(None));
    let started = Arc::new(Mutex::new(false));
    let callbacks = {
        let (failed, started) = (failed.clone(), started.clone());
        RecordingCallbacks::new()
            .on_start(move || *started.lock().unwrap() = true)
            .on_fail(move |e| *failed.lock().unwrap() = Some(e))
    };
    let output = SCRecordingOutput::new_with_delegate(&rec_config, callbacks).ok_or("couldn't set up the recording")?;
    let stream = SCStream::new(&filter, &config).map_err(|e| e.to_string())?;
    stream.add_recording_output(&output).map_err(|e| e.to_string())?;
    stream.start_capture().map_err(|e| format!("screen recording didn't start: {e}"))?;

    // Report a failure now rather than after the user has recorded for ten minutes.
    let t0 = Instant::now();
    while !*started.lock().unwrap() && t0.elapsed() < Duration::from_secs(3) {
        if let Some(e) = failed.lock().unwrap().take() {
            let _ = stream.stop_capture();
            return Err(format!("screen recording didn't start: {e}"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(Native { stream, output, file: file.to_path_buf(), failed })
}

impl Native {
    /// Stops, waits for the file to be finished, and returns it.
    pub fn stop(self) -> Result<PathBuf, String> {
        let removed = self.stream.remove_recording_output(&self.output);
        let _ = self.stream.stop_capture();
        if let Some(e) = self.failed.lock().unwrap().take() {
            return Err(format!("the recording failed: {e}"));
        }
        removed.map_err(|e| e.to_string())?;
        if !self.file.is_file() {
            return Err("the recording produced no file".into());
        }
        Ok(self.file)
    }
}
