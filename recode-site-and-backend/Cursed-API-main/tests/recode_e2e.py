#!/usr/bin/env python3
"""recode_e2e.py: Recode's keys end to end, on an instance of its own (an empty database in a temporary folder, port
8092, nothing of the staging's or the production's touched): only she makes, lists and revokes keys; a key typed any
way is taken once; the days add up, «forever» over all; a revoked key's days go; wrong keys wait after a few; the
account's Recode in /api/me. Run on the server as root: python3 tests/recode_e2e.py"""
import http.cookiejar
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

BIN = os.environ.get("CURSED_API_BIN", "/root/build/cursed-api/target/release/cursed-api")
PORT = 8092
BASE = f"http://127.0.0.1:{PORT}"
DAY = 86400
failures = []
tmp = tempfile.mkdtemp(prefix="recode-e2e-")
ENV = dict(os.environ, CURSED_DB=f"{tmp}/cursed.db", CURSED_DIST=f"{tmp}/dist", CURSED_MEDIA=f"{tmp}/media",
           CURSED_LISTEN=f"127.0.0.1:{PORT}", CURSED_ORIGIN=BASE, CURSED_SECURE_COOKIE="0")


def cli(*args):
    r = subprocess.run([BIN, *args], env=ENV, capture_output=True, text=True)
    return (r.stdout + r.stderr).strip()


