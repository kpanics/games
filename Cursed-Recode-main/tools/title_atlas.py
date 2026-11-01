"""The launch's title (v12): CURSED, then RECODE, one letter a cell of 160 x 224 px, Sofia Sans Extra Condensed 300 at
176 px to the em, the baseline at 190 (as the preview's buildAtlas). Drawn at 4x and averaged down: its coverage.

    python tools/title_atlas.py

-> native/assets/launch/title_atlas.r8.z   (zlib: u32 width, u32 height, f32 the cap's height (px over the baseline),
                                             then a byte a texel)
"""
import os
import struct
import zlib

import numpy as np
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FONT = os.path.join(ROOT, "native", "assets", "launch", "fonts", "sofia_300.ttf")
OUT = os.path.join(ROOT, "native", "assets", "launch", "title_atlas.r8.z")
CW, CH, BL, SIZE, UP = 160, 224, 190, 176, 4
WORD = "CURSEDRECODE"


def main():
    font = ImageFont.truetype(FONT, SIZE * UP)
    img = Image.new("L", (CW * len(WORD) * UP, CH * UP), 0)
    d = ImageDraw.Draw(img)
    for i, ch in enumerate(WORD):
        d.text(((i * CW + CW / 2) * UP, BL * UP), ch, font=font, fill=255, anchor="ms")
    a = np.asarray(img, dtype=np.float32).reshape(CH, UP, CW * len(WORD), UP).mean(axis=(1, 3))
    px = np.clip(a + 0.5, 0, 255).astype(np.uint8)
    x0, y0, x1, y1 = font.getbbox("E", anchor="ls")
    cap = -y0 / UP
    w, h = CW * len(WORD), CH
    data = struct.pack("<IIf", w, h, cap) + px.tobytes()
    open(OUT, "wb").write(zlib.compress(data, 9))
    Image.fromarray(px).save(os.path.join(ROOT, "tools", "out", "title_atlas.png"))
    print(w, h, "cap", cap, len(data) // 1024, "KB")


if __name__ == "__main__":
    main()
