//! The game window's own title bar ("the chrome"). Windows' bar is gone from the game's window: its client area takes
//! the whole window (WM_NCCALCSIZE), the top `bar_height` px of it are our bar (paint.rs draws it on every frame, over
//! the swap chain's top rows, vk/surface.rs) and the game's picture is under it. The bar answers Windows as its own
//! title bar would (WM_NCHITTEST): dragging, double-click, snapping, the system menu, the top edge resizing, and the
//! caption buttons — HTMAXBUTTON over ours, so Windows 11 shows its snap layouts there. The game is told of its
//! picture only (Java: the window's size and the mouse are offset by the bar, cursed.recode.chrome).
//!
//! Fullscreen (a window without WS_CAPTION) has no bar. The window before the game (twin) draws the same bar, still,
//! on the same place, so the game's window takes over on the same picture.

pub mod paint;
pub mod text;

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, Ordering};
use std::sync::Mutex;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, GetMonitorInfoW, MonitorFromPoint, ScreenToClient, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// The bar's height and a caption button's width at 96 DPI (Windows 11's own button).
pub const BAR_96: f32 = 32.0;
pub const BUTTON_96: f32 = 46.0;
/// The band along the top edge that resizes the window (it has no frame there), at 96 DPI.
const TOP_EDGE_96: f32 = 4.0;
/// How long the bar takes to come down once the window before the game has let go (s; her word, 19.10: half a second).
const DOWN_IN: f32 = 0.5;

/// The one game window with the bar.
struct State {
    hwnd: AtomicIsize,
    old: AtomicIsize,
    /// What the cursor is over in the bar: HTMINBUTTON, HTMAXBUTTON, HTCLOSE, or 0.
    hover: AtomicI32,
    /// The button held down there (the same codes).
    press: AtomicI32,
    tracking: AtomicBool,
    active: AtomicBool,
    /// Bumped on every change worth drawing again (Look).
    changed: AtomicU32,
}

static S: State = State {
    hwnd: AtomicIsize::new(0),
    old: AtomicIsize::new(0),
    hover: AtomicI32::new(0),
    press: AtomicI32::new(0),
    tracking: AtomicBool::new(false),
    active: AtomicBool::new(true),
    changed: AtomicU32::new(0),
};

static DEV_HOVER: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Where the game is (the bar shows it after its name): a world's or a server's name, or nothing.
static STATUS: Mutex<String> = Mutex::new(String::new());

/// Whether the game's window gets the bar (CURSED_DEV_NOCHROME: Windows' own, as before; the game asks the same).
pub fn wanted() -> bool {
    std::env::var_os("CURSED_DEV_NOCHROME").is_none()
}

/// The bar's height at `dpi`.
pub fn bar_height(dpi: u32) -> i32 {
    (BAR_96 * dpi.max(96) as f32 / 96.0).round() as i32
}

/// A caption button's width at `dpi`.
pub fn button_width(dpi: u32) -> i32 {
    (BUTTON_96 * dpi.max(96) as f32 / 96.0).round() as i32
}

/// The window frame's thickness at `dpi` (left/right, bottom): the part Windows keeps for resizing, all but one
/// pixel of it invisible on Windows 10 and 11.
pub fn frame(dpi: u32) -> (i32, i32) {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPEDWINDOW, 0, 0, dpi.max(96)) };
    (-r.left, r.bottom)
}

/// Whether a window with this style has the bar: a framed window. Not a fullscreen one, nor SDL's borderless one
/// (WS_POPUP: SDL keeps WS_CAPTION on it for snapping and the taskbar, and takes its frame off itself).
fn barred(hwnd: HWND) -> bool {
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    style & WS_CAPTION == WS_CAPTION && style & WS_POPUP == 0
}

fn dpi_of(hwnd: HWND) -> u32 {
    unsafe { GetDpiForWindow(hwnd) }.max(96)
}

/// The bar's height on the game's window now (0: none, fullscreen or not ours).
pub fn inset() -> i32 {
    let hwnd = S.hwnd.load(Ordering::Acquire) as HWND;
    if hwnd.is_null() || !barred(hwnd) {
        return 0;
    }
    bar_height(dpi_of(hwnd))
}

