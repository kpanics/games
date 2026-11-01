//! The cosmetics' catalog (the pack made by tools/cosmetics/pack.py in the client's sources): to members only, as it
//! lies next to the database (`cosmetics/catalog.json`); the files it names nginx serves by itself (`/cos/f/<ab>/<sha>`,
//! named by what is in them, kept for ever), to anyone who knows their names.
//!
//! And what each member wears, so that the others see it: the client says its outfit with the game's player it plays
//! as (UUID, name) and the emote it plays; another client asks for the players it sees, by their UUIDs. Only what is
//! worn (the items' ids and their settings), the emote and when it started: nothing else of the player.

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::market::member;
use crate::Shared;

fn catalog_path(app: &Shared) -> std::path::PathBuf {
    std::path::Path::new(&app.db_path).parent().unwrap_or(std::path::Path::new(".")).join("cosmetics").join("catalog.json")
}

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS outfits (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             mc_uuid TEXT NOT NULL,
             mc_name TEXT NOT NULL,
             worn TEXT NOT NULL,
             emote INTEGER,
             emote_at INTEGER,
             updated INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS outfits_uuid ON outfits(mc_uuid);",
    )
    .expect("the outfits' table");
}

fn millis() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// A UUID as one form: 32 hex digits, small letters.
fn uuid_of(s: &str) -> Option<String> {
    let u: String = s.chars().filter(|c| *c != '-').collect::<String>().to_ascii_lowercase();
    (u.len() == 32 && u.chars().all(|c| c.is_ascii_hexdigit())).then_some(u)
}

/// The catalog, to a member; "not modified" when the client has this one already (its ETag: what is in it).
async fn catalog(State(app): State<Shared>, h: HeaderMap) -> Response {
    if let Err(no) = member(&app, &h, false).await {
        return no;
    }
    let path = catalog_path(&app);
    let Ok(bytes) = crate::blocking(move || std::fs::read(path)).await else {
        return crate::fail(StatusCode::NOT_FOUND, "no-catalog");
    };
    let sum = Sha256::digest(&bytes);
    let tag = format!("\"{}\"", sum.iter().take(12).map(|b| format!("{b:02x}")).collect::<String>());
    // (Compressed on its way, nginx marks the tag weak: W/"…". It is the same catalog.)
    if h.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).map(|v| v.trim_start_matches("W/")) == Some(tag.as_str()) {
        let mut res = StatusCode::NOT_MODIFIED.into_response();
        res.headers_mut().insert(header::ETAG, HeaderValue::from_str(&tag).expect("hex"));
        return res;
    }
    let mut res = bytes.into_response();
    let hs = res.headers_mut();
    hs.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    hs.insert(header::ETAG, HeaderValue::from_str(&tag).expect("hex"));
    hs.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    res
}

#[derive(Deserialize)]
struct Outfit {
    uuid: String,
    name: String,
    worn: Map<String, Value>,
    /// The emote playing now (its number), none.
    #[serde(default)]
    emote: Option<i64>,
    /// Whether it is the same emote as last said (its start is kept).
    #[serde(default)]
    same: bool,
}

/// A member's outfit, as the game's player they play as: kept, the one before it gone (a UUID is one member's).
async fn put_outfit(State(app): State<Shared>, h: HeaderMap, Json(o): Json<Outfit>) -> Response {
    let who = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(no) => return no,
    };
    let Some(uuid) = uuid_of(&o.uuid) else { return crate::fail(StatusCode::BAD_REQUEST, "uuid") };
    let name: String = o.name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(16).collect();
    // Only short words: what is worn, its settings.
    let mut worn = Map::new();
    for (k, v) in o.worn.iter().take(96) {
        if k.len() > 80 {
            continue;
        }
        if let Some(s) = v.as_str().filter(|s| s.len() <= 64) {
            worn.insert(k.clone(), Value::String(s.to_string()));
        }
    }
    let worn = Value::Object(worn).to_string();
    let (emote, same, at) = (o.emote.filter(|e| (0..100_000).contains(e)), o.same, millis());
    let uid = who.uid;
    let done = crate::db(&app, move |c| -> rusqlite::Result<()> {
        let tx = c.transaction()?;
        tx.execute("DELETE FROM outfits WHERE mc_uuid = ?1 AND uid != ?2", params![uuid, uid])?;
        let before: Option<(Option<i64>, Option<i64>)> = tx.query_row("SELECT emote, emote_at FROM outfits WHERE uid = ?1", params![uid], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        // The same emote as before keeps its start; another starts now.
        let emote_at = match (emote, before) {
            (Some(e), Some((Some(was), Some(at_was)))) if same && was == e => Some(at_was),
            (Some(_), _) => Some(at),
            (None, _) => None,
        };
        tx.execute(
            "INSERT INTO outfits (uid, mc_uuid, mc_name, worn, emote, emote_at, updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(uid) DO UPDATE SET mc_uuid = ?2, mc_name = ?3, worn = ?4, emote = ?5, emote_at = ?6, updated = ?7",
            params![uid, uuid, name, worn, emote, emote_at, at],
        )?;
        tx.commit()
    })
    .await;
    match done {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(_) => crate::fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
}

#[derive(Deserialize)]
struct Asked {
    /// The UUIDs, by commas.
    u: String,
}

/// The outfits of the players a member sees (by their UUIDs, at most 64): what each wears, the emote and how long it
/// has played (milliseconds), and whether they play with Cursed now. Players who are not members (or said nothing for a
/// week) are not in it.
async fn get_outfits(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Asked>) -> Response {
    if let Err(no) = member(&app, &h, false).await {
        return no;
    }
    let uuids: Vec<String> = q.u.split(',').filter_map(uuid_of).take(64).collect();
    let now = millis();
    let found = crate::db(&app, move |c| -> HashMap<String, Value> {
        let mut out = HashMap::new();
        let Ok(mut st) = c.prepare("SELECT worn, emote, emote_at, updated FROM outfits WHERE mc_uuid = ?1") else { return out };
        for u in uuids {
            let row = st.query_row(params![u], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, i64>(3)?)));
            let Ok((worn, emote, at, updated)) = row else { continue };
            if now - updated > 7 * 86_400_000 {
                continue;
            }
            let worn: Value = serde_json::from_str(&worn).unwrap_or(Value::Null);
            let since = at.map(|a| (now - a).max(0));
            // In the game with Cursed now: its client says its outfit every minute while it plays (the name tags and the
            // player list mark them).
            let live = now - updated < 180_000;
            out.insert(u, json!({ "worn": worn, "emote": emote, "since": since, "live": live }));
        }
        out
    })
    .await;
    let mut res = Json(json!({ "outfits": found })).into_response();
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    res
}

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/cosmetics", get(catalog))
        .route("/api/cosmetics/outfit", post(put_outfit))
        .route("/api/cosmetics/outfits", get(get_outfits))
}
