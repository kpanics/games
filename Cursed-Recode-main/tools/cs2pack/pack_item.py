"""A weapon's model out of CS2 into ours: VRF exports one of its bodies (CS2 keeps two for some: "body_hd", its new
model, and "body_legacy", CS:GO's, which the older finishes are painted for) with the weapon's skeleton and its
textures packed as glTF wants, then pack_model.py.

    python pack_item.py WEAPON.vmdl_c OUTDIR [--mesh body_hd] [--name model] [--work DIR] [--max 2048]

    python pack_item.py weapons/models/awp/weapon_snip_awp.vmdl_c D:\\CS2\\work\\out\\rifle\\awp --mesh body_hd
    python pack_item.py weapons/models/awp/weapon_snip_awp.vmdl_c D:\\CS2\\work\\out\\rifle\\awp --mesh body_legacy --name legacy"""
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pack_model  # noqa: E402
import vrf_export  # noqa: E402


def pack(vmdl, outdir, mesh=None, name="model", work=None, most=2048):
    stem = os.path.splitext(os.path.basename(vmdl))[0]
    work = work or os.path.join(r"D:\CS2\work\items", stem, mesh or "all")
    os.makedirs(work, exist_ok=True)
    glb = os.path.join(work, stem + ".glb")
    args = [vrf_export.VRF, "-i", vrf_export.VPK, "--vpk_filepath", vmdl, "-o", glb, "-d", "--gltf_export_format", "glb", "--gltf_export_materials",
            "--gltf_textures_adapt", "--gltf_export_animations", "--gltf_animation_list", "no_animations"]
    if mesh:
        args += ["--gltf_mesh_list", mesh]
    r = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if not os.path.exists(glb):
        raise SystemExit(f"VRF made no {glb}: {(r.stdout + r.stderr).strip().splitlines()[-3:]}")
    return pack_model.pack(glb, outdir, name, None, most)


if __name__ == "__main__":
    a = sys.argv[1:]
    opts = {"--mesh": None, "--name": "model", "--work": None, "--max": "2048"}
    for k in list(opts):
        if k in a:
            i = a.index(k)
            opts[k] = a[i + 1]
            del a[i:i + 2]
    print(pack(a[0], a[1], opts["--mesh"], opts["--name"], opts["--work"], int(opts["--max"])))
