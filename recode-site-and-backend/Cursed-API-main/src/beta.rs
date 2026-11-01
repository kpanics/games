//! The beta: who plays it, on which computer, and who may not any more (her decisions, 30.09.2026).
//!
//! - Registration takes at most `registration_limit` people (15; hers is not counted). Past that the site says the
//!   beta is full; a place freed (an account deleted) is open again. A banned account keeps its place.
//! - One network (IP) registers at most `registration_per_ip` accounts (2, her word 05.10.2026, after a friend made ten
//!   with curl in a minute). The address itself is not kept: only its hash with a key of the server's own (`net.key`
//!   next to the database). An account deleted frees its network's place; one made with her key is not stopped.
//! - Everyone who registers is a tester (`roles`). Only a tester, and she, gets the client (dist.rs).
//! - A ban is for good or until a date, always with its reason, which the banned one is shown. It ends every session
//!   of the account at once, it is asked at every sign-in and every start of the game, and it closes the computer the
//!   account was bound to as well: no new account plays from there either.
//! - One account, one computer; one account on a computer. The computer is known by what the loader sends: hashes of
//!   a few of its parts (the board, the processor, the system disk, Windows' own id…), never the parts' own numbers.
//!   It is the same computer while most of the parts it shares with the one bound still match: a new disk or video
//!   card does not make it another, Windows installed again changes one part only. She frees an account's computer
//!   with a command; the player may do it from the loader, once a day.
//! - The game asks too: the loader is given a ticket for one start of the game and passes it on, and the client shows
//!   it here (the same checks, and the build it is: only the current one starts). Its answer is a session of its own.
//! - Crash reports come in from the loader and the game, a few a day per account, kept as text for her. A note
//!   of a hand put on the game's own memory is another kind (`touch`): one confirmed finding is a ban (18.10).
//!
//! She is not a tester and nothing of this stops her: her account is not banned, not bound, never counted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::market::{clean_line, run};
use crate::{db, digest, fail, new_session, now, presented, random_hex, verify, who, Shared, SignIn, OWNER};

