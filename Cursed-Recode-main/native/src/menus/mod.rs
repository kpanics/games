//! The game's menus as the client draws them: the main menu and every screen of the game that is not an inventory
//! (worlds, servers, settings, the pause menu, advancements, statistics, the player list, waiting and messages…),
//! in the main menu's language (the approved preview, Desktop\Cursed Recode Launch\menus). All of it lives here — the
//! state, the layout, the motion, the input; Java's screens (cursed.recode.menus) hand over the game's screen's data
//! as JSON, the mouse and the keys, and carry out what is asked (`actions`); the picture is the GUI's renderer's
//! (gui/render.rs, its third layer), over the menu's valley or the blurred world.

mod adv;
/// Alt Manager's card under the nickname (the main menu's, the pause's).
mod alts;
mod controls;
mod create;
mod credits;
mod death;
mod dev;
mod gamemode;
pub mod lang;
mod lang_more;
/// The loader's page (Cursed Recode.exe, loader/app.rs).
mod loader;
mod main_menu;
mod mirror;
mod multi;
mod notice;
mod packs;
mod pause;
mod settings;
mod single;
mod social;
mod stats;
mod tab;
mod toasts;
pub mod ui;

use crate::gui::draw::{Canvas, FLOATS};
use crate::gui::Look;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use ui::{Flame, WordFx};

/// A screen of the menus: which, the game's data for it, when it opened, when it began to go.
pub struct Screen {
    pub id: u32,
    pub kind: String,
    pub data: Value,
    pub opened: f32,
    pub leaving: Option<f32>,
    pub page: Page,
    /// Opened by a word flying up to be its title: its elements wait for the flight (and keep waiting the same once it
    /// landed, or they would all jump ahead at once).
    pub flown: bool,
}

pub enum Page {
    Main(main_menu::Main),
    Loader(loader::Loader),
    Mirror(mirror::Mirror),
    Settings(settings::Settings),
    Pause(pause::Pause),
    Single(single::Single),
    Create(create::Create),
    Multi(multi::Multi),
    Adv(adv::Adv),
    Stats(stats::Stats),
    Death(death::Death),
    Notice(notice::Notice),
    Social(social::Social),
    Tab(tab::Tab),
    GameMode(gamemode::GameMode),
    Credits(credits::Credits),
    Packs(packs::Packs),
    Toasts(toasts::Toasts),
    /// Not drawn by a page of its own yet: nothing (the game's screen is shown as it is).
    None,
}

/// A word flying from where it was pressed to become the next screen's title.
#[derive(Clone)]
pub struct Fly {
    pub text: String,
    pub face: crate::gui::text::Face,
    pub size: f32,
    pub at: [f32; 2],
    pub since: f32,
    pub ls: f32,
}

