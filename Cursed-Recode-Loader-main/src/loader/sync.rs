//! The build on this computer made the same as the build the server signed: every file of the manifest checked by
//! its SHA-256 (the game's assets by their SHA-1), and what differs fetched. A file the computer has an older version
//! of comes as a patch against it (zstd --patch-from, made by the server from the last three builds); another comes
//! compressed when that paid, whole otherwise. Nothing lands before its hash is right, and nothing of hers is touched:
//! only the files of the build, and a file that left the build is removed only if the loader laid it out.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver};
use std::sync::{Arc, Mutex};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use sha1::Sha1;
use sha2::{Digest, Sha256};

use super::api;
use super::store::{self, Files, Seen};

/// The public half of the key the server signs builds with (`cursed dist-key`): a release build's own (ours, or the
/// test instance's it was built for, CURSED_VERIFY_KEY, as the game's) — kept in the binaries as noise (crate::vault);
/// one from the sources takes `CURSED_LOADER_KEY` (tests).
pub fn key() -> &'static str {
    crate::vault::verify_key()
}

/// How many files come at once.
const WORKERS: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub kind: String,
    /// The size of its compressed copy on the server, if there is one.
    pub z: Option<u64>,
    /// Patches the server has for it: from which older file (its SHA-256), how big.
    pub patches: Vec<(String, u64)>,
}

#[derive(Clone, Debug)]
pub struct Manifest {
    pub build: i64,
    pub version: String,
    pub notes: String,
    pub at: i64,
    pub launch: Value,
    pub assets: Option<String>,
    pub files: Vec<Entry>,
    /// This build is under a seal: its two halves are not among `files` and never were — the server cuts
    /// this player their own at the start (protect). A build without it brings its own library among its
    /// files and keeps it. The key is written only when it is true (dist.rs), so a build of any other kind
    /// has no such key at all and reads as false here (protect::sealed, the server's own reading of it).
    pub seal: bool,
}

pub fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    (s.len() % 2 == 0).then(|| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect::<Option<Vec<u8>>>()).flatten()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The manifest, if the signature over its exact text is the server's.
pub fn verify(text: &str, sig: &str) -> Result<Manifest, String> {
    let key_hex = if crate::license::RELEASE { key().to_string() } else { std::env::var("CURSED_LOADER_KEY").ok().filter(|k| k.len() == 64).unwrap_or_else(|| key().into()) };
    let key: [u8; 32] = hex_bytes(&key_hex).and_then(|k| k.try_into().ok()).ok_or("key")?;
    let key = VerifyingKey::from_bytes(&key).map_err(|_| "key")?;
    let sig: [u8; 64] = hex_bytes(sig).and_then(|s| s.try_into().ok()).ok_or("signature")?;
    key.verify(text.as_bytes(), &Signature::from_bytes(&sig)).map_err(|_| "signature")?;
    parse(text)
}

pub fn parse(text: &str) -> Result<Manifest, String> {
    let v: Value = serde_json::from_str(text).map_err(|_| "manifest")?;
    let mut files = Vec::new();
    for f in v["files"].as_array().ok_or("manifest")? {
        let path = f["path"].as_str().ok_or("manifest")?.to_string();
        // Only plain relative paths inside the client's folder.
        if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains(':') || path.chars().any(char::is_control) || path.split('/').any(|p| p.is_empty() || p == "." || p == "..") {
            return Err(format!("manifest: {path}"));
        }
        let sha256 = f["sha256"].as_str().filter(|s| s.len() == 64).ok_or("manifest")?.to_ascii_lowercase();
        let patches = f["patches"].as_array().map(|ps| ps.iter().filter_map(|p| Some((p["from"].as_str()?.to_ascii_lowercase(), p["size"].as_u64()?))).collect()).unwrap_or_default();
        files.push(Entry { path, sha256, size: f["size"].as_u64().ok_or("manifest")?, kind: f["kind"].as_str().unwrap_or("").to_string(), z: f["z"].as_u64(), patches });
    }
    Ok(Manifest {
        build: v["build"].as_i64().ok_or("manifest")?,
        version: v["version"].as_str().unwrap_or("").to_string(),
        notes: v["notes"].as_str().unwrap_or("").to_string(),
        at: v["at"].as_i64().unwrap_or(0),
        launch: v["launch"].clone(),
        assets: v["assets"].as_str().map(str::to_string),
        files,
        seal: v["seal"].as_bool().unwrap_or(false),
    })
}

