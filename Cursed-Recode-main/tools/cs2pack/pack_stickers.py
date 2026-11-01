"""CS2's stickers (catalog.py's "sticker" lines) made ours: each one's material — csgo_weapon_sticker's switches and
values, its textures — as OUTDIR/stickers/<material>.txt, and the textures (raw, as the shader reads them: its normals
hemi-octahedral) as OUTDIR/stickers/tex/<the texture's own path>.rtx, each once. A folder at a time out of the
archive (VRF), its PNGs made ours on every core, then let go.

    python pack_stickers.py OUTDIR [--only FOLDER,…]

A material's file: "flag NAME 0|1", "float NAME V", "vec NAME A B C D", "tex NAME KEY" (KEY under OUTDIR)."""
import multiprocessing
import os
import re
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rtx  # noqa: E402
import vrf_export  # noqa: E402

WORK = r"D:\CS2\work\stickers"
# the sticker's own picture and the holographic spectrum are colours; the rest data (normals, masks)
SRGB = ("g_tSticker0", "g_tHoloSpectrumSticker0", "g_tColor")
# as large as each is worth: the picture close up in the look, the rest smaller
MOST = {"g_tSticker0": 512, "g_tHoloSpectrumSticker0": 256}
LINE = re.compile(r'^\s*"([^"]+)"\s+"([^"]*)"\s*$')


def materials(catalog):
    for line in open(catalog, encoding="utf-8"):
        f = line.rstrip("\n").split("\t")
        if f[0] == "sticker":
            yield f[3]


def export(path, dst):
    """A folder (or one file) of the archive, decompiled: materials as text, textures as PNG with their raw channels."""
    subprocess.run([vrf_export.VRF, "-i", vrf_export.VPK, "--vpk_filepath", path, "-d", "--texture_decode_flags", "none", "-o", dst, "--threads", "4"],
                   capture_output=True)


def read_vmat(path):
    """A decompiled material: its values and its compiled textures."""
    vals, texs = {}, {}
    compiled = False
    for line in open(path, encoding="utf-8", errors="replace"):
        if '"Compiled Textures"' in line:
            compiled = True
            continue
        m = LINE.match(line)
        if not m:
            continue
        k, v = m.groups()
        if compiled:
            texs[k] = v
        else:
            vals[k] = v
    return vals, texs


def convert(job):
    png, dst, most, srgb = job
    if os.path.exists(dst):
        return dst, True
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    try:
        rtx.convert(png, dst, most, srgb)
        return dst, True
    except Exception as e:  # noqa: BLE001
        return f"{dst}: {e}", False


def fetched(work, src):
    """A texture's PNG: the folder's own export's, else (one lying elsewhere: materials/default/…, stickers/default/…)
    fetched once by itself."""
    png = os.path.join(work, src + ".png")
    if os.path.exists(png):
        return png
    png = os.path.join(WORK, "_defaults", src + ".png")
    if not os.path.exists(png):
        export(src + ".vtex_c", png)
    return png if os.path.exists(png) else None


def fixup(out, pool):
    """The textures a material names that are not made yet (a run before textures outside the folder were fetched)."""
    jobs = {}
    for root, _, files in os.walk(os.path.join(out, "stickers")):
        if os.path.join(out, "stickers", "tex") in root:
            continue
        for f in files:
            if not f.endswith(".txt"):
                continue
            for line in open(os.path.join(root, f), encoding="utf-8"):
                w = line.split()
                if len(w) == 3 and w[0] == "tex":
                    dst = os.path.join(out, w[2])
                    if not os.path.exists(dst) and dst not in jobs:
                        src = w[2][len("stickers/tex/"):-len(".rtx")]
                        png = fetched(os.path.join(WORK, "_fixup"), src)
                        if png:
                            jobs[dst] = (png, dst, MOST.get(w[1], 256), w[1] in SRGB)
                        else:
                            print("still no texture", src, flush=True)
    for r, ok in pool.imap_unordered(convert, list(jobs.values()), chunksize=4):
        if not ok:
            print("failed", r, flush=True)
    return len(jobs)


def pack_folder(out, folder, mats, pool):
    work = os.path.join(WORK, folder.replace("/", "_"))
    if not all(os.path.exists(os.path.join(out, "stickers", m + ".txt")) for m in mats):
        export(f"stickers/{folder}/", work)
    jobs, written = {}, 0
    for m in mats:
        txt = os.path.join(out, "stickers", m + ".txt")
        if os.path.exists(txt):
            continue
        vmat = os.path.join(work, "stickers", m + ".vmat")
        if not os.path.exists(vmat):
            print("no material", m, flush=True)
            continue
        vals, texs = read_vmat(vmat)
        lines = []
        for k, v in vals.items():
            if k.startswith("g_b"):
                lines.append(f"flag {k} {int(float(v) != 0)}")
            elif k.startswith(("g_fl", "g_f")):
                lines.append(f"float {k} {v}")
            elif k.startswith("g_v"):
                nums = v.strip("[]").split()
                lines.append(f"vec {k} {' '.join(nums)}")
        for k, v in texs.items():
            src = v[:-len(".vtex")] if v.endswith(".vtex") else v
            key = f"stickers/tex/{src}.rtx"
            lines.append(f"tex {k} {key}")
            dst = os.path.join(out, key)
            if not os.path.exists(dst) and dst not in jobs:
                png = fetched(work, src)
                if png:
                    jobs[dst] = (png, dst, MOST.get(k, 256), k in SRGB)
                else:
                    print("no texture", src, "for", m, flush=True)
        os.makedirs(os.path.dirname(txt), exist_ok=True)
        open(txt, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
        written += 1
    for r, ok in pool.imap_unordered(convert, list(jobs.values()), chunksize=4):
        if not ok:
            print("failed", r, flush=True)
    shutil.rmtree(work, ignore_errors=True)
    return written, len(jobs)


def pack(out, only=None):
    by_folder = {}
    for m in materials(os.path.join(out, "catalog.txt")):
        by_folder.setdefault(m.split("/")[0] if "/" in m else "", []).append(m)
    total = 0
    with multiprocessing.Pool(max(1, (os.cpu_count() or 4) - 2)) as pool:
        for folder, mats in sorted(by_folder.items(), key=lambda x: len(x[1])):
            if only and folder not in only:
                continue
            w, t = pack_folder(out, folder, mats, pool)
            total += w
            print(folder, len(mats), "materials", w, "written", t, "textures", flush=True)
        print("fixed up", fixup(out, pool), "textures", flush=True)
    return total


if __name__ == "__main__":
    a = sys.argv[1:]
    only = None
    if "--only" in a:
        k = a.index("--only")
        only = set(a[k + 1].split(","))
        del a[k:k + 2]
    print(pack(a[0], only))
