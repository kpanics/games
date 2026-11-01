"""A CS2 shader's GLSL (vcs_glsl.py) with its own names back: the constant buffer's members by their byte offsets
(a combo's variables, offsets in words), the textures by their slots (binding 30 + slot), the samplers by theirs
(binding 14 + slot) — as the combo's variable table (VRF's decompile) says.

    python vcs_names.py DESCRIPTION.kv3 COMBO IN.glsl OUT.glsl"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kv3  # noqa: E402


def names(desc, combo):
    vars_ = desc["m_variableDescriptionArray"]
    k = desc["m_staticComboIDs"].index(combo)
    av = desc["m_staticComboData"][k]["m_allVars"]
    consts, textures, samplers = {}, {}, {}
    for v in av["m_indexAndRegisterOffsetArray"]:
        idx, off = v >> 16, v & 0xFFFF
        kind, vi = idx >> 12, idx & 0xFFF
        if vi >= len(vars_):
            continue
        var = vars_[vi]
        if kind == 0 and off != 0xFFFF:
            consts[off * 4] = var["m_szName"]
        elif kind == 1 and var["m_szName"].startswith("g_t"):
            textures[30 + off] = var["m_szName"]
        elif kind == 1 and var["m_szName"] in ("AddressU", "AddressV", "Filter"):
            samplers.setdefault(14 + off, f"sampler{off}")
    return consts, textures, samplers


def rename(text, consts, textures, samplers):
    # the buffer's members: "layout(offset = N) type _mK;" → its name; uses "_BLOCK._mK" → the name
    block = re.search(r"uniform (\w+)\s*\{(.*?)\}\s*(\w+);", text, re.S)
    if block:
        inst = block.group(3)
        for off, mem in re.findall(r"layout\(offset = (\d+)\) \w+ (_m\d+);", block.group(2)):
            name = consts.get(int(off))
            if name:
                text = re.sub(rf"\b{inst}\.{mem}\b", name, text)
                text = text.replace(f" {mem};", f" {name};")
    for binding, name in list(textures.items()) + list(samplers.items()):
        m = re.search(rf"binding = {binding}\) uniform (?:texture2D|sampler) (_\d+);", text)
        if m:
            text = re.sub(rf"\b{m.group(1)}\b", name, text)
    return text


if __name__ == "__main__":
    desc = kv3.parse(open(sys.argv[1], encoding="utf-8").read())
    c, t, s = names(desc, int(sys.argv[2]))
    out = rename(open(sys.argv[3], encoding="utf-8").read(), c, t, s)
    open(sys.argv[4], "w", encoding="utf-8").write(out)
    print(len(c), "constants", len(t), "textures", len(s), "samplers")
