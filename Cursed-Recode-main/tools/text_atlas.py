"""The launch menu's text, as signed distance fields (launch/text.rs draws them, any size, with a glow).

Renders every glyph of the menu's faces at 4x, takes the signed distance to its outline, keeps it at the base size
(EM px to the em, SPREAD px either side), packs them into one atlas.

    python tools/text_atlas.py

-> native/assets/launch/text_atlas.r8.z   (zlib: u32 width, u32 height, then a byte a texel, 128 = the outline,
                                            higher inside: 128 + distance * 127 / SPREAD)
-> src/main/resources/assets/cursed-recode/launch/text.json   (the metrics Java lays text out with)
"""
import json
import os
import struct
import zlib

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from scipy.ndimage import distance_transform_edt

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FONTS = os.path.join(ROOT, "native", "assets", "launch", "fonts")
EM = 48
SPREAD = 12
UP = 4
FACES = ["sofia_250", "sofia_300", "sofia_350", "sofia_450", "martian_400"]
CHARS = "".join(chr(c) for c in range(32, 127)) + "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюя" + "·—–«»…№©"
WIDTH = 2048


def glyph_field(font, ch):
    """The glyph's distance field at the base size and its box: (field, left, top) with left/top of the field's
    corner from the pen at the baseline, base px (y down)."""
    x0, y0, x1, y1 = font.getbbox(ch, anchor="ls")
    pad = SPREAD * UP
    w, h = max(1, x1 - x0) + 2 * pad, max(1, y1 - y0) + 2 * pad
    # whole base px: the field's corner on the base grid
    w += (-w) % UP
    h += (-h) % UP
    img = Image.new("L", (w, h), 0)
    ImageDraw.Draw(img).text((pad - x0, pad - y0), ch, font=font, fill=255, anchor="ls")
    a = np.asarray(img, dtype=np.float32) / 255.0
    inside = a > 0.5
    if inside.any():
        d_out = distance_transform_edt(~inside)
        d_in = distance_transform_edt(inside)
        d = (d_in - d_out) / UP  # base px, + inside
    else:
        d = np.full(a.shape, -SPREAD, dtype=np.float32)
    # the base grid's texel centres
    d = d[UP // 2::UP, UP // 2::UP]
    v = np.clip(128 + d * 127.0 / SPREAD, 0, 255).astype(np.uint8)
    return v, (x0 - pad) / UP, (y0 - pad) / UP


def main():
    faces = {}
    cells = []
    for name in FACES:
        font = ImageFont.truetype(os.path.join(FONTS, name + ".ttf"), EM * UP)
        asc, desc = font.getmetrics()
        glyphs = {}
        for ch in CHARS:
            try:
                field, left, top = glyph_field(font, ch)
            except Exception:
                continue
            adv = font.getlength(ch) / UP
            glyphs[ch] = {"adv": round(adv, 3), "left": left, "top": top}
            cells.append((name, ch, field))
        faces[name] = {"ascent": asc / UP, "descent": desc / UP, "glyphs": glyphs}
    # shelves, tallest first
    cells.sort(key=lambda c: -c[2].shape[0])
    x = y = shelf = 0
    places = []
    for name, ch, field in cells:
        h, w = field.shape
        if x + w > WIDTH:
            x, y, shelf = 0, y + shelf + 1, 0
        places.append((name, ch, field, x, y))
        x += w + 1
        shelf = max(shelf, h)
    height = y + shelf
    atlas = np.zeros((height, WIDTH), dtype=np.uint8)
    for name, ch, field, ax, ay in places:
        h, w = field.shape
        atlas[ay:ay + h, ax:ax + w] = field
        g = faces[name]["glyphs"][ch]
        g["at"] = [ax, ay, w, h]
    out = os.path.join(ROOT, "native", "assets", "launch", "text_atlas.r8.z")
    with open(out, "wb") as f:
        f.write(zlib.compress(struct.pack("<II", WIDTH, height) + atlas.tobytes(), 9))
    meta = {"em": EM, "spread": SPREAD, "atlas": [WIDTH, height], "faces": faces}
    jpath = os.path.join(ROOT, "src", "main", "resources", "assets", "cursed-recode", "launch", "text.json")
    os.makedirs(os.path.dirname(jpath), exist_ok=True)
    with open(jpath, "w", encoding="utf-8", newline="\n") as f:
        json.dump(meta, f, ensure_ascii=False, separators=(",", ":"))
    Image.fromarray(atlas).save(os.path.join(os.environ.get("TEMP", "."), "cursed_text_atlas.png"))
    print(f"{len(cells)} glyphs, atlas {WIDTH}x{height}, {os.path.getsize(out)} bytes packed")


if __name__ == "__main__":
    main()
