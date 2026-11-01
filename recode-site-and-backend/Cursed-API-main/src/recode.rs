//! Cursed Recode's subscriptions (her decisions, 15.10.2026). Sold only as keys — on FunPay (FreeKassa wants a turnover
//! first) — and a key is activated in one's cabinet on the site: it adds its days to the account's Recode, «forever»
//! over all. Keys come in the site's terms (1, 2, 7, 14, 30, 90, 180 days, forever) and look like RCD-XXXXX-XXXXX-XXXXX
//! (no 0/O, 1/I). Making, listing and revoking keys is hers alone (the site's «Ключи»), not the staff's.
//! Days are given by hand only by her (her card of a member) and by us from the VDS's console (her word, 19.10.2026):
//! whatever role an account holds, Recode is its subscription and nothing else.
//!
//! The account's Recode is not kept as a number: it is worked out from its keys, in the order they were activated
//! (each one's days from when it was activated, or from where the ones before end), so a revoked key simply drops out.
//! A used key stays used, even when its account is deleted: a key is never good twice.
//!
//! Its loader (her decisions, 15.10.2026) signs in through the site — it shows a code, the site allows it — and plays
//! Recode's channel of builds; an account plays it on one computer (beta.rs, freed once a day) while its Recode is on.
//! The game shows its ticket at its start and asks again every 30 minutes (CHECK_EVERY).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rand_core::{OsRng, RngCore};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::beta::{admit_for, ban_of, free_device, no_origin, reset_from, Ban, Parts, Product, Refusal, ACCESS_SECONDS, RESET_EVERY, TICKET_SECONDS};
use crate::market::{clean_line, member, run, said};
use crate::{db, digest, fail, new_session, now, presented, random_hex, who, Shared, OWNER};

/// The terms a key may give, in days (0: forever): the site's prices'.
pub const TERMS: [i64; 8] = [1, 2, 7, 14, 30, 90, 180, 0];
/// The letters of a key: no 0/O, no 1/I (32 of them, 5 bits each; 15 of them a key: 75 bits).
const LETTERS: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
/// Most keys made at once.
const BATCH_MOST: i64 = 500;
/// Wrong keys an account may try before it waits, and how long.
const TRIES: u32 = 8;
const TRIES_WINDOW: Duration = Duration::from_secs(60 * 60);

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS recode_keys (
             code TEXT PRIMARY KEY,                -- RCD-XXXXX-XXXXX-XXXXX
             days INTEGER NOT NULL,                -- 0: forever
             batch INTEGER NOT NULL,               -- made together
             note TEXT NOT NULL DEFAULT '',        -- hers, for the batch (where it went)
             made INTEGER NOT NULL,
             used_by INTEGER,                      -- the account it went to (no reference: a used key stays used)
             used_login TEXT,                      -- its login then
             used_at INTEGER,
             revoked INTEGER);                     -- when she took it back
         CREATE INDEX IF NOT EXISTS recode_keys_user ON recode_keys(used_by);
         CREATE INDEX IF NOT EXISTS recode_keys_batch ON recode_keys(batch);
         CREATE TABLE IF NOT EXISTS loader_links (
             code TEXT PRIMARY KEY,                -- 8 letters, shown K7Q2-M9PX
             key BLOB NOT NULL,                    -- the asking loader's own key, hashed
             device TEXT NOT NULL,                 -- its computer's parts
             product TEXT NOT NULL,
             made INTEGER NOT NULL,
             uid INTEGER,                          -- who allowed it
             allowed INTEGER,
             denied INTEGER);
         CREATE TABLE IF NOT EXISTS recode_live (
             uid INTEGER PRIMARY KEY,              -- the account's one game now
             session BLOB NOT NULL,                -- its session's hash
             device TEXT NOT NULL,
             since INTEGER NOT NULL,
             seen INTEGER NOT NULL);               -- when it last said it is alive
         CREATE TABLE IF NOT EXISTS recode_flags (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             uid INTEGER NOT NULL,
             login TEXT NOT NULL,
             kind TEXT NOT NULL,                   -- autoban, protect, tpm-changed, tpm-off, second-game, resets, sight, touch-unverified
             detail TEXT NOT NULL DEFAULT '{}',
             at INTEGER NOT NULL,
             verdict TEXT,                         -- ok, ban (an autoban is one already)
             by INTEGER,
             done INTEGER);
         CREATE INDEX IF NOT EXISTS recode_flags_uid ON recode_flags(uid);
         CREATE TABLE IF NOT EXISTS recode_resets (uid INTEGER NOT NULL, at INTEGER NOT NULL);",
    )
    .expect("make the keys' table");
    // A ticket is for one channel's game (the beta's made before have none: theirs)
    let has: bool = c.prepare("SELECT 1 FROM pragma_table_info('tickets') WHERE name = 'channel'").and_then(|mut q| q.exists([])).unwrap_or(false);
    if !has {
        c.execute("ALTER TABLE tickets ADD COLUMN channel TEXT NOT NULL DEFAULT 'beta'", []).expect("the tickets' channel");
    }
    // A key given by hand to one account says who gave it (-1: the console); the keys made to be sold say nothing
    let given: bool = c.prepare("SELECT 1 FROM pragma_table_info('recode_keys') WHERE name = 'given'").and_then(|mut q| q.exists([])).unwrap_or(false);
    if !given {
        c.execute("ALTER TABLE recode_keys ADD COLUMN given INTEGER", []).expect("the keys' giver");
    }
    // A key that makes whoever activates it a beta tester though the beta is closed (1; her word, 20.10)
    let beta: bool = c.prepare("SELECT 1 FROM pragma_table_info('recode_keys') WHERE name = 'beta'").and_then(|mut q| q.exists([])).unwrap_or(false);
    if !beta {
        c.execute("ALTER TABLE recode_keys ADD COLUMN beta INTEGER", []).expect("the keys' beta");
    }
    // Who made a key (-1: the console): a holder of the role «Ключи» sees and revokes only their own (her word, 23.10);
    // the keys made before say nothing — hers
    let maker: bool = c.prepare("SELECT 1 FROM pragma_table_info('recode_keys') WHERE name = 'maker'").and_then(|mut q| q.exists([])).unwrap_or(false);
    if !maker {
        c.execute("ALTER TABLE recode_keys ADD COLUMN maker INTEGER", []).expect("the keys' maker");
    }
}

/// A new key's code.
fn new_code() -> String {
    let mut b = [0u8; 15];
    OsRng.fill_bytes(&mut b);
    let s: String = b.iter().map(|x| LETTERS[(*x & 31) as usize] as char).collect();
    format!("RCD-{}-{}-{}", &s[0..5], &s[5..10], &s[10..15])
}

/// What was typed, as a key's code: any case, spaces or dashes anywhere, with or without the RCD; None if it cannot be
/// one.
pub fn normal(typed: &str) -> Option<String> {
    let mut s: String = typed.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    if s.len() == 18 && s.starts_with("RCD") {
        s.drain(..3);
    }
    if s.len() != 15 || !s.bytes().all(|b| LETTERS.contains(&b)) {
        return None;
    }
    Some(format!("RCD-{}-{}-{}", &s[0..5], &s[5..10], &s[10..15]))
}

/// An account's Recode: forever, or until when (0: never had it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sub {
    pub forever: bool,
    pub until: i64,
}

impl Sub {
    pub fn active(&self, at: i64) -> bool {
        self.forever || self.until > at
    }

    pub fn json(&self, at: i64) -> Value {
        json!({ "active": self.active(at), "forever": self.forever, "until": if self.until > 0 { json!(self.until) } else { Value::Null } })
    }
}

/// The keys' days laid end to end in the order they were activated: (days, when).
pub fn sum(keys: &[(i64, i64)]) -> Sub {
    let mut s = Sub { forever: false, until: 0 };
    for &(days, at) in keys {
        if days == 0 {
            s.forever = true;
        } else {
            s.until = s.until.max(at) + days * 86_400;
        }
    }
    s
}

/// The account's Recode now. Hers is forever (the client is hers).
pub fn of(c: &Connection, uid: i64) -> Sub {
    if crate::is_owner(uid) {
        return Sub { forever: true, until: 0 };
    }
    let keys: Vec<(i64, i64)> = c
        .prepare("SELECT days, used_at FROM recode_keys WHERE used_by = ?1 AND revoked IS NULL ORDER BY used_at, rowid")
        .and_then(|mut q| q.query_map(params![uid], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
        .unwrap_or_default();
    sum(&keys)
}

/// A term in words, for the journal.
fn term(days: i64) -> String {
    if days == 0 {
        "forever".into()
    } else {
        format!("{days}d")
    }
}

/// A term as typed in the console: 30, 30d or forever; None if it is not one of the site's.
pub fn term_typed(s: &str) -> Option<i64> {
    let days = if s.eq_ignore_ascii_case("forever") { 0 } else { s.trim_end_matches('d').parse().ok()? };
    TERMS.contains(&days).then_some(days)
}

/// The next batch's number.
fn next_batch(c: &Connection) -> Result<i64, &'static str> {
    c.query_row("SELECT COALESCE(MAX(batch), 0) + 1 FROM recode_keys", [], |r| r.get(0)).map_err(|_| "busy")
}

/// A batch of new keys (the site's «Ключи», or the console): its number, its codes, when.
pub fn make_keys(c: &mut Connection, by: i64, days: i64, count: i64, note: &str) -> Result<(i64, Vec<String>, i64), &'static str> {
    if !TERMS.contains(&days) {
        return Err("term");
    }
    if !(1..=BATCH_MOST).contains(&count) {
        return Err("count");
    }
    let tx = c.transaction().map_err(|_| "busy")?;
    let batch = next_batch(&tx)?;
    let at = now();
    let mut codes = Vec::with_capacity(count as usize);
    while codes.len() < count as usize {
        let code = new_code();
        // (a code met before is simply drawn again)
        if tx.execute("INSERT OR IGNORE INTO recode_keys (code, days, batch, note, made, maker) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![code, days, batch, note, at, by]).unwrap_or(0) == 1 {
            codes.push(code);
        }
    }
    crate::people::audit(&tx, by, "recode.keys.make", &format!("batch:{batch}"), json!({ "term": term(days), "count": count, "note": note }));
    tx.commit().map_err(|_| "busy")?;
    Ok((batch, codes, at))
}

/// A batch of beta keys (her word, 20.10, after the beta closed): whoever activates one is a beta tester for good,
/// whenever it is and however many places are taken — the role and the badge with it.
pub fn make_beta_keys(c: &mut Connection, by: i64, days: i64, count: i64, note: &str) -> Result<(i64, Vec<String>, i64), &'static str> {
    let (batch, codes, at) = make_keys(c, by, days, count, note)?;
    c.execute("UPDATE recode_keys SET beta = 1 WHERE batch = ?1", params![batch]).map_err(|_| "busy")?;
    crate::people::audit(c, by, "recode.keys.beta", &format!("batch:{batch}"), json!({ "count": codes.len() }));
    Ok((batch, codes, at))
}

