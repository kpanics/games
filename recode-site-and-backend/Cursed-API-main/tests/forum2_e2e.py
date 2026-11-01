"""forum2_e2e.py: the forum's second floor on the staging instance (run on the server after a reset): bugs with their
statuses (confirmed: the badge; fixed: told when the next build is out), ideas voted for, a question's best answer,
reactions, edits kept, followers told, pictures put in a post, YouTube cards, @ autocompletion, warnings and mutes,
the journal.
"""
import http.cookiejar
import json
import os
import sqlite3
import struct
import threading
import time
import urllib.error
import urllib.request
import zlib

BASE = "http://127.0.0.1:8088"
CLI = "/root/staging.sh cli "
DB = "/var/lib/cursed-staging/cursed.db"
failures = []


def ok(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        failures.append(what)


class Member:
    def __init__(self, login):
        self.login = login
        self.op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
        self.events = []

    def call(self, method, path, body=None, raw=None):
        # The forum's pace (a post every 8 seconds, a thread every 30): waited out rather than tested here.
        for _ in range(5):
            st, j = self.once(method, path, body, raw)
            if st != 429 or not isinstance(j, dict) or j.get("error") != "slow":
                return st, j
            time.sleep(9)
        return st, j

    def once(self, method, path, body=None, raw=None):
        h = {"Origin": BASE}
        data = None
        if raw is not None:
            data, h["Content-Type"] = raw, "application/octet-stream"
        elif body is not None:
            data, h["Content-Type"] = json.dumps(body).encode(), "application/json"
        req = urllib.request.Request(BASE + path, data=data, method=method, headers=h)
        try:
            with self.op.open(req, timeout=60) as r:
                b = r.read()
                return r.status, (json.loads(b) if b else {})
        except urllib.error.HTTPError as e:
            b = e.read()
            try:
                return e.code, json.loads(b)
            except Exception:
                return e.code, b

    def register(self, key=""):
        st, j = self.call("POST", "/api/register", {"login": self.login, "email": self.login.lower() + "@e.st", "password": "0123456789", "key": key})
        assert st == 200, (st, j)
        return self

    def listen(self):
        resp = self.op.open(urllib.request.Request(BASE + "/api/live"), timeout=300)

        def read():
            try:
                for line in resp:
                    line = line.decode().strip()
                    if line.startswith("data:"):
                        try:
                            self.events.append(json.loads(line[5:].strip()))
                        except ValueError:
                            pass
            except Exception:
                pass

        threading.Thread(target=read, daemon=True).start()
        time.sleep(0.3)

    def note(self, cond, secs=5):
        end = time.time() + secs
        while time.time() < end:
            for e in list(self.events):
                if e.get("t") == "note" and cond(e["n"]):
                    return e["n"]
            time.sleep(0.05)
        return None

    def badges(self):
        return [b["id"] for b in self.call("GET", f"/api/u/{self.login}")[1]["badges"]]


def png(w, h):
    raw = b"".join(b"\x00" + bytes(v for x in range(w) for v in ((x * 7) % 256, (y * 5) % 256, 120)) for y in range(h))

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


os.system(CLI + "registration open --limit 1000 >/dev/null")
her = Member("Lunuca").register(os.popen(CLI + "key").read().strip())
os.system(CLI + "renumber 1 0 >/dev/null")
a, b, m, z = Member("Anna").register(), Member("Boris").register(), Member("Mila").register(), Member("Zed").register()
os.system(CLI + "role Mila moderator >/dev/null")
for x in (a, b, m, z):
    x.listen()

print("== categories")
st, j = a.call("GET", "/api/forum")
ids = [c["id"] for c in j["categories"]]
ok(ids[:4] == ["announcements", "bugs", "ideas", "help"], f"bugs and ideas next to help ({ids})")

print("== a bug")
st, j = a.call("POST", "/api/forum/threads", {"category": "bugs", "title": "Погода мигает на AMD", "body": "Шаги: включить дождь.\n**жирный** и ||спойлер||"})
ok(st == 201, "Anna reports a bug")
bug = j["id"]
st, j = a.call("GET", f"/api/forum/threads/{bug}")
ok(j["status"] == "new" and j["following"] is True, "it is new, and its author follows it")
st, j = b.call("POST", f"/api/forum/threads/{bug}/status", {"status": "confirmed"})
ok(st == 403, "a member sets no status")
st, j = m.call("POST", f"/api/forum/threads/{bug}/status", {"status": "solved"})
ok(st == 400, "only a bug's statuses")
st, j = m.call("POST", f"/api/forum/threads/{bug}/status", {"status": "confirmed"})
ok(st == 200, "the moderator confirms it")
ok(a.note(lambda n: n["kind"] == "forum" and n["data"].get("status") == "confirmed") is not None, "Anna hears it is confirmed")
ok(a.note(lambda n: n["kind"] == "badge" and n["data"]["id"] == "bug_found") is not None and "bug_found" in a.badges(), "and gets the badge of a bug found")
st, j = m.call("POST", f"/api/forum/threads/{bug}/status", {"status": "fixed"})
st, j = a.call("GET", f"/api/forum/threads/{bug}")
ok(j["status"] == "fixed" and j["fix_pending"] is True and j["fixed_in"] is None, "fixed: waits for the next build")
st, j = a.call("GET", "/api/forum/threads?cat=bugs&status=fixed")
ok([t["id"] for t in j["items"]] == [bug] and j["items"][0]["status"] == "fixed", "the list filters by status")
# A build published (by the command line, as `cursed dist publish` would): the server sees it within half a minute.
c = sqlite3.connect(DB)
n = c.execute("SELECT COALESCE(MAX(build), 0) + 1 FROM releases").fetchone()[0]
c.execute("INSERT INTO releases (channel, build, version, notes, manifest, sig, at, by) VALUES ('beta', ?, '0.1.0-beta.9', 'x', '{}', '', 0, 'test')", (n,))
c.commit()
c.close()
told = a.note(lambda n_: n_["kind"] == "forum" and n_["data"].get("status") == "released", secs=40)
ok(told is not None and told["data"]["build"] == n, f"Anna hears it is fixed in build {n}")
st, j = a.call("GET", f"/api/forum/threads/{bug}")
ok(j["fixed_in"] == n and j["fix_pending"] is False, "the thread says which build")
ok("bug_hunter" in a.badges(), "and she is a bug hunter")

print("== an idea")
st, j = b.call("POST", "/api/forum/threads", {"category": "ideas", "title": "Мини-карта в стиле клиента", "body": "Круглая, с метками."})
idea = j["id"]
st, j2 = her.call("POST", "/api/forum/threads", {"category": "ideas", "title": "Темы для чата", "body": "Свои цвета."})
idea2 = j2["id"]
st, j = a.call("POST", f"/api/forum/threads/{idea}/vote")
ok(st == 200 and j["votes"] == 1 and j["voted"], "Anna votes for it")
st, j = z.call("POST", f"/api/forum/threads/{idea}/vote")
st, j = z.call("POST", f"/api/forum/threads/{idea}/vote")
ok(j["votes"] == 1 and not j["voted"], "Zed's vote taken back")
st, j = m.call("POST", f"/api/forum/threads/{idea}/vote")
st, j = a.call("GET", "/api/forum/threads?cat=ideas&sort=top")
ok([t["id"] for t in j["items"]][:2] == [idea, idea2] and j["items"][0]["votes"] == 2, "sorted by votes")
st, j = a.call("POST", f"/api/forum/threads/{bug}/vote")
ok(st == 400, "a bug is not voted for")
st, j = m.call("POST", f"/api/forum/threads/{idea}/status", {"status": "done"})
ok("idea" in b.badges(), "done: Boris is an idea maker")

print("== a question")
st, j = z.call("POST", "/api/forum/threads", {"category": "help", "title": "Как включить Fog?", "body": "Где он?"})
q = j["id"]
st, j = b.call("POST", f"/api/forum/threads/{q}/posts", {"body": "Ambience → Fog."})
answer = j["id"]
ok(z.note(lambda n_: n_["kind"] == "reply" and n_["data"]["thread"] == q) is not None, "Zed hears the reply")
st, j = a.call("POST", f"/api/forum/threads/{q}/solution", {"post": answer})
ok(st == 403, "not Anna's question")
st, j = z.call("POST", f"/api/forum/threads/{q}/solution", {"post": answer})
st, j = z.call("GET", f"/api/forum/threads/{q}")
ok(j["solution"] == answer and j["status"] == "solved", "Zed marks the answer: solved")
ok(b.note(lambda n_: n_["kind"] == "forum" and n_["data"].get("status") == "answer") is not None and "expert" in b.badges(), "Boris hears it and is an expert")
st, j = a.call("GET", "/api/forum/threads?cat=help&status=unsolved")
ok(q not in [t["id"] for t in j["items"]], "not among the unsolved")

print("== followers")
st, j = a.call("POST", f"/api/forum/threads/{q}/posts", {"body": "Ещё можно клавишей."})
ok(b.note(lambda n_: n_["kind"] == "reply" and n_["data"]["thread"] == q and n_["actor"]["login"] == "Anna") is not None, "Boris replied, so he follows: hears Anna")
st, j = b.call("POST", f"/api/forum/threads/{q}/follow", {"on": False})
b.events.clear()
st, j = z.call("POST", f"/api/forum/threads/{q}/posts", {"body": "Спасибо!"})
time.sleep(1)
ok(not any(e.get("t") == "note" and e["n"]["kind"] == "reply" for e in b.events), "stopped following: hears nothing")
st, j = b.call("POST", f"/api/forum/threads/{q}/posts", {"body": "Пожалуйста."})
st, j = b.call("GET", f"/api/forum/threads/{q}")
ok(j["following"] is False, "replying again does not follow it again")

print("== reactions and edits")
st, j = b.call("POST", f"/api/forum/posts/{answer}/react", {"emoji": "fire"})
ok(st == 400, "only the pack's emoji")
emoji = "c_heart"
st, j = a.call("POST", f"/api/forum/posts/{answer}/react", {"emoji": emoji})
ok(st == 200 and j["reactions"][0]["uids"], f"Anna reacts with {emoji}")
st, j = z.call("POST", f"/api/forum/posts/{answer}/react", {"emoji": emoji})
st, j = z.call("GET", f"/api/forum/threads/{q}")
post = next(p for p in j["posts"] if p["id"] == answer)
ok(post["reactions"][0]["emoji"] == emoji and len(post["reactions"][0]["uids"]) == 2, "two of them, shown on the post")
st, j = b.call("PATCH", f"/api/forum/posts/{answer}", {"body": "Ambience → Fog (в меню)."})
st, j = b.call("PATCH", f"/api/forum/posts/{answer}", {"body": "Ambience → Fog (в меню)."})
st, j = b.call("PATCH", f"/api/forum/posts/{answer}", {"body": "Ambience → Fog, правой кнопкой."})
st, j = b.call("GET", f"/api/forum/posts/{answer}/history")
ok([v["body"] for v in j["versions"]] == ["Ambience → Fog (в меню).", "Ambience → Fog."], "two edits kept (the same text again is no edit)")
st, j = z.call("GET", f"/api/forum/posts/{answer}/history")
ok(st == 403, "only its author and staff read them")
st, j = m.call("GET", f"/api/forum/posts/{answer}/history")
ok(st == 200, "staff do")

print("== pictures and cards")
st, j = a.call("POST", "/api/forum/pictures", raw=png(200, 120))
ok(st == 201 and len(j["picture"]["id"]) == 16, "a picture uploaded")
pic = j["picture"]["id"]
st, j = a.call("POST", "/api/forum/pictures", raw=b"not a picture")
ok(st == 400, "not a picture: refused")
body = f"Скрин:\n[img:{pic}]\nи видео https://www.youtube.com/watch?v=dQw4w9WgXcQ"
st, j = a.call("POST", "/api/forum/threads", {"category": "showcase", "title": "Мой скрин", "body": body})
shot = j["id"]
st, j = a.call("POST", "/api/forum/threads", {"category": "showcase", "title": "Много", "body": "\n".join(f"[img:{pic}]" for _ in range(11))})
ok(st == 400, "eleven pictures: too many")
card = None
for _ in range(30):
    st, j = b.call("GET", f"/api/forum/threads/{shot}")
    card = j["posts"][0]["cards"].get("dQw4w9WgXcQ")
    if card:
        break
    time.sleep(0.5)
ok(j["posts"][0]["pictures"].get(pic, {}).get("w") == 200, "the picture is shown with the post")
ok(card is not None and card["title"] and card["thumb"] and card["thumb"].startswith("/"), f"the YouTube card: {card and card['title']}")

print("== mentions")
st, j = a.call("GET", "/api/people/find?q=bo")
ok([p["login"] for p in j["people"]] == ["Boris"], "@bo finds Boris")
st, j = a.call("GET", "/api/people/find?q=%25")
ok(j["people"] == [], "nothing for odd characters")

print("== warnings and mutes")
st, j = b.call("POST", "/api/staff/warn", {"login": "Zed", "why": "флуд"})
ok(st == 403, "a member warns nobody")
st, j = m.call("POST", "/api/staff/warn", {"login": "Zed", "why": "флуд в теме"})
ok(st == 200 and j["warnings"] == 1, "the moderator warns Zed")
ok(z.note(lambda n_: n_["kind"] == "moderation" and n_["data"].get("what") == "warning") is not None, "Zed is told")
st, j = m.call("POST", "/api/staff/mute", {"login": "Zed", "minutes": 30, "why": "флуд"})
ok(st == 200 and j["until"] > time.time(), "and mutes him for half an hour")
st, j = z.call("POST", f"/api/forum/threads/{q}/posts", {"body": "А можно?"})
ok(st == 403 and j["error"] == "muted" and j["until"] > 0, "a reply refused while muted")
st, j = z.call("POST", "/api/forum/threads", {"category": "help", "title": "Новая тема", "body": "текст"})
ok(st == 403 and j["error"] == "muted", "a new thread too")
st, j = m.call("POST", "/api/staff/mute", {"login": "Lunuca", "minutes": 30, "why": "x"})
ok(st == 400, "staff are not muted")
st, j = m.call("GET", "/api/staff/record/Zed")
ok(len(j["warnings"]) == 1 and j["mute"]["why"] == "флуд", "his record: the warning and the mute")
st, j = m.call("DELETE", "/api/staff/mute/Zed")
st, j = z.call("POST", f"/api/forum/threads/{q}/posts", {"body": "Понял."})
ok(st == 201, "unmuted: he posts again")
c = sqlite3.connect(DB)
actions = [r[0] for r in c.execute("SELECT action FROM audit ORDER BY id")]
c.close()
ok(all(x in actions for x in ("forum.status", "forum.warn", "forum.mute", "forum.unmute")), "all of it in the journal")

print()
print("ALL OK" if not failures else f"{len(failures)} FAILED: " + "; ".join(failures))
