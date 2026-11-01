"""A texture (a PNG as VRF exports Source 2's) made into ours (.rtx, native/src/cosmetics/texture.rs reads it): RGBA8
with its whole mip chain, at most `--max` texels a side, compressed with zlib.

    python rtx.py IN.png OUT.rtx [--max 1024] [--srgb]

The layout: "RTX1", width, height, mips (u32 each), flags (u32: bit 0 its colour is sRGB — the mips are made in
linear light), then zlib of every mip's RGBA8 one after another (the largest first)."""
import struct
import sys
import zlib

import numpy as np
from PIL import Image


def to_linear(c):
    c = c / 255.0
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def to_srgb(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.power(c, 1 / 2.4) - 0.055) * 255.0


def mips(img, srgb):
    a = np.asarray(img.convert("RGBA"), dtype=np.float64)
    if srgb:
        # the colour into linear light; the alpha (no colour: a custom paint's wear mask, an icon's edge) as it is
        a = np.concatenate([to_linear(a[..., :3]), a[..., 3:] / 255.0], axis=-1)
    else:
        a = a / 255.0
    chain = [a]
    while chain[-1].shape[0] > 1 or chain[-1].shape[1] > 1:
        p = chain[-1]
        h, w = p.shape[:2]
        # odd sizes: the last row or column left out (each mip floor(size / 2), as Vulkan and texture.rs count them)
        if h % 2 and h > 1:
            p = p[:-1]
        if w % 2 and w > 1:
            p = p[:, :-1]
        h2, w2 = max(1, p.shape[0] // 2), max(1, p.shape[1] // 2)
        if p.shape[0] == 1:
            q = (p[:, 0::2] + p[:, 1::2]) / 2
        elif p.shape[1] == 1:
            q = (p[0::2] + p[1::2]) / 2
        else:
            q = (p[0::2, 0::2] + p[1::2, 0::2] + p[0::2, 1::2] + p[1::2, 1::2]) / 4
        chain.append(q.reshape(h2, w2, 4))
    out = []
    for m in chain:
        if srgb:
            c = np.concatenate([to_srgb(m[..., :3]), m[..., 3:] * 255.0], axis=-1)
        else:
            c = m * 255.0
        out.append(np.clip(np.round(c), 0, 255).astype(np.uint8))
    return out


def convert(src, dst, most=1024, srgb=False):
    """`src` a file, or a picture made here (pack_model.py's own packing)."""
    img = src if isinstance(src, Image.Image) else Image.open(src)
    w, h = img.size
    k = max(w, h) / most
    if k > 1:
        img = img.resize((max(1, round(w / k)), max(1, round(h / k))), Image.LANCZOS)
    chain = mips(img, srgb)
    raw = b"".join(m.tobytes() for m in chain)
    body = zlib.compress(raw, 9)
    data = b"RTX1" + struct.pack("<4I", chain[0].shape[1], chain[0].shape[0], len(chain), 1 if srgb else 0) + body
    open(dst, "wb").write(data)
    return {"size": (chain[0].shape[1], chain[0].shape[0]), "mips": len(chain), "bytes": len(data)}


def convert_float(rgba, dst, most=1024):
    """A picture of floats 0..1 (H×W×4: a weapon's positions) as 16 bits a channel (flags bit 1), its mips
    averaged."""
    a = np.asarray(rgba, dtype=np.float64)
    k = max(a.shape[0], a.shape[1]) / most
    while k > 1.0:
        a = (a[0::2, 0::2] + a[1::2, 0::2] + a[0::2, 1::2] + a[1::2, 1::2]) / 4
        k /= 2
    chain = [a]
    while chain[-1].shape[0] > 1 or chain[-1].shape[1] > 1:
        p = chain[-1]
        if p.shape[0] % 2 and p.shape[0] > 1:
            p = p[:-1]
        if p.shape[1] % 2 and p.shape[1] > 1:
            p = p[:, :-1]
        if p.shape[0] == 1:
            q = (p[:, 0::2] + p[:, 1::2]) / 2
        elif p.shape[1] == 1:
            q = (p[0::2] + p[1::2]) / 2
        else:
            q = (p[0::2, 0::2] + p[1::2, 0::2] + p[0::2, 1::2] + p[1::2, 1::2]) / 4
        chain.append(q)
    raw = b"".join(np.clip(np.round(m * 65535.0), 0, 65535).astype("<u2").tobytes() for m in chain)
    data = b"RTX1" + struct.pack("<4I", chain[0].shape[1], chain[0].shape[0], len(chain), 2) + zlib.compress(raw, 9)
    open(dst, "wb").write(data)
    return {"size": (chain[0].shape[1], chain[0].shape[0]), "mips": len(chain), "bytes": len(data)}


if __name__ == "__main__":
    args = sys.argv[1:]
    most = 1024
    if "--max" in args:
        k = args.index("--max")
        most = int(args[k + 1])
        del args[k:k + 2]
    srgb = "--srgb" in args
    if srgb:
        args.remove("--srgb")
    print(convert(args[0], args[1], most, srgb))
