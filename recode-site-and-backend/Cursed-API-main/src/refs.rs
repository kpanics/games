//! Referral links (09.10.2026): media, partners and admins each have a link of their own, cursedclient.xyz/r/<code>
//! (nginx sends it on to /?ref=<code>), and may make more of them, one for each place they put it up. The site keeps
//! the code a visitor came by for 30 days and hands it in with the registration (main.rs); here, what each link
//! brought: the people who opened it (a network a day, kept hashed as beta.rs keeps them, never the address), the
//! accounts made by it, and of those who started the game, who played this week, their hours. Admins see every link.
//!
//! The personal link is the login (made the first time its owner asks); its owner is shown on the registration page
//! of whoever came by it ("an invitation from …"). The others are the owner's own codes, shown to nobody: a post, a
//! video, an advert bought. A link put away takes nobody new and keeps what it brought.

use std::collections::HashMap;

use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::market::{busy, clean_line, member, run, said};
use crate::people::{can, REFS_ALL, REFS_OWN};
use crate::{db, fail, now, same_site, Shared};

/// Moscow's day: the days of the charts begin at its midnight.
const MSK: i64 = 3 * 3600;
/// Links a member may have besides the personal one (an admin, for the adverts they buy: more).
const LINKS_OWN: i64 = 20;
const LINKS_ALL: i64 = 100;
/// The roles whose holders have links (and she).
const ROLES: [&str; 3] = ["admin", "media", "partner"];

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS ref_links (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             owner INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             code TEXT NOT NULL UNIQUE,                 -- lower case: what follows /r/
             name TEXT NOT NULL DEFAULT '',             -- the owner's word for it ('' for the personal one)
             main INTEGER NOT NULL DEFAULT 0,           -- the personal link: its owner is shown to those it brings
             created INTEGER NOT NULL,
             archived INTEGER);                         -- put away: takes nobody new, keeps what it brought
         CREATE INDEX IF NOT EXISTS ref_links_owner ON ref_links(owner);
         CREATE TABLE IF NOT EXISTS ref_visits (
             link INTEGER NOT NULL REFERENCES ref_links(id) ON DELETE CASCADE,
             day INTEGER NOT NULL,                      -- Moscow's day number
             net BLOB NOT NULL,                         -- the network, hashed (beta.rs)
             n INTEGER NOT NULL DEFAULT 1,              -- how many times it was opened from there that day
             first INTEGER NOT NULL,                    -- the first of them
             PRIMARY KEY (link, day, net));
         CREATE TABLE IF NOT EXISTS referrals (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             link INTEGER NOT NULL REFERENCES ref_links(id) ON DELETE CASCADE,
             at INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS referrals_link ON referrals(link);",
    )
    .expect("make the referrals' tables");
}

fn day_of(t: i64) -> i64 {
    (t + MSK).div_euclid(86_400)
}

fn day_start(d: i64) -> i64 {
    d * 86_400 - MSK
}

/// A code: 3 to 24 of a–z, 0–9, `_` and `-`, a letter or a digit at both ends.
pub fn code_ok(s: &str) -> bool {
    let b = s.as_bytes();
    (3..=24).contains(&b.len())
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-')
        && b[0].is_ascii_alphanumeric()
        && b[b.len() - 1].is_ascii_alphanumeric()
}

/// Whether `code` may become `owner`'s: nobody's yet, and not somebody else's login nor a reserved word (nobody passes
/// for another, or for the site); one's own login is one's own even when reserved ("lunuca").
fn code_free(c: &Connection, code: &str, owner: i64) -> bool {
    let link = c.query_row("SELECT 1 FROM ref_links WHERE code = ?1", params![code], |_| Ok(())).optional().ok().flatten().is_some();
    let login = c.query_row("SELECT uid FROM users WHERE login_key = ?1", params![code], |r| r.get::<_, i64>(0)).optional().ok().flatten();
    !link && login.map_or(true, |u| u == owner) && (login == Some(owner) || !crate::RESERVED.contains(&code))
}

