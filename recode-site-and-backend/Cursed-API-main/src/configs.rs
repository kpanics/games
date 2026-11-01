//! Configs in the cloud: a member's settings of the client (every module's, and where the interface's elements
//! stand) kept here instead of in files. One of them is the member's active one — the client runs by it and sends its
//! changes up; others are kept to switch to. A config is shared by a code with a limit of activations: whoever
//! redeems it may run by it (not change it), and gets every change its owner makes; the owner sees who did and takes
//! the access back, one by one or with the whole code.
//!
//! Each config is one client's — Cursed 1.21.11's ("cursed", what was before) or Recode's ("recode"): their modules
//! differ, so each lists, makes and runs by its own (an active one for each). A config keeps its versions before (the
//! last 30): its owner sees them and takes one back.

use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::market::{busy, member, run, said};
use crate::{db, fail, now, Shared};

/// Configs a member may keep, the size of one, codes a config may have out at once.
const MOST_CONFIGS: i64 = 20;
const MOST_BYTES: usize = 256 * 1024;
const MOST_CODES: i64 = 10;
const MOST_USES: i64 = 10_000;
/// Letters of a code: no 0/O, 1/I/L — read off a screen and typed again.
const CODE_LETTERS: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
/// The clients whose configs these are; the versions a config keeps.
const CLIENTS: [&str; 2] = ["cursed", "recode"];
const MOST_VERSIONS: i64 = 30;

/// A client as asked for: "recode", or Cursed 1.21.11's (what was before).
fn client_of_ask(s: Option<&str>) -> &'static str {
    CLIENTS.iter().copied().find(|c| Some(*c) == s).unwrap_or(CLIENTS[0])
}

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS configs (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             owner INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             name TEXT NOT NULL,
             data TEXT NOT NULL,                      -- {\"modules\": {...}, \"interface\": {...}}
             bytes INTEGER NOT NULL,
             version INTEGER NOT NULL DEFAULT 1,
             created INTEGER NOT NULL,
             updated INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS configs_owner ON configs(owner);
         CREATE TABLE IF NOT EXISTS config_codes (
             code TEXT PRIMARY KEY,
             config INTEGER NOT NULL REFERENCES configs(id) ON DELETE CASCADE,
             max_uses INTEGER NOT NULL,
             uses INTEGER NOT NULL DEFAULT 0,
             created INTEGER NOT NULL,
             revoked INTEGER);
         CREATE INDEX IF NOT EXISTS config_codes_config ON config_codes(config);
         CREATE TABLE IF NOT EXISTS config_access (
             config INTEGER NOT NULL REFERENCES configs(id) ON DELETE CASCADE,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             code TEXT NOT NULL,
             at INTEGER NOT NULL,
             PRIMARY KEY (config, uid));
         CREATE INDEX IF NOT EXISTS config_access_uid ON config_access(uid);
         CREATE TABLE IF NOT EXISTS config_active (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             config INTEGER NOT NULL);",
    )
    .expect("make the configs' tables");
    // each config one client's (those before: Cursed 1.21.11's), an active one for each client, the versions kept
    let has_client = c.prepare("SELECT client FROM configs LIMIT 0").is_ok();
    if !has_client {
        c.execute_batch("ALTER TABLE configs ADD COLUMN client TEXT NOT NULL DEFAULT 'cursed';").expect("the configs' client");
    }
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS config_active_by (
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             client TEXT NOT NULL,
             config INTEGER NOT NULL,
             PRIMARY KEY (uid, client));
         INSERT OR IGNORE INTO config_active_by (uid, client, config) SELECT a.uid, 'cursed', a.config FROM config_active a JOIN users u ON u.uid = a.uid;
         CREATE TABLE IF NOT EXISTS config_history (
             config INTEGER NOT NULL REFERENCES configs(id) ON DELETE CASCADE,
             version INTEGER NOT NULL,
             data TEXT NOT NULL,
             bytes INTEGER NOT NULL,
             at INTEGER NOT NULL,
             PRIMARY KEY (config, version));",
    )
    .expect("make the configs' clients and versions");
}