def ok(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        failures.append(what)


class Member:
    def __init__(self, login):
        self.login = login
        self.op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def call(self, method, path, body=None, origin=True, text=False):
        # (origin False: another site's page asks — no Origin at all is the loader's way, with its token)
        h = {"Origin": BASE if origin else "https://another.example"}
        data = None
        if body is not None:
            data, h["Content-Type"] = json.dumps(body).encode(), "application/json"
        req = urllib.request.Request(BASE + path, data=data, method=method, headers=h)
        try:
            with self.op.open(req, timeout=30) as r:
                b = r.read()
                if text:
                    return r.status, b.decode(), {k.lower(): v for k, v in r.headers.items()}
                return r.status, (json.loads(b) if b else {})
        except urllib.error.HTTPError as e:
            b = e.read()
            if text:
                return e.code, b.decode(errors="replace"), {}
            try:
                return e.code, json.loads(b)
            except Exception:
                return e.code, {}

    def register(self, key=""):
        st, j = self.call("POST", "/api/register", {"login": self.login, "email": self.login.lower() + "@e.st", "password": "0123456789", "key": key})
        assert st == 200, (st, j)
        return self


server = subprocess.Popen([BIN, "serve"], env=ENV, stdout=subprocess.DEVNULL, stderr=open(f"{tmp}/log", "w"))
try:
    for _ in range(100):
        try:
            urllib.request.urlopen(BASE + "/api/me", timeout=1)
        except urllib.error.HTTPError:
            break
        except Exception:
            time.sleep(0.1)
    cli("registration", "open", "--limit", "100")
    her = Member("Lunuca").register(cli("key"))
    cli("renumber", "1", "0")
    a, b = Member("Anna").register(), Member("Boris").register()
    KEY = re.compile(r"^RCD-[2-9A-HJ-NP-Z]{5}-[2-9A-HJ-NP-Z]{5}-[2-9A-HJ-NP-Z]{5}$")

    print("== only hers")
    st, j = a.call("GET", "/api/admin/keys")
    ok(st == 403, f"a member sees no keys ({st})")
    st, j = a.call("POST", "/api/admin/keys", {"days": 30, "count": 1})
    ok(st == 403, f"a member makes none ({st})")
    st, j = her.call("POST", "/api/admin/keys", {"days": 30, "count": 1}, origin=False)
    ok(st == 403, f"not from the site's page: refused ({st})")

    print("== making")
    st, m30 = her.call("POST", "/api/admin/keys", {"days": 30, "count": 3, "note": "FunPay test"})
    ok(st == 200 and len(m30["codes"]) == 3 and all(KEY.match(c) for c in m30["codes"]) and m30["batch"] == 1, f"3 keys for 30 days, batch 1 ({st} {m30.get('codes')})")
    st, m7 = her.call("POST", "/api/admin/keys", {"days": 7, "count": 2})
    st2, mf = her.call("POST", "/api/admin/keys", {"days": 0, "count": 1})
    ok(st == 200 and st2 == 200 and m7["batch"] == 2 and mf["batch"] == 3, "7 days and forever, batches 2 and 3")
    for bad, why in (({"days": 3, "count": 1}, "a term not sold"), ({"days": 30, "count": 0}, "none"), ({"days": 30, "count": 501}, "more than 500")):
        st, j = her.call("POST", "/api/admin/keys", bad)
        ok(st == 400, f"refused: {why} ({st})")
    st, big = her.call("POST", "/api/admin/keys", {"days": 1, "count": 500})
    ok(st == 200 and len(set(big["codes"])) == 500, "500 at once, all different")
    st, j = her.call("GET", "/api/admin/keys")
    ok(st == 200 and j["counts"] == {"free": 506, "used": 0, "revoked": 0} and len(j["items"]) == 100 and j["more"], f"listed: 506 free, a page of 100 ({j.get('counts')})")
    ok([x["batch"] for x in j["batches"]][:4] == [4, 3, 2, 1] and j["batches"][-1]["note"] == "FunPay test", "the batches, newest first, with the note")

    print("== activating")
    st, j = a.call("GET", "/api/recode")
    ok(st == 200 and j["active"] is False and j["until"] is None, "no Recode before a key")
    k1, k2, k3 = m30["codes"]
    typed = k1[4:].replace("-", " ").lower()
    t0 = time.time()
    st, j = a.call("POST", "/api/recode/activate", {"key": typed})
    ok(st == 200 and j["active"] and j["days"] == 30 and abs(j["until"] - (t0 + 30 * DAY)) < 5, f"typed in lower case with spaces: 30 days ({st} {j})")
    until30 = j.get("until", 0)
    st, j = a.call("POST", "/api/recode/activate", {"key": m7["codes"][0]})
    ok(st == 200 and j["until"] == until30 + 7 * DAY, "a second key adds its days after the first's")
    st, j = b.call("POST", "/api/recode/activate", {"key": k1})
    ok(st == 409 and j.get("error") == "key-used", f"someone else's used key ({st} {j.get('error')})")
    st, j = a.call("POST", "/api/recode/activate", {"key": k1})
    ok(st == 409 and j.get("error") == "key-yours", f"one's own used key again ({st} {j.get('error')})")
    st, j = a.call("POST", "/api/recode/activate", {"key": k2}, origin=False)
    ok(st == 403, f"activation not from the site's page: refused ({st})")
    st, j = a.call("GET", "/api/me")
    ok(st == 200 and j["recode"]["active"] and j["recode"]["until"] == until30 + 7 * DAY, "/api/me has the account's Recode")
    st, j = a.call("GET", "/api/recode")
    ok(len(j["keys"]) == 2 and all("•" in x["code"] for x in j["keys"]), "the cabinet lists its keys, hidden but for their ends")

    print("== wrong keys")
    codes = []
    for i in range(8):
        st, j = b.call("POST", "/api/recode/activate", {"key": "RCD-22222-22222-2222" + "23456789"[i]})
        codes.append((st, j.get("error")))
    ok(all(c == (404, "key-wrong") for c in codes), f"8 wrong keys: no such key ({codes[-1]})")
    st, j = b.call("POST", "/api/recode/activate", {"key": k2})
    ok(st == 429 and j.get("error") == "key-tries", f"then it waits, even with a good key ({st} {j.get('error')})")
    st, j = b.call("POST", "/api/recode/activate", {"key": "not a key"})
    ok(st == 429, "and with nonsense")

    print("== revoking")
    st, j = a.call("POST", f"/api/admin/keys/{k1}/revoke")
    ok(st == 403, "a member revokes nothing")
    st, j = her.call("POST", f"/api/admin/keys/{k1}/revoke")
    ok(st == 200 and j["sub"]["active"] and abs(j["sub"]["until"] - (until30 - 30 * DAY + 7 * DAY)) < 5, f"the 30 days go, the 7 stay, from their own activation ({j})")
    st, j = her.call("POST", f"/api/admin/keys/{k3}/revoke")
    st, j = a.call("POST", "/api/recode/activate", {"key": k3})
    ok(st == 410 and j.get("error") == "key-revoked", f"a revoked key is not taken ({st} {j.get('error')})")
    st, j = her.call("GET", "/api/admin/keys?state=revoked")
    ok(st == 200 and sorted(x["code"] for x in j["items"]) == sorted([k1, k3]) and j["counts"]["revoked"] == 2, "listed as revoked")
    st, j = her.call("GET", "/api/admin/keys?state=used&q=anna")
    ok(st == 200 and [x["code"] for x in j["items"]] == [m7["codes"][0]] and j["items"][0]["used"]["login"] == "Anna", "found by the login it went to")

    print("== forever")
    st, j = a.call("POST", "/api/recode/activate", {"key": mf["codes"][0]})
    ok(st == 200 and j["forever"] and j["active"], "forever over all")
    st, j = her.call("GET", "/api/recode")
    ok(st == 200 and j["forever"] and j["owner"], "hers is forever without a key")

    print("== the batch as a file")
    st, body, hd = her.call("GET", "/api/admin/keys/batch/1.txt", text=True)
    ok(st == 200 and body.split() == [k2] and "attachment" in hd.get("content-disposition", "") and "30d" in hd.get("content-disposition", ""), f"batch 1's keys not used, one a line ({st} {body.split()})")
    st, body, hd = a.call("GET", "/api/admin/keys/batch/1.txt", text=True)
    ok(st == 403, "not for a member")

    print("== the role «Ключи» (her word, 23.10): one's own batches, ten at a time")
    m, rook = Member("Mentos").register(), Member("Rook").register()
    for who in ("Mentos", "Rook"):
        cli("role", who, "admin")
    st, j = m.call("GET", "/api/admin/keys")
    ok(st == 403, f"an admin without the role sees no keys ({st})")
    st, j = rook.call("POST", "/api/staff/roles", {"login": "Mentos", "role": "keys", "on": True})
    ok(st == 403, f"an admin gives the role to no one ({st})")
    st, j = her.call("POST", "/api/staff/roles", {"login": "Mentos", "role": "keys", "on": True})
    ok(st == 403, f"owner HTTP grant denied ({st} {j})")
    cli("role", "Mentos", "keys")
    st, j = m.call("GET", "/api/me")
    ok("recode.keys" in j.get("perms", []), "/api/me tells the site")
    all_before = her.call("GET", "/api/admin/keys")[1]["counts"]
    st, j = m.call("POST", "/api/admin/keys", {"days": 30, "count": 11})
    ok(st == 400 and j.get("error") == "count", f"eleven at once: refused ({st} {j.get('error')})")
    st, own = m.call("POST", "/api/admin/keys", {"days": 30, "count": 10, "note": "Mentos"})
    st2, ob = m.call("POST", "/api/admin/keys", {"days": 0, "count": 1, "beta": True})
    ok(st == 200 and len(own["codes"]) == 10 and st2 == 200 and ob["beta"], f"ten for 30 days, and a beta key forever ({st} {st2})")
    st, j = m.call("GET", "/api/admin/keys")
    ok(st == 200 and j["own"] and j["most"] == 10 and j["counts"] == {"free": 11, "used": 0, "revoked": 0}, f"his list: his 11 keys alone ({j.get('counts')})")
    ok(sorted(x["batch"] for x in j["batches"]) == sorted([own["batch"], ob["batch"]]) and any(x["beta"] for x in j["batches"]), "his batches alone, the beta one marked")
    ok(set(x["code"] for x in j["items"]) == set(own["codes"] + ob["codes"]), "his keys alone in the items")
    st, j = m.call("GET", f"/api/admin/keys?q={k2[4:9]}")
    ok(st == 200 and j["items"] == [], "hers are not found by a search")
    st, j = her.call("GET", "/api/admin/keys")
    ok(not j["own"] and j["counts"]["free"] == all_before["free"] + 11, f"she sees his among all ({j['counts']})")
    st, body, hd = m.call("GET", f"/api/admin/keys/batch/{own['batch']}.txt", text=True)
    ok(st == 200 and sorted(body.split()) == sorted(own["codes"]), "his batch as a file")
    st, body, hd = m.call("GET", "/api/admin/keys/batch/1.txt", text=True)
    ok(st == 404, f"hers not ({st})")
    st, j = m.call("POST", f"/api/admin/keys/{k2}/revoke")
    ok(st == 404, f"he revokes none of hers ({st})")
    ok(her.call("GET", "/api/admin/keys/batch/1.txt", text=True)[1].split() == [k2], "hers still free")
    st, j = m.call("POST", f"/api/admin/keys/{own['codes'][0]}/revoke")
    ok(st == 200, f"his own he revokes ({st})")
    st, j = her.call("POST", "/api/admin/users/Boris/recode", {"days": 1})
    st, j = m.call("POST", "/api/admin/users/Boris/recode", {"days": 7})
    ok(st == 200 and j["active"] and [g["days"] for g in j["given"]] == [7], f"he gives by hand; his card shows what he gave ({st} {j.get('given')})")
    st, j = rook.call("POST", "/api/admin/users/Boris/recode", {"days": 7})
    ok(st == 403, f"an admin without the role gives none ({st})")
    st, j = m.call("DELETE", "/api/admin/users/Boris/recode")
    ok(st == 200 and j["taken"] == 1 and j["active"], f"he takes back his alone: her day stays ({j.get('taken')})")
    st, j = her.call("GET", "/api/admin/users/Boris")
    ok(st == 200 and [g["days"] for g in j["recode"]["given"]] == [1], "her card: her day")
    st, j = rook.call("GET", "/api/admin/users/Boris")
    ok(st == 200 and "recode" not in j, "an admin's without the role: no Recode on it")
    st, j = her.call("POST", "/api/staff/roles", {"login": "Mentos", "role": "keys", "on": False})
    ok(st == 403, "owner HTTP revoke denied")
    cli("role", "Mentos", "keys", "off")
    st, j = m.call("GET", "/api/admin/keys")
    ok(st == 403, f"the role taken: no keys ({st})")

    print("== a renumbered account keeps its keys")
    before = a.call("GET", "/api/recode")[1]
    cli("renumber", "2", "77")
    after = a.call("GET", "/api/recode")[1]
    ok(after.get("forever") and len(after.get("keys", [])) == len(before["keys"]), "UID 2 → 77: still forever, the same keys")
finally:
    server.terminate()
    server.wait(10)
    shutil.rmtree(tmp, ignore_errors=True)

print()
print("FAILED: " + "; ".join(failures) if failures else "all good")
sys.exit(1 if failures else 0)
