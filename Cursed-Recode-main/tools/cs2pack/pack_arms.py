"""An agent's first-person hands out of CS2 into ours: VRF exports the agent's first-person meshes (the bare arms with
the default gloves, the sleeves) with its skeleton — once packed as glTF wants (occlusion, roughness, metalness), once
with the normal maps as they are (their alpha is the characters' roughness, pack_model.py) — then pack_model.py; the
bare arms are left out where the sleeves cover them (as the arm bends they would poke through at the elbow), kept where
the sleeves are short.

    python pack_arms.py AGENT.vmdl_c OUTDIR [--work D:\\CS2\\work\\arms\\NAME] [--max 2048]

    python pack_arms.py agents/models/ctm_sas/ctm_sas.vmdl_c D:\\CS2\\work\\out\\arms\\ct"""
import os
import subprocess
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pack_model  # noqa: E402
import rcm  # noqa: E402
import vrf_export  # noqa: E402

MESHES = "firstperson_default_gloves_arms,firstperson_sleeves"


def export(agent, glb, adapt):
    args = [vrf_export.VRF, "-i", vrf_export.VPK, "--vpk_filepath", agent, "-o", glb, "-d", "--gltf_export_format", "glb", "--gltf_export_materials",
            "--gltf_export_animations", "--gltf_animation_list", "no_animations", "--gltf_mesh_list", MESHES]
    if adapt:
        args.append("--gltf_textures_adapt")
    r = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if not os.path.exists(glb):
        raise SystemExit(f"VRF made no {glb}: {(r.stdout + r.stderr).strip().splitlines()[-3:]}")


def under_sleeves(glb):
    """The bare arms' vertices under the sleeves (as bound): within 4 cm of a sleeve's vertex and behind its surface —
    {(mesh, primitive): hidden or not, each vertex}."""
    m = rcm.Glb(glb)
    g = m.g
    arms, sleeves, normals = {}, [], []
    for mi, mesh in enumerate(g.get("meshes", [])):
        for pi, p in enumerate(mesh["primitives"]):
            bare = g["materials"][p.get("material", 0)].get("name", "").startswith("bare_arm")
            pos = m.acc(p["attributes"]["POSITION"]).astype(np.float64)
            if bare and "gloves_arms" in (mesh.get("name") or ""):
                arms[(mi, pi)] = pos
            elif not bare and "sleeves" in (mesh.get("name") or ""):
                sleeves.append(pos)
                normals.append(m.acc(p["attributes"]["NORMAL"]).astype(np.float64))
    if not sleeves:
        return {}
    s = np.concatenate(sleeves)
    n = np.concatenate(normals)
    out = {}
    for key, a in arms.items():
        d = np.linalg.norm(a[:, None, :] - s[None, :, :], axis=2)
        k = d.argmin(axis=1)
        near = d[np.arange(len(a)), k]
        behind = np.einsum("ij,ij->i", a - s[k], n[k])
        out[key] = (near < 0.04) & (behind < 0.003)
    return out


def pack(agent, outdir, work=None, most=2048):
    name = os.path.splitext(os.path.basename(agent))[0]
    work = work or os.path.join(r"D:\CS2\work\arms", name)
    raw = os.path.join(work, "raw")
    os.makedirs(raw, exist_ok=True)
    glb = os.path.join(work, name + ".glb")
    export(agent, glb, True)
    export(agent, os.path.join(raw, name + ".glb"), False)
    hidden = under_sleeves(glb)
    info = pack_model.pack(glb, outdir, "arms", None, most, hidden=hidden)
    info["hidden"] = {f"{k}": int(v.sum()) for k, v in hidden.items()}
    return info


if __name__ == "__main__":
    a = sys.argv[1:]
    work, most = None, 2048
    if "--work" in a:
        k = a.index("--work")
        work = a[k + 1]
        del a[k:k + 2]
    if "--max" in a:
        k = a.index("--max")
        most = int(a[k + 1])
        del a[k:k + 2]
    print(pack(a[0], a[1], work, most))