/// Days given by hand to one account: a key of a batch of its own, made and activated at once and marked as given, so
/// it is among her keys like any other and revoked like any other.
pub fn give(c: &mut Connection, by: i64, uid: i64, days: i64, note: &str) -> Result<Sub, &'static str> {
    if !TERMS.contains(&days) {
        return Err("term");
    }
    let tx = c.transaction().map_err(|_| "busy")?;
    let login: String = tx.query_row("SELECT login FROM users WHERE uid = ?1", params![uid], |r| r.get(0)).map_err(|_| "missing")?;
    let batch = next_batch(&tx)?;
    let at = now();
    let note = if note.is_empty() { "выдано вручную".to_string() } else { format!("выдано вручную · {note}") };
    let code = loop {
        let code = new_code();
        if tx
            .execute(
                "INSERT OR IGNORE INTO recode_keys (code, days, batch, note, made, used_by, used_login, used_at, given, maker) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?5, ?8, ?8)",
                params![code, days, batch, note, at, uid, login, by],
            )
            .unwrap_or(0)
            == 1
        {
            break code;
        }
    };
    crate::people::audit(&tx, by, "recode.give", &format!("user:{uid}"), json!({ "key": &code[code.len() - 5..], "term": term(days) }));
    let sub = of(&tx, uid);
    tx.commit().map_err(|_| "busy")?;
    marks(c, uid);
    Ok(sub)
}

/// What was given by hand taken back: the keys bought stay (those she revokes one by one in «Ключи»). `only`: what
/// that one gave (a holder of the role «Ключи» takes back their own), all of it when none.
pub fn take(c: &Connection, by: i64, uid: i64, only: Option<i64>) -> Result<(usize, Sub), &'static str> {
    let n = c
        .execute(
            "UPDATE recode_keys SET revoked = ?2 WHERE used_by = ?1 AND given IS NOT NULL AND revoked IS NULL AND (?3 IS NULL OR given = ?3)",
            params![uid, now(), only],
        )
        .map_err(|_| "busy")?;
    if n > 0 {
        crate::people::audit(c, by, "recode.take", &format!("user:{uid}"), json!({ "keys": n }));
        marks(c, uid);
    }
    Ok((n, of(c, uid)))
}

/// Revoke every activated entitlement, including purchases. Requires a separate
/// capability; normal key makers retain their own-key/own-gift limits.
pub fn take_all(c: &mut Connection, by: i64, uid: i64) -> Result<(usize, Sub), &'static str> {
    if !crate::people::can(by, crate::people::RECODE_REVOKE) { return Err("forbidden"); }
    if crate::is_owner(uid) { return Err("owner-protected"); }
    let tx = c.transaction().map_err(|_| "busy")?;
    let n = tx.execute("UPDATE recode_keys SET revoked = ?2 WHERE used_by = ?1 AND revoked IS NULL", params![uid, now()]).map_err(|_| "busy")?;
    if n > 0 {
        crate::people::audit(&tx, by, "recode.revoke_all", &format!("user:{uid}"), json!({ "keys": n }));
    }
    tx.commit().map_err(|_| "busy")?;
    marks(c, uid);
    Ok((n, of(c, uid)))
}

/// The places of Recode's beta (her word, 19.10): the first seventy accounts to activate a key bought — not one given
/// by hand — are its beta testers, for good; after them the beta is sold no more. A tester she makes one by hand takes
/// no place (her word, the same evening): the seventy are sales.
pub const BETA_PLACES: i64 = 70;
/// The beta closed (her word, 20.10.2026 21:30 MSK, with the prices cut): only a key made before it is the beta's, so
/// one sold since gives Recode and no place; a beta key she hands back to its buyer still does.
pub const BETA_CLOSED: i64 = 1_792_521_000;

/// Recode's marks on an account made to agree with its keys (after an activation, a gift, a taking back, a key revoked,
/// and every five minutes for the Recodes that ran out): the role «Recode» while its Recode is on; for a key bought,
/// while the beta has places, the role «Бета-тестер» and the badge «Бета Recode», for good; the badges «Recode» and
/// «Навсегда».
pub fn marks(c: &Connection, uid: i64) {
    let sub = of(c, uid);
    let on = sub.active(now());
    crate::people::follow_role(c, uid, "recode", on);
    if on {
        crate::people::award(c, uid, "recode", 1, None);
    }
    if sub.forever {
        crate::people::award(c, uid, "forever", 1, None);
    }
    let tester = c.query_row("SELECT 1 FROM roles WHERE uid = ?1 AND role = 'rc_beta'", params![uid], |_| Ok(())).optional().ok().flatten().is_some();
    if !crate::is_owner(uid) && !tester {
        let bought = c
            .query_row(
                "SELECT 1 FROM recode_keys WHERE used_by = ?1 AND given IS NULL AND revoked IS NULL AND made < ?2",
                params![uid, BETA_CLOSED],
                |_| Ok(()),
            )
            .optional()
            .ok()
            .flatten()
            .is_some();
        let taken: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM roles WHERE role = 'rc_beta' AND EXISTS (SELECT 1 FROM recode_keys WHERE used_by = roles.uid AND given IS NULL)",
                [],
                |r| r.get(0),
            )
            .unwrap_or(BETA_PLACES);
        // a beta key (make_beta_keys) makes a tester whatever the date and the places
        let beta_key = c
            .query_row("SELECT 1 FROM recode_keys WHERE used_by = ?1 AND beta = 1 AND revoked IS NULL", params![uid], |_| Ok(()))
            .optional()
            .ok()
            .flatten()
            .is_some();
        if (beta_key || bought && taken < BETA_PLACES) && crate::people::follow_role(c, uid, "rc_beta", true) {
            crate::people::award(c, uid, "rc_beta", 1, None);
            let how = if beta_key { json!({ "key": "beta" }) } else { json!({ "place": taken + 1 }) };
            crate::people::audit(c, uid, "recode.beta", &format!("user:{uid}"), how);
        }
    }
}

/// Every account with a key on it, or the role «Recode», or hers: their marks again (live.rs every five minutes, and
/// the badges' sweep).
pub fn sweep_marks(c: &Connection) {
    let uids: Vec<i64> = c
        .prepare(
            "SELECT used_by FROM recode_keys WHERE used_by IN (SELECT uid FROM users)
             UNION SELECT uid FROM roles WHERE role = 'recode' UNION SELECT uid FROM users WHERE uid IN (0, 19)",
        )
        .and_then(|mut q| q.query_map([], |r| r.get(0)).map(|m| m.flatten().collect()))
        .unwrap_or_default();
    for uid in uids {
        marks(c, uid);
    }
}

/// An account's Recode on her card of it: the subscription, how many keys are on it, and what was given by hand and
/// still counts (by whom: null for the console).
pub fn card(c: &Connection, uid: i64, only: Option<i64>) -> Value {
    let mut v = of(c, uid).json(now());
    v["owner"] = json!(crate::is_owner(uid));
    v["keys"] = json!(c.query_row("SELECT COUNT(*) FROM recode_keys WHERE used_by = ?1 AND revoked IS NULL", params![uid], |r| r.get::<_, i64>(0)).unwrap_or(0));
    // (a holder of the role «Ключи» sees what they gave themselves: `only`)
    let given: Vec<(String, i64, i64, i64)> = c
        .prepare(
            "SELECT code, days, used_at, given FROM recode_keys WHERE used_by = ?1 AND given IS NOT NULL AND revoked IS NULL AND (?2 IS NULL OR given = ?2)
             ORDER BY used_at DESC, rowid DESC",
        )
        .and_then(|mut q| q.query_map(params![uid, only], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect())
        .unwrap_or_default();
    v["given"] = json!(given
        .iter()
        .map(|(code, days, at, by)| json!({ "key": &code[code.len() - 5..], "days": days, "at": at, "by": if *by < 0 { Value::Null } else { crate::people::card_of(c, *by) } }))
        .collect::<Vec<_>>());
    v
}

// ---------------------------------------------------------------------------------------------- the member's

/// Wrong keys per account, for a while.
static WRONG: Mutex<Option<HashMap<i64, (u32, Instant)>>> = Mutex::new(None);

fn wrong_tries(uid: i64) -> u32 {
    let mut g = WRONG.lock().unwrap();
    let m = g.get_or_insert_with(HashMap::new);
    m.retain(|_, (_, at)| at.elapsed() < TRIES_WINDOW);
    m.get(&uid).map_or(0, |e| e.0)
}

fn wrong_once(uid: i64) {
    let mut g = WRONG.lock().unwrap();
    let e = g.get_or_insert_with(HashMap::new).entry(uid).or_insert((0, Instant::now()));
    e.0 += 1;
}

/// GET /api/recode: the account's Recode and the keys it took.
async fn mine(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| {
            let at = now();
            let keys: Vec<Value> = c
                .prepare("SELECT code, days, used_at, revoked FROM recode_keys WHERE used_by = ?1 ORDER BY used_at DESC, rowid DESC")
                .and_then(|mut q| {
                    q.query_map(params![uid], |r| {
                        let code: String = r.get(0)?;
                        Ok(json!({ "code": format!("RCD-•••••-•••••-{}", &code[code.len() - 5..]), "days": r.get::<_, i64>(1)?, "at": r.get::<_, i64>(2)?, "revoked": r.get::<_, Option<i64>>(3)? }))
                    })?
                    .collect()
                })
                .unwrap_or_default();
            let mut v = of(c, uid).json(at);
            v["keys"] = json!(keys);
            v["owner"] = json!(crate::is_owner(uid));
            // (the site's download button: a build of Recode is out)
            v["loader"] = json!(crate::dist::loader_sha(c, "recode").is_some());
            v
        })
        .await;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Activate {
    key: String,
}

/// POST /api/recode/activate {key}: the key's days onto the account.
async fn activate(State(app): State<Shared>, h: HeaderMap, Json(a): Json<Activate>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if wrong_tries(uid) >= TRIES {
            return Err(said(StatusCode::TOO_MANY_REQUESTS, "key-tries", "Too many wrong keys: wait an hour."));
        }
        let Some(code) = normal(&a.key) else {
            wrong_once(uid);
            return Err(said(StatusCode::BAD_REQUEST, "key-wrong", "No such key."));
        };
        let login = w.login.clone();
        let done = db(&app, move |c| -> Result<(i64, Sub), &'static str> {
            let tx = c.transaction().map_err(|_| "busy")?;
            let row: Option<(i64, Option<i64>, Option<i64>)> = tx
                .query_row("SELECT days, used_by, revoked FROM recode_keys WHERE code = ?1", params![code], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()
                .map_err(|_| "busy")?;
            let Some((days, used_by, revoked)) = row else { return Err("key-wrong") };
            if revoked.is_some() {
                return Err("key-revoked");
            }
            if let Some(by) = used_by {
                return Err(if by == uid { "key-yours" } else { "key-used" });
            }
            let at = now();
            if tx
                .execute("UPDATE recode_keys SET used_by = ?2, used_login = ?3, used_at = ?4 WHERE code = ?1 AND used_by IS NULL AND revoked IS NULL", params![code, uid, login, at])
                .map_err(|_| "busy")?
                != 1
            {
                return Err("key-used");
            }
            let sub = of(&tx, uid);
            crate::people::audit(&tx, uid, "recode.activate", &format!("user:{uid}"), json!({ "key": &code[code.len() - 5..], "term": term(days) }));
            tx.commit().map_err(|_| "busy")?;
            marks(c, uid);
            Ok((days, sub))
        })
        .await;
        match done {
            Ok((days, sub)) => {
                let mut v = sub.json(now());
                v["days"] = json!(days);
                Ok(Json(v).into_response())
            }
            Err("key-wrong") => {
                wrong_once(uid);
                Err(said(StatusCode::NOT_FOUND, "key-wrong", "No such key."))
            }
            Err("key-used") => Err(said(StatusCode::CONFLICT, "key-used", "This key was already activated.")),
            Err("key-yours") => Err(said(StatusCode::CONFLICT, "key-yours", "You have already activated this key.")),
            Err("key-revoked") => Err(said(StatusCode::GONE, "key-revoked", "This key was revoked.")),
            Err(_) => Err(fail(StatusCode::SERVICE_UNAVAILABLE, "busy")),
        }
    })
    .await
}

