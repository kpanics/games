//! The market: resource packs and shader packs that members upload, checked and rebuilt, with their pages.
//!
//! How a pack gets in:
//! 1. POST /api/uploads announces a file; its pieces come as PUT /api/uploads/{id}?at=N (8 MB each, so a big pack
//!    survives a broken connection: GET says how much arrived).
//! 2. The last piece queues it. One check at a time, in its own thread at low priority (inspect.rs): refused, or ready,
//!    with what was read from it and a clean copy stored under its SHA-256.
//! 3. POST /api/packs makes a pack of a ready upload (its first version), POST /api/packs/{key}/versions adds one.
//!
//! Everything asks for an account: the market is for members, to look at and to use (her word, 27.09). The site and
//! the client use the same calls (the client with its token). Files go out through nginx (X-Accel-Redirect), pictures
//! straight from /media/.

pub mod format;
pub mod inspect;
pub mod picture;
pub mod zip;

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

use crate::{blocking, db, fail, now, presented, random_hex, same_site, who, Shared, Who};

pub const CHUNK: usize = 8 * 1024 * 1024;
const PICTURE_MAX: usize = 12 * 1024 * 1024;
const PICTURES_PER_PACK: i64 = 12;
const UPLOADS_A_DAY: i64 = 30;
const UPLOADS_AT_ONCE: i64 = 5;
const PACKS_PER_MEMBER: i64 = 100;
const PAGE: i64 = 24;

pub struct Market {
    pub media: PathBuf,         // what nginx may serve: packs/ (only through the app's X-Accel-Redirect) and pictures/
    pub work: PathBuf,          // uploads on their way in (private)
    pub wake: Notify,           // a new upload is queued
    busy: Mutex<HashSet<String>>, // uploads taking a piece right now
}

impl Market {
    pub fn new(media: PathBuf, work: PathBuf) -> Self {
        for d in [media.join("packs"), media.join("pictures"), work.clone()] {
            fs::create_dir_all(&d).expect("make the market's folders");
        }
        Market { media, work, wake: Notify::new(), busy: Mutex::new(HashSet::new()) }
    }

    fn part(&self, id: &str) -> PathBuf {
        self.work.join(format!("{id}.part"))
    }

    fn picture_file(&self, name: &str) -> PathBuf {
        self.media.join("pictures").join(&name[..2]).join(name)
    }
}

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS packs (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             slug TEXT NOT NULL UNIQUE,
             kind TEXT NOT NULL,                        -- resourcepack | shaderpack
             owner INTEGER NOT NULL REFERENCES users(uid),
             title TEXT NOT NULL,
             blurb TEXT NOT NULL DEFAULT '',
             about TEXT NOT NULL DEFAULT '',
             visibility TEXT NOT NULL DEFAULT 'public', -- public | private
             state TEXT NOT NULL DEFAULT 'draft',       -- draft | published | hidden | removed
             hidden_why TEXT,
             latest INTEGER,                            -- versions.id
             cover INTEGER,                             -- pictures.id
             icon TEXT,                                 -- pictures/…png from pack.png
             users INTEGER NOT NULL DEFAULT 0,          -- members who took it
             search TEXT NOT NULL DEFAULT '',
             created INTEGER NOT NULL,
             updated INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS packs_owner ON packs(owner);
         CREATE INDEX IF NOT EXISTS packs_listed ON packs(state, visibility, kind);
         CREATE TABLE IF NOT EXISTS versions (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             pack INTEGER NOT NULL REFERENCES packs(id) ON DELETE CASCADE,
             number TEXT NOT NULL,
             changelog TEXT NOT NULL DEFAULT '',
             file TEXT NOT NULL,                        -- SHA-256 of the clean archive
             size INTEGER NOT NULL,
             facts TEXT NOT NULL,
             created INTEGER NOT NULL,
             UNIQUE (pack, number));
         CREATE TABLE IF NOT EXISTS pictures (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             pack INTEGER NOT NULL REFERENCES packs(id) ON DELETE CASCADE,
             file TEXT NOT NULL,                        -- <sha256>.jpg; the small one is <sha256>-s.jpg
             w INTEGER NOT NULL, h INTEGER NOT NULL,
             caption TEXT NOT NULL DEFAULT '',
             place INTEGER NOT NULL,
             created INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS uploads (
             id TEXT PRIMARY KEY,
             owner INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             name TEXT NOT NULL,
             size INTEGER NOT NULL,
             sha256 TEXT,
             received INTEGER NOT NULL DEFAULT 0,
             state TEXT NOT NULL,                       -- receiving | queued | checking | ready | refused | used
             result TEXT,
             file TEXT,
             created INTEGER NOT NULL,
             touched INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS uploads_state ON uploads(state, touched);
         CREATE TABLE IF NOT EXISTS downloads (
             pack INTEGER NOT NULL REFERENCES packs(id) ON DELETE CASCADE,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             version INTEGER NOT NULL,
             at INTEGER NOT NULL,
             PRIMARY KEY (pack, uid));",
    )
    .expect("make the market's tables");
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(64 * 1024);
    Router::new()
        .route("/api/uploads", post(start).layer(json()))
        .route("/api/uploads/{id}", get(status).put(piece).delete(cancel).layer(DefaultBodyLimit::max(CHUNK + 1024)))
        .route("/api/packs", post(create).layer(json()))
        .route("/api/packs/{key}", get(page).patch(edit).delete(remove).layer(json()))
        .route("/api/packs/{key}/versions", post(add_version).layer(json()))
        .route("/api/packs/{key}/pictures", post(add_picture).patch(arrange).layer(DefaultBodyLimit::max(PICTURE_MAX + 1024)))
        .route("/api/packs/{key}/pictures/{pid}", axum::routing::delete(drop_picture))
        .route("/api/packs/{key}/download", get(download))
        .route("/api/packs/{key}/hide", post(hide).layer(json()))
        .route("/api/packs/{key}/unhide", post(unhide))
        .route("/api/market", get(list))
        .route("/api/me/packs", get(mine))
}

// ------------------------------------------------------------------------------------------------ helpers

pub(crate) type Answer = Result<Response, Response>;

/// Whoever is signed in (the site's cookie or the client's token); for the doors that change things, only from our
/// own site or from the client.
pub(crate) async fn member(app: &Shared, h: &HeaderMap, changes: bool) -> Result<Who, Response> {
    if changes && !same_site(app, h) {
        return Err(fail(StatusCode::FORBIDDEN, "origin"));
    }
    let Some((token, _)) = presented(h) else {
        return Err(fail(StatusCode::UNAUTHORIZED, "signed-out"));
    };
    let w = db(app, move |c| who(c, &token)).await.ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
    crate::forum::seen(app, &w);
    Ok(w)
}

pub(crate) fn said(status: StatusCode, error: &str, message: &str) -> Response {
    (status, Json(json!({ "error": error, "message": message }))).into_response()
}

pub(crate) fn busy() -> Response {
    fail(StatusCode::SERVICE_UNAVAILABLE, "busy")
}

pub(crate) fn clean_line(s: &str, most: usize) -> String {
    s.chars().filter(|c| !c.is_control()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(most).collect()
}

fn clean_text(s: &str, most: usize) -> String {
    let t: String = s.replace("\r\n", "\n").chars().filter(|c| *c == '\n' || !c.is_control()).take(most).collect();
    t.trim().to_string()
}

fn version_ok(v: &str) -> bool {
    (1..=16).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+' | b'_'))
}

/// A web address for a title, Russian letters spelt in Latin: "Пепел 32x" → "pepel-32x".
fn slug_of(title: &str) -> String {
    let mut s = String::new();
    for c in title.to_lowercase().chars() {
        let t: &str = match c {
            'a'..='z' | '0'..='9' => {
                s.push(c);
                continue;
            }
            'а' => "a", 'б' => "b", 'в' => "v", 'г' | 'ґ' => "g", 'д' => "d", 'е' | 'ё' | 'э' => "e", 'є' => "ye", 'ж' => "zh", 'з' => "z",
            'и' | 'і' => "i", 'ї' => "yi", 'й' | 'ы' => "y", 'к' => "k", 'л' => "l", 'м' => "m", 'н' => "n", 'о' => "o", 'п' => "p", 'р' => "r",
            'с' => "s", 'т' => "t", 'у' => "u", 'ф' => "f", 'х' => "h", 'ц' => "ts", 'ч' => "ch", 'ш' => "sh", 'щ' => "sch", 'ю' => "yu", 'я' => "ya",
            'ъ' | 'ь' | '\'' | '’' => "",
            _ => "-",
        };
        s.push_str(t);
    }
    let mut out = String::new();
    for part in s.split('-').filter(|p| !p.is_empty()) {
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(part);
    }
    let out: String = out.chars().take(40).collect::<String>().trim_end_matches('-').to_string();
    if out.is_empty() || out.bytes().all(|b| b.is_ascii_digit()) { format!("pack-{out}").trim_end_matches('-').to_string() } else { out }
}

fn picture_url(name: &str) -> String {
    format!("/media/{}/{}", &name[..2], name)
}

/// A pack by its number or its slug.
fn find(c: &Connection, key: &str) -> Option<(i64, i64, String, String, String)> {
    let q = "SELECT id, owner, state, visibility, kind FROM packs WHERE ";
    let row = |r: &Row| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?));
    match key.parse::<i64>() {
        Ok(id) => c.query_row(&format!("{q}id = ?1"), params![id], row).optional().ok().flatten(),
        Err(_) => c.query_row(&format!("{q}slug = ?1"), params![key], row).optional().ok().flatten(),
    }
}

