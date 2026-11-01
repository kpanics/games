//! The site's wardrobe (23.10.2026): a member's look (a skin, what is worn from the cosmetics' pack) kept for their page,
//! and the skins of Minecraft names fetched for the site's figure: Mojang's first, else Ely.by's (many play on names
//! Mojang does not know). A skin fetched is kept beside the database a while (`skins/`), a name not found a shorter while.
//! Only members ask: the site is not anyone's skin proxy.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::market::member;
use crate::Shared;

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS looks (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             skin TEXT NOT NULL,
             worn TEXT NOT NULL,
             updated INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS look_likes (
             owner INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             fan INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             at INTEGER NOT NULL,
             PRIMARY KEY (owner, fan));
         CREATE INDEX IF NOT EXISTS look_likes_owner ON look_likes(owner);",
    )
    .expect("the looks' table");
}

fn dir(app: &Shared) -> PathBuf {
    std::path::Path::new(&app.db_path).parent().unwrap_or(std::path::Path::new(".")).join("skins")
}

/// A Minecraft name as it may be: 1 to 16 of a–z, 0–9, _ (small letters: the file's name).
fn name_of(s: &str) -> Option<String> {
    (!s.is_empty() && s.len() <= 16 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')).then(|| s.to_ascii_lowercase())
}

/// How long a skin is kept, and a name that has none; what is older than a day goes (a sweep at most once an hour).
const KEEP: u64 = 6 * 3600;
const KEEP_MISSING: u64 = 30 * 60;
const SWEEP_AFTER: u64 = 24 * 3600;
/// The fetches out to Mojang and Ely.by: at most this many a minute for one member, and for all of them together.
const FETCHES_A_MEMBER: usize = 12;
const FETCHES_A_MINUTE: usize = 60;
static FETCHED: Mutex<VecDeque<(Instant, i64)>> = Mutex::new(VecDeque::new());
static SWEPT: Mutex<Option<Instant>> = Mutex::new(None);

fn may_fetch(uid: i64) -> bool {
    let mut q = FETCHED.lock().unwrap_or_else(|e| e.into_inner());
    while q.front().is_some_and(|t| t.0.elapsed() > Duration::from_secs(60)) {
        q.pop_front();
    }
    if q.len() >= FETCHES_A_MINUTE || q.iter().filter(|t| t.1 == uid).count() >= FETCHES_A_MEMBER {
        return false;
    }
    q.push_back((Instant::now(), uid));
    true
}

/// The fetched skins older than a day let go (the members' own files, in `own/`, stay).
fn sweep(d: &std::path::Path) {
    {
        let mut s = SWEPT.lock().unwrap_or_else(|e| e.into_inner());
        if s.is_some_and(|t| t.elapsed() < Duration::from_secs(3600)) {
            return;
        }
        *s = Some(Instant::now());
    }
    let Ok(list) = std::fs::read_dir(d) else { return };
    for e in list.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let old = meta.modified().ok().and_then(|m| m.elapsed().ok()).is_some_and(|age| age.as_secs() > SWEEP_AFTER);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// A member's own skin file (never in the fetched names' place: a Minecraft name could be "u123").
fn own_file(app: &Shared, uid: i64) -> PathBuf {
    dir(app).join("own").join(format!("u{uid}.png"))
}

/// A picture of a skin (or a cape) as it may be: a PNG, 64 wide, 64 or 32 high (a cape: 64 × 32 or bigger, kept as it
/// is), not big.
fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || &b[..8] != b"\x89PNG\r\n\x1a\n" || &b[12..16] != b"IHDR" {
        return None;
    }
    Some((u32::from_be_bytes(b[16..20].try_into().ok()?), u32::from_be_bytes(b[20..24].try_into().ok()?)))
}
fn skin_ok(b: &[u8]) -> bool {
    b.len() <= 64 * 1024 && matches!(png_size(b), Some((64, 64)) | Some((64, 32)))
}

fn agent() -> ureq::Agent {
    // No redirects: every address is checked before it is asked (picture_url).
    ureq::AgentBuilder::new().timeout(Duration::from_secs(6)).redirects(0).build()
}

fn read_all(r: ureq::Response, most: u64) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(r.into_reader(), most), &mut bytes).ok()?;
    Some(bytes)
}

