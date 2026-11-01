"""World Effects' weather, as the paid CS2 client fatality.win has it: Dota 2's own weather particles (its cosmetic
"Weather: Snow" and "Weather: Ash", particles/rain_fx), read out of the game's files and laid out for the client
(her word, 23.10: "1 в 1 их погодные эффекты", their textures as they are).

    python tools/weather_pack.py DOTA_PAK01_DIR_VPK SOURCE2VIEWER_CLI [PREVIEW.png]

needs Source 2 Viewer's command line (ValveResourceFormat, MIT) to read the files, Pillow to pack the pictures. Writes
native/assets/weather/ (the DLL carries them, native/src/weather):
  systems.json  each particle system of the two (the root's children followed, the disabled ones left out) as the game
                keeps it, its KeyValues3 text as JSON;
  atlas.rgba.z  every frame of every texture they draw with, each frame whole (the part of the sheet the game shows:
                the cropped picture in its uncropped frame), packed with air round it;
  atlas.json    each texture's sequences: clamped or looping, their frames' display times and places in the atlas
                (u0, v0, u1, v1 of the atlas), and each frame's number (the order the frames are handed to the GPU).
The atlas is its RGBA rows, zlib'd (as the native's other pictures); PREVIEW, when given, gets it as a picture.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import zlib

from PIL import Image

VPK = sys.argv[1]
CLI = sys.argv[2]
HERE = os.path.dirname(os.path.abspath(__file__))
PREVIEW = sys.argv[3] if len(sys.argv) > 3 else None
OUT = os.path.join(HERE, "..", "native", "assets", "weather")
ROOTS = ["particles/rain_fx/econ_snow.vpcf", "particles/rain_fx/econ_weather_ash.vpcf"]
SIDE = 2048
PAD = 2


def cli(*args):
    r = subprocess.run([CLI, *args], capture_output=True, text=True, encoding="utf-8", errors="replace")
    if r.returncode != 0:
        raise SystemExit(f"Source 2 Viewer failed: {' '.join(args)}\n{r.stdout}\n{r.stderr}")
    return r.stdout


# ---- KeyValues3 text to JSON


def kv3(text):
    text = re.sub(r"<!--.*?-->", "", text, flags=re.S)
    tokens = re.findall(r'[A-Za-z_]+:"(?:[^"\\]|\\.)*"|"(?:[^"\\]|\\.)*"|[{}\[\]=,]|[^\s{}\[\]=,]+', text)
    at = [0]

    def value():
        t = tokens[at[0]]
        at[0] += 1
        if t == "{":
            o = {}
            while tokens[at[0]] != "}":
                k = tokens[at[0]]
                assert tokens[at[0] + 1] == "=", (k, tokens[at[0] + 1])
                at[0] += 2
                o[k] = value()
                if tokens[at[0]] == ",":
                    at[0] += 1
            at[0] += 1
            return o
        if t == "[":
            a = []
            while tokens[at[0]] != "]":
                a.append(value())
                if tokens[at[0]] == ",":
                    at[0] += 1
            at[0] += 1
            return a
        if t.startswith('"'):
            return json.loads(t)
        m = re.match(r'([A-Za-z_]+):(".*")$', t)
        if m:
            return json.loads(m.group(2))
        if t in ("true", "false"):
            return t == "true"
        if t == "null":
            return None
        try:
            return int(t)
        except ValueError:
            return float(t)

    return value()


def decompiled(paths, tmp):
    """The files decompiled into a folder (more than one asked: the command line writes a folder then), by path."""
    out = os.path.join(tmp, "d")
    shutil.rmtree(out, ignore_errors=True)
    cli("-i", VPK, "-f", ",".join(p if p.endswith("/") else p + "_c" for p in paths), "-d", "-o", out)
    return out


# ---- the textures' sheets and pictures


def sheet(path):
    """The texture's size and its sheet's sequences (none: the whole texture one frame)."""
    text = cli("-i", VPK, "-f", path + "_c", "-b", "DATA")
    w = int(re.search(r"Width\s+=\s+(\d+)", text).group(1))
    h = int(re.search(r"Height\s+=\s+(\d+)", text).group(1))
    seqs = []
    for block in re.split(r"\[Sequence \d+\]:", text)[1:]:
        clamp = re.search(r"m_bClamp\s+=\s+(\w+)", block).group(1) == "True"
        total = float(re.search(r"m_flTotalTime\s+=\s+([\d.]+)", block).group(1))
        frames = []
        for f in re.split(r"\[Sequence \d+ Frame \d+\]:", block)[1:]:
            t = float(re.search(r"m_flDisplayTime\s+=\s+([\d.]+)", f).group(1))
            num = r"\(\s*([-\d.]+),\s*([-\d.]+)\s*\)"
            c = [float(x) for x in re.search(r"uvCropped\s+=\s+\{\s*" + num + r",\s*" + num, f).groups()]
            u = [float(x) for x in re.search(r"uvUncropped\s+=\s+\{\s*" + num + r",\s*" + num, f).groups()]
            frames.append({"t": t, "cropped": c, "uncropped": u})
        seqs.append({"clamp": clamp, "total": total, "frames": frames})
    return w, h, seqs


def pictures(path, tmp):
    """The texture's frames as the game shows them (its pictures decompiled under `tmp`): [sequence][frame] -> RGBA."""
    w, h, seqs = sheet(path)
    out = tmp
    pngs = {}
    for root, _, files in os.walk(out):
        for f in files:
            if f.endswith(".png") and os.path.basename(root) == os.path.basename(os.path.dirname(path)):
                pngs[f] = os.path.join(root, f)
    name = os.path.splitext(os.path.basename(path))[0]
    # which picture each frame shows (the decompiler writes a picture once, frames showing it again name it)
    shown = []
    mks = None
    for root, _, files in os.walk(out):
        if name + ".mks" in files and os.path.basename(root) == os.path.basename(os.path.dirname(path)):
            mks = os.path.join(root, name + ".mks")
    if mks:
        for line in open(mks, encoding="utf-8"):
            w_ = line.split()
            if w_[:1] == ["sequence"]:
                shown.append([])
            elif w_[:1] == ["frame"]:
                shown[-1].append(w_[1])
    if not seqs:
        whole = Image.open(pngs[name + ".png"]).convert("RGBA")
        return {"clamp": [False], "total": [1.0], "frames": [[(1.0, whole, (name,))]]}
    pics = {"clamp": [], "total": [], "frames": []}
    for s, seq in enumerate(seqs):
        row = []
        for k, fr in enumerate(seq["frames"]):
            key = shown[s][k]
            crop = Image.open(pngs[key]).convert("RGBA")
            (cx0, cy0, cx1, cy1), (ux0, uy0, ux1, uy1) = fr["cropped"], fr["uncropped"]
            cw, ch = max(1, round((ux1 - ux0) * w)), max(1, round((uy1 - uy0) * h))
            frame = Image.new("RGBA", (cw, ch), (0, 0, 0, 0))
            # the cropped picture where it lies in its frame
            pw, ph = max(1, round((cx1 - cx0) * w)), max(1, round((cy1 - cy0) * h))
            if crop.size != (pw, ph):
                crop = crop.resize((pw, ph), Image.LANCZOS)
            frame.paste(crop, (round((cx0 - ux0) * w), round((cy0 - uy0) * h)))
            row.append((fr["t"], frame, (key, tuple(fr["cropped"]), tuple(fr["uncropped"]))))
        pics["clamp"].append(seq["clamp"])
        pics["total"].append(seq["total"])
        pics["frames"].append(row)
    return pics


