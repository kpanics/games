"""seed_staging.py: a lived-in market on the staging instance, to build the site's pages against (run on the server).

Resets staging with the site's dev origin, registers her (UID 0) and a few authors, uploads every pack of /root/corpus
with its Modrinth title, description and gallery, and has members take some of them. Passwords are all 0123456789.
"""
import hashlib
import json
import os
import random
import re
import subprocess
import sys
import time

import http.cookiejar
import urllib.error
import urllib.request

BASE = "http://127.0.0.1:8088"
ORIGIN = os.environ.get("ORIGIN", "http://127.0.0.1:8793")


class Member:
    def __init__(self, login):
        self.login = login
        self.op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def call(self, method, path, body=None, raw=None):
        h = {"Origin": ORIGIN}
        data = None
        if body is not None:
            data, h["Content-Type"] = json.dumps(body).encode(), "application/json"
        if raw is not None:
            data, h["Content-Type"] = raw, "application/octet-stream"
        req = urllib.request.Request(BASE + path, data=data, method=method, headers=h)
        try:
            with self.op.open(req, timeout=600) as r:
                b = r.read()
                return r.status, (json.loads(b) if r.headers.get("Content-Type", "").startswith("application/json") and b else b), r.headers
        except urllib.error.HTTPError as e:
            b = e.read()
            try:
                return e.code, json.loads(b), e.headers
            except Exception:
                return e.code, b, e.headers

    def register(self, key=""):
        st, j, _ = self.call("POST", "/api/register", {"login": self.login, "email": self.login.lower() + "@e.st", "password": "0123456789", "key": key})
        assert st == 200, (st, j)
        return self

    def upload(self, data, name):
        st, j, _ = self.call("POST", "/api/uploads", {"name": name, "size": len(data), "sha256": hashlib.sha256(data).hexdigest()})
        uid, at = j["id"], 0
        while at < len(data):
            st, r, _ = self.call("PUT", f"/api/uploads/{uid}?at={at}", raw=data[at:at + j["chunk"]])
            at = r["received"]
        while True:
            st, r, _ = self.call("GET", f"/api/uploads/{uid}")
            if r["state"] in ("ready", "refused"):
                return uid, r, 0
            time.sleep(0.2)


class lib:
    Member = Member

subprocess.run("/root/staging.sh reset", shell=True)
subprocess.run(f"ORIGIN={ORIGIN} /root/staging.sh start", shell=True)
time.sleep(0.8)
subprocess.run("/root/staging.sh cli registration open", shell=True, capture_output=True)

key = subprocess.run("/root/staging.sh cli key", shell=True, capture_output=True, text=True).stdout.strip()
her = lib.Member("Lunuca").register(key)
subprocess.run("/root/staging.sh cli renumber 1 0", shell=True, capture_output=True)
authors = [lib.Member(n).register() for n in ("v3sper", "Fr0zy", "NeKiT", "kotik_228", "mirekk")]
takers = [lib.Member(f"player{i}").register() for i in range(12)]


def plain(md, most):
    text = re.sub(r"!\[[^\]]*\]\([^)]*\)|<[^>]+>|\[([^\]]*)\]\([^)]*\)|[#*_`>|~-]{1,}", lambda m: m.group(1) or " ", md)
    paras = [" ".join(p.split()) for p in text.split("\n\n")]
    paras = [p for p in paras if len(p) > 30][:3]
    return "\n\n".join(paras)[:most]


random.seed(7)
made = []
metas = sorted(f for f in os.listdir("/root/corpus/meta") if f.endswith(".json"))
for i, m in enumerate(metas):
    meta = json.load(open(f"/root/corpus/meta/{m}"))
    who = authors[i % len(authors)]
    data = open(f"/root/corpus/{meta['kind']}/{meta['file']}", "rb").read()
    uid, r, _ = who.upload(data, meta["file"].split("__", 1)[1])
    if r["state"] != "ready":
        print("refused", m, r.get("result"))
        continue
    st, page, _ = who.call("POST", "/api/packs", {
        "upload": uid, "title": meta["title"][:48], "blurb": meta["blurb"][:120], "about": plain(meta["body"], 1500),
        "number": random.choice(["1.0", "1.2", "2.0", "2.4.1", "3.1"]), "changelog": "First upload.",
        "visibility": "private" if i == 3 else "public", "publish": i != 5,
    })
    if st != 201:
        print("not made", m, st, page)
        continue
    for pic in meta["pictures"]:
        st, _, _ = who.call("POST", f"/api/packs/{page['slug']}/pictures", raw=open(pic, "rb").read())
    made.append((who, page["slug"]))
    print(f"{page['slug']:36} by {who.login:10} {len(meta['pictures'])} pictures")