pub struct Menus {
    pub w: f32,
    pub h: f32,
    /// The menus' clock (Java's, stepping evenly with the screen), seconds; since the last frame.
    pub now: f32,
    pub dt: f32,
    pub mouse: [f32; 2],
    /// What the pointer pressed (0 nothing), when, where.
    pub held: u64,
    pub held_at: f32,
    pub held_pos: [f32; 2],
    /// This frame's boxes the pointer can press, and last frame's (input between frames goes by those).
    pub hits: Vec<([f32; 4], u64)>,
    pub prev_hits: Vec<([f32; 4], u64)>,
    /// The element the pointer is on (lit when it is a word).
    pub lit: u64,
    pub anims: HashMap<u64, f32>,
    pub words: HashMap<u64, WordFx>,
    pub word_ids: HashSet<u64>,
    pub word_ids_now: HashSet<u64>,
    pub flames: Vec<Flame>,
    /// Lines under words this frame (x0, y, x1, y): the fire follows them.
    pub rects: HashMap<u64, [f32; 4]>,
    pub screens: Vec<Screen>,
    pub actions: Vec<String>,
    /// What Java reads back each frame (see `out_*`).
    pub out: [f32; 64],
    /// What Java says each frame for the screen up (its clocks, the music…); the game's options the valley follows
    /// from 16 on (`apply_options`).
    pub inp: [f32; 32],
    pub fly: Option<Fly>,
    /// Going back: the title of the screen left flies down into the word that opened it (its text, place, size, since
    /// when); the titles drawn this frame and last (the one up's).
    pub back_fly: Option<Fly>,
    title_now: Option<Fly>,
    last_title: Option<Fly>,
    /// The world is behind (in a game): the menus are over it, frosted.
    pub in_world: bool,
    /// The valley's place and knobs the screens ask for this frame (POSE then KNOBS), what is shown (gliding there),
    /// where the camera set out from and when.
    pub scene_to: [f32; 29],
    pub scene: [f32; 29],
    pose_from: [f32; 5],
    pose_key: [f32; 5],
    pose_t0: f32,
    /// The valley has been set from the game's options once (then it glides).
    scene_seen: bool,
    /// How much the world behind is frosted (in a game).
    frost: f32,
    /// Development: the script's start and its next step (dev.rs).
    dev_t0: f32,
    dev_at: usize,
    /// This frame's elements that stand for something of the game's (a link's name, a language's code).
    pub link_ids: Vec<(u64, String)>,
    /// How blurred the valley is behind the screens other than the main menu (the game's option).
    opt_blur: f32,
    /// What the game draws over the page this frame where the page says: kind, number, x, y, size, alpha.
    pub extras: Vec<[f32; 6]>,
    /// The screen being drawn (its extras are marked with it: Java draws each page's own).
    drawing: u32,
    /// The language changed: the last frame in the old words fades as the new comes in (its shapes, since when).
    ghost: Option<(Vec<f32>, f32)>,
    last_prims: Vec<f32>,
    lang_seen: u32,
}

/// The main menu's valley: the camera's place, then the knobs (launch/mod.rs POSE, KNOBS).
pub const SCENE: [f32; 29] = [
    0.0, 22.0, -0.085, 0.0, 70.0, //
    1.0, 0.0, 1.0, 41.0, 0.0, 0.0, 6.0, 41.0, 1.0, 1e4, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0,
];

/// Indices into `out`.
pub mod out {
    /// 1 when a frame was made.
    pub const DRAWN: usize = 0;
    /// The fire under the lit word for the sparks (x0, y from the bottom, x1, run, strength), twice.
    pub const FLAME0: usize = 1;
    pub const FLAME1: usize = 6;
    pub const BOOST: usize = 11;
    pub const PULSE: usize = 12;
    /// The title's place in the main menu (the word's left edge, the build line's top).
    pub const LEFT: usize = 13;
    pub const FOOT_TOP: usize = 14;
    /// The account's head: x, y, size, alpha (the game draws it).
    pub const HEAD: usize = 15;
    /// Presses so far (each lights the sky a moment).
    pub const CLICKS: usize = 19;
    /// The camera's place for the screen (along the river, height, pitch, yaw, field of view), then the valley's knobs
    /// (launch.glsl Knob0..5): 29 numbers.
    pub const POSE: usize = 20;
    /// A text field has the keys (the window takes text).
    pub const TYPING: usize = 49;
}

static MENUS: Mutex<Option<Menus>> = Mutex::new(None);

pub fn with<R>(f: impl FnOnce(&mut Menus) -> R) -> R {
    let mut g = MENUS.lock().unwrap();
    f(g.get_or_insert_with(Menus::new))
}

