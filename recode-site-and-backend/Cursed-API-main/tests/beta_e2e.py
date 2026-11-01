#!/usr/bin/env python3
"""The beta end to end, on the staging instance (/root/staging.sh: port 8081 behind nginx on 127.0.0.1:8088):
registration's limit, the loader's sign-in on a computer, one account per computer, a build published and
downloaded (whole, compressed, patched), the game's own check at its start, bans (the account and its computer),
the player freeing his computer, crash reports, a rollback. Run as root: python3 tests/beta_e2e.py"""
import hashlib
import json
import os
import random
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request

BASE = 'http://127.0.0.1:8088'
ORIGIN = BASE
STAGING = '/root/staging.sh'
DIST = '/var/lib/cursed-staging/dist'
checks = 0


def cli(*args, stdin=None):
    r = subprocess.run([STAGING, 'cli', *args], capture_output=True, text=True, input=stdin)
    return r.returncode, (r.stdout + r.stderr).strip()


def call(method, path, body=None, token=None, origin=False, raw=None, headers=None):
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(BASE + path, data=data, method=method)
    if body is not None:
        req.add_header('Content-Type', 'application/json')
    if token:
        req.add_header('Authorization', 'Bearer ' + token)
    if origin:
        req.add_header('Origin', ORIGIN)
    for k, v in (headers or {}).items():
        req.add_header(k, v)
    try:
        with urllib.request.urlopen(req) as r:
            content = r.read()
            return r.status, content
    except urllib.error.HTTPError as e:
        return e.code, e.read()


def js(content):
    try:
        return json.loads(content)
    except Exception:
        return {}


def check(ok, what):
    global checks
    checks += 1
    if not ok:
        print('FAIL:', what)
        sys.exit(1)
    print('ok  ', what)


def device(seed):
    return {k: hashlib.sha256(f'{seed}-{k}'.encode()).hexdigest() for k in ['board', 'cpu', 'disk', 'win', 'bios']}


def sha(b):
    return hashlib.sha256(b).hexdigest()


def stage(name, files, notes='Первая сборка беты.', assets=None):
    """A build waiting in incoming/<name>: release.json and its objects."""
    inc = os.path.join(DIST, 'incoming', name)
    os.makedirs(os.path.join(inc, 'objects'), exist_ok=True)
    listed = []
    for path, kind, content in files:
        h = sha(content)
        with open(os.path.join(inc, 'objects', h), 'wb') as f:
            f.write(content)
        listed.append({'path': path, 'sha256': h, 'size': len(content), 'kind': kind})
    rel = {'version': '0.1.0-beta.' + name, 'notes': notes, 'launch': {'main': 'net.minecraft.client.main.Main'}, 'files': listed}
    if assets:
        rel['assets'] = assets
    with open(os.path.join(inc, 'release.json'), 'w') as f:
        json.dump(rel, f)


