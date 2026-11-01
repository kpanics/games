//! The servers closed to Cursed: she names a server and why, and no player of the client gets in there.
//!
//! An entry is a server under its name, the reason she gives (shown to whoever tries to join), and the rules it is
//! known by:
//! - `domain` — the address and every name under it (`funtime.su` also closes `mc.funtime.su`);
//! - `ip` — an address, or a range (`1.2.3.0/24`), the server really answers on;
//! - `icon` — the SHA-256 of the server's picture (a network keeps its picture on every address it has);
//! - `brand` — words in the server's brand, which it sends after the login (F3 shows it);
//! - `motd` — words in what the server says of itself in the list.
//!
//! Most of it is found here, not typed in: given an address, the server is looked up (its SRV record, its addresses)
//! and asked, the way the game asks a server in the list, for its picture and its words. An address it answers on
//! directly, with the same picture, joins the rules; an address of a shared proxy (one that serves many servers by
//! their names) does not, so no other server behind it is closed by mistake. The found rules are looked for again
//! every six hours: a server that moves is followed.
//!
//! The list goes to the client signed (Ed25519, the key made here once, `blocklist.key` next to the database): a copy
//! the client keeps cannot be edited to let a server through. `GET /api/blocklist?have=N` answers 204 while the
//! client has the latest.
//!
//! She changes it from the site's account (UID 0), and the Discord bot with its own key (`cursed-api service-key`,
//! kept in the bot's .env as CURSED_API_KEY; only its hash is kept here).
//!
//! The same signed list carries the partners' servers pinned at the top of every player's list of servers (they can
//! join them, not remove, edit or move them): `cursed-api pin <address> <name>`, `unpin <address>`, `pinned`.

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, Path as Key, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::{OsRng, RngCore};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::market::{clean_line, run};
use crate::{db, digest, fail, now, presented, who, Shared};

const KINDS: &[&str] = &["domain", "ip", "icon", "brand", "motd"];
/// How long a server is given to answer the ping.
const PING_WAIT: Duration = Duration::from_secs(4);
/// How often the found rules are looked for again.
const REFRESH: Duration = Duration::from_secs(6 * 3600);

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS blocked (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             name TEXT NOT NULL,
             reason TEXT NOT NULL,
             added INTEGER NOT NULL,
             by TEXT NOT NULL,
             gone INTEGER);
         CREATE TABLE IF NOT EXISTS blocked_rules (
             entry INTEGER NOT NULL REFERENCES blocked(id) ON DELETE CASCADE,
             kind TEXT NOT NULL,
             value TEXT NOT NULL,
             auto INTEGER NOT NULL DEFAULT 0,
             PRIMARY KEY (entry, kind, value));
         CREATE TABLE IF NOT EXISTS pinned (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             address TEXT NOT NULL,
             name TEXT NOT NULL,
             added INTEGER NOT NULL,
             gone INTEGER);
         INSERT OR IGNORE INTO settings VALUES ('blocklist_version', '0');",
    )
    .expect("make the blocklist's tables");
}

// ------------------------------------------------------------------------------------------------ the signing key

/// The key the list is signed with: made once, kept beside the database, readable by nobody else.
pub struct Signature {
    key: SigningKey,
    /// The signed list as it was last made: (version, body).
    made: Mutex<Option<(i64, String)>>,
}

impl Signature {
    pub fn open(db_path: &str) -> Signature {
        let path = key_path(db_path);
        let key = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() == 32 => SigningKey::from_bytes(&bytes.try_into().expect("32 bytes")),
            _ => {
                let mut seed = [0u8; 32];
                OsRng.fill_bytes(&mut seed);
                write_private(&path, &seed);
                SigningKey::from_bytes(&seed)
            }
        };
        Signature { key, made: Mutex::new(None) }
    }

    pub fn public_hex(&self) -> String {
        hex(self.key.verifying_key().as_bytes())
    }
}

fn key_path(db_path: &str) -> PathBuf {
    Path::new(db_path).parent().map_or_else(|| PathBuf::from("blocklist.key"), |d| d.join("blocklist.key"))
}

/// A key file only the service's user reads (the blocklist's, the networks' in beta.rs).
pub(crate) fn write_private(path: &Path, bytes: &[u8]) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path).expect("write a key");
        std::io::Write::write_all(&mut f, bytes).expect("write a key");
    }
    #[cfg(not(unix))]
    std::fs::write(path, bytes).expect("write a key");
}