impl Menus {
    fn new() -> Menus {
        Menus {
            w: 1.0,
            h: 1.0,
            now: 0.0,
            dt: 0.0,
            mouse: [-1.0, -1.0],
            held: 0,
            held_at: 0.0,
            held_pos: [0.0; 2],
            hits: Vec::new(),
            prev_hits: Vec::new(),
            lit: 0,
            anims: HashMap::new(),
            words: HashMap::new(),
            word_ids: HashSet::new(),
            word_ids_now: HashSet::new(),
            flames: Vec::new(),
            rects: HashMap::new(),
            screens: Vec::new(),
            actions: Vec::new(),
            out: [0.0; 64],
            inp: [0.0; 32],
            fly: None,
            back_fly: None,
            title_now: None,
            last_title: None,
            in_world: false,
            scene_to: SCENE,
            scene: SCENE,
            pose_from: [0.0, 22.0, -0.085, 0.0, 70.0],
            pose_key: [0.0, 22.0, -0.085, 0.0, 70.0],
            pose_t0: -10.0,
            scene_seen: false,
            frost: 0.0,
            dev_t0: -1.0,
            dev_at: 0,
            link_ids: Vec::new(),
            opt_blur: 0.0,
            extras: Vec::new(),
            drawing: 0,
            ghost: None,
            last_prims: Vec::new(),
            lang_seen: lang::version(),
        }
    }

    /// The screen up now (the last opened that is not going; not an overlay).
    pub fn current(&self) -> Option<&Screen> {
        self.screens.iter().rev().find(|s| s.leaving.is_none() && !overlay(&s.kind))
    }

    pub fn ask(&mut self, action: impl Into<String>) {
        let a = action.into();
        if dev::on() {
            eprintln!("[menus dev] asks «{a}»");
        }
        self.actions.push(a);
    }

    /// A screen of the game opened: `kind` says which page draws it, `data` the game's data for it.
    pub fn open(&mut self, id: u32, kind: &str, data: Value) {
        let now = self.now;
        // another of the game's screens for the same page (a settings' section opened by the game): the page stays
        if kind == "settings" || kind == "servers" {
            if let Some(i) = self.screens.iter().rposition(|s| s.leaving.is_none() && s.kind == kind) {
                let old = self.screens.remove(i);
                self.screens.push(Screen { id, kind: kind.to_string(), data, opened: old.opened, leaving: None, page: old.page, flown: old.flown });
                return;
            }
        }
        // a screen sends the one before it away; an overlay comes over whatever is up. Not opened by a word flying up to
        // be its title: back from the screen before, whose title flies down into its word (if the new one has it)
        let forward = !overlay(kind) && self.fly.as_ref().is_some_and(|f| (now - f.since).abs() < 0.3);
        if !overlay(kind) {
            self.back_fly = if forward { None } else { self.last_title.take().map(|mut f| {
                f.since = f32::NAN;
                f
            }) };
            for s in self.screens.iter_mut() {
                if s.leaving.is_none() && !overlay(&s.kind) {
                    s.leaving = Some(f32::NAN);
                }
            }
        }
        let page = match kind {
            "main" => Page::Main(main_menu::Main::new()),
            "loader" => Page::Loader(loader::Loader::new()),
            "settings" => Page::Settings(settings::Settings::new(&data)),
            "pause" => Page::Pause(pause::Pause::new()),
            "worlds" => Page::Single(single::Single::new(&data)),
            "create" => Page::Create(create::Create::new()),
            "servers" => Page::Multi(multi::Multi::new(&data)),
            "advancements" => Page::Adv(adv::Adv::new(&data)),
            "stats" => Page::Stats(stats::Stats::new()),
            "death" => Page::Death(death::Death::new()),
            "notice" => Page::Notice(notice::Notice::new()),
            "social" => Page::Social(social::Social::new()),
            "tab" => Page::Tab(tab::Tab::new()),
            "gamemode" => Page::GameMode(gamemode::GameMode::new()),
            "credits" => Page::Credits(credits::Credits::new()),
            "packs" => Page::Packs(packs::Packs::new()),
            "toasts" => Page::Toasts(toasts::Toasts::new()),
            _ => Page::Mirror(mirror::Mirror::new()),
        };
        // its clock starts with the next frame (no frames are made while the game plays with no screen: the clock
        // is still where the last screen left it, and the screen would be up at once)
        self.screens.push(Screen { id, kind: kind.to_string(), data, opened: f32::NAN, leaving: None, page, flown: forward });
    }

