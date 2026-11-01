//! Starting the game the way the build says (`launch` of the manifest), with the loader's two tickets for the game:
//! the pair's, under which its own copy is sealed, and a start's own, which it spends on confirming this one (license.rs).
//! And watching it: when it goes, whether it crashed (a crash report written after it started, the java
//! runtime's hs_err file, or a fatal status from Windows), and what to send about it. Cursed's loader's, for Recode.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::SystemTime;

use windows_sys::Win32::Foundation::CloseHandle;

use super::sync::Manifest;

pub struct Running {
    pub child: Child,
    pub started: SystemTime,
}

/// `${root}` in the manifest is the client's folder.
fn fill(text: &str, root: &str) -> String {
    text.replace("${root}", root)
}

fn local(home: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(home.to_path_buf(), |p, part| p.join(part))
}

/// The library the build is sealed as: the file its jvm flags name (`-Dcursed.recode.native=`, the one the game
/// loads). It is the stub of this player's pair — the one that opens their own sealed game (sealed_bin) — and
/// both are cut into this very place (place_pair), never a file of the manifest.
pub fn library(home: &Path, m: &Manifest) -> Result<PathBuf, String> {
    let root = home.to_string_lossy().to_string();
    for a in m.launch["jvm"].as_array().into_iter().flatten().filter_map(|a| a.as_str()) {
        if let Some(p) = a.strip_prefix("-Dcursed.recode.native=") {
            return Ok(PathBuf::from(fill(p, &root)));
        }
    }
    Err("the build names no sealed library".into())
}

/// The sealed game of this player's pair: beside the library that opens it, under the same name with `.bin`.
/// The two come from one cut on the server (api::Pair) and go in place together (place_pair) — a stub that
/// opens nothing beside it is half a pair.
pub fn sealed_bin(home: &Path, m: &Manifest) -> Result<PathBuf, String> {
    let mut at = library(home, m)?;
    at.set_extension("bin");
    Ok(at)
}

/// This player's own library in the place the game loads it from (library): the master's stub with one half
/// rewritten under their pair ticket. Written beside it first and moved into place — a library that is half
/// written is never in its place, and one already in use is moved aside the way the build's own files are (sync).
pub fn place_stub(home: &Path, m: &Manifest, bytes: &[u8]) -> Result<(), String> {
    place_at(&library(home, m)?, bytes, "the library")
}

/// This player's own pair in the two places the game takes them from: the stub into the library's place
/// (place_stub) and the sealed game beside it. One cut, one frame, one pair — neither half is ever put down
/// alone.
pub fn place_pair(home: &Path, m: &Manifest, pair: &super::api::Pair) -> Result<(), String> {
    place_stub(home, m, &pair.stub)?;
    place_at(&sealed_bin(home, m)?, &pair.bin, "the sealed game")
}

/// One file of a player's own pair in its place: written beside it first and moved into place (put_in_place).
fn place_at(full: &Path, bytes: &[u8], what: &str) -> Result<(), String> {
    let dir = full.parent().ok_or("the library's folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("the library's folder: {e}"))?;
    let name = full.file_name().map_or_else(|| "stub".to_string(), |n| n.to_string_lossy().to_string());
    let part = dir.join(format!(".{name}.part"));
    std::fs::write(&part, bytes).map_err(|e| format!("{what}: {e}"))?;
    super::sync::put_in_place(&part, &full)
}

/// Where the pair that is on disk is remembered: beside the library, which is where it was put. The marker is
/// no secret — it says only which build's pair this is and which window's it was cut under.
fn pair_marker(home: &Path, m: &Manifest) -> Result<PathBuf, String> {
    let full = library(home, m)?;
    let dir = full.parent().ok_or("the library's folder")?;
    let name = full.file_name().map_or_else(|| "stub".to_string(), |n| n.to_string_lossy().to_string());
    Ok(dir.join(format!(".{name}.pair")))
}

/// Whether this player's own pair is already here and is still the one to use: both files in place, and the
/// marker naming this build, this window and this account. A pair is good for as long as its window turns, so
/// within one window the same two files are not asked for again — they are a little over fourteen megabytes
/// together. Half of a pair, a marker of another build, another window or another account (`uid`: the pair is cut
/// for one account and opens only under its ticket — 19.10, hers kept and handed to a test account's start ended in
/// «unsupported JNI version»), or no marker at all: none of that is one.
pub fn pair_kept(home: &Path, m: &Manifest, window: i64, uid: i64) -> bool {
    let (Ok(lib), Ok(bin), Ok(at)) = (library(home, m), sealed_bin(home, m), pair_marker(home, m)) else {
        return false;
    };
    if !lib.is_file() || !bin.is_file() {
        return false;
    }
    std::fs::read_to_string(&at).is_ok_and(|text| text.trim() == format!("{} {} {}", m.build, window, uid))
}

/// Remembers the pair on disk as this build's, cut under this window for this account (pair_kept). Written after
/// both files are in place, so a pair that is still coming is never remembered as one that is here.
pub fn pair_remember(home: &Path, m: &Manifest, window: i64, uid: i64) -> Result<(), String> {
    let at = pair_marker(home, m)?;
    let dir = at.parent().ok_or("the library's folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("the library's folder: {e}"))?;
    std::fs::write(&at, format!("{} {} {}", m.build, window, uid)).map_err(|e| format!("the pair's marker: {e}"))
}

/// The command that starts this build: the game's own program (`host`, CursedRecode.exe: Java runs inside it) with
/// the Java the build brings. Both tickets go in the environment, never on the command line (any program may read
/// command lines): `pair_ticket`, the one its own copy is sealed under and which it opens the game with, and
/// `start_ticket`, this one start's own, which it spends on confirming itself (license.rs) and which nothing else
/// will take. So does the name of the window before the game this loader keeps (`twin`): the game takes over
/// from it. `server`: straight into it (the menu's «Continue»).
#[allow(clippy::too_many_arguments)]
pub fn command(home: &Path, m: &Manifest, memory: u32, login: &str, pair_ticket: &str, start_ticket: &str, twin: Option<&str>, server: Option<&str>) -> Result<Command, String> {
    let launch = &m.launch;
    let root = home.to_string_lossy().to_string();
    let host = local(home, launch["host"].as_str().ok_or("launch")?);
    if !host.is_file() {
        return Err(format!("{}: missing", host.display()));
    }
    let least = launch["memory"]["least"].as_u64().unwrap_or(2048) as u32;
    let mut cmd = Command::new(&host);
    cmd.arg("--jdk").arg(local(home, launch["jdk"].as_str().ok_or("launch")?));
    cmd.arg(format!("-Xmx{}m", memory.max(least)));
    for a in launch["jvm"].as_array().into_iter().flatten().filter_map(|a| a.as_str()) {
        cmd.arg(fill(a, &root));
    }
    let classpath: Vec<String> = launch["classpath"].as_array().ok_or("launch")?.iter().filter_map(|p| p.as_str()).map(|p| local(home, p).to_string_lossy().to_string()).collect();
    cmd.arg("-cp").arg(classpath.join(";"));
    cmd.arg(launch["main"].as_str().ok_or("launch")?);
    for a in launch["game"].as_array().into_iter().flatten().filter_map(|a| a.as_str()) {
        cmd.arg(fill(a, &root));
    }
    cmd.arg("--username").arg(login);
    if let Some(s) = server.filter(|s| !s.is_empty()) {
        cmd.arg("--quickPlayMultiplayer").arg(s);
    }
    cmd.env(crate::vault::env_ticket(), pair_ticket)
        .env(crate::vault::env_start(), start_ticket)
        .env(crate::vault::env_build(), m.build.to_string());
    match twin {
        Some(name) => cmd.env("CURSED_TWIN", name),
        None => cmd.env_remove("CURSED_TWIN"),
    };
    // the launch-time environments an assistant would put a Java agent in: taken out before the
    // game ever sees them. A defence, not a detection — no ban comes of it and no player is told.
    // `env_remove` is not enough on its own: a user who set one system-wide would otherwise hand
    // it to the game anyway, so the child is given the line without its agent flags, or nothing.
    for key in ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS"] {
        if let Ok(value) = std::env::var(key) {
            let scrubbed = without_agent_flags(&value);
            if scrubbed.is_empty() {
                cmd.env_remove(key);
            } else {
                cmd.env(key, scrubbed);
            }
        }
    }
    cmd.current_dir(home);
    Ok(cmd)
}

/// The language picked in the loader is the game's too: the first time the game starts from it, its options get that
/// language (made if there are none yet). Answered once (cursed-recode/language-asked), the game's own options rule
/// after that (the loader never changes them again).
pub fn settle_language(home: &Path, lang: &str) {
    let asked = home.join("cursed-recode").join("language-asked");
    if asked.is_file() {
        return;
    }
    let code = if lang == "ru" { "ru_ru" } else { "en_us" };
    let options = home.join("options.txt");
    let text = std::fs::read_to_string(&options).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().filter(|l| !l.starts_with("lang:")).map(str::to_string).collect();
    lines.push(format!("lang:{code}"));
    let ok = std::fs::write(&options, lines.join("
") + "
").is_ok();
    if ok {
        let _ = std::fs::create_dir_all(home.join("cursed-recode"));
        let _ = std::fs::write(&asked, code);
    }
}

/// The server joined last (the game's «Continue», cursed.recode.launch.LastServer): its name and address.
pub fn last_server(home: &Path) -> Option<(String, String)> {
    let text = std::fs::read_to_string(home.join("cursed-recode").join("last-server")).ok()?;
    let mut lines = text.lines();
    let name = lines.next()?.trim().to_string();
    let address = lines.next()?.trim().to_string();
    (!address.is_empty()).then_some((name, address))
}

#[allow(clippy::too_many_arguments)]
pub fn start(home: &Path, m: &Manifest, memory: u32, login: &str, pair_ticket: &str, start_ticket: &str, twin: Option<&str>, server: Option<&str>) -> Result<Running, String> {
    settle_language(home, &super::store::Settings::load().lang);
    let mut cmd = command(home, m, memory, login, pair_ticket, start_ticket, twin, server)?;
    let started = SystemTime::now();
    let child = cmd.spawn().map_err(|e| format!("the game: {e}"))?;
    Ok(Running { child, started })
}

/// Windows reports a crash as an NT status (0xC0000005 and the like).
pub fn fatal_status(code: i32) -> bool {
    (0xC000_0000..0xFFFF_FF00).contains(&(code as u32))
}

/// A crash report the game wrote after `since`, the newest.
fn newest_report(home: &Path, since: SystemTime) -> Option<PathBuf> {
    let dir = home.join("crash-reports");
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            let t = m.modified().ok()?;
            (t >= since && e.path().extension().is_some_and(|x| x == "txt")).then(|| (t, e.path()))
        })
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

/// Whether the game had begun its own way out when it went: its log ends in its shutdown ("Stopping!" and what
/// follows it), with no crash report written. A fall after that is at the door: everything was saved.
pub fn at_the_door(home: &Path, since: SystemTime) -> bool {
    if newest_report(home, since).is_some() {
        return false;
    }
    let Ok(log) = std::fs::read(home.join("logs").join("latest.log")) else { return false };
    let tail = &log[log.len().saturating_sub(64 * 1024)..];
    let text = String::from_utf8_lossy(tail);
    text.lines().rev().take(400).any(|l| l.contains("/INFO]: Stopping!"))
}

/// What Windows wrote of the process's fall (Application Error, event 1000): the module it blames and where, in English
/// (in the system's language its words came out as question marks once sent). Empty when it wrote nothing (or the log
/// cannot be read).
fn fault_of(pid: u32) -> String {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("wevtutil")
        .args(["qe", "Application", "/c:8", "/rd:true", "/f:text", "/q:*[System[Provider[@Name='Application Error'] and (EventID=1000)]]", "/l:en-US"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Ok(out) = out else { return String::new() };
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let wanted = format!("0x{pid:x}");
    for event in text.split("Event[").skip(1) {
        if event.to_lowercase().contains(&wanted) {
            return event.lines().map(str::trim).filter(|l| !l.is_empty()).take(24).collect::<Vec<_>>().join("\n");
        }
    }
    String::new()
}

/// What happened to the game, once it has gone: None for a normal exit, the report to send for a crash.
pub fn crash_report(home: &Path, pid: u32, code: Option<i32>, since: SystemTime, m: &Manifest) -> Option<String> {
    let report = newest_report(home, since);
    let hs_err = Some(home.join(format!("hs_err_pid{pid}.log"))).filter(|p| p.is_file());
    let fatal = code.is_some_and(fatal_status);
    if report.is_none() && hs_err.is_none() && !fatal {
        return None;
    }
    let mut out = format!(
        "Cursed Recode {} (build {})\nexit: {}\n",
        m.version,
        m.build,
        code.map_or("none".to_string(), |c| format!("{c} (0x{:08X})", c as u32))
    );
    if fatal {
        let fault = fault_of(pid);
        if !fault.is_empty() {
            out.push_str(&format!("\n===== Windows: Application Error =====\n{fault}\n"));
        }
    }
    let mut take = |label: &str, path: &Path, most: usize, tail: bool| {
        if let Ok(bytes) = std::fs::read(path) {
            let slice = if bytes.len() > most { if tail { &bytes[bytes.len() - most..] } else { &bytes[..most] } } else { &bytes[..] };
            out.push_str(&format!("\n===== {label} ({}) =====\n", path.file_name().unwrap_or_default().to_string_lossy()));
            out.push_str(&String::from_utf8_lossy(slice));
        }
    };
    if let Some(p) = &report {
        take("crash report", p, 900 * 1024, false);
    }
    if let Some(p) = &hs_err {
        take("java runtime", p, 600 * 1024, false);
    }
    take("latest.log, the end", &home.join("logs").join("latest.log"), 300 * 1024, true);
    Some(out)
}

// ------------------------------------------------------------------------------------------------ who else is here

/// The exit code the game's own library leaves when it closes the game (`protect-stub`'s `guard::EXIT`, "CURS").
/// Not an NT status, so it is not a crash of the game's own — it is something found at this machine.
const GUARD_EXIT: i32 = 0x4355_5253;

/// How often the machine is looked over again while the game runs. A dumper is usually opened after
/// the game is already up — the look before it starts alone would miss every one of those.
const EVERY: std::time::Duration = std::time::Duration::from_secs(2);

/// How often the room is looked at for her list while the game runs. Report only: the player is
/// never touched and never told (her word, 17.10). Slower than the look above — this one writes a
/// line down and nothing else.
const LOOK: std::time::Duration = std::time::Duration::from_secs(60);

/// And the first of those a little after the game is up, so what it sees is the room the game is
/// actually running in and not the moment of the start itself.
const LOOK_SOON: std::time::Duration = std::time::Duration::from_secs(10);

/// The tools a copy is taken apart with. **Kept in step by hand with `protect-stub`'s `watch.rs`** —
/// that one looks over the machine as the game starts, this one keeps looking while it runs, and the
/// two must not disagree about what a dumper is called. A name is matched against the process's own
/// name and, for the handful of interpreters below, its command line: an assistant a package manager
/// put under `node.exe` is only visible there.
const TOOLS: &[&str] = &[
    // debuggers
    "x64dbg", "x32dbg", "x96dbg", "ollydbg", "windbg", "windbgx", "ntsd", "cdb.exe", "kd.exe", "dbgview",
    "immunitydebugger", "immdbg",
    // dumpers and rebuilders
    "scylla", "extremedumper", "megadumper", "lordpe", "importrec", "imprec", "pe-sieve", "hollowshunter",
    "hollows_hunter", "processdump", "pmemdump", "memdump",
    // memory of a process, read or written
    "processhacker", "systeminformer", "processhacker2", "procexp", "procexp64", "processmonitor", "procmon",
    "procmon64", "cheatengine", "ceserver", "reclass", "artmoney",
    // taken apart from the file
    "ida.exe", "ida64", "ida32", "idaq", "idaq64", "ghidra", "ghidrarun", "analyzeheadless", "cutter",
    "radare2", "binaryninja", "binary ninja", "hiew", "hiew32", "hopper", "die.exe", "diec", "diec.exe",
    "pestudio", "pe-bear", "pebear", "cff explorer", "explorer suite", "retdec",
    // the Java a client is written in
    "jmap", "jcmd", "jstack", "jconsole", "jvisualvm", "jmc.exe", "jd-gui", "jadx", "jadx-gui", "recaf",
    "bytecodeviewer", "bytecode viewer", "cfr", "procyon", "fernflower", "krakatau",
    // the .NET the launcher and the tools around it are written in
    "dnspy", "ilspycmd", "de4dot", "x64netdumper",
];

/// The assistants that write the teardown. **Kept in step by hand with `protect-stub`'s `watch.rs`.**
const AGENTS: &[&str] = &[
    "claude", "codex", "chatgpt", "openai", "copilot", "cursor", "windsurf", "gemini", "grok", "openrouter",
    "anthropic", "aider", "opencode", "amp.exe", "trae", "qwen", "kilo", "continue", "cline", "roo",
];

/// The shells a player types into. The walk up the parents stops at the first of these: above it
/// sits what the player was doing before the command, not whoever is driving the game.
const SHELLS: &[&str] = &[
    "explorer", "cmd", "powershell", "powershell_ise", "pwsh", "windowsterminal", "wt", "bash", "sh", "zsh",
    "conhost", "openconsole", "far", "totalcmd", "totalcmd64", "files", "directoryopus",
];

/// The processes worth asking for a command line. Everything else is known by its name alone: an
/// assistant is either named after itself or sitting inside one of these, and reading the command
/// line of every process on the machine every two seconds is work a player's computer should not do.
const WORTH_A_LOOK: &[&str] = &[
    "node", "deno", "bun", "python", "pythonw", "python3", "py", "powershell", "pwsh", "cmd", "bash", "sh",
    "java", "javaw", "ruby", "perl", "php", "wsl", "wt", "windowsterminal", "conhost", "openconsole", "code",
    "cursor", "idea64", "pycharm64", "eclipse", "devenv",
];

/// One process as the snapshot has it.
struct Proc {
    pid: u32,
    ppid: u32,
    /// The image's own name, lowercase.
    name: String,
    /// Its command line, lowercase — empty unless it was worth asking for.
    line: String,
}

/// What was found at the machine, as it goes to the server.
pub struct Found {
    pub body: String,
}

/// Wait for the game to go, looking over the machine as it runs, and say what happened: its exit
/// code, the report to send and whose report it is — `"game"` for a crash of its own, `"protect"`
/// for something found at this machine.
///
/// The look over the machine is here and not in the library the game loads: this is the process
/// that stays up the whole time and has the door to the server. Something found closes the game
/// from here and the note says what was seen. With a `token`, the room is also looked at for her
/// list every so often (`a_look`) — that one never closes anything and never reaches the player.
pub fn until_it_goes(running: &mut Running, home: &Path, m: &Manifest, token: Option<&str>, ticket: Option<&str>) -> (Option<i32>, Option<String>, &'static str) {
    // the pair ticket the caller signs a touch report with (TOUCH.md §3). The body is built
    // here; `api::touch` does the signing and has that same ticket.
    let _ = ticket;
    let pid = running.child.id();
    let mut found: Option<Found> = None;
    let mut touch: Option<Found> = None;
    let mut next_look = std::time::Instant::now() + LOOK_SOON;
    let mut baseline: Option<MemBase> = None;
    let code = loop {
        match running.child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {}
            Err(_) => break running.child.wait().ok().and_then(|s| s.code()),
        }
        let now = std::time::Instant::now();
        if now >= next_look {
            next_look = now + LOOK;
            // a look at the room, written down for her list: report only. The game keeps running
            // and the player is never told — only the staff panel gets a line. Sent on its own
            // thread so a slow server never stands between this loop and the game going.
            if let Some(token) = token {
                let (score, sights) = a_look(home, pid, running.started);
                if !sights.is_empty() {
                    let body = serde_json::Value::Array(
                        sights.iter().map(|s| serde_json::json!({ "kind": s.kind, "name": s.name, "note": s.note })).collect(),
                    );
                    let token = token.to_string();
                    let build = m.build;
                    std::thread::spawn(move || {
                        let _ = super::api::sight(&token, build, score, &body);
                    });
                }
            }
            // a hand on the memory: this one closes the game and is a touch. The baseline is
            // taken here and never again — Java is still loading natives at the door.
            let base = baseline.get_or_insert_with(|| MemBase::take(pid, home, m));
            if let Some(f) = look_at_memory(pid, running.started, base) {
                touch = Some(f);
                let _ = running.child.kill();
                let _ = running.child.wait();
                break None;
            }
        }
        if let Some(f) = look_over_the_machine(pid, running.started, m) {
            found = Some(f);
            let _ = running.child.kill();
            let _ = running.child.wait();
            break None;
        }
        std::thread::sleep(EVERY);
    };

    // A note already on disk that carries a `touch: ` line is never replaced: the stub was closer
    // to the crime, and a stub's `touch: hook` overwritten by this end's "a tool was open" is a
    // ban that never happens. The source is decided by the note that actually travels.
    let note = the_note(home, m);
    if let Some(body) = &note {
        if source_of(body) == crate::vault::touch_source() {
            return (code, Some(body.clone()), crate::vault::touch_source());
        }
    }
    if let Some(f) = touch {
        return (code, Some(f.body), crate::vault::touch_source());
    }
    if let Some(f) = found {
        // Whose report this is is decided by the body that travels, not by who found it: a
        // poke-named tool that arrived while the game ran came back as a `touch:` and is a ban,
        // and the old path's own report — the tree of parents — is the lesser note it replaces.
        let source = source_of(&f.body);
        return (code, Some(f.body), source);
    }
    if code == Some(GUARD_EXIT) {
        let body = note.unwrap_or_else(|| {
            format!(
                "Cursed Recode — the game was closed\nbuild: {}\nwhy: the game's own guard closed it, with no note of its own\n",
                m.build
            )
        });
        let source = source_of(&body);
        return (code, Some(body), source);
    }
    (code, crash_report(home, pid, code, running.started, m), "game")
}

/// The note the game's library left beside itself, taken and read once. A note from an earlier run
/// is of no use and is gone with this one. The name is the vault's, the same word the stub writes
/// it under (`note_name`) — two ends naming one file separately is a note nothing reads. **A note
/// that carries a `touch: ` line is never replaced** by anything this end finds: the stub was
/// closer to the crime.
fn the_note(home: &Path, m: &Manifest) -> Option<String> {
    let path = home.join("bin").join(crate::vault::note_name());
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    if text.trim().is_empty() {
        return None;
    }
    Some(format!("Cursed Recode {version} (build {build})\n{text}", version = m.version, build = m.build))
}

/// A look over the machine: what can only be here to take the game apart, and who started it.
/// `None`: nothing worth closing the game over. This is the **two-second** pass — the memory look
/// that walks modules and pages is the one that waits a minute, and a hand that arrives and goes
/// inside that minute must still be a ban (her word, 18.10: «выдавать бан должно если в память
/// лезут или пытаются»). The names and the command lines are already in the snapshot this takes;
/// asking them here costs nothing the loop was not already paying.
fn look_over_the_machine(game_pid: u32, since: SystemTime, m: &Manifest) -> Option<Found> {
    let procs = snapshot();
    if procs.is_empty() {
        return None;
    }
    judge(&procs, game_pid, since, &m.version, m.build)
}

/// The same decision over a list of processes already taken — the whole of the question, without
/// the machine under it, so it can be asked in a test.
fn judge(procs: &[Proc], game_pid: u32, since: SystemTime, version: &str, build: i64) -> Option<Found> {
    let chain = chain_of(procs, game_pid);

    // A hand on our memory, named for what it does to a process's address space, that arrived
    // while the game was up. This is a `touch` and it is a ban at once — it is not "a tool was
    // running" and it is not two strikes. The line is the stub's own: the **file's own name**
    // carries a memory-poke word (`touch_pokes`), the process started after we did, and our name
    // is never the cause, and an ordinary noun in a longer name is nobody's hand (see `poke_in`).
    // A tool that was already on the machine when the game came up is presence, and presence keeps
    // the old path below.
    for p in procs {
        if p.pid == game_pid {
            continue;
        }
        // The name first. `started_after` opens the process, and there is no reason to ask it of
        // `explorer.exe` — only a name that already carries a poke word is worth the question.
        let Some((kind, why)) = a_hand_on_us(&p.name, &p.line) else {
            continue;
        };
        // A clock that will not answer is the aggressive side here (TOUCH.md §5) — and only
        // here: this pass holds no baseline, and the name already carries a poke word.
        if started_after(p.pid, since).unwrap_or(true) {
            return Some(Found { body: touch_body(version, build, kind, &why) });
        }
    }

    // An attacher aimed at this pid. Release only: her IntelliJ attaches to the game every day,
    // and a ban over that would ban her. Asked here rather than only in the minute's look so the
    // hand is caught while it is still reaching.
    if crate::license::RELEASE {
        for p in procs {
            if p.pid == game_pid {
                continue;
            }
            if let Some(what) = attacher(&p.line, game_pid) {
                return Some(Found {
                    body: touch_body(
                        version,
                        build,
                        KIND_INJECT,
                        &format!("{what} (pid {}) was aimed at the game", p.pid),
                    ),
                });
            }
        }
    }

    for p in procs {
        if let Some(what) = matches(p, TOOLS) {
            return Some(Found {
                body: report(
                    procs,
                    &chain,
                    game_pid,
                    &format!("a tool that takes a process apart was running: {what} (pid {})", p.pid),
                    &[],
                    Some(p),
                ),
            });
        }
    }
    for p in &chain {
        if let Some(what) = matches(p, AGENTS) {
            return Some(Found {
                body: report(
                    procs,
                    &chain,
                    game_pid,
                    &format!("the game was started from an assistant: {what} (pid {})", p.pid),
                    &[],
                    Some(p),
                ),
            });
        }
    }
    // an assistant elsewhere is worth a line in the day's papers and nothing more: a chat window on
    // the second monitor is not reading the game's memory. The look-over is repeated while the game
    // runs, so this would otherwise be a constant stream of the same note — it is only said once,
    // and only when the game is closed over something else does the whole list go with it.
    None
}

/// Whether this process is one of `list`, and under what. The name and the command line both count.
fn matches<'a>(p: &Proc, list: &[&'a str]) -> Option<&'a str> {
    let name = p.name.trim_end_matches(".exe");
    let stem = name.rsplit(['/', '\\']).next().unwrap_or(name);
    for want in list {
        let want = want.trim_end_matches(".exe");
        if named(stem, want) {
            return Some(want);
        }
        if !p.line.is_empty() && line_names(&p.line, want) {
            return Some(want);
        }
    }
    None
}

