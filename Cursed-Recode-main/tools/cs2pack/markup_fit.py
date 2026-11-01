"""Where a model's sticker markup ("Map": CS2's own flat picture of the weapon's side, its u along the weapon, its v
down) lies on the model: z = Z0 − u·L, y = Y0 − v·L (glTF metres: z along the barrel, y up; the stickers on its +x side,
the one CS2 shows). Not written down anywhere: found from the model's side silhouette — of every (L, Z0, Y0) that puts
each of the markup's polygon corners and slots on the silhouette, the middle of the largest set.

    import markup_fit; markup_fit.fit(glb_path, points)  →  (Z0, Y0, L) or None"""
import json
import struct

import numpy as np
from PIL import Image, ImageDraw


def _glb(path):
    d = open(path, "rb").read()
    ln = struct.unpack_from("<I", d, 12)[0]
    j = json.loads(d[20:20 + ln])
    return j, d[20 + ln + 8:]


def _acc(j, b, i):
    a = j["accessors"][i]
    bv = j["bufferViews"][a["bufferView"]]
    n = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[a["type"]]
    dt = {5126: np.float32, 5125: np.uint32, 5123: np.uint16, 5121: np.uint8}[a["componentType"]]
    off = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
    stride = bv.get("byteStride", 0)
    size = np.dtype(dt).itemsize * n
    if stride and stride != size:
        raw = np.frombuffer(b, dtype=np.uint8, count=stride * a["count"], offset=off).reshape(a["count"], stride)[:, :size]
        return np.frombuffer(raw.tobytes(), dtype=dt).reshape(a["count"], n)
    return np.frombuffer(b, dtype=dt, count=a["count"] * n, offset=off).reshape(a["count"], n)


def silhouette(path, S=2048):
    """The model's side (z across, y up) drawn S wide: the picture and how it maps (zmin, ymax, metres per pixel)."""
    j, b = _glb(path)
    pos, tris, base = [], [], 0
    for m in j["meshes"]:
        for pr in m["primitives"]:
            p = _acc(j, b, pr["attributes"]["POSITION"]).astype(np.float64)
            pos.append(p)
            tris.append(_acc(j, b, pr["indices"]).astype(np.int64).reshape(-1, 3) + base)
            base += len(p)
    pos, idx = np.concatenate(pos), np.concatenate(tris)
    zmin, zmax, ymin, ymax = pos[:, 2].min(), pos[:, 2].max(), pos[:, 1].min(), pos[:, 1].max()
    m = max(zmax - zmin, ymax - ymin) * 1.02 / S
    im = Image.new("L", (S, S), 0)
    d = ImageDraw.Draw(im)
    for t in idx:
        d.polygon([((pos[i, 2] - zmin) / m, (ymax - pos[i, 1]) / m) for i in t], fill=255)
    return np.asarray(im) > 0, (zmin, ymax, m), (zmin, zmax, ymin, ymax)


def fit(path, points):
    """The markup's points (u, v) on the model at `path` (a glb): (Z0, Y0, L), or None when nothing fits."""
    sil, (zmin, ymax, m), (z0lo, z0hi, y0lo, y0hi) = silhouette(path)
    S = sil.shape[0]
    P = np.asarray(points, dtype=np.float64)
    length = z0hi - z0lo
    step = 0.003
    best = None
    for L in np.arange(length * 0.9, length * 1.6, length * 0.01):
        y0s = np.arange(y0lo - L, y0hi + L * 0.2, step)
        PY = ((ymax - (y0s[:, None] - P[None, :, 1] * L)) / m).astype(np.int64)
        yok = (PY >= 0) & (PY < S)
        PYc = np.clip(PY, 0, S - 1)
        found = []
        for z0 in np.arange(z0lo, z0hi, step):
            px = ((z0 - P[:, 0] * L - zmin) / m).astype(np.int64)
            if (px < 0).any() or (px >= S).any():
                continue
            hit = sil[PYc, px[None, :]] & yok
            for yi in np.nonzero(hit.all(axis=1))[0]:
                found.append((z0, y0s[yi]))
        if found and (best is None or len(found) > best[0]):
            a = np.array(found)
            best = (len(found), float(a[:, 0].mean()), float(a[:, 1].mean()), float(L))
    return None if best is None else best[1:]
