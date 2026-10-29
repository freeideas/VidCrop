# Releases

Downloads live at https://62-84-178-253.sslip.io/VidCrop/, not on GitHub: the repository is private, and GitHub release files would be private too. Same pattern as DecisionGator.

## One command

```sh
uv run tools/release.py                  # build Mac, Linux, Windows (at the same time), then publish
uv run tools/release.py --only windows   # rebuild one platform (repeatable), then publish
uv run tools/release.py --publish-only
```

It builds the committed HEAD, so commit first. Bump `version` in `src-tauri/tauri.conf.json` for a new release.

## Where each platform builds

| Platform | Machine                                 | Output                           |
|----------|-----------------------------------------|----------------------------------|
| macOS    | this Mac (Apple Silicon)                | `.dmg`                           |
| Linux    | none yet (emeraldslate is gone) | `.AppImage` and `.deb`    |
| Windows  | none yet (see below)            | NSIS `setup.exe`           |

- The source is sent with `git archive HEAD` over SSH; no GitHub login is needed on the build machines.
- Out of date since 2026-10-26: the Linux and Windows build machines were on emeraldslate (Linux builds via `ssh ace@emeraldslate` in Docker, Windows builds in the `omarchy-windows` guest as `ssh emeraldslate-windows`). Its disk was wiped; it now runs Windows 11 Pro with no Linux, Docker or VMs, and that ssh alias is removed. A new Linux build machine is not set up yet. EmeraldSlate (`ssh human@emeraldslate`, `vnc://EmeraldSlate`) could serve for Windows builds once Rust, Node, uv and the Visual Studio build tools are installed there. The scripts `tools/build-windows.cmd` and `tools/linux-build/Dockerfile` remain as the recipe: Windows builds ran as a scheduled task because programs started from an SSH session die when it closes, and Linux builds used Ubuntu 22.04 because an AppImage only runs on systems at least as old as the one it was built on.

## Bundled ffmpeg

`tools/fetch_ffmpeg.py` downloads standalone GPL ffmpeg 9.0 builds (Martin Riedl's for macOS, BtbN's for Windows and Linux), verifies their checksums, and saves them as `src-tauri/binaries/vidcrop-ffmpeg-<target>`. `src-tauri/tauri.release.json` adds them to the bundle along with `notices/FFMPEG.md`. The `vidcrop-` prefix keeps the Linux `.deb` from clashing with a system ffmpeg package. The app looks for `vidcrop-ffmpeg` next to its own executable before anything else (`crates/core/src/ffmpeg.rs`). Development builds (`npx tauri dev`) skip bundling and use the ffmpeg on the PATH.

## Publishing

`publish` writes `SHA256SUMS.txt`, copies the files to `/var/www/textautomationlib/VidCrop/files/<version>/` on the web VM with rsync, and replaces `/VidCrop/index.html` with a download page built from `tools/download-page.html`. nginx already serves that folder; no server changes were needed.

## Not signed

Releases are unsigned (no Apple Developer ID or Windows code-signing certificate), so macOS and Windows warn on first launch. The download page explains how to get past it.
