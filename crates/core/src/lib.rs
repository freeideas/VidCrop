//! VidCrop's core: the edit model, ffmpeg calls, screen recording, and the HTTP API.
//! Everything the app can do goes through `session::Core`, so it can all be driven and
//! tested without a window.

pub mod api;
pub mod clipboard;
pub mod debuglog;
pub mod edit;
pub mod export;
pub mod ffmpeg;
pub mod paths;
pub mod preview;
pub mod record;
#[cfg(target_os = "macos")]
mod record_mac;
pub mod session;
