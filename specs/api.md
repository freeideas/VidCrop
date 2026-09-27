# API

Everything VidCrop can do can be done without looking at it. The window and the API are two front ends to the same Rust core (`crates/core/src/session.rs`), so a test that drives the API exercises the same code the buttons do. DesktopIA is for checking what the window *looks* like; the API is for checking what the app *does*.

## Three ways in

1. **HTTP, while the app is open.** The app starts the API on launch.
2. **HTTP, with no window:** `vidcrop serve [--port N]`. Same API; player commands (below) return an error because there's no player.
3. **One-shot command line:** `vidcrop export IN [-o OUT] [--crop W:H:X:Y] [--cut A-B]... [--fast]` and `vidcrop probe IN`.

`vidcrop api '{"cmd":"state"}'` sends one command to whichever API is running (it reads the connection file for you).

## Connecting

- Listens on `127.0.0.1` only, on a random free port (`VIDCROP_API_PORT` fixes it).
- On start it writes `api.json` with `url`, `token` and `pid` to the data folder: `~/Library/Application Support/VidCrop/` on macOS, `%APPDATA%\VidCrop\` on Windows, `~/.local/share/vidcrop/` on Linux. `VIDCROP_API_FILE` overrides the path. The file is readable only by the user.
- Every request needs `Authorization: Bearer <token>`. `VIDCROP_API_TOKEN` sets a fixed token for tests.
- Requests carrying an `Origin` header are refused, so a web page in a browser can't drive VidCrop even if it guesses the port.

## Endpoints

| Method | Path     | Body                  | Returns                                   |
|--------|----------|-----------------------|-------------------------------------------|
| GET    | `/`      |                       | Short help text (no token needed)         |
| GET    | `/state` |                       | Full state                                |
| POST   | `/cmd`   | `{"cmd": "...", ...}` | The command's result, or 400 `{"error"}`  |

```sh
TOKEN=$(jq -r .token "$HOME/Library/Application Support/VidCrop/api.json")
curl -s -H "Authorization: Bearer $TOKEN" -d '{"cmd":"open","path":"/path/in.mp4"}' http://127.0.0.1:PORT/cmd
```

## Commands

Times are in seconds. Crop rectangles are in source pixels of the picture as displayed (rotation applied), and get clamped and rounded to even numbers.

| Command          | Fields                                  | Does                                                |
|------------------|-----------------------------------------|-----------------------------------------------------|
| `state`          |                                         | Returns the full state                              |
| `open`           | `path`                                  | Opens a video (clears undo history)                 |
| `close`          |                                         | Closes it                                           |
| `set_crop`       | `rect: {x,y,w,h}` or `null`             | Sets or removes the crop                            |
| `delete_range`   | `start`, `end`                          | Cuts out a part                                     |
| `restore_range`  | `start`, `end`                          | Brings back deleted video in that span              |
| `reset`          |                                         | Removes crop and all cuts                           |
| `undo`, `redo`   |                                         | Edit history                                        |
| `thumbnails`     | `count?`, `height?`                     | `{image}`: a JPEG strip as a data URL               |
| `export`         | `output?`, `mode?`: `exact` or `fast`   | Starts saving; returns `{job, output}`              |
| `wait`           | `job`, `timeout?`                       | Blocks until the job ends; returns the job          |
| `cancel`         | `job`                                   | Stops a save and deletes the partial file           |
| `sources`        |                                         | Screens, mics, and `system_audio` (can it be recorded) |
| `record_start`   | `screen?`, `mic?`, `system_audio?`, `fps?` | Starts recording (the window hides)                 |
| `record_stop`    |                                         | Stops, and opens the recording in the editor        |
| `play`, `pause`  |                                         | Player (window only)                                |
| `seek`           | `time`                                  | Player (window only)                                |
| `select`         | `start`, `end`                          | Timeline selection (window only)                    |
| `clear_selection`|                                         | Timeline selection (window only)                    |
| `ui_report`      | `ui`                                    | Used by the page to publish its player state        |

Every edit command is one undo step, the same as the matching mouse action.

## State

```jsonc
{
  "file": { "path", "duration", "width", "height", "rotation", "fps", "video_codec", "has_audio", "size_bytes" },
  "edit": { "source", "crop": {x,y,w,h} | null, "deleted": [{start,end}] },
  "kept": [{start,end}],          // what will be saved
  "kept_duration": 7.5,
  "output_size": { "w", "h" },
  "can_undo": true, "can_redo": false,
  "jobs": [{ "id", "kind", "status": "running|done|failed|cancelled", "progress", "output", "error" }],
  "recording": { "seconds" } | null,
  "ui": { "playhead", "playing", "selection", "preview_crop", "zoom", "crop_shape" }  // from the window
}
```

## Tests

- `cargo test` runs unit tests plus `crates/core/tests/export_e2e.rs`, which makes a real clip with ffmpeg, edits and saves it through commands, and checks the result with ffprobe.
- For the window: run the app, drive it through the API, and use DesktopIA (or a screenshot) only to check what it looks like and that mouse gestures (dragging the crop box, selecting on the timeline) produce the right `state`.
