"""CS2's glove compositor (csgo_customglove: a glove paint's layers, pattern, wear and grime, worked into the gloves'
colour, metalness, normal and occlusion) as ours to run, as finish_shader.py makes the weapons': a static combo's
pixel shader's SPIR-V as it is and what it needs, and its vertex shader's constants (ours runs its few lines:
shaders/cosmetics/glove_finish.vert) — for native/src/cosmetics/glove.rs.

    python glove_shader.py OUTDIR COMBO [COMBO ...]

OUTDIR/<combo>.spv and OUTDIR/<combo>.txt as finish_shader.py writes them, with "vsize N" and "vconst NAME OFFSET
TYPE D0 D1 D2 D3 [EXPR]" lines (the vertex shader's: its combo is the pixel shader's S_BACKWARDS_COMPATIBILITY). The
combo: S_BACKWARDS_COMPATIBILITY 1, S_ANISOTROPIC_GLOSS 2, S_TINT_ID 4."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import finish_shader  # noqa: E402
import kv3  # noqa: E402
import vcs_glsl  # noqa: E402

VCS = r"D:\CS2\work\shaders\raw\csgo_customglove_vulkan_50_ps.vcs"
DESC = r"D:\CS2\work\shaders\customglove_ps.kv3"
VS_DESC = r"D:\CS2\work\shaders\customglove_vs.kv3"
FEATURES = r"D:\CS2\work\shaders\customglove_features.kv3"


def pack(out, combos):
    os.makedirs(out, exist_ok=True)
    desc = kv3.parse(open(DESC, encoding="utf-8").read())
    vs = kv3.parse(open(VS_DESC, encoding="utf-8").read())
    features = finish_shader.feature_names(FEATURES)
    done = []
    for c in combos:
        if c not in desc["m_staticComboIDs"]:
            print("no combo", c)
            continue
        lines = finish_shader.reflect(desc, c, features)
        vlines = finish_shader.reflect(vs, c & 1)
        lines.append("v" + vlines[0])
        lines.extend("v" + x for x in vlines[1:] if x.startswith("const "))
        open(os.path.join(out, f"{c}.spv"), "wb").write(vcs_glsl.spirv_of(VCS, desc, c))
        open(os.path.join(out, f"{c}.txt"), "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
        done.append(c)
    return done


if __name__ == "__main__":
    print(pack(sys.argv[1], [int(x) for x in sys.argv[2:]]))