/// Who may see a pack: its owner and her always; others when it is published, public, not hidden or removed.
fn can_see(w: &Who, owner: i64, state: &str, visibility: &str) -> bool {
    state != "removed" && (w.uid == owner || crate::people::can(w.uid, crate::people::MARKET_MODERATE) || (state == "published" && visibility == "public"))
}

// --------------------------------------------------------------------------------------------- cards and pages

const CARD: &str = "SELECT p.id, p.slug, p.kind, p.title, p.blurb, p.visibility, p.state, p.created, p.updated, p.users, p.icon,
    u.uid, u.login, v.number, v.size, v.facts, v.created, pic.file, pic.w, pic.h, p.hidden_why
    FROM packs p JOIN users u ON u.uid = p.owner LEFT JOIN versions v ON v.id = p.latest LEFT JOIN pictures pic ON pic.id = p.cover";

/// What a card shows: the pack, its author, its latest version, its cover, and what was read from it.
fn card(r: &Row) -> rusqlite::Result<Value> {
    let facts: Value = r.get::<_, Option<String>>(15)?.and_then(|f| serde_json::from_str(&f).ok()).unwrap_or(Value::Null);
    let kind: String = r.get(2)?;
    let tags = if kind == "shaderpack" {
        json!({ "features": facts["features"], "dimensions": facts["dimensions"], "options": facts["options"], "profiles": facts["profiles"] })
    } else {
        json!({ "resolution": facts["resolution"], "changes": facts["changes"], "game": facts["game"] })
    };
    let cover = r.get::<_, Option<String>>(17)?.map(|f| {
        let small = f.replacen(".jpg", "-s.jpg", 1);
        json!({ "big": picture_url(&f), "small": picture_url(&small), "w": r.get::<_, i64>(18).unwrap_or(0), "h": r.get::<_, i64>(19).unwrap_or(0) })
    });
    Ok(json!({
        "id": r.get::<_, i64>(0)?,
        "slug": r.get::<_, String>(1)?,
        "kind": kind,
        "title": r.get::<_, String>(3)?,
        "blurb": r.get::<_, String>(4)?,
        "visibility": r.get::<_, String>(5)?,
        "state": r.get::<_, String>(6)?,
        "created": r.get::<_, i64>(7)?,
        "updated": r.get::<_, i64>(8)?,
        "users": r.get::<_, i64>(9)?,
        "icon": r.get::<_, Option<String>>(10)?.map(|i| picture_url(&i)),
        "author": {
            "uid": r.get::<_, i64>(11)?, "login": r.get::<_, String>(12)?, "avatar": crate::avatar::of(r.get::<_, i64>(11)?),
            "uid_text": crate::people::card(r.get::<_, i64>(11)?, r.get::<_, String>(12)?)["uid_text"].clone(),
        },
        "version": r.get::<_, Option<String>>(13)?,
        "size": r.get::<_, Option<i64>>(14)?,
        "released": r.get::<_, Option<i64>>(16)?,
        "cover": cover,
        "tags": tags,
        "hidden_why": r.get::<_, Option<String>>(20)?,
    }))
}

