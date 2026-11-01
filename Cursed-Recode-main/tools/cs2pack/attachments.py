"""The places CS2's models hold their extras (a weapon's StatTrak module, its name tag, its charm; a model's inventory
camera), from the model's own data (VRF's dump of the vmdl): OUTDIR/items/<item>/attachments.txt, a line each —
"att NAME BONE OX OY OZ QX QY QZ QW" (the bone it is held by; the offset in inches and the turn, in that bone's own
frame, as Source has them). The StatTrak module's models too: OUTDIR/stattrak/module.rcm, module_knife.rcm; the name
tag's: OUTDIR/nametag/module.rcm.

The model's markup too (OUTDIR/items/<item>/markup.txt): where its stickers lie — "slot BODY INDEX U V SCALE ROTATION
ID" (BODY model or legacy; U V the sticker's middle in the body's own texture's space, its V up as the markup keeps it;
SCALE how many of the sticker go across the texture; ROTATION degrees; ID its use: Map, Team1, …), "poly BODY U V U V …"
(where a sticker may be moved to), "map BODY Z0 Y0 L" (where that picture lies on the body: markup_fit.py) — and where a
charm may hang: "charm BODY BONE X Y Z ×4 [default]" (a quad's corners in the bone's frame, inches).

    python attachments.py OUTDIR [--only weapon_awp,…]"""
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kv3  # noqa: E402
import markup_fit  # noqa: E402
import pack_all  # noqa: E402
import pack_model  # noqa: E402
import rtx  # noqa: E402
import vrf_export  # noqa: E402

WORK = r"D:\CS2\work\attach"
WANTED = ("stattrak", "stattrak_legacy", "nametag", "nametag_legacy", "keychain", "keychain_legacy", "camera_inventory")
NUM = r"(-?[\d.]+(?:e-?\d+)?)"


def dump(vmdl):
    os.makedirs(WORK, exist_ok=True)
    out = os.path.join(WORK, os.path.basename(vmdl) + ".txt")
    if not os.path.exists(out):
        r = subprocess.run([vrf_export.VRF, "-i", vrf_export.VPK, "--vpk_filepath", vmdl, "-a"], capture_output=True, text=True, encoding="utf-8", errors="replace")
        open(out, "w", encoding="utf-8").write(r.stdout)
    return open(out, encoding="utf-8").read()


def attachments(text):
    """Each attachment the dump holds: name, its first influence's bone, offset, rotation."""
    found = {}
    for m in re.finditer(r'key = "([^"]+)"\s*value =\s*\{\s*m_name = "\1"\s*m_influenceNames =\s*\[\s*"([^"]*)"', text):
        name, bone = m.group(1), m.group(2)
        rest = text[m.end():m.end() + 1200]
        rot = re.search(r"m_vInfluenceRotations =\s*\[\s*\[\s*" + r",\s*".join([NUM] * 4) + r"\s*\]", rest)
        off = re.search(r"m_vInfluenceOffsets =\s*\[\s*\[\s*" + r",\s*".join([NUM] * 3) + r"\s*\]", rest)
        if rot and off and bone:
            found[name] = (bone, [float(x) for x in off.groups()], [float(x) for x in rot.groups()])
    return found


def markup_block(text, name):
    """A markup array of the dump (KeychainMarkup, StickerMarkup), read."""
    # the array itself (the model's own list of its markups names them too, "= 1")
    m = re.search(r"\b" + name + r" = \s*\[", text)
    if not m:
        return []
    j = m.end() - 1
    depth = 0
    for k in range(j, len(text)):
        if text[k] == "[":
            depth += 1
        elif text[k] == "]":
            depth -= 1
            if depth == 0:
                return kv3.parse("{ x = " + text[j:k + 1] + " }")["x"]
    return []


# where pack_item.py left each body's model (glTF), by the vmdl's own name
BODIES = r"D:\CS2\work\items"


