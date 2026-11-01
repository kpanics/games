"""A viewmodel's clips out of CS2's archive into one file of ours: each .vnmclip of a folder (animation/anims/viewmodel/
<kind>/<item>/) decompiled by VRF to its DMX, then clips.py — the names without the item's suffix ("idle1_karambit"
→ "idle1").

    python item_clips.py FOLDER OUT.rcm [--legacy] [--index D:\\CS2\\tools\\pak01_index.tsv] [--work D:\\CS2\\work\\clips]

Some weapons have their legacy body's own clips beside ("draw_ssg08_lgcy"): --legacy takes those (named the same
without it), none of them otherwise."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import clips  # noqa: E402
import vrf_export  # noqa: E402

INDEX = r"D:\CS2\tools\pak01_index.tsv"
WORK = r"D:\CS2\work\clips"


def files(folder, index=INDEX, legacy=False):
    folder = folder.strip("/") + "/"
    out = []
    for line in open(index, encoding="utf-8"):
        p = line.split("\t", 1)[0]
        if p.startswith(folder) and p.endswith(".vnmclip_c") and "/" not in p[len(folder):] and p.endswith("_lgcy.vnmclip_c") == legacy:
            out.append(p)
    return sorted(out)


def pack(folder, out, index=INDEX, work=WORK, legacy=False):
    item = folder.strip("/").rsplit("/", 1)[-1]
    suffix = "_" + item.split("_", 1)[-1] if item.startswith(("knife_", "rifle_")) else "_" + item
    named = []
    if legacy:
        suffix += "_lgcy"
    for p in files(folder, index, legacy):
        dst = os.path.join(work, os.path.splitext(p)[0].replace("/", os.sep) + ".dmx")
        if not os.path.exists(dst):
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            code, tail = vrf_export.file(p, os.path.splitext(dst)[0] + ".vnmclip")
            # VRF writes the clip's DMX beside the decompiled text, by the clip's own name
            if not os.path.exists(dst):
                print("no dmx for", p, code, tail)
                continue
        name = os.path.basename(dst)[:-4]
        if name.endswith(suffix):
            name = name[:-len(suffix)]
        named.append((name, dst))
    info = clips.convert(out, named)
    info["names"] = [n for n, _ in named]
    return info


if __name__ == "__main__":
    a = sys.argv[1:]
    index, work = INDEX, WORK
    legacy = "--legacy" in a
    if legacy:
        a.remove("--legacy")
    if "--index" in a:
        k = a.index("--index")
        index = a[k + 1]
        del a[k:k + 2]
    if "--work" in a:
        k = a.index("--work")
        work = a[k + 1]
        del a[k:k + 2]
    print(pack(a[0], a[1], index, work, legacy))