/// A whole pack page: the card, its text, versions, pictures and everything read from its latest version (what was
/// left out and fixed only for its owner).
fn page_of(c: &Connection, id: i64, viewer: &Who) -> rusqlite::Result<Value> {
    let mut v = c.query_row(&format!("{CARD} WHERE p.id = ?1"), params![id], card)?;
    let (about_text, owner_uid): (String, i64) = c.query_row("SELECT about, owner FROM packs WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let mine = viewer.uid == owner_uid;
    let mut q = c.prepare("SELECT number, changelog, size, facts, created, file FROM versions WHERE pack = ?1 ORDER BY id DESC")?;
    let mut latest_facts = Value::Null;
    let versions: Vec<Value> = q
        .query_map(params![id], |r| {
            let facts: Value = serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or(Value::Null);
            // The clean archive is stored under its SHA-256: the client checks what it downloaded against it.
            Ok((json!({ "number": r.get::<_, String>(0)?, "changelog": r.get::<_, String>(1)?, "size": r.get::<_, i64>(2)?, "created": r.get::<_, i64>(4)?, "game": facts["game"], "sha256": r.get::<_, String>(5)? }), facts))
        })?
        .filter_map(Result::ok)
        .enumerate()
        .map(|(i, (v, f))| {
            if i == 0 {
                latest_facts = f;
            }
            v
        })
        .collect();
    if !mine {
        if let Some(o) = latest_facts.as_object_mut() {
            for k in ["dropped", "fixed", "uploaded"] {
                o.remove(k);
            }
        }
    }
    let mut q = c.prepare("SELECT id, file, w, h, caption FROM pictures WHERE pack = ?1 ORDER BY place, id")?;
    let pictures: Vec<Value> = q
        .query_map(params![id], |r| {
            let f: String = r.get(1)?;
            Ok(json!({ "id": r.get::<_, i64>(0)?, "big": picture_url(&f), "small": picture_url(&f.replacen(".jpg", "-s.jpg", 1)), "w": r.get::<_, i64>(2)?, "h": r.get::<_, i64>(3)?, "caption": r.get::<_, String>(4)? }))
        })?
        .filter_map(Result::ok)
        .collect();
    let o = v.as_object_mut().expect("a card is an object");
    o.insert("about".into(), json!(about_text));
    o.insert("versions".into(), json!(versions));
    o.insert("pictures".into(), json!(pictures));
    o.insert("facts".into(), latest_facts);
    o.insert("mine".into(), json!(mine));
    o.insert("moderator".into(), json!(crate::people::can(viewer.uid, crate::people::MARKET_MODERATE)));
    Ok(v)
}

// ------------------------------------------------------------------------------------------------ uploads

#[derive(Deserialize)]
struct Start {
    name: String,
    size: u64,
    #[serde(default)]
    sha256: Option<String>,
}

async fn start(State(app): State<Shared>, h: HeaderMap, Json(s): Json<Start>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        if s.size == 0 || s.size > inspect::MAX_ARCHIVE {
            return Err(said(StatusCode::PAYLOAD_TOO_LARGE, "size", &format!("a pack can be up to {} MB", inspect::MAX_ARCHIVE >> 20)));
        }
        let sha = s.sha256.map(|x| x.to_ascii_lowercase()).filter(|x| x.len() == 64 && x.bytes().all(|b| b.is_ascii_hexdigit()));
        let free = free_bytes(&app.market.media);
        if free < s.size * 3 + (2 << 30) {
            return Err(said(StatusCode::INSUFFICIENT_STORAGE, "full", "the server is out of room for uploads right now"));
        }
        let name = clean_line(&s.name, 200);
        let id = random_hex(12);
        let part = app.market.part(&id);
        File::create(&part).map_err(|_| busy())?;
        let (uid, size, idc) = (w.uid, s.size as i64, id.clone());
        let made = db(&app, move |c| -> Result<(), &'static str> {
            let today: i64 = c.query_row("SELECT COUNT(*) FROM uploads WHERE owner = ?1 AND created > ?2", params![uid, now() - 86_400], |r| r.get(0)).map_err(|_| "busy")?;
            let open: i64 = c.query_row("SELECT COUNT(*) FROM uploads WHERE owner = ?1 AND state IN ('receiving', 'queued', 'checking')", params![uid], |r| r.get(0)).map_err(|_| "busy")?;
            if today >= UPLOADS_A_DAY {
                return Err("day");
            }
            if open >= UPLOADS_AT_ONCE {
                return Err("once");
            }
            let at = now();
            c.execute("INSERT INTO uploads (id, owner, name, size, sha256, state, created, touched) VALUES (?1, ?2, ?3, ?4, ?5, 'receiving', ?6, ?6)", params![idc, uid, name, size, sha, at])
                .map_err(|_| "busy")?;
            Ok(())
        })
        .await;
        if let Err(why) = made {
            let _ = fs::remove_file(&part);
            return Err(match why {
                "day" => said(StatusCode::TOO_MANY_REQUESTS, "day", &format!("up to {UPLOADS_A_DAY} uploads a day")),
                "once" => said(StatusCode::TOO_MANY_REQUESTS, "once", &format!("up to {UPLOADS_AT_ONCE} uploads at once")),
                _ => busy(),
            });
        }
        Ok(Json(json!({ "id": id, "chunk": CHUNK, "received": 0, "size": s.size, "state": "receiving" })).into_response())
    })
    .await
}

pub(crate) async fn run(f: impl std::future::Future<Output = Answer>) -> Response {
    match f.await {
        Ok(r) | Err(r) => r,
    }
}

fn upload_json(r: &Row) -> rusqlite::Result<Value> {
    let result: Value = r.get::<_, Option<String>>(4)?.and_then(|x| serde_json::from_str(&x).ok()).unwrap_or(Value::Null);
    Ok(json!({ "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "size": r.get::<_, i64>(2)?, "received": r.get::<_, i64>(3)?, "result": result, "state": r.get::<_, String>(5)? }))
}

async fn status(State(app): State<Shared>, h: HeaderMap, Key(id): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let found = db(&app, move |c| {
            c.query_row("SELECT id, name, size, received, result, state FROM uploads WHERE id = ?1 AND owner = ?2", params![id, uid], upload_json).optional().ok().flatten()
        })
        .await;
        found.map(|v| Json(v).into_response()).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))
    })
    .await
}

#[derive(Deserialize)]
struct At {
    at: u64,
}

/// One piece of an upload, at the place the server says it has reached (a piece sent twice is simply refused). Its
/// bytes are read only once the upload is known to be this member's and waiting for exactly this piece: nobody can
/// make the server hold 8 MB for a request it would refuse anyway.
async fn piece(State(app): State<Shared>, h: HeaderMap, Key(id): Key<String>, Query(at): Query<At>, body: Body) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        // One piece at a time per upload.
        if !app.market.busy.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone()) {
            return Err(fail(StatusCode::CONFLICT, "busy-piece"));
        }
        let out = receive(&app, &w, &id, at.at, body).await;
        app.market.busy.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        out
    })
    .await
}

