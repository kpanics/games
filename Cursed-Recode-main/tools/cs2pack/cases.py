"""CS2's weapon cases for the Cosmetics page's cases: out of items_game each case (its prefab weapon_case), its name
and its picture, the skins it holds by rarity (its client loot list's lists) with their names and their wear's range;
the rare special items each holds (knives, gloves) out of the open data of ByMykel's CSGO-API (crates.json and
skins.json: Valve keeps those lists to itself), only those the catalog has; each skin's icon (CS2's own, the light
one), the cases' pictures, the sounds of an opening (CS2's UI ones, as events of ours).

    python cases.py OUT CRATES.json SKINS.json

OUT/cases.txt (tab between fields), oldest case first:
  "case NAME SERIES RU EN"
  "drop CASE RARITY WEAPON PAINT WEAR_MIN WEAR_MAX RU EN" (RARITY rare, mythical, legendary or ancient; the names
      "weapon | finish")
  "rare CASE WEAPON PAINT" (PAINT empty: the item's vanilla)
OUT/cases/<case>.rtx its picture, OUT/cases/<case>_rare.rtx its rare special item's (when CS2 has one);
OUT/icons/<weapon>/<paint>.rtx each skin's icon; OUT/sound/ the opening's sounds, their events added to events.txt."""
import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import catalog  # noqa: E402
import kv  # noqa: E402
import rtx  # noqa: E402
import vrf_export  # noqa: E402

DATA = catalog.DATA
ICONS = r"D:\CS2\work\icons"
WORK = r"D:\CS2\work\sounds"
FFMPEG = os.environ.get("FFMPEG", "ffmpeg")
RARITIES = ("rare", "mythical", "legendary", "ancient")
# the opening's sounds (CS2's panorama ones): our event, its volume, the sound
SOUNDS = [
    ("UI.CrateScroll", 0.35, "sounds/ui/csgo_ui_crate_item_scroll.vsnd"),
    ("UI.CrateUnlock", 0.5, "sounds/ui/panorama/case_unlock_01.vsnd"),
    ("UI.CrateDrop", 0.5, "sounds/ui/panorama/case_drop_01.vsnd"),
    ("UI.CrateReveal.rare", 0.55, "sounds/ui/panorama/case_reveal_rare_01.vsnd"),
    ("UI.CrateReveal.mythical", 0.55, "sounds/ui/panorama/case_reveal_mythical_01.vsnd"),
    ("UI.CrateReveal.legendary", 0.55, "sounds/ui/panorama/case_reveal_legendary_01.vsnd"),
    ("UI.CrateReveal.ancient", 0.55, "sounds/ui/panorama/case_reveal_ancient_01.vsnd"),
    ("UI.CrateAwarded.rare", 0.5, "sounds/ui/panorama/case_awarded_2_rare_01.vsnd"),
    ("UI.CrateAwarded.mythical", 0.5, "sounds/ui/panorama/case_awarded_3_mythical_01.vsnd"),
    ("UI.CrateAwarded.legendary", 0.5, "sounds/ui/panorama/case_awarded_4_legendary_01.vsnd"),
    ("UI.CrateAwarded.ancient", 0.5, "sounds/ui/panorama/case_awarded_5_ancient_01.vsnd"),
    ("UI.CrateKnife", 0.55, "sounds/ui/panorama/item_showcase_knife_01.vsnd"),
]


def picture(vtex, dst, size):
    """A picture of CS2's (its archive's path) as ours; false when CS2 has none."""
    if os.path.exists(dst):
        return True
    png = os.path.join(ICONS, os.path.basename(vtex).replace("_png.vtex_c", ".png"))
    if not os.path.exists(png):
        vrf_export.file(vtex, png)
    if not os.path.exists(png):
        return False
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    rtx.convert(png, dst, size, True)
    return True