    /// New data for screen `id` (its page keeps its own state: scroll, what is chosen, what is typed).
    pub fn data(&mut self, id: u32, data: Value) {
        if let Some(s) = self.screens.iter_mut().find(|s| s.id == id) {
            s.data = data;
        }
    }

    /// Screen `id` is gone from the game with no other after it (the menus are closed): it sinks away.
    pub fn close(&mut self, id: u32) {
        if let Some(s) = self.screens.iter_mut().find(|s| s.id == id && s.leaving.is_none()) {
            s.leaving = Some(f32::NAN);
        }
    }

    /// One frame: the screens going away and the one up, into shapes.
    fn frame(&mut self, w: f32, h: f32, now: f32) -> Option<Canvas> {
        // the first frame after the game played with no menus: a frame's step, not the time it played; what was going
        // away then has gone (nobody saw it sink)
        let gap = now - self.now > 0.25;
        self.dt = if gap { 1.0 / 60.0 } else { (now - self.now).clamp(0.0, 0.1) };
        self.now = now;
        if gap {
            self.screens.retain(|s| s.leaving.is_none());
            self.ghost = None;
        }
        // what opened or began to go since the last frame starts now
        for s in self.screens.iter_mut() {
            if s.opened.is_nan() {
                s.opened = now;
            }
            if s.leaving.is_some_and(f32::is_nan) {
                s.leaving = Some(now);
            }
        }
        if let Some(f) = self.back_fly.as_mut() {
            if f.since.is_nan() {
                f.since = now;
            }
        }
        self.w = w.max(1.0);
        self.h = h.max(1.0);
        self.out = [0.0; 64];
        self.dev_run();
        // what the pointer is on, by last frame's boxes (the topmost: drawn last)
        let on = self.prev_hits.iter().rev().find(|(r, _)| self.over(*r)).map_or(0, |h| h.1);
        self.light_word(on);
        self.hits.clear();
        self.rects.clear();
        self.word_ids_now.clear();
        self.title_now = None;
        if self.back_fly.as_ref().is_some_and(|f| self.now - f.since > 1.0) {
            self.back_fly = None;
        }
        let mut c = Canvas::new(self.w, self.h);
        self.scene_to = SCENE;
        self.link_ids.clear();
        self.extras.clear();
        self.apply_options();
        // screens that have sunk away are let go (and the data of the ones going)
        let now = self.now;
        self.screens.retain(|s| s.leaving.is_none_or(|t| now - t < 1.5));
        // the screens, then the overlays over them
        let n = self.screens.len();
        let order: Vec<usize> = (0..n).filter(|&i| !overlay(&self.screens[i].kind)).chain((0..n).filter(|&i| overlay(&self.screens[i].kind))).collect();
        for i in order {
            let mut page = std::mem::replace(&mut self.screens[i].page, Page::None);
            let (opened, leaving, id) = (self.screens[i].opened, self.screens[i].leaving, self.screens[i].id);
            let data = std::mem::take(&mut self.screens[i].data);
            let view = View { opened, leaving, id, data: &data, flown: self.screens[i].flown };
            self.drawing = id;
            match &mut page {
                Page::Main(m) => m.draw(self, &mut c, &view),
                Page::Loader(m) => m.draw(self, &mut c, &view),
                Page::Mirror(m) => m.draw(self, &mut c, &view),
                Page::Settings(m) => m.draw(self, &mut c, &view),
                Page::Pause(m) => m.draw(self, &mut c, &view),
                Page::Single(m) => m.draw(self, &mut c, &view),
                Page::Create(m) => m.draw(self, &mut c, &view),
                Page::Multi(m) => m.draw(self, &mut c, &view),
                Page::Adv(m) => m.draw(self, &mut c, &view),
                Page::Stats(m) => m.draw(self, &mut c, &view),
                Page::Death(m) => m.draw(self, &mut c, &view),
                Page::Notice(m) => m.draw(self, &mut c, &view),
                Page::Social(m) => m.draw(self, &mut c, &view),
                Page::Tab(m) => m.draw(self, &mut c, &view),
                Page::GameMode(m) => m.draw(self, &mut c, &view),
                Page::Credits(m) => m.draw(self, &mut c, &view),
                Page::Packs(m) => m.draw(self, &mut c, &view),
                Page::Toasts(m) => m.draw(self, &mut c, &view),
                Page::None => {}
            }
            self.screens[i].page = page;
            self.screens[i].data = data;
        }
        self.tick_flames();
        self.tick_scene();
        // a new language: the words cross over (the old frame fades out as the new fades in)
        if lang::version() != self.lang_seen {
            self.lang_seen = lang::version();
            self.ghost = Some((std::mem::take(&mut self.last_prims), self.now));
        }
        self.last_prims = c.p.clone();
        if let Some((old, at)) = &self.ghost {
            // the old words go a little ahead of the new coming in (less of the two over each other)
            let t = (self.now - at) / 0.5;
            if t >= 1.0 {
                self.ghost = None;
            } else {
                c.fade_from(0, ui::smooth(0.3, 1.0, t));
                let fresh = std::mem::take(&mut c.p);
                c.p.extend_from_slice(old);
                c.fade_from(0, 1.0 - ui::smooth(0.0, 0.6, t));
                c.p.extend_from_slice(&fresh);
            }
        }
        // the world frosted behind a screen; not behind the player list (a plate of its own over the game)
        let frost_to = if self.in_world && self.current().is_some_and(|s| s.kind != "gamemode") { 1.0 } else { 0.0 };
        self.frost = ui::toward(self.frost, frost_to, 7.0, self.dt);
        self.word_ids = std::mem::take(&mut self.word_ids_now);
        self.last_title = self.title_now.take();
        self.prev_hits = std::mem::take(&mut self.hits);
        self.out[out::DRAWN] = 1.0;
        Some(c)
    }

