"""tickets_e2e.py: support on the staging instance (run on the server after a reset): a ticket opened, the staff told,
answered (the author's turn), notes the author never sees, assigned, moved, closed and rated; a bug from the game with
what it knows of itself and its log; files only for the author and the staff; ready answers; one's own computer freed
(once a day) and by the staff from a ticket; limits.
"""
import gzip
import http.cookiejar
import json
import os
import sqlite3
import threading
import time
import urllib.error
import urllib.request

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

    def call(self, method, path, body=None, raw=None, text=False):
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

    def heard(self, t, cond=lambda e: True, secs=5):
        end = time.time() + secs
        while time.time() < end:
            if any(e.get("t") == t and cond(e) for e in list(self.events)):
                return True
            time.sleep(0.05)
        return False


os.system(CLI + "registration open --limit 1000 >/dev/null")
her = Member("Lunuca").register(os.popen(CLI + "key").read().strip())
os.system(CLI + "renumber 1 0 >/dev/null")
a, s, m = Member("Anna").register(), Member("Sam").register(), Member("Mila").register()
os.system(CLI + "role Sam support >/dev/null")
for x in (a, s, m, her):
    x.listen()

print("== a ticket")
st, j = a.call("POST", "/api/tickets", {"category": "account", "title": "Не могу войти", "body": "Пишет «другое устройство»."})
ok(st == 201, "Anna opens a ticket")
t1 = j["id"]
ok(s.note(lambda n: n["kind"] == "ticket" and n["data"]["ticket"] == t1) is not None, "Sam (support) is told")
ok(her.note(lambda n: n["kind"] == "ticket" and n["data"]["ticket"] == t1) is not None, "and so is she")
ok(m.note(lambda n: n["kind"] == "ticket", secs=1) is None, "a member is not")
st, j = m.call("GET", f"/api/tickets/{t1}")
ok(st == 404, "nor reads it")
st, j = a.call("GET", "/api/tickets")
ok([t["id"] for t in j["items"]] == [t1] and j["items"][0]["state"] == "open" and j["staff"] is False, "Anna sees her ticket, open")
st, j = s.call("GET", "/api/tickets?all=1")
ok(j["staff"] and [t["id"] for t in j["items"]] == [t1] and j["items"][0]["unread"] and j["counts"]["unassigned"] == 1, "the staff's queue: it, unread, nobody's")

print("== answered")
st, j = s.call("POST", f"/api/tickets/{t1}/messages", {"body": "Сбросил привязку, попробуйте снова.", "internal": False})
ok(st == 201 and j["state"] == "waiting", "Sam answers: Anna's turn")
ok(a.note(lambda n: n["kind"] == "ticket" and n["data"]["what"] == "reply") is not None, "Anna is told")
st, j = s.call("POST", f"/api/tickets/{t1}/messages", {"body": "Похоже на смену диска.", "internal": True})
st, j = a.call("GET", f"/api/tickets/{t1}")
ok(len(j["messages"]) == 2 and not any(x["internal"] for x in j["messages"]), "Anna does not see the staff's note")
ok(j["assignee"]["login"] == "Sam" and j["state"] == "waiting", "answering assigned it to Sam")
st, j = s.call("GET", f"/api/tickets/{t1}")
ok(len(j["messages"]) == 3 and j["messages"][2]["internal"] and j["staff_view"], "the staff do")
st, j = a.call("POST", f"/api/tickets/{t1}/messages", {"body": "Заработало, спасибо!"})
ok(j["state"] == "working", "Anna answers: the staff's turn again (it is assigned)")
st, j = a.call("POST", f"/api/tickets/{t1}/state", {"state": "working"})
ok(st == 403, "she moves it nowhere but closed")
st, j = s.call("POST", f"/api/tickets/{t1}/state", {"state": "closed"})
st, j = a.call("POST", f"/api/tickets/{t1}/messages", {"body": "ещё"})
ok(st == 409, "closed: no more messages from her")
st, j = a.call("POST", f"/api/tickets/{t1}/rate", {"rating": 5, "note": "быстро"})
st, j = s.call("GET", f"/api/tickets/{t1}")
ok(j["rating"] == 5 and j["rating_note"] == "быстро" and j["state"] == "closed", "rated 5 once closed")
ok(any(x["kind"] == "system" and x["body"] == "state:closed" for x in j["messages"]), "the closing is in its history")
st, j = a.call("POST", f"/api/tickets/{t1}/rate", {"rating": 9})
ok(st == 400, "a rating is 1 to 5")

