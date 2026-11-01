//! cursed-api: the accounts of cursedclient.xyz.
//!
//! What it keeps of a person: the login, the UID (one per registration, counting up from 1; hers is 0), the
//! e-mail, the password's argon2id hash and the moment of registration; of the network it registered from, only a
//! keyed hash (two accounts a network, beta.rs). No addresses.
//! The site signs in with a cookie; the loader asks for a token (POST /api/token) and shows the login and the UID.
//! It listens on 127.0.0.1 only, behind nginx, which also limits how often the doors may be knocked on.
//!
//! The market (market/): packs and shaders members upload, checked and rebuilt, with their pages.
//! The forum (forum.rs): categories, threads, posts, reports and moderation.
//! The blocklist (blocklist.rs): the servers closed to Cursed, signed for the client.
//!
//! The beta (beta.rs): testers, bans, one computer per account, the game's own check at its start.
//! People (people.rs): roles and their prefixes, profiles, badges, sessions; the live line (live.rs) and the
//! notifications (notify.rs) that run down it; the members' own uploads (media.rs).
//! The client's files (dist.rs): every build kept, signed and served for the loader.
//! Recode's keys (recode.rs): made by her alone, activated in the cabinet; an account's Recode is its keys' days.
//!
//! Registration is closed until opened (`cursed-api registration open`); while closed, only a one-time key made by
//! `cursed-api key` lets someone in (it also lets them take a reserved login). Open, it takes as many as its limit
//! (`--limit N`, 15 for the beta; hers is not counted).

mod avatar;
mod beta;
mod blocklist;
mod changes;
mod chat;
mod configs;
mod cosmetics;
mod dist;
mod forum;
mod forum2;
mod fps;
mod tickets;
mod tops;
mod admin;
mod friends;
mod live;
mod looks;
mod mail;
mod market;
mod marks;
mod media;
mod notify;
mod people;
mod party;
mod pay;
mod protect;
mod recode;
mod refs;
mod seal;

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, ErrorCode, OptionalExtension};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

const COOKIE: &str = "cursed_session";
const WEB_DAYS: i64 = 30; // a site session lives this long after it was last used
const LOADER_DAYS: i64 = 30; // a loader token, the same ("remember me", her word 30.09)
const CLIENT_DAYS: i64 = 2; // the game's own session, from its start (beta.rs)
const TRIES: u32 = 10; // wrong passwords for one account before it waits
const TRIES_WINDOW: Duration = Duration::from_secs(15 * 60);
// Nobody takes these by registering (a one-time key can): no one passes for her or for the site.
const OWNER: i64 = 0; // primary account; financial ledger identity stays unchanged
// Explicit co-owner authorized by Lunuca on 2026-10-28: Andrey_AKM (UID 19).
// A display role alone must never grant these account-level privileges.
const OWNERS: &[i64] = &[OWNER, 19];
fn is_owner(uid: i64) -> bool { OWNERS.contains(&uid) }
const RESERVED: &[&str] = &["lunuca", "cursed", "admin", "administrator", "moderator", "mod", "support", "staff", "official", "system", "root"];

struct App {
    db: Mutex<Connection>,
    tries: Mutex<HashMap<String, (u32, Instant)>>, // wrong passwords per account, for a short while
    secure: bool,                                  // the cookie only over HTTPS
    origin: String,                                // the only page that may sign in with a cookie
    dummy: String,                                 // checked when there is no such account, so both answers take as long
    market: market::Market,
    pace: forum::Pace, // who is online, how fast each member posts
    blocklist: blocklist::Signature,
    dist: dist::Dist,
    db_path: String,
    net_key: [u8; 32], // the networks registered from are kept hashed with it (beta.rs)
}
type Shared = Arc<App>;

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn open(path: &str) -> Connection {
    let c = Connection::open(path).expect("open the database");
    c.execute_batch(
        "PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;
         CREATE TABLE IF NOT EXISTS users (
             uid INTEGER PRIMARY KEY AUTOINCREMENT,
             login TEXT NOT NULL, login_key TEXT NOT NULL UNIQUE,
             email TEXT NOT NULL, email_key TEXT NOT NULL UNIQUE,
             pass TEXT NOT NULL,
             created INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS sessions (
             hash BLOB PRIMARY KEY,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             kind TEXT NOT NULL,
             created INTEGER NOT NULL,
             seen INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS sessions_uid ON sessions(uid);
         CREATE TABLE IF NOT EXISTS keys (hash BLOB PRIMARY KEY, made INTEGER NOT NULL, used_by INTEGER);
         CREATE TABLE IF NOT EXISTS settings (name TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT OR IGNORE INTO settings VALUES ('registration', 'closed');",
    )
    .expect("make the tables");
    market::schema(&c);
    forum::schema(&c);
    forum2::schema(&c);
    tickets::schema(&c);
    avatar::schema(&c);
    blocklist::schema(&c);
    beta::schema(&c);
    cosmetics::schema(&c);
    looks::schema(&c);
    marks::schema(&c);
    dist::schema(&c);
    media::schema(&c);
    notify::schema(&c);
    people::schema(&c);
    friends::schema(&c);
    chat::schema(&c);
    configs::schema(&c);
    // After the beta's tables: it adds to its crashes.
    admin::schema(&c);
    party::schema(&c);
    refs::schema(&c);
    fps::schema(&c);
    pay::schema(&c);
    mail::schema(&c);
    recode::schema(&c);
    c
}

// ---------------------------------------------------------------------------------------------------- pieces

/// `bytes` random bytes as hex: session tokens (32) and registration keys (16).
fn random_hex(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    OsRng.fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Tokens and keys are kept only as their hash: a stolen database signs no one in.
fn digest(t: &str) -> Vec<u8> {
    Sha256::digest(t.as_bytes()).to_vec()
}

fn hash_password(p: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(p.as_bytes(), &salt).expect("hash a password").to_string()
}

fn password_matches(p: &str, stored: &str) -> bool {
    PasswordHash::new(stored).map(|h| Argon2::default().verify_password(p.as_bytes(), &h).is_ok()).unwrap_or(false)
}

fn login_ok(s: &str) -> bool {
    (3..=16).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn email_ok(s: &str) -> bool {
    if s.len() > 254 || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    match s.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty() && local.len() <= 64 && !domain.contains('@') && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
        }
        None => false,
    }
}

fn password_ok(s: &str) -> bool {
    (10..=128).contains(&s.chars().count())
}

fn fail(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({ "error": error }))).into_response()
}

fn cookie(app: &App, value: &str, max_age: i64) -> HeaderValue {
    let secure = if app.secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{COOKIE}={value}; Path=/api; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}")).expect("a cookie")
}

/// A page elsewhere may not sign anyone in or out here. The loader sends no Origin at all.
fn same_site(app: &App, h: &HeaderMap) -> bool {
    h.get_all(header::ORIGIN).iter().all(|o| o.as_bytes() == app.origin.as_bytes())
}

/// The token a request carries: `Authorization: Bearer …` (the loader) or the cookie (the site).
fn presented(h: &HeaderMap) -> Option<(String, bool)> {
    let well_formed = |t: &str| t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit());
    if let Some(t) = h.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")) {
        let t = t.trim();
        return well_formed(t).then(|| (t.to_string(), false));
    }
    h.get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|p| p.trim().strip_prefix("cursed_session="))
        .filter(|t| well_formed(*t))
        .map(|t| (t.to_string(), true))
}

async fn db<T: Send + 'static>(app: &Shared, f: impl FnOnce(&mut Connection) -> T + Send + 'static) -> T {
    let app = app.clone();
    tokio::task::spawn_blocking(move || f(&mut app.db.lock().unwrap_or_else(|e| e.into_inner()))).await.expect("a database call")
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.expect("a blocking call")
}

