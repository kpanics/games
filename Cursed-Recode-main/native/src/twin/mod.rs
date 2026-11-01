//! The window before the game ("twin"): a see-through window of our own over the desktop, up long before the game
//! can draw. v12: a spark lights where the menu's glow will be, draws a line out to the game window's edges, and the
//! window opens along it (its edges burning, the desktop charred just past them); in it the card plays — the wings
//! drawn stroke by stroke, CURSED rising out of its line and rewritten into RECODE by a point that burns along it with
//! the game's loading — while the game loads behind it, hidden. When the game draws the menu its window is shown under
//! this one, on the same picture (both draw it from one clock, link.rs), and this one goes. On the way out the same
//! window comes back over the game's and closes onto the line, the line into the spark, which goes out in a wisp; the
//! game shuts down behind it.
//!
//! Its own thread, its own Vulkan device, kept for the whole session (the farewell starts on its first frame, no
//! device or pipelines to make then). The picture goes to the compositor on the GPU (present.rs, DirectComposition,
//! at the monitor's rate); where that cannot be made, it is read back and handed to UpdateLayeredWindow as before.
//! The schedule is the approved preview's (Desktop\Cursed Recode Launch, DT and XT).

pub mod link;
pub mod present;
pub mod titlebar;

use crate::engine::gpu::{fail, image_in, Compute};
use crate::launch::timeline::{self as tm, smooth, smoother, tl, Exit, Phase};
use crate::launch::{Inputs, Launch, LINES};
use crate::vk::device::Gpu;
use crate::vk::encoder::{memory_barrier, Encoder};
use crate::vk::memory::Place;
use crate::vk::resources::Buffer;
use crate::vk::Progress;
use ash::vk;
use link::Link;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Started by the loader (loader/app.rs): the story's second it begins from (f32 bits; 0 the drop) — the loader's own
/// window drew the valley into its outlines already — and the loader's window, hidden once this one is up over it on
/// the same picture.
static STORY_FROM: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static HIDE_ON_SHOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Closed from its taskbar button while the card plays: the window closes at once from as far as it had opened.
/// Closed while the launch played: the pane thaws away at once (v13; the glass's `quit`).
const QUICK: Exit = Exit { close_from: 0.0, close_to: 0.85, shrink_to: 0.85, spark_out: 0.9, wisp_to: 1.0, line_from: 99.0, line_to: 99.0 };
/// Room round the game's window for the edges' fire, the char, the tongues and the wisp (ui px).
const MARGIN: [f32; 4] = [90.0, 150.0, 90.0, 90.0]; // left, top, right, bottom
/// Once the game's window is up under it (and drawing), this window fades this long: the two pictures are the same,
/// the fade only hides the last few pixels of difference.
const FADE: f32 = 0.12;
/// How long the card waits for the game before giving up (a game that never comes).
const GIVE_UP: f32 = 180.0;
pub const APP_ID: &str = "Lunuca.CursedRecode";

static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
static LOG: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
/// The window was asked to close (its taskbar button), while the card plays.
static CLOSE_ASKED: AtomicBool = AtomicBool::new(false);

/// Development (cursed_twin.exe --shots): the launch drawn on a clock of its own, no window, no sound — its pictures
/// at the seconds asked written to files (premultiplied BGRA, the window's picture with its margins round the game's).
pub struct Shots {
    pub dir: std::path::PathBuf,
    /// Seconds from the start, rising.
    pub times: Vec<f32>,
    /// The step of the clock (s): the smoke and the sparks run frame by frame as they would.
    pub dt: f32,
    /// When the game is "ready" (the card's wait ends then).
    pub ready_at: f32,
    /// When its resources begin to say how far they have loaded (till `ready_at`); nothing said before.
    pub said_from: f32,
    /// The way out instead (0 from the main menu, 1 from a world), taken over this many seconds into it.
    pub farewell: Option<(u32, f32)>,
    /// The main menu's valley as it is once the story is over (its clock the seconds asked): the menu's own life.
    pub menu: bool,
}
static SHOTS: std::sync::Mutex<Option<Shots>> = std::sync::Mutex::new(None);

/// Development: the launch's pictures at `s.times` for a game window `w` × `h` (see Shots).
pub fn shots(s: Shots, w: i32, h: i32) {
    let l = link::local();
    l.epoch.store(link::qpc(), Ordering::Release);
    l.state.store(link::PLAYING, Ordering::Release);
    crate::launch::sound::volume(0.0, 0.0);
    // development: CURSED_DEV_SOUNDTAP — the sound mixed with the pictures' clock, into that file (no sound device)
    if std::env::var_os("CURSED_DEV_SOUNDTAP").is_some() {
        crate::launch::sound::offline();
        crate::launch::sound::volume(1.0, 1.0);
    }
    let Some(mut kit) = Kit::new() else { return };
    // the card's pipelines first: the clock is not real time here
    let _ = kit.renderer(true);
    let p = place(w, h, Some([200, 200]));
    let farewell = s.farewell;
    *SHOTS.lock().unwrap() = Some(s);
    unsafe {
        match farewell {
            Some((kind, after)) => {
                // (the shots' clock runs from the epoch: the way out began `after` before it)
                let now = l.epoch.load(Ordering::Acquire);
                let exit = link::qpc_at(now, -after);
                let mode = Mode::Farewell { card: tl::SETTLE + 5.0, fx: 30.0, left: -1.0, foot_top: -1.0, ask: now, exit, view: [0.0; 7], open: -1.0, kind };
                run(&mut kit, p, mode, l);
            }
            None => {
                run(&mut kit, p, Mode::Launch, l);
            }
        }
    }
    kit.destroy();
}

/// A line in the program's log (`--log`), with the milliseconds since it started.
pub fn note(text: &str) {
    if let Some(Some(p)) = LOG.get() {
        use std::io::Write;
        let ms = STARTED.get().map_or(0, |t| t.elapsed().as_millis());
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{ms:>6} ms  {text}");
        }
    }
}

/// This process's windows are one program on the taskbar with the window before the game's (her icon, one button).
pub fn app_id() {
    let id: Vec<u16> = APP_ID.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
    }
}

// ---- the game's side ------------------------------------------------------------------------------------------

/// The game's link to the window before the game: joined (a program of its own) or made here.
static LINK: std::sync::OnceLock<&'static Link> = std::sync::OnceLock::new();
/// The window runs in this process (its thread): the game waits for its farewell before it ends.
static LOCAL: AtomicBool = AtomicBool::new(false);

pub fn link() -> Option<&'static Link> {
    LINK.get().copied()
}

/// The link when the window is a program of its own (the music lives there).
pub fn external() -> Option<&'static Link> {
    link().filter(|_| !LOCAL.load(Ordering::Acquire))
}

/// The game joins the window before the game started before it (its link `name`): where the game's window goes.
pub fn attach(name: &str) -> Option<Place2> {
    let l = link::open(name, 4000)?;
    l.game_pid.store(std::process::id(), Ordering::Release);
    l.flags.fetch_or(link::JOINED, Ordering::AcqRel);
    // started by the loader: its valley's clock and its crows go on here
    crate::launch::ink::set_life_shift(link::f32_of(&l.life_shift));
    let s = link::f32_of(&l.startled);
    if s >= 0.0 {
        crate::launch::ink::set_startled(s);
    }
    let v: [i32; 8] = std::array::from_fn(|i| l.place[i].load(Ordering::Acquire));
    let _ = LINK.set(l);
    Some(Place2 { client: [v[0], v[1], v[2], v[3]], frame: [v[4], v[5], v[6], v[7]] })
}