def main():
    subprocess.run([STAGING, 'reset'], check=True)
    os.environ['CURSED_DIST'] = DIST
    shutil.rmtree(DIST, ignore_errors=True)
    os.makedirs(DIST)
    subprocess.run([STAGING, 'start'], check=True)
    time.sleep(1.0)
    cli('registration', 'open', '--limit', '2')

    # Registration's limit.
    for n in ['alpha', 'bravo']:
        s, b = call('POST', '/api/register', {'login': n, 'email': n + '@e.st', 'password': 'correct horse ' + n}, origin=True)
        check(s == 200, f'{n} registers')
    s, b = call('POST', '/api/register', {'login': 'charlie', 'email': 'c@e.st', 'password': 'correct horse c'}, origin=True)
    check(s == 403 and js(b).get('error') == 'full', 'the third is told the beta is full')
    code, out = cli('testers')
    check('2 of 2' in out and 'alpha' in out, 'the testers command lists them')

    # The loader's sign-in, one computer per account.
    A, B, C = device('A'), device('B'), device('C')
    s, b = call('POST', '/api/loader/login', {'login': 'alpha', 'password': 'correct horse alpha', 'device': A})
    check(s == 200 and js(b).get('token'), 'alpha signs in to the loader on A')
    ta = js(b)['token']
    s, b = call('POST', '/api/loader/login', {'login': 'bravo', 'password': 'correct horse bravo', 'device': A})
    check(s == 403 and js(b).get('error') == 'device-taken', 'bravo cannot on A: it is alpha\'s')
    s, b = call('POST', '/api/loader/login', {'login': 'bravo', 'password': 'correct horse bravo', 'device': B})
    check(s == 200, 'bravo signs in on B')
    tb = js(b)['token']
    s, b = call('POST', '/api/loader/login', {'login': 'alpha', 'password': 'correct horse alpha', 'device': B})
    check(s == 403 and js(b).get('error') == 'other-device', 'alpha cannot on B either: it is bound to A')
    s, b = call('POST', '/api/loader/login', {'login': 'alpha', 'password': 'x', 'device': A}, origin=True)
    check(s == 403, 'no loader sign-in from a web page')

    # No build yet.
    s, b = call('POST', '/api/loader/start', {'device': A}, token=ta)
    check(s == 503 and js(b).get('error') == 'no-release', 'nothing to start before a build')

    # Assets: an index and its objects, imported into the mirror.
    tmp = '/tmp/cursed-assets-test'
    shutil.rmtree(tmp, ignore_errors=True)
    os.makedirs(tmp + '/indexes')
    blob = os.urandom(3000)
    h1 = hashlib.sha1(blob).hexdigest()
    os.makedirs(f'{tmp}/objects/{h1[:2]}')
    open(f'{tmp}/objects/{h1[:2]}/{h1}', 'wb').write(blob)
    index = json.dumps({'objects': {'minecraft/sounds/x.ogg': {'hash': h1, 'size': len(blob)}}}).encode()
    open(f'{tmp}/indexes/29.json', 'wb').write(index)
    code, out = cli('dist', 'assets', tmp)
    check(code == 0 and '1 new' in out, 'assets imported')

    # The first build.
    big = bytes(random.getrandbits(8) for _ in range(300_000)) * 4  # 1.2 MB that compresses
    stage('1', [('bin/cursed_native.dll', 'native', big), ('cursed.jar', 'code', os.urandom(4000)), ('assets/indexes/29.json', 'assets-index', index)], assets='29')
    code, out = cli('release', 'publish', '1', 'beta')
    check(code == 0 and 'build 1' in out, 'build 1 published: ' + out)
    code, out = cli('dist', 'missing', stdin=sha(big) + '\n' + '0' * 64)
    check(out.split() == ['0' * 64], 'the store knows what it has')

    # Starting: the manifest, leave to download, a ticket.
    s, b = call('POST', '/api/loader/start', {'device': A}, token=ta)
    st = js(b)
    check(s == 200 and st.get('build') == 1 and st.get('ticket') and st.get('sig'), 'alpha starts: build 1, a ticket')
    manifest = json.loads(st['manifest'])
    dll = next(f for f in manifest['files'] if f['path'] == 'bin/cursed_native.dll')
    check(dll['z'] and dll['z'] < dll['size'], 'the native part has a compressed copy')
    s, b = call('GET', '/api/dist/object/' + dll['sha256'], token=ta)
    check(s == 200 and b == big, 'the file comes whole and right')
    s, b = call('GET', '/api/dist/object/' + dll['sha256'] + '?z=1', token=ta)
    z = subprocess.run(['zstd', '-d', '-c', '--long=27'], input=b, capture_output=True).stdout
    check(s == 200 and z == big, 'and compressed, the same once unpacked')
    s, b = call('GET', '/api/dist/asset/' + h1, token=ta)
    check(s == 200 and b == blob, 'an asset from the mirror')
    s, b = call('GET', '/api/dist/object/' + dll['sha256'])
    check(s == 401, 'nothing without a token')
    s, b = call('GET', '/api/dist/object/' + dll['sha256'], token=tb)
    check(s == 403 and js(b).get('error') == 'start', 'nothing before the loader started')

    # The game's own check.
    s, b = call('POST', '/api/client/verify', {'ticket': st['ticket'], 'device': A, 'build': 1, 'nonce': 'ab' * 32})
    check(s == 200 and js(b).get('login') == 'alpha' and js(b).get('token'), 'the game starts on the ticket')
    sig = js(b).get('sig') or ''
    check(len(sig) == 128 and all(c in '0123456789abcdef' for c in sig), "the start is signed for the game's words")
    s, b = call('POST', '/api/client/verify', {'ticket': st['ticket'], 'device': A, 'build': 1})
    check(s == 401, 'a ticket starts it once')
    s, b = call('POST', '/api/loader/start', {'device': A}, token=ta)
    s, b = call('POST', '/api/client/verify', {'ticket': js(b)['ticket'], 'device': B, 'build': 1})
    check(s == 403, 'not on another computer')
    s, b = call('POST', '/api/loader/start', {'device': A}, token=ta)
    s, b = call('POST', '/api/client/verify', {'ticket': js(b)['ticket'], 'device': A, 'build': 1})
    check(s == 200 and 'sig' not in js(b), 'an older game (no words of its own) starts unsigned')

    # The second build: the native part changed a little, a patch for it.
    big2 = bytearray(big)
    big2[5000:5100] = os.urandom(100)
    big2 = bytes(big2)
    stage('2', [('bin/cursed_native.dll', 'native', big2), ('cursed.jar', 'code', os.urandom(4000)), ('assets/indexes/29.json', 'assets-index', index)], assets='29', notes='Вторая сборка.')
    code, out = cli('release', 'publish', '2', 'beta')
    check(code == 0 and 'build 2' in out, 'build 2 published')
    s, b = call('POST', '/api/loader/start', {'device': A}, token=ta)
    st2 = js(b)
    m2 = json.loads(st2['manifest'])
    d2 = next(f for f in m2['files'] if f['path'] == 'bin/cursed_native.dll')
    check(d2['patches'] and d2['patches'][0]['from'] == dll['sha256'] and d2['patches'][0]['size'] < 20000, 'a small patch from build 1')
    s, b = call('GET', f"/api/dist/patch/{dll['sha256']}/{d2['sha256']}", token=ta)
    open('/tmp/old.bin', 'wb').write(big)
    out = subprocess.run(['zstd', '-d', '-c', '--long=27', '--patch-from=/tmp/old.bin'], input=b, capture_output=True).stdout
    check(s == 200 and out == big2, 'the patch makes build 2 out of build 1')
    s, b = call('POST', '/api/client/verify', {'ticket': st2['ticket'], 'device': A, 'build': 1})
    check(s == 403 and js(b).get('error') == 'outdated' and js(b).get('build') == 2, 'an old build does not start')

    # Bans: the account, its sessions, its computer.
    code, out = cli('ban', 'alpha', '--for', '1h', 'читы', 'на', 'сервере')
    check(code == 0, 'alpha banned for an hour')
    s, b = call('POST', '/api/loader/start', {'device': A}, token=ta)
    check(s == 403 and js(b).get('error') == 'banned' and js(b).get('reason') == 'читы на сервере' and js(b).get('device') is False, 'the loader is told of the ban at its next start')
    s, b = call('POST', '/api/loader/login', {'login': 'alpha', 'password': 'correct horse alpha', 'device': A})
    check(s == 403 and js(b).get('reason') == 'читы на сервере' and js(b).get('until'), 'alpha is told why and until when')
    s, b = call('POST', '/api/login', {'login': 'alpha', 'password': 'correct horse alpha'}, origin=True)
    check(s == 403 and js(b).get('error') == 'banned', 'nor on the site')
    cli('registration', '--limit', '3')
    s, b = call('POST', '/api/register', {'login': 'charlie', 'email': 'c@e.st', 'password': 'correct horse c'}, origin=True)
    check(s == 200, 'a place more, charlie registers')
    s, b = call('POST', '/api/loader/login', {'login': 'charlie', 'password': 'correct horse c', 'device': A})
    check(s == 403 and js(b).get('error') == 'banned' and js(b).get('device') is True, 'charlie cannot play on alpha\'s banned computer')
    code, out = cli('unban', 'alpha')
    check('1 ban' in out, 'alpha unbanned')
    s, b = call('POST', '/api/loader/login', {'login': 'alpha', 'password': 'correct horse alpha', 'device': A})
    check(s == 200, 'alpha plays again')
    ta = js(b)['token']

    # The player frees his computer: once a day; her command at any time.
    s, b = call('POST', '/api/loader/device/reset', {'login': 'bravo', 'password': 'correct horse bravo', 'device': C})
    check(s == 200, 'bravo moves to C himself')
    s, b = call('POST', '/api/loader/device/reset', {'login': 'bravo', 'password': 'correct horse bravo', 'device': B})
    check(s == 429 and js(b).get('error') == 'wait', 'not twice a day')
    code, out = cli('hwid-reset', 'bravo')
    s, b = call('POST', '/api/loader/login', {'login': 'bravo', 'password': 'correct horse bravo', 'device': B})
    check(s == 200, 'freed by her, bravo is on B again')

    # Crash reports.
    s, b = call('POST', '/api/crash?build=2&source=game', token=ta, raw=b'java.lang.NullPointerException\n\tat x.y')
    check(s == 200, 'a crash report comes in')
    code, out = cli('crashes', 'alpha')
    check('alpha' in out and 'game' in out, 'and she sees it')

    # Rollback.
    code, out = cli('release', 'rollback', 'beta')
    check('build 1' in out, 'the channel is back on build 1')
    s, b = call('POST', '/api/loader/start', {'device': A}, token=ta)
    check(js(b).get('build') == 1, 'the loader is given build 1')

    # Deleting frees a place.
    code, out = cli('delete', 'charlie')
    check(code == 0, 'charlie deleted')
    code, out = cli('registration')
    check('2 of 3' in out, 'a place is free again')

    subprocess.run([STAGING, 'stop'])
    print(f'all {checks} checks passed')


main()