// ------------------------------------------------------------------------------------------------- hers

/// Whose keys one works with: hers are all of them; a holder of the role «Ключи» (her word, 23.10.2026) has the
/// batches they made and what they gave by hand, ten keys at a time.
#[derive(Clone, Copy)]
enum Keys {
    All,
    Own(i64),
}

impl Keys {
    /// The maker (or giver) a list is cut down to: none for her.
    fn only(self) -> Option<i64> {
        match self {
            Keys::All => None,
            Keys::Own(uid) => Some(uid),
        }
    }

    fn most(self) -> i64 {
        match self {
            Keys::All => BATCH_MOST,
            Keys::Own(_) => BATCH_MOST_OWN,
        }
    }
}

/// The most keys at once for a holder of the role «Ключи» (her word, 23.10).
const BATCH_MOST_OWN: i64 = 10;

fn keys_of(uid: i64) -> Result<Keys, Response> {
    if crate::is_owner(uid) {
        Ok(Keys::All)
    } else if crate::people::can(uid, crate::people::RECODE_KEYS) {
        Ok(Keys::Own(uid))
    } else {
        Err(fail(StatusCode::FORBIDDEN, "owner"))
    }
}

fn hers(uid: i64) -> Result<(), Response> {
    if crate::is_owner(uid) {
        Ok(())
    } else {
        Err(fail(StatusCode::FORBIDDEN, "owner"))
    }
}

#[derive(Deserialize)]
struct Make {
    days: i64,
    count: i64,
    #[serde(default)]
    note: String,
    /// Beta keys (make_beta_keys): whoever activates one is a beta tester.
    #[serde(default)]
    beta: bool,
}

/// POST /api/admin/keys {days, count, note, beta}: a batch of new keys (hers, or the role «Ключи»'s, ten at a time).
async fn make(State(app): State<Shared>, h: HeaderMap, Json(m): Json<Make>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let keys = keys_of(w.uid)?;
        if !TERMS.contains(&m.days) {
            return Err(said(StatusCode::BAD_REQUEST, "term", "No such term."));
        }
        if !(1..=keys.most()).contains(&m.count) {
            return Err(said(StatusCode::BAD_REQUEST, "count", &format!("From 1 to {} keys at once.", keys.most())));
        }
        let note = clean_line(&m.note, 80);
        let by = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let made = if m.beta { make_beta_keys(c, by, m.days, m.count, &note) } else { make_keys(c, by, m.days, m.count, &note) };
            let (batch, codes, at) = made.map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            Ok(json!({ "batch": batch, "days": m.days, "note": note, "made": at, "codes": codes, "beta": m.beta }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct List {
    #[serde(default)]
    state: String, // free, used, revoked (all when empty)
    #[serde(default)]
    batch: Option<i64>,
    #[serde(default)]
    q: String, // part of a code or a login
    #[serde(default)]
    page: Option<i64>,
}

/// GET /api/admin/keys: the keys (hers all, the role «Ключи»'s their own), newest first, 100 a page; the batches and
/// the counts.
async fn list(State(app): State<Shared>, h: HeaderMap, Query(l): Query<List>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let keys = keys_of(w.uid)?;
        let only = keys.only();
        let v = db(&app, move |c| {
            let state = match l.state.as_str() {
                "free" => "used_by IS NULL AND revoked IS NULL",
                "used" => "used_by IS NOT NULL AND revoked IS NULL",
                "revoked" => "revoked IS NOT NULL",
                _ => "1",
            };
            let q = l.q.trim().to_uppercase();
            let page = l.page.unwrap_or(0).max(0);
            let sql = format!(
                "SELECT code, days, batch, note, made, used_by, used_login, used_at, revoked FROM recode_keys
                 WHERE {state} AND (?1 IS NULL OR batch = ?1) AND (?2 = '' OR code LIKE '%' || ?2 || '%' OR UPPER(COALESCE(used_login, '')) LIKE '%' || ?2 || '%')
                   AND (?4 IS NULL OR maker = ?4)
                 ORDER BY made DESC, batch DESC, rowid LIMIT 101 OFFSET ?3"
            );
            let mut items: Vec<Value> = c
                .prepare(&sql)
                .and_then(|mut s| {
                    s.query_map(params![l.batch, q, page * 100, only], |r| {
                        let used_by: Option<i64> = r.get(5)?;
                        Ok(json!({
                            "code": r.get::<_, String>(0)?, "days": r.get::<_, i64>(1)?, "batch": r.get::<_, i64>(2)?, "note": r.get::<_, String>(3)?,
                            "made": r.get::<_, i64>(4)?,
                            "used": used_by.map(|uid| json!({ "uid": uid, "login": r.get::<_, Option<String>>(6).ok().flatten(), "at": r.get::<_, Option<i64>>(7).ok().flatten() })),
                            "revoked": r.get::<_, Option<i64>>(8)?,
                        }))
                    })?
                    .collect()
                })
                .unwrap_or_default();
            let more = items.len() > 100;
            items.truncate(100);
            let batches: Vec<Value> = c
                .prepare(
                    "SELECT batch, days, note, MIN(made), COUNT(*), SUM(used_by IS NOT NULL AND revoked IS NULL), SUM(revoked IS NOT NULL), MAX(beta IS NOT NULL)
                     FROM recode_keys WHERE ?1 IS NULL OR maker = ?1 GROUP BY batch ORDER BY batch DESC LIMIT 200",
                )
                .and_then(|mut s| {
                    s.query_map(params![only], |r| {
                        let count: i64 = r.get(4)?;
                        let used: i64 = r.get(5)?;
                        let revoked: i64 = r.get(6)?;
                        Ok(json!({ "batch": r.get::<_, i64>(0)?, "days": r.get::<_, i64>(1)?, "note": r.get::<_, String>(2)?, "made": r.get::<_, i64>(3)?,
                                   "count": count, "used": used, "revoked": revoked, "free": count - used - revoked, "beta": r.get::<_, i64>(7)? == 1 }))
                    })?
                    .collect()
                })
                .unwrap_or_default();
            let n = |w: &str| -> i64 { c.query_row(&format!("SELECT COUNT(*) FROM recode_keys WHERE {w} AND (?1 IS NULL OR maker = ?1)"), params![only], |r| r.get(0)).unwrap_or(0) };
            json!({
                "items": items, "more": more, "page": page, "batches": batches, "terms": TERMS, "own": only.is_some(), "most": keys.most(),
                "counts": { "free": n("used_by IS NULL AND revoked IS NULL"), "used": n("used_by IS NOT NULL AND revoked IS NULL"), "revoked": n("revoked IS NOT NULL") },
            })
        })
        .await;
        Ok(Json(v).into_response())
    })
    .await
}

/// GET /api/admin/keys/batch/{batch}.txt: the batch's keys not yet used, one a line (for FunPay's own delivery); the
/// role «Ключи»'s of their own batches only.
async fn batch_text(State(app): State<Shared>, h: HeaderMap, Key(name): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let only = keys_of(w.uid)?.only();
        let batch: i64 = name.trim_end_matches(".txt").parse().map_err(|_| fail(StatusCode::NOT_FOUND, "no-batch"))?;
        let (codes, days): (Vec<String>, Option<i64>) = db(&app, move |c| {
            let codes = c
                .prepare("SELECT code FROM recode_keys WHERE batch = ?1 AND used_by IS NULL AND revoked IS NULL ORDER BY rowid")
                .and_then(|mut s| s.query_map(params![batch], |r| r.get(0))?.collect())
                .unwrap_or_default();
            let days = c
                .query_row("SELECT days FROM recode_keys WHERE batch = ?1 AND (?2 IS NULL OR maker = ?2) LIMIT 1", params![batch, only], |r| r.get(0))
                .optional()
                .ok()
                .flatten();
            (codes, days)
        })
        .await;
        let Some(days) = days else { return Err(fail(StatusCode::NOT_FOUND, "no-batch")) };
        let mut body = codes.join("\r\n");
        body.push_str("\r\n");
        let mut res = body.into_response();
        let file = format!("attachment; filename=\"recode-{}-batch{batch}.txt\"", if days == 0 { "forever".into() } else { format!("{days}d") });
        res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
        if let Ok(v) = HeaderValue::from_str(&file) {
            res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
        }
        Ok(res)
    })
    .await
}