/// A member's personal link, made the first time it is asked for: their login, or the login and a number when that
/// is taken.
fn ensure_main(c: &Connection, uid: i64) -> Option<i64> {
    if let Some(id) = c.query_row("SELECT id FROM ref_links WHERE owner = ?1 AND main = 1", params![uid], |r| r.get(0)).optional().ok().flatten() {
        return Some(id);
    }
    let login: String = c.query_row("SELECT login_key FROM users WHERE uid = ?1", params![uid], |r| r.get(0)).optional().ok().flatten()?;
    let base: String = login.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_').collect::<String>().trim_matches('_').chars().take(20).collect();
    let base = if base.len() < 3 { format!("uid{uid}") } else { base };
    let code = std::iter::once(base.clone()).chain((2..100).map(|n| format!("{base}-{n}"))).find(|k| code_ok(k) && code_free(c, k, uid))?;
    c.execute("INSERT INTO ref_links (owner, code, name, main, created) VALUES (?1, ?2, '', 1, ?3)", params![uid, code, now()]).ok()?;
    Some(c.last_insert_rowid())
}

/// The members who have links: the holders of the roles, she, and anyone who has links from before.
fn holders(c: &Connection) -> Vec<i64> {
    let mut v: Vec<i64> = c
        .prepare(&format!(
            "SELECT DISTINCT r.uid FROM roles r JOIN users u ON u.uid = r.uid WHERE r.role IN ({}) UNION SELECT owner FROM ref_links UNION SELECT uid FROM users WHERE uid IN (?1, ?2)",
            ROLES.iter().map(|r| format!("'{r}'")).collect::<Vec<_>>().join(", ")
        ))
        .and_then(|mut q| q.query_map(params![crate::OWNERS[0], crate::OWNERS[1]], |r| r.get(0)).map(|m| m.flatten().collect()))
        .unwrap_or_default();
    v.sort_unstable();
    v.dedup();
    v
}

/// The account just made came by `code` (in the registration's transaction; a code of nothing changes nothing).
pub fn attach(c: &Connection, uid: i64, code: &str) {
    let code = code.trim().to_ascii_lowercase();
    if !code_ok(&code) {
        return;
    }
    let _ = c.execute(
        "INSERT OR IGNORE INTO referrals (uid, link, at) SELECT ?1, id, ?3 FROM ref_links WHERE code = ?2 AND archived IS NULL AND owner != ?1",
        params![uid, code, now()],
    );
}

// ------------------------------------------------------------------------------------------------ a visit

#[derive(Deserialize)]
struct Hit {
    code: String,
}

