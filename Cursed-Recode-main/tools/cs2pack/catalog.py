"""The cosmetics' catalog out of CS2 (items_game, its localisation, its generated inventory icons): the knives, the
gloves, the AWP and the SSG 08 — each with its model and names — and every finish each comes in (an icon CS2 made for
the pair is the proof it exists), with the finish's names, rarity, wear's range, style and default seed.

    python catalog.py OUT.txt

OUT.txt (tab between fields): "item NAME DEFINDEX KIND MODEL RU EN RARITY" (KIND knife, gloves or rifle; RARITY its
prefabs' item_rarity, which a paint's own adds to), then
"paint ITEM PAINT RARITY WEAR_MIN WEAR_MAX STYLE SEED LEGACY RU EN" lines, "charm NAME RARITY ICON MODEL RU EN" (a
rifle's keychain), "sticker NAME RARITY MATERIAL RU EN" (MATERIAL its path under stickers/)."""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kv  # noqa: E402

DATA = r"D:\CS2\work\data"
INDEX = r"D:\CS2\tools\pak01_index.tsv"
RIFLES = ("weapon_awp", "weapon_ssg08")


def tokens(path):
    blk = kv.parse(open(path, encoding="utf-8-sig", errors="replace").read())
    out = {}
    for k, v in kv.pairs(kv.get(blk, "lang", "Tokens") or []):
        if isinstance(v, str):
            out[k.lower()] = v
    return out


def name(loc, key):
    return loc.get(key.lstrip("#").lower(), key.lstrip("#"))


def build(out):
    ig = kv.get(kv.parse(open(os.path.join(DATA, "items_game.txt"), encoding="utf-8", errors="replace").read()), "items_game")
    ru = tokens(os.path.join(DATA, "csgo_russian.txt"))
    en = tokens(os.path.join(DATA, "csgo_english.txt"))
    items = {}
    # the prefabs' own lines (what an item does not say itself, its prefabs say: a rifle's model, name, rarity)
    prefabs = {}
    for k, v in kv.pairs(ig):
        if k == "prefabs":
            for pn, pv in kv.pairs(v):
                if isinstance(pv, list):
                    prefabs[pn] = pv

    def inherited(it, key, depth=0):
        r = kv.get(it, key)
        if r or depth > 8:
            return r or ""
        for pn in (kv.get(it, "prefab") or "").split():
            if pn in prefabs:
                r = inherited(prefabs[pn], key, depth + 1)
                if r:
                    return r
        return ""

    for k, v in kv.pairs(ig):
        if k != "items":
            continue
        for defindex, it in kv.pairs(v):
            if not isinstance(it, list):
                continue
            n = kv.get(it, "name") or ""
            prefab = kv.get(it, "prefab") or ""
            kind = None
            if n.startswith(("weapon_knife", "weapon_bayonet")) and "melee_unusual" in prefab:
                kind = "knife"
            elif "hands_paintable" in prefab:
                kind = "gloves"
            elif n in RIFLES:
                kind = "rifle"
            if kind:
                items[n] = (defindex, kind, inherited(it, "model_player"), inherited(it, "item_name"), inherited(it, "item_rarity"))
    kits = {}
    rarity = {}
    for k, v in kv.pairs(ig):
        if k == "paint_kits":
            for _, pk in kv.pairs(v):
                if isinstance(pk, list) and kv.get(pk, "name"):
                    kits[kv.get(pk, "name")] = {a: b for a, b in kv.pairs(pk) if isinstance(b, str)}
        elif k == "paint_kits_rarity":
            for a, b in kv.pairs(v):
                rarity[a] = b
    # every pair CS2 made an icon for
    pairs = set()
    names = sorted(items, key=len, reverse=True)
    rx = re.compile(r"default_generated/(.+)_(light|medium|heavy)_png\.vtex_c$")
    for line in open(INDEX, encoding="utf-8"):
        m = rx.search(line.split("\t", 1)[0])
        if not m:
            continue
        rest = m.group(1)
        for it in names:
            if rest.startswith(it + "_"):
                paint = rest[len(it) + 1:]
                if paint in kits:
                    pairs.add((it, paint))
                break
    lines = []
    for it, (defindex, kind, model, loc, rar) in sorted(items.items()):
        lines.append("\t".join(["item", it, defindex, kind, model, name(ru, loc), name(en, loc), rar]))
    for it, paint in sorted(pairs):
        k = kits[paint]
        tag = k.get("description_tag", "")
        lines.append("\t".join(["paint", it, paint, rarity.get(paint, ""), k.get("wear_remap_min", "0"), k.get("wear_remap_max", "1"), k.get("style", "0"),
                                k.get("seed", "0"), k.get("use_legacy_model", "0"), name(ru, tag), name(en, tag)]))
    # the charms (a rifle's keychain): those with a model and an icon
    charms = 0
    for k, v in kv.pairs(ig):
        if k != "keychain_definitions":
            continue
        for _, kc in kv.pairs(v):
            if not isinstance(kc, list):
                continue
            fields = [kv.get(kc, key) or "" for key in ("name", "item_rarity", "image_inventory", "pedestal_display_model", "loc_name")]
            if fields[0] and fields[2] and fields[3]:
                lines.append("\t".join(["charm"] + fields[:4] + [name(ru, fields[4]), name(en, fields[4])]))
                charms += 1
    # the stickers: those whose material the game has (a spray's or a patch's lies elsewhere)
    have = set()
    for line in open(INDEX, encoding="utf-8"):
        path = line.split("\t", 1)[0]
        if path.startswith("stickers/") and path.endswith(".vmat_c"):
            have.add(path[len("stickers/"):-len(".vmat_c")])
    stickers = 0
    for k, v in kv.pairs(ig):
        if k != "sticker_kits":
            continue
        for _, sk in kv.pairs(v):
            if not isinstance(sk, list):
                continue
            mat = kv.get(sk, "sticker_material") or ""
            if mat in have:
                loc = kv.get(sk, "item_name") or ""
                lines.append("\t".join(["sticker", kv.get(sk, "name") or mat, kv.get(sk, "item_rarity") or "rare", mat, name(ru, loc), name(en, loc)]))
                stickers += 1
    open(out, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
    return {"items": len(items), "finishes": len(pairs), "charms": charms, "stickers": stickers}


if __name__ == "__main__":
    print(build(sys.argv[1]))
