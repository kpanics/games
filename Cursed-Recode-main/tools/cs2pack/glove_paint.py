"""A CS2 glove paint (gloves/paints/…vcompmat: CS2's glove compositor, csgo_customglove) made ours: the composite's
assembly played for each hand as CS2 plays it — its containers, the seed's rolls (the left hand's seed, the right's
"alt" one), the textures it generates (colour, metalness, normal, occlusion) and how each is set up — into
OUTDIR/paint/<name>/glove.txt, its textures (as CS2 keeps them: undecoded, a normal's hemi-octahedron as it is) in
OUTDIR/paint/gloves/, for native/src/cosmetics/glove.rs.

    python glove_paint.py OUTDIR NAME [NAME ...]        (NAME: a paint kit of gloves, glove_sport_…, specialist_fade…)
    python glove_paint.py OUTDIR --all                  (every glove paint of the catalog, then the combos)

glove.txt: "mat init|paint NAME V…" (the material the generation starts from and the paint's, parameters and
features), "tex init|paint NAME FILE", "avg init|paint NAME R G B A" (a texture's average colour, linear), "var NAME V…" (instance_params' own), then for each hand ("hand left|right"):
"roll SEED" (a roll: a stream of its own, seeded by econ_instance.SEED) and its "draw VAR COMP LO HI|none" lines,
"copyvar SRC DST", "out TARGET INIT COMBO" with its "op set NAME V…", "op copy paint|instance_params", "op wear NAME"
lines; "kit K V" (the paint kit's style, wear's range, seed)."""
import json
import os
import re
import subprocess
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import finish_item  # noqa: E402
import finish_paint  # noqa: E402
import kv  # noqa: E402
import kv3  # noqa: E402
import rtx  # noqa: E402
import vrf_export  # noqa: E402

INDEX = r"D:\CS2\tools\pak01_index.tsv"
WORK = r"D:\CS2\work\glovepaints"
DESC = r"D:\CS2\work\shaders\customglove_ps.kv3"
# the shader's static combos: each feature and its index (csgo_customglove ps)
COMBOS = (("F_BACKWARDS_COMPATIBILITY", 1), ("F_ANISOTROPIC_GLOSS", 2), ("F_TINT_ID", 4))
# the textures it generates that ours draw with (g_tAnisoGloss is CS2's own lighting's)
OUTPUTS = ("g_tColor", "g_tMetalness", "g_tNormal", "g_tAmbientOcclusion")
# the object's own maps, kept larger
OBJECT = ("g_tNormal", "g_tObjectProperties", "g_tLayerId", "g_tTintId")

_desc = None


def processing(name, combo):
    """How CS2 compiles a texture parameter for the combo that binds it (the same name may be two textures: a
    pattern's mask, or its sRGB colour): (its colour sRGB, a normal's hemi-octahedron)."""
    global _desc
    if _desc is None:
        d = kv3.parse(open(DESC, encoding="utf-8").read())
        bound = {}
        for k, cid in enumerate(d["m_staticComboIDs"]):
            av = d["m_staticComboData"][k]["m_allVars"]["m_indexAndRegisterOffsetArray"]
            bound[cid] = [(x >> 16) & 0xFFF for x in av if (x >> 28) == 1]
        _desc = (d["m_variableDescriptionArray"], d["m_textureChannelProcessorArray"], bound)
    vars_, ch, bound = _desc
    picks = [vars_[i] for i in bound.get(combo, []) if i < len(vars_) and vars_[i]["m_szName"] == name]
    v = picks[0] if picks else next((x for x in vars_ if x["m_szName"] == name), None)
    if not v:
        return False, False
    procs = [ch[i] for i in v["m_nChannelInfoIndex"] if i >= 0]
    if not procs:
        return False, False
    return procs[0]["m_outputColorSpace"] == 1, procs[0]["m_mipProcessingCommand"].startswith("HemiOct")


def find(name, index=INDEX):
    for line in open(index, encoding="utf-8"):
        p = line.split("\t", 1)[0]
        if p.endswith("/" + name + ".vcompmat_c") and p.startswith("gloves/"):
            return p
    return None