/// The site opened by a link: the visit counted (once a network a day for "people", every time for "opened"), and
/// what the registration page may show — the owner of a personal link.
async fn hit(State(app): State<Shared>, h: HeaderMap, Json(b): Json<Hit>) -> Response {
    if !same_site(&app, &h) {
        return fail(StatusCode::FORBIDDEN, "origin");
    }
    let code = b.code.trim().to_ascii_lowercase();
    if !code_ok(&code) {
        return fail(StatusCode::NOT_FOUND, "no-link");
    }
    // Without nginx in front (a test), every visit is of one network.
    let net = crate::beta::net_of(&h, &app.net_key).unwrap_or_else(|| vec![0; 32]);
    let token = crate::presented(&h).map(|(t, _)| t);
    let found = db(&app, move |c| -> Option<Value> {
        let (id, owner, main): (i64, i64, bool) =
            c.query_row("SELECT id, owner, main FROM ref_links WHERE code = ?1 AND archived IS NULL", params![code], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional().ok()??;
        // Its owner opening their own link counts for nothing.
        let viewer = token.and_then(|t| crate::who(c, &t)).map(|w| w.uid);
        if viewer != Some(owner) {
            let at = now();
            let _ = c.execute(
                "INSERT INTO ref_visits (link, day, net, n, first) VALUES (?1, ?2, ?3, 1, ?4) ON CONFLICT (link, day, net) DO UPDATE SET n = n + 1",
                params![id, day_of(at), net, at],
            );
        }
        Some(json!({ "code": code, "by": if main { crate::people::card_of(c, owner) } else { Value::Null } }))
    })
    .await;
    match found {
        Some(v) => Json(v).into_response(),
        None => fail(StatusCode::NOT_FOUND, "no-link"),
    }
}

// ------------------------------------------------------------------------------------------------ the numbers

/// The time a page looks at: today by the hour, or days by the day.
struct Span {
    since: i64,
    step: i64,
    n: i64,
}

impl Span {
    /// `days`: 1 (today, Moscow's), 7, 30, 90, or 0 for all (from the first of the links, a year at most).
    fn of(days: i64, first: i64) -> Span {
        let today = day_of(now());
        if days == 1 {
            return Span { since: day_start(today), step: 3600, n: 24 };
        }
        let from = match days {
            7 | 30 | 90 => today - days + 1,
            _ => day_of(first).clamp(today - 364, today - 6),
        };
        Span { since: day_start(from), step: 86_400, n: today - from + 1 }
    }
    fn slot(&self, t: i64) -> Option<usize> {
        let i = (t - self.since).div_euclid(self.step);
        (0..self.n).contains(&i).then_some(i as usize)
    }
}

fn ids_sql(ids: &[i64]) -> String {
    if ids.is_empty() {
        "-1".into()
    } else {
        ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
    }
}

#[derive(Default, Clone)]
struct Count {
    people: i64,
    opened: i64,
    regs: i64,
    played: i64,
    week: i64,
    minutes: i64,
    now: i64,
}

impl Count {
    fn add(&mut self, o: &Count) {
        self.people += o.people;
        self.opened += o.opened;
        self.regs += o.regs;
        self.played += o.played;
        self.week += o.week;
        self.minutes += o.minutes;
        self.now += o.now;
    }
    fn json(&self) -> Value {
        json!({ "people": self.people, "opened": self.opened, "regs": self.regs, "played": self.played, "week": self.week, "hours": self.minutes / 60, "now": self.now })
    }
}

/// Whether an account ever started the game (a session of the game, or minutes counted in it), and this week.
const PLAYED: &str = "(EXISTS (SELECT 1 FROM sessions s WHERE s.uid = r.uid AND s.kind = 'client') OR EXISTS (SELECT 1 FROM stats t WHERE t.uid = r.uid AND t.key = 'game_min' AND t.value > 0))";
const WEEK: &str = "EXISTS (SELECT 1 FROM sessions s WHERE s.uid = r.uid AND s.kind = 'client' AND s.created >= ?2)";
const MINUTES: &str = "COALESCE((SELECT t.value FROM stats t WHERE t.uid = r.uid AND t.key = 'game_min'), 0)";

/// Each link's numbers over the span, and its chart: people who opened it and accounts made, by the hour or the day.
fn counts(c: &Connection, ids: &[i64], span: &Span) -> rusqlite::Result<(HashMap<i64, Count>, HashMap<i64, (Vec<i64>, Vec<i64>)>)> {
    let list = ids_sql(ids);
    let mut out: HashMap<i64, Count> = ids.iter().map(|i| (*i, Count::default())).collect();
    let mut chart: HashMap<i64, (Vec<i64>, Vec<i64>)> = ids.iter().map(|i| (*i, (vec![0; span.n as usize], vec![0; span.n as usize]))).collect();
    let from_day = day_of(span.since);
    {
        let mut q = c.prepare(&format!("SELECT link, COUNT(DISTINCT net), SUM(n) FROM ref_visits WHERE day >= ?1 AND link IN ({list}) GROUP BY link"))?;
        for row in q.query_map(params![from_day], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))? {
            let (link, people, opened) = row?;
            let e = out.entry(link).or_default();
            e.people = people;
            e.opened = opened;
        }
        let mut q = c.prepare(&format!("SELECT link, first FROM ref_visits WHERE day >= ?1 AND link IN ({list})"))?;
        for row in q.query_map(params![from_day], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (link, first) = row?;
            if let (Some(i), Some(ch)) = (span.slot(first), chart.get_mut(&link)) {
                ch.0[i] += 1;
            }
        }
    }
    let week_ago = now() - 7 * 86_400;
    let mut q = c.prepare(&format!("SELECT r.link, r.uid, r.at, {PLAYED}, {WEEK}, {MINUTES} FROM referrals r WHERE r.at >= ?1 AND r.link IN ({list})"))?;
    for row in q.query_map(params![span.since, week_ago], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, bool>(3)?, r.get::<_, bool>(4)?, r.get::<_, i64>(5)?)))? {
        let (link, uid, at, played, week, minutes) = row?;
        let e = out.entry(link).or_default();
        e.regs += 1;
        e.played += played as i64;
        e.week += week as i64;
        e.minutes += minutes;
        e.now += crate::live::presence(uid).is_some_and(|p| p.game > 0) as i64;
        if let (Some(i), Some(ch)) = (span.slot(at), chart.get_mut(&link)) {
            ch.1[i] += 1;
        }
    }
    Ok((out, chart))
}