/// The game starts the window itself (no program of its own): for a game window at `p`. False if it could not start.
pub fn start(p: Place2, mute: bool) -> bool {
    let l = link::local();
    l.epoch.store(link::qpc(), Ordering::Release);
    l.game_pid.store(std::process::id(), Ordering::Release);
    write_place(l, &p);
    if LINK.set(l).is_err() {
        return false;
    }
    LOCAL.store(true, Ordering::Release);
    let spawned = std::thread::Builder::new().name("Cursed twin".into()).spawn(move || serve_link(l, p, mute));
    if spawned.is_err() {
        return false;
    }
    // up when its device is (or it gave up)
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        match l.state.load(Ordering::Acquire) {
            link::PLAYING => return true,
            link::DONE => return false,
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    false
}

fn write_place(l: &Link, p: &Place2) {
    for i in 0..4 {
        l.place[i].store(p.client[i], Ordering::Release);
        l.place[4 + i].store(p.frame[i], Ordering::Release);
    }
    l.placed.store(1, Ordering::Release);
}

/// The card's clocks as the window before the game keeps them: seconds of `life` (never waits), the card's second
/// (the story — it waited for the game), the desk's second (never waits: v12's card runs on the desk's clock), and
/// bits: 1 the window still plays the card, 2 its wait for the game is over. NaN without the window.
pub fn times() -> [f32; 4] {
    let Some(l) = link() else { return [f32::NAN; 4] };
    let life = l.life();
    let fx = life;
    let done = l.hold_done.load(Ordering::Acquire) != 0;
    let t = if done { fx - link::f32_of(&l.held) } else { link::f32_of(&l.card) };
    let plays = l.state.load(Ordering::Acquire) == link::PLAYING && l.flags.load(Ordering::Acquire) & link::RELEASE == 0;
    [life, t, fx, (plays as i32 | (done as i32) << 1) as f32]
}

/// The game draws its menu: the card's wait is over.
pub fn ready() {
    if let Some(l) = link() {
        l.flags.fetch_or(link::READY, Ordering::AcqRel);
    }
}

/// When the game's window took over (its bar's buttons come in from there).
static RELEASED_AT: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// The game's window is up under this one and drawing: it goes. From here the game plays the card's sounds, not
/// again those already heard.
pub fn release() {
    if let Some(l) = link() {
        if l.flags.fetch_or(link::RELEASE, Ordering::AcqRel) & link::RELEASE == 0 {
            let _ = RELEASED_AT.set(Instant::now());
            let tm = times();
            crate::launch::sound::card_from(tm[2], tm[1]);
        }
    }
}

/// For the game's bar: None without the window before the game; Some(None) while that window is over the game's; then
/// how long ago it let go (s).
pub fn released_ago() -> Option<Option<f32>> {
    let l = link()?;
    if l.flags.load(Ordering::Acquire) & link::RELEASE == 0 {
        return Some(None);
    }
    Some(Some(RELEASED_AT.get().map_or(f32::MAX, |t| t.elapsed().as_secs_f32())))
}

/// Whether the window before the game is minimised (the game's window comes up minimised then).
pub fn hidden() -> bool {
    link().is_some_and(|l| {
        let f = l.flags.load(Ordering::Acquire);
        f & link::HIDDEN != 0 && f & link::RELEASE == 0
    })
}

/// The game's resources loading (0..1): the card's point may run that far.
pub fn load(p: f32) {
    if let Some(l) = link() {
        let was = link::f32_of(&l.load);
        if p > was {
            link::set_f32(&l.load, p.min(1.0));
        }
    }
}

/// The game's folder (cursed_twin.exe --game-dir; the game's own working folder when the window is a thread of it).
static GAME_DIR: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();

/// Where what the game's launches took is kept: cursed-recode/launch.txt in its folder.
fn res_file() -> Option<std::path::PathBuf> {
    let dir = GAME_DIR.get().cloned().flatten().or_else(|| if LOCAL.load(Ordering::Acquire) { std::env::current_dir().ok() } else { None })?;
    Some(dir.join("cursed-recode").join("launch.txt"))
}

/// The desk's second the game's resources began to load, as the launches here took it till now (tm::RES_AT the first
/// time): the card's point paces itself by it while the game says nothing.
fn res_read() -> f32 {
    let text = res_file().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let said = text.lines().find_map(|l| l.strip_prefix("res ").and_then(|v| v.trim().parse::<f32>().ok()));
    said.unwrap_or(tm::RES_AT).clamp(tm::lt::REWRITE_FROM + 1.0, 90.0)
}

/// This launch's: its resources began at `res_at` (the desk's second; unknown: a little before it was ready at
/// `ready`), weighed in with the launches before (`old`).
fn res_write(old: f32, res_at: Option<f32>, ready: f32) {
    let Some(p) = res_file() else { return };
    let seen = res_at.unwrap_or(ready - 1.5).clamp(tm::lt::REWRITE_FROM + 1.0, 90.0);
    let res = old + (seen - old) * 0.6;
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&p, format!("res {res:.2}\n"));
    note(&format!("the game's resources began at {seen:.2} s (ready at {ready:.2}); next time paced by {res:.2} s"));
}

/// Whether the window before the game plays the card's sounds (until the game has taken over).
pub fn owns_card() -> bool {
    link().is_some_and(|l| l.state.load(Ordering::Acquire) == link::PLAYING && l.flags.load(Ordering::Acquire) & link::RELEASE == 0)
}

/// Whether the farewell's first picture is up over the game's window.
pub fn shown() -> bool {
    link().is_some_and(|l| l.shown.load(Ordering::Acquire) != 0)
}

/// Whether the window in this process still plays (the game waits for it before it ends).
pub fn busy() -> bool {
    LOCAL.load(Ordering::Acquire) && link().is_some_and(|l| l.state.load(Ordering::Acquire) != link::DONE)
}

/// What the game's menu looked like when it asked for the farewell.
pub struct View {
    pub card: f32,
    pub fx: f32,
    pub left: f32,
    pub foot_top: f32,
    /// How long ago the way out began (s).
    pub exit_ago: f32,
    /// look x y, cursor x y on, the hover's glow, sheet lightning
    pub view: [f32; 7],
    /// The game's window maximised; the window in use.
    pub maximized: bool,
    pub active: bool,
    /// The way out from: 0 the main menu, 1 a world.
    pub kind: u32,
}

/// The way out: over the game's window at `p`, the menu's picture as `v` says, the hole closes into the spark.
pub fn farewell(p: Place2, v: View) -> bool {
    let Some(l) = link() else { return false };
    if l.state.load(Ordering::Acquire) == link::DONE {
        return false;
    }
    for i in 0..4 {
        l.farewell_place[i].store(p.client[i], Ordering::Release);
        l.farewell_place[4 + i].store(p.frame[i], Ordering::Release);
    }
    for (a, x) in l.farewell.iter().zip([v.card, v.fx, v.left, v.foot_top]) {
        link::set_f32(a, x);
    }
    for (a, x) in l.view.iter().zip(v.view) {
        link::set_f32(a, x);
    }
    let now = link::qpc();
    l.ask_qpc.store(now, Ordering::Release);
    l.exit_qpc.store(link::qpc_ago(Duration::from_secs_f32(v.exit_ago.max(0.0))), Ordering::Release);
    l.shown.store(0, Ordering::Release);
    l.exit_kind.store(v.kind, Ordering::Release);
    if v.maximized {
        l.flags.fetch_or(link::MAXIMIZED, Ordering::AcqRel);
    }
    if !v.active {
        l.flags.fetch_or(link::INACTIVE, Ordering::AcqRel);
    }
    // the valley as the game drew it last (her options: how far, the fog, the field of view…), for the way out to go
    // on from
    match crate::launch::last_drawn() {
        Some(s) => {
            for (a, x) in l.scene.iter().zip(s) {
                link::set_f32(a, x);
            }
            l.scene_set.store(1, Ordering::Release);
        }
        None => l.scene_set.store(0, Ordering::Release),
    }
    // …and its crows: when they were startled off the tree (the window before the game was gone by then)
    link::set_f32(&l.startled, crate::launch::ink::startled());
    l.flags.fetch_or(link::FAREWELL, Ordering::AcqRel);
    true
}

/// The game's options the valley follows, from options.txt in its folder (as Java gives them to the menus,
/// MenuHost.inp): render distance, clouds (0 off, 1 fast, 2 fancy), their distance, particles, brightness, the field of
/// view (degrees), the graphics preset (0 fast … 3 custom), the background's blur. None without the file.
fn options_valley() -> Option<[f32; 8]> {
    let dir = GAME_DIR.get().cloned().flatten().or_else(|| if LOCAL.load(Ordering::Acquire) { std::env::current_dir().ok() } else { None })?;
    valley_options(&dir)
}

/// The game's options the valley follows (renderDistance, clouds, particles, gamma, fov, graphics, blur), out of
/// `dir`/options.txt; the game's defaults where a line is missing. None without the file.
pub fn valley_options(dir: &std::path::Path) -> Option<[f32; 8]> {
    let text = std::fs::read_to_string(dir.join("options.txt")).ok()?;
    // the game's defaults where a line is missing
    let mut o = [12.0, 2.0, 128.0, 0.0, 0.5, 70.0, 1.0, 5.0];
    for line in text.lines() {
        let Some((key, val)) = line.split_once(':') else { continue };
        let val = val.trim().trim_matches('"');
        let num = val.parse::<f32>().ok();
        match key {
            "renderDistance" => o[0] = num.unwrap_or(o[0]),
            "renderClouds" => o[1] = match val { "false" => 0.0, "fast" => 1.0, _ => 2.0 },
            "cloudRange" => o[2] = num.unwrap_or(o[2]),
            "particles" => o[3] = num.unwrap_or(o[3]),
            "gamma" => o[4] = num.unwrap_or(o[4]),
            // (kept as (degrees − 70) / 40)
            "fov" => o[5] = num.map_or(o[5], |f| (f * 40.0 + 70.0).round()),
            "graphicsPreset" => o[6] = match val { "fast" => 0.0, "fancy" => 1.0, "fabulous" => 2.0, _ => 3.0 },
            "menuBackgroundBlurriness" => o[7] = num.unwrap_or(o[7]),
            _ => {}
        }
    }
    Some(o)
}

/// The game's window: its picture (client area) and its visible frame, physical px.
#[derive(Clone, Copy, Default)]
pub struct Place2 {
    pub client: [i32; 4],
    pub frame: [i32; 4],
}

/// Where the game's window goes: `w` × `h` (0: about 72% of the work area, 16:9) centred in the primary monitor's
/// work area, or with its picture's corner at `at`. Physical px; the frame is what is seen of it (Windows 10 and 11
/// keep an invisible resize border round a window, all but on top: not part of the picture we burn).
pub fn place(w: i32, h: i32, at: Option<[i32; 2]>) -> Place2 {
    unsafe {
        let old = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let point = at.map_or(POINT { x: 0, y: 0 }, |a| POINT { x: a[0] + 10, y: a[1] + 10 });
        let mon = MonitorFromPoint(point, MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(mon, &mut info);
        let work = info.rcWork;
        let (ww, wh) = (work.right - work.left, work.bottom - work.top);
        let (mut cw, mut ch) = (w, h);
        if cw <= 0 || ch <= 0 {
            cw = (ww as f32 * 0.72) as i32;
            ch = cw * 9 / 16;
            if ch > (wh as f32 * 0.8) as i32 {
                ch = (wh as f32 * 0.8) as i32;
                cw = ch * 16 / 9;
            }
        }
        let mut dpi = 96u32;
        windows_sys::Win32::UI::HiDpi::GetDpiForMonitor(mon, windows_sys::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI, &mut dpi, &mut 0u32);
        if crate::chrome::wanted() {
            // our own title bar (chrome/): the picture under it, a pixel of border left, right and below (Windows draws
            // it on the frame's edge), the bar the frame's top
            let bar = crate::chrome::bar_height(dpi);
            let (cx, cy) = match at {
                Some(a) => (a[0], a[1]),
                None => (work.left + (ww - cw) / 2, work.top + (wh - ch - bar) / 2 + bar),
            };
            SetThreadDpiAwarenessContext(old);
            return Place2 { client: [cx, cy, cw, ch], frame: [cx - 1, cy - bar, cw + 2, ch + bar + 1] };
        }
        let mut r = RECT { left: 0, top: 0, right: cw, bottom: ch };
        AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPEDWINDOW, 0, 0, dpi);
        let (fw, fh) = (r.right - r.left, r.bottom - r.top);
        let (cx, cy) = match at {
            Some(a) => (a[0], a[1]),
            None => (work.left + (ww - fw) / 2 - r.left, work.top + (wh - fh) / 2 - r.top),
        };
        SetThreadDpiAwarenessContext(old);
        // the visible frame: one px of border left, right and below the picture; the title bar above it
        let inset = (-r.left - 1).max(0);
        let (vx, vy) = (cx + r.left + inset, cy + r.top);
        Place2 { client: [cx, cy, cw, ch], frame: [vx, vy, fw - 2 * inset, fh - inset] }
    }
}