fn code() -> String {
    let raw = crate::random_hex(8);
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(9);
    for i in 0..8 {
        if i == 4 {
            out.push('-');
        }
        let v = u8::from_str_radix(std::str::from_utf8(&bytes[i * 2..i * 2 + 2]).unwrap_or("00"), 16).unwrap_or(0);
        out.push(CODE_LETTERS[v as usize % CODE_LETTERS.len()] as char);
    }
    out
}

/// A code as typed: upper case, the dash wherever it was, spaces gone.
fn tidy(code: &str) -> String {
    let letters: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
    if letters.len() == 8 { format!("{}-{}", &letters[..4], &letters[4..]) } else { letters }
}

fn owner_of(c: &Connection, id: i64) -> Option<i64> {
    c.query_row("SELECT owner FROM configs WHERE id = ?1", params![id], |r| r.get(0)).optional().ok().flatten()
}

fn has_access(c: &Connection, id: i64, uid: i64) -> bool {
    c.query_row("SELECT 1 FROM config_access WHERE config = ?1 AND uid = ?2", params![id, uid], |_| Ok(())).optional().ok().flatten().is_some()
}

/// Whose client a config is.
fn client_of(c: &Connection, id: i64) -> Option<String> {
    c.query_row("SELECT client FROM configs WHERE id = ?1", params![id], |r| r.get(0)).optional().ok().flatten()
}

/// The config a member runs by in a client.
fn active_of(c: &Connection, uid: i64, client: &str) -> Option<i64> {
    c.query_row("SELECT config FROM config_active_by WHERE uid = ?1 AND client = ?2", params![uid, client], |r| r.get(0)).optional().ok().flatten()
}

fn set_active(c: &Connection, uid: i64, client: &str, id: i64) -> rusqlite::Result<usize> {
    c.execute("INSERT INTO config_active_by (uid, client, config) VALUES (?1, ?2, ?3) ON CONFLICT(uid, client) DO UPDATE SET config = ?3", params![uid, client, id])
}

/// A member lost a config they ran by (one of `client`'s): their active one there falls back to their own first (or
/// none).
fn fall_back(c: &Connection, uid: i64, gone: i64, client: &str) {
    if active_of(c, uid, client) == Some(gone) {
        let own: Option<i64> = c.query_row("SELECT id FROM configs WHERE owner = ?1 AND client = ?2 ORDER BY created LIMIT 1", params![uid, client], |r| r.get(0)).optional().ok().flatten();
        match own {
            Some(o) => {
                let _ = set_active(c, uid, client, o);
            }
            None => {
                let _ = c.execute("DELETE FROM config_active_by WHERE uid = ?1 AND client = ?2", params![uid, client]);
            }
        }
    }
    crate::live::publish(uid, &json!({ "t": "config_gone", "id": gone, "client": client, "active": active_of(c, uid, client) }));
}