struct Link {
    id: i64,
    code: String,
    name: String,
    main: bool,
    created: i64,
    archived: Option<i64>,
}

fn links_of(c: &Connection, owner: i64) -> rusqlite::Result<Vec<Link>> {
    let mut q = c.prepare("SELECT id, code, name, main, created, archived FROM ref_links WHERE owner = ?1 ORDER BY main DESC, archived IS NOT NULL, created DESC")?;
    let rows = q.query_map(params![owner], |r| Ok(Link { id: r.get(0)?, code: r.get(1)?, name: r.get(2)?, main: r.get(3)?, created: r.get(4)?, archived: r.get(5)? }))?;
    rows.collect()
}

fn link_json(l: &Link, n: &Count) -> Value {
    let mut v = n.json();
    let o = v.as_object_mut().expect("an object");
    o.insert("id".into(), json!(l.id));
    o.insert("code".into(), json!(l.code));
    o.insert("name".into(), json!(l.name));
    o.insert("main".into(), json!(l.main));
    o.insert("created".into(), json!(l.created));
    o.insert("archived".into(), json!(l.archived));
    v
}

#[derive(Deserialize)]
struct Ask {
    days: Option<i64>,
    /// Someone else's page (admins).
    uid: Option<i64>,
}

fn days_of(d: Option<i64>) -> i64 {
    match d {
        Some(d @ (1 | 7 | 30 | 90)) => d,
        Some(0) => 0,
        _ => 30,
    }
}

/// People who came by a member's links, newest first: their card, when, by which link, whether they started the
/// game and played this week (admins see their hours too).
/// Older than `before` (its time, and among those of the same second, a smaller UID than `before_uid`).
fn people(c: &Connection, ids: &[i64], before: (i64, i64), most: i64, all: bool) -> rusqlite::Result<(Vec<Value>, bool)> {
    let week_ago = now() - 7 * 86_400;
    let mut q = c.prepare(&format!(
        "SELECT r.uid, r.link, r.at, {PLAYED}, {WEEK}, {MINUTES} FROM referrals r JOIN users u ON u.uid = r.uid WHERE r.link IN ({}) AND (r.at < ?1 OR (r.at = ?1 AND r.uid < ?4)) ORDER BY r.at DESC, r.uid DESC LIMIT ?3",
        ids_sql(ids)
    ))?;
    let rows: Vec<(i64, i64, i64, bool, bool, i64)> = q
        .query_map(params![before.0, week_ago, most + 1, before.1], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let more = rows.len() as i64 > most;
    let v = rows
        .into_iter()
        .take(most as usize)
        .map(|(uid, link, at, played, week, minutes)| {
            let mut p = json!({ "uid": uid, "card": crate::people::card_of(c, uid), "link": link, "at": at, "played": played, "week": week });
            if all {
                p["hours"] = json!(minutes / 60);
            }
            p
        })
        .collect();
    Ok((v, more))
}

/// A member's page: their links with the numbers of the span, the chart, who came. Theirs, or anyone's for admins.
async fn page(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Ask>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let all = can(w.uid, REFS_ALL);
        let target = q.uid.unwrap_or(w.uid);
        if target != w.uid && !all {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        if target == w.uid && !can(w.uid, REFS_OWN) && !all {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        let days = days_of(q.days);
        let base = format!("{}/r/", app.origin);
        let v = db(&app, move |c| -> rusqlite::Result<Option<Value>> {
            if c.query_row("SELECT 1 FROM users WHERE uid = ?1", params![target], |_| Ok(())).optional()?.is_none() {
                return Ok(None);
            }
            crate::people::card_of(c, target);
            if can(target, REFS_OWN) || target == w.uid {
                ensure_main(c, target);
            }
            let links = links_of(c, target)?;
            let ids: Vec<i64> = links.iter().map(|l| l.id).collect();
            let first = links.iter().map(|l| l.created).min().unwrap_or_else(now);
            let span = Span::of(days, first);
            let (n, chart) = counts(c, &ids, &span)?;
            let mut total = Count::default();
            for x in n.values() {
                total.add(x);
            }
            // Over the whole span the same network on two days is one person.
            total.people = c.query_row(&format!("SELECT COUNT(DISTINCT net) FROM ref_visits WHERE day >= ?1 AND link IN ({})", ids_sql(&ids)), params![day_of(span.since)], |r| r.get(0))?;
            let (list, more) = people(c, &ids, (i64::MAX, i64::MAX), 40, all)?;
            let mut ch = Map::new();
            for (id, (v, r)) in chart {
                ch.insert(id.to_string(), json!({ "v": v, "r": r }));
            }
            let limit = if can(target, REFS_ALL) { LINKS_ALL } else { LINKS_OWN };
            Ok(Some(json!({
                "owner": crate::people::card_of(c, target), "mine": target == w.uid, "all": all,
                "days": days, "base": base, "limit": limit,
                "links": links.iter().map(|l| link_json(l, n.get(&l.id).unwrap_or(&Count::default()))).collect::<Vec<_>>(),
                "total": total.json(),
                "chart": { "since": span.since, "step": span.step, "n": span.n, "links": ch },
                "people": list, "more": more,
            })))
        })
        .await;
        match v {
            Ok(Some(v)) => Ok(Json(v).into_response()),
            Ok(None) => Err(fail(StatusCode::NOT_FOUND, "no-user")),
            Err(_) => Err(busy()),
        }
    })
    .await
}

#[derive(Deserialize)]
struct More {
    uid: Option<i64>,
    link: Option<i64>,
    before: i64,
    before_uid: Option<i64>,
}

/// The rest of who came, older than `before`.
async fn more_people(State(app): State<Shared>, h: HeaderMap, Query(q): Query<More>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let all = can(w.uid, REFS_ALL);
        let target = q.uid.unwrap_or(w.uid);
        if (target != w.uid || !can(w.uid, REFS_OWN)) && !all {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let ids: Vec<i64> = links_of(c, target)?.into_iter().map(|l| l.id).filter(|id| q.link.map_or(true, |k| k == *id)).collect();
            let (list, more) = people(c, &ids, (q.before, q.before_uid.unwrap_or(i64::MIN)), 40, all)?;
            Ok(json!({ "people": list, "more": more }))
        })
        .await;
        v.map(|v| Json(v).into_response()).map_err(|_| busy())
    })
    .await
}