/// Whether a name is this program: itself, or one put in a folder of its own next to another name.
fn named(stem: &str, want: &str) -> bool {
    stem == want || stem.starts_with(&format!("{want}-")) || stem.ends_with(&format!("-{want}"))
}

/// Whether the command line names this program: as the file it runs, or as the folder that file
/// sits in — `node …\claude\cli.js` is what an assistant a package manager installed looks like.
/// Any deeper part of the path is only a folder the player happened to call that: a game under
/// `C:\Users\claude\…` is nobody's assistant and must not close anything.
fn line_names(line: &str, want: &str) -> bool {
    for token in line.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
        let token = token.trim_matches(|c: char| matches!(c, ',' | ';' | '(' | ')' | '='));
        if token.is_empty() {
            continue;
        }
        // the file and the folder it is in, nearest first — nothing above them
        let parts: Vec<&str> = token.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
        for part in parts.iter().rev().take(2) {
            // `cheatengine.py`, `x64dbg.exe`, `claude` — the name before whatever it is called
            let stem = part.split('.').next().unwrap_or(part);
            if named(stem, want) {
                return true;
            }
        }
    }
    false
}

/// This process and every parent up to the player's own shell, nearest first. The shell itself is
/// not in the list.
fn chain_of(procs: &[Proc], me: u32) -> Vec<Proc> {
    let mut out = Vec::new();
    let mut pid = me;
    for _ in 0..16 {
        let Some(p) = procs.iter().find(|p| p.pid == pid) else { break };
        if is_shell(&p.name) {
            break;
        }
        out.push(Proc { pid: p.pid, ppid: p.ppid, name: p.name.clone(), line: p.line.clone() });
        if p.ppid == 0 || p.ppid == pid {
            break;
        }
        pid = p.ppid;
    }
    out
}

fn is_shell(name: &str) -> bool {
    let stem = name.trim_end_matches(".exe");
    SHELLS.iter().any(|s| stem == s.trim_end_matches(".exe"))
}

/// Every process this user can see. The name comes from a snapshot of the whole machine; the
/// command line is asked for only where an assistant could be hiding inside a host process, so this
/// is not two hundred questions every two seconds.
fn snapshot() -> Vec<Proc> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == -1isize as _ {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 {
            let n = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..n]).to_lowercase();
            let ask = WORTH_A_LOOK.iter().any(|w| name.trim_end_matches(".exe") == *w);
            out.push(Proc {
                pid: entry.th32ProcessID,
                ppid: entry.th32ParentProcessID,
                line: if ask { command_line(entry.th32ProcessID) } else { String::new() },
                name,
            });
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
    }
    out
}

/// A process's command line, or empty when it will not be read. Asked of ntdll directly:
/// `QueryFullProcessImageNameW` gives the image, and an assistant a package manager installed is
/// named `node.exe` there.
fn command_line(pid: u32) -> String {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    type NtQueryInformationProcess = unsafe extern "system" fn(isize, u32, *mut u8, u32, *mut u32) -> i32;
    static NT: std::sync::OnceLock<Option<NtQueryInformationProcess>> = std::sync::OnceLock::new();
    let nt = *NT.get_or_init(|| unsafe {
        let ntdll: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
        let module = GetModuleHandleW(ntdll.as_ptr());
        GetProcAddress(module, c"NtQueryInformationProcess".as_ptr().cast()).map(|f| std::mem::transmute(f))
    });
    let Some(nt) = nt else { return String::new() };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return String::new();
    }
    // PROCESS_COMMANDLINE_INFORMATION (60): a UNICODE_STRING the kernel fills in inside the buffer
    // it was given, its own `Buffer` pointing into that buffer.
    let mut buf = vec![0u8; 4096];
    let mut got = 0u32;
    let status = unsafe { nt(handle as isize, 60, buf.as_mut_ptr(), buf.len() as u32, &mut got) };
    unsafe { CloseHandle(handle) };
    if status != 0 {
        return String::new();
    }
    let length = u16::from_le_bytes([buf[0], buf[1]]) as usize;
    let buffer = usize::from_le_bytes(buf[8..16].try_into().unwrap_or([0; 8]));
    if length == 0 || length > 2048 || buffer == 0 {
        return String::new();
    }
    // the pointer is into this buffer's own memory: take it as an offset, never dereference blindly
    let start = buf.as_ptr() as usize;
    let at = match buffer.checked_sub(start) {
        Some(at) if at + length <= buf.len() => at,
        _ => return String::new(),
    };
    let words: Vec<u16> = buf[at..at + length].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&words).to_lowercase()
}

// ------------------------------------------------------------------------- a hand on the memory

/// The six kinds, as indices into the one list both ends keep (`vault::touch_kinds()`). The words
/// themselves are never written here — a note takes the index and asks the vault for the word. A
/// second list is exactly the drift `TOUCH.md` exists to prevent.
const KIND_PATCH: usize = 0;
const KIND_HOOK: usize = 1;
/// The stub's own (a debug register on us is seen from inside). Named here so the list of six is
/// whole in one place; this end never fires it.
#[allow(dead_code)]
const KIND_BREAK: usize = 2;
const KIND_WRITE: usize = 3;
const KIND_INJECT: usize = 4;
const KIND_SCRIPT: usize = 5;

/// The one kind word at this index of the closed list.
fn kind_word(index: usize) -> &'static str {
    crate::vault::touch_kinds().split('|').nth(index).unwrap_or_default()
}

/// The four lines of a touch note, the body the stub writes and the loader forwards. The loader's
/// own prefix is put on by `touch_body`, below — the MAC is over every byte of that, prefix
/// included (TOUCH.md §3), so the body that travels is never the four lines alone.
fn touch_lines(build: i64, kind: usize, why: &str) -> String {
    format!(
        "Cursed Recode — the game was closed\nbuild: {build}\n{}{}\nwhy: {why}\n",
        crate::vault::touch_note(),
        kind_word(kind)
    )
}

/// The whole report body of a touch: the loader's prefix, then the four lines. `api::touch` signs
/// exactly these bytes.
fn touch_body(version: &str, build: i64, kind: usize, why: &str) -> String {
    format!("Cursed Recode {version} (build {build})\n{}", touch_lines(build, kind, why))
}

/// Whose report a note is: `"touch"` when it carries a `touch: ` line, `"protect"` otherwise. The
/// word after `touch: ` is not looked at here — the server checks it against the closed list, and
/// this end translating anything is the second list the contract refuses. `touch:` with no space
/// is not a touch line.
fn source_of(body: &str) -> &'static str {
    let marker = crate::vault::touch_note();
    if body.lines().any(|line| line.starts_with(marker)) {
        crate::vault::touch_source()
    } else {
        "protect"
    }
}

/// A memory-poke or teardown word in one file's own name. Two lists, two rules — the library's
/// own matcher (`touch_scripthost::poke_word_in`) is this same question and is kept in step by
/// hand:
///
/// - `touch_pokes` is the words that are never anybody's name for ordinary work. The word must
///   sit in the name as its own run (`carries_word`): `cheatengine_hook.py` and `pymem_recode.py`
///   are hands, and `friday.py` is not — `frida` inside an ordinary word is nobody's.
/// - `touch_nouns` is `injector|ripper|dumper|unpacker|decompiler` — words that also name
///   ordinary modder tooling. A modder's `resource_unpacker.py` / `db_dumper.py` /
///   `run_decompiler.py` / `dependency_injector.py` started after the game is NOT a hand: for
///   this audience that is the ordinary Tuesday. They fire only when they ARE the file's own
///   stem — `injector.py`, `injector64.exe`, `injector_v2.py` — or when the file is one of ours
///   beside them (`recode_ripper.py`). `stem_core` takes a version tail off the stem and nothing
///   else.
fn poke_in(file_name: &str) -> Option<&'static str> {
    let lower = file_name.to_lowercase();
    for w in crate::vault::touch_pokes().split('|').map(str::trim).filter(|w| !w.is_empty()) {
        if carries_word(&lower, w) {
            return Some(w);
        }
    }
    let core = stem_core(&lower);
    let ours = wears_our_name(&lower);
    for w in crate::vault::touch_nouns().split('|').map(str::trim).filter(|w| !w.is_empty()) {
        if core == w || (ours && carries_word(&lower, w)) {
            return Some(w);
        }
    }
    None
}

/// Whether `word` sits in `name` as its own run of characters, not inside a longer word.
/// `frida` in `friday` is nobody's hand; `frida` in `frida-server`, `cheatengine` in
/// `cheatengine_hook` and `pymem` in `pymem_scan` are. A digit after the word is a version tail
/// (`cheatengine64`), not another word. This is the difference the whole `poke_in` rests on: a
/// substring of an ordinary word is not a memory-poke word.
fn carries_word(name: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let b = name.as_bytes();
    let w = word.as_bytes();
    let mut from = 0usize;
    while let Some(at) = name[from..].find(word).map(|i| from + i) {
        let before_ok = at == 0 || !b[at - 1].is_ascii_alphanumeric();
        let after = at + w.len();
        // a letter after the word is a longer word (`friday`); a digit is a tail (`…64`)
        let after_ok = after >= b.len() || !b[after].is_ascii_alphabetic();
        if before_ok && after_ok {
            return true;
        }
        from = at + 1;
    }
    false
}

/// The file's stem with its extension and a version tail taken off — and nothing else. `injector64`
/// and `injector_v2` are `injector`; `resource_unpacker` stays `resource_unpacker`. This is the
/// difference between "this file is an injector" and "the word occurs somewhere in a longer name".
fn stem_core(name: &str) -> &str {
    let stem = match name.rsplit_once('.') {
        Some((s, _)) if !s.is_empty() => s,
        _ => name,
    };
    let b = stem.as_bytes();
    let mut end = b.len();
    while end > 0 {
        let c = b[end - 1];
        if c.is_ascii_digit() || c == b'.' || c == b'-' || c == b'_' {
            end -= 1;
            continue;
        }
        // `…_v2` / `…_x86` style tails: a letter only when a digit or separator is just before it
        if matches!(c, b'v' | b'V' | b'x' | b'X') && end >= 2 {
            let prev = b[end - 2];
            if prev.is_ascii_digit() || prev == b'.' || prev == b'-' || prev == b'_' {
                end -= 1;
                continue;
            }
        }
        break;
    }
    &stem[..end]
}

/// Whether this name wears ours beside a poke — a strengthening phrase for `why:`, never a cause.
fn wears_our_name(file_name: &str) -> bool {
    file_name.to_lowercase().contains(crate::vault::ours_name())
}

/// Whether one process is named as a hand on our memory, and what to call it. The whole of the
/// question without the machine under it: the **file's own name** — this process's, or a script
/// its command line runs — carries a memory-poke word.
///
/// This is the shape only. Whether it is a ban is the caller's, and the line there is `arrived`:
/// a tool already on the machine when the game came up is presence, and presence is never a touch
/// (TOUCH.md §5) — the player may have had it open since boot for something else entirely. Our own
/// name is never the cause: a player's `recode_music.py` is a music script and is silent; our name
/// beside a poke is a strengthening phrase in `why:`.
fn a_hand_on_us(name: &str, line: &str) -> Option<(usize, String)> {
    let bare = name.trim_end_matches(".exe");
    let stem = bare.rsplit(['/', '\\']).next().unwrap_or(bare);
    if let Some(poke) = poke_in(stem) {
        let phrase = if wears_our_name(stem) {
            format!("{stem} — aimed at us, carries {poke}")
        } else {
            format!("{stem} — carries {poke}")
        };
        return Some((KIND_SCRIPT, phrase));
    }
    if let Some((poke, file, ours)) = poking(line) {
        let phrase = if ours {
            format!("{file} — aimed at us, carries {poke}")
        } else {
            format!("{file} — carries {poke}")
        };
        return Some((KIND_SCRIPT, phrase));
    }
    None
}