/// A picture's address as their answers give it, made safe: their own hosts only (Mojang's textures, Ely.by), over https.
fn picture_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    let host = host.to_ascii_lowercase();
    let ours = host == "textures.minecraft.net" || host == "ely.by" || host.ends_with(".ely.by");
    (ours && host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')).then(|| format!("https://{host}/{path}"))
}

/// What a name's textures are (Mojang's form, Ely.by's the same): the skin's address and whether it is slim, the cape's.
fn textures_of(v: &Value) -> Option<(String, bool, Option<String>)> {
    let skin = v.get("SKIN")?;
    let url = skin.get("url")?.as_str()?.to_string();
    let slim = skin.get("metadata").and_then(|m| m.get("model")).and_then(Value::as_str) == Some("slim");
    let cape = v.get("CAPE").and_then(|c| c.get("url")).and_then(Value::as_str).map(str::to_string);
    Some((url, slim, cape))
}

/// One of their answers as JSON: Ok(Some) found, Ok(None) they do not know the name (204, 404), Err anything else (a
/// limit, a timeout, an outage: tried again later, never taken for "no such name").
fn ask_json(a: &ureq::Agent, url: &str) -> Result<Option<Value>, ()> {
    match a.get(url).call() {
        Ok(r) if r.status() == 200 => r.into_json().map(Some).map_err(|_| ()),
        Ok(r) if r.status() == 204 => Ok(None),
        Ok(_) => Err(()),
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(_) => Err(()),
    }
}

/// A name's skin, from Mojang, else Ely.by: Ok(Some((skin, slim, cape, from))), Ok(None) when neither knows it, Err when
/// either could not be asked now.
#[allow(clippy::type_complexity)]
fn fetch(name: &str) -> Result<Option<(Vec<u8>, bool, Option<Vec<u8>>, &'static str)>, ()> {
    let a = agent();
    let get_png = |url: &str| -> Option<Vec<u8>> { read_all(a.get(&picture_url(url)?).call().ok()?, 256 * 1024) };
    let mojang = match ask_json(&a, &format!("https://api.mojang.com/users/profiles/minecraft/{name}"))? {
        Some(v) => {
            let id = v.get("id").and_then(Value::as_str).ok_or(())?.to_string();
            let p = ask_json(&a, &format!("https://sessionserver.mojang.com/session/minecraft/profile/{id}"))?.ok_or(())?;
            let b64 = p
                .get("properties")
                .and_then(Value::as_array)
                .and_then(|l| l.iter().find(|x| x.get("name").and_then(Value::as_str) == Some("textures")))
                .and_then(|x| x.get("value"))
                .and_then(Value::as_str)
                .ok_or(())?
                .to_string();
            let t: Value = base64(&b64).and_then(|b| serde_json::from_slice(&b).ok()).ok_or(())?;
            // A name with no skin of its own (Mojang's default): as good as none.
            t.get("textures").and_then(textures_of)
        }
        None => None,
    };
    let (found, from) = match mojang {
        Some(t) => (Some(t), "mojang"),
        None => (ask_json(&a, &format!("https://skinsystem.ely.by/textures/{name}"))?.as_ref().and_then(textures_of), "ely"),
    };
    let Some((url, slim, cape)) = found else { return Ok(None) };
    let skin = get_png(&url).filter(|b| skin_ok(b)).ok_or(())?;
    let cape = cape.and_then(|u| get_png(&u)).filter(|b| b.len() <= 256 * 1024 && png_size(b).is_some_and(|(w, h)| w >= 22 && h >= 17 && w <= 2048 && h <= 1024));
    Ok(Some((skin, slim, cape, from)))
}

