"""CS2's finish compositor (csgo_customweapon's pixel shader: a weapon's paint, its wear, its pattern, worked into the
weapon's colour and roughness textures) as ours to run: a static combo's SPIR-V as it is, and what it needs (its
constants by name and byte offset, its textures by name and binding, its samplers' bindings), for
native/src/cosmetics/finish.rs.

    python finish_shader.py OUTDIR COMBO [COMBO ...]

OUTDIR/<combo>.spv and OUTDIR/<combo>.txt: "size N", then "const NAME OFFSET TYPE D0 D1 D2 D3 [EXPR]" (its default,
and the compiled expression the material system works it out by, hex — cosmetics/vfxeval.rs runs it), "tex NAME
BINDING", "sampler SLOT BINDING [EXPR_U EXPR_V]" (its address modes' expressions), "feature NAME INDEX" lines. The combo: each S_ combo's value times its index (S_PAINT_STYLE 1, S_OVERRIDE_NORMAL 9,
S_USE_ALL_MASKS 18, S_ROUGHNESS_TEXTURE 36, S_PEARLESCENCE_MASK 72, S_METALNESS_TEXTURE 144,
S_SEPARATE_CHANNEL_INPUTS 288, S_OVERLAY_TEXTURE 576, S_CASE_HARDENING 1152, S_CASE_HARDENING_TRILINEAR 2304,
S_PATINA_AGE 4608)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kv3  # noqa: E402
import vcs_glsl  # noqa: E402

VCS = r"D:\CS2\work\shaders\raw\csgo_customweapon_vulkan_50_ps.vcs"
DESC = r"D:\CS2\work\shaders\customweapon_ps.kv3"


def default(var):
    d = var.get("m_flDefault") or [float(x) for x in (var.get("m_intDefault") or [0, 0, 0, 0])]
    return " ".join(repr(float(x)) for x in (list(d) + [0, 0, 0, 0])[:4])


def expr(var):
    e = var.get("m_pCompiledExpression")
    return e.hex() if isinstance(e, bytes) and e else ""


def reflect(desc, combo, features=()):
    vars_ = desc["m_variableDescriptionArray"]
    k = desc["m_staticComboIDs"].index(combo)
    data = desc["m_staticComboData"][k]
    lines = [f"size {data['m_nConstantBufferSize']}"]
    for v in data["m_allVars"]["m_indexAndRegisterOffsetArray"]:
        idx, off = v >> 16, v & 0xFFFF
        kind, vi = idx >> 12, idx & 0xFFF
        if vi >= len(vars_):
            continue
        var = vars_[vi]
        name = var["m_szName"]
        if kind == 0 and off != 0xFFFF:
            lines.append(f"const {name} {off * 4} {var['m_type']} {default(var)} {expr(var)}".rstrip())
        elif kind == 1 and name.startswith("g_t"):
            lines.append(f"tex {name} {30 + off}")
        elif kind == 1 and name in ("AddressU", "AddressV", "Filter"):
            line = f"sampler {off} {14 + off}"
            if not any(x.startswith(line) for x in lines):
                lines.append(line)
            if name in ("AddressU", "AddressV") and expr(var):
                # the pattern's sampling (its address mode from the material's g_nPattern…Sampling)
                lines.append(f"address {off} {name[-1]} {expr(var)}")
    for i, f in enumerate(features):
        lines.append(f"feature {f} {i}")
    return lines


FEATURES = r"D:\CS2\work\shaders\customweapon_features.kv3"


def feature_names(path=FEATURES):
    d = kv3.parse(open(path, encoding="utf-8").read())
    return [c["m_szName"] for c in d["m_programData"]["m_staticComboArray"]]


def pack(out, combos, vcs=VCS, desc_path=DESC):
    os.makedirs(out, exist_ok=True)
    desc = kv3.parse(open(desc_path, encoding="utf-8").read())
    features = feature_names()
    done = []
    for c in combos:
        if c not in desc["m_staticComboIDs"]:
            print("no combo", c)
            continue
        open(os.path.join(out, f"{c}.spv"), "wb").write(vcs_glsl.spirv_of(vcs, desc, c))
        open(os.path.join(out, f"{c}.txt"), "w", encoding="utf-8", newline="\n").write("\n".join(reflect(desc, c, features)) + "\n")
        done.append(c)
    return done


if __name__ == "__main__":
    print(pack(sys.argv[1], [int(x) for x in sys.argv[2:]]))