/// What the loader does meanwhile, for the window to show.
#[derive(Default)]
pub struct Progress {
    /// Files read so far while checking, and how many there are.
    pub checked: AtomicUsize,
    pub to_check: AtomicUsize,
    /// Bytes to come over the network, and how many came.
    pub total: AtomicU64,
    pub done: AtomicU64,
    pub files_total: AtomicUsize,
    pub files_done: AtomicUsize,
    pub cancel: AtomicBool,
    /// The last try failed on the network and nothing has come since.
    pub net_down: AtomicBool,
    /// The most `done` has been: a failed try takes its bytes back, the number on the screen does not go back.
    peak: AtomicU64,
}

impl Progress {
    /// Bytes come so far as the screen shows them: never less than it has shown.
    pub fn shown(&self) -> u64 {
        let done = self.done.load(Ordering::Relaxed);
        self.peak.fetch_max(done, Ordering::Relaxed).max(done)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum How {
    /// A patch against the file there now (its SHA-256).
    Patch { from: String, size: u64 },
    Packed(u64),
    Whole(u64),
    Asset,
}

#[derive(Clone, Debug)]
struct Job {
    /// Where it goes, relative to the client's folder.
    path: String,
    /// SHA-256 of a file of the build, SHA-1 of an asset.
    hash: String,
    size: u64,
    how: How,
    /// What to fall back on when a patch does not work: the file packed or whole.
    fallback: How,
    /// A folder of the build that comes as one object (`runtime.zip` → `runtime/`), unpacked in place
    /// after its own hash is checked (the dist on GitHub: a few thousand files of a JDK are one
    /// download there, not one each).
    zip: bool,
}

impl Job {
    fn transfer(&self) -> u64 {
        match &self.how {
            How::Patch { size, .. } | How::Packed(size) | How::Whole(size) => *size,
            How::Asset => self.size,
        }
    }
}

pub fn sha256_file(path: &Path) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(hex(&h.finalize()))
}

/// The SHA-256 a file has now: from what was seen last if its size and time are unchanged, read otherwise.
fn current_sha(home: &Path, path: &str, files: &Mutex<Files>) -> Option<String> {
    let full = home.join(path);
    let (size, mtime) = store::stat(&full)?;
    if let Some(s) = files.lock().unwrap().seen.get(path) {
        if s.size == size && s.mtime == mtime {
            return Some(s.sha256.clone());
        }
    }
    let sha = sha256_file(&full)?;
    files.lock().unwrap().seen.insert(path.to_string(), Seen { size, mtime, sha256: sha.clone() });
    Some(sha)
}

/// The asset index of the build: its objects (SHA-1, size).
fn asset_objects(home: &Path, m: &Manifest) -> Vec<(String, u64)> {
    let Some(index) = m.files.iter().find(|f| f.kind == "assets-index") else { return Vec::new() };
    let Ok(text) = std::fs::read(home.join(&index.path)) else { return Vec::new() };
    let Ok(v) = serde_json::from_slice::<Value>(&text) else { return Vec::new() };
    let mut out: Vec<(String, u64)> = v["objects"]
        .as_object()
        .map(|o| o.values().filter_map(|x| Some((x["hash"].as_str().filter(|h| h.len() == 40)?.to_ascii_lowercase(), x["size"].as_u64()?))).collect())
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

fn asset_path(sha1: &str) -> String {
    format!("assets/objects/{}/{sha1}", &sha1[..2])
}

/// What has to come, file by file. `skip` leaves out kinds handled elsewhere (the loader itself).
fn plan_files(home: &Path, m: &Manifest, files: &Mutex<Files>, progress: &Progress) -> Vec<Job> {
    let wanted: Vec<&Entry> = m.files.iter().filter(|f| f.kind != "loader").collect();
    progress.to_check.store(wanted.len(), Ordering::SeqCst);
    progress.checked.store(0, Ordering::SeqCst);
    let queue = Mutex::new(wanted.into_iter());
    let jobs = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| loop {
                let Some(f) = queue.lock().unwrap().next() else { break };
                // A zip entry is done when its folder is in place: the object itself is gone once unpacked.
                let have = if f.kind == "zip" { zip_done(home, f).then(|| f.sha256.clone()) } else { current_sha(home, &f.path, files) };
                progress.checked.fetch_add(1, Ordering::SeqCst);
                if have.as_deref() == Some(f.sha256.as_str()) {
                    continue;
                }
                let plain = match f.z {
                    Some(z) => How::Packed(z),
                    None => How::Whole(f.size),
                };
                let how = match have.and_then(|h| f.patches.iter().find(|(from, _)| *from == h).cloned()) {
                    Some((from, size)) => How::Patch { from, size },
                    None => plain.clone(),
                };
                jobs.lock().unwrap().push(Job { path: f.path.clone(), hash: f.sha256.clone(), size: f.size, how, fallback: plain, zip: f.kind == "zip" });
            });
        }
    });
    let mut jobs = jobs.into_inner().unwrap();
    jobs.sort_by(|a, b| b.transfer().cmp(&a.transfer()));
    jobs
}

