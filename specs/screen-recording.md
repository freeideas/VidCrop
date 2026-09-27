# Screen recording

The point: record the whole screen without fussing, then use the normal editor to crop to the window you care about and cut the fumbling. So recording has almost no options.

## Flow

1. Press **Record**. A small panel asks: which screen, which microphone (or none), whether to also record the computer's own sound (where supported), and smoothness. Remembers last choice.
2. 3-second countdown, then the VidCrop window hides itself. A menu bar / tray icon shows elapsed time and a Stop button. Global shortcut: Cmd/Ctrl+Shift+2 to stop.
3. On Stop, the finished recording is written straight to the Desktop, named like macOS names its own (`Screen recording 2026-09-27 at 12.17.19.mp4`), and opens in the editor. Only the raw file being recorded sits in the cache, and it is deleted once the Desktop copy is made. Saving edits then writes `... -cropped.mp4` next to it on the Desktop.

## Phase 1: ffmpeg capture devices

Used on Windows, Linux, and macOS 13-14. On macOS 15+ phase 2 below replaces it.

| OS            | Input                                                  | Notes                                      |
|---------------|--------------------------------------------------------|--------------------------------------------|
| macOS         | `-f avfoundation -capture_cursor 1 -i "<screen>:<mic>"` | List devices: `-list_devices true -i ""`   |
| Windows       | `-f lavfi -i ddagrab` (Desktop Duplication)            | Fallback `gdigrab`; mic via `dshow`        |
| Linux (X11)   | `-f x11grab -i :0.0`                                    | Mic via `pulse`                            |
| Linux Wayland | not supported in phase 1                               | Needs portal + PipeWire, see phase 2       |

- Record to MKV (survives a crash mid-recording, unlike MP4), with a fast encoder: `h264_videotoolbox` on macOS, `libx264 -preset ultrafast -crf 18` elsewhere.
- Web views don't play MKV, so on stop, rewrap it as MP4 with `-c copy` (no re-encoding, takes about a second) before opening it in the editor.
- Frame rate: 30 fps default, 60 option.
- Stop by writing `q` to ffmpeg's stdin so the file is closed cleanly.

**macOS permission (phase 1):** without Screen Recording permission, avfoundation doesn't fail or prompt, it just hangs writing nothing. So `record_start` waits up to 5 seconds for the file to get data, and otherwise stops ffmpeg and explains how to allow VidCrop in System Settings > Privacy & Security.

**Phase 1 limit:** no system audio (the sound your computer plays). avfoundation can't capture it.

## Phase 2: native capture

- **macOS 15+ (done, `crates/core/src/record_mac.rs`):** ScreenCaptureKit through the `screencapturekit` crate, with Apple's `SCRecordingOutput` writing an H.264 MOV directly. Gives a proper permission prompt (the first `sources` or `record_start` asks), full-resolution capture, optional system audio, the mic, and leaves VidCrop's own windows and sounds out. Mic and system sound arrive as separate tracks; on stop they're mixed into one AAC track (the editor keeps only the first audio track) while the picture is copied as is. Frames only arrive when the screen changes, so the frame rate is variable.
  - Building it needs the Swift toolchain (Xcode Command Line Tools are enough; `crates/core/build.rs` finds its libraries) and targets macOS 13+ (`.cargo/config.toml`).
  - Ad-hoc signed builds get a new identity every build, so macOS asks for permission again after each rebuild. A Developer ID signature would fix that.
- **Windows:** Windows.Graphics.Capture plus WASAPI loopback for system audio.
- **Linux Wayland:** xdg-desktop-portal ScreenCast plus PipeWire; system audio from the PulseAudio/PipeWire monitor source.

On Windows and Linux, frames are piped to ffmpeg (raw frames on stdin) or encoded natively; decide when we get there.

## Maybe later

- Record only a selected area or window (less cropping afterward, but cropping afterward is the whole point of the app, so low priority).
- Show key presses and clicks.
