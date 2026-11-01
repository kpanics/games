//! Friends: requests (from anyone, unless blocked), friendships, and the members one has blocked. Who is whose friend
//! is kept in memory as well, for what goes out live: when a member comes, goes or puts on another song, their friends
//! hear of it (as much as the member's privacy shows).
//!
//! A pair is one row with the smaller UID first; queries look both ways all the same (a UID moved by hand keeps its
//! rows, not their order).

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, OnceLock};

use axum::extract::{DefaultBodyLimit, Path as Key, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::market::{busy, member, run, said};
use crate::{db, fail, now, Shared};

/// Friends a member may have, and requests they may have out at once.
const MOST_FRIENDS: i64 = 500;
const MOST_OUT: i64 = 100;

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS friends (
             a INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             b INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             since INTEGER NOT NULL,
             PRIMARY KEY (a, b));
         CREATE INDEX IF NOT EXISTS friends_b ON friends(b);
         CREATE TABLE IF NOT EXISTS friend_requests (
             from_uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             to_uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             at INTEGER NOT NULL,
             PRIMARY KEY (from_uid, to_uid));
         CREATE INDEX IF NOT EXISTS friend_requests_to ON friend_requests(to_uid);
         CREATE TABLE IF NOT EXISTS mc_friends (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             names TEXT NOT NULL,
             updated INTEGER NOT NULL);",
    )
    .expect("make the friends' tables");
    load(c);
}

/// A game nick as Minecraft takes it: 1 to 16 of letters, digits and "_".
pub fn game_nick(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty() && s.len() <= 16 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then(|| s.to_string())
}

// ------------------------------------------------------------------------------------------------ in memory

static FRIENDS: OnceLock<Mutex<HashMap<i64, HashSet<i64>>>> = OnceLock::new();

fn memory() -> MutexGuard<'static, HashMap<i64, HashSet<i64>>> {
    FRIENDS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner())
}

/// Every friendship read again (at the start).
fn load(c: &Connection) {
    let pairs: Vec<(i64, i64)> = c
        .prepare("SELECT a, b FROM friends")
        .and_then(|mut q| q.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|m| m.flatten().collect()))
        .unwrap_or_default();
    let mut m = memory();
    m.clear();
    for (a, b) in pairs {
        m.entry(a).or_default().insert(b);
        m.entry(b).or_default().insert(a);
    }
}

/// A member's friends, as kept in memory.
pub fn of(uid: i64) -> Vec<i64> {
    memory().get(&uid).map(|s| s.iter().copied().collect()).unwrap_or_default()
}

pub fn are_friends(x: i64, y: i64) -> bool {
    memory().get(&x).is_some_and(|s| s.contains(&y))
}

// ------------------------------------------------------------------------------------------------ the database

fn pair(x: i64, y: i64) -> (i64, i64) {
    (x.min(y), x.max(y))
}

fn befriend(c: &Connection, x: i64, y: i64) -> rusqlite::Result<()> {
    let (a, b) = pair(x, y);
    c.execute("INSERT OR IGNORE INTO friends (a, b, since) VALUES (?1, ?2, ?3)", params![a, b, now()])?;
    c.execute("DELETE FROM friend_requests WHERE (from_uid = ?1 AND to_uid = ?2) OR (from_uid = ?2 AND to_uid = ?1)", params![x, y])?;
    {
        let mut m = memory();
        m.entry(x).or_default().insert(y);
        m.entry(y).or_default().insert(x);
    }
    for uid in [x, y] {
        crate::people::set_stat(c, uid, "friends", count(c, uid));
    }
    Ok(())
}

/// Friends no more (and no request either way).
fn part(c: &Connection, x: i64, y: i64) -> rusqlite::Result<()> {
    c.execute("DELETE FROM friends WHERE (a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1)", params![x, y])?;
    c.execute("DELETE FROM friend_requests WHERE (from_uid = ?1 AND to_uid = ?2) OR (from_uid = ?2 AND to_uid = ?1)", params![x, y])?;
    let mut m = memory();
    if let Some(s) = m.get_mut(&x) {
        s.remove(&y);
    }
    if let Some(s) = m.get_mut(&y) {
        s.remove(&x);
    }
    Ok(())
}

fn count(c: &Connection, uid: i64) -> i64 {
    c.query_row("SELECT COUNT(*) FROM friends WHERE a = ?1 OR b = ?1", params![uid], |r| r.get(0)).unwrap_or(0)
}

fn requested(c: &Connection, from: i64, to: i64) -> bool {
    c.query_row("SELECT 1 FROM friend_requests WHERE from_uid = ?1 AND to_uid = ?2", params![from, to], |_| Ok(())).optional().ok().flatten().is_some()
}