async fn receive(app: &Shared, w: &Who, id: &str, at: u64, body: Body) -> Answer {
    let (uid, idc) = (w.uid, id.to_string());
    let row = db(app, move |c| {
        c.query_row("SELECT size, received, state, sha256 FROM uploads WHERE id = ?1 AND owner = ?2", params![idc, uid], |r| {
            Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?))
        })
        .optional()
        .ok()
        .flatten()
    })
    .await;
    let Some((size, received, state, sha)) = row else {
        return Err(fail(StatusCode::NOT_FOUND, "missing"));
    };
    if state != "receiving" {
        return Err(said(StatusCode::CONFLICT, "done", "this upload has all its pieces"));
    }
    if at != received {
        return Err((StatusCode::CONFLICT, Json(json!({ "error": "at", "received": received }))).into_response());
    }
    let most = (size - received).min(CHUNK as u64) as usize;
    let body = to_bytes(body, most).await.map_err(|_| said(StatusCode::PAYLOAD_TOO_LARGE, "piece", "a piece is up to 8 MB, and no more than was announced"))?;
    if body.is_empty() {
        return Err(said(StatusCode::BAD_REQUEST, "piece", "an empty piece"));
    }
    let part = app.market.part(id);
    let len = body.len() as u64;
    blocking(move || -> std::io::Result<()> {
        let mut f = OpenOptions::new().append(true).open(&part)?;
        if f.metadata()?.len() != at {
            return Err(std::io::Error::other("the file and the count disagree"));
        }
        f.write_all(&body)?;
        f.sync_data()
    })
    .await
    .map_err(|_| busy())?;
    let got = received + len;
    let full = got == size;
    let mut state = "receiving".to_string();
    let mut problem = None;
    if full {
        // All there: the announced hash (if any) must match, then it waits for its check.
        if let Some(want) = sha {
            let part = app.market.part(id);
            let have = blocking(move || file_hash(&part)).await.unwrap_or_default();
            if have != want {
                problem = Some("the file arrived damaged: upload it again");
            }
        }
        state = if problem.is_some() { "refused".into() } else { "queued".into() };
    }
    let (idc, st, result) = (id.to_string(), state.clone(), problem.map(|m| json!({ "message": m }).to_string()));
    db(app, move |c| c.execute("UPDATE uploads SET received = ?2, state = ?3, result = ?4, touched = ?5 WHERE id = ?1", params![idc, got as i64, st, result, now()]))
        .await
        .map_err(|_| busy())?;
    if full {
        if problem.is_some() {
            let _ = fs::remove_file(app.market.part(id));
        }
        app.market.wake.notify_one();
    }
    Ok(Json(json!({ "id": id, "received": got, "size": size, "state": state })).into_response())
}

async fn cancel(State(app): State<Shared>, h: HeaderMap, Key(id): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let (uid, idc) = (w.uid, id.clone());
        let gone = db(&app, move |c| c.execute("DELETE FROM uploads WHERE id = ?1 AND owner = ?2 AND state != 'checking'", params![idc, uid])).await.map_err(|_| busy())?;
        if gone == 0 {
            return Err(fail(StatusCode::CONFLICT, "checking"));
        }
        let _ = fs::remove_file(app.market.part(&id));
        Ok(Json(json!({})).into_response())
    })
    .await
}

fn file_hash(p: &Path) -> std::io::Result<String> {
    let mut f = File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn free_bytes(p: &Path) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_bytes()) else { return 0 };
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return 0;
    }
    s.f_bavail as u64 * s.f_frsize as u64
}

// ------------------------------------------------------------------------------------------------ the checker

/// Checks queued uploads one at a time, forever. An upload caught mid-check by a restart is checked again.
pub async fn checker(app: Shared) {
    db(&app, |c| c.execute("UPDATE uploads SET state = 'queued' WHERE state = 'checking'", [])).await.ok();
    loop {
        let next = db(&app, |c| {
            c.query_row(
                "UPDATE uploads SET state = 'checking', touched = ?1 WHERE id = (SELECT id FROM uploads WHERE state = 'queued' ORDER BY touched LIMIT 1) RETURNING id, name",
                params![now()],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()
            .ok()
            .flatten()
        })
        .await;
        let Some((id, name)) = next else {
            tokio::select! {
                _ = app.market.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(30)) => {}
            }
            continue;
        };
        let (media, part, work) = (app.market.media.clone(), app.market.part(&id), app.market.work.clone());
        let idc = id.clone();
        // Its own thread, at low priority: the doors stay quick while a big pack is taken apart.
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS, 0, 10);
            }
            let _ = tx.send(check_one(&idc, &name, &part, &work, &media));
            // A big pack borrows a few hundred MB for a moment: they go back to the system, not to the allocator's pool.
            unsafe {
                libc::malloc_trim(0);
            }
        });
        let (state, result, file) = rx.await.unwrap_or_else(|_| ("refused".into(), json!({ "message": "the check broke down: upload it again" }), None));
        let _ = fs::remove_file(app.market.part(&id));
        let idc = id.clone();
        db(&app, move |c| c.execute("UPDATE uploads SET state = ?2, result = ?3, file = ?4, touched = ?5 WHERE id = ?1", params![idc, state, result.to_string(), file, now()])).await.ok();
    }
}

/// One upload through inspect.rs; the clean copy into media/packs under its hash, pack.png as an icon.
fn check_one(id: &str, name: &str, part: &Path, work: &Path, media: &Path) -> (String, Value, Option<String>) {
    let refused = |m: String, p: Option<String>| ("refused".to_string(), json!({ "message": m, "path": p }), None);
    let Ok(src) = File::open(part) else {
        return refused("the upload is gone: upload it again".into(), None);
    };
    let len = src.metadata().map(|m| m.len()).unwrap_or(0);
    let out_path = work.join(format!("{id}.clean"));
    let Ok(out) = File::create(&out_path) else {
        return refused("the server couldn't make room for the check".into(), None);
    };
    let checked = inspect::check(&src, len, name, out);
    let c = match checked {
        Ok(c) => c,
        Err(r) => {
            let _ = fs::remove_file(&out_path);
            return refused(r.message, r.path);
        }
    };
    let Ok(hash) = file_hash(&out_path) else {
        return refused("the clean copy couldn't be read back".into(), None);
    };
    let dest = media.join("packs").join(&hash[..2]).join(format!("{hash}.zip"));
    let stored = fs::create_dir_all(dest.parent().expect("a folder")).and_then(|_| if dest.exists() { fs::remove_file(&out_path) } else { fs::rename(&out_path, &dest) });
    if stored.is_err() {
        let _ = fs::remove_file(&out_path);
        return refused("the clean copy couldn't be stored".into(), None);
    }
    let icon = c.icon.as_deref().and_then(picture::icon).and_then(|png| {
        let h: String = Sha256::digest(&png).iter().map(|b| format!("{b:02x}")).collect();
        let file = format!("{h}.png");
        let p = media.join("pictures").join(&file[..2]).join(&file);
        fs::create_dir_all(p.parent()?).ok()?;
        fs::write(&p, &png).ok()?;
        Some(file)
    });
    let result = json!({ "kind": c.kind.word(), "title": c.title, "size": c.size, "icon": icon, "facts": c.facts });
    ("ready".to_string(), result, Some(hash))
}

// ------------------------------------------------------------------------------------------------ packs

#[derive(Deserialize)]
struct Create {
    upload: String,
    title: String,
    #[serde(default)]
    blurb: String,
    #[serde(default)]
    about: String,
    #[serde(default = "public")]
    visibility: String,
    number: String,
    #[serde(default)]
    changelog: String,
    #[serde(default)]
    publish: bool,
}

fn public() -> String {
    "public".into()
}