/// Standard base64 (Mojang's textures property).
fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for b in s.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\n' | b'\r' => continue,
            _ => return None,
        } as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A name's skin as kept (fetched again when it is old), asked by member `uid`: its picture, slim, a cape, from where.
/// Err: "no-skin" (nobody knows the name), "slow" (not now).
async fn skin_for(app: &Shared, name: String, uid: i64) -> Result<(Vec<u8>, bool, bool, String), &'static str> {
    let d = dir(app);
    crate::blocking(move || {
        let meta = d.join(format!("{name}.json"));
        let kept: Option<Value> = std::fs::read(&meta).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let at = kept.as_ref().and_then(|v| v.get("at")).and_then(Value::as_u64).unwrap_or(0);
        let missing = kept.as_ref().and_then(|v| v.get("missing")).and_then(Value::as_bool).unwrap_or(false);
        let fresh = now().saturating_sub(at) < if missing { KEEP_MISSING } else { KEEP };
        let kept_png = || -> Option<(Vec<u8>, bool, bool, String)> {
            let v = kept.as_ref().filter(|v| !v["missing"].as_bool().unwrap_or(false))?;
            let png = std::fs::read(d.join(format!("{name}.png"))).ok()?;
            Some((png, v["slim"].as_bool().unwrap_or(false), v["cape"].as_bool().unwrap_or(false), v["from"].as_str().unwrap_or("").to_string()))
        };
        if fresh {
            if missing {
                return Err("no-skin");
            }
            if let Some(k) = kept_png() {
                return Ok(k);
            }
        }
        if !may_fetch(uid) {
            // Too many asked this minute: what is kept, however old, else later.
            return kept_png().ok_or("slow");
        }
        let _ = std::fs::create_dir_all(&d);
        sweep(&d);
        match fetch(&name) {
            Ok(Some((png, slim, cape, from))) => {
                let _ = std::fs::write(d.join(format!("{name}.png")), &png);
                match &cape {
                    Some(c) => {
                        let _ = std::fs::write(d.join(format!("{name}.cape.png")), c);
                    }
                    None => {
                        let _ = std::fs::remove_file(d.join(format!("{name}.cape.png")));
                    }
                }
                let _ = std::fs::write(&meta, json!({ "at": now(), "slim": slim, "cape": cape.is_some(), "from": from }).to_string());
                Ok((png, slim, cape.is_some(), from.to_string()))
            }
            Ok(None) => {
                let _ = std::fs::write(&meta, json!({ "at": now(), "missing": true }).to_string());
                Err("no-skin")
            }
            // They could not be asked now: what is kept, however old, else later; nothing written.
            Err(()) => kept_png().ok_or("slow"),
        }
    })
    .await
}

fn png(bytes: Vec<u8>) -> Response {
    let mut res = bytes.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=600"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    res
}

/// A name's skin: the picture, and in its headers whether it is slim, whether there is a cape, from where.
async fn get_skin(State(app): State<Shared>, h: HeaderMap, Path(name): Path<String>) -> Response {
    let who = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(no) => return no,
    };
    let Some(name) = name_of(&name) else { return crate::fail(StatusCode::BAD_REQUEST, "name") };
    match skin_for(&app, name, who.uid).await {
        Ok((bytes, slim, cape, from)) => {
            let mut res = png(bytes);
            let h = res.headers_mut();
            h.insert("x-skin-model", HeaderValue::from_static(if slim { "slim" } else { "wide" }));
            h.insert("x-skin-cape", HeaderValue::from_static(if cape { "1" } else { "0" }));
            if let Ok(v) = HeaderValue::from_str(&from) {
                h.insert("x-skin-from", v);
            }
            res
        }
        Err(e) => crate::fail(if e == "slow" { StatusCode::TOO_MANY_REQUESTS } else { StatusCode::NOT_FOUND }, e),
    }
}

