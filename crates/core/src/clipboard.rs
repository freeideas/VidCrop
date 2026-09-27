//! Putting a video file on the clipboard, so it can be pasted straight into a chat app
//! (WhatsApp, Slack, Mail...) without the user ever handling a file. The edited video is
//! saved to `<cache>/clipboard/` first, since a clipboard can only point at a file.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where `copy` saves the edited video: named like the original, so the chat shows a sensible name.
pub fn output_for(source: &str, cache: &Path) -> PathBuf {
    let stem = Path::new(source).file_stem().and_then(|s| s.to_str()).unwrap_or("video");
    cache.join("clipboard").join(format!("{stem}.mp4"))
}

/// Puts the file itself (not its path as text) on the system clipboard.
pub fn copy_file(path: &Path) -> Result<(), String> {
    let p = path.to_string_lossy();
    let out = if cfg!(target_os = "macos") {
        // A file URL on the pasteboard, which chat apps accept as an attachment. (AppleScript's
        // `set the clipboard to POSIX file` reports success but leaves the clipboard empty.)
        const JS: &str = "function run(argv) { ObjC.import('AppKit'); const pb = $.NSPasteboard.generalPasteboard; \
            pb.clearContents; if (!pb.writeObjects($([$.NSURL.fileURLWithPath(argv[0])]))) throw 'refused'; }";
        Command::new("osascript").args(["-l", "JavaScript", "-e", JS]).arg(&*p).output()
    } else if cfg!(windows) {
        let quoted = p.replace('\'', "''");
        let mut c = crate::ffmpeg::command("powershell");
        c.args(["-NoProfile", "-Command", &format!("Set-Clipboard -LiteralPath '{quoted}'")]);
        c.output()
    } else {
        let uri = format!("file://{p}\n");
        let run = |cmd: &str, args: &[&str]| -> std::io::Result<std::process::Output> {
            use std::io::Write;
            let mut child = Command::new(cmd).args(args).stdin(std::process::Stdio::piped()).spawn()?;
            child.stdin.take().unwrap().write_all(uri.as_bytes())?;
            child.wait_with_output()
        };
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            run("wl-copy", &["--type", "text/uri-list"])
        } else {
            run("xclip", &["-selection", "clipboard", "-t", "text/uri-list"])
        }
    };
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!("couldn't copy to the clipboard: {}", String::from_utf8_lossy(&o.stderr).trim())),
        Err(e) => Err(format!("couldn't copy to the clipboard: {e}")),
    }
}

/// Copies from earlier sessions are removed after a day (the clipboard may still point at today's).
pub fn clean_old(cache: &Path) {
    let Ok(entries) = std::fs::read_dir(cache.join("clipboard")) else { return };
    for e in entries.flatten() {
        let age = e.metadata().and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok());
        if age.is_some_and(|a| a.as_secs() > 86400) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}
