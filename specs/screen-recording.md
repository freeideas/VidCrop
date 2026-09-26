# Screen recording

The point: record the whole screen without fussing, then use the normal editor to crop to the window you care about and cut the fumbling. So recording has almost no options.

## Flow

1. Press **Record**. A small panel asks: which screen (if more than one), microphone on/off. Remembers last choice.
2. 3-second countdown, then the VidCrop window hides itself. A menu bar / tray icon shows elapsed time and a Stop button. Global shortcut: Cmd/Ctrl+Shift+2 to stop.
3. On Stop, the recording opens in the editor. It lives in the cache until saved; the "Save" button offers to keep the raw recording too.

## Phase 1: ffmpeg capture devices

Quick to build, one code path shape for all platforms.

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

**macOS permission:** the first recording triggers the Screen Recording permission prompt for VidCrop, and macOS requires restarting the app after granting it. Detect a black or failed capture, then explain and offer a button that opens System Settings at the right page (`x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`). Microphone permission works the same way.

**Phase 1 limit:** no system audio (the sound your computer plays). avfoundation can't capture it.

## Phase 2: native capture

- **macOS:** ScreenCaptureKit (macOS 12.3+, system audio from 13) through the `screencapturekit` Rust crate. Gives system audio, per-window capture, and hides VidCrop's own windows from the recording.
- **Windows:** Windows.Graphics.Capture plus WASAPI loopback for system audio.
- **Linux Wayland:** xdg-desktop-portal ScreenCast plus PipeWire.

Frames are piped to ffmpeg (raw frames on stdin) or encoded natively; decide when we get there.

## Maybe later

- Record only a selected area or window (less cropping afterward, but cropping afterward is the whole point of the app, so low priority).
- Show key presses and clicks.
