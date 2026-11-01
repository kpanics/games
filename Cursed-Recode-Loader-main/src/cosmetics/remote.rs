//! CS2's files from our server as they are first wanted (none of the 7 GB comes with the client): the server's list
//! (manifest.txt: each file's SHA-256, size and path) read first; a file wanted is put on a stack (the last wanted
//! comes first: what the player looks at now) that four workers take from, each file checked against its SHA-256 and
//! laid down whole (written aside, then renamed); what is here is kept in an index beside the files (a file changed on
//! the server comes again). The stand's own folder (CURSED_DEV_COSMETICS) never asks the server.

use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Instant;

const HOST: &str = "cursedclient.xyz";
const ROOT: &str = "/cs2/";
const WORKERS: usize = 4;
const INDEX: &str = ".cs2-index";

/// The server's list: each file's SHA-256 (hex) and size.
type List = HashMap<String, (String, u64)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListState {
    Asking,
    There,
    /// Not had (no line, the server down): asked again after a while.
    Failed(Instant),
}

struct Remote {
    dir: PathBuf,
    list: Mutex<(ListState, Arc<List>)>,
    /// What is here, downloaded (its SHA-256).
    have: Mutex<HashMap<String, String>>,
    /// The stack of what is wanted, and what is on it or being downloaded; what could not be had.
    work: Mutex<(Vec<String>, HashSet<String>, HashSet<String>)>,
    wake: Condvar,
    /// A file laid down (or given up): those waiting for one look again.
    done: Condvar,
    /// Bytes downloaded, and wanted in all (the list's sizes of what went on the stack).
    got: AtomicU64,
    asked: AtomicU64,
}

static REMOTE: OnceLock<Remote> = OnceLock::new();

fn remote() -> Option<&'static Remote> {
    REMOTE.get()
}

/// The files are ours from now on under `dir` (the game's cursed-recode/cosmetics): the index read, the list asked
/// for, the workers started. Once.
pub fn start(dir: PathBuf) {
    if REMOTE.get().is_some() {
        return;
    }
    let have = std::fs::read_to_string(dir.join(INDEX))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once(' ').map(|(h, k)| (k.to_string(), h.to_string())))
        .collect();
    let r = Remote {
        dir,
        list: Mutex::new((ListState::Asking, Arc::new(HashMap::new()))),
        have: Mutex::new(have),
        work: Mutex::new((Vec::new(), HashSet::new(), HashSet::new())),
        wake: Condvar::new(),
        done: Condvar::new(),
        got: AtomicU64::new(0),
        asked: AtomicU64::new(0),
    };
    if REMOTE.set(r).is_err() {
        return;
    }
    ask_list();
    for n in 0..WORKERS {
        std::thread::Builder::new().name(format!("Cursed cosmetics download {n}")).spawn(worker).ok();
    }
}

/// Whether the files come from the server.
pub fn active() -> bool {
    remote().is_some()
}

fn ask_list() {
    std::thread::Builder::new()
        .name("Cursed cosmetics list".into())
        .spawn(|| {
            let Some(r) = remote() else { return };
            let got = fetch::Client::new(HOST).ok_or_else(|| "no connection".to_string()).and_then(|c| c.get(&format!("{ROOT}manifest.txt")));
            let mut list = r.list.lock().unwrap();
            match got {
                Ok(body) => {
                    let parsed = parse_list(&String::from_utf8_lossy(&body));
                    eprintln!("[cosmetics] the server's list: {} files", parsed.len());
                    *list = (ListState::There, Arc::new(parsed));
                }
                Err(e) => {
                    eprintln!("[cosmetics] the server's list could not be had: {e}");
                    list.0 = ListState::Failed(Instant::now());
                }
            }
            drop(list);
            r.done.notify_all();
        })
        .ok();
}

use super::fetch;

fn parse_list(text: &str) -> List {
    text.lines()
        .filter_map(|l| {
            let mut f = l.splitn(3, ' ');
            let (h, size, key) = (f.next()?, f.next()?.parse().ok()?, f.next()?);
            (h.len() == 64 && !key.contains("..")).then(|| (key.to_string(), (h.to_string(), size)))
        })
        .collect()
}

/// The list as it is (asked again when it could not be had a while ago).
fn list(r: &Remote) -> (ListState, Arc<List>) {
    let mut l = r.list.lock().unwrap();
    if let ListState::Failed(at) = l.0 {
        if at.elapsed().as_secs() >= 30 {
            l.0 = ListState::Asking;
            drop(l);
            ask_list();
            return (ListState::Asking, Arc::new(HashMap::new()));
        }
    }
    l.clone()
}

/// Whether a file here is the server's as it is now (with no list yet: whatever came before).
pub fn here(key: &str) -> bool {
    let Some(r) = remote() else { return false };
    if !r.dir.join(key).is_file() {
        return false;
    }
    let (state, list) = list(r);
    let have = r.have.lock().unwrap();
    match (state, list.get(key)) {
        (ListState::There, Some((h, _))) => have.get(key) == Some(h),
        (ListState::There, None) => false,
        _ => have.contains_key(key),
    }
}

/// Whether the server has a file (with no list yet: perhaps).
pub fn known(key: &str) -> bool {
    let Some(r) = remote() else { return false };
    match list(r) {
        (ListState::There, l) => l.contains_key(key),
        (ListState::Asking, _) => true,
        (ListState::Failed(_), _) => false,
    }
}

