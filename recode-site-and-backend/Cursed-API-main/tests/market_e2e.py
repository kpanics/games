"""market_e2e.py: the market on the staging instance, as members use it (run on the server: python3 market_e2e.py).

Uploads every pack in /root/corpus piece by piece through nginx (127.0.0.1:8088), waits for its check, makes a pack
of it, downloads the clean copy back through nginx and compares it byte for byte with the original. Then a row of
broken and hostile archives, each of which must be refused or cleaned, and the rules of the API: resuming, other
people's packs, private ones, drafts, moderation, versions, pictures, limits.
"""
import hashlib
import http.cookiejar
import io
import json
import os
import struct
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
import zlib

BASE = "http://127.0.0.1:8088"
CORPUS = "/root/corpus"
failures = []


def ok(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        failures.append(what)


class Member:
    def __init__(self, login):
        self.login = login
        self.jar = http.cookiejar.CookieJar()
        self.op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))

    def call(self, method, path, body=None, raw=None, origin=BASE, token=None):
        h = {}
        if origin:
            h["Origin"] = origin
        if token:
            h["Authorization"] = "Bearer " + token
        data = None
        if body is not None:
            data = json.dumps(body).encode()
            h["Content-Type"] = "application/json"
        if raw is not None:
            data = raw
            h["Content-Type"] = "application/octet-stream"
        req = urllib.request.Request(BASE + path, data=data, method=method, headers=h)
        try:
            with self.op.open(req, timeout=600) as r:
                b = r.read()
                ct = r.headers.get("Content-Type", "")
                return r.status, (json.loads(b) if ct.startswith("application/json") and b else b), r.headers
        except urllib.error.HTTPError as e:
            b = e.read()
            try:
                j = json.loads(b)
            except Exception:
                j = b
            return e.code, j, e.headers

    def register(self, key=""):
        st, j, _ = self.call("POST", "/api/register", {"login": self.login, "email": self.login.lower() + "@e.st", "password": "0123456789", "key": key})
        assert st == 200, (st, j)
        self.uid = j["uid"]
        return self

    def upload(self, data, name, sha=True, chunk=None):
        size = len(data)
        st, j, _ = self.call("POST", "/api/uploads", {"name": name, "size": size, "sha256": hashlib.sha256(data).hexdigest() if sha else None})
        if st != 200:
            return None, {"state": "not started", "status": st, "result": j}, 0
        uid, step, at = j["id"], chunk or j["chunk"], 0
        while at < size:
            st, r, _ = self.call("PUT", f"/api/uploads/{uid}?at={at}", raw=data[at:at + step])
            if st != 200:
                return uid, {"state": "piece refused", "status": st, "result": r}, 0
            at = r["received"]
        t0 = time.time()
        while True:
            st, r, _ = self.call("GET", f"/api/uploads/{uid}")
            if r["state"] in ("ready", "refused"):
                return uid, r, time.time() - t0
            time.sleep(0.2)


