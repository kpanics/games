"""A model (.glb, as VRF exports Source 2's) made into ours (.rcm, native/src/cosmetics/model.rs reads it): its
vertices with their skin, its triangles by material, its materials' textures, its skeleton, its animations resampled at
a steady rate (only the bones a clip moves; the others keep their rest).

    python rcm.py MODEL.glb OUT.rcm [--clips a,b,c] [--fps 30] [--drop material,material]

The layout (little endian): "RCM1", then six counts (vertices, indices, parts, materials, joints, clips); each vertex
position xyz, normal xyz, tangent xyzw, uv, four joints (u8), four weights (u8, summing to 255); the indices (u32); each
part its first index, its count, its material; each material its name, base colour, normal and packed (metalness,
roughness, occlusion) textures' file names (strings: u16 length + UTF-8); each joint its name, its parent (-1 none), its
rest translation xyz, rotation xyzw, scale xyz and its inverse bind matrix (16, column-major); each clip its name, its
frames a second, its frame count, whether it loops, its tracks (each a joint, what it moves: bit 0 translation, 1
rotation, 2 scale), then per track per frame what it moves."""
import json
import os
import struct
import sys

import numpy as np

COMPONENTS = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
WIDTH = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


class Glb:
    def __init__(self, path):
        d = open(path, "rb").read()
        magic, _, _ = struct.unpack_from("<III", d, 0)
        assert magic == 0x46546C67, "not a glb"
        jl, _ = struct.unpack_from("<II", d, 12)
        self.g = json.loads(d[20:20 + jl])
        o = 20 + jl
        bl, _ = struct.unpack_from("<II", d, o)
        self.bin = d[o + 8:o + 8 + bl]
        self.dir = os.path.dirname(path)

    def acc(self, i):
        a = self.g["accessors"][i]
        v = self.g["bufferViews"][a["bufferView"]]
        dt = COMPONENTS[a["componentType"]]
        w = WIDTH[a["type"]]
        start = v.get("byteOffset", 0) + a.get("byteOffset", 0)
        stride = v.get("byteStride", 0)
        n = a["count"]
        item = np.dtype(dt).itemsize * w
        if stride and stride != item:
            rows = [np.frombuffer(self.bin, dt, w, start + k * stride) for k in range(n)]
            out = np.array(rows)
        else:
            out = np.frombuffer(self.bin, dt, n * w, start).reshape(n, w)
        if a.get("normalized"):
            out = out.astype(np.float32) / float(np.iinfo(dt).max)
        return out


def s(b):
    e = b.encode("utf-8")
    return struct.pack("<H", len(e)) + e


def image_name(g, tex):
    if tex is None:
        return ""
    src = g["textures"][tex["index"]].get("source")
    if src is None:
        return ""
    im = g["images"][src]
    return os.path.basename(im.get("uri", im.get("name", "")))


def quat_mul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return np.array([aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx,
                     aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz])


def slerp(a, b, t):
    d = float(np.dot(a, b))
    if d < 0:
        b, d = -b, -d
    if d > 0.9995:
        q = a + (b - a) * t
    else:
        th = np.arccos(d)
        q = (np.sin((1 - t) * th) * a + np.sin(t * th) * b) / np.sin(th)
    return q / np.linalg.norm(q)


def sample(times, values, t, rot):
    """A track's value at `t` (linear; rotations spherical)."""
    if t <= times[0]:
        return values[0]
    if t >= times[-1]:
        return values[-1]
    k = int(np.searchsorted(times, t)) - 1
    k = max(0, min(k, len(times) - 2))
    f = (t - times[k]) / max(times[k + 1] - times[k], 1e-9)
    if rot:
        return slerp(values[k], values[k + 1], f)
    return values[k] + (values[k + 1] - values[k]) * f