/// A config's data now kept among its versions (before it changes): the last 30 stay.
fn keep_version(c: &Connection, id: i64) -> rusqlite::Result<()> {
    let (version, data, bytes): (i64, String, i64) = c.query_row("SELECT version, data, bytes FROM configs WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    c.execute("INSERT OR REPLACE INTO config_history (config, version, data, bytes, at) VALUES (?1, ?2, ?3, ?4, ?5)", params![id, version, data, bytes, now()])?;
    c.execute("DELETE FROM config_history WHERE config = ?1 AND version <= (SELECT MAX(version) FROM config_history WHERE config = ?1) - ?2", params![id, MOST_VERSIONS])?;
    Ok(())
}

/// A config's data changed: the one before kept, the version one more; what it is now.
fn change_data_at(c: &Connection, id: i64, data: &str, expected: Option<i64>) -> rusqlite::Result<Option<i64>> {
    // The check, history and replacement are one write transaction, including against another
    // API process. A rejected stale write cannot alter either the data or its version history.
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    let version: i64 = tx.query_row("SELECT version FROM configs WHERE id = ?1", params![id], |r| r.get(0))?;
    if expected.is_some_and(|want| want != version) { return Ok(None); }
    keep_version(&tx, id)?;
    tx.execute("UPDATE configs SET data = ?2, bytes = ?3, version = version + 1, updated = ?4 WHERE id = ?1", params![id, data, data.len() as i64, now()])?;
    tx.commit()?;
    Ok(Some(version + 1))
}

#[cfg(test)]
fn change_data(c: &Connection, id: i64, data: &str) -> rusqlite::Result<i64> {
    change_data_at(c, id, data, None)?.ok_or(rusqlite::Error::InvalidQuery)
}

/// Everyone running by a config hears it changed (its owner, who came in by a code).
fn tell_changed(c: &Connection, id: i64, owner: i64, version: i64) {
    let event = json!({ "t": "config", "id": id, "version": version });
    crate::live::publish(owner, &event);
    let users: Vec<i64> = c.prepare("SELECT uid FROM config_access WHERE config = ?1").and_then(|mut q| q.query_map(params![id], |r| r.get(0)).map(|m| m.flatten().collect())).unwrap_or_default();
    for u in users {
        crate::live::publish(u, &event);
    }
}

fn summary(c: &Connection, id: i64, viewer: i64) -> Option<Value> {
    let (owner, name, bytes, version, created, updated, client): (i64, String, i64, i64, i64, i64, String) = c
        .query_row("SELECT owner, name, bytes, version, created, updated, client FROM configs WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))
        .optional()
        .ok()
        .flatten()?;
    let own = owner == viewer;
    let active = active_of(c, viewer, &client) == Some(id);
    let mut v = json!({ "id": id, "name": name, "bytes": bytes, "version": version, "created": created, "updated": updated, "own": own, "active": active, "client": client });
    if own {
        let codes: Vec<Value> = c
            .prepare("SELECT code, max_uses, uses, created FROM config_codes WHERE config = ?1 AND revoked IS NULL ORDER BY created")
            .and_then(|mut q| q.query_map(params![id], |r| Ok(json!({ "code": r.get::<_, String>(0)?, "max_uses": r.get::<_, i64>(1)?, "uses": r.get::<_, i64>(2)?, "created": r.get::<_, i64>(3)? }))).map(|m| m.flatten().collect()))
            .unwrap_or_default();
        let access: i64 = c.query_row("SELECT COUNT(*) FROM config_access WHERE config = ?1", params![id], |r| r.get(0)).unwrap_or(0);
        v["codes"] = json!(codes);
        v["access"] = json!(access);
    } else {
        v["owner"] = crate::people::card_of(c, owner);
    }
    Some(v)
}

/// A config's data as the client sends it: an object, small enough.
fn data_of(v: &Value) -> Result<String, Response> {
    if !v.is_object() {
        return Err(said(StatusCode::BAD_REQUEST, "data", "the settings of the client"));
    }
    let text = v.to_string();
    if text.len() > MOST_BYTES {
        return Err(said(StatusCode::PAYLOAD_TOO_LARGE, "data", "a config is 256 KB at most"));
    }
    Ok(text)
}

fn name_of(s: &str) -> Result<String, Response> {
    let n = crate::market::clean_line(s, 40);
    if n.chars().count() < 1 {
        return Err(said(StatusCode::BAD_REQUEST, "name", "a name of 1 to 40 letters"));
    }
    Ok(n)
}

// ------------------------------------------------------------------------------------------------ the doors

#[derive(Deserialize)]
struct Which {
    client: Option<String>,
}

/// A member's configs of a client (`?client=recode`; none: Cursed 1.21.11's): their own, those shared with them, the
/// one they run by.
async fn list(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Which>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let client = client_of_ask(q.client.as_deref());
        let v = db(&app, move |c| -> rusqlite::Result<Value> {
            let mine: Vec<i64> = c.prepare("SELECT id FROM configs WHERE owner = ?1 AND client = ?2 ORDER BY created")?.query_map(params![uid, client], |r| r.get(0))?.flatten().collect();
            let shared: Vec<i64> = c
                .prepare("SELECT a.config FROM config_access a JOIN configs f ON f.id = a.config WHERE a.uid = ?1 AND f.client = ?2 ORDER BY a.at DESC")?
                .query_map(params![uid, client], |r| r.get(0))?
                .flatten()
                .collect();
            Ok(json!({
                "mine": mine.into_iter().filter_map(|id| summary(c, id, uid)).collect::<Vec<_>>(),
                "shared": shared.into_iter().filter_map(|id| summary(c, id, uid)).collect::<Vec<_>>(),
                "active": active_of(c, uid, client),
                "client": client,
                "most": MOST_CONFIGS,
                "versioned_updates": true,
                "versioned_history": true,
            }))
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct New {
    name: String,
    data: Value,
    #[serde(default)]
    activate: bool,
    /// Whose client it is ("recode"; none: Cursed 1.21.11's).
    #[serde(default)]
    client: Option<String>,
}

async fn create(State(app): State<Shared>, h: HeaderMap, Json(b): Json<New>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let name = name_of(&b.name)?;
        let data = data_of(&b.data)?;
        let client = client_of_ask(b.client.as_deref());
        let v = db(&app, move |c| -> Result<Value, Response> {
            let n: i64 = c.query_row("SELECT COUNT(*) FROM configs WHERE owner = ?1 AND client = ?2", params![uid, client], |r| r.get(0)).unwrap_or(0);
            if n >= MOST_CONFIGS {
                return Err(said(StatusCode::BAD_REQUEST, "many", "20 configs at most: delete one first"));
            }
            let at = now();
            c.execute("INSERT INTO configs (owner, name, data, bytes, created, updated, client) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)", params![uid, name, data, data.len() as i64, at, client])
                .map_err(|_| busy())?;
            let id = c.last_insert_rowid();
            // The first one is the active one at once (and the one asked to be).
            if b.activate || active_of(c, uid, client).is_none() {
                set_active(c, uid, client, id).map_err(|_| busy())?;
            }
            summary(c, id, uid).ok_or_else(busy)
        })
        .await?;
        Ok((StatusCode::CREATED, Json(json!({ "config": v }))).into_response())
    })
    .await
}

/// A config with its data: its owner's, or one shared with the member.
async fn one(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            let owner = owner_of(c, id).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if owner != uid && !has_access(c, id, uid) {
                return Err(fail(StatusCode::NOT_FOUND, "missing"));
            }
            let data: String = c.query_row("SELECT data FROM configs WHERE id = ?1", params![id], |r| r.get(0)).map_err(|_| busy())?;
            let mut v = summary(c, id, uid).ok_or_else(busy)?;
            v["data"] = serde_json::from_str(&data).unwrap_or(Value::Null);
            Ok(v)
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Put {
    data: Value,
    /// A client that read this version may replace it. Older clients can omit it.
    version: Option<i64>,
}

/// Its owner's settings, changed (the client sends them a few seconds after a change): everyone running by it hears.
async fn put(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<Put>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let data = data_of(&b.data)?;
        let v = db(&app, move |c| -> Result<Value, Response> {
            if owner_of(c, id) != Some(uid) {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            let version = change_data_at(c, id, &data, b.version).map_err(|_| busy())?
                .ok_or_else(|| said(StatusCode::CONFLICT, "version", "the config changed: read its current version first"))?;
            tell_changed(c, id, uid, version);
            Ok(json!({ "version": version }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Rename {
    name: String,
}

async fn rename(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<Rename>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let name = name_of(&b.name)?;
        db(&app, move |c| -> Result<(), Response> {
            if owner_of(c, id) != Some(uid) {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            c.execute("UPDATE configs SET name = ?2 WHERE id = ?1", params![id, name]).map_err(|_| busy())?;
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// A config gone: its owner's (whoever ran by it goes back to their own), or the member's access to someone's.
async fn remove(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let owner = owner_of(c, id).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            let client = client_of(c, id).unwrap_or_else(|| "cursed".into());
            if owner == uid {
                let n: i64 = c.query_row("SELECT COUNT(*) FROM configs WHERE owner = ?1 AND client = ?2", params![uid, client], |r| r.get(0)).unwrap_or(0);
                if n <= 1 && active_of(c, uid, &client) == Some(id) {
                    return Err(said(StatusCode::BAD_REQUEST, "last", "the last config of yours stays: make another first"));
                }
                let users: Vec<i64> = c.prepare("SELECT uid FROM config_access WHERE config = ?1").and_then(|mut q| q.query_map(params![id], |r| r.get(0)).map(|m| m.flatten().collect())).unwrap_or_default();
                c.execute("DELETE FROM configs WHERE id = ?1", params![id]).map_err(|_| busy())?;
                c.execute("DELETE FROM config_history WHERE config = ?1", params![id]).map_err(|_| busy())?;
                for u in users {
                    fall_back(c, u, id, &client);
                }
                fall_back(c, uid, id, &client);
            } else if has_access(c, id, uid) {
                c.execute("DELETE FROM config_access WHERE config = ?1 AND uid = ?2", params![id, uid]).map_err(|_| busy())?;
                fall_back(c, uid, id, &client);
            } else {
                return Err(fail(StatusCode::NOT_FOUND, "missing"));
            }
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// The config the member runs by now (their own, or one shared with them).
async fn activate(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            let owner = owner_of(c, id).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if owner != uid && !has_access(c, id, uid) {
                return Err(fail(StatusCode::NOT_FOUND, "missing"));
            }
            let client = client_of(c, id).unwrap_or_else(|| "cursed".into());
            set_active(c, uid, &client, id).map_err(|_| busy())?;
            crate::live::publish(uid, &json!({ "t": "config_active", "id": id, "client": client }));
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct NewCode {
    max_uses: i64,
}

/// A code to share a config with: activated by at most `max_uses` members.
async fn new_code(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(b): Json<NewCode>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        if !(1..=MOST_USES).contains(&b.max_uses) {
            return Err(said(StatusCode::BAD_REQUEST, "max_uses", "from 1 to 10 000 activations"));
        }
        let v = db(&app, move |c| -> Result<Value, Response> {
            if owner_of(c, id) != Some(uid) {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            let n: i64 = c.query_row("SELECT COUNT(*) FROM config_codes WHERE config = ?1 AND revoked IS NULL", params![id], |r| r.get(0)).unwrap_or(0);
            if n >= MOST_CODES {
                return Err(said(StatusCode::BAD_REQUEST, "many", "10 codes at most: take one back first"));
            }
            let mut code_ = code();
            while c.query_row("SELECT 1 FROM config_codes WHERE code = ?1", params![code_], |_| Ok(())).optional().ok().flatten().is_some() {
                code_ = code();
            }
            c.execute("INSERT INTO config_codes (code, config, max_uses, created) VALUES (?1, ?2, ?3, ?4)", params![code_, id, b.max_uses, now()]).map_err(|_| busy())?;
            Ok(json!({ "code": code_, "max_uses": b.max_uses, "uses": 0 }))
        })
        .await?;
        Ok((StatusCode::CREATED, Json(v)).into_response())
    })
    .await
}

/// A code taken back: it works no more, and everyone who came in by it loses the config.
async fn revoke_code(State(app): State<Shared>, h: HeaderMap, Key((id, code_)): Key<(i64, String)>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let code_ = tidy(&code_);
        let gone = db(&app, move |c| -> Result<usize, Response> {
            if owner_of(c, id) != Some(uid) {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            let changed = c.execute("UPDATE config_codes SET revoked = ?3 WHERE code = ?1 AND config = ?2 AND revoked IS NULL", params![code_, id, now()]).map_err(|_| busy())?;
            if changed == 0 {
                return Err(fail(StatusCode::NOT_FOUND, "missing"));
            }
            let users: Vec<i64> = c.prepare("SELECT uid FROM config_access WHERE config = ?1 AND code = ?2").and_then(|mut q| q.query_map(params![id, code_], |r| r.get(0)).map(|m| m.flatten().collect())).unwrap_or_default();
            c.execute("DELETE FROM config_access WHERE config = ?1 AND code = ?2", params![id, code_]).map_err(|_| busy())?;
            let client = client_of(c, id).unwrap_or_else(|| "cursed".into());
            for u in &users {
                fall_back(c, *u, id, &client);
            }
            Ok(users.len())
        })
        .await?;
        Ok(Json(json!({ "removed": gone })).into_response())
    })
    .await
}

/// Who runs by a config of the member's (came in by a code).
async fn access(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            if owner_of(c, id) != Some(uid) {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            let rows: Vec<Value> = c
                .prepare("SELECT uid, code, at FROM config_access WHERE config = ?1 ORDER BY at DESC")
                .and_then(|mut q| q.query_map(params![id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))).map(|m| m.flatten().collect::<Vec<_>>()))
                .unwrap_or_default()
                .into_iter()
                .map(|(u, code_, at)| json!({ "person": crate::people::card_of(c, u), "code": code_, "at": at, "active": client_of(c, id).is_some_and(|cl| active_of(c, u, &cl) == Some(id)) }))
                .collect();
            Ok(json!({ "people": rows }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

/// One member's access taken back by the config's owner.
async fn revoke_one(State(app): State<Shared>, h: HeaderMap, Key((id, login)): Key<(i64, String)>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        db(&app, move |c| -> Result<(), Response> {
            if owner_of(c, id) != Some(uid) {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            let other = crate::people::uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
            if c.execute("DELETE FROM config_access WHERE config = ?1 AND uid = ?2", params![id, other]).map_err(|_| busy())? == 0 {
                return Err(fail(StatusCode::NOT_FOUND, "missing"));
            }
            fall_back(c, other, id, &client_of(c, id).unwrap_or_else(|| "cursed".into()));
            Ok(())
        })
        .await?;
        Ok(Json(json!({})).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Redeem {
    code: String,
    /// The client typing it ("recode"; none: Cursed 1.21.11): another client's config is not for it.
    #[serde(default)]
    client: Option<String>,
}

/// A code typed in: the config is the member's to run by (one activation of the code's), and their active one now.
async fn redeem(State(app): State<Shared>, h: HeaderMap, Json(b): Json<Redeem>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let code_ = tidy(&b.code);
        let v = db(&app, move |c| -> Result<Value, Response> {
            let (id, max_uses, uses, revoked): (i64, i64, i64, Option<i64>) = c
                .query_row("SELECT config, max_uses, uses, revoked FROM config_codes WHERE code = ?1", params![code_], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .optional()
                .map_err(|_| busy())?
                .ok_or_else(|| said(StatusCode::NOT_FOUND, "code", "no such code"))?;
            if revoked.is_some() {
                return Err(said(StatusCode::GONE, "revoked", "this code was taken back"));
            }
            let owner = owner_of(c, id).ok_or_else(|| said(StatusCode::NOT_FOUND, "code", "no such code"))?;
            if owner == uid {
                return Err(said(StatusCode::BAD_REQUEST, "own", "this config is yours"));
            }
            let client = client_of(c, id).unwrap_or_else(|| "cursed".into());
            if b.client.is_some() && client_of_ask(b.client.as_deref()) != client {
                return Err(said(StatusCode::BAD_REQUEST, "client", if client == "recode" { "this config is Recode's" } else { "this config is Cursed 1.21.11's" }));
            }
            if crate::friends::apart(c, owner, uid) {
                return Err(said(StatusCode::FORBIDDEN, "blocked", "one of you has blocked the other"));
            }
            if !has_access(c, id, uid) {
                if uses >= max_uses {
                    return Err(said(StatusCode::GONE, "used", "this code has no activations left"));
                }
                c.execute("INSERT INTO config_access (config, uid, code, at) VALUES (?1, ?2, ?3, ?4)", params![id, uid, code_, now()]).map_err(|_| busy())?;
                c.execute("UPDATE config_codes SET uses = uses + 1 WHERE code = ?1", params![code_]).map_err(|_| busy())?;
                crate::people::bump(c, owner, "config_uses", 1);
            }
            set_active(c, uid, &client, id).map_err(|_| busy())?;
            crate::live::publish(owner, &json!({ "t": "configs" }));
            summary(c, id, uid).ok_or_else(busy)
        })
        .await?;
        Ok(Json(json!({ "config": v })).into_response())
    })
    .await
}

/// A config's versions before (its owner's): each one's number, when it was left, how big — the newest first.
async fn history(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            if owner_of(c, id) != Some(uid) {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            let rows: Vec<Value> = c
                .prepare("SELECT version, bytes, at FROM config_history WHERE config = ?1 ORDER BY version DESC")
                .and_then(|mut q| q.query_map(params![id], |r| Ok(json!({ "version": r.get::<_, i64>(0)?, "bytes": r.get::<_, i64>(1)?, "at": r.get::<_, i64>(2)? }))).map(|m| m.flatten().collect()))
                .unwrap_or_default();
            Ok(json!({ "versions": rows }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

/// A config back to one of its versions (as a new one: what it was just now is kept too); everyone running by it hears.
fn restore_version_at(c: &Connection, id: i64, version: i64, expected: Option<i64>) -> Result<i64, Response> {
    let data: String = c
        .query_row("SELECT data FROM config_history WHERE config = ?1 AND version = ?2", params![id, version], |r| r.get(0))
        .optional()
        .map_err(|_| busy())?
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))?;
    change_data_at(c, id, &data, expected).map_err(|_| busy())?
        .ok_or_else(|| said(StatusCode::CONFLICT, "version", "the config changed: read its current version first"))
}

#[cfg(test)]
fn restore_version(c: &Connection, id: i64, version: i64) -> Result<i64, Response> {
    restore_version_at(c, id, version, None)
}

#[derive(Deserialize)]
struct RestoreFrom { version: Option<i64> }

async fn restore(State(app): State<Shared>, h: HeaderMap, Key((id, version)): Key<(i64, i64)>, Query(q): Query<RestoreFrom>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let v = db(&app, move |c| -> Result<Value, Response> {
            if owner_of(c, id) != Some(uid) {
                return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
            }
            let now_version = restore_version_at(c, id, version, q.version)?;
            tell_changed(c, id, uid, now_version);
            Ok(json!({ "version": now_version }))
        })
        .await?;
        Ok(Json(v).into_response())
    })
    .await
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(MOST_BYTES + 4096);
    Router::new()
        .route("/api/configs", get(list).post(create).layer(json()))
        .route("/api/configs/redeem", post(redeem).layer(json()))
        .route("/api/configs/{id}", get(one).put(put).patch(rename).delete(remove).layer(json()))
        .route("/api/configs/{id}/active", post(activate))
        .route("/api/configs/{id}/codes", post(new_code).layer(json()))
        .route("/api/configs/{id}/codes/{code}", delete(revoke_code))
        .route("/api/configs/{id}/access", get(access))
        .route("/api/configs/{id}/access/{login}", delete(revoke_one))
        .route("/api/configs/{id}/history", get(history))
        .route("/api/configs/{id}/history/{version}", post(restore))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_reads_well_and_is_typed_back() {
        for _ in 0..200 {
            let c = code();
            assert_eq!(c.len(), 9);
            assert!(c.chars().all(|ch| ch == '-' || CODE_LETTERS.contains(&(ch as u8))), "{c}");
            assert_eq!(tidy(&c.to_lowercase().replace('-', " ")), c);
        }
    }

    fn base() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE users (uid INTEGER PRIMARY KEY); INSERT INTO users (uid) VALUES (7);").unwrap();
        c
    }

    #[test]
    fn stale_writes_keep_both_current_data_and_history_untouched() {
        let c = base();
        schema(&c);
        c.execute("INSERT INTO configs (owner, name, data, bytes, created, updated, client) VALUES (7, 'a', '{}', 2, 1, 1, 'recode')", []).unwrap();
        assert_eq!(change_data_at(&c, 1, r#"{"pc":"one"}"#, Some(1)).unwrap(), Some(2));
        assert_eq!(change_data_at(&c, 1, r#"{"pc":"two"}"#, Some(1)).unwrap(), None);
        let kept: (String, i64) = c.query_row("SELECT data, version FROM configs WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(kept, (r#"{"pc":"one"}"#.into(), 2));
        let history: Vec<(i64, String)> = c.prepare("SELECT version, data FROM config_history WHERE config=1 ORDER BY version").unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
        assert_eq!(history, vec![(1, "{}".into())]);
        assert_eq!(change_data_at(&c, 1, r#"{"pc":"two"}"#, Some(2)).unwrap(), Some(3));
        // Existing Cursed clients remain compatible; restores use the same atomic history path.
        assert_eq!(change_data(&c, 1, "{}").unwrap(), 4);
        assert_eq!(restore_version(&c, 1, 2).unwrap(), 5);
    }

    #[test]
    fn a_stale_history_restore_cannot_replace_a_newer_devices_settings() {
        let c = base();
        schema(&c);
        c.execute("INSERT INTO configs (owner, name, data, bytes, created, updated, client) VALUES (7, 'a', '{}', 2, 1, 1, 'recode')", []).unwrap();
        assert_eq!(change_data(&c, 1, r#"{"pc":"one"}"#).unwrap(), 2);
        assert_eq!(change_data(&c, 1, r#"{"pc":"two"}"#).unwrap(), 3);
        let denied = restore_version_at(&c, 1, 1, Some(2)).unwrap_err();
        assert_eq!(denied.status(), StatusCode::CONFLICT);
        let current: (String, i64) = c.query_row("SELECT data, version FROM configs WHERE id=1", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(current, (r#"{"pc":"two"}"#.into(),3));
        assert_eq!(c.query_row("SELECT COUNT(*) FROM config_history", [], |r| r.get::<_,i64>(0)).unwrap(),2);
        assert_eq!(restore_version_at(&c, 1, 1, Some(3)).unwrap(),4);
        assert_eq!(c.query_row("SELECT data FROM configs WHERE id=1", [], |r| r.get::<_,String>(0)).unwrap(),"{}");
    }

    #[test]
    fn failed_history_write_rolls_back_the_whole_change() {
        let c = base();
        schema(&c);
        c.execute("INSERT INTO configs (owner, name, data, bytes, created, updated, client) VALUES (7, 'a', '{}', 2, 1, 1, 'recode')", []).unwrap();
        c.execute_batch("CREATE TRIGGER fail_config_update BEFORE UPDATE ON configs BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(change_data_at(&c, 1, r#"{"new":true}"#, Some(1)).is_err());
        let kept: (String, i64) = c.query_row("SELECT data, version FROM configs WHERE id=1", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(kept, ("{}".into(),1));
        assert_eq!(c.query_row("SELECT COUNT(*) FROM config_history", [], |r| r.get::<_,i64>(0)).unwrap(),0);
    }

    /// The tables before the clients: their configs are Cursed 1.21.11's, their active ones kept as its.
    #[test]
    fn configs_before_are_cursed_ones_and_stay_active() {
        let c = base();
        c.execute_batch(
            "CREATE TABLE configs (id INTEGER PRIMARY KEY AUTOINCREMENT, owner INTEGER NOT NULL, name TEXT NOT NULL, data TEXT NOT NULL,
                 bytes INTEGER NOT NULL, version INTEGER NOT NULL DEFAULT 1, created INTEGER NOT NULL, updated INTEGER NOT NULL);
             CREATE TABLE config_active (uid INTEGER PRIMARY KEY, config INTEGER NOT NULL);
             INSERT INTO configs (owner, name, data, bytes, created, updated) VALUES (7, 'a', '{}', 2, 1, 1);
             INSERT INTO config_active (uid, config) VALUES (7, 1);",
        )
        .unwrap();
        schema(&c);
        schema(&c);
        assert_eq!(client_of(&c, 1).as_deref(), Some("cursed"));
        assert_eq!(active_of(&c, 7, "cursed"), Some(1));
        assert_eq!(active_of(&c, 7, "recode"), None);
        assert_eq!(client_of_ask(Some("recode")), "recode");
        assert_eq!(client_of_ask(Some("other")), "cursed");
        assert!(CLIENTS.contains(&client_of_ask(None)));
    }

    /// A change keeps the version before (the last 30), and a version taken back is a new one.
    #[test]
    fn versions_are_kept_and_taken_back() {
        let c = base();
        schema(&c);
        c.execute("INSERT INTO configs (owner, name, data, bytes, created, updated, client) VALUES (7, 'a', '{\"v\":0}', 7, 1, 1, 'recode')", []).unwrap();
        for v in 1..=40 {
            change_data(&c, 1, &format!("{{\"v\":{v}}}")).unwrap();
        }
        let kept: i64 = c.query_row("SELECT COUNT(*) FROM config_history WHERE config = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(kept, MOST_VERSIONS);
        let newest: i64 = c.query_row("SELECT MAX(version) FROM config_history WHERE config = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(newest, 40);
        let version = restore_version(&c, 1, 35).unwrap();
        assert_eq!(version, 42);
        let data: String = c.query_row("SELECT data FROM configs WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(data, "{\"v\":34}");
        assert!(restore_version(&c, 1, 3).is_err());
    }
}
