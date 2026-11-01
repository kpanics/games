//! The HUD in the game. Its elements are the modules of the HUD's section (data::hud_modules): each a plate of dark
//! glass where its place says, drawn with the menu's shapes (draw.rs) by the menu's shader into a texture of its own,
//! which Java puts in the game's live stratum, under the hotbar (HudMixin). Java hands over what the game knows each
//! frame (Facts); the icons glass can't hold (effects' sprites, items, the target's portrait) it draws itself where
//! this says (Hud::icons).
//! The editor is the chat, or "Edit the layout": the left button moves an element (to the edges, to the others, on a
//! grid), the right one opens its settings by it (the menu's rows, the colour wheel), the round arrows put it back.

use super::data::{Cat, Kind, Setting};
use super::i18n::tr;
use super::draw::{grey, mix, Canvas, Rgba};
use super::text::{self, fonts, Face};
use super::theme::{self, fade, ElemLook, HudLook};
use super::{ak, ease_out, inside, smooth, switch, Gui, Hit, Owner, Phase};
use std::cell::Cell;
use std::collections::HashMap;

#[path = "hudbar.rs"]
mod hudbar;
pub use hudbar::Body;
#[path = "hudchat.rs"]
mod hudchat;
pub use hudchat::Chat;
#[path = "hudtext.rs"]
mod hudtext;
#[path = "hudboard.rs"]
mod hudboard;
pub use hudboard::{Board, BoardPlace};
#[path = "hudother.rs"]
mod hudother;
pub use hudother::{BossState, Other, Placed};
#[path = "hudscreen.rs"]
mod hudscreen;
pub use hudscreen::ScreenF;
#[path = "hudinv.rs"]
mod hudinv;
pub use hudinv::InvF;
#[path = "hudpack.rs"]
mod hudpack;
pub use hudpack::PackF;
#[path = "hudcreative.rs"]
mod hudcreative;
#[path = "hudf3.rs"]
mod hudf3;
#[path = "hudcool.rs"]
mod hudcool;

#[path = "hudinvui.rs"]
mod hudinvui;
pub use hudcreative::CrF;
#[path = "chat/mod.rs"]
mod chat;

/// The client's chat (chat/): the server's address and your name, the search on or off, the faces Java draws.
pub fn nchat_server(payload: &str) {
    chat::server(payload);
}

pub fn nchat_find(on: bool) {
    chat::find(on);
}

pub fn nchat_faces() -> String {
    chat::faces()
}

pub fn nchat_online(payload: &str) {
    chat::online(payload);
}

pub fn nchat_to() -> String {
    chat::talking_to()
}

pub fn nchat_sent(to: &str) {
    chat::sent(to);
}

/// A message come (its runs), the chat cleared, its field and hints this frame.
pub fn nchat_add(payload: &str, stack: bool) {
    chat::add(payload, stack);
}

/// The chat's hidden players listed over its window (its settings' button).
pub fn chat_hidden() {
    chat::show_hidden();
}

pub fn nchat_clear() {
    chat::clear();
}

pub fn nchat_input(payload: &str) {
    chat::input(payload);
}

const A_HUD: u64 = 20;
const A_ROW: u64 = 21;
const A_POPH: u64 = 23;
const A_EDIT: u64 = 24;
const A_NOTE: u64 = 25;
const A_EFF: u64 = 26;
/// the music's playhead, eased between the player's answers
const A_PLAYED: u64 = 53;
/// An element is drawn this far in from the corner first, then moved to its place (its glow and shadow fit).
const M: f32 = 160.0;

// ---------------------------------------------------------------------------------------------- what the game knows
#[derive(Clone, Default)]
pub struct Effect {
    pub name: String,
    pub amp: i32,
    /// seconds left (below 0: for ever)
    pub secs: f32,
    pub harmful: bool,
    /// it just ended (Java sends it a moment more, for it to fade out where it was)
    pub gone: bool,
}

#[derive(Clone, Default)]
pub struct Cool {
    pub name: String,
    pub frac: f32,
    pub secs: f32,
}

#[derive(Clone, Default)]
pub struct Target {
    pub name: String,
    pub hp: f32,
    pub max: f32,
    pub abs: f32,
    pub armor: [bool; 4],
    pub dist: f32,
    pub dead: bool,
    pub id: i32,
    pub player: bool,
    /// seconds since the player last hit it
    pub since: f32,
}

#[derive(Clone, Default)]
pub struct Music {
    pub title: String,
    pub artist: String,
    pub pos: f32,
    pub len: f32,
    pub playing: bool,
}

#[derive(Clone, Default)]
pub struct Facts {
    pub fps: f32,
    pub ping: f32,
    pub server: String,
    pub pos: [f64; 3],
    pub dim: i32,
    pub yaw: f32,
    /// seconds into the local day
    pub clock: f32,
    pub chat: bool,
    pub f3: bool,
    /// the way into a world or out of one plays over the game (v12): the HUD away
    pub veiled: bool,
    /// some screen other than the chat is open (the menu too)
    pub screen: bool,
    /// the layout editor is open
    pub editor: bool,
    pub mouse: [f32; 2],
    pub effects: Vec<Effect>,
    pub cools: Vec<Cool>,
    pub target: Option<Target>,
    /// a server's PvP mode (its boss bar): seconds left, how much of it is left (only its end is told, a notification)
    pub combat: Option<[f32; 2]>,
    pub music: Option<Music>,
    /// the placeholders' values (Java's HudVars): {name} in any text of the HUD
    pub vars: HashMap<String, String>,
    /// the player's body (the Custom Hotbar's: Java's HudBody)
    pub body: Option<Body>,
    /// the chat's lines as Java lays them out (the Custom Chat's: Java's HudChat)
    pub chat_lines: Option<Chat>,
    /// the sidebar as Java measures it (the Custom Scoreboard's: Java's HudBoard)
    pub board: Option<Board>,
    /// the boss bars, the title, the action bar (Other Custom Things: Java's HudOther)
    pub other: Option<Other>,
    /// the open container (Custom Gui: Java's HudScreen)
    pub container: Option<ScreenF>,
}

impl Facts {
    /// From Java (cursed.recode.hud.HudFacts): the numbers (a head of 20, then each effect's 4, each cooldown's 2, the
    /// target's 9, the music's 3) and the names, one a line (the server, the effects', the cooldowns', the target's, the
    /// music's title and artist).
    pub fn parse(n: &[f64], s: &str) -> Facts {
        let g = |i: usize| n.get(i).copied().unwrap_or(0.0);
        let (s, vars) = s.split_once("\u{1}\n").unwrap_or((s, ""));
        let mut lines = s.split('\n');
        let mut line = move || lines.next().unwrap_or("").to_string();
        let flags = g(7) as i32;
        let mut f = Facts {
            fps: g(0) as f32,
            ping: g(1) as f32,
            pos: [g(2), g(3), g(4)],
            dim: g(5) as i32,
            yaw: g(6) as f32,
            chat: flags & 1 != 0,
            f3: flags & 2 != 0,
            screen: flags & 4 != 0,
            editor: flags & 8 != 0,
            veiled: flags & 16 != 0,
            mouse: [g(8) as f32, g(9) as f32],
            clock: g(16) as f32,
            ..Default::default()
        };
        f.server = line();
        let mut at = 24;
        for _ in 0..(g(10) as usize).min(48) {
            f.effects.push(Effect { name: line(), amp: g(at) as i32, secs: g(at + 1) as f32, harmful: g(at + 2) > 0.5, gone: g(at + 3) > 1.5 });
            at += 4;
        }
        for _ in 0..(g(11) as usize).min(48) {
            f.cools.push(Cool { name: line(), frac: g(at) as f32, secs: g(at + 1) as f32 });
            at += 2;
        }
        if g(12) > 0.5 {
            let bits = g(at + 3) as i32;
            f.target = Some(Target {
                name: line(),
                hp: g(at) as f32,
                max: (g(at + 1) as f32).max(1.0),
                abs: g(at + 2) as f32,
                armor: [bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0],
                dist: g(at + 4) as f32,
                dead: g(at + 5) > 0.5,
                id: g(at + 6) as i32,
                since: g(at + 7) as f32,
                player: g(at + 8) > 0.5,
            });
            at += 9;
        }
        if g(17) > 0.5 {
            let name = line();
            f.body = Some(Body::parse(&g, at, name));
            at += 55;
        }
        if g(18) > 0.5 {
            let (ch, used) = Chat::parse(&g, at);
            f.chat_lines = Some(ch);
            at += used;
        }
        if g(19) > 0.5 {
            let (b, used) = Board::parse(&g, at, &mut line);
            f.board = b;
            at += used;
        }
        if g(20) > 0.5 {
            let (o, used) = Other::parse(&g, at, &mut line);
            f.other = Some(o);
            at += used;
        }
        if g(21) > 0.5 {
            let (s, used) = ScreenF::parse(&g, at);
            f.container = s;
            at += used;
        }
        if g(13) >= 0.0 {
            f.combat = Some([g(13) as f32, (g(14) as f32).clamp(0.0, 1.0)]);
        }
        if g(15) > 0.5 {
            f.music = Some(Music { title: line(), artist: line(), pos: g(at) as f32, len: g(at + 1) as f32, playing: g(at + 2) > 0.5 });
        }
        for l in vars.lines() {
            if let Some((k, v)) = l.split_once('=') {
                f.vars.insert(k.to_string(), v.to_string());
            }
        }
        f
    }
}

// ---------------------------------------------------------------------------------------------- the HUD's own state
/// The one fought, as the target card shows it (it stays a while after the last hit, and through the death).
#[derive(Clone)]
struct Shown {
    t: Target,
    seen: f32,
    hit_at: f32,
    dead_at: Option<f32>,
    totems: u32,
    /// the health shown (it eases), the lost health's trail (it waits, then follows)
    hp: f32,
    trail: f32,
    trail_at: f32,
}

struct Note {
    title: String,
    line: String,
    at: f32,
    /// its own number (its place in the stack eases by it)
    id: u64,
    /// a thing Java draws by it (Auto Swap's: the number Java keeps it by; below 0 none)
    icon: i64,
}

/// An element's last picture (at (M, M)): it fades out from it when it has nothing more to show.
#[derive(Default)]
struct Ghost {
    prims: Vec<f32>,
    glass: Vec<[f32; 4]>,
    icons: Vec<[f32; 7]>,
    size: [f32; 2],
    /// its plate's shape and glass, counted from its first
    plate: Option<(usize, usize)>,
}

pub struct Hud {
    pub(super) places: HashMap<&'static str, (u8, [f32; 2])>,
    pub(super) defaults: HashMap<&'static str, (u8, [f32; 2])>,
    /// an element being stretched by its corner in the editor: which, the pointer then, its width and height then
    resize: Option<(usize, [f32; 2], [f32; 2])>,
    /// the cover of what plays (its file, its number), Java draws it
    pub(super) cover: Option<(String, u32)>,
    /// the Custom Hotbar's bars: the trail of what each lost, since when it waits, what it was last; the level last
    /// seen and when it went up
    pub(super) bar_trail: [f32; 8],
    pub(super) bar_trail_at: [f32; 8],
    pub(super) bar_trail_seen: [f32; 8],
    pub(super) level_was: i32,
    pub(super) level_at: f32,
    /// the Custom Hotbar's held item's name over it: the one shown, the one it crosses over from, since when
    pub(super) hb_name: String,
    pub(super) hb_name_prev: String,
    pub(super) hb_name_at: f32,
    /// Custom F3's graph: the frames a second sampled, when the last was
    pub(super) f3_fps: std::collections::VecDeque<f32>,
    pub(super) f3_at: f32,
    /// when a message was last copied from the chat
    pub(super) chat_copied_at: f32,
    /// where the Custom Scoreboard's words go (said back to Java), its accent (packed)
    pub(super) board_place: BoardPlace,
    pub(super) board_accent: u32,
    /// where Other Custom Things' words go (by element), the boss bars' fills and trails
    pub(super) placed: HashMap<&'static str, Placed>,
    pub(super) boss: BossState,
    /// Custom Gui: where the hovered slot's light is (window px); the player's elements being drawn over a container
    /// (not the HUD), the icons Java draws over it
    pub(super) screen_hover: [f32; 2],
    pub(super) on_screen: bool,
    pub(super) screen_icons: Vec<[f32; 7]>,
    pub(super) facts: Facts,
    last_t: f32,
    pub(super) speed: f32,
    last_pos: Option<([f64; 3], f32)>,
    eff_max: HashMap<String, f32>,
    /// the effects in the order they came (a row keeps its place while it goes)
    eff_order: Vec<String>,
    target: Option<Shown>,
    notes: Vec<Note>,
    /// Auto Swap's, for the Swap Ring: a swap waiting out FunTime's pause (the number Java keeps its thing by, since
    /// when, how long, its name), the last one put on (its thing, when, its name)
    pub(super) swap_wait: Option<(i64, f32, f32, String)>,
    pub(super) swap_done: Option<(i64, f32, String)>,
    combat_was: bool,
    drag: Option<(usize, [f32; 2])>,
    pub(super) pop: Option<usize>,
    pop_at: f32,
    pop_scroll: f32,
    pop_to: f32,
    pop_rect: [f32; 4],
    /// the search over a long list of an element's settings (the chat's), and whether it is being typed in
    pop_q: String,
    pop_find: bool,
    rects: Vec<(usize, [f32; 4])>,
    /// where the Custom Hotbar's parts were put this frame (what rests on them is lifted over them)
    stack: Vec<([f32; 4], f32)>,
    guides: Vec<(bool, f32)>,
    /// the hits, the pointer, what is under it, the menu's unit: the HUD's (swapped with the menu's while it is drawn
    /// or handles input)
    hits: Vec<([f32; 4], Hit)>,
    mouse: [f32; 2],
    hover: Hit,
    r: f32,
    pub icons: Vec<[f32; 7]>,
    /// Item Cooldowns' things last frame (their names, seconds): one gone as it ran out is ready
    cool_last: Vec<(String, f32)>,
    pub hide_effects: bool,
    pub editing: bool,
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) u: f32,
    dirty: bool,
    /// development: the pointer the HUD's script holds, and when the editor opened
    pub dev_mouse: Option<[f32; 2]>,
    edit_at: f32,
    input_was: bool,
    ghosts: HashMap<usize, Ghost>,
    /// where an element's settings stand (they stay while it changes, moving only when it would cover them)
    pop_pin: Option<(usize, [f32; 2])>,
    note_seq: u64,
    /// the order the modules were switched on in (the keybinds' "As enabled")
    pub lit: HashMap<usize, u64>,
    pub lit_seq: u64,
    /// how far the world behind an open screen is greyed and darkened (the Theme's "World behind the screens"), 0..1
    pub veil: f32,
    /// Custom Inventory's screens: their headers taken, their settings' panel (hudinvui)
    inv: hudinvui::InvUi,
}