def composite(path):
    os.makedirs(WORK, exist_ok=True)
    out = os.path.join(WORK, os.path.basename(path).replace(".vcompmat_c", ".vcompmat"))
    if not os.path.exists(out):
        vrf_export.file(path, out)
    return kv3.parse(open(out, encoding="utf-8").read())


def assemble(path, seen=None):
    """The composite and its includes: the containers by alias (the paint's own over the included; instance_params'
    variables merged, its own winning) and the mutators (the included's, then its own)."""
    seen = seen or set()
    if path in seen:
        return {}, []
    seen.add(path)
    containers, mutators = {}, []
    for proc in finish_paint.procedures(composite(path)):
        for inc in proc.get("m_vecCompMatIncludes") or []:
            c2, m2 = assemble(inc.replace("\\", "/") + "_c", seen)
            for alias, c in c2.items():
                containers[alias] = c
            mutators = mutators + m2
        for c in proc.get("m_vecCompositeInputContainers") or []:
            alias = c.get("m_strAlias")
            if alias in containers and alias == "instance_params":
                old = {v["m_strName"]: v for v in containers[alias].get("m_vecLooseVariables") or []}
                for v in c.get("m_vecLooseVariables") or []:
                    old[v["m_strName"]] = v
                c = dict(c, m_vecLooseVariables=list(old.values()))
            containers[alias] = c
        mutators = mutators + (proc.get("m_vecPropertyMutators") or [])
    return containers, mutators


def material(path):
    """A material's lines (name, value) and its compiled textures (name, vtex), decompiled."""
    blk = finish_item.layer(finish_item.decompile_vmat(path, os.path.join(WORK, "mats")))
    lines, textures = [], []
    for k, v in kv.pairs(blk):
        if isinstance(v, str) and (k.startswith("F_") or (k.startswith("g_") and not k.startswith("g_t"))):
            lines.append((k, v.strip("[]")))
    for k, v in kv.pairs(kv.get(blk, "Compiled Textures") or []):
        textures.append((k, v))
    return lines, textures


_averages = None


def average(vtex):
    """A compiled texture's average colour as CS2 keeps it (its "reflectivity", linear: TextureAverageColor)."""
    global _averages
    path = os.path.join(WORK, "averages.json")
    if _averages is None:
        _averages = json.load(open(path)) if os.path.exists(path) else {}
    if vtex not in _averages:
        r = subprocess.run([vrf_export.VRF, "-i", vrf_export.VPK, "--vpk_filepath", vtex.replace("\\", "/") + "_c", "-a"],
                           capture_output=True, text=True, encoding="utf-8", errors="replace")
        m = re.search(r"Reflectivity\s*=\s*\(\s*([-\d.e]+),\s*([-\d.e]+),\s*([-\d.e]+),\s*([-\d.e]+)", r.stdout)
        _averages[vtex] = [float(x) for x in m.groups()] if m else None
        json.dump(_averages, open(path, "w"))
    return _averages[vtex]


def texture(out, name, vtex, most, combo):
    """A compiled texture as CS2 keeps it (no decoding: its normals' hemi-octahedron stays), ours under paint/gloves/."""
    srgb, _ = processing(name, combo)
    stem = os.path.splitext(os.path.basename(vtex))[0]
    key = stem + ("_srgb" if srgb else "")
    dst = os.path.join(out, "paint", "gloves", key + ".rtx")
    if os.path.exists(dst):
        return key + ".rtx"
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    png = os.path.join(WORK, "tex", stem + ".png")
    if not os.path.exists(png):
        os.makedirs(os.path.dirname(png), exist_ok=True)
        subprocess.run([vrf_export.VRF, "-i", vrf_export.VPK, "--vpk_filepath", vtex.replace("\\", "/") + "_c", "-o", png, "-d", "--texture_decode_flags", "none"],
                       capture_output=True, text=True, encoding="utf-8", errors="replace")
    if not os.path.exists(png):
        print("no picture for", name, vtex, flush=True)
        return None
    rtx.convert(png, dst, most, srgb)
    return key + ".rtx"


