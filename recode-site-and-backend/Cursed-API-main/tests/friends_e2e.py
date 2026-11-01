"""friends_e2e.py: friends and conversations on the staging instance (run on the server after a reset): requests,
friendships, blocks; direct conversations and who may start one; messages (text, a picture, a voice message, a reply,
reactions, an edit, removal), read and typing on the live line, the pin; groups (invitations, the link, admins,
removal, the owner leaving), reports of a message and what staff see of it; the badges that come of it.
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
        self.jar = http.cookiejar.CookieJar()
        self.op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))
        self.events = []

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
        self.events = []
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
        return resp

    def wait(self, t, cond=lambda e: True, secs=5):
        end = time.time() + secs
        while time.time() < end:
            for e in list(self.events):
                if e.get("t") == t and cond(e):
                    return e
            time.sleep(0.05)
        return None

    def forget(self):
        self.events.clear()


def png(w, h):
    rows = []
    for y in range(h):
        row = bytearray([0])
        for x in range(w):
            row += bytes((x % 256, y % 256, 140))
        rows.append(bytes(row))
    raw = b"".join(rows)

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return bytes([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def ogg_crc(data):
    crc = 0
    for byte in data:
        crc ^= byte << 24
        for _ in range(8):
            crc = ((crc << 1) ^ 0x04C11DB7) & 0xFFFFFFFF if crc & 0x80000000 else (crc << 1) & 0xFFFFFFFF
    return crc


def ogg_page(kind, granule, seq, packet):
    lacing = []
    n = len(packet)
    while n >= 255:
        lacing.append(255)
        n -= 255
    lacing.append(n)
    head = b"OggS" + bytes([0, kind]) + struct.pack("<qII", granule, 7, seq) + b"\x00\x00\x00\x00" + bytes([len(lacing)]) + bytes(lacing)
    page = head + packet
    return page[:22] + struct.pack("<I", ogg_crc(page)) + page[26:]


def ogg_opus(seconds):
    """An Ogg Opus of silence: its head (pre-skip 312), its tags, and frames of 20 ms (a silent CELT frame each)."""
    pre = 312
    head = b"OpusHead" + bytes([1, 1]) + struct.pack("<HIhB", pre, 48000, 0, 0)
    tags = b"OpusTags" + struct.pack("<I", 6) + b"Cursed" + struct.pack("<I", 0)
    frames = int(seconds * 50)
    pages = [ogg_page(2, 0, 0, head), ogg_page(0, 0, 1, tags)]
    seq, done = 2, 0
    while done < frames:
        take = min(50, frames - done)
        done += take
        packets = b"\xf8\xff\xfe" * take
        # Each 3-byte packet is its own segment.
        lacing = bytes([3] * take)
        granule = pre + done * 960
        kind = 4 if done == frames else 0
        h = b"OggS" + bytes([0, kind]) + struct.pack("<qII", granule, 7, seq) + b"\x00\x00\x00\x00" + bytes([len(lacing)]) + lacing
        page = h + packets
        pages.append(page[:22] + struct.pack("<I", ogg_crc(page)) + page[26:])
        seq += 1
    return b"".join(pages)


os.system(CLI + "registration open --limit 1000 >/dev/null")
her = Member("Lunuca").register(os.popen(CLI + "key").read().strip())
os.system(CLI + "renumber 1 0 >/dev/null")
her = Member("Lunuca")
her.call("POST", "/api/login", {"login": "Lunuca", "password": "0123456789"})
a, b, m, z = Member("Anna").register(), Member("Boris").register(), Member("Mila").register(), Member("Zed").register()
for x in (a, b, m, z, her):
    x.listen()

print("== friend requests")
st, j = a.call("POST", "/api/friends", {"login": "Boris"})
ok(st == 200 and j.get("state") == "sent", f"a request is sent ({st} {j})")
ok(b.wait("friends") is not None, "Boris hears his list changed")
st, j = b.call("GET", "/api/friends")
ok([x["person"]["login"] for x in j.get("incoming", [])] == ["Anna"], "Boris has Anna's request waiting")
st, n = b.call("GET", "/api/notifications")
ok(any(x["kind"] == "friend" and x["data"].get("request") for x in n.get("items", [])), "and a notification of it")
st, j = a.call("POST", "/api/friends", {"login": "Boris"})
ok(j.get("state") == "sent", "asking again changes nothing")
st, j = a.call("POST", "/api/friends", {"login": "Anna"})
ok(st == 400 and j.get("error") == "self", "nobody befriends themselves")
st, j = b.call("POST", "/api/friends/Anna/accept")
ok(st == 200 and j.get("state") == "friends", "Boris accepts")
st, j = a.call("GET", "/api/friends")
ok([x["person"]["login"] for x in j.get("friends", [])] == ["Boris"], "Anna's friends: Boris")
ok(j["friends"][0].get("presence", {}).get("online") == "site", "with where he is (on the site)")
st, n = a.call("GET", "/api/notifications")
ok(any(x["kind"] == "friend" and x["data"].get("accepted") for x in n.get("items", [])), "Anna hears he accepted")
# A request both ways is a friendship at once.
m.call("POST", "/api/friends", {"login": "Anna"})
st, j = a.call("POST", "/api/friends", {"login": "Mila"})
ok(j.get("state") == "friends", "a request answered by a request is a friendship")
st, p = a.call("GET", "/api/u/boris")
ok(p.get("relation", {}).get("friends") is True, "a profile says they are friends")
# Presence goes to friends, as much as each lets be seen.
a.forget()
b.call("PATCH", "/api/me/profile", {"privacy": {"listening": False}})
z.call("POST", "/api/friends", {"login": "Anna"})
z.call("DELETE", "/api/friends/Anna")
st, j = a.call("GET", "/api/friends")
ok(not j.get("incoming"), "a request taken back is gone")

print("== blocks")
st, j = z.call("POST", "/api/friends", {"login": "Boris"})
st, j = b.call("POST", "/api/blocks", {"login": "Zed"})
ok(st == 200, "Boris blocks Zed")
st, j = b.call("GET", "/api/friends")
ok(not j.get("incoming") and [x["person"]["login"] for x in j.get("blocked", [])] == ["Zed"], "Zed's request is gone, Zed is in Boris's blocked")
st, j = z.call("POST", "/api/friends", {"login": "Boris"})
ok(st == 403 and j.get("error") == "blocked", "Zed can't ask again")
st, j = b.call("POST", "/api/blocks", {"login": "Lunuca"})
ok(st == 400 and j.get("error") == "staff", "staff can't be blocked")

print("== direct conversations")
st, j = z.call("POST", "/api/chats/dm", {"login": "Anna"})
ok(st == 403 and j.get("error") == "friends", f"Anna takes messages from friends only ({st} {j})")
st, j = z.call("POST", "/api/chats/dm", {"login": "Boris"})
ok(st == 403 and j.get("error") == "blocked", "Boris blocked Zed")
st, j = her.call("POST", "/api/chats/dm", {"login": "Zed"})
ok(st == 200, "staff may write to anyone")
st, j = a.call("POST", "/api/chats/dm", {"login": "Boris"})
ok(st == 200 and j["chat"]["kind"] == "dm" and j["chat"]["other"]["login"] == "Boris", "Anna opens a conversation with Boris")
dm = j["chat"]["id"]
st, j2 = b.call("POST", "/api/chats/dm", {"login": "Anna"})
ok(j2["chat"]["id"] == dm, "the same one from his side")
a.forget(); b.forget()
st, j = a.call("POST", f"/api/chats/{dm}/messages", {"body": "  привет!\r\nкак ты  "})
ok(st == 201 and j["message"]["body"] == "привет!\nкак ты", "a message, cleaned")
first = j["message"]["id"]
e = b.wait("msg", lambda e: e["chat"] == dm)
ok(e is not None and e["message"]["body"] == "привет!\nкак ты" and e.get("game") is True and e.get("push") is True, "Boris gets it live, to be shown in the game and on Windows")
ok(e is not None and e.get("unread") == 1, "with his unread count")
e = a.wait("msg", lambda e: e["chat"] == dm)
ok(e is not None and e.get("game") is False, "Anna's own pages get it quietly")
st, l = b.call("GET", "/api/chats")
ok(l.get("unread") == 1 and l["chats"][0]["unread"] == 1 and l["chats"][0]["last"]["text"].startswith("привет!"), f"Boris's list: one unread, the last line ({l.get('unread')} {[(x['unread'], x['last']) for x in l.get('chats', [])]})")
st, j = b.call("POST", f"/api/chats/{dm}/messages", {"body": "норм", "reply_to": first})
ok(st == 201 and j["message"]["reply"]["id"] == first and j["message"]["reply"]["author"] == "Anna", "a reply quotes what it answers")
reply = j["message"]["id"]
st, j = b.call("POST", f"/api/chats/{dm}/read", {"upto": reply})
e = a.wait("read", lambda e: e["chat"] == dm and e["uid"] != None)
ok(e is not None and e["upto"] == reply, "Anna sees Boris has read")
st, l = b.call("GET", "/api/chats")
ok(l.get("unread") == 0, "Boris has nothing unread")
st, j = a.call("POST", f"/api/chats/{dm}/typing")
ok(b.wait("typing", lambda e: e["chat"] == dm and e["login"] == "Anna") is not None, "Boris sees Anna typing")
st, j = a.call("POST", f"/api/chats/{dm}/messages/{reply}/react", {"emoji": "c_heart"})
e = b.wait("react", lambda e: e["id"] == reply)
ok(e is not None and e["reactions"] == [{"emoji": "c_heart", "uids": [e["reactions"][0]["uids"][0]]}], "a reaction, heard")
st, j = a.call("POST", f"/api/chats/{dm}/messages/{reply}/react", {"emoji": "😀"})
ok(st == 400, "only the pack's emoji")
st, j = a.call("POST", f"/api/chats/{dm}/messages/{reply}/react", {"emoji": "c_heart"})
st, page = a.call("GET", f"/api/chats/{dm}")
ok([x["reactions"] for x in page["messages"] if x["id"] == reply] == [[]], "the same reaction again takes it back")
st, j = a.call("PATCH", f"/api/chats/{dm}/messages/{first}", {"body": "привет!!"})
e = b.wait("msg_edit", lambda e: e["message"]["id"] == first)
ok(st == 200 and e is not None and e["message"]["body"] == "привет!!" and e["message"]["edited"], "an edit, heard")
st, j = b.call("PATCH", f"/api/chats/{dm}/messages/{first}", {"body": "чужое"})
ok(st == 403, "not someone else's")
st, j = a.call("POST", f"/api/chats/{dm}/pin", {"message": first})
st, l = b.call("GET", "/api/chats")
ok(l["chats"][0]["pinned"]["id"] == first, "a pinned message, at the top")

print("== a picture and a voice message")
st, j = a.call("POST", f"/api/chats/{dm}/pictures", raw=png(320, 200))
ok(st == 201 and j["message"]["kind"] == "picture" and j["message"]["media"]["thumb"].startswith("/media/"), f"a picture ({st})")
pic = j["message"]["id"]
voice = ogg_opus(2.5)
st, j = a.call("POST", f"/api/chats/{dm}/voice", raw=voice)
ok(st == 201 and j["message"]["kind"] == "voice" and abs(j["message"]["media"]["ms"] - 2500) <= 20 and j["message"]["media"]["format"] == "ogg", f"a voice message, its length read from it ({st} {j.get('message', {}).get('media') if isinstance(j, dict) else j})")
st, j = a.call("POST", f"/api/chats/{dm}/voice", raw=b"\x1a\x45\xdf\xa3" + b"\x00" * 40 + b"webm" + b"\x00" * 100 + b"A_OPUS" + b"\x00" * 500)
ok(st == 400, "a WebM without its measured length is refused")
st, j = a.call("POST", f"/api/chats/{dm}/voice?ms=4000", raw=b"\x1a\x45\xdf\xa3" + b"\x00" * 40 + b"webm" + b"\x00" * 100 + b"A_OPUS" + b"\x00" * 500)
ok(st == 201 and j["message"]["media"]["ms"] == 4000 and j["message"]["media"]["format"] == "webm", "a WebM with it is taken")
st, j = a.call("POST", f"/api/chats/{dm}/voice", raw=b"RIFF" + b"\x00" * 100)
ok(st == 400, "nothing but Opus")
st, j = a.call("DELETE", f"/api/chats/{dm}/messages/{pic}")
e = b.wait("msg_del", lambda e: e["id"] == pic)
ok(st == 200 and e is not None, "a message taken away, heard")
st, page = b.call("GET", f"/api/chats/{dm}")
gone = [x for x in page["messages"] if x["id"] == pic][0]
ok(gone["deleted"] and gone["media"] is None, "its place stays, its picture is gone")
st, j = a.call("DELETE", f"/api/chats/{dm}/messages/{pic}")
ok(st == 404, "once")

print("== pages")
# Twelve messages in ten seconds each at most: the two of them take turns, and wait when they have to.
time.sleep(10.5)
for i in range(60):
    if i and i % 22 == 0:
        time.sleep(10.5)
    (a, b)[i % 2].call("POST", f"/api/chats/{dm}/messages", {"body": f"#{i}"})
st, page = b.call("GET", f"/api/chats/{dm}")
ok(len(page["messages"]) == 50 and page["more"] and page["messages"][-1]["body"] == "#59", "the newest fifty, oldest first")
st, older = b.call("GET", f"/api/chats/{dm}?before={page['messages'][0]['id']}")
ok(older["messages"] and older["messages"][-1]["id"] < page["messages"][0]["id"], "and the page before them")

print("== privacy of direct conversations")
m.call("PATCH", "/api/me/profile", {"privacy": {"dm": "none"}})
st, j = a.call("POST", "/api/chats/dm", {"login": "Mila"})
ok(st == 403 and j.get("error") == "closed", "Mila takes no messages, even from friends")
m.call("PATCH", "/api/me/profile", {"privacy": {"dm": "all"}})
st, j = z.call("POST", "/api/chats/dm", {"login": "Mila"})
ok(st == 200, "now she takes them from anyone")
mz = j["chat"]["id"]
st, j = z.call("POST", f"/api/chats/{mz}/messages", {"body": "hi"})
ok(st == 201, "Zed writes")
m.call("POST", "/api/blocks", {"login": "Zed"})
st, j = z.call("POST", f"/api/chats/{mz}/messages", {"body": "hi again"})
ok(st == 403 and j.get("error") == "blocked", "until she blocks him")

print("== groups")
st, j = a.call("POST", "/api/chats/groups", {"title": "Гриферы", "invite": ["Boris", "Mila", "Zed"]})
ok(st == 201 and j["chat"]["kind"] == "group" and j["chat"]["role"] == "owner", f"Anna makes a group ({st})")
g = j["chat"]["id"]
ok(b.wait("chats") is not None, "Boris hears of the invitation")
st, l = b.call("GET", "/api/chats")
ok([x["chat"] for x in l.get("invites", [])] == [g], "it waits in his list")
st, l = z.call("GET", "/api/chats")
ok(not l.get("invites"), "Zed, not Anna's friend, was not invited")
st, n = b.call("GET", "/api/notifications")
ok(any(x["kind"] == "group" and x["data"]["chat"] == g for x in n.get("items", [])), "and a notification of it")
st, j = z.call("POST", f"/api/chats/{g}/join")
ok(st == 403, "no joining without an invitation")
st, j = b.call("POST", f"/api/chats/{g}/join")
ok(st == 200 and j["chat"]["role"] == "member", "Boris joins")
st, j = m.call("DELETE", f"/api/chats/{g}/invites/me")
st, l = m.call("GET", "/api/chats")
ok(not l.get("invites"), "Mila turns it down")
st, j = a.call("POST", f"/api/chats/{g}/link", {"on": True})
code = j.get("link")
ok(st == 200 and code, "a link")
st, j = z.call("GET", f"/api/chats/link/{code}")
ok(j.get("title") == "Гриферы" and j.get("members") == 2 and j.get("in") is False, "Zed sees where it leads")
st, j = z.call("POST", f"/api/chats/link/{code}")
ok(st == 200, "and comes in by it")
st, page = a.call("GET", f"/api/chats/{g}")
sys_lines = [x["body"]["what"] for x in page["messages"] if x["kind"] == "system"]
ok(sys_lines == ["created", "joined", "joined"], f"the history says what happened ({sys_lines})")
ok([p["login"] for p in page["people"]] == ["Anna", "Boris", "Zed"] and page["people"][0]["role"] == "owner", "who is in it, the owner first")
st, j = b.call("POST", f"/api/chats/{g}/messages", {"body": "всем привет"})
ok(st == 201 and z.wait("msg", lambda e: e["chat"] == g and e["message"]["body"] == "всем привет") is not None, "a message reaches everyone")
st, j = b.call("POST", f"/api/chats/{g}/invites", {"login": "Mila"})
ok(st == 403, "a member doesn't invite")
st, j = a.call("POST", f"/api/chats/{g}/members/Boris/role", {"admin": True})
ok(st == 200, "Anna makes Boris an admin")
st, j = b.call("POST", f"/api/chats/{g}/invites", {"login": "Mila"})
ok(st == 200, "now he may invite")
st, j = b.call("PATCH", f"/api/chats/{g}", {"title": "Гриферы 2"})
ok(st == 200 and z.wait("chat", lambda e: e["chat"]["title"] == "Гриферы 2") is not None, "an admin renames it; everyone hears")
st, j = z.call("PATCH", f"/api/chats/{g}", {"muted": True})
z.forget()
b.call("POST", f"/api/chats/{g}/messages", {"body": "тихо"})
e = z.wait("msg", lambda e: e["message"]["body"] == "тихо")
ok(e is not None and e["game"] is False and e["push"] is False, "a muted group comes quietly")
st, j = b.call("DELETE", f"/api/chats/{g}/members/Anna")
ok(st == 403, "an admin can't remove the owner")
st, j = b.call("DELETE", f"/api/chats/{g}/members/Zed")
ok(st == 200, "an admin removes a member")
st, j = z.call("GET", f"/api/chats/{g}")
ok(st == 404, "who is out sees nothing of it")
st, j = a.call("DELETE", f"/api/chats/{g}/members/me")
st, page = b.call("GET", f"/api/chats/{g}")
ok(page["chat"]["role"] == "owner", "the owner left: the group is the admin's")
st, j = b.call("POST", f"/api/chats/{g}/avatar", raw=png(128, 128))
ok(st == 200, "a group's picture")
st, l = b.call("GET", "/api/chats")
ok([x for x in l["chats"] if x["id"] == g][0]["avatar"]["url"].startswith("/media/"), "shown in the list")

print("== reports")
st, j = b.call("POST", f"/api/chats/{dm}/messages/{first}/report", {"why": "оскорбления"})
ok(st == 201, "Boris reports Anna's message")
st, j = b.call("POST", f"/api/chats/{dm}/messages/{reply}/report", {"why": "мой"})
ok(st == 400, "not one's own")
st, j = m.call("GET", "/api/staff/chat-reports")
ok(st == 403, "only staff see reports")
st, j = her.call("GET", "/api/staff/chat-reports")
r = j["reports"][0]
ok(r["why"] == "оскорбления" and r["snapshot"][-1]["id"] == first and r["accused"]["login"] == "Anna", "staff see the message and what came before")
st, j = her.call("GET", f"/api/chats/{dm}")
ok(st == 404, "but not the conversation itself")
st, j = her.call("POST", f"/api/staff/chat-reports/{r['id']}", {"outcome": "hide"})
ok(st == 200, "the report is answered: hidden")
st, page = a.call("GET", f"/api/chats/{dm}")
ok([x for x in page["messages"] if x["id"] == first] == [] or [x for x in page["messages"] if x["id"] == first][0]["deleted"], "the message is gone")

print("== badges")
st, p = a.call("GET", "/api/u/anna")
ok(p.get("stats", {}) is not None, "a profile answers")

print("\n" + ("ALL OK" if not failures else f"{len(failures)} FAILED: " + "; ".join(failures)))
sys.exit(1 if failures else 0)
