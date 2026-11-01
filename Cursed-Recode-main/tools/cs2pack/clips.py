"""A weapon's viewmodel clips (VRF's DMX of CS2's .vnmclip: the arms' skeleton "characters/viewmodel" with the
weapon's own beside it) made one file of ours (.rcm without a mesh: the clips' skeleton and the clips, native/src/
cosmetics/model.rs reads it; the models it poses take each joint's place by name, as VRF's SkeletonRetargeter does).

    python clips.py OUT.rcm NAME=CLIP.dmx [NAME=CLIP.dmx ...] [--fps 30]

The clips' events after them (what CS2's clip says happens at a time in it: a sound event, a particle): "EVT1", their
count, each its clip (u16), its time (f32, s), its kind (u8: 0 a sound, 1 a particle), its name.

Into the glTF space VRF exports the models in: metres (an inch is 0.0254), and its turn at the roots (C, Source's z up
→ glTF's y up) on the skeleton's roots — so a clip's world places and a model's are one space."""
import os
import struct
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dmx  # noqa: E402

INCH = 0.0254
# VRF's turn at a glTF skeleton's roots (Source → glTF): x → z, y → x, z → y
C = np.array([-0.5, -0.5, -0.5, 0.5])


def qmul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return np.array([aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx, aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz])


def qrot(q, v):
    x, y, z, w = q
    u = np.array([x, y, z])
    v = np.array(v, dtype=np.float64)
    return v + 2.0 * np.cross(u, np.cross(u, v) + w * v)


def slerp(a, b, t):
    d = float(np.dot(a, b))
    if d < 0:
        b, d = -b, -d
    if d > 0.9995:
        q = a + (b - a) * t
    else:
        th = np.arccos(min(d, 1.0))
        q = (np.sin((1 - t) * th) * a + np.sin(t * th) * b) / np.sin(th)
    return q / np.linalg.norm(q)


def sample(times, values, t, rot):
    if len(times) == 0:
        return None
    if t <= times[0]:
        return values[0]
    if t >= times[-1]:
        return values[-1]
    k = int(np.searchsorted(times, t)) - 1
    k = max(0, min(k, len(times) - 2))
    f = (t - times[k]) / max(times[k + 1] - times[k], 1e-9)
    return slerp(values[k], values[k + 1], f) if rot else values[k] + (values[k + 1] - values[k]) * f


def s(b):
    e = b.encode("utf-8")
    return struct.pack("<H", len(e)) + e


def skeleton(root):
    """The DMX's joints in order (parents before children), each (name, parent index, t, q) — as ours."""
    sk = root["skeleton"]
    order = []
    parent = {}

    def walk(e, p):
        for c in e.get("children", []) or []:
            parent[c.name] = p
            order.append(c)
            walk(c, c.name)

    walk(sk, None)
    index = {e.name: k for k, e in enumerate(order)}
    out = []
    for e in order:
        tr = e["transform"]
        t = np.array(tr["position"], dtype=np.float64)
        q = np.array(tr["orientation"], dtype=np.float64)
        p = parent.get(e.name)
        out.append((e.name, index[p] if p is not None else -1, t, q))
    return out


def to_gltf(t, q, root):
    """A joint's place under its parent (Source units) into the glTF space (roots turned by C)."""
    t = np.array(t, dtype=np.float64) * INCH
    q = np.array(q, dtype=np.float64)
    if root:
        return qrot(C, t), qmul(C, q)
    return t, q


def channels(root):
    """Each joint's position and orientation keys (times in seconds)."""
    clip = root["animationList"]["animations"][0]
    out = {}
    for ch in clip["channels"]:
        el = ch.get("toElement")
        attr = ch.get("toAttribute")
        log = ch.get("log")
        if el is None or log is None or attr not in ("position", "orientation"):
            continue
        layers = log.get("layers") or []
        if not layers:
            continue
        lay = layers[0]
        times = np.array(lay.get("times") or [], dtype=np.float64)
        vals = np.array(lay.get("values") or [], dtype=np.float64)
        if len(times) == 0:
            continue
        out.setdefault(el.name, {})[attr] = (times, vals)
    tf = clip.get("timeFrame") or {}
    return out, float(tf.get("duration", 0.0)), float(clip.get("frameRate", 30.0))


