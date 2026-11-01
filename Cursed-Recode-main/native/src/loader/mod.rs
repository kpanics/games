//! The loader of Cursed Recode: the one program a player downloads from the site (only with a subscription), «Cursed
//! Recode.exe». It signs in through the site (her word, 15.10: a code here, «allow» there; no password typed into it),
//! keeps the game in its folder (picked at the first start) the same as the build the server signed, starts the game
//! with a ticket the game confirms (license.rs), and waits in the tray while the game is open.
//!
//! The logic is Cursed's loader's (api, sync, store, install, game), for Recode's server and build.
//!
//! `recode_loader --headless --token <T> [--play] [--server <address>]` does it without a window (tests: with
//! CURSED_LOADER_HOME, CURSED_LOADER_API and CURSED_LOADER_KEY it runs against the test instance, never her folder);
//! `--headless --link` signs in through the site and prints the session.

pub mod api;
mod app;
pub mod game;
pub mod install;
pub mod stop;
pub mod store;
pub mod sync;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

/// A line in the loader's log (`<home>/loader/loader.log`, the last start's), and on the console when there is one.
pub fn log(text: &str) {
    use std::io::Write;
    eprintln!("[loader] {text}");
    let _ = std::fs::create_dir_all(store::dir());
    let path = store::dir().join("loader.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = writeln!(f, "{at} {text}");
    }
}

fn say(text: &str) {
    println!("{text}");
}

/// This player's own pair — the library the build is sealed as and the sealed game beside it, when the server
/// keeps a master to cut it out of (`stub` of the start): fetched under their pair ticket and put where the game
/// takes it. The pair is good for a whole window's turn (the ticket it is under turns with the window), so while
/// the build is the same and the window has not turned, what is already here is used as it is — the two together
/// are a little over fourteen megabytes, and a start should not ask for them again.
///
/// A build under a seal never starts without one. No master up there to cut from, a pair that would not come,
/// a half of it that would not go in place: every one of those is a game that opens nothing, and is said here
/// instead of being left for the game to find out. How much was written comes back, or None when there was
/// nothing to write (nothing to fetch, or the same pair still here).
fn own_library(token: &str, start: &Value, home: &std::path::Path, m: &sync::Manifest) -> Result<Option<usize>, api::Fail> {
    // where the game will look for it: named by the build's own jvm flags. A build that names none is a build
    // nothing can load, and is refused here rather than started into its own missing-library error.
    let at = game::library(home, m).map_err(|e| api::Fail::Refused {
        status: 0,
        error: "stub".into(),
        body: serde_json::json!({ "detail": e }),
    })?;
    if start["stub"].as_bool() != Some(true) {
        // no master up there to cut from. A build under a seal is then a build nothing can open — the two
        // halves are not among its files and never were (the manifest's `seal` is the word that says so).
        if m.seal {
            return Err(api::Fail::Refused {
                status: 0,
                error: "stub".into(),
                body: serde_json::json!({ "detail": "the server kept no master to cut this build a pair from" }),
            });
        }
        // a build of any other kind brings its own library among its files. When it is not here either, the
        // game would go looking for one that is not there — say so now instead.
        if !at.is_file() {
            return Err(api::Fail::Refused {
                status: 0,
                error: "stub".into(),
                body: serde_json::json!({ "detail": format!("{}: missing", at.display()) }),
            });
        }
        return Ok(None);
    }
    let ticket = start["ticket"].as_str().unwrap_or_default();
    let window = start["pair_window"].as_i64().unwrap_or(-1);
    // whose start this is: a pair is cut for one account, and another account signed in on this computer is
    // never handed it
    let uid = start["uid"].as_i64().unwrap_or(-1);
    // a start of a build that says no window, or no account, is never served off the disk: what is there is not
    // known to be its pair, and a wrong pair is worse than one download
    if window >= 0 && uid >= 0 && game::pair_kept(home, m, window, uid) {
        return Ok(None);
    }
    // asked twice at the most: a start that drops on the network should not cost a player their evening
    let mut asked = api::stub(token, ticket);
    if asked.is_err() {
        std::thread::sleep(std::time::Duration::from_millis(800));
        asked = api::stub(token, ticket);
    }
    let pair = asked.map_err(|f| api::Fail::Refused {
        status: 0,
        error: f.error().into(),
        body: serde_json::json!({ "detail": format!("this start's pair did not come: {}", f.detail()) }),
    })?;
    game::place_pair(home, m, &pair).map_err(|e| api::Fail::Refused {
        status: 0,
        error: "stub".into(),
        body: serde_json::json!({ "detail": e }),
    })?;
    // both halves, or neither: a stub that went in and a seal that did not is half a pair, and half a pair
    // opens nothing. Checked against the two places the game takes them from, not against what was written.
    if !at.is_file() {
        return Err(api::Fail::Refused {
            status: 0,
            error: "stub".into(),
            body: serde_json::json!({ "detail": format!("{}: the pair did not stay in place", at.display()) }),
        });
    }
    if window >= 0 && uid >= 0 {
        game::pair_remember(home, m, window, uid).map_err(|e| api::Fail::Refused {
            status: 0,
            error: "stub".into(),
            body: serde_json::json!({ "detail": e }),
        })?;
    }
    Ok(Some(pair.stub.len() + pair.bin.len()))
}

