# VidCrop specs

Technical plans for VidCrop. The user-facing pitch is in the [top-level README](../README.md).

- [architecture.md](architecture.md): the app's parts, the tech stack, and how they talk to each other.
- [editor.md](editor.md): the editing screen, the edit model, keyboard shortcuts, undo.
- [export.md](export.md): how an edit becomes an ffmpeg command, encoder choices, progress, and the lossless "fast save".
- [screen-recording.md](screen-recording.md): recording the screen and handing the result to the editor.

## Guiding rules

1. One job: crop in space and time. Every feature request gets measured against "does this make cropping easier?"
2. No settings the user must understand before they can save. Sensible defaults, with an "Advanced" section tucked away.
3. The original file is never modified.
4. Everything runs locally. No network calls.

## Milestones

1. **Skeleton:** Tauri app opens a video (file dialog or drag and drop) and plays it.
2. **Crop box:** draggable overlay, snaps to even pixel sizes, shows the resulting size.
3. **Timeline cuts:** thumbnail strip, range selection, delete, undo/redo, playback that skips deleted ranges.
4. **Save:** ffmpeg export with progress bar and cancel. macOS release build.
5. **Screen recording:** macOS first.
6. **Windows and Linux:** builds, ffmpeg bundling, preview fallback for formats the system player can't play.
7. **Fast save:** lossless time-only export.