    fn press(&mut self, x: f32, y: f32, button: i32) {
        self.mouse = [x, y];
        let id = self.prev_hits.iter().rev().find(|(r, _)| self.over(*r)).map_or(0, |h| h.1);
        self.held = id;
        self.held_at = self.now;
        self.held_pos = [x, y];
        if dev::on() {
            let near: Vec<String> = self.prev_hits.iter().filter(|(r, _)| (r[1] - y).abs() < 80.0).map(|(r, i)| format!("{i:#x}@{:.0},{:.0}-{:.0},{:.0}", r[0], r[1], r[2], r[3])).collect();
            eprintln!("[menus dev] press at {x:.0},{y:.0}: {id:#x} ({} boxes; near: {})", self.prev_hits.len(), near.join(" "));
        }
        self.page_event(Event::Press(id, button));
    }

    fn release(&mut self, x: f32, y: f32, button: i32) {
        self.mouse = [x, y];
        let id = self.held;
        self.held = 0;
        self.page_event(Event::Release(id, button));
    }

    fn page_event(&mut self, e: Event) {
        let Some(i) = self.screens.iter().rposition(|s| s.leaving.is_none() && !overlay(&s.kind)) else { return };
        let mut page = std::mem::replace(&mut self.screens[i].page, Page::None);
        let data = std::mem::take(&mut self.screens[i].data);
        let view = View { opened: self.screens[i].opened, leaving: None, id: self.screens[i].id, data: &data, flown: self.screens[i].flown };
        match &mut page {
            Page::Main(m) => m.event(self, &view, e),
            Page::Loader(m) => m.event(self, &view, e),
            Page::Mirror(m) => m.event(self, &view, e),
            Page::Settings(m) => m.event(self, &view, e),
            Page::Pause(m) => m.event(self, &view, e),
            Page::Single(m) => m.event(self, &view, e),
            Page::Create(m) => m.event(self, &view, e),
            Page::Multi(m) => m.event(self, &view, e),
            Page::Adv(m) => m.event(self, &view, e),
            Page::Stats(m) => m.event(self, &view, e),
            Page::Death(m) => m.event(self, &view, e),
            Page::Notice(m) => m.event(self, &view, e),
            Page::Social(m) => m.event(self, &view, e),
            // the player list hears nothing (it is over the game, not a screen)
            Page::Tab(_) => {}
            Page::GameMode(m) => m.event(self, &view, e),
            Page::Credits(m) => m.event(self, &view, e),
            Page::Packs(m) => m.event(self, &view, e),
            Page::Toasts(_) => {}
            Page::None => {}
        }
        self.screens[i].page = page;
        self.screens[i].data = data;
    }
}

