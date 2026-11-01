#!/usr/bin/env python3
"""Accounts per network (05.10.2026), on the staging instance (/root/staging.sh): two a network, an IPv6 home counted
by its /64, her key not stopped, a deleted account freeing its place, renumber and nets-import keeping the count, the
limit changed by command. Talks to the app itself on 8081 and gives X-Real-IP as nginx would.
Run as root: python3 tests/nets_e2e.py"""
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request

APP = 'http://127.0.0.1:8081'
STAGING = '/root/staging.sh'
checks = 0


def cli(*args):
    r = subprocess.run([STAGING, 'cli', *args], capture_output=True, text=True)
    return r.returncode, (r.stdout + r.stderr).strip()


def register(login, ip=None, key=''):
    login = 'net_' + login  # 3 characters at least
    req = urllib.request.Request(APP + '/api/register', method='POST',
                                 data=json.dumps({'login': login, 'email': login + '@e.st', 'password': 'correct horse ' + login, 'key': key}).encode())
    req.add_header('Content-Type', 'application/json')
    if ip:
        req.add_header('X-Real-IP', ip)
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read() or b'{}')


def check(ok, what):
    global checks
    checks += 1
    if not ok:
        print('FAIL:', what)
        subprocess.run([STAGING, 'stop'])
        sys.exit(1)
    print('ok  ', what)


def passes(login, ip=None, key=''):
    s, b = register(login, ip, key)
    check(s == 200, f'{login} from {ip or "no address"} registers')
    return b.get('uid')


def stopped(login, ip):
    s, b = register(login, ip)
    check(s == 403 and b.get('error') == 'ip-limit', f'{login} from {ip} is stopped: too many from there')


def main():
    subprocess.run([STAGING, 'reset'], check=True)
    subprocess.run([STAGING, 'start'], check=True)
    time.sleep(1.0)
    code, out = cli('registration', 'open', '--limit', '100')
    check('2 per IP' in out, 'two a network unless she says otherwise')
    check(os.stat('/var/lib/cursed-staging/net.key').st_mode & 0o777 == 0o600, 'the key is made, readable by its owner only')

    # Two a network.
    passes('a1', '10.0.0.1')
    passes('a2', '10.0.0.1')
    stopped('a3', '10.0.0.1')
    b1 = passes('b1', '10.0.0.2')
    stopped('a3', '::ffff:10.0.0.1')

    # An IPv6 home is its /64.
    passes('c1', '2001:db8:1:2::1')
    passes('c2', '2001:db8:1:2:aaaa:bbbb:cccc:dddd')
    stopped('c3', '2001:db8:1:2::ffff')
    passes('c3', '2001:db8:1:3::1')

    # Past nginx nothing is counted (the staging's own tests, a call on the server itself).
    d = [passes(n) for n in ['d1', 'd2', 'd3']]

    # Her key lets one in from a full network.
    code, key = cli('key')
    passes('k1', '10.0.0.1', key)
    stopped('a3', '10.0.0.1')

    # A deleted account frees its place (one, not two: k1 counts too).
    code, out = cli('delete', 'net_a1')
    check(code == 0, 'a1 deleted')
    stopped('a3', '10.0.0.1')
    code, out = cli('delete', 'net_k1')
    passes('a3', '10.0.0.1')
    stopped('a4', '10.0.0.1')

    # Renumbered, its network goes with it.
    code, out = cli('renumber', str(b1), '500')
    check(code == 0, 'b1 renumbered to 500: ' + out)
    passes('b2', '10.0.0.2')
    stopped('b3', '10.0.0.2')

    # The accounts made before: their networks from nginx's log.
    with open('/tmp/nets_e2e.txt', 'w') as f:
        f.write(f'{d[1]} 10.0.0.9\n{d[2]} 10.0.0.9\n99999 10.0.0.9\nnot a line\n500 10.0.0.9\n')
    code, out = cli('nets-import', '/tmp/nets_e2e.txt')
    check('2 kept, 3 skipped' in out, 'nets-import keeps d2 and d3, skips the unknown, the bad line and one already kept: ' + out)
    stopped('e1', '10.0.0.9')

    # The limit is hers to change.
    code, out = cli('registration', '--per-ip', '3')
    check('3 per IP' in out, 'three a network now')
    passes('e1', '10.0.0.9')
    stopped('e2', '10.0.0.9')
    code, out = cli('registration', '--per-ip', '0')
    check('any number per IP' in out, 'any number')
    passes('e2', '10.0.0.9')
    code, out = cli('registration', '--per-ip', '-1')
    check(code != 0, 'a limit below 0 is refused')
    cli('registration', '--per-ip', '2')

    subprocess.run([STAGING, 'stop'])
    print(f'all {checks} checks passed')


main()