/// A name's cape (Mojang's own, or Ely.by's), when the name has one: asked after its skin.
async fn get_cape(State(app): State<Shared>, h: HeaderMap, Path(name): Path<String>) -> Response {
    if let Err(no) = member(&app, &h, false).await {
        return no;
    }
    let Some(name) = name_of(&name) else { return crate::fail(StatusCode::BAD_REQUEST, "name") };
    let d = dir(&app);
    match crate::blocking(move || std::fs::read(d.join(format!("{name}.cape.png")))).await {
        Ok(b) => png(b),
        Err(_) => crate::fail(StatusCode::NOT_FOUND, "no-cape"),
    }
}

/// A member by their login: uid and the login as written.
fn by_login(c: &Connection, login: &str) -> Option<(i64, String)> {
    c.query_row("SELECT uid, login FROM users WHERE login_key = ?1", params![login.to_lowercase()], |r| Ok((r.get(0)?, r.get(1)?))).optional().ok().flatten()
}

/// Where a member's look takes its skin from, and what it wears: the site's look, else the game's outfit (Cursed's, and
/// the name of the game's player they play as). The name is never shown to anyone but them: others get the picture
/// (`/api/look/<login>/skin`), so the site does not tell who plays as whom.
struct Found {
    uid: i64,
    login: String,
    /// "site", "game" or "" (nothing).
    from: &'static str,
    /// "" (none), "name:<a Minecraft name>", "file".
    skin: String,
    worn: Value,
    updated: i64,
    /// The game's outfit, when there is one (its player's name, what it wears).
    game: Option<(String, Value, i64)>,
}

fn found(c: &Connection, login: &str) -> Option<Found> {
    let (uid, login) = by_login(c, login)?;
    let site: Option<(String, String, i64)> = c.query_row("SELECT skin, worn, updated FROM looks WHERE uid = ?1", params![uid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional().ok().flatten();
    let game: Option<(String, Value, i64)> = c
        .query_row("SELECT mc_name, worn, updated FROM outfits WHERE uid = ?1", params![uid], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))
        .optional()
        .ok()
        .flatten()
        .map(|(n, w, u)| (n, serde_json::from_str(&w).unwrap_or(Value::Null), u / 1000));
    Some(match site {
        Some((skin, worn, updated)) => Found { uid, login, from: "site", skin, worn: serde_json::from_str(&worn).unwrap_or(Value::Null), updated, game },
        None => match &game {
            Some((name, worn, updated)) => Found { uid, login, from: "game", skin: format!("name:{name}"), worn: worn.clone(), updated: *updated, game: game.clone() },
            None => Found { uid, login, from: "", skin: String::new(), worn: Value::Null, updated: 0, game: None },
        },
    })
}

/// A member's look. To them: all of it (the skin's source, the game's outfit for "as in the game"); to others: what is
/// worn, and whether there is a skin to ask for ("shown").
async fn get_look(State(app): State<Shared>, h: HeaderMap, Path(login): Path<String>) -> Response {
    let who = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(no) => return no,
    };
    let login: String = login.chars().take(32).collect();
    let Some(f) = crate::db(&app, move |c| found(c, &login)).await else { return crate::fail(StatusCode::NOT_FOUND, "no-member") };
    let own = f.uid == who.uid;
    if !own && f.from != "site" {
        let mut res = Json(json!({ "login": f.login, "from": null, "skin": "", "worn": null, "updated": 0, "game": null, "own": false })).into_response();
        res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
        return res;
    }
    let from = if f.from.is_empty() { Value::Null } else { Value::from(f.from) };
    let (owner, fan) = (f.uid, who.uid);
    let (likes, liked) = crate::db(&app, move |c| likes_of(c, owner, fan)).await;
    let skin = if own || f.skin.is_empty() { f.skin.clone() } else { "shown".to_string() };
    let game = if own { f.game.as_ref().map(|(n, w, u)| json!({ "skin": format!("name:{n}"), "worn": w, "updated": u })) } else { None };
    let mut res = Json(json!({ "login": f.login, "from": from, "skin": skin, "worn": f.worn, "updated": f.updated, "game": game, "own": own, "likes": likes, "liked": liked })).into_response();
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    res
}