/// The script this command line runs whose own file name carries a poke word. **Only files of a
/// script's shape are read** — `python recode_ripper.py` runs a file whose name is the hand;
/// `pip install pymem` names a package in an argument and is a player installing one. The whole
/// command line is never matched; the file that runs is. Our own name beside a poke strengthens
/// the reason; our own name alone is silent (`recode_music.py` is a music script).
fn poking(line: &str) -> Option<(&'static str, String, bool)> {
    const SCRIPT_EXT: [&str; 7] = ["py", "js", "ps1", "bat", "cmd", "vbs", "jar"];
    for token in line.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
        let token = token.trim_matches(|c: char| matches!(c, ',' | ';' | '(' | ')' | '='));
        if token.is_empty() {
            continue;
        }
        let name = token.rsplit(['/', '\\']).next().unwrap_or(token);
        let lower = name.to_lowercase();
        let ext = lower.rsplit('.').next().unwrap_or("");
        if !SCRIPT_EXT.contains(&ext) {
            continue;
        }
        if let Some(poke) = poke_in(&lower) {
            let also_us = wears_our_name(&lower);
            return Some((poke, lower, also_us));
        }
    }
    None
}

/// Late unsigned mod natives may load from any directory. Signature and location
/// are not evidence of injection; actual memory changes are checked separately.
fn foreign_module(name: &str, _dir: &str, _known_dirs: &[String], _has_signature: Option<bool>) -> Option<&'static str> {
    if poke_in(name).is_some() {
        return Some("its name is an injector's shape");
    }
    if is_tls_holder(name) {
        return None;
    }
    None
}

/// The TLS holder the protector drops for a hand-mapped PE: `crd` + a pid/run/clock of hex + `.dll`
/// (see the stub's `holder::drop_file`). It is ours and is silence forever. The shape is exact:
/// `crd` + exactly twenty hex digits + `.dll`. An ordinary `crd*.dll` somebody else dropped is
/// still a stranger and must still close the game.
fn is_tls_holder(name: &str) -> bool {
    let low = name.to_lowercase();
    let Some(stem) = low.strip_suffix(".dll") else {
        return false;
    };
    let Some(hex) = stem.strip_prefix("crd") else {
        return false;
    };
    hex.len() == 20 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// One folder into the baseline's list, without letting the compare swallow the whole of the
/// user's temp. The compare treats a known folder's parent as known too (`bin` beside
/// `bin\natives`), so a folder that *is* temp or sits above it would hand over every drop in temp.
/// Nothing under temp is ever taken — not even the unpack roots. Those are answered by
/// `unpack_ground`, which is a child-of-temp question only and never widens the ground.
fn push_known_dir(known: &mut Vec<String>, dir: &str) {
    if dir.is_empty() {
        return;
    }
    let temp = std::env::temp_dir().to_string_lossy().to_lowercase();
    let temp = temp.trim_end_matches(['\\', '/']);
    let lower = dir.to_lowercase();
    let lower = lower.trim_end_matches(['\\', '/']).to_string();
    if lower == temp
        || temp.starts_with(&lower)
        || lower.strip_prefix(temp).is_some_and(|r| r.starts_with(['\\', '/']))
    {
        return;
    }
    if !known.iter().any(|d| d == &lower) {
        known.push(lower);
    }
}

/// An attacher aimed at this pid: `jattach`, `jcmd`'s loading half, `jdb`, `jshell`, or a bare
/// `java.exe` running an `Attach` class — and only with our pid in its argv. `jps`, `jstack`,
/// `jmap` and a read-only `jcmd` are the OS talking and are silent. Her own IntelliJ attaches to
/// the game every day, so the ban this leads to is a release-only door (`license::RELEASE`).
fn attacher(line: &str, game_pid: u32) -> Option<&'static str> {
    let pid = game_pid.to_string();
    let mut first = "";
    let mut aimed = false;
    let mut loading_jcmd = false;
    let mut attach_class = false;
    for token in line.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
        let token = token.trim_matches(|c: char| matches!(c, ',' | ';' | '(' | ')' | '='));
        if token.is_empty() {
            continue;
        }
        if first.is_empty() {
            first = token;
        }
        if token == pid {
            aimed = true;
        }
        let stem = token.rsplit(['/', '\\']).next().unwrap_or(token).split('.').next().unwrap_or("");
        if stem.eq_ignore_ascii_case("attach") {
            attach_class = true;
        }
    }
    // `jcmd <pid> load instrument …` and `JVMTI.agent_load` are phrases across tokens, so the
    // line is read whole for them. A read-only `jcmd` carries neither.
    let lower_line = line.to_lowercase();
    if lower_line.contains("jvmti.agent_load") || lower_line.contains("load instrument") || lower_line.contains("load agent") {
        loading_jcmd = true;
    }
    if !aimed {
        return None;
    }
    let stem = first.rsplit(['/', '\\']).next().unwrap_or(first).split('.').next().unwrap_or("");
    match stem.to_ascii_lowercase().as_str() {
        "jattach" => Some("jattach"),
        "jdb" => Some("jdb"),
        "jshell" => Some("jshell"),
        "jcmd" if loading_jcmd => Some("jcmd loading an agent"),
        "java" | "javaw" if attach_class => Some("a Java attach"),
        _ => None,
    }
}

/// The Java agent flags out of one of the launch-time environments (`-javaagent`, `-agentpath`,
/// `-agentlib`), the rest of the line left as it was. Always done, to the child's own environment
/// and not only with `env_remove` — a user who set one system-wide would otherwise hand it to the
/// game anyway. A defence, not a detection: no ban comes of it and no player is told.
fn without_agent_flags(value: &str) -> String {
    value
        .split_whitespace()
        .filter(|t| {
            let lower = t.to_lowercase();
            !(lower.starts_with("-javaagent") || lower.starts_with("-agentpath") || lower.starts_with("-agentlib"))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A Java agent in the launch-time environments that is aimed at us: its own file name carries a
/// poke word. A corporate New Relic on someone's work laptop is not aimed at us and is silent —
/// and so is anything that merely wears our name, because **our own name is never a trigger**
/// (TOUCH.md §5): a player's `recode_music.py` is a music script. Our name beside a poke is a
/// strengthening phrase in the script layer's `why:`, never the cause here. The file's own name,
/// never the whole of the option.
fn aimed_agent(agent: &str) -> Option<&'static str> {
    let name = agent.rsplit(['/', '\\']).next().unwrap_or(agent);
    // `-javaagent:path.jar=opts` — the name is before whatever the agent is given
    let name = name.split('=').next().unwrap_or(name);
    if poke_in(name).is_some() {
        return Some("its name is a poke at our memory");
    }
    None
}

/// A Java agent that **appeared** in the launch-time environments while the game ran, and is aimed
/// at us. One set before we came up is silence (TOUCH.md `touch_jvm`: "one set before we came up") —
/// the player may have had a system-wide `JAVA_TOOL_OPTIONS` since forever, and this end already
/// scrubbed agent flags out of the child before the game ever saw them (`without_agent_flags`).
/// Presence is never a touch; the line is arrival. The name that comes back is a file name, never a
/// path.
fn new_aimed_agent(now: &[String], was: &[String]) -> Option<(String, &'static str)> {
    for agent in now {
        if was.iter().any(|b| b == agent) {
            continue;
        }
        if let Some(why) = aimed_agent(agent) {
            let name = agent.rsplit(['/', '\\']).next().unwrap_or(agent).split('=').next().unwrap_or(agent).to_string();
            return Some((name, why));
        }
    }
    None
}

/// Every agent path the launch-time environments still carry, after their names.
fn agents_in_env() -> Vec<String> {
    let mut out = Vec::new();
    for key in ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS"] {
        let Ok(value) = std::env::var(key) else { continue };
        for token in value.split_whitespace() {
            let lower = token.to_lowercase();
            if lower.starts_with("-javaagent") || lower.starts_with("-agentpath") || lower.starts_with("-agentlib") {
                // `-javaagent:path` — the path is what follows the first colon
                if let Some((_, path)) = token.split_once(':') {
                    let path = path.split('=').next().unwrap_or(path);
                    if !path.is_empty() {
                        out.push(path.to_string());
                    }
                }
            }
        }
    }
    out
}

/// What the game's memory looked like when the look began: every module it had loaded, a fold of
/// the code of our own among them, and every thread it had. Taken once, a few seconds after the
/// child is up — Java loads natives for minutes, and a baseline at the door would call every one
/// of them foreign. Directories are kept for the comparison and never leave this process.
struct MemBase {
    build: i64,
    version: String,
    modules: Vec<BaseModule>,
    /// Every folder the game was already loading from when the look began, plus the two places its
    /// own pair lives under. Compared internally and never written down — a note that carries a
    /// path carries the player's name.
    known_dirs: Vec<String>,
    /// Every process that was already up — so a script is one that *appeared*, and not one the
    /// player had open since before the game.
    procs: Vec<u32>,
    /// Every agent path the launch-time environments already carried when the look began. One set
    /// before we came up is silence (TOUCH.md `touch_jvm`), the same line `procs` draws for a tool.
    agents: Vec<String>,
}

struct BaseModule {
    /// Lowercase file name. Never a path in a note.
    name: String,
    base: usize,
    ours: bool,
    /// FNV of the module's **executable sections** (capped), ours only. Never the whole image:
    /// headers, `.rdata`, `.data`, `.bss`, the IAT and the relocations are not code, and our own
    /// stub writes three of them on purpose — wiping its import names and its key slots is not a
    /// patch of our code. `None` means the read did not cover those sections, and this round is
    /// not comparable against anything.
    fold: Option<u64>,
    /// How many code bytes that fold covered. Two folds of unequal length differ over identical
    /// bytes (`Toolhelp32ReadProcessMemory` fills `got`), so they are never compared.
    code_len: usize,
    /// The entry point's own first bytes, ours only — a head at its own place.
    head: [u8; 16],
    /// SizeOfImage and the section count: a threshold.
    mark: u64,
    /// The executable sections, as `(rva, len)` from the module base. Ours only — this is the whole
    /// of what the byte checks and the page check look at.
    texts: Vec<(usize, usize)>,
    /// The spans inside those sections that were **already** writable when the baseline was taken.
    /// `Some(empty)` on every honest module: `.text` is execute-read and stays that way. `None`
    /// when the walk could not be finished — then no page may be called newly open.
    text_writable_was: Option<Vec<(usize, usize)>>,
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// `IMAGE_SCN_MEM_EXECUTE` — the section may be run. The only sections this look polices.
/// A section meant to be written (`IMAGE_SCN_MEM_WRITE`) is `.data`/`.bss` — our own key slots
/// live there and are never asked about here.
const SCN_MEM_EXECUTE: u32 = 0x2000_0000;
/// `IMAGE_SCN_MEM_DISCARDABLE` — `.reloc` and friends, which legitimately stop matching their file
/// form the moment the loader has applied them.
const SCN_MEM_DISCARDABLE: u32 = 0x0200_0000;

/// The executable sections of a module, as `(rva, len)` from the image base — the code, and only
/// the code. `.data` and `.rdata` are not here: our own stub fills its key slots in one and wipes
/// its import names in the other, and neither is a patch. `.reloc` is not here either: the loader
/// rewrites it and that is its job. A section that is executable **and** discardable is likewise
/// left out.
///
/// A section that is executable *and* writable (a packed stub) is kept for the fold — code is code
/// and a byte of it moving is a patch — but the page check reads its being writable as what it
/// already was, and not as a page that was "made" writable.
fn text_ranges(header: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let Some((_, image, count)) = pe_shape(header) else {
        return out;
    };
    let lfanew = u32::from_le_bytes(header[0x3C..0x40].try_into().unwrap_or([0; 4])) as usize;
    if lfanew + 22 > header.len() {
        return out;
    }
    let opt_size = u16::from_le_bytes(header[lfanew + 20..lfanew + 22].try_into().unwrap_or([0; 2])) as usize;
    let mut at = lfanew + 24 + opt_size;
    let image = image as usize;
    for _ in 0..count {
        if at + 40 > header.len() {
            break;
        }
        let s = &header[at..at + 40];
        at += 40;
        let chars = u32::from_le_bytes(s[36..40].try_into().unwrap_or([0; 4]));
        if chars & SCN_MEM_EXECUTE == 0 {
            continue;
        }
        if chars & SCN_MEM_DISCARDABLE != 0 {
            continue;
        }
        let rva = u32::from_le_bytes(s[12..16].try_into().unwrap_or([0; 4])) as usize;
        let vsize = u32::from_le_bytes(s[8..12].try_into().unwrap_or([0; 4])) as usize;
        let raw = u32::from_le_bytes(s[16..20].try_into().unwrap_or([0; 4])) as usize;
        let len = if vsize > 0 { vsize } else { raw };
        if len == 0 || rva >= image {
            continue;
        }
        out.push((rva, len.min(image.saturating_sub(rva))));
    }
    out
}

/// Fold the executable sections of one module's image. `None` when the read did not cover them —
/// a short read is not a byte change and must never be compared against a full one.
fn text_fold(bytes: &[u8], got: usize, texts: &[(usize, usize)]) -> Option<(u64, usize)> {
    if texts.is_empty() {
        return None;
    }
    let mut code = Vec::new();
    for &(rva, len) in texts {
        let end = rva.saturating_add(len);
        if end > got || end > bytes.len() {
            return None;
        }
        code.extend_from_slice(&bytes[rva..end]);
    }
    if code.is_empty() {
        None
    } else {
        let n = code.len();
        Some((fnv(&code), n))
    }
}

/// The PE shape of a module from its first page: the entry point's RVA, SizeOfImage, and how many
/// sections there are. A threshold that moves is not a head that changed where it always was.
fn pe_shape(header: &[u8]) -> Option<(u32, u32, u16)> {
    if header.len() < 64 {
        return None;
    }
    let lfanew = u32::from_le_bytes(header[0x3C..0x40].try_into().ok()?) as usize;
    if lfanew + 24 + 60 > header.len() {
        return None;
    }
    if &header[lfanew..lfanew + 4] != b"PE\0\0" {
        return None;
    }
    let sections = u16::from_le_bytes(header[lfanew + 6..lfanew + 8].try_into().ok()?);
    let opt = lfanew + 24;
    let entry = u32::from_le_bytes(header[opt + 16..opt + 20].try_into().ok()?);
    let size = u32::from_le_bytes(header[opt + 56..opt + 60].try_into().ok()?);
    Some((entry, size, sections))
}

/// Whether the **file on disk** behind a module carries an Authenticode blob. `None`: the file
/// could not be read or its header could not be parsed — and that is silence, never a guess
/// (TOUCH.md §5: "a check that cannot answer is not a finding").
///
/// This is the cheap half of the question and deliberately so. It answers "is there a signature
/// blob at all", not "does it chain to a trusted root" — the stub inside the game asks
/// `WinVerifyTrust` for the real answer (`touch_unsigned`). What this end needs is the
/// false-positive guard: Discord's overlay, a GPU runtime and every vendor DLL arrive with a blob
/// and must never be a finding, while a dropped `evil.dll` has none. Only the optional header is
/// read (the certificate table lives at the end of the file and is never touched).
fn pe_signed_on_disk(dir: &str, name: &str) -> Option<bool> {
    if dir.is_empty() || name.is_empty() {
        return None;
    }
    let path = format!("{dir}\\{name}");
    let bytes = std::fs::read(&path).ok()?;
    if bytes.len() < 64 {
        return None;
    }
    let lfanew = u32::from_le_bytes(bytes[0x3C..0x40].try_into().ok()?) as usize;
    if lfanew + 24 > bytes.len() || &bytes[lfanew..lfanew + 4] != b"PE\0\0" {
        return None;
    }
    let opt = lfanew + 24;
    if opt + 2 > bytes.len() {
        return None;
    }
    let magic = u16::from_le_bytes(bytes[opt..opt + 2].try_into().ok()?);
    // PE32 puts the data directories at 96, PE32+ at 112; the security directory is index 4 and
    // each entry is two dwords (for this one: a file offset and a size).
    let (dirs_at, dir_count_at) = match magic {
        0x10B => (opt + 96, opt + 92),
        0x20B => (opt + 112, opt + 108),
        _ => return None,
    };
    if dir_count_at + 4 > bytes.len() {
        return None;
    }
    let count = u32::from_le_bytes(bytes[dir_count_at..dir_count_at + 4].try_into().ok()?) as usize;
    if count < 5 {
        // a PE that names no security directory is a PE with no signature
        return Some(false);
    }
    let sec = dirs_at + 8 * 4;
    if sec + 8 > bytes.len() {
        return None;
    }
    let size = u32::from_le_bytes(bytes[sec + 4..sec + 8].try_into().ok()?);
    Some(size > 0)
}

/// One process's own name and folder of a module, from the loader's side of the wall.
fn modules_of(pid: u32) -> Vec<(String, String, usize, usize)> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32,
    };
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
        if snap == -1isize as _ {
            return out;
        }
        let mut entry: MODULEENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
        let mut ok = Module32FirstW(snap, &mut entry);
        while ok != 0 {
            let n = entry.szModule.iter().position(|&c| c == 0).unwrap_or(entry.szModule.len());
            let name = String::from_utf16_lossy(&entry.szModule[..n]).to_lowercase();
            let p = entry.szExePath.iter().position(|&c| c == 0).unwrap_or(entry.szExePath.len());
            let path = String::from_utf16_lossy(&entry.szExePath[..p]).to_lowercase();
            let dir = path.rsplit_once('\\').map(|(d, _)| d.to_string()).unwrap_or_default();
            out.push((name, dir, entry.modBaseAddr as usize, entry.modBaseSize as usize));
            ok = Module32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
    }
    out
}

/// Bytes out of the game's own address space, the loader's side of the wall.
fn read_remote(pid: u32, base: usize, buf: &mut [u8]) -> usize {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::Toolhelp32ReadProcessMemory;
    let mut got = 0usize;
    let ok = unsafe {
        Toolhelp32ReadProcessMemory(pid, base as *const _, buf.as_mut_ptr() as *mut _, buf.len(), &mut got)
    };
    if ok == 0 {
        0
    } else {
        got
    }
}

/// Whether this process came up after the game did. `None` when Windows will not say when it
/// began — and that is silence, never a guess on its own.
///
/// The callers draw the line themselves. `judge` treats `None` as arrival (TOUCH.md §5: "when the
/// start time cannot be read, the loader assumes it arrived. That is the aggressive side, and it is
/// the side she asked for") — but only as one half of a name that already carries a poke word. A
/// look that already holds the process in its baseline does **not** get to overwrite that fact with
/// a clock that would not answer: see `arrived_since`.
fn started_after(pid: u32, since: SystemTime) -> Option<bool> {
    use windows_sys::Win32::Foundation::{CloseHandle as Close, FILETIME};
    use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut created = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut exit = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut kernel = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut user = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let ok = unsafe { GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) };
    unsafe { Close(handle) };
    if ok == 0 {
        return None;
    }
    // FILETIME is 100ns ticks since 1601; SystemTime is seconds since 1970. 116444736000000000
    // is the gap between the two epochs.
    let ticks = ((created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64).saturating_sub(116_444_736_000_000_000);
    let Ok(ago) = since.duration_since(std::time::UNIX_EPOCH) else {
        return None;
    };
    Some(ticks >= (ago.as_nanos() / 100) as u64)
}

/// Whether this process is one that **arrived** while the game ran, against a list of pids the
/// look already held. Already in that list is presence, and presence is never a touch (TOUCH.md
/// §5) — a start time that cannot be read must not overwrite the fact that we saw it here. Only a
/// pid the list did not see, or one that positively began after us (the number was reused), is
/// arrival.
fn arrived_since(pid: u32, baseline_pids: &[u32], since: SystemTime) -> bool {
    if baseline_pids.contains(&pid) {
        started_after(pid, since) == Some(true)
    } else {
        true
    }
}