/// POST /api/admin/keys/{code}/revoke: the key taken back (hers any, the role «Ключи»'s their own); an activated
/// one's days go from its account.
async fn revoke(State(app): State<Shared>, h: HeaderMap, Key(code): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let only = keys_of(w.uid)?.only();
        let code = normal(&code).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no-key"))?;
        let by = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let row: Option<(Option<i64>, Option<i64>)> = c
                .query_row("SELECT used_by, revoked FROM recode_keys WHERE code = ?1 AND (?2 IS NULL OR maker = ?2)", params![code, only], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
                .map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            let Some((used_by, revoked)) = row else { return Err(fail(StatusCode::NOT_FOUND, "no-key")) };
            if revoked.is_none() {
                c.execute("UPDATE recode_keys SET revoked = ?2 WHERE code = ?1", params![code, now()]).map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
                crate::people::audit(c, by, "recode.keys.revoke", &used_by.map_or(String::new(), |u| format!("user:{u}")), json!({ "key": code }));
                if let Some(u) = used_by {
                    marks(c, u);
                }
            }
            Ok(json!({ "code": code, "sub": used_by.map(|u| of(c, u).json(now())) }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Give {
    days: i64,
    #[serde(default)]
    note: String,
}

/// POST /api/admin/users/{login}/recode {days, note}: days given by hand (hers, and the role «Ключи»'s); the card's
/// Recode after.
async fn give_days(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>, Json(g): Json<Give>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let only = keys_of(w.uid)?.only();
        let note = clean_line(&g.note, 60);
        let by = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let uid = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            give(c, by, uid, g.days, &note).map_err(|e| match e {
                "term" => said(StatusCode::BAD_REQUEST, "term", "No such term."),
                "missing" => fail(StatusCode::NOT_FOUND, "missing"),
                _ => fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
            })?;
            Ok(card(c, uid, only))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

/// DELETE /api/admin/users/{login}/recode: what was given by hand taken back (hers all of it, the role «Ключи»'s what
/// they gave); the card's Recode after.
async fn take_days(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let only = keys_of(w.uid)?.only();
        let by = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let uid = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let (n, _) = take(c, by, uid, only).map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            let mut v = card(c, uid, only);
            v["taken"] = json!(n);
            Ok(v)
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

async fn take_all_days(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let by = w.uid;
        if !crate::people::can(by, crate::people::RECODE_REVOKE) {
            return Err(fail(StatusCode::FORBIDDEN, "staff"));
        }
        let v = db(&app, move |c| -> Result<Value, Response> {
            let uid = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let (n, sub) = take_all(c, by, uid).map_err(|e| fail(if e == "owner-protected" || e == "forbidden" { StatusCode::FORBIDDEN } else { StatusCode::SERVICE_UNAVAILABLE }, e))?;
            let mut v = sub.json(now());
            v["taken"] = json!(n);
            Ok(v)
        }).await?;
        Ok(Json(v).into_response())
    }).await
}

// ------------------------------------------------------------------------------------------- the loader's

/// How long a loader's code waits to be allowed on the site.
const LINK_SECONDS: i64 = 600;
/// How often the game asks whether it may go on (her word, 15.10: every 30 minutes).
pub const CHECK_EVERY: i64 = 1800;

/// A loader's code: 8 of the keys' letters.
fn new_link_code() -> String {
    let mut b = [0u8; 8];
    OsRng.fill_bytes(&mut b);
    b.iter().map(|x| LETTERS[(*x & 31) as usize] as char).collect()
}

/// The code as typed or as in the page's address: K7Q2-M9PX, k7q2m9px.
fn link_code(s: &str) -> Option<String> {
    let c: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    (c.len() == 8 && c.bytes().all(|b| LETTERS.contains(&b))).then_some(c)
}

fn shown(code: &str) -> String {
    format!("{}-{}", &code[..4], &code[4..])
}

#[derive(Deserialize)]
struct LinkAsk {
    device: Parts,
}

/// POST /api/loader/link {device}: the loader asks to be signed in through the site. A code it shows, the page it
/// opens in the browser, and a key of its own: only the loader that asked takes the answer, whoever saw the code.
async fn link_new(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    run(async move {
        no_origin(&h)?;
        let (sealed, mark, asked, a) = crate::seal::words::<LinkAsk>("POST", "/api/loader/link", &body)?;
        if !a.device.valid() {
            return Err(fail(StatusCode::BAD_REQUEST, "device"));
        }
        let key = random_hex(32);
        let (k, device) = (digest(&key), a.device.text());
        let code = db(&app, move |c| -> Result<String, Response> {
            let at = now();
            let _ = c.execute("DELETE FROM loader_links WHERE made < ?1", params![at - LINK_SECONDS]);
            for _ in 0..8 {
                let code = new_link_code();
                if c.execute("INSERT OR IGNORE INTO loader_links (code, key, device, product, made) VALUES (?1, ?2, ?3, 'recode', ?4)", params![code, k, device, at]).unwrap_or(0) == 1 {
                    return Ok(code);
                }
            }
            Err(fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))
        })
        .await?;
        Ok(Json(crate::seal::said(sealed, &mark, "POST", "/api/loader/link", &asked, json!({ "code": shown(&code), "key": key, "url": format!("{}/#/link/{code}", app.origin), "expires_in": LINK_SECONDS }))?).into_response())
    })
    .await
}

/// GET /api/loader/link/{code} (the site, signed in): what the loader asks, for the page that allows it.
async fn link_seen(State(app): State<Shared>, h: HeaderMap, Key(code): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let code = link_code(&code).ok_or_else(|| fail(StatusCode::NOT_FOUND, "link-gone"))?;
        let uid = w.uid;
        let login = w.login.clone();
        let v = db(&app, move |c| -> Result<Value, Response> {
            let row: Option<(i64, Option<i64>, Option<i64>)> = c
                .query_row("SELECT made, allowed, denied FROM loader_links WHERE code = ?1", params![code], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()
                .map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            let Some((made, allowed, denied)) = row.filter(|r| r.0 >= now() - LINK_SECONDS) else { return Err(fail(StatusCode::NOT_FOUND, "link-gone")) };
            let state = if allowed.is_some() { "allowed" } else if denied.is_some() { "denied" } else { "waiting" };
            Ok(json!({ "code": shown(&code), "state": state, "made": made, "expires": made + LINK_SECONDS, "login": login, "recode": of(c, uid).json(now()) }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

/// POST /api/loader/link/{code}/allow | /deny (the site, signed in): the loader signs in as this account, or not.
async fn link_answer(State(app): State<Shared>, h: HeaderMap, Key((code, answer)): Key<(String, String)>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let code = link_code(&code).ok_or_else(|| fail(StatusCode::NOT_FOUND, "link-gone"))?;
        let allow = match answer.as_str() {
            "allow" => true,
            "deny" => false,
            _ => return Err(fail(StatusCode::NOT_FOUND, "no-such")),
        };
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let at = now();
            let sql = if allow {
                "UPDATE loader_links SET uid = ?2, allowed = ?3 WHERE code = ?1 AND allowed IS NULL AND denied IS NULL AND made >= ?4"
            } else {
                "UPDATE loader_links SET denied = ?3 WHERE code = ?1 AND allowed IS NULL AND denied IS NULL AND made >= ?4 AND (?2 = ?2)"
            };
            let n = c.execute(sql, params![code, uid, at, at - LINK_SECONDS]).map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            if n != 1 {
                return Err(fail(StatusCode::NOT_FOUND, "link-gone"));
            }
            if allow {
                crate::people::audit(c, uid, "loader.link", &format!("user:{uid}"), json!({ "product": "recode" }));
            }
            Ok(())
        })
        .await?;
        Ok(Json(json!({ "ok": true })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct LinkPoll {
    code: String,
    key: String,
}

/// POST /api/loader/link/poll {code, key} (the loader, every couple of seconds): not yet (202), or the account's
/// loader token once it was allowed (the code is spent then), or no.
async fn link_poll(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    run(async move {
        no_origin(&h)?;
        let (sealed, mark, asked, p) = crate::seal::words::<LinkPoll>("POST", "/api/loader/link/poll", &body)?;
        let code = link_code(&p.code).ok_or_else(|| fail(StatusCode::GONE, "link-gone"))?;
        let k = digest(&p.key);
        let v = db(&app, move |c| -> Result<Option<Value>, Response> {
            let row: Option<(Vec<u8>, i64, Option<i64>, Option<i64>, Option<i64>)> = c
                .query_row("SELECT key, made, uid, allowed, denied FROM loader_links WHERE code = ?1", params![code], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
                .optional()
                .map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            let Some((key, made, uid, allowed, denied)) = row.filter(|r| r.0 == k && r.1 >= now() - LINK_SECONDS) else {
                return Err(fail(StatusCode::GONE, "link-gone"));
            };
            if denied.is_some() {
                let _ = c.execute("DELETE FROM loader_links WHERE code = ?1", params![code]);
                return Err(fail(StatusCode::FORBIDDEN, "link-denied"));
            }
            let (Some(uid), Some(_)) = (uid, allowed) else { return Ok(None) };
            let _ = (key, made);
            if let Some(b) = ban_of(c, uid) {
                let _ = c.execute("DELETE FROM loader_links WHERE code = ?1", params![code]);
                return Err(Refusal::Banned(b).response());
            }
            let token = new_session(c, uid, "loader").map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            let _ = c.execute("DELETE FROM loader_links WHERE code = ?1", params![code]);
            let login: String = c.query_row("SELECT login FROM users WHERE uid = ?1", params![uid], |r| r.get(0)).unwrap_or_default();
            Ok(Some(json!({ "uid": uid, "login": login, "token": token })))
        })
        .await?;
        match v {
            Some(v) => Ok(Json(crate::seal::said(sealed, &mark, "POST", "/api/loader/link/poll", &asked, v)?).into_response()),
            None => Ok((StatusCode::ACCEPTED, Json(crate::seal::said(sealed, &mark, "POST", "/api/loader/link/poll", &asked, json!({ "state": "waiting" }))?)).into_response()),
        }
    })
    .await
}

#[derive(Deserialize)]
struct StartAsk {
    device: Parts,
}

/// The account signed in by the loader's (or the game's) token, or why not.
fn signed(c: &Connection, h: &HeaderMap) -> Result<(i64, String), Response> {
    let (token, _) = presented(h).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
    let w = who(c, &token).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
    Ok((w.uid, w.login))
}

/// A refusal with whose it is (the loader shows the account even when it may not play).
fn refused(r: Refusal, uid: i64, login: &str) -> Response {
    let mut v = r.json();
    v["uid"] = json!(uid);
    v["login"] = json!(login);
    (StatusCode::FORBIDDEN, Json(v)).into_response()
}

/// POST /api/loader/recode/start {device} (the loader's token): may this account play Recode on this computer (it is
/// bound to it if it had none); the channel's current build, signed; leave to download for a few hours; a ticket for
/// one start of the game.
async fn start(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    run(async move {
        no_origin(&h)?;
        let (sealed, mark, asked, s) = crate::seal::words::<StartAsk>("POST", "/api/loader/recode/start", &body)?;
        if !s.device.valid() {
            return Err(fail(StatusCode::BAD_REQUEST, "device"));
        }
        // whether a master pair is here to cut this start's own library out of
        let has_master = crate::protect::there(&app.dist.root);
        let root = app.dist.root.clone();
        let v = db(&app, move |c| -> Result<Value, Response> {
            let (uid, login) = signed(c, &h)?;
            admit_recode(c, uid, &s.device, true).map_err(|r| refused(r, uid, &login))?;
            let Some((build, manifest, sig)) = crate::dist::current(c, "recode") else { return Err(fail(StatusCode::SERVICE_UNAVAILABLE, "no-release")) };
            // a master up here, and a build that is under a seal: only then is there a stub to cut
            let cut_here = has_master && crate::protect::sealed(&manifest);
            let at = now();
            crate::dist::grant(c, uid, "recode", at + ACCESS_SECONDS).map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            c.execute("DELETE FROM tickets WHERE made < ?1", params![at - TICKET_SECONDS]).map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            let start_ticket = random_hex(32);
            c.execute("INSERT INTO tickets (hash, uid, build, made, channel) VALUES (?1, ?2, ?3, ?4, 'recode')", params![digest(&start_ticket), uid, build, at])
                .map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            Ok(json!({
                "uid": uid, "uid_text": crate::people::card_of(c, uid)["uid_text"].clone(), "login": login, "recode": of(c, uid).json(at),
                "build": build, "manifest": manifest, "sig": sig,
                // one start's own words, spent by the game when it says it is starting (license.rs)
                "start_ticket": start_ticket, "ticket_seconds": TICKET_SECONDS,
                // what their copy is keyed by: theirs and the window's, the same while the window stands. The
                // loader needs it every start whether or not it fetches anything — it is what opens the copy
                // already on disk, and it is never written there with it.
                "ticket": crate::protect::pair_ticket(&root, uid, at),
                // which window that is: the loader writes it beside the pair it keeps, so a start within the
                // same window of the same build takes what is already there instead of asking again
                "pair_window": crate::protect::window_of(at),
                "pair_seconds": crate::protect::pair_seconds(at),
                "reset_from": reset_from(c, uid),
                // a master pair is here: the loader comes back for a pair of its own under /stub
                "stub": cut_here,
            }))
        })
        .await?;
        Ok(Json(crate::seal::said(sealed, &mark, "POST", "/api/loader/recode/start", &asked, v)?).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct StubAsk {
    ticket: String,
}

/// POST /api/loader/recode/stub {ticket} (the loader's token): this player's own copy of the pair, cut out of
/// the master — their own halves, their own sealed game, their own last day — under the pair ticket the start
/// named. The master and its card stay here; what goes out is one pair, and one crack of it opens that copy and
/// nothing else. Both files of one cut go out under one frame (protect::framed): two answers could straddle the
/// window turning and hand over a stub that opens nothing beside it.
async fn stub(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    run(async move {
        no_origin(&h)?;
        // what goes asking is sealed like any other words (the ticket is the one thing in it worth reading); what
        // comes back is the pair itself, already good for that ticket alone
        let (_sealed, _mark, _asked, s) = crate::seal::words::<StubAsk>("POST", "/api/loader/recode/stub", &body)?;
        if s.ticket.len() != 64 {
            return Err(fail(StatusCode::BAD_REQUEST, "ticket"));
        }
        let ticket = s.ticket.clone();
        let root = app.dist.root.clone();
        let asked_for = ticket.clone();
        let uid = db(&app, move |c| -> Result<i64, Response> {
            let (uid, login) = signed(c, &h)?;
            if let Some(b) = ban_of(c, uid) {
                return Err(refused(Refusal::Banned(b), uid, &login));
            }
            // the pair ticket is this player's and this window's (or the one that has just turned): not a
            // string anyone can name for themselves, and not the one before's or another account's
            if !crate::protect::pair_ticket_is(&root, uid, &asked_for, now()) {
                return Err(fail(StatusCode::UNAUTHORIZED, "ticket"));
            }
            Ok(uid)
        })
        .await?;
        let master = crate::protect::load(&app.dist.root).map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "protect"))?;
        // its own last day lands on the window's end (protect::pair_keep_days) — under the key, so moving it
        // is another key and no game. So does the account: it goes into the stub and the mapped game is
        // marked with it at load, so a dump of a running game says whose copy it was.
        let mine = master
            .cut(&app.dist.root, &ticket, crate::protect::pair_keep_days(now()), uid)
            .map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "protect"))?;
        Ok(crate::protect::framed(&mine).into_response())
    })
    .await
}

/// POST /api/loader/recode/device/reset {device} (the loader's token): "free my computer", once a day; this one is
/// bound in the same breath.
async fn device_reset(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    run(async move {
        no_origin(&h)?;
        let (sealed, mark, asked, s) = crate::seal::words::<StartAsk>("POST", "/api/loader/recode/device/reset", &body)?;
        if !s.device.valid() {
            return Err(fail(StatusCode::BAD_REQUEST, "device"));
        }
        let v = db(&app, move |c| -> Result<Value, Response> {
            let (uid, login) = signed(c, &h)?;
            if let Some(b) = ban_of(c, uid) {
                return Err(refused(Refusal::Banned(b), uid, &login));
            }
            free_device(c, uid, true).map_err(|from| (StatusCode::TOO_MANY_REQUESTS, Json(json!({ "error": "wait", "reset_from": from }))).into_response())?;
            // freed often: worth her look
            let at = now();
            let _ = c.execute("INSERT INTO recode_resets (uid, at) VALUES (?1, ?2)", params![uid, at]);
            let week: i64 = c.query_row("SELECT COUNT(*) FROM recode_resets WHERE uid = ?1 AND at > ?2", params![uid, at - 7 * 86_400], |r| r.get(0)).unwrap_or(0);
            if week >= RESETS_FLAG {
                flag(c, uid, "resets", json!({ "week": week }));
            }
            admit_recode(c, uid, &s.device, true).map_err(|r| refused(r, uid, &login))?;
            Ok(json!({ "reset_from": now() + RESET_EVERY }))
        })
        .await?;
        Ok(Json(crate::seal::said(sealed, &mark, "POST", "/api/loader/recode/device/reset", &asked, v)?).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Verify {
    ticket: String,
    device: Parts,
    build: i64,
    #[serde(default)]
    nonce: String,
}

/// POST /api/client/recode/verify {ticket, device, build, nonce}: the game at its start, with the loader's ticket (for
/// this one start, of Recode's channel). The same checks, only the current build; the game's own session, how often it
/// asks again, the answer signed with its nonce.
async fn client_verify(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    run(async move {
        no_origin(&h)?;
        let (sealed, mark, asked, v) = crate::seal::words::<Verify>("POST", "/api/client/recode/verify", &body)?;
        if !v.device.valid() || v.ticket.len() != 64 {
            return Err(fail(StatusCode::BAD_REQUEST, "ticket"));
        }
        let (ticket, nonce, build) = (v.ticket.clone(), v.nonce.clone(), v.build);
        let mut answer = db(&app, move |c| -> Result<Value, Response> {
            let hash = digest(&v.ticket);
            let found: Option<(i64, i64, String)> = c
                .query_row("SELECT uid, made, channel FROM tickets WHERE hash = ?1", params![hash], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()
                .ok()
                .flatten();
            let _ = c.execute("DELETE FROM tickets WHERE hash = ?1", params![hash]);
            let Some((uid, _, _)) = found.filter(|t| now() - t.1 <= TICKET_SECONDS && t.2 == "recode") else { return Err(fail(StatusCode::UNAUTHORIZED, "ticket")) };
            let login: String = c.query_row("SELECT login FROM users WHERE uid = ?1", params![uid], |r| r.get(0)).map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            let current = crate::dist::current(c, "recode").map(|(b, _, _)| b).unwrap_or(0);
            if v.build != current {
                return Err(refused(Refusal::Outdated { build: current }, uid, &login));
            }
            admit_recode(c, uid, &v.device, false).map_err(|r| refused(r, uid, &login))?;
            // one game at a time: the first keeps it (her word); a second from another computer is told, and she sees
            // it. From the same computer the new start takes over: the one before fell (a crash, its goodbye unsaid),
            // or it is a second window, told to go at its next word.
            if !crate::is_owner(uid) {
                if let Some(device) = live_device(c, uid) {
                    if !Parts::read(&device).is_some_and(|p| p.same(&v.device)) {
                        flag(c, uid, "second-game", json!({}));
                        return Err(refused(Refusal::InGame, uid, &login));
                    }
                }
            }
            let token = new_session(c, uid, "client").map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            let at = now();
            c.execute("INSERT OR REPLACE INTO recode_live (uid, session, device, since, seen) VALUES (?1, ?2, ?3, ?4, ?4)", params![uid, digest(&token), v.device.text(), at])
                .map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            Ok(json!({
                "uid": uid, "uid_text": crate::people::card_of(c, uid)["uid_text"].clone(), "login": login, "token": token,
                "recode": of(c, uid).json(at), "check_every": CHECK_EVERY, "alive_every": ALIVE_EVERY,
            }))
        })
        .await?;
        if nonce.len() == 64 && nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
            let uid = answer["uid"].as_i64().unwrap_or(0);
            answer["sig"] = json!(app.dist.sign_start(&nonce, &ticket, uid, build));
        }
        Ok(Json(crate::seal::said(sealed, &mark, "POST", "/api/client/recode/verify", &asked, answer)?).into_response())
    })
    .await
}

// ----------------------------------------------------------------------------- one game at a time, and her watch

/// A game alive says so this often (s); one silent this long no longer holds the account (it fell, or lost its net).
pub const ALIVE_EVERY: i64 = 60;
const ALIVE_GONE: i64 = 150;
/// Frees of the computer in a week that make the account worth her look.
const RESETS_FLAG: i64 = 3;

/// What the account's computer was bound to before (its parts), for the TPM's change.
fn bound_parts(c: &Connection, uid: i64) -> Option<Parts> {
    c.query_row("SELECT parts FROM devices WHERE uid = ?1", params![uid], |r| r.get::<_, String>(0)).optional().ok().flatten().as_deref().and_then(Parts::read)
}

/// Something about an account worth her look (her list), or a ban made by the rules (kept in it too). The same kind
/// still open is not repeated: it is counted.
pub fn flag(c: &Connection, uid: i64, kind: &str, detail: Value) {
    let open: Option<(i64, String)> = c
        .query_row("SELECT id, detail FROM recode_flags WHERE uid = ?1 AND kind = ?2 AND verdict IS NULL", params![uid, kind], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .ok()
        .flatten();
    if let Some((id, was)) = open {
        let mut d: Value = serde_json::from_str(&was).unwrap_or_else(|_| json!({}));
        let times = d["times"].as_i64().unwrap_or(1) + 1;
        // The newest report is what was found last, and that is the one she needs to read. Whatever
        // this report does not name stays as it was, so nothing already on the line is lost; `times`
        // is ours and is kept counting over both.
        if let (Some(base), Some(fresh)) = (d.as_object_mut(), detail.as_object()) {
            for (k, v) in fresh {
                base.insert(k.clone(), v.clone());
            }
        }
        d["times"] = json!(times);
        let _ = c.execute("UPDATE recode_flags SET detail = ?2, at = ?3 WHERE id = ?1", params![id, d.to_string(), now()]);
        return;
    }
    let login: String = c.query_row("SELECT login FROM users WHERE uid = ?1", params![uid], |r| r.get(0)).unwrap_or_default();
    let mut d = detail;
    d["times"] = json!(1);
    let verdict: Option<&str> = if kind == "autoban" { Some("ban") } else { None };
    let _ = c.execute(
        "INSERT INTO recode_flags (uid, login, kind, detail, at, verdict, done) VALUES (?1, ?2, ?3, ?4, ?5, ?6, CASE WHEN ?6 IS NULL THEN NULL ELSE ?5 END)",
        params![uid, login, kind, d.to_string(), now(), verdict],
    );
}

/// Recode's door: the beta's checks for Recode (admit_for), and her rules over them (15.10). An account coming from a
/// banned account's computer is banned too («обход бана»: such a thing does not happen honestly); a TPM changed while
/// the rest of the computer is the same goes to her list (a TPM reset in the BIOS changes it honestly), and so does one
/// gone while the rest is the same (20.10: Recode plays without a TPM now, so switching it off is the way round it —
/// the computer is still known by its other parts, a ban on it stands; a BIOS reset switches it off honestly too).
fn admit_recode(c: &mut Connection, uid: i64, parts: &Parts, bind: bool) -> Result<(), Refusal> {
    let before = bound_parts(c, uid);
    match admit_for(c, uid, parts, bind, Product::Recode) {
        Err(Refusal::Banned(b)) if b.device => {
            let reason = format!("Обход бана: компьютер заблокированного аккаунта ({})", b.reason);
            match crate::beta::ban(c, uid, &reason, b.until, "auto") {
                Ok(id) => {
                    flag(c, uid, "autoban", json!({ "ban": id, "of": b.id, "reason": reason }));
                    Err(Refusal::Banned(Ban { id, reason, until: b.until, device: false }))
                }
                Err(_) => Err(Refusal::Banned(b)),
            }
        }
        Ok(()) => {
            match (before.as_ref().and_then(|p| p.0.get("tpm")), parts.0.get("tpm")) {
                (Some(old), Some(new)) if !old.eq_ignore_ascii_case(new) => flag(c, uid, "tpm-changed", json!({})),
                (Some(_), None) => flag(c, uid, "tpm-off", json!({})),
                _ => {}
            }
            Ok(())
        }
        Err(r) => Err(r),
    }
}

/// The game of this account alive now (its session's hash, when it last said so), if one is.
fn live_game(c: &Connection, uid: i64) -> Option<(Vec<u8>, i64)> {
    c.query_row("SELECT session, seen FROM recode_live WHERE uid = ?1", params![uid], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .ok()
        .flatten()
        .filter(|l: &(Vec<u8>, i64)| l.1 > now() - ALIVE_GONE)
}

/// The computer the account's live game plays on (its parts), if one plays.
fn live_device(c: &Connection, uid: i64) -> Option<String> {
    c.query_row("SELECT device FROM recode_live WHERE uid = ?1 AND seen > ?2", params![uid, now() - ALIVE_GONE], |r| r.get(0)).optional().ok().flatten()
}

/// POST /api/client/recode/alive {device} (the game's session, every minute; /check is the same): it holds the account
/// (one game at a time: another that took it tells this one to go), and may it go on — its Recode, its ban, its
/// computer. When not, why; for the end of its Recode the game counts down its last 30 minutes.
async fn client_alive(State(app): State<Shared>, h: HeaderMap, uri: axum::http::Uri, body: Bytes) -> Response {
    run(async move {
        no_origin(&h)?;
        // two doors on this one: the seal is tied to the door it came through, so it is that one's name it opens under
        let path = uri.path().to_string();
        let (sealed, mark, asked, s) = crate::seal::words::<StartAsk>("POST", &path, &body)?;
        if !s.device.valid() {
            return Err(fail(StatusCode::BAD_REQUEST, "device"));
        }
        let v = db(&app, move |c| -> Result<Value, Response> {
            let (token, _) = presented(&h).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
            let hash = digest(&token);
            let Some(w) = who(c, &token) else {
                // its session ended (a ban ends the game's): why, when it was a game of Recode
                let uid: Option<i64> = c.query_row("SELECT uid FROM recode_live WHERE session = ?1", params![hash], |r| r.get(0)).optional().ok().flatten();
                if let Some(uid) = uid {
                    let _ = c.execute("DELETE FROM recode_live WHERE session = ?1", params![hash]);
                    if let Some(b) = ban_of(c, uid) {
                        let login: String = c.query_row("SELECT login FROM users WHERE uid = ?1", params![uid], |r| r.get(0)).unwrap_or_default();
                        return Err(refused(Refusal::Banned(b), uid, &login));
                    }
                }
                return Err(fail(StatusCode::UNAUTHORIZED, "signed-out"));
            };
            let (uid, login) = (w.uid, w.login);
            if !crate::is_owner(uid) {
                if let Some((session, _)) = live_game(c, uid) {
                    if session != hash {
                        return Err(refused(Refusal::InGame, uid, &login));
                    }
                }
            }
            admit_recode(c, uid, &s.device, false).map_err(|r| refused(r, uid, &login))?;
            let at = now();
            // its minutes in Recode: the time since this same game last said it was alive (a minute; never more than
            // the while after which a quiet game counts as gone)
            let minutes = live_game(c, uid).filter(|(session, _)| *session == hash).map_or(0, |(_, seen)| ((at - seen + 30) / 60).clamp(0, ALIVE_GONE / 60 + 1));
            c.execute(
                "INSERT INTO recode_live (uid, session, device, since, seen) VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(uid) DO UPDATE SET since = CASE WHEN session = excluded.session THEN since ELSE excluded.since END, session = excluded.session, device = excluded.device, seen = excluded.seen",
                params![uid, hash, s.device.text(), at],
            )
            .map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            if minutes > 0 {
                crate::people::played_recode(c, uid, minutes, at);
            }
            Ok(json!({ "ok": true, "recode": of(c, uid).json(at), "alive_every": ALIVE_EVERY, "check_every": CHECK_EVERY, "now": at }))
        })
        .await?;
        Ok(Json(crate::seal::said(sealed, &mark, "POST", &path, &asked, v)?).into_response())
    })
    .await
}

/// POST /api/client/recode/bye (the game's session, as it closes): the account is free for another start at once.
async fn client_bye(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    run(async move {
        no_origin(&h)?;
        // nothing in the words but their seal: what is said is that this one is going. A bye that comes with no
        // body at all is the same one — though in a world that keeps the seal even that is a letter, and an empty
        // body is not one, so it is turned away like any other words that were not sealed.
        let body = if body.is_empty() { Bytes::from_static(b"{}") } else { body };
        let (sealed, mark, asked, _) = crate::seal::words::<Value>("POST", "/api/client/recode/bye", &body)?;
        let (token, _) = presented(&h).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
        let hash = digest(&token);
        db(&app, move |c| c.execute("DELETE FROM recode_live WHERE session = ?1", params![hash])).await.map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
        Ok(Json(crate::seal::said(sealed, &mark, "POST", "/api/client/recode/bye", &asked, json!({ "ok": true }))?).into_response())
    })
    .await
}

// -------------------------------------------------------------------------- a look at the room (report only)

/// The words a look may carry: what kind of sign it is. The list is closed so her stays readable — a
/// name invented on either side is turned away rather than written down as something new. A hand on
/// the game's own memory is not a look: that is `touch_report` in beta.rs, and it bans.
const SIGHTS: &[&str] = &[
    "script-ours",  // a script or module named after our own files
    "module-ours",  // our names sitting in a foreign process
    "file-near",    // something of ours' shape beside the install — "dump" and the like
    "date-cluster", // timestamps gathering around ours
    "tool",         // a tool a copy is taken apart with
    "debugger",     // a debugger
    "dumper",       // a dumper
    "sandbox",      // sandbox or hypervisor furniture (a look only; the game itself is turned away by vm::here)
    "parent",       // an unusual parent process
];

/// One sign of the room around the game, and no more than a name beside it.
#[derive(Deserialize)]
struct Seen {
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    note: String,
}

#[derive(Deserialize)]
struct Sight {
    #[serde(default)]
    build: Option<i64>,
    #[serde(default)]
    score: Option<i64>,
    #[serde(default)]
    sights: Vec<Seen>,
}

/// What a look at the room leaves on her list (report only): one line per account, counted when it
/// comes again, holding the newest words. Never a ban, never a hold, never a word to the player —
/// whatever stands beside the game is not the player's doing, and the player keeps playing. The
/// guard's findings are the ones her rules act on (beta.rs, `guard_report`), and so is a hand on the
/// game's own memory (beta.rs, `touch_report`); this is the same list with nothing but a look on it.
fn sight_keep(c: &Connection, uid: i64, detail: Value) {
    let open: Option<(i64, String)> = c
        .query_row("SELECT id, detail FROM recode_flags WHERE uid = ?1 AND kind = 'sight' AND verdict IS NULL", params![uid], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .ok()
        .flatten();
    if let Some((id, was)) = open {
        let times = serde_json::from_str::<Value>(&was).ok().and_then(|d| d["times"].as_i64()).unwrap_or(1) + 1;
        let mut d = detail;
        d["times"] = json!(times);
        let _ = c.execute("UPDATE recode_flags SET detail = ?2, at = ?3 WHERE id = ?1", params![id, d.to_string(), now()]);
        return;
    }
    let login: String = c.query_row("SELECT login FROM users WHERE uid = ?1", params![uid], |r| r.get(0)).unwrap_or_default();
    let mut d = detail;
    d["times"] = json!(1);
    let _ = c.execute("INSERT INTO recode_flags (uid, login, kind, detail, at, verdict, done) VALUES (?1, ?2, 'sight', ?3, ?4, NULL, NULL)", params![uid, login, d.to_string(), now()]);
}

/// POST /api/client/recode/sight {build, score, sights} (the game's session): what stands around the
/// game — scripts carrying our own names, tools a copy is taken apart with, dates that gather around
/// ours. Kept for her list and nothing more: this door never bans, never holds the account, never
/// turns the player away. A look, not a hand. The body is a handful of signs and a score between them
/// (0-100, how sure the game is), so that her list is a line and not a flood.
async fn client_sight(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    run(async move {
        no_origin(&h)?;
        let (sealed, mark, asked, s) = crate::seal::words::<Sight>("POST", "/api/client/recode/sight", &body)?;
        // what was seen, as short words only: a kind from the list above, and a name beside it
        let mut signs = Vec::new();
        for seen in s.sights.iter().take(16) {
            let kind = clean_line(&seen.kind, 24);
            if !SIGHTS.contains(&kind.as_str()) {
                return Err(fail(StatusCode::BAD_REQUEST, "kind"));
            }
            signs.push(json!({ "kind": kind, "name": clean_line(&seen.name, 80), "note": clean_line(&seen.note, 80) }));
        }
        let score = s.score.unwrap_or(0).clamp(0, 100);
        let detail = json!({ "build": s.build, "score": score, "sights": signs });
        let (token, _) = presented(&h).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
        db(&app, move |c| -> Result<(), Response> {
            let w = who(c, &token).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
            sight_keep(c, w.uid, detail);
            Ok(())
        })
        .await?;
        Ok(Json(crate::seal::said(sealed, &mark, "POST", "/api/client/recode/sight", &asked, json!({ "ok": true }))?).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct FlagsQ {
    #[serde(default)]
    all: Option<u8>,
}

/// GET /api/admin/recode/flags[?all=1] (hers alone): what her rules saw — the open ones first, or every one.
async fn flags(State(app): State<Shared>, h: HeaderMap, Query(q): Query<FlagsQ>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        hers(w.uid)?;
        let all = q.all == Some(1);
        // Where the notes themselves live. A row written before the detail carried what the note
        // said is filled from that note on the way out — see `fill_from_note`.
        let dir = crate::beta::crashes_dir(&app.db_path);
        let v = db(&app, move |c| {
            let sql = format!(
                "SELECT f.id, f.uid, f.login, f.kind, f.detail, f.at, f.verdict, f.done, (SELECT login FROM users WHERE uid = f.by) FROM recode_flags f {} ORDER BY f.verdict IS NOT NULL, f.at DESC LIMIT 300",
                if all { "" } else { "WHERE f.verdict IS NULL OR f.kind = 'autoban' AND f.done > ?1" }
            );
            let week = now() - 7 * 86_400;
            let items: Vec<Value> = c
                .prepare(&sql)
                .and_then(|mut s| {
                    let map = |r: &rusqlite::Row| -> rusqlite::Result<Value> {
                        let detail: String = r.get(4)?;
                        let uid: i64 = r.get(1)?;
                        Ok(json!({
                            "id": r.get::<_, i64>(0)?, "uid": uid, "login": r.get::<_, String>(2)?, "kind": r.get::<_, String>(3)?,
                            "detail": serde_json::from_str::<Value>(&detail).unwrap_or(json!({})), "at": r.get::<_, i64>(5)?,
                            "verdict": r.get::<_, Option<String>>(6)?, "done": r.get::<_, Option<i64>>(7)?, "by": r.get::<_, Option<String>>(8)?,
                        }))
                    };
                    let rows: Vec<Value> = if all { s.query_map([], map)?.collect::<rusqlite::Result<_>>()? } else { s.query_map(params![week], map)?.collect::<rusqlite::Result<_>>()? };
                    Ok(rows)
                })
                .unwrap_or_default();
            let mut items = items;
            for it in items.iter_mut() {
                let uid = it["uid"].as_i64().unwrap_or(0);
                it["banned"] = json!(ban_of(c, uid).map(|b| b.json()));
                it["recode"] = of(c, uid).json(now());
                // A detail that predates the note's own words on the line is filled from the note
                // it came from — see `fill_row`. A ban of an older build named no log at all; its
                // report of the same second did.
                let mut d = it["detail"].take();
                crate::beta::fill_row(c, &dir, uid, it["at"].as_i64().unwrap_or(0), &mut d);
                it["detail"] = d;
            }
            let open: i64 = c.query_row("SELECT COUNT(*) FROM recode_flags WHERE verdict IS NULL", [], |r| r.get(0)).unwrap_or(0);
            json!({ "items": items, "open": open })
        })
        .await;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Verdict {
    verdict: String, // ok | ban
    #[serde(default)]
    reason: String,
    #[serde(default)]
    length: String, // as she types a ban's length: 7d, 12h; empty: for good
}

/// POST /api/admin/recode/flags/{id} {verdict, reason, length} (hers alone): all right, or banned.
async fn flag_verdict(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(v): Json<Verdict>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        hers(w.uid)?;
        let (by, by_login) = (w.uid, w.login.clone());
        let r = db(&app, move |c| -> Result<Value, Response> {
            let uid: i64 = c
                .query_row("SELECT uid FROM recode_flags WHERE id = ?1", params![id], |r| r.get(0))
                .optional()
                .map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?
                .ok_or_else(|| fail(StatusCode::NOT_FOUND, "no-flag"))?;
            match v.verdict.as_str() {
                "ok" => {}
                "ban" => {
                    let until = if v.length.trim().is_empty() {
                        None
                    } else {
                        Some(now() + crate::beta::parse_length(&v.length).ok_or_else(|| said(StatusCode::BAD_REQUEST, "length", "A length like 7d or 12h."))?)
                    };
                    let reason = clean_line(&v.reason, 200);
                    let reason = if reason.is_empty() { "Нарушение правил Recode".to_string() } else { reason };
                    crate::beta::ban(c, uid, &reason, until, &by_login).map_err(|e| said(StatusCode::CONFLICT, "ban", &e))?;
                }
                _ => return Err(fail(StatusCode::BAD_REQUEST, "verdict")),
            }
            c.execute("UPDATE recode_flags SET verdict = ?2, by = ?3, done = ?4 WHERE uid = ?5 AND verdict IS NULL", params![id, v.verdict, by, now(), uid])
                .map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            crate::people::audit(c, by, "recode.flag", &format!("user:{uid}"), json!({ "verdict": v.verdict }));
            Ok(json!({ "ok": true }))
        })
        .await?;
        Ok(Json(r).into_response())
    })
    .await
}

/// GET /api/dist/recode/loader (the site's button): the loader of Recode's current build, only to an account whose
/// Recode is on (her word: no one downloads it without it).
async fn loader_file(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let sha = db(&app, move |c| -> Result<String, Response> {
            if let Some(b) = ban_of(c, uid) {
                return Err((StatusCode::FORBIDDEN, Json(b.json())).into_response());
            }
            let sub = of(c, uid);
            if !sub.active(now()) {
                return Err(said(StatusCode::FORBIDDEN, "no-recode", "No Recode on this account."));
            }
            crate::dist::loader_sha(c, "recode").ok_or_else(|| fail(StatusCode::SERVICE_UNAVAILABLE, "no-release"))
        })
        .await?;
        crate::dist::send_file(&app.dist, &sha, "Cursed Recode.exe")
    })
    .await
}

pub fn routes() -> Router<Shared> {
    // what the loader and the game say comes sealed (seal.rs), which is to say twice as long as the words within
    let json = || DefaultBodyLimit::max(16 * 1024);
    Router::new()
        .route("/api/recode", get(mine))
        .route("/api/recode/activate", post(activate).layer(json()))
        .route("/api/admin/keys", get(list).post(make).layer(json()))
        .route("/api/admin/keys/batch/{name}", get(batch_text))
        .route("/api/admin/keys/{code}/revoke", post(revoke))
        .route("/api/admin/users/{login}/recode/all", axum::routing::delete(take_all_days))
        .route("/api/admin/users/{login}/recode", post(give_days).delete(take_days).layer(json()))
        .route("/api/loader/link", post(link_new).layer(json()))
        .route("/api/loader/link/poll", post(link_poll).layer(json()))
        .route("/api/loader/link/{code}", get(link_seen))
        .route("/api/loader/link/{code}/{answer}", post(link_answer))
        .route("/api/loader/recode/start", post(start).layer(json()))
        .route("/api/loader/recode/stub", post(stub).layer(json()))
        .route("/api/loader/recode/device/reset", post(device_reset).layer(json()))
        .route("/api/client/recode/verify", post(client_verify).layer(json()))
        .route("/api/client/recode/alive", post(client_alive).layer(json()))
        .route("/api/client/recode/check", post(client_alive).layer(json()))
        .route("/api/client/recode/bye", post(client_bye))
        .route("/api/client/recode/sight", post(client_sight).layer(json()))
        .route("/api/admin/recode/flags", get(flags))
        .route("/api/admin/recode/flags/{id}", post(flag_verdict).layer(json()))
        .route("/api/dist/recode/loader", get(loader_file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_look_right_and_read_back() {
        for _ in 0..1000 {
            let c = new_code();
            assert_eq!(c.len(), 21);
            assert!(c.starts_with("RCD-"));
            assert_eq!(normal(&c).as_deref(), Some(c.as_str()));
            // as anyone may type it: lower case, spaces, no dashes, no RCD
            let typed = c[4..].replace('-', " ").to_lowercase();
            assert_eq!(normal(&typed).as_deref(), Some(c.as_str()));
        }
        assert_eq!(normal("RCD-7KQ2M-X9PTA-4HW8C").as_deref(), Some("RCD-7KQ2M-X9PTA-4HW8C"));
        assert_eq!(normal("rcd7kq2mx9pta4hw8c").as_deref(), Some("RCD-7KQ2M-X9PTA-4HW8C"));
        assert_eq!(normal("RCD-7KQ2M-X9PTA-4HW80"), None); // 0 is not a key's letter
        assert_eq!(normal("RCD-7KQ2M-X9PTA"), None);
        assert_eq!(normal(""), None);
    }

    #[test]
    fn days_add_up_in_order() {
        let d = 86_400;
        // nothing
        assert_eq!(sum(&[]), Sub { forever: false, until: 0 });
        // 30 days from the activation
        assert_eq!(sum(&[(30, 1000)]).until, 1000 + 30 * d);
        // a second one while the first runs: after it
        assert_eq!(sum(&[(30, 1000), (7, 2000)]).until, 1000 + 37 * d);
        // a second one after the first ran out: from its own activation
        let late = 1000 + 40 * d;
        assert_eq!(sum(&[(30, 1000), (7, late)]).until, late + 7 * d);
        // forever over all
        let s = sum(&[(30, 1000), (0, 2000), (7, 3000)]);
        assert!(s.forever && s.active(i64::MAX));
        assert!(!sum(&[(1, 1000)]).active(1000 + d));
        assert!(sum(&[(1, 1000)]).active(1000 + d - 1));
    }

    /// A look at the room is kept and the player keeps playing: it is a line on her list and nothing
    /// else, whatever the look holds.
    #[test]
    fn a_look_is_kept_and_the_player_plays_on() {
        let (c, path) = fresh();
        let a = add(&c, "ivy");
        sight_keep(&c, a, json!({ "build": 6, "score": 40, "sights": [{ "kind": "tool", "name": "x64dbg" }] }));
        let (kind, detail): (String, String) = c
            .query_row("SELECT kind, detail FROM recode_flags WHERE uid = ?1", params![a], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("kept on her list");
        assert_eq!(kind, "sight");
        let d: Value = serde_json::from_str(&detail).unwrap();
        assert_eq!(d["score"], json!(40));
        assert_eq!(d["times"], json!(1));
        assert_eq!(d["sights"][0]["kind"], json!("tool"));
        // no ban, and the account is whole
        assert_eq!(crate::beta::ban_of(&c, a), None, "a look bans no one");
        assert_eq!(c.query_row("SELECT COUNT(*) FROM bans WHERE uid = ?1", params![a], |r| r.get::<_, i64>(0)).unwrap(), 0);
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    /// The same look coming again is not another line: it is counted, and what it holds now is what
    /// she reads.
    #[test]
    fn the_same_look_again_is_counted_not_repeated() {
        let (c, path) = fresh();
        let a = add(&c, "jack");
        sight_keep(&c, a, json!({ "score": 10, "sights": [{ "kind": "script-ours", "name": "recode_guard.py" }] }));
        sight_keep(&c, a, json!({ "score": 55, "sights": [{ "kind": "file-near", "name": "dump" }] }));
        sight_keep(&c, a, json!({ "score": 70, "sights": [{ "kind": "parent", "name": "python.exe" }] }));
        let rows: i64 = c.query_row("SELECT COUNT(*) FROM recode_flags WHERE uid = ?1", params![a], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1, "one line, however often it comes");
        let detail: String = c.query_row("SELECT detail FROM recode_flags WHERE uid = ?1", params![a], |r| r.get(0)).unwrap();
        let d: Value = serde_json::from_str(&detail).unwrap();
        assert_eq!(d["times"], json!(3));
        assert_eq!(d["score"], json!(70), "the newest words are the ones she reads");
        assert_eq!(d["sights"][0]["name"], json!("python.exe"));
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    /// A look of one account is its own; two players looking alike are two lines.
    #[test]
    fn each_account_keeps_its_own_look() {
        let (c, path) = fresh();
        let a = add(&c, "kate");
        let b = add(&c, "liam");
        sight_keep(&c, a, json!({ "score": 20, "sights": [{ "kind": "tool", "name": "ida" }] }));
        sight_keep(&c, b, json!({ "score": 20, "sights": [{ "kind": "tool", "name": "ida" }] }));
        let rows: i64 = c.query_row("SELECT COUNT(*) FROM recode_flags WHERE kind = 'sight'", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 2);
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    /// The names a look may carry are the ones the two sides agreed on; anything else is not written
    /// down as a sight at all.
    #[test]
    fn only_the_agreed_signs_are_a_sight() {
        for kind in ["script-ours", "module-ours", "file-near", "date-cluster", "tool", "debugger", "dumper", "sandbox", "parent"] {
            assert!(SIGHTS.contains(&kind), "{kind} is one of ours");
        }
        for kind in ["dump-name", "cheat", "autoban", "protect", "touch", "touch-unverified", ""] {
            assert!(!SIGHTS.contains(&kind), "{kind} is nobody's sight");
        }
    }

    /// Days given by hand add up like a key's, show on her card, and are all that «take» takes back: a key bought
    /// stays (her word, 19.10.2026).
    #[test]
    fn given_by_hand_and_taken_back() {
        let (mut c, path) = fresh();
        let a = add(&c, "ivy");
        assert!(!of(&c, a).active(now()));
        // a key bought and activated, then 30 days given from the console
        let (_, codes, _) = make_keys(&mut c, OWNER, 7, 1, "funpay").unwrap();
        c.execute("UPDATE recode_keys SET used_by = ?2, used_login = 'ivy', used_at = ?3 WHERE code = ?1", params![codes[0], a, now()]).unwrap();
        let s = give(&mut c, -1, a, 30, "").unwrap();
        assert!(s.active(now()) && !s.forever);
        assert!(s.until >= now() + 37 * 86_400 - 5, "after the key bought");
        let k = card(&c, a, None);
        assert_eq!(k["keys"], json!(2));
        assert_eq!(k["given"].as_array().unwrap().len(), 1);
        assert_eq!(k["given"][0]["days"], json!(30));
        assert_eq!(k["given"][0]["by"], Value::Null, "the console");
        // among her keys like any other: a batch of its own, activated by ivy
        let (note, login, batch): (String, String, i64) = c
            .query_row("SELECT note, used_login, batch FROM recode_keys WHERE given IS NOT NULL", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!((note.as_str(), login.as_str(), batch), ("выдано вручную", "ivy", 2));
        // taken back: the given days go, the key bought stays
        let (n, s) = take(&c, OWNER, a, None).unwrap();
        assert_eq!(n, 1);
        assert!(s.active(now()) && s.until <= now() + 7 * 86_400 + 5);
        assert!(card(&c, a, None)["given"].as_array().unwrap().is_empty());
        assert_eq!(take(&c, OWNER, a, None).unwrap().0, 0, "nothing more to take");
        // forever; what is not a term of the site's; nobody
        assert!(give(&mut c, OWNER, a, 0, "раздача").unwrap().forever);
        assert_eq!(give(&mut c, OWNER, a, 3, ""), Err("term"));
        assert_eq!(give(&mut c, OWNER, 99_999, 30, ""), Err("missing"));
        assert_eq!(make_keys(&mut c, OWNER, 5, 1, "").err(), Some("term"));
        assert_eq!(make_keys(&mut c, OWNER, 30, 0, "").err(), Some("count"));
        assert_eq!(make_keys(&mut c, OWNER, 30, 501, "").err(), Some("count"));
        assert_eq!((term_typed("forever"), term_typed("30d"), term_typed("180"), term_typed("3"), term_typed("x")), (Some(0), Some(30), Some(180), None, None));
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    fn has(c: &Connection, uid: i64, role: &str) -> bool {
        c.query_row("SELECT 1 FROM roles WHERE uid = ?1 AND role = ?2", params![uid, role], |_| Ok(())).optional().unwrap().is_some()
    }

    fn badge(c: &Connection, uid: i64, id: &str) -> bool {
        c.query_row("SELECT 1 FROM user_badges WHERE uid = ?1 AND badge = ?2", params![uid, id], |_| Ok(())).optional().unwrap().is_some()
    }

    /// Her word, 20.10: a beta key makes its activator a tester though the beta is closed and the places are full; a
    /// key of an ordinary batch made at the same time does not, and a beta key revoked before it was used gives nothing.
    #[test]
    fn a_beta_key_makes_a_tester_after_the_close() {
        let (mut c, path) = fresh();
        let (_, beta, _) = make_beta_keys(&mut c, OWNER, 0, 2, "бета").unwrap();
        let (_, plain, _) = make_keys(&mut c, OWNER, 0, 1, "funpay").unwrap();
        // the seventy places full
        for i in 0..BETA_PLACES {
            let u = add(&c, &format!("full{i}"));
            bought(&mut c, u, 30, now());
            crate::people::follow_role(&c, u, "rc_beta", true);
        }
        let use_key = |c: &Connection, code: &str, uid: i64| {
            c.execute("UPDATE recode_keys SET used_by = ?2, used_login = 'x', used_at = ?3 WHERE code = ?1", params![code, uid, now()]).unwrap();
            marks(c, uid);
        };
        let one = add(&c, "beta_one");
        use_key(&c, &beta[0], one);
        assert!(has(&c, one, "recode") && has(&c, one, "rc_beta") && badge(&c, one, "rc_beta"));
        let other = add(&c, "plain_one");
        use_key(&c, &plain[0], other);
        assert!(has(&c, other, "recode") && !has(&c, other, "rc_beta"));
        c.execute("UPDATE recode_keys SET revoked = ?2 WHERE code = ?1", params![beta[1], now()]).unwrap();
        let late = add(&c, "revoked_one");
        use_key(&c, &beta[1], late);
        assert!(!has(&c, late, "rc_beta"));
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    /// A key bought while the beta was on (made before it closed), activated at `at`.
    fn bought(c: &mut Connection, uid: i64, days: i64, at: i64) {
        let (_, codes, _) = make_keys(c, OWNER, days, 1, "funpay").unwrap();
        c.execute(
            "UPDATE recode_keys SET used_by = ?2, used_login = 'x', used_at = ?3, made = ?4 WHERE code = ?1",
            params![codes[0], uid, at, BETA_CLOSED - 3 * 86_400],
        )
        .unwrap();
    }

    /// Her word, 20.10: the beta closed with the prices cut — a key sold since is Recode alone, a beta key handed back
    /// to its buyer later is still the beta's, and the testers keep it.
    #[test]
    fn the_beta_is_closed() {
        let (mut c, path) = fresh();
        let new = add(&c, "after");
        let (_, codes, _) = make_keys(&mut c, OWNER, 30, 1, "funpay 299").unwrap();
        c.execute("UPDATE recode_keys SET used_by = ?2, used_login = 'after', used_at = ?3 WHERE code = ?1", params![codes[0], new, now()]).unwrap();
        marks(&c, new);
        assert!(has(&c, new, "recode") && !has(&c, new, "rc_beta") && !badge(&c, new, "rc_beta"));
        let back = add(&c, "handed-back");
        bought(&mut c, back, 0, now());
        marks(&c, back);
        assert!(has(&c, back, "rc_beta") && badge(&c, back, "rc_beta"));
        // a tester who buys again after it closed stays one
        let (_, more, _) = make_keys(&mut c, OWNER, 30, 1, "funpay 299").unwrap();
        c.execute("UPDATE recode_keys SET used_by = ?2, used_login = 'x', used_at = ?3 WHERE code = ?1", params![more[0], back, now()]).unwrap();
        sweep_marks(&c);
        assert!(has(&c, back, "rc_beta") && !has(&c, new, "rc_beta"));
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    /// Her word, 19.10: «Бета-тестер» to the first seventy who bought Recode (a gift is not a purchase, and hers is
    /// not one of them), «Recode» to whoever has it on, while it is; the badges with them.
    #[test]
    fn the_beta_is_seventy_who_bought_it() {
        let (mut c, path) = fresh();
        // Reserve the co-owner identity: beta buyers must be ordinary accounts.
        c.execute("INSERT INTO users(uid,login,login_key,email,email_key,pass,created) VALUES(19,'Andrey_AKM','andrey_akm','a@e.st','a@e.st','x',0)", []).unwrap();
        marks(&c, 19);
        assert!(of(&c, 19).forever && !has(&c, 19, "rc_beta"));
        let a = add(&c, "ivy");
        bought(&mut c, a, 0, now());
        marks(&c, a);
        assert!(has(&c, a, "rc_beta") && has(&c, a, "recode"));
        assert!(badge(&c, a, "rc_beta") && badge(&c, a, "recode") && badge(&c, a, "forever"));
        // given by hand: Recode, not the beta
        let g = add(&c, "gift");
        give(&mut c, OWNER, g, 30, "").unwrap();
        assert!(has(&c, g, "recode") && !has(&c, g, "rc_beta") && !badge(&c, g, "forever"));
        // taken back: the role goes, the badge stays
        take(&c, OWNER, g, None).unwrap();
        assert!(!has(&c, g, "recode") && badge(&c, g, "recode"));
        // hers: Recode forever, no place of the beta
        marks(&c, OWNER);
        assert!(!has(&c, OWNER, "rc_beta"));
        // a key that ran out: no role «Recode»; the beta for good
        let late = add(&c, "late");
        bought(&mut c, late, 1, now() - 2 * 86_400);
        marks(&c, late);
        assert!(has(&c, late, "rc_beta") && !has(&c, late, "recode"));
        // a tester she makes by hand (a gift, the role given) takes no place
        crate::people::set_role(&c, -1, "gift", "rc_beta", true).unwrap();
        // seventy places (ivy and late hold two), then no more
        for i in 2..BETA_PLACES {
            let u = add(&c, &format!("b{i}"));
            bought(&mut c, u, 30, now());
        }
        sweep_marks(&c);
        let taken: i64 = c.query_row("SELECT COUNT(*) FROM roles WHERE role = 'rc_beta'", [], |r| r.get(0)).unwrap();
        assert_eq!(taken, BETA_PLACES + 1, "seventy who bought it, and hers by hand beside them");
        let over = add(&c, "seventy-first");
        bought(&mut c, over, 30, now());
        marks(&c, over);
        assert!(has(&c, over, "recode") && !has(&c, over, "rc_beta") && !badge(&c, over, "rc_beta"));
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    fn fresh() -> (Connection, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("cursed-sight-{}.db", crate::random_hex(4)));
        let c = crate::open(path.to_str().unwrap());
        (c, path)
    }

    fn add(c: &Connection, login: &str) -> i64 {
        c.execute("INSERT INTO users (login, login_key, email, email_key, pass, created) VALUES (?1, lower(?1), ?1 || '@e.st', lower(?1) || '@e.st', 'x', 0)", params![login]).unwrap();
        c.last_insert_rowid()
    }
}
