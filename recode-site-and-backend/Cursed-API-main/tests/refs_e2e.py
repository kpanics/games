#!/usr/bin/env python3
"""Referral links (09.10.2026), on an instance of its own (a fresh database in /tmp, port 8082; the staging instance is
not touched): who has a page, the personal link made from the login, visits counted once a network a day (and not the
owner's own), registrations by a link and by a code of nothing, links made, refused, put away and brought back, the
numbers and the chart, the people and their pages, admins seeing everyone, the accounts made without a link, renumber
and a deleted account. Gives X-Real-IP as nginx would.
Run as root: python3 tests/refs_e2e.py"""
import http.cookiejar
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import time
import urllib.error
import urllib.request

BIN = '/root/build/cursed-api/target/release/cursed-api'
DIR = '/tmp/refs-e2e'
APP = 'http://127.0.0.1:8082'
ENV = dict(os.environ, CURSED_DB=DIR + '/cursed.db', CURSED_DIST=DIR + '/dist', CURSED_MEDIA=DIR + '/media',
           CURSED_LISTEN='127.0.0.1:8082', CURSED_ORIGIN='http://127.0.0.1:8082', CURSED_SECURE_COOKIE='0')
checks = 0
server = None


def cli(*args):
    r = subprocess.run([BIN, *args], capture_output=True, text=True, env=ENV)
    return r.returncode, (r.stdout + r.stderr).strip()


def check(ok, what):
    global checks
    checks += 1
    if not ok:
        print('FAIL:', what)
        stop()
        sys.exit(1)
    print('ok  ', what)


def stop():
    if server:
        server.terminate()
        server.wait()


def start():
    # The command line works in a process of its own: the server's cards (roles, numbers) are made again at its start.
    global server
    stop()
    server = subprocess.Popen([BIN, 'serve'], env=ENV, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(1.2)


class Member:
    def __init__(self, login=None):
        self.jar = http.cookiejar.CookieJar()
        self.open = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar)).open
        self.login = login
        self.uid = None

    def call(self, method, path, body=None, ip=None):
        req = urllib.request.Request(APP + path, method=method, data=None if body is None else json.dumps(body).encode())
        if body is not None:
            req.add_header('Content-Type', 'application/json')
        if ip:
            req.add_header('X-Real-IP', ip)
        try:
            with self.open(req) as r:
                return r.status, json.loads(r.read() or b'{}')
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read() or b'{}')

    def get(self, path):
        return self.call('GET', path)

    def register(self, ref='', ip='10.9.0.1'):
        s, b = self.call('POST', '/api/register', {'login': self.login, 'email': self.login + '@e.st', 'password': 'correct horse ' + self.login, 'ref': ref}, ip)
        check(s == 200, f'{self.login} registers{" by " + ref if ref else ""}')
        self.uid = b['uid']
        return self


def hit(code, ip, who=None):
    return (who or Member()).call('POST', '/api/ref/hit', {'code': code}, ip)


def db():
    return sqlite3.connect(DIR + '/cursed.db')


def link_of(page, code):
    return next(l for l in page['links'] if l['code'] == code)


