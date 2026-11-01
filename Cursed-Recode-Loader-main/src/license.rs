//! Recode's license, in the game (her rules, 15.10 and 16.10). The loader starts the game with a ticket for one start
//! (CURSED_START and CURSED_BUILD in its environment); the game shows it to the server with this computer's parts and
//! its build, and the server's yes — signed with its key over this start's own random words, so no one else's server
//! and no earlier answer opens it — is the game's own session. Then the game says it is alive every minute: one game
//! at a time on an account (a second one is not let in; one that took the account while this one was cut off tells
//! this one to go), a ban ends it at once. The subscription's last half hour runs out on the screen and the game
//! closes at its end (her word, 16.10); renewed meanwhile, the time goes. Without the server for half an hour the game
//! counts down another half hour (a short fall of the server ends no one's game).
//!
//! Only a release build asks (tools/Release.java builds the library with CURSED_RELEASE); one built from the sources
//! plays as it is (CURSED_DEV_LICENSE shows what the player would see, for the stands). The game's device (`open`)
//! waits for the start's answer, so the Java side cannot skip it; refused, the window before the game goes into the
//! valley and says why, as the loader says it (loader/stop.rs), and the game is ended here. Told to go in the game, the
//! Java side leaves the world and says why (`now`); should it not, the game is ended from here a minute and a half
//! later.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::menus::lang::{tr, trf};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::{json, Value};

/// Built for a release: the game plays only on a start the server confirmed.
pub const RELEASE: bool = option_env!("CURSED_RELEASE").is_some();

/// The site of ours (crate::vault — not itself in the binaries).
pub fn ours() -> &'static str {
    crate::vault::ours()
}

/// The account's page on the site: its subscription, its keys.
pub fn cabinet() -> &'static str {
    crate::vault::cabinet()
}

/// The public half of the key the server signs the builds and the starts with (`cursed dist-key`). A build for the
/// test instance is made with that one's: it is written into the binaries as noise by `build.rs`, from
/// CURSED_VERIFY_KEY when one is given and the site's own when not.
pub fn key() -> &'static str {
    crate::vault::verify_key()
}

/// The server: ours, or the test instance a build was made for (CURSED_VERIFY_API).
fn api() -> &'static str {
    crate::vault::verify_api().trim_end_matches('/')
}

/// The account session confirmed after the loader's one-use game start. It stays in Rust;
/// cloud features never receive a password or the loader's own persistent credential.
pub(crate) fn site_session() -> Option<String> {
    let state = SHARED.lock().ok()?;
    if state.verdict != Some(true) || state.leave.is_some() {
        return None;
    }
    state.session.clone()
}

pub(crate) fn site_api() -> &'static str {
    api()
}

/// The subscription's last stretch on the screen before the game closes at its end.
const WARN: Duration = Duration::from_secs(30 * 60);
/// Without the server this long, the game starts counting down (CLOSE_IN).
const OFFLINE_AFTER: Duration = Duration::from_secs(30 * 60);
const CLOSE_IN: Duration = Duration::from_secs(30 * 60);
/// Told to go, the game has this long to leave by itself before it is ended from here.
const GRACE: Duration = Duration::from_secs(90);

/// What this build has to be kept alive by the server. Not one flag a dumper can flip: a release is also a build
/// that speaks sealed, and a build with the seal's word taken out of it cannot talk to the server at all. Flipping
/// what the build says it is leaves the word sitting in it; taking the word out leaves a build that cannot start
/// anyone's game — and between the two there is no free game to be had by turning one of them.
fn keeps_alive() -> bool {
    kept(RELEASE, crate::vault::seal_key().len() == 64)
}

/// Two things go in: what the build says it is, and what it carries. Neither alone decides — a dumper who flips
/// what the build says still carries the seal's word, and one who takes the word out has a build that cannot speak
/// to the server at all. Folded so the answer is not one jz on one of them.
fn kept(says: bool, carries: bool) -> bool {
    // 0 only when neither is set: a build from the sources that speaks no seal, and plays on its own. Every other
    // combination is a game the server has to keep up.
    (says as u8 | carries as u8) != 0
}

/// Why the time runs out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Why {
    /// The subscription ends.
    Ending,
    /// The server has not answered for half an hour.
    Offline,
}

/// What stops the game, as the screen says it: what it is about (small, above), the headline, the words, and what may
/// be pressed (action, label, kind: 1 the main one, 2 the way out).
#[derive(Clone, Debug)]
pub struct Stop {
    pub cap: String,
    pub title: String,
    pub text: String,
    pub words: Vec<(String, String, i64)>,
}

/// What the game is told.
#[derive(Clone, Debug)]
pub enum Say {
    Fine,
    /// Over everything: the time running out, until when, and why.
    Closing { end: Instant, why: Why },
    /// Go now: what the screen says; since when.
    Leave { stop: Stop, since: Instant },
}

struct Shared {
    /// Told to go (kept: once told, it goes).
    leave: Option<(Stop, Instant)>,
    /// The subscription's end (None: for ever, or not known).
    ends: Option<Instant>,
    /// Without the server: when the game closes.
    cut: Option<Instant>,
    /// The start's answer: None while it is asked.
    verdict: Option<bool>,
    /// The game's session (the server's answer), for its words later and its goodbye.
    session: Option<String>,
    /// When the server was last heard from: when this game asked to start, and every word that came back after.
    /// A game whose start was never confirmed has none at all — that is not a slow start, that is no start — and a
    /// game whose words stopped reaching the server goes stale. Both are the same thing from here: this game has no
    /// business playing, and it is not left to its own to notice.
    heard_at: Option<Instant>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { leave: None, ends: None, cut: None, verdict: None, session: None, heard_at: None });
static ANSWERED: Condvar = Condvar::new();

/// The words' language: the game's, or before it has one its options' (the game's folder: where it runs, or above
/// the program's own bin\), else Russian (the others' players read Russian better than English).
fn speak() {
    use crate::menus::lang::{code, set};
    if !code().is_empty() {
        return;
    }
    let mut places = vec![std::path::PathBuf::from("options.txt")];
    if let Some(home) = std::env::current_exe().ok().and_then(|e| e.parent().and_then(|b| b.parent()).map(|h| h.join("options.txt"))) {
        places.push(home);
    }
    let said = places.iter().find_map(|p| {
        let text = std::fs::read_to_string(p).ok()?;
        text.lines().find_map(|l| l.strip_prefix("lang:")).map(|l| l.trim().to_string()).filter(|l| !l.is_empty())
    });
    set(&said.unwrap_or_else(|| "ru_ru".into()));
}

