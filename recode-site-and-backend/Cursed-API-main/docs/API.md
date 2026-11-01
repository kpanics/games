# cursed-api

The accounts and the market of cursedclient.xyz. Rust (axum + SQLite), on the VDS behind nginx at
`https://cursedclient.xyz/api/`.

What is kept of a person: login, UID (one per registration, from 1 up; hers is **0**, so never read 0 as "none"), e-mail, the password's
argon2id hash, the moment of registration. Of the network it registered from, only a hash with the server's own key
(`/var/lib/cursed/net.key`): one IP registers two accounts at most (05.10.2026). No IP addresses.

## For the loader

1. Sign in once: `POST /api/token` with `Content-Type: application/json` and **no `Origin` header**:
   `{"login": "<login or e-mail>", "password": "…"}` →
   - `200 {"uid": 0, "login": "Lunuca", "token": "<64 hex>"}`;
   - `401 {"error":"wrong"}` if the login or password is wrong;
   - `429` if there were too many tries: `{"error":"wait"}` for 10 wrong passwords in 15 minutes, or nginx's own page for more than 8 quick requests from one address.
2. Keep the token. It lives 180 days after it was last used and ends when the password changes.
3. Who is signed in: `GET /api/me` with `Authorization: Bearer <token>` →
   - `200 {"uid": 0, "login": "Lunuca", "since": <unix seconds>, "admin": true}` (no e-mail for the loader);
   - `401 {"error":"signed-out"}` → sign in again.
4. Pass `login` and `uid` on to the native side (Discord RPC and the rest).

## For the site

The site uses the same API with the `cursed_session` cookie (HttpOnly, Secure, SameSite=Strict, Path=/api, 30 days).

| Request | Body | Answer |
|---|---|---|
| `POST /api/register` | `login, email, password, key?, ref?` | sets the cookie; `{uid, login}` (`ref`: the referral code the visitor came by) |
| `POST /api/login` | `login, password` | sets the cookie; `{uid, login}` |
| `POST /api/logout` | none | clears the cookie |
| `POST /api/password` | `current, new` | `{}`; ends every other session and token |
| `GET /api/me` | none | `{uid, login, since, admin, email}` |

A POST whose `Origin` is not `https://cursedclient.xyz` is refused (`403 origin`).

Error words:
- `login`, `email`, `password`: the field is not right;
- `login-taken`, `email-taken`: already used;
- `closed`: registration is closed;
- `ip-limit`: this IP (an IPv6 one: its /64) has registered as many accounts as it may; her key lets one in anyway;
- `wrong`: wrong login or password;
- `wait`: too many wrong passwords;
- `signed-out`: no live session;
- `busy`: something failed on the server.

Rules:
- Login is 3–16 characters of `a–z`, `0–9` and `_`. It is unique regardless of case, and so is the e-mail.
- Password is 10–128 characters.
- Reserved logins (lunuca, cursed, admin…) can be taken only with a key.

## The market

Resource packs and shader packs that members upload. Every call needs an account (cookie or token): the market is
for members, to look at and to use. The site and the client use the same calls.

### Uploading a pack

1. `POST /api/uploads {name, size, sha256?}` returns `{id, chunk: 8388608, received: 0}`.
   - The size can be up to 250 MB.
   - A member can have up to 5 uploads at once and up to 30 a day.
2. Send the pieces: `PUT /api/uploads/{id}?at=<received>` with raw bytes, up to 8 MB each, in order.
   - The answer is `{received, state}`.
   - A piece at the wrong place gets `409 {"error":"at","received":N}`: go on from `N`. That is how a broken upload resumes.
3. After the last piece, the upload is `queued`, then `checking`. Ask `GET /api/uploads/{id}` until it is one of two states:
   - `ready`: `result` holds `{kind, title, size, icon, facts}`;
   - `refused`: `result` holds `{message, path}`.
4. Make a pack of a ready upload: `POST /api/packs {upload, title, number, blurb?, about?, visibility?: public|private, changelog?, publish?}`.
   Or add it as a new version to a pack you already have: `POST /api/packs/{key}/versions {upload, number, changelog?}`.
   A new version has to be the same kind of pack.

### What the check does (market/inspect.rs, market/zip.rs)

**Refused:**
- the zip tricks: bombs, lying sizes, files that overlap, something hidden in front or after the archive, names that
  differ between the list and the header, encryption, links, archives split into parts, damaged files;
- programs (by name or by their first bytes);
- paths that leave the pack;
- two files with the same name;
- more than 30 000 files, or more than 1 GB unpacked;
- neither a resource pack nor a shader pack.

**Left out, with a note:**
- scripts, archives inside, junk (`__MACOSX`, `.DS_Store`, `Thumbs.db`, editor files);
- files the game never reads;
- pictures that don't decode, or are over 8192 px;
- names the game can't use;
- text files over 8 MB, single files over 128 MB.

