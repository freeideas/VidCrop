# Architecture

## Stack

| Part            | Choice                                   | Why                                                   |
|-----------------|------------------------------------------|-------------------------------------------------------|
| App shell       | Tauri 2 (Rust)                           | Cross-platform, small download (~10 MB plus ffmpeg)   |
| User interface  | TypeScript + Vite, no UI framework       | One screen; a framework adds more than it saves       |
| Playback        | HTML `<video>` in the system web view    | Free scrubbing, playback and hardware decoding        |
| Probing         | `ffprobe` (JSON output)                  | Duration, size, rotation, frame rate, streams         |
| Thumbnails      | `ffmpeg` sprite sheet                    | One pass, one image, cheap to draw                    |
| Export          | `ffmpeg` child process                   | Does all the real video work                          |
| Screen capture  | `ffmpeg` capture devices, later native   | See [screen-recording.md](screen-recording.md)        |

Electron was considered and rejected: consistent codecs everywhere, but about 150 MB per install for a one-screen tool.

## Processes

```
+------------------------+        Tauri commands        +-------------------------+
|  Web view (TypeScript) |  ------------------------->  |  Rust core              |
|  - video player        |  <-------------------------  |  - probe / thumbnails   |
|  - crop overlay        |     events (progress, done)  |  - export job           |
|  - timeline + undo     |                              |  - recording job        |
+------------------------+                              +-----------+-------------+
                                                                    |
                                                                    | spawns
                                                                    v
                                                           ffmpeg / ffprobe
```

- The web view holds the whole edit state (see [editor.md](editor.md)). The Rust side is stateless except for running jobs.
- Rust commands: `probe(path)`, `thumbnails(path, count)`, `export(edit, out_path, mode)`, `cancel(job_id)`, `list_capture_sources()`, `start_recording(opts)`, `stop_recording(job_id)`.
- Long jobs emit Tauri events: `job://progress {job_id, fraction}`, `job://done {job_id, path}`, `job://error {job_id, message, log_tail}`.
- Local files reach the `<video>` element through Tauri's asset protocol (`convertFileSrc`), scoped to files the user opened or recorded.

## Finding ffmpeg

Order of lookup:

1. Bundled sidecar (Tauri `externalBin`, one binary per target triple, e.g. `ffmpeg-aarch64-apple-darwin`).
2. `ffmpeg` on the PATH, plus the usual Homebrew locations (`/opt/homebrew/bin`, `/usr/local/bin`), since apps launched from Finder don't get the shell's PATH.

Development uses the Homebrew copy. Release builds bundle it.

**License:** GPL is fine. Bundle a full GPL ffmpeg build (with libx264) and ship its license text and a pointer to its source with the app.

## Preview on each platform

- **macOS (WKWebView):** plays H.264, HEVC, ProRes in MP4/MOV. Covers nearly everything a Mac user has.
- **Windows (WebView2, i.e. Edge):** H.264 fine, HEVC only with the system extension, no MOV/ProRes.
- **Linux (WebKitGTK):** depends on installed GStreamer plugins; often poor.

When the `<video>` element fires `error` or can't decode, generate a **preview proxy**: a low-resolution H.264 MP4 in the cache folder, used only for display. Crop coordinates are always stored in source pixels, so the proxy's lower resolution doesn't matter for export.

## Files and folders

- Output default: next to the source, named `<name>-cropped.mp4`, adding `-2`, `-3` if taken.
- Cache (thumbnails, proxies, recordings in progress): the OS app cache folder, cleaned on startup of anything older than 7 days.