/// A page over the game and the screens that is not a screen (the player list, the toasts): it hears nothing and
/// sends no screen away.
fn overlay(kind: &str) -> bool {
    kind == "tab" || kind == "toasts"
}

/// What a page is told about its screen while it draws or hears.
pub struct View<'a> {
    pub opened: f32,
    pub leaving: Option<f32>,
    pub id: u32,
    pub data: &'a Value,
    /// Opened by a word flying up to be its title (Screen::flown).
    pub flown: bool,
}

impl View<'_> {
    pub fn str(&self, key: &str) -> &str {
        self.data.get(key).and_then(Value::as_str).unwrap_or("")
    }
}

/// What the pointer and the keys did.
#[derive(Clone, Debug)]
pub enum Event {
    Press(u64, i32),
    Release(u64, i32),
    Key(String, u32),
    Char(char),
    Scroll(f32),
}

// ---------------------------------------------------------------------- the game's side (lib.rs's JNI calls)

pub fn open(id: u32, kind: &str, json: &str) {
    let data = serde_json::from_str(json).unwrap_or(Value::Null);
    with(|m| m.open(id, kind, data))
}

pub fn data(id: u32, json: &str) {
    let data = serde_json::from_str(json).unwrap_or(Value::Null);
    with(|m| m.data(id, data))
}

pub fn close(id: u32) {
    with(|m| m.close(id))
}

/// A frame: `now` the menus' clock (s), `inp` what the screen up says; its number for the frame op (0 nothing).
pub fn frame(w: f32, h: f32, now: f32, inp: &[f32], in_world: bool) -> (u32, [f32; 64]) {
    with(|m| {
        let n = inp.len().min(m.inp.len());
        m.inp[..n].copy_from_slice(&inp[..n]);
        m.in_world = in_world;
        let Some(c) = m.frame(w, h, now) else { return (0, m.out) };
        if c.p.is_empty() {
            return (0, m.out);
        }
        let (runs, idx, tx) = c.bins();
        debug_assert!(c.p.len() % FLOATS == 0);
        let look = m.look();
        let areas = c.glass_area.into_iter().collect();
        let built = crate::gui::Built { prims: c.p, runs, idx, tiles_x: tx, look, areas, layer: 2, preview: None };
        (crate::gui::push(Some(built)), m.out)
    })
}

impl Menus {
    fn look(&self) -> Look {
        let k = self.frost;
        Look {
            win: [0.0, 0.0, self.w, self.h],
            // in a game the world behind is the HUD's veil (greyed and darkened as the Theme says): no glass of its own
            open: 0.0,
            line: 0.0,
            heat: 0.0,
            line_mode: 0.0,
            veil: 0.0,
            rail: 0.0,
            radius: 0.0,
            cursor: [self.mouse[0], self.mouse[1], 0.0],
            time: self.now,
            blur: self.inp[15].max(0.3),
            fade: 1.0,
            glass: [0.02, 0.02, 0.025, 0.46],
            veil_col: [0.0; 4],
            theme: [0.5, 0.0, 0.0, 0.0],
            fire: [1.0, 0.55, 0.25, 0.0],
            anim: [k, k, -1.0, 0.0],
        }
    }