/// Whether a file not here yet is coming (on the stack, or the list still being read).
pub fn coming(key: &str) -> bool {
    let Some(r) = remote() else { return false };
    if here(key) || r.work.lock().unwrap().2.contains(key) {
        return false;
    }
    known(key)
}

/// A file wanted: on the stack (on top) when it is not here and the server has it.
pub fn want(key: &str) {
    let Some(r) = remote() else { return };
    if key.contains("..") || here(key) {
        return;
    }
    let (state, list) = list(r);
    if state == ListState::There && !list.contains_key(key) {
        return;
    }
    let mut w = r.work.lock().unwrap();
    if w.2.contains(key) {
        return;
    }
    if w.1.insert(key.to_string()) {
        r.asked.fetch_add(list.get(key).map_or(0, |e| e.1), Ordering::Relaxed);
        w.0.push(key.to_string());
    } else if let Some(i) = w.0.iter().position(|k| k == key) {
        // wanted again: to the top
        let k = w.0.remove(i);
        w.0.push(k);
    }
    drop(w);
    r.wake.notify_one();
}

/// A file wanted and waited for (a thread of its own only — never the frame's): whether it is here now.
pub fn wait(key: &str) -> bool {
    let Some(r) = remote() else { return false };
    loop {
        if here(key) {
            return true;
        }
        match list(r) {
            (ListState::Failed(_), _) => return false,
            (ListState::There, l) if !l.contains_key(key) => return false,
            _ => {}
        }
        if r.work.lock().unwrap().2.contains(key) {
            return false;
        }
        want(key);
        let w = r.work.lock().unwrap();
        let _ = r.done.wait_timeout(w, std::time::Duration::from_millis(500)).unwrap();
    }
}

/// How much has come and how much was wanted (bytes), while files are still coming; none when nothing is.
pub fn progress() -> Option<(u64, u64)> {
    let r = remote()?;
    let busy = !r.work.lock().unwrap().1.is_empty();
    busy.then(|| (r.got.load(Ordering::Relaxed), r.asked.load(Ordering::Relaxed)))
}

/// Whether anything is still coming.
pub fn busy() -> bool {
    remote().is_some_and(|r| !r.work.lock().unwrap().1.is_empty() || matches!(r.list.lock().unwrap().0, ListState::Asking))
}

fn worker() {
    let Some(r) = remote() else { return };
    let mut client: Option<fetch::Client> = None;
    loop {
        let key = {
            let mut w = r.work.lock().unwrap();
            loop {
                if matches!(r.list.lock().unwrap().0, ListState::There) {
                    if let Some(k) = w.0.pop() {
                        break k;
                    }
                }
                w = r.wake.wait_timeout(w, std::time::Duration::from_millis(500)).unwrap().0;
            }
        };
        let expect = r.list.lock().unwrap().1.get(&key).cloned();
        let mut ok = false;
        if let Some((sha, _)) = expect {
            for attempt in 0..3 {
                if client.is_none() {
                    client = fetch::Client::new(HOST);
                }
                let Some(c) = client.as_ref() else { break };
                match c.get(&format!("{ROOT}{key}")) {
                    Ok(body) => {
                        let got = hex(&Sha256::digest(&body));
                        if got != sha {
                            eprintln!("[cosmetics] {key}: came changed (try {attempt})");
                            continue;
                        }
                        r.got.fetch_add(body.len() as u64, Ordering::Relaxed);
                        ok = lay_down(r, &key, &body, &sha);
                        break;
                    }
                    Err(e) => {
                        eprintln!("[cosmetics] {key}: {e} (try {attempt})");
                        client = None;
                        std::thread::sleep(std::time::Duration::from_millis(400 * (attempt + 1)));
                    }
                }
            }
        }
        let mut w = r.work.lock().unwrap();
        w.1.remove(&key);
        if !ok {
            w.2.insert(key);
        }
        if w.1.is_empty() {
            // all of it come: the next wave counted anew
            r.got.store(0, Ordering::Relaxed);
            r.asked.store(0, Ordering::Relaxed);
        }
        drop(w);
        r.done.notify_all();
    }
}

/// A file laid down whole (written aside, renamed into its place), kept in the index.
fn lay_down(r: &Remote, key: &str, body: &[u8], sha: &str) -> bool {
    let path = r.dir.join(key);
    if let Some(p) = path.parent() {
        if std::fs::create_dir_all(p).is_err() {
            return false;
        }
    }
    let part = path.with_extension("part");
    if std::fs::write(&part, body).is_err() || std::fs::rename(&part, &path).is_err() {
        let _ = std::fs::remove_file(&part);
        return false;
    }
    let mut have = r.have.lock().unwrap();
    have.insert(key.to_string(), sha.to_string());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(r.dir.join(INDEX)) {
        let _ = writeln!(f, "{sha} {key}");
    }
    true
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_reads_its_lines_and_refuses_a_way_out() {
        let l = parse_list("0123456789012345678901234567890123456789012345678901234567890123 42 items/a/model.rcm\nbad line\n0123456789012345678901234567890123456789012345678901234567890123 1 ../x\n");
        assert_eq!(l.len(), 1);
        assert_eq!(l["items/a/model.rcm"].1, 42);
        assert_eq!(hex(&[0, 255, 16]), "00ff10");
    }
}