/// The public half, for the client (printed by `cursed-api blocklist-key`).
pub fn public_key(db_path: &str) -> String {
    Signature::open(db_path).public_hex()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ------------------------------------------------------------------------------------------------ the rules

/// An address as it is written anywhere (`mc.FunTime.su:25565`, `play.x.ru.`, `[::1]:25565`): the host alone,
/// lowercase, and the port if one was given.
pub fn split_address(s: &str) -> Option<(String, Option<u16>)> {
    let s = s.trim().trim_start_matches("minecraft://").trim_end_matches('/');
    if s.is_empty() || s.len() > 253 + 6 {
        return None;
    }
    let (host, port) = if let Some(rest) = s.strip_prefix('[') {
        let (h, tail) = rest.split_once(']')?;
        (h.to_string(), tail.strip_prefix(':').and_then(|p| p.parse().ok()))
    } else if s.matches(':').count() == 1 {
        let (h, p) = s.split_once(':')?;
        (h.to_string(), Some(p.parse().ok()?))
    } else {
        (s.to_string(), None)
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.parse::<IpAddr>().is_ok() {
        return Some((host, port));
    }
    let ok = !host.is_empty()
        && host.contains('.')
        && host.split('.').all(|l| !l.is_empty() && l.len() <= 63 && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') && !l.starts_with('-'));
    ok.then_some((host, port))
}

/// A rule's value, made into the one form it is kept in; None when it cannot be one of its kind.
pub fn clean_rule(kind: &str, value: &str) -> Option<String> {
    let v = value.trim();
    match kind {
        "domain" => split_address(v).map(|(h, _)| h).filter(|h| h.parse::<IpAddr>().is_err()),
        "ip" => {
            if let Some((ip, bits)) = v.split_once('/') {
                let ip: IpAddr = ip.trim().parse().ok()?;
                let bits: u8 = bits.trim().parse().ok()?;
                let most = if ip.is_ipv4() { 32 } else { 128 };
                (bits >= if ip.is_ipv4() { 16 } else { 32 } && bits <= most).then(|| format!("{ip}/{bits}"))
            } else {
                split_address(v).and_then(|(h, _)| h.parse::<IpAddr>().ok()).map(|ip| ip.to_string())
            }
        }
        "icon" => (v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())).then(|| v.to_ascii_lowercase()),
        "brand" | "motd" => {
            let t = clean_line(v, 64).to_lowercase();
            (t.chars().count() >= 3).then_some(t)
        }
        _ => None,
    }
}

// ------------------------------------------------------------------------------------------------ asking a server

/// What a server says of itself when asked the way the game's server list asks.
#[derive(Clone, Default)]
pub struct Status {
    pub motd: String,
    pub version: String,
    pub online: i64,
    pub max: i64,
    pub icon: Option<Vec<u8>>,
}

impl Status {
    pub fn icon_sha(&self) -> Option<String> {
        self.icon.as_ref().map(|b| hex(&Sha256::digest(b)))
    }
}

fn varint(mut v: i32, out: &mut Vec<u8>) {
    loop {
        let mut byte = (v & 0x7F) as u8;
        v = ((v as u32) >> 7) as i32;
        if v != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if v == 0 {
            break;
        }
    }
}

async fn read_varint(s: &mut tokio::net::TcpStream) -> std::io::Result<i32> {
    let mut v: i32 = 0;
    for i in 0..5 {
        let b = s.read_u8().await?;
        v |= ((b & 0x7F) as i32) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "varint"))
}

/// Asks `addr` for its status, introducing itself as going to `host` (a proxy chooses the server by that name).
pub async fn ping(addr: SocketAddr, host: &str) -> Option<Status> {
    tokio::time::timeout(PING_WAIT, async {
        let mut s = tokio::net::TcpStream::connect(addr).await.ok()?;
        let mut body = Vec::new();
        varint(0, &mut body); // the handshake
        varint(-1, &mut body); // no particular version: only asking
        varint(host.len() as i32, &mut body);
        body.extend_from_slice(host.as_bytes());
        body.extend_from_slice(&addr.port().to_be_bytes());
        varint(1, &mut body); // next: status
        let mut packet = Vec::new();
        varint(body.len() as i32, &mut packet);
        packet.extend_from_slice(&body);
        packet.extend_from_slice(&[1, 0]); // the status request
        s.write_all(&packet).await.ok()?;
        let length = read_varint(&mut s).await.ok()?;
        if !(1..=2_000_000).contains(&length) {
            return None;
        }
        let mut data = vec![0u8; length as usize];
        s.read_exact(&mut data).await.ok()?;
        let mut at = 0;
        let mut next = || -> Option<i32> {
            let mut v: i32 = 0;
            for i in 0..5 {
                let b = *data.get(at)?;
                at += 1;
                v |= ((b & 0x7F) as i32) << (7 * i);
                if b & 0x80 == 0 {
                    return Some(v);
                }
            }
            None
        };
        if next()? != 0 {
            return None;
        }
        let len = next()? as usize;
        let text = std::str::from_utf8(data.get(at..at + len)?).ok()?;
        parse_status(text)
    })
    .await
    .ok()
    .flatten()
}

pub fn parse_status(text: &str) -> Option<Status> {
    let v: Value = serde_json::from_str(text).ok()?;
    let icon = v["favicon"].as_str().and_then(|f| f.strip_prefix("data:image/png;base64,")).and_then(|b| base64(&b.replace('\n', "")));
    Some(Status {
        motd: plain(&v["description"]),
        version: v["version"]["name"].as_str().map(|s| strip_codes(s)).unwrap_or_default(),
        online: v["players"]["online"].as_i64().unwrap_or(0),
        max: v["players"]["max"].as_i64().unwrap_or(0),
        icon,
    })
}

/// A text component of the game as plain words (its colour codes out).
pub fn plain(v: &Value) -> String {
    fn walk(v: &Value, out: &mut String) {
        match v {
            Value::String(s) => out.push_str(s),
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => {
                if let Some(t) = o.get("text") {
                    walk(t, out);
                }
                if let Some(e) = o.get("extra") {
                    walk(e, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    walk(v, &mut out);
    strip_codes(&out).lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

fn strip_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

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

/// A server looked up and asked: where it really is, and what it says.
pub struct Probe {
    pub host: String,
    pub port: u16,
    /// The name the SRV record points to, when there is one.
    pub target: Option<String>,
    /// The main domains of the host and of the target (`funtime.su` for `mc.funtime.su`) that answer as this very
    /// server: closing them closes every name under them. One that answers as something else (a free hosting's
    /// `aternos.me`) is not here.
    pub bases: Vec<String>,
    /// Its addresses, each whether it answers as this very server (then it is safe to close) or is shared.
    pub addresses: Vec<(IpAddr, bool)>,
    pub status: Option<Status>,
}

pub async fn probe(address: &str) -> Option<Probe> {
    let (host, port) = split_address(address)?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        let port = port.unwrap_or(25565);
        let status = ping(SocketAddr::new(ip, port), &host).await;
        return Some(Probe { host, port, target: None, bases: Vec::new(), addresses: vec![(ip, status.is_some())], status });
    }
    // The SRV record comes first, as the game does it (unless a port was given).
    let mut target = None;
    let mut real_port = port.unwrap_or(25565);
    if port.is_none() {
        if let Ok(resolver) = hickory_resolver::TokioAsyncResolver::tokio_from_system_conf() {
            if let Ok(srv) = resolver.srv_lookup(format!("_minecraft._tcp.{host}.")).await {
                if let Some(r) = srv.iter().next() {
                    target = Some(r.target().to_utf8().trim_end_matches('.').to_ascii_lowercase());
                    real_port = r.port();
                }
            }
        }
    }
    let lookup = target.clone().unwrap_or_else(|| host.clone());
    let ips: Vec<IpAddr> = tokio::net::lookup_host((lookup.as_str(), real_port)).await.map(|a| a.map(|s| s.ip()).collect()).unwrap_or_default();
    let mut ips_unique: Vec<IpAddr> = Vec::new();
    for ip in ips {
        if !ips_unique.contains(&ip) {
            ips_unique.push(ip);
        }
    }
    let status = match ips_unique.first() {
        Some(ip) => ping(SocketAddr::new(*ip, real_port), &host).await,
        None => None,
    };
    // Is an address this server's own? Asked by its bare address, it must answer as the server does by its name.
    let mut addresses = Vec::new();
    for ip in ips_unique.into_iter().take(8) {
        let own = match &status {
            Some(named) => match ping(SocketAddr::new(ip, real_port), &ip.to_string()).await {
                Some(bare) => same_server(named, &bare),
                None => false,
            },
            None => false,
        };
        addresses.push((ip, own));
    }
    // The main domains, when they are the same server.
    let mut bases = Vec::new();
    if let Some(named) = &status {
        for name in std::iter::once(&host).chain(target.iter()) {
            let Some(base) = main_domain(name) else { continue };
            if base == *name || bases.contains(&base) {
                continue;
            }
            if let Some(b) = Box::pin(probe_status(&base)).await {
                if same_server(named, &b) {
                    bases.push(base);
                }
            }
        }
    }
    Some(Probe { host, port: real_port, target, bases, addresses, status })
}

/// Only what a name answers (its SRV record followed), without looking further.
async fn probe_status(host: &str) -> Option<Status> {
    let mut port = 25565;
    let mut lookup = host.to_string();
    if let Ok(resolver) = hickory_resolver::TokioAsyncResolver::tokio_from_system_conf() {
        if let Ok(srv) = resolver.srv_lookup(format!("_minecraft._tcp.{host}.")).await {
            if let Some(r) = srv.iter().next() {
                lookup = r.target().to_utf8().trim_end_matches('.').to_string();
                port = r.port();
            }
        }
    }
    let ip = tokio::net::lookup_host((lookup.as_str(), port)).await.ok()?.next()?.ip();
    ping(SocketAddr::new(ip, port), host).await
}

/// Second-level names that are someone's own under a shared end (`x.com.ru`, `x.co.uk`): the main domain is three
/// labels there.
const SHARED_ENDS: &[&str] = &[
    "com.ru", "net.ru", "org.ru", "pp.ru", "msk.ru", "spb.ru", "com.ua", "net.ua", "org.ua", "pp.ua", "in.ua", "kiev.ua", "com.by", "net.by", "com.kz", "org.kz",
    "co.uk", "org.uk", "com.br", "com.tr", "com.pl", "net.pl", "com.au", "co.jp", "co.kr", "eu.org", "us.to",
];

/// The domain a name belongs to: `funtime.su` for `mc.funtime.su`, `x.com.ru` for `mc.x.com.ru`.
pub fn main_domain(name: &str) -> Option<String> {
    if name.parse::<IpAddr>().is_ok() {
        return None;
    }
    let labels: Vec<&str> = name.split('.').collect();
    if labels.len() < 2 {
        return None;
    }
    let two = labels[labels.len() - 2..].join(".");
    if SHARED_ENDS.contains(&two.as_str()) {
        (labels.len() >= 3).then(|| labels[labels.len() - 3..].join("."))
    } else {
        Some(two)
    }
}

/// Two answers are the same server: the same picture, or with no picture the same words.
fn same_server(a: &Status, b: &Status) -> bool {
    match (a.icon_sha(), b.icon_sha()) {
        (Some(x), Some(y)) => x == y,
        (None, None) => !a.motd.is_empty() && a.motd == b.motd,
        _ => false,
    }
}

fn probe_json(p: &Probe) -> Value {
    json!({
        "host": p.host,
        "port": p.port,
        "target": p.target,
        "bases": p.bases,
        "addresses": p.addresses.iter().map(|(ip, own)| json!({ "ip": ip.to_string(), "own": own })).collect::<Vec<_>>(),
        "online": p.status.is_some(),
        "motd": p.status.as_ref().map(|s| s.motd.clone()),
        "version": p.status.as_ref().map(|s| s.version.clone()),
        "players": p.status.as_ref().map(|s| json!({ "online": s.online, "max": s.max })),
        "icon": p.status.as_ref().and_then(|s| s.icon.as_ref()).map(|b| b64(b)),
        "icon_sha": p.status.as_ref().and_then(|s| s.icon_sha()),
    })
}

fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The rules a probe finds: the host (and its SRV target) as domains, its own addresses, its picture.
fn found_rules(p: &Probe) -> Vec<(&'static str, String)> {
    let mut rules = Vec::new();
    if p.host.parse::<IpAddr>().is_err() {
        rules.push(("domain", p.host.clone()));
    }
    if let Some(t) = &p.target {
        rules.push(("domain", t.clone()));
    }
    for b in &p.bases {
        rules.push(("domain", b.clone()));
    }
    for (ip, own) in &p.addresses {
        if *own || p.host.parse::<IpAddr>().is_ok() {
            rules.push(("ip", ip.to_string()));
        }
    }
    if let Some(sha) = p.status.as_ref().and_then(|s| s.icon_sha()) {
        rules.push(("icon", sha));
    }
    rules
}

// ------------------------------------------------------------------------------------------------ the list

fn version(c: &Connection) -> i64 {
    c.query_row("SELECT value FROM settings WHERE name = 'blocklist_version'", [], |r| r.get::<_, String>(0)).ok().and_then(|v| v.parse().ok()).unwrap_or(0)
}

pub(crate) fn bump(c: &Connection) {
    let _ = c.execute("UPDATE settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT) WHERE name = 'blocklist_version'", []);
}

/// Every server in force with its rules; `all` also the ones taken off (for her list), with who added them.
fn entries(c: &Connection, all: bool) -> Vec<Value> {
    let q = if all { "SELECT id, name, reason, added, by, gone FROM blocked ORDER BY id DESC" } else { "SELECT id, name, reason, added, by, gone FROM blocked WHERE gone IS NULL ORDER BY id" };
    let Ok(mut st) = c.prepare(q) else { return Vec::new() };
    let rows: Vec<(i64, String, String, i64, String, Option<i64>)> =
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))).map(|m| m.flatten().collect()).unwrap_or_default();
    rows.into_iter()
        .map(|(id, name, reason, added, by, gone)| {
            let mut v = json!({ "id": id, "name": name, "reason": reason, "since": added });
            let mut rules: serde_json::Map<String, Value> = KINDS.iter().map(|k| (format!("{k}s"), json!([]))).collect();
            let mut auto = Vec::new();
            if let Ok(mut rs) = c.prepare("SELECT kind, value, auto FROM blocked_rules WHERE entry = ?1 ORDER BY kind, value") {
                for (kind, value, is_auto) in rs.query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))).map(|m| m.flatten().collect::<Vec<_>>()).unwrap_or_default() {
                    if let Some(Value::Array(list)) = rules.get_mut(&format!("{kind}s")) {
                        list.push(json!(value));
                    }
                    if is_auto != 0 {
                        auto.push(json!(format!("{kind}:{value}")));
                    }
                }
            }
            for (k, list) in rules {
                v[k] = list;
            }
            if all {
                v["by"] = json!(by);
                v["gone"] = json!(gone);
                v["auto"] = json!(auto);
            }
            v
        })
        .collect()
}

