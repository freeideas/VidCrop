//! VidCrop's core: the edit model, ffmpeg calls, screen recording, and the HTTP API.
//! Everything the app can do goes through `session::Core`, so it can all be driven and
//! tested without a window.

pub mod api;
pub mod edit;
pub mod export;
pub mod ffmpeg;
pub mod paths;
pub mod record;
pub mod session;