/// What the screen says for a refusal of the server's (its `error`), at the start or in the game. `no-recode` with
/// `until` (or `ended`): it had one, it ended.
///
/// The words it is told are looked up among the ones the binaries keep as noise (crate::vault), so the choice of
/// what to say is not sitting next to the word itself for a dumper to read.
pub fn stop(v: &Value) -> Stop {
    speak();
    let w = |a: &str, key: &str, kind: i64| (a.to_string(), tr(key).to_string(), kind);
    let open = || w(crate::vault::open_loader(), "lic.openloader", 1);
    let quit = || w("quit", "quit", 2);
    let said = v["error"].as_str().unwrap_or("");
    let (cap, title, text, words): (&str, &str, String, Vec<(String, String, i64)>) = if said == crate::vault::banned() {
        let reason = v["reason"].as_str().unwrap_or("").trim();
        let mut text = if reason.is_empty() { String::new() } else { trf("lic.reason", &[reason]) };
        if let Some(until) = v["until"].as_i64() {
            let days = ((until - unix_now()) as f64 / 86_400.0).ceil().max(1.0) as i64;
            text.push(' ');
            text.push_str(&trf("lic.banends", &[&days_of(days)]));
        }
        ("lic.cap.account", "lic.banned", text.trim().to_string(), vec![quit()])
    } else if said == crate::vault::in_game() {
        ("lic.cap.account", "lic.ingame", tr("lic.ingame.text").into(), vec![quit()])
    } else if said == crate::vault::no_recode() {
        if v["until"].is_i64() || v["ended"].as_bool() == Some(true) {
            ("lic.cap.sub", "lic.ended", tr("lic.ended.text").into(), vec![w("site", "lic.site", 1), quit()])
        } else {
            ("lic.cap.sub", "lic.nosub", tr("lic.nosub.text").into(), vec![w("site", "lic.site", 1), quit()])
        }
    } else if said == crate::vault::no_tpm() {
        ("lic.cap.pc", "lic.tpm", tr("lic.tpm.text").into(), vec![quit()])
    } else if said == crate::vault::other_device() {
        ("lic.cap.pc", "lic.device", tr("lic.device.text").into(), vec![open(), quit()])
    } else if said == crate::vault::device_taken() {
        ("lic.cap.pc", "lic.device", tr("lic.taken.text").into(), vec![open(), quit()])
    } else if said == crate::vault::outdated() {
        ("lic.cap.update", "lic.outdated", tr("lic.outdated.text").into(), vec![open(), quit()])
    } else if said == crate::vault::offline() {
        ("lic.cap.net", "lic.offline", tr("lic.offline.text").into(), vec![open(), quit()])
    } else if said == crate::vault::offline_long() {
        ("lic.cap.net", "lic.offline", tr("lic.offlinelong.text").into(), vec![open(), quit()])
    } else if said == crate::vault::signed_out() {
        ("lic.cap.start", "lic.session", tr("lic.session.text").into(), vec![open(), quit()])
    } else {
        ("lic.cap.start", "lic.loader", tr("lic.loader.text").into(), vec![open(), quit()])
    };
    Stop { cap: tr(cap).into(), title: tr(title).into(), text, words }
}

/// `n` days, in the words' language.
fn days_of(n: i64) -> String {
    use crate::menus::lang::{lang, Lang};
    match lang() {
        Lang::Ru => format!("{n} {}", plural(n, "день", "дня", "дней")),
        Lang::Uk => format!("{n} {}", plural(n, "день", "дні", "днів")),
        Lang::Kk => format!("{n} күн"),
        Lang::En => format!("{n} day{}", if n == 1 { "" } else { "s" }),
    }
}