/// Every link, by member, for admins: who brought how many, against the accounts made without a link.
async fn everyone(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Ask>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        if !can(w.uid, REFS_ALL) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        let days = days_of(q.days);
        let base = format!("{}/r/", app.origin);
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let owners = holders(c);
            for uid in &owners {
                crate::people::card_of(c, *uid);
                if can(*uid, REFS_OWN) {
                    ensure_main(c, *uid);
                }
            }
            let mut by_owner: Vec<(i64, Vec<Link>)> = Vec::new();
            for uid in owners {
                let links = links_of(c, uid)?;
                if !links.is_empty() {
                    by_owner.push((uid, links));
                }
            }
            let ids: Vec<i64> = by_owner.iter().flat_map(|(_, l)| l.iter().map(|x| x.id)).collect();
            let first = by_owner.iter().flat_map(|(_, l)| l.iter().map(|x| x.created)).min().unwrap_or_else(now);
            let span = Span::of(days, first);
            let (n, chart) = counts(c, &ids, &span)?;
            let mut sum = Count::default();
            let (mut visits, mut regs) = (vec![0i64; span.n as usize], vec![0i64; span.n as usize]);
            for (v, r) in chart.values() {
                for i in 0..span.n as usize {
                    visits[i] += v[i];
                    regs[i] += r[i];
                }
            }
            let mut rows: Vec<Value> = by_owner
                .iter()
                .map(|(uid, links)| {
                    let mut t = Count::default();
                    for l in links {
                        if let Some(x) = n.get(&l.id) {
                            t.add(x);
                        }
                    }
                    sum.add(&t);
                    json!({
                        "card": crate::people::card_of(c, *uid), "total": t.json(),
                        "links": links.iter().map(|l| link_json(l, n.get(&l.id).unwrap_or(&Count::default()))).collect::<Vec<_>>(),
                    })
                })
                .collect();
            rows.sort_by_key(|r| std::cmp::Reverse((r["total"]["regs"].as_i64().unwrap_or(0), r["total"]["people"].as_i64().unwrap_or(0))));
            sum.people = c.query_row(&format!("SELECT COUNT(DISTINCT net) FROM ref_visits WHERE day >= ?1 AND link IN ({})", ids_sql(&ids)), params![day_of(span.since)], |r| r.get(0))?;
            // The accounts made in the span without any link, by the same slots.
            let mut organic = vec![0i64; span.n as usize];
            let mut q = c.prepare("SELECT created FROM users u WHERE created >= ?1 AND NOT EXISTS (SELECT 1 FROM referrals r WHERE r.uid = u.uid)")?;
            let made: Vec<i64> = q.query_map(params![span.since], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            for t in &made {
                if let Some(i) = span.slot(*t) {
                    organic[i] += 1;
                }
            }
            Ok(json!({
                "days": days, "base": base, "owners": rows, "total": sum.json(), "organic": made.len(),
                "chart": { "since": span.since, "step": span.step, "n": span.n, "v": visits, "r": regs, "o": organic },
            }))
        })
        .await;
        v.map(|v| Json(v).into_response()).map_err(|_| busy())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ the links themselves

