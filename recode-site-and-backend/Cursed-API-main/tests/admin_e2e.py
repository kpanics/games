"""admin_e2e.py: the staff's panel on the staging instance (run on the server after a reset): the summary, crashes
grouped by cause, members found and opened, notes, bans, a computer freed, a UID changed (hers only), registration,
the pinned partners, builds, roles, the journal, and announcements — to the site, the forum, everyone's notes, and
for the bot (its key) to take to Discord and Telegram.
"""
import http.cookiejar
import json
import os
import threading
import time
import urllib.error
import urllib.request

BASE = "http://127.0.0.1:8088"
CLI = "/root/staging.sh cli "
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
        self.key = None

    def call(self, method, path, body=None, raw=None, text=False, headers=None):
        h = {"Origin": BASE}
        h.update(headers or {})
        data = None
        if raw is not None:
            data, h["Content-Type"] = raw, "application/octet-stream"
        elif body is not None:
            data, h["Content-Type"] = json.dumps(body).encode(), "application/json"
        req = urllib.request.Request(BASE + path, data=data, method=method, headers=h)
        try:
            with self.op.open(req, timeout=60) as r:
                b = r.read()
                if text:
                    return r.status, b
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

    def token(self):
        for c in self.op.handlers:
            if isinstance(c, urllib.request.HTTPCookieProcessor):
                for k in c.cookiejar:
                    if k.name == "cursed_session":
                        return k.value
        return None

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


os.system(CLI + "registration open --limit 1000 >/dev/null")
her = Member("Lunuca").register(os.popen(CLI + "key").read().strip())
os.system(CLI + "renumber 1 0 >/dev/null")
a, m, s, z = Member("Anna").register(), Member("Mila").register(), Member("Sam").register(), Member("Zed").register()
os.system(CLI + "role Mila moderator >/dev/null")
os.system(CLI + "role Sam support >/dev/null")
bot_key = os.popen(CLI + "service-key").read().strip()
for x in (a, z):
    x.listen()

print("== the summary")
st, j = her.call("GET", "/api/admin/summary")
ok(st == 200 and j["accounts"] == 5 and j["new_day"] == 5 and j["online"]["site"] >= 2, f"accounts, new today, online ({j.get('online')})")
ok("tickets" in j and "crashes_day" in j and j["registration"] == "open", "tickets, crashes, registration")
st, j = a.call("GET", "/api/admin/summary")
ok(st == 403, "a member sees no panel")
st, j = s.call("GET", "/api/admin/summary")
ok(st == 200, "support sees the summary")

print("== crashes by cause")
native = "Cursed 0.1.0-beta (build 7)\nexit: 0\n===== java runtime =====\n#  EXCEPTION_ACCESS_VIOLATION (0xc0000005) at pc=1\n# Problematic frame:\n# C  [atio6axx.dll+0x1234]\n[x] Renderer: Radeon RX 580 Series\n"
for x, body in ((a, native), (z, native), (a, native.replace("atio6axx", "ig12icd64").replace("Radeon RX 580 Series", "Intel(R) UHD Graphics"))):
    st, j = x.call("POST", "/api/crash?build=7&source=game", raw=body.encode(), headers={"Authorization": "Bearer " + x.token()})
    ok(st == 200, "a crash sent by the loader")
st, j = m.call("GET", "/api/admin/crashes")
groups = j.get("groups", [])
ok(st == 200 and groups and groups[0]["cause"] == "EXCEPTION_ACCESS_VIOLATION in atio6axx.dll" and groups[0]["count"] == 2 and groups[0]["people"] == 2, "the AMD driver's crash first: twice, two people")
ok(groups and groups[0]["gpus"] == ["Radeon RX 580 Series"] and groups[0]["builds"] == [7], "on which card and build")
st, body = m.call("GET", f"/api/admin/crashes/{groups[0]['crashes'][0]}", text=True)
ok(st == 200 and b"atio6axx" in body, "its report readable")
st, j = s.call("GET", "/api/admin/crashes")
ok(st == 200, "support reads crashes (users.view)")

print("== members")
st, j = m.call("GET", "/api/admin/users?q=ann")
ok([u["login"] for u in j["items"]] == ["Anna"] and j["items"][0]["card"]["login"] == "Anna", "found by a piece of the login")
st, j = m.call("GET", "/api/admin/users?q=%23" + str(j["items"][0]["uid"]))
ok([u["login"] for u in j["items"]] == ["Anna"], "and by #UID")
st, j = m.call("POST", "/api/admin/users/Anna/notes", {"text": "Часто вылетает, AMD."})
st, j = m.call("GET", "/api/admin/users/Anna")
ok(st == 200 and j["notes"][0]["text"] == "Часто вылетает, AMD." and len(j["crash_list"]) == 2 and j["email"] is None, "opened whole: notes, crashes (no e-mail for a moderator)")
st, j = her.call("GET", "/api/admin/users/Anna")
ok(j["email"] == "anna@e.st", "she sees the e-mail")
st, j = m.call("POST", "/api/admin/users/Zed/ban", {"reason": "читы", "length": "7d"})
ok(st == 403, "a moderator bans nobody (users.ban)")
st, j = her.call("POST", "/api/admin/users/Zed/ban", {"reason": "читы", "length": "7d"})
ok(st == 200, "she bans Zed for a week")
st, j = her.call("GET", "/api/admin/users/Zed")
ok(j["ban"] and j["ban"]["reason"] == "читы" and j["ban"]["until"] > time.time() + 6 * 86400, "the ban, its reason, its end")
ok(any(h["action"] == "user.ban" for h in j["history"]), "and in Zed's history")
st, j = her.call("DELETE", "/api/admin/users/Zed/ban")
ok(st == 200 and j["lifted"] == 1, "lifted")
st, j = s.call("POST", "/api/admin/users/Anna/device")
ok(st == 200, "support frees a computer (users.device)")
st, j = m.call("POST", "/api/admin/users/Zed/uid", {"to": 500})
ok(st == 403, "a UID is changed by her only")
st, j = her.call("POST", "/api/admin/users/Zed/uid", {"to": 500})
ok(st == 200 and j["to"] == 500, "Zed is UID 500 now")
st, j = her.call("GET", "/api/u/Zed")
ok(j["person"]["uid"] == 500, "and shows as 500 at once (no restart)")
st, j = her.call("POST", "/api/admin/users/Anna/uid", {"to": 500})
ok(st == 409, "a UID taken is refused")

