//! What stops the game at its start (license.rs), said as the loader says what stops it (her word, 16.10: «в окне до
//! игры»): the loader's page (menus/loader.rs) over the valley, in a window of the loader's kind at the game window's
//! place. With the window before the game there, this one takes the card over from it as the game's own window would
//! (the same picture from the same clock), the story goes on into the valley quicker than a launch's — nothing loads —
//! and the message rises where the menu would: what it is about, the headline, the words, what may be pressed (open
//! the loader, the account on the site, quit). Quit: the window before the game plays the way out over this one, as
//! from the main menu. Without that window (a fullscreen game, Java started by hand) this one comes up as the loader's
//! does, out of black.

use super::app::{client_size, make_window_as, open_url, rand01, Gfx, In, BAR, INPUT, MENU_T};
use crate::launch::timeline::{self as tm, smooth, tl, Phase};
use crate::launch::{Inputs, LINES};
use crate::license::Stop;
use crate::twin::Place2;
use serde_json::json;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "CursedRecodeStop";
/// The story's pace once this window has the card: nothing loads, it goes on into the valley quicker than a launch.
const PACE: f32 = 2.2;
/// The page rises this long after the picture began to fill in (R0).
const PAGE_AT: f32 = 0.9;
/// The way out asked: the page sinks this long before the window before the game takes the picture.
const SINK: f32 = 0.35;

fn log(text: &str) {
    eprintln!("[license] {text}");
}

