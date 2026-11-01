//! Discord Rich Presence: what the client shows on the player's Discord profile (as Cursed 1.21.11's rpc.rs).
//!
//! Discord listens on a local named pipe and speaks a tiny protocol of its own: an eight byte header
//! (opcode, length) and a JSON body. We talk to it directly instead of pulling in a library: one worker
//! thread, and the rest of the client never waits for it.
//!
//! Everything here is best-effort. No Discord, a Discord that starts later, a Discord that is restarted
//! in the middle of a session — the client does not care and keeps playing.
//!
//! **Who the player is.** Recode has no accounts yet: the two lines are 1.21.11's placeholders (UID 000, Dev). When it
//! has, [`set_profile`] tells the real ones and only [`compose`] decides what that looks like.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, OPEN_EXISTING};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;

/// Her Discord application «Cursed Recode». Its name is the first line Discord shows, and it is set in the developer
/// portal.
const APP_ID: &str = "1556436270417313882";
/// The logo. Discord fetches the address through its own proxy, so it needs no upload and never changes.
const LARGE_IMAGE: &str = "https://s15.gifyu.com/images/bQqv4.gif";
/// What the logo says when the mouse rests on it: the client and its version.
const LARGE_TEXT: &str = concat!("Cursed Recode ", env!("CARGO_PKG_VERSION"));

/// Who the player is until the client knows (1.21.11's defaults, the developer's). They live here and nowhere else.
const UNKNOWN_UID: &str = "000";
const UNKNOWN_USER: &str = "Dev";

/// Discord accepts a handful of updates per twenty seconds; one every few seconds is plenty for us.
const MIN_INTERVAL: Duration = Duration::from_secs(4);
/// Discord is not running (yet): look again this often.
const RECONNECT_EVERY: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(250);
/// Discord answers every command; if it does not, the connection is dead.
const REPLY_TIMEOUT: Duration = Duration::from_secs(3);

/// Who the client says the player is.
#[derive(Clone, PartialEq, Eq)]
pub struct Profile {
    pub uid: String,
    pub user: String,
}

impl Default for Profile {
    fn default() -> Self {
        Profile { uid: UNKNOWN_UID.to_string(), user: UNKNOWN_USER.to_string() }
    }
}

/// What Discord is told. Built only by [`compose`].
struct Activity {
    details: String,
    state: String,
    /// Unix seconds the "elapsed" clock counts from.
    since: u64,
}

struct State {
    profile: Profile,
    since: u64,
    /// Bumped on every change; the worker sends when this moves.
    revision: u64,
    /// The client is going away: clear the presence and let go of the pipe.
    over: bool,
}

