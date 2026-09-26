#!/usr/bin/env -S uv run --script
# /// script
# dependencies = ["pillow"]
# ///
from PIL import Image, ImageDraw
import sys

# Usage: tools/make_icon.py out.png, then: npx tauri icon out.png -o src-tauri/icons

K = 4  # draw big, shrink for smooth edges
N = 1024 * K
def s(*v): return [x * K for x in v]

img = Image.new("RGBA", (N, N), (0, 0, 0, 0))
d = ImageDraw.Draw(img)
d.rounded_rectangle(s(64, 64, 960, 960), radius=200 * K, fill="#1d2433")

# Film strip
top, bot = 300, 724
d.rectangle(s(64, top, 960, bot), fill="#0e1016")
for i in range(8):
    x = 92 + i * 112
    for y in (top + 22, bot - 58):
        d.rounded_rectangle(s(x, y, x + 56, y + 36), radius=8 * K, fill="#c9ced9")

# Three frames with little landscapes
frames = [(96, 360), (380, 644), (664, 928)]
skies = [("#3a6fd8", "#9cc4ff"), ("#ff8a3d", "#ffd36b"), ("#6a4bc4", "#c79bff")]
ft, fb = top + 78, bot - 78
for (x0, x1), (c0, c1) in zip(frames, skies):
    frame = Image.new("RGBA", (x1 - x0, fb - ft))
    fd = ImageDraw.Draw(frame)
    h = fb - ft
    a, b = [tuple(int(c[i:i + 2], 16) for i in (1, 3, 5)) for c in (c0, c1)]
    for y in range(h):
        t = y / h
        fd.line([(0, y), (x1 - x0, y)], fill=tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3)))
    w = x1 - x0
    fd.polygon([(0, h), (w * 0.35, h * 0.45), (w * 0.6, h * 0.75), (w * 0.8, h * 0.55), (w, h * 0.8), (w, h)], fill="#1f3a2c")
    fd.ellipse([w * 0.65, h * 0.12, w * 0.85, h * 0.32], fill="#fff6d6")
    img.paste(frame.resize(((x1 - x0) * K, h * K)), (x0 * K, ft * K))

# Dim everything outside the crop
cx0, cy0, cx1, cy1 = 350, 250, 674, 774
shade = Image.new("RGBA", (N, N), (0, 0, 0, 0))
sd = ImageDraw.Draw(shade)
sd.rectangle(s(0, 0, 1024, 1024), fill=(0, 0, 0, 140))
sd.rectangle(s(cx0, cy0, cx1, cy1), fill=(0, 0, 0, 0))
mask = Image.new("L", (N, N), 0)
ImageDraw.Draw(mask).rounded_rectangle(s(64, 64, 960, 960), radius=200 * K, fill=255)
shade.putalpha(Image.composite(shade.getchannel("A"), Image.new("L", (N, N), 0), mask))
img = Image.alpha_composite(img, shade)

# Amber crop corners
d = ImageDraw.Draw(img)
amber, t, L = "#ffb020", 34, 120
for (x, y, dx, dy) in [(cx0, cy0, 1, 1), (cx1, cy0, -1, 1), (cx0, cy1, 1, -1), (cx1, cy1, -1, -1)]:
    d.rectangle(s(min(x, x + dx * L), min(y, y + dy * t), max(x, x + dx * L), max(y, y + dy * t)), fill=amber)
    d.rectangle(s(min(x, x + dx * t), min(y, y + dy * L), max(x, x + dx * t), max(y, y + dy * L)), fill=amber)

img.resize((1024, 1024), Image.LANCZOS).save(sys.argv[1])