def textures_of(system, out):
    for r in system.get("m_Renderers", []):
        for t in r.get("m_vecTexturesInput", []):
            if t.get("m_hTexture"):
                out.add(t["m_hTexture"])
        if r.get("m_hTexture"):
            out.add(r["m_hTexture"])


def main():
    tmp = tempfile.mkdtemp(prefix="weather_")
    try:
        # all the weather's systems at once, then the two roots' own followed
        folder = decompiled(["particles/rain_fx/econ_snow.vpcf", "particles/rain_fx/"], tmp)
        texts = {}
        for root, _, files in os.walk(folder):
            for f in files:
                if f.endswith(".vpcf"):
                    texts["particles/rain_fx/" + f] = open(os.path.join(root, f), encoding="utf-8").read()
        systems = {}
        todo = list(ROOTS)
        while todo:
            p = todo.pop()
            if p in systems:
                continue
            s = kv3(texts[p])
            systems[p] = s
            for c in s.get("m_Children", []):
                if not c.get("m_bDisableChild", False):
                    todo.append(c["m_ChildRef"])
        textures = set()
        for s in systems.values():
            textures_of(s, textures)
        print(len(systems), "systems,", len(textures), "textures")
        # every frame, packed in shelves (the tallest first) with air round each
        frames = []
        sheets = {}
        pictured = decompiled(sorted(textures), tmp)
        placed = {}
        for t in sorted(textures):
            pics = pictures(t, pictured)
            seqs = []
            for s, row in enumerate(pics["frames"]):
                seq = {"clamp": pics["clamp"][s], "total": pics["total"][s], "frames": []}
                for dt, im, same in row:
                    if (t, same) not in placed:
                        frames.append(im)
                        placed[(t, same)] = len(frames) - 1
                    seq["frames"].append({"t": dt, "i": placed[(t, same)]})
                seqs.append(seq)
            sheets[t] = seqs
            print(f"  {t}: {len(seqs)} sequences, {sum(len(s['frames']) for s in seqs)} frames")
        order = sorted(range(len(frames)), key=lambda i: -frames[i].size[1])
        at = [None] * len(frames)
        x = y = shelf = 0
        for i in order:
            fw, fh = frames[i].size
            if x + fw + PAD * 2 > SIDE:
                x, y, shelf = 0, y + shelf, 0
            at[i] = (x + PAD, y + PAD)
            x += fw + PAD * 2
            shelf = max(shelf, fh + PAD * 2)
        height = y + shelf
        if height > SIDE:
            raise SystemExit(f"the frames do not fit: {height} px tall")
        atlas_h = 1
        while atlas_h < height:
            atlas_h *= 2
        atlas = Image.new("RGBA", (SIDE, atlas_h), (0, 0, 0, 0))
        rects = []
        for i, im in enumerate(frames):
            ax, ay = at[i]
            atlas.paste(im, (ax, ay))
            rects.append([ax / SIDE, ay / atlas_h, (ax + im.size[0]) / SIDE, (ay + im.size[1]) / atlas_h])
        for seqs in sheets.values():
            for seq in seqs:
                for f in seq["frames"]:
                    f["r"] = [round(v, 6) for v in rects[f["i"]]]
        os.makedirs(OUT, exist_ok=True)
        open(os.path.join(OUT, "atlas.rgba.z"), "wb").write(zlib.compress(atlas.tobytes(), 9))
        if PREVIEW:
            atlas.save(PREVIEW)
        json.dump({"size": [SIDE, atlas_h], "frames": len(frames), "textures": sheets}, open(os.path.join(OUT, "atlas.json"), "w"), separators=(",", ":"))
        json.dump(systems, open(os.path.join(OUT, "systems.json"), "w"), separators=(",", ":"))
        print(f"atlas {SIDE}x{atlas_h}, {len(frames)} frames -> {OUT}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


main()