print("== her switches")
st, j = s.call("POST", "/api/admin/switch/registration", {"state": "closed"})
ok(st == 403, "registration is hers")
st, j = her.call("POST", "/api/admin/switch/registration", {"state": "closed", "limit": 20})
ok(st == 200 and j["state"] == "closed" and j["limit"] == 20, "closed, limit 20")
st, j = her.call("POST", "/api/admin/switch/registration", {"state": "open"})
st, j = her.call("POST", "/api/admin/pinned", {"address": "mc.example.net", "name": "Example"})
st, j = m.call("GET", "/api/admin/pinned")
ok(any(p["address"] == "mc.example.net" for p in j["items"]), "a partner pinned")
st, j = her.call("DELETE", "/api/admin/pinned/mc.example.net")
st, j = m.call("GET", "/api/admin/pinned")
ok(all(p["address"] != "mc.example.net" or p.get("gone") for p in j["items"]), "and unpinned")
st, j = m.call("GET", "/api/admin/builds")
ok(st == 200 and isinstance(j["items"], list), "the builds listed")
st, j = m.call("POST", "/api/admin/builds/rollback", {"channel": "beta"})
ok(st == 403, "a rollback is hers")

print("== roles, the journal")
st, j = m.call("GET", "/api/admin/roles")
roles = {r["id"]: r for r in j["roles"]}
ok("tickets.handle" in roles["support"]["perms"] and [p["login"] for p in roles["support"]["people"]] == ["Sam"], "support: its rights, Sam")
ok(len(j["perms"]) >= 10, "every right listed")
st, j = her.call("GET", "/api/admin/journal")
actions = [x["action"] for x in j["items"]]
ok(all(x in actions for x in ("user.ban", "user.unban", "device.reset", "user.uid", "registration", "server.pin")), "the journal: what was done")
ok(any(x["action"] == "user.ban" and x["person"] and x["person"]["login"] == "Zed" for x in j["items"]), "a member's acts name them")
st, j = m.call("GET", "/api/admin/journal")
ok(st == 403, "a moderator has no journal (audit.view)")

print("== an announcement")
st, j = her.call("POST", "/api/admin/announcements", {"title": "Сборка 8", "body": "Новое: **форум 2.0** и поддержка.", "site": True, "forum": True, "game": True, "discord": True, "everyone": True, "telegram": True})
ok(st == 201, "she announces everywhere")
aid = j["id"]
ok(a.note(lambda n: n["kind"] == "system" and n["data"].get("announcement") == aid) is not None, "everyone hears it (the game shows it)")
st, j = a.call("GET", "/api/news")
ok(j["items"] and j["items"][0]["title"] == "Сборка 8" and j["items"][0]["targets"] is None, "on the site's news (no inner fields)")
st, j = a.call("GET", "/api/forum/threads?cat=announcements")
ok(j["items"] and j["items"][0]["title"] == "Сборка 8", "and in the forum's announcements")
st, j = a.call("POST", "/api/admin/announcements", {"title": "Хак", "body": "x", "site": True})
ok(st == 403, "only those who may announce")
st, j = a.call("GET", "/api/service/announcements")
ok(st == 401, "the bot's queue is for its key")
st, j = a.call("GET", "/api/service/announcements", headers={"X-Cursed-Key": bot_key})
ok(st == 200 and [x["id"] for x in j["items"]] == [aid] and j["items"][0]["targets"]["everyone"], "the bot sees it, with @everyone")
st, j = a.call("POST", f"/api/service/announcements/{aid}", {"discord": [{"guild": "1", "channel": "2", "message": "3"}], "telegram": True}, headers={"X-Cursed-Key": bot_key})
st, j = a.call("GET", "/api/service/announcements", headers={"X-Cursed-Key": bot_key})
ok(j["items"] == [], "posted: no more in its queue")
st, j = her.call("GET", "/api/admin/announcements")
ok(j["items"][0]["discord_at"] and j["items"][0]["telegram_at"] and j["items"][0]["discord"][0]["message"] == "3", "she sees where it went")

print()
print("ALL OK" if not failures else f"{len(failures)} FAILED: " + "; ".join(failures))
