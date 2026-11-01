"""A weapon's finish inputs (CS2's composite inputs: what any paint is worked onto it with — its ambient occlusion,
its masks, its surface's normals, its positions, its own colour and metalness, its size) made ours, beside its model:
OUTDIR/finish/<material>.txt and the textures (.rtx; the positions 16 bits a channel).

    python finish_item.py MODEL.glb OUTDIR

<material>.txt: "material NAME" (the model's material the finish replaces), "param NAME V…", "feature NAME V",
"tex NAME FILE" lines."""
import json
import os
import struct
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kv  # noqa: E402
import rtx  # noqa: E402
import vrf_export  # noqa: E402

WORK = r"D:\CS2\work\inputs"
FFMPEG = os.environ.get("FFMPEG", "ffmpeg")
# what each input is (sRGB colour or linear data)
SRGB = {"g_tColor"}


def decompile_vmat(path, work):
    """A material decompiled (its textures beside, as VRF writes them): its KeyValues."""
    os.makedirs(work, exist_ok=True)
    out = os.path.join(work, os.path.basename(path))
    if not os.path.exists(out):
        vrf_export.file(path.replace("\\", "/") + "_c", out)
    return kv.parse(open(out, encoding="utf-8", errors="replace").read())


def layer(blk):
    return kv.get(blk, "Layer0") or blk


def picture(work, vtex):
    """The file VRF wrote for a compiled texture (its name without the hash), or None."""
    stem = os.path.basename(vtex).rsplit(".", 1)[0]
    for cand in (stem, stem.rsplit("_", 1)[0]):
        for base in (cand, cand.rsplit("_", 1)[0]):
            for ext in (".png", ".exr", ".tga"):
                p = os.path.join(work, base + ext)
                if os.path.exists(p):
                    return p
    # VRF names a texture after its source: try every file whose name starts so
    for f in os.listdir(work):
        if f.startswith(stem.split("_psd")[0].split("_png")[0].split("_tga")[0].split("_pfm")[0]) and f.endswith((".png", ".exr")):
            return os.path.join(work, f)
    return None


def to_rtx(src, dst, srgb, most=2048):
    if src.endswith(".exr"):
        import numpy as np
        from PIL import Image  # noqa: F401
        raw = dst + ".raw"
        ffprobe = os.path.join(os.path.dirname(FFMPEG), os.path.basename(FFMPEG).replace("ffmpeg", "ffprobe"))
        probe = subprocess.run([ffprobe, "-v", "error", "-show_entries", "stream=width,height", "-of", "csv=p=0", src],
                               capture_output=True, text=True).stdout.strip().split(",")
        w, h = int(probe[0]), int(probe[1])
        subprocess.run([FFMPEG, "-y", "-loglevel", "error", "-i", src, "-f", "rawvideo", "-pix_fmt", "gbrapf32le", raw], check=True)
        a = np.fromfile(raw, dtype="<f4").reshape(4, h, w)
        os.remove(raw)
        rgba = np.stack([a[2], a[0], a[1], np.ones((h, w), np.float32)], axis=-1)
        return rtx.convert_float(rgba, dst, most)
    return rtx.convert(src, dst, most, srgb)


def pack(glb, outdir, most=2048):
    d = open(glb, "rb").read()
    g = json.loads(d[20:20 + struct.unpack_from("<I", d, 12)[0]])
    fdir = os.path.join(outdir, "finish")
    os.makedirs(fdir, exist_ok=True)
    done = []
    for m in g.get("materials", []):
        vmat = (m.get("extras", {}).get("vmat", {}) or {}).get("Name")
        if not vmat:
            continue
        own = layer(decompile_vmat(vmat, os.path.join(WORK, "mat")))
        inputs = kv.get(own, "Attributes", "composite_inputs") if kv.get(own, "Attributes") else None
        if not inputs:
            continue
        name = os.path.splitext(os.path.basename(inputs))[0]
        work = os.path.join(WORK, name)
        ci = layer(decompile_vmat(inputs, work))
        lines = [f"material {m['name']}"]
        for k, v in kv.pairs(ci):
            if isinstance(v, str) and k.startswith("g_") and not k.startswith("g_t"):
                lines.append(f"param {k} {v.strip('[]')}")
            elif isinstance(v, str) and k.startswith("F_"):
                lines.append(f"feature {k} {v}")
        for k, v in kv.pairs(kv.get(ci, "Compiled Textures") or []):
            src = picture(work, v)
            if not src:
                print("no picture for", k, v)
                continue
            out = os.path.splitext(os.path.basename(src))[0] + ".rtx"
            to_rtx(src, os.path.join(fdir, out), k in SRGB, most)
            lines.append(f"tex {k} {out}")
        open(os.path.join(fdir, m["name"] + ".txt"), "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
        done.append(m["name"])
    return done


if __name__ == "__main__":
    print(pack(sys.argv[1], sys.argv[2]))
