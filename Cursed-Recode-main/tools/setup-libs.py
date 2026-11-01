"""
Собирает папку libs/ с нуля (на другом ПК или после чистки): jar игры, библиотеки Minecraft,
Fabric Loader с зависимостями, Fabric API (вложенные модули) и исходники для IDE.
Берёт файлы из %APPDATA%\\.minecraft, если они там есть, иначе качает.

    python tools\\setup-libs.py

После этого: tools\\aw.cmd (применить access widener) и при желании tools\\decompile.cmd (исходники игры).
"""
import hashlib
import json
import os
import re
import shutil
import sys
import urllib.parse
import urllib.request
import zipfile
from concurrent.futures import ThreadPoolExecutor

MC_VERSION = "26.3"
LOADER_VERSION = "0.19.5"
FABRIC_API_URL = "https://cdn.modrinth.com/data/P7dR8mSH/versions/bNnaTiuM/fabric-api-0.161.0%2B26.3.jar"
FABRIC_API_FILE = "fabric-api-0.161.0+26.3.jar"
MIXINEXTRAS = ("io.github.llamalad7", "mixinextras-fabric", "0.5.5")
FABRIC_MAVEN = "https://maven.fabricmc.net/"
VIA = [
    ("com/viaversion/viaversion-api/5.12.1/viaversion-api-5.12.1.jar", "ed26996ea02a86e1b948e4569b93c4a738726b8c"),
    ("com/viaversion/viaversion-common/5.12.1/viaversion-common-5.12.1.jar", "01aa06e440d567b9fdd39803e1a5f2edb4b29f79"),
    ("com/viaversion/vialoader/4.0.6/vialoader-4.0.6.jar", "6e8c367f360a12f8ddcf114199a6eea48802e4d5"),
]
# OpenGL and VMA (Mojang's Vulkan memory allocator) serve only Mojang's renderers.
NOT_NEEDED = {"lwjgl-opengl", "lwjgl-vma"}

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MC_DIR = os.path.join(os.environ.get("APPDATA", ""), ".minecraft")


def p(*parts):
    path = os.path.join(ROOT, *parts)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    return path


def sha1(path):
    h = hashlib.sha1()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


# Серверы, откуда берутся ViaVersion-артефакты: repo.viaversion.com режет 403 запросам из диапазонов
# GitHub Actions, Maven Central и зеркало PaperMC - нет. Порядок = что пробуем сначала.
VIA_SOURCES = [
    "https://repo.viaversion.com/",
    "https://repo.papermc.io/repository/maven-public/",
    "https://repo1.maven.org/maven2/",
]

UA = "cursed-recode-setup-libs/1.0 (+https://github.com/kpanics/games)"


def _request(url, timeout=60):
    return urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": UA}), timeout=timeout)


def fetch(url, dst):
    print("  download", url)
    last = None
    for _ in range(3):
        try:
            with _request(url) as r, open(dst, "wb") as f:
                shutil.copyfileobj(r, f)
            return
        except Exception as e:
            last = e
            print("  again:", e)
    raise SystemExit(f"[setup-libs] не скачалось: {url}: {last}")


def get_json(url):
    with _request(url) as r:
        return json.load(r)


def fabric_api_urls():
    """Откуда брать fabric-api, по порядку: API Modrinth (последняя под нашу версию MC), закреплённый
    файл cdn.modrinth.com, maven Fabric с версией закреплённого файла."""
    urls = []
    try:
        with _request(f"https://api.modrinth.com/v2/project/P7dR8mSH/version?game_versions=%5B%22{MC_VERSION}%22%5D", timeout=20) as r:
            versions = json.load(r)
        files = next(v["files"] for v in versions if v.get("loaders") and "fabric" in v["loaders"])
        url = next(f["url"] for f in files if f.get("primary")) or files[0]["url"]
        print("  fabric-api:", url.rsplit("/", 1)[-1])
        urls.append(url)
    except Exception as e:
        print("  modrinth api не ответил:", e)
    urls.append(FABRIC_API_URL)
    quoted = urllib.parse.quote(FABRIC_API_FILE)
    urls.append(f"https://maven.fabricmc.net/net/fabricmc/fabric-api/fabric-api/{quoted}/{quoted}")
    return urls