/// Without a window (tests): see the module.
pub fn headless(args: &[String]) -> i32 {
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let home = store::home();
    let _ = std::fs::create_dir_all(store::dir());
    let device = crate::device::parts_json();
    say(&format!("home {}, api {}, device parts {}", home.display(), api::base(), device.as_object().map_or(0, |o| o.len())));
    let token = if args.iter().any(|a| a == "--link") {
        match link_headless(&device) {
            Some(t) => t,
            None => return 1,
        }
    } else {
        get("--token").unwrap_or_default()
    };
    let started = match api::start(&token, &device) {
        Ok(v) => v,
        Err(f) => {
            say(&format!("start: {} {}", f.error(), f.body()));
            return 1;
        }
    };
    let login = started["login"].as_str().unwrap_or("Player").to_string();
    // tests: the session kept as a sign-in would keep it (the window's loader starts signed in)
    if args.iter().any(|a| a == "--remember") {
        store::Session { token: token.clone(), uid: started["uid"].as_i64().unwrap_or(0), login: login.clone() }.save();
        say("the session is kept");
        if !args.iter().any(|a| a == "--play" || a == "--sync") {
            return 0;
        }
    }
    let manifest = match sync::verify(started["manifest"].as_str().unwrap_or(""), started["sig"].as_str().unwrap_or("")) {
        Ok(m) => m,
        Err(e) => {
            say(&format!("manifest: {e}"));
            return 1;
        }
    };
    say(&format!("{login}: build {} ({}), {} files", manifest.build, manifest.version, manifest.files.len()));
    let progress = sync::shared();
    let t0 = std::time::Instant::now();
    let watcher = {
        let p = progress.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            let (done, total) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed));
            say(&format!(
                "  checked {}/{}  files {}/{}  {:.1}/{:.1} MB",
                p.checked.load(Ordering::Relaxed),
                p.to_check.load(Ordering::Relaxed),
                p.files_done.load(Ordering::Relaxed),
                p.files_total.load(Ordering::Relaxed),
                done as f64 / 1048576.0,
                total as f64 / 1048576.0
            ));
            if p.cancel.load(Ordering::Relaxed) {
                break;
            }
        })
    };
    let renew = || api::start(&token, &device).is_ok();
    let result = sync::run(&home, &manifest, &token, &renew, &progress);
    progress.cancel.store(true, Ordering::Relaxed);
    let _ = watcher.join();
    match result {
        Ok(()) => say(&format!("the build is in place ({:.1} MB came in {:.1} s)", progress.done.load(Ordering::Relaxed) as f64 / 1048576.0, t0.elapsed().as_secs_f64())),
        Err(e) => {
            say(&format!("sync: {e}"));
            return 1;
        }
    }
    if let Some(entry) = sync::loader_update(&manifest) {
        say(&format!("a newer loader is in the build ({})", entry.sha256));
    }
    if args.iter().any(|a| a == "--play") {
        let fresh = match api::start(&token, &device) {
            Ok(v) => v,
            Err(f) => {
                say(&format!("start: {}", f.error()));
                return 1;
            }
        };
        match own_library(&token, &fresh, &home, &manifest) {
            Ok(Some(n)) => say(&format!("this start's library is in place ({n} bytes)")),
            Ok(None) => {}
            Err(f) => {
                let more = f.body()["detail"].as_str().map(str::to_string).unwrap_or_else(|| f.detail());
                say(&format!("stub: {} {more}", f.error()));
                return 1;
            }
        }
        let settings = store::Settings::load();
        let server = get("--server");
        // the pair ticket every start hands the game: the note of a hand on its memory is signed
        // with this same one (TOUCH.md §3)
        let pair_ticket = fresh["ticket"].as_str().unwrap_or("");
        match game::start(&home, &manifest, settings.memory, &login, pair_ticket, fresh["start_ticket"].as_str().unwrap_or(""), None, server.as_deref()) {
            Ok(mut running) => {
                say(&format!("the game started, pid {}", running.child.id()));
                let (code, report, source) = game::until_it_goes(&mut running, &home, &manifest, Some(&token), Some(pair_ticket));
                say(&format!("the game went, exit {code:?}"));
                if let Some(report) = report {
                    // a touch is its own kind of note and travels under its MAC; anything else is
                    // the old crash path, untouched
                    let sent = if source == crate::vault::touch_source() {
                        api::touch(&token, manifest.build, pair_ticket, report.as_bytes())
                    } else {
                        api::crash(&token, manifest.build, source, report.as_bytes())
                    };
                    say(&format!("{source}: {} bytes, sent: {:?}", report.len(), sent));
                }
            }
            Err(e) => {
                say(&format!("play: {e}"));
                return 1;
            }
        }
    }
    let _ = Arc::strong_count(&progress);
    0
}