/// The spans of our code that are writable right now, as `(rva_lo, rva_hi)` from the module base.
/// Only the executable sections are walked. `Some(empty)` on every honest module: `.text` is
/// execute-read and stays that way, while `.data` and `.fptable` are born `PAGE_READWRITE` and
/// relocation pages `PAGE_WRITECOPY` from the moment the loader maps the image — and those are not
/// code, so they are never asked.
///
/// `None` when the walk could not be finished — a query that will not answer is not a list of
/// spans, and a caller that has no baseline of what was open cannot call any page "newly" open.
fn writable_text_spans(pid: u32, base: usize, texts: &[(usize, usize)]) -> Option<Vec<(usize, usize)>> {
    use windows_sys::Win32::System::Memory::{
        MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_READWRITE,
        PAGE_WRITECOPY, VirtualQueryEx,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION};
    let mut out = Vec::new();
    if texts.is_empty() {
        return Some(out);
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    for &(rva, len) in texts {
        let mut at = base.saturating_add(rva);
        let end = base.saturating_add(rva.saturating_add(len));
        while at < end {
            let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
            let got = unsafe {
                VirtualQueryEx(handle, at as *const _, &mut info, std::mem::size_of::<MEMORY_BASIC_INFORMATION>())
            };
            if got == 0 {
                // a region that will not answer is not a walk we finished
                unsafe { CloseHandle(handle) };
                return None;
            }
            if info.State == MEM_COMMIT {
                let page = info.Protect & 0xff;
                let writable = page == PAGE_READWRITE
                    || page == PAGE_WRITECOPY
                    || page == PAGE_EXECUTE_READWRITE
                    || page == PAGE_EXECUTE_WRITECOPY;
                if writable {
                    let lo = (at as usize).saturating_sub(base).max(rva);
                    let hi = (at as usize)
                        .saturating_add(info.RegionSize as usize)
                        .saturating_sub(base)
                        .min(rva.saturating_add(len));
                    if hi > lo {
                        out.push((lo, hi));
                    }
                }
            }
            if info.RegionSize == 0 {
                unsafe { CloseHandle(handle) };
                return None;
            }
            at += info.RegionSize;
        }
    }
    unsafe { CloseHandle(handle) };
    Some(out)
}

/// A page of our **code** that is writable and was not writable when the baseline was taken. RW /
/// WC / ERW / EWC are writable; `PAGE_EXECUTE`, `PAGE_EXECUTE_READ` and `PAGE_READONLY` are not.
/// A page turned writable is a finding **before** a byte lands on it (TOUCH.md §1). Returns the
/// kind of page, not an address — an address is never written down.
///
/// The question is the **transition on a code page**, never the current protection of the image.
/// A page that was already open at baseline is nobody's finding here — and neither is one when
/// there is **no** baseline of what was open (`None`): "became writable" is a claim about a
/// before, and a look that never had one is silence (TOUCH.md §5).
fn writable_text_page_of(
    pid: u32,
    base: usize,
    texts: &[(usize, usize)],
    was: Option<&[(usize, usize)]>,
) -> Option<&'static str> {
    use windows_sys::Win32::System::Memory::{
        MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READWRITE, PAGE_READWRITE, PAGE_WRITECOPY, VirtualQueryEx,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION};
    let was = was?;
    for (lo, hi) in writable_text_spans(pid, base, texts)? {
        if covered_by(lo, hi, was) {
            continue;
        }
        // the kind of page this span sits on, for the note
        let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let got = unsafe {
            VirtualQueryEx(handle, (base + lo) as *const _, &mut info, std::mem::size_of::<MEMORY_BASIC_INFORMATION>())
        };
        let page = if got == 0 { 0 } else { info.Protect & 0xff };
        unsafe { CloseHandle(handle) };
        return Some(if page == PAGE_READWRITE {
            "RW"
        } else if page == PAGE_WRITECOPY {
            "WC"
        } else if page == PAGE_EXECUTE_READWRITE {
            "ERW"
        } else {
            "EWC"
        });
    }
    None
}

/// Was the span `[lo, hi)` inside one the baseline already knew was writable?
fn covered_by(lo: usize, hi: usize, was: &[(usize, usize)]) -> bool {
    was.iter().any(|&(a, b)| a <= lo && hi <= b)
}

impl MemBase {
    fn take(game_pid: u32, home: &Path, m: &Manifest) -> MemBase {
        let mut modules = Vec::new();
        let mut known_dirs: Vec<String> = Vec::new();
        for (name, dir, base, size) in modules_of(game_pid) {
            push_known_dir(&mut known_dirs, &dir);
            let ours = name.contains(crate::vault::ours_dll()) || name.contains(crate::vault::ours_name());
            let mut fold = None;
            let mut code_len = 0usize;
            let mut head = [0u8; 16];
            let mut mark = 0u64;
            let mut texts = Vec::new();
            let mut text_writable_was: Option<Vec<(usize, usize)>> = None;
            if ours && size > 0 {
                let cap = size.min(8 * 1024 * 1024);
                let mut bytes = vec![0u8; cap];
                let got = read_remote(game_pid, base, &mut bytes);
                if got > 0 {
                    texts = text_ranges(&bytes);
                    if let Some((f, n)) = text_fold(&bytes, got, &texts) {
                        fold = Some(f);
                        code_len = n;
                    }
                    if let Some((entry, image, sections)) = pe_shape(&bytes) {
                        mark = (image as u64) << 16 | sections as u64;
                        let at = entry as usize;
                        if at + 16 <= got {
                            head.copy_from_slice(&bytes[at..at + 16]);
                        }
                    }
                    text_writable_was = writable_text_spans(game_pid, base, &texts);
                }
            }
            modules.push(BaseModule {
                name,
                base,
                ours,
                fold,
                code_len,
                head,
                mark,
                texts,
                text_writable_was,
            });
        }
        // the game's own ground, even before anything of it is in the module list: the folder the
        // build runs from, and the folder its own pair is cut into. A sealed game is mapped by hand
        // and never shows up in the module list, so nothing of `bin\` would otherwise be known —
        // and our own stub, loading a second late, would look like a stranger from a strange place.
        let root = home.to_string_lossy().to_lowercase();
        for extra in [root.clone(), library(home, m).ok().and_then(|p| p.parent().map(|d| d.to_string_lossy().to_lowercase())).unwrap_or_default()] {
            push_known_dir(&mut known_dirs, &extra);
        }
        let procs = snapshot().into_iter().map(|p| p.pid).collect();
        let agents = agents_in_env();
        MemBase {
            build: m.build,
            version: m.version.clone(),
            modules,
            known_dirs,
            procs,
            agents,
        }
    }
}

/// The outside look at the game's memory: what was put on it while it ran. Called on the look's
/// cadence (once soon after the game is up, then every minute). A finding closes the game and the
/// note is the contract's four lines under the loader's prefix — the `touch:` line carries one of
/// the six words by index, and `why:` carries the name of what was caught and nothing else. Never
/// a path, never a user name, never a word read out of a document.
///
/// What this end sees and what it does not: a new library needs a reason (Java loads natives for
/// minutes); a script is named for our memory in its own file name and never for our name alone;
/// a head that changed where it always was is `hook` and a threshold that moved is `patch`; a page
/// made writable is `write` before a byte lands. A debug register, an unbacked thread, a manual
/// map and an IAT slot belong to the stub inside the game — this end does not fire those words.
fn look_at_memory(game_pid: u32, since: SystemTime, baseline: &MemBase) -> Option<Found> {
    // a Java agent aimed at us that **arrived** while the game ran. One the launch-time
    // environments already carried when the look began is silence — presence is never a touch
    // (TOUCH.md `touch_jvm`), and this end scrubs agent flags out of the child anyway.
    if let Some((name, why)) = new_aimed_agent(&agents_in_env(), &baseline.agents) {
        return Some(Found {
            body: touch_body(
                &baseline.version,
                baseline.build,
                KIND_INJECT,
                &format!("{name} — {why}"),
            ),
        });
    }

    // an attacher aimed at this pid. Release only: her IntelliJ attaches to the game every day,
    // and a ban over that would ban her.
    if crate::license::RELEASE {
        let procs = snapshot();
        for p in &procs {
            if p.pid == game_pid {
                continue;
            }
            if let Some(what) = attacher(&p.line, game_pid) {
                return Some(Found {
                    body: touch_body(
                        &baseline.version,
                        baseline.build,
                        KIND_INJECT,
                        &format!("{what} (pid {}) was aimed at the game", p.pid),
                    ),
                });
            }
        }
    }

    // a script named for our memory. The file's own name, never the whole command line, and our
    // name alone is silent — `recode_music.py` is a music script. Only one that **arrived** while
    // the game was up is a hand here: a tool the player already had open since boot is presence,
    // and presence is never a touch (TOUCH.md §5) — `judge` takes the same line over the same
    // names and puts that case on the old path instead of a ban.
    let procs = snapshot();
    for p in &procs {
        if p.pid == game_pid {
            continue;
        }
        let hit = poking(&p.line).or_else(|| {
            let stem = p.name.trim_end_matches(".exe");
            poke_in(stem).map(|poke| (poke, p.name.clone(), wears_our_name(&p.name)))
        });
        if let Some((poke, file, also_us)) = hit {
            // Presence in the baseline is a fact the clock does not get to overwrite.
            if !arrived_since(p.pid, &baseline.procs, since) {
                continue;
            }
            let why = if also_us {
                format!("{file} — {poke}, and it wears our name")
            } else {
                format!("{file} — {poke}, aimed at our memory")
            };
            return Some(Found { body: touch_body(&baseline.version, baseline.build, KIND_SCRIPT, &why) });
        }
    }

    // what the game has loaded now, against what it had when the look began. The folders the
    // baseline already loaded from are the game's own ground: a library that arrives out of one of
    // them (a late lwjgl native, a JVM DLL pulled in on first use) is the game going about its day.
    // What is left — unsigned, and from nowhere the game already knew — is the dropped DLL.
    let now = modules_of(game_pid);

    for (name, dir, _base, _size) in &now {
        if baseline.modules.iter().any(|b| b.name == *name && b.base == *_base) {
            continue;
        }
        if let Some(reason) = foreign_module(name, dir, &baseline.known_dirs, pe_signed_on_disk(dir, name)) {
            return Some(Found {
                body: touch_body(
                    &baseline.version,
                    baseline.build,
                    KIND_INJECT,
                    &format!("{name} — {reason}"),
                ),
            });
        }
    }

    // our own code: a head that changed where it always was is `hook`, a threshold that moved is
    // `patch`, and any other byte of ours that moved is `patch` too. Only modules the baseline
    // already had and that are nameable as ours are read — never a library of somebody else's.
    for was in baseline.modules.iter().filter(|m| m.ours) {
        let Some(now_mod) = now.iter().find(|n| n.0 == was.name && n.2 == was.base) else {
            // ours went away entirely: the section was rebuilt or the game is on its way out.
            // Not a hand — a load of our own library again is a restart of the look's business.
            continue;
        };
        let cap = now_mod.3.min(8 * 1024 * 1024);
        if cap == 0 {
            continue;
        }
        let mut bytes = vec![0u8; cap];
        let got = read_remote(game_pid, now_mod.2, &mut bytes);
        if got == 0 {
            continue;
        }
        let mut head = [0u8; 16];
        let mut mark = 0u64;
        if let Some((entry, image, sections)) = pe_shape(&bytes) {
            mark = (image as u64) << 16 | sections as u64;
            let at = entry as usize;
            if at + 16 <= got {
                head.copy_from_slice(&bytes[at..at + 16]);
            }
        }
        if mark != 0 && was.mark != 0 && mark != was.mark {
            return Some(Found {
                body: touch_body(
                    &baseline.version,
                    baseline.build,
                    KIND_PATCH,
                    &format!("{} — a threshold moved (the section was rebuilt)", was.name),
                ),
            });
        }
        if head != was.head && head != [0u8; 16] && was.head != [0u8; 16] {
            return Some(Found {
                body: touch_body(
                    &baseline.version,
                    baseline.build,
                    KIND_HOOK,
                    &format!("{} — a function head changed where it always was", was.name),
                ),
            });
        }
        // Only the executable sections, only a full read of them, and only against a fold taken
        // over the same number of bytes. Our own stub wipes its import names in `.rdata` and its
        // key slots in `.data`, and a second `JNI_OnLoad` rewrites its own bookkeeping — none of
        // that is code and none of it is a patch. A short read is not a byte change either.
        if let (Some((fold, code_len)), Some(was_fold)) = (text_fold(&bytes, got, &was.texts), was.fold) {
            if code_len == was.code_len && fold != was_fold {
                return Some(Found {
                    body: touch_body(
                        &baseline.version,
                        baseline.build,
                        KIND_PATCH,
                        &format!("{} — bytes of our own code changed", was.name),
                    ),
                });
            }
        }
    }

    // a page of our code made writable is a finding before a byte lands. The transition is the
    // question: `.data` is born writable on every honest module and is nobody's finding — and
    // neither is any page when the baseline never recorded what was open.
    for was in baseline.modules.iter().filter(|m| m.ours) {
        if let Some(page) = writable_text_page_of(game_pid, was.base, &was.texts, was.text_writable_was.as_deref()) {
            return Some(Found {
                body: touch_body(
                    &baseline.version,
                    baseline.build,
                    KIND_WRITE,
                    &format!("{} — a page was made writable ({page}) before any byte landed", was.name),
                ),
            });
        }
    }

    None
}

// ------------------------------------------------------------------------------- a look at the room

/// One sign of the room around the game, and no more than a name beside it (the kind's own word is
/// in the vault, so the list is not sitting next to what checks for it).
#[derive(Debug)]
pub struct Sight {
    pub kind: &'static str,
    pub name: String,
    pub note: String,
}

/// A look over the room, for her list and nothing more. **This never closes the game and never says
/// a word to the player** — whatever stands beside the game is not the player's doing, and the
/// player plays on (her word, 17.10: report only). The look that closes the game is `judge`, above;
/// this is the same room, only written down.
///
/// What is worth a line: a tool a copy is taken apart with (and which of them), our own names
/// sitting in a script or in a foreign process, something of our shape beside the install, dates
/// gathering around ours, the furniture of a virtual machine, and a parent that is nothing a player
/// would have started the game from. Each is one kind from the list the server keeps closed too
/// (`seal.rs`'s caller in `recode.rs`, `SIGHTS`).
///
/// What is never in it: a path, a user name, a document's name, anything read out of a file. Names
/// of processes and of files beside the install only — what she needs to see what stands around the
/// game, and not a look into the player's computer.
///
/// `since` is when the game came up: the install itself wrote everything beside it in one go long
/// before that, and a cluster of those is nobody's finding.
pub fn a_look(home: &Path, game_pid: u32, since: SystemTime) -> (i64, Vec<Sight>) {
    let procs = snapshot();
    look_at_the_room(&procs, home, since, game_pid)
}

/// The same look over a list of processes already taken — the whole of the question, without the
/// machine under it, so it can be asked in a test.
fn look_at_the_room(procs: &[Proc], home: &Path, since: SystemTime, game_pid: u32) -> (i64, Vec<Sight>) {
    let chain = chain_of(procs, game_pid);
    let mut out = Vec::new();

    // what is taking a copy apart, and which of them it is
    for p in procs {
        if let Some(what) = matches(p, TOOLS) {
            let kind = if TOOL_KINDS.0.iter().any(|t| named(what, t)) {
                crate::vault::sight_debugger()
            } else if TOOL_KINDS.1.iter().any(|t| named(what, t)) {
                crate::vault::sight_dumper()
            } else {
                crate::vault::sight_tool()
            };
            out.push(Sight { kind, name: what.to_string(), note: format!("pid {}", p.pid) });
        }
    }

    // our own names, where they have no business being: a script that carries them
    // (`recode_guard.py`), or a foreign process named after us. Ours are not signs of anything.
    let ours = [crate::vault::ours_name(), crate::vault::ours_dll()];
    for p in procs {
        if p.pid == game_pid || is_ours(&p.name) {
            continue;
        }
        let stem = p.name.trim_end_matches(".exe");
        if ours.iter().any(|o| stem.contains(o)) {
            out.push(Sight { kind: crate::vault::sight_module(), name: p.name.clone(), note: String::new() });
            continue;
        }
        if let Some((name, script)) = carrying(&p.line, &ours) {
            out.push(Sight {
                kind: if script { crate::vault::sight_script() } else { crate::vault::sight_module() },
                name,
                note: String::new(),
            });
        }
    }

    // something of our shape beside the install — "dump" and the like. Only the name: a path carries
    // the player's name and that is not ours to read.
    if let Ok(what) = std::fs::read_dir(home) {
        let names: Vec<&str> = crate::vault::sight_names().split('|').collect();
        for e in what.flatten() {
            let Some(name) = e.file_name().to_str().map(str::to_lowercase) else { continue };
            if names.iter().any(|w| name.contains(w)) {
                out.push(Sight { kind: crate::vault::sight_near(), name, note: String::new() });
            }
        }
    }

    // dates gathering while the game was up: several files beside the install made within a minute
    // of each other — what a dumper leaves when it writes a copy out. Not the install's own writing:
    // those all came at once, before the game was ever up (`since`).
    if let Some(times) = dates_around(home, since) {
        if times >= 3 {
            out.push(Sight { kind: crate::vault::sight_dates(), name: String::new(), note: format!("{times} together") });
        }
    }

    // the furniture of a virtual machine: a look only. The game itself is turned away by the
    // library's own `vm::here` on the way in; this is what stands around a game that is already up.
    let guests: Vec<&str> = crate::vault::sight_guest().split('|').collect();
    for p in procs {
        if let Some(what) = matches(p, &guests) {
            out.push(Sight { kind: crate::vault::sight_sandbox(), name: what.to_string(), note: format!("pid {}", p.pid) });
        }
    }

    // a parent nothing a player would have started the game from. The loader is ours and is the
    // parent of every game it starts; a shell is where the walk up stops.
    for p in chain.iter().skip(1) {
        if !is_shell(&p.name) && !is_ours(&p.name) {
            out.push(Sight { kind: crate::vault::sight_parent(), name: p.name.clone(), note: format!("pid {}", p.pid) });
        }
    }

    (score(&out), out)
}

/// Our own programs: the game, its library, the loader that starts it (in the build as
/// `recode_loader.exe`, and as the site gives it out), the window before the game, and the Java the
/// game runs inside. Windows gives these back lowercase and without their capitals, so that is how
/// they are written here. None of these is a sign of anything — a *third* process wearing one of
/// these names is.
const OURS: &[&str] = &[
    "cursedrecode", "cursedrecodeloader", "cursed recode", "recode_loader", "cursed_twin", "cursed_recode", "javaw", "java",
];

fn is_ours(name: &str) -> bool {
    let stem = name.trim_end_matches(".exe");
    OURS.iter().any(|o| named(stem, o))
}

