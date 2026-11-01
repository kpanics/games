//! The client's files, for the loader to download (her decisions, 30.09.2026: everything from this server, only for
//! the beta's testers, every update a must).
//!
//! Kept under `CURSED_DIST` (/var/lib/cursed-dist), which the service reads and nginx serves (only through the app's
//! X-Accel-Redirect, after it asked who wants it):
//! - `objects/<2>/<sha256>` — every file of every build, once, never changed; beside it `<sha256>.zst` where
//!   compressing it with zstd saves something (the jars, the sounds and the pictures are packed already);
//! - `patches/<from>-<to>.zst` — a file of a build made out of the same file of one of the three builds before it
//!   (`zstd --patch-from`), where that is much smaller than the file;
//! - `assets/objects/<2>/<sha1>` — the game's own assets (sounds, languages, textures past the jar's), laid out as
//!   Mojang lays them out, by the SHA-1 their index names them by;
//! - `incoming/<name>/` — a build on its way in: `release.json` and `objects/<sha256>` (only what is not here yet).
//!
//! A build is published by `cursed release publish <name>` (her PC's tools/Release.java uploads it and asks for it):
//! its files are checked against their hashes and taken into the store, compressed, patched, and listed in a manifest
//! the server signs (Ed25519, `dist.key` beside the database; the public half, `cursed dist-key`, goes into the
//! loader). The channel then points at it; `cursed release rollback beta` points it back at the build before.
//!
//! The loader gets the manifest from POST /api/loader/start (beta.rs), which also lets its account download for a few
//! hours: GET /api/dist/object/{sha256}[?z=1], /api/dist/patch/{from}/{to}, /api/dist/asset/{sha1}.

use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{Path as Key, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::{OsRng, RngCore};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::{Digest, Sha256};

use crate::market::run;
use crate::{db, digest, fail, now, presented, who, Shared};

/// What a file of a build is to the loader (where it goes, how it is used).
pub const KINDS: &[&str] = &["code", "sounds", "library", "native", "watchdog", "runtime", "loader", "assets-index", "config"];
/// Files this big are patched from the builds before (smaller ones are sent whole).
const PATCH_FROM: u64 = 512 * 1024;
/// zstd reads back this far when patching (2^27 = 128 MB): bigger files are sent whole.
const PATCH_WINDOW: u64 = 1 << 27;
/// Files this big are worth compressing.
const COMPRESS_FROM: u64 = 64 * 1024;
/// How long a download's leave is trusted before it is asked again.
const ACCESS_CHECK: Duration = Duration::from_secs(30);

pub struct Dist {
    pub root: PathBuf,
    key: SigningKey,
    /// A token's leave to download as last asked: (account, the channels it may download from and until when, when
    /// asked).
    access: Mutex<HashMap<Vec<u8>, (i64, Vec<(String, i64)>, Instant)>>,
    /// Which channels' builds name each file of the store (its SHA-256), as the releases were when last counted (their
    /// last id): a file goes only to a loader started for one of them (her word, 16.10: Cursed's and Recode's apart).
    owners: Mutex<(i64, HashMap<String, Vec<String>>)>,
}

impl Dist {
    pub fn open(db_path: &str, root: PathBuf) -> Dist {
        let path = key_path(db_path);
        let key = match fs::read(&path) {
            Ok(bytes) if bytes.len() == 32 => SigningKey::from_bytes(&bytes.try_into().expect("32 bytes")),
            _ => {
                let mut seed = [0u8; 32];
                OsRng.fill_bytes(&mut seed);
                write_private(&path, &seed);
                SigningKey::from_bytes(&seed)
            }
        };
        Dist { root, key, access: Mutex::new(HashMap::new()), owners: Mutex::new((-1, HashMap::new())) }
    }

    pub fn public_hex(&self) -> String {
        hex(self.key.verifying_key().as_bytes())
    }

    /// The game's start, confirmed (beta.rs): signed with the builds' key, so a server of someone else's cannot say yes
    /// for us. The game's own random words are in it (an old answer does not fit a new start).
    pub fn sign_start(&self, nonce: &str, ticket: &str, uid: i64, build: i64) -> String {
        hex(&self.key.sign(start_message(nonce, ticket, uid, build).as_bytes()).to_bytes())
    }

    fn object(&self, sha: &str) -> PathBuf {
        self.root.join("objects").join(&sha[..2]).join(sha)
    }

    fn asset(&self, sha1: &str) -> PathBuf {
        self.root.join("assets").join("objects").join(&sha1[..2]).join(sha1)
    }

    fn patch(&self, from: &str, to: &str) -> PathBuf {
        self.root.join("patches").join(format!("{from}-{to}.zst"))
    }
}

fn key_path(db_path: &str) -> PathBuf {
    Path::new(db_path).parent().map_or_else(|| PathBuf::from("dist.key"), |d| d.join("dist.key"))
}

fn write_private(path: &Path, bytes: &[u8]) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path).expect("write the manifests' key");
        std::io::Write::write_all(&mut f, bytes).expect("write the manifests' key");
    }
    #[cfg(not(unix))]
    fs::write(path, bytes).expect("write the manifests' key");
}