/// Where a shown window is: its picture (client area) and its visible frame, physical px.
pub fn place_of(hwnd: HWND) -> Place2 {
    unsafe {
        let old = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mut c = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetClientRect(hwnd, &mut c);
        let mut at = POINT { x: 0, y: 0 };
        ClientToScreen(hwnd, &mut at);
        let mut f = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let got = windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute(
            hwnd,
            windows_sys::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut f as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        );
        if got != 0 {
            GetWindowRect(hwnd, &mut f);
        }
        SetThreadDpiAwarenessContext(old);
        // our own title bar is the client area's top: the picture is under it
        let bar = crate::chrome::inset_of(hwnd as isize);
        Place2 { client: [at.x, at.y + bar, c.right - c.left, c.bottom - c.top - bar], frame: [f.left, f.top, f.right - f.left, f.bottom - f.top] }
    }
}

// ---- the window ---------------------------------------------------------------------------------------------

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        // its taskbar button's «close»: the program closes, the hole closes over it
        WM_CLOSE => {
            CLOSE_ASKED.store(true, Ordering::Release);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---- the program for Windows before the game's window is there ---------------------------------------------------
// The window before the game lets every click through (it is only a picture). Over the game window's place lies a
// window of its own, all but invisible, that is the program for Windows meanwhile (her word, 19.10.2026): its taskbar
// button (minimised and restored from there), the focus — a click anywhere else takes it away, and the window
// minimises —, the cursor near the top, which brings the lone minimise down into the bar, and the click on it. No
// maximise, no close (the taskbar button's menu still closes the program).

static CATCH_IN: AtomicBool = AtomicBool::new(false);
static CATCH_TRACK: AtomicBool = AtomicBool::new(false);
static CATCH_X: AtomicI32 = AtomicI32::new(-1);
static CATCH_Y: AtomicI32 = AtomicI32::new(-1);
/// The minimise's cell (x0 y0 x1 y1, the catcher's px), and whether it is down there to be clicked.
static PILL_AT: [AtomicI32; 4] = [AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0)];
static PILL_LIVE: AtomicBool = AtomicBool::new(false);
static PILL_PRESS: AtomicBool = AtomicBool::new(false);
/// The minimise clicked.
static MIN_ASKED: AtomicBool = AtomicBool::new(false);
/// The focus went away (where to, the frame looks: the game's own window taking it is the hand-over, not a click
/// elsewhere).
static FOCUS_LOST: AtomicBool = AtomicBool::new(false);

/// Stands (CURSED_DEV_TWIN_AT, a build from the sources) leave her focus where it is: the catcher neither takes it nor
/// minimises as it goes elsewhere.
fn quiet() -> bool {
    !crate::license::RELEASE && std::env::var_os("CURSED_DEV_TWIN_AT").is_some()
}

fn over_pill(x: i32, y: i32) -> bool {
    let a: [i32; 4] = std::array::from_fn(|i| PILL_AT[i].load(Ordering::Acquire));
    PILL_LIVE.load(Ordering::Acquire) && x >= a[0] && y >= a[1] && x < a[2] && y < a[3]
}

unsafe extern "system" fn catch_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
    let (x, y) = ((l & 0xFFFF) as i16 as i32, ((l >> 16) & 0xFFFF) as i16 as i32);
    match msg {
        WM_MOUSEMOVE => {
            if !CATCH_TRACK.swap(true, Ordering::AcqRel) {
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                TrackMouseEvent(&mut t);
            }
            CATCH_X.store(x, Ordering::Release);
            CATCH_Y.store(y, Ordering::Release);
            CATCH_IN.store(true, Ordering::Release);
            0
        }
        WM_MOUSELEAVE => {
            CATCH_TRACK.store(false, Ordering::Release);
            CATCH_IN.store(false, Ordering::Release);
            0
        }
        WM_LBUTTONDOWN => {
            if over_pill(x, y) {
                PILL_PRESS.store(true, Ordering::Release);
                SetCapture(hwnd);
            }
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            if PILL_PRESS.swap(false, Ordering::AcqRel) && over_pill(x, y) {
                MIN_ASKED.store(true, Ordering::Release);
            }
            0
        }
        WM_ACTIVATE => {
            if (w & 0xFFFF) as u32 == WA_INACTIVE && IsIconic(hwnd) == 0 {
                FOCUS_LOST.store(true, Ordering::Release);
            }
            DefWindowProcW(hwnd, msg, w, l)
        }
        // its taskbar button's «close»: the program closes, the hole closes over it
        WM_CLOSE => {
            CLOSE_ASKED.store(true, Ordering::Release);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

/// The catcher over the game window's place `frame` (x y w h, screen px), hidden: shown with the first picture.
unsafe fn make_catch(frame: [i32; 4]) -> HWND {
    let class = wide("CursedRecodeTwinHold");
    let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(catch_proc),
        hInstance: instance,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        lpszClassName: class.as_ptr(),
        ..std::mem::zeroed()
    };
    RegisterClassExW(&wc);
    let title = wide(titlebar::TITLE);
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_APPWINDOW,
        class.as_ptr(),
        title.as_ptr(),
        WS_POPUP | WS_MINIMIZEBOX | WS_SYSMENU,
        frame[0],
        frame[1],
        frame[2],
        frame[3],
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        instance,
        std::ptr::null(),
    );
    if !hwnd.is_null() {
        // all but invisible, and still the cursor's: its picture is the window before the game's
        SetLayeredWindowAttributes(hwnd, 0, 1, LWA_ALPHA);
        titlebar::set_icons(hwnd);
    }
    for b in [&CATCH_IN, &CATCH_TRACK, &PILL_LIVE, &PILL_PRESS, &MIN_ASKED, &FOCUS_LOST] {
        b.store(false, Ordering::Release);
    }
    hwnd
}

#[derive(Clone, Copy)]
enum Mode {
    Launch,
    /// The menu's picture from the card's second `card` (and the desk's `fx`) on, the title at `left` / `foot_top`;
    /// `ask`: when it was asked for (QPC), `exit`: when the game's way out began; `view`: the game's look, cursor,
    /// hover glow, sheet lightning then. `open`: how far the window had opened, when it was closed while opening (-1:
    /// open, the game's way out). `kind`: from the main menu (0) or a world (1).
    Farewell { card: f32, fx: f32, left: f32, foot_top: f32, ask: i64, exit: i64, view: [f32; 7], open: f32, kind: u32 },
}

enum Ended {
    /// The game's window took over.
    Released,
    /// Its taskbar button closed it while the card played: the card's second then, the desk's, how far it had opened.
    Closed { t: f32, fx: f32, open: f32 },
    /// The farewell is over.
    Gone,
    Failed,
}

/// What lives for the whole session: the device, the card's renderer, this window's own pipeline.
struct Kit {
    gpu: Box<Gpu>,
    launch: Option<Launch>,
    maker: Option<std::thread::JoinHandle<crate::launch::Made>>,
    twin: Compute,
    sampler: vk::Sampler,
    /// The game window's own title bar (chrome/), made with its first showing.
    painter: Option<crate::chrome::paint::Painter>,
}

impl Kit {
    fn new() -> Option<Kit> {
        let gpu = Box::new(Gpu::new(0, false, false).ok()?);
        // The card's pipelines are the heavy part (the driver compiles them the first time): made beside the spark,
        // which needs only this window's own.
        let gpu_ref: &'static Gpu = unsafe { &*(&*gpu as *const Gpu) };
        let maker = Some(std::thread::spawn(move || crate::launch::Made(Launch::new(gpu_ref))));
        let twin = Compute::new(
            &gpu,
            "launch_twin",
            &crate::launch::words(include_bytes!("../../shaders/launch/spv/twin.spv")),
            &[vk::DescriptorType::UNIFORM_BUFFER, vk::DescriptorType::COMBINED_IMAGE_SAMPLER, vk::DescriptorType::STORAGE_IMAGE, vk::DescriptorType::UNIFORM_BUFFER, vk::DescriptorType::COMBINED_IMAGE_SAMPLER],
            0,
        );
        let sampler = fail(
            unsafe {
                gpu.device.create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
            },
            "Can't make the twin's sampler",
        );
        Some(Kit { gpu, launch: None, maker, twin, sampler, painter: None })
    }

    /// The card's renderer once made (waiting for it only when `need`).
    fn renderer(&mut self, need: bool) -> Option<&mut Launch> {
        if self.launch.is_none() && self.maker.as_ref().is_some_and(|m| m.is_finished() || need) {
            self.launch = self.maker.take().and_then(|m| m.join().ok()).map(|m| m.0);
            note("the card's pipelines made");
        }
        self.launch.as_mut()
    }

