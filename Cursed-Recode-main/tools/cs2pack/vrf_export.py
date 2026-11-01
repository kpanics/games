"""CS2's files out of its archive through VRF (Source2Viewer-CLI): a model with its materials' textures and its
animations (the clips of the graphs it plays, with the weapon's own skeleton beside) as glTF, a file decompiled.

    python vrf_export.py model PATH.vmdl_c [OUT]     # glb + PNGs into OUT (D:\\CS2\\work\\vrf by default)
    python vrf_export.py file PATH [OUT]             # decompiled as VRF does (text, sounds, pictures)

VRF reads the archive's index (pak01_dir.vpk) and the parts beside it."""
import os
import subprocess
import sys

VRF = os.environ.get("VRF_CLI", "Source2Viewer-CLI")
VPK = os.environ.get("CS2_VPK", r"D:\CS2\game\game\csgo\pak01_dir.vpk")
OUT = r"D:\CS2\work\vrf"


def run(args):
    r = subprocess.run([VRF, "-i", VPK] + args, capture_output=True, text=True, encoding="utf-8", errors="replace")
    tail = (r.stdout + r.stderr).strip().splitlines()[-3:]
    return r.returncode, tail


def model(path, out=OUT, animations=True, mesh_list=None):
    args = ["--vpk_filepath", path, "-o", out, "-d", "--gltf_export_format", "glb", "--gltf_export_materials"]
    if animations:
        args.append("--gltf_export_animations")
    if mesh_list:
        args += ["--gltf_mesh_list", mesh_list]
    code, tail = run(args)
    glb = os.path.join(out, os.path.splitext(path)[0].replace("/", os.sep) + ".glb")
    return glb if os.path.exists(glb) else None, code, tail


def file(path, out=OUT):
    code, tail = run(["--vpk_filepath", path, "-o", out, "-d"])
    return code, tail


if __name__ == "__main__":
    kind, path = sys.argv[1], sys.argv[2]
    out = sys.argv[3] if len(sys.argv) > 3 else OUT
    if kind == "model":
        print(model(path, out))
    else:
        print(file(path, out))
