"""social_e2e.py: people on the staging instance (run on the server after a reset): roles and their prefixes in
every list, the profile and its parts (about, status, accent, backdrop, links, privacy, banner, music, screenshots),
the badges, the sessions, the live line and the notifications that run down it (a reply, a mention, moderation),
and what a moderator may do now that it is not hers alone.
"""
import http.cookiejar
import json
import os
import struct
import sys
import threading
import time
import urllib.error
import urllib.request
import zlib

BASE = os.environ.get("BASE", "http://127.0.0.1:8088")
CLI = os.environ.get("CLI", "/root/staging.sh cli ")
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

    def call(self, method, path, body=None, raw=None, origin=BASE):
        h = {"Origin": origin} if origin else {}
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
        """The live line, read on its own thread: every event kept in self.events."""
        self.events = []
        req = urllib.request.Request(BASE + "/api/live")
        resp = self.op.open(req, timeout=120)

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
        return resp

    def wait(self, t, cond=lambda e: True, secs=5):
        end = time.time() + secs
        while time.time() < end:
            for e in self.events:
                if e.get("t") == t and cond(e):
                    return e
            time.sleep(0.05)
        return None


def png(w, h):
    rows = []
    for y in range(h):
        row = bytearray([0])
        for x in range(w):
            row += bytes((x % 256, y % 256, 90))
        rows.append(bytes(row))
    raw = b"".join(rows)

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return bytes([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def mp3(frames=120):
    tag = b"TIT2" + struct.pack(">I", 10) + b"\x00\x00" + b"\x03Night Run" + b"TPE1" + struct.pack(">I", 7) + b"\x00\x00" + b"\x03Wraith"
    n = len(tag)
    head = b"ID3\x03\x00\x00" + bytes([(n >> 21) & 0x7F, (n >> 14) & 0x7F, (n >> 7) & 0x7F, n & 0x7F])
    frame = b"\xFF\xFB\x90\x64" + b"\x00" * 413
    return head + tag + frame * frames + b"TAG" + b"\x00" * 125


os.system(CLI + "registration open --limit 1000 >/dev/null")
her = Member("Lunuca").register(os.popen(CLI + "key").read().strip())
os.system(CLI + "renumber 1 0 >/dev/null")
her = Member("Lunuca")
her.call("POST", "/api/login", {"login": "Lunuca", "password": "0123456789"})
a, b, m = Member("Anna").register(), Member("Boris").register(), Member("Mila").register()

print("== cards and prefixes")
st, me = her.call("GET", "/api/me")
ok(st == 200 and me["card"]["prefix"]["role"] == "creator" and me["staff"] and "forum.moderate" in me["perms"], "she is the creator, staff, with every right")
st, me = a.call("GET", "/api/me")
ok(st == 200 and me["card"]["prefix"] is None and not me["staff"] and me["perms"] == [], "a member: no prefix, no rights")
st, r = a.call("POST", "/api/staff/roles", {"login": "Mila", "role": "moderator", "on": True})
ok(st == 403, "a member gives no roles")
st, r = her.call("POST", "/api/staff/roles", {"login": "Mila", "role": "moderator", "on": True})
ok(st == 403, "even the owner cannot assign roles over HTTP")
assert os.system(CLI + "role Mila moderator >/dev/null") == 0
os.system(CLI + "role Boris media >/dev/null")
st, me = m.call("GET", "/api/me")
ok(me["card"]["prefix"]["text"] == "MOD" and "forum.moderate" in me["perms"] and "users.roles" not in me["perms"], "Mila: MOD, may moderate, may not give roles")
st, r = m.call("POST", "/api/staff/roles", {"login": "Anna", "role": "beta", "on": True})
ok(st == 403, "a moderator gives no roles")
st, p = a.call("GET", "/api/u/Boris")
ok(st == 200 and p["person"]["prefix"]["text"] == "MEDIA" and any(x["id"] == "media" for x in p["badges"]), "Boris shows MEDIA, and the media badge came with it")
st, ppl = a.call("GET", f"/api/people?ids={p['person']['uid']},0,999999")
ok(st == 200 and len(ppl["people"]) == 2, "cards of many at once (the unknown left out)")

print("== the profile")
st, r = a.call("PATCH", "/api/me/profile", {"about": "Hi.\nI build.", "status_text": "  looking   for a team  ", "status_emoji": "c_team", "accent": "#6669f5", "theme": "aurora",
                                            "socials": {"telegram": "@anna_tg", "youtube": "https://www.youtube.com/@AnnaPlays", "github": "", "site": "https://anna.example"},
                                            "privacy": {"server": True, "dm": "all"}})
ok(st == 200, "about, status, accent, backdrop, links, privacy at once")
st, p = a.call("GET", "/api/u/anna")
ok(p["about"] == "Hi.\nI build." and p["person"]["status"] == {"text": "looking for a team", "emoji": "c_team"} and p["person"]["accent"] == "#6669F5" and p["theme"] == "aurora", "kept and shown")
ok(p["socials"] == {"telegram": "anna_tg", "youtube": "@AnnaPlays", "site": "https://anna.example"}, "links made from what was typed, the empty one left out")
ok(p["privacy"]["server"] and p["privacy"]["dm"] == "all" and p["privacy"]["online"], "her own privacy, the rest at its defaults")
st, q = b.call("GET", "/api/u/Anna")
ok("privacy" not in q and not q["viewer"]["self"], "another sees no privacy settings")
for bad, field in [({"accent": "red"}, "accent"), ({"theme": "pink"}, "theme"), ({"status_emoji": "x"}, "emoji"), ({"socials": {"site": "javascript:alert(1)"}}, "social"),
                   ({"socials": {"myspace": "x"}}, "social"), ({"prefix": "admin"}, "prefix"), ({"showcase": ["founder"]}, "showcase")]:
    st, r = a.call("PATCH", "/api/me/profile", bad)
    ok(st == 400 and r.get("error") == field, f"refused: {bad}")
st, r = a.call("PATCH", "/api/me/profile", {"showcase": ["first100", "beta"]})
ok(st == 200 and a.call("GET", "/api/u/Anna")[1]["showcase"] == ["first100", "beta"], "a showcase of her own badges")
st, r = Member("Ghost").call("GET", "/api/u/Anna")
ok(st == 401, "profiles are for members only")
ok(a.call("GET", "/api/u/nobody_here")[0] == 404, "no such member")

print("== banner, music, screenshots")
st, r = a.call("POST", "/api/me/banner", raw=png(900, 400))
ok(st == 200 and r["banner"]["w"] == 1500 and r["banner"]["h"] == 500 and r["banner"]["url"].startswith("/media/"), "a banner, cut to 3:1")
st, r = a.call("POST", "/api/me/banner", raw=b"not a picture")
ok(st == 400, "a banner that is no picture")
st, r = a.call("POST", "/api/me/music", raw=mp3())
ok(st == 200 and r["music"]["kind"] == "file" and r["music"]["title"] == "Night Run" and r["music"]["artist"] == "Wraith" and r["music"]["ms"] > 3000, "a song, named by its tags")
st, r = a.call("PATCH", "/api/me/music", {"title": "Ночной бег"})
ok(st == 200 and r["music"]["title"] == "Ночной бег", "renamed")
st, r = a.call("PATCH", "/api/me/music", {"soundcloud": "https://soundcloud.com/wraith/night-run?si=x"})
ok(st == 200 and r["music"] == {"kind": "soundcloud", "url": "https://soundcloud.com/wraith/night-run", "title": "night run", "artist": "wraith"}, "a SoundCloud track instead")
st, r = a.call("PATCH", "/api/me/music", {"soundcloud": "https://soundcloud.com/wraith"})
ok(st == 400, "an artist is not a track")
st, s1 = a.call("POST", "/api/me/shots", raw=png(640, 360))
st2, s2 = a.call("POST", "/api/me/shots", raw=png(320, 180))
ok(st == 201 and st2 == 201 and s1["shot"]["thumb"], "two screenshots")
st, sh = b.call("GET", "/api/u/Anna/shots")
ok(st == 200 and [x["id"] for x in sh["shots"]] == [s2["shot"]["id"], s1["shot"]["id"]], "the gallery, newest first")
ok(b.call("DELETE", f"/api/me/shots/{s1['shot']['id']}")[0] == 404, "another's screenshot is not hers to take")
ok(a.call("DELETE", f"/api/me/shots/{s1['shot']['id']}")[0] == 200 and len(b.call("GET", "/api/u/Anna/shots")[1]["shots"]) == 1, "her own taken away")
ok(a.call("DELETE", "/api/me/banner")[0] == 200 and a.call("GET", "/api/u/Anna")[1]["banner"] is None, "the banner taken away")

print("== badges")
st, cat = a.call("GET", "/api/badges")
by = {x["id"]: x for x in cat["badges"]}
ok(st == 200 and by["founder"]["holders"] == 1 and by["first100"]["holders"] == 4 and by["beta"]["holders"] == 4, "founder hers alone; the first hundred and the beta testers: the four of them, at their registration")
ok(by["owl"]["en"] == "???" and by["owl"]["icon"] == "c_lock", "a secret says nothing of itself")
st, r = a.call("POST", "/api/me/found", {"what": "eclipse"})
ok(st == 200 and any(x["id"] == "eclipse" for x in a.call("GET", "/api/u/Anna")[1]["badges"]), "the stargazer, found")
ok(a.call("GET", "/api/badges")[1]["badges"][[x["id"] for x in cat["badges"]].index("eclipse")]["en"] == "Stargazer", "and then its name is shown to her")
st, r = m.call("POST", "/api/staff/badges", {"login": "Anna", "badge": "bug_critical", "on": True})
ok(st == 403, "a moderator gives no badges")
st, r = her.call("POST", "/api/staff/badges", {"login": "Anna", "badge": "bug_critical", "on": True})
ok(st == 200, "she gives one")

print("== the live line and the notifications")
a.listen()
hello = a.wait("hello")
ok(hello is not None and hello["me"]["login"] == "Anna" and hello["unread"] >= 1, "hello: who, and how many unread (the badges so far)")
st, t = a.call("POST", "/api/forum/threads", {"category": "help", "title": "Where is the config", "body": "Help me find it"})
time.sleep(0.2)
st, r = b.call("POST", f"/api/forum/threads/{t['id']}/posts", {"body": "In the menu, @Mila knows more"})
note = a.wait("note", lambda e: e["n"]["kind"] == "reply")
ok(note is not None and note["n"]["actor"]["login"] == "Boris" and note["n"]["data"]["title"] == "Where is the config" and note["push"], "Anna hears of Boris's reply at once")
st, ns = m.call("GET", "/api/notifications")
ok(any(n["kind"] == "mention" and n["data"]["thread"] == t["id"] for n in ns["items"]), "Mila is told she was mentioned")
time.sleep(8.2)
b.call("POST", f"/api/forum/threads/{t['id']}/posts", {"body": "and one more thing"})
st, ns = a.call("GET", "/api/notifications")
ok(len([n for n in ns["items"] if n["kind"] == "reply" and n["subject"] == f"thread:{t['id']}"]) == 1, "two replies: one line, the newest")
st, r = a.call("PUT", "/api/me/notify", {"reply": {"push": False}})
ok(st == 200 and next(k for k in a.call("GET", "/api/me/notify")[1]["kinds"] if k["kind"] == "reply") == {"kind": "reply", "en": "Replies in threads you follow", "ru": "Ответы в темах, на которые вы подписаны", "site": True, "game": True, "push": False}, "no Windows notifications of replies, the rest as it was")
st, r = a.call("PUT", "/api/me/notify", {"nope": {"push": False}})
ok(st == 400, "no such kind")
st, r = a.call("POST", "/api/notifications/read", {"all": True})
ok(st == 200 and r["unread"] == 0 and a.wait("unread", lambda e: e["unread"] == 0), "all read, and the page hears it")

print("== a moderator, not only her")
st, tt = a.call("GET", f"/api/forum/threads/{t['id']}")
target = tt["posts"][1]["id"]
st, r = m.call("POST", f"/api/forum/posts/{target}/hide", {"why": "no spam"})
ok(st == 200, "Mila hides a post")
st, ns = b.call("GET", "/api/notifications")
ok(any(n["kind"] == "moderation" and n["data"]["why"] == "no spam" for n in ns["items"]), "its author is told why")
st, r = a.call("POST", f"/api/forum/posts/{target}/unhide")
ok(st == 403, "a member moderates nothing")
st, r = b.call("POST", "/api/reports", {"kind": "thread", "target": t["id"], "why": "wrong place"})
st, rs = m.call("GET", "/api/reports")
ok(st == 200, "a moderator sees the reports")

print("== sessions")
a2 = Member("Anna")
a2.call("POST", "/api/login", {"login": "Anna", "password": "0123456789"})
st, ss = a.call("GET", "/api/me/sessions")
ok(st == 200 and len(ss["sessions"]) == 2 and sum(s["current"] for s in ss["sessions"]) == 1, "two sessions, this one marked")
other = next(s for s in ss["sessions"] if not s["current"])
ok(a.call("DELETE", f"/api/me/sessions/{other['id']}")[0] == 200 and a2.call("GET", "/api/me")[0] == 401, "the other one ended: it is signed out")
a3 = Member("Anna")
a3.call("POST", "/api/login", {"login": "Anna", "password": "0123456789"})
st, r = a.call("POST", "/api/me/sessions/others")
ok(st == 200 and r["ended"] == 1 and a3.call("GET", "/api/me")[0] == 401 and a.call("GET", "/api/me")[0] == 200, "every other session ended, this one kept")

print("== manual roles are console-only")
d, e, p = Member("Dora").register(), Member("Egor").register(), Member("Petr").register()
for x in ("Dora", "Egor"):
    ok(her.call("POST", "/api/staff/roles", {"login": x, "role": "admin", "on": True})[0] == 403, "owner HTTP grant denied")
    assert os.system(CLI + "role " + x + " admin >/dev/null") == 0
ok(d.call("POST", "/api/staff/roles", {"login": "Petr", "role": "moderator", "on": True})[0] == 403, "admin HTTP grant denied")
ok(d.call("POST", "/api/staff/roles", {"login": "Petr", "role": "moderator", "on": False})[0] == 403, "admin HTTP revoke denied")
for login, role, on, what in (("Petr", "admin", True, "makes no admins"), ("Egor", "partner", True, "gives another admin no role"),
                              ("Egor", "admin", False, "takes no admin's role away"), ("Dora", "partner", True, "gives themselves no role"),
                              ("Lunuca", "moderator", True, "touches none of hers")):
    st, r = d.call("POST", "/api/staff/roles", {"login": login, "role": role, "on": on})
    ok(st == 403 and r.get("error") == "roles-console-only", "an admin " + what)
ok(her.call("POST", "/api/staff/roles", {"login": "Egor", "role": "partner", "on": True})[0] == 403, "owner HTTP grant denied")
ok(her.call("POST", "/api/staff/roles", {"login": "Egor", "role": "partner", "on": False})[0] == 403, "owner HTTP revoke denied")
st, j = her.call("GET", "/api/admin/users/Egor")
ok(st == 200 and "admin" in j["roles"] and "partner" not in j["roles"], "Egor is an admin still, and nothing else came of it")
st, j = her.call("GET", "/api/admin/users/Dora")
ok(st == 200 and "admin" in j["roles"] and "partner" not in j["roles"], "Dora gave herself nothing")

print()
print("FAILED:", len(failures)) if failures else print("all passed")
sys.exit(1 if failures else 0)