/// A member's skin as their look has it: their own file, or the picture of the Minecraft name it names (fetched and
/// kept here); its model in the headers when known.
async fn get_look_skin(State(app): State<Shared>, h: HeaderMap, Path(login): Path<String>) -> Response {
    let who = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(no) => return no,
    };
    let login: String = login.chars().take(32).collect();
    let Some(f) = crate::db(&app, move |c| found(c, &login)).await else { return crate::fail(StatusCode::NOT_FOUND, "no-member") };
    if f.uid != who.uid && f.from != "site" {
        return crate::fail(StatusCode::NOT_FOUND, "no-skin");
    }
    if f.skin == "file" {
        let path = own_file(&app, f.uid);
        return match crate::blocking(move || std::fs::read(path)).await {
            Ok(b) => png(b),
            Err(_) => crate::fail(StatusCode::NOT_FOUND, "no-skin"),
        };
    }
    let Some(name) = f.skin.strip_prefix("name:").and_then(name_of) else { return crate::fail(StatusCode::NOT_FOUND, "no-skin") };
    match skin_for(&app, name, who.uid).await {
        Ok((bytes, slim, cape, _)) => {
            let mut res = png(bytes);
            let h = res.headers_mut();
            h.insert("x-skin-model", HeaderValue::from_static(if slim { "slim" } else { "wide" }));
            h.insert("x-skin-cape", HeaderValue::from_static(if cape { "1" } else { "0" }));
            res
        }
        Err(e) => crate::fail(if e == "slow" { StatusCode::TOO_MANY_REQUESTS } else { StatusCode::NOT_FOUND }, e),
    }
}

/// The cape of a member's skin's name, when it has one (asked after the skin, which fetched it).
async fn get_look_cape(State(app): State<Shared>, h: HeaderMap, Path(login): Path<String>) -> Response {
    let who = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(no) => return no,
    };
    let login: String = login.chars().take(32).collect();
    let Some(f) = crate::db(&app, move |c| found(c, &login)).await else { return crate::fail(StatusCode::NOT_FOUND, "no-member") };
    if f.uid != who.uid && f.from != "site" {
        return crate::fail(StatusCode::NOT_FOUND, "no-cape");
    }
    let Some(name) = f.skin.strip_prefix("name:").and_then(name_of) else { return crate::fail(StatusCode::NOT_FOUND, "no-cape") };
    let d = dir(&app);
    match crate::blocking(move || std::fs::read(d.join(format!("{name}.cape.png")))).await {
        Ok(b) => png(b),
        Err(_) => crate::fail(StatusCode::NOT_FOUND, "no-cape"),
    }
}

#[derive(Deserialize)]
struct Page {
    /// Older than this (seconds), for the next page of the newest.
    #[serde(default)]
    before: Option<i64>,
    /// "top": the most liked first (then the page is `skip` rows on).
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    skip: Option<i64>,
}