def loader_profile():
    """Профиль Fabric Loader под нашу версию; если закреплённый лоудер не подошёл — последний стабильный."""
    try:
        return get_json(f"https://meta.fabricmc.net/v2/versions/loader/{MC_VERSION}/{LOADER_VERSION}/profile/json")
    except Exception as e:
        print(f"  loader {LOADER_VERSION} под {MC_VERSION} не нашёлся ({e}), берём последний стабильный")
    latest = next(v["version"] for v in get_json("https://meta.fabricmc.net/v2/versions/loader") if v.get("stable"))
    print("  loader:", latest)
    return get_json(f"https://meta.fabricmc.net/v2/versions/loader/{MC_VERSION}/{latest}/profile/json")


def allowed_on_windows(rules):
    if not rules:
        return True
    ok = False
    for r in rules:
        os_rule = r.get("os")
        match = os_rule is None or os_rule.get("name") in (None, "windows")
        if match:
            ok = r["action"] == "allow"
    return ok


def maven_path(group, artifact, version, classifier=None):
    name = f"{artifact}-{version}" + (f"-{classifier}" if classifier else "") + ".jar"
    return "/".join([*group.split("."), artifact, version, name])


def main():
    # --- vanilla version json ---
    local_json = os.path.join(MC_DIR, "versions", MC_VERSION, f"{MC_VERSION}.json")
    if os.path.exists(local_json):
        v = json.load(open(local_json, encoding="utf-8"))
    else:
        manifest = get_json("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
        url = next(x["url"] for x in manifest["versions"] if x["id"] == MC_VERSION)
        v = get_json(url)
    print("Minecraft", v["id"], "java", v["javaVersion"]["majorVersion"], "assetIndex", v["assetIndex"]["id"])

    # --- game jar (pristine copy in libs/vanilla; libs/minecraft gets the AW-applied one via tools/aw.cmd) ---
    client = v["downloads"]["client"]
    dst = p("libs", "vanilla", f"minecraft-{MC_VERSION}.jar")
    if not (os.path.exists(dst) and sha1(dst) == client["sha1"]):
        candidates = [os.path.join(MC_DIR, "versions", MC_VERSION, f"{MC_VERSION}.jar")]
        vdir = os.path.join(MC_DIR, "versions")
        if os.path.isdir(vdir):
            for d in os.listdir(vdir):
                candidates.append(os.path.join(vdir, d, f"{d}.jar"))
        src = next((c for c in candidates if os.path.exists(c) and os.path.getsize(c) == client["size"] and sha1(c) == client["sha1"]), None)
        if src:
            shutil.copy2(src, dst)
        else:
            fetch(client["url"], dst)
        assert sha1(dst) == client["sha1"], "game jar sha1 mismatch"
    print("game jar ok")
    # Папку создаём, но jar сюда не копируем: его пишет tools\aw.cmd (vanilla + access widener).
    # Копия с сохранённым временем файла (copy2) для aw.cmd выглядела бы «новее входов» - и он
    # молча пропускал расширение доступа, оставляя javac с приватными полями игры.
    os.makedirs(os.path.join(ROOT, "libs", "minecraft"), exist_ok=True)

    # --- Minecraft libraries (windows x64 only) ---
    n = 0
    for lib in v["libraries"]:
        if not allowed_on_windows(lib.get("rules")) or "natives-windows-arm64" in lib["name"]:
            continue
        # Mojang's renderers only: Cursed Recode draws with its own Vulkan (native/), these never load.
        if lib["name"].split(":")[1] in NOT_NEEDED:
            continue
        art = lib["downloads"]["artifact"]
        dst = p("libs", "mc", os.path.basename(art["path"]))
        if os.path.exists(dst) and sha1(dst) == art["sha1"]:
            n += 1
            continue
        local = os.path.join(MC_DIR, "libraries", *art["path"].split("/"))
        if os.path.exists(local) and sha1(local) == art["sha1"]:
            shutil.copy2(local, dst)
        else:
            fetch(art["url"], dst)
        assert sha1(dst) == art["sha1"], lib["name"]
        n += 1
    print("minecraft libraries:", n)

    # servers of older versions (cursed.recode.via.Via): ViaVersion and ViaLoader, beside the game's libraries (they go
    # with them to a player's libraries/). Пробуем по очереди: главный repo режет 403 с GitHub Actions.
    for path, digest in VIA:
        dst = p("libs", "mc", path.rsplit("/", 1)[1])
        if not (os.path.exists(dst) and sha1(dst) == digest):
            got = False
            for base in VIA_SOURCES:
                try:
                    fetch(base + path, dst)
                except SystemExit as e:
                    print(f"  источник {base} не дал: {e}")
                    continue
                if sha1(dst) == digest:
                    got = True
                    break
                print(f"  {base} дал не те байты (sha1 не сходится)")
                os.remove(dst)
            if not got:
                raise SystemExit(f"[setup-libs] ViaVersion не досталась ниоткуда: {path}")
        assert sha1(dst) == digest, path
    print("via:", len(VIA))

    # --- Fabric Loader + deps (from the launcher profile json) ---
    prof = loader_profile()
    for lib in prof["libraries"]:
        g, a, ver = lib["name"].split(":")
        if a == "intermediary":
            continue  # 26.x: intermediary нет, игра без обфускации
        rel = maven_path(g, a, ver)
        dst = p("libs", "fabric", f"{a}-{ver}.jar")
        if os.path.exists(dst):
            continue
        local = os.path.join(MC_DIR, "libraries", *rel.split("/"))
        if os.path.exists(local):
            shutil.copy2(local, dst)
        else:
            fetch(lib.get("url", FABRIC_MAVEN).rstrip("/") + "/" + rel, dst)
        print("fabric lib", a, ver)
    # dev-only: MixinExtras лежит внутри loader-jar и в проде грузится оттуда, в dev нужен на classpath
    g, a, ver = MIXINEXTRAS
    dst = p("libs", "fabric", f"{a}-{ver}.jar")
    if not os.path.exists(dst):
        fetch(FABRIC_MAVEN + maven_path(g, a, ver), dst)

    # --- sources for IDE: loader, mixin, mixinextras ---
    for f in os.listdir(p("libs", "fabric", "x")[:-1]):
        if not f.endswith(".jar"):
            continue
        m = re.match(r"(.+?)-(\d.+)\.jar$", f)
        a, ver = m.group(1), m.group(2)
        dst = p("libs", "sources", "fabric", f"{a}-{ver}-sources.jar")
        if os.path.exists(dst):
            continue
        group = {"mixinextras-fabric": "io.github.llamalad7", "sponge-mixin": "net.fabricmc", "fabric-loader": "net.fabricmc"}.get(a, "org.ow2.asm")
        try:
            fetch(FABRIC_MAVEN + maven_path(group, a, ver, "sources"), dst)
        except Exception as e:
            print("  no sources for", f, e)

    # --- Fabric API: nested module jars onto the classpath (+ sources) ---
    fat = p("tools", FABRIC_API_FILE)
    if not os.path.exists(fat):
        for url in fabric_api_urls():
            try:
                fetch(url, fat)
                zipfile.ZipFile(fat).namelist()  # не zip - пробуем следующий источник
                break
            except SystemExit as e:
                print(f"  источник не дал: {e}")
            except zipfile.BadZipFile:
                print(f"  {url} - не zip, пробуем дальше")
        else:
            raise SystemExit("[setup-libs] fabric-api не досталась ниоткуда")
    with zipfile.ZipFile(fat) as z:
        nested = [x for x in z.namelist() if x.startswith("META-INF/jars/") and x.endswith(".jar")]
        for x in nested:
            dst = p("libs", "fabric-api", os.path.basename(x))
            if not os.path.exists(dst):
                with open(dst, "wb") as f:
                    f.write(z.read(x))
    print("fabric-api modules:", len(nested))

    def api_sources(f):
        m = re.match(r"(.+?)-(\d.+)\.jar$", f)
        a, ver = m.group(1), m.group(2)
        dst = p("libs", "sources", "fabric-api", f"{a}-{ver}-sources.jar")
        if os.path.exists(dst):
            return
        for _ in range(3):
            try:
                urllib.request.urlretrieve(f"{FABRIC_MAVEN}net/fabricmc/fabric-api/{a}/{ver}/{a}-{ver}-sources.jar", dst)
                return
            except Exception:
                pass
        print("  no sources for", f)

    with ThreadPoolExecutor(8) as ex:
        list(ex.map(api_sources, [f for f in os.listdir(p("libs", "fabric-api", "x")[:-1]) if f.endswith(".jar")]))

    print("done. next: tools\\aw.cmd, then tools\\decompile.cmd (optional, few minutes)")


if __name__ == "__main__":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass
    sys.exit(main())