fn new_session(c: &Connection, uid: i64, kind: &str) -> rusqlite::Result<String> {
    let t = random_hex(32);
    let at = now();
    c.execute("INSERT INTO sessions (hash, uid, kind, created, seen) VALUES (?1, ?2, ?3, ?4, ?4)", params![digest(&t), uid, kind, at])?;
    Ok(t)
}

struct Who {
    uid: i64,
    login: String,
    email: String,
    created: i64,
    hash: Vec<u8>,
}

/// Whose session this token is, if it is alive; an old one is thrown away here.
fn who(c: &Connection, token: &str) -> Option<Who> {
    let hash = digest(token);
    let row = c
        .query_row(
            "SELECT s.uid, s.kind, s.seen, u.login, u.email, u.created FROM sessions s JOIN users u ON u.uid = s.uid WHERE s.hash = ?1",
            params![hash],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, i64>(5)?)),
        )
        .optional()
        .ok()??;
    let (uid, kind, seen, login, email, created) = row;
    let days = match kind.as_str() {
        "loader" => LOADER_DAYS,
        "client" => CLIENT_DAYS,
        _ => WEB_DAYS,
    };
    let at = now();
    if at - seen > days * 86_400 || beta::ban_of(c, uid).is_some() {
        let _ = c.execute("DELETE FROM sessions WHERE hash = ?1", params![hash]);
        return None;
    }
    if at - seen > 3600 {
        let _ = c.execute("UPDATE sessions SET seen = ?2 WHERE hash = ?1", params![hash, at]);
    }
    people::ensure(c, uid, &login);
    Some(Who { uid, login, email, created, hash })
}

/// Long-lived responses must recheck access without extending the session's lifetime.
fn session_alive(c: &Connection, hash: &[u8]) -> bool {
    let row = c.query_row("SELECT uid, kind, seen FROM sessions WHERE hash = ?1", params![hash], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
    }).optional().ok().flatten();
    let Some((uid, kind, seen)) = row else { return false };
    let days = match kind.as_str() { "loader" => LOADER_DAYS, "client" => CLIENT_DAYS, _ => WEB_DAYS };
    now() - seen <= days * 86_400 && beta::ban_of(c, uid).is_none()
}

// ----------------------------------------------------------------------------------------------- the doors

#[derive(Deserialize)]
struct Register {
    login: String,
    email: String,
    password: String,
    #[serde(default)]
    key: String,
    /// The referral link's code the visitor came by (refs.rs).
    #[serde(default, rename = "ref")]
    referral: String,
    /// The site's language, for the confirmation letter (mail.rs).
    #[serde(default)]
    lang: String,
}

async fn register(State(app): State<Shared>, h: HeaderMap, Json(r): Json<Register>) -> Response {
    if !same_site(&app, &h) {
        return fail(StatusCode::FORBIDDEN, "origin");
    }
    let login = r.login.trim().to_string();
    let email = r.email.trim().to_string();
    if !login_ok(&login) {
        return fail(StatusCode::BAD_REQUEST, "login");
    }
    if !email_ok(&email) {
        return fail(StatusCode::BAD_REQUEST, "email");
    }
    if !password_ok(&r.password) {
        return fail(StatusCode::BAD_REQUEST, "password");
    }
    let key = r.key.trim().to_string();
    let key_hash = (!key.is_empty() && key.len() <= 64).then(|| digest(&key));
    let net = beta::net_of(&h, &app.net_key);
    let referral = r.referral;
    let (mail_to, lang) = (email.clone(), r.lang.clone());

    // Is the door open for this one? Asked before the password is hashed, so a closed door costs nothing.
    let kh = key_hash.clone();
    let n = net.clone();
    let (open, full, crowded, keyed) = db(&app, move |c| {
        let open = c.query_row("SELECT value FROM settings WHERE name = 'registration'", [], |r| r.get::<_, String>(0)).map(|v| v == "open").unwrap_or(false);
        let keyed = kh.map_or(false, |k| c.query_row("SELECT 1 FROM keys WHERE hash = ?1 AND used_by IS NULL", params![k], |_| Ok(())).optional().ok().flatten().is_some());
        (open, beta::full(c), n.is_some_and(|n| beta::net_full(c, &n)), keyed)
    })
    .await;
    if !keyed {
        if !open {
            return fail(StatusCode::FORBIDDEN, "closed");
        }
        if full {
            return fail(StatusCode::FORBIDDEN, "full");
        }
        if crowded {
            return fail(StatusCode::FORBIDDEN, "ip-limit");
        }
    }
    if !keyed && RESERVED.contains(&login.to_ascii_lowercase().as_str()) {
        return fail(StatusCode::CONFLICT, "login-taken");
    }

    let password = r.password;
    let pass = blocking(move || hash_password(&password)).await;
    let made = db(&app, move |c| -> Result<(i64, String, String), &'static str> {
        let tx = c.transaction().map_err(|_| "busy")?;
        // The last place taken by two at once: only the first of them gets it.
        if !keyed && beta::full(&tx) {
            return Err("full");
        }
        // The same for a network's last place.
        if !keyed && net.as_deref().is_some_and(|n| beta::net_full(&tx, n)) {
            return Err("ip-limit");
        }
        if keyed {
            // The key is used up in the same breath as the account is made: two at once cannot both have it.
            let taken = tx.execute("UPDATE keys SET used_by = -1 WHERE hash = ?1 AND used_by IS NULL", params![key_hash]).map_err(|_| "busy")?;
            if taken == 0 {
                return Err("closed");
            }
        }
        let uid = next_uid(&tx).map_err(|_| "busy")?;
        let inserted = tx.execute(
            "INSERT INTO users (uid, login, login_key, email, email_key, pass, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![uid, login, login.to_ascii_lowercase(), email, email.to_lowercase(), pass, now()],
        );
        if let Err(rusqlite::Error::SqliteFailure(e, Some(msg))) = &inserted {
            if e.code == ErrorCode::ConstraintViolation {
                return Err(if msg.contains("email_key") { "email-taken" } else { "login-taken" });
            }
        }
        inserted.map_err(|_| "busy")?;
        if keyed {
            tx.execute("UPDATE keys SET used_by = ?2 WHERE hash = ?1", params![key_hash, uid]).map_err(|_| "busy")?;
        }
        if let Some(n) = &net {
            beta::remember_net(&tx, uid, n).map_err(|_| "busy")?;
        }
        beta::made_tester(&tx, uid).map_err(|_| "busy")?;
        refs::attach(&tx, uid, &referral);
        // The badges of where they came from (the first hundred, the beta, the day of the launch), at once.
        people::sweep(&tx);
        let token = new_session(&tx, uid, "web").map_err(|_| "busy")?;
        tx.commit().map_err(|_| "busy")?;
        Ok((uid, login, token))
    })
    .await;
    match made {
        Ok((uid, login, token)) => {
            mail::welcome(&app, uid, login.clone(), mail_to, lang).await;
            let mut res = Json(json!({ "uid": uid, "login": login })).into_response();
            res.headers_mut().insert(header::SET_COOKIE, cookie(&app, &token, WEB_DAYS * 86_400));
            res
        }
        Err(e @ ("closed" | "full" | "ip-limit")) => fail(StatusCode::FORBIDDEN, e),
        Err(e @ ("login-taken" | "email-taken")) => fail(StatusCode::CONFLICT, e),
        Err(_) => fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
}

#[derive(Deserialize)]
struct SignIn {
    login: String,
    password: String,
}