    fn destroy(mut self) {
        let gpu = &self.gpu;
        unsafe {
            let _ = gpu.device.device_wait_idle();
        }
        if let Some(m) = self.maker.take() {
            if let Ok(l) = m.join() {
                l.0.destroy(gpu);
            }
        }
        if let Some(l) = self.launch.take() {
            l.destroy(gpu);
        }
        self.twin.destroy(gpu);
        if let Some(p) = self.painter.take() {
            p.destroy(gpu);
        }
        unsafe { gpu.device.destroy_sampler(self.sampler, None) };
        gpu.destroy();
    }
}

/// How the picture reaches the screen.
enum Out {
    Composed(present::Composed),
    /// Read back and UpdateLayeredWindow (the old way).
    Layered { back: Vec<Buffer>, mem: HDC, dib: HBITMAP, old: HGDIOBJ, bits: *mut c_void },
    /// Development: read back into files (Shots).
    Shots(Buffer),
}

/// The window over the desktop for the game's window at `p`: its place, size and DPI scale.
struct Win {
    hwnd: HWND,
    sx: i32,
    sy: i32,
    sw: i32,
    sh: i32,
    scale: f32,
}

/// (Never on the taskbar itself: before the game the catcher is the program there; the farewell has none.)
unsafe fn make_window(p: &Place2, composed: bool) -> Option<Win> {
    let class = wide("CursedRecodeTwin");
    let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
    let wc = WNDCLASSEXW { cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32, lpfnWndProc: Some(proc), hInstance: instance, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
    RegisterClassExW(&wc);
    let probe = CreateWindowExW(WS_EX_TOOLWINDOW, class.as_ptr(), std::ptr::null(), WS_POPUP, p.frame[0], p.frame[1], 1, 1, std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null());
    let dpi = if probe.is_null() { 96 } else { GetDpiForWindow(probe).max(96) };
    if !probe.is_null() {
        DestroyWindow(probe);
    }
    let scale = dpi as f32 / 96.0;
    let m: [i32; 4] = std::array::from_fn(|i| (MARGIN[i] * scale) as i32);
    let (sx, sy) = (p.frame[0] - m[0], p.frame[1] - m[1]);
    let (sw, sh) = (p.frame[2] + m[0] + m[2], p.frame[3] + m[1] + m[3]);
    let title = wide(titlebar::TITLE);
    let ex = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | if composed { WS_EX_NOREDIRECTIONBITMAP } else { 0 };
    let hwnd = CreateWindowExW(ex, class.as_ptr(), title.as_ptr(), WS_POPUP, sx, sy, sw, sh, std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null());
    if hwnd.is_null() {
        return None;
    }
    if composed {
        // layered for the clicks to go through it; its picture is the compositor's
        SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
    }
    titlebar::set_icons(hwnd);
    Some(Win { hwnd, sx, sy, sw, sh, scale })
}

unsafe fn make_layered(gpu: &Gpu, sw: i32, sh: i32) -> Out {
    let bytes = sw as u64 * sh as u64 * 4;
    let back: Vec<Buffer> = (0..2).map(|_| Buffer::with(gpu, vk::BufferUsageFlags::TRANSFER_DST, bytes, Place::Readback).expect("Can't make the twin's read-back")).collect();
    let screen = GetDC(std::ptr::null_mut());
    let mem = CreateCompatibleDC(screen);
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = sw;
    bmi.bmiHeader.biHeight = -sh;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut c_void = std::ptr::null_mut();
    let dib = CreateDIBSection(screen, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    let old = SelectObject(mem, dib);
    ReleaseDC(std::ptr::null_mut(), screen);
    Out::Layered { back, mem, dib, old, bits }
}

/// One showing of the window: the launch, or the farewell. Draws until it is over.
unsafe fn run(kit: &mut Kit, p: Place2, mode: Mode, l: &Link) -> Ended {
    SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let mut shots = SHOTS.lock().unwrap().take();
    if shots.is_some() {
        let m: [i32; 4] = std::array::from_fn(|i| MARGIN[i] as i32);
        let win = Win { hwnd: std::ptr::null_mut(), sx: p.frame[0] - m[0], sy: p.frame[1] - m[1], sw: p.frame[2] + m[0] + m[2], sh: p.frame[3] + m[1] + m[3], scale: 1.0 };
        let back = Buffer::with(&kit.gpu, vk::BufferUsageFlags::TRANSFER_DST, win.sw as u64 * win.sh as u64 * 4, Place::Readback).expect("Can't make the shots' read-back");
        return run_on(kit, p, mode, l, win, Out::Shots(back), shots.take());
    }
    // the window, on the compositor's swap chain if it can be made
    let Some(mut win) = make_window(&p, true) else { return Ended::Failed };
    note("its window made");
    let composed = if std::env::var_os("CURSED_DEV_NODCOMP").is_some() { Err("CURSED_DEV_NODCOMP".to_string()) } else { present::Composed::new(&kit.gpu, win.hwnd as *mut c_void, win.sw as u32, win.sh as u32) };
    let out = match composed {
        Ok(c) => {
            note("shown by the compositor");
            Out::Composed(c)
        }
        Err(e) => {
            note(&format!("the compositor's way failed ({e}); read back"));
            DestroyWindow(win.hwnd);
            let Some(w) = make_window(&p, false) else { return Ended::Failed };
            win = w;
            make_layered(&kit.gpu, win.sw, win.sh)
        }
    };
    run_on(kit, p, mode, l, win, out, None)
}

/// The showing's frames, into `out`.
unsafe fn run_on(kit: &mut Kit, p: Place2, mode: Mode, l: &Link, win: Win, mut out: Out, mut shots: Option<Shots>) -> Ended {
    let launch_mode = matches!(mode, Mode::Launch);
    // the device lives in its box for the whole session (the kit is only borrowed for the card's renderer below)
    let gpu: &Gpu = &*(&*kit.gpu as *const Gpu);
    let (sx, sy, sw, sh, scale) = (win.sx, win.sy, win.sw, win.sh, win.scale);
    let timeline = Encoder::timeline(gpu);
    let mut enc = Encoder::new(gpu, timeline);
    let progress = Progress::new();
    let cmd = enc.cmd(gpu);
    let (card_t, card_v) = image_in(gpu, cmd, "Twin card", vk::Format::R8G8B8A8_UNORM, p.client[2].max(1) as u32, p.client[3].max(1) as u32, 1, false);
    let (surf_t, surf_v) = image_in(gpu, cmd, "Twin picture", vk::Format::R8G8B8A8_UNORM, sw as u32, sh as u32, 1, false);
    // the game window's title bar, as Windows will draw it on the game's window
    let bar_h = (p.client[1] - p.frame[1]).max(1);
    let chrome = crate::chrome::wanted();
    let bar_w = if chrome { p.client[2] } else { p.frame[2] }.max(1);
    let (bar_t, bar_v) = image_in(gpu, cmd, "Twin title bar", vk::Format::R8G8B8A8_UNORM, bar_w as u32, bar_h as u32, 1, false);
    if chrome {
        // the game's window's own bar, still: as it will be the frame it takes over (or was, the farewell's first)
        let painter = kit.painter.get_or_insert_with(|| crate::chrome::paint::Painter::new(gpu, &mut enc, cmd));
        let look = crate::chrome::Look {
            width: bar_w,
            height: bar_h,
            scale,
            active: if !launch_mode && l.flags.load(Ordering::Acquire) & link::INACTIVE != 0 { 0.0 } else { 1.0 },
            maximized: !launch_mode && l.flags.load(Ordering::Acquire) & link::MAXIMIZED != 0,
            status: if launch_mode { String::new() } else { crate::chrome::status() },
            // (before the game no bar, only black: the game's comes down as this window goes; the farewell's is there)
            down: if launch_mode { 0.0 } else { 1.0 },
            ..crate::chrome::Look::default()
        };
        painter.draw(gpu, &mut enc, cmd, &look, bar_v.raw);
    } else {
        crate::launch::upload(gpu, &mut enc, cmd, &bar_t, &titlebar::render(p.frame[2], bar_h, scale));
    }
    note("its pictures made");

    let start = link::qpc();
    let epoch = l.epoch.load(Ordering::Acquire);
    let mut last = Instant::now();
    let mut released_at: Option<Instant> = None;
    let mut first_shown: Option<u64> = None;
    let mut frame = 0u64;
    let mut gaps: Vec<(f32, f32)> = Vec::new();
    let mut held = 0.0f32;
    let mut hold_done = false;
    // the story's clock (while the point runs it keeps to the game's loading), its pace, the desk's second it was last
    // moved at; when the game's resources began to load, and when they did the last times (the desk's seconds)
    let from = if launch_mode { f32::from_bits(STORY_FROM.load(Ordering::Acquire)) } else { 0.0 };
    let (mut story, mut pace, mut story_d) = (from, 1.0f32, from);
    let mut res_at: Option<f32> = None;
    let res_was = if launch_mode && shots.is_none() { res_read() + from } else { tm::RES_AT };
    let mut d_prev = 0.0f32;
    let mut counts_from: Option<(u32, u32)> = None;
    let mut intervals: Vec<f32> = Vec::with_capacity(4096);
    let mut works: Vec<f32> = Vec::with_capacity(4096);
    // the frame drawn last time (its submit, its shared texture): shown at the start of this one, by then done
    let mut pending: Option<(u64, usize)> = None;
    let mut ended = Ended::Gone;
    let (gx, gy) = ((p.client[0] - sx) as f32, (p.client[1] - sy) as f32);
    let (fx0, fy0) = ((p.frame[0] - sx) as f32, (p.frame[1] - sy) as f32);
    let (ui, left0, foot0) = crate::launch::timeline::menu_anchor(p.client[2] as f32, p.client[3] as f32);
    let (left0, foot0) = match mode {
        Mode::Farewell { left, foot_top, .. } if left >= 0.0 => (left, foot_top),
        _ => (left0, foot0),
    };
    // where the spark is: where the menu's glow will be (the card's tip), in this window's px; the window's edges from it
    let lay = tm::layout(p.client[2] as f32 / ui, p.client[3] as f32 / ui, left0 / ui, foot0 / ui, 115.5);
    let spark_px = [gx + lay.tip[0] * ui, gy + lay.tip[1] * ui];
    let (tip_up, tip_down) = ((spark_px[1] - fy0) / scale, (fy0 + p.frame[3] as f32 - spark_px[1]) / scale);
    let (line_l, line_r) = ((spark_px[0] - fx0) / scale, (fx0 + p.frame[2] as f32 - spark_px[0]) / scale);
    let corner = if titlebar::windows11() { 8.0 } else { 0.0 };
    // the program for Windows before the game's window is there (the catcher), and its lone minimise at the top right
    // (our own bar only), in the catcher's px
    let bw = crate::chrome::button_width((96.0 * scale) as u32);
    let mut catcher: HWND = if launch_mode && shots.is_none() { make_catch(p.frame) } else { std::ptr::null_mut() };
    if chrome {
        let x1 = p.client[0] - p.frame[0] + bar_w;
        for (a, v) in PILL_AT.iter().zip([x1 - bw, 0, x1, bar_h]) {
            a.store(v, Ordering::Release);
        }
    }
    // the minimise: how far down, how lit; when the cursor was last near the top
    let mut pill = [0.0f32; 2];
    let mut near_at: Option<Instant> = None;
    // minimised (the catcher is), how much of the picture still shows, its window hidden; when the focus went away
    let (mut minimised, mut vis, mut win_hidden) = (false, 1.0f32, false);
    let mut focus_lost: Option<Instant> = None;
    // development: CURSED_DEV_TWIN_PILL=near|hover — the cursor at the top, over the minimise (a stand's posted moves
    // cannot hold it there)
    let dev_pill = std::env::var("CURSED_DEV_TWIN_PILL").ok();
    // the valley: as the game's options make it (the menu's, which the game's window takes over), on the way out as the
    // game drew it last
    let valley: [f32; 29] = if !launch_mode && l.scene_set.load(Ordering::Acquire) == 1 {
        std::array::from_fn(|k| link::f32_of(&l.scene[k]))
    } else {
        crate::launch::valley()
    };
    if !launch_mode {
        // the crows as the game's: startled when its menu came alive (in this program they never were — it had gone)
        let s = link::f32_of(&l.startled);
        if s >= 0.0 {
            crate::launch::ink::set_startled(s);
        }
    }
    note(&format!("the valley: fov {:.0}, fog {:.2}, far {:.0}; the crows startled at {:.2}", valley[4], valley[5 + 8], valley[5 + 9], crate::launch::ink::startled()));
    loop {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // paced by the compositor: one frame each refresh
        let t_wait = Instant::now();
        if let Out::Composed(c) = &out {
            c.wait(50);
        }
        let shot_clock = shots.as_ref().map(|s| s.dt * frame as f32);
        let mut presented = false;
        if let (Out::Composed(c), Some((index, slot))) = (&mut out, pending.take()) {
            if enc.await_submit(gpu, index, 1_000_000_000) {
                presented = c.present(slot);
            }
        }
        let now_i = Instant::now();
        let waited = (now_i - t_wait).as_secs_f32();
        let gap = (now_i - last).as_secs_f32();
        if frame > 2 && gap > 0.034 {
            gaps.push((d_prev, gap));
        }
        let dt_s = shots.as_ref().map_or(gap.min(0.1), |s| s.dt);
        if frame > 2 {
            intervals.push(gap);
        }
        last = now_i;
        let now = shot_clock.map_or_else(link::qpc, |c| link::qpc_at(epoch, c));
        let d = link::secs(epoch, now);
        if let Some(s) = shots.as_ref() {
            if d >= s.ready_at {
                l.flags.fetch_or(link::READY, Ordering::AcqRel);
            }
            // (the game's resources as they would load: from `said_from` to the ready)
            if d >= s.said_from {
                link::set_f32(&l.load, ((d - s.said_from) / (s.ready_at - s.said_from).max(0.1)).clamp(0.0, 1.0) * 0.98);
            }
        }
        let mut i = Inputs {
            t: 0.0,
            fx: 0.0,
            life: d,
            dt: dt_s,
            launched: false,
            gather: 0.0,
            smoke_reset: false,
            cursor: [0.5, 0.5, 0.0],
            look: [0.0; 2],
            boost: 0.0,
            flash_c: 0.0,
            fade: 1.0,
            ui,
            left: left0,
            foot_top: foot0,
            hover: [[0.0; 5]; 2],
            hover_sparks: 0.0,
            lines: [[0.0; 8]; LINES],
            icons: [[0.0, 0.0, 0.0, -1.0]; 3],
            icon_light: [0.0; 4],
            disc: [0.0; 8],
            no_rays: false,
            no_bloom: false,
            menus_fire: false,
            knobs: valley[5..].try_into().unwrap(),
            pose: valley[..5].try_into().unwrap(),
            phase: Phase::Run,
            pt: 0.0,
            load: 1.0,
            loaded_at: -1.0,
            world: None,
            quit: -1.0,
        };
        let mut fade = 1.0f32;
        let bar_k: f32;
        let (desk, card_on, done) = match mode {
            Mode::Launch => {
                let flags = l.flags.load(Ordering::Acquire);
                let released = flags & link::RELEASE != 0;
                // the story's clock: the desk's, but while the point rewrites the title it keeps to the game's
                // loading — slower as the point comes up to how far the game has got, waiting there, faster once the
                // game is ready — so the point never stops at a letter to leap to the end; at the rewrite's end it
                // waits for the game's menu. The desk's clock never waits.
                if !hold_done {
                    let ready = flags & link::READY != 0 || d > GIVE_UP || released;
                    let said = link::f32_of(&l.load);
                    if said >= 0.0 && res_at.is_none() {
                        res_at = Some(d);
                    }
                    let dd = (d - story_d).max(0.0);
                    story_d = d;
                    let goal = tm::game_load(d, said, ready, res_was);
                    pace += (tm::story_pace(story, goal, ready, d - story) - pace) * (1.0 - (-dd / 0.18).exp());
                    story = (story + pace * dd).min(if ready { f32::MAX } else { tl::HOLD_AT });
                    held = d - story;
                    if ready && (story >= tl::HOLD_AT || held < 0.002 && (pace - 1.0).abs() < 0.002) {
                        hold_done = true;
                        held = held.max(0.0);
                        link::set_f32(&l.held, held);
                        l.hold_done.store(1, Ordering::Release);
                        note(&format!("the game is ready: the card waited {held:.2} s"));
                        if flags & link::READY != 0 && shots.is_none() {
                            res_write(res_was, res_at, d);
                        }
                    }
                }
                let t = if shots.as_ref().is_some_and(|s| s.menu) { 1000.0 } else { d - held };
                let desk = tm::desk_launch(d, tip_up, tip_down, line_l, line_r);
                if released_at.is_none() {
                    crate::launch::sound::launch(d, t, dt_s);
                }
                i.launched = true;
                i.t = t;
                i.fx = d;
                i.pt = t;
                link::set_f32(&l.card, t);
                bar_k = smooth(tm::lt::CHROME, tm::lt::CHROME + 0.4, d);
                if released_at.is_none() && released {
                    released_at = Some(now_i);
                    note("the game's window is up under it: going");
                    // the game's window is the program now: its own taskbar button, the focus, its bar
                    if !catcher.is_null() {
                        PILL_LIVE.store(false, Ordering::Release);
                        DestroyWindow(catcher);
                        catcher = std::ptr::null_mut();
                    }
                }
                if !catcher.is_null() && released_at.is_none() {
                    let game_pid = l.game_pid.load(Ordering::Acquire);
                    // the focus gone: to another program's window — a click elsewhere: the window minimises —, or to the
                    // game's (the hand-over) or to one of this process's (nothing)
                    if FOCUS_LOST.swap(false, Ordering::AcqRel) && !quiet() {
                        focus_lost = Some(now_i);
                    }
                    if let Some(at) = focus_lost {
                        let fg = GetForegroundWindow();
                        let mut pid = 0u32;
                        if !fg.is_null() {
                            GetWindowThreadProcessId(fg, &mut pid);
                        }
                        if fg == catcher || pid == windows_sys::Win32::System::Threading::GetCurrentProcessId() || pid != 0 && pid == game_pid {
                            focus_lost = None;
                        } else if pid != 0 || (now_i - at).as_secs_f32() > 0.25 {
                            focus_lost = None;
                            if IsIconic(catcher) == 0 {
                                note("the focus went to another program: minimised");
                                ShowWindow(catcher, SW_MINIMIZE);
                            }
                        }
                    }
                    if MIN_ASKED.swap(false, Ordering::AcqRel) && IsIconic(catcher) == 0 {
                        note("its minimise: minimised");
                        ShowWindow(catcher, SW_MINIMIZE);
                    }
                    let iconic = IsIconic(catcher) != 0;
                    if iconic != minimised {
                        minimised = iconic;
                        if minimised {
                            l.flags.fetch_or(link::HIDDEN, Ordering::AcqRel);
                        } else {
                            l.flags.fetch_and(!link::HIDDEN, Ordering::AcqRel);
                            note("restored");
                            if win_hidden {
                                win_hidden = false;
                                ShowWindow(win.hwnd, SW_SHOWNOACTIVATE);
                                SetWindowPos(win.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                            }
                            // (the focus is this process's again: the game may take it at the hand-over)
                            AllowSetForegroundWindow(game_pid);
                        }
                    }
                }
                // the picture goes as the catcher minimises and comes back as it is restored
                vis = if minimised { (vis - dt_s / 0.16).max(0.0) } else { (vis + dt_s / 0.2).min(1.0) };
                if minimised && vis <= 0.0 && !win_hidden {
                    win_hidden = true;
                    ShowWindow(win.hwnd, SW_HIDE);
                }
                if let Some(at) = released_at {
                    // (minimised as the game took over: its window came up minimised, nothing to fade)
                    fade = if minimised { 0.0 } else { 1.0 - ((now_i - at).as_secs_f32() / FADE).min(1.0) };
                }
                if CLOSE_ASKED.swap(false, Ordering::AcqRel) && released_at.is_none() {
                    ended = Ended::Closed { t, fx: d, open: smoother(tm::lt::OPEN_FROM, tm::lt::OPEN_TO, d) };
                    break;
                }
                (desk, d >= tm::lt::OPEN_FROM - 0.05, fade <= 0.0)
            }
            Mode::Farewell { card, fx, left: _, foot_top: _, ask, exit, view, open, kind } => {
                let a = link::secs(ask, now);
                let (x, e) = if open >= 0.0 { (QUICK, a) } else if kind == 1 { (tm::XW, link::secs(exit, now)) } else { (tm::XM, link::secs(exit, now)) };
                // closed while opening: from as far as it had opened
                let k = if open >= 0.0 { open } else { 1.0 };
                let desk = tm::desk_closing(e, &x, tip_up * k, tip_down * k, line_l, line_r);
                crate::launch::sound::way_out(e, &x);
                i.launched = true;
                if open >= 0.0 {
                    i.t = card;
                    i.pt = card;
                    i.quit = a;
                } else {
                    i.phase = if kind == 1 { Phase::ExitWorld } else { Phase::ExitMenu };
                    i.t = card;
                    i.pt = e;
                }
                i.load = 1.0;
                i.fx = fx + a;
                let rest = 1.0 - smoother(0.0, 1.2, a);
                i.look = [view[0] * rest, view[1] * rest];
                i.cursor = [view[2], view[3], view[4] * rest];
                i.boost = view[5] * (-2.2 * a).exp();
                i.flash_c = view[6] * (-6.5 * a).exp();
                // (v13: the bar goes with the pane as it thaws)
                bar_k = 1.0 - smooth(x.close_from, x.close_to - 0.15, e);
                (desk, e < x.close_to + 0.3, e >= x.wisp_to)
            }
        };
        if done {
            break;
        }
        // minimised and off the screen: nothing to draw (the clocks and the music go on)
        if win_hidden {
            last = Instant::now();
            std::thread::sleep(Duration::from_millis(15));
            continue;
        }
        // record: the card into its picture, this window's picture, and out
        let t_rec = Instant::now();
        let slot = (frame % 2) as usize;
        let card_on = card_on && kit.renderer(launch_mode && d > tm::lt::OPEN_FROM - 0.3 || !launch_mode).is_some();
        let cmd = enc.cmd(gpu);
        // the lone minimise: down into the bar while the cursor is near the top (and a moment after), lit over it; the
        // bar drawn again while anything of it moves
        if !catcher.is_null() && chrome {
            let (cx, cy, inside) = (CATCH_X.load(Ordering::Acquire), CATCH_Y.load(Ordering::Acquire), CATCH_IN.load(Ordering::Acquire));
            let dev = dev_pill.as_deref();
            if dev.is_some() || inside && cy >= 0 && cy < bar_h + (18.0 * scale) as i32 {
                near_at = Some(now_i);
            }
            let want = bar_k > 0.5 && !minimised && (near_at.is_some_and(|t| (now_i - t).as_secs_f32() < 0.45) || PILL_PRESS.load(Ordering::Acquire));
            let before = pill;
            pill[0] = if want { (pill[0] + dt_s / 0.22).min(1.0) } else { (pill[0] - dt_s / 0.3).max(0.0) };
            PILL_LIVE.store(pill[0] > 0.6, Ordering::Release);
            let hover = pill[0] > 0.6 && (dev == Some("hover") || inside && over_pill(cx, cy));
            pill[1] = if hover { (pill[1] + dt_s * 9.0).min(1.0) } else { (pill[1] - dt_s * 6.0).max(0.0) };
            if pill != before {
                if let Some(painter) = kit.painter.as_mut() {
                    let look = crate::chrome::Look { width: bar_w, height: bar_h, scale, time: d, active: 1.0, pill: pill[0], pill_hover: pill[1], ..crate::chrome::Look::default() };
                    memory_barrier(gpu, cmd);
                    painter.draw(gpu, &mut enc, cmd, &look, bar_v.raw);
                }
            }
        }
        if card_on {
            if let Some(lr) = kit.launch.as_mut() {
                lr.frame(gpu, &mut enc, &i, &[], &card_t);
            }
        }
        let mut u = [0f32; 36];
        let (pw, ph) = (sw as f32, sh as f32);
        u[0..4].copy_from_slice(&[gx, ph - (gy + p.client[3] as f32), p.client[2] as f32, p.client[3] as f32]);
        u[4..8].copy_from_slice(&[fx0, ph - (fy0 + p.frame[3] as f32), p.frame[2] as f32, p.frame[3] as f32]);
        u[8..12].copy_from_slice(&[spark_px[0], ph - spark_px[1], 0.0, d]);
        u[12..16].copy_from_slice(&[desk.left, desk.right, desk.line, desk.head]);
        u[16..20].copy_from_slice(&[desk.up, desk.down, desk.edge, desk.closing]);
        u[20..24].copy_from_slice(&[fade * vis, if card_on { 1.0 } else { 0.0 }, scale, corner]);
        u[24..28].copy_from_slice(&[pw, ph, desk.spark, desk.wisp]);
        let cap = titlebar::caption_rgb();
        // (the bar's picture spans the picture's width with our bar, the frame's with Windows')
        u[28..32].copy_from_slice(&[cap[0], cap[1], cap[2], if chrome { 2.0 } else { 1.0 }]);
        u[32..36].copy_from_slice(&[desk.shadow, bar_k, 1.0, 0.0]);
        let piece = enc.transient.mapped(gpu, 144, gpu.info.min_uniform_align.max(16) as u64, 144, 1);
        std::ptr::copy_nonoverlapping(u.as_ptr() as *const u8, piece.host, 144);
        let ubo = vk::DescriptorBufferInfo { buffer: (*piece.buffer).raw, offset: piece.offset, range: 144 };
        // (the launch's block the shared head declares: unread here, but bound whole)
        let dummy = enc.transient.mapped(gpu, 896, gpu.info.min_uniform_align.max(16) as u64, 896, 1);
        let dubo = vk::DescriptorBufferInfo { buffer: (*dummy.buffer).raw, offset: dummy.offset, range: 896 };
        let images = [
            vk::DescriptorImageInfo { sampler: kit.sampler, image_view: card_v.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: surf_v.raw, image_layout: vk::ImageLayout::GENERAL },
            vk::DescriptorImageInfo { sampler: kit.sampler, image_view: bar_v.raw, image_layout: vk::ImageLayout::GENERAL },
        ];
        let writes = [
            vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&dubo)),
            vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&images[0])),
            vk::WriteDescriptorSet::default().dst_binding(2).descriptor_type(vk::DescriptorType::STORAGE_IMAGE).image_info(std::slice::from_ref(&images[1])),
            vk::WriteDescriptorSet::default().dst_binding(3).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(std::slice::from_ref(&ubo)),
            vk::WriteDescriptorSet::default().dst_binding(4).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(std::slice::from_ref(&images[2])),
        ];
        let dev = &gpu.device;
        dev.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, kit.twin.pipeline);
        gpu.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::COMPUTE, kit.twin.layout, 0, &writes);
        dev.cmd_dispatch(cmd, (sw as u32).div_ceil(8), (sh as u32).div_ceil(8), 1);
        memory_barrier(gpu, cmd);
        match &mut out {
            Out::Composed(c) => {
                // Direct3D's copy out of this shared texture (two frames ago) is done before ours goes in
                c.idle(slot);
                present::copy_into(gpu, cmd, surf_t.image, c.images[slot], sw as u32, sh as u32);
            }
            Out::Shots(back) => {
                let region = vk::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 },
                    image_offset: vk::Offset3D::default(),
                    image_extent: vk::Extent3D { width: sw as u32, height: sh as u32, depth: 1 },
                };
                dev.cmd_copy_image_to_buffer(cmd, surf_t.image, vk::ImageLayout::GENERAL, back.raw, &[region]);
                memory_barrier(gpu, cmd);
            }
            Out::Layered { back, .. } => {
                let region = vk::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 },
                    image_offset: vk::Offset3D::default(),
                    image_extent: vk::Extent3D { width: sw as u32, height: sh as u32, depth: 1 },
                };
                dev.cmd_copy_image_to_buffer(cmd, surf_t.image, vk::ImageLayout::GENERAL, back[(frame % 2) as usize].raw, &[region]);
                memory_barrier(gpu, cmd);
            }
        }
        let recorded = t_rec.elapsed().as_secs_f32();
        enc.submit(gpu, &[], &progress);
        let t_gpu = Instant::now();
        let mut gpu_done = 0.0f32;
        let mut visible = false;
        match &mut out {
            Out::Shots(back) => {
                let s = shots.as_mut().unwrap();
                crate::launch::sound::pump(dt_s);
                if s.times.first().is_some_and(|&t| d + 1e-4 >= t) {
                    let at = s.times.remove(0);
                    if enc.await_submit(gpu, enc.current - 1, 5_000_000_000) {
                        let bytes = std::slice::from_raw_parts(back.ptr() as *const u8, sw as usize * sh as usize * 4);
                        let _ = std::fs::write(s.dir.join(format!("d{at:06.2}_{sw}x{sh}.bgra")), bytes);
                    }
                }
                if s.times.is_empty() {
                    break;
                }
            }
            Out::Composed(_) => {
                // shown next time round, when our GPU has done it (it has a refresh for it)
                pending = Some((enc.current - 1, slot));
                visible = presented;
                gpu_done = 0.0;
            }
            Out::Layered { back, mem, bits, .. } => {
                // the frame before this one is done: onto the window
                if frame > 0 {
                    let bytes = sw as usize * sh as usize * 4;
                    std::ptr::copy_nonoverlapping(back[((frame + 1) % 2) as usize].ptr(), *bits as *mut u8, bytes);
                    let pos = POINT { x: sx, y: sy };
                    let size = SIZE { cx: sw, cy: sh };
                    let src = POINT { x: 0, y: 0 };
                    let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
                    UpdateLayeredWindow(win.hwnd, std::ptr::null_mut(), &pos, &size, *mem, &src, 0, &blend, ULW_ALPHA);
                    visible = true;
                }
                DwmFlush();
            }
        }
        if visible && first_shown.is_none() {
            ShowWindow(win.hwnd, SW_SHOWNOACTIVATE);
            SetWindowPos(win.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            first_shown = Some(frame);
            // the catcher is the program for Windows from now on: shown and given the focus while this process still has
            // it (the loader's window goes next), the game allowed to take it at the hand-over
            if !catcher.is_null() && quiet() {
                ShowWindow(catcher, SW_SHOWNOACTIVATE);
            } else if !catcher.is_null() {
                ShowWindow(catcher, SW_SHOW);
                SetForegroundWindow(catcher);
                AllowSetForegroundWindow(l.game_pid.load(Ordering::Acquire));
            }
            // the loader's window under it, on the same picture: it goes
            let under = HIDE_ON_SHOW.swap(0, Ordering::AcqRel);
            if under != 0 {
                ShowWindow(under as HWND, SW_HIDE);
            }
            note(if launch_mode { "the first picture over the desktop" } else { "the farewell's first picture" });
        }
        // the farewell's first picture is on screen (one refresh after it was shown): the game hides its window
        if !launch_mode && first_shown.is_some_and(|f| frame >= f + 1) && l.shown.load(Ordering::Acquire) == 0 {
            l.shown.store(1, Ordering::Release);
            note(&format!("the game told its window may go ({:.0} ms after it asked)", link::secs(l.ask_qpc.load(Ordering::Acquire), link::qpc()) * 1e3));
        }
        // development (--log): a frame that took long, by its parts
        let total = now_i.elapsed().as_secs_f32();
        works.push(total);
        if total > 0.012 && frame > 1 {
            let prof = kit.launch.as_ref().map_or(String::new(), |l| format!(" | card GPU ms: smoke {:.1} scene {:.1} sparks {:.1} glow {:.1} rays {:.1} text {:.1} picture {:.1}", l.prof_ms[0], l.prof_ms[1], l.prof_ms[2], l.prof_ms[3], l.prof_ms[4], l.prof_ms[5], l.prof_ms[6]));
            note(&format!("frame {frame} at {d:.2} s: waited {:.1} ms, recorded {:.1}, GPU {:.1}, present {:.1} (total {:.1}){prof}", waited * 1e3, recorded * 1e3, gpu_done * 1e3, (t_gpu.elapsed().as_secs_f32() - gpu_done) * 1e3, total * 1e3));
        }
        if frame == 10 {
            if let Out::Composed(c) = &out {
                counts_from = c.counts();
            }
        }
        frame += 1;
        d_prev = d;
    }
    // how smooth it was (the program's log)
    let worst = gaps.iter().map(|g| g.1).fold(0.0f32, f32::max);
    let list: Vec<String> = gaps.iter().take(12).map(|(d, g)| format!("{d:.1}s:{:.0}ms", g * 1e3)).collect();
    let secs = link::secs(start, link::qpc()).max(1e-3);
    if let Out::Composed(c) = &out {
        note(&c.stats(counts_from));
    }
    if intervals.len() > 10 {
        // a refresh missed: a frame that took half a refresh more than most
        let mut sorted = intervals.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let median = sorted[sorted.len() / 2];
        let missed = intervals.iter().filter(|&&g| g > median * 1.5).count();
        works.sort_by(|a, b| a.total_cmp(b));
        let pct = |q: f32| works[((works.len() - 1) as f32 * q) as usize] * 1e3;
        note(&format!("frame to frame: median {:.2} ms ({:.0} Hz), {missed} of {} took a refresh or more extra; a frame's work {:.1} / {:.1} / {:.1} ms (half, 95%, worst)", median * 1e3, 1.0 / median, intervals.len(), pct(0.5), pct(0.95), pct(1.0)));
    }
    note(&format!("{frame} frames in {secs:.1} s ({:.0} a second), {} over 34 ms (worst {:.0} ms): {}", frame as f32 / secs, gaps.len(), worst * 1e3, list.join(" ")));
    // done: everything goes, the window last
    let _ = gpu.device.device_wait_idle();
    match out {
        Out::Shots(b) => crate::vk::resources::Doomed::Buffer(Box::new(b)).destroy(gpu),
        Out::Composed(c) => c.destroy(gpu),
        Out::Layered { back, mem, dib, old, .. } => {
            SelectObject(mem, old);
            DeleteObject(dib);
            DeleteDC(mem);
            for b in back {
                crate::vk::resources::Doomed::Buffer(Box::new(b)).destroy(gpu);
            }
        }
    }
    if !catcher.is_null() {
        PILL_LIVE.store(false, Ordering::Release);
        DestroyWindow(catcher);
    }
    if !win.hwnd.is_null() {
        DestroyWindow(win.hwnd);
    }
    for t in [card_t, surf_t, bar_t] {
        let mut t = t;
        gpu.device.destroy_image(t.image, None);
        if let Some(a) = t.alloc.take() {
            gpu.memory.free(&gpu.device, a);
        }
    }
    for v in [card_v, surf_v, bar_v] {
        gpu.device.destroy_image_view(v.raw, None);
    }
    enc.destroy(gpu);
    gpu.device.destroy_semaphore(timeline, None);
    if launch_mode && matches!(ended, Ended::Gone) {
        ended = Ended::Released;
    }
    ended
}