/// The bar's height on `hwnd` (0 when it is not the game's barred window).
pub fn inset_of(hwnd: isize) -> i32 {
    if hwnd == 0 || hwnd != S.hwnd.load(Ordering::Acquire) {
        return 0;
    }
    inset()
}

/// Whether the game's window is the one in use (its bar lit).
pub fn active() -> bool {
    S.active.load(Ordering::Acquire)
}

/// Whether the game's window is maximised.
pub fn maximized() -> bool {
    let hwnd = S.hwnd.load(Ordering::Acquire) as HWND;
    !hwnd.is_null() && unsafe { IsZoomed(hwnd) } != 0
}

pub fn set_status(text: &str) {
    let mut s = STATUS.lock().unwrap();
    if *s != text {
        *s = text.to_string();
        S.changed.fetch_add(1, Ordering::AcqRel);
    }
}

pub fn status() -> String {
    STATUS.lock().unwrap().clone()
}

/// The game's window gets the bar: Windows' own goes, its picture is put at `picture` (x y w h, physical px; its
/// client area as SDL placed it when None), the bar over it.
pub unsafe fn install(hwnd: HWND, picture: Option<[i32; 4]>) {
    if hwnd.is_null() || S.hwnd.load(Ordering::Acquire) == hwnd as isize {
        return;
    }
    let pic = picture.unwrap_or_else(|| {
        let mut c = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetClientRect(hwnd, &mut c);
        let mut at = POINT { x: 0, y: 0 };
        ClientToScreen(hwnd, &mut at);
        [at.x, at.y, c.right - c.left, c.bottom - c.top]
    });
    let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, proc as *const () as isize);
    S.old.store(old, Ordering::Release);
    S.hwnd.store(hwnd as isize, Ordering::Release);
    place(hwnd, pic);
}