**Kept wherever they lie:** licences and readmes.

**Fixed, with a note:** a pack zipped inside a folder, backslashes in names.

What is served is a clean archive rebuilt from the checked files. The same pack always gives the same bytes, stored
under its SHA-256.

**Facts** (in the upload's result and on the pack page):
- resource packs:
  - `game {min, max, versions, fits, problem}`: pack.mcmeta read exactly as 1.21.11 reads it (market/format.rs);
  - `resolution` (the width most block textures have);
  - `changes` (blocks, items, gui, sounds, fonts, ctm, emissive, entities, armor, sky, models, cem, cit, animated…);
  - `textures`, `sounds`, `animated`, `big`, `namespaces`, `description`;
- shader packs: `features` (Shadows, Water, Volumetric light…), `dimensions`, `options`, `profiles`, `languages`, `iris`;
- both: `files`, `unpacked`, `archive`, `fixed`, `dropped` (only for the owner), `warnings`.

### Packs

| Request | What |
|---|---|
| `GET /api/market?kind=resourcepack\|shaderpack&q=&sort=new\|users\|updated\|name&page=` | published public packs, 24 a page: `{items: [card], total, page}` |
| `GET /api/packs/{key}` | a pack page (`key` is its number or slug): the card plus `about`, `versions`, `pictures`, `facts`, `mine` |
| `GET /api/packs/{key}/download[?v=number]` | the clean archive (nginx sends it: `X-Accel-Redirect`); counts the member once among its `users` |
| `PATCH /api/packs/{key} {title?, blurb?, about?, visibility?, state?: published\|draft}` | its owner |
| `DELETE /api/packs/{key}` | its owner or her; gone for good after 30 days |
| `POST /api/packs/{key}/pictures?caption=` (raw PNG/JPEG/WebP, up to 12 MB) | a screenshot becomes two JPEGs (2560×1440 and 640×360); up to 12 pictures a pack |
| `PATCH /api/packs/{key}/pictures {order?, cover?, captions?}` · `DELETE /api/packs/{key}/pictures/{id}` | arranging them |
| `GET /api/me/packs` | my packs, drafts and hidden ones too |
| `POST /api/packs/{key}/hide {why}` · `POST /api/packs/{key}/unhide` | moderation (UID 0): hidden packs are gone for others; the author sees why |

Pictures are at `/media/<2>/<sha256>.jpg`, and the small one is `…-s.jpg`. They never change, so they are cached forever.

### Testing

- `tests/market_e2e.py` runs on the staging instance (`/root/staging.sh`, port 8081 behind nginx on 127.0.0.1:8088).
- It uses real packs from Modrinth (in `/root/corpus`, fetched by `/root/fetch_corpus.py`), hostile archives, and the API's rules.
- It checks that every download equals its original, byte for byte.

## The forum (src/forum.rs)

For members only, like the market. Posts are plain text; the site draws the links, quotes (`> `), @mentions and
`code` itself, from escaped text.

| Request | What |
|---|---|
| `GET /api/forum` | categories (with counts and last activity), the 12 latest threads, who is online (active in the last 5 minutes), the unread count |
| `GET /api/forum/threads?cat=&q=&page=` | a category's threads (pinned first), or threads matching words; 30 a page; each with `unread` |
| `POST /api/forum/threads {category, title, body}` | a new thread (Announcements are hers only); one every 30 s, 6 an hour |
| `GET /api/forum/threads/{id}?page=` | a thread and 30 of its posts (with no page: the first page with something new); marks it read; `new` on posts since the last visit |
| `POST /api/forum/threads/{id}/posts {body}` | a reply (not in a closed thread, except hers); one every 8 s, 40 an hour; answers with the page it is on |
| `PATCH /api/forum/posts/{id} {body}` · `DELETE /api/forum/posts/{id}` | its author edits it or takes it back (the opening post of a thread with replies stays) |
| `PATCH /api/forum/threads/{id} {title?, category?, pinned?, locked?}` | its author renames it; she moves, pins and closes it |
| `POST /api/forum/threads/{id}/hide {why}` · `…/unhide`, `POST /api/forum/posts/{id}/hide {why}` · `…/unhide` | her moderation; the author sees why |
| `POST /api/reports {kind: post\|thread\|pack, id, why}` | a report to her (10 a day, each thing once) |
| `GET /api/reports` · `POST /api/reports/{id}/close {action: dismiss\|hide, why?}` | her list of open reports, with what each is about; closing one hides its target, if she says so |

Tests: `tests/forum_e2e.py` (36 checks) on the staging instance.

## Referral links (src/refs.rs, 09.10.2026)

Media, partners and admins (`refs.own`; admins also `refs.all`) have a personal link — their login — and up to 20 of
their own (admins 100), one for each place they put a link up. `https://cursedclient.xyz/r/<code>` is sent by nginx to
`/?ref=<code>`; the site counts the visit, keeps the code for 30 days and gives it with the registration. A visit is
counted once a network a day (the network kept hashed with `net.key`, as registrations are; the owner's own visits not
at all); the last link a visitor came by is the one that counts.

| Request | What |
|---|---|
| `POST /api/ref/hit {code}` | a visit (anyone; nginx sets X-Real-IP); `{code, by}` — `by`: the owner's card for a personal link |
| `GET /api/refs?days=1\|7\|30\|90\|0&uid=?` | a member's links with their numbers, the chart (by the hour today, else by the day, Moscow's), the 40 newest who came; `uid`: someone else's (admins) |
| `GET /api/refs/people?before=&before_uid=&link=?&uid=?` | the rest of who came |
| `GET /api/refs/all?days=` | every holder's links, the sum, the accounts made without a link (admins) |
| `POST /api/refs {name, code}` · `PATCH /api/refs/{id} {name?, archived?}` | a link of one's own made, renamed, put away (takes nobody new) or brought back; the personal one stays |

