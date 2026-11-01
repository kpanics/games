"""Everything of the catalog made ours (catalog.py's file): each item's model (its HD body, its legacy one when it
has both), its clips (a knife's or a rifle's folder of viewmodel clips), its finish inputs, every finish's paint, the
inventory icons CS2 made for each pair (cards on the page), the compositor's combos they need, the clips' sounds.

    python pack_all.py OUTDIR [--only weapon_knife_karambit,…] [--skip models,clips,inputs,paints,icons,shaders,sounds]

OUTDIR/items/<item>/model.rcm (+legacy.rcm), clips.rcm (+clips_legacy.rcm), finish/…; OUTDIR/paint/<paint>/…;
OUTDIR/icons/<item>/<paint>.rtx ("default" the item's own); OUTDIR/finish/shaders/…; OUTDIR/sound/…;
OUTDIR/catalog.txt."""
import os
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import catalog  # noqa: E402
import finish_item  # noqa: E402
import finish_paint  # noqa: E402
import finish_shader  # noqa: E402
import item_clips  # noqa: E402
import pack_item  # noqa: E402
import rtx  # noqa: E402
import vrf_export  # noqa: E402

INDEX = r"D:\CS2\tools\pak01_index.tsv"
ICONS = r"D:\CS2\work\icons"


def read_catalog(path):
    items, paints = {}, []
    for line in open(path, encoding="utf-8"):
        f = line.rstrip("\n").split("\t")
        if f[0] == "item":
            items[f[1]] = {"defindex": f[2], "kind": f[3], "model": f[4]}
        elif f[0] == "paint":
            paints.append((f[1], f[2]))
    return items, paints


def bodies(model):
    """The model's bodies (meshes): HD and legacy, or the one it has."""
    stem = os.path.splitext(model)[0] + ".vmdl_c"
    out = subprocess.run([vrf_export.VRF, "-i", vrf_export.VPK, "--vpk_filepath", stem, "-a"], capture_output=True, text=True, encoding="utf-8", errors="replace").stdout
    names = set()
    for b in ("body_hd", "body_legacy"):
        if b in out:
            names.add(b)
    return names


# the items whose clips' folder is not named after them
CLIPS = {"weapon_bayonet": "knife/knife_bayonet", "weapon_knife_survival_bowie": "knife/knife_bowie", "weapon_knife_m9_bayonet": "knife/knife_m9",
         "weapon_knife_gypsy_jackknife": "knife/knife_navaja", "weapon_knife_widowmaker": "knife/knife_talon"}


def clip_folder(item, kind):
    """The item's viewmodel clips' folder."""
    if item in CLIPS:
        return "animation/anims/viewmodel/" + CLIPS[item]
    if kind == "rifle":
        return "animation/anims/viewmodel/rifle/rifle_" + item.replace("weapon_", "")
    return "animation/anims/viewmodel/knife/" + item.replace("weapon_", "")


def icons(out, item, paints_of_item):
    """The items' and pairs' icons (the light wear's), as cards."""
    os.makedirs(ICONS, exist_ok=True)
    idir = os.path.join(out, "icons", item)
    os.makedirs(idir, exist_ok=True)
    wanted = [f"panorama/images/econ/default_generated/{item}_{p}_light_png.vtex_c" for p in paints_of_item]
    wanted.append(f"panorama/images/econ/weapons/base_weapons/{item}_png.vtex_c")
    for w in wanted:
        stem = os.path.basename(w).replace("_light_png.vtex_c", "").replace("_png.vtex_c", "")
        paint = stem[len(item) + 1:] if stem.startswith(item + "_") else "default"
        dst = os.path.join(idir, paint + ".rtx")
        if os.path.exists(dst):
            continue
        png = os.path.join(ICONS, stem + ".png")
        if not os.path.exists(png):
            vrf_export.file(w, png)
        if os.path.exists(png):
            rtx.convert(png, dst, 256, True)


