"""Isolated Linux security regression. Usage: python3 security_access_e2e.py /path/to/binary

Creates its own empty database and ephemeral loopback port. No staging, production
database, real accounts, mail credentials or payment credentials are used.
"""
import hashlib
import http.client
import json
import os
from pathlib import Path
import secrets
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

binary = str(Path(sys.argv[1]).resolve())
root = Path(tempfile.mkdtemp(prefix="cursed-security-"))
with socket.socket() as s:
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
base = f"http://127.0.0.1:{port}"
env = {"PATH": os.environ["PATH"], "HOME": str(root), "CURSED_DB": str(root / "test.db"),
       "CURSED_LISTEN": f"127.0.0.1:{port}", "CURSED_ORIGIN": base,
       "CURSED_SECURE_COOKIE": "0", "CURSED_MEDIA": str(root / "media"),
       "CURSED_DIST": str(root / "dist")}
subprocess.run([binary, "registration", "open"], env=env, check=True, capture_output=True)
c = sqlite3.connect(root / "test.db")
tokens = {}
coowner = "--coowner" in sys.argv[2:]
fixtures = [(0, "Lunuca"), (1, "Ordinary"), (2, "OtherUser")]
if coowner:
    fixtures += [(19, "Andrey_AKM"), (3, "AdminControl"), (4, "CreatorControl"), (6, "Mentos"), (622, "Janade")]
for uid, login in fixtures:
    c.execute("INSERT INTO users(uid,login,login_key,email,email_key,pass,created) VALUES(?,?,?,?,?,'x',?)",
              (uid, login, login.lower(), login + "@e.st", login.lower() + "@e.st", int(time.time())))
    c.execute("INSERT INTO roles(uid,role,since) VALUES(?,'beta',?)", (uid, int(time.time())))
    tokens[uid] = secrets.token_hex(32)
    c.execute("INSERT INTO sessions(hash,uid,kind,created,seen) VALUES(?,?,'web',?,?)",
              (hashlib.sha256(tokens[uid].encode()).digest(), uid, int(time.time()), int(time.time())))
if coowner:
    for uid, role in [(3, "admin"), (3, "keys"), (4, "creator"), (6, "admin"), (6, "keys"), (6, "subscription_revoke")]:
        c.execute("INSERT INTO roles(uid,role,since) VALUES(?,?,?)", (uid, role, int(time.time())))
c.commit()
count = 0


def call(method, path, body=None, uid=None, origin=None):
    h = {"Origin": origin or base}
    if uid is not None:
        h["Cookie"] = "cursed_session=" + tokens[uid]
    data = None
    if body is not None:
        data = json.dumps(body).encode()
        h["Content-Type"] = "application/json"
    req = urllib.request.Request(base + path, data=data, headers=h, method=method)
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()


def expect(method, path, body, uid, status, origin=None):
    global count
    st, raw = call(method, path, body, uid, origin)
    assert st == status, (method, path, uid, st, status, raw[:200])
    count += 1