// ---- the window's program (src/bin/cursed_twin.rs) or thread ---------------------------------------------------

fn options_volumes(game_dir: Option<&str>) -> (f32, f32) {
    let mut v = (1.0f32, 1.0f32);
    if let Some(d) = game_dir {
        if let Ok(text) = std::fs::read_to_string(std::path::Path::new(d).join("options.txt")) {
            for line in text.lines() {
                if let Some(x) = line.strip_prefix("soundCategory_master:") {
                    v.0 = x.trim().parse().unwrap_or(1.0);
                }
                if let Some(x) = line.strip_prefix("soundCategory_music:") {
                    v.1 = x.trim().parse().unwrap_or(1.0);
                }
            }
        }
    }
    v
}

/// Whether the game's process has gone (it joined, then ended or fell). `handle`: kept between calls (as a number:
/// it crosses threads).
fn game_gone(l: &Link, handle: &mut Option<usize>) -> bool {
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    let pid = l.game_pid.load(Ordering::Acquire);
    if pid == 0 || pid == std::process::id() {
        return false;
    }
    unsafe {
        if handle.is_none() {
            let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if h.is_null() {
                return true;
            }
            *handle = Some(h as usize);
        }
        WaitForSingleObject(handle.unwrap() as *mut c_void, 0) == 0
    }
}