    /// The camera's place for this screen and how the valley is dimmed (the preview's poses): asked each frame, the
    /// screen up last wins; the corner's title goes.
    pub fn scene_pose(&mut self, pose: [f32; 5], dim: f32, right: f32) {
        self.scene_to[..5].copy_from_slice(&pose);
        self.scene_to[5 + 17] = dim;
        self.scene_to[5 + 19] = right;
        self.scene_to[5 + 20] = 0.0;
        self.scene_to[5 + 21] = 0.0;
        self.scene_to[5 + 18] = self.opt_blur;
    }

    /// Something the game draws over the page (an item: kind 1; a player's face: 2), the page's number for it, its box
    /// (x, y, size) and how seen.
    pub fn extra(&mut self, kind: u32, number: f32, at: [f32; 3], alpha: f32) {
        if number >= 0.0 && alpha > 0.01 {
            // the kind, and in its thousands the screen it is of
            self.extras.push([(kind + 1000 * (self.drawing % 16000)) as f32, number, at[0], at[1], at[2], alpha]);
        }
    }

    /// One of the valley's knobs (launch.glsl Knob0..5, in order) for this frame.
    pub fn knob(&mut self, at: usize, v: f32) {
        self.scene_to[5 + at] = v;
    }

    /// The valley as the game's options make it (Java gives them from inp[16]): how far one sees, clouds and how far,
    /// sparks, brightness, the field of view, reflections (the graphics), the background's blur.
    fn apply_options(&mut self) {
        let o: [f32; 8] = self.inp[16..24].try_into().unwrap();
        if o[0] <= 0.0 {
            return;
        }
        // (kept for the ways into and out of a world and the window before the game: launch::valley)
        crate::launch::set_options(o);
        // the background's blur: the other screens' (the main menu's valley stays sharp, as the game's title screen)
        self.opt_blur = crate::launch::options_into(&mut self.scene_to, &o);
    }

    /// The camera travels to its new place (in and out, 1.4 s); the knobs glide.
    fn tick_scene(&mut self) {
        // the first frame with the game's options: the valley as they make it at once (it was the launch's, in the
        // window before the game, as they make it: nothing glides there from the defaults after the hand-over)
        if !self.scene_seen && self.inp[16] > 0.0 {
            self.scene_seen = true;
            self.scene = self.scene_to;
            self.pose_from.copy_from_slice(&self.scene_to[..5]);
            self.pose_key.copy_from_slice(&self.scene_to[..5]);
            self.pose_t0 = self.now - 10.0;
        }
        let to: [f32; 5] = self.scene_to[..5].try_into().unwrap();
        if to != self.pose_key {
            self.pose_from.copy_from_slice(&self.scene[..5]);
            self.pose_key = to;
            self.pose_t0 = self.now;
        }
        let e = ui::ease_io((self.now - self.pose_t0) / 1.4);
        for i in 0..5 {
            self.scene[i] = self.pose_from[i] + (to[i] - self.pose_from[i]) * e;
        }
        for i in 5..29 {
            // the title in the corner sinks quickly (as the main menu's foot does); the world's knobs glide
            let rate = if i == 25 { 7.0 } else { 3.2 };
            self.scene[i] = ui::toward(self.scene[i], self.scene_to[i], rate, self.dt);
        }
        self.out[out::POSE..out::POSE + 29].copy_from_slice(&self.scene);
    }

    /// How far element `order` of a screen is up: rising out of its line once the screen opened (after its title has
    /// flown in, if one did), sinking back into it as the screen goes.
    pub fn vis(&self, v: &View, order: f32) -> f32 {
        let delay = if v.flown { 0.45 } else { 0.08 };
        let up = ui::smooth(0.0, 1.0, (self.now - v.opened - delay - order.min(14.0) * 0.045) / 0.7);
        match v.leaving {
            Some(at) => up.min(1.0 - ui::smooth(0.0, 1.0, (self.now - at - order.min(14.0) * 0.012) / 0.3)),
            None => up,
        }
    }

