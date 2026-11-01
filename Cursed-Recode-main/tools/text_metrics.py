"""The launch text's metrics (src/main/resources/assets/cursed-recode/launch/text.json, made by tools/text_atlas.py) in
a form the DLL reads without a JSON parser: the window's own title bar (native/src/chrome) sets its text natively,
also in the window before the game, where there is no Java.

    python tools/text_metrics.py

-> native/assets/launch/text_metrics.bin   little-endian: "CTM1", u32 em, u32 spread, u32 atlas width, u32 atlas height,
   u32 faces; a face: u8 name length, the name, f32 ascent, f32 descent, u32 glyphs; a glyph: u32 code point, f32 advance,
   f32 left, f32 top, u16 x, y, w, h (its field in the atlas)
"""
import json
import os
import struct

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "src", "main", "resources", "assets", "cursed-recode", "launch", "text.json")
DST = os.path.join(ROOT, "native", "assets", "launch", "text_metrics.bin")


def main():
    d = json.load(open(SRC, encoding="utf-8"))
    out = bytearray(b"CTM1")
    out += struct.pack("<IIIII", d["em"], d["spread"], d["atlas"][0], d["atlas"][1], len(d["faces"]))
    for name, face in d["faces"].items():
        nb = name.encode()
        out += struct.pack("<B", len(nb)) + nb
        out += struct.pack("<ffI", face["ascent"], face["descent"], len(face["glyphs"]))
        for ch, g in sorted(face["glyphs"].items(), key=lambda kv: ord(kv[0])):
            out += struct.pack("<Ifff4H", ord(ch), g["adv"], g["left"], g["top"], *g["at"])
    open(DST, "wb").write(out)
    print(DST, len(out), "bytes")


if __name__ == "__main__":
    main()