def markup(text, stem=None):
    """The markup's lines (as markup.txt has them); with the vmdl's name (`stem`), where it lies on each body."""
    lines, polys = [], set()
    points = {"model": [], "legacy": []}
    for e in markup_block(text, "StickerMarkup"):
        if e.get("Deprecated"):
            continue
        body = "legacy" if e.get("LegacyModel") else "model"
        u, v = e.get("Offset", [0.0, 0.0])
        lines.append(f"slot {body} {e.get('Index', 0)} {u!r} {v!r} {float(e.get('Scale', 1.0))!r} {float(e.get('Rotation', 0.0))!r} {e.get('SpecialIdentifier', '-') or '-'}")
        points[body].append((float(u), float(v)))
        for poly in e.get("Polygons", []):
            vs = tuple(float(x) for x in poly.get("Vertices", []))
            if (body, vs) not in polys and vs:
                polys.add((body, vs))
                lines.append(f"poly {body} " + " ".join(repr(x) for x in vs))
                points[body] += list(zip(vs[0::2], vs[1::2]))
    for body, pts in points.items():
        glb = os.path.join(BODIES, stem or "", "body_hd" if body == "model" else "body_legacy", f"{stem}.glb")
        if pts and stem and os.path.exists(glb):
            found = markup_fit.fit(glb, pts)
            if found:
                lines.append(f"map {body} {found[0]!r} {found[1]!r} {found[2]!r}")
    for e in markup_block(text, "KeychainMarkup"):
        body = "legacy" if e.get("LegacyModel") else "model"
        corners = " ".join(repr(float(x)) for x in e.get("Corners", []))
        lines.append(f"charm {body} {e.get('BoneName', '')} {corners}" + (" default" if e.get("DefaultLocation") else ""))
    return lines


def module(out):
    """The StatTrak modules (a weapon's, a knife's) and the name tag's (its plate the name is written on)."""
    for folder, src, name in (("stattrak", "stattrak_module", "module"), ("stattrak", "stattrak_module_knife", "module_knife"), ("nametag", "nametag_module", "module")):
        sdir = os.path.join(out, folder)
        if os.path.exists(os.path.join(sdir, name + ".rcm")):
            continue
        work = os.path.join(WORK, src)
        glb = os.path.join(work, src + ".glb")
        if not os.path.exists(glb):
            os.makedirs(work, exist_ok=True)
            subprocess.run([vrf_export.VRF, "-i", vrf_export.VPK, "--vpk_filepath", f"weapons/models/shared/{folder}/{src}.vmdl_c", "-o", glb, "-d",
                            "--gltf_export_format", "glb", "--gltf_export_materials", "--gltf_textures_adapt"], capture_output=True)
        print(folder, name, pack_model.pack(glb, sdir, name, None, 512), flush=True)
    sdir = os.path.join(out, "stattrak")
    # the knife's digits are its atlas's alpha (alpha-tested): the model's export leaves it out, the texture's own keeps it
    png = os.path.join(WORK, "stattrak_digit_atlas_knife_color.png")
    if not os.path.exists(png):
        vrf_export.file("weapons/models/shared/stattrak/materials/stattrak_digit_atlas_knife_color_psd_966841ca.vtex_c", png)
    rtx.convert(png, os.path.join(sdir, "stattrak_digit_atlas_knife_color_psd_966841ca.rtx"), 512, True)


def pack(out, only=None):
    items, _ = pack_all.read_catalog(os.path.join(out, "catalog.txt"))
    done = 0
    for item, info in items.items():
        if info["kind"] == "gloves" or (only and item not in only) or not info["model"]:
            continue
        vmdl = info["model"].replace(".vmdl", ".vmdl_c") if info["model"].endswith(".vmdl") else info["model"]
        text = dump(vmdl)
        found = attachments(text)
        lines = [f"att {n} {b} {' '.join(repr(x) for x in o)} {' '.join(repr(x) for x in q)}" for n, (b, o, q) in found.items() if n in WANTED]
        idir = os.path.join(out, "items", item)
        os.makedirs(idir, exist_ok=True)
        open(os.path.join(idir, "attachments.txt"), "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
        marks = markup(text, os.path.splitext(os.path.basename(vmdl))[0].replace(".vmdl", ""))
        if marks:
            open(os.path.join(idir, "markup.txt"), "w", encoding="utf-8", newline="\n").write("\n".join(marks) + "\n")
        done += 1
        print(item, [n for n in found if n in WANTED], flush=True)
    module(out)
    return done


if __name__ == "__main__":
    a = sys.argv[1:]
    only = None
    if "--only" in a:
        k = a.index("--only")
        only = set(a[k + 1].split(","))
        del a[k:k + 2]
    print(pack(a[0], only))
