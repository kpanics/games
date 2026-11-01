"""CS2's sound events ours: each event (game_sounds_weapons.vsndevts, as VRF decompiles it) with the sounds it picks
from at random, its volume (at no distance: the first point of its distance curve; its mix group's level in CS2's
default mix, scripts/soundmixers.txt: the weapons' 0.6), its pitch and the spread of it,
its delay, the events it sets off with it — written to OUT/sound/events.txt, its sounds as Ogg Vorbis beside
(VRF's WAV through ffmpeg). The events: those the clips of OUT ask for (their EVT1, clips.py), and any named.

    python sounds.py OUT [EVENT ...] [--events D:\\CS2\\work\\data\\game_sounds_weapons.vsndevts]

events.txt: a line an event — name, volume, pitch, pitch's spread down and up, delay (s), its sounds (comma), its
events (comma); tab between."""
import glob
import os
import struct
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kv3  # noqa: E402
import vrf_export  # noqa: E402

EVENTS = r"D:\CS2\work\data\game_sounds_weapons.vsndevts"
MIXERS = r"D:\CS2\work\data\soundmixers.txt"
WORK = r"D:\CS2\work\sounds"
FFMPEG = os.environ.get("FFMPEG", "ffmpeg")


def clip_events(path):
    """The sound events a clips file of ours asks for."""
    d = open(path, "rb").read()
    k = d.find(b"EVT1")
    if k < 0:
        return []
    n = struct.unpack_from("<I", d, k + 4)[0]
    o = k + 8
    out = []
    for _ in range(n):
        _, _, kind = struct.unpack_from("<HfB", d, o)
        o += 7
        ln = struct.unpack_from("<H", d, o)[0]
        name = d[o + 2:o + 2 + ln].decode("utf-8")
        o += 2 + ln
        if kind == 0:
            out.append(name)
    return out


def mix_levels(path=MIXERS):
    """Each mix group's volume in CS2's default mix."""
    try:
        d = kv3.parse(open(path, encoding="utf-8").read())
    except OSError:
        return {}
    return {g["mixgroup"]: float(g.get("vol", 1.0)) for g in d.get("SoundMixers", {}).get("Default_Mix", []) if "mixgroup" in g}


MIX = None


def near_volume(e):
    """The event's volume where it is played from (its distance curve's value at no distance), in its mix group."""
    global MIX
    if MIX is None:
        MIX = mix_levels()
    v = float(e.get("volume", 1.0)) * MIX.get(e.get("mixgroup", ""), 1.0)
    curve = e.get("distance_volume_mapping_curve")
    if e.get("use_distance_volume_mapping_curve") and curve:
        v *= float(curve[0][1])
    return v


def stem(vsnd):
    """A sound's name of ours: its path under sounds/, dotted (two folders' "zoom" are two sounds)."""
    p = os.path.splitext(vsnd.replace("\\", "/"))[0]
    if p.startswith("sounds/"):
        p = p[len("sounds/"):]
    return p.replace("/", ".")


def pack(out, names, events_path=EVENTS):
    allev = kv3.parse(open(events_path, encoding="utf-8").read())
    want = list(dict.fromkeys(names))
    done = {}
    while want:
        n = want.pop(0)
        if n in done:
            continue
        e = allev.get(n)
        if not isinstance(e, dict):
            print("no event", n)
            done[n] = None
            continue
        files = []
        for k in sorted(e):
            if k.startswith("vsnd_files"):
                v = e[k]
                files += v if isinstance(v, list) else [v]
        children = []
        for k in sorted(e):
            if k.startswith("soundevent_"):
                v = e[k]
                children += v if isinstance(v, list) else [v]
        want += [c for c in children if c not in done]
        done[n] = (e, files, children)
    sdir = os.path.join(out, "sound")
    os.makedirs(sdir, exist_ok=True)
    lines = []
    made = 0
    for n, got in done.items():
        if got is None:
            continue
        e, files, children = got
        for f in files:
            st = stem(f)
            ogg = os.path.join(sdir, st + ".ogg")
            if os.path.exists(ogg):
                continue
            wav = os.path.join(WORK, st + ".wav")
            if not os.path.exists(wav):
                vrf_export.file(f.replace("\\", "/") + "_c", wav)
            if not os.path.exists(wav):
                print("no sound", f)
                continue
            subprocess.run([FFMPEG, "-y", "-loglevel", "error", "-i", wav, "-ar", "44100", "-ac", "2", "-c:a", "libvorbis", "-q:a", "5", ogg], check=True)
            made += 1
        lines.append("\t".join([n, f"{near_volume(e):.4g}", f"{float(e.get('pitch', 1.0)):.4g}", f"{float(e.get('pitch_random_min', 0.0)):.4g}",
                                f"{float(e.get('pitch_random_max', 0.0)):.4g}", f"{float(e.get('delay', 0.0)):.4g}",
                                ",".join(stem(f) for f in files), ",".join(children)]))
    open(os.path.join(sdir, "events.txt"), "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
    return {"events": len(lines), "sounds made": made}


if __name__ == "__main__":
    a = sys.argv[1:]
    ev = EVENTS
    if "--events" in a:
        k = a.index("--events")
        ev = a[k + 1]
        del a[k:k + 2]
    out = a[0]
    names = list(a[1:])
    for c in glob.glob(os.path.join(out, "**", "clips*.rcm"), recursive=True):
        names += clip_events(c)
    print(pack(out, names, ev))
