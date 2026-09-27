# Export

Turns an `Edit` (see [editor.md](editor.md)) into one ffmpeg run.

## Normal save (exact, re-encoded)

Always frame-accurate. Required whenever there's a crop, since cropping changes every frame.

For kept ranges `[s0,e0], [s1,e1], ...` and crop `w:h:x:y`, build a filter graph:

```
[0:v]trim=start=s0:end=e0,setpts=PTS-STARTPTS[v0];
[0:a]atrim=start=s0:end=e0,asetpts=PTS-STARTPTS[a0];
[0:v]trim=start=s1:end=e1,setpts=PTS-STARTPTS[v1];
[0:a]atrim=start=s1:end=e1,asetpts=PTS-STARTPTS[a1];
[v0][a0][v1][a1]concat=n=2:v=1:a=1[vc][ac];
[vc]crop=w:h:x:y[vout]
```

Then:

```
ffmpeg -hide_banner -y -i SRC -filter_complex_script graph.txt \
  -map [vout] -map [ac] <encoder args> -c:a aac -b:a 192k \
  -movflags +faststart -progress pipe:1 -nostats OUT.tmp.mp4
```

- The graph goes in a file (`-filter_complex_script`) so a hundred cuts don't hit command-line length limits.
- With one kept range, skip `concat` and use `-ss`/`-to` before `-i` instead of `trim` (much faster: ffmpeg seeks instead of decoding from the start). Still frame-accurate when re-encoding.
- No crop: drop the `crop` filter.
- No audio stream: drop all audio parts and `-map`.
- Only the first video and first audio stream are kept. Subtitles and extra tracks are dropped (noted in "Advanced").
- Write to `OUT.tmp.mp4`, rename to `OUT.mp4` on success, delete on failure or cancel.

## Encoder

| Platform | Default encoder           | Arguments                                           |
|----------|---------------------------|-----------------------------------------------------|
| macOS    | `h264_videotoolbox`       | `-q:v 65` (hardware, fast)                          |
| Windows  | `h264_mf` or `libx264`    | TBD after testing quality                           |
| Linux    | `libx264`                 | `-crf 18 -preset medium`                            |
| Any      | fallback `libx264`        | `-crf 18 -preset medium -pix_fmt yuv420p`           |

- Output is H.264 MP4 by default: plays everywhere, including on phones and in every chat app.
- Where: next to the original as `<name>-cropped.mp4` (`-2`, `-3`... if taken). Screen recordings already live on the Desktop, so their edits land there too. With no crop and no cuts, saving is a straight copy.
- "Advanced" offers: HEVC (smaller), quality slider, keep original frame rate (default) or cap it.
- Pick the encoder by probing `ffmpeg -encoders` once at startup, then test-encode a few frames if hardware encoders are unreliable on the machine.
- Frame rate: keep the source's, including variable frame rate (screen recordings and phones). Add `-fps_mode passthrough` so ffmpeg doesn't duplicate frames.

## Progress and cancel

- `-progress pipe:1` prints `out_time_us=...` lines. Progress = `out_time / total_kept_duration`.
- Cancel: send `q` on stdin (clean stop), then kill after 2 seconds.
- On failure, show a plain message ("Couldn't save the video") with a "Show details" toggle holding the last 30 lines of ffmpeg's stderr.

## Fast save (lossless, time cuts only)

Available only when `crop` is `null`. No quality loss and takes seconds, but cuts snap to keyframes.

```
# list.txt
file 'SRC'
inpoint s0
outpoint e0
file 'SRC'
inpoint s1
outpoint e1
```

```
ffmpeg -f concat -safe 0 -i list.txt -c copy -map 0 -avoid_negative_ts make_zero OUT.mp4
```

- Before offering it, find the keyframes near each cut with `ffprobe -skip_frame nokey -show_frames` (or `-show_packets` flags) and show the user how far each cut moves ("cuts will shift by up to 1.8 s"). If that's under a quarter second, just do it.
- Output keeps the source container and codecs.

## Tests

- Unit: `Edit` to filter-graph text, for zero, one and many cuts, with and without crop and audio.
- Integration: generate short test clips with `ffmpeg -f lavfi -i testsrc2` (with a visible frame counter and a beep in audio), export, then check duration and dimensions with `ffprobe`, and pixel-check a frame just after each cut.