fn plural(n: i64, one: &str, few: &str, many: &str) -> String {
    let (m10, m100) = (n % 10, n % 100);
    if m10 == 1 && m100 != 11 {
        one
    } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
        few
    } else {
        many
    }
    .to_string()
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// This start's random words (64 hex), which the server's signature takes in.
fn nonce() -> Option<String> {
    use windows_sys::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
    let mut bytes = [0u8; 32];
    let status = unsafe { BCryptGenRandom(std::ptr::null_mut(), bytes.as_mut_ptr(), bytes.len() as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    (status == 0).then(|| bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// What the server signs for a confirmed start (the server's dist.rs, `start_message`).
fn start_message(nonce: &str, ticket: &str, uid: i64, build: i64) -> String {
    format!("cursed-client-verify/1\n{nonce}\n{ticket}\n{uid}\n{build}")
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// Whether `sig` is the server's over this start.
fn signed(key: &str, nonce: &str, ticket: &str, uid: i64, build: i64, sig: &str) -> bool {
    let Some(key) = hex_bytes(key).and_then(|k| <[u8; 32]>::try_from(k).ok()).and_then(|k| VerifyingKey::from_bytes(&k).ok()) else {
        return false;
    };
    let Some(sig) = hex_bytes(sig).and_then(|s| <[u8; 64]>::try_from(s).ok()) else {
        return false;
    };
    key.verify(start_message(nonce, ticket, uid, build).as_bytes(), &Signature::from_bytes(&sig)).is_ok()
}

/// A request to the server: its status and its JSON (None: it did not reach it). What is said goes sealed, and an
/// answer that is not sealed under the same seal is not believed (net.rs); a refusal comes plain, as it always did.
/// Built with no word of its own, this one speaks as it always did (net.rs: `sealing`).
fn ask(path: &str, token: Option<&str>, body: &Value) -> Option<(u16, Value)> {
    let auth = token.map(|t| format!("Bearer {t}"));
    let mut headers = vec![("Content-Type", "application/json")];
    if let Some(a) = auth.as_deref() {
        headers.push(("Authorization", a));
    }
    let sealed = crate::net::sealing();
    // the letter's own words, so that the answer can name them: an answer to another one is not this one's
    let mut asked = String::new();
    let words = if sealed {
        let (letter, nonce) = crate::net::seal("POST", path, body)?;
        asked = nonce;
        letter.to_string()
    } else {
        body.to_string()
    };
    let mut got = Vec::new();
    let head = crate::net::request("POST", &format!("{}{path}", api()), &headers, Some(words.as_bytes()), &mut |_, c| {
        got.extend_from_slice(c);
        got.len() < 1 << 20
    })
    .ok()?;
    if head.status >= 300 {
        return Some((head.status, serde_json::from_slice(&got).unwrap_or(Value::Null)));
    }
    if !sealed {
        return Some((head.status, serde_json::from_slice(&got).unwrap_or(Value::Null)));
    }
    let envelope: Value = serde_json::from_slice(&got).ok()?;
    Some((head.status, crate::net::open("POST", path, &asked, &envelope)?))
}

/// Started not from the loader (no tickets of its): the game's program sees it before Java (src/bin/host.rs).
///
/// Two tickets come in the loader's environment and the game needs both. `CURSED_TICKET` is the pair's —
/// what their own copy of the library is keyed by (protect-stub reads it, and nothing else will open the
/// seal). `CURSED_START` is this one start's own words, spent here when the game says it is starting. They
/// are not the same string and neither stands for the other: one opens a copy on disk, the other is a
/// single-use word for the server.
/// This one start's own words, the once they are in hand. The environment is not a place to keep
/// them: a JVM comes up between the first look at them and the moment they are spent, and a dump of
/// a process with a JVM in it reads the whole block. `no_ticket` takes them in the game's program and
/// `verify` spends them in the game's library — one process, but two copies of this code, each with
/// a stash of its own: the library asks the program for them (`from_the_program`).
static START: Mutex<String> = Mutex::new(String::new());

/// How long an environment variable is, without ever taking a copy of it. `no_ticket` has only
/// ever asked how long the two tickets were — and a value that is never read is a value that is
/// never in a buffer a dump can take. (A value that is read is not so lucky: `std::env::var` drops
/// its copy without wiping and leaves the wide one Windows used to hand it over in a dropped `Vec`
/// of its own. Both are why `env_take` below does the read by hand.)
fn env_len(name: &str) -> usize {
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn GetEnvironmentVariableW(name: *const u16, buf: *mut u16, size: u32) -> u32;
    }
    let wide: Vec<u16> = std::ffi::OsStr::new(name).encode_wide().chain(std::iter::once(0)).collect();
    // a first ask with no buffer answers only how long it is, and hands over nothing
    let need = unsafe { GetEnvironmentVariableW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    need.saturating_sub(1) as usize
}

/// Write this name's value over **inside the environment block itself**, without asking Windows
/// to change anything.
///
/// `SetEnvironmentVariable` never edits the block: it builds a new one and frees the old. The old
/// one is still readable in the heap for as long as the allocator leaves it there — a dump of the
/// game after the variable is «gone» walks straight into the value it was supposed to have lost.
/// So the value is written over first, in the one block every later call will copy from, and only
/// then is the entry removed: every block freed afterwards is freed with nothing to find in it.
/// It is masked with `MASK` and not zeroed — the block is a list of strings ended by a double NUL,
/// so a zero in a value ends the list early and takes every later name with it.
/// What a taken value is written over with. Not zero: see the mask inside.
const MASK: u16 = b'*' as u16;

fn env_scrub(name: &str) {
    // x64: the TEB is what GS points at (its own Self pointer sits at +0x30 — that is what
    // `NtCurrentTeb` reads), the PEB hangs off it at +0x60, the process parameters off that at
    // +0x20, and the block they keep at +0x80. `GetEnvironmentStringsW` hands out a *copy* of it:
    // a look that writes there changes nothing a dump would find. Ask this machine — the value is
    // still readable through `GetEnvironmentVariableW` afterwards.
    let block: *mut u16 = unsafe {
        let teb: usize;
        core::arch::asm!("mov {}, gs:[0x30]", out(reg) teb, options(nostack, preserves_flags, readonly));
        if teb == 0 {
            return;
        }
        let peb = *(teb.wrapping_add(0x60) as *const usize);
        if peb == 0 {
            return;
        }
        let params = *(peb.wrapping_add(0x20) as *const usize);
        if params == 0 {
            return;
        }
        *(params.wrapping_add(0x80) as *const usize) as *mut u16
    };
    if block.is_null() {
        return;
    }
    let mut prefix: Vec<u16> = name.encode_utf16().collect();
    for w in prefix.iter_mut() {
        if (*w as u32) < 128 && (*w as u8).is_ascii_alphabetic() {
            *w = (*w as u8).to_ascii_uppercase() as u16;
        }
    }
    prefix.push(b'=' as u16);
    unsafe {
        let mut at = block;
        let mut entries = 0usize;
        while entries < 4096 {
            let mut n = 0usize;
            while n < 4096 && *at.add(n) != 0 {
                n += 1;
            }
            if n == 0 {
                break; // the block's own end: \0\0
            }
            if n >= 4096 {
                return; // not a block of `NAME=VALUE` at all — leave the memory alone
            }
            entries += 1;
            // names are case-insensitive here, so the entry is matched the way Windows matches
            let mut hit = n >= prefix.len();
            if hit {
                for (i, p) in prefix.iter().enumerate() {
                    let mut c = *at.add(i);
                    if (c as u32) < 128 && (c as u8).is_ascii_alphabetic() {
                        c = (c as u8).to_ascii_uppercase() as u16;
                    }
                    if c != *p {
                        hit = false;
                        break;
                    }
                }
            }
            if hit {
                // Masked, never zeroed. The block is a list of `NAME=VALUE` strings ended by a
                // double NUL, so a zero written over the value makes every walker — and the
                // `SetEnvironmentVariable` that comes next — see the block end right there, and
                // every name that sorts after this one (`PATH`, `TEMP`, `USERPROFILE`, the lot)
                // is dropped on the floor. The mask is not the value: a dump reads `********…`
                // where the words were, and the entry still ends where it always did.
                for i in prefix.len()..n {
                    std::ptr::write_volatile(at.add(i), MASK);
                }
                std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
                return;
            }
            at = at.add(n + 1);
        }
    }
}

/// Where `needle` sits in `hay` at or after `from`.
fn find_from(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let end = hay.len() - needle.len();
    let mut i = from.min(end + 1);
    while i <= end {
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Every other copy of these words in this process, written over with the mask.
///
/// The environment block is not the only copy a take leaves behind. The CRT keeps a **narrow** copy
/// of the whole block from the moment the process starts — the `environ` that `envp` points at — and
/// nothing in the Windows environment API ever updates it: `SetEnvironmentVariable` rebuilds the wide
/// block and that is all it does. A dump reads that narrow copy as easily as the block itself (the
/// look found `CURSED_TICKET=…` sitting in two of them, whole `NAME=VALUE` runs of ASCII). The same
/// words also turn up in every dropped buffer a read makes along the way.
///
/// So once the words are in hand they are looked for in every writable page of this process — as they
/// are and as their wide form, which is how an environment block and a Java String hold them — and
/// written over where they are found. `keep` is the one buffer still allowed to hold them: what is
/// about to be handed on. Never a zero for the same reason the block itself is masked and not zeroed
/// — but here it is simpler than that: these pages are not a list of strings, and a `*` is simply
/// not the words.
fn scrub_copies(words: &[u8], keep: &[(usize, usize)]) {
    use windows_sys::Win32::System::Memory::{
        VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_MAPPED, MEM_PRIVATE,
        PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS, PAGE_READWRITE,
        PAGE_WRITECOPY,
    };
    if words.is_empty() {
        return;
    }
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn ReadProcessMemory(h: *mut std::ffi::c_void, base: *const u8, buf: *mut u8, size: usize, got: *mut usize) -> i32;
    }
    // both tickets are the 64 hex characters, so the wide form is this byte and a zero after it
    let mut wide: Vec<u8> = words.iter().flat_map(|&b| [b, 0]).collect();
    let needles: [&[u8]; 2] = [words, &wide];
    let mut at = 0usize;
    let mut mbi = MEMORY_BASIC_INFORMATION {
        BaseAddress: std::ptr::null_mut(),
        AllocationBase: std::ptr::null_mut(),
        AllocationProtect: 0,
        PartitionId: 0,
        RegionSize: 0,
        State: 0,
        Protect: 0,
        Type: 0,
    };
    while at < 0x0000_7fff_ffff_ffff {
        let got = unsafe {
            VirtualQuery(at as *const _, &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>())
        };
        if got == 0 {
            break;
        }
        let base = mbi.BaseAddress as usize;
        let size = mbi.RegionSize;
        let next = base.saturating_add(size);
        if next <= at {
            break;
        }
        at = next;
        // only pages a dump can read the words out of and this can write over: our own, writable,
        // not a guard and not the images (the stub's own copy is its business — it folds it into
        // the key and wipes it there)
        if mbi.State != MEM_COMMIT || size == 0 {
            continue;
        }
        if mbi.Type != MEM_PRIVATE && mbi.Type != MEM_MAPPED {
            continue;
        }
        let p = mbi.Protect & 0xff;
        if mbi.Protect & PAGE_GUARD != 0 || p == PAGE_NOACCESS {
            continue;
        }
        if p != PAGE_READWRITE && p != PAGE_WRITECOPY && p != PAGE_EXECUTE_READWRITE && p != PAGE_EXECUTE_WRITECOPY {
            continue;
        }
        if size > (256 << 20) {
            continue; // no one keeps a 64-byte string in a quarter of a gigabyte of one region
        }
        // read out first: this is this process's own memory and another thread may be changing what
        // is mapped under a plain read. `ReadProcessMemory` answers with what it got instead of
        // falling over.
        let mut copy = vec![0u8; size];
        let mut got_n = 0usize;
        let ok = unsafe {
            ReadProcessMemory(GetCurrentProcess(), base as *const u8, copy.as_mut_ptr(), size, &mut got_n)
        };
        if ok == 0 || got_n == 0 {
            continue;
        }
        let hay = &copy[..got_n];
        for needle in needles {
            let mut from = 0usize;
            while let Some(hit) = find_from(hay, needle, from) {
                from = hit + 1;
                // `keep` is in addresses this process runs with, and `hit` is where in this region
                // the words sit — the two are only comparable once the region's own base is on.
                let there = base + hit;
                if keep.iter().any(|&(k, n)| there < k.saturating_add(n) && k < there.saturating_add(needle.len())) {
                    continue;
                }
                // still the same writable run of pages it was when it was read, whole
                let mut probe = mbi;
                let g2 = unsafe {
                    VirtualQuery(there as *const _, &mut probe, std::mem::size_of::<MEMORY_BASIC_INFORMATION>())
                };
                if g2 == 0 {
                    continue;
                }
                let pp = probe.Protect & 0xff;
                if probe.State != MEM_COMMIT
                    || (probe.Type != MEM_PRIVATE && probe.Type != MEM_MAPPED)
                    || probe.Protect & PAGE_GUARD != 0
                    || !(pp == PAGE_READWRITE || pp == PAGE_WRITECOPY || pp == PAGE_EXECUTE_READWRITE || pp == PAGE_EXECUTE_WRITECOPY)
                {
                    continue;
                }
                if there + needle.len() > probe.BaseAddress as usize + probe.RegionSize {
                    continue;
                }
                let there = there as *mut u8;
                for i in 0..needle.len() {
                    unsafe { std::ptr::write_volatile(there.add(i), MASK as u8) };
                }
            }
        }
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    // and the wide form this made to look with goes too
    for b in wide.iter_mut() {
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

/// An environment variable taken into a buffer of our own, and every buffer that held it zeroed —
/// the wide one Windows used to hand it over as well, and the block itself before it is rebuilt.
/// See `env_len` for what `std::env::var` leaves behind.
fn env_take(name: &str) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn GetEnvironmentVariableW(name: *const u16, buf: *mut u16, size: u32) -> u32;
        fn SetEnvironmentVariableW(name: *const u16, value: *const u16) -> i32;
    }
    let wide_name: Vec<u16> = std::ffi::OsStr::new(name).encode_wide().chain(std::iter::once(0)).collect();
    let need = unsafe { GetEnvironmentVariableW(wide_name.as_ptr(), std::ptr::null_mut(), 0) };
    if need == 0 {
        return Vec::new();
    }
    let mut wide = vec![0u16; need as usize];
    let got = unsafe { GetEnvironmentVariableW(wide_name.as_ptr(), wide.as_mut_ptr(), need) };
    let out = if got == 0 {
        Vec::new()
    } else {
        String::from_utf16_lossy(&wide[..got as usize]).into_bytes()
    };
    for w in wide.iter_mut() {
        unsafe { std::ptr::write_volatile(w, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    // the block first: the call below rebuilds it and frees this one with whatever is still in it
    env_scrub(name);
    unsafe { SetEnvironmentVariableW(wide_name.as_ptr(), std::ptr::null()) };
    // and every other copy this process ever made of them — the CRT's narrow block from its first
    // moment, a dropped buffer from a read along the way — is written over too. `keep` is this very
    // buffer: it is what is being handed on.
    if !out.is_empty() {
        let keep = [(out.as_ptr() as usize, out.len())];
        scrub_copies(&out, &keep);
    }
    out
}

/// Zero a buffer so a dump cannot read it back. The same promise the stub makes over the key.
fn wipe(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

/// Take this start's words out of the environment and keep them here. Called the first time
/// anything looks at them — which is before Java, so no dump of the running game finds them.
fn stash_start() {
    let mut t = env_take(crate::vault::env_start());
    if t.is_empty() {
        return;
    }
    if let Ok(mut g) = START.lock() {
        *g = String::from_utf8_lossy(&t).into_owned();
    }
    // the take's own buffer goes as well: the stash above is the only place these words are kept
    wipe(&mut t);
}

/// This start's words for the server. Prefers the stash; a program that never called `no_ticket`
/// takes them now and wipes the environment's copy either way. The game's library finds neither:
/// its stash is not the program's and the program already took them out of the environment — so it
/// asks the program for them.
fn take_start() -> String {
    let from = START.lock().map(|mut g| std::mem::take(&mut *g)).unwrap_or_default();
    if !from.is_empty() {
        return from;
    }
    let mut t = env_take(crate::vault::env_start());
    if t.is_empty() {
        t = from_the_program();
    }
    let words = String::from_utf8_lossy(&t).into_owned();
    wipe(&mut t);
    words
}

/// This start's words, out of the game's program (CursedRecode.exe, `cursed_recode_start`), which
/// took them before Java (`no_ticket`). A library loaded by a program that is not ours — a stand's
/// java.exe — finds no such door, and the start goes as one with no ticket.
fn from_the_program() -> Vec<u8> {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    let program = unsafe { GetModuleHandleW(std::ptr::null()) };
    if program.is_null() {
        return Vec::new();
    }
    let Some(at) = (unsafe { GetProcAddress(program, c"cursed_recode_start".as_ptr() as *const u8) }) else {
        return Vec::new();
    };
    let hand: unsafe extern "system" fn(*mut u8, usize) -> usize = unsafe { std::mem::transmute(at) };
    let mut words = vec![0u8; 64];
    let n = unsafe { hand(words.as_mut_ptr(), words.len()) };
    words.truncate(n.min(64));
    words
}

/// The program's side of `from_the_program`: the stash, into the library's buffer, once — and gone
/// from here as it goes. A buffer too short for them is handed nothing, and they stay.
///
/// # Safety
/// `buf` is null or writable for `len` bytes.
pub unsafe fn hand_start(buf: *mut u8, len: usize) -> usize {
    if buf.is_null() {
        return 0;
    }
    let Ok(mut g) = START.lock() else { return 0 };
    let n = g.len();
    if n == 0 || n > len {
        return 0;
    }
    std::ptr::copy_nonoverlapping(g.as_ptr(), buf, n);
    let mut words = std::mem::take(&mut *g).into_bytes();
    wipe(&mut words);
    n
}

pub fn no_ticket() -> bool {
    let answer = if RELEASE {
        // lengths only: the strings themselves are never in a buffer here, and what the seal needs
        // is still standing in the environment for `keep_the_pair_from_java` to hand over
        let pair = env_len(crate::vault::env_ticket()) != 64;
        let start = env_len(crate::vault::env_start()) != 64;
        pair || start
    } else {
        std::env::var(crate::vault::env_dev_license()).is_ok_and(|v| v == "host")
    };
    // looked at for the last time here. `verify` is the only thing that spends them and it gets
    // them from this stash (the library through `cursed_recode_start`), so from here the
    // environment's copy is of no use to anyone but a dump — and what is still in it is one of the
    // two strings that start a player's game. The pair's own ticket goes to the stub next, before
    // the JVM is up (`keep_the_pair_from_java`).
    stash_start();
    answer
}

/// The pair's own ticket, out of the environment and into the stub's hands — before a byte of the
/// JVM is brought up. A JVM that starts with the variable still standing keeps a copy of every name
/// in the block (`System.getenv` caches the whole of it), and this is the one string that opens the
/// player's own copy of the game. protect-stub folds it into the seal key and wipes it; what the
/// game spends on the license is `CURSED_START`, a different string that `no_ticket` already took.
///
/// The stub is the same program's own library (`-Dcursed.recode.native=…`, or beside this one) and
/// answers on `cursed_recode_keep`. It is never put back if the stub cannot be reached: a pair that
/// cannot open its seal is a pair that does not start, which is the right answer — the environment
/// is the one place a dump reads without any skill at all. (A stand that loads the library on its
/// own still finds it there: this is the game's program, not the stub's own `read_ticket`.)
pub fn keep_the_pair_from_java() {
    let name = crate::vault::env_ticket();
    let mut pair = env_take(name);
    if pair.is_empty() {
        return;
    }
    let _ = give_to_the_stub(&pair);
    wipe(&mut pair);
}

/// Hand the pair's ticket to protect-stub, which is this very process's own library. True when the
/// stub took it.
fn give_to_the_stub(pair: &[u8]) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    let Some(path) = stub_path() else { return false };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let module = unsafe { LoadLibraryW(wide.as_ptr()) };
    if module.is_null() {
        return false;
    }
    let Some(at) = (unsafe { GetProcAddress(module, c"cursed_recode_keep".as_ptr() as *const u8) }) else {
        return false;
    };
    let keep: unsafe extern "system" fn(*const u8, usize) = unsafe { std::mem::transmute(at) };
    unsafe { keep(pair.as_ptr(), pair.len()) };
    true
}

/// Where protect-stub is: where the launch named it (`-Dcursed.recode.native=…`), else beside this
/// program. The game's library and its library's library live in the same folder of an install.
fn stub_path() -> Option<std::path::PathBuf> {
    for a in std::env::args() {
        if let Some(v) = a.strip_prefix("-Dcursed.recode.native=") {
            let p = std::path::PathBuf::from(v);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let beside = std::env::current_exe().ok()?.parent()?.join("cursed_recode.dll");
    beside.is_file().then_some(beside)
}

/// Started not from the loader: said as any refusal at the start is, before Java (src/bin/host.rs).
pub fn refuse_unticketed() -> ! {
    refuse(&json!({ "error": crate::vault::loader() }))
}

/// The start: whether the game may play, its session, how often it says it is alive and its subscription; if not, why.
fn verify() -> Result<(String, u64, Value), Value> {
    // this one start's own words — not the pair's. The pair's ticket is for the seal on disk and stays the
    // same for as long as its window stands; this one is spent here and never again. Out of the stash
    // `no_ticket` made, so it is not sitting in the environment between the two.
    let ticket = take_start();
    if ticket.len() != 64 {
        return Err(json!({ "error": crate::vault::loader() }));
    }
    let build: i64 = std::env::var(crate::vault::env_build()).ok().and_then(|b| b.parse().ok()).unwrap_or(0);
    let nonce = nonce().ok_or_else(|| json!({ "error": crate::vault::loader() }))?;
    let body = json!({ "ticket": ticket, "device": crate::device::parts_json(), "build": build, "nonce": nonce });
    let (status, v) = ask(crate::vault::path_verify(), None, &body).ok_or_else(|| json!({ "error": crate::vault::offline() }))?;
    if status != 200 {
        return Err(if v["error"].is_string() { v } else { json!({ "error": if status >= 500 { "offline" } else { "loader" } }) });
    }
    // not signed by our server for this very start: someone else's yes
    if !signed(key(), &nonce, &ticket, v["uid"].as_i64().unwrap_or(-1), build, v["sig"].as_str().unwrap_or("")) {
        return Err(json!({ "error": crate::vault::loader() }));
    }
    let token = v["token"].as_str().filter(|t| t.len() == 64).ok_or_else(|| json!({ "error": crate::vault::loader() }))?;
    // Discord shows who plays: the account the ticket belongs to
    if let (Some(uid), Some(login)) = (v["uid"].as_i64(), v["login"].as_str()) {
        crate::rpc::set_profile(&v["uid_text"].as_str().map_or_else(|| uid.to_string(), str::to_string), login);
    }
    Ok((token.to_string(), v["alive_every"].as_u64().unwrap_or(60).clamp(20, 600), v["recode"].clone()))
}

/// Started once, from JNI_OnLoad: a release build asks the server, one from the sources plays (or shows what
/// CURSED_DEV_LICENSE asks for).
pub fn start() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // This game is asking. From here it has to be kept up: `heard_at` is what says so to anyone who looks at
        // the state later. A start that is never confirmed leaves it where it was — none — and that is not a slow
        // start, that is no start.
        if let Ok(mut s) = SHARED.lock() {
            if s.heard_at.is_none() {
                s.heard_at = Some(Instant::now());
            }
        }
        let _ = std::thread::Builder::new().name("Cursed license".into()).spawn(|| {
            if RELEASE {
                let outcome = std::panic::catch_unwind(verify).unwrap_or_else(|_| Err(json!({ "error": crate::vault::loader() })));
                match outcome {
                    Ok((token, every, recode)) => {
                        subscription(&recode, unix_now());
                        answered(true, Some(token.clone()));
                        alive(token, Duration::from_secs(every));
                    }
                    Err(v) => {
                        answered(false, None);
                        refuse(&v);
                    }
                }
            } else if let Some(v) = rehearsed_refusal() {
                answered(false, None);
                refuse(&v);
            } else {
                answered(true, None);
                rehearse();
            }
        });
    });
}

fn answered(yes: bool, session: Option<String>) {
    if let Ok(mut s) = SHARED.lock() {
        s.verdict = Some(yes);
        s.session = session;
        if yes {
            // the server just said this game may play
            s.heard_at = Some(Instant::now());
        }
    }
    ANSWERED.notify_all();
}

/// The game's device asks first (lib.rs, `open`): a release build goes on only once the server said yes. Refused, it
/// waits here while the refusal is shown and the game is ended.
pub fn confirm() {
    if !RELEASE && rehearsed_refusal().is_none() {
        return;
    }
    start();
    let mut s = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    while s.verdict.is_none() {
        s = ANSWERED.wait(s).unwrap_or_else(|e| e.into_inner());
    }
    if s.verdict == Some(false) {
        drop(s);
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
    // The game is opening. A build that has to be kept alive and has no session is not opening — not here, not now.
    // The verdict above says the start was answered; this says it was answered by the server.
    let heard = s.heard_at;
    drop(s);
    if keeps_alive() && heard.is_none() {
        end();
    }
}

/// Refused at the start: the window before the game goes into the valley and says why (loader/stop.rs; Windows' own
/// message where that window cannot be made), and the game is ended.
pub fn refuse(v: &Value) -> ! {
    let s = stop(v);
    eprintln!("[license] refused: {} ({})", s.title, v["error"].as_str().unwrap_or(""));
    if !crate::loader::stop::show(&s) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_TOPMOST};
        let wide = |t: &str| t.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        unsafe {
            MessageBoxW(std::ptr::null_mut(), wide(&format!("{}\n\n{}", s.title, s.text)).as_ptr(), wide("Cursed Recode").as_ptr(), MB_OK | MB_ICONERROR | MB_TOPMOST | MB_SETFOREGROUND);
        }
    }
    end()
}

fn end() -> ! {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
    unsafe {
        TerminateProcess(GetCurrentProcess(), 1);
    }
    std::process::exit(1)
}

/// Started where the game is loaded (lib.rs `JNI_OnLoad`) and where it opens (lib.rs `open`): a second pair of eyes
/// on what `now` says, ending the process where the game does not go on its own. Two places start it because a
/// dumper who finds one still has the other, and it is not called what it does.
pub fn watch() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = std::thread::Builder::new().name("Cursed clock".into()).spawn(|| {
            let opened = Instant::now();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if !keeps_alive() {
                    continue;
                }
                let since = {
                    let s = SHARED.lock().unwrap_or_else(|e| e.into_inner());
                    going(&s, opened)
                };
                if since.is_some_and(|at| at.elapsed() > GRACE) {
                    end();
                }
            }
        });
    });
}

/// Whether this game goes now, and when that began: told to go (its grace is running), never started at all (no
/// session, and there is not going to be one), or the words stopped reaching the server and its two half hours are
/// already up. The rest is the game's own to say on its screen.
fn going(s: &Shared, opened: Instant) -> Option<Instant> {
    match (&s.leave, s.heard_at) {
        (Some((_, at)), _) => Some(*at),
        (None, None) => Some(opened),
        (None, Some(at)) if at.elapsed() >= OFFLINE_AFTER + CLOSE_IN => Some(at + OFFLINE_AFTER + CLOSE_IN),
        _ => None,
    }
}

/// Told to go (once).
fn leave(v: &Value) {
    let s = stop(v);
    if let Ok(mut sh) = SHARED.lock() {
        if sh.leave.is_none() {
            sh.leave = Some((s, Instant::now()));
        }
    }
}

/// The subscription as the server said it (`recode`: active, forever, until — unix seconds, the server's `now`): its
/// end, or none for ever. Not active: the game goes now.
fn subscription(recode: &Value, server_now: i64) {
    if recode.is_null() {
        return;
    }
    if recode["active"].as_bool() == Some(false) {
        leave(&json!({ "error": crate::vault::no_recode(), "ended": true }));
        return;
    }
    let ends = match recode["until"].as_i64() {
        Some(until) if recode["forever"].as_bool() != Some(true) => Some(Instant::now() + Duration::from_secs(until.saturating_sub(server_now).max(0) as u64)),
        _ => None,
    };
    if let Ok(mut s) = SHARED.lock() {
        s.ends = ends;
    }
}

/// What the game is told now: the subscription's last half hour, the server gone a while, a time run out is the time
/// to go. Also, and first: a game that has to be kept alive by the server and is not. Its start was never confirmed
/// — no session at all — or the words stopped reaching the server long ago. The first is not a slow start, that is
/// no start, and it goes at once; the second gets the same two half hours as the net gone.
pub fn now() -> Say {
    now_keeping(keeps_alive())
}

fn now_keeping(kept: bool) -> Say {
    let mut s = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((stop, since)) = &s.leave {
        return Say::Leave { stop: stop.clone(), since: *since };
    }
    if kept {
        match s.heard_at {
            None => {
                drop(s);
                leave(&json!({ "error": crate::vault::no_recode(), "ended": true }));
                return now_keeping(kept);
            }
            Some(at) => {
                if at.elapsed() >= OFFLINE_AFTER && s.cut.is_none() {
                    s.cut = Some(at + OFFLINE_AFTER + CLOSE_IN);
                }
            }
        }
    }
    let t = Instant::now();
    let mut closing: Option<(Instant, Why)> = s.ends.filter(|e| e.saturating_duration_since(t) <= WARN).map(|e| (e, Why::Ending));
    if let Some(cut) = s.cut {
        if closing.is_none_or(|(e, _)| cut < e) {
            closing = Some((cut, Why::Offline));
        }
    }
    match closing {
        Some((end, why)) if t >= end => {
            drop(s);
            leave(&json!({ "error": if why == Why::Ending { crate::vault::no_recode() } else { crate::vault::offline_long() }, "ended": true }));
            now()
        }
        Some((end, why)) => Say::Closing { end, why },
        None => Say::Fine,
    }
}

/// The game alive, every minute (the server's `alive_every`): see the module.
fn alive(token: String, every: Duration) {
    let mut heard = Instant::now();
    let mut next = Instant::now() + every;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let t = Instant::now();
        if let Say::Leave { since, .. } = now() {
            if t.duration_since(since) > GRACE {
                end();
            }
            continue;
        }
        if t < next {
            continue;
        }
        next = t + every;
        let answer = std::panic::catch_unwind(|| ask(crate::vault::path_alive(), Some(&token), &json!({ "device": crate::device::parts_json() }))).ok().flatten();
        match answer {
            Some((200, v)) => {
                heard = Instant::now();
                // the server is back; renewed, the end moves (for ever: none)
                if let Ok(mut s) = SHARED.lock() {
                    s.cut = None;
                    s.heard_at = Some(heard);
                }
                subscription(&v["recode"], v["now"].as_i64().unwrap_or_else(unix_now));
            }
            Some((403 | 401, v)) if v["error"].is_string() => {
                // the subscription is over (its end, or its key taken back): the game closes now (her word, 16.10)
                if v["error"].as_str().unwrap_or("") == crate::vault::no_recode() {
                    leave(&json!({ "error": crate::vault::no_recode(), "ended": true }));
                } else {
                    leave(&v);
                }
            }
            // the server fell, or the net: the game plays on for half an hour, then the time runs out on the screen
            _ => {
                if heard.elapsed() >= OFFLINE_AFTER {
                    if let Ok(mut s) = SHARED.lock() {
                        if s.cut.is_none() {
                            s.cut = Some(heard + OFFLINE_AFTER + CLOSE_IN);
                        }
                    }
                }
            }
        }
    }
}

/// The game closes (its shutdown, Native.licenseBye): the account is free for the next start at once. At most a moment.
pub fn bye() {
    let Some(token) = SHARED.lock().ok().and_then(|s| s.session.clone()) else {
        return;
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new().name("Cursed license bye".into()).spawn(move || {
        let _ = ask(crate::vault::path_bye(), Some(&token), &json!({}));
        let _ = tx.send(());
    });
    let _ = rx.recv_timeout(Duration::from_millis(1500));
}

/// A build from the sources shows the start refused as a player would see it (CURSED_DEV_LICENSE=refuse[:error]: the
/// server's error, `loader` when none).
fn rehearsed_refusal() -> Option<Value> {
    let what = std::env::var(crate::vault::env_dev_license()).ok()?;
    let rest = what.strip_prefix(crate::vault::refuse())?;
    let error = rest.strip_prefix(':').filter(|e| !e.is_empty()).unwrap_or(crate::vault::loader());
    Some(if error == crate::vault::banned() {
        json!({ "error": crate::vault::banned(), "reason": "Проверка стенда", "until": unix_now() + 3 * 86_400 })
    } else if error == crate::vault::ended() {
        json!({ "error": crate::vault::no_recode(), "until": unix_now() - 60 })
    } else {
        json!({ "error": error })
    })
}

/// A build from the sources shows what a player would see (CURSED_DEV_LICENSE): `ending[:seconds]` the subscription
/// ends in so many seconds (29:30), `offline[:s]` the server gone that long and the countdown on (30 minutes),
/// `kick[:s]`, `ban[:s]`, `taken[:s]` and `ended[:s]` tell the game to go after so many seconds (5).
fn rehearse() {
    let Ok(what) = std::env::var(crate::vault::env_dev_license()) else {
        return;
    };
    let (kind, secs) = what.split_once(':').map_or((what.as_str(), None), |(k, s)| (k, s.parse::<u64>().ok()));
    if kind == crate::vault::ending() {
        subscription(&json!({ "active": true, "forever": false, "until": unix_now() + secs.unwrap_or(29 * 60 + 30) as i64 }), unix_now());
    } else if kind == crate::vault::offline() {
        if let Ok(mut s) = SHARED.lock() {
            s.cut = Some(Instant::now() + secs.map_or(CLOSE_IN, Duration::from_secs));
        }
    } else if kind == crate::vault::kick() || kind == crate::vault::ban() || kind == crate::vault::taken() || kind == crate::vault::ended() {
        std::thread::sleep(Duration::from_secs(secs.unwrap_or(5)));
        let v = if kind == crate::vault::ban() {
            json!({ "error": crate::vault::banned(), "reason": "Проверка стенда", "until": unix_now() + 3 * 86_400 })
        } else if kind == crate::vault::taken() {
            json!({ "error": crate::vault::other_device() })
        } else if kind == crate::vault::ended() {
            json!({ "error": crate::vault::no_recode(), "ended": true })
        } else {
            json!({ "error": crate::vault::in_game() })
        };
        leave(&v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// One game's state is one game's state: the tests that write to it run one at a time.
    static TOGETHER: Mutex<()> = Mutex::new(());

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn only_the_servers_signature_over_this_start_opens_it() {
        let server = SigningKey::from_bytes(&[7; 32]);
        let key = hex(server.verifying_key().as_bytes());
        let (nonce, ticket) = ("ab".repeat(32), "cd".repeat(32));
        let sig = hex(&server.sign(start_message(&nonce, &ticket, 414, 28).as_bytes()).to_bytes());
        assert!(signed(&key, &nonce, &ticket, 414, 28, &sig));
        // another start's words, ticket, account or build: no
        assert!(!signed(&key, &"ef".repeat(32), &ticket, 414, 28, &sig));
        assert!(!signed(&key, &nonce, &"00".repeat(32), 414, 28, &sig));
        assert!(!signed(&key, &nonce, &ticket, 415, 28, &sig));
        assert!(!signed(&key, &nonce, &ticket, 414, 27, &sig));
        // someone else's key, or nothing signed
        let other = SigningKey::from_bytes(&[8; 32]);
        let forged = hex(&other.sign(start_message(&nonce, &ticket, 414, 28).as_bytes()).to_bytes());
        assert!(!signed(&key, &nonce, &ticket, 414, 28, &forged));
        assert!(!signed(&key, &nonce, &ticket, 414, 28, ""));
    }

    #[test]
    fn a_starts_words_are_its_own() {
        let (a, b) = (nonce().expect("random words"), nonce().expect("random words"));
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn this_starts_words_do_not_stay_in_the_environment() {
        let _one = TOGETHER.lock().unwrap_or_else(|e| e.into_inner());
        let words = "s".repeat(64);
        std::env::set_var(crate::vault::env_start(), &words);
        // `no_ticket` is the first thing that looks at them and it is before Java
        let _ = no_ticket();
        assert!(
            std::env::var(crate::vault::env_start()).is_err(),
            "the start's words are still sitting in the environment"
        );
        // `verify` is the only thing that spends them, and it finds them where they were put.
        // Made again here on purpose: the sweep is allowed to take every other copy, including the
        // one this test was holding in `words`.
        assert_eq!(take_start(), "s".repeat(64));
        // and nothing of them is kept about for a second read
        assert_eq!(take_start(), "");
    }

    /// The program takes the words and the library spends them: two copies of this code, two
    /// stashes. What the program took reaches the library once, whole, and is gone from the program.
    #[test]
    fn the_programs_words_reach_the_library_once() {
        let _one = TOGETHER.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(crate::vault::env_start(), "h".repeat(64));
        let _ = no_ticket();
        // a buffer too short is handed nothing, and the words stay for the real one
        let mut short = [0u8; 10];
        assert_eq!(unsafe { hand_start(short.as_mut_ptr(), short.len()) }, 0);
        let mut got = [0u8; 64];
        assert_eq!(unsafe { hand_start(got.as_mut_ptr(), got.len()) }, 64);
        assert_eq!(&got[..], "h".repeat(64).as_bytes());
        // once: the stash is empty after it
        assert_eq!(unsafe { hand_start(got.as_mut_ptr(), got.len()) }, 0);
        // and a program that is not ours (this test's) has no door to ask at: no ticket, not a fall
        assert_eq!(take_start(), "");
    }

    /// The pair's ticket is a different string and gets the same treatment: `no_ticket` never even
    /// copies it (it asks how long it is), and `keep_the_pair_from_java` is what takes it away —
    /// into the stub, or out of the world when there is no stub to give it to. It is never put
    /// back: the environment is the one place a dump reads without any skill at all.
    #[test]
    fn the_pairs_ticket_is_never_copied_for_a_length() {
        let _one = TOGETHER.lock().unwrap_or_else(|e| e.into_inner());
        let words = "p".repeat(64);
        std::env::set_var(crate::vault::env_ticket(), &words);
        // lengths only, and the string is still standing for the stub
        assert_eq!(env_len(crate::vault::env_ticket()), 64);
        assert!(std::env::var(crate::vault::env_ticket()).is_ok(), "a length took the ticket away");
        // and the take is the only thing that removes it — with no stub here it is gone, not kept
        // where a stand could find it and a dump could too
        keep_the_pair_from_java();
        assert!(
            std::env::var(crate::vault::env_ticket()).map_or(true, |v| v.is_empty()),
            "with no stub to give it to, the ticket was put back"
        );
        assert!(env_take(crate::vault::env_ticket()).is_empty(), "the take found it still there");
    }

    /// The value is gone from the block itself before the entry is removed. `SetEnvironmentVariable`
    /// never edits that block — it builds a new one and frees the old — so a value that is only
    /// removed is a value the freed block still has, and a dump of the game after the variable is
    /// «gone» walks straight into it.
    #[test]
    fn the_value_is_out_of_the_block_itself_not_only_out_of_the_names() {
        let _one = TOGETHER.lock().unwrap_or_else(|e| e.into_inner());
        let name = "CURSED_SCRUB_PROBE";
        let words = "s".repeat(64);
        std::env::set_var(name, &words);
        assert_eq!(env_len(name), 64);
        env_scrub(name);
        // the words are gone from the block. Windows still answers with the entry — and with the
        // mask that now stands where the value was, which is the point: a dump of the process
        // reads `********…`, not the words. (The entry's own length is all it has left to report,
        // so `env_len` still says 64.)
        assert!(!std::env::var(name).unwrap_or_default().contains(&words), "the value is still readable");
        // and the block is still whole afterwards: the names that sort after this one must not
        // have gone with it (a zero written into a value is the block's own end mark — a double
        // NUL — and takes `PATH` and `TEMP` down with it)
        assert!(std::env::var("PATH").is_ok(), "the scrub ended the environment block early");
        assert!(std::env::var("TEMP").is_ok(), "the scrub ended the environment block early");
        // and the ordinary take is what finishes the job
        let gone = env_take(name);
        assert_ne!(gone, words.as_bytes(), "the take still had the value to hand over");
        assert!(std::env::var(name).is_err(), "the take left it in the environment");
    }

    /// The take is not the end of it. The CRT keeps a **narrow** copy of the whole environment from
    /// the moment the process starts — the `environ` that `envp` points at — and the Windows
    /// environment API never updates it. A dump of the game reads that copy as easily as the block
    /// itself, so the words are written over wherever else this process still has them.
    #[test]
    fn the_narrow_copy_the_crt_keeps_is_masked_too() {
        let _one = TOGETHER.lock().unwrap_or_else(|e| e.into_inner());
        let name = "CURSED_SCRUB_NARROW";
        let words = "n".repeat(64);
        std::env::set_var(name, &words);
        extern "system" {
            fn GetEnvironmentStringsA() -> *mut u8;
            fn FreeEnvironmentStringsA(block: *mut u8) -> i32;
        }
        // exactly the shape a dumper reads: one narrow block of `NAME=VALUE` strings. Taken and
        // deliberately not freed — that is what a leak of it looks like.
        let block = unsafe { GetEnvironmentStringsA() };
        assert!(!block.is_null());
        let mut n = 0usize;
        unsafe {
            while n < (1 << 20) {
                if *block.add(n) == 0 && *block.add(n + 1) == 0 {
                    n += 2;
                    break;
                }
                n += 1;
            }
        }
        let hay = unsafe { std::slice::from_raw_parts(block, n) };
        assert!(find_from(hay, words.as_bytes(), 0).is_some(), "the narrow copy had nothing to lose");
        let mut taken = env_take(name);
        // The take still has them to hand on — that is the whole point of taking. The words are
        // made again here on purpose: the sweep is allowed to take every other copy, including the
        // one this test was holding in `words`.
        assert_eq!(taken.as_slice(), "n".repeat(64).as_bytes(), "the take lost the words before the hand-over");
        // and the narrow copy no longer does
        assert!(find_from(hay, "n".repeat(64).as_bytes(), 0).is_none(), "the narrow copy still holds them");
        // and the process is still whole
        assert!(std::env::var("PATH").is_ok(), "the sweep ended the environment block early");
        assert!(std::env::var("TEMP").is_ok(), "the sweep ended the environment block early");
        wipe(&mut taken);
        unsafe { FreeEnvironmentStringsA(block) };
    }

    #[test]
    fn the_builds_key_is_a_key() {
        assert!(hex_bytes(key()).and_then(|k| <[u8; 32]>::try_from(k).ok()).and_then(|k| VerifyingKey::from_bytes(&k).ok()).is_some());
    }

    #[test]
    fn every_refusal_has_its_words_in_every_language() {
        let _one = TOGETHER.lock().unwrap_or_else(|e| e.into_inner());
        let errors = ["banned", "in-game", "no-recode", "no-tpm", "other-device", "device-taken", "outdated", "offline", "offline-long", "signed-out", "ticket", ""];
        for code in ["ru_ru", "en_us", "uk_ua", "kk_kz"] {
            crate::menus::lang::set(code);
            for e in errors {
                let s = stop(&json!({ "error": e, "reason": "чит", "until": unix_now() + 86_400 }));
                // a key with no word would come back as itself
                for part in [&s.cap, &s.title, &s.text] {
                    assert!(!part.is_empty() && !part.contains("lic."), "{code} {e}: {part:?}");
                }
                assert!(s.words.iter().any(|w| w.0 == "quit") && s.words.iter().all(|w| !w.1.is_empty() && !w.1.contains("lic.")), "{code} {e}");
            }
            for key in ["lic.ending", "lic.closesin", "lic.extend", "lic.backon", "lic.offline"] {
                assert!(!tr(key).contains("lic."), "{code} {key}");
            }
        }
        crate::menus::lang::set("ru_ru");
        assert!(stop(&json!({ "error": "banned", "reason": "чит", "until": unix_now() + 2 * 86_400 - 10 })).text.contains("через 2 дня"));
        // not had one / had one: the words differ, the site's page is the way in both
        assert_eq!(stop(&json!({ "error": "no-recode" })).title, "Подписки нет");
        assert_eq!(stop(&json!({ "error": "no-recode", "until": 1 })).title, "Подписка закончилась");
        assert_eq!(stop(&json!({ "error": "no-recode" })).words[0].0, "site");
        assert_eq!(stop(&json!({ "error": crate::vault::loader() })).words[0].0, "open-loader");
        crate::menus::lang::set("uk_ua");
        assert!(stop(&json!({ "error": "banned", "until": unix_now() + 5 * 86_400 - 10 })).text.contains("5 днів"));
        assert_eq!(plural(21, "день", "дня", "дней"), "день");
        assert_eq!(plural(12, "день", "дня", "дней"), "дней");
    }

    #[test]
    fn the_last_half_hour_runs_out_and_then_the_game_goes() {
        let _one = TOGETHER.lock().unwrap_or_else(|e| e.into_inner());
        let s = |ends: Option<Instant>, cut: Option<Instant>| {
            let mut sh = SHARED.lock().unwrap();
            sh.leave = None;
            sh.ends = ends;
            sh.cut = cut;
        };
        let t = Instant::now();
        // more than half an hour left: nothing shown
        s(Some(t + Duration::from_secs(31 * 60)), None);
        assert!(matches!(now(), Say::Fine));
        // within it: the time on the screen
        s(Some(t + Duration::from_secs(29 * 60)), None);
        assert!(matches!(now(), Say::Closing { why: Why::Ending, .. }));
        // the server gone too, its countdown sooner: that one is shown
        s(Some(t + Duration::from_secs(29 * 60)), Some(t + Duration::from_secs(10 * 60)));
        assert!(matches!(now(), Say::Closing { why: Why::Offline, .. }));
        // renewed for ever, the server back: nothing
        s(None, None);
        assert!(matches!(now(), Say::Fine));
        // the end come: the game goes, and keeps going
        s(Some(t), None);
        assert!(matches!(now(), Say::Leave { .. }));
        if let Ok(mut sh) = SHARED.lock() {
            sh.ends = None;
        }
        assert!(matches!(now(), Say::Leave { .. }));
        s(None, None);
    }

    /// The state, as a game's own; `heard_at` is what says whether the server knows about this one at all.
    fn state(heard_at: Option<Instant>) {
        let mut sh = SHARED.lock().unwrap();
        sh.leave = None;
        sh.ends = None;
        sh.cut = None;
        sh.verdict = None;
        sh.session = None;
        sh.heard_at = heard_at;
    }

    #[test]
    fn a_game_that_never_started_does_not_play() {
        let _one = TOGETHER.lock().unwrap_or_else(|e| e.into_inner());
        // what a dumper gets by taking out both the ask and the wait: no session, and there is not going to be one
        state(None);
        assert!(matches!(now_keeping(true), Say::Leave { .. }));
        // it stays gone
        assert!(matches!(now_keeping(true), Say::Leave { .. }));
        state(None);
    }

    #[test]
    fn a_build_from_the_sources_plays_as_it_is() {
        let _one = TOGETHER.lock().unwrap_or_else(|e| e.into_inner());
        state(None);
        assert!(matches!(now_keeping(false), Say::Fine));
        state(None);
    }

    #[test]
    fn a_game_the_server_stopped_hearing_from_goes_the_way_one_without_the_net_does() {
        let _one = TOGETHER.lock().unwrap_or_else(|e| e.into_inner());
        // the ask is not an answer: a session that is only said to be there runs out like the net gone
        let asked = Instant::now() - OFFLINE_AFTER - Duration::from_secs(60);
        state(Some(asked));
        // within the first half hour: nothing
        state(Some(Instant::now() - Duration::from_secs(5 * 60)));
        assert!(matches!(now_keeping(true), Say::Fine));
        // past it: the time runs out on the screen, and then the game goes
        state(Some(asked));
        assert!(matches!(now_keeping(true), Say::Closing { why: Why::Offline, .. }));
        state(Some(asked - CLOSE_IN - Duration::from_secs(60)));
        assert!(matches!(now_keeping(true), Say::Leave { .. }));
        // told to go stays told
        assert!(matches!(now_keeping(true), Say::Leave { .. }));
        state(None);
    }

    #[test]
    fn whether_the_server_has_to_keep_this_one_up_is_not_one_flag() {
        // a release is also a build that speaks sealed: flipping what the build says leaves the word in it
        assert!(kept(true, true));
        assert!(kept(true, false));
        assert!(kept(false, true));
        // only a build from the sources that speaks no seal plays on its own
        assert!(!kept(false, false));
    }

    #[test]
    fn the_clock_ends_only_a_game_that_goes() {
        let _one = TOGETHER.lock().unwrap_or_else(|e| e.into_inner());
        let opened = Instant::now();
        let mut sh = Shared { leave: None, ends: None, cut: None, verdict: None, session: None, heard_at: None };
        // never started: from the moment the clock opened
        assert_eq!(going(&sh, opened), Some(opened));
        // just asked: it may still be waiting for the answer
        sh.heard_at = Some(Instant::now());
        assert_eq!(going(&sh, opened), None);
        // its words stopped reaching the server and the two half hours are up
        sh.heard_at = Some(Instant::now() - OFFLINE_AFTER - CLOSE_IN - Duration::from_secs(1));
        assert!(going(&sh, opened).is_some());
        // told to go: from when it was told
        let since = Instant::now() - Duration::from_secs(5);
        sh.leave = Some((stop(&json!({})), since));
        assert_eq!(going(&sh, opened), Some(since));
    }
}
