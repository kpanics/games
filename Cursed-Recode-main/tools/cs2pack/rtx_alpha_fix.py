"""The sRGB textures made before rtx.py kept their alpha (it was pushed to 255 wherever it was not 0) made again from
their pictures: those whose picture has an alpha of its own (a custom paint's wear mask, an icon's soft edge), at the
size they were.

    python rtx_alpha_fix.py OUTDIR [WORK]

WORK (D:\\CS2\\work by default) is where the pictures VRF wrote are: found by the texture's name (an icon's by its
item and paint)."""
import os
import struct
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rtx  # noqa: E402


def head(path):
    with open(path, "rb") as f:
        d = f.read(20)
    if d[:4] != b"RTX1":
        return None
    return struct.unpack("<4I", d[4:20])


def pictures(work, out):
    """Every picture under WORK by its name (the packed folder left out; links not followed)."""
    found = {}
    skip = os.path.normcase(os.path.abspath(out))
    for root, dirs, files in os.walk(work):
        dirs[:] = [d for d in dirs if os.path.normcase(os.path.abspath(os.path.join(root, d))) != skip and not os.path.islink(os.path.join(root, d))]
        for f in files:
            if f.endswith(".png"):
                found.setdefault(os.path.splitext(f)[0], os.path.join(root, f))
    return found


def fix(out, work=r"D:\CS2\work"):
    pics = pictures(work, out)
    done, kept, lost = 0, 0, []
    for root, _, files in os.walk(out):
        for f in files:
            if not f.endswith(".rtx"):
                continue
            path = os.path.join(root, f)
            h = head(path)
            if not h or not h[3] & 1:
                continue
            stem = os.path.splitext(f)[0]
            rel = os.path.relpath(path, out).replace("\\", "/").split("/")
            if rel[0] == "icons" and len(rel) == 3:
                stem = rel[1] if stem == "default" else f"{rel[1]}_{stem}"
            src = pics.get(stem)
            if not src:
                lost.append(path)
                continue
            a = np.asarray(Image.open(src).convert("RGBA"))[..., 3]
            if a.min() == 255:
                kept += 1
                continue
            rtx.convert(src, path, max(h[0], h[1]), True)
            done += 1
            print("made again", os.path.relpath(path, out), flush=True)
    return {"made again": done, "no alpha of their own": kept, "no picture": len(lost), "lost": lost[:10]}


if __name__ == "__main__":
    print(fix(*sys.argv[1:]))