log = open(root / "server.log", "w")
proc = subprocess.Popen([binary, "serve"], env=env, stdout=log, stderr=log)
try:
    for _ in range(100):
        try:
            call("GET", "/api/me")
            break
        except OSError:
            time.sleep(.1)
    gets = ["summary", "crashes", "users", "users/OtherUser", "switch/registration", "pinned",
            "builds", "roles", "journal", "announcements"]
    for path in gets:
        expect("GET", "/api/admin/" + path, None, None, 401)
        expect("GET", "/api/admin/" + path, None, 1, 403)
    commands = [
        ("POST", "/api/staff/roles", {"login": "Ordinary", "role": "admin", "on": True}),
        ("POST", "/api/staff/badges", {"login": "Ordinary", "badge": "recode", "on": True}),
        ("DELETE", "/api/admin/users/OtherUser", None),
        ("POST", "/api/admin/users/OtherUser/notes", {"text": "test"}),
        ("POST", "/api/admin/users/OtherUser/ban", {"reason": "test"}),
        ("DELETE", "/api/admin/users/OtherUser/ban", None),
        ("POST", "/api/admin/users/OtherUser/device", None),
        ("POST", "/api/admin/users/OtherUser/reset-link", None),
        ("POST", "/api/admin/users/OtherUser/uid", {"to": 5}),
        ("POST", "/api/admin/switch/registration", {"state": "closed"}),
        ("POST", "/api/admin/pinned", {"address": "example.invalid", "name": "test"}),
        ("DELETE", "/api/admin/pinned/example.invalid", None),
        ("POST", "/api/admin/builds/rollback", {}),
        ("POST", "/api/admin/announcements", {"title": "test", "body": "test"}),
    ]
    for method, path, body in commands:
        expect(method, path, body, None, 401)
        expect(method, path, body, 1, 403)
        expect(method, path, body, 0, 403, "https://other.invalid")
    for role in ["creator", "admin", "moderator", "keys", "recode"]:
        expect("POST", "/api/staff/roles", {"login": "Ordinary", "role": role, "on": True}, 1, 403)
    expect("GET", "/api/admin/summary", None, 0, 200)
    expect("PATCH", "/api/me/profile", {"prefix": "admin"}, 1, 400)
    st, raw = call("POST", "/api/configs", {"name": "Private", "data": {"test": True}}, 1)
    assert st == 201, (st, raw)
    config = json.loads(raw)["config"]["id"]
    for method, tail, body, status in [
        ("GET", "", None, 404), ("PUT", "", {"data": {}}, 403),
        ("PATCH", "", {"name": "Stolen"}, 403), ("DELETE", "", None, 404),
        ("GET", "/history", None, 403), ("GET", "/access", None, 403),
    ]:
        expect(method, f"/api/configs/{config}" + tail, body, 2, status)
    expect("GET", f"/api/configs/{config}", None, 1, 200)
    if coowner:
        for owner in [0, 19]:
            st, raw = call("GET", "/api/me", uid=owner)
            assert st == 200 and json.loads(raw)["admin"] is True
            count += 1
            st, raw = call("GET", "/api/income", uid=owner)
            assert st == 200 and json.loads(raw)["owner"] is True, (st, raw[:200])
            count += 1
            for path in ["/api/admin/keys", "/api/admin/fps", "/api/blocklist/all"]:
                expect("GET", path, None, owner, 200)
            for method, path, body in [
                ("POST", "/api/admin/switch/registration", {"state": "open"}),
                ("POST", "/api/admin/pinned", {"address": "example.invalid", "name": "Test"}),
                ("DELETE", "/api/admin/pinned/example.invalid", None),
                ("POST", "/api/staff/roles", {"login": "OtherUser", "role": "keys", "on": True}),
                ("POST", "/api/staff/roles", {"login": "OtherUser", "role": "keys", "on": False}),
                ("POST", "/api/income/rates", {"admin": 1000, "admin2": 1000, "media": 1000}),
                ("POST", "/api/admin/keys", {"days": 30, "count": 11, "note": "isolated coowner test"}),
            ]:
                expect(method, path, body, owner, 403 if path == "/api/staff/roles" else 200)
            for target in ["Lunuca", "Andrey_AKM"]:
                expect("DELETE", "/api/admin/users/" + target, None, owner, 409)
                expect("POST", "/api/admin/users/" + target + "/ban", {"reason": "test"}, owner, 400)
                expect("POST", "/api/admin/users/" + target + "/uid", {"to": 7}, owner, 400)
            for reserved in [0, 19]:
                expect("POST", "/api/admin/users/OtherUser/uid", {"to": reserved}, owner, 400)
        for ordinary_admin in [3, 4]:
            for path in ["/api/admin/fps", "/api/blocklist/all"]:
                expect("GET", path, None, ordinary_admin, 403)
            for path, body in [
                ("/api/admin/switch/registration", {"state": "closed"}),
                ("/api/admin/pinned", {"address": "example.invalid"}),
                ("/api/income/rates", {"admin": 0, "admin2": 0, "media": 0}),
                ("/api/staff/roles", {"login": "OtherUser", "role": "keys", "on": True}),
                ("/api/staff/roles", {"login": "Andrey_AKM", "role": "admin", "on": False}),
            ]:
                expect("POST", path, body, ordinary_admin, 403)
        st, raw = call("GET", "/api/admin/roles", uid=19)
        assert st == 200
        creators = next(r for r in json.loads(raw)["roles"] if r["id"] == "creator")
        assert creators["count"] == 2 and {p["uid"] for p in creators["people"]} == {0, 19}
        assert c.execute("SELECT count(*) FROM recode_keys").fetchone()[0] == 22
        count += 1
    st, body = call("POST", "/api/register", {"login": "Injected", "email": "i@e.st", "password": "testing-password-123", "uid": 0, "admin": True, "roles": ["admin", "creator"]})
    assert st == 200, (st, body)
    registered = json.loads(body)["uid"]
    assert registered != 0
    assert not c.execute("SELECT 1 FROM roles WHERE uid=? AND role IN ('admin','creator')", (registered,)).fetchone()
    count += 1
    # Stored-XSS boundaries: clients cannot forge system statuses or message kinds.
    hostile = '<img src=x onerror="window.__xss++">'
    st, raw = call("POST", "/api/forum/threads", {"category": "bugs", "title": "Security fixture", "body": hostile}, 1)
    assert st == 201, (st, raw)
    thread_id = json.loads(raw)["id"]
    expect("POST", f"/api/forum/threads/{thread_id}/status", {"status": hostile}, 1, 403)
    expect("POST", f"/api/forum/threads/{thread_id}/status", {"status": hostile}, 0, 400)
    st, raw = call("POST", "/api/tickets", {"category": "other", "title": "Security fixture", "body": hostile, "kind": "system"}, 1)
    assert st == 201, (st, raw)
    ticket_id = json.loads(raw)["id"]
    for uid in [1, 0]:
        expect("POST", f"/api/tickets/{ticket_id}/state", {"state": hostile}, uid, 400)
    st, raw = call("POST", f"/api/tickets/{ticket_id}/messages", {"body": "state:" + hostile, "kind": "system", "internal": True}, 1)
    assert st == 201, (st, raw)
    row = c.execute("SELECT kind,internal FROM ticket_messages WHERE ticket=? ORDER BY id DESC LIMIT 1", (ticket_id,)).fetchone()
    assert row == ("text", 0), row
    for field in ["accent", "theme", "status_emoji", "prefix"]:
        expect("PATCH", "/api/me/profile", {field: hostile}, 1, 400)
    for url in ["javascript:alert(1)", "data:text/html,test", "https://example.invalid/\" onmouseover=alert(1)"]:
        expect("PATCH", "/api/me/profile", {"socials": {"site": url}}, 1, 400)
    # SVG/HTML must never become same-origin executable uploads.
    for path in ["/api/avatar", "/api/forum/pictures"]:
        req = urllib.request.Request(base + path, data=hostile.encode(), headers={"Origin": base, "Cookie": "cursed_session=" + tokens[1], "Content-Type": "image/svg+xml"}, method="POST")
        try:
            urllib.request.urlopen(req, timeout=10)
            raise AssertionError("Hostile image accepted: " + path)
        except urllib.error.HTTPError as e:
            assert e.code == 400, (path, e.code)
        count += 1
    # Every account class, every role, both grant and revoke: HTTP never mutates roles.
    before_roles = c.execute("SELECT * FROM roles ORDER BY uid,role").fetchall()
    before_audit = c.execute("SELECT count(*) FROM audit WHERE action LIKE 'role.%'").fetchone()
    for uid in tokens:
        for role in ["creator", "admin", "moderator", "support", "partner", "media", "ai", "keys", "beta", "recode", "rc_beta", "subscription_revoke"]:
            for on in [True, False]:
                expect("POST", "/api/staff/roles", {"login": "OtherUser", "role": role, "on": on}, uid, 403)
    assert before_roles == c.execute("SELECT * FROM roles ORDER BY uid,role").fetchall()
    assert before_audit == c.execute("SELECT count(*) FROM audit WHERE action LIKE 'role.%'").fetchone()
    subprocess.run([binary, "role", "OtherUser", "partner"], env=env, check=True, capture_output=True)
    assert c.execute("SELECT 1 FROM roles WHERE uid=2 AND role='partner'").fetchone()
    subprocess.run([binary, "role", "OtherUser", "partner", "off"], env=env, check=True, capture_output=True)
    assert not c.execute("SELECT 1 FROM roles WHERE uid=2 AND role='partner'").fetchone()
    if coowner:
        # Full revocation is a separate capability, not implied by admin/keys.
        target = "/api/admin/users/OtherUser/recode/all"
        for uid, status in [(None, 401), (1, 403), (3, 403)]:
            expect("DELETE", target, None, uid, status)
        expect("DELETE", target, None, 6, 403, "https://other.invalid")
        for owner_name in ["Lunuca", "Andrey_AKM"]:
            expect("DELETE", "/api/admin/users/" + owner_name + "/recode/all", None, 6, 403)
        for uid, allowed in [(6, True), (3, False), (0, True)]:
            st, raw = call("GET", "/api/admin/users/OtherUser", uid=uid)
            assert st == 200 and json.loads(raw)["recode"]["can_revoke_all"] is allowed
            count += 1
        at = int(time.time())
        for code, used_by, given in [("RCD-TEST-PAID-FOREVER", 2, None), ("RCD-TEST-PAID-MONTH", 2, None), ("RCD-TEST-GIFT-OWNER", 2, 0), ("RCD-TEST-OTHER-USER", 1, None), ("RCD-TEST-UNUSED-KEY", None, None)]:
            c.execute("INSERT INTO recode_keys(code,days,batch,note,made,used_by,used_login,used_at,given,maker) VALUES(?,0,999,'fixture',?,?,?, ?,?,0)", (code, at, used_by, 'fixture' if used_by else None, at if used_by else None, given))
        c.execute("UPDATE recode_keys SET days=30 WHERE code='RCD-TEST-PAID-MONTH'")
        c.commit()
        st, raw = call("DELETE", target, uid=6)
        result = json.loads(raw)
        assert st == 200 and result["taken"] == 3 and not result["active"], (st, raw)
        assert c.execute("SELECT count(*) FROM recode_keys WHERE used_by=2 AND revoked IS NULL").fetchone()[0] == 0
        assert c.execute("SELECT count(*) FROM recode_keys WHERE code IN ('RCD-TEST-OTHER-USER','RCD-TEST-UNUSED-KEY') AND revoked IS NULL").fetchone()[0] == 2
        assert c.execute("SELECT count(*) FROM audit WHERE actor=6 AND action='recode.revoke_all' AND target='user:2'").fetchone()[0] == 1
        st, raw = call("DELETE", target, uid=6)
        assert st == 200 and json.loads(raw)["taken"] == 0
        count += 2
    # Existing live response must end after its session is revoked, with no reconnect.
    stream = http.client.HTTPConnection("127.0.0.1", port, timeout=18)
    stream.request("GET", "/api/live", headers={"Cookie": "cursed_session=" + tokens[1]})
    response = stream.getresponse()
    assert response.status == 200
    while response.readline().strip():
        pass
    expect("POST", "/api/logout", None, 1, 200)
    start = time.monotonic()
    while response.readline():
        pass
    assert time.monotonic() - start < 20
    count += 1
    stream.close()
    expect("GET", "/api/me", None, 1, 401)
    # Simulate a login racing with a ban: valid session remains in DB, ban still wins.
    c.execute("INSERT INTO bans(uid,login,reason,at,by) VALUES(2,'OtherUser','test',?,'test')", (int(time.time()),))
    c.commit()
    expect("GET", "/api/me", None, 2, 401)
    expect("GET", "/api/live", None, 2, 401)
    assert c.execute("SELECT count(*) FROM users").fetchone()[0] == len(fixtures) + 1
    assert not c.execute("SELECT 1 FROM announcements").fetchone()
    if coowner:
        expect("GET", "/api/income", None, 622, 200)
        expect("POST", "/api/income/rates", {"admin": 0, "admin2": 500, "media": 1500}, 0, 200)
        expect("GET", "/api/income", None, 622, 403)
        st, raw = call("GET", "/api/income/access", uid=622)
        assert st == 200 and json.loads(raw)["allowed"] is False, raw
        count += 1
        expect("GET", "/api/income", None, 0, 200)
        expect("GET", "/api/income", None, 19, 200)
    print(f"PASS: {count} isolated access/session checks; fixtures: {root}")
finally:
    c.close()
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()
    log.close()