/// The one place that turns what we know into what Discord shows.
fn compose(state: &State) -> Activity {
    Activity {
        details: format!("UID: {}", state.profile.uid),
        state: format!("User: {}", state.profile.user),
        since: state.since,
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
static RUNNING: AtomicBool = AtomicBool::new(false);
/// The worker has let go of Discord (it cleared the presence on the way out, or never was).
static DONE: AtomicBool = AtomicBool::new(false);

fn now_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// For the game's log (it takes the native messages each frame, vk/device.rs MESSAGES).
fn log(text: &str) {
    if let Ok(mut m) = crate::vk::device::MESSAGES.lock() {
        m.push(format!("I Discord: {text}"));
    }
}

/// Changes the shared state; the worker only sends when the change says something actually moved.
fn edit(change: impl FnOnce(&mut State) -> bool) {
    if let Ok(mut guard) = STATE.lock() {
        if let Some(state) = guard.as_mut() {
            if change(state) {
                state.revision += 1;
            }
        }
    }
}

/// Starts the worker. Called once, as early as the native library is loaded.
pub fn start() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Ok(mut guard) = STATE.lock() {
        *guard = Some(State { profile: Profile::default(), since: now_seconds(), revision: 1, over: false });
    }
    let spawned = std::thread::Builder::new().name("cursed-rpc".into()).spawn(|| {
        let _ = std::panic::catch_unwind(worker);
        DONE.store(true, Ordering::SeqCst);
    });
    if spawned.is_err() {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

/// Who this is, once the client knows. Until then the placeholders stand.
#[allow(dead_code)]
pub fn set_profile(uid: &str, user: &str) {
    let profile = Profile { uid: uid.to_string(), user: user.to_string() };
    edit(move |state| {
        if state.profile == profile {
            return false;
        }
        state.profile = profile;
        true
    });
}

/// The client is closing: Discord is told the game is over, so the profile does not keep it for a minute. Waits for
/// that at most `most` (the process may end right after).
pub fn shutdown(most: Duration) {
    if !RUNNING.load(Ordering::SeqCst) {
        return;
    }
    edit(|state| {
        state.over = true;
        true
    });
    let until = Instant::now() + most;
    while !DONE.load(Ordering::SeqCst) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
}

// ---------------------------------------------------------------------------------------------- transport

struct Pipe {
    handle: HANDLE,
}

impl Drop for Pipe {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

impl Pipe {
    /// Several Discords can run side by side, each on its own pipe; the first that answers wins.
    fn connect() -> Option<Pipe> {
        // Dev switch: CURSED_RPC_PIPE=name talks to that pipe only (a stand-in Discord for tests).
        let names: Vec<String> = match std::env::var("CURSED_RPC_PIPE") {
            Ok(name) if !name.is_empty() => vec![format!("\\\\.\\pipe\\{name}")],
            _ => (0..10).map(|index| format!("\\\\.\\pipe\\discord-ipc-{index}")).collect(),
        };
        for name in names {
            let name = wide(&name);
            let handle = unsafe { CreateFileW(name.as_ptr(), GENERIC_READ | GENERIC_WRITE, 0, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut()) };
            if handle != INVALID_HANDLE_VALUE && !handle.is_null() {
                return Some(Pipe { handle });
            }
        }
        None
    }

    fn write_all(&self, bytes: &[u8]) -> bool {
        let mut sent = 0usize;
        while sent < bytes.len() {
            let mut written = 0u32;
            let ok = unsafe { WriteFile(self.handle, bytes[sent..].as_ptr(), (bytes.len() - sent) as u32, &mut written, std::ptr::null_mut()) };
            if ok == 0 || written == 0 {
                return false;
            }
            sent += written as usize;
        }
        true
    }

    fn send(&self, opcode: u32, body: &str) -> bool {
        let mut frame = Vec::with_capacity(8 + body.len());
        frame.extend_from_slice(&opcode.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(body.as_bytes());
        self.write_all(&frame)
    }

    /// Bytes waiting to be read, or None when the pipe is broken: Discord was closed or restarted.
    fn peek(&self) -> Option<u32> {
        let mut total = 0u32;
        let ok = unsafe { PeekNamedPipe(self.handle, std::ptr::null_mut(), 0, std::ptr::null_mut(), &mut total, std::ptr::null_mut()) };
        (ok != 0).then_some(total)
    }

    fn available(&self) -> u32 {
        self.peek().unwrap_or(0)
    }

    /// Discord is still on the other end. Asked every poll, so a restarted Discord gets our presence back at once.
    fn alive(&self) -> bool {
        self.peek().is_some()
    }

    /// Reads exactly this many bytes, or gives up. A read only starts once the bytes are already there, so a silent
    /// Discord can never block this thread.
    fn read_exact(&self, want: usize, deadline: Instant) -> Option<Vec<u8>> {
        let mut buffer = vec![0u8; want];
        let mut have = 0usize;
        while have < want {
            if self.available() == 0 {
                if Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            let mut read = 0u32;
            let ok = unsafe { ReadFile(self.handle, buffer[have..].as_mut_ptr(), (want - have) as u32, &mut read, std::ptr::null_mut()) };
            if ok == 0 || read == 0 {
                return None;
            }
            have += read as usize;
        }
        Some(buffer)
    }

    /// One answer from Discord. The body is returned as it came, for the log.
    fn receive(&self) -> Option<(u32, String)> {
        let deadline = Instant::now() + REPLY_TIMEOUT;
        let header = self.read_exact(8, deadline)?;
        let opcode = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let body = if length == 0 { Vec::new() } else { self.read_exact(length, deadline)? };
        Some((opcode, String::from_utf8_lossy(&body).into_owned()))
    }

    /// Throws away anything Discord sent on its own (it does not ask us anything we answer).
    fn drain(&self) {
        while self.available() >= 8 {
            if self.receive().is_none() {
                return;
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------- JSON

/// Discord's answer to a command was an error: its message, for the log.
fn error_of(body: &str) -> Option<String> {
    if !body.contains("\"evt\":\"ERROR\"") {
        return None;
    }
    let message = body.split("\"message\":\"").nth(1).and_then(|rest| rest.split('"').next()).unwrap_or("unknown");
    Some(message.to_string())
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn handshake_body() -> String {
    format!("{{\"v\":1,\"client_id\":\"{APP_ID}\"}}")
}

fn activity_body(activity: &Activity, pid: u32, nonce: u64) -> String {
    format!(
        "{{\"cmd\":\"SET_ACTIVITY\",\"nonce\":\"cursed-{nonce}\",\"args\":{{\"pid\":{pid},\"activity\":{{\
         \"details\":\"{details}\",\"state\":\"{state}\",\"timestamps\":{{\"start\":{since}}},\
         \"assets\":{{\"large_image\":\"{image}\",\"large_text\":\"{image_text}\"}}}}}}}}",
        details = escape(&activity.details),
        state = escape(&activity.state),
        since = activity.since,
        image = escape(LARGE_IMAGE),
        image_text = escape(LARGE_TEXT),
    )
}

/// An empty activity: Discord drops the entry from the profile at once.
fn clear_body(pid: u32) -> String {
    format!("{{\"cmd\":\"SET_ACTIVITY\",\"nonce\":\"cursed-off\",\"args\":{{\"pid\":{pid}}}}}")
}

// ------------------------------------------------------------------------------------------------ worker

/// Reads the shared state: what to show, which revision it is, and whether we are closing.
fn snapshot() -> Option<(Activity, u64, bool)> {
    let guard = STATE.lock().ok()?;
    let state = guard.as_ref()?;
    Some((compose(state), state.revision, state.over))
}

fn worker() {
    let pid = unsafe { GetCurrentProcessId() };
    let mut pipe: Option<Pipe> = None;
    let mut looked_at = Instant::now() - RECONNECT_EVERY;
    let mut sent_revision = 0u64;
    let mut sent_at = Instant::now() - MIN_INTERVAL;

    loop {
        let Some((activity, revision, over)) = snapshot() else {
            return;
        };

        if over {
            if let Some(open) = pipe.as_ref() {
                open.send(1, &clear_body(pid));
                // The frame has to reach Discord before the process goes; it is a few dozen bytes.
                let _ = open.receive();
                log("cleared");
            }
            return;
        }

        if pipe.is_none() && looked_at.elapsed() >= RECONNECT_EVERY {
            looked_at = Instant::now();
            if let Some(open) = Pipe::connect() {
                let answer = if open.send(0, &handshake_body()) { open.receive() } else { None };
                match answer {
                    // Opcode 1 with READY: Discord knows the application and listens.
                    Some((1, body)) if body.contains("\"READY\"") => {
                        log("connected");
                        pipe = Some(open);
                        sent_revision = 0;
                        sent_at = Instant::now() - MIN_INTERVAL;
                    }
                    Some((opcode, body)) => log(&format!("refused the handshake ({opcode}): {body}")),
                    None => log("did not answer the handshake"),
                }
            }
        }

        if pipe.as_ref().is_some_and(|open| !open.alive()) {
            // Closed or restarted: look for it again; the presence is sent anew once it is back.
            log("lost");
            pipe = None;
        }

        if let Some(open) = pipe.as_ref() {
            open.drain();
            if revision != sent_revision && sent_at.elapsed() >= MIN_INTERVAL {
                let alive = open.send(1, &activity_body(&activity, pid, revision));
                let answer = if alive { open.receive() } else { None };
                match answer {
                    Some((_, body)) => {
                        sent_revision = revision;
                        sent_at = Instant::now();
                        match error_of(&body) {
                            Some(message) => log(&format!("refused the presence: {message}")),
                            None => log(&format!("{} / {}", activity.details, activity.state)),
                        }
                    }
                    // Discord was closed or restarted: drop the pipe and look for it again.
                    None => {
                        log("lost");
                        pipe = None;
                    }
                }
            }
        }

        std::thread::sleep(POLL);
    }
}