/// A ready upload as it waits for a pack: its kind, its clean copy, its size, what was read, its icon.
fn ready(c: &Connection, id: &str, uid: i64) -> Option<(String, String, i64, Value, Option<String>)> {
    let (file, result): (String, String) = c
        .query_row("SELECT file, result FROM uploads WHERE id = ?1 AND owner = ?2 AND state = 'ready'", params![id, uid], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .ok()
        .flatten()?;
    let v: Value = serde_json::from_str(&result).ok()?;
    Some((v["kind"].as_str()?.to_string(), file, v["size"].as_i64()?, v["facts"].clone(), v["icon"].as_str().map(str::to_string)))
}

async fn create(State(app): State<Shared>, h: HeaderMap, Json(p): Json<Create>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let title = clean_line(&p.title, 48);
        if title.chars().count() < 2 {
            return Err(said(StatusCode::BAD_REQUEST, "title", "a name of 2 to 48 characters"));
        }
        let number = p.number.trim().to_string();
        if !version_ok(&number) {
            return Err(said(StatusCode::BAD_REQUEST, "number", "a version: up to 16 of 0–9, a–z, . - + _"));
        }
        if p.visibility != "public" && p.visibility != "private" {
            return Err(said(StatusCode::BAD_REQUEST, "visibility", "public or private"));
        }
        let (blurb, about, changelog) = (clean_line(&p.blurb, 120), clean_text(&p.about, 4000), clean_text(&p.changelog, 2000));
        let (uid, login, upload, visibility, publish) = (w.uid, w.login.clone(), p.upload, p.visibility, p.publish);
        let made = db(&app, move |c| -> Result<i64, &'static str> {
            let count: i64 = c.query_row("SELECT COUNT(*) FROM packs WHERE owner = ?1 AND state != 'removed'", params![uid], |r| r.get(0)).map_err(|_| "busy")?;
            if count >= PACKS_PER_MEMBER {
                return Err("many");
            }
            let (kind, file, size, facts, icon) = ready(c, &upload, uid).ok_or("upload")?;
            let base = slug_of(&title);
            let mut slug = base.clone();
            let mut n = 2;
            while c.query_row("SELECT 1 FROM packs WHERE slug = ?1", params![slug], |_| Ok(())).optional().map_err(|_| "busy")?.is_some() {
                slug = format!("{base}-{n}");
                n += 1;
            }
            let at = now();
            let tx = c.unchecked_transaction().map_err(|_| "busy")?;
            let search = format!("{} {} {}", title, blurb, login).to_lowercase();
            tx.execute(
                "INSERT INTO packs (slug, kind, owner, title, blurb, about, visibility, state, icon, search, created, updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
                params![slug, kind, uid, title, blurb, about, visibility, if publish { "published" } else { "draft" }, icon, search, at],
            )
            .map_err(|_| "busy")?;
            let pack = tx.last_insert_rowid();
            tx.execute("INSERT INTO versions (pack, number, changelog, file, size, facts, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![pack, number, changelog, file, size, facts.to_string(), at])
                .map_err(|_| "busy")?;
            let version = tx.last_insert_rowid();
            tx.execute("UPDATE packs SET latest = ?2 WHERE id = ?1", params![pack, version]).map_err(|_| "busy")?;
            tx.execute("UPDATE uploads SET state = 'used', touched = ?2 WHERE id = ?1", params![upload, at]).map_err(|_| "busy")?;
            tx.commit().map_err(|_| "busy")?;
            Ok(pack)
        })
        .await;
        let id = match made {
            Ok(id) => id,
            Err("many") => return Err(said(StatusCode::TOO_MANY_REQUESTS, "many", &format!("up to {PACKS_PER_MEMBER} packs"))),
            Err("upload") => return Err(said(StatusCode::CONFLICT, "upload", "that upload isn't ready (or isn't yours)")),
            Err(_) => return Err(busy()),
        };
        let viewer = w;
        let page = db(&app, move |c| page_of(c, id, &viewer)).await.map_err(|_| busy())?;
        Ok((StatusCode::CREATED, Json(page)).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct NewVersion {
    upload: String,
    number: String,
    #[serde(default)]
    changelog: String,
}

async fn add_version(State(app): State<Shared>, h: HeaderMap, Key(key): Key<String>, Json(p): Json<NewVersion>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let number = p.number.trim().to_string();
        if !version_ok(&number) {
            return Err(said(StatusCode::BAD_REQUEST, "number", "a version: up to 16 of 0–9, a–z, . - + _"));
        }
        let changelog = clean_text(&p.changelog, 2000);
        let uid = w.uid;
        let done = db(&app, move |c| -> Result<i64, &'static str> {
            let (id, owner, state, _, kind) = find(c, &key).ok_or("missing")?;
            if owner != uid || state == "removed" {
                return Err("missing");
            }
            let (ukind, file, size, facts, icon) = ready(c, &p.upload, uid).ok_or("upload")?;
            if ukind != kind {
                return Err("kind");
            }
            let at = now();
            let tx = c.unchecked_transaction().map_err(|_| "busy")?;
            let added = tx.execute("INSERT INTO versions (pack, number, changelog, file, size, facts, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![id, number, changelog, file, size, facts.to_string(), at]);
            if added.is_err() {
                return Err("number");
            }
            let version = tx.last_insert_rowid();
            tx.execute("UPDATE packs SET latest = ?2, updated = ?3, icon = COALESCE(?4, icon) WHERE id = ?1", params![id, version, at, icon]).map_err(|_| "busy")?;
            tx.execute("UPDATE uploads SET state = 'used', touched = ?2 WHERE id = ?1", params![p.upload, at]).map_err(|_| "busy")?;
            tx.commit().map_err(|_| "busy")?;
            Ok(id)
        })
        .await;
        let id = match done {
            Ok(id) => id,
            Err("missing") => return Err(fail(StatusCode::NOT_FOUND, "missing")),
            Err("upload") => return Err(said(StatusCode::CONFLICT, "upload", "that upload isn't ready (or isn't yours)")),
            Err("kind") => return Err(said(StatusCode::CONFLICT, "kind", "a new version has to be the same kind of pack")),
            Err("number") => return Err(said(StatusCode::CONFLICT, "number", "that version number is taken")),
            Err(_) => return Err(busy()),
        };
        let page = db(&app, move |c| page_of(c, id, &w)).await.map_err(|_| busy())?;
        Ok(Json(page).into_response())
    })
    .await
}

async fn page(State(app): State<Shared>, h: HeaderMap, Key(key): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let found = db(&app, move |c| {
            let (id, owner, state, visibility, _) = find(c, &key)?;
            if !can_see(&w, owner, &state, &visibility) {
                return None;
            }
            page_of(c, id, &w).ok()
        })
        .await;
        found.map(|p| Json(p).into_response()).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))
    })
    .await
}

#[derive(Deserialize)]
struct Edit {
    title: Option<String>,
    blurb: Option<String>,
    about: Option<String>,
    visibility: Option<String>,
    state: Option<String>,
}