/// What a confirmed start signs. Its first line keeps it apart from a manifest (that is JSON).
pub fn start_message(nonce: &str, ticket: &str, uid: i64, build: i64) -> String {
    format!("cursed-client-verify/1
{nonce}
{ticket}
{uid}
{build}")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS releases (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             channel TEXT NOT NULL,
             build INTEGER NOT NULL UNIQUE,
             version TEXT NOT NULL,
             notes TEXT NOT NULL,
             manifest TEXT NOT NULL,
             sig TEXT NOT NULL,
             at INTEGER NOT NULL,
             by TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS channels (
             name TEXT PRIMARY KEY,
             release INTEGER NOT NULL REFERENCES releases(id));
         -- a loader's leave to download, a channel's files only (beta.rs and recode.rs grant it at a start)
         CREATE TABLE IF NOT EXISTS dist_access (
             uid INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             channel TEXT NOT NULL,
             until INTEGER NOT NULL,
             PRIMARY KEY (uid, channel));",
    )
    .expect("make the builds' tables");
}

/// A channel's current build: its number, its manifest, the manifest's signature.
pub fn current(c: &Connection, channel: &str) -> Option<(i64, String, String)> {
    c.query_row(
        "SELECT r.build, r.manifest, r.sig FROM channels ch JOIN releases r ON r.id = ch.release WHERE ch.name = ?1",
        params![channel],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
    .ok()
    .flatten()
}

// ------------------------------------------------------------------------------------------------ checking what comes in

pub fn sha_ok(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn sha1_ok(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A path inside the client's folder, as the manifest may name one: relative, forward slashes, plain names only.
pub fn path_ok(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 200
        && !p.starts_with('/')
        && p.split('/').all(|part| !part.is_empty() && part != "." && part != ".." && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+')))
}

fn version_ok(v: &str) -> bool {
    (1..=40).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+' | b'_'))
}

/// A file's SHA-256 (and its size), read through.
fn sha256_of(path: &Path) -> std::io::Result<(String, u64)> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        h.update(&buf[..n]);
    }
    Ok((hex(&h.finalize()), size))
}

fn sha1_of(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

#[derive(Deserialize)]
struct Staged {
    version: String,
    notes: String,
    #[serde(default)]
    loader_min: i64,
    launch: Value,
    files: Vec<StagedFile>,
    #[serde(default)]
    assets: Option<String>,
    /// This build is under a seal: its library is not one of its files and never was — the server cuts a
    /// player their own at the start (protect). The word goes into the manifest the loader is given, which
    /// is what the start reads it off; it is written only when it is true, so a build of any other kind has
    /// no such key at all (protect::sealed).
    #[serde(default)]
    seal: bool,
}

#[derive(Deserialize, Clone)]
struct StagedFile {
    path: String,
    sha256: String,
    size: u64,
    kind: String,
}

/// Which of these objects the store lacks (the uploader sends only those).
pub fn missing(dist: &Dist, hashes: &[String]) -> Vec<String> {
    hashes.iter().filter(|h| sha_ok(h) && !dist.object(h).is_file()).cloned().collect::<BTreeSet<_>>().into_iter().collect()
}

/// Takes a file into the store under its hash, checked (renamed when it can be, copied when not).
fn take_in(dist: &Dist, from: &Path, sha: &str, size: u64) -> Result<(), String> {
    let (got, got_size) = sha256_of(from).map_err(|e| format!("{}: {e}", from.display()))?;
    if got != sha || got_size != size {
        return Err(format!("{sha}: the file that came is another ({got}, {got_size} bytes)"));
    }
    let to = dist.object(sha);
    fs::create_dir_all(to.parent().expect("a folder")).map_err(|e| e.to_string())?;
    if fs::rename(from, &to).is_err() {
        let tmp = to.with_extension("part");
        fs::copy(from, &tmp).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &to).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Whether a file of this name is worth compressing (what is packed already is not).
fn compressible(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    ![".jar", ".zip", ".ogg", ".png", ".jpg", ".jpeg", ".zst", ".gz"].iter().any(|e| lower.ends_with(e))
}

/// zstd, if it is here; a patch or a compressed copy is left out without it.
fn zstd(args: &[&std::ffi::OsStr]) -> bool {
    Command::new("zstd").args(args).status().map(|s| s.success()).unwrap_or(false)
}

/// The compressed copy of an object, made once (an empty `.nz` beside it says it did not pay). Its size, if it has one.
fn compressed(dist: &Dist, sha: &str, size: u64) -> Option<u64> {
    let obj = dist.object(sha);
    let z = obj.with_extension("zst");
    let no = obj.with_extension("nz");
    if !z.is_file() && !no.exists() {
        let tmp = obj.with_extension("zst.part");
        let ok = zstd(&["-q".as_ref(), "-19".as_ref(), "--long=27".as_ref(), "-T2".as_ref(), "-f".as_ref(), "-o".as_ref(), tmp.as_os_str(), obj.as_os_str()]);
        let small = ok && fs::metadata(&tmp).map(|m| (m.len() as f64) < size as f64 * 0.9).unwrap_or(false);
        if small {
            let _ = fs::rename(&tmp, &z);
        } else {
            let _ = fs::remove_file(&tmp);
            let _ = File::create(&no);
        }
    }
    fs::metadata(&z).ok().map(|m| m.len())
}

/// A patch from an older object to a newer one, made once (an empty `.np` says it did not pay). Its size, if kept.
fn patched(dist: &Dist, from: &str, to: &str, full: u64) -> Option<u64> {
    let p = dist.patch(from, to);
    let no = p.with_extension("np");
    let (old, new) = (dist.object(from), dist.object(to));
    if !p.is_file() && !no.exists() && old.is_file() {
        let _ = fs::create_dir_all(p.parent().expect("a folder"));
        let tmp = p.with_extension("zst.part");
        let mut reference = std::ffi::OsString::from("--patch-from=");
        reference.push(old.as_os_str());
        let ok = zstd(&["-q".as_ref(), "-19".as_ref(), "--long=27".as_ref(), "-T2".as_ref(), "-f".as_ref(), reference.as_os_str(), "-o".as_ref(), tmp.as_os_str(), new.as_os_str()]);
        let small = ok && fs::metadata(&tmp).map(|m| (m.len() as f64) < full as f64 * 0.6).unwrap_or(false);
        if small {
            let _ = fs::rename(&tmp, &p);
        } else {
            let _ = fs::remove_file(&tmp);
            let _ = File::create(&no);
        }
    }
    fs::metadata(&p).ok().map(|m| m.len())
}

/// Publishes the build waiting in `incoming/<name>`: its files checked and taken into the store, compressed and
/// patched, the manifest made and signed, the channel pointed at it. Its number.
pub fn publish(c: &Connection, dist: &Dist, name: &str, channel: &str, by: &str) -> Result<i64, String> {
    if !path_ok(name) || name.contains('/') || !path_ok(channel) || channel.contains('/') {
        return Err("a plain name".into());
    }
    let dir = dist.root.join("incoming").join(name);
    let staged: Staged = serde_json::from_slice(&fs::read(dir.join("release.json")).map_err(|e| format!("release.json: {e}"))?).map_err(|e| format!("release.json: {e}"))?;
    if !version_ok(&staged.version) {
        return Err("a version like 0.1.0-beta.1".into());
    }
    let notes = staged.notes.replace("\r\n", "\n").trim().to_string();
    if notes.is_empty() || notes.chars().count() > 4000 {
        return Err("the changes, in words (up to 4000 letters): she reads them before a build goes out".into());
    }
    if !staged.launch.is_object() || staged.launch.to_string().len() > 16 * 1024 {
        return Err("launch: how to start the game".into());
    }
    let mut seen = BTreeSet::new();
    for f in &staged.files {
        if !path_ok(&f.path) || !sha_ok(&f.sha256) || !KINDS.contains(&f.kind.as_str()) || f.size > (2u64 << 30) {
            return Err(format!("{}: not a file a build may have", f.path));
        }
        if !seen.insert(f.path.to_ascii_lowercase()) {
            return Err(format!("{}: twice", f.path));
        }
    }
    // Into the store.
    for f in &staged.files {
        let there = dist.object(&f.sha256);
        if there.is_file() {
            let size = fs::metadata(&there).map(|m| m.len()).unwrap_or(0);
            if size != f.size {
                return Err(format!("{}: the store has another size for it", f.path));
            }
            continue;
        }
        let came = dir.join("objects").join(&f.sha256);
        if !came.is_file() {
            return Err(format!("{}: not uploaded", f.path));
        }
        take_in(dist, &came, &f.sha256, f.size)?;
    }
    // The assets its index names must all be here.
    if let Some(id) = &staged.assets {
        let index = staged.files.iter().find(|f| f.kind == "assets-index").ok_or("assets: its index is not among the files")?;
        let text = fs::read(dist.object(&index.sha256)).map_err(|e| e.to_string())?;
        let objects = asset_objects(&text).ok_or("the assets' index cannot be read")?;
        let lacking: Vec<&String> = objects.iter().filter(|(h, size)| fs::metadata(dist.asset(h)).map(|m| m.len() != *size).unwrap_or(true)).map(|(h, _)| h).collect();
        if !lacking.is_empty() {
            return Err(format!("assets {id}: {} of {} are not here (first {}): `cursed dist assets <folder>` first", lacking.len(), objects.len(), lacking[0]));
        }
    }
    // The builds before it in this channel: what each file was there.
    let before: Vec<Value> = {
        let mut st = c.prepare("SELECT manifest FROM releases WHERE channel = ?1 ORDER BY id DESC LIMIT 3").map_err(|e| e.to_string())?;
        let rows = st.query_map(params![channel], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
        rows.flatten().filter_map(|m| serde_json::from_str(&m).ok()).collect()
    };
    let old_of = |path: &str| -> Vec<String> {
        let mut out = Vec::new();
        for m in &before {
            if let Some(f) = m["files"].as_array().and_then(|fs| fs.iter().find(|f| f["path"] == path)) {
                if let Some(s) = f["sha256"].as_str() {
                    if !out.iter().any(|o: &String| o == s) {
                        out.push(s.to_string());
                    }
                }
            }
        }
        out
    };
    let mut files = Vec::new();
    for f in &staged.files {
        let z = if compressible(&f.path) && f.size >= COMPRESS_FROM { compressed(dist, &f.sha256, f.size) } else { None };
        let mut patches = Vec::new();
        if f.size >= PATCH_FROM && f.size <= PATCH_WINDOW {
            for old in old_of(&f.path).into_iter().filter(|o| *o != f.sha256) {
                if let Some(size) = patched(dist, &old, &f.sha256, z.unwrap_or(f.size)) {
                    patches.push(json!({ "from": old, "size": size }));
                }
            }
        }
        files.push(json!({ "path": f.path, "sha256": f.sha256, "size": f.size, "kind": f.kind, "z": z, "patches": patches }));
    }
    let build: i64 = c.query_row("SELECT COALESCE(MAX(build), 0) + 1 FROM releases", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let mut made = json!({
        "channel": channel, "build": build, "version": staged.version, "at": now(), "notes": notes,
        "loader_min": staged.loader_min, "launch": staged.launch, "assets": staged.assets, "files": files,
    });
    // only when it is true: a key that is there and says no would be read as a key that is there
    if staged.seal {
        made["seal"] = json!(true);
    }
    let manifest = made.to_string();
    let sig = hex(&dist.key.sign(manifest.as_bytes()).to_bytes());
    c.execute(
        "INSERT INTO releases (channel, build, version, notes, manifest, sig, at, by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![channel, build, staged.version, notes, manifest, sig, now(), by],
    )
    .map_err(|e| e.to_string())?;
    // The master pair of the seal: out of the incoming folder and into the place starts cut their own
    // library from, and before the channel points here. Nothing of the store serves that place, and the
    // master is not among the objects above — it never goes out whole. A build under a seal is not published
    // at all unless what is now up there is whole: a build that is live and whose master cannot be cut hands
    // every start `stub: false` and leaves the player without a pair of their own (protect::there is what
    // recode.rs `cut_here` reads). Better no build than a build that plays unprotected.
    crate::protect::take_in(&dist.root, &dir.join("protect"))?;
    if staged.seal && !crate::protect::there(&dist.root) {
        return Err("protect: the master is not whole — no start could be cut a pair".into());
    }
    let id = c.last_insert_rowid();
    c.execute("INSERT INTO channels (name, release) VALUES (?1, ?2) ON CONFLICT(name) DO UPDATE SET release = excluded.release", params![channel, id]).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&dir);
    Ok(build)
}

/// Points a channel back at the build before its current one. The build it points at now.
pub fn rollback(c: &Connection, channel: &str) -> Result<i64, String> {
    let now_id: i64 = c.query_row("SELECT release FROM channels WHERE name = ?1", params![channel], |r| r.get(0)).optional().map_err(|e| e.to_string())?.ok_or("no such channel")?;
    let (id, build): (i64, i64) = c
        .query_row("SELECT id, build FROM releases WHERE channel = ?1 AND id < ?2 ORDER BY id DESC LIMIT 1", params![channel, now_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("no build before it")?;
    c.execute("UPDATE channels SET release = ?2 WHERE name = ?1", params![channel, id]).map_err(|e| e.to_string())?;
    Ok(build)
}

/// A channel's builds, newest first: number, version, when, by whom, whether it is the current one, how many files,
/// how many bytes.
pub fn list(c: &Connection, channel: &str) -> Vec<Value> {
    let current: Option<i64> = c.query_row("SELECT release FROM channels WHERE name = ?1", params![channel], |r| r.get(0)).optional().ok().flatten();
    let Ok(mut st) = c.prepare("SELECT id, build, version, at, by, manifest, notes FROM releases WHERE channel = ?1 ORDER BY id DESC LIMIT 30") else { return Vec::new() };
    let rows = st.query_map(params![channel], |r| {
        let m: Value = serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(Value::Null);
        let files = m["files"].as_array().cloned().unwrap_or_default();
        let bytes: u64 = files.iter().filter_map(|f| f["size"].as_u64()).sum();
        Ok(json!({
            "build": r.get::<_, i64>(1)?, "version": r.get::<_, String>(2)?, "at": r.get::<_, i64>(3)?, "by": r.get::<_, String>(4)?,
            "current": Some(r.get::<_, i64>(0)?) == current, "files": files.len(), "bytes": bytes, "notes": r.get::<_, String>(6)?,
        }))
    });
    rows.map(|r| r.flatten().collect()).unwrap_or_default()
}

/// The objects an asset index names: their SHA-1 and size.
fn asset_objects(index: &[u8]) -> Option<Vec<(String, u64)>> {
    let v: Value = serde_json::from_slice(index).ok()?;
    let objects = v["objects"].as_object()?;
    let mut out: Vec<(String, u64)> = objects.values().filter_map(|o| Some((o["hash"].as_str()?.to_string(), o["size"].as_u64()?))).filter(|(h, _)| sha1_ok(h)).collect();
    out.sort();
    out.dedup();
    Some(out)
}

/// Takes the game's assets into the mirror from a folder laid out as Mojang lays them out (`indexes/<id>.json`,
/// `objects/<2>/<sha1>`): each checked against its SHA-1. How many were new, how many were here already.
pub fn import_assets(dist: &Dist, from: &Path) -> Result<(usize, usize), String> {
    let indexes: Vec<PathBuf> = fs::read_dir(from.join("indexes")).map_err(|e| format!("indexes: {e}"))?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    if indexes.is_empty() {
        return Err(format!("{}: no assets' index (indexes/*.json)", from.display()));
    }
    let (mut new, mut had) = (0, 0);
    for index in indexes {
        let objects = asset_objects(&fs::read(&index).map_err(|e| e.to_string())?).ok_or_else(|| format!("{}: cannot be read", index.display()))?;
        for (h, size) in objects {
            let to = dist.asset(&h);
            if fs::metadata(&to).map(|m| m.len() == size).unwrap_or(false) {
                had += 1;
                continue;
            }
            let came = from.join("objects").join(&h[..2]).join(&h);
            if sha1_of(&came).map_err(|e| format!("{h}: {e}"))? != h {
                return Err(format!("{h}: the file that came is another"));
            }
            fs::create_dir_all(to.parent().expect("a folder")).map_err(|e| e.to_string())?;
            if fs::rename(&came, &to).is_err() {
                fs::copy(&came, &to).map_err(|e| e.to_string())?;
            }
            new += 1;
        }
    }
    Ok((new, had))
}

// ------------------------------------------------------------------------------------------------ the downloads

/// A loader's leave to download `channel`'s files until `until` (its start: beta.rs, recode.rs).
pub fn grant(c: &Connection, uid: i64, channel: &str, until: i64) -> rusqlite::Result<()> {
    c.execute("INSERT OR REPLACE INTO dist_access (uid, channel, until) VALUES (?1, ?2, ?3)", params![uid, channel, until]).map(|_| ())
}

/// The channels whose builds name `sha` (a file of the store), counted again when a build was published since.
fn owners_of(app: &Shared, c: &Connection, sha: &str) -> Vec<String> {
    let last: i64 = c.query_row("SELECT COALESCE(MAX(id), 0) FROM releases", [], |r| r.get(0)).unwrap_or(0);
    let mut o = app.dist.owners.lock().unwrap_or_else(|e| e.into_inner());
    if o.0 != last {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        if let Ok(mut st) = c.prepare("SELECT channel, manifest FROM releases") {
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
            for (channel, manifest) in rows.into_iter().flatten().flatten() {
                let m: Value = serde_json::from_str(&manifest).unwrap_or(Value::Null);
                for f in m["files"].as_array().into_iter().flatten() {
                    if let Some(s) = f["sha256"].as_str() {
                        let e = map.entry(s.to_string()).or_default();
                        if !e.contains(&channel) {
                            e.push(channel.clone());
                        }
                    }
                }
            }
        }
        *o = (last, map);
    }
    o.1.get(sha).cloned().unwrap_or_default()
}

/// What a download is of: a file of the store (by its SHA-256; a patch by the file it makes), or the game's assets
/// (Mojang's own files, for any loader that may download).
enum Of<'a> {
    File(&'a str),
    Assets,
}

/// Whether the token this request carries may download this now: its loader started lately (beta.rs, recode.rs), for
/// a channel whose builds have the file (Cursed's loader gets none of Recode's, nor Recode's of Cursed's).
async fn allowed(app: &Shared, h: &HeaderMap, of: Of<'_>) -> Result<(), Response> {
    let (token, _) = presented(h).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
    let hash = digest(&token);
    let at = now();
    let cached = app.dist.access.lock().unwrap_or_else(|e| e.into_inner()).get(&hash).cloned().filter(|(_, _, asked)| asked.elapsed() < ACCESS_CHECK);
    let leave = match cached {
        Some((uid, chans, _)) => Some((uid, chans)),
        None => {
            let found = db(app, move |c| {
                let w = who(c, &token)?;
                let chans: Vec<(String, i64)> = c
                    .prepare("SELECT channel, until FROM dist_access WHERE uid = ?1")
                    .and_then(|mut s| s.query_map(params![w.uid], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
                    .unwrap_or_default();
                Some((w.uid, chans))
            })
            .await;
            if let Some((uid, chans)) = found.clone() {
                let mut cache = app.dist.access.lock().unwrap_or_else(|e| e.into_inner());
                if cache.len() > 10_000 {
                    cache.clear();
                }
                cache.insert(hash, (uid, chans, Instant::now()));
            }
            found
        }
    };
    let Some((_, chans)) = leave else { return Err(fail(StatusCode::UNAUTHORIZED, "signed-out")) };
    let live: Vec<String> = chans.into_iter().filter(|(_, until)| *until > at).map(|(c, _)| c).collect();
    if live.is_empty() {
        // no leave (the loader has not started lately): "start the loader first"
        return Err(fail(StatusCode::FORBIDDEN, "start"));
    }
    match of {
        Of::Assets => Ok(()),
        Of::File(sha) => {
            let app2 = app.clone();
            let sha = sha.to_string();
            let owners = db(app, move |c| owners_of(&app2, c, &sha)).await;
            if owners.iter().any(|o| live.contains(o)) {
                Ok(())
            } else {
                Err(fail(StatusCode::FORBIDDEN, "start"))
            }
        }
    }
}

fn send(internal: &str, exists: bool) -> Result<Response, Response> {
    if !exists {
        return Err(fail(StatusCode::NOT_FOUND, "missing"));
    }
    let mut res = StatusCode::OK.into_response();
    let hd = res.headers_mut();
    hd.insert("X-Accel-Redirect", HeaderValue::from_str(internal).map_err(|_| fail(StatusCode::BAD_REQUEST, "name"))?);
    hd.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    Ok(res)
}

#[derive(Deserialize)]
struct Packed {
    #[serde(default)]
    z: Option<u8>,
}

async fn object(State(app): State<Shared>, h: HeaderMap, Key(sha): Key<String>, Query(q): Query<Packed>) -> Response {
    run(async move {
        if !sha_ok(&sha) {
            return Err(fail(StatusCode::BAD_REQUEST, "name"));
        }
        allowed(&app, &h, Of::File(&sha)).await?;
        let path = app.dist.object(&sha);
        if q.z == Some(1) && path.with_extension("zst").is_file() {
            return send(&format!("/_dist/objects/{}/{sha}.zst", &sha[..2]), true);
        }
        send(&format!("/_dist/objects/{}/{sha}", &sha[..2]), path.is_file())
    })
    .await
}

async fn patch(State(app): State<Shared>, h: HeaderMap, Key((from, to)): Key<(String, String)>) -> Response {
    run(async move {
        if !sha_ok(&from) || !sha_ok(&to) {
            return Err(fail(StatusCode::BAD_REQUEST, "name"));
        }
        allowed(&app, &h, Of::File(&to)).await?;
        send(&format!("/_dist/patches/{from}-{to}.zst"), app.dist.patch(&from, &to).is_file())
    })
    .await
}

/// The site's download button: the loader of the current build, to a tester signed in on the site (or her). The loader
/// asks for nothing more to start; everything else comes through it.
/// The `loader` file of a channel's current build (its SHA-256), for the site's download buttons.
pub fn loader_sha(c: &Connection, channel: &str) -> Option<String> {
    let (_, manifest, _) = current(c, channel)?;
    let m: Value = serde_json::from_str(&manifest).ok()?;
    m["files"].as_array()?.iter().find(|f| f["kind"] == "loader").and_then(|f| f["sha256"].as_str()).filter(|s| sha_ok(s)).map(str::to_string)
}

/// An object of the store sent as a download named `name` (through nginx).
pub fn send_file(dist: &Dist, sha: &str, name: &str) -> Result<Response, Response> {
    let mut res = send(&format!("/_dist/objects/{}/{sha}", &sha[..2]), dist.object(sha).is_file())?;
    let v = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")).map_err(|_| fail(StatusCode::BAD_REQUEST, "name"))?;
    res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

async fn loader_file(State(app): State<Shared>, h: HeaderMap) -> Response {
    run(async move {
        let (token, _) = presented(&h).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
        let sha = db(&app, move |c| {
            let w = who(c, &token).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "signed-out"))?;
            if let Some(b) = crate::beta::ban_of(c, w.uid) {
                return Err((StatusCode::FORBIDDEN, Json(b.json())).into_response());
            }
            if !crate::beta::is_tester(c, w.uid) {
                return Err(fail(StatusCode::FORBIDDEN, "not-tester"));
            }
            let (_, manifest, _) = current(c, "beta").ok_or_else(|| fail(StatusCode::SERVICE_UNAVAILABLE, "no-release"))?;
            let m: Value = serde_json::from_str(&manifest).map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "busy"))?;
            m["files"]
                .as_array()
                .and_then(|fs| fs.iter().find(|f| f["kind"] == "loader"))
                .and_then(|f| f["sha256"].as_str())
                .filter(|s| sha_ok(s))
                .map(str::to_string)
                .ok_or_else(|| fail(StatusCode::SERVICE_UNAVAILABLE, "no-release"))
        })
        .await?;
        let mut res = send(&format!("/_dist/objects/{}/{sha}", &sha[..2]), app.dist.object(&sha).is_file())?;
        res.headers_mut().insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("attachment; filename=\"Cursed.exe\""));
        res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        Ok(res)
    })
    .await
}

async fn asset(State(app): State<Shared>, h: HeaderMap, Key(sha1): Key<String>) -> Response {
    run(async move {
        if !sha1_ok(&sha1) {
            return Err(fail(StatusCode::BAD_REQUEST, "name"));
        }
        allowed(&app, &h, Of::Assets).await?;
        send(&format!("/_dist/assets/objects/{}/{sha1}", &sha1[..2]), app.dist.asset(&sha1).is_file())
    })
    .await
}

#[derive(Deserialize)]
struct Channel {
    #[serde(default)]
    channel: Option<String>,
}

async fn admin_releases(State(app): State<Shared>, h: HeaderMap, Query(q): Query<Channel>) -> Response {
    run(async move {
        crate::blocklist::keeper(&app, &h).await?;
        let channel = q.channel.unwrap_or_else(|| "beta".into());
        Ok(Json(json!({ "releases": db(&app, move |c| list(c, &channel)).await })).into_response())
    })
    .await
}

async fn admin_rollback(State(app): State<Shared>, h: HeaderMap, Json(q): Json<Channel>) -> Response {
    run(async move {
        crate::blocklist::keeper(&app, &h).await?;
        let channel = q.channel.unwrap_or_else(|| "beta".into());
        db(&app, move |c| rollback(c, &channel)).await.map(|b| Json(json!({ "build": b })).into_response()).map_err(|e| fail(StatusCode::CONFLICT, &e))
    })
    .await
}

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/dist/object/{sha}", get(object))
        .route("/api/dist/patch/{from}/{to}", get(patch))
        .route("/api/dist/asset/{sha1}", get(asset))
        .route("/api/dist/loader", get(loader_file))
        .route("/api/admin/releases", get(admin_releases))
        .route("/api/admin/rollback", post(admin_rollback))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside() {
        for good in ["cursed.jar", "libraries/org/lwjgl/lwjgl-3.3.3.jar", "runtime/bin/javaw.exe", "assets/indexes/29.json", "a+b_c-d.txt"] {
            assert!(path_ok(good), "{good}");
        }
        for bad in ["", "/etc/passwd", "../x", "a/../../x", "a//b", "a\\b", "C:/x", "a/./b", "имя.jar", "a b.jar"] {
            assert!(!path_ok(bad), "{bad}");
        }
    }

    #[test]
    fn a_build_goes_in_and_back() {
        let dir = std::env::temp_dir().join(format!("cursed-dist-{}", crate::random_hex(4)));
        let db_path = dir.join("cursed.db");
        fs::create_dir_all(&dir).unwrap();
        let c = crate::open(db_path.to_str().unwrap());
        let dist = Dist::open(db_path.to_str().unwrap(), dir.join("dist"));
        let stage = |name: &str, content: &[u8]| {
            let inc = dist.root.join("incoming").join(name);
            fs::create_dir_all(inc.join("objects")).unwrap();
            let sha = hex(&Sha256::digest(content));
            fs::write(inc.join("objects").join(&sha), content).unwrap();
            let rel = json!({
                "version": "0.1.0-beta.1", "notes": "Первая сборка.", "launch": { "main": "net.minecraft.client.main.Main" },
                "files": [{ "path": "bin/cursed_native.dll", "sha256": sha, "size": content.len(), "kind": "native" }],
            });
            fs::write(inc.join("release.json"), rel.to_string()).unwrap();
            sha
        };
        let first = stage("one", b"the first build's native part");
        assert_eq!(publish(&c, &dist, "one", "beta", "test"), Ok(1));
        assert!(dist.object(&first).is_file());
        let (build, manifest, sig) = current(&c, "beta").unwrap();
        assert_eq!(build, 1);
        // The signature holds for exactly the manifest's text.
        use ed25519_dalek::{Signature, Verifier};
        let sig = Signature::from_slice(&(0..64).map(|i| u8::from_str_radix(&sig[i * 2..i * 2 + 2], 16).unwrap()).collect::<Vec<_>>()).unwrap();
        assert!(dist.key.verifying_key().verify(manifest.as_bytes(), &sig).is_ok());
        assert!(dist.key.verifying_key().verify(manifest.replace("beta", "beto").as_bytes(), &sig).is_err());
        // A confirmed start: its signature holds for that start only.
        let start = dist.sign_start(&"a".repeat(64), &"b".repeat(64), 7, 1);
        let start = Signature::from_slice(&(0..64).map(|i| u8::from_str_radix(&start[i * 2..i * 2 + 2], 16).unwrap()).collect::<Vec<_>>()).unwrap();
        assert!(dist.key.verifying_key().verify(start_message(&"a".repeat(64), &"b".repeat(64), 7, 1).as_bytes(), &start).is_ok());
        assert!(dist.key.verifying_key().verify(start_message(&"c".repeat(64), &"b".repeat(64), 7, 1).as_bytes(), &start).is_err());
        // A file that came broken is refused; nothing is published.
        let second = stage("two", b"the second build");
        fs::write(dist.root.join("incoming/two/objects").join(&second), b"something else").unwrap();
        assert!(publish(&c, &dist, "two", "beta", "test").is_err());
        let _ = stage("two", b"the second build");
        assert_eq!(publish(&c, &dist, "two", "beta", "test"), Ok(2));
        assert_eq!(rollback(&c, "beta"), Ok(1));
        assert_eq!(current(&c, "beta").unwrap().0, 1);
        assert!(rollback(&c, "beta").is_err(), "nothing before the first");
        assert_eq!(missing(&dist, &[first.clone(), "0".repeat(64)]), vec!["0".repeat(64)]);
        drop(c);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_without_an_index_says_so() {
        let dir = std::env::temp_dir().join(format!("cursed-dist-{}", crate::random_hex(4)));
        fs::create_dir_all(dir.join("indexes")).unwrap();
        let dist = Dist::open(dir.join("cursed.db").to_str().unwrap(), dir.join("dist"));
        let e = import_assets(&dist, &dir).unwrap_err();
        assert!(e.contains("no assets' index"), "{e}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A build that names an assets' index is refused until every object that index names is in the store
    /// (the publish of 18.10 that said 749 of 5147 were not here: they were not — they came after).
    #[test]
    fn assets_must_be_here_before_a_build_names_them() {
        let dir = std::env::temp_dir().join(format!("cursed-dist-{}", crate::random_hex(4)));
        fs::create_dir_all(&dir).unwrap();
        let c = crate::open(dir.join("cursed.db").to_str().unwrap());
        let dist = Dist::open(dir.join("cursed.db").to_str().unwrap(), dir.join("dist"));
        let body = b"a sound of the game";
        let sha1 = hex(&Sha1::digest(body));
        let index = json!({ "objects": { "a": { "hash": sha1, "size": body.len() } } }).to_string();
        let index_sha = hex(&Sha256::digest(index.as_bytes()));
        let inc = dist.root.join("incoming").join("one");
        fs::create_dir_all(inc.join("objects")).unwrap();
        fs::write(inc.join("objects").join(&index_sha), index.as_bytes()).unwrap();
        let other = b"the game's own program";
        let other_sha = hex(&Sha256::digest(other));
        fs::write(inc.join("objects").join(&other_sha), other).unwrap();
        let rel = json!({
            "version": "0.1.0-beta.1", "notes": "Первая сборка.", "assets": "34",
            "launch": { "main": "net.minecraft.client.main.Main" },
            "files": [
                { "path": "assets/indexes/34.json", "sha256": index_sha, "size": index.len(), "kind": "assets-index" },
                { "path": "bin/cursed_native.dll", "sha256": other_sha, "size": other.len(), "kind": "native" },
            ],
        });
        fs::write(inc.join("release.json"), rel.to_string()).unwrap();
        let e = publish(&c, &dist, "one", "beta", "test").unwrap_err();
        assert!(e.contains("are not here") && e.contains(&sha1), "{e}");
        // What the error asks for: the game's assets laid out as Mojang lays them out.
        let from = dir.join("came");
        fs::create_dir_all(from.join("indexes")).unwrap();
        fs::create_dir_all(from.join("objects").join(&sha1[..2])).unwrap();
        fs::write(from.join("indexes/34.json"), index.as_bytes()).unwrap();
        fs::write(from.join("objects").join(&sha1[..2]).join(&sha1), body).unwrap();
        assert_eq!(import_assets(&dist, &from), Ok((1, 0)));
        assert_eq!(publish(&c, &dist, "one", "beta", "test"), Ok(1));
        drop(c);
        let _ = fs::remove_dir_all(&dir);
    }

    /// An object that is there under its name but of another size is not the index's: import replaces it.
    #[test]
    fn import_replaces_an_asset_of_another_size() {
        let dir = std::env::temp_dir().join(format!("cursed-dist-{}", crate::random_hex(4)));
        fs::create_dir_all(&dir).unwrap();
        let dist = Dist::open(dir.join("cursed.db").to_str().unwrap(), dir.join("dist"));
        let body = b"the right bytes";
        let sha1 = hex(&Sha1::digest(body));
        let index = json!({ "objects": { "a": { "hash": sha1, "size": body.len() } } }).to_string();
        let wrong = dist.asset(&sha1);
        fs::create_dir_all(wrong.parent().unwrap()).unwrap();
        fs::write(&wrong, b"no").unwrap();
        let from = dir.join("came");
        fs::create_dir_all(from.join("indexes")).unwrap();
        fs::create_dir_all(from.join("objects").join(&sha1[..2])).unwrap();
        fs::write(from.join("indexes/34.json"), index.as_bytes()).unwrap();
        fs::write(from.join("objects").join(&sha1[..2]).join(&sha1), body).unwrap();
        assert_eq!(import_assets(&dist, &from), Ok((1, 0)));
        assert_eq!(fs::read(&wrong).unwrap(), body);
        let _ = fs::remove_dir_all(&dir);
    }
}