def events_of(vnmclip, rate):
    """A clip's events from its decompiled .vnmclip (KV3): (time s, kind, name) — sounds and particles."""
    import kv3

    try:
        d = kv3.parse(open(vnmclip, encoding="utf-8").read())
    except (OSError, AssertionError, IndexError):
        return []
    out = []
    for track in d.get("m_eventTracks") or []:
        kind = {"CNmClipDocEvent_Sound": 0, "CNmClipDocEvent_Particle": 1}.get(track.get("m_eventClassName"))
        if kind is None:
            continue
        for e in track.get("m_events") or []:
            name = e.get("m_name") if kind == 0 else e.get("m_particleSystem")
            if name:
                out.append((float(e.get("m_flStartTime", 0.0)) / rate, kind, str(name)))
    return out


def convert(out, named, fps=30.0):
    first = dmx.load(named[0][1])
    joints = skeleton(first)
    names = [j[0] for j in joints]
    jbytes = []
    for name, parent, t, q in joints:
        gt, gq = to_gltf(t, q, parent < 0)
        # the inverse bind is not used for a clip's skeleton: identity
        jbytes.append(s(name) + struct.pack("<i", parent) + struct.pack("<3f4f3f", *gt, *gq, 1.0, 1.0, 1.0) + struct.pack("<16f", *np.eye(4).reshape(16)))
    cbytes = []
    events = []
    for ci, (cname, path) in enumerate(named):
        root = first if path == named[0][1] else dmx.load(path)
        chans, duration, rate = channels(root)
        if duration <= 0:
            duration = max((max(v[0][-1] for v in c.values()) for c in chans.values()), default=0.0)
        frames = max(1, int(round(duration * fps)) + 1)
        body = b""
        tcount = 0
        for k, (name, parent, rest_t, rest_q) in enumerate(joints):
            c = chans.get(name)
            if not c:
                continue
            what = (1 if "position" in c else 0) | (2 if "orientation" in c else 0)
            tcount += 1
            body += struct.pack("<HB", k, what)
            for f in range(frames):
                tm = min(f / fps, duration)
                t = sample(*c["position"], tm, False) if "position" in c else rest_t
                q = sample(*c["orientation"], tm, True) if "orientation" in c else rest_q
                gt, gq = to_gltf(t, q, parent < 0)
                if what & 1:
                    body += struct.pack("<3f", *gt)
                if what & 2:
                    body += struct.pack("<4f", *gq)
        looping = 1 if cname.startswith("idle") else 0
        cbytes.append(s(cname) + struct.pack("<fIBI", fps, frames, looping, tcount) + body)
        for t, kind, name in events_of(os.path.splitext(path)[0] + ".vnmclip", rate or 30.0):
            events.append(struct.pack("<HfB", ci, t, kind) + s(name))
    data = b"RCM1" + struct.pack("<6I", 0, 0, 0, 0, len(jbytes), len(cbytes)) + b"".join(jbytes) + b"".join(cbytes)
    if events:
        data += b"EVT1" + struct.pack("<I", len(events)) + b"".join(events)
    open(out, "wb").write(data)
    return {"joints": len(names), "clips": len(cbytes), "events": len(events), "bytes": len(data)}


if __name__ == "__main__":
    a = sys.argv[1:]
    fps = 30.0
    if "--fps" in a:
        k = a.index("--fps")
        fps = float(a[k + 1])
        del a[k:k + 2]
    named = [tuple(x.split("=", 1)) for x in a[1:]]
    print(convert(a[0], named, fps))