def zip_of(files, prefix=b""):
    buf = io.BytesIO()
    buf.write(prefix)
    with zipfile.ZipFile(buf, "a" if prefix else "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in files:
            if isinstance(name, zipfile.ZipInfo):
                z.writestr(name, data)
            else:
                z.writestr(name, data)
    return buf.getvalue()


def png(w, h, colour=(120, 80, 200)):
    raw = b"".join(b"\x00" + bytes(colour) * w for _ in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


META = json.dumps({"pack": {"pack_format": 75, "min_format": 75, "max_format": 75, "description": "test"}}).encode()


def same_as_original(original, clean):
    """Every file of the clean copy equals its original (the original's folder around it, if any, taken off)."""
    a = zipfile.ZipFile(io.BytesIO(original))
    b = zipfile.ZipFile(io.BytesIO(clean))
    if b.testzip() is not None:
        return False, "the clean copy fails its own test"
    names = {n.replace("\\", "/"): n for n in a.namelist()}
    differ = 0
    for n in b.namelist():
        cand = [n] if n in names else [o for o in names if o.endswith("/" + n)]
        if not cand:
            return False, "a file of the clean copy has no original: " + n
        if a.read(names[cand[0]]) != b.read(n):
            differ += 1
    return differ == 0, f"{len(b.namelist())} files, {differ} differ"


# ------------------------------------------------------------------------------------------ real packs
print("== real packs")
lu = Member("Lunuca").register(os.popen("/root/staging.sh cli key").read().strip())
mx = Member("Maks").register()
os.system("/root/staging.sh cli renumber 1 0 >/dev/null")  # her UID, as on the live site
os.system("/root/staging.sh stop; sleep 0.3; /root/staging.sh start; sleep 0.6")
st, me, _ = lu.call("GET", "/api/me")
ok(me.get("uid") == 0 and me.get("admin") is True, "her account is UID 0 and moderates")

made = []
for kind in ("resourcepack", "shader"):
    for f in sorted(os.listdir(os.path.join(CORPUS, kind))):
        data = open(os.path.join(CORPUS, kind, f), "rb").read()
        name = f.split("__", 1)[1]
        uid, r, took = mx.upload(data, name)
        res = r.get("result") or {}
        facts = res.get("facts") or {}
        if r["state"] != "ready":
            print(f"  -- {f[:60]:60} {r['state']}: {res.get('message')} {res.get('path') or ''}")
            continue
        game = facts.get("game") or {}
        tag = facts.get("resolution") or ""
        what = ",".join(facts.get("changes") or facts.get("features") or [])
        dropped = sum(v["count"] for v in (facts.get("dropped") or {}).values())
        print(f"  {res['kind'][:6]} {len(data)/1e6:6.1f}MB→{res['size']/1e6:6.1f}MB {took:5.1f}s {f.split('__')[0][:28]:28} {tag:5} {(game.get('versions') or '')[:34]:34} {what[:60]}  (left out {dropped})")
        for wrn in facts.get("warnings") or []:
            print(f"       ! {wrn[:150]}")
        st, page, _ = mx.call("POST", "/api/packs", {"upload": uid, "title": res["title"], "number": "1.0", "blurb": "from Modrinth, for the test", "publish": True})
        ok(st == 201, f"a pack is made of {f[:40]}")
        st, clean, hd = mx.call("GET", f"/api/packs/{page['slug']}/download")
        good, how = same_as_original(data, clean) if st == 200 else (False, f"download {st}")
        ok(good and hd.get("Content-Disposition", "").startswith("attachment"), f"the download is a clean copy of the original: {how}")
        made.append(page)

# ------------------------------------------------------------------------------------------ hostile and broken
print("== hostile and broken archives")
pe = bytearray(b"MZ" + b"\0" * 126)
pe[60] = 64
pe[64:68] = b"PE\0\0"
link = zipfile.ZipInfo("assets/minecraft/link")
link.create_system = 3
link.external_attr = (0o120777 << 16)


def patched(data, fn):
    b = bytearray(data)
    fn(b)
    return bytes(b)


def lie_sizes(b):
    # Say 10 bytes where there are a million: both headers agree, the data doesn't.
    for sig, off in ((b"PK\x03\x04", 22), (b"PK\x01\x02", 24)):
        i = b.find(sig, 0 if sig == b"PK\x03\x04" else 0)
        while i != -1:
            name_len = struct.unpack_from("<H", b, i + (26 if sig == b"PK\x03\x04" else 28))[0]
            nm = bytes(b[i + (30 if sig == b"PK\x03\x04" else 46): i + (30 if sig == b"PK\x03\x04" else 46) + name_len])
            if nm == b"big.txt":
                struct.pack_into("<I", b, i + off, 10)
            i = b.find(sig, i + 4)


def encrypt_flag(b):
    for sig, off in ((b"PK\x03\x04", 6), (b"PK\x01\x02", 8)):
        i = b.find(sig)
        while i != -1:
            struct.pack_into("<H", b, i + off, struct.unpack_from("<H", b, i + off)[0] | 1)
            i = b.find(sig, i + 4)


def rename_central(b):
    i = b.find(b"PK\x01\x02")
    j = b.find(b"pack.png", i)
    b[j:j + 8] = b"pack.exe"


dup = io.BytesIO()
with zipfile.ZipFile(dup, "w") as z:
    z.writestr("pack.mcmeta", META)
    import warnings
    warnings.simplefilter("ignore")
    z.writestr("assets/minecraft/a.txt", b"one")
    z.writestr("assets/minecraft/a.txt", b"two")

bomb = io.BytesIO()
with zipfile.ZipFile(bomb, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("pack.mcmeta", META)
    zeros = b"OggS" + b"\0" * (60 << 20)
    for i in range(18):
        z.writestr(f"assets/minecraft/sounds/z{i}.ogg", zeros)
    del zeros
cases = [
    ("a path out of the pack", zip_of([("pack.mcmeta", META), ("../../etc/evil.txt", b"x")]), "refused", "leads out"),
    ("a link", zip_of([("pack.mcmeta", META), (link, b"/etc/passwd")]), "refused", "link"),
    ("a program by name", zip_of([("pack.mcmeta", META), ("assets/minecraft/run.exe", b"x")]), "refused", "program"),
    ("a program dressed as a PNG", zip_of([("pack.mcmeta", META), ("assets/minecraft/textures/block/stone.png", bytes(pe))]), "refused", "program"),
    ("an archive inside", zip_of([("pack.mcmeta", META), ("assets/minecraft/inner.zip", zip_of([("a", b"b")])), ("assets/minecraft/textures/block/c.png", png(16, 16))]), "ready", "archives inside"),
    ("a script inside", zip_of([("pack.mcmeta", META), ("tools/run.bat", b"format c:"), ("assets/minecraft/textures/block/c.png", png(16, 16))]), "ready", "scripts"),
    ("a program glued in front", zip_of([("pack.mcmeta", META)], prefix=bytes(pe) + b"junk" * 100), "refused", ""),
    ("a single file over 128 MB", zip_of([("pack.mcmeta", META), ("assets/minecraft/sounds/big.ogg", b"OggS" + b"\0" * (130 << 20)), ("assets/minecraft/textures/block/c.png", png(16, 16))]), "ready", "over 128 MB"),
    ("a text file of 100 MB", zip_of([("pack.mcmeta", META), ("assets/minecraft/big.json", b" " * (100 << 20)), ("assets/minecraft/textures/block/c.png", png(16, 16))]), "ready", "text files over 8 MB"),
    ("over 1 GB altogether", bomb.getvalue(), "refused", "unpacked"),
    ("sizes that lie", patched(zip_of([("pack.mcmeta", META), ("big.txt", b"a" * 1_000_000)]), lie_sizes), "refused", ""),
    ("encrypted", patched(zip_of([("pack.mcmeta", META)]), encrypt_flag), "refused", "encrypted"),
    ("a name that differs in the list", patched(zip_of([("pack.mcmeta", META), ("pack.png", png(16, 16))]), rename_central), "refused", "named one way"),
    ("two files of one name", dup.getvalue(), "refused", "same name"),
    ("not a zip at all", os.urandom(5000), "refused", "not a zip"),
    ("neither kind", zip_of([("readme.txt", b"hi"), ("textures/stone.png", png(16, 16))]), "refused", "neither"),
    ("wrapped in a folder, backslashes, junk", zip_of([("My Pack\\pack.mcmeta", META), ("My Pack\\assets\\minecraft\\textures\\block\\stone.png", png(16, 16)), ("__MACOSX/x", b"j"), ("My Pack/Thumbs.db", b"j")]), "ready", "inside a folder"),
    ("a fake picture and a huge one", zip_of([("pack.mcmeta", META), ("assets/minecraft/textures/block/a.png", b"\x89PNG\r\n\x1a\nnope"), ("assets/minecraft/textures/block/b.png", png(9000, 1)), ("assets/minecraft/textures/block/c.png", png(16, 16))]), "ready", "pictures that don't decode"),
    ("windows names and capitals", zip_of([("pack.mcmeta", META), ("assets/minecraft/texts/con.txt", b"x"), ("assets/minecraft/textures/block/Stone.png", png(16, 16)), ("assets/minecraft/textures/block/dirt.png", png(16, 16))]), "ready", "names"),
]
for i, (what, data, want, word) in enumerate(cases):
    uid, r, _ = Member(f"Hostile{i}").register().upload(data, what + ".zip")
    res = r.get("result") or {}
    text = json.dumps(res)
    ok(r["state"] == want and word.lower() in text.lower(), f"{what}: {r['state']} — {(res.get('message') or ', '.join((res.get('facts') or {}).get('fixed') or []) or list(((res.get('facts') or {}).get('dropped') or {}).keys()))}")

mx = Member("Author").register()
st, j, _ = mx.call("POST", "/api/uploads", {"name": "x.zip", "size": 300 << 20})
ok(st == 413, "a pack over 250 MB can't even start")
data = zip_of([("pack.mcmeta", META), ("assets/minecraft/textures/block/stone.png", png(16, 16))])
st, j, _ = mx.call("POST", "/api/uploads", {"name": "x.zip", "size": len(data), "sha256": "0" * 64})
mx.call("PUT", f"/api/uploads/{j['id']}?at=0", raw=data)
st, r, _ = mx.call("GET", f"/api/uploads/{j['id']}")
ok(r["state"] == "refused" and "damaged" in json.dumps(r), "a file that doesn't match its hash is refused")

# ------------------------------------------------------------------------------------------ the rules
print("== the rules of the API")
data = zip_of([("pack.mcmeta", META), ("assets/minecraft/textures/block/stone.png", png(32, 32)), ("pack.png", png(64, 64))])
st, j, _ = mx.call("POST", "/api/uploads", {"name": "resume.zip", "size": len(data)})
up = j["id"]
st, r, _ = mx.call("PUT", f"/api/uploads/{up}?at=0", raw=data[:100])
st, r, _ = mx.call("PUT", f"/api/uploads/{up}?at=0", raw=data[:100])
ok(st == 409 and r.get("received") == 100, "a piece sent twice is refused, with where to go on from")
st, r, _ = mx.call("GET", f"/api/uploads/{up}")
ok(r["received"] == 100, "the status says how much arrived")
st, r, _ = Member("Stranger").call("PUT", f"/api/uploads/{up}?at=100", raw=data[100:])
ok(st == 401, "a stranger's piece is refused before it is read")
st, r, _ = mx.call("PUT", f"/api/uploads/{up}?at=100", raw=data[100:] + b"extra")
ok(st == 413, "more than was announced is refused")
st, r, _ = mx.call("PUT", f"/api/uploads/{up}?at=100", raw=data[100:])
while r["state"] not in ("ready", "refused"):
    time.sleep(0.2)
    st, r, _ = mx.call("GET", f"/api/uploads/{up}")
ok(r["state"] == "ready" and r["result"]["icon"], "the rest arrives, the pack is ready, pack.png became an icon")
st, r, _ = lu.call("GET", f"/api/uploads/{up}")
ok(st == 404, "someone else's upload can't be seen")
st, r, _ = lu.call("POST", "/api/packs", {"upload": up, "title": "Stolen", "number": "1"})
ok(st == 409, "someone else's upload can't be made a pack")
st, p, _ = mx.call("POST", "/api/packs", {"upload": up, "title": "Пепел 32x", "number": "1.0", "about": "Line one\nLine two", "visibility": "private"})
ok(st == 201 and p["slug"] == "pepel-32x" and p["state"] == "draft" and p["visibility"] == "private", f"a private draft: {p.get('slug')}")
slug = p["slug"]
st, r, _ = lu.call("GET", f"/api/packs/{slug}")
ok(st == 200, "she (moderation) sees a private draft")
outsider = Member("Outsider").register()
st, r, _ = outsider.call("GET", f"/api/packs/{slug}")
ok(st == 404, "another member doesn't")
st, r, _ = outsider.call("GET", f"/api/packs/{slug}/download")
ok(st == 404, "nor downloads it")
st, r, _ = Member("Nobody").call("GET", "/api/market")
ok(st == 401, "without an account the market is closed")
st, r, _ = mx.call("PATCH", f"/api/packs/{slug}", {"visibility": "public", "state": "published", "blurb": "Stone after fire"})
ok(st == 200 and r["state"] == "published", "published")
st, r, _ = outsider.call("GET", "/api/market?q=" + urllib.parse.quote("пепел"))
ok(st == 200 and any(i["slug"] == slug for i in r["items"]), "found by a Russian word")
st, r, _ = outsider.call("GET", f"/api/market?q=fire&kind=shaderpack")
ok(not any(i["slug"] == slug for i in r["items"]), "not among the shaders")
st, r, _ = outsider.call("PATCH", f"/api/packs/{slug}", {"title": "Mine now"})
ok(st == 404, "someone else can't edit it")
st, r, _ = mx.call("PATCH", f"/api/packs/{slug}", {"title": "x"}, origin="https://evil.example")
ok(st == 403, "not from another site")
# A new version: same kind only, new number only.
d2 = zip_of([("pack.mcmeta", META), ("assets/minecraft/textures/block/stone.png", png(64, 64))])
u2, r2, _ = mx.upload(d2, "v2.zip")
st, r, _ = mx.call("POST", f"/api/packs/{slug}/versions", {"upload": u2, "number": "1.0"})
ok(st == 409, "a taken version number is refused")
st, r, _ = mx.call("POST", f"/api/packs/{slug}/versions", {"upload": u2, "number": "1.1", "changelog": "64x stone"})
ok(st == 200 and r["version"] == "1.1" and r["tags"]["resolution"] == "64x" and len(r["versions"]) == 2, "version 1.1, read as 64x")
shader = zip_of([("shaders/shaders.properties", b"profile.LOW="), ("shaders/gbuffers_water.fsh", b"void main(){}")])
u3, r3, _ = mx.upload(shader, "shader.zip")
st, r, _ = mx.call("POST", f"/api/packs/{slug}/versions", {"upload": u3, "number": "2.0"})
ok(st == 409, "a shader pack can't become a version of a resource pack")
# Pictures.
pic = png(1280, 720, (40, 60, 90))
st, a, _ = mx.call("POST", f"/api/packs/{slug}/pictures?caption=Stone", raw=pic)
st2, b, _ = mx.call("POST", f"/api/packs/{slug}/pictures", raw=png(800, 600))
ok(st == 201 and st2 == 201, "two pictures")
st, r, _ = mx.call("POST", f"/api/packs/{slug}/pictures", raw=b"GIF89a")
ok(st == 422, "a GIF is refused")
st, img, hd = mx.call("GET", a["big"].replace(BASE, ""), origin=None)
ok(st == 200 and img[:2] == b"\xff\xd8", "the picture is served, as a JPEG")
st, r, _ = mx.call("PATCH", f"/api/packs/{slug}/pictures", {"order": [b["id"], a["id"]], "cover": b["id"]})
ok(st == 200 and r["pictures"][0]["id"] == b["id"] and r["cover"]["big"] == b["big"], "reordered, a new cover")
st, r, _ = mx.call("DELETE", f"/api/packs/{slug}/pictures/{b['id']}")
st, r, _ = mx.call("GET", f"/api/packs/{slug}")
ok(r["cover"]["big"] == a["big"] and len(r["pictures"]) == 1, "the cover moves to what is left")
# Taking it: counted once per member, not for its author.
for m in (outsider, outsider, lu):
    m.call("GET", f"/api/packs/{slug}/download")
mx.call("GET", f"/api/packs/{slug}/download")
st, r, _ = mx.call("GET", f"/api/packs/{slug}")
ok(r["users"] == 2, f"two members took it: {r['users']}")
st, r, _ = mx.call("GET", f"/api/packs/{slug}/download?v=1.0")
ok(st == 200 and zipfile.ZipFile(io.BytesIO(r)).read("assets/minecraft/textures/block/stone.png") == png(32, 32), "the old version still downloads")
# The client's way: a token, no cookie, no Origin.
st, t, _ = Member("x").call("POST", "/api/token", {"login": "Maks", "password": "0123456789"}, origin=None)
st, r, _ = Member("x").call("GET", "/api/market", origin=None, token=t["token"])
ok(st == 200 and r["total"] >= 1, "the client (a token) sees the market")
# Moderation.
st, r, _ = outsider.call("POST", f"/api/packs/{slug}/hide", {"why": "x"})
ok(st == 403, "only she moderates")
st, r, _ = lu.call("POST", f"/api/packs/{slug}/hide", {"why": "Stolen textures (report #3)"})
st2, r2, _ = outsider.call("GET", f"/api/packs/{slug}")
st3, r3, _ = mx.call("GET", f"/api/packs/{slug}")
ok(st == 200 and st2 == 404 and r3["hidden_why"].startswith("Stolen"), "hidden: gone for others, its author sees why")
st, r, _ = mx.call("PATCH", f"/api/packs/{slug}", {"state": "published"})
ok(st == 403, "its author can't publish it back")
lu.call("POST", f"/api/packs/{slug}/unhide")
st, r, _ = outsider.call("GET", f"/api/packs/{slug}")
ok(st == 200, "back after she unhides it")
st, r, _ = outsider.call("GET", "/api/market?res=64x")
ok(any(i["slug"] == slug for i in r["items"]) and all(i["tags"].get("resolution") == "64x" for i in r["items"]), "among 64x, and only 64x there")
st, r, _ = outsider.call("GET", "/api/market?res=16x")
ok(not any(i["slug"] == slug for i in r["items"]) and r["total"] > 0, f"not among 16x ({r['total']} there)")
st, r, _ = outsider.call("GET", "/api/market?author=AUTHOR")
ok(r["total"] >= 1 and all(i["author"]["login"] == "Author" for i in r["items"]), "by its author")
st, r, _ = outsider.call("GET", "/api/market?fits=1")
ok(r["total"] >= 1 and all(i["tags"]["game"]["fits"] is True for i in r["items"]), f"only packs for 1.21.11: {r['total']}")
st, r, _ = outsider.call("GET", "/api/market")
ok(r["items"][0]["slug"] == slug, "trending first: the one members took this week")
st, r, _ = outsider.call("GET", "/api/market?state=hidden")
ok(st == 403, "only she sees what moderation hid")
lu.call("POST", f"/api/packs/{slug}/hide", {"why": "check"})
st, r, _ = lu.call("GET", "/api/market?state=hidden")
ok(st == 200 and [i["slug"] for i in r["items"]] == [slug], "her list of hidden packs")
lu.call("POST", f"/api/packs/{slug}/unhide")
st, r, _ = mx.call("GET", "/api/me/packs")
ok(st == 200 and any(i["slug"] == slug for i in r["items"]), "my packs")
st, r, _ = outsider.call("GET", "/api/market?sort=users")
ok(st == 200 and r["items"][0]["users"] >= r["items"][-1]["users"], f"sorted by users, {r['total']} packs")
st, r, _ = mx.call("DELETE", f"/api/packs/{slug}")
st2, r2, _ = mx.call("GET", f"/api/packs/{slug}")
ok(st == 200 and st2 == 404, "removed")

print()
print("FAILURES:" if failures else "everything as it should be", *failures, sep="\n  ")
sys.exit(1 if failures else 0)