impl Hud {
    pub fn new() -> Hud {
        let mut defaults: HashMap<&'static str, (u8, [f32; 2])> = super::data::all_elements().iter().map(|e| (e.id, (e.anchor, e.off))).collect();
        // the inventory's blocks: their offsets from the game's arrangement (Custom Inventory's editor)
        defaults.extend(hudpack::INV_BLOCKS.iter().chain(hudcreative::CR_BLOCKS.iter()).map(|id| (*id, (4u8, [0.0f32, 0.0f32]))));
        Hud {
            places: defaults.clone(),
            defaults,
            resize: None,
            cover: None,
            bar_trail: [0.0; 8],
            bar_trail_at: [0.0; 8],
            bar_trail_seen: [0.0; 8],
            level_was: -1,
            level_at: -10.0,
            hb_name: String::new(),
            hb_name_prev: String::new(),
            hb_name_at: -10.0,
            f3_fps: std::collections::VecDeque::new(),
            f3_at: -10.0,
            chat_copied_at: -10.0,
            board_place: BoardPlace::default(),
            board_accent: 0xffffff,
            placed: HashMap::new(),
            boss: BossState::new(),
            screen_hover: [0.0, 0.0],
            on_screen: false,
            screen_icons: Vec::new(),
            facts: Facts::default(),
            last_t: 0.0,
            speed: 0.0,
            last_pos: None,
            eff_max: HashMap::new(),
            eff_order: Vec::new(),
            target: None,
            notes: Vec::new(),
            swap_wait: None,
            swap_done: None,
            combat_was: false,
            drag: None,
            pop: None,
            pop_at: 0.0,
            pop_scroll: 0.0,
            pop_to: 0.0,
            pop_rect: [0.0; 4],
            pop_q: String::new(),
            pop_find: false,
            rects: Vec::new(),
            stack: Vec::new(),
            guides: Vec::new(),
            hits: Vec::new(),
            mouse: [-1.0, -1.0],
            hover: Hit::None,
            r: 16.0,
            icons: Vec::new(),
            cool_last: Vec::new(),
            hide_effects: false,
            editing: false,
            w: 1.0,
            h: 1.0,
            u: 1.0,
            dirty: false,
            dev_mouse: None,
            edit_at: 0.0,
            input_was: false,
            ghosts: HashMap::new(),
            pop_pin: None,
            note_seq: 0,
            lit: HashMap::new(),
            lit_seq: 0,
            veil: 0.0,
            inv: hudinvui::InvUi::default(),
        }
    }

    pub(super) fn note(&mut self, title: String, line: String, at: f32) {
        self.note_icon(title, line, at, -1);
    }

    /// A note with a thing by it (Java draws it: icon kind 10, `icon` its number).
    pub(super) fn note_icon(&mut self, title: String, line: String, at: f32, icon: i64) {
        self.note_seq += 1;
        self.notes.push(Note { title, line, at, id: self.note_seq, icon });
    }

    pub fn reset_all(&mut self) {
        self.places = self.defaults.clone();
        self.dirty = true;
    }

    /// Custom Inventory's blocks (the inventory's, the containers', creative's) where the game's arrangement has them.
    pub fn reset_inv_places(&mut self) {
        for id in hudpack::INV_BLOCKS.iter().chain(hudcreative::CR_BLOCKS.iter()) {
            self.reset_place(id);
        }
    }

    pub fn reset_place(&mut self, id: &str) {
        if let Some((k, v)) = self.defaults.get_key_value(id) {
            self.places.insert(k, *v);
        }
        self.dirty = true;
    }

    pub fn save_places(&self) -> String {
        let mut ids: Vec<_> = self.places.iter().collect();
        ids.sort_by_key(|p| *p.0);
        ids.iter().map(|(id, (a, o))| format!("hud.{id}.place={a}/{}/{}\n", o[0], o[1])).collect()
    }

    pub fn load_places(&mut self, vals: &HashMap<String, String>) {
        // the player's own elements are made before (Gui::custom_ensure): their places are read here too
        for (id, place) in self.places.iter_mut() {
            let Some(v) = vals.get(&format!("hud.{id}.place")) else { continue };
            let p: Vec<f32> = v.split('/').filter_map(|x| x.parse().ok()).collect();
            if p.len() == 3 && p.iter().all(|v| v.is_finite()) && p[0] >= 0.0 && p[0] < 9.0 {
                *place = (p[0] as u8, [p[1], p[2]]);
            }
        }
    }

    /// What happened since the last frame: speed, effects' longest, the target, the notes it makes.
    fn observe(&mut self, f: Facts, t: f32, dt: f32, notes_on: [bool; 3]) {
        // speed over the ground, eased
        if let Some((p, t0)) = self.last_pos {
            let dtp = t - t0;
            if dtp >= 0.05 {
                let d = ((f.pos[0] - p[0]).powi(2) + (f.pos[2] - p[2]).powi(2)).sqrt() as f32;
                let v = if d > 50.0 { 0.0 } else { d / dtp };
                self.speed += (v - self.speed) * (1.0 - (-dtp * 6.0).exp());
                self.last_pos = Some((f.pos, t));
            }
        } else {
            self.last_pos = Some((f.pos, t));
        }
        // an effect's longest time is the time it was given
        self.eff_max.retain(|k, _| f.effects.iter().any(|e| &e.name == k));
        for e in &f.effects {
            let m = self.eff_max.entry(e.name.clone()).or_insert(e.secs);
            if e.secs > *m {
                *m = e.secs;
            }
            if !self.eff_order.contains(&e.name) {
                self.eff_order.push(e.name.clone());
            }
        }
        self.eff_order.retain(|k| f.effects.iter().any(|e| &e.name == k));
        // the target: shown for a while after the player's last hit on it (Java keeps it half a minute; the element
        // says how long it shows); its death makes a note
        let mut killed = None;
        match (&mut self.target, f.target.clone()) {
            (Some(s), Some(nt)) if s.t.id == nt.id => {
                if nt.hp < s.t.hp - 0.01 {
                    s.hit_at = t;
                    s.trail_at = t + 0.35;
                    s.trail = s.trail.max(s.t.hp);
                }
                if nt.dead && s.dead_at.is_none() {
                    s.dead_at = Some(t);
                    if notes_on[1] {
                        killed = Some(nt.name.clone());
                    }
                }
                s.seen = t - nt.since;
                s.t = nt;
            }
            (_, Some(nt)) => {
                self.target = Some(Shown { hp: nt.hp, trail: nt.hp, trail_at: 0.0, seen: t - nt.since, t: nt, hit_at: -9.0, dead_at: None, totems: 0 });
            }
            _ => {}
        }
        if let Some(name) = killed {
            self.note(name, tr("Killed").into(), t);
        }
        if let Some(s) = self.target.as_mut() {
            s.hp += (s.t.hp - s.hp) * (1.0 - (-dt * 14.0).exp());
            if t > s.trail_at {
                s.trail += (s.hp - s.trail) * (1.0 - (-dt * 5.0).exp());
            }
            s.trail = s.trail.max(s.hp);
        }
        // the fight is over
        let fight = f.combat.is_some();
        if self.combat_was && !fight && notes_on[2] {
            self.note(tr("PvP mode is over").into(), tr("You may leave now").into(), t);
        }
        self.combat_was = fight;
        self.facts = f;
    }
}

// ---------------------------------------------------------------------------------------------- small things
fn mmss(s: f32) -> String {
    let s = s.max(0.0) as u32;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn roman(n: i32) -> String {
    const R: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    if (1..=10).contains(&n) {
        R[(n - 1) as usize].to_string()
    } else {
        n.to_string()
    }
}

pub(super) fn facing_of(yaw: f32) -> &'static str {
    facing(yaw)
}

pub(super) fn mmss_of(s: f32) -> String {
    mmss(s)
}

