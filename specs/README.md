# VidCrop specs

Technical plans for VidCrop. The user-facing pitch is in the [top-level README](../README.md).

- [architecture.md](architecture.md): the app's parts, the tech stack, and how they talk to each other.
- [editor.md](editor.md): the editing screen, the edit model, keyboard shortcuts, undo.
- [export.md](export.md): how an edit becomes an ffmpeg command, encoder choices, progress, and the lossless "fast save".
- [screen-recording.md](screen-recording.md): recording the screen and handing the result to the editor.
- [api.md](api.md): driving everything without the window (HTTP API and command line), and how testing works.
- [releases.md](releases.md): building for all three platforms, bundling ffmpeg, and publishing downloads.

## Guiding rules

1. One job: crop in space and time. Every feature request gets measured against "does this make cropping easier?"
2. No settings the user must understand before they can save. Sensible defaults, with an "Advanced" section tucked away.
3. The original file is never modified.
4. Everything runs locally. No network calls (the API listens only on this computer).
5. Everything is operable through the API. Tests drive the API; screenshots are only for checking looks.

## Working on VidCrop

- **Every feature must be reachable through the core's JSON commands** (`crates/core/src/session.rs`), not only through the page. The page and the HTTP API are both thin front ends to it.
- **Test by driving the API** (`cargo test`, `vidcrop serve`, `vidcrop api '{...}'`); [api.md](api.md) explains how. Use DesktopIA or screenshots only to check looks and mouse gestures.
- **Run the app with `npx tauri dev`.** The running app writes its API address and token to `~/Library/Application Support/VidCrop/api.json` (macOS).
- **Only the first video and first audio stream are saved.** Saving always writes a new file and never touches the original.
- **There is deliberately no `AGENTS.md` or `CLAUDE.md`.** An agent reads the [README](../README.md), then this page and the spec its task needs.

## Milestones

Done on macOS (September 2026): milestones 1 to 5, a basic fast save, the API, and an unsigned release build. Not yet: autosaving the edit, the preview copy for videos the window can't play, the keyframe warning for fast save, Windows and Linux builds, bundling ffmpeg.

1. **Skeleton:** Tauri app opens a video (file dialog or drag and drop) and plays it.
2. **Crop box:** draggable overlay, snaps to even pixel sizes, shows the resulting size.
3. **Timeline cuts:** thumbnail strip, range selection, delete, undo/redo, playback that skips deleted ranges.
4. **Save:** ffmpeg export with progress bar and cancel. macOS release build.
5. **Screen recording:** macOS first.
6. **Windows and Linux:** builds, ffmpeg bundling, preview fallback for formats the system player can't play.
7. **Fast save:** lossless time-only export.