def sounds(out):
    """The opening's sounds as Ogg Vorbis beside the hands' ones, their events added (or put right) in events.txt."""
    sdir = os.path.join(out, "sound")
    os.makedirs(WORK, exist_ok=True)
    path = os.path.join(sdir, "events.txt")
    lines = [l for l in open(path, encoding="utf-8").read().split("\n") if l and not l.startswith("UI.Crate")] if os.path.exists(path) else []
    for name, volume, vsnd in SOUNDS:
        st = os.path.splitext(vsnd[len("sounds/"):])[0].replace("/", ".")
        ogg = os.path.join(sdir, st + ".ogg")
        if not os.path.exists(ogg):
            wav = os.path.join(WORK, st + ".wav")
            if not os.path.exists(wav):
                vrf_export.file(vsnd + "_c", wav)
            subprocess.run([FFMPEG, "-y", "-loglevel", "error", "-i", wav, "-ar", "44100", "-ac", "2", "-c:a", "libvorbis", "-q:a", "5", ogg], check=True)
        lines.append("\t".join([name, str(volume), "1", "0", "0", "0", st, ""]))
    open(path, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")


def build(out, crates_path, skins_path):
    ig = kv.get(kv.parse(open(os.path.join(DATA, "items_game.txt"), encoding="utf-8", errors="replace").read()), "items_game")
    ru = catalog.tokens(os.path.join(DATA, "csgo_russian.txt"))
    en = catalog.tokens(os.path.join(DATA, "csgo_english.txt"))
    prefabs, kits, rev, cll = {}, {}, {}, {}
    for k, v in kv.pairs(ig):
        if k == "prefabs":
            prefabs.update({a: b for a, b in kv.pairs(v) if isinstance(b, list)})
        elif k == "paint_kits":
            for _, pk in kv.pairs(v):
                if isinstance(pk, list) and kv.get(pk, "name"):
                    kits[kv.get(pk, "name")] = {a: b for a, b in kv.pairs(pk) if isinstance(b, str)}
        elif k == "revolving_loot_lists":
            rev.update({a: b for a, b in kv.pairs(v)})
        elif k == "client_loot_lists":
            cll.update({a: b for a, b in kv.pairs(v)})

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

    weapons, cases = {}, []
    for k, v in kv.pairs(ig):
        if k != "items":
            continue
        for defindex, it in kv.pairs(v):
            if not isinstance(it, list):
                continue
            n = kv.get(it, "name") or ""
            if n.startswith("weapon_"):
                weapons[n] = inherited(it, "item_name")
            if (kv.get(it, "prefab") or "") != "weapon_case":
                continue
            series = None
            for an, av in kv.pairs(kv.get(it, "attributes") or []):
                if an == "set supply crate series" and isinstance(av, list):
                    series = kv.get(av, "value")
            if series and rev.get(series):
                cases.append((int(series), defindex, n, rev[series], kv.get(it, "item_name") or "", inherited(it, "image_inventory")))
    cases.sort()
    # the rare special items (open data): each case's by its definition
    skins = {s["id"]: s for s in json.load(open(skins_path, encoding="utf-8"))}
    rares = {}
    for c in json.load(open(crates_path, encoding="utf-8")):
        got = []
        for r in c.get("contains_rare") or []:
            sid = r.get("id", "")
            if sid.startswith("skin-vanilla-"):
                got.append((sid[len("skin-vanilla-"):], ""))
            elif sid in skins:
                s = skins[sid]
                got.append((s["weapon"]["id"], s.get("pattern", {}).get("id", "")))
        rares[str(c.get("def_index"))] = got
    have = set()
    for line in open(os.path.join(out, "catalog.txt"), encoding="utf-8"):
        f = line.rstrip("\n").split("\t")
        if f[0] == "paint":
            have.add((f[1], f[2]))
        elif f[0] == "item" and f[3] == "knife":
            have.add((f[1], ""))
    lines, stats = [], {"cases": 0, "drops": 0, "rares": 0, "icons missing": 0}
    for series, defindex, name, loot, loc, inventory_image in cases:
        lines.append("\t".join(["case", name, str(series), catalog.name(ru, loc), catalog.name(en, loc)]))
        stats["cases"] += 1
        # The item ID is not always the image ID (Winter 2013 and Operation Vanguard).
        image = inventory_image or f"econ/weapon_cases/{name}"
        if not picture(f"panorama/images/{image}_png.vtex_c", os.path.join(out, "cases", name + ".rtx"), 384):
            raise FileNotFoundError(f"Missing case image: {name} ({image})")
        picture(f"panorama/images/econ/weapon_cases/{name}_rare_item_png.vtex_c", os.path.join(out, "cases", name + "_rare.rtx"), 256)
        # its skins by rarity: the lists under its own whose names end with one
        todo, seen = [loot], set()
        while todo:
            n = todo.pop(0)
            if n in seen or n not in cll:
                continue
            seen.add(n)
            rarity = next((r for r in RARITIES if n.endswith("_" + r)), None)
            for entry, _ in kv.pairs(cll[n]):
                if entry in cll:
                    todo.append(entry)
                elif rarity and entry.startswith("[") and "]" in entry:
                    paint, weapon = entry[1:].split("]", 1)
                    k = kits.get(paint, {})
                    tag = k.get("description_tag", "")
                    wloc = weapons.get(weapon, weapon)
                    lines.append("\t".join(["drop", name, rarity, weapon, paint, k.get("wear_remap_min", "0"), k.get("wear_remap_max", "1"),
                                            f"{catalog.name(ru, wloc)} | {catalog.name(ru, tag)}", f"{catalog.name(en, wloc)} | {catalog.name(en, tag)}"]))
                    stats["drops"] += 1
                    # (the archive's names are lower case: a paint's own may not be)
                    if not picture(f"panorama/images/econ/default_generated/{weapon}_{paint}_light_png.vtex_c".lower(), os.path.join(out, "icons", weapon, paint + ".rtx"), 256):
                        stats["icons missing"] += 1
        for weapon, paint in dict.fromkeys(rares.get(defindex, [])):
            if (weapon, paint) in have:
                lines.append("\t".join(["rare", name, weapon, paint]))
                stats["rares"] += 1
    open(os.path.join(out, "cases.txt"), "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
    sounds(out)
    return stats


if __name__ == "__main__":
    print(build(sys.argv[1], sys.argv[2], sys.argv[3]))
