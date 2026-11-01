"""CS2's charms (a rifle's keychain: catalog.py's "charm" lines) made ours: each one's model — its chain's joints, its
pendant — and its inventory icon: OUTDIR/charms/<name>/model.rcm (+ textures), OUTDIR/icons/charms/<name>.rtx.

    python pack_charms.py OUTDIR [--only kc_…,…]"""
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pack_model  # noqa: E402
import rtx  # noqa: E402
import vrf_export  # noqa: E402

WORK = r"D:\CS2\work\charms"


def charms(catalog):
    for line in open(catalog, encoding="utf-8"):
        f = line.rstrip("\n").split("\t")
        if f[0] == "charm":
            yield f[1], f[3], f[4]


def pack(out, only=None):
    done = 0
    for name, icon, model in charms(os.path.join(out, "catalog.txt")):
        if only and name not in only:
            continue
        cdir = os.path.join(out, "charms", name)
        if not os.path.exists(os.path.join(cdir, "model.rcm")):
            work = os.path.join(WORK, name)
            glb = os.path.join(work, name + ".glb")
            if not os.path.exists(glb):
                os.makedirs(work, exist_ok=True)
                vmdl = model.replace(".vmdl", ".vmdl_c") if model.endswith(".vmdl") else model
                subprocess.run([vrf_export.VRF, "-i", vrf_export.VPK, "--vpk_filepath", vmdl, "-o", glb, "-d", "--gltf_export_format", "glb",
                                "--gltf_export_materials", "--gltf_textures_adapt", "--gltf_animation_list", "no_animations", "--gltf_export_animations"],
                               capture_output=True)
            if os.path.exists(glb):
                print(name, pack_model.pack(glb, cdir, "model", None, 512), flush=True)
            else:
                print("no model for", name, flush=True)
        dst = os.path.join(out, "icons", "charms", name + ".rtx")
        if not os.path.exists(dst):
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            png = os.path.join(WORK, "icons", name + ".png")
            if not os.path.exists(png):
                os.makedirs(os.path.dirname(png), exist_ok=True)
                vrf_export.file(f"panorama/images/{icon}_png.vtex_c", png)
            if os.path.exists(png):
                rtx.convert(png, dst, 256, True)
        done += 1
    # the charms the game has no model or no icon for (CS2 itself: a Major's own, unshown) out of the catalog
    path = os.path.join(out, "catalog.txt")
    lines = open(path, encoding="utf-8").read().splitlines()
    kept = [ln for ln in lines if not ln.startswith("charm\t")
            or (os.path.exists(os.path.join(out, "charms", ln.split("\t")[1], "model.rcm"))
                and os.path.exists(os.path.join(out, "icons", "charms", ln.split("\t")[1] + ".rtx")))]
    if len(kept) != len(lines):
        open(path, "w", encoding="utf-8", newline="\n").write("\n".join(kept) + "\n")
        print("dropped", len(lines) - len(kept), flush=True)
    return done


if __name__ == "__main__":
    a = sys.argv[1:]
    only = None
    if "--only" in a:
        k = a.index("--only")
        only = set(a[k + 1].split(","))
        del a[k:k + 2]
    print(pack(a[0], only))