for who, slug in made:
    for t in random.sample(takers, random.randint(0, len(takers))):
        t.call("GET", f"/api/packs/{slug}/download")
print("seeded", len(made), "packs")

# ---- the forum: threads in every category, replies with quotes and mentions, her announcement, a report.
members = authors + takers
threads = [
    ("announcements", her, "0.1.0 is out", "The first build of Cursed is here.\n\nWhat's new: the market, the forum, `Item Physic`, Target ESP styles.\nReport bugs in Help."),
    ("help", takers[0], "шейдер 20 фпс на 3060??", "Complementary Reimagined, 1.21.11, 2560×1440.\nВ меню 60, в мире 20. Что выключить первым?"),
    ("making", authors[1], "как сделать emissive руду?", "Хочу чтобы руда светилась как в Ashfall. Нужен OptiFine emissive?\n\nСейчас так: `stone_e.png` рядом с `stone.png`, но не светится."),
    ("market", authors[2], "нужен снежный пвп пак 16х", "Ищу зимний пак под 1.21.11: снег, синие мечи, чистый интерфейс. Есть такие в маркете?"),
    ("showcase", authors[3], "скрины с шейдерами кидайте сюда", "Кидайте свои скрины с шейдерами из маркета. Моё: Photon на закате, https://cursedclient.xyz/#/pack/photon-shaders"),
    ("help", takers[1], "try-on shows the old version of a pack", "I updated my pack to 1.3 but Try on still shows 1.2. Cache?"),
    ("offtopic", takers[2], "что слушаете пока играете", "Daniel Deluxe, Carpenter Brut. Ваше?"),
    ("making", authors[0], "CTM glass without OptiFine files", "Can connected glass work with just the vanilla model system? Asking for a pack that should work everywhere."),
]
replies = {
    1: ["Выключи volumetric light и облака, это самое тяжёлое.", "> Выключи volumetric light\nПомогло, стало 48. Спасибо @player3", "Ещё SSR в воде режет, попробуй профиль MEDIUM."],
    2: ["Нужен `emissive.properties` в `assets/minecraft/optifine/` с `suffix.emissive=_e`.", "Точно, заработало. Лучше бы это было в вики."],
    3: ["Глянь Crystal PvP, там есть снежная версия.", "Он не под 1.21.11 же?", "Под 1.21.11, сайт пишет «Built for 1.21.11»."],
    4: ["Complementary + ночь, ставлю на обои.", "Bliss на рассвете, лучше всех по воде."],
    5: ["Did you publish 1.3 or save it as a draft?", "Oh. Draft. Thanks!"],
    6: ["Perturbator и Gunship", "Hotline Miami OST на повторе"],
}
made_threads = []
for i, (cat, who, title, body) in enumerate(threads):
    st, r, _ = who.call("POST", "/api/forum/threads", {"category": cat, "title": title, "body": body})
    if st != 201:
        print("thread not made", title, st, r)
        continue
    made_threads.append(r["id"])
    for j, text in enumerate(replies.get(i, [])):
        m = members[(i * 3 + j * 5 + 2) % len(members)]
        for attempt in range(3):
            st, r2, _ = m.call("POST", f"/api/forum/threads/{r['id']}/posts", {"body": text})
            if st != 429:
                break
            time.sleep(8)
her.call("PATCH", f"/api/forum/threads/{made_threads[0]}", {"pinned": True})
takers[4].call("POST", "/api/reports", {"kind": "thread", "id": made_threads[6], "why": "не по теме маркета, спам"})
takers[5].call("POST", "/api/reports", {"kind": "pack", "id": 3, "why": "textures taken from another pack"})
print("forum seeded:", len(made_threads), "threads")