/// The list as the client gets it: its text, and the signature of exactly that text.
fn signed_body(c: &Connection, sig: &Signature) -> (i64, String) {
    let v = version(c);
    if let Some((made_v, body)) = sig.made.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if *made_v == v {
            return (v, body.clone());
        }
    }
    // The mods' rules ride along (marks.rs): which are cheats, which some servers ban.
    let list = json!({ "v": v, "at": now(), "servers": entries(c, false), "pinned": pinned(c, false), "mods": crate::marks::rules(c) }).to_string();
    let signature = hex(&sig.key.sign(list.as_bytes()).to_bytes());
    let body = json!({ "list": list, "sig": signature }).to_string();
    *sig.made.lock().unwrap_or_else(|e| e.into_inner()) = Some((v, body.clone()));
    (v, body)
}

// ------------------------------------------------------------------------------------------------ pinned servers

/// The partners' servers in force, in the order they were pinned (`all`: the unpinned too, with when).
pub fn pinned(c: &Connection, all: bool) -> Vec<Value> {
    let q = if all { "SELECT address, name, added, gone FROM pinned ORDER BY id" } else { "SELECT address, name, added, gone FROM pinned WHERE gone IS NULL ORDER BY id" };
    let Ok(mut st) = c.prepare(q) else { return Vec::new() };
    st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<i64>>(3)?)))
        .map(|m| {
            m.flatten()
                .map(|(address, name, added, gone)| if all { json!({ "address": address, "name": name, "added": added, "gone": gone }) } else { json!({ "address": address, "name": name }) })
                .collect()
        })
        .unwrap_or_default()
}