Numbers of a link: `people` (networks that opened it), `opened` (every time), `regs`, `played` (started the game: a
session of the game or minutes in it), `week` (a session this week), `hours`, `now` (in the game now).
Tests: `tests/refs_e2e.py` (81 checks; an instance of its own on port 8082, staging untouched), `cargo test refs::`.

## On the server

- Binary: `/opt/cursed/cursed-api`.
- Service: `cursed-api.service`, running as user `cursed` in a systemd sandbox, on 127.0.0.1:8080.
- Database: `/var/lib/cursed/cursed.db`; uploads on their way in: `/var/lib/cursed/uploads`.
- The market's files: `/var/lib/cursed-media` (`packs/`, `pictures/`). The service writes it, and nginx reads it through the group `www-data`.
- Copies: `cursed-backup.timer` makes one a day at about 04:30 into `/var/backups/cursed`, and keeps the last 14.
- nginx: `/etc/nginx/conf.d/cursed-limits.conf` sets the limits, and the `/api/` locations are in `sites-available/cursedclient.conf`.
- Firewall: `/etc/nftables.conf` lets in only ports 22, 80 and 443.

Commands (as root; they run as the `cursed` user):

```
cursed key                          # a one-time key: https://cursedclient.xyz/#/register/<key>, works while closed
cursed registration [open|closed]   # show or set it
cursed registration --per-ip N      # accounts one IP may register (2; 0 = any number)
cursed nets-import <file>           # lines "<uid> <ip>": networks of accounts made before 05.10 (from nginx's log)
cursed users                        # uid, login, e-mail, registered
cursed renumber <from> <to>         # move an account to another UID (sessions and key with it; the count follows)
cursed backup /path/copy.db         # a whole copy, safe while it runs
```

## Building and deploying

Build on the VDS, not on her PC:

```
scp Cargo.toml Cursed:/root/build/cursed-api/
scp src/main.rs Cursed:/root/build/cursed-api/src/
ssh Cursed 'cd /root/build/cursed-api && . ~/.cargo/env && nice cargo test --release && nice cargo build --release'
ssh Cursed 'install -m 755 /root/build/cursed-api/target/release/cursed-api /opt/cursed/ && systemctl restart cursed-api'
```

Unit files, the nginx limits and the firewall are in `deploy/`.

## Config updates and concurrent devices

`GET /api/configs?client=recode` advertises `versioned_updates: true`. A client can send
`PUT /api/configs/{id}` with `{"version": <version read>, "data": {...}}`. If another device
has already changed it, the API returns `409 {"error":"version", ...}` without changing
the current settings or history. The client should read the new version and keep its local
changes until the user resolves the conflict.

The version check, previous-version snapshot and update are one SQLite write transaction.
Requests that omit `version` remain supported for existing Cursed clients. History restores
use the same atomic snapshot/update path. With `versioned_history: true` in the list response,
`POST /api/configs/{id}/history/{old}?version={current}` checks the last version the user saw
before restoring an old snapshot. A concurrent save returns 409 and leaves history intact.
Omitting this query keeps the previous restore behavior for existing clients.
# Game presence and client badges

`GET /api/live?kind=game` and `POST /api/me/presence` must use the same authenticated
session. Presence accepts `mc`, `server`, `client` (`cursed` or `recode`), `build`
and `listening`. Reannounce after each stream reconnect, including when the metadata
has not changed. A POST alone never makes a player online.

`GET /api/game/ours?server=<address>` returns `players: [{mc, client}]` for active
game sessions on that server. Two clients using separate sessions of one account
remain independent. Closing one stream preserves other sessions and overlapping
streams of the same session. Public account presence and played minutes still count
the account once. Unopened metadata expires after two minutes; at most eight pending
sessions are retained per account. Addresses are case insensitive; the default port
25565 and a trailing hostname dot are normalized, while other ports are retained.