def value(v):
    """A loose variable's value as the composite's conditions read it."""
    t = v.get("m_nVariableType", "")
    if "BOOLEAN" in t:
        return "1" if v.get("m_bValueBoolean") else "0"
    if "INTEGER" in t:
        return str(v.get("m_nValueIntX", 0))
    if "FLOAT" in t:
        x = v.get("m_flValueFloatX", 0.0)
        return str(int(x)) if float(x).is_integer() else str(x)
    return ""


def passes(conds, look):
    """Whether a mutator's conditions all pass (each: its container's value equals the given one, or — when it is to
    pass when false — does not)."""
    for c in conds or []:
        got = look(c.get("m_strMutatorConditionContainerName", ""), c.get("m_strMutatorConditionContainerVarName", ""))
        equal = got == str(c.get("m_strMutatorConditionContainerVarValue", ""))
        if equal != c.get("m_bPassWhenTrue", True):
            return False
    return True


def set_line(sv):
    t = sv.get("m_nVariableType", "")
    if "BOOLEAN" in t:
        vals = [1.0 if sv.get("m_bValueBoolean") else 0.0]
    elif "INTEGER" in t:
        vals = [float(sv.get("m_nValueIntX", 0))]
    else:
        n = int(t[-1]) if t[-1].isdigit() else 1
        vals = [float(sv.get(f"m_flValueFloat{c}", 0.0)) for c in "XYZW"[:n]]
    return f"op set {sv['m_strName']} {' '.join(repr(x) for x in vals)}"