#[derive(Deserialize)]
struct NewLink {
    name: String,
    code: String,
}

async fn make(State(app): State<Shared>, h: HeaderMap, Json(b): Json<NewLink>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        if !can(w.uid, REFS_OWN) && !can(w.uid, REFS_ALL) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        let name = clean_line(&b.name, 40);
        let code = b.code.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err(said(StatusCode::BAD_REQUEST, "name", "Give it a name: where it goes."));
        }
        if !code_ok(&code) {
            return Err(said(StatusCode::BAD_REQUEST, "code", "3–24 of a–z, 0–9, _ and -, a letter or a digit at both ends."));
        }
        let uid = w.uid;
        let limit = if can(uid, REFS_ALL) { LINKS_ALL } else { LINKS_OWN };
        let made = db(&app, move |c| -> Result<i64, &'static str> {
            ensure_main(c, uid);
            let has: i64 = c.query_row("SELECT COUNT(*) FROM ref_links WHERE owner = ?1 AND main = 0", params![uid], |r| r.get(0)).map_err(|_| "busy")?;
            if has >= limit {
                return Err("limit");
            }
            if !code_free(c, &code, uid) {
                return Err("taken");
            }
            c.execute("INSERT INTO ref_links (owner, code, name, main, created) VALUES (?1, ?2, ?3, 0, ?4)", params![uid, code, name, now()]).map_err(|_| "taken")?;
            let id = c.last_insert_rowid();
            crate::people::audit(c, uid, "ref.link", &id.to_string(), json!({ "code": code, "name": name }));
            Ok(id)
        })
        .await;
        match made {
            Ok(id) => Ok(Json(json!({ "id": id })).into_response()),
            Err("limit") => Err(said(StatusCode::CONFLICT, "limit", &format!("Up to {limit} links besides your own."))),
            Err("taken") => Err(said(StatusCode::CONFLICT, "taken", "This code is taken.")),
            Err(_) => Err(busy()),
        }
    })
    .await
}

#[derive(Deserialize)]
struct Change {
    name: Option<String>,
    archived: Option<bool>,
}