/// Login (or e-mail) and password right? Ten wrong ones for an account and it waits a quarter of an hour; an unknown
/// login is checked against a dummy hash, so the answer's time tells nothing.
async fn verify(app: &Shared, s: SignIn) -> Result<(i64, String), Response> {
    let id = s.login.trim().to_lowercase();
    if id.is_empty() || id.len() > 254 || s.password.len() > 512 {
        return Err(fail(StatusCode::UNAUTHORIZED, "wrong"));
    }
    {
        let mut tries = app.tries.lock().unwrap_or_else(|e| e.into_inner());
        tries.retain(|_, (_, at)| at.elapsed() < TRIES_WINDOW);
        if tries.get(&id).is_some_and(|(n, _)| *n >= TRIES) {
            return Err(fail(StatusCode::TOO_MANY_REQUESTS, "wait"));
        }
    }
    let key = id.clone();
    let found = db(app, move |c| {
        c.query_row("SELECT uid, login, pass FROM users WHERE login_key = ?1 OR email_key = ?1", params![key], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })
        .optional()
        .ok()
        .flatten()
    })
    .await;
    let dummy = app.dummy.clone();
    let password = s.password;
    let user = blocking(move || match found {
        Some((uid, login, pass)) => password_matches(&password, &pass).then_some((uid, login)),
        None => {
            password_matches(&password, &dummy);
            None
        }
    })
    .await;
    {
        let mut tries = app.tries.lock().unwrap_or_else(|e| e.into_inner());
        if user.is_none() {
            let entry = tries.entry(id).or_insert((0, Instant::now()));
            entry.0 += 1;
            return Err(fail(StatusCode::UNAUTHORIZED, "wrong"));
        }
        tries.remove(&id);
    }
    let (uid, login) = user.expect("checked above");
    // A banned account signs in nowhere; it is told why and until when.
    if let Some(b) = db(app, move |c| beta::ban_of(c, uid)).await {
        return Err((StatusCode::FORBIDDEN, Json(b.json())).into_response());
    }
    Ok((uid, login))
}

