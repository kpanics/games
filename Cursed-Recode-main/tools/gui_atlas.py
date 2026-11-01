"""The GUI's text (Geist, Geist Mono; the menus' Sofia Sans Extra Condensed and Martian Mono), as signed distance
fields: native/src/gui draws them at any size.

Same fields as the launch's (tools/text_atlas.py): every glyph rendered at 4x, the signed distance to its outline kept
at the base size (EM px to the em, SPREAD px either side), all packed into one atlas.

    python tools/gui_atlas.py

-> native/assets/gui/text_atlas.r8.z   (zlib: u32 width, u32 height, then a byte a texel, 128 = the outline,
                                         higher inside: 128 + distance * 127 / SPREAD)
-> native/assets/gui/text.bin           (the metrics, little-endian: u32 faces; a face: f32 ascent, descent,
                                         u32 glyphs; a glyph: u32 char, f32 advance, left, top, u16 x y w h)
"""
import os
import struct
import zlib

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from scipy.ndimage import distance_transform_edt
from fontTools.ttLib import TTFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FONTS = os.path.join(ROOT, "native", "assets", "gui", "fonts")
LAUNCH_FONTS = os.path.join(ROOT, "native", "assets", "launch", "fonts")
OUT = os.path.join(ROOT, "native", "assets", "gui")
EM = 48
# as the launch's (its words glow as far out as the menus' do)
SPREAD = 12
UP = 4
# order = the face's number in native/src/gui/text.rs
FACES = ["geist_400", "geist_500", "geist_600", "geist_700", "geistmono_500", "sofia_250", "sofia_300", "sofia_350", "sofia_450", "martian_400"]
# what the game's languages write most (Latin with its accents, Cyrillic), the client's own marks; what a face lacks
# is drawn from the system's fonts at run time (native/src/gui/text.rs)
CHARS = ("".join(chr(c) for c in range(32, 127)) + "".join(chr(c) for c in range(0xA0, 0x180)) + "".join(chr(c) for c in range(0x400, 0x460))
         + "ґҐєЄіІїЇўЎ" + "·—–«»…№©×≈♥°→←↑↓•‹›‘’“”„‚‐−±÷≤≥∞✓✕↺⏎€₽")
WIDTH = 2048


def glyph_field(font, ch):
    x0, y0, x1, y1 = font.getbbox(ch, anchor="ls")
    pad = SPREAD * UP
    w, h = max(1, x1 - x0) + 2 * pad, max(1, y1 - y0) + 2 * pad
    w += (-w) % UP
    h += (-h) % UP
    img = Image.new("L", (w, h), 0)
    ImageDraw.Draw(img).text((pad - x0, pad - y0), ch, font=font, fill=255, anchor="ls")
    a = np.asarray(img, dtype=np.float32) / 255.0
    inside = a > 0.5
    if inside.any():
        d = (distance_transform_edt(inside) - distance_transform_edt(~inside)) / UP
    else:
        d = np.full(a.shape, -SPREAD, dtype=np.float32)
    d = d[UP // 2::UP, UP // 2::UP]
    v = np.clip(128 + d * 127.0 / SPREAD, 0, 255).astype(np.uint8)
    return v, (x0 - pad) / UP, (y0 - pad) / UP


def main():
    faces = []
    cells = []
    for fi, name in enumerate(FACES):
        path = os.path.join(FONTS, name + ".ttf")
        font = ImageFont.truetype(path if os.path.exists(path) else os.path.join(LAUNCH_FONTS, name + ".ttf"), EM * UP)
        have = set(TTFont(path if os.path.exists(path) else os.path.join(LAUNCH_FONTS, name + ".ttf")).getBestCmap())
        asc, desc = font.getmetrics()
        glyphs = {}
        for ch in CHARS:
            if ord(ch) not in have or (ch not in "  " and font.getmask(ch).getbbox() is None):
                continue
            field, left, top = glyph_field(font, ch)
            glyphs[ch] = {"adv": font.getlength(ch) / UP, "left": left, "top": top}
            cells.append((fi, ch, field))
        faces.append({"ascent": asc / UP, "descent": desc / UP, "glyphs": glyphs})
    cells.sort(key=lambda c: -c[2].shape[0])
    x = y = shelf = 0
    places = []
    for fi, ch, field in cells:
        h, w = field.shape
        if x + w > WIDTH:
            x, y, shelf = 0, y + shelf + 1, 0
        places.append((fi, ch, field, x, y))
        x += w + 1
        shelf = max(shelf, h)
    height = y + shelf
    atlas = np.zeros((height, WIDTH), dtype=np.uint8)
    for fi, ch, field, ax, ay in places:
        h, w = field.shape
        atlas[ay:ay + h, ax:ax + w] = field
        faces[fi]["glyphs"][ch]["at"] = [ax, ay, w, h]
    with open(os.path.join(OUT, "text_atlas.r8.z"), "wb") as f:
        f.write(zlib.compress(struct.pack("<II", WIDTH, height) + atlas.tobytes(), 9))
    b = bytearray(struct.pack("<I", len(faces)))
    for face in faces:
        b += struct.pack("<ffI", face["ascent"], face["descent"], len(face["glyphs"]))
        for ch, g in face["glyphs"].items():
            b += struct.pack("<Ifff4H", ord(ch), g["adv"], g["left"], g["top"], *g["at"])
    with open(os.path.join(OUT, "text.bin"), "wb") as f:
        f.write(b)
    Image.fromarray(atlas).save(os.path.join(os.environ.get("TEMP", "."), "cursed_gui_atlas.png"))
    print(f"{len(cells)} glyphs, atlas {WIDTH}x{height}, {os.path.getsize(os.path.join(OUT, 'text_atlas.r8.z'))} bytes packed, metrics {len(b)} bytes")


if __name__ == "__main__":
    main()