async fn edit(State(app): State<Shared>, h: HeaderMap, Key(key): Key<String>, Json(e): Json<Edit>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let done = db(&app, move |c| -> Result<i64, &'static str> {
            let (id, owner, state, _, _) = find(c, &key).ok_or("missing")?;
            if owner != uid || state == "removed" {
                return Err("missing");
            }
            let at = now();
            if let Some(t) = &e.title {
                let t = clean_line(t, 48);
                if t.chars().count() < 2 {
                    return Err("title");
                }
                c.execute("UPDATE packs SET title = ?2 WHERE id = ?1", params![id, t]).map_err(|_| "busy")?;
            }
            if let Some(b) = &e.blurb {
                c.execute("UPDATE packs SET blurb = ?2 WHERE id = ?1", params![id, clean_line(b, 120)]).map_err(|_| "busy")?;
            }
            if let Some(a) = &e.about {
                c.execute("UPDATE packs SET about = ?2 WHERE id = ?1", params![id, clean_text(a, 4000)]).map_err(|_| "busy")?;
            }
            if let Some(v) = &e.visibility {
                if v != "public" && v != "private" {
                    return Err("visibility");
                }
                c.execute("UPDATE packs SET visibility = ?2 WHERE id = ?1", params![id, v]).map_err(|_| "busy")?;
            }
            if let Some(s) = &e.state {
                if s != "published" && s != "draft" {
                    return Err("state");
                }
                if state == "hidden" {
                    return Err("hidden");
                }
                c.execute("UPDATE packs SET state = ?2 WHERE id = ?1", params![id, s]).map_err(|_| "busy")?;
            }
            // The words search looks in, lowered here: SQLite's lower() knows only Latin letters ("Пепел" stayed "Пепел").
            let words: String = c
                .query_row("SELECT p.title || ' ' || p.blurb || ' ' || u.login FROM packs p JOIN users u ON u.uid = p.owner WHERE p.id = ?1", params![id], |r| r.get(0))
                .map_err(|_| "busy")?;
            c.execute("UPDATE packs SET updated = ?2, search = ?3 WHERE id = ?1", params![id, at, words.to_lowercase()]).map_err(|_| "busy")?;
            Ok(id)
        })
        .await;
        let id = match done {
            Ok(id) => id,
            Err("missing") => return Err(fail(StatusCode::NOT_FOUND, "missing")),
            Err("hidden") => return Err(said(StatusCode::FORBIDDEN, "hidden", "the pack was hidden by moderation")),
            Err(field @ ("title" | "visibility" | "state")) => return Err(fail(StatusCode::BAD_REQUEST, field)),
            Err(_) => return Err(busy()),
        };
        let page = db(&app, move |c| page_of(c, id, &w)).await.map_err(|_| busy())?;
        Ok(Json(page).into_response())
    })
    .await
}

async fn remove(State(app): State<Shared>, h: HeaderMap, Key(key): Key<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let done = db(&app, move |c| -> Option<()> {
            let (id, owner, state, _, _) = find(c, &key)?;
            if (owner != uid && !crate::is_owner(uid)) || state == "removed" {
                return None;
            }
            c.execute("UPDATE packs SET state = 'removed', updated = ?2 WHERE id = ?1", params![id, now()]).ok()?;
            Some(())
        })
        .await;
        done.map(|_| Json(json!({})).into_response()).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))
    })
    .await
}

#[derive(Deserialize)]
struct Why {
    #[serde(default)]
    why: String,
}

/// Moderation: a pack out of the market (its owner still sees it, with the reason) and back.
async fn hide(State(app): State<Shared>, h: HeaderMap, Key(key): Key<String>, Json(y): Json<Why>) -> Response {
    moderate(app, h, key, Some(clean_line(&y.why, 200))).await
}

async fn unhide(State(app): State<Shared>, h: HeaderMap, Key(key): Key<String>) -> Response {
    moderate(app, h, key, None).await
}

async fn moderate(app: Shared, h: HeaderMap, key: String, why: Option<String>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        if !crate::people::can(w.uid, crate::people::MARKET_MODERATE) {
            return Err(fail(StatusCode::FORBIDDEN, "moderation"));
        }
        let done = db(&app, move |c| -> Option<()> {
            let (id, _, state, _, _) = find(c, &key)?;
            match &why {
                Some(y) if state != "removed" => c.execute("UPDATE packs SET state = 'hidden', hidden_why = ?2 WHERE id = ?1", params![id, y]).ok()?,
                None if state == "hidden" => c.execute("UPDATE packs SET state = 'published', hidden_why = NULL WHERE id = ?1", params![id]).ok()?,
                _ => return None,
            };
            Some(())
        })
        .await;
        done.map(|_| Json(json!({})).into_response()).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))
    })
    .await
}

// ------------------------------------------------------------------------------------------------ pictures

#[derive(Deserialize)]
struct Caption {
    #[serde(default)]
    caption: String,
}