fn plan_assets(home: &Path, m: &Manifest) -> Vec<Job> {
    asset_objects(home, m)
        .into_iter()
        .filter(|(sha1, size)| store::stat(&home.join(asset_path(sha1))).map(|(s, _)| s) != Some(*size))
        .map(|(sha1, size)| Job { path: asset_path(&sha1), hash: sha1, size, how: How::Asset, fallback: How::Asset, zip: false })
        .collect()
}

/// Bytes handed over from the network thread to the one unpacking them.
struct Pipe {
    rx: Receiver<Vec<u8>>,
    buf: Vec<u8>,
    at: usize,
}

impl Read for Pipe {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while self.at >= self.buf.len() {
            match self.rx.recv() {
                Ok(next) => {
                    self.buf = next;
                    self.at = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len() - self.at);
        out[..n].copy_from_slice(&self.buf[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// Where a new file is written before its hash is known to be right.
fn part_path(full: &Path) -> PathBuf {
    let mut name = full.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    full.with_file_name(name)
}

/// One file fetched, unpacked and checked, then put in its place. The status on refusal (403: leave ran out).
fn fetch_one(home: &Path, job: &Job, token: &str, progress: &Progress, counted_here: &AtomicU64) -> Result<(), String> {
    let full = home.join(&job.path);
    let part = part_path(&full);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", job.path))?;
    }
    let url = if api::github() {
        // The dist on GitHub: the files are the release's assets (`object-<sha256>`), and the game's
        // assets are Mojang's own, off Mojang's own CDN — nothing of theirs is ours to mirror. Patches
        // a static dist does not make; the whole file is what there is.
        match &job.how {
            How::Asset => format!("https://resources.download.minecraft.net/{}/{}", &job.hash[..2], job.hash),
            How::Patch { .. } | How::Packed(_) | How::Whole(_) => format!("{}/object-{}", api::base(), job.hash),
        }
    } else {
        match &job.how {
            How::Patch { from, .. } => format!("/api/dist/patch/{from}/{}", job.hash),
            How::Packed(_) => format!("/api/dist/object/{}?z=1", job.hash),
            How::Whole(_) => format!("/api/dist/object/{}", job.hash),
            How::Asset => format!("/api/dist/asset/{}", job.hash),
        }
    };
    let counted = |n: usize| {
        progress.net_down.store(false, Ordering::Relaxed);
        progress.done.fetch_add(n as u64, Ordering::Relaxed);
        counted_here.fetch_add(n as u64, Ordering::Relaxed);
    };
    let (hash, written) = match &job.how {
        How::Whole(_) | How::Asset => {
            let mut out = BufWriter::new(File::create(&part).map_err(|e| format!("{}: {e}", job.path))?);
            let (mut h256, mut h1) = (Sha256::new(), Sha1::new());
            let mut written = 0u64;
            let mut failed = None;
            let status = api::fetch(&url, token, &mut |chunk| {
                counted(chunk.len());
                if job.how == How::Asset { h1.update(chunk) } else { h256.update(chunk) }
                written += chunk.len() as u64;
                if let Err(e) = out.write_all(chunk) {
                    failed = Some(e.to_string());
                    return false;
                }
                !progress.cancel.load(Ordering::Relaxed)
            })
            .map_err(|f| f.error().to_string())?;
            if let Some(e) = failed {
                return Err(format!("{}: {e}", job.path));
            }
            if status != 200 {
                return Err(format!("status {status}"));
            }
            out.flush().map_err(|e| e.to_string())?;
            let hash = if job.how == How::Asset { hex(&h1.finalize()) } else { hex(&h256.finalize()) };
            (hash, written)
        }
        How::Packed(_) | How::Patch { .. } => {
            // The old file stays whole in memory as the patch's reference (a patch is only made for files up to 128 MB).
            let reference = match &job.how {
                How::Patch { .. } => std::fs::read(&full).map_err(|e| format!("{}: {e}", job.path))?,
                _ => Vec::new(),
            };
            let (tx, rx) = sync_channel::<Vec<u8>>(32);
            let unpacked = std::thread::scope(|s| {
                let writer = s.spawn(|| -> Result<(String, u64), String> {
                    let pipe = std::io::BufReader::new(Pipe { rx, buf: Vec::new(), at: 0 });
                    let mut decoder = if reference.is_empty() {
                        zstd::stream::read::Decoder::with_buffer(pipe).map_err(|e| e.to_string())?
                    } else {
                        zstd::stream::read::Decoder::with_ref_prefix(pipe, &reference).map_err(|e| e.to_string())?
                    };
                    decoder.window_log_max(27).map_err(|e| e.to_string())?;
                    let mut out = BufWriter::new(File::create(&part).map_err(|e| e.to_string())?);
                    let mut h = Sha256::new();
                    let mut buf = vec![0u8; 256 * 1024];
                    let mut written = 0u64;
                    loop {
                        let n = decoder.read(&mut buf).map_err(|e| e.to_string())?;
                        if n == 0 {
                            break;
                        }
                        h.update(&buf[..n]);
                        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                        written += n as u64;
                    }
                    out.flush().map_err(|e| e.to_string())?;
                    Ok((hex(&h.finalize()), written))
                });
                let status = api::fetch(&url, token, &mut |chunk| {
                    counted(chunk.len());
                    tx.send(chunk.to_vec()).is_ok() && !progress.cancel.load(Ordering::Relaxed)
                });
                drop(tx);
                let unpacked = writer.join().unwrap_or_else(|_| Err("unpack".into()));
                match status {
                    Err(f) => Err(f.error().to_string()),
                    Ok(200) => unpacked,
                    Ok(other) => Err(format!("status {other}")),
                }
            });
            unpacked.map_err(|e| format!("{}: {e}", job.path))?
        }
    };
    if hash != job.hash || written != job.size {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{}: not the file of the build", job.path));
    }
    if job.zip {
        // A folder of the build as one object: unpacked beside where it lives, the marker of its being
        // the build's inside. The bytes were the build's before they were a folder (the hash above).
        let dir = zip_dir(&full);
        unpack_zip(&part, &dir).map_err(|e| format!("{}: {e}", job.path))?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", job.path))?;
        std::fs::write(dir.join(format!(".unpacked-{}", &job.hash[..16])), job.hash.as_bytes()).map_err(|e| format!("{}: {e}", job.path))?;
        let _ = std::fs::remove_file(&part);
        return Ok(());
    }
    put_in_place(&part, &full).map_err(|e| format!("{}: {e}", job.path))?;
    Ok(())
}

/// What a file moved aside is called after: `<name>.cursed-old-<time>` beside it (see put_in_place).
const ASIDE: &str = ".cursed-old-";

/// The folder a `zip` entry of the build unpacks into: the file's own name without its ending
/// (`runtime.zip` → `runtime/`), beside where the object itself would sit.
fn zip_dir(full: &Path) -> PathBuf {
    full.with_extension("")
}

/// Whether a `zip` entry is already in place: its folder is, with this build's own marker in it.
/// The marker names the object the folder came out of, so a newer build unpacks again.
fn zip_done(home: &Path, f: &Entry) -> bool {
    zip_dir(&home.join(&f.path)).join(format!(".unpacked-{}", &f.sha256[..16])).is_file()
}

/// One object of the build opened into its folder (`runtime.zip` → `runtime/`). A single top-level
/// folder inside (the JDK's own `jdk-25.0.4+7/...`) is peeled off; anything else is kept as it lies.
/// Every member lands by `enclosed_name`, never by the name it says: a zip that says `..\x` is a zip
/// that unpacks nothing.
fn unpack_zip(object: &Path, into: &Path) -> Result<(), String> {
    let f = File::open(object).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    let names: Vec<PathBuf> = (0..archive.len())
        .filter_map(|i| {
            let e = archive.by_index(i).ok()?;
            (!e.is_dir()).then(|| e.enclosed_name().map(Path::to_path_buf)).flatten()
        })
        .collect();
    // the one top-level folder, if every member sits under one and the same
    let peel = {
        let mut tops = names.iter().map(|n| n.components().next()).collect::<std::collections::BTreeSet<_>>();
        tops.len() == 1 && tops.pop().flatten().is_some() && names.iter().any(|n| n.components().count() >= 2)
    };
    for name in &names {
        let rel: PathBuf = if peel { name.components().skip(1).collect() } else { name.clone() };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = into.join(&rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", rel.display()))?;
        }
        let mut e = archive.by_name(&name.to_string_lossy()).map_err(|e| e.to_string())?;
        let mut o = File::create(&out).map_err(|e| format!("{}: {e}", rel.display()))?;
        std::io::copy(&mut e, &mut o).map_err(|e| format!("{}: {e}", rel.display()))?;
    }
    Ok(())
}

/// The new file in the old one's place. A file in use cannot be written over on Windows (04.10, a player: "bin/
/// cursed_native.dll: access denied", the game, or a process of it left behind, still holding the library), but it
/// can be moved aside: the old one goes beside it under another name, the new one into its place, and the old one is
/// removed once nothing holds it (finish). Only when even that is refused is the file busy: "busy:" says so.
pub(super) fn put_in_place(part: &Path, full: &Path) -> Result<(), String> {
    let first = match std::fs::rename(part, full) {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };
    if full.exists() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let name = full.file_name().map_or_else(String::new, |n| n.to_string_lossy().to_string());
        let aside = full.with_file_name(format!("{name}{ASIDE}{stamp}"));
        if std::fs::rename(full, &aside).is_ok() {
            match std::fs::rename(part, full) {
                Ok(()) => return Ok(()),
                // Put back as it was: the next try finds the old one where it was.
                Err(_) => {
                    let _ = std::fs::rename(&aside, full);
                }
            }
        }
    }
    let _ = std::fs::remove_file(part);
    if first.kind() == std::io::ErrorKind::PermissionDenied || first.raw_os_error() == Some(32) {
        return Err(format!("busy: {first}"));
    }
    Err(first.to_string())
}

/// Files moved aside earlier (put_in_place) removed where nothing holds them any more.
fn sweep_aside(home: &Path, m: &Manifest) {
    let dirs: BTreeSet<PathBuf> = m.files.iter().filter_map(|f| home.join(&f.path).parent().map(Path::to_path_buf)).collect();
    for dir in dirs {
        let Ok(list) = std::fs::read_dir(&dir) else { continue };
        for e in list.flatten() {
            if e.file_name().to_string_lossy().contains(ASIDE) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// What this computer lacks of the build: every file checked, nothing fetched but the assets' index (a few hundred
/// kilobytes, when it is not here: it is what names the assets). The loader shows how much there is to download and
/// fetches it when play is pressed.
pub struct Plan {
    files: Vec<Job>,
    assets: Vec<Job>,
    pub bytes: u64,
    pub count: usize,
}

pub fn plan(home: &Path, m: &Manifest, token: &str, renew: &(dyn Fn() -> bool + Sync), progress: &Progress) -> Result<Plan, String> {
    let files = Mutex::new(Files::load());
    let mut jobs = plan_files(home, m, &files, progress);
    let index = m.files.iter().find(|f| f.kind == "assets-index").map(|f| f.path.clone());
    if let Some(i) = index.and_then(|path| jobs.iter().position(|j| j.path == path)) {
        let mut first = vec![jobs.remove(i)];
        let quiet = Progress::default();
        let got = fetch_all(home, &mut first, token, renew, &quiet, &files);
        files.lock().unwrap().save();
        got?;
    }
    files.lock().unwrap().save();
    let assets = plan_assets(home, m);
    let bytes = jobs.iter().chain(assets.iter()).map(Job::transfer).sum();
    let count = jobs.len() + assets.len();
    Ok(Plan { files: jobs, assets, bytes, count })
}

/// Fetches what the plan found missing, then tidies up (see `finish`).
pub fn fetch(home: &Path, m: &Manifest, mut plan: Plan, token: &str, renew: &(dyn Fn() -> bool + Sync), progress: &Progress) -> Result<(), String> {
    let files = Mutex::new(Files::load());
    progress.total.store(plan.bytes, Ordering::SeqCst);
    progress.files_total.store(plan.count, Ordering::SeqCst);
    let first = fetch_all(home, &mut plan.files, token, renew, progress, &files);
    files.lock().unwrap().save();
    first?;
    fetch_all(home, &mut plan.assets, token, renew, progress, &files)?;
    finish(home, m);
    Ok(())
}

/// Files the last build had and this one has not are removed (only ever ones the loader laid out); the build is
/// remembered as the one in place.
pub fn finish(home: &Path, m: &Manifest) {
    sweep_aside(home, m);
    let now: BTreeSet<&str> = m.files.iter().map(|f| f.path.as_str()).collect();
    let mut state = Files::load();
    for old in state.installed.clone() {
        if !now.contains(old.as_str()) {
            let _ = std::fs::remove_file(home.join(&old));
            state.seen.remove(&old);
        }
    }
    state.installed = m.files.iter().filter(|f| f.kind != "loader").map(|f| f.path.clone()).collect();
    state.save();
}

/// The whole build checked and fetched in one go (the headless mode).
pub fn run(home: &Path, m: &Manifest, token: &str, renew: &(dyn Fn() -> bool + Sync), progress: &Progress) -> Result<(), String> {
    let p = plan(home, m, token, renew, progress)?;
    fetch(home, m, p, token, renew, progress)
}

/// An error that is the network's, not the file's or the disk's: worth waiting for and trying again.
pub fn is_network(e: &str) -> bool {
    ["offline", "timeout", "tls", "net ", "status 5", "status 0", "status 429", "cancelled"].iter().any(|w| e.contains(w))
}

/// Pauses between tries of one file: short at first, then longer, so a moment without a network is waited out.
const BACKOFF_MS: [u64; 6] = [500, 1000, 2000, 4000, 6000, 8000];

/// Fetches every job, WORKERS at a time. A file that keeps failing does not stop the others: it is put aside and tried
/// again once the rest are in, after a pause. Only what still fails then is an error.
fn fetch_all(home: &Path, jobs: &mut Vec<Job>, token: &str, renew: &(dyn Fn() -> bool + Sync), progress: &Progress, files: &Mutex<Files>) -> Result<(), String> {
    let mut last = String::new();
    for round in 0..2 {
        if jobs.is_empty() {
            return Ok(());
        }
        if round > 0 {
            super::log(&format!("loader: {} files again after a pause ({last})", jobs.len()));
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
        let queue = Mutex::new(std::mem::take(jobs));
        let failed: Mutex<Vec<(Job, String)>> = Mutex::new(Vec::new());
        let renewing = Mutex::new(());
        std::thread::scope(|s| {
            for _ in 0..WORKERS {
                s.spawn(|| loop {
                    if progress.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let Some(mut job) = queue.lock().unwrap().pop() else { break };
                    let mut tries = 0;
                    loop {
                        let here = AtomicU64::new(0);
                        match fetch_one(home, &job, token, progress, &here) {
                            Ok(()) => {
                                if job.how != How::Asset {
                                    if let Some((size, mtime)) = store::stat(&home.join(&job.path)) {
                                        files.lock().unwrap().seen.insert(job.path.clone(), Seen { size, mtime, sha256: job.hash.clone() });
                                    }
                                }
                                progress.files_done.fetch_add(1, Ordering::SeqCst);
                                break;
                            }
                            Err(e) => {
                                // What this try counted does not count.
                                progress.done.fetch_sub(here.load(Ordering::Relaxed), Ordering::Relaxed);
                                if is_network(&e) {
                                    progress.net_down.store(true, Ordering::Relaxed);
                                }
                                tries += 1;
                                if e == "status 403" {
                                    let _one = renewing.lock().unwrap();
                                    if tries <= 2 && renew() {
                                        continue;
                                    }
                                }
                                if tries > BACKOFF_MS.len() || progress.cancel.load(Ordering::Relaxed) {
                                    failed.lock().unwrap().push((job, e));
                                    break;
                                }
                                // A patch that did not work: the file itself next time.
                                if matches!(job.how, How::Patch { .. }) {
                                    super::log(&format!("loader: patch failed ({e}), fetching the file"));
                                    let before = job.transfer();
                                    job.how = job.fallback.clone();
                                    progress.total.fetch_add(job.transfer(), Ordering::SeqCst);
                                    progress.total.fetch_sub(before, Ordering::SeqCst);
                                }
                                std::thread::sleep(std::time::Duration::from_millis(BACKOFF_MS[tries - 1]));
                            }
                        }
                    }
                });
            }
        });
        if progress.cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        for (job, e) in failed.into_inner().unwrap() {
            last = e;
            jobs.push(job);
        }
    }
    if jobs.is_empty() { Ok(()) } else { Err(last) }
}

/// The loader's own file in this build, if it is not the one running.
pub fn loader_update(m: &Manifest) -> Option<Entry> {
    let entry = m.files.iter().find(|f| f.kind == "loader")?.clone();
    let me = std::env::current_exe().ok()?;
    (sha256_file(&me)? != entry.sha256).then_some(entry)
}

/// Fetches the new loader next to the running one (as `<exe>.new`), checked. The caller swaps them.
pub fn fetch_loader(entry: &Entry, token: &str, into: &Path) -> Result<(), String> {
    let mut h = Sha256::new();
    let mut out = BufWriter::new(File::create(into).map_err(|e| e.to_string())?);
    let packed = entry.z.is_some();
    let path = if api::github() {
        format!("{}/object-{}", api::base(), entry.sha256)
    } else {
        format!("/api/dist/object/{}{}", entry.sha256, if packed { "?z=1" } else { "" })
    };
    let mut got = Vec::new();
    let status = api::fetch(&path, token, &mut |chunk| {
        got.extend_from_slice(chunk);
        true
    })
    .map_err(|f| f.error().to_string())?;
    if status != 200 {
        return Err(format!("status {status}"));
    }
    let bytes = if packed { zstd::stream::decode_all(&got[..]).map_err(|e| e.to_string())? } else { got };
    h.update(&bytes);
    if hex(&h.finalize()) != entry.sha256 {
        return Err("the loader that came is not the one of the build".into());
    }
    out.write_all(&bytes).map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())
}

pub fn shared() -> Arc<Progress> {
    Arc::new(Progress::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn sample() -> String {
        serde_json::json!({
            "channel": "beta", "build": 3, "version": "0.1.0-beta", "at": 1, "notes": "a\nb", "loader_min": 0,
            "launch": { "java": "runtime/bin/javaw.exe" }, "assets": "29",
            "files": [
                { "path": "cursed.jar", "sha256": "AB".repeat(32), "size": 10, "kind": "code", "z": 7, "patches": [{ "from": "cd".repeat(32), "size": 2 }] },
                { "path": "bin/cursed_native.dll", "sha256": "ef".repeat(32), "size": 5, "kind": "native", "z": null, "patches": [] }
            ]
        })
        .to_string()
    }

    #[test]
    fn a_manifest_is_read() {
        let m = parse(&sample()).unwrap();
        assert_eq!(m.build, 3);
        assert_eq!(m.files[0].sha256, "ab".repeat(32));
        assert_eq!(m.files[0].patches, vec![("cd".repeat(32), 2)]);
        assert_eq!(m.files[1].z, None);
    }

    #[test]
    fn a_path_out_of_the_folder_is_refused() {
        // As they stand in the JSON text: "a\\\\b" is a backslash, "a\\bb" a control character.
        for bad in ["../x", "/etc/x", "a/../../x", "C:/x", "a\\\\b", "a\\bb", "a//b"] {
            let text = sample().replace("bin/cursed_native.dll", bad);
            assert!(parse(&text).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_the_servers_signature_passes() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let text = sample();
        let sig = hex(&key.sign(text.as_bytes()).to_bytes());
        std::env::set_var("CURSED_LOADER_KEY", hex(key.verifying_key().as_bytes()));
        assert!(verify(&text, &sig).is_ok());
        assert!(verify(&text.replace("0.1.0-beta", "0.1.1-beta"), &sig).is_err());
        std::env::remove_var("CURSED_LOADER_KEY");
        assert!(verify(&text, &sig).is_err(), "another key");
    }

    #[test]
    fn a_patch_rebuilds_the_new_file_from_the_old() {
        // The same as the server: zstd --patch-from, a 128 MB window.
        let old: Vec<u8> = (0..400_000u32).flat_map(|i| (i.wrapping_mul(2654435761)).to_le_bytes()).collect();
        let mut new = old.clone();
        new[1000..1010].copy_from_slice(b"0123456789");
        new.extend_from_slice(b"tail");
        let mut patch = Vec::new();
        {
            let mut w = zstd::stream::write::Encoder::with_ref_prefix(&mut patch, 19, &old).unwrap();
            w.long_distance_matching(true).unwrap();
            w.window_log(27).unwrap();
            w.write_all(&new).unwrap();
            w.finish().unwrap();
        }
        assert!(patch.len() < 1000, "a patch, not the file: {}", patch.len());
        let mut d = zstd::stream::read::Decoder::with_ref_prefix(std::io::BufReader::new(&patch[..]), &old).unwrap();
        d.window_log_max(27).unwrap();
        let mut back = Vec::new();
        d.read_to_end(&mut back).unwrap();
        assert_eq!(back, new);
    }
}