/// Whether either has blocked the other.
pub fn apart(c: &Connection, x: i64, y: i64) -> bool {
    crate::people::blocked(c, x, y) || crate::people::blocked(c, y, x)
}

/// Both of them hear that their lists changed (each page and game asks for its list again).
fn tell(uids: &[i64]) {
    for uid in uids {
        crate::live::publish(*uid, &json!({ "t": "friends" }));
    }
}

// ------------------------------------------------------------------------------------------------ the doors

/// The member's lists: friends (with where they are, as each lets them be seen), requests in and out, the blocked.
async fn lists(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let rows = |sql: &str| -> rusqlite::Result<Vec<(i64, i64)>> { c.prepare(sql)?.query_map(params![uid], |r| Ok((r.get(0)?, r.get(1)?)))?.collect() };
            let friends: Vec<Value> = rows("SELECT CASE WHEN a = ?1 THEN b ELSE a END, since FROM friends WHERE a = ?1 OR b = ?1 ORDER BY since DESC")?
                .into_iter()
                .map(|(f, since)| json!({ "person": crate::people::card_of(c, f), "since": since, "presence": crate::people::friend_presence(f) }))
                .collect();
            let incoming: Vec<Value> = rows("SELECT from_uid, at FROM friend_requests WHERE to_uid = ?1 ORDER BY at DESC")?.into_iter().map(|(f, at)| json!({ "person": crate::people::card_of(c, f), "at": at })).collect();
            let outgoing: Vec<Value> = rows("SELECT to_uid, at FROM friend_requests WHERE from_uid = ?1 ORDER BY at DESC")?.into_iter().map(|(f, at)| json!({ "person": crate::people::card_of(c, f), "at": at })).collect();
            let blocked: Vec<Value> = rows("SELECT blocked, at FROM blocks WHERE uid = ?1 ORDER BY at DESC")?.into_iter().map(|(f, at)| json!({ "person": crate::people::card_of(c, f), "at": at })).collect();
            Ok(json!({ "friends": friends, "incoming": incoming, "outgoing": outgoing, "blocked": blocked }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Who {
    login: String,
}

/// Asks someone to be friends; a request of theirs already waiting is accepted instead.
async fn ask(State(app): State<Shared>, h: HeaderMap, Json(b): Json<Who>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let login = b.login.trim().to_string();
        let out = db(&app, move |c| -> Result<(&'static str, i64), Response> {
            let other = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if other == uid {
                return Err(said(StatusCode::BAD_REQUEST, "self", "that is you"));
            }
            if apart(c, uid, other) {
                return Err(said(StatusCode::FORBIDDEN, "blocked", "one of you has blocked the other"));
            }
            if are_friends(uid, other) {
                return Ok(("friends", other));
            }
            if requested(c, other, uid) {
                if count(c, uid) >= MOST_FRIENDS || count(c, other) >= MOST_FRIENDS {
                    return Err(said(StatusCode::BAD_REQUEST, "full", "a list of friends holds 500"));
                }
                befriend(c, uid, other).map_err(|_| busy())?;
                crate::notify::add(c, other, "friend", Some(uid), &format!("friend:{uid}"), json!({ "accepted": true }));
                return Ok(("friends", other));
            }
            if requested(c, uid, other) {
                return Ok(("sent", other));
            }
            let out: i64 = c.query_row("SELECT COUNT(*) FROM friend_requests WHERE from_uid = ?1", params![uid], |r| r.get(0)).unwrap_or(0);
            if out >= MOST_OUT {
                return Err(said(StatusCode::BAD_REQUEST, "many", "100 requests wait already: let some be answered first"));
            }
            c.execute("INSERT INTO friend_requests (from_uid, to_uid, at) VALUES (?1, ?2, ?3)", params![uid, other, now()]).map_err(|_| busy())?;
            crate::notify::add_or_refresh(c, other, "friend", Some(uid), &format!("friend:{uid}"), json!({ "request": true }));
            Ok(("sent", other))
        })
        .await?;
        tell(&[uid, out.1]);
        Ok(Json(json!({ "state": out.0 })).into_response())
    })
    .await
}

/// A request accepted.
async fn accept(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let other = db(&app, move |c| -> Result<i64, Response> {
            let other = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if !requested(c, other, uid) {
                return Err(fail(StatusCode::NOT_FOUND, "missing"));
            }
            if apart(c, uid, other) {
                return Err(said(StatusCode::FORBIDDEN, "blocked", "one of you has blocked the other"));
            }
            if count(c, uid) >= MOST_FRIENDS || count(c, other) >= MOST_FRIENDS {
                return Err(said(StatusCode::BAD_REQUEST, "full", "a list of friends holds 500"));
            }
            befriend(c, uid, other).map_err(|_| busy())?;
            crate::notify::add(c, other, "friend", Some(uid), &format!("friend:{uid}"), json!({ "accepted": true }));
            Ok(other)
        })
        .await?;
        tell(&[uid, other]);
        Ok(Json(json!({ "state": "friends" })).into_response())
    })
    .await
}