async fn signin(State(app): State<Shared>, h: HeaderMap, Json(s): Json<SignIn>) -> Response {
    if !same_site(&app, &h) {
        return fail(StatusCode::FORBIDDEN, "origin");
    }
    let (uid, login) = match verify(&app, s).await {
        Ok(u) => u,
        Err(res) => return res,
    };
    let label = people::agent_label(&h);
    match db(&app, move |c| {
        let t = new_session(c, uid, "web")?;
        c.execute("UPDATE sessions SET label = ?2 WHERE hash = ?1", params![digest(&t), label])?;
        Ok::<_, rusqlite::Error>(t)
    })
    .await
    {
        Ok(token) => {
            let mut res = Json(json!({ "uid": uid, "login": login })).into_response();
            res.headers_mut().insert(header::SET_COOKIE, cookie(&app, &token, WEB_DAYS * 86_400));
            res
        }
        Err(_) => fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
}

/// The loader's sign-in: the same check, a token in the answer (kept by the loader, sent as `Authorization: Bearer`).
async fn loader_token(State(app): State<Shared>, h: HeaderMap, Json(s): Json<SignIn>) -> Response {
    if h.contains_key(header::ORIGIN) {
        return fail(StatusCode::FORBIDDEN, "origin"); // for the loader only, never for a web page
    }
    let (uid, login) = match verify(&app, s).await {
        Ok(u) => u,
        Err(res) => return res,
    };
    match db(&app, move |c| new_session(c, uid, "loader")).await {
        Ok(token) => Json(json!({ "uid": uid, "login": login, "token": token })).into_response(),
        Err(_) => fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
}

/// Who is signed in. The e-mail only to the site (the loader has no use for it).
async fn me(State(app): State<Shared>, h: HeaderMap) -> Response {
    let Some((token, by_cookie)) = presented(&h) else {
        return fail(StatusCode::UNAUTHORIZED, "signed-out");
    };
    match db(&app, move |c| who(c, &token).map(|w| {
        let tester = beta::is_tester(c, w.uid);
        let card = people::card_of(c, w.uid);
        let unread = notify::unread(c, w.uid);
        let confirmed = mail::confirmed(c, w.uid);
        let recode = recode::of(c, w.uid).json(now());
        (w, tester, card, unread, confirmed, recode)
    }))
    .await
    {
        Some((w, tester, card, unread, confirmed, recode)) => {
            forum::seen(&app, &w);
            // A tester sees the loader's download on the site; staff see what their roles let them do.
            let perms: Vec<&str> = [
                people::STAFF_PANEL, people::USERS_VIEW, people::USERS_BAN, people::USERS_MUTE, people::USERS_DEVICE, people::FORUM_MODERATE,
                people::REPORTS, people::TICKETS, people::MEDIA_MODERATE, people::MARKET_MODERATE, people::MESSAGES_REPORTS, people::BADGES_GIVE, people::ANNOUNCE, people::AUDIT,
                people::REFS_OWN, people::REFS_ALL, people::RECODE_KEYS, people::RECODE_REVOKE,
            ]
            .into_iter()
            .filter(|p| people::can(w.uid, p))
            .collect();
            let mut body = json!({
                "uid": w.uid, "uid_text": card["uid_text"].clone(), "uid_self": people::may_text_uid(w.uid),
                "login": w.login, "since": w.created, "admin": crate::is_owner(w.uid), "avatar": avatar::of(w.uid), "tester": tester,
                "card": card, "unread": unread, "staff": people::is_staff(w.uid), "perms": perms, "recode": recode,
            });
            if by_cookie {
                body["email"] = json!(w.email);
                body["email_confirmed"] = json!(confirmed);
                body["mail"] = json!(mail::configured());
            }
            Json(body).into_response()
        }
        None => {
            let mut res = fail(StatusCode::UNAUTHORIZED, "signed-out");
            if by_cookie {
                res.headers_mut().insert(header::SET_COOKIE, cookie(&app, "", 0));
            }
            res
        }
    }
}

async fn logout(State(app): State<Shared>, h: HeaderMap) -> Response {
    if !same_site(&app, &h) {
        return fail(StatusCode::FORBIDDEN, "origin");
    }
    if let Some((token, _)) = presented(&h) {
        db(&app, move |c| c.execute("DELETE FROM sessions WHERE hash = ?1", params![digest(&token)])).await.ok();
    }
    let mut res = Json(json!({})).into_response();
    res.headers_mut().insert(header::SET_COOKIE, cookie(&app, "", 0));
    res
}

#[derive(Deserialize)]
struct NewPassword {
    current: String,
    new: String,
}

/// A new password (the current one asked again); every other session and token of the account ends with it.
async fn password(State(app): State<Shared>, h: HeaderMap, Json(p): Json<NewPassword>) -> Response {
    if !same_site(&app, &h) {
        return fail(StatusCode::FORBIDDEN, "origin");
    }
    let Some((token, _)) = presented(&h) else {
        return fail(StatusCode::UNAUTHORIZED, "signed-out");
    };
    let Some(w) = db(&app, move |c| who(c, &token)).await else {
        return fail(StatusCode::UNAUTHORIZED, "signed-out");
    };
    if !password_ok(&p.new) {
        return fail(StatusCode::BAD_REQUEST, "password");
    }
    if let Err(res) = verify(&app, SignIn { login: w.login.clone(), password: p.current }).await {
        return res;
    }
    let new = p.new;
    let pass = blocking(move || hash_password(&new)).await;
    let (uid, keep) = (w.uid, w.hash);
    let done = db(&app, move |c| -> rusqlite::Result<()> {
        let tx = c.transaction()?;
        tx.execute("UPDATE users SET pass = ?2 WHERE uid = ?1", params![uid, pass])?;
        tx.execute("DELETE FROM sessions WHERE uid = ?1 AND hash != ?2", params![uid, keep])?;
        tx.commit()
    })
    .await;
    match done {
        Ok(()) => Json(json!({})).into_response(),
        Err(_) => fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
}

#[derive(Deserialize)]
struct NewEmail {
    password: String,
    email: String,
    #[serde(default)]
    lang: String,
}

/// Another e-mail for the account (hers, 11.10.2026), the password asked again. One e-mail is one account's: a taken
/// one is refused. The journal keeps that it changed, not the addresses.
async fn change_email(State(app): State<Shared>, h: HeaderMap, Json(e): Json<NewEmail>) -> Response {
    if !same_site(&app, &h) {
        return fail(StatusCode::FORBIDDEN, "origin");
    }
    let Some((token, _)) = presented(&h) else {
        return fail(StatusCode::UNAUTHORIZED, "signed-out");
    };
    let Some(w) = db(&app, move |c| who(c, &token)).await else {
        return fail(StatusCode::UNAUTHORIZED, "signed-out");
    };
    let email = e.email.trim().to_string();
    if !email_ok(&email) {
        return fail(StatusCode::BAD_REQUEST, "email");
    }
    if email.to_lowercase() == w.email.to_lowercase() {
        return fail(StatusCode::BAD_REQUEST, "same-email");
    }
    if let Err(res) = verify(&app, SignIn { login: w.login.clone(), password: e.password }).await {
        return res;
    }
    let uid = w.uid;
    let to = email.clone();
    let done = db(&app, move |c| -> Result<(), &'static str> {
        let changed = c.execute("UPDATE users SET email = ?2, email_key = ?3, email_confirmed = NULL WHERE uid = ?1", params![uid, to, to.to_lowercase()]);
        if let Err(rusqlite::Error::SqliteFailure(err, _)) = &changed {
            if err.code == ErrorCode::ConstraintViolation {
                return Err("email-taken");
            }
        }
        changed.map_err(|_| "busy")?;
        people::audit(c, uid, "user.email", &format!("user:{uid}"), json!({}));
        Ok(())
    })
    .await;
    match done {
        Ok(()) => {
            // the old address is told, the new one asked to confirm (mail.rs)
            mail::changed(&app, uid, w.login.clone(), w.email.clone(), email.clone(), e.lang).await;
            Json(json!({ "email": email })).into_response()
        }
        Err("email-taken") => fail(StatusCode::CONFLICT, "email-taken"),
        Err(_) => fail(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
}

/// Moves an account to another UID (hers from 1 to 0, 27.09; Anton4k272's to 272 and sequwi's to 1, 02.10), and
/// everything that is its with it: sessions, its key, the beta's records, the market's, the forum's, its outfit. The
/// count is the run's highest number: one given by hand above it (272) does not move it, so the next registration takes
/// the first number after the run and none is skipped (next_uid passes over the one given by hand when the run gets
/// there).
fn renumber(c: &mut Connection, from: i64, to: i64) -> Result<(), String> {
    c.execute_batch("PRAGMA foreign_keys = OFF;").map_err(|e| e.to_string())?;
    let done = renumber_in(c, from, to);
    // Whatever came of it, the links are checked again: an early answer ("UID taken") used to leave them off on the
    // server's own connection.
    let on = c.execute_batch("PRAGMA foreign_keys = ON;").map_err(|e| e.to_string());
    if done.is_ok() {
        // the member's own skin (looks.rs) goes with the number
        if let Some(own) = c.path().and_then(|p| std::path::Path::new(p).parent()).map(|d| d.join("skins").join("own")) {
            let _ = std::fs::rename(own.join(format!("u{from}.png")), own.join(format!("u{to}.png")));
        }
    }
    done.and(on)
}

fn renumber_in(c: &mut Connection, from: i64, to: i64) -> Result<(), String> {
    let tx = c.transaction().map_err(|e| e.to_string())?;
    let taken: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM users WHERE uid = ?1)", params![to], |r| r.get(0)).map_err(|e| e.to_string())?;
    if taken {
        return Err(format!("UID {to} is taken"));
    }
    if tx.execute("UPDATE users SET uid = ?2 WHERE uid = ?1", params![from, to]).map_err(|e| e.to_string())? != 1 {
        return Err(format!("there is no UID {from}"));
    }
    tx.execute("UPDATE sessions SET uid = ?2 WHERE uid = ?1", params![from, to]).map_err(|e| e.to_string())?;
    tx.execute("UPDATE keys SET used_by = ?2 WHERE used_by = ?1", params![from, to]).map_err(|e| e.to_string())?;
    // The beta's records of the account go with it, and the market's, the forum's, the cosmetics'.
    let columns = [
        ("roles", "uid"),
        ("devices", "uid"),
        ("device_resets", "uid"),
        ("access", "uid"),
        ("dist_access", "uid"),
        ("tickets", "uid"),
        ("bans", "uid"),
        ("crashes", "uid"),
        ("packs", "owner"),
        ("uploads", "owner"),
        ("downloads", "uid"),
        ("threads", "author"),
        ("threads", "last_by"),
        ("posts", "author"),
        ("reads", "uid"),
        ("reports", "by"),
        ("outfits", "uid"),
        ("looks", "uid"),
        ("look_likes", "owner"),
        ("look_likes", "fan"),
        ("cheat_marks", "uid"),
        // People (people.rs, notify.rs, media.rs).
        ("profiles", "uid"),
        ("user_badges", "uid"),
        ("user_badges", "by"),
        ("stats", "uid"),
        ("blocks", "uid"),
        ("blocks", "blocked"),
        ("audit", "actor"),
        ("notifications", "uid"),
        ("notifications", "actor"),
        ("notify_prefs", "uid"),
        ("media", "owner"),
        // Friends and conversations (friends.rs, chat.rs).
        ("friends", "a"),
        ("friends", "b"),
        ("friend_requests", "from_uid"),
        ("friend_requests", "to_uid"),
        ("convs", "owner"),
        ("conv_members", "uid"),
        ("dm_pairs", "a"),
        ("dm_pairs", "b"),
        ("conv_invites", "uid"),
        ("conv_invites", "by"),
        ("messages", "uid"),
        ("messages", "deleted_by"),
        ("reactions", "uid"),
        ("chat_reports", "reporter"),
        ("chat_reports", "accused"),
        ("chat_reports", "handled_by"),
        // Configs (configs.rs).
        ("configs", "owner"),
        ("config_access", "uid"),
        ("config_active", "uid"),
        // The forum's second floor (forum2.rs).
        ("post_reactions", "uid"),
        ("thread_subs", "uid"),
        ("thread_votes", "uid"),
        ("forum_mutes", "uid"),
        ("forum_mutes", "by"),
        ("warnings", "uid"),
        ("warnings", "by"),
        // Support (tickets.rs).
        ("support_tickets", "uid"),
        ("support_tickets", "assignee"),
        ("ticket_messages", "uid"),
        ("ticket_files", "uid"),
        ("ticket_replies", "by"),
        // The staff's panel (admin.rs).
        ("user_notes", "uid"),
        ("user_notes", "by"),
        ("announcements", "by"),
        // Parties (party.rs).
        ("parties", "leader"),
        ("party_members", "uid"),
        ("party_seen", "uid"),
        ("party_invites", "uid"),
        ("party_invites", "by"),
        ("party_marks", "uid"),
        // The network it registered from (beta.rs).
        ("reg_nets", "uid"),
        // Referral links and whom they brought (refs.rs).
        ("ref_links", "owner"),
        ("referrals", "uid"),
        // The game's friends a member keeps (friends.rs), the mail's links not yet followed (mail.rs).
        ("mc_friends", "uid"),
        ("mail_tokens", "uid"),
        // Recode's keys it activated, gave by hand and made (recode.rs).
        ("recode_keys", "used_by"),
        ("recode_keys", "given"),
        ("recode_keys", "maker"),
    ];
    for (table, column) in columns {
        let there: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)", params![table], |r| r.get(0)).map_err(|e| e.to_string())?;
        if there {
            tx.execute(&format!("UPDATE {table} SET {column} = ?2 WHERE {column} = ?1"), params![from, to]).map_err(|e| e.to_string())?;
        }
    }
    // What the staff did to them (the journal names a member by number in text).
    tx.execute(
        "UPDATE audit SET target = CAST(?2 AS TEXT) WHERE target = CAST(?1 AS TEXT) AND (action LIKE 'user.%' OR action LIKE 'device.%' OR action IN ('forum.warn', 'forum.mute', 'forum.unmute'))",
        params![from, to],
    )
    .map_err(|e| e.to_string())?;
    // The run's highest number (one given by hand above the count is not part of the run).
    let count: i64 = tx.query_row("SELECT seq FROM sqlite_sequence WHERE name = 'users'", [], |r| r.get(0)).optional().map_err(|e| e.to_string())?.unwrap_or(0);
    tx.execute("UPDATE sqlite_sequence SET seq = (SELECT COALESCE(MAX(uid), 0) FROM users WHERE uid <= ?1) WHERE name = 'users'", params![count]).map_err(|e| e.to_string())?;
    let broken: i64 = tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if broken != 0 {
        return Err(format!("{broken} broken links, nothing changed"));
    }
    tx.commit().map_err(|e| e.to_string())
}

/// The number a new account gets: the next of the run (the count's), passing over one given by hand (272). A number
/// freed by an account deleted is not given again (a ban of it would fall on someone new).
fn next_uid(c: &Connection) -> rusqlite::Result<i64> {
    let mut uid = c.query_row("SELECT seq FROM sqlite_sequence WHERE name = 'users'", [], |r| r.get::<_, i64>(0)).optional()?.unwrap_or(0) + 1;
    while c.query_row("SELECT 1 FROM users WHERE uid = ?1", params![uid], |_| Ok(())).optional()?.is_some() {
        uid += 1;
    }
    Ok(uid)
}

// ----------------------------------------------------------------------------------------------- serving

async fn serve(path: String) {
    let listen = env::var("CURSED_LISTEN").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let app: Shared = Arc::new(App {
        db: Mutex::new(open(&path)),
        tries: Mutex::new(HashMap::new()),
        secure: env::var("CURSED_SECURE_COOKIE").map_or(true, |v| v != "0"),
        origin: env::var("CURSED_ORIGIN").unwrap_or_else(|_| "https://cursedclient.xyz".into()),
        dummy: hash_password(&random_hex(16)),
        market: market::Market::new(
            PathBuf::from(env::var("CURSED_MEDIA").unwrap_or_else(|_| "/var/lib/cursed-media".into())),
            PathBuf::from(&path).parent().map_or_else(|| PathBuf::from("uploads"), |d| d.join("uploads")),
        ),
        pace: forum::Pace::new(),
        blocklist: blocklist::Signature::open(&path),
        dist: dist::Dist::open(&path, dist_root()),
        db_path: path.clone(),
        net_key: beta::net_key(&path),
    });

    // Avatars whose pictures are gone from the media folder are not handed out (the login's head stands in).
    {
        let c = app.db.lock().unwrap_or_else(|e| e.into_inner());
        let gone = avatar::forget_missing(&c, &app.market.media);
        if gone > 0 {
            println!("avatars without their pictures, not shown: {gone}");
        }
    }

    // Sessions nobody used for their whole life go once a day.
    let sweeper = app.clone();
    tokio::spawn(async move {
        loop {
            let at = now();
            db(&sweeper, move |c| {
                c.execute(
                    "DELETE FROM sessions WHERE (kind = 'web' AND seen < ?1) OR (kind = 'loader' AND seen < ?2)",
                    params![at - WEB_DAYS * 86_400, at - LOADER_DAYS * 86_400],
                )
            })
            .await
            .ok();
            let s = sweeper.clone();
            db(&sweeper, move |c| market::sweep(c, &s.market)).await;
            db(&sweeper, |c| people::sweep(c)).await;
            tokio::time::sleep(Duration::from_secs(86_400)).await;
        }
    });

    tokio::spawn(market::checker(app.clone()));
    tokio::spawn(blocklist::refresher(app.clone()));
    tokio::spawn(live::counter(app.clone()));
    tokio::spawn(pay::recover(app.clone()));
    forum2::watch(app.clone());

    let accounts = Router::new()
        .route("/api/register", post(register))
        .route("/api/login", post(signin))
        .route("/api/token", post(loader_token))
        .route("/api/logout", post(logout))
        .route("/api/password", post(password))
        .route("/api/email", post(change_email))
        .route("/api/email/confirm", post(mail::confirm))
        .route("/api/email/confirm/send", post(mail::confirm_send))
        .route("/api/reset/ask", post(mail::reset_ask))
        .route("/api/reset/check", post(mail::reset_check))
        .route("/api/reset/set", post(mail::reset_set))
        .route("/api/me", get(me))
        .layer(DefaultBodyLimit::max(4096));
    let routes = accounts
        .merge(market::routes())
        .merge(forum::routes())
        .merge(avatar::routes())
        .merge(blocklist::routes())
        .merge(beta::routes())
        .merge(dist::routes())
        .merge(cosmetics::routes())
        .merge(looks::routes())
        .merge(changes::routes())
        .merge(tops::routes())
        .merge(marks::routes())
        .merge(people::routes())
        .merge(live::routes())
        .merge(notify::routes())
        .merge(friends::routes())
        .merge(chat::routes())
        .merge(configs::routes())
        .merge(forum2::routes())
        .merge(tickets::routes())
        .merge(admin::routes())
        .merge(party::routes())
        .merge(refs::routes())
        .merge(fps::routes())
        .merge(pay::routes())
        .merge(recode::routes())
        .layer(axum::middleware::map_response(|mut res: Response| async move {
            res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            res
        }))
        .with_state(app);

    let listener = tokio::net::TcpListener::bind(&listen).await.expect("listen");
    eprintln!("cursed-api on {listen}, database {path}");
    axum::serve(listener, routes)
        .with_graceful_shutdown(async {
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
        })
        .await
        .expect("serve");
}

/// Where the client's files are kept (dist.rs).
fn dist_root() -> PathBuf {
    PathBuf::from(env::var("CURSED_DIST").unwrap_or_else(|_| "/var/lib/cursed-dist".into()))
}

/// An account by its login, for the commands (the program stops if there is none).
fn account(c: &Connection, login: &str) -> (i64, String) {
    beta::uid_of(c, login).unwrap_or_else(|| {
        eprintln!("no account {login}");
        std::process::exit(1);
    })
}

fn say(v: serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
}

/// A console command that cannot go on: why, and out.
fn die(e: &str) -> ! {
    eprintln!("{e}");
    std::process::exit(1)
}

fn when(t: Option<i64>) -> String {
    match t {
        None => "never".into(),
        Some(t) => {
            let d = t - now();
            if d > 0 {
                format!("in {}h{:02}m", d / 3600, d % 3600 / 60)
            } else {
                format!("{}h{:02}m ago", -d / 3600, -d % 3600 / 60)
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let path = env::var("CURSED_DB").unwrap_or_else(|_| "/var/lib/cursed/cursed.db".into());
    match args.first().map(String::as_str) {
        None | Some("serve") => tokio::runtime::Runtime::new().expect("a runtime").block_on(serve(path)),
        // nets-import <file>: lines "<uid> <ip>", the networks of accounts made before they were kept (from nginx's log).
        Some("nets-import") if args.len() == 2 => {
            let c = open(&path);
            let key = beta::net_key(&path);
            let (mut kept, mut skipped) = (0, 0);
            for line in std::fs::read_to_string(&args[1]).expect("read the file").lines() {
                let mut it = line.split_whitespace();
                let row = it.next().and_then(|u| u.parse::<i64>().ok()).zip(it.next().and_then(|a| a.parse::<std::net::IpAddr>().ok()));
                let Some((uid, ip)) = row else {
                    skipped += 1;
                    continue;
                };
                // Only where nothing is kept yet: a registration's own record is the truer one.
                match c.execute(
                    "INSERT OR IGNORE INTO reg_nets (uid, net, at) SELECT uid, ?2, created FROM users WHERE uid = ?1",
                    params![uid, beta::net_hash(&key, ip)],
                ) {
                    Ok(1) => kept += 1,
                    _ => skipped += 1,
                }
            }
            println!("{kept} kept, {skipped} skipped");
        }
        // A one-time key: https://cursedclient.xyz/#/register/<key> registers one account, even while registration is closed.
        Some("key") => {
            let c = open(&path);
            let key = random_hex(16);
            c.execute("INSERT INTO keys (hash, made) VALUES (?1, ?2)", params![digest(&key), now()]).expect("keep the key");
            println!("{key}");
        }
        // registration [open|closed] [--limit N] [--per-ip N]
        Some("registration") => {
            let c = open(&path);
            let state = args.get(1).map(String::as_str).filter(|v| matches!(*v, "open" | "closed"));
            let limit = args.iter().position(|a| a == "--limit").and_then(|i| args.get(i + 1)).map(|n| n.parse::<i64>().expect("a number"));
            let per_ip = args.iter().position(|a| a == "--per-ip").and_then(|i| args.get(i + 1)).map(|n| n.parse::<i64>().expect("a number"));
            if let Some(Err(e)) = per_ip.map(|n| beta::set_per_ip(&c, n)) {
                eprintln!("{e}");
                std::process::exit(1);
            }
            match beta::set_registration(&c, state, limit) {
                Ok(v) => println!(
                    "registration {}, {} of {} places taken{}, {} per IP",
                    v["state"].as_str().unwrap_or(""),
                    v["taken"],
                    v["limit"],
                    if v["full"] == true { " (full)" } else { "" },
                    if v["per_ip"] == 0 { "any number".to_string() } else { v["per_ip"].to_string() }
                ),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        Some("testers") => {
            let c = open(&path);
            let v = beta::testers_json(&c);
            println!("registration {}, {} of {} places taken", v["registration"].as_str().unwrap_or(""), v["taken"], v["limit"]);
            for t in v["testers"].as_array().into_iter().flatten() {
                let ban = match &t["ban"] {
                    serde_json::Value::Null => String::new(),
                    b => format!("  BANNED ({}, ends {})", b["reason"].as_str().unwrap_or(""), when(b["until"].as_i64())),
                };
                let device = if t["device"].is_null() { "no computer" } else { "computer bound" };
                println!("{:>4}  {:<16}  {}  seen {}  crashes {}{ban}", t["uid"], t["login"].as_str().unwrap_or(""), device, when(t["seen"].as_i64()), t["crashes"]);
            }
        }
        Some("tester") if args.len() == 2 => {
            let c = open(&path);
            let (uid, _) = account(&c, &args[1]);
            say(beta::tester_json(&c, uid, true).unwrap_or_default());
        }
        // ban <login> [--for 7d|12h|30m] <reason...>
        Some("ban") if args.len() >= 3 => {
            let mut c = open(&path);
            let (uid, login) = account(&c, &args[1]);
            let mut rest: Vec<String> = args[2..].to_vec();
            let mut until = None;
            if rest.first().map(String::as_str) == Some("--for") && rest.len() >= 2 {
                let length = beta::parse_length(&rest[1]).unwrap_or_else(|| {
                    eprintln!("a length like 7d, 12h or 30m");
                    std::process::exit(1);
                });
                until = Some(now() + length);
                rest.drain(..2);
            }
            let reason = rest.join(" ");
            if reason.trim().is_empty() {
                eprintln!("a reason: it is shown to the player");
                std::process::exit(1);
            }
            match beta::ban(&mut c, uid, reason.trim(), until, "command") {
                Ok(_) => println!("{login} banned {}: {}", until.map_or("for good".to_string(), |u| format!("until {}", when(Some(u)))), reason.trim()),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        Some("unban") if args.len() == 2 => {
            let c = open(&path);
            let (uid, login) = account(&c, &args[1]);
            println!("{login}: {} ban(s) lifted", beta::unban(&c, uid, "command"));
        }
        Some("hwid-reset") if args.len() == 2 => {
            let c = open(&path);
            let (uid, login) = account(&c, &args[1]);
            let _ = beta::free_device(&c, uid, false);
            println!("{login}: the computer freed, the next one it starts on is its own");
        }
        Some("delete") if args.len() == 2 => {
            let mut c = open(&path);
            let (uid, login) = account(&c, &args[1]);
            match beta::delete(&mut c, uid) {
                Ok(()) => println!("{login} deleted, the place is free"),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        Some("crashes") => {
            let c = open(&path);
            let only = args.get(1).map(|l| account(&c, l).0);
            let mut st = c.prepare("SELECT c.at, u.login, c.build, c.source, c.file, c.size FROM crashes c LEFT JOIN users u ON u.uid = c.uid WHERE ?1 IS NULL OR c.uid = ?1 ORDER BY c.id DESC LIMIT 50").expect("query");
            let rows = st.query_map(params![only], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, i64>(5)?))).expect("rows");
            let dir = beta::crashes_dir(&path);
            for (at, login, build, source, file, size) in rows.flatten() {
                println!("{}  {:<16}  build {:<4} {:<7} {:>7} B  {}", when(Some(at)), login.unwrap_or_default(), build.map_or("?".into(), |b| b.to_string()), source, size, dir.join(file).display());
            }
        }
        // release publish <incoming name> [channel] | release list [channel] | release rollback [channel]
        Some("release") if args.len() >= 2 => {
            let c = open(&path);
            let d = dist::Dist::open(&path, dist_root());
            match args[1].as_str() {
                "publish" if args.len() >= 3 => {
                    let channel = args.get(3).map_or("beta", String::as_str);
                    match dist::publish(&c, &d, &args[2], channel, "command") {
                        Ok(b) => println!("build {b} is the current one of {channel}"),
                        Err(e) => {
                            eprintln!("not published: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                "list" => {
                    let channel = args.get(2).map_or("beta", String::as_str);
                    for r in dist::list(&c, channel) {
                        println!(
                            "{} build {:<4} {:<18} {}  {} files, {:.1} MB  by {}",
                            if r["current"] == true { "*" } else { " " },
                            r["build"],
                            r["version"].as_str().unwrap_or(""),
                            when(r["at"].as_i64()),
                            r["files"],
                            r["bytes"].as_f64().unwrap_or(0.0) / 1048576.0,
                            r["by"].as_str().unwrap_or("")
                        );
                    }
                }
                "rollback" => {
                    let channel = args.get(2).map_or("beta", String::as_str);
                    match dist::rollback(&c, channel) {
                        Ok(b) => println!("{channel} is back on build {b}"),
                        Err(e) => {
                            eprintln!("{e}");
                            std::process::exit(1);
                        }
                    }
                }
                _ => {
                    eprintln!("release publish <name> [channel] | list [channel] | rollback [channel]");
                    std::process::exit(2);
                }
            }
        }
        // dist missing (hashes on stdin, the ones the store lacks out) | dist assets <folder> | dist-key
        Some("dist") if args.len() >= 2 => {
            let d = dist::Dist::open(&path, dist_root());
            match args[1].as_str() {
                "missing" => {
                    let mut text = String::new();
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).expect("read the hashes");
                    let hashes: Vec<String> = text.split_whitespace().map(str::to_string).collect();
                    for h in dist::missing(&d, &hashes) {
                        println!("{h}");
                    }
                }
                "assets" if args.len() == 3 => match dist::import_assets(&d, std::path::Path::new(&args[2])) {
                    Ok((new, had)) => println!("assets: {new} new, {had} here already"),
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                },
                _ => {
                    eprintln!("dist missing | dist assets <folder>");
                    std::process::exit(2);
                }
            }
        }
        // protect — is the master whole? A release asks this before it lets go (Release.java): a build
        // that cannot cut a pair must not go out, and the answer is a line and an exit code, not a page.
        Some("protect") => {
            match crate::protect::status(&dist_root()) {
                Ok(line) => println!("{line}"),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        Some("dist-key") => {
            open(&path);
            println!("{}", dist::Dist::open(&path, dist_root()).public_hex());
        }
        Some("users") => {
            let c = open(&path);
            let mut q = c.prepare("SELECT uid, login, email, created FROM users ORDER BY uid").expect("query");
            let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))).expect("rows");
            for row in rows.flatten() {
                println!("{}\t{}\t{}\t{}", row.0, row.1, row.2, row.3);
            }
        }
        // A whole copy of the database, taken safely while it runs.
        Some("renumber") if args.len() == 3 => {
            let (from, to): (i64, i64) = (args[1].parse().expect("a UID"), args[2].parse().expect("a UID"));
            let mut c = open(&path);
            match renumber(&mut c, from, to) {
                Ok(()) => {
                    people::sweep(&c);
                    println!("UID {from} -> {to} (restart cursed-api: what it keeps in memory is by number)")
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        // A member's text UID, shown instead of the number ("-": the number again).
        Some("uidtext") if args.len() == 3 => {
            let c = open(&path);
            let Some(uid) = people::uid_of(&c, &args[1]) else {
                eprintln!("no such login");
                std::process::exit(1);
            };
            let text = if args[2] == "-" { "" } else { args[2].as_str() };
            match people::set_text_uid(&c, uid, text) {
                Ok(()) => {
                    people::audit(&c, OWNER, "user.uid_text", &uid.to_string(), json!({ "text": text, "by": "cli" }));
                    println!("UID {uid} is shown as {}", if text.is_empty() { uid.to_string() } else { text.to_string() })
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        // The bot's key for the blocklist (the one before stops working); only its hash is kept.
        Some("service-key") => println!("{}", blocklist::service_key(&open(&path))),
        // The public half of the blocklist's signing key, for the client.
        Some("blocklist-key") => {
            open(&path);
            println!("{}", blocklist::public_key(&path));
        }
        // The partners' servers pinned at the top of every player's list (they go with the signed blocklist).
        Some("pin") if args.len() >= 3 => match blocklist::pin(&open(&path), &args[1], &args[2..].join(" ")) {
            Ok(()) => println!("pinned {}", args[1]),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        Some("unpin") if args.len() == 2 => match blocklist::unpin(&open(&path), &args[1]) {
            Ok(()) => println!("unpinned {}", args[1]),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        // The mods' rules (marks.rs), in the signed blocklist: `mod-rule <name> cheat <ids> [slugs]`, `mod-rule <name>
        // on <Server,Server> <ids> [slugs]` (ids and slugs by commas, "-" for none), `mod-rule <name> off`, `mod-rules`.
        Some("mod-rule") if args.len() == 3 && args[2] == "off" => match marks::remove_rule(&open(&path), &args[1]) {
            Ok(()) => println!("{} taken off", args[1]),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        Some("mod-rule") if args.len() >= 4 && (args[2] == "cheat" || (args[2] == "on" && args.len() >= 5)) => {
            let (servers, rest) = if args[2] == "on" { (args[3].as_str(), &args[4..]) } else { ("", &args[3..]) };
            let none = |s: Option<&String>| s.map(|s| if s == "-" { String::new() } else { s.clone() }).unwrap_or_default();
            match marks::set_rule(&open(&path), &args[1], &none(rest.first()), &none(rest.get(1)), servers, "cli") {
                Ok(()) => println!("{} set", args[1]),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        Some("mod-rules") => {
            for r in marks::rules(&open(&path)) {
                let words = |v: &serde_json::Value| v.as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(",")).unwrap_or_default();
                let kind = if r["cheat"] == true { "cheat".to_string() } else { format!("on {}", words(&r["on"])) };
                println!("{}\t{}\tids {}\tslugs {}", r["name"].as_str().unwrap_or(""), kind, words(&r["ids"]), words(&r["slugs"]));
            }
        }
        Some("pinned") => {
            for p in blocklist::pinned(&open(&path), true) {
                println!(
                    "{}\t{}\tsince {}{}",
                    p["address"].as_str().unwrap_or(""),
                    p["name"].as_str().unwrap_or(""),
                    when(p["added"].as_i64()),
                    p["gone"].as_i64().map_or(String::new(), |g| format!("\tunpinned {}", when(Some(g))))
                );
            }
        }
        // Roles and badges by hand: `role <login> <role> [off]`, `badge <login> <badge> [level|off]`.
        Some("role") if args.len() >= 3 => {
            let c = open(&path);
            match people::set_role(&c, -1, &args[1], &args[2], args.get(3).map_or(true, |a| a != "off")) {
                Ok(()) => println!("{} {} {}", args[1], if args.get(3).is_some_and(|a| a == "off") { "loses" } else { "has" }, args[2]),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        Some("badge") if args.len() >= 3 => {
            let c = open(&path);
            let off = args.get(3).is_some_and(|a| a == "off");
            let level = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(1);
            match people::set_badge(&c, -1, &args[1], &args[2], level, !off) {
                Ok(()) => println!("{} {} {}", args[1], if off { "loses" } else { "has" }, args[2]),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        // Recode by hand (hers and ours from here; her word, 19.10.2026): `recode keys <days|forever> <count> [note]` —
        // a batch, its codes one a line; `recode of <login>`; `recode give <login> <days|forever> [note]`; `recode take
        // <login>` — what was given by hand taken back, the keys bought stay.
        Some("recode") if args.len() >= 2 => {
            let mut c = open(&path);
            let term = |s: &str| recode::term_typed(s).unwrap_or_else(|| die("a term: 1, 2, 7, 14, 30, 90, 180 or forever"));
            match args[1].as_str() {
                "keys" if args.len() >= 4 => {
                    let days = term(&args[2]);
                    let count = args[3].parse().unwrap_or_else(|_| die("how many: 1 to 500"));
                    let note = market::clean_line(&args[4..].join(" "), 80);
                    match recode::make_keys(&mut c, -1, days, count, &note) {
                        Ok((batch, codes, _)) => {
                            eprintln!("batch {batch}: {} keys of {}", codes.len(), if days == 0 { "forever".into() } else { format!("{days}d") });
                            for code in codes {
                                println!("{code}");
                            }
                        }
                        Err(e) => die(e),
                    }
                }
                // beta keys (her word, 20.10): whoever activates one is a beta tester, the beta closed or not
                "beta-keys" if args.len() >= 4 => {
                    let days = term(&args[2]);
                    let count = args[3].parse().unwrap_or_else(|_| die("how many: 1 to 500"));
                    let note = market::clean_line(&args[4..].join(" "), 80);
                    match recode::make_beta_keys(&mut c, -1, days, count, &note) {
                        Ok((batch, codes, _)) => {
                            eprintln!("batch {batch}: {} beta keys of {}", codes.len(), if days == 0 { "forever".into() } else { format!("{days}d") });
                            for code in codes {
                                println!("{code}");
                            }
                        }
                        Err(e) => die(e),
                    }
                }
                "of" if args.len() == 3 => {
                    let (uid, login) = account(&c, &args[2]);
                    let mut v = recode::card(&c, uid, None);
                    v["login"] = json!(login);
                    v["until_in"] = json!(v["until"].as_i64().map(|t| when(Some(t))));
                    say(v);
                }
                "give" if args.len() >= 4 => {
                    let (uid, login) = account(&c, &args[2]);
                    let days = term(&args[3]);
                    let note = market::clean_line(&args[4..].join(" "), 60);
                    match recode::give(&mut c, -1, uid, days, &note) {
                        Ok(s) if s.forever => println!("{login}: Recode forever"),
                        Ok(s) => println!("{login}: Recode until {} ({})", s.until, when(Some(s.until))),
                        Err(e) => die(e),
                    }
                }
                "take" if args.len() == 3 => {
                    let (uid, login) = account(&c, &args[2]);
                    match recode::take(&c, -1, uid, None) {
                        Ok((n, s)) => println!("{login}: {n} given keys taken back; Recode {}", if s.forever { "forever".into() } else if s.active(now()) { format!("until {}", when(Some(s.until))) } else { "off".into() }),
                        Err(e) => die(e),
                    }
                }
                _ => die("recode keys|beta-keys <days|forever> <count> [note] | of <login> | give <login> <days|forever> [note] | take <login>"),
            }
        }
        Some("backup") if args.len() == 2 => {
            let c = open(&path);
            c.execute("VACUUM INTO ?1", params![args[1]]).expect("copy");
        }
        _ => {
            eprintln!(
                "cursed-api [serve | key | registration [open|closed] [--limit N] | users | testers | tester <login> | ban <login> [--for 7d] <reason> | \
                 unban <login> | hwid-reset <login> | delete <login> | crashes [login] | release publish|list|rollback | protect | dist missing|assets | \
                 dist-key | renumber <from> <to> | backup <file> | service-key | blocklist-key | pin <address> <name> | unpin <address> | pinned | role <login> <role> [off] | badge <login> <badge> [level|off] | \
                 recode keys|beta-keys <days|forever> <count> [note] | recode of|give|take <login> [days|forever]]"
            );
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logins() {
        assert!(login_ok("Lunuca") && login_ok("a_1") && login_ok("abcdefghijklmnop"));
        assert!(!login_ok("ab") && !login_ok("abcdefghijklmnopq") && !login_ok("lu nuca") && !login_ok("луна") && !login_ok("a-b"));
    }

    #[test]
    fn emails() {
        assert!(email_ok("a@b.co") && email_ok("first.last+tag@mail.example.org"));
        for bad in ["", "a", "a@", "@b.co", "a@b", "a@.co", "a@b.", "a b@c.de", "a@b@c.de"] {
            assert!(!email_ok(bad), "{bad}");
        }
    }

    #[test]
    fn passwords() {
        assert!(password_ok("0123456789") && !password_ok("012345678") && !password_ok(&"x".repeat(129)));
        let h = hash_password("correct horse");
        assert!(h.starts_with("$argon2id$") && password_matches("correct horse", &h) && !password_matches("wrong horse", &h));
    }

    #[test]
    fn her_uid_moves_to_zero_and_the_count_goes_on_from_one() {
        let path = std::env::temp_dir().join(format!("cursed-test-{}.db", random_hex(4)));
        let mut c = open(path.to_str().unwrap());
        let add = |c: &Connection, login: &str| {
            c.execute("INSERT INTO users (login, login_key, email, email_key, pass, created) VALUES (?1, lower(?1), ?1 || '@e.st', lower(?1) || '@e.st', 'x', 0)", params![login]).unwrap();
            c.last_insert_rowid()
        };
        assert_eq!(add(&c, "Lunuca"), 1);
        let token = new_session(&c, 1, "web").unwrap();
        c.execute("INSERT INTO keys (hash, made, used_by) VALUES (x'00', 0, 1)", []).unwrap();
        assert!(renumber(&mut c, 1, 0).is_ok());
        assert_eq!(who(&c, &token).map(|w| w.uid), Some(0));
        assert_eq!(c.query_row("SELECT used_by FROM keys", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert_eq!(add(&c, "Second"), 1);
        assert_eq!(add(&c, "Third"), 2);
        assert!(renumber(&mut c, 2, 1).is_err(), "a taken UID");
        assert!(renumber(&mut c, 7, 9).is_err(), "no such UID");
        // One moved to a number of its own far up, another into the place it left: the run goes on after its last.
        let fourth = add(&c, "Fourth");
        assert_eq!(fourth, 3);
        c.execute("INSERT INTO outfits (uid, mc_uuid, mc_name, worn, updated) VALUES (1, 'u', 'Second', '{}', 0)", []).unwrap();
        assert!(renumber(&mut c, 1, 272).is_ok());
        assert!(renumber(&mut c, 3, 1).is_ok());
        assert_eq!(c.query_row("SELECT uid FROM outfits", [], |r| r.get::<_, i64>(0)).unwrap(), 272);
        // The run's last moved down: its number is free (all of it went along) and comes next, none skipped.
        assert_eq!(next_uid(&c).unwrap(), 3);
        c.execute("UPDATE sqlite_sequence SET seq = 271 WHERE name = 'users'", []).unwrap();
        assert_eq!(next_uid(&c).unwrap(), 273, "272 is passed over");
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    #[test]
    fn tokens_come_in_both_ways() {
        let t = random_hex(32);
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_str(&format!("x=1; cursed_session={t}")).unwrap());
        assert_eq!(presented(&h), Some((t.clone(), true)));
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {t}")).unwrap());
        assert_eq!(presented(&h), Some((t, false)));
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_static("cursed_session=nothex"));
        assert_eq!(presented(&h), None);
    }

    #[test]
    fn revoked_expired_and_banned_sessions_cannot_keep_access() {
        let path = std::env::temp_dir().join(format!("cursed-session-test-{}.db", random_hex(4)));
        let c = open(path.to_str().unwrap());
        c.execute("INSERT INTO users(uid,login,login_key,email,email_key,pass,created) VALUES(900001,'SessionTest','sessiontest','s@e.st','s@e.st','x',0)", []).unwrap();
        for kind in ["web", "loader", "client"] {
            let t = new_session(&c, 900001, kind).unwrap();
            let h = digest(&t);
            assert!(session_alive(&c, &h));
            c.execute("UPDATE sessions SET seen=0 WHERE hash=?1", params![h]).unwrap();
            assert!(!session_alive(&c, &h));
            assert!(who(&c, &t).is_none());
        }
        let t = new_session(&c, 900001, "web").unwrap();
        c.execute("DELETE FROM sessions WHERE uid=900001", []).unwrap();
        assert!(!session_alive(&c, &digest(&t)));
        let t = new_session(&c, 900001, "web").unwrap();
        // A session issued by a concurrent login after ban cleanup must still fail.
        c.execute("INSERT INTO bans(uid,login,reason,at,by) VALUES(900001,'SessionTest','test',?1,'test')", params![now()]).unwrap();
        assert!(!session_alive(&c, &digest(&t)));
        assert!(who(&c, &t).is_none());
        c.execute("UPDATE bans SET lifted=?1 WHERE uid=900001", params![now()]).unwrap();
        let t = new_session(&c, 900001, "web").unwrap();
        assert!(session_alive(&c, &digest(&t)));
        assert!(who(&c, &t).is_some());
        drop(c);
        for ext in ["", "-wal", "-shm"] { let _ = std::fs::remove_file(format!("{}{ext}", path.display())); }
    }
}
