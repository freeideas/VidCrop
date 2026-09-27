//! Where VidCrop keeps its files, per OS.

use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

/// Settings and the API connection file.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("VIDCROP_DATA_DIR") {
        return PathBuf::from(d);
    }
    if cfg!(target_os = "macos") {
        home().join("Library/Application Support/VidCrop")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(home).join("VidCrop")
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("vidcrop")
    }
}

/// Recordings in progress and scratch files for jobs.
pub fn cache_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("VIDCROP_CACHE_DIR") {
        return PathBuf::from(d);
    }
    if cfg!(target_os = "macos") {
        home().join("Library/Caches/VidCrop")
    } else if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(home).join("VidCrop/cache")
    } else {
        std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".cache")).join("vidcrop")
    }
}

/// Where finished screen recordings go: the Desktop, or the home folder if there isn't one.
pub fn desktop_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("VIDCROP_DESKTOP_DIR") {
        return PathBuf::from(d);
    }
    let d = home().join("Desktop");
    if d.is_dir() { d } else { home() }
}
