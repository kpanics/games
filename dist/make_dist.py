"""Собирает дистрибутив Cursed Recode «на GitHub» (см. dist/README.md): манифест билда с ed25519-подписью,
start.json (статический ответ /api/loader/recode/start) и файлы билда как object-<sha256> — всё это ложится
в GitHub Release, откуда лоадер (loader/api.rs `github()`) это и берёт.

    python dist/make_dist.py --home dist-home --out dist-out --key <hex-private> \
        --build 7 --version 0.1.0 --notes "..." [--fetch-assets 26.3] [--publish]

--home — собранный билд (bin/, game/, libraries/, loader/, assets/indexes/, runtime.zip, log4j2.xml);
--out  — куда положить start.json и object-<sha256>;
--key  — приватный ключ (64 hex), парная публичная половина вшита в лоадер при сборке (CURSED_VERIFY_KEY);
--fetch-assets <версия MC> — скачать индекс ассетов с piston-meta в home/assets/indexes/<id>.json;
--publish — пересоздать релиз с тегом `dist` и выложить туда всё (нужен GH_TOKEN).
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.request

# Как запускается игра (см. Cursed-Recode-main/run.cmd — тот же набор, путями билда)
MAIN = "net.fabricmc.loader.impl.launch.knot.KnotClient"
HOST = "bin/CursedRecode.exe"
JVM = [
    "-Xss1M",
    "-XX:StackShadowPages=32",
    "--enable-native-access=ALL-UNNAMED",
    "--add-exports", "java.base/jdk.internal.misc=ALL-UNNAMED",
    "-Dfabric.development=true",
    "-Dfabric.log.disableAnsi=true",
    "-Dlog4j2.formatMsgNoLookups=true",
    "-Dlog4j.configurationFile=${root}/log4j2.xml",
]
# Порядок classpath: fabric-лоудер, игра, библиотеки, fabric-api, мод
CLASSPATH_DIRS = ["libraries/fabric", "game", "libraries/mc", "libraries/fabric-api"]
CLASSPATH_FILES = {"game": ["minecraft-{v}.jar", "cursed-recode-{v}.jar"]}


def kind_of(path: str) -> str:
    p = path.replace("\\", "/")
    if p.startswith("loader/"):
        return "loader"
    if p.startswith("assets/indexes/") and p.endswith(".json"):
        return "assets-index"
    if p == "runtime.zip":
        return "zip"
    if p == "bin/cursed_twin.exe":
        return "watchdog"
    if p.startswith("bin/"):
        return "native"
    if p.startswith("libraries/"):
        return "library"
    if p.startswith("game/"):
        return "code"
    if p == "log4j2.xml":
        return "config"
    return "code"


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def fetch_assets(home: str, mc_version: str) -> str:
    manifest = json.load(urllib.request.urlopen("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"))
    url = next(x["url"] for x in manifest["versions"] if x["id"] == mc_version)
    v = json.load(urllib.request.urlopen(url))
    index = v["assetIndex"]
    dst = os.path.join(home, "assets", "indexes", f"{index['id']}.json")
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    urllib.request.urlretrieve(index["url"], dst)
    print(f"assets index {index['id']} ({os.path.getsize(dst)} bytes)")
    return index["id"]


def classpath(home: str, version: str) -> list:
    out = []
    for d in CLASSPATH_DIRS:
        full = os.path.join(home, d)
        if not os.path.isdir(full):
            continue
        jars = sorted(j for j in os.listdir(full) if j.endswith(".jar"))
        for j in jars:
            # jar игры и jar мода — точечно, остальное из game/ не берём
            if d == "game" and not any(j == t.format(v=version) or j.startswith("minecraft-") or j.startswith("cursed-recode-") for t in CLASSPATH_FILES.get("game", [])):
                continue
            out.append(f"{d}/{j}")
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--home", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--key", required=True, help="ed25519 private key, 64 hex")
    ap.add_argument("--build", type=int, required=True)
    ap.add_argument("--version", default="0.1.0")
    ap.add_argument("--notes", default="")
    ap.add_argument("--assets", default=None, help="id индекса ассетов (по умолчанию — из имени файла)")
    ap.add_argument("--fetch-assets", default=None, help="версия Minecraft: скачать индекс ассетов")
    ap.add_argument("--publish", action="store_true", help="выложить всё в GitHub Release с тегом dist")
    ap.add_argument("--repo", default=None, help="owner/repo для --publish")
    ap.add_argument("--title", default="Cursed Recode")
    a = ap.parse_args()

    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

    home, out = a.home, a.out
    os.makedirs(out, exist_ok=True)

    if a.fetch_assets:
        a.assets = a.assets or fetch_assets(home, a.fetch_assets)

    files = []
    for root, _, names in os.walk(home):
        for n in sorted(names):
            full = os.path.join(root, n)
            rel = os.path.relpath(full, home).replace("\\", "/")
            files.append({"path": rel, "sha256": sha256(full), "size": os.path.getsize(full), "kind": kind_of(rel)})
    files.sort(key=lambda f: f["path"])

    assets_id = a.assets
    if not assets_id:
        for f in files:
            if f["kind"] == "assets-index":
                assets_id = os.path.basename(f["path"])[:-5]
    if not assets_id:
        print("нет индекса ассетов: --assets или --fetch-assets", file=sys.stderr)
        return 1

    manifest = {
        "build": a.build,
        "version": a.version,
        "notes": a.notes,
        "at": int(time.time()),
        "launch": {
            "host": HOST,
            "jdk": "runtime",
            "main": MAIN,
            "classpath": classpath(home, a.version),
            "jvm": ["-Dcursed.recode.native=${root}/bin/cursed_recode.dll", *JVM],
            "memory": {"least": 2048},
            "game": [
                "--assetIndex", assets_id,
                "--assetsDir", "${root}/assets",
                "--gameDir", "${root}",
                "--versionType", "Cursed Recode",
            ],
        },
        "assets": assets_id,
        "files": files,
    }
    text = json.dumps(manifest, ensure_ascii=False, separators=(",", ":"))
    key = Ed25519PrivateKey.from_private_bytes(bytes.fromhex(a.key))
    sig = key.sign(text.encode("utf-8")).hex()

    start = {
        "uid": 0,
        "uid_text": "0",
        "login": "Player",
        "recode": {"active": True, "forever": True},
        "build": a.build,
        "manifest": text,
        "sig": sig,
        "start_ticket": "00" * 32,
        "ticket_seconds": 3600,
        "ticket": "00" * 32,
        "pair_window": 0,
        "pair_seconds": 0,
        "reset_from": 0,
        "stub": False,
    }
    with open(os.path.join(out, "start.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(start, f, ensure_ascii=False, separators=(",", ":"))

    for f in files:
        dst = os.path.join(out, f"object-{f['sha256']}")
        if os.path.exists(dst) and sha256(dst) == f["sha256"]:
            continue
        shutil.copyfile(os.path.join(home, f["path"]), dst)

    total = sum(f["size"] for f in files)
    print(f"build {a.build} ({a.version}): {len(files)} файлов, {total / 1048576:.1f} MB, подпись {sig[:16]}…")
    kinds = {}
    for f in files:
        kinds[f["kind"]] = kinds.get(f["kind"], 0) + 1
    print("по видам:", ", ".join(f"{k}={v}" for k, v in sorted(kinds.items())))
    print(f"classpath: {len(manifest['launch']['classpath'])} записей")

    if a.publish:
        repo = a.repo
        if not repo:
            repo = subprocess.run(["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"],
                                  capture_output=True, text=True, check=True).stdout.strip()
        subprocess.run(["gh", "release", "delete", "dist", "--cleanup-tag", "--yes", "--repo", repo])
        assets = [os.path.join(out, x) for x in sorted(os.listdir(out))]
        notes = f"Cursed Recode, сборка {a.build} ({a.version}). {a.notes}".strip()
        subprocess.run(["gh", "release", "create", "dist", *assets, "--repo", repo, "--title", a.title,
                        "--notes", notes, "--latest"], check=True)
        print(f"релиз `dist` опубликован: {len(assets)} файлов")
    return 0


if __name__ == "__main__":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass
    sys.exit(main())