def pack(outdir, name):
    path = find(name)
    if not path:
        raise SystemExit(f"no glove composite for {name}")
    containers, mutators = assemble(path)
    paint_c = containers.get("paint", {})
    paint_path = paint_c.get("m_strSpecificContainerMaterial") or next(
        (v.get("m_strResourceMaterial") for v in paint_c.get("m_vecLooseVariables") or [] if v.get("m_strResourceMaterial")), None)
    template = (containers.get("paint_compositor") or {}).get("m_strSpecificContainerMaterial")
    mats = {"paint": material(paint_path)} if paint_path else {}
    if template:
        mats["paint_compositor"] = material(template)
    inst = {v["m_strName"]: v for v in (containers.get("instance_params") or {}).get("m_vecLooseVariables") or []}
    lines = []
    for alias, (params, texs) in mats.items():
        tag = "init" if alias == "paint_compositor" or not template else "paint"
        for k, v in params:
            lines.append(f"mat {tag} {k} {v}")
    for n, v in inst.items():
        vals = finish_paint.var_values(v)
        if vals is not None:
            lines.append(f"var {n} {' '.join(repr(x) for x in vals)}")
    paint_vals = dict(mats.get("paint", ([], []))[0])
    combos = set()
    for hand, attrs in (("left", {"left": "1"}), ("right", {"seed_offset": "1"})):
        def look(cont, var):
            if cont == "instance_params":
                return value(inst[var]) if var in inst else ""
            if cont == "handedness":
                return attrs.get("left", "")
            if cont == "material_seed":
                return attrs.get("seed_offset", "")
            if cont == "paint":
                return paint_vals.get(var, "")
            return ""
        lines.append(f"hand {hand}")
        for m in mutators:
            kind = m.get("m_nMutatorCommandType", "")
            if not passes(m.get("m_vecConditions"), look):
                continue
            if kind.endswith("RANDOM_ROLL_INPUT_VARIABLES"):
                lines.append(f"roll {m['m_strRandomRollInputVars_SeedInputVar'].split('.', 1)[1]}")
                for r in m.get("m_vecRandomRollInputVars_InputVarsToRoll") or []:
                    cont, rest = r.split(".", 1)
                    vname, comps = rest.rsplit(".", 1)
                    v = inst.get(vname)
                    for c in comps:
                        axis = "XYZW"["xyzw".index(c)]
                        if v is not None and v.get("m_bHasFloatBounds"):
                            lines.append(f"draw {vname} {c} {float(v.get(f'm_flValueFloat{axis}_Min', 0.0))!r} {float(v.get(f'm_flValueFloat{axis}_Max', 0.0))!r}")
                        else:
                            lines.append(f"draw {vname} {c} none")
            elif kind.endswith("COPY_PROPERTY") and m.get("m_strCopyProperty_InputContainerSrc") == "instance_params" \
                    and m.get("m_strCopyProperty_TargetProperty", "").startswith("instance_params."):
                lines.append(f"copyvar {m['m_strCopyProperty_InputContainerProperty']} {m['m_strCopyProperty_TargetProperty'].split('.', 1)[1]}")
            elif kind.endswith("GENERATE_TEXTURE") and m.get("m_strGenerateTexture_TargetParam") in OUTPUTS:
                init = m.get("m_strGenerateTexture_InitialContainer", "paint")
                feats = dict(mats.get(init, ([], []))[0])
                ops = []
                for ins in m.get("m_vecTexGenInstructions") or []:
                    if not passes(ins.get("m_vecConditions"), look):
                        continue
                    k = ins.get("m_nMutatorCommandType", "")
                    if k.endswith("SET_VALUE"):
                        sv = ins["m_nSetValue_Value"]
                        ops.append(set_line(sv))
                        feats[sv["m_strName"]] = set_line(sv).split()[3]
                    elif k.endswith("COPY_MATCHING_KEYS"):
                        src = ins["m_strCopyMatchingKeys_InputContainerSrc"]
                        ops.append(f"op copy {src}")
                        if src == "paint":
                            feats.update(paint_vals)
                    elif k.endswith("COPY_PROPERTY") and ins.get("m_strCopyProperty_InputContainerSrc") == "econ_instance" \
                            and ins.get("m_strCopyProperty_InputContainerProperty") == "g_flWearAmount":
                        ops.append(f"op wear {ins['m_strCopyProperty_TargetProperty']}")
                combo = sum(int(float(feats.get(f, "0"))) * k for f, k in COMBOS)
                combos.add(combo)
                lines.append(f"out {m['m_strGenerateTexture_TargetParam']} {'init' if init == 'paint_compositor' or not template else 'paint'} {combo}")
                lines.extend(ops)
    # the textures, compiled as the combo it runs binds them (one combo for all a paint makes)
    if len(combos) > 1:
        print("several combos for", name, sorted(combos), flush=True)
    combo = min(combos) if combos else 0
    for alias, (params, texs) in mats.items():
        tag = "init" if alias == "paint_compositor" or not template else "paint"
        for k, v in texs:
            f = texture(outdir, k, v, 2048 if k in OBJECT else 1024, combo)
            if f:
                lines.append(f"tex {tag} {k} {f}")
            avg = average(v)
            if avg:
                lines.append(f"avg {tag} {k} {' '.join(repr(x) for x in avg)}")
    for k, v in finish_paint.kit(name).items():
        if k in ("style", "wear_remap_min", "wear_remap_max", "seed", "wear_default"):
            lines.append(f"kit {k} {v}")
    pdir = os.path.join(outdir, "paint", name)
    os.makedirs(pdir, exist_ok=True)
    open(os.path.join(pdir, "glove.txt"), "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
    return {"name": name, "lines": len(lines), "combos": sorted(combos)}


def pack_all(outdir):
    """Every glove paint of the catalog not packed yet, then the compositor's combos they all need."""
    import glove_shader
    import pack_all as pa
    _, pairs = pa.read_catalog(os.path.join(outdir, "catalog.txt"))
    items, _ = pa.read_catalog(os.path.join(outdir, "catalog.txt"))
    combos = set()
    for item, paint in pairs:
        if items.get(item, {}).get("kind") != "gloves":
            continue
        txt = os.path.join(outdir, "paint", paint, "glove.txt")
        if not os.path.exists(txt):
            try:
                print(pack(outdir, paint), flush=True)
            except (SystemExit, Exception) as e:
                print("paint", paint, e, flush=True)
                continue
        for line in open(txt, encoding="utf-8"):
            if line.startswith("out "):
                combos.add(int(line.split()[3]))
    print("combos", sorted(combos), flush=True)
    return glove_shader.pack(os.path.join(outdir, "finish", "glove_shaders"), sorted(combos))


if __name__ == "__main__":
    if sys.argv[2:] == ["--all"]:
        print(pack_all(sys.argv[1]))
    else:
        for n in sys.argv[2:]:
            print(pack(sys.argv[1], n))
