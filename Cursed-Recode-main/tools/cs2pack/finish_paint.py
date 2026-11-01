"""A CS2 paint (a finish: a paint kit's composite material) made ours: its paint material's numbers, features and
textures, what its seed rolls and between what, the defaults round it — OUTDIR/paint/<name>/paint.txt and its
textures (.rtx; the ones every paint shares once, in OUTDIR/paint/shared/), for native/src/cosmetics/finish.rs.

    python finish_paint.py OUTDIR NAME [NAME ...]

paint.txt: "feature NAME V", "param NAME V…" (the paint material's), "tex NAME FILE" (shared/ ones by that path),
"var CONTAINER NAME V…" (the composite's loose variables' values), "roll CONTAINER.NAME COMPONENT MIN MAX" (in the
order the seed rolls them), "flag override_vars.NAME V", "kit style S", "kit wear_min/wear_max/seed V"."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import finish_item  # noqa: E402
import kv  # noqa: E402
import kv3  # noqa: E402
import vrf_export  # noqa: E402

INDEX = r"D:\CS2\tools\pak01_index.tsv"
ITEMS = r"D:\CS2\work\data\items_game.txt"
WORK = r"D:\CS2\work\paints"
SHARED = ("paint_wear", "gun_grunge", "squares_glitter_normal")
SRGB = {"g_tPattern", "g_tColor", "g_tCaseHardeningColorRamp", "g_tOverlay"}


def find(name, index=INDEX):
    for line in open(index, encoding="utf-8"):
        p = line.split("\t", 1)[0]
        if p.endswith("/" + name + ".vcompmat_c"):
            return p
    return None


def composite(path):
    out = os.path.join(WORK, os.path.basename(path).replace(".vcompmat_c", ".vcompmat"))
    if not os.path.exists(out):
        vrf_export.file(path, out)
    return kv3.parse(open(out, encoding="utf-8").read())


def procedures(doc):
    for pt in doc.get("m_Points", []) or []:
        for proc in pt.get("m_vecCompositeMaterialAssemblyProcedures", []) or []:
            yield proc


def gather(path, seen=None):
    """The composite and its includes, the includes first (what the paint's own says wins): the containers' loose
    variables by alias, the paint material, the variables the seed rolls (in order)."""
    seen = seen or set()
    if path in seen:
        return {}, None, []
    seen.add(path)
    doc = composite(path)
    loose, paint, rolls = {}, None, []
    for proc in procedures(doc):
        for inc in proc.get("m_vecCompMatIncludes", []) or []:
            l2, p2, r2 = gather(inc.replace("\\", "/") + "_c", seen)
            for a, vs in l2.items():
                loose.setdefault(a, {}).update(vs)
            paint = p2 or paint
            rolls = r2 or rolls
        for c in proc.get("m_vecCompositeInputContainers", []) or []:
            alias = c.get("m_strAlias")
            if c.get("m_nCompositeMaterialInputContainerSourceType") == "CONTAINER_SOURCE_TYPE_SPECIFIC_MATERIAL" and alias == "paint":
                paint = c.get("m_strSpecificContainerMaterial")
            for v in c.get("m_vecLooseVariables", []) or []:
                loose.setdefault(alias, {})[v["m_strName"]] = v
        for mut in proc.get("m_vecPropertyMutators", []) or []:
            for cm in [mut] + (mut.get("m_vecConditionalMutators") or []):
                if cm.get("m_nMutatorCommandType") == "COMP_MAT_PROPERTY_MUTATOR_RANDOM_ROLL_INPUT_VARIABLES":
                    rolls = cm.get("m_vecRandomRollInputVars_InputVarsToRoll") or rolls
    return loose, paint, rolls


def var_values(v):
    t = v.get("m_nVariableType", "")
    if "FLOAT" in t:
        n = int(t[-1]) if t[-1].isdigit() else 1
        return [float(v.get(f"m_flValueFloat{c}", 0.0)) for c in "XYZW"[:n]]
    if "INTEGER" in t:
        return [float(v.get("m_nValueIntX", 0))]
    if "BOOLEAN" in t:
        return [1.0 if v.get("m_bValueBoolean") else 0.0]
    if "COLOR" in t or "TEXTURE" in t or "STRING" in t:
        return None
    return None


def kit(name, items=ITEMS):
    """The paint kit's own lines in items_game (style, wear's range, seed)."""
    blk = kv.parse(open(items, encoding="utf-8", errors="replace").read())
    root = kv.get(blk, "items_game")
    for k, v in kv.pairs(root):
        if k != "paint_kits":
            continue
        for _, pk in kv.pairs(v):
            if isinstance(pk, list) and kv.get(pk, "name") == name:
                return {a: b for a, b in kv.pairs(pk) if isinstance(b, str)}
    return {}


def pack(outdir, name, most=2048):
    path = find(name)
    if not path:
        raise SystemExit(f"no composite for {name}")
    loose, paint, rolls = gather(path)
    if not paint:
        raise SystemExit(f"no paint material in {name}'s composite (gloves' are made otherwise)")
    pdir = os.path.join(outdir, "paint", name)
    sdir = os.path.join(outdir, "paint", "shared")
    os.makedirs(pdir, exist_ok=True)
    os.makedirs(sdir, exist_ok=True)
    work = os.path.join(WORK, name)
    mat = finish_item.layer(finish_item.decompile_vmat(paint, work))
    lines = []
    for k, v in kv.pairs(mat):
        if not isinstance(v, str):
            continue
        if k.startswith("F_"):
            lines.append(f"feature {k} {v}")
        elif k.startswith("g_") and not k.startswith("g_t"):
            lines.append(f"param {k} {v.strip('[]')}")
    for k, v in kv.pairs(kv.get(mat, "Compiled Textures") or []):
        if "materials/default/" in v and "glitter" not in v:
            # a placeholder: the weapon's own input, or none
            continue
        # a shared texture already made (VRF may not write it beside every paint)
        base = os.path.basename(v)
        for cut in ("_psd", "_png", "_tga"):
            base = base.split(cut)[0]
        if base in SHARED and os.path.exists(os.path.join(sdir, base + ".rtx")):
            lines.append(f"tex {k} shared/{base}.rtx")
            continue
        src = finish_item.picture(work, v)
        if not src:
            print("no picture for", k, v)
            continue
        stem = os.path.splitext(os.path.basename(src))[0]
        shared = any(stem.startswith(s) for s in SHARED)
        dst = os.path.join(sdir if shared else pdir, stem + ".rtx")
        if not os.path.exists(dst):
            finish_item.to_rtx(src, dst, k in SRGB, most)
        lines.append(f"tex {k} {'shared/' if shared else ''}{stem}.rtx")
    for alias, vs in loose.items():
        for vname, v in vs.items():
            vals = var_values(v)
            if vals is not None:
                lines.append(f"var {alias} {vname} {' '.join(repr(x) for x in vals)}")
    for r in rolls:
        cont, rest = r.split(".", 1)
        vname, comps = rest.rsplit(".", 1)
        v = loose.get(cont, {}).get(vname)
        for c in comps:
            axis = "XYZW"["xyzw".index(c)]
            if v is not None and v.get("m_bHasFloatBounds"):
                lines.append(f"roll {cont}.{vname} {c} {float(v.get(f'm_flValueFloat{axis}_Min', 0.0))!r} {float(v.get(f'm_flValueFloat{axis}_Max', 0.0))!r}")
            else:
                lines.append(f"roll {cont}.{vname} {c} none")
    for k, v in kit(name).items():
        if k in ("style", "wear_remap_min", "wear_remap_max", "seed", "use_legacy_model", "wear_default"):
            lines.append(f"kit {k} {v}")
    open(os.path.join(pdir, "paint.txt"), "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
    return {"name": name, "lines": len(lines)}


if __name__ == "__main__":
    for n in sys.argv[2:]:
        print(pack(sys.argv[1], n))
