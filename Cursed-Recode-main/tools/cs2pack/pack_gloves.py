"""CS2's gloves made ours for the hands: each pair's first-person model (the gloves and the bare forearm they come
with), the forearm cut under each side's sleeves as pack_arms.py cuts the agent's own; each side's sleeves alone (with
gloves on, the agent's default gloves and arms are not drawn); the gloves alone for the Cosmetics page's look.

    python pack_gloves.py OUTDIR [--only sporty_gloves,…]

OUTDIR/items/<gloves>/arms_ct.rcm, arms_t.rcm, model.rcm (+ their textures); OUTDIR/arms/<side>/sleeves.rcm. The
agents' exports pack_arms.py made (D:\\CS2\\work\\arms\\<side>) are taken as they are."""
import json
import os
import struct
import subprocess
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pack_all  # noqa: E402
import pack_model  # noqa: E402
import rcm  # noqa: E402
import vrf_export  # noqa: E402

WORK = r"D:\CS2\work"
SIDES = {"ct": "ctm_sas", "t": "tm_phoenix"}


def materials_of(glb):
    d = open(glb, "rb").read()
    g = json.loads(d[20:20 + struct.unpack_from("<I", d, 12)[0]])
    return g, [m.get("name", "") for m in g.get("materials", [])]


def export(model, glb, adapt, animations="no_animations"):
    """The model's first-person mesh ("viewmodel"; the world's is left out) with its skeleton and materials (and the
    animations named)."""
    if os.path.exists(glb):
        return
    os.makedirs(os.path.dirname(glb), exist_ok=True)
    args = [vrf_export.VRF, "-i", vrf_export.VPK, "--vpk_filepath", model, "-o", glb, "-d", "--gltf_export_format", "glb", "--gltf_export_materials",
            "--gltf_export_animations", "--gltf_animation_list", animations, "--gltf_mesh_list", "viewmodel"]
    if adapt:
        args.append("--gltf_textures_adapt")
    r = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if not os.path.exists(glb):
        raise SystemExit(f"VRF made no {glb}: {(r.stdout + r.stderr).strip().splitlines()[-3:]}")


def sleeves_of(agent_glb):
    """The agent's sleeves' vertices and normals (as bound)."""
    m = rcm.Glb(agent_glb)
    g = m.g
    pos, nrm = [], []
    for mesh in g.get("meshes", []):
        if "sleeves" not in (mesh.get("name") or ""):
            continue
        for p in mesh["primitives"]:
            if g["materials"][p.get("material", 0)].get("name", "").startswith("bare_arm"):
                continue
            pos.append(m.acc(p["attributes"]["POSITION"]).astype(np.float64))
            nrm.append(m.acc(p["attributes"]["NORMAL"]).astype(np.float64))
    return (np.concatenate(pos), np.concatenate(nrm)) if pos else (None, None)


def under(glove_glb, sleeves):
    """The gloves' bare forearm's vertices under the sleeves (within 4 cm of a sleeve's vertex, behind its surface)."""
    s, n = sleeves
    if s is None:
        return {}
    m = rcm.Glb(glove_glb)
    g = m.g
    out = {}
    for mi, mesh in enumerate(g.get("meshes", [])):
        for pi, p in enumerate(mesh["primitives"]):
            if not g["materials"][p.get("material", 0)].get("name", "").startswith("bare_arm"):
                continue
            a = m.acc(p["attributes"]["POSITION"]).astype(np.float64)
            hidden = np.zeros(len(a), bool)
            # in pieces: a forearm against every sleeve vertex at once is too much memory
            for i in range(0, len(a), 2048):
                part = a[i:i + 2048]
                d = np.linalg.norm(part[:, None, :] - s[None, :, :], axis=2)
                k = d.argmin(axis=1)
                near = d[np.arange(len(part)), k]
                behind = np.einsum("ij,ij->i", part - s[k], n[k])
                hidden[i:i + 2048] = (near < 0.04) & (behind < 0.003)
            out[(mi, pi)] = hidden
    return out


def sleeves(out, side):
    """The side's sleeves alone (the agent's default gloves and bare arms left out)."""
    name = SIDES[side]
    glb = os.path.join(WORK, "arms", side, name + ".glb")
    g, mats = materials_of(glb)
    drop = tuple(m for m in mats if m.startswith(("bare_arm", "glove_")))
    return pack_model.pack(glb, os.path.join(out, "arms", side), "sleeves", None, 2048, drop=drop)


def gloves(out, item, model):
    """A pair: for each side its forearm cut under that side's sleeves; the gloves alone."""
    vmdl = model.replace(".vmdl", ".vmdl_c") if model.endswith(".vmdl") else model
    stem = os.path.splitext(os.path.basename(model))[0]
    work = os.path.join(WORK, "gloves", stem)
    glb = os.path.join(work, stem + ".glb")
    export(vmdl, glb, True)
    export(vmdl, os.path.join(work, "raw", stem + ".glb"), False)
    idir = os.path.join(out, "items", item)
    info = {}
    for side, name in SIDES.items():
        hidden = under(glb, sleeves_of(os.path.join(WORK, "arms", side, name + ".glb")))
        info[side] = pack_model.pack(glb, idir, f"arms_{side}", None, 2048, hidden=hidden)
        info[side]["hidden"] = int(sum(v.sum() for v in hidden.values()))
    # the gloves alone, with CS2's pose for its inventory's picture (the pair held together)
    icon = os.path.join(work, "icon", stem + ".glb")
    export(vmdl, icon, True, "icon_pose")
    _, mats = materials_of(icon)
    info["model"] = pack_model.pack(icon, idir, "model", {"icon_pose"}, 2048, drop=tuple(m for m in mats if m.startswith("bare_arm")))
    return info


if __name__ == "__main__":
    a = sys.argv[1:]
    only = None
    if "--only" in a:
        k = a.index("--only")
        only = set(a[k + 1].split(","))
        del a[k:k + 2]
    out = a[0]
    items, _ = pack_all.read_catalog(os.path.join(out, "catalog.txt"))
    for side in SIDES:
        print("sleeves", side, sleeves(out, side), flush=True)
    for item, v in items.items():
        if v["kind"] == "gloves" and (not only or item in only):
            print(item, gloves(out, item, v["model"]), flush=True)