print("== a bug from the game")
time.sleep(61)  # a new ticket once a minute
report = {"build": 7, "version": "0.1.0-beta.7", "gpu": "AMD Radeon RX 6600", "os": "Windows 11 26200", "ram": "16 GB", "java": "21.0.12", "junk": {"x": 1}}
st, j = a.call("POST", "/api/tickets", {"category": "bug", "title": "Погода не рисуется на AMD", "body": "Дождь пропадает.", "report": report})
t2 = j["id"]
log = ("[12:00:00] [Render thread/INFO]: a line of the log\n" * 2000).encode()
st, j = a.call("POST", f"/api/tickets/{t2}/files?name=latest.log", raw=log)
ok(st == 201 and j["file"]["name"] == "latest.log" and j["file"]["size"] == len(log), "the log attached")
fid = j["file"]["id"]
st, j = s.call("GET", f"/api/tickets/{t2}")
ok(j["meta"]["gpu"] == "AMD Radeon RX 6600" and j["meta"]["build"] == "7" and "junk" not in j["meta"], "what the game said of itself, kept plain")
ok([f["name"] for f in j["files"]] == ["latest.log"], "its files listed")
st, body = s.call("GET", f"/api/tickets/{t2}/files/{fid}", text=True)
ok(st == 200 and body == log, "the staff read the log as it was")
st, body = m.call("GET", f"/api/tickets/{t2}/files/{fid}", text=True)
ok(st == 404, "nobody else does")
try:
    st, j = a.call("POST", f"/api/tickets/{t2}/files?name=big.log", raw=b"x" * (5 << 20))
except (ConnectionError, OSError):
    st = 413  # the server answered 413 and closed before the whole body went: the same refusal
ok(st in (400, 413), "4 MB at most")
c = sqlite3.connect(DB)
on_disk = c.execute("SELECT file FROM ticket_files WHERE id = ?", (fid,)).fetchone()[0]
c.close()
ok(len(open("/var/lib/cursed-staging/ticket-files/" + on_disk, "rb").read()) < len(log) // 10, "kept compressed")

print("== assigned, ready answers, limits")
st, j = her.call("POST", f"/api/tickets/{t2}/assign", {"login": "Sam"})
ok(st == 200 and j["assignee"]["login"] == "Sam", "she gives it to Sam")
ok(s.note(lambda n: n["kind"] == "ticket" and n["data"].get("what") == "assigned") is not None, "Sam hears it is his")
st, j = her.call("POST", f"/api/tickets/{t2}/assign", {"login": "Mila"})
ok(st == 400, "only to the staff")
st, j = s.call("POST", "/api/staff/replies", {"title": "Сброс", "body": "Привязку сбросили, войдите заново."})
st, j = s.call("GET", "/api/staff/replies")
ok([r["title"] for r in j["items"]] == ["Сброс"], "a ready answer kept")
st, j = a.call("GET", "/api/staff/replies")
ok(st == 403, "for the staff only")
st, j = a.call("POST", "/api/tickets", {"category": "other", "title": "Ещё вопрос", "body": "?"})
ok(st == 429, "a new ticket once a minute")
st, j = a.call("POST", "/api/tickets", {"category": "nonsense", "title": "Вопрос", "body": "?"})
ok(st == 400, "only its categories")
st, j = s.call("GET", "/api/tickets?all=1&assignee=me&state=active")
ok([t["id"] for t in j["items"]] == [t2], "Sam's own, not closed: the bug")

print("== one's own computer")
c = sqlite3.connect(DB)
uid = c.execute("SELECT uid FROM users WHERE login = 'Anna'").fetchone()[0]
c.execute("INSERT OR REPLACE INTO devices (uid, parts, bound, seen) VALUES (?, '[]', 1, 1)", (uid,))
c.commit()
st, j = a.call("GET", "/api/me/device")
ok(j["bound"] == 1 and j["reset_from"] == 0, "bound, free to reset")
st, j = a.call("POST", "/api/me/device/reset", {})
ok(st == 200 and j["reset_from"] > time.time(), "freed by herself")
ok(c.execute("SELECT COUNT(*) FROM devices WHERE uid = ?", (uid,)).fetchone()[0] == 0, "the binding is gone")
st, j = a.call("POST", "/api/me/device/reset", {})
ok(st == 429 and j["until"] > time.time(), "not again today")
c.execute("INSERT OR REPLACE INTO devices (uid, parts, bound, seen) VALUES (?, '[]', 1, 1)", (uid,))
c.commit()
st, j = m.call("POST", f"/api/tickets/{t2}/device-reset")
ok(st in (403, 404), "a member frees nobody's")
st, j = s.call("POST", f"/api/tickets/{t2}/device-reset")
ok(st == 200 and c.execute("SELECT COUNT(*) FROM devices WHERE uid = ?", (uid,)).fetchone()[0] == 0, "Sam frees it from the ticket at once")
ok(a.note(lambda n: n["kind"] == "ticket" and n["data"]["what"] == "device-reset") is not None, "Anna is told")
actions = [r[0] for r in c.execute("SELECT action FROM audit ORDER BY id")]
c.close()
ok(all(x in actions for x in ("ticket.state", "ticket.assign", "device.reset")), "the staff's acts in the journal")

print()
print("ALL OK" if not failures else f"{len(failures)} FAILED: " + "; ".join(failures))