/// The window put so that its picture (under the bar) is at `pic` (x y w h, physical px).
pub unsafe fn place(hwnd: HWND, pic: [i32; 4]) {
    if !barred(hwnd) {
        SetWindowPos(hwnd, std::ptr::null_mut(), pic[0], pic[1], pic[2], pic[3], SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        return;
    }
    let dpi = dpi_of(hwnd);
    let (bx, by) = frame(dpi);
    let bar = bar_height(dpi);
    let mut y = pic[1] - bar;
    // (on a monitor's top edge the bar would go off it: the window comes down)
    let mon = MonitorFromPoint(POINT { x: pic[0] + pic[2] / 2, y: pic[1] }, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(mon, &mut info) != 0 && y < info.rcWork.top {
        y = info.rcWork.top;
    }
    SetWindowPos(hwnd, std::ptr::null_mut(), pic[0] - bx, y, pic[2] + 2 * bx, pic[3] + bar + by, SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
}

fn is_button(code: i32) -> bool {
    code == HTMINBUTTON as i32 || code == HTMAXBUTTON as i32 || code == HTCLOSE as i32
}

/// What a point of the window is to Windows (screen px): the top edge, the caption buttons, the bar, the picture.
unsafe fn hit(hwnd: HWND, l: LPARAM) -> LRESULT {
    let mut pt = POINT { x: (l & 0xFFFF) as i16 as i32, y: ((l >> 16) & 0xFFFF) as i16 as i32 };
    ScreenToClient(hwnd, &mut pt);
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    GetClientRect(hwnd, &mut rc);
    let dpi = dpi_of(hwnd);
    let s = dpi as f32 / 96.0;
    if pt.y >= bar_height(dpi) || pt.y < 0 {
        return HTCLIENT as LRESULT;
    }
    if IsZoomed(hwnd) == 0 && (pt.y as f32) < TOP_EDGE_96 * s {
        let corner = (16.0 * s) as i32;
        return if pt.x < corner { HTTOPLEFT } else if pt.x >= rc.right - corner { HTTOPRIGHT } else { HTTOP } as LRESULT;
    }
    let bw = button_width(dpi);
    let from_right = rc.right - pt.x;
    (if from_right <= bw { HTCLOSE } else if from_right <= 2 * bw { HTMAXBUTTON } else if from_right <= 3 * bw { HTMINBUTTON } else { HTCAPTION }) as LRESULT
}

/// A caption button let go over itself: what Windows' own would do.
unsafe fn act(hwnd: HWND, code: i32) {
    let cmd = if code == HTMINBUTTON as i32 {
        SC_MINIMIZE
    } else if code == HTMAXBUTTON as i32 {
        if IsZoomed(hwnd) != 0 { SC_RESTORE } else { SC_MAXIMIZE }
    } else {
        SC_CLOSE
    };
    PostMessageW(hwnd, WM_SYSCOMMAND, cmd as WPARAM, 0);
}

fn set_hover(code: i32) {
    if S.hover.swap(code, Ordering::AcqRel) != code {
        S.changed.fetch_add(1, Ordering::AcqRel);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let old: WNDPROC = std::mem::transmute(S.old.load(Ordering::Acquire));
    let fwd = || CallWindowProcW(old, hwnd, msg, w, l);
    if msg == WM_NCDESTROY {
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, S.old.load(Ordering::Acquire));
        S.hwnd.store(0, Ordering::Release);
        return fwd();
    }
    match msg {
        WM_ACTIVATE => {
            S.active.store((w & 0xFFFF) as u32 != WA_INACTIVE, Ordering::Release);
            S.changed.fetch_add(1, Ordering::AcqRel);
            return fwd();
        }
        WM_NCACTIVATE => {
            S.active.store(w != 0, Ordering::Release);
            S.changed.fetch_add(1, Ordering::AcqRel);
        }
        _ => {}
    }
    if !barred(hwnd) {
        return fwd();
    }
    match msg {
        // the client area is the whole window but its sides and foot (they stay Windows' resizing edges); maximised,
        // the window reaches past the monitor by its frame: the client starts on the monitor
        WM_NCCALCSIZE if w != 0 => {
            let p = &mut *(l as *mut NCCALCSIZE_PARAMS);
            let top = p.rgrc[0].top;
            let r = fwd();
            p.rgrc[0].top = top + if IsZoomed(hwnd) != 0 { frame(dpi_of(hwnd)).1 } else { 0 };
            r
        }
        WM_NCHITTEST => {
            let r = fwd();
            if r != HTCLIENT as LRESULT {
                return r;
            }
            hit(hwnd, l)
        }
        WM_NCMOUSEMOVE => {
            if !S.tracking.swap(true, Ordering::AcqRel) {
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE | TME_NONCLIENT, hwndTrack: hwnd, dwHoverTime: 0 };
                TrackMouseEvent(&mut t);
            }
            let code = w as i32;
            set_hover(if is_button(code) { code } else { 0 });
            if is_button(code) {
                return 0;
            }
            fwd()
        }
        WM_NCMOUSELEAVE => {
            S.tracking.store(false, Ordering::Release);
            set_hover(0);
            S.press.store(0, Ordering::Release);
            fwd()
        }
        WM_MOUSEMOVE => {
            set_hover(0);
            fwd()
        }
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if is_button(w as i32) => {
            S.press.store(w as i32, Ordering::Release);
            S.changed.fetch_add(1, Ordering::AcqRel);
            0
        }
        WM_NCLBUTTONUP if is_button(w as i32) => {
            let held = S.press.swap(0, Ordering::AcqRel);
            S.changed.fetch_add(1, Ordering::AcqRel);
            if held == w as i32 {
                act(hwnd, held);
            }
            0
        }
        WM_NCRBUTTONDOWN | WM_NCRBUTTONUP | WM_NCMBUTTONDOWN | WM_NCMBUTTONUP if is_button(w as i32) => 0,
        _ => fwd(),
    }
}

// ---- what the bar shows (paint.rs), eased frame to frame -----------------------------------------------------------

/// One picture of the bar: its size, the window's state, how lit each button is.
#[derive(Clone, Default)]
pub struct Look {
    pub width: i32,
    pub height: i32,
    pub scale: f32,
    pub time: f32,
    pub active: f32,
    pub maximized: bool,
    pub hover: [f32; 3],
    pub press: [f32; 3],
    pub lines: [f32; 3],
    pub status: String,
    /// How far the bar has come down into its place (0..1, its buttons with it): the game's window gets it as the
    /// window before the game lets go of it; that window has none, only its own minimise.
    pub down: f32,
    /// The window before the game's lone minimise: how far in (0..1), how lit.
    pub pill: f32,
    pub pill_hover: f32,
}

/// The bar of the game's window, eased: the cursor's light comes and goes in a tenth of a second, the line under a
/// button is drawn in and goes on out to the right after it.
pub struct Ease {
    look: Look,
    last: Option<std::time::Instant>,
    start: std::time::Instant,
    seen: u32,
}

impl Default for Ease {
    fn default() -> Self {
        Ease { look: Look { active: 1.0, status: status(), ..Look::default() }, last: None, start: std::time::Instant::now(), seen: S.changed.load(Ordering::Acquire) }
    }
}

impl Ease {
    /// The bar for a picture `width` px wide now.
    pub fn next(&mut self, width: i32) -> &Look {
        let now = std::time::Instant::now();
        let dt = self.last.map_or(0.0, |l| (now - l).as_secs_f32().min(0.1));
        self.last = Some(now);
        let hwnd = S.hwnd.load(Ordering::Acquire) as HWND;
        let dpi = if hwnd.is_null() { 96 } else { dpi_of(hwnd) };
        let toward = |v: f32, to: f32, rate: f32| if to > v { (v + rate * dt).min(to) } else { (v - rate * dt).max(to) };
        let (mut hover, mut press) = (S.hover.load(Ordering::Acquire), S.press.load(Ordering::Acquire));
        // development: CURSED_DEV_CHROME=min|max|close[,pressed] — the cursor over a button (the stand has none)
        if let Some(v) = DEV_HOVER.get_or_init(|| std::env::var("CURSED_DEV_CHROME").ok()) {
            hover = if v.starts_with("min") { HTMINBUTTON as i32 } else if v.starts_with("max") { HTMAXBUTTON as i32 } else { HTCLOSE as i32 };
            if v.ends_with("pressed") {
                press = hover;
            }
        }
        let codes = [HTMINBUTTON as i32, HTMAXBUTTON as i32, HTCLOSE as i32];
        let l = &mut self.look;
        for (i, &c) in codes.iter().enumerate() {
            l.hover[i] = toward(l.hover[i], if hover == c { 1.0 } else { 0.0 }, if hover == c { 9.0 } else { 6.0 });
            l.press[i] = toward(l.press[i], if press == c && hover == c { 1.0 } else { 0.0 }, 14.0);
            let lp = &mut l.lines[i];
            if hover == c {
                // (going out to the right still: drawn in again from the left)
                if *lp > 1.0 {
                    *lp = 0.0;
                }
                *lp = (*lp + dt / 0.2).min(1.0);
            } else if *lp > 0.0 {
                *lp += dt / 0.28;
                if *lp >= 2.0 {
                    *lp = 0.0;
                }
            }
        }
        l.active = toward(l.active, if S.active.load(Ordering::Acquire) { 1.0 } else { 0.0 }, 5.0);
        // up while the window before the game is over this one, then down as it goes (over DOWN_IN); there without it
        l.down = match crate::twin::released_ago() {
            None => 1.0,
            Some(None) => 0.0,
            Some(Some(ago)) => (ago / DOWN_IN).clamp(0.0, 1.0),
        };
        l.maximized = !hwnd.is_null() && unsafe { IsZoomed(hwnd) } != 0;
        l.width = width;
        l.height = bar_height(dpi);
        l.scale = dpi as f32 / 96.0;
        l.time = (now - self.start).as_secs_f32();
        let changed = S.changed.load(Ordering::Acquire);
        if changed != self.seen {
            self.seen = changed;
            l.status = status();
        }
        l
    }
}