/// A server pinned for every player (its name as the list shows it); pinned again, it takes the new name.
pub fn pin(c: &Connection, address: &str, name: &str) -> Result<(), String> {
    let address = address.trim().to_lowercase();
    let name = clean_line(name, 48);
    if split_address(&address).is_none() {
        return Err(format!("not an address: {address}"));
    }
    if name.is_empty() {
        return Err("a name is needed".into());
    }
    c.execute("UPDATE pinned SET gone = ?1 WHERE address = ?2 AND gone IS NULL", params![now(), address]).map_err(|e| e.to_string())?;
    c.execute("INSERT INTO pinned (address, name, added) VALUES (?1, ?2, ?3)", params![address, name, now()]).map_err(|e| e.to_string())?;
    bump(c);
    Ok(())
}

/// Unpinned: it leaves the players' lists (whoever added it themselves keeps their own).
pub fn unpin(c: &Connection, address: &str) -> Result<(), String> {
    let n = c.execute("UPDATE pinned SET gone = ?1 WHERE address = ?2 AND gone IS NULL", params![now(), address.trim().to_lowercase()]).map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("not pinned: {address}"));
    }
    bump(c);
    Ok(())
}

#[derive(Deserialize)]
struct Have {
    have: Option<i64>,
}

async fn list(State(app): State<Shared>, Query(q): Query<Have>) -> Response {
    let app2 = app.clone();
    let (v, body) = db(&app, move |c| signed_body(c, &app2.blocklist)).await;
    if q.have == Some(v) {
        return StatusCode::NO_CONTENT.into_response();
    }
    ([(axum::http::header::CONTENT_TYPE, "application/json")], body).into_response()
}

