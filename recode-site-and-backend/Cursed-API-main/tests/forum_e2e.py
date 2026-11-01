"""forum_e2e.py: the forum on the staging instance, as members use it (run on the server after a reset).

Threads and replies, the flood limits, reading marks, editing and taking back, what she can do (announcements, pin,
lock, move, hide), what others then see, search, pages, who is online, and reports (of posts and packs) through to
their closing.
"""
import http.cookiejar
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

BASE = "http://127.0.0.1:8088"
failures = []


def ok(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        failures.append(what)


class Member:
    def __init__(self, login):
        self.login = login
        self.op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def call(self, method, path, body=None, origin=BASE):
        h = {"Origin": origin} if origin else {}
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


her = Member("Lunuca").register(os.popen("/root/staging.sh cli key").read().strip())
os.system("/root/staging.sh cli renumber 1 0 >/dev/null")
a, b, c = Member("Anna").register(), Member("Boris").register(), Member("Chris").register()

print("== the front")
st, f = a.call("GET", "/api/forum")
ok(st == 200 and [x["id"] for x in f["categories"]] == ["announcements", "bugs", "ideas", "help", "market", "showcase", "making", "teams", "offtopic"], "nine categories, in order")
ok(any(o["login"] == "Anna" for o in f["online"]), "she who asked is online")
st, _ = Member("Nobody").call("GET", "/api/forum")
ok(st == 401, "closed without an account")

print("== threads and replies")
st, r = a.call("POST", "/api/forum/threads", {"category": "announcements", "title": "Hello all", "body": "x"})
ok(st == 403, "announcements are hers")
st, r = a.call("POST", "/api/forum/threads", {"category": "help", "title": "ab", "body": "x"})
ok(st == 400, "a title too short")
st, r = a.call("POST", "/api/forum/threads", {"category": "help", "title": "Shader runs at 20 fps", "body": "   "})
ok(st == 400, "an empty post")
st, t1 = a.call("POST", "/api/forum/threads", {"category": "help", "title": "Шейдер 20 фпс на 3060??", "body": "Complementary, 1.21.11.\n\n> quote\nWhat do I turn off?"})
ok(st == 201, "a thread")
st, r = a.call("POST", "/api/forum/threads", {"category": "help", "title": "Another one right away", "body": "x"})
ok(st == 429, "not a second thread right away")
st, ann = her.call("POST", "/api/forum/threads", {"category": "announcements", "title": "0.1.0 is out", "body": "Notes."})
ok(st == 201, "her announcement")
st, r = b.call("POST", f"/api/forum/threads/{t1['id']}/posts", {"body": "Turn off volumetric light."})
ok(st == 201 and r["page"] == 1, "a reply")
reply1 = r["id"]
st, r = b.call("POST", f"/api/forum/threads/{t1['id']}/posts", {"body": "And shadows."})
ok(st == 429, "not two replies within 8 seconds")
st, r = c.call("POST", f"/api/forum/threads/{t1['id']}/posts", {"body": "Same here @Anna"})
reply2 = r["id"]

print("== reading")
st, f = a.call("GET", "/api/forum")
latest = {x["id"]: x for x in f["latest"]}
ok(latest[t1["id"]]["unread"] and latest[t1["id"]]["replies"] == 2 and latest[t1["id"]]["last"]["by"]["login"] == "Chris", "unread for its author after replies; two replies; last by Chris")
st, t = a.call("GET", f"/api/forum/threads/{t1['id']}")
ok(st == 200 and len(t["posts"]) == 3 and [p["new"] for p in t["posts"]] == [False, True, True], "the new posts are marked new")
st, f = a.call("GET", "/api/forum")
ok(not {x["id"]: x for x in f["latest"]}[t1["id"]]["unread"], "read after opening")

print("== editing and taking back")
st, _ = b.call("PATCH", f"/api/forum/posts/{reply1}", {"body": "Turn off volumetric light and clouds."})
st2, _ = a.call("PATCH", f"/api/forum/posts/{reply1}", {"body": "mine now"})
st3, t = c.call("GET", f"/api/forum/threads/{t1['id']}")
ok(st == 200 and st2 == 404 and t["posts"][1]["edited"] and "clouds" in t["posts"][1]["body"], "edited by its author only, marked edited")
first = t["posts"][0]["id"]
st, r = a.call("DELETE", f"/api/forum/posts/{first}")
ok(st == 409, "the opening post of a thread with replies stays")
st, _ = c.call("DELETE", f"/api/forum/posts/{reply2}")
st2, t = a.call("GET", f"/api/forum/threads/{t1['id']}")
ok(st == 200 and t["posts"][2]["state"] == "deleted" and t["posts"][2]["body"] == "" and t["replies"] == 1, "a reply taken back: its place stays, its words go, one reply left")
st, lone = c.call("POST", "/api/forum/threads", {"category": "offtopic", "title": "Delete me later", "body": "x"})
st, _ = c.call("DELETE", f"/api/forum/posts/{c.call('GET', '/api/forum/threads/' + str(lone['id']))[1]['posts'][0]['id']}")
st2, _ = a.call("GET", f"/api/forum/threads/{lone['id']}")
ok(st == 200 and st2 == 404, "a thread with nothing but its opening post goes with it")

print("== her moderation")
st, _ = a.call("PATCH", f"/api/forum/threads/{t1['id']}", {"locked": True})
ok(st == 403, "only she locks")
her.call("PATCH", f"/api/forum/threads/{t1['id']}", {"locked": True, "pinned": True})
st, r = b.call("POST", f"/api/forum/threads/{t1['id']}/posts", {"body": "late"})
ok(st == 403, "a locked thread takes no replies")
st, r = her.call("POST", f"/api/forum/threads/{t1['id']}/posts", {"body": "Closed: see the pinned guide."})
ok(st == 201, "but hers")
time.sleep(0.3)
st, filler = b.call("POST", "/api/forum/threads", {"category": "help", "title": "A newer thread", "body": "x"})
st, r = a.call("GET", "/api/forum/threads?cat=help")
ok(r["items"][0]["id"] == t1["id"] and r["items"][0]["pinned"], "pinned first in its category")
st, _ = a.call("PATCH", f"/api/forum/threads/{t1['id']}", {"title": "Шейдер 20 фпс на 3060 (solved)"})
ok(st == 200, "its author renames it")
her.call("PATCH", f"/api/forum/threads/{filler['id']}", {"category": "offtopic"})
st, r = a.call("GET", "/api/forum/threads?cat=offtopic")
ok(any(x["id"] == filler["id"] for x in r["items"]), "moved to off topic")
her.call("POST", f"/api/forum/posts/{reply1}/hide", {"why": "Spoilers"})
st, t = c.call("GET", f"/api/forum/threads/{t1['id']}")
st2, t2 = b.call("GET", f"/api/forum/threads/{t1['id']}")
ok(t["posts"][1]["state"] == "hidden" and t["posts"][1]["body"] == "" and t2["posts"][1]["body"] and t2["posts"][1]["hidden_why"] == "Spoilers", "a hidden post: nothing for others, its words and why for its author")
her.call("POST", f"/api/forum/posts/{reply1}/unhide")
her.call("POST", f"/api/forum/threads/{filler['id']}/hide", {"why": "Duplicate"})
st, _ = a.call("GET", f"/api/forum/threads/{filler['id']}")
st2, t2 = b.call("GET", f"/api/forum/threads/{filler['id']}")
ok(st == 404 and st2 == 200 and t2["hidden_why"] == "Duplicate", "a hidden thread: gone for others, its author sees why")

print("== search, pages, online")
st, r = c.call("GET", "/api/forum/threads?q=" + urllib.parse.quote("фпс"))
ok([x["id"] for x in r["items"]] == [t1["id"]], "found by a Russian word")
for i in range(40):
    her.call("POST", f"/api/forum/threads/{ann['id']}/posts", {"body": f"note {i}"})
st, t = a.call("GET", f"/api/forum/threads/{ann['id']}?page=2")
ok(t["pages"] == 2 and len(t["posts"]) == 11 and t["page"] == 2, f"41 posts: two pages ({t.get('pages')}, {len(t.get('posts', []))} on the second)")
st, t = b.call("GET", f"/api/forum/threads/{ann['id']}")
ok(t["page"] == 1, "a first visit opens the first page")
st, t = b.call("GET", f"/api/forum/threads/{ann['id']}?page=2")
her.call("POST", f"/api/forum/threads/{ann['id']}/posts", {"body": "one more"})
st, t = b.call("GET", f"/api/forum/threads/{ann['id']}")
ok(t["page"] == 2 and t["posts"][-1]["new"] and not t["posts"][0]["new"], "back later: the page with what is new")
st, f = her.call("GET", "/api/forum")
ok({"Anna", "Boris", "Chris", "Lunuca"} <= {o["login"] for o in f["online"]}, "everyone active is online")

print("== reports")
st, r = a.call("POST", "/api/reports", {"kind": "post", "id": reply1, "why": "rude"})
st2, _ = a.call("POST", "/api/reports", {"kind": "post", "id": reply1, "why": "rude again"})
ok(st == 201 and st2 == 409, "a report, once")
st, _ = a.call("POST", "/api/reports", {"kind": "post", "id": 999999, "why": "nothing"})
ok(st == 404, "nothing to report")
st, _ = a.call("GET", "/api/reports")
ok(st == 403, "only she reads reports")
st, rs = her.call("GET", "/api/reports")
rep = rs["items"][0]
ok(rep["kind"] == "post" and rep["about"]["author"]["login"] == "Boris" and "volumetric" in rep["about"]["text"], "her list says what the report is about")
st, _ = her.call("POST", f"/api/reports/{rep['id']}/close", {"action": "hide", "why": "Rude"})
st2, t = c.call("GET", f"/api/forum/threads/{t1['id']}")
st3, rs = her.call("GET", "/api/reports")
ok(st == 200 and t["posts"][1]["state"] == "hidden" and not rs["items"], "closed by hiding the post; nothing open left")

print()
print("FAILURES:" if failures else "everything as it should be", *failures, sep="\n  ")
sys.exit(1 if failures else 0)
