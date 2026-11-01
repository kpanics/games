"""Her icon (native/assets/icons/recode.ico, its 64 px image) as raw pixels for the title bar (native/src/chrome):
RGBA8, premultiplied (it is filtered down to the bar's size), rows from the top.

    python tools/icon_raw.py   -> native/assets/icons/recode_64.rgba
"""
import io
import os
import struct

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICO = os.path.join(ROOT, "native", "assets", "icons", "recode.ico")
OUT = os.path.join(ROOT, "native", "assets", "icons", "recode_64.rgba")

b = open(ICO, "rb").read()
for i in range(struct.unpack("<H", b[4:6])[0]):
    e = b[6 + i * 16:22 + i * 16]
    size, off = struct.unpack("<II", e[8:16])
    if (e[0] or 256) == 64:
        im = Image.open(io.BytesIO(b[off:off + size])).convert("RGBA")
        px = bytearray()
        for r, g, bl, a in im.getdata():
            px += bytes((r * a // 255, g * a // 255, bl * a // 255, a))
        open(OUT, "wb").write(px)
        print(OUT, im.size, len(px))