/// Signs in through the site without a window: the code and the page printed, the answer waited for (5 minutes).
fn link_headless(device: &serde_json::Value) -> Option<String> {
    let asked = match api::link(device) {
        Ok(v) => v,
        Err(f) => {
            say(&format!("link: {}", f.error()));
            return None;
        }
    };
    let (code, key) = (asked["code"].as_str().unwrap_or(""), asked["key"].as_str().unwrap_or(""));
    say(&format!("link: code {code}, open {}", asked["url"].as_str().unwrap_or("")));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(300);
    while std::time::Instant::now() < until {
        match api::link_poll(code, key) {
            Ok(Some(v)) => {
                say(&format!("signed in as {} (uid {})", v["login"], v["uid"]));
                return v["token"].as_str().map(str::to_string);
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_secs(2)),
            Err(f) => {
                say(&format!("link: {}", f.error()));
                return None;
            }
        }
    }
    say("link: no answer");
    None
}

/// The loader with its window (app.rs): one at a time (a second start brings the first one's window up).
pub fn main() -> i32 {
    install::cleanup();
    // started by the loader it has just replaced: that one may still be closing
    let patient = std::env::args().any(|a| a == "--updated") || install::old_in_use();
    if another_is_running(patient) {
        return 0;
    }
    // a folder picked already: the loader runs from its place in it (the first start copies it there)
    if store::chosen().is_some() && install::ensure_installed() {
        return 0;
    }
    app::run()
}

/// Whether another loader runs (its window brought up then). `patient`: the one before may still be closing.
fn another_is_running(patient: bool) -> bool {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name: Vec<u16> = "Local\\CursedRecodeLoader".encode_utf16().chain(std::iter::once(0)).collect();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(if patient { 10 } else { 0 });
    loop {
        let h = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if !h.is_null() && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
            // kept for the process's life
            return false;
        }
        if !h.is_null() {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(h) };
        }
        if std::time::Instant::now() >= until {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let class: Vec<u16> = "CursedRecodeLoader".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let hwnd = FindWindowW(class.as_ptr(), std::ptr::null());
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    /// The door the player opens is the one Windows asks for administrator at, and without it that
    /// program does not start at all. Kept here so it cannot be quietly taken out again.
    #[test]
    fn the_loader_asks_for_administrator_at_the_door() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("app").join("app.manifest");
        let text = std::fs::read_to_string(&manifest).unwrap_or_else(|e| panic!("{}: {e}", manifest.display()));
        assert!(text.contains("requestedExecutionLevel"), "{text}");
        assert!(text.contains("requireAdministrator"), "{text}");
    }

    /// Only the loader asks. The game it begins is a child of it and comes in under that door — if
    /// the game asked too, every stand and every second start would bring another prompt.
    #[test]
    fn only_the_loader_carries_the_ask() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("app");
        let rc = std::fs::read_to_string(dir.join("app.rc")).expect("app.rc");
        assert!(!rc.contains("app.manifest"), "the ask is not a shared resource: {rc}");
        let build = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs")).expect("build.rs");
        assert!(build.contains("rustc-link-arg-bin=recode_loader"), "{build}");
    }
}