def main():
    global server
    shutil.rmtree(DIR, ignore_errors=True)
    os.makedirs(DIR)
    start()
    code, out = cli('registration', 'open', '--limit', '1000', '--per-ip', '0')
    check(code == 0, 'registration open, no limit a network')

    blogger = Member('Blogger_').register()
    partner = Member('partner1').register()
    plain = Member('plain1').register()
    boss = Member('boss1').register()
    for login, role in [('Blogger_', 'media'), ('partner1', 'partner'), ('boss1', 'admin')]:
        code, out = cli('role', login, role)
        check(code == 0, f'{login} is {role}')
    start()

    # Who has the page.
    s, b = plain.get('/api/refs')
    check(s == 403, 'a member without the roles has no links')
    s, me = blogger.get('/api/me')
    check('refs.own' in me['perms'] and 'refs.all' not in me['perms'], 'media: their own links, not everyone’s')
    s, me = boss.get('/api/me')
    check({'refs.own', 'refs.all'} <= set(me['perms']), 'an admin: both')
    s, page = blogger.get('/api/refs')
    check(s == 200 and page['mine'] and not page['all'], 'media see their page')
    check([l['code'] for l in page['links']] == ['blogger'] and page['links'][0]['main'], 'the personal link is the login, made at once (the _ at the end dropped)')
    check(page['base'] == 'http://127.0.0.1:8082/r/', 'the address before the code')
    check(page['total']['regs'] == 0 and page['total']['people'] == 0 and page['people'] == [], 'nothing yet')
    main_id = page['links'][0]['id']

    # Visits.
    s, b = hit('BLOGGER', '10.2.0.1')
    check(s == 200 and b['code'] == 'blogger' and b['by']['login'] == 'Blogger_', 'a visit by the personal link (any case): its owner is shown')
    hit('blogger', '10.2.0.1')
    hit('blogger', '10.2.0.2')
    s, b = hit('blogger', '10.2.0.9', blogger)
    check(s == 200, 'the owner may open their own link')
    for bad in ['nothing', 'a', 'ev il', '../x']:
        s, b = hit(bad, '10.2.0.1')
        check(s == 404, f'no link “{bad}”')
    s, page = blogger.get('/api/refs')
    l = link_of(page, 'blogger')
    check(l['people'] == 2 and l['opened'] == 3, 'two networks, three openings; the owner’s own not counted')

    # Registrations by it.
    fan1 = Member('fan1').register('blogger', '10.2.0.1')
    fan2 = Member('fan2').register('Blogger', '10.2.0.2')
    fan3 = Member('fan3').register('nothing-here', '10.2.0.3')
    check(db().execute('SELECT COUNT(*) FROM referrals').fetchone()[0] == 2, 'two came by the link; a code of nothing registers and counts for nobody')

    # Links of their own.
    s, b = blogger.call('POST', '/api/refs', {'name': 'TG post', 'code': 'TG-Post'})
    check(s == 200 and b['id'], 'a link of their own')
    tg = b['id']
    s, b = blogger.call('POST', '/api/refs', {'name': 'again', 'code': 'tg-post'})
    check(s == 409 and b['error'] == 'taken', 'a code is one link')
    s, b = blogger.call('POST', '/api/refs', {'name': 'x', 'code': 'plain1'})
    check(s == 409 and b['error'] == 'taken', 'nobody takes another member’s login')
    s, b = blogger.call('POST', '/api/refs', {'name': 'x', 'code': 'admin'})
    check(s == 409, 'nor a reserved word')
    for bad in ['ab', '-abc', 'abc-', 'a b c', 'абв', 'x' * 25]:
        s, b = blogger.call('POST', '/api/refs', {'name': 'x', 'code': bad})
        check(s == 400 and b['error'] == 'code', f'no code “{bad}”')
    s, b = blogger.call('POST', '/api/refs', {'name': '   ', 'code': 'fine-one'})
    check(s == 400 and b['error'] == 'name', 'a link has a name')
    s, b = plain.call('POST', '/api/refs', {'name': 'x', 'code': 'mine-too'})
    check(s == 403, 'nobody else makes links')
    s, b = hit('tg-post', '10.3.0.1')
    check(s == 200 and b['by'] is None, 'a link of their own shows nobody')
    fan4 = Member('fan4').register('tg-post', '10.3.0.1')

    # The numbers.
    s, page = blogger.get('/api/refs?days=30')
    t = page['total']
    check((t['people'], t['regs'], t['played'], t['week'], t['hours']) == (3, 3, 0, 0, 0), 'three people, three accounts, nobody in the game yet')
    check([l['code'] for l in page['links']] == ['blogger', 'tg-post'], 'the personal link first')
    check(link_of(page, 'blogger')['regs'] == 2 and link_of(page, 'tg-post')['regs'] == 1, 'each link its own')
    check([p['card']['login'] for p in page['people']] == ['fan4', 'fan2', 'fan1'], 'who came, newest first')
    check(all('hours' not in p for p in page['people']), 'their hours only for admins')
    ch = page['chart']
    check(ch['step'] == 86400 and ch['n'] == 30, 'thirty days by the day')
    check(sum(ch['links'][str(main_id)]['r']) == 2 and sum(ch['links'][str(main_id)]['v']) == 2 and ch['links'][str(main_id)]['r'][-1] == 2, 'the chart: today’s column')
    s, today = blogger.get('/api/refs?days=1')
    check(today['chart']['step'] == 3600 and today['chart']['n'] == 24 and sum(today['chart']['links'][str(tg)]['r']) == 1, 'today by the hour')

    # Started the game, played this week, hours.
    c = db()
    c.execute("INSERT INTO sessions (hash, uid, kind, created, seen) VALUES (randomblob(32), ?1, 'client', ?2, ?2)", (fan1.uid, int(time.time()) - 3600))
    c.execute("INSERT INTO sessions (hash, uid, kind, created, seen) VALUES (randomblob(32), ?1, 'client', ?2, ?2)", (fan2.uid, int(time.time()) - 20 * 86400))
    c.execute("INSERT INTO stats (uid, key, value) VALUES (?1, 'game_min', 125)", (fan1.uid,))
    c.commit()
    s, page = blogger.get('/api/refs')
    t = page['total']
    check((t['played'], t['week'], t['hours']) == (2, 1, 2), 'two started the game, one this week; two hours')
    p = {x['card']['login']: x for x in page['people']}
    check(p['fan1']['played'] and p['fan1']['week'] and p['fan2']['played'] and not p['fan2']['week'] and not p['fan4']['played'], 'each one’s marks')

    # Admins.
    s, b = partner.get(f'/api/refs?uid={blogger.uid}')
    check(s == 403, 'a partner does not see another’s page')
    s, b = boss.get(f'/api/refs?uid={blogger.uid}')
    check(s == 200 and not b['mine'] and b['owner']['login'] == 'Blogger_' and b['total']['regs'] == 3, 'an admin sees anyone’s page')
    check(next(x for x in b['people'] if x['card']['login'] == 'fan1')['hours'] == 2, '…with their hours')
    s, b = boss.get('/api/refs/all?days=30')
    check(s == 200, 'an admin sees everyone')
    owners = [o['card']['login'] for o in b['owners']]
    check(owners[0] == 'Blogger_' and {'partner1', 'boss1'} <= set(owners), 'the one who brought most first; every holder there, links made for them')
    check(b['total']['regs'] == 3 and b['total']['people'] == 3, 'the sum of all links')
    check(b['organic'] == 5, 'five accounts made without a link (four of the test and fan3)')
    check(sum(b['chart']['o']) == 5 and sum(b['chart']['r']) == 3, 'the chart: by links and without')
    s, b = blogger.get('/api/refs/all')
    check(s == 403, 'media do not see everyone')

    # Put away and back.
    s, b = partner.call('PATCH', f'/api/refs/{tg}', {'archived': True})
    check(s == 404, 'nobody puts away another’s link')
    s, b = blogger.call('PATCH', f'/api/refs/{tg}', {'archived': True, 'name': 'TG post, 9 Oct'})
    check(s == 200, 'its owner puts it away and renames it')
    s, b = hit('tg-post', '10.3.0.2')
    check(s == 404, 'a link put away opens nothing')
    Member('fan5').register('tg-post', '10.3.0.2')
    check(db().execute('SELECT COUNT(*) FROM referrals').fetchone()[0] == 3, '…and brings nobody')
    s, page = blogger.get('/api/refs')
    l = link_of(page, 'tg-post')
    check(l['archived'] and l['name'] == 'TG post, 9 Oct' and l['regs'] == 1, '…keeping what it brought')
    s, b = boss.call('PATCH', f'/api/refs/{tg}', {'archived': False})
    check(s == 200, 'an admin brings it back')
    s, b = hit('tg-post', '10.3.0.2')
    check(s == 200, '…and it opens again')
    s, b = blogger.call('PATCH', f'/api/refs/{main_id}', {'archived': True})
    check(s == 409 and b['error'] == 'main', 'the personal link stays')

    # A partner's limit.
    for i in range(20):
        s, b = partner.call('POST', '/api/refs', {'name': f'n{i}', 'code': f'p1-{i:02}'})
        check(s == 200, f'link {i + 1} of 20') if i in (0, 19) else None
    s, b = partner.call('POST', '/api/refs', {'name': 'one more', 'code': 'p1-more'})
    check(s == 409 and b['error'] == 'limit', 'twenty besides the personal one')

    # The list of who came, page by page.
    c = db()
    now = int(time.time())
    for i in range(45):
        c.execute("INSERT INTO users (uid, login, login_key, email, email_key, pass, created) VALUES (?1, ?2, ?3, ?4, ?4, 'x', ?5)", (5000 + i, f'many{i}', f'many{i}', f'many{i}@e.st', now - 1000 - i))
        c.execute('INSERT INTO referrals (uid, link, at) VALUES (?1, ?2, ?3)', (5000 + i, main_id, now - 1000 - i))
    c.commit()
    s, page = blogger.get('/api/refs')
    check(len(page['people']) == 40 and page['more'], 'forty at a time')
    s, b = blogger.get(f'/api/refs/people?before={page["people"][-1]["at"]}&before_uid={page["people"][-1]["uid"]}')
    check(len(b['people']) == 8 and not b['more'], 'the rest after them (48 in all)')
    s, b = blogger.get(f'/api/refs/people?before={now + 10}&link={tg}')
    check([x['card']['login'] for x in b['people']] == ['fan4'], 'one link’s people')
    s, b = partner.get(f'/api/refs/people?before={now + 10}&uid={blogger.uid}')
    check(s == 403, 'nobody else’s people')

    # Renumbered and deleted.
    code, out = cli('renumber', str(blogger.uid), '7001')
    check(code == 0, 'the blogger renumbered')
    code, out = cli('renumber', str(fan1.uid), '7002')
    check(code == 0, 'a fan renumbered')
    start()
    s, page = blogger.get('/api/refs')
    check(s == 200 and page['owner']['uid'] == 7001 and len(page['links']) == 2 and 'fan1' in [x['card']['login'] for x in page['people']], 'links and people follow the numbers')
    code, out = cli('delete', 'fan2')
    check(code == 0, 'a fan deleted')
    s, page = blogger.get('/api/refs')
    check('fan2' not in [x['card']['login'] for x in page['people']] and link_of(page, 'blogger')['regs'] == 46, '…and gone from the list')
    broken = db().execute('SELECT COUNT(*) FROM pragma_foreign_key_check').fetchone()[0]
    check(broken == 0, 'no broken links in the database')

    stop()
    print(f'{checks} checks, all good')


if __name__ == '__main__':
    try:
        main()
    finally:
        stop()
