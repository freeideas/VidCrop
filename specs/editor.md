# Editor

## Layout

```
+-----------------------------------------------------------+
|  [Open]  [Record]                     1280 x 720  [Save]  |
+-----------------------------------------------------------+
|                                                           |
|        +-----------------------------+                    |
|        |                             |   dimmed outside   |
|        |      kept area (crop)       |   the crop box     |
|        |                             |                    |
|        +-----------------------------+                    |
|                                                           |
+-----------------------------------------------------------+
|  > 00:12.40 / 01:03.00                                    |
|  [thumb][thumb][/////deleted/////][thumb][thumb][thumb]   |
|              ^ playhead                                   |
+-----------------------------------------------------------+
```

Empty state: a big drop zone, "Drop a video here, or press Record."

## Edit model

The entire edit is one small plain object, owned by the Rust core (`crates/core/src/edit.rs`), which makes undo, saving and testing simple:

```ts
type Edit = {
  source: string;                          // absolute path
  crop: { x: number; y: number; w: number; h: number } | null; // source pixels, display orientation
  deleted: Array<{ start: number; end: number }>; // seconds, sorted, non-overlapping, merged
};
```

- `kept` ranges are derived: the source duration minus `deleted`.
- Undo/redo: a stack of `Edit` snapshots (they're tiny). Every committed drag or delete pushes one; mid-drag changes don't.
- Not built yet: the edit is autosaved to the cache, keyed by source path and file size plus modified time, so reopening the same file offers to restore it.

## Crop box

- Starts as the full frame (meaning "no crop"; `crop` stays `null` until changed).
- Drag edges and corners to resize; drag inside to move. Clamped to the frame.
- Shift while dragging keeps the aspect ratio. Optional ratio presets in a small menu: Free, 16:9, 9:16, 1:1, 4:3.
- Width, height, x and y snap to even numbers (required by the 4:2:0 color format most video uses).
- The toolbar shows the output size, e.g. `1280 x 720`. Clicking it lets you type exact numbers.
- Outside the box is dimmed, not hidden, so you can see what you're cutting.
- Rotation: `ffprobe` reports rotation metadata (common on phone videos). The web view and ffmpeg both show the rotated picture, so crop coordinates are in the rotated ("as displayed") orientation.

## Timeline

- Thumbnail strip generated once per file (see [architecture.md](architecture.md)).
- Click to move the playhead. Drag across to select a range. The selection shows its start, end and length.
- Delete or Backspace removes the selection. Deleted ranges show hatched and dimmed, and stay visible so they can be clicked and restored ("Restore this part").
- Zoom with pinch or Ctrl/Cmd + scroll for long videos.
- Snapping: range edges snap to the playhead and to other range edges within a few pixels.

## Playback

- Space plays and pauses.
- During playback, when the playhead enters a deleted range it jumps to that range's end, so the preview matches the output. Checked on `requestVideoFrameCallback` (falls back to `timeupdate`), which is accurate to about one frame.
- The crop box stays visible during playback. A "Preview crop" toggle (key `C`) hides everything outside the box.

## Keyboard shortcuts

| Key                   | Action                                   |
|-----------------------|------------------------------------------|
| Space                 | Play / pause                             |
| Left / Right          | Step one frame (with Shift: one second)  |
| I / O                 | Set selection start / end at playhead    |
| Delete, Backspace     | Delete selection                         |
| Cmd/Ctrl+Z, Shift+Z   | Undo / redo                              |
| C                     | Toggle crop preview                      |
| Cmd/Ctrl+S            | Save                                     |
| Esc                   | Clear selection                          |
