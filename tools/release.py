#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Build VidCrop for macOS, Linux and Windows, and publish the downloads.

Run on the Mac. Linux builds on emeraldslate and Windows in its Windows guest, both over
SSH; the source sent is the committed HEAD, so commit first. Results are collected in
released/<version>/, then published to https://62-84-178-253.sslip.io/VidCrop/.

  uv run tools/release.py                    # build all three and publish
  uv run tools/release.py --only linux       # just one build (repeatable)
  uv run tools/release.py --publish-only     # publish what's already in released/<version>/
  uv run tools/release.py --only windows --collect   # wait for a build already running, fetch it

Machines (override with env vars): VIDCROP_LINUX_HOST=ace@emeraldslate,
VIDCROP_WINDOWS_HOST=emeraldslate-windows, VIDCROP_WEB_HOST=ace@62.84.178.253.
"""
import argparse
import hashlib
import html
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LINUX = os.environ.get("VIDCROP_LINUX_HOST", "ace@emeraldslate")
WINDOWS = os.environ.get("VIDCROP_WINDOWS_HOST", "emeraldslate-windows")
WEB = os.environ.get("VIDCROP_WEB_HOST", "ace@62.84.178.253")
WEB_DIR = "/var/www/textautomationlib/VidCrop"
WEB_URL = "https://62-84-178-253.sslip.io/VidCrop"
WIN_DIR = r"D:\VidCrop-build"  # D: is a build drive; the guest's C: is nearly full
SSH = ["ssh", "-o", "BatchMode=yes", "-o", "ServerAliveInterval=30"]


def version():
    return json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())["version"]


def run(cmd, **kw):
    print("+", cmd if isinstance(cmd, str) else " ".join(cmd), flush=True)
    return subprocess.run(cmd, check=True, **kw)


def ssh(host, command, **kw):
    return run(SSH + [host, command], **kw)


def send_source(host, extract):
    """Streams `git archive HEAD` to the host, where `extract` unpacks it from stdin."""
    if subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True).stdout.strip():
        print("note: uncommitted changes are NOT included; the build uses HEAD", flush=True)
    archive = subprocess.Popen(["git", "archive", "HEAD"], cwd=ROOT, stdout=subprocess.PIPE)
    run(SSH + [host, extract], stdin=archive.stdout)
    archive.wait()


def wait_for_log(host, read_log, label, timeout_min=120):
    """Polls a remote log until it ends with BUILD-OK or BUILD-FAILED."""
    t0, last = time.time(), ""
    while time.time() - t0 < timeout_min * 60:
        time.sleep(30)
        out = subprocess.run(SSH + [host, read_log], capture_output=True, text=True, errors="replace").stdout.replace("\r", "")
        lines = [l for l in out.splitlines() if l.strip()]
        tail = lines[-1] if lines else ""
        if tail != last:
            print(f"[{label} {int(time.time() - t0) // 60}m] {tail[:150]}", flush=True)
            last = tail
        if "BUILD-OK" in out:
            return
        if "BUILD-FAILED" in out:
            print("\n".join(lines[-40:]))
            sys.exit(f"{label} build failed")
    sys.exit(f"{label} build timed out")


def build_mac(out):
    run(["uv", "run", "-q", "tools/fetch_ffmpeg.py"], cwd=ROOT)
    run(["npm", "ci", "--no-audit", "--no-fund"], cwd=ROOT)
    # CI=true stops the DMG step from scripting Finder to lay out the window.
    run(["npx", "tauri", "build", "--bundles", "app,dmg", "--config", "src-tauri/tauri.release.json"], cwd=ROOT,
        env={**os.environ, "CI": "true"})
    dmg = next((ROOT / "target/release/bundle/dmg").glob(f"VidCrop_{version()}_*.dmg"))
    shutil.copy2(dmg, out / f"VidCrop-{version()}-macos-arm64.dmg")


def _start_linux():
    docker = "$(docker info >/dev/null 2>&1 && echo docker || echo 'sudo -n docker')"
    # The container runs as root and hands the files back only when a build finishes, so an
    # interrupted build leaves root-owned files that only a container can delete.
    ssh(LINUX, f"rm -rf ~/build/VidCrop 2>/dev/null || {{ D={docker}; "
               "$D run --rm -v ~/build:/b ubuntu:22.04 rm -rf /b/VidCrop; }")
    send_source(LINUX, "mkdir -p ~/build/VidCrop && tar -x -C ~/build/VidCrop")
    ssh(LINUX, f"D={docker}; cd ~/build/VidCrop && $D build -q -t vidcrop-linux-build tools/linux-build")
    inner = (
        "(npm ci --no-audit --no-fund && uv run -q tools/fetch_ffmpeg.py && "
        "npx tauri build --bundles appimage,deb --config src-tauri/tauri.release.json "
        "&& echo BUILD-OK || echo BUILD-FAILED) > /src/build.log 2>&1; chown -R $(id -u):$(id -g) /src"
    )
    ssh(LINUX, f"D={docker}; $D run --rm -d --name vidcrop-build -v ~/build/VidCrop:/src "
               "-v vidcrop-cargo:/root/.cargo/registry -e APPIMAGE_EXTRACT_AND_RUN=1 -e CARGO_BUILD_JOBS=3 "
               f'vidcrop-linux-build bash -c "{inner}"')


def fetch_linux(out):
    # Built in an Ubuntu 22.04 container (tools/linux-build/Dockerfile): an AppImage only runs
    # on systems at least as old as where it was built, and Arch is too new.
    wait_for_log(LINUX, "tail -c 4000 ~/build/VidCrop/build.log", "linux")
    v = version()
    bundle = "~/build/VidCrop/target/release/bundle"
    run(["scp", "-o", "BatchMode=yes", f"{LINUX}:{bundle}/appimage/VidCrop_{v}_amd64.AppImage",
         str(out / f"VidCrop-{v}-linux-x86_64.AppImage")])
    run(["scp", "-o", "BatchMode=yes", f"{LINUX}:{bundle}/deb/VidCrop_{v}_amd64.deb", str(out / f"vidcrop_{v}_amd64.deb")])


def _start_windows():
    ssh(WINDOWS, f"(if exist {WIN_DIR} rmdir /s /q {WIN_DIR}) & mkdir {WIN_DIR}")
    send_source(WINDOWS, f"tar -x -C {WIN_DIR}")
    ssh(WINDOWS, f'schtasks /create /tn VidCropBuild /sc once /st 00:00 /f /tr "{WIN_DIR}\\tools\\build-windows.cmd"'
                 " && schtasks /run /tn VidCropBuild")


def fetch_windows(out):
    wait_for_log(WINDOWS, f'powershell -NoProfile -Command "Get-Content {WIN_DIR}\\build.log -Tail 40"', "windows")
    v = version()
    src = f"{WINDOWS}:{WIN_DIR.replace(chr(92), '/')}/target/release/bundle/nsis/VidCrop_{v}_x64-setup.exe"
    run(["scp", "-o", "BatchMode=yes", src, str(out / f"VidCrop-{v}-windows-x64-setup.exe")])


def sha256(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        while chunk := f.read(1 << 20):
            h.update(chunk)
    return h.hexdigest()


def publish(out):
    v = version()
    files = sorted(p for p in out.iterdir() if p.is_file() and p.name not in ("SHA256SUMS.txt", "index.html"))
    if not files:
        sys.exit(f"nothing to publish in {out}")
    (out / "SHA256SUMS.txt").write_text(
        "# VidCrop release files. Check with: sha256sum -c SHA256SUMS.txt\n"
        + "".join(f"{sha256(p)}  {p.name}\n" for p in files))
    page = (ROOT / "tools/download-page.html").read_text()
    rows = "\n".join(
        f'<li><a href="files/{v}/{html.escape(p.name)}">{html.escape(p.name)}</a> '
        f'<span>{p.stat().st_size / 1e6:.0f} MB</span></li>' for p in files)
    (out / "index.html").write_text(page.replace("{{VERSION}}", v).replace("{{FILES}}", rows))
    ssh(WEB, f"mkdir -p {WEB_DIR}/files/{v}")
    run(["rsync", "-av", "--exclude", "index.html", f"{out}/", f"{WEB}:{WEB_DIR}/files/{v}/"])
    run(["rsync", "-av", str(out / "index.html"), f"{WEB}:{WEB_DIR}/index.html"])
    print(f"published: {WEB_URL}/")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--only", action="append", choices=["mac", "linux", "windows"])
    ap.add_argument("--publish-only", action="store_true")
    ap.add_argument("--no-publish", action="store_true")
    ap.add_argument("--collect", action="store_true", help="don't start builds; wait for ones already running and fetch the results")
    a = ap.parse_args()
    out = ROOT / "released" / version()
    out.mkdir(parents=True, exist_ok=True)
    if not a.publish_only:
        names = a.only or ["mac", "linux", "windows"]
        remote = [n for n in ("linux", "windows") if n in names]
        # Linux and Windows build on their own machines, so start both, build the Mac one
        # here meanwhile, then collect. A failed build stops the script; the other keeps
        # running remotely and can be fetched with --collect.
        if not a.collect:
            for name in remote:
                {"linux": _start_linux, "windows": _start_windows}[name]()
        if "mac" in names:
            build_mac(out)
        for name in remote:
            {"linux": fetch_linux, "windows": fetch_windows}[name](out)
    if not a.no_publish:
        publish(out)


if __name__ == "__main__":
    main()