/// Ends the game's process (the program was closed before the game showed anything: it has nothing to keep).
fn end_game(l: &Link) {
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    let pid = l.game_pid.load(Ordering::Acquire);
    if pid == 0 || pid == std::process::id() {
        return;
    }
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !h.is_null() {
            TerminateProcess(h, 0);
            windows_sys::Win32::Foundation::CloseHandle(h);
        }
    }
}

/// The music and the volumes as the game asks through the link; the music's state back.
fn serve_music(l: &Link, mute: bool, seq: &mut u32) {
    use crate::launch::sound;
    let (m, v) = (link::f32_of(&l.volume[0]), link::f32_of(&l.volume[1]));
    sound::volume(if mute { 0.0 } else { m }, v);
    let now = l.music_seq.load(Ordering::Acquire);
    if now != *seq {
        *seq = now;
        match l.music_cmd.load(Ordering::Acquire) {
            0 => {
                sound::music_toggle();
            }
            1 => sound::music_restart(),
            2 => sound::music_out(1.0),
            _ => sound::music_in(7.0),
        }
    }
    let (pos, len, on) = sound::music_state();
    link::set_f32(&l.music[0], pos);
    link::set_f32(&l.music[1], len);
    link::set_f32(&l.music[2], on as i32 as f32);
}

