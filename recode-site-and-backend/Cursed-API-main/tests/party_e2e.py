"""party_e2e.py: parties on a staging instance (run on the server after a reset): made, joined by its code, invited by
the nickname played under on the same server, a ping and a mark seen by all of it, a ping put again replacing the last,
kicked, left (the lead passed on), and the party gone when nobody is in it.
"""
import http.cookiejar
import json
import os
import time
import urllib.error
import urllib.request

BASE = "http://127.0.0.1:8088"
CLI = os.environ.get("CLI", "/root/staging.sh cli ")
failures = []


def ok(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        failures.append(what)


class Member:
    def __init__(self, login):
        self.login = login
        self.op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def call(self, method, path, body=None):
        h = {"Origin": BASE}
        data = None
        if body is not None:
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

    def look(self, mc, server="mc.funtime.su", world="mc.funtime.su|minecraft:overworld"):
        st, j = self.call("GET", f"/api/party?mc={mc}&server={server}&world={world}")
        assert st == 200, (st, j)
        return j


os.system(CLI + "registration open --limit 1000 >/dev/null")
a, b, c, d = Member("Anna").register(os.popen(CLI + "key").read().strip()), Member("Boris").register(), Member("Chris").register(), Member("Dana").register()
a.look("AnnaMC"), b.look("BorisPvP"), c.look("Chris_x"), d.look("DanaD", server="play.other.net")

print("== made and joined")
j = a.look("AnnaMC")
ok(j["party"] is None and j["invites"] == [], "nobody's in a party at first")
st, j = a.call("POST", "/api/party")
ok(st == 201 and len(j["code"]) == 6, f"Anna makes one ({j.get('code')})")
code = j["code"]
st, j = b.call("POST", "/api/party/join", {"code": code.lower()})
ok(st == 200, "Boris joins by its code, typed small")
j = a.look("AnnaMC")
ok([m["login"] for m in j["party"]["members"]] == ["Anna", "Boris"] and j["party"]["leader"], "two in it, Anna leads")
ok(j["party"]["members"][1]["mc"] == "BorisPvP" and j["party"]["members"][1]["online"], "Boris's nickname, playing now")
st, j = c.call("POST", "/api/party/join", {"code": "ZZZZZZ"})
ok(st == 404, "a wrong code")

print("== invited by the nickname")
st, j = a.call("POST", "/api/party/invite", {"mc": "chris_X"})
ok(st == 200, "Anna invites Chris_x")
j = c.look("Chris_x")
ok(len(j["invites"]) == 1 and j["invites"][0]["from"] == "Anna", "Chris sees the invitation")
st, j = a.call("POST", "/api/party/invite", {"mc": "DanaD"})
j = d.look("DanaD", server="play.other.net")
ok(j["invites"] == [], "Dana plays elsewhere: no invitation")
party = c.look("Chris_x")["invites"][0]["party"]
st, j = c.call("POST", "/api/party/join", {"party": party})
ok(st == 200 and len(c.look("Chris_x")["party"]["members"]) == 3, "Chris accepts: three")

print("== marks")
st, j = b.call("POST", "/api/party/marks", {"world": "mc.funtime.su|minecraft:overworld", "x": 100.5, "y": 64, "z": -20.25, "name": "Сюда", "icon": "mark-flag", "colour": 0xF2F1CB, "ping": True})
ok(st == 201, "Boris pings")
marks = a.look("AnnaMC")["party"]["marks"]
ok(len(marks) == 1 and marks[0]["by"] == "Boris" and marks[0]["ping"] and marks[0]["x"] == 100.5 and not marks[0]["mine"], "Anna sees his ping")
ok(marks[0]["until"] - marks[0]["created"] <= 60, "a ping lives under a minute")
st, j = b.call("POST", "/api/party/marks", {"world": "mc.funtime.su|minecraft:overworld", "x": 0, "z": 0, "ping": True})
marks = a.look("AnnaMC")["party"]["marks"]
ok(len(marks) == 1 and marks[0]["x"] == 0, "a new ping of his replaces the last")
st, j = a.call("POST", "/api/party/marks", {"world": "mc.funtime.su|minecraft:overworld", "x": 10, "y": 70, "z": 10, "name": "База", "icon": "mark-home", "colour": 0xBFC0E7})
mid = j["id"]
marks = c.look("Chris_x")["party"]["marks"]
ok(len(marks) == 2 and any(m["name"] == "База" and not m["ping"] and m["until"] - m["created"] > 3600 for m in marks), "a mark kept for hours")
st, j = c.call("DELETE", f"/api/party/marks/{mid}")
ok(len(a.look("AnnaMC")["party"]["marks"]) == 2, "Chris takes nobody's mark away")
st, j = a.call("DELETE", f"/api/party/marks/{mid}")
ok(len(a.look("AnnaMC")["party"]["marks"]) == 1, "Anna takes hers")
st, j = d.call("POST", "/api/party/marks", {"world": "x", "x": 0, "z": 0})
ok(st == 404, "no party: no marks")

print("== kicked, left")
st, j = c.call("POST", "/api/party/marks", {"world": "mc.funtime.su|minecraft:overworld", "x": 5, "z": 5, "name": "Chris's", "ping": False})
ok(any(m["by"] == "Chris" for m in a.look("AnnaMC")["party"]["marks"]), "Chris puts a mark")
st, j = b.call("POST", "/api/party/kick", {"login": "Chris"})
ok(st == 403, "only its leader kicks")
st, j = a.call("POST", "/api/party/kick", {"login": "Chris"})
ok(c.look("Chris_x")["party"] is None, "Chris is out")
ok(not any(m["by"] == "Chris" for m in a.look("AnnaMC")["party"]["marks"]), "his marks went with him")
st, j = a.call("POST", "/api/party/leave")
j = b.look("BorisPvP")
ok(j["party"]["leader"] and [m["login"] for m in j["party"]["members"]] == ["Boris"], "Anna left: Boris leads")
st, j = b.call("POST", "/api/party/leave")
st, j = c.call("POST", "/api/party/join", {"code": code})
ok(st == 404, "nobody left: the party is gone")

print()
print("ALL OK" if not failures else f"{len(failures)} FAILED: " + "; ".join(failures))