def pack(out, only=None, skip=()):
    os.makedirs(out, exist_ok=True)
    cat = os.path.join(out, "catalog.txt")
    catalog.build(cat)
    items, pairs = read_catalog(cat)
    if only:
        items = {k: v for k, v in items.items() if k in only}
    combos = set()
    for item, info in items.items():
        idir = os.path.join(out, "items", item)
        os.makedirs(idir, exist_ok=True)
        print("==", item, info["kind"], flush=True)
        if info["kind"] == "gloves":
            # gloves are hands (a model of their own on the arms): packed with the arms, not here
            pass
        elif "models" not in skip and info["model"] and not os.path.exists(os.path.join(idir, "model.rcm")):
            vmdl = info["model"].replace(".vmdl", ".vmdl_c") if info["model"].endswith(".vmdl") else info["model"]
            b = bodies(info["model"])
            if "body_hd" in b:
                pack_item.pack(vmdl, idir, "body_hd", "model")
                if "body_legacy" in b:
                    pack_item.pack(vmdl, idir, "body_legacy", "legacy")
            else:
                pack_item.pack(vmdl, idir, None, "model")
        if "clips" not in skip and info["kind"] != "gloves" and not os.path.exists(os.path.join(idir, "clips.rcm")):
            folder = clip_folder(item, info["kind"])
            if folder:
                print(item_clips.pack(folder, os.path.join(idir, "clips.rcm")), flush=True)
                if any(line.split("\t", 1)[0].startswith(folder + "/") and "_lgcy." in line for line in open(INDEX, encoding="utf-8")):
                    item_clips.pack(folder, os.path.join(idir, "clips_legacy.rcm"), legacy=True)
            else:
                print("no clips for", item)
        if "inputs" not in skip and info["kind"] != "gloves" and not os.path.isdir(os.path.join(idir, "finish")):
            for glb in ("model", "legacy"):
                src = os.path.join(r"D:\CS2\work\items", os.path.splitext(os.path.basename(info["model"]))[0])
                for sub in os.listdir(src) if os.path.isdir(src) else []:
                    g = os.path.join(src, sub, os.path.splitext(os.path.basename(info["model"]))[0] + ".glb")
                    if os.path.exists(g):
                        finish_item.pack(g, idir)
                break
        mine = [p for (i, p) in pairs if i == item]
        if "paints" not in skip:
            for p in mine:
                if not os.path.exists(os.path.join(out, "paint", p, "paint.txt")):
                    try:
                        finish_paint.pack(out, p)
                    except (SystemExit, Exception) as e:
                        print("paint", p, e, flush=True)
        if "icons" not in skip:
            icons(out, item, mine)
    # the compositor's combos every paint on every item needs
    if "shaders" not in skip:
        for item in items:
            idir = os.path.join(out, "items", item, "finish")
            sep = 0
            if os.path.isdir(idir):
                for f in os.listdir(idir):
                    if f.endswith(".txt") and "F_SEPARATE_CHANNEL_INPUTS 1" in open(os.path.join(idir, f), encoding="utf-8").read():
                        sep = 288
            for (i, p) in pairs:
                if i != item:
                    continue
                pt = os.path.join(out, "paint", p, "paint.txt")
                if not os.path.exists(pt):
                    continue
                feats = {}
                for line in open(pt, encoding="utf-8"):
                    f = line.split()
                    if f and f[0] == "feature":
                        feats[f[1]] = int(float(f[2]))
                c = sum(feats.get(n, 0) * k for n, k in [("F_PAINT_STYLE", 1), ("F_OVERRIDE_NORMAL", 9), ("F_USE_ALL_MASKS", 18), ("F_ROUGHNESS_TEXTURE", 36),
                                                        ("F_PEARLESCENCE_MASK", 72), ("F_METALNESS_TEXTURE", 144), ("F_OVERLAY_TEXTURE", 576),
                                                        ("F_CASE_HARDENING", 1152), ("F_CASE_HARDENING_TRILINEAR", 2304), ("F_PATINA_AGE", 4608)])
                combos.add(c)
                combos.add(c + 288)
        print("combos", sorted(combos), flush=True)
        finish_shader.pack(os.path.join(out, "finish", "shaders"), sorted(combos))
    return {"items": len(items)}


if __name__ == "__main__":
    a = sys.argv[1:]
    only, skip = None, ()
    if "--only" in a:
        k = a.index("--only")
        only = set(a[k + 1].split(","))
        del a[k:k + 2]
    if "--skip" in a:
        k = a.index("--skip")
        skip = tuple(a[k + 1].split(","))
        del a[k:k + 2]
    print(pack(a[0], only, skip))
