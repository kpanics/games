"""configs_e2e.py: configs in the cloud on the staging instance (run on the server after a reset): one's own (made,
the first one active, changed, renamed, the last one kept), shared by a code with a limit of activations, redeemed
(active at once, read but not changed), a change heard by whoever runs by it, access taken back one by one and with
the whole code (they fall back to their own), the badge of the configurator.
"""
import http.cookiejar
import json
import os
import sys
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

    def wait(self, t, cond=lambda e: True, secs=5):
        end = time.time() + secs
        while time.time() < end:
            for e in list(self.events):
                if e.get("t") == t and cond(e):
                    return e
            time.sleep(0.05)
        return None


SETTINGS = {"modules": {"auto-sprint": {"on": True, "key": -1, "settings": {"Swim in water": False}}}, "interface": {"hotbar": {"mode": "auto", "at": None, "docked": False}}}

os.system(CLI + "registration open --limit 1000 >/dev/null")
her = Member("Lunuca").register(os.popen(CLI + "key").read().strip())
os.system(CLI + "renumber 1 0 >/dev/null")
a, b, m, z = Member("Anna").register(), Member("Boris").register(), Member("Mila").register(), Member("Zed").register()
for x in (a, b, m):
    x.listen()

print("== one's own")
st, j = a.call("GET", "/api/configs")
ok(st == 200 and j["mine"] == [] and j["active"] is None, "nothing at first")
st, j = a.call("POST", "/api/configs", {"name": "PvP", "data": SETTINGS})
ok(st == 201 and j["config"]["active"] and j["config"]["own"], "the first config is the active one")
pvp = j["config"]["id"]
st, j = a.call("POST", "/api/configs", {"name": "Красиво", "data": {"modules": {}}})
pretty = j["config"]["id"]
st, j = a.call("GET", "/api/configs")
ok([c["name"] for c in j["mine"]] == ["PvP", "Красиво"] and j["active"] == pvp, "two of them, the first still active")
st, j = a.call("POST", "/api/configs", {"name": "x", "data": "not an object"})
ok(st in (400, 422), "settings are an object")
st, j = a.call("POST", "/api/configs", {"name": "big", "data": {"x": "a" * 300000}})
ok(st == 413, "256 KB at most")
changed = json.loads(json.dumps(SETTINGS))
changed["modules"]["auto-sprint"]["on"] = False
st, j = a.call("PUT", f"/api/configs/{pvp}", {"data": changed})
ok(st == 200 and j["version"] == 2, "a change raises its version")
ok(a.wait("config", lambda e: e["id"] == pvp and e["version"] == 2) is not None, "her other devices hear it")
st, j = a.call("GET", f"/api/configs/{pvp}")
ok(j["data"]["modules"]["auto-sprint"]["on"] is False, "and read it back")
st, j = a.call("PATCH", f"/api/configs/{pvp}", {"name": "PvP · FunTime"})
st, j = a.call("GET", "/api/configs")
ok(j["mine"][0]["name"] == "PvP · FunTime", "renamed")
st, j = a.call("POST", f"/api/configs/{pretty}/active")
st, j = a.call("GET", "/api/configs")
ok(j["active"] == pretty, "another one made active")
st, j = b.call("GET", f"/api/configs/{pvp}")
ok(st == 404, "nobody else reads it")
st, j = b.call("PUT", f"/api/configs/{pvp}", {"data": SETTINGS})
ok(st == 403, "nor changes it")