/// The window before the game this process has or joins (its place): the game's own joined it already, or joins it
/// now (Twin.java: the library is loaded before), or starts it in the game's process; or none (a fullscreen game).
fn twin() -> Option<Place2> {
    use crate::twin::link;
    // the link a moment (the game's process starts its window right after loading this library); its window, while
    // it makes its device
    let named = std::env::var("CURSED_TWIN").ok().filter(|n| !n.is_empty());
    // (a program of its own, started a moment ago by the game's: its link comes once it is up)
    let mut until = Instant::now() + Duration::from_millis(if named.is_some() { 5000 } else { 1500 });
    let mut linked = false;
    loop {
        if let Some(l) = crate::twin::link() {
            if !linked {
                linked = true;
                until = Instant::now() + Duration::from_secs(8);
            }
            let state = l.state.load(Ordering::Acquire);
            if state == link::PLAYING && l.placed.load(Ordering::Acquire) == 1 && l.flags.load(Ordering::Acquire) & link::RELEASE == 0 {
                let v: [i32; 8] = std::array::from_fn(|i| l.place[i].load(Ordering::Acquire));
                return Some(Place2 { client: [v[0], v[1], v[2], v[3]], frame: [v[4], v[5], v[6], v[7]] });
            }
            if state != 0 {
                // done already, or past its card
                return None;
            }
        } else if let Some(name) = named.as_deref() {
            if crate::twin::attach(name).is_some() {
                continue;
            }
        }
        if Instant::now() > until {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The loader the player has: in the game's folder (the program runs from its bin\), or in the folder the loader
/// keeps the game in. Started by it (the window before the game is the loader's), it is there already: none.
fn loader() -> Option<std::path::PathBuf> {
    let mut places = Vec::new();
    if let Some(home) = std::env::current_exe().ok().and_then(|e| e.parent().and_then(|b| b.parent()).map(|h| h.to_path_buf())) {
        places.push(home.join(super::install::EXE));
    }
    places.push(super::store::home().join(super::install::EXE));
    places.into_iter().find(|p| p.is_file())
}

fn open_loader() {
    if std::env::var("CURSED_TWIN").is_ok_and(|n| n.starts_with("CursedRecodeLoader-")) {
        // its window comes back by itself once the game has gone
        return;
    }
    match loader() {
        // tests (a build from the sources): nothing started on her screen
        Some(p) if !crate::license::RELEASE && std::env::var_os("CURSED_LOADER_NOBROWSER").is_some() => log(&format!("would start the loader {}", p.display())),
        Some(p) => {
            log(&format!("the loader: {}", p.display()));
            let mut c = std::process::Command::new(&p);
            if let Some(dir) = p.parent() {
                c.current_dir(dir);
            }
            let _ = c.spawn();
        }
        // none here: the site's page has it (with a subscription)
        None => open_url(crate::license::cabinet()),
    }
}

/// Over the other windows, as the game's own comes (twin/titlebar.rs bring_up: a window a process in the background
/// shows is put under the one the player is in) — under the window before the game, the only one over it then. The
/// stands (CURSED_DEV_TWIN_AT, a build from the sources) leave her focus where it is.
unsafe fn raise(hwnd: HWND) {
    if !crate::license::RELEASE && std::env::var_os("CURSED_DEV_TWIN_AT").is_some() {
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags);
        SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
    } else {
        crate::twin::titlebar::bring_up(hwnd);
    }
}

/// The game's folder: where the game runs (its options, the valley's look), or above the program's own bin\.
fn game_dir() -> std::path::PathBuf {
    let here = std::env::current_dir().unwrap_or_default();
    if here.join("options.txt").is_file() {
        return here;
    }
    std::env::current_exe().ok().and_then(|e| e.parent().and_then(|b| b.parent()).map(|h| h.to_path_buf())).filter(|h| h.join("options.txt").is_file()).unwrap_or(here)
}

/// The window's life: see the module. False when it could not be made (no Vulkan device): the caller says it another
/// way.
pub fn show(stop: &Stop) -> bool {
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    crate::twin::app_id();
    // the valley as her options make it (the window before the game draws it so)
    if let Some(o) = crate::twin::valley_options(&game_dir()) {
        crate::launch::set_options(o);
    }
    let card = twin();
    let place = card.unwrap_or_else(|| {
        let at = std::env::var("CURSED_DEV_WINDOW_AT").ok().and_then(|v| {
            let mut p = v.split(',');
            Some([p.next()?.trim().parse().ok()?, p.next()?.trim().parse().ok()?])
        });
        crate::twin::place(0, 0, at)
    });
    log(&format!("the window: {} the window before the game", if card.is_some() { "taking over from" } else { "without" }));
    let hwnd = unsafe { make_window_as(&place, CLASS) };
    if hwnd.is_null() {
        return false;
    }
    // taking the card over, it is simply there under it: Windows' own way in would show the desktop through both
    unsafe { crate::twin::titlebar::style_game_window(hwnd, card.is_none()) };
    let Some(gfx) = Gfx::new() else {
        unsafe { DestroyWindow(hwnd) };
        return false;
    };
    let mut w = Window {
        hwnd,
        gfx: Some(gfx),
        ease: crate::chrome::Ease::default(),
        stop: stop.clone(),
        card: card.is_some(),
        story: if card.is_some() { -1.0 } else { MENU_T },
        pace: 1.0,
        epoch: Instant::now(),
        shown_at: None,
        page: 0,
        quit_at: None,
        farewell_at: None,
        mouse: [-1.0, -1.0],
        cur: [0.5, 0.5, 0.0],
        look: [0.0; 2],
        flash_at: -100.0,
        next_flash: 0.0,
        clicks: 0.0,
        left: [-1.0; 2],
    };
    w.run();
    if w.page != 0 {
        crate::menus::close(w.page);
    }
    if let Some(g) = w.gfx.take() {
        g.destroy();
    }
    unsafe { DestroyWindow(hwnd) };
    true
}

struct Window {
    hwnd: HWND,
    gfx: Option<Gfx>,
    ease: crate::chrome::Ease,
    stop: Stop,
    /// It took the card over from the window before the game (or will).
    card: bool,
    /// The story's second (Run): the card's, then on into the valley; -1 till it has the card.
    story: f32,
    pace: f32,
    epoch: Instant,
    shown_at: Option<f32>,
    page: u32,
    /// The way out asked (this window's seconds); the window before the game asked for its farewell.
    quit_at: Option<f32>,
    farewell_at: Option<f32>,
    mouse: [f32; 2],
    cur: [f32; 3],
    look: [f32; 2],
    flash_at: f64,
    next_flash: f64,
    clicks: f32,
    /// Where the page's column and its foot are (the valley's title goes there): as the page last said.
    left: [f32; 2],
}

impl Window {
    fn now(&self) -> f32 {
        self.epoch.elapsed().as_secs_f32()
    }

    /// The valley's clock that never waits: the window before the game's (the same picture), else this window's.
    fn fx(&self) -> f32 {
        crate::twin::link().filter(|_| self.card).map_or(self.now(), |l| l.life())
    }

    fn run(&mut self) {
        let mut last = Instant::now();
        loop {
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            self.input();
            let now = self.now();
            let dt = last.elapsed().as_secs_f32().min(0.1);
            last = Instant::now();
            // the card: taken over once its window is whole (its picture is this one's from there) and this one can draw
            if self.card && self.story < 0.0 {
                let ready = self.gfx.as_mut().is_some_and(|g| g.ready());
                let t = crate::twin::times();
                let playing = t[3] as i32 & 1 != 0;
                if !playing {
                    // it went (closed from its taskbar button, or gave up): this window comes up by itself
                    self.card = false;
                    self.story = MENU_T;
                } else if ready && t[1] >= tm::lt::COOL_TO + 0.05 {
                    self.story = t[1];
                    log(&format!("the card taken over at its {:.2} s", self.story));
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
            } else if self.story >= 0.0 && self.story < MENU_T {
                // nothing loads: quicker into the valley, the launch's own pace again as the picture fills in
                let goal = if self.story < tl::HOLD_AT { PACE } else { 1.0 };
                self.pace += (goal - self.pace) * (1.0 - (-dt / 0.3).exp());
                self.story += dt * self.pace;
            }
            // the page rises where the menu would
            let page_at = if self.card { tl::R0 + PAGE_AT } else { MENU_T };
            if self.page == 0 && self.quit_at.is_none() && self.story >= page_at {
                self.page = 9_001;
                let words: Vec<serde_json::Value> = self.stop.words.iter().map(|w| json!([if w.0 == "quit" { "cancel" } else { w.0.as_str() }, w.1, w.2])).collect();
                let data = json!({ "cap": self.stop.cap, "title": self.stop.title, "title_face": "thin", "text": self.stop.text, "words": words, "nick": "", "media": null, "build": "Cursed Recode", "news": [], "sub": "" });
                crate::menus::open(self.page, "loader", &data.to_string());
            }
            if let Some(q) = self.quit_at {
                if self.page != 0 {
                    crate::menus::close(self.page);
                    self.page = 0;
                }
                if !self.card {
                    // out of the picture as it came, into black
                    if now - q > SINK + 0.45 {
                        return;
                    }
                } else if self.farewell_at.is_none() && now - q > SINK {
                    self.farewell();
                } else if let Some(f) = self.farewell_at {
                    // its first picture up over this window: this one goes; the way out plays on there
                    if crate::twin::shown() || now - f > 1.5 {
                        unsafe { ShowWindow(self.hwnd, SW_HIDE) };
                        crate::twin::wait_farewell(Duration::from_secs(8));
                        let until = Instant::now() + Duration::from_secs(8);
                        while crate::twin::busy() && Instant::now() < until {
                            std::thread::sleep(Duration::from_millis(15));
                        }
                        return;
                    }
                }
            }
            if unsafe { IsIconic(self.hwnd) } != 0 {
                std::thread::sleep(Duration::from_millis(30));
                continue;
            }
            if !self.draw(dt) {
                return;
            }
        }
    }

    fn input(&mut self) {
        let events: Vec<In> = std::mem::take(&mut *INPUT.lock().unwrap());
        for e in events {
            match e {
                In::Move(x, y) => {
                    self.mouse = [x, y];
                    crate::menus::mouse(x, y, 0, 0);
                }
                In::Button(b, a, x, y) => {
                    self.mouse = [x, y];
                    crate::menus::mouse(x, y, b, a);
                }
                In::Wheel(d) => crate::menus::scroll(d),
                In::Key(k, m) => crate::menus::key(&k, m),
                In::Char(c) => crate::menus::typed(c),
                In::Close => self.quit(),
            }
        }
        for line in crate::menus::poll().lines().map(str::to_string).collect::<Vec<_>>() {
            match line.as_str() {
                "open-loader" => {
                    open_loader();
                    self.quit();
                }
                "site" => open_url(crate::license::cabinet()),
                "cancel" => self.quit(),
                _ => {}
            }
        }
    }

    fn quit(&mut self) {
        if self.quit_at.is_none() {
            self.quit_at = Some(self.now());
            // still on the card: nothing to say goodbye from (its window goes as from its taskbar button)
            if self.card && self.story < 0.0 {
                self.card = false;
                self.quit_at = Some(self.now() - SINK - 1.0);
            }
        }
    }

    /// The way out, played by the window before the game over this one (as the game's main menu asks for it).
    fn farewell(&mut self) {
        self.farewell_at = Some(self.now());
        let place = crate::twin::place_of(self.hwnd);
        let (cw, ch) = client_size(self.hwnd);
        let (_, l0, f0) = tm::menu_anchor(cw as f32, ch.saturating_sub(BAR.load(Ordering::Relaxed) as u32) as f32);
        let (left, foot) = if self.left[0] > 0.0 { (self.left[0], self.left[1]) } else { (l0, f0) };
        let flash = self.flash(self.epoch.elapsed().as_secs_f64());
        let v = crate::twin::View {
            card: self.story,
            fx: self.fx(),
            left,
            foot_top: foot,
            exit_ago: 0.0,
            view: [self.look[0], self.look[1], self.cur[0], self.cur[1], self.cur[2], 0.0, flash],
            maximized: crate::chrome::maximized(),
            active: crate::chrome::active(),
            kind: 0,
        };
        if !crate::twin::farewell(place, v) {
            // it is gone: this window simply closes
            log("the farewell was not taken");
            self.card = false;
        }
    }

    /// Sheet lightning now and then, and a press lights the sky a moment (as the loader's).
    fn flash(&self, now: f64) -> f32 {
        let pulse = |y: f64| if y < 0.0 { 0.0 } else { (-y * 6.5).exp() * (1.0 - (-y * 45.0).exp()) };
        let x = now - self.flash_at;
        ((pulse(x) + 0.55 * pulse(x - 0.2) + 0.3 * pulse(x - 0.45)) * 0.9) as f32
    }

    /// One frame; false when the window cannot draw (its device lost).
    fn draw(&mut self, dt: f32) -> bool {
        let (cw, ch) = client_size(self.hwnd);
        let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(self.hwnd) }.max(96);
        let bar_h = crate::chrome::bar_height(dpi) as u32;
        BAR.store(bar_h as i32, Ordering::Relaxed);
        let hwnd = self.hwnd;
        let Some(g) = self.gfx.as_mut() else { return false };
        if !g.size(hwnd, cw, ch, bar_h) {
            return false;
        }
        if !g.ready() {
            std::thread::sleep(Duration::from_millis(5));
            return true;
        }
        let (w, h) = (cw as f32, ch.saturating_sub(bar_h) as f32);
        let now = self.now();
        let mut inp = [0f32; 32];
        inp[15] = 0.5;
        inp[16..24].copy_from_slice(&crate::twin::valley_options(&game_dir()).unwrap_or([12.0, 2.0, 128.0, 0.0, 0.5, 70.0, 1.0, 5.0]));
        let (id, said) = crate::menus::frame(w, h, now, &inp, false);
        let built = if id != 0 { crate::gui::take(id) } else { None };
        if said[13] > 0.0 {
            self.left = [said[13], said[14]];
        }
        let i = self.inputs(&said, w, h, dt);
        crate::launch::drew(&i.pose, &i.knobs);
        let look = self.ease.next(cw as i32).clone();
        // without the card: up out of black as the loader's window comes, and back into it on the way out
        let fade = if self.card {
            1.0
        } else {
            let up = self.shown_at.map_or(0.0, |s| ((now - s) / 0.45).clamp(0.0, 1.0));
            let down = self.quit_at.map_or(1.0, |q| 1.0 - ((now - q - SINK) / 0.45).clamp(0.0, 1.0));
            up.min(down)
        };
        let Some(g) = self.gfx.as_mut() else { return false };
        g.frame(&i, built, &look, fade);
        match self.shown_at {
            None => {
                // (with the card: under the window before the game, the same picture; it goes once this one has drawn)
                self.shown_at = Some(now);
                unsafe {
                    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    raise(hwnd);
                }
            }
            // (two frames shown: the first is on the screen by now)
            Some(s) if self.card && now - s > 0.05 && crate::twin::owns_card() => {
                crate::twin::release();
                log("the window before the game released");
            }
            _ => {}
        }
        true
    }

    /// The launch's numbers for this frame: the card's (Run, its story) as the window before the game made them, the
    /// menu's valley under the page from then on (as the loader's).
    fn inputs(&mut self, said: &[f32; 64], w: f32, h: f32, dt: f32) -> Inputs {
        let toward = |v: f32, to: f32, rate: f32| v + (to - v) * (dt * rate).min(1.0);
        let (cx, cy) = (self.mouse[0] / w.max(1.0), self.mouse[1] / h.max(1.0));
        self.look[0] = toward(self.look[0], (cx.clamp(0.0, 1.0) - 0.5) * 0.05, 1.8);
        self.look[1] = toward(self.look[1], -(cy.clamp(0.0, 1.0) - 0.5) * 0.028, 1.8);
        self.cur[0] = toward(self.cur[0], cx, 5.0);
        self.cur[1] = toward(self.cur[1], cy, 5.0);
        self.cur[2] = toward(self.cur[2], if self.mouse[0] >= 0.0 { 1.0 } else { 0.0 }, 2.0);
        let now = self.epoch.elapsed().as_secs_f64();
        if said[19] != self.clicks {
            self.clicks = said[19];
            self.flash_at = now - 0.05;
        }
        if self.next_flash == 0.0 {
            self.next_flash = now + 4.0 + rand01() * 5.0;
        }
        // (the sky's flashes only once the valley is alive)
        if now >= self.next_flash && self.story - tl::R0 > tl::SETTLE - tl::R0 {
            self.flash_at = now;
            self.next_flash = now + 7.0 + rand01() * 9.0;
        }
        let flash = self.flash(now);
        let mut hover = [[0f32; 5]; 2];
        for (k, hv) in hover.iter_mut().enumerate() {
            hv.copy_from_slice(&said[1 + k * 5..6 + k * 5]);
        }
        let main = crate::launch::valley();
        let (mut pose, mut knobs): ([f32; 5], [f32; 24]) = (main[..5].try_into().unwrap(), main[5..].try_into().unwrap());
        if said[24] > 1.0 {
            pose.copy_from_slice(&said[20..25]);
            knobs.copy_from_slice(&said[25..49]);
        }
        let (ui, l0, f0) = tm::menu_anchor(w, h);
        let (left, foot_top) = if self.left[0] > 0.0 { (self.left[0], self.left[1]) } else { (l0, f0) };
        let fx = self.fx();
        let t = self.story.max(0.0);
        // (the valley's clock is the card's own while the card plays: the window before the game's)
        let alive = smooth(0.0, 1.0, t - tl::R0);
        Inputs {
            t,
            fx,
            life: fx,
            dt,
            launched: true,
            gather: 0.0,
            smoke_reset: false,
            cursor: [self.cur[0], self.cur[1], self.cur[2] * alive],
            look: [self.look[0] * alive, self.look[1] * alive],
            boost: said[11],
            flash_c: flash,
            fade: 1.0,
            ui,
            left,
            foot_top,
            hover,
            hover_sparks: alive,
            lines: [[0.0; 8]; LINES],
            icons: [[0.0, 0.0, 0.0, -1.0]; 3],
            icon_light: [0.0; 4],
            disc: [0.0; 8],
            no_rays: false,
            no_bloom: false,
            menus_fire: self.page != 0,
            knobs,
            pose,
            phase: Phase::Run,
            pt: t,
            load: 1.0,
            loaded_at: -1.0,
            world: None,
            quit: -1.0,
        }
    }
}
