#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Download standalone ffmpeg + ffprobe for bundling into a VidCrop release.

Writes src-tauri/binaries/vidcrop-ffmpeg-<target>[.exe] and vidcrop-ffprobe-... (the names
Tauri's externalBin wants) and records where they came from in SOURCES-<target>.txt.
Checksums published next to each download are verified.

  uv run tools/fetch_ffmpeg.py                  # for this computer
  uv run tools/fetch_ffmpeg.py --target x86_64-pc-windows-msvc

Sources (GPL builds of FFmpeg 9.0):
  macOS:   https://ffmpeg.martin-riedl.de  (single static binaries)
  Windows, Linux: https://github.com/BtbN/FFmpeg-Builds  (static builds)
"""
import argparse
import hashlib
import io
import json
import re
import subprocess
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "src-tauri" / "binaries"
FFMPEG_SERIES = "9.0"


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "vidcrop-release"})
    with urllib.request.urlopen(req, timeout=300) as r:
        return r.read(), r.geturl()


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def host_target():
    out = subprocess.check_output(["rustc", "-vV"], text=True)
    return re.search(r"^host: (\S+)", out, re.M).group(1)


def martin_riedl(arch):
    """macOS: one zip per tool, each with a .sha256 file beside it."""
    files = {}
    for tool in ("ffmpeg", "ffprobe"):
        data, url = get(f"https://ffmpeg.martin-riedl.de/redirect/latest/macos/{arch}/release/{tool}.zip")
        want = get(url + ".sha256")[0].decode().split()[0]
        if sha256(data) != want:
            sys.exit(f"checksum mismatch for {url}")
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            files[tool] = (z.read(tool), url)
    return files


def btbn(platform):
    """Windows/Linux: one archive with bin/ffmpeg and bin/ffprobe, listed in checksums.sha256."""
    release = json.loads(get("https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/tags/latest")[0])
    assets = {a["name"]: a["browser_download_url"] for a in release["assets"]}
    ext = "zip" if platform == "win64" else "tar.xz"
    name = f"ffmpeg-n{FFMPEG_SERIES}-latest-{platform}-gpl-{FFMPEG_SERIES}.{ext}"
    if name not in assets:
        sys.exit(f"{name} is not in BtbN's latest release")
    data, url = get(assets[name])
    sums = get(assets["checksums.sha256"])[0].decode()
    want = next((l.split()[0] for l in sums.splitlines() if l.strip().endswith(name)), None)
    if want != sha256(data):
        sys.exit(f"checksum mismatch for {name}")
    files = {}
    exe = ".exe" if platform == "win64" else ""
    if ext == "zip":
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            for tool in ("ffmpeg", "ffprobe"):
                member = next(n for n in z.namelist() if n.endswith(f"/bin/{tool}{exe}"))
                files[tool] = (z.read(member), url)
    else:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:xz") as t:
            for tool in ("ffmpeg", "ffprobe"):
                member = next(m for m in t.getmembers() if m.name.endswith(f"/bin/{tool}"))
                files[tool] = (t.extractfile(member).read(), url)
    return files


SOURCES = {
    "aarch64-apple-darwin": lambda: martin_riedl("arm64"),
    "x86_64-apple-darwin": lambda: martin_riedl("amd64"),
    "x86_64-pc-windows-msvc": lambda: btbn("win64"),
    "x86_64-unknown-linux-gnu": lambda: btbn("linux64"),
}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--target", help="Rust target triple (default: this computer)")
    target = ap.parse_args().target or host_target()
    if target not in SOURCES:
        sys.exit(f"no ffmpeg source for {target}; known: {', '.join(SOURCES)}")
    OUT.mkdir(parents=True, exist_ok=True)
    exe = ".exe" if "windows" in target else ""
    lines = [f"ffmpeg/ffprobe for {target}. FFmpeg is GPL; source code: https://git.ffmpeg.org/ffmpeg.git"]
    for tool, (data, url) in SOURCES[target]().items():
        path = OUT / f"vidcrop-{tool}-{target}{exe}"
        path.write_bytes(data)
        path.chmod(0o755)
        lines.append(f"{sha256(data)}  {path.name}  from {url}")
        print(f"{path.relative_to(ROOT)}  {len(data) // 1_000_000} MB")
    (OUT / f"SOURCES-{target}.txt").write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