// ------------------------------------------------------------------------------------------------ her doors

/// Who may change the list: her own account, or the bot with its key. The answer is who it was, for the record.
pub(crate) async fn keeper(app: &Shared, h: &HeaderMap) -> Result<String, Response> {
    if let Some(key) = h.get("x-cursed-key").and_then(|v| v.to_str().ok()) {
        let hash = hex(&digest(key.trim()));
        let known = db(app, move |c| c.query_row("SELECT value FROM settings WHERE name = 'service_key'", [], |r| r.get::<_, String>(0)).optional().ok().flatten()).await;
        if known.as_deref() == Some(hash.as_str()) {
            let who = h.get("x-cursed-by").and_then(|v| v.to_str().ok()).map(|s| clean_line(s, 80)).unwrap_or_default();
            return Ok(if who.is_empty() { "bot".to_string() } else { format!("bot:{who}") });
        }
        return Err(fail(StatusCode::UNAUTHORIZED, "key"));
    }
    let Some((token, _)) = presented(h) else {
        return Err(fail(StatusCode::UNAUTHORIZED, "signed-out"));
    };
    let w = db(app, move |c| who(c, &token)).await.ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
    if !crate::is_owner(w.uid) {
        return Err(fail(StatusCode::FORBIDDEN, "not-yours"));
    }
    Ok(format!("uid:{}", w.uid))
}

#[derive(Deserialize)]
struct Asked {
    address: String,
}

