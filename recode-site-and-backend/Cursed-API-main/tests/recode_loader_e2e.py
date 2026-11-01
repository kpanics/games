#!/usr/bin/env python3
"""recode_loader_e2e.py: Recode's loader and game end to end, on an instance of its own (an empty database in a
temporary folder, port 8093): the loader signed in through the site (its code allowed or denied, only its own key takes
the answer, once), its start (no build yet; a build of Recode's channel; another computer, freed once a day; no Recode;
hers), the two tickets of a start (the pair's, and one start's own), the pair cut for the player out of the release's
master (both files of one cut, under one frame, for one window), the game's ticket (once, its build only), its check
every 30 minutes (a key revoked: no), the site's download of the loader (only with Recode). With CURSED_SEAL_KEY set
the loader's and the game's words go sealed and their answers come sealed (seal.rs); without it this file speaks as it
always did. The cutter beside the master stands in for protect-pack's mint (the seal itself is that program's own job,
Cursed Protect's chain tests); it stamps the pair so the test can see which cut was asked for. With CURSED_REAL_MASTER
naming a folder that holds a real master and CURSED_REAL_PACK a real protect-pack, the pair comes back cut by the real
packer and the stamps are not read — the whole seal, end to end. Run on the server as
root: python3 tests/recode_loader_e2e.py"""
import hashlib
import http.cookiejar
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

BIN = os.environ.get("CURSED_API_BIN", "/root/build/cursed-api/target/release/cursed-api")
PORT = 8093
BASE = f"http://127.0.0.1:{PORT}"
failures = []
tmp = tempfile.mkdtemp(prefix="recode-loader-e2e-")
ENV = dict(os.environ, CURSED_DB=f"{tmp}/cursed.db", CURSED_DIST=f"{tmp}/dist", CURSED_MEDIA=f"{tmp}/media",
           CURSED_LISTEN=f"127.0.0.1:{PORT}", CURSED_ORIGIN=BASE, CURSED_SECURE_COOKIE="0")


def cli(*args):
    r = subprocess.run([BIN, *args], env=ENV, capture_output=True, text=True)
    return (r.stdout + r.stderr).strip()