/// A file this command line runs whose own name carries one of ours: `python …\recode_guard.py`.
/// Only the file's own name is taken — a folder called `recode` above it is a path, and paths are
/// not ours to read. The second half says whether it is a script, which is the kind her list gets.
fn carrying(line: &str, ours: &[&str]) -> Option<(String, bool)> {
    for token in line.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
        let token = token.trim_matches(|c: char| matches!(c, ',' | ';' | '(' | ')' | '='));
        if token.is_empty() {
            continue;
        }
        let name = token.rsplit(['/', '\\']).next().unwrap_or(token);
        let lower = name.to_lowercase();
        if !ours.iter().any(|o| lower.contains(o)) {
            continue;
        }
        let script = matches!(lower.rsplit('.').next().unwrap_or(""), "py" | "js" | "ps1" | "bat" | "cmd" | "vbs" | "jar");
        return Some((lower, script));
    }
    None
}

/// Which of the tools is a debugger and which is a dumper. The names are the same as `TOOLS`' — this
/// only says which word her list gets beside them. Kept in step with `protect-stub`'s `watch.rs` the
/// same as that list is.
const TOOL_KINDS: (&[&str], &[&str]) = (
    &["x64dbg", "x32dbg", "x96dbg", "ollydbg", "windbg", "windbgx", "ntsd", "cdb", "kd", "dbgview", "immunitydebugger", "immdbg"],
    &["scylla", "extremedumper", "megadumper", "lordpe", "importrec", "imprec", "pe-sieve", "hollowshunter", "hollows_hunter", "processdump", "pmemdump", "memdump", "x64netdumper"],
);

/// How sure this is, 0-100: each kind is worth so much, and the whole is never over the top. The
/// weights are in the vault for the same reason the words are — they are the shape of the test.
fn score(sights: &[Sight]) -> i64 {
    let weights: Vec<i64> = crate::vault::sight_score().split_whitespace().filter_map(|w| w.parse().ok()).collect();
    let order = [
        crate::vault::sight_script(),
        crate::vault::sight_module(),
        crate::vault::sight_near(),
        crate::vault::sight_dates(),
        crate::vault::sight_tool(),
        crate::vault::sight_debugger(),
        crate::vault::sight_dumper(),
        crate::vault::sight_sandbox(),
        crate::vault::sight_parent(),
    ];
    let mut total = 0i64;
    for s in sights {
        if let Some(i) = order.iter().position(|k| *k == s.kind) {
            total += weights.get(i).copied().unwrap_or(10);
        }
    }
    total.clamp(0, 100)
}

/// How many files beside the install were made within a minute of one another since the game came
/// up — three or more is a dumper writing a copy out. Only the count goes anywhere; never a name,
/// never a time.
fn dates_around(home: &Path, since: SystemTime) -> Option<u32> {
    let what = std::fs::read_dir(home).ok()?;
    let mut times: Vec<i64> = Vec::new();
    for e in what.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        // one unreadable file is not the whole of the look's business: step over it
        let Ok(at) = meta.created().or_else(|_| meta.modified()) else { continue };
        if at < since {
            continue;
        }
        let Ok(ago) = at.duration_since(std::time::UNIX_EPOCH) else { continue };
        times.push(ago.as_secs() as i64);
    }
    if times.is_empty() {
        return Some(0);
    }
    times.sort_unstable();
    let mut best = 1u32;
    let mut run = 1u32;
    for w in times.windows(2) {
        if w[1] - w[0] <= 60 {
            run += 1;
            best = best.max(run);
        } else {
            run = 1;
        }
    }
    Some(best)
}

/// The note the server gets: what closed the game, the chain of parents above it, and the tree
/// under whatever else was found. Written to be read by a person at the staff panel.
fn report(procs: &[Proc], chain: &[Proc], game_pid: u32, why: &str, also: &[&Proc], near: Option<&Proc>) -> String {
    let mut out = String::new();
    out.push_str("Cursed Recode — the game was closed\n");
    out.push_str(&format!("why: {why}\n"));
    let up: Vec<String> = chain.iter().map(|p| format!("{} ({})", p.name, p.pid)).collect();
    out.push_str(&format!(
        "the game (pid {game_pid}) was under: {}\n",
        if up.is_empty() { "nothing this could see".into() } else { up.join(" <- ") }
    ));
    if let Some(p) = near {
        out.push_str(&format!("it sits under: {}\n", under(procs, p)));
    }
    if !also.is_empty() {
        out.push_str("elsewhere at this machine:\n");
        for p in also {
            out.push_str(&format!("  {}\n", under(procs, p)));
        }
    }
    out
}