/// The looks members keep on the site, the newest first (the game's outfits are not in it: who plays as whom would be
/// read from them). Only what is worn and whose. Banned members are not in it.
async fn get_looks(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Page>) -> Response {
    let who = match member(&app, &h, false).await {
        Ok(w) => w,
        Err(no) => return no,
    };
    let before = q.before.unwrap_or(i64::MAX);
    let top = q.sort.as_deref() == Some("top");
    let skip = q.skip.unwrap_or(0).clamp(0, 100_000);
    let now = now() as i64;
    let fan = who.uid;
    let items = crate::db(&app, move |c| -> Vec<Value> {
        let sql = format!(
            "SELECT u.login, l.worn, l.updated, l.skin != '', (SELECT COUNT(*) FROM look_likes k WHERE k.owner = l.uid) AS n,
                    EXISTS(SELECT 1 FROM look_likes k WHERE k.owner = l.uid AND k.fan = ?3)
               FROM looks l JOIN users u ON u.uid = l.uid
              WHERE l.updated < ?1 AND (l.skin != '' OR l.worn != '{{}}')
                AND l.uid NOT IN (SELECT uid FROM bans WHERE lifted IS NULL AND (until IS NULL OR until > ?2))
              ORDER BY {} LIMIT 30 OFFSET ?4",
            if top { "n DESC, l.updated DESC" } else { "l.updated DESC" }
        );
        let Ok(mut st) = c.prepare(&sql) else { return Vec::new() };
        let rows = st.query_map(params![if top { i64::MAX } else { before }, now, fan, if top { skip } else { 0 }], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, bool>(3)?, r.get::<_, i64>(4)?, r.get::<_, bool>(5)?))
        });
        let Ok(rows) = rows else { return Vec::new() };
        rows.flatten()
            .map(|(login, worn, updated, skin, likes, liked)| json!({ "login": login, "from": "site", "worn": serde_json::from_str::<Value>(&worn).unwrap_or(Value::Null), "updated": updated, "skin": skin, "likes": likes, "liked": liked }))
            .collect()
    })
    .await;
    Json(json!({ "items": items })).into_response()
}

/// How many like a member's look, and whether `fan` does.
fn likes_of(c: &Connection, owner: i64, fan: i64) -> (i64, bool) {
    let n = c.query_row("SELECT COUNT(*) FROM look_likes WHERE owner = ?1", params![owner], |r| r.get(0)).unwrap_or(0);
    let mine = c.query_row("SELECT EXISTS(SELECT 1 FROM look_likes WHERE owner = ?1 AND fan = ?2)", params![owner, fan], |r| r.get(0)).unwrap_or(false);
    (n, mine)
}

static LIKED: Mutex<VecDeque<(Instant, i64)>> = Mutex::new(VecDeque::new());

#[derive(Deserialize)]
struct Like {
    on: bool,
}

async fn like_look(State(app): State<Shared>, h: HeaderMap, Path(login): Path<String>, Json(l): Json<Like>) -> Response {
    let who = match member(&app, &h, true).await {
        Ok(w) => w,
        Err(no) => return no,
    };
    {
        // thirty a minute for a member: enough for a hand, not for a script
        let mut q = LIKED.lock().unwrap_or_else(|e| e.into_inner());
        while q.front().is_some_and(|t| t.0.elapsed() > Duration::from_secs(60)) {
            q.pop_front();
        }
        if q.iter().filter(|t| t.1 == who.uid).count() >= 30 {
            return crate::fail(StatusCode::TOO_MANY_REQUESTS, "slow");
        }
        q.push_back((Instant::now(), who.uid));
    }
    let login: String = login.chars().take(32).collect();
    let fan = who.uid;
    let fan_login = who.login.clone();
    let done = crate::db(&app, move |c| -> Result<(i64, bool), &'static str> {
        let f = found(c, &login).ok_or("no-member")?;
        if f.from != "site" {
            return Err("no-look");
        }
        if f.uid == fan {
            return Err("own");
        }
        if l.on {
            let added = c.execute("INSERT OR IGNORE INTO look_likes (owner, fan, at) VALUES (?1, ?2, ?3)", params![f.uid, fan, now() as i64]).unwrap_or(0) > 0;
            if added {
                let n: i64 = c.query_row("SELECT COUNT(*) FROM look_likes WHERE owner = ?1", params![f.uid], |r| r.get(0)).unwrap_or(0);
                crate::people::set_stat(c, f.uid, "look_likes", n);
                let subject = format!("look-like:{fan}");
                let told: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM notifications WHERE uid = ?1 AND kind = 'look' AND subject = ?2)", params![f.uid, subject], |r| r.get(0)).unwrap_or(true);
                if !told {
                    let href = format!("#/u/{}", f.login);
                    crate::notify::add(c, f.uid, "look", Some(fan), &subject, json!({ "text": format!("{fan_login} оценил(а) ваш образ"), "href": href }));
                }
            }
        } else {
            let _ = c.execute("DELETE FROM look_likes WHERE owner = ?1 AND fan = ?2", params![f.uid, fan]);
        }
        Ok(likes_of(c, f.uid, fan))
    })
    .await;
    match done {
        Ok((likes, liked)) => Json(json!({ "likes": likes, "liked": liked })).into_response(),
        Err(e) => crate::fail(if e == "no-member" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST }, e),
    }
}