/// The program: up at once over the desktop for a game window `w` × `h` (0: most of the screen) or at `at`, the card
/// while the game loads, the music for the session, the farewell when the game ends. `game_dir`: its options' volumes.
pub fn serve(name: &str, w: i32, h: i32, at: Option<[i32; 2]>, game_dir: Option<&str>, mute: bool, started: Instant, log: Option<std::path::PathBuf>) {
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // the game loads beside it on every core: its frames come first
        use windows_sys::Win32::System::Threading::*;
        SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL_PRIORITY_CLASS);
    }
    serve_here(name, w, h, at, game_dir, mute, started, log);
}

/// The link of the window this process serves (CursedRecode.exe: the game runs in the same process).
static HOST_LINK: std::sync::OnceLock<&'static Link> = std::sync::OnceLock::new();

/// The same as a thread of the game's own process (CursedRecode.exe, src/bin/host.rs): the game joins it by `name`.
pub fn serve_here(name: &str, w: i32, h: i32, at: Option<[i32; 2]>, game_dir: Option<&str>, mute: bool, started: Instant, log: Option<std::path::PathBuf>) {
    let _ = STARTED.set(started);
    let _ = LOG.set(log);
    let _ = GAME_DIR.set(game_dir.map(std::path::PathBuf::from));
    note("up");
    app_id();
    let Some(l) = link::create(name) else { return };
    let _ = HOST_LINK.set(l);
    l.epoch.store(link::qpc_ago(started.elapsed()), Ordering::Release);
    let (master, music) = options_volumes(game_dir);
    link::set_f32(&l.volume[0], master);
    link::set_f32(&l.volume[1], music);
    crate::launch::sound::volume(if mute { 0.0 } else { master }, music);
    let p = place(w, h, at);
    write_place(l, &p);
    serve_link(l, p, mute);
}

