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
+------------------------+                               +---------------------------+
|  Web view (TypeScript) |  -- cmd (JSON command) ---->  |  Rust core (Core)         |
|  - video player        |  <-- "state" / "ui" events -- |  - edit + undo history    |
|  - crop overlay        |                               |  - probe / thumbnails     |
|  - timeline            |                               |  - save jobs, recording   |
+------------------------+                               +-------------+-------------+
                                                                       ^      |
  HTTP API (127.0.0.1) / vidcrop CLI  -- same JSON commands ---------->+      | spawns
                                                                              v
                                                                       ffmpeg / ffprobe
```

- The Rust core owns the edit, the undo history and the jobs. The page keeps only player things (playhead, selection, zoom) and publishes them back with `ui_report` so API clients can see them. See [api.md](api.md) for every command.
- The page has one Tauri command, `cmd`, taking the same JSON as the HTTP API's `POST /cmd`. After every change the core pushes the full state to the page as a `state` event.
- Player commands from the API (`play`, `seek`, `select`...) reach the page as `ui` events.
- Local files reach the `<video>` element through Tauri's asset protocol (`convertFileSrc`), scoped at runtime to files that were opened or recorded.

## Code layout

| Path                  | What                                                        |
|-----------------------|-------------------------------------------------------------|
| `crates/core`         | Edit model, ffmpeg calls, save, recording, session, HTTP API |
| `crates/cli`          | `vidcrop` command: probe, export, serve, api                |
| `src-tauri`           | The desktop app: window, tray icon, hosts the core and API  |
| `src`, `index.html`   | The editor page                                             |

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

**Preview copy** (`crates/core/src/preview.rs`): for a video over 1920x1088, or not H.264/HEVC, opening it starts a background job (kind `preview`) that makes a smaller H.264 copy in the cache (at most 1920x1080, a keyframe every half second, no B-frames). Full-resolution screen recordings need this: the web view's decoder showed green blotches or froze after seeking in them. The player switches to the copy when it's ready (`state.preview`), keeping its place. Crop and cuts are stored in the original's pixels and seconds, and saving always reads the original, so the copy only affects the screen. Copies unused for a week are removed.

## Files and folders

- Output default: next to the source, named `<name>-cropped.mp4`, adding `-2`, `-3` if taken.
- Cache (thumbnails, proxies, recordings in progress): the OS app cache folder, cleaned on startup of anything older than 7 days.