/// A server looked up and asked, before it is added (the bot shows it for a yes).
async fn look(State(app): State<Shared>, h: HeaderMap, Json(a): Json<Asked>) -> Response {
    run(async {
        keeper(&app, &h).await?;
        let p = probe(&a.address).await.ok_or_else(|| fail(StatusCode::BAD_REQUEST, "address"))?;
        Ok(Json(probe_json(&p)).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Add {
    name: String,
    reason: String,
    /// Addresses the server is known by; each is looked up and asked.
    #[serde(default)]
    addresses: Vec<String>,
    /// Rules typed in by hand: {"kind": "brand", "value": "FunTime"}.
    #[serde(default)]
    rules: Vec<RuleIn>,
}

#[derive(Deserialize)]
struct RuleIn {
    kind: String,
    value: String,
}

async fn add(State(app): State<Shared>, h: HeaderMap, Json(a): Json<Add>) -> Response {
    run(async {
        let by = keeper(&app, &h).await?;
        let name = clean_line(&a.name, 48);
        let reason = clean_line(&a.reason, 400);
        if name.is_empty() || reason.chars().count() < 3 {
            return Err(fail(StatusCode::BAD_REQUEST, "name-or-reason"));
        }
        let mut rules: Vec<(String, String, bool)> = Vec::new();
        let mut probes = Vec::new();
        for address in a.addresses.iter().take(8) {
            let Some(p) = probe(address).await else {
                return Err(fail(StatusCode::BAD_REQUEST, "address"));
            };
            for (i, (kind, value)) in found_rules(&p).into_iter().enumerate() {
                // The address as it was given is hers; what was found beside it is the server's own and refreshed.
                rules.push((kind.to_string(), value, i != 0));
            }
            probes.push(probe_json(&p));
        }
        for r in a.rules.iter().take(16) {
            let value = clean_rule(&r.kind, &r.value).ok_or_else(|| fail(StatusCode::BAD_REQUEST, "rule"))?;
            rules.push((r.kind.clone(), value, false));
        }
        if rules.is_empty() {
            return Err(fail(StatusCode::BAD_REQUEST, "no-rules"));
        }
        let id = db(&app, move |c| -> rusqlite::Result<i64> {
            let tx = c.transaction()?;
            tx.execute("INSERT INTO blocked (name, reason, added, by) VALUES (?1, ?2, ?3, ?4)", params![name, reason, now(), by])?;
            let id = tx.last_insert_rowid();
            for (kind, value, auto) in &rules {
                tx.execute("INSERT OR IGNORE INTO blocked_rules (entry, kind, value, auto) VALUES (?1, ?2, ?3, ?4)", params![id, kind, value, *auto as i64])?;
            }
            bump(&tx);
            tx.commit()?;
            Ok(id)
        })
        .await
        .map_err(|_| crate::market::busy())?;
        let entry = db(&app, move |c| entries(c, true).into_iter().find(|e| e["id"] == json!(id))).await;
        Ok(Json(json!({ "entry": entry, "probes": probes })).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct Change {
    name: Option<String>,
    reason: Option<String>,
    #[serde(default)]
    addresses: Vec<String>,
    #[serde(default)]
    rules: Vec<RuleIn>,
    /// Rules to take away: "kind:value".
    #[serde(default)]
    drop: Vec<String>,
}

async fn change(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>, Json(ch): Json<Change>) -> Response {
    run(async {
        keeper(&app, &h).await?;
        let mut rules: Vec<(String, String, bool)> = Vec::new();
        for address in ch.addresses.iter().take(8) {
            let p = probe(address).await.ok_or_else(|| fail(StatusCode::BAD_REQUEST, "address"))?;
            for (i, (kind, value)) in found_rules(&p).into_iter().enumerate() {
                rules.push((kind.to_string(), value, i != 0));
            }
        }
        for r in ch.rules.iter().take(16) {
            let value = clean_rule(&r.kind, &r.value).ok_or_else(|| fail(StatusCode::BAD_REQUEST, "rule"))?;
            rules.push((r.kind.clone(), value, false));
        }
        let name = ch.name.map(|n| clean_line(&n, 48)).filter(|n| !n.is_empty());
        let reason = ch.reason.map(|r| clean_line(&r, 400)).filter(|r| r.chars().count() >= 3);
        let drop: Vec<(String, String)> = ch.drop.iter().filter_map(|d| d.split_once(':')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let done = db(&app, move |c| -> rusqlite::Result<bool> {
            let tx = c.transaction()?;
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM blocked WHERE id = ?1 AND gone IS NULL)", params![id], |r| r.get(0))?;
            if !exists {
                return Ok(false);
            }
            if let Some(n) = &name {
                tx.execute("UPDATE blocked SET name = ?2 WHERE id = ?1", params![id, n])?;
            }
            if let Some(r) = &reason {
                tx.execute("UPDATE blocked SET reason = ?2 WHERE id = ?1", params![id, r])?;
            }
            for (kind, value, auto) in &rules {
                tx.execute("INSERT OR IGNORE INTO blocked_rules (entry, kind, value, auto) VALUES (?1, ?2, ?3, ?4)", params![id, kind, value, *auto as i64])?;
            }
            for (kind, value) in &drop {
                tx.execute("DELETE FROM blocked_rules WHERE entry = ?1 AND kind = ?2 AND value = ?3", params![id, kind, value])?;
            }
            bump(&tx);
            tx.commit()?;
            Ok(true)
        })
        .await
        .map_err(|_| crate::market::busy())?;
        if !done {
            return Err(fail(StatusCode::NOT_FOUND, "missing"));
        }
        let entry = db(&app, move |c| entries(c, true).into_iter().find(|e| e["id"] == json!(id))).await;
        Ok(Json(json!({ "entry": entry })).into_response())
    })
    .await
}

/// Taken off the list (kept, with when, for her history).
async fn remove(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        keeper(&app, &h).await?;
        let n = db(&app, move |c| {
            let n = c.execute("UPDATE blocked SET gone = ?2 WHERE id = ?1 AND gone IS NULL", params![id, now()]).unwrap_or(0);
            if n > 0 {
                bump(c);
            }
            n
        })
        .await;
        if n == 0 {
            return Err(fail(StatusCode::NOT_FOUND, "missing"));
        }
        Ok(Json(json!({})).into_response())
    })
    .await
}

/// Put back on the list (a server taken off by mistake).
async fn restore(State(app): State<Shared>, h: HeaderMap, Key(id): Key<i64>) -> Response {
    run(async {
        keeper(&app, &h).await?;
        let n = db(&app, move |c| {
            let n = c.execute("UPDATE blocked SET gone = NULL WHERE id = ?1 AND gone IS NOT NULL", params![id]).unwrap_or(0);
            if n > 0 {
                bump(c);
            }
            n
        })
        .await;
        if n == 0 {
            return Err(fail(StatusCode::NOT_FOUND, "missing"));
        }
        let entry = db(&app, move |c| entries(c, true).into_iter().find(|e| e["id"] == json!(id))).await;
        Ok(Json(json!({ "entry": entry })).into_response())
    })
    .await
}

/// Everything, taken off too, with who added it: for her list and the bot's choices.
async fn all(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async {
        keeper(&app, &h).await?;
        let list = db(&app, |c| entries(c, true)).await;
        Ok(Json(json!({ "servers": list })).into_response())
    })
    .await
}

pub fn routes() -> Router<Shared> {
    let json = || DefaultBodyLimit::max(16 * 1024);
    Router::new()
        .route("/api/blocklist", get(list).post(add).layer(json()))
        .route("/api/blocklist/all", get(all))
        .route("/api/blocklist/look", post(look).layer(json()))
        .route("/api/blocklist/{id}", patch(change).delete(remove).layer(json()))
        .route("/api/blocklist/{id}/restore", post(restore))
}

/// Every six hours, each server's found rules are looked for again: a new address or picture joins, what was found
/// before stays (a server that answers nothing this time keeps what it had).
pub async fn refresher(app: Shared) {
    loop {
        tokio::time::sleep(Duration::from_secs(90)).await;
        let targets: Vec<(i64, String)> = db(&app, |c| {
            let mut st = c.prepare("SELECT r.entry, r.value FROM blocked_rules r JOIN blocked b ON b.id = r.entry WHERE b.gone IS NULL AND r.kind = 'domain' AND r.auto = 0").ok()?;
            let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).ok()?.flatten().collect();
            Some(rows)
        })
        .await
        .unwrap_or_default();
        let mut found: Vec<(i64, String, String)> = Vec::new();
        for (id, domain) in targets {
            if let Some(p) = probe(&domain).await {
                for (kind, value) in found_rules(&p).into_iter().skip(1) {
                    found.push((id, kind.to_string(), value));
                }
            }
        }
        db(&app, move |c| {
            let mut new = 0;
            for (id, kind, value) in &found {
                new += c.execute("INSERT OR IGNORE INTO blocked_rules (entry, kind, value, auto) VALUES (?1, ?2, ?3, 1)", params![id, kind, value]).unwrap_or(0);
            }
            if new > 0 {
                bump(c);
            }
        })
        .await;
        tokio::time::sleep(REFRESH).await;
    }
}

/// A new key for the bot (the old one stops working); printed once, only its hash kept.
pub fn service_key(c: &Connection) -> String {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    let key = hex(&b);
    c.execute("INSERT INTO settings (name, value) VALUES ('service_key', ?1) ON CONFLICT(name) DO UPDATE SET value = excluded.value", params![hex(&digest(&key))]).expect("keep the key");
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signature as Sig, Verifier};

    #[test]
    fn addresses_are_split_the_way_they_are_typed() {
        assert_eq!(split_address("mc.FunTime.su"), Some(("mc.funtime.su".into(), None)));
        assert_eq!(split_address("play.x.ru.:25566"), Some(("play.x.ru".into(), Some(25566))));
        assert_eq!(split_address("1.2.3.4:25565"), Some(("1.2.3.4".into(), Some(25565))));
        assert_eq!(split_address("[2a01::1]:25565"), Some(("2a01::1".into(), Some(25565))));
        assert_eq!(split_address("minecraft://mc.rw.ru/"), Some(("mc.rw.ru".into(), None)));
        for bad in ["", "localhost", "a..b", "-x.ru", "x.ru:port", "x ru.com"] {
            assert!(split_address(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn rules_are_kept_in_one_form() {
        assert_eq!(clean_rule("domain", "MC.FunTime.su:25565"), Some("mc.funtime.su".into()));
        assert_eq!(clean_rule("domain", "1.2.3.4"), None);
        assert_eq!(clean_rule("ip", "1.2.3.4:25565"), Some("1.2.3.4".into()));
        assert_eq!(clean_rule("ip", "1.2.3.0/24"), Some("1.2.3.0/24".into()));
        assert_eq!(clean_rule("ip", "1.2.3.0/8"), None, "too wide a range");
        assert_eq!(clean_rule("icon", &"AB".repeat(32)), Some("ab".repeat(32)));
        assert_eq!(clean_rule("brand", "  FunTime  "), Some("funtime".into()));
        assert_eq!(clean_rule("brand", "ab"), None);
        assert_eq!(clean_rule("nope", "x"), None);
    }

    #[test]
    fn main_domains() {
        assert_eq!(main_domain("mc.funtime.su").as_deref(), Some("funtime.su"));
        assert_eq!(main_domain("funtime.su").as_deref(), Some("funtime.su"));
        assert_eq!(main_domain("play.x.com.ru").as_deref(), Some("x.com.ru"));
        assert_eq!(main_domain("com.ru"), None);
        assert_eq!(main_domain("1.2.3.4"), None);
    }

    #[test]
    fn a_status_reads_like_the_game_reads_it() {
        let s = parse_status(r#"{"version":{"name":"§6Velocity 1.8-1.21","protocol":-1},"players":{"max":5000,"online":1234},"description":{"text":"","extra":[{"text":"§bFun","bold":true},{"text":"Time\n  §7лучший   сервер"}]},"favicon":"data:image/png;base64,iVBORw0KGgo="}"#).unwrap();
        assert_eq!(s.motd, "FunTime\nлучший сервер");
        assert_eq!(s.version, "Velocity 1.8-1.21");
        assert_eq!((s.online, s.max), (1234, 5000));
        assert_eq!(s.icon.as_deref(), Some(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A][..]));
        assert_eq!(b64(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]), "iVBORw0KGgo=");
    }

    #[test]
    fn the_list_is_signed_and_the_signature_holds_only_for_its_text() {
        let dir = std::env::temp_dir().join(format!("cursed-bl-{}", crate::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("x.db");
        let c = crate::open(db.to_str().unwrap());
        let before = version(&c);
        c.execute("INSERT INTO blocked (name, reason, added, by) VALUES ('FunTime', 'ворует', 1, 'test')", []).unwrap();
        c.execute("INSERT INTO blocked_rules (entry, kind, value) VALUES (1, 'domain', 'funtime.su')", []).unwrap();
        bump(&c);
        let sig = Signature::open(db.to_str().unwrap());
        let (v, body) = signed_body(&c, &sig);
        assert_eq!(v, before + 1);
        let outer: Value = serde_json::from_str(&body).unwrap();
        let text = outer["list"].as_str().unwrap();
        let sig_bytes: Vec<u8> = (0..64).map(|i| u8::from_str_radix(&outer["sig"].as_str().unwrap()[i * 2..i * 2 + 2], 16).unwrap()).collect();
        let s = Sig::from_bytes(&sig_bytes.try_into().unwrap());
        let key = sig.key.verifying_key();
        assert!(key.verify(text.as_bytes(), &s).is_ok());
        assert!(key.verify(text.replace("funtime", "funtimf").as_bytes(), &s).is_err());
        let list: Value = serde_json::from_str(text).unwrap();
        assert_eq!(list["servers"][0]["domains"][0], "funtime.su");
        assert!(list["mods"].as_array().unwrap().iter().any(|m| m["name"] == "Meteor Client" && m["cheat"] == true));
        // The same key next time: the client's copy of the public half stays right.
        assert_eq!(Signature::open(db.to_str().unwrap()).public_hex(), sig.public_hex());
        drop(c);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pinned_servers_go_with_the_list_and_a_change_makes_a_new_version() {
        let dir = std::env::temp_dir().join(format!("cursed-pin-{}", crate::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("x.db");
        let c = crate::open(db.to_str().unwrap());
        let sig = Signature::open(db.to_str().unwrap());
        assert!(pin(&c, "not an address", "x").is_err());
        pin(&c, " MC.ForceGrief.pw ", "ForceGrief").unwrap();
        pin(&c, "mc.other.net:25570", "Other").unwrap();
        // Pinned again: the new name, once.
        pin(&c, "mc.forcegrief.pw", "ForceGrief 1.21").unwrap();
        let (v, body) = signed_body(&c, &sig);
        let outer: Value = serde_json::from_str(&body).unwrap();
        let list: Value = serde_json::from_str(outer["list"].as_str().unwrap()).unwrap();
        assert_eq!(list["pinned"], json!([{ "address": "mc.other.net:25570", "name": "Other" }, { "address": "mc.forcegrief.pw", "name": "ForceGrief 1.21" }]));
        unpin(&c, "mc.other.net:25570").unwrap();
        assert!(unpin(&c, "mc.other.net:25570").is_err());
        let (v2, body2) = signed_body(&c, &sig);
        assert!(v2 > v);
        let list: Value = serde_json::from_str(serde_json::from_str::<Value>(&body2).unwrap()["list"].as_str().unwrap()).unwrap();
        assert_eq!(list["pinned"].as_array().unwrap().len(), 1);
        drop(c);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