#[derive(Deserialize)]
struct Look {
    /// "" (none), "name:<a Minecraft name>", or "file" (their own picture, sent before).
    skin: String,
    worn: Map<String, Value>,
}

/// What worn may be: the slots and the items' settings ("opt:<item>:<name>"), short words only.
fn clean_worn(w: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in w.iter().take(64) {
        if k.is_empty() || k.len() > 80 || !k.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'.' | b'-')) {
            continue;
        }
        if let Some(s) = v.as_str().filter(|s| !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))) {
            out.insert(k.clone(), Value::String(s.to_string()));
        }
    }
    out
}

async fn put_look(State(app): State<Shared>, h: HeaderMap, Json(l): Json<Look>) -> Response {
    let who = match member(&app, &h, true).await {
        Ok(w) => w,
        Err(no) => return no,
    };
    let skin = match l.skin.as_str() {
        "" | "file" => l.skin.clone(),
        s => match s.strip_prefix("name:").and_then(name_of) {
            Some(n) => format!("name:{n}"),
            None => return crate::fail(StatusCode::BAD_REQUEST, "skin"),
        },
    };
    let worn = clean_worn(&l.worn);
    let wears = worn.keys().any(|k| !k.starts_with("opt:") && k != "emote");
    let worn = Value::Object(worn).to_string();
    let uid = who.uid;
    let at = now() as i64;
    let done = crate::db(&app, move |c| {
        let n = c.execute(
            "INSERT INTO looks (uid, skin, worn, updated) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(uid) DO UPDATE SET skin = ?2, worn = ?3, updated = ?4",
            params![uid, skin, worn, at],
        )?;
        // a look with something on it: the wardrobe's first badge
        if wears {
            crate::people::award(c, uid, "stylist", 1, None);
        }
        Ok::<usize, rusqlite::Error>(n)
    })
    .await;
    match done {
        Ok(_) => Json(json!({ "ok": true, "updated": at })).into_response(),
        Err(_) => crate::market::busy(),
    }
}

/// A member's own skin picture (a PNG, 64 × 64 or 64 × 32), kept for their look.
async fn put_look_skin(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    let who = match member(&app, &h, true).await {
        Ok(w) => w,
        Err(no) => return no,
    };
    if !skin_ok(&body) {
        return crate::fail(StatusCode::BAD_REQUEST, "skin-file");
    }
    let path = own_file(&app, who.uid);
    let done = crate::blocking(move || {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&path, &body)
    })
    .await;
    match done {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(_) => crate::market::busy(),
    }
}

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/skin/{name}", get(get_skin))
        .route("/api/skin/{name}/cape", get(get_cape))
        .route("/api/look", post(put_look).layer(DefaultBodyLimit::max(16 * 1024)))
        .route("/api/look-skin", post(put_look_skin).layer(DefaultBodyLimit::max(70 * 1024)))
        .route("/api/look/{login}", get(get_look))
        .route("/api/look/{login}/skin", get(get_look_skin))
        .route("/api/look/{login}/cape", get(get_look_cape))
        .route("/api/look/{login}/like", post(like_look).layer(DefaultBodyLimit::max(1024)))
        .route("/api/looks", get(get_looks))
}