def convert(src, out, clips_wanted=None, fps=30.0, drop=(), hidden=None):
    """`drop`: materials whose triangles are left out; `hidden`: {(mesh, primitive): each vertex hidden or not} — the
    triangles with two or more hidden ones left out (an agent's bare arms under its sleeves)."""
    m = Glb(src)
    g = m.g
    nodes = g.get("nodes", [])
    skin = g["skins"][0] if g.get("skins") else None
    joints = skin["joints"] if skin else []
    jindex = {n: k for k, n in enumerate(joints)}
    parent_of = {}
    for i, n in enumerate(nodes):
        for c in n.get("children", []):
            parent_of[c] = i
    # vertices and parts (every mesh primitive; positions in the mesh node's space, which VRF leaves at the origin)
    verts = []
    idx = []
    parts = []
    base = 0
    for mi, mesh in enumerate(g.get("meshes", [])):
        for pi, p in enumerate(mesh["primitives"]):
            if g.get("materials") and g["materials"][p.get("material", 0)].get("name") in drop:
                continue
            a = p["attributes"]
            pos = m.acc(a["POSITION"]).astype(np.float32)
            n = len(pos)
            nrm = m.acc(a["NORMAL"]).astype(np.float32) if "NORMAL" in a else np.tile([0, 1, 0], (n, 1)).astype(np.float32)
            tan = m.acc(a["TANGENT"]).astype(np.float32) if "TANGENT" in a else np.tile([1, 0, 0, 1], (n, 1)).astype(np.float32)
            uv = m.acc(a["TEXCOORD_0"]).astype(np.float32) if "TEXCOORD_0" in a else np.zeros((n, 2), np.float32)
            jo = m.acc(a["JOINTS_0"]).astype(np.uint8) if "JOINTS_0" in a else np.zeros((n, 4), np.uint8)
            we = m.acc(a["WEIGHTS_0"]).astype(np.float32) if "WEIGHTS_0" in a else np.tile([1, 0, 0, 0], (n, 1)).astype(np.float32)
            # weights to bytes summing to 255 (the largest takes the rounding)
            we = we / np.maximum(we.sum(axis=1, keepdims=True), 1e-9)
            wb = np.round(we * 255).astype(np.int32)
            wb[np.arange(n), wb.argmax(axis=1)] += 255 - wb.sum(axis=1)
            for k in range(n):
                verts.append(struct.pack("<3f3f4f2f4B4B", *pos[k], *nrm[k], *tan[k], *uv[k], *jo[k], *wb[k].astype(np.uint8)))
            ind = m.acc(p["indices"]).reshape(-1).astype(np.uint32) if "indices" in p else np.arange(n, dtype=np.uint32)
            if hidden and (mi, pi) in hidden:
                tri = ind.reshape(-1, 3)
                ind = tri[hidden[(mi, pi)][tri].sum(axis=1) < 2].reshape(-1)
            parts.append((len(idx), len(ind), p.get("material", 0)))
            idx.extend((ind + base).tolist())
            base += n
    # materials: the textures as VRF wrote them beside the model
    mats = []
    for mat in g.get("materials", []):
        pbr = mat.get("pbrMetallicRoughness", {})
        mats.append(s(mat.get("name", "")) + s(image_name(g, pbr.get("baseColorTexture"))) + s(image_name(g, mat.get("normalTexture")))
                    + s(image_name(g, pbr.get("metallicRoughnessTexture"))))
    if not mats:
        mats.append(s("default") + s("") + s("") + s(""))
    # the skeleton: each joint's rest and its inverse bind
    ibm = m.acc(skin["inverseBindMatrices"]).reshape(-1, 16) if skin and "inverseBindMatrices" in skin else np.tile(np.eye(4).T.reshape(16), (len(joints), 1))
    jbytes = []
    for k, nd in enumerate(joints):
        n = nodes[nd]
        par = parent_of.get(nd)
        while par is not None and par not in jindex:
            par = parent_of.get(par)
        t = n.get("translation", [0, 0, 0])
        r = n.get("rotation", [0, 0, 0, 1])
        sc = n.get("scale", [1, 1, 1])
        jbytes.append(s(n.get("name", f"joint{k}")) + struct.pack("<i", jindex[par] if par is not None else -1) + struct.pack("<3f4f3f", *t, *r, *sc) + struct.pack("<16f", *ibm[k]))
    # the clips: each bone's tracks resampled at `fps`
    cbytes = []
    for an in g.get("animations", []):
        name = an.get("name", "")
        if clips_wanted and name not in clips_wanted:
            continue
        tracks = {}
        length = 0.0
        for ch in an["channels"]:
            nd = ch["target"].get("node")
            if nd not in jindex:
                continue
            path = ch["target"]["path"]
            smp = an["samplers"][ch["sampler"]]
            times = m.acc(smp["input"]).reshape(-1).astype(np.float64)
            vals = m.acc(smp["output"]).astype(np.float64)
            length = max(length, float(times[-1]))
            tracks.setdefault(jindex[nd], {})[path] = (times, vals)
        frames = max(1, int(round(length * fps)) + 1)
        body = b""
        tcount = 0
        for j in sorted(tracks):
            tr = tracks[j]
            what = (1 if "translation" in tr else 0) | (2 if "rotation" in tr else 0) | (4 if "scale" in tr else 0)
            if not what:
                continue
            tcount += 1
            body += struct.pack("<HB", j, what)
            for f in range(frames):
                t = f / fps
                if what & 1:
                    body += struct.pack("<3f", *sample(*tr["translation"], t, False))
                if what & 2:
                    body += struct.pack("<4f", *sample(*tr["rotation"], t, True))
                if what & 4:
                    body += struct.pack("<3f", *sample(*tr["scale"], t, False))
        cbytes.append(s(name) + struct.pack("<fIBI", fps, frames, 1, tcount) + body)
    head = b"RCM1" + struct.pack("<6I", len(verts), len(idx), len(parts), len(mats), len(joints), len(cbytes))
    data = head + b"".join(verts) + struct.pack(f"<{len(idx)}I", *idx) + b"".join(struct.pack("<3I", *p) for p in parts) + b"".join(mats) + b"".join(jbytes) + b"".join(cbytes)
    open(out, "wb").write(data)
    return {"vertices": len(verts), "triangles": len(idx) // 3, "parts": len(parts), "materials": len(mats), "joints": len(joints), "clips": len(cbytes), "bytes": len(data)}


if __name__ == "__main__":
    args = sys.argv[1:]
    clips = None
    fps = 30.0
    if "--clips" in args:
        k = args.index("--clips")
        clips = set(args[k + 1].split(","))
        del args[k:k + 2]
    if "--fps" in args:
        k = args.index("--fps")
        fps = float(args[k + 1])
        del args[k:k + 2]
    drop = ()
    if "--drop" in args:
        k = args.index("--drop")
        drop = tuple(args[k + 1].split(","))
        del args[k:k + 2]
    print(convert(args[0], args[1], clips, fps, drop))