def ok(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        failures.append(what)


def h64(s):
    return hashlib.sha256(s.encode()).hexdigest()


D1 = {"win": h64("w1"), "cpu": h64("c1"), "disk": h64("d1"), "board": h64("b1"), "tpm": h64("t1")}
D2 = {"win": h64("w2"), "cpu": h64("c2"), "disk": h64("d2"), "board": h64("b2"), "tpm": h64("t2")}
D3 = {"win": h64("w3"), "cpu": h64("c3"), "disk": h64("d3"), "board": h64("b3"), "tpm": h64("t3")}
D4 = {"win": h64("w4"), "cpu": h64("c4"), "disk": h64("d4"), "board": h64("b4"), "tpm": h64("t4")}
D5 = {"win": h64("w5"), "cpu": h64("c5"), "disk": h64("d5"), "board": h64("b5"), "tpm": h64("t5")}
D_STAFF = {"win": h64("w-staff"), "cpu": h64("c-staff"), "disk": h64("d-staff"), "board": h64("b-staff"), "tpm": h64("t-staff")}
NO_TPM = {k: v for k, v in D1.items() if k != "tpm"}

# The seal on the loader's and the game's words (server/src/seal.rs, the client's native/src/net.rs): what they say
# is a letter under the build's word and this computer's mark, tied to the door it goes to and the minute it was
# written in. Here it is spoken only when the instance keeps one — with no CURSED_SEAL_KEY the words go as they
# always have, and so do the answers. The mark is a diversifier and nothing more: the server takes whatever mark a
# letter carries and opens it under the word and that mark together, so a mark of ours will do. Refusals come back
# as plain words either way, and the stub of a start is a library, not words at all.
SEAL = os.environ.get("CURSED_SEAL_KEY", "")
MARK = hashlib.sha256(b"cursed-seal-mark|e2e").hexdigest()
SEALED = frozenset({
    "/api/loader/link", "/api/loader/link/poll", "/api/loader/recode/start", "/api/loader/recode/stub",
    "/api/loader/recode/device/reset", "/api/client/recode/verify", "/api/client/recode/alive",
    "/api/client/recode/check", "/api/client/recode/bye", "/api/client/recode/sight",
})


def seal_key(mark):
    return hashlib.sha256(b"cursed-seal/v1|" + bytes.fromhex(SEAL) + b"|" + mark.encode()).digest()


def letter(method, path, body):
    """These words, as a letter to that door. A letter answers nothing (`asked` is empty): the answer will name it."""
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM
    nonce, at = os.urandom(12), int(time.time())
    n = nonce.hex()
    tied = f"{method}\n{path}\n{at}\n{n}\n".encode()
    sealed = AESGCM(seal_key(MARK)).encrypt(nonce, json.dumps(body).encode(), tied)
    return {"v": 2, "k": MARK, "n": n, "t": at, "c": sealed.hex()}, n


def open_letter(method, path, env, asked):
    """Their answer, opened: only a letter of theirs, and only the answer to this one's own question."""
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM
    if env.get("r", "") != asked:
        raise ValueError("an answer to another letter")
    n = env["n"]
    tied = f"{method}\n{path}\n{env['t']}\n{n}\n{asked}".encode()
    plain = AESGCM(seal_key(env["k"])).decrypt(bytes.fromhex(n), bytes.fromhex(env["c"]), tied)
    return json.loads(plain)


def seal_body(method, path, body):
    """What goes asking, and the letter's own words so the answer can name them (empty when nothing was sealed)."""
    if SEAL and path in SEALED:
        envelope, asked = letter(method, path, body)
        return envelope, asked
    return body, ""


def open_body(method, path, body, went, asked):
    return open_letter(method, path, body, asked) if went else body



def raw(method, path, body=None, token=None, origin=None):
    """The loader and the game: no cookie, no Origin; a token when signed in."""
    h = {}
    data = None
    asked = ""
    went = bool(SEAL) and path in SEALED and body is not None
    if body is not None:
        sealed, asked = seal_body(method, path, body)
        data, h["Content-Type"] = json.dumps(sealed).encode(), "application/json"
    if token:
        h["Authorization"] = "Bearer " + token
    if origin:
        h["Origin"] = origin
    req = urllib.request.Request(BASE + path, data=data, method=method, headers=h)
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            b = r.read()
            return r.status, open_body(method, path, (json.loads(b) if b else {}), went and r.status < 300, asked), {k.lower(): v for k, v in r.headers.items()}
    except urllib.error.HTTPError as e:
        b = e.read()
        try:
            # a refusal is the same plain words to whoever reads it
            return e.code, json.loads(b), {}
        except Exception:
            return e.code, {}, {}


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
            with self.op.open(req, timeout=30) as r:
                b = r.read()
                hd = {k.lower(): v for k, v in r.headers.items()}
                try:
                    return r.status, (json.loads(b) if b else {}), hd
                except Exception:
                    return r.status, {}, hd
        except urllib.error.HTTPError as e:
            b = e.read()
            try:
                return e.code, json.loads(b), {}
            except Exception:
                return e.code, {}, {}

    def register(self, key=""):
        st, j, _ = self.call("POST", "/api/register", {"login": self.login, "email": self.login.lower() + "@e.st", "password": "0123456789", "key": key})
        assert st == 200, (st, j)
        return self


def link(member, device=D1, allow=True):
    """The loader asks, the member answers on the site; the loader's token (or the answer it got)."""
    st, j, _ = raw("POST", "/api/loader/link", {"device": device})
    assert st == 200, (st, j)
    member.call("POST", f"/api/loader/link/{j['code']}/{'allow' if allow else 'deny'}")
    st, p, _ = raw("POST", "/api/loader/link/poll", {"code": j["code"], "key": j["key"]})
    return st, p


def stage(name, files, seal=False):
    """A build waiting in incoming/<name>, as tools/Release.java leaves it. `seal`: this build is under one —
    its library is not one of its files and never was; the server cuts a player their own at the start."""
    d = f"{tmp}/dist/incoming/{name}"
    os.makedirs(d + "/objects", exist_ok=True)
    listed = []
    for path, kind, content in files:
        sha = hashlib.sha256(content).hexdigest()
        open(f"{d}/objects/{sha}", "wb").write(content)
        listed.append({"path": path, "sha256": sha, "size": len(content), "kind": kind})
    made = {"version": "0.1.0-recode.1", "notes": "test", "launch": {"main": "x"}, "files": listed}
    if seal:
        made["seal"] = True
    json.dump(made, open(d + "/release.json", "w"))
    return {f["path"]: f["sha256"] for f in listed}


def cutter_at(path):
    """The cutter the release brings, in its protect folder. With CURSED_REAL_PACK naming a real protect-pack
    (the Linux one the release ships), that binary is placed here and the pair the server hands out is a real
    one — the whole seal, end to end. Returns whether this is the stand-in.

    The stand-in is `protect-pack`'s shape and no more than its shape: `mint <stub> <meta> <bin> <out>
    --ticket-file <path> --keep-days <n> --uid <whose>`, and in the out folder a library and a seal — `MZ`
    is the stub's word, `CRB1` the seal's (protect-container's). The seal itself is that program's own job
    (Cursed Protect's chain tests cut and open a real one) — this is a stand-in, and it says so in every
    line. What it does do is honour the card (the halves come from the offsets the meta names) and stamp the
    pair with the ticket it was cut under, the days it was given and whose it is, so a test can see the
    server asked for the right cut. An account is not optional: a pair cut for nobody is not a pair anybody
    is answerable for, and the real packer refuses one (`mint needs --uid`)."""
    real = os.environ.get("CURSED_REAL_PACK")
    if real:
        shutil.copyfile(real, path)
        os.chmod(path, 0o755)
        return False
    open(path, "w", newline="\n").write(r'''#!/usr/bin/env python3
"""protect-pack, standing in. Not the seal: that is protect-pack's own job."""
import hashlib
import os
import sys

a = sys.argv[1:]
if not a or a[0] != "mint":
    sys.exit("protect-pack: only mint")
if len(a) < 5:
    sys.exit("protect-pack: mint <stub> <meta> <bin> <out>")
stub_at, meta_at, bin_at, out = a[1], a[2], a[3], a[4]
rest, ticket, days, uid = a[5:], None, None, None
while rest:
    if rest[0] == "--ticket-file" and len(rest) > 1:
        ticket = open(rest[1], "rb").read().rstrip(b"\r\n").decode()
        rest = rest[2:]
    elif rest[0] == "--keep-days" and len(rest) > 1:
        days = int(rest[1])
        rest = rest[2:]
    elif rest[0] == "--uid" and len(rest) > 1:
        uid = int(rest[1])
        rest = rest[2:]
    else:
        sys.exit("protect-pack: unknown " + " ".join(rest))
for at in (stub_at, meta_at, bin_at):
    if not os.path.isfile(at):
        sys.exit("protect-pack: no master: " + at)
if not ticket:
    sys.exit("protect-pack: no ticket")
if days is None:
    sys.exit("protect-pack: no keep-days")
if uid is None:
    sys.exit("protect-pack: mint needs --uid <whose pair this is>")
stub, seal = open(stub_at, "rb").read(), open(bin_at, "rb").read()
if len(stub) < 2 or stub[:2] != b"MZ":
    sys.exit("protect-pack: the master stub is not a library")
if len(seal) < 64 or seal[:4] != b"CRB1":
    sys.exit("protect-pack: the master seal is not one")
a_at = b_at = None
for line in open(meta_at).read().splitlines():
    if line.startswith("half_a_at="):
        a_at = int(line.split("=", 1)[1])
    elif line.startswith("half_b_at="):
        b_at = int(line.split("=", 1)[1])
if a_at is None or b_at is None or a_at + 32 > len(stub) or b_at + 32 > len(stub):
    sys.exit("protect-pack: the card says nothing of the halves")
os.makedirs(out, exist_ok=True)
# one cut of one ticket: halves of their own, a seal of their own, the days they were given and whose it
# is, stamped where a test can read them. The seal's head is left as the packer wrote it — its word and its
# body's length are what the server checks before it hands the pair over. The master's own words go on as
# they were.
own = hashlib.sha256(ticket.encode()).digest()
other = hashlib.sha256(b"seal|" + ticket.encode()).digest()
days_b = days.to_bytes(8, "little")
uid_b = uid.to_bytes(8, "little")
cut_stub, cut_seal = bytearray(stub), bytearray(seal)
cut_stub[2:34], cut_stub[34:42], cut_stub[42:50] = own, days_b, uid_b
cut_seal[64:96], cut_seal[96:104] = other, days_b
cut_stub[a_at:a_at + 32] = hashlib.sha256(b"a|" + ticket.encode()).digest()
cut_stub[b_at:b_at + 32] = hashlib.sha256(b"b|" + ticket.encode()).digest()
open(os.path.join(out, "cursed_recode.dll"), "wb").write(bytes(cut_stub))
open(os.path.join(out, "cursed_recode.bin"), "wb").write(bytes(cut_seal))
print("protect-pack: the pair is cut (account %d is written into the stub; the mapped game is marked with it)" % uid)
'''.replace("\r\n", "\n"))
    os.chmod(path, 0o755)
    return True


def fake_seal(body=8192):
    """A seal in the packer's own shape — its word, its version, a body of exactly the length the head
    names (protect-container's `magic`/`HEADER`) — sealed under nothing. The seal is protect-pack's job;
    what is here is the shape the server checks before it hands a pair over."""
    head = bytearray(64)
    head[0:4] = b"CRB1"
    head[4:8] = (2).to_bytes(4, "little")
    head[8:16] = body.to_bytes(8, "little")
    return bytes(head) + bytes(body)


def pair(name):
    """A master beside a release's files — as `protect-pack` leaves one (the halves in their slots of the
    stub, a card in the packer's own words) — and the cutter that turns it into one player's pair. All four
    go into the release's protect folder; none of them is a file of the store and nothing serves it.

    With CURSED_REAL_MASTER naming a folder that holds a real one (`protect-pack pack` writes
    cursed_recode.dll, cursed_recode.bin and master.meta), those three are used as they are — then the cut
    is the real packer's own, and nothing here is standing in for the seal.

    Answer: the master's two files and its card, where the card says the halves sit, and whether the cutter
    placed beside them is the stand-in (only then is there a stamp to read)."""
    d = f"{tmp}/dist/incoming/{name}/protect"
    os.makedirs(d, exist_ok=True)
    real = os.environ.get("CURSED_REAL_MASTER")
    if real:
        master = open(f"{real}/cursed_recode.dll", "rb").read()
        sealed = open(f"{real}/cursed_recode.bin", "rb").read()
        meta = open(f"{real}/master.meta", "rb").read()
    else:
        master = bytearray(b"MZ" + bytes(4096))
        half_a, half_b, mticket = os.urandom(32), os.urandom(32), b"master-ticket-of-this-release"
        master[128:160] = half_a
        master[256:288] = half_b
        sealed = fake_seal()
        meta = ("CRD_MASTER1\nhalf_a_at=128\nhalf_b_at=256\nticket=" + mticket.hex() + "\n").encode()
        master = bytes(master)
    open(d + "/cursed_recode.dll", "wb").write(master)
    open(d + "/cursed_recode.bin", "wb").write(sealed)
    open(d + "/master.meta", "wb").write(meta)
    stood = cutter_at(d + "/protect-pack")
    a_at = b_at = None
    for line in meta.decode().splitlines():
        if line.startswith("half_a_at="):
            a_at = int(line.split("=", 1)[1])
        elif line.startswith("half_b_at="):
            b_at = int(line.split("=", 1)[1])
    return {"stub": master, "bin": sealed, "meta": meta, "a_at": a_at, "b_at": b_at, "stood": stood}


def open_pair(frame):
    """A pair under its frame (protect::framed): the word, the two lengths, the two files."""
    assert frame[:8] == b"CRDPAIR1", frame[:8]
    n_stub = int.from_bytes(frame[8:12], "little")
    n_bin = int.from_bytes(frame[12:16], "little")
    assert 16 + n_stub + n_bin == len(frame), (n_stub, n_bin, len(frame))
    return frame[16:16 + n_stub], frame[16 + n_stub:]


def raw_bytes(method, path, body, token=None):
    """An answer that is not JSON: the stub of a start is a library. What goes asking is still words, and sealed."""
    h = {"Content-Type": "application/json"}
    if token:
        h["Authorization"] = "Bearer " + token
    # what goes asking is words and sealed; what comes back is the library itself, not words at all
    sealed, _ = seal_body(method, path, body)
    req = urllib.request.Request(BASE + path, data=json.dumps(sealed).encode(), method=method, headers=h)
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()


def raw_body(method, path, body, token=None):
    """A body sent as it is: a report from the game's own guard is words, not JSON."""
    h = {}
    if token:
        h["Authorization"] = "Bearer " + token
    req = urllib.request.Request(BASE + path, data=body.encode() if isinstance(body, str) else body, method=method, headers=h)
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()


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
    st, made, _ = her.call("POST", "/api/admin/keys", {"days": 30, "count": 3})
    keys = made["codes"]
    a.call("POST", "/api/recode/activate", {"key": keys[0]})

    print("== the loader signs in through the site")
    st, j, _ = raw("POST", "/api/loader/link", {"device": D1}, origin=BASE)
    ok(st == 403, f"not from a web page ({st})")
    st, j, _ = raw("POST", "/api/loader/link", {"device": {"win": "x"}})
    ok(st == 400, f"a computer described wrong ({st})")
    st, j, _ = raw("POST", "/api/loader/link", {"device": D1})
    ok(st == 200 and len(j["code"]) == 9 and j["code"][4] == "-" and len(j["key"]) == 64 and j["url"].endswith("/#/link/" + j["code"].replace("-", "")), f"a code, a key, the page ({j.get('code')} {j.get('url')})")
    code, key = j["code"], j["key"]
    st, s, _ = a.call("GET", f"/api/loader/link/{code.replace('-', '').lower()}")
    ok(st == 200 and s["state"] == "waiting" and s["login"] == "Anna" and s["recode"]["active"], f"the site sees it waiting, for Anna ({st} {s.get('state')})")
    st, p, _ = raw("POST", "/api/loader/link/poll", {"code": code, "key": key})
    ok(st == 202, f"not yet ({st})")
    st, x, _ = a.call("POST", f"/api/loader/link/{code}/allow")
    ok(st == 200, f"allowed on the site ({st})")
    st, p, _ = raw("POST", "/api/loader/link/poll", {"code": code, "key": "0" * 64})
    ok(st == 410, f"someone else's key takes nothing ({st})")
    st, p, _ = raw("POST", "/api/loader/link/poll", {"code": code, "key": key})
    ok(st == 200 and p["login"] == "Anna" and len(p["token"]) == 64, f"the loader's token ({st})")
    tok_a = p.get("token")
    st, p2, _ = raw("POST", "/api/loader/link/poll", {"code": code, "key": key})
    ok(st == 410, f"once only ({st})")
    st, p = link(b, allow=False)
    ok(st == 403 and p.get("error") == "link-denied", f"denied ({st} {p.get('error')})")

    print("== the loader starts")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D1}, token=tok_a)
    ok(st == 503 and j.get("error") == "no-release", f"no build yet ({st} {j.get('error')})")
    files = stage("r1", [("CursedRecode.exe", "loader", b"MZ the loader " * 100), ("game/recode.jar", "code", b"PK the game " * 100)])
    out = cli("release", "publish", "r1", "recode")
    ok("current one of recode" in out, f"a build published to recode ({out})")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": NO_TPM}, token=tok_a)
    ok(st == 200 and j.get("build"), f"no TPM: Recode all the same, the computer told by the rest (her word, 20.10) ({st} {j.get('error')})")
    # (her computer was bound with its TPM at the sign-in: switched off since, it goes on her list — «her rules» below)
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D1}, token=tok_a)
    m = json.loads(j.get("manifest", "{}"))
    ok(st == 200 and m.get("channel") == "recode" and j["build"] == m["build"] and len(j["ticket"]) == 64 and len(j["start_ticket"]) == 64 and j["ticket"] != j["start_ticket"] and j["recode"]["active"] and j["login"] == "Anna", f"the build, signed, the two tickets ({st})")
    build, ticket1 = j.get("build"), j.get("start_ticket")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D2}, token=tok_a)
    ok(st == 403 and j.get("error") == "other-device" and j.get("login") == "Anna", f"another computer ({st} {j.get('error')})")
    st, j, _ = raw("POST", "/api/loader/recode/device/reset", {"device": D2}, token=tok_a)
    ok(st == 200 and j["reset_from"] > time.time() + 86000, f"freed from the loader, this one bound ({st})")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D2}, token=tok_a)
    ok(st == 200, f"now it starts on it ({st})")
    ticket2 = j.get("start_ticket")

    print("== Cursed's files and Recode's apart (her word, 16.10)")
    def obj(sha, tok):
        return raw("GET", f"/api/dist/object/{sha}", token=tok)
    st, _, hd = obj(files["game/recode.jar"], tok_a)
    ok(st == 200 and "x-accel-redirect" in hd, f"Recode's loader: Recode's file ({st})")
    bfiles = stage("b1", [("cursed.jar", "code", b"PK cursed " * 100)])
    out = cli("release", "publish", "b1", "beta")
    ok("current one of beta" in out, f"a build of Cursed's beta ({out})")
    st, _, _ = obj(bfiles["cursed.jar"], tok_a)
    ok(st == 403, f"Recode's loader: not Cursed's file ({st})")
    dora = Member("Dora").register()
    cli("tester", "Dora")
    st, lb, _ = raw("POST", "/api/loader/login", {"login": "Dora", "password": "0123456789", "device": D5})
    tok_d = lb.get("token")
    ok(st == 200 and tok_d, f"Cursed's loader signs in, a tester ({st} {lb.get('error')})")
    st, j, _ = raw("POST", "/api/loader/start", {"device": D5, "channel": "recode"}, token=tok_d)
    ok(st == 403 and j.get("error") == "channel", f"Cursed's loader asks for Recode's channel: no ({st} {j.get('error')})")
    st, j, _ = raw("POST", "/api/loader/start", {"device": D5, "channel": "beta"}, token=tok_d)
    ok(st == 200, f"Cursed's loader starts its beta ({st} {j.get('error')})")
    st, _, _ = obj(bfiles["cursed.jar"], tok_d)
    ok(st == 200, f"Cursed's loader: Cursed's file ({st})")
    st, _, _ = obj(files["game/recode.jar"], tok_d)
    ok(st == 403, f"Cursed's loader: not Recode's file ({st})")
    st, _, _ = raw("GET", f"/api/dist/patch/{bfiles['cursed.jar']}/{files['game/recode.jar']}", token=tok_d)
    ok(st == 403, f"nor a patch into it ({st})")
    st, j, _ = raw("POST", "/api/loader/recode/device/reset", {"device": D1}, token=tok_a)
    ok(st == 429 and j.get("error") == "wait", f"once a day ({st})")
    st, pb = link(b)
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D1}, token=pb.get("token"))
    ok(st == 403 and j.get("error") == "no-recode" and j.get("login") == "Boris" and j.get("until") is None, f"no Recode, and whose ({st} {j.get('error')})")
    # her question 16.10: signed in without Recode, or not signed in, the loader downloads nothing of it
    st, _, _ = obj(files["game/recode.jar"], pb.get("token"))
    ok(st == 403, f"signed in without Recode: none of its files ({st})")
    st, _, _ = raw("GET", f"/api/dist/asset/{'0' * 40}", token=pb.get("token"))
    ok(st == 403, f"nor the game's assets ({st})")
    st, _, _ = raw("GET", f"/api/dist/patch/{bfiles['cursed.jar']}/{files['game/recode.jar']}", token=pb.get("token"))
    ok(st == 403, f"nor a patch into its files ({st})")
    st, _, _ = raw("GET", f"/api/dist/object/{files['game/recode.jar']}")
    ok(st == 401, f"not signed in: none at all ({st})")
    st, ph = link(her, device=D1)
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D1}, token=ph.get("token"))
    ok(st == 200, f"hers starts without a key, on any computer ({st})")

    print("== the game")
    st, j, _ = raw("POST", "/api/client/recode/verify", {"ticket": ticket1, "device": D2, "build": build})
    ok(st == 200 and len(j["token"]) == 64 and j["check_every"] == 1800 and j["recode"]["active"], f"the ticket of an earlier start, on its computer ({st} {j.get('error')})")
    game = j.get("token")
    st, j, _ = raw("POST", "/api/client/recode/verify", {"ticket": ticket1, "device": D2, "build": build})
    ok(st == 401, f"a ticket once ({st})")
    st, j, _ = raw("POST", "/api/client/recode/verify", {"ticket": ticket2, "device": D2, "build": build + 5})
    ok(st == 403 and j.get("error") == "outdated" and j.get("build") == build, f"only the current build ({st} {j.get('error')})")
    st, j, _ = raw("POST", "/api/client/recode/check", {"device": D2}, token=game)
    ok(st == 200 and j["ok"] and j["recode"]["active"], f"every 30 minutes: on ({st})")
    her.call("POST", f"/api/admin/keys/{keys[0]}/revoke")
    st, j, _ = raw("POST", "/api/client/recode/check", {"device": D2}, token=game)
    ok(st == 403 and j.get("error") == "no-recode" and j.get("until") is None, f"the only key revoked: no Recode, never had it now ({st} {j.get('error')})")
    st, j, _ = raw("POST", "/api/client/recode/check", {"device": D2}, origin=BASE, token=game)
    ok(st == 403 and j.get("error") == "origin", "not from a web page")

    print("== the site's download")
    st, j, hd = a.call("GET", "/api/dist/recode/loader")
    ok(st == 403 and j.get("error") == "no-recode", f"no Recode: no loader ({st} {j.get('error')})")
    a.call("POST", "/api/recode/activate", {"key": keys[1]})
    st, j, hd = a.call("GET", "/api/dist/recode/loader")
    ok(st == 200 and files["CursedRecode.exe"] in hd.get("x-accel-redirect", "") and "Cursed Recode.exe" in hd.get("content-disposition", ""), f"with Recode: the loader of the current build ({st} {hd.get('x-accel-redirect')})")
    st, j, hd = b.call("GET", "/api/dist/recode/loader")
    ok(st == 403, f"Boris: no ({st})")

    print("== a role gives no Recode; only she, and the console, give days (her word, 19.10)")
    staff = {}
    for login, role in (("Sam", "admin"), ("Mia", "moderator"), ("Sue", "support"), ("Pat", "partner"), ("Med", "media"), ("Ari", "ai")):
        m = Member(login).register()
        cli("role", login, role)
        st, p = link(m)
        staff[login] = (m, p.get("token"))
        st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D1}, token=p.get("token"))
        ok(st == 403 and j.get("error") == "no-recode", f"{role}: no Recode at the start ({st} {j.get('error')})")
        st, j, _ = m.call("GET", "/api/dist/recode/loader")
        ok(st == 403 and j.get("error") == "no-recode", f"{role}: no loader from the site ({st} {j.get('error')})")
        st, j, _ = m.call("GET", "/api/admin/keys")
        st2, j2, _ = m.call("POST", "/api/admin/keys", {"days": 30, "count": 1})
        ok(st == 403 and st2 == 403, f"{role}: neither sees nor makes keys ({st} {st2})")
        st, j, _ = m.call("POST", "/api/admin/users/Boris/recode", {"days": 30})
        st2, j2, _ = m.call("DELETE", "/api/admin/users/Anna/recode")
        ok(st == 403 and st2 == 403, f"{role}: neither gives days nor takes them ({st} {st2})")
    st, j, _ = staff["Sam"][0].call("GET", "/api/admin/users/Anna")
    ok(st == 200 and "recode" not in j, f"an admin's card of a member has no Recode on it ({st})")
    st, j, _ = her.call("GET", "/api/admin/users/Anna")
    ok(st == 200 and j.get("recode", {}).get("active") and j["recode"]["given"] == [], f"hers has: on, nothing given by hand ({st} {j.get('recode')})")
    st, j, _ = her.call("POST", "/api/admin/users/Mia/recode", {"days": 3})
    ok(st == 400 and j.get("error") == "term", f"a term not sold: no ({st} {j.get('error')})")
    st, j, _ = her.call("POST", "/api/admin/users/Nobody/recode", {"days": 7})
    ok(st == 404, f"nobody of that name ({st})")
    st, j, _ = her.call("POST", "/api/admin/users/Mia/recode", {"days": 7, "note": "test"})
    ok(st == 200 and j["active"] and not j["forever"] and abs(j["until"] - (time.time() + 7 * 86400)) < 10 and len(j["given"]) == 1
       and j["given"][0]["days"] == 7 and j["given"][0]["by"]["login"] == "Lunuca", f"she gives the moderator 7 days ({st} {j})")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D_STAFF}, token=staff["Mia"][1])
    ok(st == 200, f"with days given the moderator starts ({st} {j.get('error')})")
    st, j, hd = staff["Mia"][0].call("GET", "/api/dist/recode/loader")
    ok(st == 200, f"and downloads the loader ({st})")
    st, k, _ = her.call("GET", "/api/admin/keys?q=mia")
    ok(st == 200 and len(k["items"]) == 1 and k["items"][0]["used"]["login"] == "Mia" and k["batches"][0]["note"] == "выдано вручную · test", f"among her keys, a batch of its own ({k.get('batches', [{}])[0]})")
    st, j, _ = her.call("DELETE", "/api/admin/users/Mia/recode")
    ok(st == 200 and j["taken"] == 1 and not j["active"] and j["given"] == [], f"taken back: off ({st} {j})")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D_STAFF}, token=staff["Mia"][1])
    ok(st == 403 and j.get("error") == "no-recode", f"and she starts no more ({st} {j.get('error')})")
    out = cli("recode", "give", "Pat", "30", "partner")
    ok(out.startswith("Pat: Recode until"), f"the console gives ({out})")
    st, j, _ = staff["Pat"][0].call("GET", "/api/recode")
    ok(st == 200 and j["active"], "the partner has 30 days")
    out = json.loads(cli("recode", "of", "Pat"))
    ok(out["active"] and out["given"][0]["by"] is None and out["login"] == "Pat", f"the console reads it: given from the console ({out.get('given')})")
    out = cli("recode", "take", "Pat")
    ok(out.startswith("Pat: 1 given keys taken back; Recode off"), f"the console takes it back ({out})")
    out = cli("recode", "keys", "forever", "2", "giveaway")
    codes = [x for x in out.splitlines() if x.startswith("RCD-")]
    ok(len(codes) == 2 and "keys of forever" in out, f"the console makes keys ({out.splitlines()[0]})")
    st, j, _ = staff["Sue"][0].call("POST", "/api/recode/activate", {"key": codes[0]})
    ok(st == 200 and j["forever"], f"one of them activated by the support: forever ({st})")
    for bad in (("recode", "keys", "3", "1"), ("recode", "keys", "30", "0"), ("recode", "give", "Pat", "5"), ("recode", "give", "Nobody", "7")):
        r = subprocess.run([BIN, *bad], env=ENV, capture_output=True, text=True)
        ok(r.returncode != 0, f"the console refuses {' '.join(bad[1:])} ({(r.stdout + r.stderr).strip()})")
    st, jr, _ = her.call("GET", "/api/admin/journal?action=recode")
    acts = sorted({x["action"] for x in jr.get("items", [])})
    ok(st == 200 and {"recode.give", "recode.take", "recode.keys.make"} <= set(acts), f"all of it in the journal ({acts})")

    print("== one game at a time")
    st, j, _ = raw("POST", "/api/client/recode/bye", {}, token=game)
    ok(st == 200, "the old game said goodbye")
    def start_game(token, device):
        st, j, _ = raw("POST", "/api/loader/recode/start", {"device": device}, token=token)
        assert st == 200, (st, j)
        return raw("POST", "/api/client/recode/verify", {"ticket": j["start_ticket"], "device": device, "build": j["build"]})
    st, g1, _ = start_game(tok_a, D2)
    ok(st == 200 and g1.get("alive_every") == 60, f"a game ({st} {g1.get('error')})")
    st, j, _ = raw("POST", "/api/client/recode/alive", {"device": D2}, token=g1.get("token"))
    ok(st == 200 and j["ok"], f"the first is alive ({st})")
    st, g2, _ = start_game(tok_a, D2)
    ok(st == 200, f"a new start on the same computer takes over (the first fell, its goodbye unsaid) ({st} {g2.get('error')})")
    st, j, _ = raw("POST", "/api/client/recode/alive", {"device": D2}, token=g1.get("token"))
    ok(st == 403 and j.get("error") == "in-game", f"the old one, if it speaks again, is told another holds the account ({st} {j.get('error')})")
    st, j, _ = raw("POST", "/api/client/recode/alive", {"device": D2}, token=g2.get("token"))
    ok(st == 200 and j["ok"], f"the new one plays ({st})")
    # the account moved to another computer while the game plays (its day's free taken back through the database)
    import sqlite3
    db0 = sqlite3.connect(f"{tmp}/cursed.db")
    db0.execute("DELETE FROM device_resets WHERE uid = (SELECT uid FROM users WHERE login = 'Anna')")
    db0.commit()
    st, j, _ = raw("POST", "/api/loader/recode/device/reset", {"device": D4}, token=tok_a)
    ok(st == 200, f"Anna frees her computer for another ({st} {j.get('error')})")
    st, g3, _ = start_game(tok_a, D4)
    ok(st == 403 and g3.get("error") == "in-game", f"a second game from another computer: not let in, the first keeps it ({st} {g3.get('error')})")
    st, j, _ = raw("POST", "/api/client/recode/alive", {"device": D2}, token=g2.get("token"))
    ok(st == 403 and j.get("error") in ("other-device", "device-taken"), f"the first, its computer no longer the account's, is told so ({st} {j.get('error')})")
    raw("POST", "/api/client/recode/bye", {}, token=g2.get("token"))
    st, g4, _ = start_game(tok_a, D4)
    ok(st == 200, f"after it closed, a game starts on the new computer at once ({st} {g4.get('error')})")

    print("== her rules")
    st, c1 = link(Member("Carl").register())
    carl = c1.get("token")
    st, cm, _ = her.call("POST", "/api/admin/keys", {"days": 30, "count": 3})
    carl_m = Member("Carl")  # (signed in through the link only: a key through the site needs his session)
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D3}, token=carl)
    ok(st == 403 and j.get("error") == "no-recode", "Carl, no Recode yet")
    # Carl's Recode, given through the database (his site session is another member's object here)
    import sqlite3
    db = sqlite3.connect(f"{tmp}/cursed.db")
    carl_uid = db.execute("SELECT uid FROM users WHERE login = 'Carl'").fetchone()[0]
    db.execute("UPDATE recode_keys SET used_by = ?, used_login = 'Carl', used_at = ? WHERE code = ?", (carl_uid, int(time.time()), cm["codes"][0]))
    db.commit()
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D3}, token=carl)
    ok(st == 200, f"Carl plays on his own computer ({st} {j.get('error')})")
    d3b = dict(D3, tpm=h64("t3-reset"))
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": d3b}, token=carl)
    ok(st == 200, f"his TPM reset (the rest the same): he plays ({st})")
    d3_off = {k: v for k, v in D3.items() if k != "tpm"}
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": d3_off}, token=carl)
    ok(st == 200, f"his TPM switched off (the rest the same): he plays, her list hears of it ({st} {j.get('error')})")
    # Anna is banned; her computer (D4) with her
    print("  ", cli("ban", "Anna", "test"))
    st, j, _ = raw("POST", "/api/client/recode/alive", {"device": D4}, token=g4.get("token"))
    ok(st == 403 and j.get("error") == "banned" and j.get("reason") == "test", f"the game in play hears the ban at once ({st} {j.get('error')})")
    # Boris, with Recode, on Anna's computer: banned by the rules (by its TPM alone, too)
    a.call("POST", "/api/recode/activate", {"key": keys[2]})  # (Anna's session is gone: nothing)
    db.execute("UPDATE recode_keys SET used_by = (SELECT uid FROM users WHERE login = 'Boris'), used_login = 'Boris', used_at = ? WHERE code = ?", (int(time.time()), cm["codes"][1]))
    db.commit()
    only_tpm = dict(D1, tpm=D4["tpm"])
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": only_tpm}, token=pb.get("token"))
    ok(st == 403 and j.get("error") == "banned" and j.get("device") is False and "Обход бана" in j.get("reason", ""), f"another account on a banned computer (its TPM): banned itself ({st} {j.get('reason')})")
    # Fay on Anna's computer with its TPM switched off: the rest of it is Anna's, banned all the same
    st, dl = link(Member("Fay").register())
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": {k: v for k, v in D4.items() if k != "tpm"}}, token=dl.get("token"))
    ok(st == 403 and j.get("error") == "banned" and "Обход бана" in j.get("reason", ""), f"a banned computer with its TPM off: banned ({st} {j.get('reason')})")
    # Carl freed his computer often (two earlier this week, one now)
    for back in (3, 2):
        db.execute("INSERT INTO recode_resets (uid, at) VALUES (?, ?)", (carl_uid, int(time.time()) - back * 86400))
    db.commit()
    st, j, _ = raw("POST", "/api/loader/recode/device/reset", {"device": D3}, token=carl)
    ok(st == 200, f"Carl frees his computer ({st} {j.get('error')})")
    st, f, _ = a.call("GET", "/api/admin/recode/flags")
    ok(st in (401, 403), f"a member sees no list ({st})")
    st, f, _ = her.call("GET", "/api/admin/recode/flags")
    kinds = sorted((x["login"], x["kind"]) for x in f.get("items", []))
    ok(st == 200 and ("Boris", "autoban") in kinds and ("Fay", "autoban") in kinds and ("Carl", "tpm-changed") in kinds and ("Carl", "tpm-off") in kinds
       and ("Carl", "resets") in kinds and ("Anna", "second-game") in kinds and ("Anna", "tpm-off") in kinds,
       f"her list: the autobans, the TPM changed and off, the frees, the second game ({kinds})")
    ok(f.get("open") == 5, f"five open, the autobans done already ({f.get('open')})")
    for kind in ("tpm-changed", "tpm-off"):
        tid = next(x["id"] for x in f["items"] if x["kind"] == kind)
        st, j, _ = her.call("POST", f"/api/admin/recode/flags/{tid}", {"verdict": "ok"})
    st, f2, _ = her.call("GET", "/api/admin/recode/flags")
    ok(st == 200 and f2.get("open") == 2 and not any(x["login"] == "Carl" and x["verdict"] is None for x in f2["items"]), f"all right: Carl's go from the open ones ({f2.get('open')})")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D3}, token=carl)
    ok(st == 200, "Carl plays")

    print("== one pair of its own for every start (the seal's master pair up here)")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D3}, token=carl)
    ok(st == 200 and j.get("stub") is not True, f"no master yet: the build brings its own library ({st} {j.get('stub')})")
    # a sealed build brings no library of its own — the library is the player's own pair, cut at the start
    files2 = stage("r2", [("CursedRecode.exe", "loader", b"MZ the loader " * 100),
                          ("game/recode.jar", "code", b"PK the game " * 100)], seal=True)
    made = pair("r2")
    master, mbin, meta = made["stub"], made["bin"], made["meta"]
    out = cli("release", "publish", "r2", "recode")
    ok("current one of recode" in out, f"a sealed build published, its master pair and its cutter with it ({out})")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D3}, token=carl)
    ok(st == 200 and j.get("stub") is True, f"the start says there is a master to cut it a pair from ({st} {j.get('stub')})")
    ok("\"seal\":" in j.get("manifest", ""), "and the build's own word says it is under one")
    pair_ticket, start_ticket = j["ticket"], j["start_ticket"]
    window, build = j["pair_window"], j["build"]
    ok(len(pair_ticket) == 64 and len(start_ticket) == 64 and pair_ticket != start_ticket,
       f"two tickets: the pair's, and this one start's own ({pair_ticket[:8]}… {start_ticket[:8]}…)")
    ok(isinstance(window, int) and window == int(time.time()) // (14 * 86400) and 0 < j.get("pair_seconds", 0) <= 14 * 86400,
       f"the window this pair is good for (#{window}, {j.get('pair_seconds')} s left of it)")
    ok(j.get("ticket_seconds") == 600, "a start's own ticket is ten minutes' (beta's TICKET_SECONDS); a pair's is the window's")

    st, framed = raw_bytes("POST", "/api/loader/recode/stub", {"ticket": pair_ticket}, token=carl)
    ok(st == 200 and framed[:8] == b"CRDPAIR1", f"the pair, under one frame ({st} {framed[:8]!r})")
    got_stub, got_bin = open_pair(framed)
    ok(len(got_stub) == len(master) and len(got_bin) == len(mbin) and got_stub[:2] == b"MZ" and got_bin[:4] == b"CRB1",
       f"two files of one cut: a library and a seal ({len(got_stub)} + {len(got_bin)} bytes)")
    a_at, b_at = made["a_at"], made["b_at"]
    ok(got_stub[a_at:a_at + 32] != master[a_at:a_at + 32] and got_stub[b_at:b_at + 32] != master[b_at:b_at + 32],
       "halves of their own: one crack of this pair opens this copy and nothing of the master's")
    if made["stood"]:
        # what the stand-in's stamp shows: this pair was cut under this start's pair ticket, for the days the
        # window has left, and for this account — so the server asked for the right cut. The seal itself is
        # protect-pack's own job.
        days_left = (j["pair_seconds"] + 86399) // 86400
        ok(got_stub[2:34] == hashlib.sha256(pair_ticket.encode()).digest(), "cut under this start's own pair ticket")
        ok(got_stub[34:42] == days_left.to_bytes(8, "little"), f"and for the days this window has left ({days_left})")
        ok(got_stub[42:50] == int(carl_uid).to_bytes(8, "little"),
           f"and for this account and no other ({carl_uid}) — a dump of the mapped game is marked with it")
        ok(got_bin[64:96] != got_stub[2:34] and got_bin[64:96] == hashlib.sha256(b"seal|" + pair_ticket.encode()).digest(),
           "the seal beside it is its own file, of the same ticket and not the stub twice")
    else:
        ok(got_stub != master and got_bin != mbin, "a real cut of its own: neither file is the master's own bytes")
    # every cut is a pair of its own — protect-pack takes fresh halves and fresh dates every time, so one
    # leak of a pair costs that one copy and not the shelf. What the window holds is the *ticket*; the loader
    # keeps the two files for the window so a start never asks twice, and a lost cache gets another pair of
    # that same ticket. So asked again: another cut, of the same master and the same ticket.
    st, again = raw_bytes("POST", "/api/loader/recode/stub", {"ticket": pair_ticket}, token=carl)
    a_stub, a_bin = open_pair(again)
    ok(st == 200 and len(a_stub) == len(master) and len(a_bin) == len(mbin)
       and a_stub[a_at:a_at + 32] != master[a_at:a_at + 32],
       f"asked again: another cut of the same master, halves of its own ({st})")
    # the master stays home, whole — its card as the packer wrote it, word for word
    home = f"{tmp}/dist/protect"
    ok(open(home + "/cursed_recode.dll", "rb").read() == master
       and open(home + "/cursed_recode.bin", "rb").read() == mbin
       and open(home + "/master.meta", "rb").read() == meta,
       "the master stays home, whole, after every cut")
    ok(hashlib.sha256(master).hexdigest() not in files2.values()
       and hashlib.sha256(mbin).hexdigest() not in files2.values()
       and os.path.isfile(home + "/protect-pack"),
       "and neither half of it is a file of the store (the cutter is here, and nothing serves it)")
    st, bad = raw_bytes("POST", "/api/loader/recode/stub", {"ticket": "b" * 64}, token=carl)
    ok(st == 401, f"a ticket that was never given ({st})")
    st, bad = raw_bytes("POST", "/api/loader/recode/stub", {"ticket": pair_ticket}, token=ph.get("token"))
    ok(st == 401, f"another account's ticket ({st})")
    st, bad = raw_bytes("POST", "/api/loader/recode/stub", {"ticket": "abc"}, token=carl)
    ok(st == 400, f"a ticket that is not one ({st})")
    st, bad = raw_bytes("POST", "/api/loader/recode/stub", {"ticket": pair_ticket})
    ok(st == 401, f"not signed in ({st})")
    # the pair's ticket opens the pair and spends nothing; the start's own is spent by the game alone
    st, j, _ = raw("POST", "/api/client/recode/verify", {"ticket": start_ticket, "device": D3, "build": build})
    ok(st == 200, f"asking for the pair does not spend the start's ticket: the game still confirms it ({st} {j.get('error')})")
    st, j, _ = raw("POST", "/api/client/recode/verify", {"ticket": pair_ticket, "device": D3, "build": build})
    ok(st == 401, f"and the pair's ticket is not a start's ({st})")
    # the next start of the same window: the same pair ticket. What the window holds is the ticket, not
    # the bytes — a start never asks for the same two files twice (the loader keeps them for the window),
    # and one that loses its cache gets another pair of that same ticket
    st, j2, _ = raw("POST", "/api/loader/recode/start", {"device": D3}, token=carl)
    ok(st == 200 and j2.get("ticket") == pair_ticket and j2.get("pair_window") == window,
       f"the same window: the same pair ticket ({st} window {j2.get('pair_window')})")
    ok(len(j2.get("start_ticket", "")) == 64 and j2.get("start_ticket") not in (start_ticket, pair_ticket),
       "and a start's ticket of its own, every start")
    st, second = raw_bytes("POST", "/api/loader/recode/stub", {"ticket": j2["ticket"]}, token=carl)
    s_stub, s_bin = open_pair(second)
    ok(st == 200 and len(s_stub) == len(master) and len(s_bin) == len(mbin)
       and s_stub[a_at:a_at + 32] != master[a_at:a_at + 32],
       f"and a pair cut under it, for this start too ({st})")
    # one player's pair is nobody else's: her pair ticket, her pair
    st, hr, _ = raw("POST", "/api/loader/recode/start", {"device": D1}, token=ph.get("token"))
    ok(st == 200 and hr.get("stub") is True and hr["ticket"] != pair_ticket, f"her own pair ticket ({st})")
    st, hers = raw_bytes("POST", "/api/loader/recode/stub", {"ticket": hr["ticket"]}, token=ph.get("token"))
    hstub, hbin = open_pair(hers)
    ok(st == 200 and hstub[a_at:a_at + 32] != master[a_at:a_at + 32] and hstub[a_at:a_at + 32] != got_stub[a_at:a_at + 32],
       "one player's pair opens that copy and nothing else")
    if made["stood"]:
        ok(hstub[2:34] == hashlib.sha256(hr["ticket"].encode()).digest(), "cut under her pair ticket, and his is his")
    # a release that brings no pair leaves the master be (the one the sealed builds go on being cut from)
    stage("r3", [("CursedRecode.exe", "loader", b"MZ the loader " * 100),
                 ("game/recode.jar", "code", b"PK the game " * 100)], seal=True)
    out = cli("release", "publish", "r3", "recode")
    ok("current one of recode" in out, f"one more sealed build ({out})")
    st, j3, _ = raw("POST", "/api/loader/recode/start", {"device": D3}, token=carl)
    ok(st == 200 and j3.get("stub") is True, f"the master is still what starts are cut from ({st} {j3.get('stub')})")
    # a build that is not under a seal keeps whatever library it brought
    stage("r4", [("CursedRecode.exe", "loader", b"MZ the loader " * 100),
                 ("bin/cursed_recode.dll", "native", b"MZ a library of its own " * 100)])
    out = cli("release", "publish", "r4", "recode")
    ok("current one of recode" in out, f"a build of another kind ({out})")
    st, j4, _ = raw("POST", "/api/loader/recode/start", {"device": D3}, token=carl)
    ok(st == 200 and j4.get("stub") is not True, f"no stub for one that is not sealed ({st} {j4.get('stub')})")

    print("== what the game's own guard finds (her rules, 16.10)")
    # (Eve's an old computer: no TPM, Recode's other nine parts — more than Cursed's eight)
    eve_d = {k: h64(k + "6") for k in ("win", "cpu", "disk", "board", "uuid", "vol", "gpu", "monitor", "msdm")}
    st, eve = link(Member("Eve").register())
    eve_t = eve.get("token")
    db.execute("UPDATE recode_keys SET used_by = (SELECT uid FROM users WHERE login = 'Eve'), used_login = 'Eve', used_at = ? WHERE code = ?", (int(time.time()), cm["codes"][2]))
    db.commit()
    note = "Cursed Recode — the game was closed\nwhy: a tool that takes a process apart was running: x64dbg (pid 404)\n"
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": eve_d}, token=eve_t)
    ok(st == 200, f"Eve plays ({st} {j.get('error')})")
    # a crash of the game's own is nobody's ban, however often it comes
    for i in (1, 2, 3):
        st, _ = raw_body("POST", "/api/crash?build=6&source=game", f"a crash of its own {i}", token=eve_t)
        ok(st == 200, f"a crash of the game's own, kept ({st})")
    ok(db.execute("SELECT COUNT(*) FROM bans WHERE uid = (SELECT uid FROM users WHERE login = 'Eve')").fetchone()[0] == 0,
       "a crash of the game's own, and no ban")
    # the first finding of the guard: her list, and the player keeps playing
    st, _ = raw_body("POST", "/api/crash?build=6&source=protect", note, token=eve_t)
    ok(st == 200, f"the guard's first note, kept ({st})")
    kinds = sorted((x["login"], x["kind"]) for x in her.call("GET", "/api/admin/recode/flags")[1].get("items", []))
    ok(("Eve", "protect") in kinds, f"the first finding is a line on her list ({kinds})")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": eve_d}, token=eve_t)
    ok(st == 200 and j.get("error") != "banned", f"and the player keeps playing ({st} {j.get('error')})")
    # the second one inside a week: a ban, at once and for good
    st, _ = raw_body("POST", "/api/crash?build=6&source=protect", note, token=eve_t)
    ok(st == 200, f"the guard's second note, kept ({st})")
    row = db.execute("SELECT reason, until FROM bans WHERE uid = (SELECT uid FROM users WHERE login = 'Eve')").fetchone()
    ok(row is not None and "повторное" in row[0] and row[1] is None, f"the second finding inside a week is a ban for good ({row})")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": eve_d}, token=eve_t)
    ok(st == 403 and j.get("error") == "banned", f"and the next start is told at once ({st} {j.get('error')})")
    kinds = sorted((x["login"], x["kind"]) for x in her.call("GET", "/api/admin/recode/flags")[1].get("items", []))
    ok(("Eve", "autoban") in kinds, f"and it is on her list as an autoban ({kinds})")
    # the notes are kept as they came
    kept = db.execute("SELECT source, file FROM crashes WHERE uid = (SELECT uid FROM users WHERE login = 'Eve') ORDER BY id").fetchall()
    ok([s for s, _ in kept] == ["game", "game", "game", "protect", "protect"], f"every note kept, under its own source ({kept})")

    print("== a look at the room (report only, 17.10)")
    st, j, _ = raw("POST", "/api/client/recode/sight", {
        "build": 6,
        "score": 40,
        "sights": [{"kind": "tool", "name": "x64dbg"}, {"kind": "script-ours", "name": "recode_guard.py"}],
    }, token=carl)
    ok(st == 200 and j.get("ok") is True, f"a look is kept ({st} {j})")
    kinds = sorted((x["login"], x["kind"]) for x in her.call("GET", "/api/admin/recode/flags")[1].get("items", []))
    ok(("Carl", "sight") in kinds, f"and it is a line on her list ({kinds})")
    row = db.execute("SELECT detail, verdict FROM recode_flags WHERE uid = ? AND kind = 'sight'", (carl_uid,)).fetchone()
    ok(row is not None and row[1] is None, f"report only: never a verdict on the player ({row})")
    st, j, _ = raw("POST", "/api/loader/recode/start", {"device": D3}, token=carl)
    ok(st == 200 and j.get("error") != "banned", f"and the player keeps playing ({st} {j.get('error')})")
    ok(db.execute("SELECT COUNT(*) FROM bans WHERE uid = ?", (carl_uid,)).fetchone()[0] == 0,
       "a look at the room bans no one")
finally:
    server.terminate()
    server.wait(10)
    shutil.rmtree(tmp, ignore_errors=True)

print()
print("FAILED: " + "; ".join(failures) if failures else "all good")
sys.exit(1 if failures else 0)