fn facing(yaw: f32) -> &'static str {
    const D: [&str; 8] = ["South", "South-west", "West", "North-west", "North", "North-east", "East", "South-east"];
    D[((yaw.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8]
}

fn hash(s: &str, i: u32) -> f32 {
    let mut h: u32 = 2166136261 ^ i.wrapping_mul(0x9e3779b9);
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

fn union(a: Option<[f32; 4]>, b: [f32; 4]) -> Option<[f32; 4]> {
    Some(match a {
        Some(g) => [g[0].min(b[0]), g[1].min(b[1]), g[2].max(b[2]), g[3].max(b[3])],
        None => b,
    })
}

/// How an element is drawn: its unit (px a design px), the HUD's look and its own, its settings, whether it shows what
/// it would (the editor), the time.
struct Ctx {
    s: f32,
    look: HudLook,
    el: ElemLook,
    set: Vec<Setting>,
    sample: bool,
    /// Java draws the icons (in the game's HUD; not in the menu's preview, which shows marks in their place)
    icons: bool,
    t: f32,
    /// the element's plate as drawn: its shape and its glass's place (its size eases after)
    main: Cell<Option<(usize, usize)>>,
}

impl Ctx {
    fn on(&self, l: &str) -> bool {
        theme::on(&self.set, l)
    }
    fn num(&self, l: &str) -> f32 {
        theme::num(&self.set, l)
    }
    fn pick(&self, l: &str) -> usize {
        theme::pick(&self.set, l)
    }
    fn col(&self, l: &str, theme_col: Rgba) -> Rgba {
        theme::or(theme::col(&self.set, l), theme_col, self.t)
    }
    fn pad(&self) -> f32 {
        10.0 * self.s * self.look.pad * self.el.pad
    }
    /// Its corners' radius.
    fn radius(&self) -> f32 {
        self.look.radius * self.el.corners * self.s
    }
    /// The element's plate: its glass, if it has one (the element's whole size: it eases to it as the element grows).
    fn plate(&self, c: &mut Canvas, r: [f32; 4]) {
        let at = (c.p.len() / super::draw::FLOATS, c.glass_rects.len());
        self.part(c, r);
        if c.p.len() / super::draw::FLOATS > at.0 {
            self.main.set(Some(at));
        }
    }
    /// A plate of a part of it (a notification of the stack).
    fn part(&self, c: &mut Canvas, r: [f32; 4]) {
        self.el.draw_plate(c, r, &self.look, self.s, self.t);
    }
}

/// Which of `set` a search for `q` (lowercase) shows: a setting whose name (or its words in the language) holds it, a
/// section with all of it when the section's name does, and the section's name over what was found in it.
fn rows_matching(set: &[Setting], q: &str) -> Vec<bool> {
    let says = |l: &str| l.to_lowercase().contains(q) || tr(l).to_lowercase().contains(q);
    let mut m = vec![false; set.len()];
    let mut head: Option<usize> = None;
    let mut head_says = false;
    for (i, s) in set.iter().enumerate() {
        if matches!(s.kind, Kind::Header) {
            head = Some(i);
            head_says = says(s.label);
            m[i] = head_says;
            continue;
        }
        if head_says || says(s.label) {
            m[i] = true;
            if let Some(h) = head {
                m[h] = true;
            }
        }
    }
    m
}

// ---------------------------------------------------------------------------------------------- the frame
impl Gui {
    /// The Theme's settings (the HUD's look is a part of it).
    pub(super) fn hud_set(&self) -> Vec<Setting> {
        self.theme.clone()
    }

    /// Swaps the HUD's hits, pointer, hover and unit in for the menu's (and back): the rows and the colour picker are
    /// the menu's code, working on whichever is in.
    fn hud_swap(&mut self) {
        std::mem::swap(&mut self.hits, &mut self.hud.hits);
        std::mem::swap(&mut self.mouse, &mut self.hud.mouse);
        std::mem::swap(&mut self.hover, &mut self.hud.hover);
        std::mem::swap(&mut self.r, &mut self.hud.r);
    }

    pub(super) fn in_hud_ref<R>(&mut self, f: impl FnOnce(&Gui) -> R) -> R {
        self.hud_swap();
        let r = f(self);
        self.hud_swap();
        r
    }

    pub(super) fn in_hud<R>(&mut self, f: impl FnOnce(&mut Gui) -> R) -> R {
        self.hud_swap();
        let r = f(self);
        self.hud_swap();
        r
    }

    /// Custom Inventory's "All blocks back": every block where the game's arrangement has it, kept.
    pub(super) fn inv_places_back(&mut self) {
        self.hud.reset_inv_places();
        self.save();
    }

    /// Whether the HUD takes the pointer and the keys: the chat or the editor is open, the menu is not.
    pub(super) fn hud_input(&self) -> bool {
        (self.hud.facts.chat || self.hud.facts.editor) && self.phase == Phase::Closed
    }

    /// One frame of the HUD, and what is open by an element (its settings, a list, the colour wheel: drawn over the
    /// game's whole GUI, by the chat or the editor); None: nothing to draw.
    pub(super) fn hud_frame(&mut self, w: f32, h: f32, facts: Facts) -> Option<(Canvas, Option<Canvas>)> {
        self.ensure_loaded();
        let t = self.clock.elapsed().as_secs_f32();
        let dt = (t - self.hud.last_t).clamp(0.0, 0.1);
        self.hud.last_t = t;
        self.now = t;
        self.keys_notes(t);
        let menu_dt = self.dt;
        self.dt = dt;
        let hset = self.hud_set();
        let mut look = HudLook::of(&hset, t);
        // the Hud module's switch: the whole HUD on or off
        look.show = self.mods.iter().any(|m| m.id == "hud" && m.on);
        let notes_id = self.mod_index("notes");
        let nset = self.mods[notes_id].set.clone();
        let notes_on = [theme::on(&nset, "Totems"), theme::on(&nset, "Kills"), theme::on(&nset, "Combat")];
        let was_editor = self.hud.editing;
        let mut facts = facts;
        // what plays on the computer (Windows' media controls), when the game said nothing of it
        if facts.music.is_none() {
            if let Some(n) = super::media::now(self.dir.as_deref()) {
                self.hud.cover = n.cover.clone();
                facts.music = Some(Music { title: n.title, artist: n.artist, pos: n.pos, len: n.len, playing: n.playing });
            } else {
                self.hud.cover = None;
            }
        }
        self.hud.observe(facts, t, dt, notes_on);
        self.iu_stale(t);
        // (the swing's editor is the editor's screen too, but not the layout's)
        self.hud.editing = self.hud.facts.editor && !self.swing_ed.on;
        if was_editor && !self.hud.editing {
            self.hud_closed();
        }
        self.swing_follow();
        self.undo_tick();
        self.profile_follow();
        let input_now = self.hud.facts.chat || self.hud.facts.editor;
        let was_input = std::mem::replace(&mut self.hud.input_was, input_now);
        if !was_input && input_now && std::env::var_os("CURSED_DEV_HUDPLAY").is_none() {
            self.hud.edit_at = t;
            if self.hud_dev.is_some() {
                let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
                eprintln!("[hud dev] editor at {wall}");
            }
            if let Some(d) = self.hud_dev.as_mut() {
                d.restart();
            }
        }
        // development: CURSED_DEV_HUDPLAY plays the script from the HUD's first frame, the chat or the editor open or not
        let play = std::env::var_os("CURSED_DEV_HUDPLAY").is_some();
        if play && self.hud_dev.is_some() && self.hud.edit_at <= 0.0 {
            self.hud.edit_at = t;
            let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
            eprintln!("[hud dev] editor at {wall}");
        }
        if self.hud_dev.is_some() && (input_now || play) {
            if let Some(m) = self.hud.dev_mouse {
                self.hud.facts.mouse = m;
            }
            let since = t - self.hud.edit_at;
            self.hud_dev_run(since);
            if let Some(m) = self.hud.dev_mouse {
                self.hud.facts.mouse = m;
            }
        }
        let shown = |g: &Gui, id: &str| g.mods.iter().any(|m| m.id == id && m.on);
        self.hud.hide_effects = look.show && look.hide_effects && shown(self, "effects");
        self.hud.w = w;
        self.hud.h = h;
        let base = ((w / 1920.0).min(h / 1080.0)).clamp(0.5, 3.0);
        self.hud.u = base * look.scale;
        let f = &self.hud.facts;
        // a screen open over the world (not the chat, not the layout editor): the world behind it greys and darkens as
        // the Theme says (the menu's window does that itself), and the HUD goes away under it — but in the menu's
        // Interface section, where the HUD is what is being changed
        let gui_open = self.phase != Phase::Closed;
        let under = f.screen && !f.editor && !f.veiled;
        let veil_to = if under && !gui_open { 1.0 } else { 0.0 };
        let hidden = !look.show || (look.hide_f3 && f.f3) || (under && !(gui_open && self.cat == Cat::Hud)) || f.veiled;
        self.hud.veil = self.an(ak(A_HUD, 9001), veil_to, if look.anim { 9.0 * look.speed } else { 1000.0 }, 0.0);
        let input = self.hud_input();
        let mut c = Canvas::new(w, h);
        self.hud.icons.clear();
        self.hud.rects.clear();
        self.hud.stack.clear();
        self.hud.board_place.alpha = 0.0;
        for pl in self.hud.placed.values_mut() {
            pl.alpha = 0.0;
        }
        // our HUD away (F3, a menu, switched off) fades out, and back in; the game's own effect icons come back at once
        let away = hidden && !self.hud.editing && !self.swing_ed.on;
        let show = self.an(ak(A_HUD, 9000), if away { 0.0 } else { 1.0 }, if look.anim { 10.0 * look.speed } else { 1000.0 }, 1.0);
        if away {
            self.hud.hide_effects = false;
            if show <= 0.005 {
                self.dt = menu_dt;
                self.hud.hits.clear();
                // nothing of the HUD, but the world behind a screen still greys (and CS's crosshair stays)
                let cross = cs_crosshair(&mut c, w, h);
                return (self.hud.veil > 0.001 || cross).then(|| (c, None));
            }
        }
        let p = theme::Palette::of(&self.theme, t);
        theme::set_pal(p);
        self.hud_swap();
        self.r = (base.clamp(0.62, 1.6)) * 16.0;
        self.w = w;
        self.h = h;
        // the pointer first, on the last frame's hits: a slider or the wheel being dragged finds its track there (the
        // chat and the editor hand over no moves, only this, each frame)
        if input {
            let m = self.hud.facts.mouse;
            self.hud_move_in(m[0], m[1]);
        }
        self.hits.clear();
        // the editor: the world a shade darker, the grid, a word on how
        let ev = self.an(ak(A_EDIT, 0), if self.hud.editing { 1.0 } else { 0.0 }, 9.0, 0.0);
        if ev > 0.01 {
            c.rect([0.0, 0.0, w, h], 0.0, grey(0.0, 0.28 * ev));
            if look.grid >= 4.0 {
                let gs = look.grid * self.hud.u;
                let mut x = gs;
                while x < w {
                    c.rect([x - 0.5, 0.0, x + 0.5, h], 0.0, grey(1.0, 0.035 * ev));
                    x += gs;
                }
                let mut y = gs;
                while y < h {
                    c.rect([0.0, y - 0.5, w, y + 0.5], 0.0, grey(1.0, 0.035 * ev));
                    y += gs;
                }
            }
        }
        text::set_scale(look.text_k);
        let mut ids: Vec<usize> = (0..self.mods.len()).filter(|&i| self.mods[i].cat == Cat::HudEl && !self.mods[i].plain).collect();
        // the open chat is a window over the rest (the other elements' examples go under it); the world behind the
        // HUD darkened or blurred as it sets (easing out as it closes)
        let ci = self.mod_index("chat");
        if self.hud.facts.chat && !self.hud.editing {
            if let Some(at) = ids.iter().position(|&i| i == ci) {
                ids.remove(at);
                ids.push(ci);
            }
        }
        if look.show && !self.hud.editing && ids.contains(&ci) {
            self.chat_behind(&mut c, w, h);
        }
        // Name Tags over the players: the world's, under the HUD's elements
        if !self.hud.editing {
            self.tags_draw(&mut c, w, h);
        }
        for i in ids {
            self.hud_element(&mut c, i, &look, t);
        }
        // Shulker Preview's boxes over the world, over the elements
        if !self.hud.editing {
            self.shulk_draw(&mut c);
        }
        // the swing's editor: the HUD steps aside (only the hand and the editor are seen), back as it closes
        let keep = self.an(ak(A_HUD, 9002), if self.swing_ed.on { 0.0 } else { 1.0 }, if look.anim { 9.0 * look.speed } else { 1000.0 }, 1.0);
        if keep < 0.999 {
            c.fade_from(0, keep);
            for ic in self.hud.icons.iter_mut() {
                ic[6] *= keep;
            }
            self.hud.board_place.alpha *= keep;
            for pl in self.hud.placed.values_mut() {
                pl.alpha *= keep;
            }
        }
        // Server Adaptation's zone of the thing held and the times running (over the world, under the rest)
        self.adapt_draw(&mut c, w, h);
        // Kill Effects' shockwaves and impact frame
        self.killfx_draw(&mut c, w, h);
        // Streamer Mode's mark
        self.streamer_draw(&mut c, w);
        // Free Look's ring round the crosshair; Item Cooldowns' rings under it, its sound
        self.fl_draw(&mut c, w, h);
        self.cool_cross(&mut c, w, h);
        self.cool_sound();
        if self.swing_ed.on {
            text::set_scale(1.0);
            self.swing_draw(&mut c);
            text::set_scale(look.text_k);
        }
        // the guides an element snapped to
        if self.hud.drag.is_some() && look.guides {
            for &(vert, v) in &self.hud.guides {
                let col = fade(look.accent, 0.55);
                if vert {
                    c.rect([v - 0.5, 0.0, v + 0.5, h], 0.0, col);
                } else {
                    c.rect([0.0, v - 0.5, w, v + 0.5], 0.0, col);
                }
            }
        }
        if ev > 0.01 {
            text::set_scale(1.0);
            let r = self.r;
            let line = tr("Drag to move  ·  Right click to change  ·  Ctrl+Z to undo  ·  Esc to finish");
            // what Ctrl+Z (or Ctrl+Y) just did takes the line's place a while, the line's width gliding to its own
            let said = self.toast.clone().filter(|t| self.now - t.1 < 2.6);
            let tk = said.as_ref().map_or(0.0, |t| smooth(0.0, 0.18, self.now - t.1) * (1.0 - smooth(2.1, 2.6, self.now - t.1)));
            let lw = fonts().width(Face::Medium, 0.8 * r, line);
            let sw = said.as_ref().map_or(0.0, |t| fonts().width(Face::Medium, 0.8 * r, &t.0));
            let tw = lw + (sw - lw) * tk + 2.4 * r;
            // in the middle of the screen's lower half (the boss bars cover the top, the hotbar the bottom)
            let y0 = h * 0.7 + (1.0 - ev) * 0.6 * r;
            let b = [(w - tw) / 2.0, y0, (w + tw) / 2.0, y0 + 2.2 * r];
            c.alpha = ev;
            c.glass(b, 1.1 * r, [p.glass[0], p.glass[1], p.glass[2], p.glass[3].max(0.8)], p.blur, 0.6, 0.0);
            c.alpha = ev * (1.0 - tk);
            c.text_c(w / 2.0, b[1] + 1.38 * r - tk * 0.4 * r, 0.8 * r, Face::Medium, line, p.t1);
            if let Some((s, _)) = &said {
                c.alpha = ev * tk;
                c.text_c(w / 2.0, b[1] + 1.38 * r + (1.0 - tk) * 0.4 * r, 0.8 * r, Face::Medium, s, p.t1);
            }
            c.alpha = 1.0;
        }
        text::set_scale(p.text);
        let mut over = None;
        if input {
            let mut o = Canvas::new(w, h);
            self.hud_pop(&mut o);
            self.popover(&mut o);
            self.picker(&mut o);
            self.card(&mut o);
            if !o.p.is_empty() {
                over = Some(o);
            }
        } else {
            self.hud.pop = None;
        }
        text::set_scale(1.0);
        self.chat_cut_icons();
        if show < 0.999 {
            c.fade_from(0, show);
            for ic in self.hud.icons.iter_mut() {
                ic[6] *= show;
            }
            // (the words Java draws over the HUD's plates — the scoreboard's, the boss bars', the title — go with it)
            self.hud.board_place.alpha *= show;
            for pl in self.hud.placed.values_mut() {
                pl.alpha *= show;
            }
        }
        // CS's crosshair over all of it (not faded with the HUD: it is the hands')
        cs_crosshair(&mut c, w, h);
        self.hud_swap();
        self.dt = menu_dt;
        if self.hud.dirty && self.hud.drag.is_none() {
            self.hud.dirty = false;
            self.save();
        }
        Some((c, over))
    }

    /// How high the Custom Hotbar's hotbar stands this frame: its top as a part of the HUD's height from its foot (none
    /// while it is not drawn). The game's parts given back rest over it.
    pub(super) fn hotbar_top(&self) -> Option<f32> {
        let i = self.mods.iter().position(|m| m.id == "hb-hotbar")?;
        let r = self.hud.rects.iter().find(|r| r.0 == i)?.1;
        Some(((self.hud.h - r[1]) / self.hud.h.max(1.0)).clamp(0.0, 1.0))
    }

    /// One element: drawn by the corner, measured, moved to its place (as its anchor and offset say, or where it is
    /// being dragged), its hit for the editor. Nothing about it jumps: its plate eases to its size, it comes in and goes
    /// out scaling a little and fading (with nothing more to show, from its last picture).
    fn hud_element(&mut self, c: &mut Canvas, i: usize, look: &HudLook, t: f32) {
        use super::draw::FLOATS;
        let id = self.mods[i].id;
        // a part of a module of its own (the Custom Hotbar): nothing of it while that module is off
        if self.mods[i].sub.is_some_and(|g| !self.group_on(g)) {
            return;
        }
        // Item Cooldowns' panel: off with the module (or not chosen there), gone where the server forbids it
        if id == "cooldowns" && self.hidden_here(hudcool::MODULE) {
            return;
        }
        let on = self.mods[i].on && (id != "cooldowns" || self.cool_at("Panel"));
        let editing = self.hud.editing;
        let input = self.hud_input() && !self.hud.on_screen;
        let set = self.mods[i].set.clone();
        // the player's element shown over the inventories only (Custom Gui): not on the HUD (the editor shows it)
        if theme::pick(&set, "Show on") == 1 && !editing && !self.hud.on_screen {
            return;
        }
        let el = ElemLook::of(&set, look, t);
        let ctx = Ctx { s: self.hud.u * el.scale, look: *look, el, set, sample: input, icons: true, t, main: Cell::new(None) };
        let speed = if look.anim { look.speed } else { 100.0 };
        let a = self.an(ak(A_HUD, i), if on || editing { 1.0 } else { 0.0 }, 8.0 * speed, 0.0);
        if a < 0.005 {
            return;
        }
        let start = c.p.len() / FLOATS;
        let clip0 = c.clip_now();
        let icons0 = self.hud.icons.len();
        let area0 = c.glass_area.take();
        let gr0 = c.glass_rects.len();
        let hov = input && matches!(self.hover, Hit::HudElem(j) if j == i);
        let dragging = matches!(self.hud.drag, Some((j, _)) if j == i);
        let open = self.hud.pop == Some(i);
        let lift = self.an(ak(A_HUD, 1000 + i), if dragging { 1.0 } else if hov || open { 0.6 } else { 0.0 }, 14.0, 0.0);
        c.alpha = a * ctx.el.alpha * if on { 1.0 } else { 0.35 };
        text::set_scale(look.text_k * ctx.el.text_k);
        text::set_style(ctx.el.family, ctx.el.bolder, ctx.el.tshadow);
        let drawn = self.el_draw(c, id, &ctx);
        text::set_style(0, false, 0.0);
        text::set_scale(look.text_k);
        c.alpha = 1.0;
        let vis = self.an(ak(A_HUD, 2000 + i), if drawn.is_some() { 1.0 } else { 0.0 }, 9.0 * speed, 0.0);
        let (size, plate) = match drawn {
            Some(sz) => {
                let g = self.hud.ghosts.entry(i).or_default();
                g.prims.clear();
                g.prims.extend_from_slice(&c.p[start * FLOATS..]);
                g.glass = c.glass_rects[gr0..].to_vec();
                g.icons = self.hud.icons[icons0..].to_vec();
                g.size = sz;
                g.plate = ctx.main.get().map(|(p, gi)| (p - start, gi - gr0));
                (sz, ctx.main.get())
            }
            None => {
                c.p.truncate(start * FLOATS);
                self.hud.icons.truncate(icons0);
                c.glass_rects.truncate(gr0);
                c.glass_area = area0;
                if vis <= 0.005 {
                    self.hud.ghosts.remove(&i);
                    return;
                }
                let Some(g) = self.hud.ghosts.get(&i) else { return };
                c.p.extend_from_slice(&g.prims);
                c.glass_rects.extend_from_slice(&g.glass);
                self.hud.icons.extend_from_slice(&g.icons);
                (g.size, g.plate.map(|(p, gi)| (p + start, gi + gr0)))
            }
        };
        // the notifications move each of their own: their stack is laid out from its edge, no easing of the whole; the
        // chat's lines are where Java puts them (its panel eases by itself)
        let own = id == "notes" || id == "chat";
        let [nw, nh] = size;
        let (ew, eh) = if own {
            (nw, nh)
        } else {
            (self.an(ak(A_HUD, 3000 + i), nw, 14.0 * speed, nw), self.an(ak(A_HUD, 4000 + i), nh, 14.0 * speed, nh))
        };
        if let Some((pi, gi)) = plate.filter(|_| !own) {
            // the plate, and the interference over it when it has it
            let n = if ctx.el.style == 2 && ctx.el.bg { 2 } else { 1 };
            for k in pi..(pi + n).min(c.p.len() / FLOATS) {
                let q = &mut c.p[k * FLOATS..(k + 1) * FLOATS];
                q[2] = q[0] + ew;
                q[3] = q[1] + eh;
            }
            if let Some(g) = c.glass_rects.get_mut(gi) {
                g[2] += ew - nw;
                g[3] += eh - nh;
            }
        }
        // a line of light along its top, a gleam crossing it now and then
        if ctx.el.light && ctx.el.bg && !own {
            let rad = ctx.radius();
            let a0 = c.alpha;
            c.alpha = a * ctx.el.alpha * vis;
            c.rect([M + rad * 0.6, M + 0.6, M + ew - rad * 0.6, M + 1.6], 0.5, fade(ctx.el.accent, 0.22));
            let ph = (t * 0.17 + i as f32 * 0.37).fract();
            let gx = M + rad + (ew - 2.0 * rad) * ph;
            let gw = (ew * 0.22).max(20.0);
            let gc = fade(ctx.el.accent, 0.75 * (std::f32::consts::PI * ph).sin());
            c.rect_lit([gx - gw, M + 0.4, gx, M + 1.8], 0.7, fade(gc, 0.0), gc, 1.0);
            c.rect_lit([gx, M + 0.4, gx + gw, M + 1.8], 0.7, gc, fade(gc, 0.0), 1.0);
            c.alpha = a0;
        }
        // under the pointer in the editor: a light over it (no outline)
        if lift > 0.01 {
            c.rect([M, M, M + ew, M + eh], ctx.radius(), grey(1.0, 0.045 * lift));
        }
        // a card or a picture: its corner to stretch it by, in the editor
        if editing && self.hud_resizable(i) {
            let k = self.an(ak(A_HUD, 5000 + i), if hov || matches!(self.hud.resize, Some((j, ..)) if j == i) { 1.0 } else { 0.35 }, 12.0, 0.35);
            let q = [M + ew, M + eh];
            let l = 9.0 * self.hud.u.max(0.7);
            let col = grey(1.0, 0.75 * k);
            c.line([q[0] - l, q[1] - 2.0], [q[0] - 2.0, q[1] - 2.0], 2.0, col, 0.0);
            c.line([q[0] - 2.0, q[1] - l], [q[0] - 2.0, q[1] - 2.0], 2.0, col, 0.0);
        }
        // coming and going: a little smaller, fainter
        if vis < 0.999 {
            if !own {
                let k = 0.92 + 0.08 * ease_out(vis);
                let mid = [M + ew / 2.0, M + eh / 2.0];
                c.scale_about(start, mid, k, [0.0, 0.0]);
                for ic in &mut self.hud.icons[icons0..] {
                    ic[2] = mid[0] + (ic[2] - mid[0]) * k;
                    ic[3] = mid[1] + (ic[3] - mid[1]) * k;
                    ic[4] *= k;
                    ic[5] *= k;
                }
            }
            c.fade_from(start, vis);
            for ic in &mut self.hud.icons[icons0..] {
                ic[6] *= vis;
            }
        }
        let (w, h) = (self.hud.w, self.hud.h);
        let (anchor, off) = self.hud.places.get(id).copied().unwrap_or((0, [24.0, 24.0]));
        let (col, row) = ((anchor % 3) as usize, (anchor / 3) as usize);
        let fx = [0.0, 0.5, 1.0];
        let ax = [0.0, w / 2.0, w][col];
        let ay = [0.0, h / 2.0, h][row];
        let u = self.hud.u;
        let mut x0 = ax + off[0] * u - fx[col] * ew;
        let mut y0 = ay + off[1] * u - fx[row] * eh;
        // the Custom Hotbar's parts left where they are by default rest on each other, as the game's do: one grown
        // taller (hearts in two rows, absorption) lifts what is over it, smoothly
        let part = self.mods[i].sub == Some("Custom Hotbar");
        // (what lies under it is told by where each is by default: a part already lifted is still under)
        let base = y0 + eh;
        if part && self.hud.places.get(id) == self.hud.defaults.get(id) {
            let gap = 4.0 * u;
            let mut bottom = base;
            for (r, their) in &self.hud.stack {
                if r[0] < x0 + ew - 1.0 && r[2] > x0 + 1.0 && r[1] < bottom + gap - 0.5 && *their > base + 0.5 {
                    bottom = r[1] - gap;
                }
            }
            y0 -= self.an(ak(A_HUD, 7000 + i), base - bottom, 10.0 * speed, 0.0);
        }
        if let Some((j, grab)) = self.hud.drag {
            if j == i {
                let (nx, ny) = self.hud_snap(i, [self.mouse[0] - grab[0], self.mouse[1] - grab[1]], [ew, eh], look);
                x0 = nx;
                y0 = ny;
                // where it is now, as an anchor of the screen's third it is in and an offset from it
                let (cx, cy) = (x0 + ew / 2.0, y0 + eh / 2.0);
                let col = if cx < w / 3.0 { 0 } else if cx > w * 2.0 / 3.0 { 2 } else { 1 };
                let row = if cy < h / 3.0 { 0 } else if cy > h * 2.0 / 3.0 { 2 } else { 1 };
                let ax = [0.0, w / 2.0, w][col];
                let ay = [0.0, h / 2.0, h][row];
                let off = [(x0 + fx[col] * ew - ax) / u, (y0 + fx[row] * eh - ay) / u];
                self.hud.places.insert(id, ((row * 3 + col) as u8, off));
                self.hud.dirty = true;
            }
        }
        x0 = x0.clamp(0.0, (w - ew).max(0.0));
        y0 = y0.clamp(0.0, (h - eh).max(0.0));
        // the target shakes when hit
        let mut sx = 0.0;
        if id == "target" && ctx.on("Shake on a hit") {
            if let Some(s) = self.hud.target.as_ref() {
                let k = 1.0 - ((t - s.hit_at) / 0.35).clamp(0.0, 1.0);
                sx = (t * 70.0).sin() * 3.0 * ctx.s * k * k;
            }
        }
        let d = [x0 - M + sx, y0 - M];
        c.shift(start, d);
        c.shift_own_clips(start, clip0, d);
        let mut area = area0;
        for g in &mut c.glass_rects[gr0..] {
            g[0] += d[0];
            g[2] += d[0];
            g[1] += d[1];
            g[3] += d[1];
            area = union(area, *g);
        }
        c.glass_area = area;
        for ic in &mut self.hud.icons[icons0..] {
            ic[2] += d[0];
            ic[3] += d[1];
        }
        let rect = [x0, y0, x0 + ew, y0 + eh];
        self.hud.rects.push((i, rect));
        if part && drawn.is_some() {
            self.hud.stack.push((rect, base));
        }
        if id == "chat" {
            self.chat_placed([x0, y0]);
        }
        if matches!(id, "bossbars" | "titles" | "actionbar") {
            let al = a * ctx.el.alpha * vis * if on { 1.0 } else { 0.35 };
            if let Some(pl) = self.hud.placed.get_mut(id) {
                pl.abs = [x0 + pl.rel[0], y0 + pl.rel[1]];
                pl.alpha = al;
            }
        }
        if id == "scoreboard" {
            let rel = self.hud.board_place.rel;
            self.hud.board_place.corner = [x0 + rel[0], y0 + rel[1]];
            self.hud.board_place.alpha = a * ctx.el.alpha * vis * if on { 1.0 } else { 0.35 };
            let ac = ctx.el.accent;
            self.hud.board_accent = ((ac[0].clamp(0.0, 1.0) * 255.0) as u32) << 16 | ((ac[1].clamp(0.0, 1.0) * 255.0) as u32) << 8 | (ac[2].clamp(0.0, 1.0) * 255.0) as u32;
        }
        // (the chat is moved in the editor only: in the open chat its words are clicked)
        if input && !(id == "chat" && !editing) {
            self.hits.push((rect, Hit::HudElem(i)));
        }
    }

    /// Where a dragged element goes: on the grid, then to an edge or another element's edge or middle near it.
    fn hud_snap(&mut self, i: usize, p: [f32; 2], size: [f32; 2], look: &HudLook) -> (f32, f32) {
        let (w, h) = (self.hud.w, self.hud.h);
        let u = self.hud.u;
        let mut x = p[0];
        let mut y = p[1];
        if look.grid >= 2.0 {
            let g = look.grid * u;
            x = (x / g).round() * g;
            y = (y / g).round() * g;
        }
        self.hud.guides.clear();
        let thr = 7.0 * u.max(0.6);
        let mut xs: Vec<f32> = Vec::new();
        let mut ys: Vec<f32> = Vec::new();
        if look.snap_edges {
            let m = 8.0 * u;
            xs.extend([m, w / 2.0, w - m]);
            ys.extend([m, h / 2.0, h - m]);
        }
        if look.snap_elems {
            for &(j, r) in &self.hud.rects {
                if j != i {
                    xs.extend([r[0], r[2], (r[0] + r[2]) / 2.0]);
                    ys.extend([r[1], r[3], (r[1] + r[3]) / 2.0]);
                }
            }
        }
        let best = |v0: f32, len: f32, cands: &[f32]| -> Option<(f32, f32)> {
            let mut best: Option<(f32, f32, f32)> = None;
            for &cnd in cands {
                for (k, edge) in [0.0, len / 2.0, len].iter().enumerate() {
                    let _ = k;
                    let d = (v0 + edge - cnd).abs();
                    if d < thr && best.map_or(true, |b| d < b.0) {
                        best = Some((d, cnd - edge, cnd));
                    }
                }
            }
            best.map(|b| (b.1, b.2))
        };
        if let Some((nx, line)) = best(x, size[0], &xs) {
            x = nx;
            self.hud.guides.push((true, line));
        }
        if let Some((ny, line)) = best(y, size[1], &ys) {
            y = ny;
            self.hud.guides.push((false, line));
        }
        (x, y)
    }

    // ------------------------------------------------------------------------------------------ the elements
    /// An element drawn from (M, M), its size; None: it has nothing to show.
    fn el_draw(&mut self, c: &mut Canvas, id: &str, x: &Ctx) -> Option<[f32; 2]> {
        match id {
            "watermark" => self.el_watermark(c, x),
            "keybinds" => self.el_keybinds(c, x),
            "coords" => self.el_coords(c, x),
            "music" => self.el_music(c, x),
            "effects" => self.el_effects(c, x),
            "target" => self.el_target(c, x),
            "cooldowns" => self.el_cooldowns(c, x),
            "swap-ring" => self.el_swap_ring(c, x),
            "notes" => self.el_notes(c, x),
            "chat" => self.el_chat(c, x),
            "scoreboard" => self.el_board(c, x),
            "bossbars" => self.el_bossbars(c, x),
            "titles" => self.el_titles(c, x),
            "actionbar" => self.el_actionbar(c, x),
            "holograms" => self.el_holograms(c, x),
            "hb-hotbar" => self.el_hotbar(c, x),
            "hb-health" => self.el_health(c, x),
            "hb-food" => self.el_food(c, x),
            "hb-armor" => self.el_armor(c, x),
            "hb-air" => self.el_air(c, x),
            "hb-xp" => self.el_xp(c, x),
            "hb-mount" => self.el_mount(c, x),
            "f3-position" => self.el_f3_position(c, x),
            "f3-performance" => self.el_f3_performance(c, x),
            "f3-world" => self.el_f3_world(c, x),
            "f3-target" => self.el_f3_target(c, x),
            _ if id.starts_with("x-card") => self.el_card(c, x),
            _ if id.starts_with("x-image") => self.el_image(c, x, id),
            _ if id.starts_with("x-") => self.el_ctext(c, x),
            _ => None,
        }
    }

    /// The menu's panel of a HUD element: the element itself in the preview's place, over the world as the HUD shows
    /// it (with examples where the game has nothing to show), opening from its line as the other previews do. The
    /// HUD's Look shows the watermark and the effects.
    pub(super) fn hud_preview(&mut self, c: &mut Canvas, st: [f32; 4], i: usize, reveal: f32) {
        let r = self.r;
        let open = ease_out(((reveal - 0.12) / 0.7).clamp(0.0, 1.0));
        let cy = (st[1] + st[3]) / 2.0;
        let hh = (st[3] - st[1]) / 2.0 * open;
        c.push_clip([st[0], cy - hh, st[2], cy + hh]);
        c.rect(st, 0.75 * r, grey(0.0, 0.28));
        let t = self.now;
        let hset = self.hud_set();
        let look = HudLook::of(&hset, t);
        let ids: Vec<usize> = if self.mods[i].plain { ["watermark", "effects"].iter().map(|id| self.mod_index(id)).collect() } else { vec![i] };
        let menu_text = text::scale();
        text::set_scale(look.text_k);
        let icons0 = self.hud.icons.len();
        let base = c.alpha;
        let n = ids.len() as f32;
        for (k, &j) in ids.iter().enumerate() {
            let id = self.mods[j].id;
            let set = self.mods[j].set.clone();
            let el = ElemLook::of(&set, &look, t);
            // drawn once to be measured, then again at the size that fits its place
            let mut s = (r / 16.0) * 1.35 * look.scale * el.scale;
            let mut size = None;
            for pass in 0..2 {
                let start = c.p.len() / super::draw::FLOATS;
                let gr0 = c.glass_rects.len();
                let area0 = c.glass_area;
                c.alpha = base * el.alpha * if self.mods[j].on { 1.0 } else { 0.4 };
                let ctx = Ctx { s, look, el, set: set.clone(), sample: true, icons: false, t, main: Cell::new(None) };
                text::set_scale(look.text_k * el.text_k);
                text::set_style(el.family, el.bolder, el.tshadow);
                let got = self.el_draw(c, id, &ctx);
                text::set_style(0, false, 0.0);
                text::set_scale(look.text_k);
                let Some([w, h]) = got else {
                    c.p.truncate(start * super::draw::FLOATS);
                    c.glass_rects.truncate(gr0);
                    c.glass_area = area0;
                    break;
                };
                // what it drew out of its box counts too (the hotbar's off hand beside it); not its shadow nor a glow
                let mut bb = [M, M, M + w, M + h];
                for q in c.p[start * super::draw::FLOATS..].chunks(super::draw::FLOATS) {
                    if q[12] != super::draw::SHADOW && q[12] != super::draw::GLOW {
                        bb = [bb[0].min(q[0]), bb[1].min(q[1]), bb[2].max(q[2]), bb[3].max(q[3])];
                    }
                }
                let (bw, bh) = (bb[2] - bb[0], bb[3] - bb[1]);
                let box_h = (st[3] - st[1]).min(15.0 * r);
                let room = [(st[2] - st[0]) * 0.86, box_h * 0.86 / n];
                let fit = (room[0] / bw).min(room[1] / bh).min(1.0);
                if pass == 0 && fit < 0.999 {
                    c.p.truncate(start * super::draw::FLOATS);
                    c.glass_rects.truncate(gr0);
                    c.glass_area = area0;
                    s *= fit;
                    continue;
                }
                let slot_cy = st[1] + (st[3] - st[1]).min(15.0 * r) * (k as f32 + 0.5) / n;
                let d = [(st[0] + st[2]) / 2.0 - (bb[0] + bb[2]) / 2.0, slot_cy - (bb[1] + bb[3]) / 2.0];
                c.shift(start, d);
                for g in &mut c.glass_rects[gr0..] {
                    g[0] += d[0];
                    g[2] += d[0];
                    g[1] += d[1];
                    g[3] += d[1];
                }
                size = got;
                break;
            }
            if size.is_none() {
                c.alpha = base;
                c.text_c((st[0] + st[2]) / 2.0, st[1] + (st[3] - st[1]).min(15.0 * r) / 2.0, 0.8 * r, Face::Medium, tr("Nothing to show yet"), fade(grey(1.0, 1.0), 0.35));
            }
        }
        self.hud.icons.truncate(icons0);
        c.alpha = base;
        text::set_scale(menu_text);
        c.pop_clip();
        // the line it opens from, as the other previews
        let lk = (reveal / 0.22).clamp(0.0, 1.0);
        let f = 1.0 - smooth(0.22, 0.9, reveal);
        if f > 0.0 {
            let wl = (st[2] - st[0]) * lk;
            let x0 = (st[0] + st[2]) / 2.0 - wl / 2.0;
            for yy in [cy - hh, cy + hh] {
                c.line([x0, yy], [x0 + wl, yy], 1.0, grey(1.0, f), 8.0);
            }
        }
    }

    fn el_watermark(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let f = &self.hud.facts;
        let p = x.pad();
        let hgt = 32.0 * s * x.look.pad.max(0.9);
        let title = self.fill(theme::text(&x.set, "Title"));
        let mut items: Vec<(String, &str)> = Vec::new();
        if x.on("FPS") {
            items.push((format!("{}", f.fps.round() as i32), " fps"));
        }
        if x.on("Ping") {
            items.push((format!("{}", f.ping.round() as i32), " ms"));
        }
        if x.on("Clock") {
            let secs = f.clock as u32;
            let (hh, mm, ss) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
            let (hh, suffix) = if x.on("24-hour clock") { (hh, "") } else { (if hh % 12 == 0 { 12 } else { hh % 12 }, if hh < 12 { " am" } else { " pm" }) };
            let v = if x.on("Seconds") { format!("{hh:02}:{mm:02}:{ss:02}") } else { format!("{hh:02}:{mm:02}") };
            items.push((v, suffix));
        }
        if x.on("Server") && !f.server.is_empty() {
            items.push((f.server.clone(), ""));
        }
        let sep = x.pick("Separators");
        let (vs, us) = (12.5 * s, 11.0 * s);
        let gap = if sep == 2 { 14.0 * s } else { 9.0 * s };
        let logo = x.on("Logo");
        let ls = 17.0 * s;
        let tw = if title.is_empty() { 0.0 } else { fonts().width(Face::Semi, 13.5 * s, &title) };
        let mut w = p + if logo { ls + if tw > 0.0 { 7.0 * s } else { 0.0 } } else { 0.0 } + tw;
        for (k, (v, u)) in items.iter().enumerate() {
            if k > 0 || logo || tw > 0.0 {
                w += 2.0 * gap + if sep == 1 { 1.0 } else { 0.0 };
            }
            w += fonts().width(Face::Medium, vs, v) + fonts().width(Face::Regular, us, u);
        }
        w += p;
        x.plate(c, [M, M, M + w, M + hgt]);
        let mid = M + hgt / 2.0;
        let mut cx = M + p;
        if logo {
            let lc = x.col("Logo colour", x.el.accent);
            c.glow([cx + ls / 2.0, mid], ls * 0.9, fade(lc, 0.12));
            c.logo([cx, mid - ls / 2.0, cx + ls, mid + ls / 2.0], lc);
            cx += ls + if tw > 0.0 { 7.0 * s } else { 0.0 };
        }
        if tw > 0.0 {
            c.text(cx, mid + 4.7 * s, 13.5 * s, Face::Semi, &title, x.el.text);
            cx += tw;
        }
        for (k, (v, u)) in items.iter().enumerate() {
            if k > 0 || logo || tw > 0.0 {
                cx += gap;
                match sep {
                    0 => c.rect([cx - 1.6 * s, mid - 1.6 * s, cx + 1.6 * s, mid + 1.6 * s], 1.6 * s, fade(x.el.text2, 0.7)),
                    1 => c.rect([cx, mid - 6.5 * s, cx + 1.0, mid + 6.5 * s], 0.0, fade(x.el.text2, 0.45)),
                    _ => {}
                }
                cx += gap + if sep == 1 { 1.0 } else { 0.0 };
            }
            cx += c.text(cx, mid + 4.4 * s, vs, Face::Medium, v, x.el.text);
            cx += c.text(cx, mid + 4.4 * s, us, Face::Regular, u, x.el.text2);
        }
        Some([w, hgt])
    }

    fn el_keybinds(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let only_keys = x.on("Only modules with a key");
        let show_keys = x.on("Keys");
        let speed = if x.look.anim { 12.0 * x.look.speed } else { 1000.0 };
        let order = x.pick("Order");
        let sort = |g: &Gui, rows: &mut Vec<(usize, &'static str, String, f32)>| match order {
            1 => rows.sort_by(|a, b| a.1.cmp(b.1)),
            2 => rows.sort_by(|a, b| fonts().width(Face::Medium, 12.0, b.1).total_cmp(&fonts().width(Face::Medium, 12.0, a.1))),
            // the last switched on at the bottom (those on from the start in the menu's order)
            _ => rows.sort_by_key(|r| (g.hud.lit.get(&r.0).copied().unwrap_or(0), r.0)),
        };
        // the rows to show: those on, as many as it may show (in the order they were switched on the oldest give way,
        // so the one just switched on is always seen); each comes and goes by its own
        let mut rows: Vec<(usize, &'static str, String, f32)> = Vec::new();
        for i in 0..self.mods.len() {
            let m = &self.mods[i];
            // (a module may keep itself out of it, "Show in list"; one hidden on this server is off here)
            let listed = !m.set.iter().any(|s| s.label == "Show in list" && matches!(s.kind, Kind::Toggle(false)));
            if !(m.plain || m.cat == Cat::Hud || m.cat == Cat::HudEl || (only_keys && m.binds.is_empty())) && m.on && listed && !self.hidden_here(m.id) {
                rows.push((i, m.name, m.binds.first().map(|b| b.label()).unwrap_or_default(), 1.0));
            }
        }
        sort(self, &mut rows);
        let most = x.num("Most rows").max(1.0) as usize;
        if order == 0 && rows.len() > most {
            rows.drain(..rows.len() - most);
        }
        rows.truncate(most);
        let seen: Vec<usize> = rows.iter().map(|r| r.0).collect();
        rows.clear();
        for i in 0..self.mods.len() {
            let m = &self.mods[i];
            if m.plain || m.cat == Cat::Hud || m.cat == Cat::HudEl {
                continue;
            }
            let (name, key) = (m.name, m.binds.first().map(|b| b.label()).unwrap_or_default());
            let v = self.an(ak(A_ROW, i), if seen.contains(&i) { 1.0 } else { 0.0 }, speed, 0.0);
            if v > 0.01 {
                rows.push((i, name, key, v));
            }
        }
        let sample = rows.is_empty() && x.sample;
        if sample {
            rows = vec![(usize::MAX, "Sprint", "R".into(), 1.0), (usize::MAX, "Name Tags", "N".into(), 1.0), (usize::MAX, "Full Bright", String::new(), 1.0)];
        }
        if rows.is_empty() {
            return None;
        }
        sort(self, &mut rows);
        let (ns, ks) = (12.5 * s, 10.5 * s);
        let rh = 20.0 * s;
        // the title as typed; its default in the game's language
        let title = if x.on("Title") { Some(self.fill(tr(theme::text(&x.set, "Title text")))) } else { None };
        let mut w: f32 = title.as_ref().map_or(0.0, |t| fonts().width(Face::Semi, 11.0 * s, t));
        for r in &rows {
            let kw = if show_keys && !r.2.is_empty() { fonts().width(Face::Mono, ks, &r.2) + 16.0 * s } else { 0.0 };
            w = w.max(fonts().width(Face::Medium, ns, r.1) + kw);
        }
        w += 2.0 * p;
        let th = if title.is_some() { 20.0 * s } else { 0.0 };
        let hgt: f32 = 2.0 * p - 3.0 * s + th + rows.iter().map(|r| rh * ease_out(r.3)).sum::<f32>();
        x.plate(c, [M, M, M + w, M + hgt]);
        let mut y = M + p - 1.5 * s;
        if let Some(tt) = title {
            c.text(M + p, y + 12.5 * s, 11.0 * s, Face::Semi, &tt, x.el.text2);
            y += th;
        }
        let base = c.alpha;
        for (i, name, key, v) in rows {
            // a row opens its place and slides in from the side as it lights; going, the other way round
            let e = ease_out(v);
            c.alpha = base * smooth(0.3, 1.0, v);
            let mid = y + rh * e / 2.0;
            let dx = -(1.0 - e) * 12.0 * s;
            let heat = if i != usize::MAX && x.on("Light up when switched") { self.heat.get(&i).map_or(0.0, |t0| 1.0 - ((self.now - t0) / 0.9).clamp(0.0, 1.0)) } else { 0.0 };
            if heat > 0.0 {
                c.glow([M + p + 30.0 * s + dx, mid], 34.0 * s, fade(x.el.accent, 0.22 * heat));
            }
            c.text(M + p + dx, mid + 4.4 * s, ns, Face::Medium, name, mix(x.el.text, x.el.accent, heat));
            if show_keys && !key.is_empty() {
                c.text_r(M + w - p, mid + 3.8 * s, ks, Face::Mono, &key, x.el.text2);
            }
            y += rh * e;
        }
        c.alpha = base;
        Some([w, hgt])
    }

    fn el_coords(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let f = &self.hud.facts;
        let d = x.pick("Decimals");
        let mut rows: Vec<(String, String)> = Vec::new();
        // (Streamer Mode hiding them: dots in their place)
        let hide = self.streamer_hides("Coordinates");
        let dots = |v: String| if hide { "•••".to_string() } else { v };
        if x.on("XYZ") {
            rows.push(("XYZ".into(), dots(format!("{:.*}  {:.*}  {:.*}", d, f.pos[0], d, f.pos[1], d, f.pos[2]))));
        }
        if x.on("Other dimension") && f.dim != 2 {
            let (name, k) = if f.dim == 1 { ("Overworld", 8.0) } else { ("Nether", 0.125) };
            rows.push((tr(name).into(), dots(format!("{:.*}  {:.*}", d, f.pos[0] * k, d, f.pos[2] * k))));
        }
        if x.on("Facing") {
            rows.push((tr("Facing").into(), format!("{}  {:.0}°", tr(facing(f.yaw)), f.yaw.rem_euclid(360.0))));
        }
        if x.on("Speed") {
            rows.push((tr("Speed").into(), format!("{:.2} {}", self.hud.speed, tr("b/s"))));
        }
        if rows.is_empty() {
            return None;
        }
        let (ls, vs) = (11.0 * s, 12.0 * s);
        let lw = rows.iter().map(|r| fonts().width(Face::Regular, ls, &r.0)).fold(0.0, f32::max) + 11.0 * s;
        let vw = rows.iter().map(|r| fonts().width(Face::Mono, vs, &r.1)).fold(0.0, f32::max);
        let rh = 19.0 * s;
        let w = 2.0 * p + lw + vw;
        let hgt = 2.0 * p + rh * rows.len() as f32 - 3.0 * s;
        x.plate(c, [M, M, M + w, M + hgt]);
        for (k, (l, v)) in rows.iter().enumerate() {
            let base = M + p - 1.5 * s + rh * k as f32 + rh / 2.0 + 4.2 * s;
            c.text(M + p, base, ls, Face::Regular, l, x.el.text2);
            c.text(M + p + lw, base, vs, Face::Mono, v, x.el.text);
        }
        Some([w, hgt])
    }

    fn el_music(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let m = match self.hud.facts.music.clone() {
            Some(m) if m.playing || !x.on("Only while playing") => m,
            _ if x.sample => Music { title: tr("Nothing is playing").into(), artist: tr("Play something in any player").into(), pos: 42.0, len: 200.0, playing: false },
            _ => return None,
        };
        let cover = x.on("Cover");
        let wave = x.on("Waveform");
        let cs = 48.0 * s;
        let w = 252.0 * s;
        let hgt = 2.0 * p + cs.max(if wave { 40.0 * s } else { 32.0 * s }) + if x.on("Time") { 12.0 * s } else { 0.0 };
        x.plate(c, [M, M, M + w, M + hgt]);
        let mut tx = M + p;
        let t = x.t;
        let real = cover && x.icons && self.hud.cover.is_some() && !x.sample;
        if real {
            // the cover itself (Java draws it in its place, kind 8), on a plate of its own
            c.rect([M + p, M + p, M + p + cs, M + p + cs], 6.0 * s, grey(0.0, 0.5));
            let n = self.hud.cover.as_ref().map_or(0, |c| c.1);
            self.hud.icons.push([8.0, n as f32, M + p, M + p, cs, cs, c.alpha]);
            tx += cs + 11.0 * s;
        } else if cover {
            // a record turning while it plays: grooves, its label in the accent, a gleam going round
            let cc = [M + p + cs / 2.0, M + p + cs / 2.0];
            c.rect([cc[0] - cs / 2.0, cc[1] - cs / 2.0, cc[0] + cs / 2.0, cc[1] + cs / 2.0], cs / 2.0, grey(0.035, 0.95));
            for k in 0..5 {
                c.ring(cc, cs * (0.2 + 0.055 * k as f32), 1.0, grey(1.0, 0.05));
            }
            c.rect([cc[0] - cs * 0.15, cc[1] - cs * 0.15, cc[0] + cs * 0.15, cc[1] + cs * 0.15], cs * 0.15, fade(x.el.accent, 0.85));
            c.rect([cc[0] - 1.5 * s, cc[1] - 1.5 * s, cc[0] + 1.5 * s, cc[1] + 1.5 * s], 1.5 * s, grey(0.0, 1.0));
            let a = if m.playing { t * 1.9 } else { 0.6 };
            for k in [0.0f32, 3.14159] {
                let aa = a + k;
                c.line([cc[0] + aa.cos() * cs * 0.2, cc[1] + aa.sin() * cs * 0.2], [cc[0] + aa.cos() * cs * 0.46, cc[1] + aa.sin() * cs * 0.46], 2.0 * s, grey(1.0, 0.07), 2.0 * s);
            }
            tx += cs + 11.0 * s;
        }
        let x1 = M + w - p;
        let title = fonts().fit(Face::Semi, 13.0 * s, &m.title, x1 - tx);
        c.text(tx, M + p + 12.0 * s, 13.0 * s, Face::Semi, &title, x.el.text);
        if x.on("Artist") {
            let artist = fonts().fit(Face::Regular, 11.0 * s, &m.artist, x1 - tx);
            c.text(tx, M + p + 26.0 * s, 11.0 * s, Face::Regular, &artist, x.el.text2);
        }
        let played = if m.len > 0.0 { (m.pos / m.len).clamp(0.0, 1.0) } else { 0.0 };
        // eased: the player answers twice a second, a song changed goes back smoothly too
        let played = self.an(ak(A_PLAYED, 0), played, 6.0, played);
        if wave {
            // the waveform: what is played bright, the rest dim; the bars breathe while it plays
            let wc = x.col("Waveform colour", x.el.accent);
            let n = 44;
            let (y0, y1) = (M + p + 31.0 * s, M + p + 47.0 * s);
            let bw = (x1 - tx) / n as f32;
            for k in 0..n {
                let base = 0.25 + 0.75 * (0.6 * hash(&m.title, k) + 0.4 * hash(&m.title, k / 3 + 99));
                let live = if m.playing { 0.82 + 0.18 * (t * 7.0 + k as f32 * 0.9).sin() } else { 1.0 };
                let hh = (y1 - y0) * base * live;
                let xm = tx + bw * (k as f32 + 0.5);
                // how much of this bar is played: the one the playhead is in fills from its left as it goes
                let lit = (played * n as f32 - k as f32).clamp(0.0, 1.0);
                let mid = (y0 + y1) / 2.0;
                let bar = [xm - bw * 0.28, mid - hh / 2.0, xm + bw * 0.28, mid + hh / 2.0];
                if lit < 1.0 {
                    c.rect(bar, bw * 0.28, fade(wc, 0.25));
                }
                if lit >= 1.0 {
                    c.rect(bar, bw * 0.28, wc);
                } else if lit > 0.0 {
                    c.push_clip([bar[0], bar[1] - 1.0, bar[0] + (bar[2] - bar[0]) * lit, bar[3] + 1.0]);
                    c.rect(bar, bw * 0.28, wc);
                    c.pop_clip();
                }
            }
        }
        if x.on("Time") {
            let yb = M + hgt - p + 1.0 * s;
            if m.len > 0.0 {
                c.text(tx, yb, 10.0 * s, Face::Mono, &mmss(m.pos), x.el.text2);
                c.text_r(x1, yb, 10.0 * s, Face::Mono, &mmss(m.len), x.el.text2);
            } else {
                // a player that keeps its length to itself (a stream, a radio): what it is doing instead
                let st = if m.playing { tr("Playing") } else { tr("Paused") };
                c.text(tx, yb, 10.0 * s, Face::Regular, st, x.el.text2);
            }
        }
        Some([w, hgt])
    }

    fn el_effects(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let speed = if x.look.anim { 11.0 * x.look.speed } else { 1000.0 };
        // each row comes in and goes out where it is (an effect that ended is sent a moment more, gone)
        let mut list: Vec<(usize, Effect, f32, f32)> = Vec::new();
        for (k, e) in self.hud.facts.effects.clone().into_iter().enumerate() {
            let max = *self.hud.eff_max.get(&e.name).unwrap_or(&e.secs);
            let v = self.an(ak(A_EFF, (hash(&e.name, 7) * 65535.0) as usize), if e.gone { 0.0 } else { 1.0 }, speed, 0.0);
            if v > 0.01 {
                list.push((k, e, max, v));
            }
        }
        let sample = list.is_empty() && x.sample;
        if sample {
            list = vec![
                (usize::MAX, Effect { name: tr("Speed").into(), amp: 1, secs: 94.0, harmful: false, gone: false }, 180.0, 1.0),
                (usize::MAX, Effect { name: tr("Strength").into(), amp: 0, secs: 41.0, harmful: false, gone: false }, 90.0, 1.0),
                (usize::MAX, Effect { name: tr("Poison").into(), amp: 0, secs: 6.0, harmful: true, gone: false }, 20.0, 1.0),
            ];
        }
        if list.is_empty() {
            return None;
        }
        match x.pick("Order") {
            1 => list.sort_by(|a, b| (if a.1.secs < 0.0 { 1e9 } else { a.1.secs }).total_cmp(&(if b.1.secs < 0.0 { 1e9 } else { b.1.secs }))),
            2 => list.sort_by(|a, b| a.1.name.cmp(&b.1.name)),
            _ => list.sort_by_key(|e| self.hud.eff_order.iter().position(|n| *n == e.1.name).unwrap_or(usize::MAX)),
        }
        let rings = x.on("Rings");
        let ring = x.col("Ring", x.el.accent);
        let harm = x.col("Harmful", x.el.accent);
        let (ns, ts) = (12.5 * s, 10.5 * s);
        let rh = 30.0 * s;
        let label = |e: &Effect| if x.on("Level") && e.amp > 0 { format!("{} {}", e.name, roman(e.amp + 1)) } else { e.name.clone() };
        let time = |e: &Effect| if e.secs < 0.0 { "∞".to_string() } else { mmss(e.secs) };
        let nw = list.iter().map(|e| fonts().width(Face::Medium, ns, &label(&e.1))).fold(0.0, f32::max);
        let tw = if x.on("Time") { list.iter().map(|e| fonts().width(Face::Mono, ts, &time(&e.1))).fold(0.0, f32::max) + 12.0 * s } else { 0.0 };
        let w = 2.0 * p + 34.0 * s + nw + tw;
        let hgt = 2.0 * p - 4.0 * s + list.iter().map(|e| rh * ease_out(e.3)).sum::<f32>();
        x.plate(c, [M, M, M + w, M + hgt]);
        let base = c.alpha;
        let mut y = M + p - 2.0 * s;
        for (idx, e, max, v) in list.iter() {
            let ev = ease_out(*v);
            let cy = y + rh * ev / 2.0;
            y += rh * ev;
            let dx = (1.0 - ev) * 12.0 * s;
            let blink = x.on("Blink before the end") && !e.gone && e.secs >= 0.0 && e.secs < 10.0;
            c.alpha = base * smooth(0.25, 1.0, *v) * if blink { 0.62 + 0.38 * (x.t * 7.0).cos().abs() } else { 1.0 };
            let rc = [M + p + 13.0 * s + dx, cy];
            let col = if e.harmful { harm } else { ring };
            if rings {
                c.ring(rc, 12.5 * s, 2.0 * s, fade(col, 0.14));
                let part = if e.gone { 0.0 } else if e.secs < 0.0 { 1.0 } else { (e.secs / max.max(1.0)).clamp(0.0, 1.0) };
                if part >= 0.999 {
                    c.ring(rc, 12.5 * s, 2.0 * s, col);
                } else {
                    c.arc(rc, 12.5 * s, 2.0 * s, part, col);
                }
            }
            if *idx == usize::MAX || !x.icons {
                c.rect([rc[0] - 3.0 * s, rc[1] - 3.0 * s, rc[0] + 3.0 * s, rc[1] + 3.0 * s], 3.0 * s, fade(col, 0.8));
            } else {
                self.hud.icons.push([1.0, *idx as f32, rc[0] - 8.5 * s, rc[1] - 8.5 * s, 17.0 * s, 17.0 * s, c.alpha]);
            }
            c.text(M + p + 34.0 * s + dx, cy + 4.4 * s, ns, Face::Medium, &label(e), if e.harmful { mix(x.el.text, harm, 0.35) } else { x.el.text });
            if x.on("Time") {
                c.text_r(M + w - p + dx, cy + 3.8 * s, ts, Face::Mono, &if e.gone { mmss(0.0) } else { time(e) }, x.el.text2);
            }
        }
        c.alpha = base;
        Some([w, hgt])
    }

    fn el_target(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let keep = x.num("Shown after a hit");
        let shown = match self.hud.target.clone() {
            Some(sh) if x.t - sh.seen < keep && sh.dead_at.map_or(true, |d| x.t - d < 1.6) => Some(sh),
            _ => None,
        };
        let sh = match shown {
            Some(sh) => sh,
            // the example is the player: their nick, their model and armour (Java draws them), a fight's numbers
            None if x.sample => Shown {
                t: Target { name: self.player.clone(), hp: 13.0, max: 20.0, abs: 4.0, armor: [true; 4], dist: 3.2, dead: false, id: -1, player: true, since: 0.0 },
                seen: x.t,
                hit_at: -9.0,
                dead_at: None,
                totems: 1,
                hp: 13.0,
                trail: 16.0,
                trail_at: 0.0,
            },
            None => return None,
        };
        let sample = sh.t.id == -1;
        let portrait = x.pick("Portrait");
        let ps = 54.0 * s;
        let w = 240.0 * s;
        let hgt = 2.0 * p + ps;
        let dead_k = sh.dead_at.map_or(0.0, |d| ((x.t - d) / 1.6).clamp(0.0, 1.0));
        let base = c.alpha;
        c.alpha = base * (1.0 - smooth(0.45, 1.0, dead_k));
        x.plate(c, [M, M, M + w, M + hgt]);
        let mut tx = M + p;
        if portrait < 2 {
            let pr = [M + p, M + p, M + p + ps, M + p + ps];
            c.rect(pr, 9.0 * s, grey(0.0, 0.32));
            c.glow([pr[0] + ps / 2.0, pr[1] + ps * 0.62], ps * 0.55, fade(x.el.accent, 0.06));
            if x.icons {
                // index 1: the player themself (the example)
                self.hud.icons.push([if portrait == 0 { 3.0 } else { 4.0 }, if sample { 1.0 } else { 0.0 }, pr[0], pr[1], ps, ps, c.alpha]);
            }
            tx += ps + 11.0 * s;
        }
        let x1 = M + w - p;
        let health = x.col("Health", x.el.text);
        let gold = x.col("Absorption", x.el.accent);
        // the name, the health as a number
        let hp_txt = if x.on("Health number") { format!("{:.1}", sh.t.hp.max(0.0)) } else { String::new() };
        let abs_txt = if x.on("Health number") && sh.t.abs > 0.0 { format!(" +{:.0}", sh.t.abs) } else { String::new() };
        let nw = fonts().width(Face::Mono, 12.0 * s, &hp_txt) + fonts().width(Face::Mono, 11.0 * s, &abs_txt);
        let name = fonts().fit(Face::Semi, 13.5 * s, &sh.t.name, x1 - tx - nw - 8.0 * s);
        let ny = M + p + 13.0 * s;
        c.text(tx, ny, 13.5 * s, Face::Semi, &name, mix(x.el.text, x.el.text2, dead_k * 0.8));
        let ax = c.text_r(x1, ny, 11.0 * s, Face::Mono, &abs_txt, gold);
        c.text_r(x1 - ax, ny, 12.0 * s, Face::Mono, &hp_txt, x.el.text);
        // the bar: health, the lost health's trail, absorption in gold after it
        let total = (sh.t.max + sh.t.abs).max(1.0);
        let (bx0, bx1) = (tx, x1);
        let by = M + p + 22.0 * s;
        let bh = 6.0 * s;
        let bw = bx1 - bx0;
        c.rect([bx0, by, bx1, by + bh], bh / 2.0, grey(1.0, 0.08));
        let hpk = (sh.hp / total).clamp(0.0, 1.0);
        let trk = (sh.trail / total).clamp(0.0, 1.0);
        if x.on("Lost health trail") && trk > hpk {
            c.rect([bx0, by, bx0 + bw * trk, by + bh], bh / 2.0, fade(health, 0.32));
        }
        if hpk > 0.0 {
            c.rect([bx0, by, bx0 + bw * hpk, by + bh], bh / 2.0, health);
        }
        if sh.t.abs > 0.0 {
            let a0 = bx0 + bw * hpk;
            let a1 = bx0 + bw * ((sh.hp + sh.t.abs) / total).clamp(0.0, 1.0);
            c.rect([a0, by, a1, by + bh], bh / 2.0, gold);
            c.glow([(a0 + a1) / 2.0, by + bh / 2.0], (a1 - a0) * 0.6, fade(gold, 0.15));
        }
        // under it: the armour (Java draws the items), the totems it popped, how far
        let ry = by + bh + 7.0 * s;
        let mut rx = tx;
        if x.on("Armour") {
            for k in 0..4 {
                let r = [rx, ry, rx + 16.0 * s, ry + 16.0 * s];
                if sample && x.icons {
                    // the player's own armour (an empty slot stays a faint mark)
                    c.rect([r[0] + 2.0 * s, r[1] + 2.0 * s, r[2] - 2.0 * s, r[3] - 2.0 * s], 3.0 * s, grey(1.0, 0.05));
                    self.hud.icons.push([5.0, 4.0 + k as f32, r[0], r[1], 16.0 * s, 16.0 * s, c.alpha]);
                } else if sh.t.armor[k] && x.icons {
                    self.hud.icons.push([5.0, k as f32, r[0], r[1], 16.0 * s, 16.0 * s, c.alpha]);
                } else {
                    c.rect([r[0] + 2.0 * s, r[1] + 2.0 * s, r[2] - 2.0 * s, r[3] - 2.0 * s], 3.0 * s, grey(1.0, if sh.t.armor[k] { 0.16 } else { 0.05 }));
                }
                rx += 18.0 * s;
            }
            rx += 6.0 * s;
        }
        if x.on("Totems") && sh.totems > 0 {
            if !x.icons {
                c.rect([rx + 3.0 * s, ry + 2.0 * s, rx + 13.0 * s, ry + 14.0 * s], 3.0 * s, fade(gold, 0.5));
            } else {
                self.hud.icons.push([6.0, 0.0, rx, ry, 16.0 * s, 16.0 * s, c.alpha]);
            }
            rx += 18.0 * s;
            c.text(rx, ry + 12.0 * s, 11.0 * s, Face::Mono, &format!("×{}", sh.totems), gold);
        }
        if x.on("Distance") {
            c.text_r(x1, ry + 12.0 * s, 10.5 * s, Face::Mono, &format!("{:.1} m", sh.t.dist), x.el.text2);
        }
        c.alpha = base;
        Some([w, hgt])
    }

    fn el_cooldowns(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let mut list: Vec<(usize, Cool)> = self.hud.facts.cools.iter().cloned().enumerate().collect();
        if list.is_empty() && x.sample {
            list = vec![(usize::MAX, Cool { name: tr("Ender Pearl").into(), frac: 0.6, secs: 0.6 }), (usize::MAX, Cool { name: tr("Shield").into(), frac: 0.3, secs: 1.5 })];
        }
        if list.is_empty() {
            return None;
        }
        if theme::pick(&x.set, "Kind") == 1 {
            return self.cool_list(c, x, &list);
        }
        let rings = x.on("Rings");
        let ring = x.col("Ring", x.el.accent);
        let secs = x.on("Seconds");
        let cw = 34.0 * s;
        let w = 2.0 * p + cw * list.len() as f32 - 4.0 * s;
        let hgt = 2.0 * p + 30.0 * s + if secs { 13.0 * s } else { 0.0 };
        x.plate(c, [M, M, M + w, M + hgt]);
        for (k, (idx, cd)) in list.iter().enumerate() {
            let cc = [M + p + cw * k as f32 + 15.0 * s, M + p + 15.0 * s];
            if rings {
                c.ring(cc, 13.5 * s, 2.0 * s, fade(ring, 0.13));
                c.arc(cc, 13.5 * s, 2.0 * s, cd.frac.clamp(0.0, 1.0), ring);
            }
            if *idx == usize::MAX || !x.icons {
                c.rect([cc[0] - 5.0 * s, cc[1] - 5.0 * s, cc[0] + 5.0 * s, cc[1] + 5.0 * s], 2.0 * s, grey(1.0, 0.3));
            } else {
                self.hud.icons.push([2.0, *idx as f32, cc[0] - 8.0 * s, cc[1] - 8.0 * s, 16.0 * s, 16.0 * s, c.alpha]);
            }
            if secs {
                let txt = self.cool_secs(cd.secs);
                c.text_c(cc[0], M + p + 41.0 * s, 10.0 * s, Face::Mono, &txt, x.el.text2);
            }
        }
        Some([w, hgt])
    }

    /// Auto Swap's, by the crosshair: what a swap puts on — while FunTime's pause holds it, in a ring filling as the
    /// pause goes; put on, it comes in a little bigger and stays a moment.
    fn el_swap_ring(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let t = x.t;
        let stay = x.num("Stays").max(0.1);
        // the one waiting (its thing, how far its pause is, its name, how long it has shown), else the one put on
        let wait = self.hud.swap_wait.as_ref().map(|(tok, at, dur, name)| (*tok, ((t - at) / dur.max(0.01)).clamp(0.0, 1.0), name.clone(), t - at, false));
        let done = if x.pick("Shown") == 1 {
            None
        } else {
            self.hud.swap_done.as_ref().filter(|d| t - d.1 < stay).map(|(tok, at, name)| (*tok, 1.0, name.clone(), t - at, true))
        };
        let (tok, frac, name, age, put) = match wait.or(done) {
            Some(v) => v,
            None if x.sample => (-1, 0.6, tr("Shield").to_string(), 1.0, false),
            None => return None,
        };
        let named = x.on("Thing's name");
        let fs = 11.5 * s;
        let nw = if named { fonts().width(Face::Semi, fs, &name).min(160.0 * s) + 8.0 * s } else { 0.0 };
        let w = 2.0 * p + 30.0 * s + nw;
        let hgt = 2.0 * p + 30.0 * s;
        let base = c.alpha;
        // put on: it comes in a little bigger, and goes as its time ends
        let pop = if put { 1.0 + 0.22 * (1.0 - ease_out((age / 0.25).min(1.0))) } else { 1.0 };
        let kin = if put { 1.0 - smooth(stay - 0.25, stay, age) } else { smooth(0.0, 0.12, age) };
        c.alpha = base * kin;
        x.plate(c, [M, M, M + w, M + hgt]);
        let cc = [M + p + 15.0 * s, M + p + 15.0 * s];
        if x.on("Rings") {
            let ring = x.col("Ring", x.el.accent);
            c.ring(cc, 13.5 * s, 2.0 * s, fade(ring, 0.13));
            c.arc(cc, 13.5 * s, 2.0 * s, frac, ring);
            if put {
                // the ring's flash as it goes on
                c.ring(cc, 13.5 * s * pop, 2.0 * s, fade(ring, 0.5 * (1.0 - smooth(0.0, 0.35, age))));
            }
        }
        let is = 8.0 * s * pop;
        if tok < 0 || !x.icons {
            c.rect([cc[0] - 5.0 * s, cc[1] - 5.0 * s, cc[0] + 5.0 * s, cc[1] + 5.0 * s], 2.0 * s, grey(1.0, 0.3));
        } else {
            self.hud.icons.push([10.0, tok as f32, cc[0] - is, cc[1] - is, 2.0 * is, 2.0 * is, c.alpha]);
        }
        if named {
            let shown = fonts().fit(Face::Semi, fs, &name, 160.0 * s);
            c.text(M + p + 36.0 * s, cc[1] + fs * 0.36, fs, Face::Semi, &shown, x.el.text);
        }
        c.alpha = base;
        Some([w, hgt])
    }

    fn el_notes(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let life = x.num("Duration").max(0.5);
        let most = x.num("Most at once").max(1.0) as usize;
        let t = x.t;
        // more than may be shown: the oldest go, fading as their time would end
        let over = self.hud.notes.len().saturating_sub(most);
        for n in self.hud.notes.iter_mut().take(over) {
            n.at = n.at.min(t - life + 0.3);
        }
        for n in self.hud.notes.iter().filter(|n| t - n.at >= life) {
            self.anim.remove(&ak(A_NOTE, n.id as usize));
        }
        self.hud.notes.retain(|n| t - n.at < life);
        let mut notes: Vec<(String, String, f32, u64, i64)> = self.hud.notes.iter().map(|n| (n.title.clone(), n.line.clone(), t - n.at, n.id, n.icon)).collect();
        if notes.is_empty() && x.sample {
            notes.push((tr("Notifications").into(), tr("A word on what happened").into(), 0.6, 0, -1));
        }
        if notes.is_empty() {
            return None;
        }
        let nh = 40.0 * s * x.look.pad.max(0.9);
        let gap = 6.0 * s;
        // a note with a thing (Auto Swap's) has it on its left, its words after it
        let lead = |icon: i64| if icon >= 0 { 26.0 * s } else { 0.0 };
        let w = notes.iter().map(|n| lead(n.4) + fonts().width(Face::Semi, 12.5 * s, &n.0).max(fonts().width(Face::Regular, 11.0 * s, &n.1))).fold(150.0 * s, f32::max) + 2.0 * p;
        // the newest by the edge the element is placed at (the bottom's, or the top's), the older pushed away from it;
        // each glides to its place as notes come and go
        let bottom = self.hud.places.get("notes").map_or(true, |pl| pl.0 / 3 == 2);
        let n = notes.len();
        let hgt = n as f32 * nh + (n - 1) as f32 * gap;
        let speed = if x.look.anim { 18.0 * x.look.speed } else { 1000.0 };
        let base = c.alpha;
        for (k, (title, line, age, id, icon)) in notes.iter().enumerate() {
            let at = (n - 1 - k) as f32 * (nh + gap);
            let dist = self.an(ak(A_NOTE, *id as usize), at, speed, at);
            let kin = if x.look.anim { ease_out((age / 0.28 * x.look.speed).min(1.0)) } else { 1.0 };
            // a new one also comes a little from beyond the edge, as the others make room
            let rise = (1.0 - kin) * 10.0 * s;
            let y = if bottom { M + hgt - nh - dist + rise } else { M + dist - rise };
            let kout = 1.0 - smooth(life - 0.3, life, *age);
            let dx = (1.0 - kin) * 36.0 * s;
            c.alpha = base * kin.min(kout);
            let r = [M + dx, y, M + w + dx, y + nh];
            x.part(c, r);
            let tx = r[0] + p + lead(*icon);
            if *icon >= 0 && x.icons {
                let mid = y + nh * 0.45 + 2.0 * s;
                self.hud.icons.push([10.0, *icon as f32, r[0] + p, mid - 9.0 * s, 18.0 * s, 18.0 * s, c.alpha]);
            }
            c.text(tx, y + nh * 0.45, 12.5 * s, Face::Semi, title, x.el.text);
            c.text(tx, y + nh * 0.45 + 13.5 * s, 11.0 * s, Face::Regular, line, x.el.text2);
            let left = (1.0 - age / life).clamp(0.0, 1.0);
            let ly = r[3] - 3.0 * s;
            c.rect([r[0] + p, ly - 0.6 * s, r[0] + p + (w - 2.0 * p) * left, ly + 0.6 * s], s, fade(x.el.accent, 0.7));
        }
        c.alpha = base;
        Some([w, hgt])
    }


    // ------------------------------------------------------------------------------------------ the player's own
    /// A text of the player's: its lines with their placeholders filled.
    fn el_ctext(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let p = x.pad();
        let mut lines: Vec<String> = ["Line 1", "Line 2", "Line 3", "Line 4"].iter().map(|l| self.fill(theme::text(&x.set, l))).filter(|l| !l.trim().is_empty()).collect();
        if lines.is_empty() {
            if !x.sample {
                return None;
            }
            lines.push(tr("Your words here").to_string());
        }
        let size = x.num("Size").max(4.0) * s;
        let face = super::hudx::face(x.pick("Face"));
        let lh = size * x.num("Line spacing").max(0.5);
        let ws: Vec<f32> = lines.iter().map(|l| fonts().width(face, size, l)).collect();
        let tw = ws.iter().copied().fold(0.0, f32::max);
        let w = tw + 2.0 * p;
        let hgt = 2.0 * p + size * 1.05 + lh * (lines.len() - 1) as f32;
        x.plate(c, [M, M, M + w, M + hgt]);
        let align = x.pick("Align");
        let dim = x.on("Other lines dimmer");
        for (k, l) in lines.iter().enumerate() {
            let base = M + p + size * 0.82 + lh * k as f32;
            let lx = match align {
                1 => M + (w - ws[k]) / 2.0,
                2 => M + w - p - ws[k],
                _ => M + p,
            };
            c.text(lx, base, size, face, l, if k > 0 && dim { x.el.text2 } else { x.el.text });
        }
        Some([w, hgt])
    }

    /// A card of the player's: its size, its fill, a line of light, words anywhere on it.
    fn el_card(&mut self, c: &mut Canvas, x: &Ctx) -> Option<[f32; 2]> {
        let s = x.s;
        let w = x.num("Width").max(8.0) * s;
        let h = x.num("Height").max(8.0) * s;
        let rad = x.num("Corner") * s;
        let r = [M, M, M + w, M + h];
        let at = (c.p.len() / super::draw::FLOATS, c.glass_rects.len());
        match x.pick("Fill") {
            0 => c.glass(r, rad, x.el.glass, x.look.blur, if x.look.shadow { 0.55 } else { 0.0 }, 0.0),
            1 => c.glass(r, rad, [x.el.glass[0], x.el.glass[1], x.el.glass[2], 0.0], x.look.blur.max(0.4), 0.0, 0.0),
            2 => c.interference(r, rad, x.t, 3.0, 0.9),
            3 => c.rect(r, rad, super::hudx::col_of(&x.set, "Fill colour", x.el.glass, x.t)),
            _ => {}
        }
        if c.p.len() / super::draw::FLOATS > at.0 {
            x.main.set(Some(at));
        }
        // (no clips in an element: it is drawn at one place and moved to its own, its clips would stay behind)
        if x.on("Line of light") {
            let yy = r[1] + rad * 0.5 + (r[3] - r[1] - rad) * ((x.t * 0.23).fract());
            c.line([r[0] + rad * 0.5, yy], [r[2] - rad * 0.5, yy], 1.0, fade(x.el.accent, 0.16), 6.0 * s);
        }
        let pad = 10.0 * s;
        for k in 1..=3 {
            let lbl = |what: &str| -> String { format!("Text {k}{what}") };
            let content = theme::text(&x.set, &lbl("")).to_string();
            let content = self.fill(&content);
            if content.trim().is_empty() {
                continue;
            }
            let size = x.num(&lbl(" size")).max(4.0) * s;
            let face = super::hudx::face(x.pick(&lbl(" face")));
            let col = super::hudx::col_of(&x.set, &lbl(" colour"), x.el.text, x.t);
            let tw = fonts().width(face, size, &content);
            let pos = x.pick(&lbl(" at"));
            let (cx, cy) = (pos % 3, pos / 3);
            let lx = match cx {
                0 => r[0] + pad,
                1 => (r[0] + r[2] - tw) / 2.0,
                _ => r[2] - pad - tw,
            } + x.num(&lbl(" x")) * s;
            let base = match cy {
                0 => r[1] + pad + size * 0.8,
                1 => (r[1] + r[3]) / 2.0 + size * 0.34,
                _ => r[3] - pad - size * 0.18,
            } + x.num(&lbl(" y")) * s;
            c.text(lx, base, size, face, &content, col);
        }
        Some([w, h])
    }

    /// A picture of the player's: Java draws it where this leaves room (the icons, kind 7: its number).
    fn el_image(&mut self, c: &mut Canvas, x: &Ctx, id: &str) -> Option<[f32; 2]> {
        let s = x.s;
        let file = theme::text(&x.set, "File").to_string();
        if file.trim().is_empty() && !x.sample {
            return None;
        }
        let (w, mut h) = (x.num("Width").max(4.0) * s, x.num("Height").max(4.0) * s);
        if x.on("Keep its shape") {
            if let Some(&(pw, ph)) = super::hudx::PIC_SIZES.lock().unwrap().get(file.trim()) {
                if pw > 0.0 && ph > 0.0 {
                    h = w * ph / pw;
                }
            }
        }
        let r = [M, M, M + w, M + h];
        if x.el.bg {
            x.plate(c, r);
        }
        if file.trim().is_empty() {
            c.rect(r, 8.0 * s, grey(1.0, 0.05));
            c.text_c(M + w / 2.0, M + h / 2.0 + 4.0 * s, 11.0 * s, Face::Medium, tr("Choose a picture"), x.el.text2);
            return Some([w.max(1.0), h.max(1.0)]);
        }
        let n: f32 = id.rsplit('-').next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        if x.icons {
            self.hud.icons.push([7.0, n, M, M, w, h, c.alpha]);
        } else {
            // the menu's preview: the picture's place, its name in it
            c.rect(r, 6.0 * s, grey(1.0, 0.06));
            let name = file.rsplit(['\\', '/']).next().unwrap_or("").to_string();
            let nm = fonts().fit(Face::Medium, 10.0 * s, &name, w - 8.0 * s);
            c.text_c(M + w / 2.0, M + h / 2.0 + 3.5 * s, 10.0 * s, Face::Medium, &nm, x.el.text2);
        }
        let _ = c;
        Some([w, h])
    }

    // ------------------------------------------------------------------------------------------ the settings by it
    fn hud_pop(&mut self, c: &mut Canvas) {
        let Some(i) = self.hud.pop else {
            for k in 0..3 {
                self.anim.remove(&ak(A_POPH, k));
            }
            self.hud.pop_pin = None;
            return;
        };
        let Some(&(_, er)) = self.hud.rects.iter().find(|q| q.0 == i) else { return };
        let r = self.r;
        let p = theme::pal();
        let k = self.an(ak(A_POPH, 0), 1.0, 14.0, 0.0);
        let (w, h) = (self.hud.w, self.hud.h);
        let pw = 25.0 * r;
        let set = self.mods[i].set.clone();
        // a long list (the chat's) has a search over it; the panel keeps the whole list's height while it is typed
        let find_h = if set.len() > 24 { 2.6 * r } else { 0.0 };
        let q = if find_h > 0.0 { self.hud.pop_q.trim().to_lowercase() } else { String::new() };
        let mask = (!q.is_empty()).then(|| rows_matching(&set, &q));
        let shown: Vec<Setting> = match &mask {
            Some(m) => set.iter().zip(m).filter(|(_, on)| **on).map(|(s, _)| s.clone()).collect(),
            None => set.clone(),
        };
        let body = self.rows_height(&shown, pw - 2.4 * r);
        let ph = (4.6 * r + find_h + self.rows_height(&set, pw - 2.4 * r) + 1.2 * r).min(h - 16.0);
        // by the element, where there is room; then it stays there while the element changes (a title typed, a row
        // more), and moves (gliding, a little farther than it must) only when the element would go under it
        let gap = 0.8 * r;
        let place = |slack: f32| -> [f32; 2] {
            let x = if er[2] + gap + pw < w - 8.0 { (er[2] + gap + slack).min(w - pw - 8.0) } else { (er[0] - gap - pw - slack).max(8.0) };
            [x.clamp(8.0, (w - pw - 8.0).max(8.0)), er[1].clamp(8.0, (h - ph - 8.0).max(8.0))]
        };
        let clear = |p: [f32; 2]| -> bool {
            let q = [p[0], p[1], p[0] + pw, p[1] + ph];
            let off = q[0] < 7.0 || q[2] > w - 7.0 || q[1] < 7.0 || q[3] > h - 7.0;
            let covers = q[0] < er[2] + gap * 0.5 && q[2] > er[0] - gap * 0.5 && q[1] < er[3] && q[3] > er[1];
            !off && !covers
        };
        // while one of its rows is held (a slider dragged, a title typed, a list or the colour wheel open) it never
        // moves: the element may grow under it (dragging its scale would otherwise move the slider from the pointer)
        let held = self.slider.is_some() || self.text_focus.is_some() || self.pick.is_some() || self.pop.is_some();
        let pin = match self.hud.pop_pin {
            Some((j, p)) if j == i && self.hud.drag.is_none() => {
                if clear(p) || held {
                    p
                } else {
                    place(3.0 * r)
                }
            }
            _ => place(0.0),
        };
        self.hud.pop_pin = Some((i, pin));
        let x = self.an(ak(A_POPH, 1), pin[0], 16.0, pin[0]);
        let y = self.an(ak(A_POPH, 2), pin[1], 16.0, pin[1]);
        let rr = [x, y + (1.0 - k) * 0.5 * r, x + pw, y + ph + (1.0 - k) * 0.5 * r];
        self.hud.pop_rect = rr;
        c.alpha = k;
        c.glass(rr, 1.1 * r, [p.glass[0], p.glass[1], p.glass[2], p.glass[3].max(0.82)], p.blur, 1.0, if p.sheen { 0.6 } else { 0.0 });
        self.hits.push((rr, Hit::Panel(i)));
        let (x0, x1) = (rr[0] + 1.2 * r, rr[2] - 1.2 * r);
        // the header: its name, its switch, close
        let m = self.head(i);
        let xb = [x1 - 1.8 * r, rr[1] + 1.0 * r, x1, rr[1] + 2.8 * r];
        let hxv = self.an(ak(super::A_HOV, 2100), if self.hover == Hit::PanelClose { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(xb, 0.55 * r, mix(p.f1, p.f3, hxv));
        let xc = [(xb[0] + xb[2]) / 2.0, (xb[1] + xb[3]) / 2.0];
        let xs = 0.28 * r;
        c.line([xc[0] - xs, xc[1] - xs], [xc[0] + xs, xc[1] + xs], 1.5, mix(p.t3, p.t1, hxv), 0.0);
        c.line([xc[0] + xs, xc[1] - xs], [xc[0] - xs, xc[1] + xs], 1.5, mix(p.t3, p.t1, hxv), 0.0);
        self.hits.push((xb, Hit::PanelClose));
        let onv = self.an(ak(super::A_SW, i), if m.on { 1.0 } else { 0.0 }, 10.0, 0.0);
        let sw = [xb[0] - 0.6 * r - 2.3 * r, rr[1] + 1.22 * r, xb[0] - 0.6 * r, rr[1] + 2.58 * r];
        let hv = self.an(ak(super::A_HOV, 2101), if self.hover == Hit::PanelSwitch(i) { 1.0 } else { 0.0 }, 12.0, 0.0);
        switch(c, sw, onv, hv, false);
        self.hits.push((sw, Hit::PanelSwitch(i)));
        let name = fonts().fit(Face::Bold, 1.2 * r, m.name, sw[0] - x0 - 0.6 * r);
        c.text(x0, rr[1] + 2.35 * r, 1.2 * r, Face::Bold, &name, p.t1);
        // the search
        if find_h > 0.0 {
            let fr = [x0, rr[1] + 3.6 * r, x1, rr[1] + 5.6 * r];
            let on = self.hud.pop_find;
            let fv = self.an(ak(super::A_HOV, 2102), if on || self.hover == Hit::PopFind { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect(fr, 0.6 * r, mix(p.f1, p.f2, fv));
            let s = 0.8 * r;
            let mid = (fr[1] + fr[3]) / 2.0;
            c.icon([fr[0] + 0.7 * r, mid - s / 2.0, fr[0] + 0.7 * r + s, mid + s / 2.0], 13, p.t3);
            let tx = fr[0] + 2.0 * r;
            let typed = self.hud.pop_q.clone();
            let w = if typed.is_empty() {
                c.text(tx, mid + 0.3 * r, 0.8 * r, Face::Regular, tr("Search the settings"), p.t4);
                0.0
            } else {
                c.text(tx, mid + 0.3 * r, 0.8 * r, Face::Regular, &fonts().fit(Face::Regular, 0.8 * r, &typed, fr[2] - tx - 0.6 * r), p.t1)
            };
            if on && (self.now * 1.9).fract() < 0.55 {
                c.rect([tx + w + 0.06 * r, mid - 0.5 * r, tx + w + 0.06 * r + 1.5, mid + 0.5 * r], 0.5, p.t1);
            }
            self.hits.push((fr, Hit::PopFind));
            if mask.as_ref().is_some_and(|m| !m.contains(&true)) {
                c.text(x0, fr[3] + 1.6 * r, 0.8 * r, Face::Regular, tr("Nothing found"), p.t3);
            }
        }
        // the rows, scrolled within it
        self.hud.pop_scroll += (self.hud.pop_to - self.hud.pop_scroll) * (1.0 - (-self.dt * 14.0).exp());
        let top = rr[1] + 3.6 * r + find_h;
        let clip = [rr[0], top, rr[2], rr[3] - 0.4 * r];
        let max = (body + 1.2 * r - (clip[3] - clip[1])).max(0.0);
        self.hud.pop_to = self.hud.pop_to.clamp(0.0, max);
        let h0 = self.hits.len();
        c.push_clip(clip);
        let opened = self.now - self.hud.pop_at;
        self.rows_mask = mask;
        self.rows(c, Owner::Module(i), x0, x1, top - self.hud.pop_scroll, |d| {
            let kk = ((opened - d * 0.5) / 0.3).clamp(0.0, 1.0);
            (smooth(0.0, 1.0, kk), (1.0 - ease_out(kk)) * 0.25 * r)
        });
        self.rows_mask = None;
        c.pop_clip();
        // the rows' hits only where they are seen
        let mut kept = Vec::new();
        for (q, hit) in self.hits.drain(h0..) {
            let cq = [q[0].max(clip[0]), q[1].max(clip[1]), q[2].min(clip[2]), q[3].min(clip[3])];
            if cq[2] > cq[0] && cq[3] > cq[1] {
                kept.push((cq, hit));
            }
        }
        self.hits.extend(kept);
        c.alpha = 1.0;
    }

    // ------------------------------------------------------------------------------------------ input (the chat, the editor)
    fn hud_move_in(&mut self, x: f32, y: f32) {
        self.mouse = [x, y];
        if self.swing_ed.on {
            self.swing_move(x, y);
            return;
        }
        if let Some((i, m0, wh0)) = self.hud.resize {
            let k = self.hud.u * (theme::num(&self.mods[i].set, "Scale") / 100.0).max(0.1);
            let (nw, nh) = (wh0[0] + (x - m0[0]) / k, wh0[1] + (y - m0[1]) / k);
            for s in self.mods[i].set.iter_mut() {
                let want = match s.label {
                    "Width" => nw,
                    "Height" => nh,
                    _ => continue,
                };
                if let Kind::Slider { min, max, step, v, .. } = &mut s.kind {
                    *v = ((want / *step).round() * *step).clamp(*min, *max);
                }
            }
            self.hud.dirty = true;
        }
        if let Some((o, si)) = self.slider {
            self.slide(o, si, x);
        }
        if let Some(part) = self.pick.as_ref().map(|p| p.drag).filter(|d| *d > 0) {
            self.pick_at(part, x, y);
        }
        self.hover = self.hit_at(self.mouse);
    }

    /// A press in the chat or the editor; true when it was the HUD's.
    fn hud_press_in(&mut self, x: f32, y: f32, button: i32) -> bool {
        self.hud_move_in(x, y);
        if self.swing_ed.on {
            return self.swing_press(x, y, button);
        }
        self.card_shut = None;
        let h = self.hit_at([x, y]);
        if h != Hit::PopFind {
            self.hud.pop_find = false;
        }
        let had_pop = self.hud.pop.is_some() || self.pick.is_some() || self.card.is_some();
        // (a list or the colour wheel open from the settings: a press past it closes only it)
        let had_list = self.pick.is_some() || self.pop.is_some();
        if self.card.is_some() && self.card_press(h, button) {
            return true;
        }
        if self.press_common(h, x, y, button) {
            return true;
        }
        let owner = self.hud.pop.map(Owner::Module);
        if let Some(o) = owner {
            if self.press_rows(h, x, y, o, button) {
                self.hud.dirty = true;
                return true;
            }
        }
        match (button, h) {
            (1, Hit::HudElem(i)) => {
                self.open_pop(i);
                true
            }
            (0, Hit::HudElem(i)) => {
                let r = self.hud.rects.iter().find(|q| q.0 == i).map_or([x, y, x, y], |q| q.1);
                // a card or a picture by its corner: stretched
                let corner = 14.0 * self.hud.u.max(0.7);
                if self.hud_resizable(i) && x > r[2] - corner && y > r[3] - corner {
                    let set = &self.mods[i].set;
                    self.hud.resize = Some((i, [x, y], [theme::num(set, "Width"), theme::num(set, "Height")]));
                    return true;
                }
                self.hud.drag = Some((i, [x - r[0], y - r[1]]));
                true
            }
            (0, Hit::PanelSwitch(i)) => {
                self.toggle(i);
                self.hud.dirty = true;
                true
            }
            (0, Hit::PanelClose) => {
                self.hud.pop = None;
                true
            }
            (0, Hit::PopFind) => {
                self.hud.pop_find = true;
                true
            }
            (_, Hit::Panel(_)) => {
                self.hud.pop_find = false;
                true
            }
            _ => {
                // a press past it closes the settings (a list open from them: only it); in the chat the press is the
                // chat's then
                if had_list {
                    self.pick = None;
                    self.pop = None;
                    return true;
                }
                if self.hud.pop.take().is_some() || had_pop {
                    return self.hud.editing;
                }
                self.hud.editing
            }
        }
    }

    fn hud_release_in(&mut self) -> bool {
        if self.swing_ed.on {
            self.swing_release();
            return true;
        }
        let was = self.hud.drag.take().is_some() || self.hud.resize.take().is_some() || self.slider.is_some() || self.pick.as_ref().is_some_and(|p| p.drag > 0);
        self.release();
        self.hud.guides.clear();
        if was {
            self.hud.dirty = true;
        }
        was
    }

    /// An element's settings opened by it (closed again when they were open).
    pub(super) fn open_pop(&mut self, i: usize) {
        if self.hud.pop == Some(i) {
            self.hud.pop = None;
            return;
        }
        self.hud.pop = Some(i);
        self.hud.pop_at = self.now;
        self.hud.pop_to = 0.0;
        self.hud.pop_scroll = 0.0;
        self.hud.pop_pin = None;
        self.hud.pop_q.clear();
        self.hud.pop_find = false;
        for k in 0..3 {
            self.anim.remove(&ak(A_POPH, k));
        }
        self.sound_tick(0.7);
    }

    /// The chat or the editor closed: what was open by an element goes, what changed is kept.
    pub(super) fn hud_closed(&mut self) {
        self.swing_finish();
        self.hud.pop = None;
        self.hud.drag = None;
        self.eat = None;
        if self.phase == Phase::Closed {
            self.pick = None;
            self.pop = None;
            self.text_focus = None;
            self.slider = None;
        }
        self.save();
    }
}

// ---------------------------------------------------------------------------------------------- the game's side
impl Gui {
    pub(super) fn hud_mouse(&mut self, x: f32, y: f32, button: i32, action: i32) -> bool {
        if !self.hud_input() {
            return false;
        }
        match action {
            1 => self.undo_poke(Some(true)),
            2 => self.undo_poke(Some(false)),
            _ => {}
        }
        // the client's own chat takes what is pressed on it
        if self.hud.facts.chat && !self.hud.facts.editor {
            match action {
                1 if self.chat_press(x, y, button) => return true,
                2 => self.chat_release(),
                0 => self.chat_drag(x, y),
                _ => {}
            }
        }
        self.in_hud(|g| match action {
            1 => g.hud_press_in(x, y, button),
            2 => g.hud_release_in(),
            _ => {
                g.hud_move_in(x, y);
                false
            }
        })
    }

    pub(super) fn hud_scroll(&mut self, dy: f32) -> bool {
        self.undo_poke(None);
        // an inventory's settings under the pointer: they scroll
        if self.iu_scroll(dy) {
            return true;
        }
        let over_settings = self.hud.pop.is_some() && inside(self.hud.pop_rect, self.hud.mouse);
        if self.hud_input() && self.hud.facts.chat && !self.hud.facts.editor && !over_settings && self.chat_scroll(dy) {
            return true;
        }
        if !self.hud_input() || self.hud.pop.is_none() || !inside(self.hud.pop_rect, self.hud.mouse) {
            return false;
        }
        // the wheel scrolls the settings, it never changes the one under the pointer
        self.in_hud(|g| {
            g.hud.pop_to -= dy * 3.2 * g.r;
            true
        })
    }

    /// A key in the chat or the editor: true when it was the HUD's (the hex being typed, a text field, Esc closing
    /// what is open by an element).
    pub(super) fn hud_key(&mut self, escape: bool, enter: bool, backspace: bool) -> bool {
        self.undo_poke(None);
        // an inventory's settings: the colour wheel's hex being typed
        if self.iu_keys() {
            self.key(escape, enter, backspace, String::new(), false);
            self.hud.inv.dirty = true;
            return true;
        }
        if !self.hud_input() {
            return false;
        }
        if self.pick.as_ref().is_some_and(|p| p.hex.is_some()) {
            self.key(escape, enter, backspace, String::new(), false);
            return true;
        }
        if let Some((o, si)) = self.text_focus {
            if backspace {
                if let Kind::Text(v) = &mut self.set_list(o)[si].kind {
                    v.pop();
                }
            } else if escape || enter {
                self.text_focus = None;
            }
            self.hud.dirty = true;
            return true;
        }
        if self.hud.pop_find && self.hud.pop.is_some() {
            if backspace {
                self.hud.pop_q.pop();
                self.hud.pop_to = 0.0;
            } else if escape && !self.hud.pop_q.is_empty() {
                self.hud.pop_q.clear();
                self.hud.pop_to = 0.0;
            } else if escape || enter {
                self.hud.pop_find = false;
            }
            return true;
        }
        if escape {
            if self.pick.take().is_some() || self.pop.take().is_some() || self.hud.pop.take().is_some() {
                return true;
            }
            // the client's own chat: its player's card or settings first
            if chat::escape() {
                return true;
            }
        }
        false
    }

    pub(super) fn hud_char(&mut self, ch: char) -> bool {
        // the letter of a key a bind being recorded took: typed nowhere (not in the chat's field either)
        if self.eat.is_some() {
            return true;
        }
        if self.iu_keys() {
            self.char_typed(ch);
            self.hud.inv.dirty = true;
            return true;
        }
        if !self.hud_input() {
            return false;
        }
        if self.pick.as_ref().is_some_and(|p| p.hex.is_some()) || self.text_focus.is_some() {
            self.char_typed(ch);
            self.hud.dirty = true;
            return true;
        }
        if self.hud.pop_find && self.hud.pop.is_some() {
            if !ch.is_control() && self.hud.pop_q.chars().count() < 40 {
                self.hud.pop_q.push(ch);
                self.hud.pop_to = 0.0;
            }
            return true;
        }
        false
    }

    /// A key in the world with no screen (the old way in: pressed only): the binds on it go off.
    pub(super) fn hud_bind(&mut self, name: &str) -> bool {
        let fired = self.world_key(name, true);
        self.world_key(name, false);
        fired
    }

    /// Something the game saw: 1 a totem popped (whose), 2 the target was hit.
    pub(super) fn hud_event(&mut self, kind: i32, who: &str) {
        let t = self.clock.elapsed().as_secs_f32();
        match kind {
            1 => {
                let totems = self.mods[self.mod_index("notes")].set.iter().any(|s| s.label == "Totems" && s.kind == Kind::Toggle(true));
                if let Some(s) = self.hud.target.as_mut().filter(|s| s.t.name == who) {
                    s.totems += 1;
                    s.hit_at = t;
                }
                if totems {
                    let mine = who == self.player;
                    self.hud.note(if mine { tr("Your totem").into() } else { who.to_string() }, tr(if mine { "It saved you" } else { "Used a totem" }).into(), t);
                }
            }
            2 => {
                if let Some(s) = self.hud.target.as_mut() {
                    s.hit_at = t;
                }
            }
            // a message copied from the Custom Chat
            3 => self.hud.chat_copied_at = t,
            _ => {}
        }
    }
}

impl Gui {
    /// Whether an element is stretched by its corner (the player's cards and pictures).
    pub(super) fn hud_resizable(&self, i: usize) -> bool {
        self.mods.get(i).is_some_and(|m| m.id.starts_with("x-card") || m.id.starts_with("x-image"))
    }
}

/// CS's crosshair while a CS knife is in the hands (cosmetics/crosshair.rs's bars, crisp on whole pixels): true when it
/// was drawn (the game's own is not then).
fn cs_crosshair(c: &mut Canvas, w: f32, h: f32) -> bool {
    let Some((x, spread)) = crate::cosmetics::view::crosshair() else { return false };
    let alpha = c.alpha;
    c.alpha = 1.0;
    for (r, col) in crate::cosmetics::crosshair::bars(&x, [w / 2.0, h / 2.0], h, spread) {
        c.rect(r, 0.0, col.map(|v| v as f32 / 255.0));
    }
    c.alpha = alpha;
    true
}