/// A link renamed, put away or brought back: by its owner, or an admin. The personal link stays as it is.
async fn change(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<Change>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let all = can(uid, REFS_ALL);
        let name = b.name.map(|n| clean_line(&n, 40));
        if name.as_deref() == Some("") {
            return Err(said(StatusCode::BAD_REQUEST, "name", "Give it a name: where it goes."));
        }
        let done = db(&app, move |c| -> Result<(), &'static str> {
            let (owner, main): (i64, bool) = c.query_row("SELECT owner, main FROM ref_links WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(|_| "busy")?.ok_or("none")?;
            if owner != uid && !all {
                return Err("none");
            }
            if main {
                return Err("main");
            }
            if let Some(n) = &name {
                c.execute("UPDATE ref_links SET name = ?2 WHERE id = ?1", params![id, n]).map_err(|_| "busy")?;
            }
            if let Some(a) = b.archived {
                c.execute("UPDATE ref_links SET archived = ?2 WHERE id = ?1", params![id, a.then(now)]).map_err(|_| "busy")?;
            }
            crate::people::audit(c, uid, "ref.change", &id.to_string(), json!({ "name": name, "archived": b.archived }));
            Ok(())
        })
        .await;
        match done {
            Ok(()) => Ok(Json(json!({})).into_response()),
            Err("none") => Err(fail(StatusCode::NOT_FOUND, "no-link")),
            Err("main") => Err(said(StatusCode::CONFLICT, "main", "Your own link stays as it is.")),
            Err(_) => Err(busy()),
        }
    })
    .await
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(4096);
    Router::new()
        .route("/api/ref/hit", post(hit).layer(json()))
        .route("/api/refs", get(page).post(make).layer(json()))
        .route("/api/refs/people", get(more_people))
        .route("/api/refs/all", get(everyone))
        .route("/api/refs/{id}", axum::routing::patch(change).layer(json()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        for ok in ["lunuca", "tg-post", "yt_09", "a1b", "abcdefghijklmnopqrstuvwx"] {
            assert!(code_ok(ok), "{ok}");
        }
        for bad in ["ab", "-ab", "ab-", "_ab", "Abc", "a b", "абв", "abcdefghijklmnopqrstuvwxy", "a.b", ""] {
            assert!(!code_ok(bad), "{bad}");
        }
    }

    #[test]
    fn days_and_spans() {
        // Moscow's midnight starts the day: 21:00 UTC.
        assert_eq!(day_start(day_of(0)), -MSK);
        assert_eq!(day_of(21 * 3600 - 1), 0);
        assert_eq!(day_of(21 * 3600), 1);
        let s = Span::of(1, 0);
        assert_eq!((s.step, s.n), (3600, 24));
        assert_eq!(s.slot(s.since), Some(0));
        assert_eq!(s.slot(s.since + 23 * 3600 + 59), Some(23));
        assert_eq!(s.slot(s.since - 1), None);
        let w = Span::of(7, 0);
        assert_eq!((w.step, w.n), (86_400, 7));
        // All of it: from the first link, at least a week, at most a year.
        let a = Span::of(0, now());
        assert_eq!(a.n, 7);
        let y = Span::of(0, 0);
        assert_eq!(y.n, 365);
    }

    #[test]
    fn personal_codes_and_attaching() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE users (uid INTEGER PRIMARY KEY, login TEXT, login_key TEXT UNIQUE, created INTEGER);
             INSERT INTO users VALUES (1, 'Blogger_', 'blogger_', 0), (2, 'tg-post', 'tg-post', 0), (3, 'Newbie', 'newbie', 0), (4, 'ab', 'ab', 0), (0, 'Lunuca', 'lunuca', 0);",
        )
        .unwrap();
        schema(&c);
        let id = ensure_main(&c, 1).unwrap();
        assert_eq!(ensure_main(&c, 1), Some(id));
        let code: String = c.query_row("SELECT code FROM ref_links WHERE id = ?1", params![id], |r| r.get(0)).unwrap();
        assert_eq!(code, "blogger");
        // A short login gets its number.
        let short = ensure_main(&c, 4).unwrap();
        let code4: String = c.query_row("SELECT code FROM ref_links WHERE id = ?1", params![short], |r| r.get(0)).unwrap();
        assert_eq!(code4, "uid4");
        // Somebody else's login is not free; one's own is.
        assert!(!code_free(&c, "tg-post", 1));
        assert!(code_free(&c, "tg-post", 2));
        assert!(!code_free(&c, "blogger", 2));
        assert!(!code_free(&c, "admin", 1));
        // Her login is reserved for everyone else, and hers.
        assert!(!code_free(&c, "lunuca", 1));
        let hers = ensure_main(&c, 0).unwrap();
        let code0: String = c.query_row("SELECT code FROM ref_links WHERE id = ?1", params![hers], |r| r.get(0)).unwrap();
        assert_eq!(code0, "lunuca");
        attach(&c, 3, "Blogger");
        let link: i64 = c.query_row("SELECT link FROM referrals WHERE uid = 3", [], |r| r.get(0)).unwrap();
        assert_eq!(link, id);
        // Nobody by their own link, nothing by a code of nothing or by a link put away.
        attach(&c, 1, "blogger");
        attach(&c, 2, "nothing");
        c.execute("UPDATE ref_links SET archived = 1 WHERE id = ?1", params![id]).unwrap();
        attach(&c, 4, "blogger");
        let n: i64 = c.query_row("SELECT COUNT(*) FROM referrals", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }
}