print("== shared by a code")
st, j = a.call("POST", f"/api/configs/{pvp}/codes", {"max_uses": 2})
ok(st == 201 and len(j["code"]) == 9 and j["code"][4] == "-", f"a code of two activations ({j.get('code')})")
code = j["code"]
st, j = a.call("POST", f"/api/configs/{pvp}/codes", {"max_uses": 0})
ok(st == 400, "at least one activation")
st, j = b.call("POST", "/api/configs/redeem", {"code": code.lower().replace("-", " ")})
ok(st == 200 and j["config"]["id"] == pvp and not j["config"]["own"] and j["config"]["owner"]["login"] == "Anna", "Boris redeems it, typed loosely")
st, j = b.call("GET", "/api/configs")
ok(j["active"] == pvp and [c["id"] for c in j["shared"]] == [pvp], "it is his active one, among the shared")
st, j = b.call("GET", f"/api/configs/{pvp}")
ok(st == 200 and j["data"]["modules"]["auto-sprint"]["on"] is False, "he reads its settings")
st, j = b.call("PUT", f"/api/configs/{pvp}", {"data": SETTINGS})
ok(st == 403, "but does not change them")
st, j = b.call("POST", "/api/configs/redeem", {"code": code})
st, j2 = a.call("GET", "/api/configs")
ok(j2["mine"][0]["codes"][0]["uses"] == 1, "redeeming again takes no second activation")
st, j = m.call("POST", "/api/configs/redeem", {"code": code})
ok(st == 200, "Mila takes the second")
st, j = z.call("POST", "/api/configs/redeem", {"code": code})
ok(st == 410 and j.get("error") == "used", "Zed finds it used up")
st, j = a.call("POST", "/api/configs/redeem", {"code": code})
ok(st == 400 and j.get("error") == "own", "its owner needs no code")
st, j = z.call("POST", "/api/configs/redeem", {"code": "ZZZZ-ZZZZ"})
ok(st == 404, "no such code")
st, j = a.call("PUT", f"/api/configs/{pvp}", {"data": SETTINGS})
ok(b.wait("config", lambda e: e["id"] == pvp and e["version"] == 3) is not None, "Boris hears its owner changed it")
st, j = a.call("GET", f"/api/configs/{pvp}/access")
ok(sorted(p["person"]["login"] for p in j["people"]) == ["Boris", "Mila"], "Anna sees who runs by it")

print("== access taken back")
st, j = a.call("DELETE", f"/api/configs/{pvp}/access/Mila")
ok(st == 200, "Mila's access taken back")
e = m.wait("config_gone", lambda e: e["id"] == pvp)
ok(e is not None and e["active"] is None, "she hears it; with no config of her own she has none active")
st, j = m.call("GET", f"/api/configs/{pvp}")
ok(st == 404, "she reads it no more")
st, j = b.call("POST", "/api/configs", {"name": "Мой", "data": {"modules": {}}})
own_b = j["config"]["id"]
st, j = b.call("POST", f"/api/configs/{pvp}/active")
st, j = a.call("DELETE", f"/api/configs/{pvp}/codes/{code}")
ok(st == 200 and j["removed"] == 1, "the whole code taken back: Boris with it")
e = b.wait("config_gone", lambda e: e["id"] == pvp)
ok(e is not None and e["active"] == own_b, "Boris falls back to his own")
st, j = z.call("POST", "/api/configs/redeem", {"code": code})
ok(st == 410 and j.get("error") == "revoked", "a code taken back works no more")

print("== deleting, the configurator")
st, j = b.call("DELETE", f"/api/configs/{own_b}")
ok(st == 400 and j.get("error") == "last", "the last config of one's own stays while it is active")
st, j = a.call("POST", f"/api/configs/{pvp}/codes", {"max_uses": 100})
big = j["code"]
for i in range(50):
    x = Member(f"User{i:02d}").register()
    x.call("POST", "/api/configs/redeem", {"code": big})
st, p = a.call("GET", "/api/u/anna")
ok(any(bd["id"] == "configurator" for bd in p.get("badges", [])), "fifty activations: the configurator")
st, j = a.call("DELETE", f"/api/configs/{pvp}")
st, j = a.call("GET", "/api/configs")
ok([c["id"] for c in j["mine"]] == [pretty] and j["active"] == pretty, "a config deleted is gone; the active one stays")
st, j = x.call("GET", "/api/configs")
ok(j["shared"] == [] and j["active"] is None, "and whoever ran by it has it no more")

print("\n" + ("ALL OK" if not failures else f"{len(failures)} FAILED: " + "; ".join(failures)))
sys.exit(1 if failures else 0)
