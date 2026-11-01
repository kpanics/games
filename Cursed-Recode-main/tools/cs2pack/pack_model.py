"""A model and its textures made ours (.rcm and .rtx beside it, as native/src/cosmetics/store.rs looks for them).

    python pack_model.py MODEL.glb OUTDIR NAME [--clips a,b] [--max 1024]

VRF exported with --gltf_textures_adapt (its packing: occlusion, roughness, metalness); a character's normal maps as
they are (with their alpha, which that packing drops) in the folder "raw" beside the glb.

The model goes to OUTDIR/NAME.rcm; each texture its materials name (VRF's PNGs beside the glb) to OUTDIR/<stem>.rtx —
base colours as sRGB, the rest linear."""
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rcm  # noqa: E402
import rtx  # noqa: E402


def textures(glb, drop=()):
    d = open(glb, "rb").read()
    jl = struct.unpack_from("<I", d, 12)[0]
    g = json.loads(d[20:20 + jl])
    out = {}
    for m in g.get("materials", []):
        if m.get("name") in drop:
            continue
        pbr = m.get("pbrMetallicRoughness", {})
        for slot, srgb in ((pbr.get("baseColorTexture"), True), (m.get("normalTexture"), False), (pbr.get("metallicRoughnessTexture"), False)):
            name = rcm.image_name(g, slot)
            if name:
                out[name] = out.get(name, False) or srgb
    return out


def character_packing(glb):
    """CS2's characters (csgo_character.vfx: the arms, the gloves, the sleeves) keep their roughness in the normal map's
    alpha, which VRF's packing leaves out (it knows not that shader): each such material's packed texture (occlusion,
    roughness, metalness) with its roughness from there, as the material adjusts it (g_vRoughnessAdjustBrightnessContrast)
    — {packed file: (normal file, brightness, contrast)}."""
    d = open(glb, "rb").read()
    jl = struct.unpack_from("<I", d, 12)[0]
    g = json.loads(d[20:20 + jl])
    out = {}
    for m in g.get("materials", []):
        if (m.get("extras", {}).get("vmat", {}).get("ShaderName")) != "csgo_character.vfx":
            continue
        packed = rcm.image_name(g, m.get("pbrMetallicRoughness", {}).get("metallicRoughnessTexture"))
        normal = rcm.image_name(g, m.get("normalTexture"))
        adjust = m["extras"]["vmat"].get("VectorParams", {}).get("g_vRoughnessAdjustBrightnessContrast", [0, 1])
        if packed and normal:
            out[packed] = (normal, float(adjust[0]), float(adjust[1]))
    return out


def with_roughness(packed, normal, brightness=0.0, contrast=1.0):
    from PIL import Image
    import numpy as np
    n = Image.open(normal).convert("RGBA")
    p = Image.open(packed).convert("RGB").resize(n.size, Image.BILINEAR)
    a = np.asarray(p).copy()
    r = np.asarray(n)[..., 3].astype(np.float64) / 255.0
    r = (r - 0.5) * contrast + 0.5 + brightness
    a[..., 1] = np.clip(np.round(r * 255.0), 0, 255).astype(np.uint8)
    return Image.fromarray(a, "RGB")


def pack(glb, outdir, name, clips=None, most=1024, drop=(), hidden=None):
    os.makedirs(outdir, exist_ok=True)
    info = rcm.convert(glb, os.path.join(outdir, name + ".rcm"), clips, drop=drop, hidden=hidden)
    src = os.path.dirname(glb)
    character = character_packing(glb)
    for f, srgb in textures(glb, drop).items():
        p = os.path.join(src, f)
        if os.path.exists(p):
            raw = os.path.join(src, "raw", character[f][0]) if f in character else ""
            if raw and os.path.isfile(raw):
                p = with_roughness(p, raw, character[f][1], character[f][2])
            elif f in character:
                print("no raw normal map for", f)
            rtx.convert(p, os.path.join(outdir, os.path.splitext(f)[0] + ".rtx"), most, srgb)
        else:
            print("missing texture", f)
    return info


if __name__ == "__main__":
    a = sys.argv[1:]
    clips = None
    most = 1024
    if "--clips" in a:
        k = a.index("--clips")
        clips = set(a[k + 1].split(","))
        del a[k:k + 2]
    if "--max" in a:
        k = a.index("--max")
        most = int(a[k + 1])
        del a[k:k + 2]
    print(pack(a[0], a[1], a[2], clips, most))