async fn add_picture(State(app): State<Shared>, h: HeaderMap, Key(key): Key<String>, Query(q): Query<Caption>, body: Body) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let pack = db(&app, move |c| -> Result<i64, &'static str> {
            let (id, owner, state, _, _) = find(c, &key).ok_or("missing")?;
            if owner != uid || state == "removed" {
                return Err("missing");
            }
            let n: i64 = c.query_row("SELECT COUNT(*) FROM pictures WHERE pack = ?1", params![id], |r| r.get(0)).map_err(|_| "busy")?;
            if n >= PICTURES_PER_PACK {
                return Err("many");
            }
            Ok(id)
        })
        .await;
        let pack = match pack {
            Ok(p) => p,
            Err("many") => return Err(said(StatusCode::TOO_MANY_REQUESTS, "many", &format!("up to {PICTURES_PER_PACK} pictures"))),
            Err("missing") => return Err(fail(StatusCode::NOT_FOUND, "missing")),
            Err(_) => return Err(busy()),
        };
        // Read only now: the pack is this member's and has room for another picture.
        let body = to_bytes(body, PICTURE_MAX).await.map_err(|_| said(StatusCode::PAYLOAD_TOO_LARGE, "size", "a picture is up to 12 MB"))?;
        let made = blocking(move || picture::make(&body)).await.map_err(|m| said(StatusCode::UNPROCESSABLE_ENTITY, "picture", &m))?;
        let hash: String = Sha256::digest(&made.big).iter().map(|b| format!("{b:02x}")).collect();
        let (big, small) = (format!("{hash}.jpg"), format!("{hash}-s.jpg"));
        let (pb, ps) = (app.market.picture_file(&big), app.market.picture_file(&small));
        let (bb, sb) = (made.big, made.small);
        blocking(move || -> std::io::Result<()> {
            fs::create_dir_all(pb.parent().expect("a folder"))?;
            fs::write(&pb, &bb)?;
            fs::write(&ps, &sb)
        })
        .await
        .map_err(|_| busy())?;
        let (caption, (pw, ph)) = (clean_line(&q.caption, 120), (made.w as i64, made.h as i64));
        let id = db(&app, move |c| -> rusqlite::Result<i64> {
            let place: i64 = c.query_row("SELECT COALESCE(MAX(place), 0) + 1 FROM pictures WHERE pack = ?1", params![pack], |r| r.get(0))?;
            c.execute("INSERT INTO pictures (pack, file, w, h, caption, place, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![pack, big, pw, ph, caption, place, now()])?;
            let id = c.last_insert_rowid();
            c.execute("UPDATE packs SET cover = COALESCE(cover, ?2) WHERE id = ?1", params![pack, id])?;
            Ok(id)
        })
        .await
        .map_err(|_| busy())?;
        let f = format!("{hash}.jpg");
        Ok((StatusCode::CREATED, Json(json!({ "id": id, "big": picture_url(&f), "small": picture_url(&format!("{hash}-s.jpg")), "w": pw, "h": ph }))).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Arrange {
    #[serde(default)]
    order: Vec<i64>,
    cover: Option<i64>,
    #[serde(default)]
    captions: HashMap<String, String>,
}

async fn arrange(State(app): State<Shared>, h: HeaderMap, Key(key): Key<String>, Json(a): Json<Arrange>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let done = db(&app, move |c| -> Option<i64> {
            let (id, owner, state, _, _) = find(c, &key)?;
            if owner != uid || state == "removed" {
                return None;
            }
            for (place, pid) in a.order.iter().enumerate() {
                c.execute("UPDATE pictures SET place = ?3 WHERE id = ?1 AND pack = ?2", params![pid, id, place as i64 + 1]).ok()?;
            }
            if let Some(cover) = a.cover {
                c.execute("UPDATE packs SET cover = ?2 WHERE id = ?1 AND EXISTS (SELECT 1 FROM pictures WHERE id = ?2 AND pack = ?1)", params![id, cover]).ok()?;
            }
            for (pid, text) in &a.captions {
                let pid: i64 = pid.parse().ok()?;
                c.execute("UPDATE pictures SET caption = ?3 WHERE id = ?1 AND pack = ?2", params![pid, id, clean_line(text, 120)]).ok()?;
            }
            Some(id)
        })
        .await;
        let Some(id) = done else {
            return Err(fail(StatusCode::NOT_FOUND, "missing"));
        };
        let page = db(&app, move |c| page_of(c, id, &w)).await.map_err(|_| busy())?;
        Ok(Json(page).into_response())
    })
    .await
}

async fn drop_picture(State(app): State<Shared>, h: HeaderMap, Key((key, pid)): Key<(String, i64)>) -> Response {
    run(async {
        let w = member(&app, &h, true).await?;
        let uid = w.uid;
        let done = db(&app, move |c| -> Option<()> {
            let (id, owner, _, _, _) = find(c, &key)?;
            if owner != uid {
                return None;
            }
            if c.execute("DELETE FROM pictures WHERE id = ?1 AND pack = ?2", params![pid, id]).ok()? == 0 {
                return None;
            }
            c.execute("UPDATE packs SET cover = (SELECT id FROM pictures WHERE pack = ?1 ORDER BY place, id LIMIT 1) WHERE id = ?1 AND (cover = ?2 OR cover IS NULL)", params![id, pid]).ok()?;
            Some(())
        })
        .await;
        done.map(|_| Json(json!({})).into_response()).ok_or_else(|| fail(StatusCode::NOT_FOUND, "missing"))
    })
    .await
}

// ------------------------------------------------------------------------------------------------ taking a pack

#[derive(Deserialize)]
struct Which {
    v: Option<String>,
}

/// The clean archive of a version (the latest by default), through nginx. Counts the member among its users.
async fn download(State(app): State<Shared>, h: HeaderMap, Key(key): Key<String>, Query(q): Query<Which>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let found = db(&app, move |c| -> Option<(String, String, String)> {
            let (id, owner, state, visibility, _) = find(c, &key)?;
            if !can_see(&w, owner, &state, &visibility) {
                return None;
            }
            let (vid, number, file): (i64, String, String) = match &q.v {
                Some(n) => c.query_row("SELECT id, number, file FROM versions WHERE pack = ?1 AND number = ?2", params![id, n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))),
                None => c.query_row("SELECT v.id, v.number, v.file FROM versions v JOIN packs p ON p.latest = v.id WHERE p.id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))),
            }
            .ok()?;
            let slug: String = c.query_row("SELECT slug FROM packs WHERE id = ?1", params![id], |r| r.get(0)).ok()?;
            if w.uid != owner && c.execute("INSERT OR IGNORE INTO downloads (pack, uid, version, at) VALUES (?1, ?2, ?3, ?4)", params![id, w.uid, vid, now()]).ok()? == 1 {
                c.execute("UPDATE packs SET users = users + 1 WHERE id = ?1", params![id]).ok()?;
            }
            Some((slug, number, file))
        })
        .await;
        let Some((slug, number, file)) = found else {
            return Err(fail(StatusCode::NOT_FOUND, "missing"));
        };
        let mut res = StatusCode::OK.into_response();
        let hd = res.headers_mut();
        hd.insert("X-Accel-Redirect", HeaderValue::from_str(&format!("/_packs/{}/{}.zip", &file[..2], file)).map_err(|_| busy())?);
        hd.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
        hd.insert(header::CONTENT_DISPOSITION, HeaderValue::from_str(&format!("attachment; filename=\"{slug}-{number}.zip\"")).map_err(|_| busy())?);
        Ok(res)
    })
    .await
}

// ------------------------------------------------------------------------------------------------ the shop window

#[derive(Deserialize)]
struct Filter {
    kind: Option<String>,
    q: Option<String>,
    sort: Option<String>,
    page: Option<i64>,
    res: Option<String>,     // 16x | 32x | 64x | 128x+ (the resolution read from its block textures)
    fits: Option<String>,    // "1": only packs 1.21.11 takes as its own
    author: Option<String>,  // a login
    state: Option<String>,   // "hidden": the packs moderation took out (hers only)
}

