"""The CS2 files' list for the server (each: sha256, size, key — its path under the cosmetics' folder), only what the
client reads; junctions are never entered.

    python tools/cs2pack/manifest.py <out_dir> <manifest_path>
"""
import hashlib
import os
import sys

ROOT, OUT = sys.argv[1], sys.argv[2]
PARTS = ["arms", "charms", "finish", "icons", "items", "nametag", "paint", "sound", "stattrak", "stickers", "catalog.txt", "cases", "cases.txt"]


def files(path):
    if os.path.islink(path) or os.path.isjunction(path):
        raise ValueError(f"Refusing linked asset path: {path}")
    if os.path.isfile(path):
        yield path
        return
    with os.scandir(path) as it:
        for e in it:
            if e.is_junction() or e.is_symlink():
                continue
            if e.is_dir(follow_symlinks=False):
                yield from files(e.path)
            elif e.is_file(follow_symlinks=False):
                yield e.path


lines = []
total = 0
for part in PARTS:
    for f in files(os.path.join(ROOT, part)):
        key = os.path.relpath(f, ROOT).replace("\\", "/")
        h = hashlib.sha256()
        with open(f, "rb") as fh:
            for chunk in iter(lambda: fh.read(1 << 20), b""):
                h.update(chunk)
        size = os.path.getsize(f)
        total += size
        lines.append(f"{h.hexdigest()} {size} {key}")
lines.sort(key=lambda l: l.split(" ", 2)[2])
with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write("\n".join(lines) + "\n")
print(len(lines), "files", round(total / 2**30, 2), "GiB")