/// A process and its parents, nearest first — the tree she asked for ("a terminal, and the
/// assistant's processes inside it").
fn under(procs: &[Proc], p: &Proc) -> String {
    let mut out = format!("{} ({})", p.name, p.pid);
    let mut pid = p.ppid;
    for _ in 0..8 {
        let Some(parent) = procs.iter().find(|x| x.pid == pid) else { break };
        out.push_str(&format!(" <- {} ({})", parent.name, parent.pid));
        if parent.ppid == 0 || parent.ppid == pid {
            break;
        }
        pid = parent.ppid;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loaders_language_goes_into_the_game_once() {
        let home = std::env::temp_dir().join(format!("recode-lang-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("options.txt"), "version:4556
lang:en_us
fov:0.0
").unwrap();
        settle_language(&home, "ru");
        let text = std::fs::read_to_string(home.join("options.txt")).unwrap();
        assert!(text.contains("lang:ru_ru") && !text.contains("lang:en_us") && text.contains("fov:0.0"), "{text}");
        assert!(home.join("cursed-recode").join("language-asked").is_file());
        // answered once: the game's own options rule from then on
        std::fs::write(home.join("options.txt"), "lang:en_us
").unwrap();
        settle_language(&home, "ru");
        assert_eq!(std::fs::read_to_string(home.join("options.txt")).unwrap(), "lang:en_us
");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_crash_is_told_from_an_exit() {
        assert!(fatal_status(0xC000_0005u32 as i32));
        assert!(!fatal_status(0));
        assert!(!fatal_status(1));
        assert!(!fatal_status(-1));
    }

    #[test]
    fn the_command_follows_the_manifest() {
        let dir = std::env::temp_dir().join(format!("recode-loader-cmd-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin/CursedRecode.exe"), b"").unwrap();
        let m = super::super::sync::parse(
            &serde_json::json!({
                "build": 4, "files": [],
                "launch": { "host": "bin/CursedRecode.exe", "jdk": "runtime", "main": "a.Main", "classpath": ["libraries/x.jar", "game/minecraft-26.3.jar"],
                            "jvm": ["-Dcursed.recode.native=${root}/bin/cursed_recode.dll"], "memory": { "least": 2048 },
                            "game": ["--gameDir", "${root}"] }
            })
            .to_string(),
        )
        .unwrap();
        let cmd = command(&dir, &m, 1024, "Lunuca", "t".repeat(64).as_str(), "s".repeat(64).as_str(), Some("CursedRecode-1"), Some("mc.funtime.su")).unwrap();
        let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().to_string()).collect();
        let root = dir.to_string_lossy().to_string();
        assert_eq!(args[0], "--jdk");
        assert!(args[1].ends_with("runtime"));
        assert_eq!(args[2], "-Xmx2048m", "never less than the build needs");
        assert!(args.contains(&format!("-Dcursed.recode.native={root}/bin/cursed_recode.dll")));
        assert!(args.iter().any(|a| a.contains("x.jar;") && a.contains("minecraft-26.3.jar")));
        assert_eq!(args[args.len() - 4..], ["--username", "Lunuca", "--quickPlayMultiplayer", "mc.funtime.su"].map(String::from));
        assert!(!args.iter().any(|a| a.contains(&"t".repeat(64))), "the pair's ticket is not on the command line");
        assert!(!args.iter().any(|a| a.contains(&"s".repeat(64))), "the start's ticket is not on the command line");
        let env: Vec<(String, Option<String>)> = cmd.get_envs().map(|(k, v)| (k.to_string_lossy().to_string(), v.map(|v| v.to_string_lossy().to_string()))).collect();
        assert!(env.contains(&("CURSED_TWIN".into(), Some("CursedRecode-1".into()))));
        assert!(env.contains(&("CURSED_BUILD".into(), Some("4".into()))));
        // the two tickets are two: the pair's opens the player's own copy, the start's is spent once
        assert!(env.contains(&("CURSED_TICKET".into(), Some("t".repeat(64)))));
        assert!(env.contains(&("CURSED_START".into(), Some("s".repeat(64)))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A start's own library goes exactly where the build says the game loads its sealed one, and the next
    /// start's takes that place (the old one moves aside as the build's own files do).
    #[test]
    fn the_stub_goes_where_the_build_names_its_library() {
        let dir = std::env::temp_dir().join(format!("recode-loader-stub-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let m = super::super::sync::parse(
            &serde_json::json!({
                "build": 4, "files": [],
                "launch": { "host": "bin/CursedRecode.exe", "jdk": "runtime", "main": "a.Main", "classpath": [],
                            "jvm": ["-Dcursed.recode.native=${root}/bin/cursed_recode.dll"], "memory": { "least": 2048 },
                            "game": [] }
            })
            .to_string(),
        )
        .unwrap();
        place_stub(&dir, &m, b"MZ-first").unwrap();
        let at = dir.join("bin").join("cursed_recode.dll");
        assert_eq!(std::fs::read(&at).unwrap(), b"MZ-first");
        place_stub(&dir, &m, b"MZ-second").unwrap();
        assert_eq!(std::fs::read(&at).unwrap(), b"MZ-second");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn pair_build() -> Manifest {
        super::super::sync::parse(&pair_build_json(4)).unwrap()
    }

    /// A pair is two files of one cut, and the two go in place together: the stub where the game loads its
    /// library, the sealed game beside it under the same name with `.bin`.
    #[test]
    fn the_pair_lands_as_two_of_one_cut() {
        let dir = std::env::temp_dir().join(format!("recode-loader-pair-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let m = pair_build();
        place_pair(&dir, &m, &super::super::api::Pair { stub: b"MZ-stub".to_vec(), bin: b"MZ-seal".to_vec() }).unwrap();
        assert_eq!(std::fs::read(dir.join("bin").join("cursed_recode.dll")).unwrap(), b"MZ-stub");
        assert_eq!(std::fs::read(dir.join("bin").join("cursed_recode.bin")).unwrap(), b"MZ-seal");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What is on disk is used again while the same build's pair of the same window is still there for the same
    /// account — and not one file of it alone: a half of a pair is no pair, and neither is one of another build,
    /// another window or another account.
    #[test]
    fn a_pair_is_kept_while_the_window_has_not_turned() {
        let dir = std::env::temp_dir().join(format!("recode-loader-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let m = pair_build();
        assert!(!pair_kept(&dir, &m, 3, 7), "nothing here yet");
        place_pair(&dir, &m, &super::super::api::Pair { stub: b"MZ-stub".to_vec(), bin: b"MZ-seal".to_vec() }).unwrap();
        assert!(!pair_kept(&dir, &m, 3, 7), "the two are here but not yet remembered");
        pair_remember(&dir, &m, 3, 7).unwrap();
        assert!(pair_kept(&dir, &m, 3, 7));
        assert!(!pair_kept(&dir, &m, 4, 7), "the window turned: another pair is cut");
        // one half alone is no pair
        std::fs::remove_file(dir.join("bin").join("cursed_recode.bin")).unwrap();
        assert!(!pair_kept(&dir, &m, 3, 7));
        place_pair(&dir, &m, &super::super::api::Pair { stub: b"MZ-stub".to_vec(), bin: b"MZ-seal".to_vec() }).unwrap();
        assert!(pair_kept(&dir, &m, 3, 7), "and back once both are there again");
        // another build's marker is not this one's
        let newer = super::super::sync::parse(&pair_build_json(5)).unwrap();
        assert!(!pair_kept(&dir, &newer, 3, 7));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Another account signed in on the same computer, in the same window of the same build, gets a pair of its own:
    /// the one on disk opens only under the ticket of the account it was cut for (19.10: hers, kept and handed to a
    /// test account's start, ended in «unsupported JNI version»). A marker from before accounts were in it is no one's.
    #[test]
    fn another_accounts_pair_is_not_kept() {
        let dir = std::env::temp_dir().join(format!("recode-loader-whose-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let m = pair_build();
        place_pair(&dir, &m, &super::super::api::Pair { stub: b"MZ-stub".to_vec(), bin: b"MZ-seal".to_vec() }).unwrap();
        pair_remember(&dir, &m, 3, 0).unwrap();
        assert!(pair_kept(&dir, &m, 3, 0));
        assert!(!pair_kept(&dir, &m, 3, 1246), "her pair is not the test account's");
        std::fs::write(pair_marker(&dir, &m).unwrap(), format!("{} 3", m.build)).unwrap();
        assert!(!pair_kept(&dir, &m, 3, 0), "a marker naming no account is asked for again");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn pair_build_json(build: i64) -> String {
        serde_json::json!({
            "build": build, "files": [],
            "launch": { "host": "bin/CursedRecode.exe", "jdk": "runtime", "main": "a.Main", "classpath": [],
                        "jvm": ["-Dcursed.recode.native=${root}/bin/cursed_recode.dll"], "memory": { "least": 2048 },
                        "game": [] }
        })
        .to_string()
    }

    /// A build that names no sealed library has nowhere for a stub to go, and says so rather than guessing.
    #[test]
    fn a_build_without_a_named_library_is_told_apart() {
        let m = super::super::sync::parse(
            &serde_json::json!({
                "build": 4, "files": [],
                "launch": { "host": "bin/CursedRecode.exe", "jdk": "runtime", "main": "a.Main", "classpath": [],
                            "jvm": [], "memory": { "least": 2048 }, "game": [] }
            })
            .to_string(),
        )
        .unwrap();
        assert!(library(&std::env::temp_dir(), &m).is_err());
    }

    // ---- who else is at the machine ----------------------------------------------------

    fn proc(pid: u32, ppid: u32, name: &str, line: &str) -> Proc {
        Proc { pid, ppid, name: name.to_lowercase(), line: line.to_lowercase() }
    }

    /// A dumper is a dumper whatever it was named to look like: the name it was given on disk, and
    /// the name it answers to inside a host process, both count. **Kept in step by hand with
    /// `protect-stub`'s `watch.rs`** — if those two ever disagree, a copy is taken apart on the
    /// side the other one does not cover.
    #[test]
    fn a_tool_is_named_however_it_was_started() {
        assert!(matches(&proc(1, 0, "x64dbg.exe", ""), TOOLS).is_some());
        assert!(matches(&proc(1, 0, "ollydbg.exe", ""), TOOLS).is_some());
        // a helper put in a folder of its own under another name
        assert!(matches(&proc(1, 0, "my-dumper-scylla.exe", ""), TOOLS).is_some());
        // and one that is only named in the command line of the interpreter carrying it
        assert!(matches(&proc(1, 0, "python.exe", r"c:\temp\tools\cheatengine.py --scan"), TOOLS).is_some());
        assert!(matches(&proc(1, 0, "node.exe", r"node c:\x\claude.js --help"), AGENTS).is_some());
        assert!(matches(&proc(1, 0, "node.exe", r"node c:\users\a\.npm\claude\cli.js"), AGENTS).is_some());
        // the game and the everyday processes are nobody's dumper
        assert!(matches(&proc(1, 0, "java.exe", "-Xmx4g a.Main"), TOOLS).is_none());
        assert!(matches(&proc(1, 0, "chrome.exe", ""), TOOLS).is_none());
        assert!(matches(&proc(1, 0, "explorer.exe", ""), AGENTS).is_none());
    }

    /// A player's own folder called like a tool is not the tool. Only the file a command runs and
    /// the folder that file sits in count — above them is just a path, and closing a game over one
    /// would close it for anyone whose user name happens to be a word on the list.
    #[test]
    fn a_folder_with_a_tools_name_is_not_the_tool() {
        assert!(matches(&proc(1, 0, "java.exe", r"c:\users\claude\game\java.exe"), AGENTS).is_none());
        assert!(matches(&proc(1, 0, "java.exe", r"c:\users\x64dbg\desktop\game\java.exe"), TOOLS).is_none());
        assert!(matches(&proc(1, 0, "java.exe", r"c:\games\cheatengine\bin\launcher.exe"), TOOLS).is_none());
    }

    /// The walk up the parents stops at the player's own shell: above it sits what they were doing
    /// before the command, not whoever is driving the game. This is also what lets the game be
    /// tested here at all — every stand is started from a terminal.
    #[test]
    fn the_walk_up_stops_at_the_players_own_shell() {
        let procs = vec![
            proc(10, 20, "java.exe", ""),
            proc(20, 30, "CursedRecode.exe", ""),
            proc(30, 40, "bash.exe", ""),
            proc(40, 0, "claude.exe", ""),
        ];
        let chain = chain_of(&procs, 10);
        let names: Vec<&str> = chain.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["java.exe", "cursedrecode.exe"], "{names:?}: the shell itself is not in the list");
    }

    /// The game, driven out of a prompt of an assistant, is closed and said so: the assistant is
    /// in the chain of parents, above nothing the player typed into.
    #[test]
    fn an_assistant_that_started_the_game_is_caught() {
        let procs = vec![
            proc(10, 20, "java.exe", ""),
            proc(20, 30, "CursedRecode.exe", ""),
            proc(30, 40, "node.exe", r"node c:\users\a\.npm\claude\cli.js"),
            proc(40, 50, "claude.exe", ""),
            proc(50, 0, "bash.exe", ""),
        ];
        let found = judge(&procs, 10, std::time::UNIX_EPOCH, "1", 1).expect("an assistant above the game must close it");
        assert!(found.body.contains("started from an assistant: claude (pid 30)"), "{}", found.body);
        // the note carries the tree she asked for
        assert!(found.body.contains("node.exe (30) <- claude.exe (40)"), "{}", found.body);
    }

    /// An assistant on the second monitor is not reading the game's memory. A chat window a player
    /// happens to have open closes nothing; it is a look over the machine, not a verdict on the room.
    #[test]
    fn this_computer_is_not_a_reason_to_close_the_game() {
        let procs = vec![
            proc(10, 20, "java.exe", ""),
            proc(20, 30, "CursedRecode.exe", ""),
            proc(30, 0, "explorer.exe", ""),
            proc(40, 0, "claude.exe", ""),
            proc(50, 0, "chrome.exe", ""),
        ];
        assert!(judge(&procs, 10, std::time::UNIX_EPOCH, "1", 1).is_none(), "a plain machine is nobody's dumper");
    }

    /// A dumper anywhere on the machine closes the game — it does not have to be its parent.
    #[test]
    fn a_dumper_anywhere_is_enough() {
        let procs = vec![
            proc(10, 20, "java.exe", ""),
            proc(20, 30, "CursedRecode.exe", ""),
            proc(30, 0, "explorer.exe", ""),
            proc(40, 0, "x64dbg.exe", ""),
        ];
        let found = judge(&procs, 10, std::time::UNIX_EPOCH, "1", 1).expect("a dumper anywhere closes the game");
        assert!(found.body.contains("takes a process apart"), "{}", found.body);
    }

    /// A tool named for what it does to a process's memory, opened while the game runs, is a hand
    /// on our memory — a `touch`, and a ban at once. Not "a tool was running", and not two strikes.
    /// This is the whole point of the 18.10 word: «выдавать бан должно если в память лезут или
    /// пытаются».
    #[test]
    fn a_memory_tool_that_arrived_is_a_touch_not_two_strikes() {
        let procs = vec![
            proc(10, 20, "java.exe", ""),
            proc(20, 30, "CursedRecode.exe", ""),
            proc(30, 0, "cheatengine.exe", ""),
        ];
        let found = judge(&procs, 10, std::time::UNIX_EPOCH, "1", 1).expect("a memory tool that arrived closes it");
        assert_eq!(source_of(&found.body), crate::vault::touch_source(), "{}", found.body);
        assert!(found.body.contains("carries cheatengine"), "{}", found.body);
        // and it is one of the six, by the vault's own list — never a word this end made up
        let line = found.body.lines().find(|l| l.starts_with(crate::vault::touch_note())).expect("a touch line");
        let word = line[crate::vault::touch_note().len()..].trim();
        assert!(crate::vault::touch_kinds().split('|').any(|k| k == word), "{word}");
    }

    /// The shape of a hand is a question of names alone. What is **not** a hand here never becomes
    /// one when the caller asks whether it arrived — the false-positive guard lives in this
    /// function and nowhere else.
    #[test]
    fn only_a_poke_word_in_the_own_file_name_is_a_hand() {
        // the cause is a poke word in the file's own name
        let (kind, why) = a_hand_on_us("cheatengine.exe", "").expect("cheatengine is named for a memory");
        assert_eq!(kind, KIND_SCRIPT);
        assert!(why.contains("carries cheatengine"), "{why}");
        // a script whose own name carries one
        let (_, why) = a_hand_on_us("python.exe", r"python c:\x\recode_ripper.py").expect("ripper is a poke");
        assert!(why.contains("recode_ripper.py"), "{why}");
        // our name beside a poke strengthens the reason and is never the cause
        assert!(why.contains("aimed at us"), "{why}");
        assert!(why.contains("ripper"), "{why}");
        let (_, why) = a_hand_on_us("python.exe", r"python c:\x\writeprocessmemory.py").expect("the API name is a poke");
        assert!(why.contains("writeprocessmemory"), "{why}");
        // our own name alone is a player's music script and is silent
        assert!(a_hand_on_us("python.exe", r"python c:\x\recode_music.py").is_none(), "our name is never the cause");
        // a package being installed is not a file being run
        assert!(a_hand_on_us("pip.exe", "pip install pymem").is_none(), "installing a library is nobody's hand");
        // a folder above a script is a path, and a path is the player's
        assert!(a_hand_on_us("python.exe", r"python c:\pymem\player.py").is_none(), "a folder is not a name");
        // a debugger is the old path — open and not attached is nobody's hand
        assert!(a_hand_on_us("x64dbg.exe", "").is_none(), "a debugger keeps the old path");
        // and so is a dumper's, whose name carries no poke word of the closed list
        assert!(a_hand_on_us("scylla.exe", "").is_none(), "scylla is a tool, not a poke");
    }

    /// `recode_music.py` is the case that decides whether a paying player loses the game over their
    /// own music script. It must never fire — with our name, without a poke word, on any path.
    #[test]
    fn a_players_own_script_shaped_like_ours_is_silent() {
        for line in [
            r"python c:\x\recode_music.py",
            r"python c:\recode\music.py",
            r"python recode.py",
            r"python c:\x\recode_helper.py",
        ] {
            assert!(a_hand_on_us("python.exe", line).is_none(), "must be silent: {line}");
        }
    }

    /// The other half of the rule: a tool that was **already** on the machine when the game came up
    /// is presence, and presence is never a touch. The player may have had it open since boot for
    /// something else. It keeps the old path — the game goes and the report is the lesser note.
    ///
    /// This uses our own pid, which really was created before `now`, so `started_after` has
    /// something true to say about it instead of the "cannot tell, assume it arrived" default a
    /// made-up pid gets.
    #[test]
    fn a_memory_tool_already_on_the_machine_is_not_a_touch() {
        let me = std::process::id();
        let procs = vec![proc(me, 0, "cheatengine.exe", "")];
        let found = judge(&procs, 999_999, std::time::SystemTime::now(), "1", 1)
            .expect("it is still a tool and still closes the game");
        assert_eq!(source_of(&found.body), "protect", "presence is the old path: {}", found.body);
        assert!(found.body.contains("takes a process apart"), "{}", found.body);
    }

    /// Arrival is the line, and a look that already holds the process does not get to throw that
    /// fact away because the clock would not answer. A pid in the baseline is presence; only a
    /// brand-new pid, or one that positively began after us (the number was reused), is arrival.
    #[test]
    fn a_baseline_process_is_never_arrival_on_a_clock_that_will_not_answer() {
        let me = std::process::id();
        // not in the baseline: a new pid is arrival, whatever the clock says
        assert!(arrived_since(me, &[], SystemTime::now()));
        // in the baseline, and the clock says it began before us: presence
        assert!(!arrived_since(me, &[me], SystemTime::now()));
        // in the baseline, and the clock will not answer (a made-up pid): silence, not arrival
        let ghost = 0xFFFF_FFF0u32;
        assert!(!arrived_since(ghost, &[ghost], SystemTime::now()), "a clock that cannot answer does not overwrite the baseline");
        // a made-up pid the baseline never saw is still arrival (the fast pass's aggressive side)
        assert!(arrived_since(ghost, &[], SystemTime::now()));
        // pid reuse: the baseline held the number, and this one positively began after the game
        let mut up = Command::new(r"C:\Windows\System32\ping.exe");
        up.args(["-n", "30", "127.0.0.1"]);
        up.stdout(std::process::Stdio::null());
        up.stderr(std::process::Stdio::null());
        let mut child = up.spawn().expect("the stand-in whose number is reused");
        let pid = child.id();
        let ago = SystemTime::now() - std::time::Duration::from_secs(3600);
        assert!(arrived_since(pid, &[pid], ago), "a positive 'began after us' is arrival even beside the baseline");
        let _ = child.kill();
        let _ = child.wait();
    }

    /// The note the game's library leaves beside itself is taken and read once — an old note from
    /// an earlier run must not become this one's story.
    #[test]
    fn a_note_beside_the_library_is_read_once() {
        let dir = std::env::temp_dir().join(format!("recode-caught-note-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let m = super::super::sync::parse(
            &serde_json::json!({
                "build": 7, "files": [],
                "launch": { "host": "bin/CursedRecode.exe", "jdk": "runtime", "main": "a.Main", "classpath": [],
                            "jvm": [], "memory": { "least": 2048 }, "game": [] }
            })
            .to_string(),
        )
        .unwrap();
        assert!(the_note(&dir, &m).is_none(), "nothing left, nothing said");
        std::fs::write(dir.join("bin").join("caught.txt"), "why: a debugger is on the game\n").unwrap();
        let note = the_note(&dir, &m).expect("the note goes to the server");
        assert!(note.contains("build 7") && note.contains("a debugger is on the game"), "{note}");
        assert!(!dir.join("bin").join("caught.txt").exists(), "read once");
        assert!(the_note(&dir, &m).is_none(), "and then gone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The code the game's own library leaves when it closes the game is the one its guard takes —
    /// the same number in `protect-stub`'s `guard::EXIT` ("CURS"). Not an NT status, so it is never
    /// read as a crash of the game's own.
    #[test]
    fn the_exit_the_guard_leaves_is_the_one_the_game_says() {
        assert_eq!(GUARD_EXIT, 0x4355_5253, "protect-stub's guard::EXIT, \"CURS\"");
        assert!(!fatal_status(GUARD_EXIT), "it is not a crash of the game's own");
    }

    /// The machine is really seen: the snapshot has us in it, and a command line is really read
    /// (ntdll's `ProcessCommandLineInformation`, not a guess). Without this the whole look-over
    /// would pass its own tests over an empty list and see nothing at a player's machine either.
    #[test]
    fn the_machine_is_actually_seen() {
        let me = std::process::id();
        let procs = snapshot();
        assert!(!procs.is_empty(), "a snapshot of this machine came back empty");
        assert!(procs.iter().any(|p| p.pid == me), "this very process is in the snapshot");
        let line = command_line(me);
        assert!(!line.is_empty(), "a command line is really read (ntdll's class 60)");
        assert!(line.contains("cursed") || line.contains(".exe"), "{line}");
        // and the chain from here reaches this test's own parents
        let chain = chain_of(&procs, me);
        assert!(!chain.is_empty() || is_shell(&procs.iter().find(|p| p.pid == me).map(|p| p.name.as_str()).unwrap_or("")), "something stands above this process");
    }

    /// This machine is nobody's dumper and nobody's reason to close a game — the same machine the
    /// game is developed and tested on, with a terminal and an assistant open the whole time. If
    /// this ever fails, a player is being closed over something of ours.
    #[test]
    fn this_machine_is_not_a_reason_to_close_the_game() {
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        let me = std::process::id();
        let procs = snapshot();
        if let Some(f) = judge(&procs, me, std::time::UNIX_EPOCH, "1", 1) {
            panic!("a game on this machine would be closed: {}", f.body);
        }
    }

    // ---- a look at the room, written down for her list -------------------------------
    // The same room `judge` closes the game over, only written down: report only. Never a path,
    // never a user name, never the player told a word.

    fn room(what: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("recode-a-look-{what}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A look taken after the install and before the game was ever up: everything beside it was
    /// written in one go long ago, and the dates say nothing.
    fn quiet() -> SystemTime {
        SystemTime::now() + std::time::Duration::from_secs(3600)
    }

    /// A plain room is nobody's sight: the game, the loader that started it, a browser, a shell.
    /// Nothing is written down and the score is nothing — the player plays on and nobody is told.
    #[test]
    fn a_plain_room_is_nobodys_sight() {
        let dir = room("plain");
        let procs = vec![
            proc(10, 20, "CursedRecode.exe", ""),
            proc(20, 30, "recode_loader.exe", ""),
            proc(30, 0, "explorer.exe", ""),
            proc(40, 0, "chrome.exe", ""),
            proc(50, 0, "javaw.exe", "-Xmx4g a.Main"),
        ];
        let (score, sights) = look_at_the_room(&procs, &dir, quiet(), 10);
        assert!(sights.is_empty(), "{sights:?}");
        assert_eq!(score, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Our own are nobody's sight: the loader the game is under (in the build, and as the site
    /// gives it out), the game itself, the window before it, the library's own shape, the Java
    /// inside, and the test binary's own name. They carry our names every day and every day must
    /// say nothing — a *third* thing wearing one of these names is somebody making a copy of us.
    #[test]
    fn our_own_are_nobodys_sight() {
        let dir = room("ours");
        let procs = vec![
            proc(10, 20, "CursedRecode.exe", ""),
            proc(20, 30, "recode_loader.exe", ""),
            proc(21, 30, "CursedRecodeLoader.exe", ""),
            proc(22, 30, "cursed_twin.exe", ""),
            proc(23, 30, "cursed_recode-abc123.exe", ""),
            proc(40, 0, "javaw.exe", ""),
            proc(50, 0, "java.exe", r"-Dcursed.recode.native=c:\g\bin\cursed_recode.dll"),
        ];
        let (score, sights) = look_at_the_room(&procs, &dir, quiet(), 10);
        assert!(sights.is_empty(), "{sights:?}");
        assert_eq!(score, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A script that carries our names is a line: `python …\recode_guard.py` is somebody driving a
    /// copy of us. What her list gets is the script's own name — never the folder it sits in, which
    /// is a path, and a path is not ours to read.
    #[test]
    fn a_script_carrying_our_names_is_a_line() {
        let dir = room("script");
        let procs = vec![
            proc(10, 20, "CursedRecode.exe", ""),
            proc(20, 0, "explorer.exe", ""),
            proc(30, 0, "python.exe", r"python c:\users\bob\tools\recode_guard.py --scan"),
            proc(40, 0, "python.exe", r"python c:\users\bob\downloads\cursed_recode_dump.py"),
        ];
        let (_, sights) = look_at_the_room(&procs, &dir, quiet(), 10);
        let kinds: Vec<&str> = sights.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [crate::vault::sight_script(), crate::vault::sight_script()], "{sights:?}");
        assert_eq!(crate::vault::sight_script(), "script-ours", "the word the server's closed list has");
        let names: Vec<&str> = sights.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["recode_guard.py", "cursed_recode_dump.py"], "{names:?}");
        assert!(names.iter().all(|n| !n.contains('\\') && !n.contains('/')), "a path is never written down");
        // a folder of ours above somebody else's script is just a path
        assert_eq!(carrying(r"python c:\recode\main.py --go", &["recode", "cursed_recode.dll"]), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A foreign process wearing our name is a line of its own kind: not ours, and named after us.
    #[test]
    fn a_foreigner_wearing_our_name_is_a_line() {
        let dir = room("foreign");
        let procs = vec![
            proc(10, 20, "CursedRecode.exe", ""),
            proc(30, 0, "recode_hack.exe", ""),
            proc(40, 0, "cursed_recode_helper.exe", ""),
        ];
        let (_, sights) = look_at_the_room(&procs, &dir, quiet(), 10);
        assert_eq!(sights.len(), 2, "{sights:?}");
        assert_eq!(crate::vault::sight_module(), "module-ours");
        assert_eq!(sights[0].kind, crate::vault::sight_module());
        assert_eq!(sights[0].name, "recode_hack.exe");
        assert_eq!(sights[1].name, "cursed_recode_helper.exe");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A tool is sorted under the kind it is: a debugger is not a dumper is not a reassembler, and
    /// her list says which. The weights are what the score is counted with.
    #[test]
    fn a_tool_is_sorted_under_its_own_kind() {
        let dir = room("tools");
        let procs = vec![
            proc(10, 20, "CursedRecode.exe", ""),
            proc(30, 0, "x64dbg.exe", ""),
            proc(40, 0, "scylla.exe", ""),
            proc(50, 0, "ida64.exe", ""),
        ];
        let (score, sights) = look_at_the_room(&procs, &dir, quiet(), 10);
        let kinds: Vec<&str> = sights.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [crate::vault::sight_debugger(), crate::vault::sight_dumper(), crate::vault::sight_tool()],
            "{sights:?}"
        );
        assert_eq!(crate::vault::sight_debugger(), "debugger");
        assert_eq!(crate::vault::sight_dumper(), "dumper");
        assert_eq!(crate::vault::sight_tool(), "tool");
        assert_eq!(score, 34 + 34 + 28);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Something of our shape beside the install is noticed by its name alone — "dump" and the
    /// like. Never its path: a path carries the player's name and that is not ours to read.
    #[test]
    fn a_file_beside_the_install_is_noticed_by_name_only() {
        let dir = room("near");
        std::fs::write(dir.join("cursed_dump.exe"), b"MZ").unwrap();
        std::fs::write(dir.join("ok.txt"), b"nothing").unwrap();
        let (_, sights) = look_at_the_room(&[proc(10, 0, "CursedRecode.exe", "")], &dir, quiet(), 10);
        assert_eq!(sights.len(), 1, "{sights:?}");
        assert_eq!(crate::vault::sight_near(), "file-near");
        assert_eq!(sights[0].kind, crate::vault::sight_near());
        assert_eq!(sights[0].name, "cursed_dump.exe");
        assert!(!sights[0].name.contains('\\') && !sights[0].name.contains('/'));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Dates gathering while the game was up are said — three files written out together is what a
    /// dumper leaves. The install's own writing is nobody's finding: those all came before the game
    /// was ever up, and `since` is what keeps them out.
    #[test]
    fn dates_that_gather_while_the_game_runs_are_said() {
        let dir = room("dates");
        std::fs::write(dir.join("a.bin"), b"1").unwrap();
        std::fs::write(dir.join("b.bin"), b"2").unwrap();
        std::fs::write(dir.join("c.bin"), b"3").unwrap();
        // the game came up a moment ago: all three are this game's own while
        let since = SystemTime::now() - std::time::Duration::from_secs(2);
        let (_, sights) = look_at_the_room(&[proc(10, 0, "CursedRecode.exe", "")], &dir, since, 10);
        let dates: Vec<&Sight> = sights.iter().filter(|s| s.kind == crate::vault::sight_dates()).collect();
        assert_eq!(dates.len(), 1, "{sights:?}");
        assert_eq!(crate::vault::sight_dates(), "date-cluster");
        assert_eq!(dates[0].note, "3 together");
        // and the same three, written by the install before the game was ever up, are nobody's
        let (score, sights) = look_at_the_room(&[proc(10, 0, "CursedRecode.exe", "")], &dir, quiet(), 10);
        assert!(sights.is_empty(), "{sights:?}");
        assert_eq!(score, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The date-cluster look is a line on her list and nothing more. The game writes `options.txt`,
    /// `usercache.json` and `servers.dat` constantly — three of those inside a minute is a Tuesday,
    /// not a dumper. It never closes the game and never becomes a touch (17.10: report only).
    #[test]
    fn a_date_cluster_is_report_only_and_never_a_ban() {
        let dir = room("dates-ban");
        std::fs::write(dir.join("options.txt"), b"lang:en_us").unwrap();
        std::fs::write(dir.join("usercache.json"), b"[]").unwrap();
        std::fs::write(dir.join("servers.dat"), b"").unwrap();
        let since = SystemTime::now() - std::time::Duration::from_secs(2);
        let (_score, sights) = look_at_the_room(&[proc(10, 0, "CursedRecode.exe", "")], &dir, since, 10);
        assert!(sights.iter().any(|s| s.kind == crate::vault::sight_dates()), "the look says so: {sights:?}");
        // and the same room closes nothing: `judge` has no date-cluster path at all
        assert!(
            judge(&[proc(10, 0, "CursedRecode.exe", "")], 10, since, "1", 1).is_none(),
            "a day's own writing never closes a game"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A parent nothing a player would have started the game from is said. The loader is ours and
    /// is the parent of every game it starts — that one is nobody's line.
    #[test]
    fn a_parent_no_player_would_have_is_said_and_the_loader_is_not() {
        let dir = room("parent");
        let procs = vec![
            // the game under the loader, as it always is
            proc(10, 20, "CursedRecode.exe", ""),
            proc(20, 30, "recode_loader.exe", ""),
            proc(30, 0, "explorer.exe", ""),
            // and one started from something no player would have open
            proc(50, 40, "CursedRecode.exe", ""),
            proc(40, 60, "autohotkey.exe", ""),
            proc(60, 0, "explorer.exe", ""),
        ];
        assert_eq!(crate::vault::sight_parent(), "parent");
        let (_, sights) = look_at_the_room(&procs, &dir, quiet(), 10);
        let parents: Vec<&str> = sights.iter().filter(|s| s.kind == crate::vault::sight_parent()).map(|s| s.name.as_str()).collect();
        assert!(parents.is_empty(), "{sights:?}: the loader is ours, and the shell stops the walk");
        let (_, sights) = look_at_the_room(&procs, &dir, quiet(), 50);
        let parents: Vec<&str> = sights.iter().filter(|s| s.kind == crate::vault::sight_parent()).map(|s| s.name.as_str()).collect();
        assert_eq!(parents, ["autohotkey.exe"], "{sights:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The score is what the look is sure of, 0-100: each kind is worth its weight, they add up,
    /// and the whole is never over the top. The weights live in the vault so a `strings` pass does
    /// not sit beside the test that walks in by them.
    #[test]
    fn the_score_adds_up_and_is_capped() {
        let one = |kind: &'static str| Sight { kind, name: String::new(), note: String::new() };
        assert_eq!(score(&[]), 0);
        assert_eq!(score(&[one(crate::vault::sight_script())]), 30);
        assert_eq!(score(&[one(crate::vault::sight_module())]), 12);
        assert_eq!(score(&[one(crate::vault::sight_near())]), 18);
        assert_eq!(score(&[one(crate::vault::sight_dates())]), 10);
        assert_eq!(score(&[one(crate::vault::sight_tool())]), 28);
        assert_eq!(score(&[one(crate::vault::sight_debugger())]), 34);
        assert_eq!(score(&[one(crate::vault::sight_dumper())]), 34);
        assert_eq!(score(&[one(crate::vault::sight_sandbox())]), 22);
        assert_eq!(score(&[one(crate::vault::sight_parent())]), 16);
        assert_eq!(score(&[one(crate::vault::sight_script()), one(crate::vault::sight_dumper())]), 64);
        let flood: Vec<Sight> = (0..20).map(|_| one(crate::vault::sight_debugger())).collect();
        assert_eq!(score(&flood), 100);
    }

    /// The kinds a look may carry are the ones the two sides agreed on — the same words the server
    /// keeps its closed list of (`recode.rs`, `SIGHTS`). If these ever drift, a look is written
    /// down and then thrown away at the door.
    #[test]
    fn the_kinds_are_the_servers() {
        for (word, said) in [
            (crate::vault::sight_script(), "script-ours"),
            (crate::vault::sight_module(), "module-ours"),
            (crate::vault::sight_near(), "file-near"),
            (crate::vault::sight_dates(), "date-cluster"),
            (crate::vault::sight_tool(), "tool"),
            (crate::vault::sight_debugger(), "debugger"),
            (crate::vault::sight_dumper(), "dumper"),
            (crate::vault::sight_sandbox(), "sandbox"),
            (crate::vault::sight_parent(), "parent"),
        ] {
            assert_eq!(word, said, "the vault's word is the list's");
        }
    }

    // ---- the look-over while a game runs, against a real machine ----------------------
    // These start real processes, one of them under a dumper's name, so they share a lock: a look
    // over the machine taken while another test's dummy is up would be a true finding of a dummy.

    static LIVE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn manifest() -> Manifest {
        super::super::sync::parse(
            &serde_json::json!({
                "build": 9, "version": "26.3",
                "files": [],
                "launch": { "host": "bin/CursedRecode.exe", "jdk": "runtime", "main": "a.Main", "classpath": [],
                            "jvm": [], "memory": { "least": 2048 }, "game": [] }
            })
            .to_string(),
        )
        .unwrap()
    }

    fn home() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("recode-until-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        dir
    }

    /// A stand for a running game. Its own words stay out of the test's — a stand-in that talks
    /// (a ping) would fill the output with what nobody asked for.
    fn spawn(what: &str, args: &[&str]) -> Running {
        let mut cmd = Command::new(what);
        cmd.args(args);
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
        let child = cmd.spawn().unwrap_or_else(|e| panic!("{what}: {e}"));
        Running { child, started: SystemTime::now() }
    }

    /// A game that goes on its own is a game's own going: its exit code, no note, and nothing said
    /// about this machine.
    #[test]
    fn a_game_that_goes_on_its_own_is_nobodys_finding() {
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        let dir = home();
        let m = manifest();
        let mut running = spawn("cmd", &["/c", "exit", "0"]);
        let (code, report, source) = until_it_goes(&mut running, &dir, &m, None, None);
        assert_eq!(code, Some(0));
        assert_eq!(source, "game");
        assert!(report.is_none(), "a plain going is nobody's report: {report:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The code the game's own library leaves on the way out is read as that, and the note it left
    /// beside itself goes with it — not as a crash of the game's own.
    #[test]
    fn the_guards_own_exit_carries_its_note() {
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        let dir = home();
        let m = manifest();
        std::fs::write(dir.join("bin").join("caught.txt"), "why: a debugger is on the game\n").unwrap();
        // the very code `protect-stub`'s `guard::EXIT` leaves ("CURS") — taken from the constant
        // itself, so the stand cannot drift from it
        let code_arg = GUARD_EXIT.to_string();
        let mut running = spawn("cmd", &["/c", "exit", &code_arg]);
        let (code, report, source) = until_it_goes(&mut running, &dir, &m, None, None);
        assert_eq!(code, Some(GUARD_EXIT));
        assert_eq!(source, "protect");
        let body = report.expect("the note goes with it");
        assert!(body.contains("a debugger is on the game"), "{body}");
        assert!(body.contains("build 9"), "{body}");
        assert!(!dir.join("bin").join("caught.txt").exists(), "read once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A dumper opened while the game is already up closes it. The look-over is not once at the
    /// door: it happens again while the game runs, which is the only time a real one appears.
    #[test]
    fn a_dumper_opened_while_the_game_runs_closes_it() {
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        let dir = home();
        let m = manifest();
        let mut running = spawn("cmd", &["/c", "ping", "-n", "30", "127.0.0.1"]);
        let pid = running.child.id();
        // the dumper is opened a little later, the way a player opens one
        let dummy = dir.join("x64dbg.exe");
        std::fs::copy(r"C:\Windows\System32\ping.exe", &dummy).expect("the stand-in for a dumper");
        let later = {
            let dummy = dummy.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(2500));
                let mut cmd = Command::new(&dummy);
                cmd.args(["-n", "60", "127.0.0.1"]);
                cmd.stdout(std::process::Stdio::null());
                cmd.stderr(std::process::Stdio::null());
                cmd.spawn()
            })
        };
        let (code, report, source) = until_it_goes(&mut running, &dir, &m, None, None);
        if let Ok(Ok(mut dummy)) = later.join() {
            let _ = dummy.kill();
            let _ = dummy.wait();
        }
        assert_eq!(source, "protect", "a dumper is this machine's finding, not the game's own");
        assert!(code.is_none() || code != Some(0), "it did not go on its own: {code:?}");
        let body = report.expect("the note goes to the server");
        assert!(body.contains("takes a process apart"), "{body}");
        assert!(body.contains("x64dbg"), "{body}");
        assert!(body.contains(&format!("the game (pid {pid})")), "{body}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The look is really taken at this machine: every kind is one of the nine, the score is in
    /// 0-100, and nothing written down is a path or one of our own. This is the machine the game is
    /// developed and tested on, with a terminal and an assistant open the whole time — if this ever
    /// fails, a player's day is being written up over something of ours.
    #[test]
    fn this_machine_is_a_quiet_room() {
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        let dir = room("live");
        let me = std::process::id();
        // the install's own writing is nobody's finding: the dates stay quiet
        let (score, sights) = a_look(&dir, me, quiet());
        let nine = [
            crate::vault::sight_script(),
            crate::vault::sight_module(),
            crate::vault::sight_near(),
            crate::vault::sight_dates(),
            crate::vault::sight_tool(),
            crate::vault::sight_debugger(),
            crate::vault::sight_dumper(),
            crate::vault::sight_sandbox(),
            crate::vault::sight_parent(),
        ];
        assert!((0..=100).contains(&score), "score {score}");
        for s in &sights {
            assert!(nine.contains(&s.kind), "kind {} is not one of the nine", s.kind);
            assert!(!s.name.contains('\\') && !s.name.contains('/'), "a path: {}", s.name);
            assert!(!is_ours(&s.name), "our own is nobody's sight: {}", s.name);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Our name on our own process is nobody's sight; the same name on a process that is not ours
    /// is a line. The difference is the whole of what this look says about us — and it is taken at
    /// a real machine, over two real processes standing beside a real game.
    #[test]
    fn our_name_on_a_foreigner_is_a_line_and_on_ours_is_not() {
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        let dir = room("wear");
        // a stand-in under our own name (the shape of the test binary: `cursed_recode-<hash>`)
        let ours = dir.join("cursed_recode-stand.exe");
        std::fs::copy(r"C:\Windows\System32\ping.exe", &ours).expect("the stand-in that is ours");
        // and one wearing our name that is nobody's
        let foreign = dir.join("recode_hack.exe");
        std::fs::copy(r"C:\Windows\System32\ping.exe", &foreign).expect("the stand-in that is a foreigner");
        let mut up = Vec::new();
        for stand in [&ours, &foreign] {
            let mut cmd = Command::new(stand);
            cmd.args(["-n", "30", "127.0.0.1"]);
            cmd.stdout(std::process::Stdio::null());
            cmd.stderr(std::process::Stdio::null());
            up.push(cmd.spawn().unwrap_or_else(|e| panic!("{}: {e}", stand.display())));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        let (_, sights) = a_look(&dir, std::process::id(), quiet());
        for mut stand in up {
            let _ = stand.kill();
            let _ = stand.wait();
        }
        let names: Vec<&str> = sights.iter().map(|s| s.name.as_str()).collect();
        assert!(!names.iter().any(|n| n.contains("cursed_recode-stand")), "{sights:?}: ours is nobody's sight");
        assert!(names.iter().any(|n| n.contains("recode_hack")), "{sights:?}: a foreigner wearing our name is said");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- a hand on the memory: the note, the route, and what is never a finding ----------
    // The contract's four lines, the one word by index, and the false-positive guard that keeps
    // a paying player's ordinary day out of a ban.

    /// The note carries the loader's prefix (the MAC is over the whole of it) and the four lines
    /// of the body — and the `touch:` word is one of the six, taken by index from the one list
    /// both ends keep. No path, no user name, never a second code line.
    #[test]
    fn a_touch_note_carries_one_of_the_six_words_under_the_prefix() {
        let body = touch_body("0.1.0-beta.3", 3, KIND_HOOK, "an IAT slot was moved");
        assert!(body.starts_with("Cursed Recode 0.1.0-beta.3 (build 3)\n"), "the prefix is part of what the MAC signs: {body}");
        assert!(body.contains("Cursed Recode — the game was closed\n"), "{body}");
        assert!(body.contains("build: 3\n"), "{body}");
        assert!(body.contains("touch: hook\n"), "{body}");
        assert!(body.contains("why: an IAT slot was moved\n"), "{body}");
        assert!(!body.contains('\\') && !body.contains('/'), "a path is never in a note: {body}");
        // every index of the closed list is one of the six words, and the note takes that word
        let kinds = crate::vault::touch_kinds();
        assert_eq!(kinds, "patch|hook|break|write|inject|script", "the two ends' one list");
        for index in 0..6 {
            let word = kind_word(index);
            assert!(kinds.split('|').any(|w| w == word), "index {index} is {word:?} of {kinds}");
            let note = touch_body("1", 1, index, "why");
            assert!(note.contains(&format!("touch: {word}\n")), "{note}");
        }
    }

    /// The route is one line and the word after `touch: ` is not looked at here — the server
    /// checks it against the closed list. `touch:` with no space is not a touch line. A note
    /// without one falls through to the old `protect` path (two strikes) and is never a ban.
    #[test]
    fn a_touch_line_routes_to_touch_and_nothing_else_does() {
        assert_eq!(source_of(&touch_body("1", 1, KIND_HOOK, "a head moved")), "touch");
        assert_eq!(source_of("Cursed Recode — the game was closed\nbuild: 1\nwhy: a debugger is on the game\n"), "protect");
        assert_eq!(source_of("build: 1\ntouch:hook\n"), "protect", "touch: with no space is not a touch line");
        assert_eq!(source_of("build: 1\ntouch: nonsense\n"), "touch", "the word is the server's to check");
        assert_eq!(source_of(""), "protect");
    }

    /// A script is named for our memory in its own file name, never for our name alone and never
    /// for a word that only appears in an argument. `python recode_ripper.py` is a hand on our
    /// memory; `pip install pymem` is a player installing a package; `recode_music.py` is a
    /// music script. The whole command line is never matched.
    #[test]
    fn a_script_is_named_for_our_memory_not_for_our_name() {
        let (poke, file, also_us) = poking(r"python c:\tools\recode_ripper.py --scan").expect("ripper is in the poke list");
        assert_eq!(poke, "ripper");
        assert_eq!(file, "recode_ripper.py");
        assert!(also_us, "our name beside a poke strengthens the reason");
        // our name alone is silent
        assert!(poking(r"python c:\music\recode_music.py").is_none(), "a music script is nobody's hand");
        assert!(poking(r"python c:\tools\recode_helper.py").is_none());
        // a package install is a player installing a package — the word is an argument
        assert!(poking(r"pip install pymem").is_none());
        assert!(poking(r"pip install cheatengine").is_none());
        assert!(poking(r"python -m pip install pymem").is_none());
        // a poke word in a plain argument is not a script name
        assert!(poking(r"echo pymem > notes.txt").is_none());
        // a script named for a poke without our name still fires — the poke is the cause
        let (poke, file, also_us) = poking(r"python c:\temp\pymem_scan.py").expect("pymem is in the poke list");
        assert_eq!(poke, "pymem");
        assert_eq!(file, "pymem_scan.py");
        assert!(!also_us);
        // and one of the teardown words the list grew — as the file's own stem
        assert!(poking(r"node unpacker.js").is_some());
        assert_eq!(poke_in("injector64.exe"), Some("injector"));
        assert_eq!(poke_in("injector_v2.py"), Some("injector"));
        // an ordinary noun inside a longer name is the ordinary Tuesday, not a hand
        assert!(poking(r"node dump_unpacker.js").is_none(), "unpacker is not the file's own name");
        assert!(poking(r"python resource_unpacker.py").is_none());
        assert!(poking(r"python dependency_injector.py").is_none(), "dependency injection is a library, not a hand");
        assert!(poking(r"python db_dumper.py").is_none());
        assert!(poking(r"python run_decompiler.py").is_none());
        assert!(poking(r"python my_ripper.py").is_none(), "the name alone must not answer for my_ripper.py");
        // the process's own name counts the same way
        assert_eq!(poke_in("cheatengine.exe"), Some("cheatengine"));
        assert_eq!(poke_in("recode_music.py"), None);
    }

    /// A poke word inside an ordinary word is nobody's hand. `frida` in `friday.py` is a Friday,
    /// not Frida — the same class of mistake that called `lwjgl.dll` a dropped DLL: a weak fact
    /// standing alone as a cause. The words that *are* hands still fire across a separator
    /// (`frida-server`, `cheatengine_hook`, `pymem_scan`) and across a version tail
    /// (`cheatengine64`).
    #[test]
    fn a_poke_word_inside_an_ordinary_word_is_nobodys_hand() {
        assert_eq!(poke_in("friday.py"), None, "frida inside friday is not frida");
        assert_eq!(poke_in("fridays.js"), None);
        assert_eq!(poke_in("friday.exe"), None);
        assert!(poking(r"python c:\x\friday.py").is_none(), "the script that runs is nobody's hand");
        assert!(a_hand_on_us("friday.exe", "").is_none());
        assert!(a_hand_on_us("python.exe", r"python friday.py").is_none());
        // the real thing, still a hand
        assert_eq!(poke_in("frida.exe"), Some("frida"));
        assert_eq!(poke_in("frida-server.py"), Some("frida"));
        assert_eq!(poke_in("my_frida.py"), Some("frida"));
        assert_eq!(poke_in("cheatengine_hook.py"), Some("cheatengine"));
        assert_eq!(poke_in("pymem_scan.py"), Some("pymem"));
        assert_eq!(poke_in("cheatengine64.exe"), Some("cheatengine"), "a digit after is a version tail");
        // and a poke word that merely begins a longer word is the same silence
        assert_eq!(poke_in("reopenedprocess.py"), None, "openprocess inside reopenedprocess is not openprocess");
    }

    /// Ordinary late natives remain allowed regardless of signature and location.
    #[test]
    fn a_new_library_alone_is_nobodys_finding() {
        let dirs = vec!["c:\\game\\runtime\\bin".to_string(), "c:\\game\\bin".to_string(), "c:\\windows\\system32".to_string()];
        // the ordinary natives, wherever the game pulls them in from
        assert!(foreign_module("awt.dll", "c:\\game\\runtime\\bin", &dirs, Some(false)).is_none());
        assert!(foreign_module("net.dll", "c:\\game\\runtime\\bin", &dirs, Some(false)).is_none());
        assert!(foreign_module("java.dll", "c:\\game\\runtime\\bin\\server", &dirs, Some(false)).is_none(), "a folder under a known one is known");
        assert!(foreign_module("lwjgl.dll", "c:\\game\\bin\\natives", &dirs, Some(false)).is_none(), "an unsigned native out of the game's own ground is the game's day");
        assert!(foreign_module("d3d11.dll", "c:\\windows\\system32", &dirs, Some(false)).is_none(), "the OS going about its day");
        // the two false positives this rule is built around
        assert!(foreign_module("lwjgl.dll", "c:\\game\\bin\\natives", &dirs, Some(false)).is_none(), "a new thread beside it was never a reason");
        assert!(foreign_module("discordhook64.dll", "c:\\users\\bob\\appdata\\local\\discord", &dirs, Some(true)).is_none(), "a signed overlay is silence whatever moment it shows up");
        // a check that cannot answer is silence, never a guess
        assert!(foreign_module("something.dll", "c:\\users\\bob\\temp", &dirs, None).is_none());
        // the dropped DLL: unsigned, and from nowhere the game was loading from
        assert!(foreign_module("evil.dll", "c:\\users\\bob\\temp", &dirs, Some(false)).is_none());
        // an injector's shape is a finding wherever it is and whatever its signature says
        assert!(foreign_module("injector.dll", "c:\\game\\runtime\\bin", &dirs, Some(true)).is_some());
        assert!(foreign_module("injector.dll", "c:\\game\\runtime\\bin", &dirs, Some(false)).is_some());
        assert!(foreign_module("pymem.dll", "c:\\users\\bob\\temp", &dirs, None).is_some());
        // signed and inside the game's ground is nobody's finding
        assert!(foreign_module("game_overlay.dll", "c:\\game\\bin", &dirs, Some(true)).is_none());
    }

    #[test]
    fn controlify_from_an_unknown_folder_is_not_injection() {
        for sig in [Some(false), Some(true), None] {
            assert!(foreign_module("controlify-native-160053463595141896sdl3.dll", "c:/new-native-dir", &[], sig).is_none());
            assert!(foreign_module("another-mod-native.dll", "d:/arbitrary", &[], sig).is_none());
        }
    }

    /// The game unpacks its own natives under the user's temp, and they are unsigned: lwjgl and
    /// its OpenAL/freetype/jemalloc/SDL/shaderc family in `lwjgl_<user>\<ver>\x64`, JNA in
    /// `jna-<id>`, and Mojang's jtracy's JNI in `jtracy-<id>`. Every one of them arrived after the
    /// baseline and would read as a dropped DLL. The folder's own name is what marks the ground;
    /// temp's root itself is still a stranger's place. A note never carries any of this — it is a
    /// path, and a path carries the player's name.
    #[test]
    fn the_game_unpacked_here_is_its_own_ground() {
        let dirs: Vec<String> = vec!["c:\\game\\bin".to_string()];
        let temp = std::env::temp_dir().to_string_lossy().to_lowercase();
        let temp = temp.trim_end_matches(['\\', '/']);
        let unpack = format!("{temp}\\lwjgl_someuser\\3.4.3+4\\x64");
        let jna = format!("{temp}\\jna-95354808");
        let jtracy = format!("{temp}\\jtracy-3f2a9c10-8b4d-4e2a-9c10-8b4d4e2a9c10");
        // every ordinary native is silence from there, whatever the moment it shows up
        assert!(foreign_module("lwjgl.dll", &unpack, &dirs, Some(false)).is_none(), "the game's own unpack is its day");
        assert!(foreign_module("OpenAL.dll", &unpack, &dirs, Some(false)).is_none());
        assert!(foreign_module("jna1592548377394835548.dll", &jna, &dirs, Some(false)).is_none(), "JNA unpacks there too");
        assert!(
            foreign_module("jtracy-jni-windows.dll", &jtracy, &dirs, Some(false)).is_none(),
            "jtracy's JNI unpacks under its own root the same way"
        );
        // the unpack root itself and a folder under it are the same ground
        let root = format!("{temp}\\lwjgl_someuser");
        assert!(foreign_module("lwjgl.dll", &root, &dirs, Some(false)).is_none());
        // the old name, with no underscore before the user
        let old = format!("{temp}\\lwjglsomeuser\\3.2.1-build-12");
        assert!(foreign_module("lwjgl.dll", &old, &dirs, Some(false)).is_none());
        // temp's own root is not ground: a dropped DLL there is still a stranger
        assert!(foreign_module("evil.dll", temp, &dirs, Some(false)).is_none());
        // and neither is any other folder under it
        let other = format!("{temp}\\evil_drop");
        assert!(foreign_module("evil.dll", &other, &dirs, Some(false)).is_none());
        // a poke's shape is a finding wherever it stands, unpack or not
        assert!(foreign_module("injector.dll", &unpack, &dirs, Some(false)).is_some());
        // the name alone, away from temp, is not ground
        assert!(foreign_module("lwjgl.dll", "c:\\users\\bob\\lwjgl_fake", &dirs, Some(false)).is_none());
    }

    /// The TLS holder the protector drops for a hand-mapped PE lands in temp's root after the
    /// baseline, unsigned, under a name that wears none of our words. It is ours by shape and is
    /// silence forever. The shape is exact — a `crd*.dll` that is not that shape is still a
    /// stranger, and temp's root is still not ground.
    #[test]
    fn the_tls_holder_is_ours_and_is_silence() {
        let dirs: Vec<String> = vec!["c:\\game\\bin".to_string()];
        let temp = std::env::temp_dir().to_string_lossy().to_lowercase();
        let temp = temp.trim_end_matches(['\\', '/']);
        // the name holder::drop_file really writes: crd + pid:08x + run:04x + ticks:08x (20 hex)
        assert!(is_tls_holder("crd0123456789abcdef0123.dll"));
        assert!(
            foreign_module("crd0123456789abcdef0123.dll", temp, &dirs, Some(false)).is_none(),
            "the holder is ours and is silence, even unsigned and in temp's root"
        );
        assert!(
            foreign_module("crd0123456789abcdef0123.dll", &format!("{temp}\\evil_drop"), &dirs, Some(false)).is_none(),
            "and in any folder"
        );
        // not that shape: still a stranger
        assert!(!is_tls_holder("crd1234.dll"), "too short");
        assert!(!is_tls_holder("crd0123456789abcdef012345.dll"), "too long");
        assert!(!is_tls_holder("crd0123456789abcdef012g.dll"), "not hex");
        assert!(!is_tls_holder("crd0123456789abcdef0123.bin"), "not a dll");
        assert!(!is_tls_holder("xc0123456789abcdef0123.dll"), "not our prefix");
        assert!(
            foreign_module("crd1234.dll", temp, &dirs, Some(false)).is_none(),
            "an unfamiliar ordinary native is not proof of injection"
        );
        // and the holder never becomes a reason to open temp's root
        assert!(foreign_module("evil.dll", temp, &dirs, Some(false)).is_none());
    }

    /// The compare treats a known folder's parent as known (`bin` beside `bin\natives`). Let one
    /// unpack root into that list and the whole of the user's temp becomes "the game knew it", and
    /// every DLL dropped there goes free. Nothing under temp is ever taken; the unpack question is
    /// `unpack_ground`'s alone.
    #[test]
    fn nothing_under_temp_enters_the_ground_list() {
        let temp = std::env::temp_dir().to_string_lossy().to_lowercase();
        let temp = temp.trim_end_matches(['\\', '/']);
        let mut known: Vec<String> = Vec::new();
        // the game's own trees are taken
        push_known_dir(&mut known, "c:\\game\\bin");
        push_known_dir(&mut known, "c:\\game\\runtime\\bin");
        assert_eq!(known.len(), 2);
        // an unpack root, its parents and a drop folder beside it are all refused
        push_known_dir(&mut known, &format!("{temp}\\lwjgl_someuser\\3.4.3+4\\x64"));
        push_known_dir(&mut known, &format!("{temp}\\lwjgl_someuser"));
        push_known_dir(&mut known, &format!("{temp}\\jna-123"));
        push_known_dir(&mut known, &format!("{temp}\\jtracy-3f2a9c10-8b4d-4e2a-9c10-8b4d4e2a9c10"));
        push_known_dir(&mut known, temp);
        push_known_dir(&mut known, &format!("{temp}\\evil_drop"));
        let parents: Vec<String> = std::path::Path::new(temp).ancestors().map(|p| p.to_string_lossy().to_lowercase()).collect();
        for p in parents {
            push_known_dir(&mut known, &p);
        }
        assert_eq!(known.len(), 2, "temp and its family never enter the ground list");
        // and so temp is still a stranger's place for a dropped DLL
        assert!(foreign_module("evil.dll", &format!("{temp}\\evil_drop"), &known, Some(false)).is_none());
        // a name that merely looks like the temp folder, away from temp, is not the ground
        let mut known2: Vec<String> = Vec::new();
        push_known_dir(&mut known2, "c:\\game\\bin");
        assert!(foreign_module("evil.dll", "c:\\users\\bob\\temp", &known2, Some(false)).is_none());
    }

    /// The launch-time environments are scrubbed of `-javaagent`, `-agentpath` and `-agentlib`
    /// before the game ever sees them — always, as a defence. A corporate agent is not aimed at
    /// us. **Our own name is never the trigger** (TOUCH.md §5): a `recode_something.jar` a player
    /// runs for their own reasons is silent, and only a poke word in the file's own name is a hand.
    #[test]
    fn agent_flags_are_scrubbed_and_only_an_aimed_agent_is_a_hand() {
        assert_eq!(without_agent_flags("-Xmx2g -javaagent:tools/agent.jar=opt"), "-Xmx2g");
        assert_eq!(without_agent_flags("-agentpath:c:/a.dll=1 -Xmx2g"), "-Xmx2g");
        assert_eq!(without_agent_flags("-agentlib:name=1 -Xmx2g"), "-Xmx2g");
        assert_eq!(without_agent_flags("-Xmx2g -Xss1m"), "-Xmx2g -Xss1m", "the rest of the line stays");
        assert_eq!(without_agent_flags("-javaagent:newrelic.jar"), "");
        // a corporate agent is not aimed at us and must be silent
        assert!(aimed_agent("c:/ops/newrelic.jar").is_none());
        assert!(aimed_agent("c:/ops/dynatrace/agent.jar").is_none());
        // our name alone is never a trigger
        assert!(aimed_agent("c:/temp/recode_hack.jar").is_none(), "our name is not a cause (TOUCH.md §5)");
        assert!(aimed_agent("c:/temp/recode_music_agent.jar").is_none(), "a player's own agent wearing our name is silent");
        // one whose own file name carries a poke word is a hand
        assert!(aimed_agent("c:/temp/pymem.jar").is_some());
        assert!(aimed_agent("c:/temp/injector.dll=opts").is_some());
        assert!(aimed_agent("c:/temp/recode_ripper.jar").is_some(), "our name beside a poke is the strengthening line, the poke is the cause");
    }

    /// A Java agent the launch-time environments already carried when the look began is silence:
    /// TOUCH.md `touch_jvm` says "one set before we came up" is never on. The player's laptop may
    /// have had `JAVA_TOOL_OPTIONS=-javaagent:…` set for some other project since forever, and
    /// this end already scrubbed those flags out of the child before the game ever saw them —
    /// presence here is the player's own machine, not a hand on our game. The line is **arrival**,
    /// the same line a poke-named tool draws.
    #[test]
    fn a_java_agent_already_in_the_environment_is_silence() {
        // a system-wide agent that was already there when the look began: silence whatever its name
        let was = vec!["c:/ops/newrelic.jar".to_string(), "c:/tools/pymem.jar".to_string()];
        assert!(new_aimed_agent(&was.clone(), &was).is_none(), "set before we came up is never on");
        // a corporate New Relic appearing mid-run is still not aimed at us
        assert!(new_aimed_agent(&["c:/ops/newrelic.jar".to_string()], &[]).is_none());
        // our own name is never the trigger (TOUCH.md §5)
        assert!(new_aimed_agent(&["c:/temp/recode_music_agent.jar".to_string()], &[]).is_none());
        // an aimed one that **arrived** while the game ran is a hand — the file name, not the path
        let (name, why) = new_aimed_agent(&["c:/users/bob/tools/pymem.jar".to_string()], &[]).expect("pymem arrived");
        assert_eq!(name, "pymem.jar", "a file name, never a path");
        assert!(!name.contains('\\') && !name.contains('/'), "a path is never written down");
        assert!(why.contains("poke"), "{why}");
        // one that appeared beside an old one is still the new one that is caught
        let (name, _) = new_aimed_agent(
            &["c:/ops/newrelic.jar".to_string(), "c:/temp/injector.jar".to_string()],
            &["c:/ops/newrelic.jar".to_string()],
        )
        .expect("the new one is the finding");
        assert_eq!(name, "injector.jar", "{name}");
    }

    /// An attacher aimed at our pid is a hand; the OS talking to itself is not. `jps`, `jstack`,
    /// `jmap` and a read-only `jcmd` are the OS. Our pid must be in the argv — a `jattach` with
    /// somebody else's pid is somebody else's business.
    #[test]
    fn an_attacher_must_have_our_pid_in_its_argv() {
        assert_eq!(attacher(r"jattach 4242 load instrument false agent.jar", 4242), Some("jattach"));
        assert_eq!(attacher(r"jcmd 4242 JVMTI.agent_load /x/agent.jar", 4242), Some("jcmd loading an agent"));
        assert_eq!(attacher(r"jcmd 4242 load instrument true a.jar", 4242), Some("jcmd loading an agent"));
        assert_eq!(attacher(r"jdb -attach 4242", 4242), Some("jdb"));
        assert_eq!(attacher(r"jshell --class-path . Attach 4242", 4242), Some("jshell"));
        assert_eq!(attacher(r"java -cp x Attach 4242", 4242), Some("a Java attach"));
        // somebody else's pid is somebody else's business
        assert_eq!(attacher(r"jattach 1111 load instrument false agent.jar", 4242), None);
        // the OS talking to itself
        assert_eq!(attacher(r"jcmd 4242 Thread.print", 4242), None);
        assert_eq!(attacher(r"jcmd 4242 GC.heap_info", 4242), None);
        assert_eq!(attacher(r"jstack 4242", 4242), None);
        assert_eq!(attacher(r"jps -l", 4242), None);
        assert_eq!(attacher(r"jmap -heap 4242", 4242), None);
    }

    /// The guard's own exit carries the note, and when that note is a touch the route is the
    /// contract's — not the old `protect` path. A note with no `touch:` line stays `protect`.
    #[test]
    fn a_touch_note_from_the_guard_is_routed_to_touch() {
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        let dir = home();
        let m = manifest();
        std::fs::write(dir.join("bin").join("caught.txt"), "Cursed Recode — the game was closed\nbuild: 9\ntouch: hook\nwhy: an IAT slot was moved\n").unwrap();
        let code_arg = GUARD_EXIT.to_string();
        let mut running = spawn("cmd", &["/c", "exit", &code_arg]);
        let (code, report, source) = until_it_goes(&mut running, &dir, &m, None, None);
        assert_eq!(code, Some(GUARD_EXIT));
        assert_eq!(source, "touch", "a touch line routes to the contract's path");
        let body = report.expect("the note goes with it");
        assert!(body.contains("touch: hook\n"), "{body}");
        assert!(body.contains("an IAT slot was moved"), "{body}");
        assert!(body.starts_with("Cursed Recode"), "the prefix is on what travels: {body}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A note already on disk that carries a `touch: ` line is never replaced: the stub was closer
    /// to the crime. This end's own finding is a lesser thing by construction and must not take a
    /// touch ban away.
    #[test]
    fn a_touch_note_on_disk_is_never_replaced() {
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        let dir = home();
        let m = manifest();
        std::fs::write(dir.join("bin").join("caught.txt"), "Cursed Recode — the game was closed\nbuild: 9\ntouch: write\nwhy: a page was made writable\n").unwrap();
        // a dumper is opened while the game runs — this end would close it and say so
        let mut running = spawn("cmd", &["/c", "ping", "-n", "30", "127.0.0.1"]);
        let dummy = dir.join("x64dbg.exe");
        std::fs::copy(r"C:\Windows\System32\ping.exe", &dummy).expect("the stand-in for a dumper");
        let later = {
            let dummy = dummy.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(2500));
                let mut cmd = Command::new(&dummy);
                cmd.args(["-n", "60", "127.0.0.1"]);
                cmd.stdout(std::process::Stdio::null());
                cmd.stderr(std::process::Stdio::null());
                cmd.spawn()
            })
        };
        let (_code, report, source) = until_it_goes(&mut running, &dir, &m, None, None);
        if let Ok(Ok(mut dummy)) = later.join() {
            let _ = dummy.kill();
            let _ = dummy.wait();
        }
        assert_eq!(source, "touch", "a touch note on disk is never replaced");
        let body = report.expect("the note goes to the server");
        assert!(body.contains("touch: write"), "{body}");
        assert!(!body.contains("takes a process apart"), "this end's lesser finding does not overwrite it: {body}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A tiny PE with one executable section and one writable data section, so the code checks can
    /// be asked what they look at without a real module. `.text` sits at rva 0x1000, `.data` at
    /// 0x2000, and the image is 0x3000 long.
    fn a_module_with_code_and_data() -> Vec<u8> {
        let mut pe = vec![0u8; 0x3000];
        pe[0] = b'M';
        pe[1] = b'Z';
        pe[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes()); // e_lfanew
        let lfanew = 0x80usize;
        pe[lfanew..lfanew + 4].copy_from_slice(b"PE\0\0");
        // COFF: NumberOfSections at +6, SizeOfOptionalHeader at +20
        pe[lfanew + 6..lfanew + 8].copy_from_slice(&2u16.to_le_bytes());
        pe[lfanew + 20..lfanew + 22].copy_from_slice(&0xE0u16.to_le_bytes());
        let opt = lfanew + 24;
        pe[opt + 16..opt + 20].copy_from_slice(&0x1000u32.to_le_bytes()); // AddressOfEntryPoint
        pe[opt + 56..opt + 60].copy_from_slice(&0x3000u32.to_le_bytes()); // SizeOfImage
        // two section headers, 40 bytes each
        let mut sec = lfanew + 24 + 0xE0;
        // .text — code, execute-read
        pe[sec..sec + 8].copy_from_slice(b".text\0\0\0");
        pe[sec + 8..sec + 12].copy_from_slice(&0x100u32.to_le_bytes()); // VirtualSize
        pe[sec + 12..sec + 16].copy_from_slice(&0x1000u32.to_le_bytes()); // VirtualAddress
        pe[sec + 16..sec + 20].copy_from_slice(&0x100u32.to_le_bytes()); // SizeOfRawData
        pe[sec + 36..sec + 40].copy_from_slice(&0x6000_0020u32.to_le_bytes()); // CODE|EXECUTE|READ
        sec += 40;
        // .data — initialized data, read-write
        pe[sec..sec + 8].copy_from_slice(b".data\0\0\0");
        pe[sec + 8..sec + 12].copy_from_slice(&0x100u32.to_le_bytes());
        pe[sec + 12..sec + 16].copy_from_slice(&0x2000u32.to_le_bytes());
        pe[sec + 16..sec + 20].copy_from_slice(&0x100u32.to_le_bytes());
        pe[sec + 36..sec + 40].copy_from_slice(&0xC000_0040u32.to_le_bytes()); // DATA|READ|WRITE
        // content
        for i in 0..0x100u32 {
            pe[0x1000 + i as usize] = 0x41 + (i % 26) as u8;
            pe[0x2000 + i as usize] = 0x61 + (i % 26) as u8;
        }
        pe
    }

    #[test]
    fn only_the_executable_sections_are_the_code() {
        let pe = a_module_with_code_and_data();
        let texts = text_ranges(&pe);
        assert_eq!(texts, vec![(0x1000usize, 0x100usize)], "the data section is never code");
    }

    #[test]
    fn our_own_key_slots_moving_is_not_a_patch_of_our_code() {
        let mut pe = a_module_with_code_and_data();
        let got = pe.len();
        let before = text_fold(&pe, got, &text_ranges(&pe)).expect("the code is covered");
        // our own stub wipes its key slots in .data and its import names in .rdata — this is that
        for b in pe[0x2000..0x2100].iter_mut() {
            *b = 0;
        }
        let after = text_fold(&pe, got, &text_ranges(&pe)).expect("the code is still covered");
        assert_eq!(before, after, "a wipe of our own data is not a patch of our code");
    }

    #[test]
    fn a_byte_of_our_own_code_moving_is_still_a_patch() {
        let mut pe = a_module_with_code_and_data();
        let got = pe.len();
        let before = text_fold(&pe, got, &text_ranges(&pe)).expect("the code is covered");
        pe[0x1040] ^= 0xFF;
        let after = text_fold(&pe, got, &text_ranges(&pe)).expect("the code is covered");
        assert_ne!(before, after, "a byte of .text that moved is exactly what this is for");
    }

    #[test]
    fn a_short_read_is_not_a_byte_change() {
        let pe = a_module_with_code_and_data();
        let texts = text_ranges(&pe);
        // the read stops in the middle of .text
        assert!(text_fold(&pe, 0x1050, &texts).is_none(), "a partial code read is never compared");
        // and a read that covers the code is
        assert!(text_fold(&pe, 0x1100, &texts).is_some());
    }

    #[test]
    fn a_page_that_was_already_writable_is_nobodys_finding() {
        // a span of code that was open at baseline, and one that was not
        let was = vec![(0x1000usize, 0x1080usize)];
        assert!(covered_by(0x1000, 0x1080, &was), "the whole span was already open");
        assert!(covered_by(0x1010, 0x1020, &was), "a part of it was already open");
        assert!(!covered_by(0x1000, 0x1100, &was), "a span that grew past it is new");
        assert!(!covered_by(0x1080, 0x1100, &was), "a span past it is new");
        assert!(!covered_by(0x0F00, 0x1000, &was), "a span before it is new");
    }

    /// A page "became writable" is a claim about a before. A baseline that never recorded what
    /// was open (`None` — the walk could not finish) has no before, and the question is silence,
    /// never a finding. This is the same class as the lwjgl ban: a fact with no baseline standing
    /// alone as a reason.
    #[test]
    fn a_page_with_no_baseline_of_what_was_open_is_nobodys_finding() {
        let pe = a_module_with_code_and_data();
        let texts = text_ranges(&pe);
        assert!(!texts.is_empty(), "the stand-in has code to ask about");
        // no baseline at all: silence
        assert!(writable_text_page_of(std::process::id(), 0, &texts, None).is_none(), "no before, no finding");
        // a finished walk that had nothing to walk is an answer, and an empty one
        assert_eq!(writable_text_spans(std::process::id(), 0, &[]), Some(Vec::new()), "no code is a finished, empty answer");
    }
}