/// The market: published public packs of a kind, of a resolution, for this game, by an author, matching words; in
/// order of how many took them this week (trending), of all time (users), newest, last updated, or by name. For her,
/// with state=hidden, the packs moderation took out.
async fn list(State(app): State<Shared>, h: HeaderMap, Query(f): Query<Filter>) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let page = f.page.unwrap_or(1).clamp(1, 10_000);
        let week = now() - 7 * 86_400;
        let order = match f.sort.as_deref() {
            Some("users") => "p.users DESC, p.id DESC".to_string(),
            Some("updated") => "p.updated DESC, p.id DESC".to_string(),
            Some("name") => "lower(p.title), p.id".to_string(),
            Some("new") => "p.created DESC, p.id DESC".to_string(),
            _ => format!("(SELECT COUNT(*) FROM downloads d WHERE d.pack = p.id AND d.at > {week}) DESC, p.users DESC, p.id DESC"),
        };
        let hidden = f.state.as_deref() == Some("hidden");
        if hidden && !crate::people::can(w.uid, crate::people::MARKET_MODERATE) {
            return Err(fail(StatusCode::FORBIDDEN, "moderation"));
        }
        let kind = f.kind.filter(|k| k == "resourcepack" || k == "shaderpack");
        let res = f.res.filter(|r| matches!(r.as_str(), "16x" | "32x" | "64x" | "128x+"));
        let fits = f.fits.as_deref() == Some("1");
        let author = f.author.map(|a| a.trim().to_ascii_lowercase()).filter(|a| !a.is_empty() && a.len() <= 16);
        let words = f.q.map(|q| clean_line(&q, 60).to_lowercase()).filter(|q| !q.is_empty());
        let found = db(&app, move |c| -> rusqlite::Result<(Vec<Value>, i64)> {
            let mut filter = if hidden { "p.state = 'hidden'".to_string() } else { "p.state = 'published' AND p.visibility = 'public'".to_string() };
            let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            if let Some(k) = kind {
                args.push(Box::new(k));
                filter += &format!(" AND p.kind = ?{}", args.len());
            }
            match res.as_deref() {
                Some("128x+") => filter += " AND CAST(rtrim(json_extract(v.facts, '$.resolution'), 'x') AS INTEGER) >= 128",
                Some(r) => {
                    args.push(Box::new(r.to_string()));
                    filter += &format!(" AND json_extract(v.facts, '$.resolution') = ?{}", args.len());
                }
                None => {}
            }
            if fits {
                filter += " AND json_extract(v.facts, '$.game.fits') = 1";
            }
            if let Some(a) = author {
                args.push(Box::new(a));
                filter += &format!(" AND u.login_key = ?{}", args.len());
            }
            if let Some(q) = words {
                for word in q.split(' ').take(6) {
                    let escaped = word.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
                    args.push(Box::new(format!("%{escaped}%")));
                    filter += &format!(" AND p.search LIKE ?{} ESCAPE '\\'", args.len());
                }
            }
            let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
            let from = "FROM packs p JOIN users u ON u.uid = p.owner LEFT JOIN versions v ON v.id = p.latest";
            let total: i64 = c.query_row(&format!("SELECT COUNT(*) {from} WHERE {filter}"), refs.as_slice(), |r| r.get(0))?;
            let sql = format!("{CARD} WHERE {filter} ORDER BY {order} LIMIT {PAGE} OFFSET {}", (page - 1) * PAGE);
            let mut q = c.prepare(&sql)?;
            let items = q.query_map(refs.as_slice(), card)?.filter_map(Result::ok).collect();
            Ok((items, total))
        })
        .await;
        let (items, total) = found.map_err(|_| busy())?;
        Ok(Json(json!({ "items": items, "total": total, "page": page, "per_page": PAGE })).into_response())
    })
    .await
}

/// A member's own packs, drafts and hidden ones too.
async fn mine(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        let w = member(&app, &h, false).await?;
        let uid = w.uid;
        let items = db(&app, move |c| -> rusqlite::Result<Vec<Value>> {
            let mut q = c.prepare(&format!("{CARD} WHERE p.owner = ?1 AND p.state != 'removed' ORDER BY p.updated DESC"))?;
            let items = q.query_map(params![uid], card)?.filter_map(Result::ok).collect();
            Ok(items)
        })
        .await
        .map_err(|_| busy())?;
        Ok(Json(json!({ "items": items })).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ the sweep

/// Once a day: uploads never finished (a day) or never used (two days) go, removed packs go for good after 30 days,
/// and files nothing points at any more leave the disk.
pub fn sweep(c: &Connection, market: &Market) {
    let at = now();
    let mut stale: Vec<String> = Vec::new();
    if let Ok(mut q) = c.prepare("SELECT id FROM uploads WHERE (state = 'receiving' AND touched < ?1) OR (state IN ('ready', 'refused', 'used') AND touched < ?2)") {
        if let Ok(rows) = q.query_map(params![at - 86_400, at - 2 * 86_400], |r| r.get::<_, String>(0)) {
            stale = rows.filter_map(Result::ok).collect();
        }
    }
    for id in &stale {
        let _ = fs::remove_file(market.part(id));
        let _ = c.execute("DELETE FROM uploads WHERE id = ?1", params![id]);
    }
    let _ = c.execute("DELETE FROM packs WHERE state = 'removed' AND updated < ?1", params![at - 30 * 86_400]);

    let used = |sql: &str| -> HashSet<String> {
        c.prepare(sql).and_then(|mut q| q.query_map([], |r| r.get::<_, Option<String>>(0)).map(|rows| rows.filter_map(Result::ok).flatten().collect())).unwrap_or_default()
    };
    let mut packs = used("SELECT file FROM versions");
    packs.extend(used("SELECT file FROM uploads WHERE file IS NOT NULL"));
    let mut pictures = used("SELECT file FROM pictures");
    pictures.extend(used("SELECT replace(file, '.jpg', '-s.jpg') FROM pictures"));
    pictures.extend(used("SELECT icon FROM packs"));
    pictures.extend(used("SELECT json_extract(result, '$.icon') FROM uploads WHERE result IS NOT NULL"));
    // Avatars live among the pictures too.
    pictures.extend(used("SELECT j.value FROM users u, json_each(json_extract(u.avatar, '$.files')) j WHERE u.avatar IS NOT NULL"));
    let old = |p: &Path| p.metadata().and_then(|m| m.modified()).map_or(false, |t| t.elapsed().map_or(false, |e| e > Duration::from_secs(86_400)));
    for (dir, keep, strip) in [(market.media.join("packs"), &packs, true), (market.media.join("pictures"), &pictures, false)] {
        let Ok(subs) = fs::read_dir(&dir) else { continue };
        for sub in subs.filter_map(Result::ok) {
            let Ok(files) = fs::read_dir(sub.path()) else { continue };
            for f in files.filter_map(Result::ok) {
                let name = f.file_name().to_string_lossy().into_owned();
                let key = if strip { name.trim_end_matches(".zip").to_string() } else { name.clone() };
                if !keep.contains(&key) && old(&f.path()) {
                    let _ = fs::remove_file(f.path());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug_of("Ashfall"), "ashfall");
        assert_eq!(slug_of("Пепел 32x — зимний"), "pepel-32x-zimniy");
        assert_eq!(slug_of("lantern pvp [16x]"), "lantern-pvp-16x");
        assert_eq!(slug_of("2024"), "pack-2024");
        assert_eq!(slug_of("!!!"), "pack");
        assert_eq!(slug_of("Щука ёжик"), "schuka-ezhik");
    }

    #[test]
    fn versions_and_text() {
        assert!(version_ok("1.4") && version_ok("2.0-beta+3") && !version_ok("") && !version_ok("1.0 final") && !version_ok(&"1".repeat(17)));
        assert_eq!(clean_line("  a\u{0}b \n c  ", 10), "ab c");
        assert_eq!(clean_text("line one\r\nline two\u{7}", 100), "line one\nline two");
    }
}
