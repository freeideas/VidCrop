# Releases

Downloads live at https://62-84-178-253.sslip.io/VidCrop/, not on GitHub: the repository is private, and GitHub release files would be private too. Same pattern as DecisionGator.

## One command

```sh
uv run tools/release.py                  # build Mac, Linux, Windows, then publish
uv run tools/release.py --only windows   # rebuild one platform (repeatable), then publish
uv run tools/release.py --publish-only
```

It builds the committed HEAD, so commit first. Bump `version` in `src-tauri/tauri.conf.json` for a new release.

## Where each platform builds

| Platform | Machine                                 | Output                           |
|----------|-----------------------------------------|----------------------------------|
| macOS    | this Mac (Apple Silicon)                | `.dmg`                           |
| Linux    | `ssh ace@emeraldslate`, in an Ubuntu 22.04 container | `.AppImage` and `.deb`    |
| Windows  | `ssh emeraldslate-windows` (Windows 11 guest) | NSIS `setup.exe`           |

- The source is sent with `git archive HEAD` over SSH; no GitHub login is needed on the build machines.
- The Windows guest runs in emeraldslate's `omarchy-windows` Docker container. Windows builds run as a scheduled task (`tools/build-windows.cmd`), because programs started from an SSH session die when it closes. The guest has Rust (via winget's rustup), Node, uv and the Visual Studio build tools.
- Linux builds run in Docker from `tools/linux-build/Dockerfile` (Ubuntu 22.04), because an AppImage only runs on systems at least as old as the one it was built on. emeraldslate is short on memory while the Windows guest runs, so builds use 3 parallel jobs. Docker there is not started at boot; the script uses `sudo docker` when needed.
- DesktopIA can reach emeraldslate's screen (`vnc://EmeraldSlate`) if a build machine ever waits on a dialog.

## Bundled ffmpeg

`tools/fetch_ffmpeg.py` downloads standalone GPL ffmpeg 9.0 builds (Martin Riedl's for macOS, BtbN's for Windows and Linux), verifies their checksums, and saves them as `src-tauri/binaries/vidcrop-ffmpeg-<target>`. `src-tauri/tauri.release.json` adds them to the bundle along with `notices/FFMPEG.md`. The `vidcrop-` prefix keeps the Linux `.deb` from clashing with a system ffmpeg package. The app looks for `vidcrop-ffmpeg` next to its own executable before anything else (`crates/core/src/ffmpeg.rs`). Development builds (`npx tauri dev`) skip bundling and use the ffmpeg on the PATH.

## Publishing

`publish` writes `SHA256SUMS.txt`, copies the files to `/var/www/textautomationlib/VidCrop/files/<version>/` on the web VM with rsync, and replaces `/VidCrop/index.html` with a download page built from `tools/download-page.html`. nginx already serves that folder; no server changes were needed.

## Not signed

Releases are unsigned (no Apple Developer ID or Windows code-signing certificate), so macOS and Windows warn on first launch. The download page explains how to get past it.