/// The loader (loader/app.rs) as the window before the game for the game it starts: the link `name`, made before the
/// game is started (it finds it by name at once), for a game window at `p` (the loader's window's own place); the
/// story from its second `from` (the loader drew the valley into its outlines), the desk's clock at `from` now and the
/// valley's `life_shift` ahead of it, the crows as the loader's were; `under` (the loader's window) hidden once this
/// one is up over it on the same picture. Then `serve_loader` plays its whole life.
pub struct LoaderLink {
    link: &'static Link,
    place: Place2,
    mute: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn link_for_loader(name: &str, p: Place2, from: f32, life_shift: f32, under: isize, game_dir: Option<&str>, mute: bool, log: Option<std::path::PathBuf>) -> Option<LoaderLink> {
    let _ = STARTED.set(Instant::now());
    let _ = LOG.set(log);
    let _ = GAME_DIR.set(game_dir.map(std::path::PathBuf::from));
    // (a close asked of the window before the last game is not this one's)
    CLOSE_ASKED.store(false, Ordering::Release);
    let l = link::create(name)?;
    l.epoch.store(link::qpc_ago(Duration::from_secs_f32(from.max(0.0))), Ordering::Release);
    link::set_f32(&l.life_shift, life_shift);
    link::set_f32(&l.startled, crate::launch::ink::startled());
    crate::launch::ink::set_life_shift(life_shift);
    let (master, music) = options_volumes(game_dir);
    link::set_f32(&l.volume[0], master);
    link::set_f32(&l.volume[1], music);
    crate::launch::sound::volume(if mute { 0.0 } else { master }, music);
    write_place(l, &p);
    STORY_FROM.store(from.to_bits(), Ordering::Release);
    HIDE_ON_SHOW.store(under, Ordering::Release);
    note(&format!("the loader's: the story from {from:.2} s, the valley {life_shift:.2} s ahead"));
    Some(LoaderLink { link: l, place: p, mute })
}

impl LoaderLink {
    /// The game's process, started: its end is seen even if it never joins.
    pub fn game_started(&self, pid: u32) {
        self.link.game_pid.store(pid, Ordering::Release);
    }
}

/// The launch, the session, the farewell (see link_for_loader): returns when the game has ended and its farewell is
/// over.
pub fn serve_loader(l: LoaderLink) {
    serve_link(l.link, l.place, l.mute);
}

/// The game has ended in this process (CursedRecode.exe): if its farewell is playing, waits for it (at most `most`).
pub fn wait_farewell(most: Duration) {
    let Some(l) = HOST_LINK.get() else { return };
    let until = Instant::now() + most;
    loop {
        let leaving = l.state.load(Ordering::Acquire) == link::LEAVING || l.flags.load(Ordering::Acquire) & link::FAREWELL != 0 && l.state.load(Ordering::Acquire) != link::DONE;
        if !leaving || Instant::now() > until {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The window's whole life on link `l`: the launch, then the session (the music), then the farewell.
fn serve_link(l: &'static Link, p: Place2, mute: bool) {
    let local = LOCAL.load(Ordering::Acquire);
    unsafe {
        use windows_sys::Win32::System::Threading::*;
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
    }
    crate::launch::sound::reset();
    // the valley as her options make it, as the game's menu will show it (launch::valley)
    if let Some(o) = options_valley() {
        crate::launch::set_options(o);
        note(&format!("the valley by the game's options {o:?}"));
    }
    let Some(mut kit) = Kit::new() else {
        l.state.store(link::DONE, Ordering::Release);
        return;
    };
    note("its device made");
    l.state.store(link::PLAYING, Ordering::Release);
    // while it plays, a helper keeps the music in step with the game (a program of its own only: in the game's
    // process the game drives the music itself) and watches for the game's end
    let playing = std::sync::Arc::new(AtomicBool::new(true));
    let p2 = playing.clone();
    let from_loader = f32::from_bits(STORY_FROM.load(Ordering::Acquire)) > 0.0;
    let helper = std::thread::spawn(move || {
        let mut seq = 0u32;
        let mut handle = None;
        let since = Instant::now();
        while p2.load(Ordering::Acquire) {
            if !local {
                serve_music(l, mute, &mut seq);
            }
            if game_gone(l, &mut handle) {
                // a game that fell: nothing will take the card over
                l.flags.fetch_or(link::RELEASE, Ordering::AcqRel);
            }
            // the loader's game never joined (another build's, or it fell before it could): its window is its own
            if from_loader && l.flags.load(Ordering::Acquire) & link::JOINED == 0 && since.elapsed() > Duration::from_secs(20) {
                if l.flags.fetch_or(link::RELEASE, Ordering::AcqRel) & link::RELEASE == 0 {
                    note("the game never joined: the window goes");
                }
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        (seq, handle)
    });
    let ended = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { run(&mut kit, p, Mode::Launch, l) })).unwrap_or(Ended::Failed);
    playing.store(false, Ordering::Release);
    let (mut seq, mut handle) = helper.join().unwrap_or((0, None));
    let mut farewell = None;
    match ended {
        Ended::Closed { t, fx, open } => {
            // closed from its taskbar button before the game showed: the game goes at once, the hole closes
            note("closed from the taskbar");
            if !local {
                end_game(l);
            }
            crate::launch::sound::music_out(1.0);
            let now = link::qpc();
            farewell = Some(Mode::Farewell { card: t, fx, left: -1.0, foot_top: -1.0, ask: now, exit: now, view: [0.0; 7], open, kind: 0 });
            l.state.store(link::LEAVING, Ordering::Release);
        }
        Ended::Failed => {}
        _ => {
            l.state.store(link::IDLE, Ordering::Release);
            if HOST_LINK.get().is_some() {
                // the game's own process (CursedRecode.exe): back to the others' priority now the game plays
                unsafe {
                    use windows_sys::Win32::System::Threading::*;
                    SetPriorityClass(GetCurrentProcess(), NORMAL_PRIORITY_CLASS);
                }
            }
            // nothing is drawn until the farewell: the card's big pictures go
            unsafe {
                let _ = kit.gpu.device.device_wait_idle();
            }
            if let Some(lr) = kit.launch.as_mut() {
                lr.trim(&kit.gpu);
            }
            // the game's session: the music, then the farewell or the game's end
            let since = Instant::now();
            loop {
                if !local {
                    serve_music(l, mute, &mut seq);
                }
                if l.flags.load(Ordering::Acquire) & link::FAREWELL != 0 {
                    l.state.store(link::LEAVING, Ordering::Release);
                    note(&format!("the farewell asked for ({:.0} ms ago)", link::secs(l.ask_qpc.load(Ordering::Acquire), link::qpc()) * 1e3));
                    let f: [f32; 4] = std::array::from_fn(|i| link::f32_of(&l.farewell[i]));
                    farewell = Some(Mode::Farewell {
                        card: f[0],
                        fx: f[1],
                        left: f[2],
                        foot_top: f[3],
                        ask: l.ask_qpc.load(Ordering::Acquire),
                        exit: l.exit_qpc.load(Ordering::Acquire),
                        view: std::array::from_fn(|i| link::f32_of(&l.view[i])),
                        open: -1.0,
                        kind: l.exit_kind.load(Ordering::Acquire),
                    });
                    break;
                }
                if game_gone(l, &mut handle) || (l.game_pid.load(Ordering::Acquire) == 0 && since.elapsed() > Duration::from_secs(60)) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        }
    }
    if let Some(Mode::Farewell { card, fx, mut left, mut foot_top, ask, exit, view, open, kind }) = farewell {
        let fp = if l.flags.load(Ordering::Acquire) & link::FAREWELL != 0 {
            let v: [i32; 8] = std::array::from_fn(|i| l.farewell_place[i].load(Ordering::Acquire));
            Place2 { client: [v[0], v[1], v[2], v[3]], frame: [v[4], v[5], v[6], v[7]] }
        } else {
            p
        };
        if left < 0.0 {
            let (_, a, b) = crate::launch::timeline::menu_anchor(fp.client[2] as f32, fp.client[3] as f32);
            left = a;
            foot_top = b;
        }
        let mode = Mode::Farewell { card, fx, left, foot_top, ask, exit, view, open, kind };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { run(&mut kit, fp, mode, l) }));
    }
    let closed = l.state.load(Ordering::Acquire) == link::LEAVING && l.flags.load(Ordering::Acquire) & link::FAREWELL == 0;
    l.state.store(link::DONE, Ordering::Release);
    kit.destroy();
    if closed && (local || HOST_LINK.get().is_some()) {
        // the game's own process, closed from the window's taskbar button before it showed: nothing to keep
        std::process::exit(0);
    }
}