    /// A screen's title, top left: flown in from the word that opened the screen (it moves and grows into place), or
    /// risen out of its line; sunk back as the screen goes.
    pub fn title(&mut self, c: &mut Canvas, v: &View, text: &str, left: f32, top: f32, size: f32) {
        if v.leaving.is_none() {
            self.title_now = Some(Fly { text: text.to_string(), face: crate::gui::text::Face::Light, size, at: [left, top], since: 0.0, ls: 0.02 });
        } else if self.back_fly.as_ref().is_some_and(|f| f.text == text && v.leaving.is_some_and(|at| (at - f.since).abs() < 0.3)) {
            // it is flying down into its word (ui.rs word)
            return;
        }
        let flying = self.fly.clone().filter(|f| (f.since - v.opened).abs() < 0.3);
        let line = 0.98;
        if let Some(f) = flying {
            let k = ui::ease_io((self.now - f.since) / 0.85);
            let sz = f.size + (size - f.size) * k;
            let x = f.at[0] + (left - f.at[0]) * k;
            // the word's text box top (its line box from .03 em down) to the title's
            let y0 = f.at[1] + 0.03 * f.size;
            let y = y0 + (top - y0) * k;
            let alpha = match v.leaving {
                Some(at) => 1.0 - ui::smooth(0.0, 1.0, (self.now - at) / 0.3),
                None => 1.0,
            };
            let base = crate::gui::text::fonts().baseline(crate::gui::text::Face::Light, y, sz, line);
            c.text_ls(x, base, sz, crate::gui::text::Face::Light, text, crate::gui::draw::grey(1.0, alpha), 0.02, 1.0 - k);
            if k >= 1.0 && v.leaving.is_none() {
                self.fly = None;
            }
            return;
        }
        // a title that flew in is in its place: it does not rise again; one that did not rises with the rest
        let p = if v.flown && v.leaving.is_none() { 1.0 } else { self.vis(v, 0.0) };
        let q = ui::clamp01(ui::ease_out(0.22, 1.0, p) * 1.15);
        self.rising_text(c, crate::gui::text::Face::Light, text, left, top, size, line, 0.02, crate::gui::draw::grey(1.0, 1.0), 0.0, q);
    }
}

/// The mouse in window px: action 0 moved, 1 pressed, 2 released; button 0 left, 1 right, 2 middle (+16: a double click).
pub fn mouse(x: f32, y: f32, button: i32, action: i32) {
    with(|m| match action {
        1 => m.press(x, y, button),
        2 => m.release(x, y, button),
        // a script plays (dev.rs): the stand's pointer is not ours
        _ if dev::on() => {}
        _ => m.mouse = [x, y],
    })
}

pub fn scroll(dy: f32) {
    with(|m| m.page_event(Event::Scroll(dy)))
}

/// A key by SDL's name for it ("Escape", "Return", "Backspace", "Left", "A"…); `mods` 1 shift, 2 ctrl, 4 alt.
pub fn key(name: &str, mods: u32) {
    with(|m| m.page_event(Event::Key(name.to_string(), mods)))
}

pub fn typed(c: u32) {
    if let Some(ch) = char::from_u32(c) {
        with(|m| m.page_event(Event::Char(ch)))
    }
}

/// What the game draws over the page this frame, 6 numbers each, into `out`: how many.
pub fn extras(out: &mut [f32]) -> usize {
    with(|m| {
        let n = m.extras.len().min(out.len() / 6);
        for (i, e) in m.extras.iter().take(n).enumerate() {
            out[i * 6..i * 6 + 6].copy_from_slice(e);
        }
        n
    })
}

/// What the menus ask of the game since the last time, a line each.
pub fn poll() -> String {
    with(|m| std::mem::take(&mut m.actions).join("\n"))
}