/// Friends no more, or a request taken back or turned down: whichever there is between the two.
async fn unfriend(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let other = db(&app, move |c| -> Result<i64, Response> {
            let other = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            part(c, uid, other).map_err(|_| busy())?;
            crate::people::set_stat(c, uid, "friends", count(c, uid));
            crate::people::set_stat(c, other, "friends", count(c, other));
            Ok(other)
        })
        .await?;
        tell(&[uid, other]);
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// Someone blocked: no requests, no messages, no invitations from them; a friendship between them ends.
async fn block(State(app): State<Shared>, h: HeaderMap, Json(b): Json<Who>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let login = b.login.trim().to_string();
        let other = db(&app, move |c| -> Result<i64, Response> {
            let other = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if other == uid {
                return Err(said(StatusCode::BAD_REQUEST, "self", "that is you"));
            }
            if crate::people::is_staff(other) {
                return Err(said(StatusCode::BAD_REQUEST, "staff", "Cursed's staff can't be blocked"));
            }
            c.execute("INSERT OR IGNORE INTO blocks (uid, blocked, at) VALUES (?1, ?2, ?3)", params![uid, other, now()]).map_err(|_| busy())?;
            part(c, uid, other).map_err(|_| busy())?;
            crate::people::set_stat(c, other, "friends", count(c, other));
            Ok(other)
        })
        .await?;
        tell(&[uid, other]);
        Ok(Json(json!({})).into_response())
    })
    .await
}

async fn unblock(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let other = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            c.execute("DELETE FROM blocks WHERE uid = ?1 AND blocked = ?2", params![uid, other]).map_err(|_| busy())?;
            Ok(())
        })
        .await?;
        tell(&[uid]);
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// Between the member and someone: friends, a request either way, a block either way (for a profile's buttons).
pub fn relation(c: &Connection, uid: i64, other: i64) -> Value {
    json!({
        "friends": are_friends(uid, other),
        "asked": requested(c, uid, other),
        "asks": requested(c, other, uid),
        "blocked": crate::people::blocked(c, uid, other),
        "blocks_me": crate::people::blocked(c, other, uid),
    })
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(4 * 1024);
    Router::new()
        .route("/api/friends", get(lists).post(ask).layer(json()))
        .route("/api/friends/{login}/accept", post(accept))
        .route("/api/friends/{login}", delete(unfriend))
        .route("/api/blocks", post(block).layer(json()))
        .route("/api/blocks/{login}", delete(unblock))
        .route("/api/me/mc-friends", get(mc_friends).put(set_mc_friends).layer(DefaultBodyLimit::max(16 * 1024)))
}

/// No Friend Damage's friends by nick (the game's own list, kept here so a new install has it): {"names": [...]}.
async fn mc_friends(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let got: Option<(String, i64)> = c
                .query_row("SELECT names, updated FROM mc_friends WHERE uid = ?1", params![uid], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
                .map_err(|_| busy())?;
            Ok(match got {
                Some((names, updated)) => json!({ "names": serde_json::from_str::<Value>(&names).unwrap_or(json!([])), "updated": updated }),
                None => json!({ "names": [], "updated": 0 }),
            })
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct McFriends {
    names: Vec<String>,
}

async fn set_mc_friends(State(app): State<Shared>, h: HeaderMap, Json(b): Json<McFriends>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let mut names: Vec<String> = b.names.iter().filter_map(|n| game_nick(n)).map(|n| n.to_lowercase()).collect();
        names.sort();
        names.dedup();
        if names.len() > 500 {
            return Err(said(StatusCode::BAD_REQUEST, "many", "500 at most"));
        }
        let at = now();
        let text = serde_json::to_string(&names).unwrap_or_else(|_| "[]".into());
        db(&app, move |c| {
            c.execute(
                "INSERT INTO mc_friends (uid, names, updated) VALUES (?1, ?2, ?3) ON CONFLICT(uid) DO UPDATE SET names = ?2, updated = ?3",
                params![uid, text, at],
            )
            .map_err(|_| busy())
        })
        .await?;
        Ok(Json(json!({ "updated": at })).into_response())
    })
    .await
}