/// Registration's limit until she sets another.
pub const LIMIT: i64 = 15;
/// Accounts one network may register until she sets another (0: any number).
const PER_IP: i64 = 2;
/// How long the loader's ticket for one start of the game lives.
pub const TICKET_SECONDS: i64 = 600;
/// How long a start of the loader lets its account download (the whole of a first install fits well within it).
pub const ACCESS_SECONDS: i64 = 6 * 3600;
/// How often a player may free his computer himself.
pub const RESET_EVERY: i64 = 86_400;
/// Crash reports a day per account, and how big one may be.
const CRASHES_A_DAY: i64 = 30;
const CRASH_MOST: usize = 2 * 1024 * 1024;

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS roles (
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             role TEXT NOT NULL,
             since INTEGER NOT NULL,
             PRIMARY KEY (uid, role));
         CREATE TABLE IF NOT EXISTS bans (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             uid INTEGER NOT NULL,
             login TEXT NOT NULL,
             reason TEXT NOT NULL,
             until INTEGER,
             at INTEGER NOT NULL,
             by TEXT NOT NULL,
             lifted INTEGER,
             lifted_by TEXT);
         CREATE INDEX IF NOT EXISTS bans_uid ON bans(uid);
         CREATE TABLE IF NOT EXISTS banned_devices (
             ban INTEGER NOT NULL REFERENCES bans(id) ON DELETE CASCADE,
             parts TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS devices (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             parts TEXT NOT NULL,
             bound INTEGER NOT NULL,
             seen INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS device_resets (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS access (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             until INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS tickets (
             hash BLOB PRIMARY KEY,
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             build INTEGER NOT NULL,
             made INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS crashes (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             uid INTEGER NOT NULL,
             at INTEGER NOT NULL,
             build INTEGER,
             source TEXT NOT NULL,
             file TEXT NOT NULL,
             size INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS reg_nets (
             uid INTEGER PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
             net BLOB NOT NULL,
             at INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS reg_nets_net ON reg_nets(net);
         INSERT OR IGNORE INTO settings VALUES ('registration_limit', '15');",
    )
    .expect("make the beta's tables");
    // The accounts made before the beta are its testers too (once: a role taken away later stays away).
    let seeded = c.query_row("SELECT 1 FROM settings WHERE name = 'roles_seeded'", [], |_| Ok(())).optional().ok().flatten().is_some();
    if !seeded {
        c.execute_batch(
            "INSERT OR IGNORE INTO roles (uid, role, since) SELECT uid, 'beta', created FROM users WHERE uid != 0;
             INSERT OR IGNORE INTO settings VALUES ('roles_seeded', '1');",
        )
        .expect("seed the roles");
    }
}

// ------------------------------------------------------------------------------------------------ registration

pub fn limit(c: &Connection) -> i64 {
    c.query_row("SELECT value FROM settings WHERE name = 'registration_limit'", [], |r| r.get::<_, String>(0)).ok().and_then(|v| v.parse().ok()).unwrap_or(LIMIT)
}

/// The places taken: every account but hers (a banned one too).
pub fn taken(c: &Connection) -> i64 {
    c.query_row("SELECT COUNT(*) FROM users WHERE uid NOT IN (?1, ?2)", params![crate::OWNERS[0], crate::OWNERS[1]], |r| r.get(0)).unwrap_or(0)
}

pub fn full(c: &Connection) -> bool {
    taken(c) >= limit(c)
}

pub fn per_ip(c: &Connection) -> i64 {
    c.query_row("SELECT value FROM settings WHERE name = 'registration_per_ip'", [], |r| r.get::<_, String>(0)).ok().and_then(|v| v.parse().ok()).unwrap_or(PER_IP)
}

pub fn set_per_ip(c: &Connection, n: i64) -> Result<(), String> {
    if !(0..=1000).contains(&n) {
        return Err("accounts per IP: from 0 (any number)".into());
    }
    c.execute("INSERT INTO settings (name, value) VALUES ('registration_per_ip', ?1) ON CONFLICT(name) DO UPDATE SET value = excluded.value", params![n.to_string()])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The key the networks are hashed with: made once, kept beside the database, readable by nobody else. Without it a
/// stolen database does not tell the addresses (all of IPv4 is quickly tried against a hash with no key).
pub fn net_key(db_path: &str) -> [u8; 32] {
    let path = std::path::Path::new(db_path).parent().map_or_else(|| PathBuf::from("net.key"), |d| d.join("net.key"));
    match std::fs::read(&path) {
        Ok(bytes) if bytes.len() == 32 => bytes.try_into().expect("32 bytes"),
        _ => {
            let mut key = [0u8; 32];
            rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut key);
            crate::blocklist::write_private(&path, &key);
            key
        }
    }
}

/// A network as kept: an IPv4 address whole, an IPv6 one by its /64 (one home's, which picks the rest itself).
pub fn net_hash(key: &[u8; 32], ip: std::net::IpAddr) -> Vec<u8> {
    use sha2::Digest;
    let bytes = match ip {
        std::net::IpAddr::V4(a) => a.octets().to_vec(),
        std::net::IpAddr::V6(a) => match a.to_ipv4_mapped() {
            Some(v4) => v4.octets().to_vec(),
            None => a.octets()[..8].to_vec(),
        },
    };
    let mut d = sha2::Sha256::new();
    d.update(key);
    d.update(&bytes);
    d.finalize().to_vec()
}

/// The network a request came from, as nginx saw it: it sets X-Real-IP itself, over whatever the request carried, and
/// nothing else reaches the app (it listens on 127.0.0.1). None when called past nginx (the tests, the staging).
pub fn net_of(h: &HeaderMap, key: &[u8; 32]) -> Option<Vec<u8>> {
    let ip = h.get("x-real-ip")?.to_str().ok()?.trim().parse().ok()?;
    Some(net_hash(key, ip))
}

/// Has this network registered as many accounts as it may?
pub fn net_full(c: &Connection, net: &[u8]) -> bool {
    let most = per_ip(c);
    most > 0 && c.query_row("SELECT COUNT(*) FROM reg_nets WHERE net = ?1", params![net], |r| r.get::<_, i64>(0)).unwrap_or(0) >= most
}

pub fn remember_net(c: &Connection, uid: i64, net: &[u8]) -> rusqlite::Result<()> {
    c.execute("INSERT OR REPLACE INTO reg_nets (uid, net, at) VALUES (?1, ?2, ?3)", params![uid, net, now()]).map(|_| ())
}

/// A new account's role (in the transaction that makes it).
pub fn made_tester(c: &Connection, uid: i64) -> rusqlite::Result<()> {
    c.execute("INSERT OR IGNORE INTO roles (uid, role, since) VALUES (?1, 'beta', ?2)", params![uid, now()]).map(|_| ())
}

pub fn is_tester(c: &Connection, uid: i64) -> bool {
    crate::is_owner(uid) || c.query_row("SELECT 1 FROM roles WHERE uid = ?1 AND role = 'beta'", params![uid], |_| Ok(())).optional().ok().flatten().is_some()
}

// ------------------------------------------------------------------------------------------------ bans

#[derive(Clone, Debug, PartialEq)]
pub struct Ban {
    pub id: i64,
    pub reason: String,
    /// When it ends; None: never.
    pub until: Option<i64>,
    /// Found by the computer, not by the account (another account on a banned computer).
    pub device: bool,
}

impl Ban {
    pub fn json(&self) -> Value {
        json!({ "error": "banned", "reason": self.reason, "until": self.until, "device": self.device })
    }
}

const ACTIVE: &str = "lifted IS NULL AND (until IS NULL OR until > ?2)";

/// The ban an account is under now, the latest if more than one.
pub fn ban_of(c: &Connection, uid: i64) -> Option<Ban> {
    if crate::is_owner(uid) {
        return None;
    }
    c.query_row(&format!("SELECT id, reason, until FROM bans WHERE uid = ?1 AND {ACTIVE} ORDER BY id DESC LIMIT 1"), params![uid, now()], |r| {
        Ok(Ban { id: r.get(0)?, reason: r.get(1)?, until: r.get(2)?, device: false })
    })
    .optional()
    .ok()
    .flatten()
}

/// The ban a computer is under now: one whose account was bound to a computer these parts are.
pub fn device_ban(c: &Connection, parts: &Parts) -> Option<Ban> {
    let mut st = c.prepare(&format!("SELECT b.id, b.reason, b.until, d.parts FROM bans b JOIN banned_devices d ON d.ban = b.id WHERE b.uid NOT IN (?1, ?3) AND b.{ACTIVE} ORDER BY b.id DESC")).ok()?;
    let rows = st.query_map(params![crate::OWNERS[0], now(), crate::OWNERS[1]], |r| Ok((Ban { id: r.get(0)?, reason: r.get(1)?, until: r.get(2)?, device: true }, r.get::<_, String>(3)?))).ok()?;
    let found = rows.flatten().find(|(_, p)| Parts::read(p).is_some_and(|p| p.same(parts))).map(|(b, _)| b);
    found
}

/// Bans an account: every session of it ends now, and its computer (if one is bound) is banned with it.
pub fn ban(c: &mut Connection, uid: i64, reason: &str, until: Option<i64>, by: &str) -> Result<i64, String> {
    if crate::is_owner(uid) {
        return Err("her account is not banned".into());
    }
    let tx = c.transaction().map_err(|e| e.to_string())?;
    let login: String = tx.query_row("SELECT login FROM users WHERE uid = ?1", params![uid], |r| r.get(0)).optional().map_err(|e| e.to_string())?.ok_or("no such account")?;
    tx.execute("INSERT INTO bans (uid, login, reason, until, at, by) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![uid, login, reason, until, now(), by]).map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.execute("INSERT INTO banned_devices (ban, parts) SELECT ?1, parts FROM devices WHERE uid = ?2", params![id, uid]).map_err(|e| e.to_string())?;
    // The loader's session stays: its next start is told of the ban at once, reason and all (her word: the ban on the
    // whole window as soon as the loader opens). The site's and the game's end now.
    tx.execute("DELETE FROM sessions WHERE uid = ?1 AND kind != 'loader'", params![uid]).map_err(|e| e.to_string())?;
    for table in ["access", "dist_access", "tickets"] {
        tx.execute(&format!("DELETE FROM {table} WHERE uid = ?1"), params![uid]).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(id)
}

/// Lifts every ban the account is under. How many there were.
pub fn unban(c: &Connection, uid: i64, by: &str) -> usize {
    c.execute(&format!("UPDATE bans SET lifted = ?2, lifted_by = ?3 WHERE uid = ?1 AND {ACTIVE}"), params![uid, now(), by]).unwrap_or(0)
}

/// A ban's length as she types it: `7d`, `12h`, `30m`, a bare number of days, or nothing (for good).
pub fn parse_length(s: &str) -> Option<i64> {
    let s = s.trim().to_lowercase();
    let (n, unit) = match s.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((i, _)) => (&s[..i], &s[i..]),
        None => (s.as_str(), "d"),
    };
    let n: i64 = n.parse().ok().filter(|n| *n > 0 && *n <= 100_000)?;
    let seconds = match unit {
        "d" | "д" | "day" | "days" => 86_400,
        "h" | "ч" | "hour" | "hours" => 3600,
        "m" | "м" | "min" | "minutes" => 60,
        _ => return None,
    };
    Some(n * seconds)
}

// ------------------------------------------------------------------------------------------------ computers

/// A computer as the loader describes it: its parts' names and their hashes (64 hex digits each).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct Parts(pub BTreeMap<String, String>);

impl Parts {
    /// Three to twelve parts, each named in a few small letters, each a SHA-256 in hex (Recode's loader sends up to ten:
    /// the TPM, the video card, the monitor and Windows' key in the firmware on top of Cursed's six).
    pub fn valid(&self) -> bool {
        (3..=12).contains(&self.0.len())
            && self.0.iter().all(|(k, v)| (1..=12).contains(&k.len()) && k.bytes().all(|b| b.is_ascii_lowercase()) && v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
    }

    pub fn read(s: &str) -> Option<Parts> {
        serde_json::from_str::<Parts>(s).ok().filter(Parts::valid)
    }

    pub fn text(&self) -> String {
        serde_json::to_string(&self.0).unwrap_or_default()
    }

    /// The same computer: the same TPM (its endorsement key: one per chip), or two of the parts a spoofer seldom
    /// reaches the same (Recode's: the video card's own id, the monitor's serial, Windows' key in the firmware —
    /// each one of a kind), or at least two of the parts both have match, and more than half of those they both have
    /// (a TPM reset in the BIOS gives a new key: then the others).
    pub fn same(&self, other: &Parts) -> bool {
        let alike = |k: &str| matches!((self.0.get(k), other.0.get(k)), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b));
        if alike("tpm") || ["gpu", "monitor", "msdm"].into_iter().filter(|k| alike(k)).count() >= 2 {
            return true;
        }
        let common: Vec<&String> = self.0.keys().filter(|k| other.0.contains_key(*k)).collect();
        let equal = common.iter().filter(|k| alike(k)).count();
        equal >= 2 && equal * 2 > common.len()
    }
}

/// Why an account may not play (the loader shows it; `json` is its answer).
#[derive(Debug, PartialEq)]
pub enum Refusal {
    Banned(Ban),
    /// The account is bound to another computer; the player may free it himself from then on.
    OtherDevice { reset_from: i64 },
    /// This computer is another account's.
    DeviceTaken,
    NotTester,
    /// No Recode on the account (recode.rs): since when it ended (0: it never had it).
    NoRecode { until: i64 },
    /// The account plays Recode on another computer right now (one game at a time; the first keeps it).
    InGame,
    /// Only the current build starts.
    Outdated { build: i64 },
}

impl Refusal {
    pub fn response(&self) -> Response {
        (StatusCode::FORBIDDEN, Json(self.json())).into_response()
    }

    /// What the loader and the game are told.
    pub fn json(&self) -> Value {
        match self {
            Refusal::Banned(b) => b.json(),
            Refusal::OtherDevice { reset_from } => json!({ "error": "other-device", "reset_from": reset_from }),
            Refusal::DeviceTaken => json!({ "error": "device-taken" }),
            Refusal::NotTester => json!({ "error": "not-tester" }),
            Refusal::NoRecode { until } => json!({ "error": "no-recode", "until": if *until > 0 { json!(until) } else { Value::Null } }),
            Refusal::InGame => json!({ "error": "in-game" }),
            Refusal::Outdated { build } => json!({ "error": "outdated", "build": build }),
        }
    }
}

/// When the player may free his computer himself next.
pub fn reset_from(c: &Connection, uid: i64) -> i64 {
    c.query_row("SELECT at FROM device_resets WHERE uid = ?1", params![uid], |r| r.get::<_, i64>(0)).optional().ok().flatten().map_or(0, |at| at + RESET_EVERY)
}

/// What is played: Cursed's beta (its testers) or Recode (its subscribers, recode.rs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Product {
    Beta,
    Recode,
}

/// Whether this account may play on this computer: not banned, the computer not banned, a tester, and the computer
/// its own. An account with no computer yet is bound to this one (`bind`), unless it is another account's. The parts
/// the bound computer has now are kept (a part changed is followed, so a few changes over time do not lose it).
pub fn admit(c: &Connection, uid: i64, parts: &Parts, bind: bool) -> Result<(), Refusal> {
    admit_for(c, uid, parts, bind, Product::Beta)
}

/// The same for what is played: the beta wants a tester, Recode an account whose Recode is on (one computer either
/// way, the same one: an account's computer is one).
pub fn admit_for(c: &Connection, uid: i64, parts: &Parts, bind: bool, product: Product) -> Result<(), Refusal> {
    if crate::is_owner(uid) {
        return Ok(());
    }
    if let Some(b) = ban_of(c, uid) {
        return Err(Refusal::Banned(b));
    }
    if let Some(b) = device_ban(c, parts) {
        return Err(Refusal::Banned(b));
    }
    match product {
        Product::Beta if !is_tester(c, uid) => return Err(Refusal::NotTester),
        // a TPM is not asked for (her word, 20.10: players on Windows 10 have none, or it is off and nobody goes into
        // the BIOS for it): without one the computer is told by its other parts
        Product::Recode => {
            let sub = crate::recode::of(c, uid);
            if !sub.active(now()) {
                return Err(Refusal::NoRecode { until: sub.until });
            }
        }
        _ => {}
    }
    let at = now();
    let bound = c.query_row("SELECT parts FROM devices WHERE uid = ?1", params![uid], |r| r.get::<_, String>(0)).optional().ok().flatten();
    match bound.as_deref().and_then(Parts::read) {
        Some(b) if b.same(parts) => {
            let _ = c.execute("UPDATE devices SET parts = ?2, seen = ?3 WHERE uid = ?1", params![uid, parts.text(), at]);
            Ok(())
        }
        Some(_) => Err(Refusal::OtherDevice { reset_from: reset_from(c, uid) }),
        None => {
            if device_owner(c, parts, uid).is_some() {
                return Err(Refusal::DeviceTaken);
            }
            if bind {
                let _ = c.execute("INSERT OR REPLACE INTO devices (uid, parts, bound, seen) VALUES (?1, ?2, ?3, ?3)", params![uid, parts.text(), at]);
            }
            Ok(())
        }
    }
}

/// Another account this computer is bound to.
fn device_owner(c: &Connection, parts: &Parts, not: i64) -> Option<i64> {
    let mut st = c.prepare("SELECT uid, parts FROM devices WHERE uid != ?1").ok()?;
    let rows = st.query_map(params![not], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).ok()?;
    let found = rows.flatten().find(|(_, p)| Parts::read(p).is_some_and(|p| p.same(parts))).map(|(u, _)| u);
    found
}

/// Frees an account's computer. By the player himself only once a day (`Err(when he may)`); by her at any time.
pub fn free_device(c: &Connection, uid: i64, by_player: bool) -> Result<(), i64> {
    if by_player {
        let from = reset_from(c, uid);
        if now() < from {
            return Err(from);
        }
        let _ = c.execute("INSERT OR REPLACE INTO device_resets (uid, at) VALUES (?1, ?2)", params![uid, now()]);
    }
    let _ = c.execute("DELETE FROM devices WHERE uid = ?1", params![uid]);
    let _ = c.execute("DELETE FROM access WHERE uid = ?1", params![uid]);
    let _ = c.execute("DELETE FROM dist_access WHERE uid = ?1", params![uid]);
    Ok(())
}

// ------------------------------------------------------------------------------------------------ the loader

#[derive(Deserialize)]
struct LoaderSignIn {
    login: String,
    password: String,
    device: Parts,
}

pub fn no_origin(h: &HeaderMap) -> Result<(), Response> {
    if h.contains_key(header::ORIGIN) {
        return Err(fail(StatusCode::FORBIDDEN, "origin")); // for the loader and the game only, never for a web page
    }
    Ok(())
}

/// The loader's sign-in: the password, then whether this account may play on this computer (it is bound to it now if
/// it had none). A token for the loader in the answer.
async fn sign_in(State(app): State<Shared>, h: HeaderMap, Json(s): Json<LoaderSignIn>) -> Response {
    run(async move {
        no_origin(&h)?;
        if !s.device.valid() {
            return Err(fail(StatusCode::BAD_REQUEST, "device"));
        }
        let (uid, login) = verify(&app, SignIn { login: s.login, password: s.password }).await?;
        let parts = s.device;
        let token = db(&app, move |c| {
            admit(c, uid, &parts, true).map_err(|r| r.response())?;
            new_session(c, uid, "loader").map_err(|_| busy())
        })
        .await?;
        Ok(Json(json!({ "uid": uid, "login": login, "token": token })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Start {
    device: Parts,
    #[serde(default = "beta")]
    channel: String,
}

fn beta() -> String {
    "beta".into()
}

/// The loader starts: whether this account may still play here, the current build of its channel (signed), leave to
/// download for a few hours, and a ticket for one start of the game.
async fn start(State(app): State<Shared>, h: HeaderMap, Json(s): Json<Start>) -> Response {
    run(async move {
        no_origin(&h)?;
        let (token, _) = presented(&h).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
        if !s.device.valid() {
            return Err(fail(StatusCode::BAD_REQUEST, "device"));
        }
        let channel = s.channel;
        // Recode's builds are Recode's loader's alone (recode.rs: its own door, a subscription, a TPM)
        if channel == "recode" {
            return Err(fail(StatusCode::FORBIDDEN, "channel"));
        }
        let parts = s.device;
        let answer = db(&app, move |c| {
            let w = who(c, &token).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
            admit(c, w.uid, &parts, true).map_err(|r| r.response())?;
            let Some((build, manifest, sig)) = crate::dist::current(c, &channel) else {
                return Err(fail(StatusCode::SERVICE_UNAVAILABLE, "no-release"));
            };
            let at = now();
            crate::dist::grant(c, w.uid, &channel, at + ACCESS_SECONDS).map_err(|_| busy())?;
            c.execute("DELETE FROM tickets WHERE made < ?1", params![at - TICKET_SECONDS]).map_err(|_| busy())?;
            let ticket = random_hex(32);
            c.execute("INSERT INTO tickets (hash, uid, build, made) VALUES (?1, ?2, ?3, ?4)", params![digest(&ticket), w.uid, build, at]).map_err(|_| busy())?;
            Ok(json!({ "uid": w.uid, "login": w.login, "build": build, "manifest": manifest, "sig": sig, "ticket": ticket, "ticket_seconds": TICKET_SECONDS }))
        })
        .await?;
        Ok(Json(answer).into_response())
    })
    .await
}

fn busy() -> Response {
    fail(StatusCode::SERVICE_UNAVAILABLE, "busy")
}

#[derive(Deserialize)]
struct Reset {
    #[serde(default)]
    login: String,
    #[serde(default)]
    password: String,
    device: Parts,
}

/// The loader's "free my computer": the account signed in by its token, or by its login and password (on the new
/// computer it has no token yet). Once a day; the computer asking is bound in the same breath.
async fn reset(State(app): State<Shared>, h: HeaderMap, Json(r): Json<Reset>) -> Response {
    run(async move {
        no_origin(&h)?;
        if !r.device.valid() {
            return Err(fail(StatusCode::BAD_REQUEST, "device"));
        }
        let uid = match presented(&h) {
            Some((token, _)) => db(&app, move |c| who(c, &token)).await.ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?.uid,
            None => verify(&app, SignIn { login: r.login, password: r.password }).await?.0,
        };
        let parts = r.device;
        db(&app, move |c| {
            if let Some(b) = ban_of(c, uid) {
                return Err(Refusal::Banned(b).response());
            }
            free_device(c, uid, true).map_err(|from| (StatusCode::TOO_MANY_REQUESTS, Json(json!({ "error": "wait", "reset_from": from }))).into_response())?;
            admit(c, uid, &parts, true).map_err(|r| r.response())?;
            Ok(Json(json!({ "reset_from": now() + RESET_EVERY })).into_response())
        })
        .await
    })
    .await
}

#[derive(Deserialize)]
struct Verify {
    ticket: String,
    device: Parts,
    build: i64,
    /// The game's random words (64 hex): the answer is signed with them (dist.rs, sign_start). An older game sends none.
    #[serde(default)]
    nonce: String,
}

/// The game at its start: the loader's ticket (for this one start), this computer, the build it is. The same checks
/// as the loader's, and only the current build; the answer is the game's own session.
async fn client_verify(State(app): State<Shared>, h: HeaderMap, Json(v): Json<Verify>) -> Response {
    run(async move {
        no_origin(&h)?;
        if !v.device.valid() || v.ticket.len() != 64 {
            return Err(fail(StatusCode::BAD_REQUEST, "ticket"));
        }
        let (ticket, nonce, build) = (v.ticket.clone(), v.nonce.clone(), v.build);
        let answer = db(&app, move |c| {
            let hash = digest(&v.ticket);
            let found = c
                .query_row("SELECT uid, build, made FROM tickets WHERE hash = ?1", params![hash], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))
                .optional()
                .ok()
                .flatten();
            let _ = c.execute("DELETE FROM tickets WHERE hash = ?1", params![hash]);
            let Some((uid, _, _)) = found.filter(|t| now() - t.2 <= TICKET_SECONDS) else {
                return Err(fail(StatusCode::UNAUTHORIZED, "ticket"));
            };
            let channel_build = crate::dist::current(c, "beta").map(|(b, _, _)| b).unwrap_or(0);
            if v.build != channel_build {
                return Err(Refusal::Outdated { build: channel_build }.response());
            }
            admit(c, uid, &v.device, false).map_err(|r| r.response())?;
            let login: String = c.query_row("SELECT login FROM users WHERE uid = ?1", params![uid], |r| r.get(0)).map_err(|_| busy())?;
            let token = new_session(c, uid, "client").map_err(|_| busy())?;
            Ok(json!({ "uid": uid, "uid_text": crate::people::card_of(c, uid)["uid_text"].clone(), "login": login, "token": token }))
        })
        .await?;
        let mut answer = answer;
        if nonce.len() == 64 && nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
            let uid = answer["uid"].as_i64().unwrap_or(0);
            answer["sig"] = json!(app.dist.sign_start(&nonce, &ticket, uid, build));
        }
        Ok(Json(answer).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct CrashAt {
    #[serde(default)]
    build: Option<i64>,
    #[serde(default)]
    source: Option<String>,
}

/// Where the crash reports are kept (next to the database, where the service may write).
pub fn crashes_dir(db_path: &str) -> PathBuf {
    std::path::Path::new(db_path).parent().map_or_else(|| PathBuf::from("crashes"), |d| d.join("crashes"))
}

/// How long a first finding is remembered before a second one is only a first one again. Her rule
/// (16.10): a repeat within a week is a ban; a single one is a line on her list and nothing more.
const A_WEEK: i64 = 7 * 86_400;

/// One day, in the same seconds `crashes.at` is counted in. A UTC day is what "a different day"
/// means here: the clock the note is written under is the server's, and one number cannot drift
/// between the two ends.
const A_DAY: i64 = 86_400;

/// What her rules do with a report from the game's own guard (16.10): the first finding goes on her
/// list as it was seen; a second one inside a week is a ban — the same thing was there again on
/// purpose, and a player who was told once has been told. The first one never bans: a player who
/// had a dumper open for something else entirely is not a copy being taken apart.
///
/// What is counted is **days**, not notes (her word, 21.10). One run of the game can leave two
/// notes twenty seconds apart — the game writes one and the loader sends another — and that is one
/// finding seen twice, not two findings. A player gets a line on her list and keeps playing; only
/// the same thing turning up again on another day inside the week is a repeat. The ban's own words
/// the player sees are exactly as they were; what the note actually said goes on her list only.
///
/// Says whether it banned.
fn guard_report(c: &mut Connection, uid: i64, at: i64, build: Option<i64>, file: &str, note: &str) -> bool {
    let days: i64 = c
        .query_row(
            "SELECT COUNT(DISTINCT at / ?3) FROM crashes WHERE uid = ?1 AND source = 'protect' AND at > ?2",
            params![uid, at - A_WEEK, A_DAY],
            |r| r.get(0),
        )
        .unwrap_or(1);
    // What the note said, on her list and nowhere else: the player's own words are the ones above.
    // `why` is the one line the note carries under `why:`; `note` is the whole of it, cut down so a
    // list of these still reads as a list.
    let why = note_why(note);
    // `why` is the one line the player is told; `found` is the block under it — what the check
    // actually saw (her word, 21.10: the detail has to be readable on her list). `note` is the
    // whole of it, kept far enough along that the detail is not cut in half.
    let detail = json!({
        "build": build,
        "file": file,
        "days": days,
        "why": why,
        "found": guard_found(note),
        "note": guard_note(note)
    });
    if days < 2 {
        crate::recode::flag(c, uid, "protect", detail);
        return false;
    }
    let reason = format!("Инструменты разбора игры — повторное обнаружение ({days} за неделю)");
    match ban(c, uid, &reason, None, "watch") {
        Ok(id) => {
            let mut d = detail;
            d["ban"] = json!(id);
            d["reason"] = json!(reason);
            crate::recode::flag(c, uid, "autoban", d);
            true
        }
        // her own account, or a ban that would not go on: the report above stands either way
        Err(_) => false,
    }
}

/// The note as her list shows it: one line, cut down. The whole of it is beside the database as a
/// crash file, and a table cell is not where a paragraph belongs. Long enough to hold the detail
/// block under the `why:` line whole — that block is the reason the note is worth reading at all.
fn guard_note(note: &str) -> String {
    note.replace(['\n', '\r', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(800)
        .collect()
}

/// Everything under the `why:` line: what the check actually saw, in its own words. The stub writes
/// it as `found:` lines (the thread's page, the guest's own words, the hand that was held) and
/// `watch`'s process notes carry their tree there too. Flattened to one line for a table cell and
/// cut down the same way the note is. A note of one line has nothing here.
fn guard_found(note: &str) -> String {
    let mut out = String::new();
    let mut under = false;
    for line in note.lines() {
        if under {
            out.push_str(line.trim());
            out.push(' ');
        } else if line.starts_with("why: ") {
            under = true;
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(600).collect()
}

/// Puts what the note said into a detail that was written before it carried it. A row can sit on
/// her list from a build whose detail kept only the name of the log; the note beside the database
/// is still the same text the report came with, so the list is filled from that and nothing is
/// guessed. Only the fields the note actually has are set — what is already on the line stays.
pub fn fill_from_note(d: &mut Value, note: &str) {
    let Some(o) = d.as_object_mut() else { return };
    if o.get("why").and_then(Value::as_str).unwrap_or("").is_empty() {
        let why = note_why(note);
        if !why.is_empty() {
            o.insert("why".to_string(), json!(why));
        }
    }
    if o.get("found").and_then(Value::as_str).unwrap_or("").is_empty() {
        let found = guard_found(note);
        if !found.is_empty() {
            o.insert("found".to_string(), json!(found));
        }
    }
}

/// The name of the log a row was written from. The row usually names it (`file`); a ban of an
/// older build kept only the ban's own words, and the report of the same second is the one that
/// named the log — a report and the ban it caused are written with the same `at`. A name is
/// returned only when it is a plain name: a note held whole in `file` (touch-unverified as it
/// came) is not one.
pub fn log_name_of(c: &Connection, uid: i64, at: i64, d: &Value) -> Option<String> {
    let plain = |f: &str| !f.contains(['\n', '/', '\\']);
    let named = d["file"].as_str().unwrap_or("");
    if !named.is_empty() {
        // a name of a log, or a note held whole — the whole note is already on the row and there
        // is nothing to look up
        return plain(named).then(|| named.to_string());
    }
    c.query_row(
        "SELECT file FROM crashes WHERE uid = ?1 AND at = ?2 AND source IN ('protect', 'touch') ORDER BY id DESC LIMIT 1",
        params![uid, at],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .filter(|f| plain(f))
}

/// Fills a row's detail from the note it came from, on the way out to her list. Only what is
/// missing is set: the words under `why:`, what the check saw under that, and the name of the log
/// when the row never had one — that name is how she reaches the whole note beside the database.
pub fn fill_row(c: &Connection, dir: &std::path::Path, uid: i64, at: i64, d: &mut Value) {
    if !d["why"].as_str().unwrap_or("").is_empty() {
        return;
    }
    let Some(name) = log_name_of(c, uid, at, d) else { return };
    if d["file"].as_str().unwrap_or("").is_empty() {
        d["file"] = json!(name);
    }
    if let Ok(note) = std::fs::read_to_string(dir.join(&name)) {
        fill_from_note(d, &note);
    }
}

/// The kind of hand the note names (`touch: patch` and five more). The six are what the game may
/// see and what this file will act on; anything else is not a finding this rule can stand on.
///
/// They live as one `|`-joined string — the same string both ends keep under `touch_kinds`
/// (TOUCH.md §1) — and are split on `|` and matched as words. Six literals in a `matches!` is how
/// ends drift apart: the word goes in one place and the others keep the old list.
const TOUCH_KINDS: &str = "patch|hook|break|write|inject|script";

fn touch_kind(file: &str) -> Option<&str> {
    let line = file.lines().find(|l| l.starts_with("touch: "))?;
    let kind = line["touch: ".len()..].trim();
    TOUCH_KINDS.split('|').any(|k| k == kind).then_some(kind)
}

/// The `why:` line of the note, cut down. Used two ways: for the ban's own words (the player is
/// told what the game saw, in the game's words, and not this file's guess at it) and for what goes
/// on her list when nothing is banned at all.
fn note_why(file: &str) -> String {
    let why = file.lines().find_map(|l| l.strip_prefix("why: ")).unwrap_or("");
    why.replace(['\n', '\r'], " ").chars().take(120).collect()
}

/// The MAC as the client writes it: the pair ticket, a newline, and the body as it came, hashed and
/// set out in hex. Nothing else goes under the hash — no header, no name of the crash file.
fn touch_mac_of(ticket: &str, body: &[u8]) -> String {
    use sha2::Digest;
    let mut d = sha2::Sha256::new();
    d.update(ticket.as_bytes());
    d.update(b"\n");
    d.update(body);
    d.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether this MAC closes over this body under one of the two pair tickets that are the player's
/// now: the window `at` falls in, or the one just turned. A pair handed out last week is still the
/// pair the player is running, and its ticket is still what that copy is keyed by (her word, 18.10).
fn touch_mac(root: &Path, uid: i64, at: i64, body: &[u8], mac: &str) -> bool {
    if mac.len() != 64 || !mac.bytes().all(|b| b.is_ascii_hexdigit()) {
        return false;
    }
    let mac = mac.to_ascii_lowercase();
    touch_mac_of(&crate::protect::pair_ticket(root, uid, at), body) == mac
        || touch_mac_of(&crate::protect::pair_ticket(root, uid, at - crate::protect::PAIR_WINDOW), body) == mac
}

/// What her rules do with a report of a hand put on the game's own memory (her word, 18.10): one
/// confirmed finding is a ban. The game closes itself when it finds its own memory written into —
/// its code patched, its import table hooked, a hardware breakpoint set, a thread with no file
/// behind it, a script aimed at it — and sends a note of what was done and why. This is not a tool
/// left open beside the game (guard_report, 16.10), where one finding is only a line on her list
/// and a second one inside a week is a ban: here a hand was on the game itself, and one finding is
/// the whole of it. A false positive bans a paying player, so a report only counts when it is ours —
/// the MAC over the body under the pair ticket the copy is keyed by, under the window now or the one
/// just turned. Anything short of that (a header missing, a kind outside the six, a MAC that does
/// not close) is written down as `touch-unverified` and bans no one. `file` is the note as it came.
/// Says whether it banned.
fn touch_report<B: AsRef<[u8]>>(c: &mut Connection, uid: i64, at: i64, build: Option<i64>, body: B, mac: &str, root: &Path) -> bool {
    // The note as it came. `body` is the request's own bytes and those are what the MAC closes over —
    // never a re-encoded string in between. A body that is not UTF-8 parses to "" and so becomes
    // unverified rather than a ban on the wrong hash.
    let body = body.as_ref();
    let file = std::str::from_utf8(body).unwrap_or("");
    // the note has to name one of the six hands and carry a MAC this server could have written;
    // either missing and the whole of it is a claim nobody here can stand behind
    let verified = touch_kind(file).is_some() && touch_mac(root, uid, at, body, mac);
    // The note's own words go on her list either way — `why` is the one line the player is told and
    // `found` is the block under it (her word, 21.10: the detail has to be readable there). This is
    // the path a false ban comes down, and the block is the only thing that says one from the other.
    let why = note_why(file);
    let found = guard_found(file);
    if !verified {
        crate::recode::flag(c, uid, "touch-unverified", json!({ "build": build, "file": file, "why": why, "found": found, "times": 1 }));
        return false;
    }
    let kind = touch_kind(file).expect("the kind was read above");
    let reason = if why.is_empty() { format!("Рука в памяти игры — {kind}") } else { format!("Рука в памяти игры — {kind} ({why})") };
    match ban(c, uid, &reason, None, "watch") {
        Ok(id) => {
            crate::recode::flag(c, uid, "autoban", json!({ "ban": id, "reason": reason, "kind": kind, "why": why, "found": found, "times": 1 }));
            true
        }
        // her own account, or a ban that would not go on: the finding goes on her list either way
        Err(_) => {
            crate::recode::flag(c, uid, "autoban", json!({ "reason": reason, "kind": kind, "why": why, "found": found, "times": 1 }));
            false
        }
    }
}

/// Whether this account has had as many crash reports today as it may. A report of a hand on the
/// game's own memory (`source = touch`) is never one of the counted: a ban must not be turned away
/// because the player already sent a few crash logs (her word, 18.10).
fn crash_day_full(c: &Connection, uid: i64, source: &str) -> bool {
    if source == "touch" {
        return false;
    }
    let today: i64 = c.query_row("SELECT COUNT(*) FROM crashes WHERE uid = ?1 AND at > ?2", params![uid, now() - 86_400], |r| r.get(0)).unwrap_or(0);
    today >= CRASHES_A_DAY
}

/// A crash report from the loader or the game (its token): kept as it came, a few a day per account.
/// A `touch` report is a ban's own note and goes through whatever the day holds.
async fn crash(State(app): State<Shared>, h: HeaderMap, Query(q): Query<CrashAt>, body: axum::body::Bytes) -> Response {
    run(async move {
        let (token, _) = presented(&h).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
        if body.is_empty() || body.len() > CRASH_MOST {
            return Err(fail(StatusCode::PAYLOAD_TOO_LARGE, "size"));
        }
        let dir = crashes_dir(&app.db_path);
        let source = q.source.map(|s| clean_line(&s, 16)).filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or_else(|| "game".into());
        let build = q.build;
        // what the game signed the note with (X-Cursed-Mac); a header that is not there is the same
        // as one that closes on nothing
        let mac = h.get("x-cursed-mac").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let root = app.dist.root.clone();
        let saved = db(&app, move |c| {
            let w = who(c, &token).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
            if crash_day_full(c, w.uid, &source) {
                return Err(fail(StatusCode::TOO_MANY_REQUESTS, "day"));
            }
            let at = now();
            let name = format!("{}-{}-{}-{}.log", w.uid, at, source, random_hex(3));
            std::fs::create_dir_all(&dir).map_err(|_| busy())?;
            std::fs::write(dir.join(&name), &body).map_err(|_| busy())?;
            let (sig, gpu) = crate::admin::crash_cause(&String::from_utf8_lossy(&body));
            c.execute("INSERT INTO crashes (uid, at, build, source, file, size, sig, gpu) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", params![w.uid, at, build, source, name, body.len() as i64, sig, gpu]).map_err(|_| busy())?;
            if source == "protect" {
                guard_report(c, w.uid, at, build, &name, &String::from_utf8_lossy(&body));
            }
            if source == "touch" {
                // the note as it came, and nothing around it: the MAC is over these exact bytes
                touch_report(c, w.uid, at, build, &body, &mac, &root);
            }
            Ok(())
        })
        .await;
        saved.map(|()| Json(json!({})).into_response())
    })
    .await
}

// ------------------------------------------------------------------------------------------------ her doors (the bot's too)

/// An account by its login, as she types it.
pub fn uid_of(c: &Connection, login: &str) -> Option<(i64, String)> {
    c.query_row("SELECT uid, login FROM users WHERE login_key = ?1", params![login.trim().to_ascii_lowercase()], |r| Ok((r.get(0)?, r.get(1)?))).optional().ok().flatten()
}

/// A tester as she sees one: the account, its role, its ban, its computer, when it was last seen, its crashes.
pub fn tester_json(c: &Connection, uid: i64, whole: bool) -> Option<Value> {
    let (login, email, created): (String, String, i64) =
        c.query_row("SELECT login, email, created FROM users WHERE uid = ?1", params![uid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional().ok()??;
    let device = c
        .query_row("SELECT bound, seen FROM devices WHERE uid = ?1", params![uid], |r| Ok(json!({ "bound": r.get::<_, i64>(0)?, "seen": r.get::<_, i64>(1)? })))
        .optional()
        .ok()
        .flatten();
    let seen: Option<i64> = c.query_row("SELECT MAX(seen) FROM sessions WHERE uid = ?1", params![uid], |r| r.get(0)).ok().flatten();
    let crashes: i64 = c.query_row("SELECT COUNT(*) FROM crashes WHERE uid = ?1", params![uid], |r| r.get(0)).unwrap_or(0);
    let mut out = json!({
        "uid": uid, "login": login, "since": created,
        "tester": is_tester(c, uid),
        "ban": ban_of(c, uid).map(|b| json!({ "reason": b.reason, "until": b.until })),
        "device": device, "seen": seen, "crashes": crashes,
        "reset_from": reset_from(c, uid),
    });
    if whole {
        out["email"] = json!(email);
        let mut st = c.prepare("SELECT reason, until, at, by, lifted, lifted_by FROM bans WHERE uid = ?1 ORDER BY id DESC LIMIT 20").ok()?;
        let bans: Vec<Value> = st
            .query_map(params![uid], |r| {
                Ok(json!({ "reason": r.get::<_, String>(0)?, "until": r.get::<_, Option<i64>>(1)?, "at": r.get::<_, i64>(2)?, "by": r.get::<_, String>(3)?, "lifted": r.get::<_, Option<i64>>(4)?, "lifted_by": r.get::<_, Option<String>>(5)? }))
            })
            .ok()?
            .flatten()
            .collect();
        out["bans"] = json!(bans);
    }
    Some(out)
}

pub fn testers_json(c: &Connection) -> Value {
    let uids: Vec<i64> = c.prepare("SELECT uid FROM users WHERE uid NOT IN (?1, ?2) ORDER BY uid").and_then(|mut s| s.query_map(params![crate::OWNERS[0], crate::OWNERS[1]], |r| r.get(0))?.collect()).unwrap_or_default();
    json!({
        "testers": uids.into_iter().filter_map(|u| tester_json(c, u, false)).collect::<Vec<_>>(),
        "limit": limit(c), "taken": taken(c),
        "registration": c.query_row("SELECT value FROM settings WHERE name = 'registration'", [], |r| r.get::<_, String>(0)).unwrap_or_default(),
    })
}

/// Deletes an account (its place is free again). One that wrote on the forum or put packs in the market is not
/// deleted here: ban it instead (what it wrote stays under its name).
pub fn delete(c: &mut Connection, uid: i64) -> Result<(), String> {
    if crate::is_owner(uid) {
        return Err("her account is not deleted".into());
    }
    let tx = c.transaction().map_err(|e| e.to_string())?;
    // Out of its party first: the lead goes to the next one, its marks go.
    crate::party::leave(&tx, uid);
    match tx.execute("DELETE FROM users WHERE uid = ?1", params![uid]) {
        Ok(1) => tx.commit().map_err(|e| e.to_string()),
        Ok(_) => Err("no such account".into()),
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => {
            Err("it wrote on the forum or has packs in the market: ban it instead".into())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Deserialize)]
struct Named {
    login: String,
}

#[derive(Deserialize)]
struct BanAsk {
    login: String,
    reason: String,
    /// `7d`, `12h`, `30m`, a number of days; missing or empty: for good.
    #[serde(default)]
    length: Option<String>,
}

async fn admin_testers(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async move {
        crate::blocklist::keeper(&app, &h).await?;
        Ok(Json(db(&app, |c| testers_json(c)).await).into_response())
    })
    .await
}

async fn admin_tester(State(app): State<Shared>, h: HeaderMap, Key(login): Key<String>) -> Response {
    run(async move {
        crate::blocklist::keeper(&app, &h).await?;
        let found = db(&app, move |c| uid_of(c, &login).and_then(|(u, _)| tester_json(c, u, true))).await;
        found.map(|v| Json(v).into_response()).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no-such"))
    })
    .await
}

async fn admin_ban(State(app): State<Shared>, h: HeaderMap, Json(a): Json<BanAsk>) -> Response {
    run(async move {
        let by = crate::blocklist::keeper(&app, &h).await?;
        let reason = clean_line(&a.reason, 200);
        if reason.is_empty() {
            return Err(fail(StatusCode::BAD_REQUEST, "reason"));
        }
        let length = match a.length.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => Some(parse_length(s).ok_or_else(|| fail(StatusCode::BAD_REQUEST, "length"))?),
            None => None,
        };
        let login = a.login;
        db(&app, move |c| {
            let (uid, login) = uid_of(c, &login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no-such"))?;
            let until = length.map(|l| now() + l);
            ban(c, uid, &reason, until, &by).map_err(|e| fail(StatusCode::CONFLICT, &e))?;
            Ok(Json(json!({ "uid": uid, "login": login, "reason": reason, "until": until })).into_response())
        })
        .await
    })
    .await
}

async fn admin_unban(State(app): State<Shared>, h: HeaderMap, Json(a): Json<Named>) -> Response {
    run(async move {
        let by = crate::blocklist::keeper(&app, &h).await?;
        db(&app, move |c| {
            let (uid, login) = uid_of(c, &a.login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no-such"))?;
            let lifted = unban(c, uid, &by);
            Ok(Json(json!({ "uid": uid, "login": login, "lifted": lifted })).into_response())
        })
        .await
    })
    .await
}

async fn admin_free(State(app): State<Shared>, h: HeaderMap, Json(a): Json<Named>) -> Response {
    run(async move {
        crate::blocklist::keeper(&app, &h).await?;
        db(&app, move |c| {
            let (uid, login) = uid_of(c, &a.login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no-such"))?;
            let had = c.query_row("SELECT 1 FROM devices WHERE uid = ?1", params![uid], |_| Ok(())).optional().ok().flatten().is_some();
            let _ = free_device(c, uid, false);
            Ok(Json(json!({ "uid": uid, "login": login, "had": had })).into_response())
        })
        .await
    })
    .await
}

async fn admin_delete(State(app): State<Shared>, h: HeaderMap, Json(a): Json<Named>) -> Response {
    run(async move {
        crate::blocklist::keeper(&app, &h).await?;
        db(&app, move |c| {
            let (uid, login) = uid_of(c, &a.login).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no-such"))?;
            delete(c, uid).map_err(|e| fail(StatusCode::CONFLICT, &e))?;
            Ok(Json(json!({ "uid": uid, "login": login })).into_response())
        })
        .await
    })
    .await
}

#[derive(Deserialize)]
struct Registration {
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
}

pub fn set_registration(c: &Connection, state: Option<&str>, lim: Option<i64>) -> Result<Value, String> {
    if let Some(s) = state {
        if !matches!(s, "open" | "closed") {
            return Err("open or closed".into());
        }
        c.execute("UPDATE settings SET value = ?1 WHERE name = 'registration'", params![s]).map_err(|e| e.to_string())?;
    }
    if let Some(l) = lim {
        if !(0..=100_000).contains(&l) {
            return Err("a limit from 0".into());
        }
        c.execute("INSERT INTO settings (name, value) VALUES ('registration_limit', ?1) ON CONFLICT(name) DO UPDATE SET value = excluded.value", params![l.to_string()])
            .map_err(|e| e.to_string())?;
    }
    let state: String = c.query_row("SELECT value FROM settings WHERE name = 'registration'", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    Ok(json!({ "state": state, "limit": limit(c), "taken": taken(c), "full": full(c), "per_ip": per_ip(c) }))
}

async fn admin_registration(State(app): State<Shared>, h: HeaderMap, body: Option<Json<Registration>>) -> Response {
    run(async move {
        crate::blocklist::keeper(&app, &h).await?;
        let r = body.map(|Json(r)| r).unwrap_or(Registration { state: None, limit: None });
        db(&app, move |c| set_registration(c, r.state.as_deref(), r.limit).map(|v| Json(v).into_response()).map_err(|e| fail(StatusCode::BAD_REQUEST, &e))).await
    })
    .await
}

pub fn routes() -> Router<Shared> {
    let small = || DefaultBodyLimit::max(8 * 1024);
    Router::new()
        .route("/api/loader/login", post(sign_in).layer(small()))
        .route("/api/loader/start", post(start).layer(small()))
        .route("/api/loader/device/reset", post(reset).layer(small()))
        .route("/api/client/verify", post(client_verify).layer(small()))
        .route("/api/crash", post(crash).layer(DefaultBodyLimit::max(CRASH_MOST + 1024)))
        .route("/api/admin/testers", get(admin_testers))
        .route("/api/admin/testers/{login}", get(admin_tester))
        .route("/api/admin/ban", post(admin_ban).layer(small()))
        .route("/api/admin/unban", post(admin_unban).layer(small()))
        .route("/api/admin/device-reset", post(admin_free).layer(small()))
        .route("/api/admin/delete", post(admin_delete).layer(small()))
        .route("/api/admin/registration", get(admin_registration).post(admin_registration).layer(small()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(v: &[(&str, char)]) -> Parts {
        Parts(v.iter().map(|(k, c)| (k.to_string(), c.to_string().repeat(64))).collect())
    }

    fn base() -> Parts {
        parts(&[("board", 'a'), ("cpu", 'b'), ("disk", 'c'), ("win", 'd'), ("bios", 'e')])
    }

    #[test]
    fn a_computer_is_the_same_while_most_of_it_is() {
        let pc = base();
        assert!(pc.valid());
        assert!(pc.same(&pc));
        // Windows installed again: one part of five.
        assert!(pc.same(&parts(&[("board", 'a'), ("cpu", 'b'), ("disk", 'c'), ("win", 'f'), ("bios", 'e')])));
        // A new disk and Windows with it: three of five still match.
        assert!(pc.same(&parts(&[("board", 'a'), ("cpu", 'b'), ("disk", '1'), ("win", '2'), ("bios", 'e')])));
        // Three of five new: another computer.
        assert!(!pc.same(&parts(&[("board", '1'), ("cpu", 'b'), ("disk", '2'), ("win", '3'), ("bios", 'e')])));
        // Too few parts to tell anything.
        assert!(!parts(&[("board", 'a'), ("cpu", 'b')]).valid());
        let mut bad = base();
        bad.0.insert("x".into(), "zz".into());
        assert!(!bad.valid());
    }

    /// Recode without a TPM (her word, 20.10): the computer is told by the rest of it. Its TPM switched off is still
    /// the same computer, and a ban on it stands; Recode's ten parts are all taken, twelve at most.
    #[test]
    fn a_computer_without_its_tpm_is_still_itself() {
        let with = parts(&[("board", 'a'), ("cpu", 'b'), ("disk", 'c'), ("win", 'd'), ("uuid", 'e'), ("vol", 'f'), ("tpm", '1'), ("gpu", '2'), ("monitor", '3'), ("msdm", '4')]);
        assert!(with.valid());
        let mut off = with.clone();
        off.0.remove("tpm");
        assert!(off.valid() && off.same(&with) && with.same(&off));
        // a common spoofer's work: the board, the disks, Windows' ids made new and the TPM gone — the video card, the
        // monitor and the firmware's key are left, and two of those are enough
        let spoofed = parts(&[("board", '5'), ("cpu", 'b'), ("disk", '6'), ("win", '7'), ("uuid", '8'), ("vol", '9'), ("gpu", '2'), ("monitor", '3'), ("msdm", '4')]);
        assert!(spoofed.same(&with));
        let mut card = spoofed.clone();
        card.0.insert("monitor".into(), "5".repeat(64));
        assert!(card.same(&with), "the video card and the firmware's key");
        // one of them alone is not: a monitor moves from one computer to another
        card.0.insert("gpu".into(), "6".repeat(64));
        assert!(!card.same(&with));
        // one that reaches the card and the key too (the monitor left) makes another computer, as far as parts can tell
        card.0.insert("msdm".into(), "7".repeat(64));
        card.0.insert("monitor".into(), "3".repeat(64));
        assert!(!card.same(&with));
        let mut many = with.clone();
        for k in ["aa", "bb", "cc"] {
            many.0.insert(k.into(), "0".repeat(64));
        }
        assert!(!many.valid(), "thirteen parts");
        let (mut c, path) = fresh();
        let a = add(&c, "tpm_on");
        assert_eq!(admit(&c, a, &with, true), Ok(()));
        // the same account with its TPM switched off: its own computer still
        assert_eq!(admit(&c, a, &off, true), Ok(()));
        ban(&mut c, a, "cheats", None, "test").unwrap();
        let b = add(&c, "tpm_off");
        assert!(matches!(admit(&c, b, &off, true), Err(Refusal::Banned(ref x)) if x.device && x.reason == "cheats"));
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    #[test]
    fn lengths_read_as_typed() {
        assert_eq!(parse_length("7d"), Some(7 * 86_400));
        assert_eq!(parse_length("12h"), Some(12 * 3600));
        assert_eq!(parse_length("30m"), Some(1800));
        assert_eq!(parse_length("3"), Some(3 * 86_400));
        assert_eq!(parse_length("7д"), Some(7 * 86_400));
        assert_eq!(parse_length("0"), None);
        assert_eq!(parse_length("week"), None);
    }

    fn fresh() -> (Connection, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("cursed-beta-{}.db", random_hex(4)));
        let c = crate::open(path.to_str().unwrap());
        (c, path)
    }

    fn add(c: &Connection, login: &str) -> i64 {
        c.execute("INSERT INTO users (login, login_key, email, email_key, pass, created) VALUES (?1, lower(?1), ?1 || '@e.st', lower(?1) || '@e.st', 'x', 0)", params![login]).unwrap();
        let uid = c.last_insert_rowid();
        made_tester(c, uid).unwrap();
        uid
    }

    #[test]
    fn one_account_one_computer_and_bans_close_both() {
        let (mut c, path) = fresh();
        let a = add(&c, "alpha");
        let b = add(&c, "beta_one");
        let pc = base();
        let other = parts(&[("board", '1'), ("cpu", '2'), ("disk", '3'), ("win", '4'), ("bios", '5')]);
        // The first computer an account starts on is its own.
        assert_eq!(admit(&c, a, &pc, true), Ok(()));
        // Another account cannot play there.
        assert_eq!(admit(&c, b, &pc, true), Err(Refusal::DeviceTaken));
        // Nor the first account elsewhere.
        assert!(matches!(admit(&c, a, &other, true), Err(Refusal::OtherDevice { .. })));
        // Freed by the player: once a day.
        assert_eq!(free_device(&c, a, true), Ok(()));
        assert!(free_device(&c, a, true).is_err());
        assert_eq!(admit(&c, a, &other, true), Ok(()));
        // Freed by her: at once.
        assert_eq!(free_device(&c, a, false), Ok(()));
        assert_eq!(admit(&c, a, &pc, true), Ok(()));
        // A ban closes the account and its computer; a new account there is closed too.
        ban(&mut c, a, "cheats", None, "test").unwrap();
        assert!(matches!(admit(&c, a, &pc, true), Err(Refusal::Banned(_))));
        let c2 = add(&c, "gamma");
        assert!(matches!(admit(&c, c2, &pc, true), Err(Refusal::Banned(ref x)) if x.reason == "cheats"));
        assert_eq!(unban(&c, a, "test"), 1);
        assert_eq!(admit(&c, a, &pc, true), Ok(()));
        // A ban that has run out holds no more.
        ban(&mut c, b, "spam", Some(now() - 1), "test").unwrap();
        assert_eq!(ban_of(&c, b), None);
        // Her account: never banned, never bound.
        assert!(ban(&mut c, OWNER, "x", None, "test").is_err());
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    /// What the game's own guard finds, as her rules say (16.10, sharpened 21.10): the first one
    /// is a line on her list and the player keeps playing; the same thing seen again on **another
    /// day** inside a week is a ban. One run of the game can leave two notes seconds apart and that
    /// is one finding, not two. A crash of the game's own is nobody's ban, and a finding older than
    /// a week is only a first one again.
    #[test]
    fn a_guard_finding_on_two_days_in_a_week_is_a_ban() {
        let (mut c, path) = fresh();
        let a = add(&c, "hunter");
        // Noon UTC today, so "the same day" and "the next day" answer the same at any hour this
        // test is run at — a report twenty seconds later must not cross a midnight by accident.
        let at = now() / A_DAY * A_DAY + 12 * 3600;
        let seen = |c: &Connection, uid: i64| -> i64 { c.query_row("SELECT COUNT(*) FROM crashes WHERE uid = ?1", params![uid], |r| r.get(0)).unwrap() };
        // Exactly what the stub writes under the one line the player is told: the detail block is
        // the reading the conclusion was drawn from (21.10).
        let note = "Cursed Recode — the game was closed\nbuild: 47\nwhy: this machine is a virtual machine's guest\nfound: word virtual machine\nfw: Microsoft Corporation Virtual Machine\ndsk: Msft Virtual Disk\n";
        let raw = "Cursed Recode — the game was closed\nbuild: 47\nwhy: a thread 0x7ffa9148a9e4 with no module\nfound: tid=12 start=0x7ffa9148a9e4 mem=private prot=0x40 base=0x7ffa91480000 where=-\n";

        // a crash of the game's own is nobody's ban, however often it comes
        c.execute("INSERT INTO crashes (uid, at, build, source, file, size) VALUES (?1, ?2, 3, 'game', 'a.log', 1)", params![a, at]).unwrap();
        c.execute("INSERT INTO crashes (uid, at, build, source, file, size) VALUES (?1, ?2, 3, 'game', 'b.log', 1)", params![a, at]).unwrap();
        assert!(!guard_report(&mut c, a, at, Some(3), "c.log", raw), "a crash of the game's own bans no one");
        assert_eq!(ban_of(&c, a), None);

        // the first finding: her list, and the game goes on
        c.execute("INSERT INTO crashes (uid, at, build, source, file, size) VALUES (?1, ?2, 3, 'protect', 'p1.log', 1)", params![a, at]).unwrap();
        assert!(!guard_report(&mut c, a, at, Some(3), "p1.log", note), "the first finding bans no one");
        assert_eq!(ban_of(&c, a), None);
        assert_eq!(seen(&c, a), 3);

        // …and what the check saw under that line is on her list as itself: which sign fired, and
        // what the machine itself said. Without it a wrong one cannot be read (21.10).
        let first: String = c
            .query_row("SELECT detail FROM recode_flags WHERE uid = ?1 AND kind = 'protect' ORDER BY id DESC LIMIT 1", params![a], |r| r.get(0))
            .unwrap();
        assert!(first.contains("virtual machine's guest"), "her list says what was found: {first}");
        assert!(first.contains("word virtual machine"), "and which sign fired: {first}");
        assert!(first.contains("dsk: Msft Virtual Disk"), "and what the machine itself said: {first}");
        assert!(first.contains("\"found\":"), "in a field of its own: {first}");
        assert!(first.contains("\"days\":1"), "and on how many days: {first}");

        // the very same run leaves a second note twenty seconds later: one finding seen twice, and
        // the player keeps playing. This is the false ban of 21.10 (nikiytiki, three notes).
        // …and it says something else entirely: her list must show what was found **last**, not
        // only what was found first, or the one note that explains the case is thrown away
        c.execute("INSERT INTO crashes (uid, at, build, source, file, size) VALUES (?1, ?2, 3, 'protect', 'p2.log', 1)", params![a, at + 20]).unwrap();
        assert!(!guard_report(&mut c, a, at + 20, Some(3), "p2.log", raw), "two notes in one run are one finding");
        assert_eq!(ban_of(&c, a), None, "the player was not told twice on one day");

        let flagged: String = c
            .query_row("SELECT detail FROM recode_flags WHERE uid = ?1 AND kind = 'protect' ORDER BY id DESC LIMIT 1", params![a], |r| r.get(0))
            .unwrap();
        assert!(flagged.contains("with no module"), "her list says what was found last: {flagged}");
        assert!(!flagged.contains("virtual machine's guest"), "and not only the first note: {flagged}");
        assert!(flagged.contains("\"days\":1"), "and on how many days: {flagged}");
        // …and the block under that line is there too, in a field of its own — the one that would
        // have said at once whether the thread was shellcode or a walk that missed its own entry
        assert!(flagged.contains("mem=private"), "her list carries the detail under the why: {flagged}");
        assert!(flagged.contains("tid=12"), "and the thread it was: {flagged}");
        assert!(flagged.contains("\"found\":"), "in a field of its own: {flagged}");

        // the same thing on another day inside the week: a ban, at once and for good
        c.execute("INSERT INTO crashes (uid, at, build, source, file, size) VALUES (?1, ?2, 3, 'protect', 'p3.log', 1)", params![a, at + A_DAY]).unwrap();
        assert!(guard_report(&mut c, a, at + A_DAY, Some(3), "p3.log", note), "the finding on a second day inside a week bans");
        let b = ban_of(&c, a).expect("banned");
        assert!(b.reason.contains("повторное"), "{}", b.reason);
        assert_eq!(b.until, None, "«бан сразу» is for good; a staff member lifts it");
        let auto: i64 = c.query_row("SELECT COUNT(*) FROM recode_flags WHERE uid = ?1 AND kind = 'autoban'", params![a], |r| r.get(0)).unwrap();
        assert_eq!(auto, 1, "and it is on her list");

        // a finding older than a week is only a first one again
        let b2 = add(&c, "hunter2");
        c.execute("INSERT INTO crashes (uid, at, build, source, file, size) VALUES (?1, ?2, 3, 'protect', 'old.log', 1)", params![b2, at - A_WEEK - 60]).unwrap();
        c.execute("INSERT INTO crashes (uid, at, build, source, file, size) VALUES (?1, ?2, 3, 'protect', 'new.log', 1)", params![b2, at]).unwrap();
        assert!(!guard_report(&mut c, b2, at, Some(3), "new.log", note), "an old finding is not a repeat");
        assert_eq!(ban_of(&c, b2), None);

        // her account is never banned, whatever is found
        c.execute("INSERT INTO crashes (uid, at, build, source, file, size) VALUES (?1, ?2, 3, 'protect', 'o1.log', 1)", params![OWNER, at]).unwrap();
        c.execute("INSERT INTO crashes (uid, at, build, source, file, size) VALUES (?1, ?2, 3, 'protect', 'o2.log', 1)", params![OWNER, at + A_DAY]).unwrap();
        assert!(!guard_report(&mut c, OWNER, at + A_DAY, Some(3), "o2.log", note));
        assert_eq!(ban_of(&c, OWNER), None);
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    /// A row written before the detail carried what the note said still reads on her list: the note
    /// beside the database is the same text the report came with. What is already on the line is
    /// never taken away — this only sets what is missing.
    #[test]
    fn an_old_detail_is_filled_from_the_note_it_names() {
        // the shape an older build left behind: the name of the log and nothing else
        let mut d = json!({ "build": 47, "file": "1354-1792583146-protect-75cf32.log", "times": 1 });
        let note = "Cursed Recode — the game was closed\nbuild: 47\nwhy: this machine is a virtual machine's guest\nfound: word virtual machine\nfw: Microsoft Corporation Virtual Machine\ndsk: Msft Virtual Disk\n";
        fill_from_note(&mut d, note);
        assert_eq!(d["why"], "this machine is a virtual machine's guest", "{d}");
        assert!(d["found"].as_str().unwrap().starts_with("found: word virtual machine"), "{d}");
        assert!(d["found"].as_str().unwrap().contains("dsk: Msft Virtual Disk"), "{d}");
        assert_eq!(d["build"], 47, "nothing else is touched: {d}");
        assert_eq!(d["file"], "1354-1792583146-protect-75cf32.log", "{d}");
        assert_eq!(d["times"], 1, "{d}");

        // the note nikiytiki's three rows actually carry: one line and nothing under it. Her list
        // shows that line and must not invent a `found` that the check never wrote.
        let mut thin = json!({ "build": 47, "file": "1354-1792583166-protect-0e29e6.log" });
        fill_from_note(&mut thin, "Cursed Recode — the game was closed\nbuild: 47\nwhy: this machine is a virtual machine's guest\n");
        assert_eq!(thin["why"], "this machine is a virtual machine's guest", "{thin}");
        assert!(thin.get("found").is_none(), "nothing under the line, no `found`: {thin}");

        // what is already on the line is what was found last, and this never overwrites it
        let mut last = json!({ "why": "a thread 0x7ffa9148a9e4 with no module", "found": "found: tid=12 mem=private" });
        fill_from_note(&mut last, note);
        assert_eq!(last["why"], "a thread 0x7ffa9148a9e4 with no module", "{last}");
        assert_eq!(last["found"], "found: tid=12 mem=private", "{last}");
    }

    /// A ban of an older build kept only the ban's own words — the misleading line about tools of
    /// analysis — and named no log. Its report of the same second did, and that note is what her
    /// list must show under the ban: the reason and what the check saw are two different things.
    #[test]
    fn a_row_that_never_named_its_log_finds_the_note_of_its_own_moment() {
        let (mut c, path) = fresh();
        let a = add(&c, "guestbox");
        let at = 1792583166;
        let name = "1354-1792583166-protect-0e29e6.log";
        c.execute(
            "INSERT INTO crashes (uid, at, build, source, file, size) VALUES (?1, ?2, 47, 'protect', ?3, 126)",
            params![a, at, name],
        )
        .unwrap();
        // the note beside the database, as the stub wrote it that day: the why line and nothing under it
        let dir = std::env::temp_dir().join(format!("cursed-notes-{}", random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let note = "Cursed Recode — the game was closed\nbuild: 47\nwhy: this machine is a virtual machine's guest\n";
        std::fs::write(dir.join(name), note).unwrap();
        // the ban row as an older build wrote it: the ban and its reason, and nothing else
        let mut d = json!({ "ban": 6, "reason": "Инструменты разбора игры — повторное обнаружение (2 за неделю)", "times": 1 });
        fill_row(&c, &dir, a, at, &mut d);
        assert_eq!(d["why"], "this machine is a virtual machine's guest", "{d}");
        assert_eq!(d["file"], name, "the name of the log is how she reaches the whole note: {d}");
        assert_eq!(d["reason"], "Инструменты разбора игры — повторное обнаружение (2 за неделю)", "the ban's own words stay: {d}");
        assert!(d.get("found").is_none(), "nothing under the why line, no `found`: {d}");

        // a row that already knows its note is not filled again
        let mut last = json!({ "file": name, "why": "a thread with no module" });
        fill_row(&c, &dir, a, at, &mut last);
        assert_eq!(last["why"], "a thread with no module", "{last}");

        // a note held whole in `file` is left as it came
        let mut whole = json!({ "file": "touch: patch\nwhy: says the game\n" });
        fill_row(&c, &dir, a, at, &mut whole);
        assert_eq!(whole["file"], "touch: patch\nwhy: says the game\n", "{whole}");
        assert!(whole.get("why").is_none(), "{whole}");

        // another second, another account: nothing to find, nothing invented
        let mut other = json!({ "ban": 7, "reason": "x" });
        fill_row(&c, &dir, a, at + 1, &mut other);
        assert_eq!(other.get("why"), None, "{other}");
        assert_eq!(other.get("file"), None, "{other}");
        let b = add(&c, "nobody");
        fill_row(&c, &dir, b, at, &mut other);
        assert_eq!(other.get("why"), None, "{other}");

        drop(c);
        let _ = std::fs::remove_dir_all(&dir);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    /// A folder pair tickets are made under for one test: a key of its own, and nothing else.
    fn touch_root() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("cursed-touch-{}", random_hex(4)));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// The MAC as the game writes it: this player's pair ticket, a newline, the note as it came.
    fn touch_mac_of(root: &Path, uid: i64, at: i64, body: &str) -> String {
        super::touch_mac_of(&crate::protect::pair_ticket(root, uid, at), body.as_bytes())
    }

    /// A hand on the game's own memory, one confirmed finding: the account is banned at once, and
    /// the reason names the kind and the game's own words.
    #[test]
    fn a_hand_on_the_games_memory_bans_at_once() {
        let (mut c, path) = fresh();
        let root = touch_root();
        let a = add(&c, "toucher");
        let at = now();
        let body = "touch: patch\nwhy: an old dumper wrote over our guard\nfound: tid=12 mem=private where=-\n";
        let mac = touch_mac_of(&root, a, at, body);
        assert!(touch_report(&mut c, a, at, Some(3), body, &mac, &root), "one confirmed finding is a ban");
        let b = ban_of(&c, a).expect("banned");
        assert_eq!(b.until, None, "«бан сразу» is for good; a staff member lifts it");
        assert!(b.reason.contains("Рука в памяти игры"), "{}", b.reason);
        assert!(b.reason.contains("patch"), "{}", b.reason);
        assert!(b.reason.contains("old dumper"), "{}", b.reason);
        let auto: i64 = c.query_row("SELECT COUNT(*) FROM recode_flags WHERE uid = ?1 AND kind = 'autoban'", params![a], |r| r.get(0)).unwrap();
        assert_eq!(auto, 1, "and it is on her list");
        // her list carries the note's own words beside the reason: the one line, and the block under
        // it — that block is what tells a false ban from a real one (her word, 21.10)
        let detail: String = c
            .query_row("SELECT detail FROM recode_flags WHERE uid = ?1 AND kind = 'autoban'", params![a], |r| r.get(0))
            .unwrap();
        let d: Value = serde_json::from_str(&detail).unwrap();
        assert_eq!(d["why"], json!("an old dumper wrote over our guard"));
        assert!(d["found"].as_str().unwrap().contains("tid=12 mem=private"), "{}", d["found"]);
        // her account is never banned, whatever the note says
        let body = "touch: hook\nwhy: something of hers\n";
        let mac = touch_mac_of(&root, OWNER, at, body);
        assert!(!touch_report(&mut c, OWNER, at, Some(3), body, &mac, &root));
        assert_eq!(ban_of(&c, OWNER), None);
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The same note under a MAC that closes on nothing: nobody is banned, and the claim sits on her
    /// list as `touch-unverified` — a false positive bans a paying player (her word, 18.10).
    #[test]
    fn a_touch_report_under_a_wrong_mac_bans_no_one() {
        let (mut c, path) = fresh();
        let root = touch_root();
        let a = add(&c, "liar");
        let at = now();
        let body = "touch: patch\nwhy: says the game\nfound: tid=7 mem=private where=-\n";
        let mac = touch_mac_of(&root, a + 1, at, body); // another player's ticket over the same note
        assert!(!touch_report(&mut c, a, at, Some(3), body, &mac, &root), "a MAC that closes on nothing bans no one");
        assert_eq!(ban_of(&c, a), None);
        let (kind, detail): (String, String) = c
            .query_row("SELECT kind, detail FROM recode_flags WHERE uid = ?1", params![a], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(kind, "touch-unverified");
        // the claim is the one kind of line where a false ban is decided from, so her list carries
        // what the note said under its `why:` line as well as the line itself (her word, 21.10)
        let d: Value = serde_json::from_str(&detail).unwrap();
        assert_eq!(d["why"], json!("says the game"));
        assert!(d["found"].as_str().unwrap().contains("tid=7 mem=private"), "{}", d["found"]);
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// No `X-Cursed-Mac` at all is the same as one that closes on nothing.
    #[test]
    fn a_touch_report_without_its_mac_bans_no_one() {
        let (mut c, path) = fresh();
        let root = touch_root();
        let a = add(&c, "headerless");
        let at = now();
        let body = "touch: write\nwhy: a claim with no name on it\n";
        assert!(!touch_report(&mut c, a, at, Some(3), body, "", &root), "no header, no ban");
        assert!(!touch_report(&mut c, a, at, Some(3), body, "not-hex", &root), "a header that is not hex, no ban");
        assert_eq!(ban_of(&c, a), None);
        let (rows, detail): (i64, String) = c
            .query_row("SELECT COUNT(*), COALESCE(MAX(detail), '{}') FROM recode_flags WHERE uid = ?1 AND kind = 'touch-unverified'", params![a], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(rows, 1, "one line on her list, counted");
        let d: Value = serde_json::from_str(&detail).unwrap();
        assert_eq!(d["times"], json!(2), "both claims land on the same line");
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A kind outside the six is not a finding this rule can act on, whatever the MAC says.
    #[test]
    fn a_touch_kind_outside_the_six_bans_no_one() {
        let (mut c, path) = fresh();
        let root = touch_root();
        let a = add(&c, "unknown-hand");
        let at = now();
        for body in ["touch: cheat\nwhy: a word of their own\n", "why: no touch line at all\n", "touch:\nwhy: a kind left out\n"] {
            let mac = touch_mac_of(&root, a, at, body);
            assert!(!touch_report(&mut c, a, at, Some(3), body, &mac, &root), "{body:?} is not one of the six");
        }
        assert_eq!(ban_of(&c, a), None);
        for kind in ["patch", "hook", "break", "write", "inject", "script"] {
            let body = format!("touch: {kind}\nwhy: a hand of our own words\n");
            let mac = touch_mac_of(&root, a, at, &body);
            assert!(touch_report(&mut c, a, at, Some(3), &body, &mac, &root), "{kind} is one of the six");
            assert_eq!(unban(&c, a, "test"), 1, "the next kind has to stand on its own");
        }
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The pair handed out last week is still the pair the player is running: the MAC closes under
    /// the window now or the one just turned (her word, 18.10).
    #[test]
    fn the_previous_windows_ticket_still_closes_the_mac() {
        let (mut c, path) = fresh();
        let root = touch_root();
        let a = add(&c, "old-pair");
        let at = crate::protect::PAIR_WINDOW * 3 + 1000;
        let body = "touch: break\nwhy: a breakpoint on our own code\n";
        // under the ticket of the window just turned: still theirs
        let mac = touch_mac_of(&root, a, at - crate::protect::PAIR_WINDOW, body);
        assert!(touch_report(&mut c, a, at, Some(3), body, &mac, &root), "the previous window's ticket still closes the MAC");
        assert!(ban_of(&c, a).is_some());
        // and the window before that one is nobody's
        let b = add(&c, "too-old");
        let body = "touch: inject\nwhy: a thread with no file behind it\n";
        let mac = touch_mac_of(&root, b, at - crate::protect::PAIR_WINDOW * 2, body);
        assert!(!touch_report(&mut c, b, at, Some(3), body, &mac, &root), "two windows on, no one's ticket");
        assert_eq!(ban_of(&c, b), None);
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A `touch` report is a ban's own note and is never turned away for the day's quota of crash
    /// logs (her word, 18.10).
    #[test]
    fn a_touch_report_is_never_dropped_for_the_days_quota() {
        let (mut c, path) = fresh();
        let root = touch_root();
        let a = add(&c, "flood");
        let at = now();
        for i in 0..CRASHES_A_DAY {
            c.execute("INSERT INTO crashes (uid, at, build, source, file, size) VALUES (?1, ?2, 3, 'game', ?3, 1)", params![a, at, format!("g{i}.log")]).unwrap();
        }
        assert!(crash_day_full(&c, a, "game"), "a day of crash logs is a day of crash logs");
        assert!(crash_day_full(&c, a, "protect"), "and the guard's own count is the same");
        assert!(!crash_day_full(&c, a, "touch"), "a hand on the game's own memory goes through whatever the day holds");
        // several notes in, every one taken — none turned away for the day
        for i in 0..3 {
            let body = format!("touch: script\nwhy: note number {i}\n");
            assert!(!touch_report(&mut c, a, at, Some(3), &body, "", &root), "an unverified note is taken and bans no one");
        }
        let body = "touch: script\nwhy: the one the game signed\n";
        let mac = touch_mac_of(&root, a, at, body);
        assert!(touch_report(&mut c, a, at, Some(3), body, &mac, &root), "and the confirmed one is taken too");
        assert!(ban_of(&c, a).is_some(), "the one confirmed finding among them is a ban");
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn registration_fills_up() {
        let (c, path) = fresh();
        set_registration(&c, Some("open"), Some(2)).unwrap();
        assert!(!full(&c));
        add(&c, "one");
        add(&c, "two");
        assert!(full(&c));
        let v = set_registration(&c, None, Some(3)).unwrap();
        assert_eq!(v["full"], json!(false));
        drop(c);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }
}
