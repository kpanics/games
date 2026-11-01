//! Advancements as constellations. The page is a look up at a night sky: out of a world the valley's camera tilts up
//! into its sky; in a world the game's own camera rises to the sky (cursed.recode.mixin.client.MenusCameraMixin) and the
//! frosted world goes dark under the page. Tiny stars twinkle and drift behind as the sky is moved, a faint milky way
//! and a nebula the colour of the tab's world (the Nether's ember, the End's violet) lie behind them, a shooting star
//! now and then. The game's tabs are words on the left (their icon, how many are made, a hair of light for how far, how
//! many are new since the last look); the chosen tab hangs in the sky as a constellation, its name in wide capitals
//! under its first star. Another tab swings the sky round: the old constellation is carried off to one side, the sky's
//! stars streak past, the new one comes in from the other side and forms.
//!
//! Every advancement is a star: a small square for a task (hollow when it can be made next), a diamond for a goal, a
//! ring with a corona for a challenge. What is made burns white (a challenge with fire), sparkles with a cross of light
//! and sends light along the lines out of it; what lies further is barely there and joined to the rest by a dotted
//! trace. An advancement of many parts (the biomes, the mobs) has a ring of ticks round its star, one lit for each part
//! done. The constellation forms outward from its root: each star pops in as the line from the one before reaches it.
//! Over each star its item, a real model (the menus' item stage) turning as an item dropped in the world: the made ones
//! spin, the next ones sway, the far ones hold still and dim; the one under the pointer turns to face you, and the way
//! from the root to it lights with a spark running along. What was made since the last look breathes until it is looked
//! at; what is made while the page is up bursts like a new star. The sky is dragged (it glides on, and gives at its
//! edges), the wheel comes closer round the pointer. The star under the pointer tells about itself at the bottom.
//!
//! A click on a star flies to it: the sky comes closer and carries it to the left, the others dim, its item grows and
//! turns slowly, and beside it is all of it: what it is, its name, what it asks, when it was made or how far it is, and
//! for one of many parts each part by name, what is left first. A click on the sky, Esc or the right button flies back.

use super::controls::{hid, hid_kind, hid_n, hid_sub, k};
use super::lang::{ru, tr};
use super::ui::{clamp01, ease, ease_io, ease_out, smooth, toward, tw, WordStyle, CAP, DIM};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, Canvas, Rgba};
use crate::gui::text::{fonts, Face};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::f32::consts::{PI, TAU};

const CHALLENGE: i64 = 1;
const GOAL: i64 = 2;
const STAR: u32 = 70;
const TABS: u32 = 1;
const SKY: u32 = 701;

/// The fire of the client (a made challenge burns with it).
const FIRE: Rgba = [1.0, 0.62, 0.32, 1.0];

/// A star of a constellation that is going (the tab was changed): where it stood on the screen and how lit.
#[derive(Clone)]
struct Gone {
    at: [f32; 2],
    parent: Option<[f32; 2]>,
    kind: i64,
    lit: f32,
    size: f32,
}

pub struct Adv {
    tab: i64,
    /// When the shown constellation began to form.
    born: f32,
    /// The view: the drag's offset (px), its glide, how close; what the sky's own stars follow (a little behind).
    pan: [f32; 2],
    vel: [f32; 2],
    zoom: f32,
    zoom_to: f32,
    sky_pan: [f32; 2],
    drag: Option<[f32; 2]>,
    last_mouse: [f32; 2],
    /// Where the wheel brought it closer (what stays under the pointer as it zooms); the sky's middle.
    anchor: [f32; 2],
    mid: [f32; 2],
    /// How far the constellation reaches past its part of the sky each way (px): the drag gives beyond it.
    range: [f32; 2],
    hovered: Option<usize>,
    hover_since: f32,
    hover_k: f32,
    /// The new ones that were looked at (they stop breathing); when each was made while the page was up.
    seen: HashSet<usize>,
    made: HashMap<usize, f32>,
    done_before: Vec<bool>,
    /// The constellation of the tab before, carried off where it went.
    leaving: Vec<Gone>,
    left_at: f32,
    last_drawn: Vec<Gone>,
    /// A shooting star: when it set out, from where, which way.
    shooting: Option<(f32, [f32; 2], f32)>,
    next_shot: f32,
    /// How far round each star's item is (degrees).
    yaw: Vec<f32>,
    /// The sky swinging to another tab: when, which way (1 the next to the right); how far its own stars have turned
    /// with all the swings (px), how far this one has carried them, how fast they go now (px/s, for the streaks).
    swing_at: f32,
    swing_dir: f32,
    sky_turn: f32,
    swing_done: f32,
    sky_speed: f32,
    /// The nebula's colour, gliding to the tab's.
    tint: [f32; 3],
    /// The star flown to (a click on it), since when, how far there; the one before (to see a change).
    focus: Option<usize>,
    focus_at: f32,
    focus_k: f32,
    focus_was: Option<usize>,
    /// The last star flown to: its card fades out after it.
    focus_shown: Option<usize>,
    /// The card of the star flown to before another was: which, since when (it sinks).
    card_gone: Option<(usize, f32)>,
    /// A press on the sky or a star: where, on which star (a click if it does not move).
    press: Option<([f32; 2], Option<usize>)>,
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn n(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(-1)
}
fn f(v: &Value, key: &str) -> f32 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32
}
fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// A number of its own for `i` and `salt` (0..1).
fn rnd(i: u32, salt: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h % 100_000) as f32 / 100_000.0
}

/// `text` coming out of noise: the first `k` of it (0..1) as it is, the rest scrambled (it changes 20 times a second).
fn decode(text: &str, k: f32, now: f32, salt: u32) -> String {
    if k >= 1.0 {
        return text.to_string();
    }
    let noise: Vec<char> = if ru() { "АБВГДЕЖЗИКЛМНОПРСТУФХЦЧШЭЯ0123456789".chars().collect() } else { "ABCDEFGHJKLMNPQRSTUVWXYZ0123456789".chars().collect() };
    let count = text.chars().count();
    let shown = (k * count as f32) as usize;
    let tick = (now * 20.0) as u32;
    text.chars()
        .enumerate()
        .map(|(i, ch)| if i < shown || ch == ' ' { ch } else { noise[(rnd(i as u32 ^ tick, salt) * noise.len() as f32) as usize % noise.len()] })
        .collect()
}

/// The colour of a tab's sky by its root: the Overworld's cold blue, the Nether's ember, the End's violet, the
/// adventure's sea, husbandry's green; another's a pale grey-blue.
fn tint_of(id: &str) -> [f32; 3] {
    match id {
        "minecraft:story/root" => [0.5, 0.66, 1.0],
        "minecraft:nether/root" => [1.0, 0.4, 0.24],
        "minecraft:end/root" => [0.74, 0.52, 1.0],
        "minecraft:adventure/root" => [0.4, 0.86, 0.95],
        "minecraft:husbandry/root" => [0.5, 0.95, 0.55],
        _ => [0.66, 0.72, 0.9],
    }
}

/// The nearest whole turn to `y` (degrees).
fn front(y: f32) -> f32 {
    (y / 360.0).round() * 360.0
}

impl Adv {
    pub fn new(data: &Value) -> Adv {
        Adv {
            tab: n(data, "sel"),
            born: f32::NAN,
            pan: [0.0; 2],
            vel: [0.0; 2],
            zoom: 1.0,
            zoom_to: 1.0,
            sky_pan: [0.0; 2],
            drag: None,
            last_mouse: [0.0; 2],
            anchor: [0.0; 2],
            mid: [0.0; 2],
            range: [0.0; 2],
            hovered: None,
            hover_since: 0.0,
            hover_k: 0.0,
            seen: HashSet::new(),
            made: HashMap::new(),
            done_before: Vec::new(),
            leaving: Vec::new(),
            left_at: -10.0,
            last_drawn: Vec::new(),
            shooting: None,
            next_shot: 0.0,
            yaw: Vec::new(),
            swing_at: -10.0,
            swing_dir: 1.0,
            sky_turn: 0.0,
            swing_done: 0.0,
            sky_speed: 0.0,
            tint: [0.66, 0.72, 0.9],
            focus: None,
            focus_at: 0.0,
            focus_k: 0.0,
            focus_was: None,
            focus_shown: None,
            card_gone: None,
            press: None,
        }
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        let vh = m.vh();
        let vw = w / 100.0;
        let narrow = w < 900.0 * ui;
        let left = if narrow { 20.0 * ui } else { 6.2 * vw };
        let right = w - if narrow { 20.0 * ui } else { 4.0 * vw };
        let live = v.leaving.is_none() && m.now - v.opened > 0.25;
        let now = m.now;
        let dt = m.dt;
        let shade = m.vis(v, 0.0);
        let empty = Vec::new();
        let tabs = v.data.get("tabs").and_then(Value::as_array).unwrap_or(&empty);
        let nodes = v.data.get("nodes").and_then(Value::as_array).unwrap_or(&empty);
        let sel = n(v.data, "sel");
        let tab_id = tabs.get(sel.max(0) as usize).map_or("", |t| s(t, "id"));
        if self.born.is_nan() {
            // the camera rises first; the constellation forms as the sky comes into view
            self.born = v.opened + if m.in_world { 0.7 } else { 0.45 };
            self.next_shot = now + 2.5;
            self.tint = tint_of(tab_id);
        }
        // another tab: the sky swings round, the old constellation is carried off, the new one comes in and forms
        if sel != self.tab {
            let count_t = tabs.len() as i64;
            let mut dir = if sel > self.tab { 1.0 } else { -1.0 };
            if count_t > 2 && (sel - self.tab).abs() == count_t - 1 {
                // round from the last to the first (Tab): still onward
                dir = -dir;
            }
            self.tab = sel;
            self.leaving = std::mem::take(&mut self.last_drawn);
            self.left_at = now;
            self.swing_at = now;
            self.swing_dir = dir;
            self.swing_done = 0.0;
            self.born = now + 0.3;
            self.pan = [0.0; 2];
            self.vel = [0.0; 2];
            self.zoom_to = 1.0;
            self.hovered = None;
            self.focus = None;
            self.focus_shown = None;
            self.seen.clear();
            self.made.clear();
            self.done_before.clear();
            self.yaw.clear();
            m.tick_sound(0.9);
        }
        let to = tint_of(tab_id);
        for i in 0..3 {
            self.tint[i] = toward(self.tint[i], to[i], 2.5, dt);
        }

        // ---- the sky: out of a world the camera looks up into the valley's; in a world the game's camera rises
        if !m.in_world {
            m.scene_pose([8.0, 20.5, 0.42, 0.03, m.scene_to[4]], 0.55, 0.0);
        }
        // in a world: the camera rises through the day's sky, then night falls over it
        let night = if m.in_world { 0.32 + 0.54 * smooth(0.3, 1.6, now - v.opened) } else { 0.42 };
        c.veil([0.0, 0.0, w, h], [0.005, 0.006, 0.014, night * shade], 1.0, 0.0);
        // deeper toward the ground (what of it is still seen as the camera rises) and behind the words on the left
        c.veil([0.0, h * 0.45, w, h], grey(0.0, 0.35 * shade), 30.0 * vh, 3.0);
        c.veil([0.0, 0.0, w * 0.45, h], grey(0.0, 0.35 * shade), 30.0 * vh, 1.0);
        // the sky follows the drag, a little behind
        let follow = 1.0 - (-dt * 5.0).exp();
        self.sky_pan = [self.sky_pan[0] + (self.pan[0] - self.sky_pan[0]) * follow, self.sky_pan[1] + (self.pan[1] - self.sky_pan[1]) * follow];
        // the swing carries the sky's own stars half a screen across
        let sk = clamp01((now - self.swing_at) / 0.95);
        let carried = -self.swing_dir * w * 0.5 * ease_io(sk);
        let step = carried - self.swing_done;
        self.swing_done = carried;
        self.sky_turn += step;
        self.sky_speed = if dt > 0.0 { step / dt } else { 0.0 };
        let stars_k = ease(clamp01((now - v.opened - 0.1) / 0.9)) * shade;
        // looking up: the sky rises into view as the page opens (the near stars more), sinks as it goes
        let look = (1.0 - ease_io(clamp01((now - v.opened) / 1.2))) * 26.0 * vh + (1.0 - shade) * 10.0 * vh;
        // the nebula: great soft clouds the colour of the tab's world, drifting slowly
        let tint = self.tint;
        for i in 0..6u32 {
            let bx = (w * (0.2 + 0.75 * rnd(i, 31)) + self.sky_pan[0] * 0.04 + self.sky_turn * 0.2 + (now * 0.01 + rnd(i, 32) * 6.0).sin() * 3.0 * vw).rem_euclid(w * 1.6) - w * 0.3;
            let by = h * (0.1 + 0.6 * rnd(i, 33)) + self.sky_pan[1] * 0.04 + look * 0.3;
            let br = (26.0 + 22.0 * rnd(i, 34)) * vh;
            let a = (0.035 + 0.035 * rnd(i, 35)) * stars_k;
            c.glow([bx, by], br, [tint[0], tint[1], tint[2], a]);
        }
        // the milky way: a faint band of light across, its clouds drifting with the sky
        for i in 0..14u32 {
            let t_ = i as f32 / 13.0;
            let bx = (w * (0.18 + 0.9 * t_) + self.sky_pan[0] * 0.06 + self.sky_turn * 0.3).rem_euclid(w * 1.5) - w * 0.25;
            let by = h * (0.95 - 0.85 * t_) + (rnd(i, 21) - 0.5) * 12.0 * vh + self.sky_pan[1] * 0.06 + look * 0.5;
            let br = (14.0 + 16.0 * rnd(i, 22)) * vh;
            c.glow([bx, by], br, [0.75, 0.8, 1.0, (0.022 + 0.02 * rnd(i, 23)) * stars_k]);
        }
        for i in 0..240u32 {
            let depth = 0.15 + rnd(i, 3) * 0.85;
            let x = (rnd(i, 1) * w * 1.2 + self.sky_pan[0] * 0.12 * depth + self.sky_turn * depth).rem_euclid(w * 1.2) - w * 0.1;
            let y = (rnd(i, 2) * h * 1.2 + self.sky_pan[1] * 0.12 * depth).rem_euclid(h * 1.2) - h * 0.1 + look * (0.4 + 0.6 * depth);
            let tw_ = 0.55 + 0.45 * (now * (0.6 + rnd(i, 4) * 1.8) + rnd(i, 5) * 6.28).sin();
            let born = clamp01((stars_k * 1.6) - rnd(i, 6) * 0.6);
            let sz = (0.5 + 1.3 * depth * depth) * ui;
            let a = (0.08 + 0.42 * depth * tw_) * born;
            if a > 0.01 {
                // as the sky swings past, the stars streak
                let streak = self.sky_speed * depth * 0.03;
                if streak.abs() > sz * 2.0 {
                    c.line([x - streak, y], [x, y], sz * 1.4, grey(1.0, a * 0.8), 0.0);
                } else {
                    c.rect([x - sz, y - sz, x + sz, y + sz], sz, grey(1.0, a));
                }
                if depth > 0.93 {
                    c.glow([x, y], sz * 7.0, grey(1.0, 0.05 * a * tw_));
                }
            }
        }
        // a shooting star now and then
        if now > self.next_shot && self.shooting.is_none() {
            let r = rnd((now * 10.0) as u32, 9);
            self.shooting = Some((now, [w * (0.35 + 0.6 * r), h * (0.05 + 0.3 * rnd((now * 7.0) as u32, 11))], 2.6 + 0.4 * r));
            self.next_shot = now + 5.0 + 7.0 * rnd((now * 3.0) as u32, 12);
        }
        if let Some((at, from, ang)) = self.shooting {
            let k_ = (now - at) / 0.8;
            if k_ >= 1.0 {
                self.shooting = None;
            } else {
                let len = 22.0 * vh;
                let head = ease_out(0.0, 1.0, k_) * len;
                let tail = (head - 9.0 * vh).max(0.0);
                let dir = [ang.cos(), ang.sin()];
                let a = (1.0 - smooth(0.6, 1.0, k_)) * stars_k;
                c.line([from[0] + dir[0] * tail, from[1] + dir[1] * tail], [from[0] + dir[0] * head, from[1] + dir[1] * head], 1.2 * ui, grey(1.0, 0.55 * a), 4.0 * ui);
            }
        }

        m.title(c, v, &tr("adv").to_uppercase(), left, 6.0 * vh, m.cl(34.0, 120.0, 7.6));
        let cap = m.cl(9.0, 18.0, 1.15);

        // ---- the tabs, down the left: their item, name, how many made, how far, how many new
        let tsize = m.cl(16.0, 42.0, 2.7);
        let icon_t = tsize * 1.05;
        let col_w = if narrow { w - 2.0 * left } else { 21.0 * vw };
        let mut ty = 18.0 * vh;
        let done_all: (i64, i64) = tabs.iter().fold((0, 0), |a, t| (a.0 + n(t, "done").max(0), a.1 + n(t, "all").max(0)));
        for (i, t) in tabs.iter().enumerate() {
            let p = m.vis(v, 1.0 + i as f32 * 0.35);
            let id = hid(k::TAB, TABS, i as u32);
            let chosen = i as i64 == sel;
            let kk = m.an(id ^ 0xc405e, if chosen { 1.0 } else { 0.0 }, 6.0);
            m.extra(1, n(t, "icon") as f32, [left, ty + (1.12 * tsize - icon_t) / 2.0, icon_t], ease(p) * (0.55 + 0.45 * kk));
            let wx = left + icon_t + 1.2 * vh + kk * 0.8 * vh;
            let st = WordStyle { glow: 0.0, ..WordStyle::new(Face::Light, tsize).rest(if chosen { 1.0 } else { DIM }) };
            let label = s(t, "title").to_uppercase();
            let label = fonts().fit_ls(Face::Light, tsize, &label, col_w - icon_t - 7.0 * vh, 0.02);
            let r = m.word(c, id, &label, wx, ty, st, p, live, false);
            if live {
                m.hit([left, ty, left + col_w, r[3] + 1.2 * vh], id);
            }
            let (dn, all) = (n(t, "done").max(0), n(t, "all").max(1));
            let count = format!("{dn}/{all}");
            let cw = tw(Face::Caps, &count, cap, 0.1);
            m.caps(c, &count, left + col_w - cw, ty + 1.12 * tsize - cap * 1.6, cap, 0.1, if chosen { 0.75 } else { 0.4 }, clamp01(ease(p) * 1.15));
            // how far: a hair of light under it
            let ly = ty + 1.12 * tsize + 0.55 * vh;
            let part = dn as f32 / all as f32;
            c.line([wx, ly], [left + col_w, ly], 1.0, grey(1.0, 0.07 * ease(p)), 0.0);
            c.light(wx, ly, left + col_w, (0.45 + 0.5 * kk) * ease(p), 0.0, part * ease_out(0.2, 1.0, p), 0.5, ui);
            let fresh = n(t, "new");
            if fresh > 0 {
                let word = format!("+{fresh}");
                let pulse = 0.6 + 0.4 * (now * 3.0).sin();
                m.rising_text(c, Face::Caps, &word, left + col_w - cw - tw(Face::Caps, &word, cap, 0.1) - 1.0 * vh, ty + 1.12 * tsize - cap * 1.6, cap, 1.3, 0.1, [FIRE[0], FIRE[1], FIRE[2], pulse * ease(p)], 0.4, 1.0);
            }
            ty += 1.12 * tsize + 2.6 * vh;
        }
        let pc = m.vis(v, 1.0);
        let all_text = format!("{} {} / {}", tr("obtained").to_uppercase(), done_all.0, done_all.1);
        let atw = tw(Face::Caps, &all_text, cap, 0.22);
        m.caps(c, &all_text, right - atw, 7.6 * vh, cap, 0.22, CAP, clamp01(ease(pc) * 1.15));

        // ---- the constellation
        let sky = [if narrow { left } else { left + col_w + 6.0 * vw }, 14.0 * vh, right, h - 23.0 * vh];
        let count = nodes.len();
        // depth of each (its parents up to the root), for the order it forms in
        let mut depth = vec![0usize; count];
        for i in 0..count {
            let mut d = 0;
            let mut p = n(&nodes[i], "parent");
            while p >= 0 && (p as usize) < count && d < 64 {
                d += 1;
                p = n(&nodes[p as usize], "parent");
            }
            depth[i] = d;
        }
        let deepest = depth.iter().copied().max().unwrap_or(0).max(1) as f32;
        let step_t = (1.4 / deepest).min(0.13);
        let born_at = |i: usize| self.born + depth[i] as f32 * step_t;
        // made while it was up: a new star
        if self.done_before.len() != count {
            self.done_before = nodes.iter().map(|nd| b(nd, "done")).collect();
        }
        for (i, nd) in nodes.iter().enumerate() {
            if b(nd, "done") && !self.done_before[i] {
                self.made.insert(i, now);
                m.tick_sound(1.0);
            }
            self.done_before[i] = b(nd, "done");
        }
        if self.focus.is_some_and(|i| i >= count) {
            self.focus = None;
        }
        // fitted into its part of the sky, then moved and brought closer by hand
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for nd in nodes {
            x0 = x0.min(f(nd, "x"));
            y0 = y0.min(f(nd, "y"));
            x1 = x1.max(f(nd, "x"));
            y1 = y1.max(f(nd, "y"));
        }
        let (span_x, span_y) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        // fitted, but never so tight that the items over the stars run into each other (a row of the game's tree is 27
        // of its px): a tall tree (Adventure) is dragged through, as a sky is
        let fit = ((sky[2] - sky[0]) * 0.86 / span_x).min((sky[3] - sky[1]) * 0.8 / span_y).clamp(7.2 * vh / 27.0, 4.6 * ui.max(vh * 0.11));
        let mid = [(sky[0] + sky[2]) / 2.0, (sky[1] + sky[3]) / 2.0];
        self.mid = mid;
        // the swing: the new constellation comes in from the side it swung to
        let slide = self.swing_dir * w * 0.6 * (1.0 - ease(clamp01((now - self.swing_at - 0.12) / 0.85)));
        let at = |nd: &Value, pan: [f32; 2], unit: f32| [mid[0] + (f(nd, "x") - (x0 + x1) / 2.0) * unit + pan[0] + slide, mid[1] + (f(nd, "y") - (y0 + y1) / 2.0) * unit + pan[1]];
        // a star flown to (or left): closer round it, back to the whole
        if self.focus != self.focus_was {
            if let Some(fi) = self.focus {
                if let Some(old) = self.focus_was {
                    // from one star to another: its card sinks as the new one rises a moment after
                    self.card_gone = Some((old, now));
                }
                self.anchor = at(&nodes[fi], self.pan, fit * self.zoom);
                self.zoom_to = 1.75;
                self.focus_at = now + if self.focus_was.is_some() { 0.18 } else { 0.0 };
                m.tick_sound(0.7);
            } else {
                self.anchor = mid;
                self.zoom_to = 1.0;
            }
            self.focus_was = self.focus;
        }
        self.focus_k = toward(self.focus_k, if self.focus.is_some() { 1.0 } else { 0.0 }, 5.0, dt);
        // closer step by step, what is under the pointer staying under it
        let z_prev = self.zoom;
        self.zoom = toward(self.zoom, self.zoom_to, 7.0, dt);
        let rz = self.zoom / z_prev;
        if (rz - 1.0).abs() > 1e-5 {
            for a in 0..2 {
                self.pan[a] = self.pan[a] * rz + (mid[a] - self.anchor[a]) * (rz - 1.0);
            }
        }
        let unit = fit * self.zoom;
        // the drag, its glide after, and the give at the edges; a star flown to is carried to the left of the sky
        self.range = [((span_x * unit - (sky[2] - sky[0])) / 2.0 + 8.0 * vh).max(4.0 * vh), ((span_y * unit - (sky[3] - sky[1])) / 2.0 + 8.0 * vh).max(4.0 * vh)];
        if let Some(press) = self.press {
            let d = [m.mouse[0] - press.0[0], m.mouse[1] - press.0[1]];
            if self.focus.is_some() && self.drag.is_some() && (d[0] * d[0] + d[1] * d[1]).sqrt() > 6.0 * ui {
                // dragged away from it: back to the whole sky, by hand
                self.focus = None;
            }
        }
        if let Some(fi) = self.focus {
            let p = at(&nodes[fi], self.pan, unit);
            let target = [sky[0] + (sky[2] - sky[0]) * 0.24, mid[1] + 0.03 * h];
            let k_ = 1.0 - (-dt * 4.5).exp();
            self.pan[0] += (target[0] - p[0]) * k_;
            self.pan[1] += (target[1] - p[1]) * k_;
            self.vel = [0.0; 2];
        } else if self.drag.is_some() {
            let d = [m.mouse[0] - self.last_mouse[0], m.mouse[1] - self.last_mouse[1]];
            for a in 0..2 {
                let give = if self.pan[a].abs() > self.range[a] { 0.35 } else { 1.0 };
                self.pan[a] += d[a] * give;
                if dt > 0.0 {
                    self.vel[a] = self.vel[a] * 0.5 + d[a] / dt * 0.5;
                }
            }
        } else {
            let keep = (-dt * 4.0).exp();
            let back = 1.0 - (-dt * 6.0).exp();
            for a in 0..2 {
                self.pan[a] += self.vel[a] * dt;
                self.vel[a] *= keep;
                let inside = self.pan[a].clamp(-self.range[a], self.range[a]);
                if (inside - self.pan[a]).abs() > 0.01 {
                    self.pan[a] += (inside - self.pan[a]) * back;
                    self.vel[a] *= keep;
                }
            }
        }
        self.last_mouse = m.mouse;
        if live {
            m.hit(sky, hid(k::PAGE, SKY, 0));
        }
        let sky_k = shade;
        let star_r = (0.55 * vh * self.zoom.sqrt()).clamp(4.0 * ui, 15.0 * ui);
        // what fades toward the edges of its part of the sky
        let feather = 6.0 * vh;
        let field = |p: [f32; 2]| {
            clamp01((p[0] - sky[0] + feather) / feather) * clamp01((sky[2] + feather - p[0]) / feather) * clamp01((p[1] - sky[1] + feather) / feather) * clamp01((sky[3] + feather - p[1]) / feather)
        };

        // the old tab's constellation, carried off the way the sky swung, fading
        let gone = clamp01((now - self.left_at) / 0.6);
        if !self.leaving.is_empty() {
            if gone >= 1.0 {
                self.leaving.clear();
            } else {
                let a = (1.0 - ease(gone)).powf(1.4) * sky_k;
                let off = -self.swing_dir * w * 0.5 * gone.powf(2.2);
                let sc = |p: [f32; 2]| [p[0] + off, p[1]];
                for g in &self.leaving {
                    let fa = field(sc(g.at));
                    if let Some(pp) = g.parent {
                        c.line(sc(pp), sc(g.at), 1.0, grey(1.0, 0.3 * a * g.lit.max(0.3) * fa), 0.0);
                    }
                    let p = sc(g.at);
                    let r = g.size;
                    c.rect([p[0] - r, p[1] - r, p[0] + r, p[1] + r], r * if g.kind == CHALLENGE { 1.0 } else { 0.2 }, grey(1.0, a * (0.3 + 0.7 * g.lit) * fa));
                }
            }
        }

        // the way from the root to the star looked at (or flown to): lit along, a spark running out to it
        let way_to = self.focus.or(self.hovered.filter(|&i| i < count && self.hover_k > 0.05));
        let way_k = if self.focus.is_some() { ease(self.focus_k) } else { ease(self.hover_k) };
        let way_since = if self.focus.is_some() { self.focus_at } else { self.hover_since };
        let mut way: Vec<usize> = Vec::new();
        if let Some(hv) = way_to {
            let mut at_ = hv as i64;
            while at_ >= 0 && (at_ as usize) < count && !way.contains(&(at_ as usize)) {
                way.push(at_ as usize);
                at_ = n(&nodes[at_ as usize], "parent");
            }
            way.reverse();
        }
        let chain: HashSet<usize> = way.iter().copied().collect();
        // a star flown to: the rest of the sky dims
        let focus_k = self.focus_k;
        let dim_of = |i: usize| if chain.contains(&i) { 1.0 } else { 1.0 - 0.7 * focus_k };
        // how far each is: made, can be made next, further away
        let state = |i: usize| -> f32 {
            let nd = &nodes[i];
            if b(nd, "done") {
                return 1.0;
            }
            let p = n(nd, "parent");
            if p < 0 || p as usize >= count || b(&nodes[p as usize], "done") {
                0.5
            } else {
                0.0
            }
        };
        // the lines: from each star to the one before it, drawn out as the constellation forms
        let mut drawn: Vec<Gone> = Vec::with_capacity(count);
        for i in 0..count {
            let pi = n(&nodes[i], "parent");
            if pi < 0 || pi as usize >= count {
                continue;
            }
            let pi = pi as usize;
            let a_ = at(&nodes[pi], self.pan, unit);
            let b_ = at(&nodes[i], self.pan, unit);
            let start = born_at(pi) + 0.03;
            let end = (born_at(i) + 0.02).max(start + 0.1);
            let grow = ease_out(0.0, 1.0, (now - start) / (end - start));
            if grow <= 0.0 {
                continue;
            }
            let (dx, dy) = (b_[0] - a_[0], b_[1] - a_[1]);
            let len = (dx * dx + dy * dy).sqrt();
            let trim = star_r * 1.8;
            if len <= trim * 2.0 + 1.0 {
                continue;
            }
            let u = [dx / len, dy / len];
            let p0 = [a_[0] + u[0] * trim, a_[1] + u[1] * trim];
            let reach = (len - trim * 2.0) * grow;
            let p1 = [p0[0] + u[0] * reach, p0[1] + u[1] * reach];
            let st = state(i);
            let base = sky_k * dim_of(i);
            // seen as far as its part of the sky goes: a line out past the edge fades there, piece by piece
            let fa = field(p0).max(field(p1)).max(field([(p0[0] + p1[0]) / 2.0, (p0[1] + p1[1]) / 2.0])) * base;
            if fa <= 0.01 {
                continue;
            }
            // wholly inside: one line; else in pieces (their glows would pile up where they meet: none)
            let inside = field(p0).min(field(p1)) > 0.98;
            let pieces = if inside { 1 } else { ((reach / (5.0 * vh)).ceil() as usize).clamp(1, 16) };
            let faded = |c: &mut Canvas, width: f32, col: Rgba, glow: f32| {
                let glow = if pieces > 1 { 0.0 } else { glow };
                for q in 0..pieces {
                    let (k0, k1) = (q as f32 / pieces as f32, (q + 1) as f32 / pieces as f32);
                    let a0 = [p0[0] + (p1[0] - p0[0]) * k0, p0[1] + (p1[1] - p0[1]) * k0];
                    let a1 = [p0[0] + (p1[0] - p0[0]) * k1, p0[1] + (p1[1] - p0[1]) * k1];
                    let fq = field([(a0[0] + a1[0]) / 2.0, (a0[1] + a1[1]) / 2.0]) * base;
                    if fq > 0.01 {
                        c.line(a0, a1, width, [col[0], col[1], col[2], col[3] * fq], glow);
                    }
                }
            };
            let on_way = if chain.contains(&i) && chain.contains(&pi) { way_k } else { 0.0 };
            if on_way > 0.01 {
                // the way to the one looked at: a line of light over whatever it is
                faded(c, 1.6 * ui, grey(1.0, 0.85 * on_way), 7.0 * ui);
            }
            if st < 0.25 {
                // toward what lies further: a dotted trace
                let gap = 7.0 * ui;
                let dots = (reach / gap) as i32;
                for d in 0..=dots {
                    let q = d as f32 * gap;
                    let dp = [p0[0] + u[0] * q, p0[1] + u[1] * q];
                    let dr = 0.8 * ui;
                    let fd = field(dp) * base;
                    if fd > 0.01 {
                        c.rect([dp[0] - dr, dp[1] - dr, dp[0] + dr, dp[1] + dr], dr, grey(1.0, 0.22 * fd));
                    }
                }
            } else if st >= 1.0 {
                faded(c, 1.3 * ui, grey(1.0, 0.75), 5.0 * ui);
                // light runs out along what is made, from the root outward
                if grow >= 1.0 {
                    let ph = (now * 0.3 - depth[pi] as f32 * 0.2 + rnd(i as u32, 41) * 0.35).rem_euclid(1.0);
                    let pp = [p0[0] + u[0] * reach * ph, p0[1] + u[1] * reach * ph];
                    let pa = (ph * PI).sin().powf(1.5) * field(pp) * base;
                    c.glow(pp, 7.0 * ui, grey(1.0, 0.3 * pa));
                    let dr = 1.4 * ui;
                    c.rect([pp[0] - dr, pp[1] - dr, pp[0] + dr, pp[1] + dr], dr, grey(1.0, 0.95 * pa));
                }
            } else {
                faded(c, 1.0 * ui, grey(0.75, 0.45), 0.0);
            }
        }
        // the spark along the way to the one looked at: from the root to it once, then it rests there
        if way.len() > 1 && way_k > 0.05 {
            let pts: Vec<[f32; 2]> = way.iter().map(|&i| at(&nodes[i], self.pan, unit)).collect();
            let total: f32 = pts.windows(2).map(|p| ((p[1][0] - p[0][0]).powi(2) + (p[1][1] - p[0][1]).powi(2)).sqrt()).sum();
            let run = ease_io(clamp01((now - way_since) / (0.35 + total / (60.0 * vh))));
            let mut left_ = run * total;
            for seg in pts.windows(2) {
                let l = ((seg[1][0] - seg[0][0]).powi(2) + (seg[1][1] - seg[0][1]).powi(2)).sqrt();
                if left_ <= l || l <= 0.0 {
                    let q = if l > 0.0 { left_ / l } else { 1.0 };
                    let hp = [seg[0][0] + (seg[1][0] - seg[0][0]) * q, seg[0][1] + (seg[1][1] - seg[0][1]) * q];
                    let a = way_k * sky_k * (1.0 - smooth(0.85, 1.0, run));
                    c.glow(hp, 12.0 * ui, grey(1.0, 0.45 * a));
                    c.glow(hp, 4.0 * ui, grey(1.0, 0.9 * a));
                    break;
                }
                left_ -= l;
            }
        }

        // the stars and their items
        let mut over: Option<usize> = None;
        let iz = (4.0 * vh * self.zoom.sqrt()).clamp(26.0 * ui, 84.0 * ui);
        if self.yaw.len() != count {
            // each starts at its own angle: turning in step they would all go edge-on at once
            self.yaw = (0..count).map(|i| (i as f32 * 137.5) % 360.0 - 180.0).collect();
        }
        for i in 0..count {
            let nd = &nodes[i];
            let p = at(nd, self.pan, unit);
            let since = now - born_at(i);
            let fa = field(p) * sky_k;
            let st = state(i);
            drawn.push(Gone { at: p, parent: n(nd, "parent").try_into().ok().filter(|&q: &usize| q < count).map(|q| at(&nodes[q], self.pan, unit)), kind: n(nd, "type"), lit: st, size: star_r });
            if since <= 0.0 || fa <= 0.01 {
                continue;
            }
            let dimk = dim_of(i);
            let focused = self.focus == Some(i);
            let fk = ease(m.an(hid(k::LIST, STAR, i as u32) ^ 0xf0c5, if focused { self.focus_k } else { 0.0 }, 7.0));
            let id = hid(k::LIST, STAR, i as u32);
            let rise = star_r * 2.2 + iz * 0.5 * (1.0 + 0.9 * fk) + fk * 1.5 * vh;
            let hit = [p[0] - iz * 0.7, p[1] - rise - iz * 0.6, p[0] + iz * 0.7, p[1] + star_r * 1.6];
            if live && self.drag.is_none() && fa > 0.5 {
                m.hit(hit, id);
            }
            let is_hot = m.lit == id || (self.hovered == Some(i) && m.over(hit) && self.drag.is_none());
            if live && m.over(hit) && self.drag.is_none() {
                over = Some(i);
            }
            let hot = m.an(id ^ 0x77, if is_hot { 1.0 } else { 0.0 }, 9.0).max(fk);
            if is_hot && b(nd, "new") {
                self.seen.insert(i);
            }
            // how it pops in, breathes when new, bursts when made just now
            let pop = {
                let k_ = clamp01(since / 0.45);
                1.0 + 0.35 * (k_ * 3.4).sin() * (1.0 - k_) - (1.0 - k_).powi(3)
            };
            let breath = if b(nd, "new") && !self.seen.contains(&i) {
                let ph = ((now + rnd(i as u32, 7) * 2.4) % 2.4) / 2.4;
                if ph < 0.62 { 0.0 } else if ph < 0.76 { ease_io((ph - 0.62) / 0.14) } else { 1.0 - ease_io((ph - 0.76) / 0.24) }
            } else {
                0.0
            };
            let flare = self.made.get(&i).map_or(0.0, |t0| 1.0 - ease(clamp01((now - t0) / 1.4)));
            let a = fa * clamp01(since / 0.2) * dimk;
            let made = clamp01((st - 0.5) * 2.0);
            let next = clamp01(st * 2.0);
            let r = star_r * pop.max(0.0) * (1.0 + 0.55 * hot + 0.5 * breath + 0.9 * flare);
            let kind = n(nd, "type");
            let bright = 0.42 + 0.33 * next + 0.25 * made;
            let col = grey(bright, a * (0.35 + 0.65 * next));
            let halo = (0.45 * made + 0.5 * breath + 0.35 * hot * next + 0.9 * flare).min(1.0) * a;
            let warm = kind == CHALLENGE && made > 0.0;
            if halo > 0.01 {
                let hc = if warm { [FIRE[0], FIRE[1], FIRE[2], 0.42 * halo] } else { grey(1.0, 0.32 * halo) };
                c.glow(p, r * 6.5, hc);
            }
            match kind {
                CHALLENGE => {
                    // a small eclipse: dark inside a burning ring
                    c.rect([p[0] - r, p[1] - r, p[0] + r, p[1] + r], r, grey(0.02, a * 0.9));
                    let ring_c = if warm { [FIRE[0], FIRE[1], FIRE[2], a] } else { col };
                    c.ring(p, r, (1.3 * ui).max(r * 0.22), ring_c);
                }
                GOAL => {
                    let d = r * 1.3;
                    let pts = [[p[0], p[1] - d], [p[0] + d, p[1]], [p[0], p[1] + d], [p[0] - d, p[1]], [p[0], p[1] - d]];
                    c.polyline(&pts, (1.4 * ui).max(r * 0.28), col);
                    let core = r * 0.45 * (0.4 + 0.6 * next);
                    c.rect([p[0] - core, p[1] - core, p[0] + core, p[1] + core], core * 0.3, col);
                }
                _ => {
                    let e = r * 0.85;
                    if made > 0.5 || st < 0.25 {
                        c.rect([p[0] - e, p[1] - e, p[0] + e, p[1] + e], r * 0.2, col);
                    } else {
                        // what can be made next: hollow
                        let pts = [[p[0] - e, p[1] - e], [p[0] + e, p[1] - e], [p[0] + e, p[1] + e], [p[0] - e, p[1] + e], [p[0] - e, p[1] - e]];
                        c.polyline(&pts, (1.2 * ui).max(r * 0.22), col);
                    }
                }
            }
            // a made one glints: a short cross of light, fading to nothing toward its ends, breathing (long and even it
            // read as more lines of the constellation going off nowhere)
            if made > 0.0 {
                let tw_ = 0.6 + 0.4 * (now * (1.1 + rnd(i as u32, 13)) + rnd(i as u32, 14) * 6.28).sin();
                let len = r * (2.0 + 2.5 * flare) * tw_;
                let base_a = 0.42 * a * made * (1.0 - 0.6 * hot);
                for (dx, dy) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                    for q in 0..3 {
                        let (k0, k1) = (q as f32 / 3.0, (q + 1) as f32 / 3.0);
                        let al = base_a * (1.0 - k0).powi(2);
                        let sc = if warm { [FIRE[0], FIRE[1], FIRE[2], al] } else { grey(1.0, al) };
                        let (a0, a1) = (r * 0.9 + (len - r * 0.9) * k0, r * 0.9 + (len - r * 0.9) * k1);
                        c.line([p[0] + dx * a0, p[1] + dy * a0], [p[0] + dx * a1, p[1] + dy * a1], 0.8 * ui, sc, 0.0);
                    }
                }
            }
            // made just now: a new star bursts, rays and two rings going out
            if flare > 0.0 {
                let kk = 1.0 - flare;
                let fc = |al: f32| if warm { [FIRE[0], FIRE[1], FIRE[2], al] } else { grey(1.0, al) };
                c.ring(p, r + 9.0 * vh * ease(kk), 1.5 * ui, fc(0.8 * flare * a));
                let k2 = clamp01((kk - 0.12) / 0.88);
                if k2 > 0.0 {
                    c.ring(p, r + 16.0 * vh * ease(k2), 1.0 * ui, fc(0.45 * (1.0 - k2) * a));
                }
                for j in 0..12u32 {
                    let ang = j as f32 / 12.0 * TAU + rnd(i as u32 ^ j, 51) * 0.4;
                    let reach = r * (2.0 + (10.0 + 8.0 * rnd(j, i as u32)) * ease(kk));
                    let r0 = r * (1.6 + 6.0 * ease(kk));
                    let dir = [ang.cos(), ang.sin()];
                    c.line([p[0] + dir[0] * r0, p[1] + dir[1] * r0], [p[0] + dir[0] * reach, p[1] + dir[1] * reach], 1.0 * ui, fc(0.6 * flare * a), 3.0 * ui);
                }
                c.glow(p, r * 14.0 * (0.5 + kk), fc(0.25 * flare * a));
            }
            // of many parts: a ring of ticks, one lit for each part done; else started and not made: an arc of how far
            let call = n(nd, "call");
            let rr = r * 2.3 + 0.4 * vh * hot;
            if call > 1 && st < 1.0 {
                let ticks = call.min(48) as usize;
                let lit = ((n(nd, "cdone").max(0) as f32 / call as f32) * ticks as f32).round() as usize;
                let shown_t = clamp01((since - 0.2) / 0.6);
                for j in 0..ticks {
                    if j as f32 / ticks as f32 > shown_t {
                        break;
                    }
                    let ang = -PI / 2.0 + j as f32 / ticks as f32 * TAU;
                    let dir = [ang.cos(), ang.sin()];
                    let l = if j < lit { r * 0.75 } else { r * 0.45 };
                    let tc = if j < lit { grey(1.0, 0.85 * a) } else { grey(1.0, 0.22 * a) };
                    c.line([p[0] + dir[0] * rr, p[1] + dir[1] * rr], [p[0] + dir[0] * (rr + l), p[1] + dir[1] * (rr + l)], 0.9 * ui, tc, 0.0);
                }
            } else {
                let pct = f(nd, "pct");
                if st < 1.0 && pct > 0.0 {
                    c.arc(p, rr, 1.2 * ui, pct, grey(1.0, 0.7 * a));
                }
            }
            // the item floats over its star, bobbing, turning as an item dropped in the world
            let shown = clamp01((since - 0.05) / 0.3);
            let bob = (now * 1.7 + rnd(i as u32, 8) * 6.28).sin() * 0.25 * vh * next * (1.0 - fk);
            let near = 0.84 + 0.16 * made.max(hot);
            let isz = iz * near * (1.0 + 0.4 * hot + 0.25 * flare + 0.9 * fk);
            let ia = a * shown * (0.18 + 0.32 * next + 0.5 * made.max(hot * 0.95));
            let ix = p[0] - isz / 2.0;
            let iy = p[1] - rise - isz / 2.0 + bob - (1.0 - shown) * 0.8 * vh - hot * 0.6 * vh;
            c.glow([p[0], iy + isz / 2.0], isz * 0.75, grey(0.0, 0.35 * ia));
            if made > 0.0 || hot > 0.01 {
                let glow_a = (0.10 * made + 0.10 * hot + 0.3 * flare + 0.1 * breath) * ia;
                c.glow([p[0], iy + isz / 2.0], isz * 0.9, if warm { [FIRE[0], FIRE[1], FIRE[2], glow_a] } else { grey(1.0, glow_a) });
            }
            // how it turns: the one flown to slowly round, the one looked at to face you, the made ones spin, the next
            // ones sway, the far ones hold still
            let y = &mut self.yaw[i];
            if focused {
                *y += dt * 28.0;
            } else if is_hot {
                *y += (front(*y) - *y) * (1.0 - (-dt * 9.0).exp());
            } else if made > 0.5 {
                *y += dt * 40.0;
            } else {
                let sway = if next > 0.5 { 20.0 * (now * 0.9 + i as f32 * 1.7).sin() } else { 0.0 };
                *y += (front(*y) + sway - *y) * (1.0 - (-dt * 4.0).exp());
            }
            if y.abs() > 3600.0 {
                *y -= front(*y);
            }
            let icon = n(nd, "icon");
            if icon >= 0 {
                let turn = (*y / 360.0).rem_euclid(1.0).min(0.9995);
                m.extra(8, icon as f32 + turn, [ix, iy, isz], ia);
            }
        }
        self.last_drawn = drawn;
        // the constellation's name in wide capitals under its first star
        if let (Some(root), Some(t)) = (nodes.iter().position(|nd| n(nd, "parent") < 0), tabs.get(sel.max(0) as usize)) {
            let p = at(&nodes[root], self.pan, unit);
            let since = now - self.born - 0.2;
            let a = clamp01(since / 0.8) * field(p) * sky_k * (1.0 - 0.7 * self.focus_k);
            if a > 0.01 {
                let name = s(t, "title").to_uppercase();
                let nsz = m.cl(9.0, 16.0, 1.1);
                let ls = 0.55 + 0.4 * (1.0 - ease(clamp01(since / 1.2)));
                let nw = tw(Face::Caps, &name, nsz, ls);
                m.rising_text(c, Face::Caps, &name, p[0] - nw / 2.0, p[1] + star_r * 3.2, nsz, 1.3, ls, grey(0.75, 0.55 * a), 0.0, 1.0);
            }
        }
        if over != self.hovered && over.is_some() {
            self.hover_since = now;
            m.tick_sound(0.25);
        }
        if over.is_some() {
            self.hovered = over;
        }
        self.hover_k = toward(self.hover_k, if over.is_some() { 1.0 } else { 0.0 }, 7.0, dt);

        // ---- what the star under the pointer is (not while one is flown to): its kind, its name, what it asks, how far
        let low_k = self.hover_k * sky_k * (1.0 - self.focus_k);
        if let Some(i) = self.hovered.filter(|&i| i < count && low_k > 0.01) {
            let since = now - self.hover_since;
            let x = sky[0];
            let mut yy = h - 19.5 * vh;
            Self::about(m, c, &nodes[i], x, &mut yy, right - x, (right - x).min(60.0 * vw), since, low_k, 3, false);
        }

        // ---- the star flown to: all of it beside it
        if self.focus.is_some() {
            self.focus_shown = self.focus;
        }
        // the card of the star before, sinking
        if let Some((gi, at_)) = self.card_gone {
            let k_out = 1.0 - clamp01((now - at_) / 0.28);
            if k_out <= 0.0 || gi >= count {
                self.card_gone = None;
            } else if self.focus.is_some() {
                let x = sky[0] + (sky[2] - sky[0]) * 0.47;
                let width = right - x;
                let mut yy = sky[1] + 2.0 * vh;
                Self::about(m, c, &nodes[gi], x, &mut yy, width, width, 10.0, ease(k_out) * sky_k, 6, true);
            }
        }
        if let Some(i) = self.focus_shown.filter(|&i| i < count) {
            let k_in = ease(self.focus_k) * sky_k;
            if k_in > 0.01 {
                let nd = &nodes[i];
                let since = now - self.focus_at - 0.15;
                let x = sky[0] + (sky[2] - sky[0]) * 0.47;
                let width = right - x;
                let mut yy = sky[1] + 2.0 * vh;
                Self::about(m, c, nd, x, &mut yy, width, width, since, k_in, 6, true);
                // its parts by name, what is left first
                if let Some(crit) = nd.get("crit").and_then(Value::as_array) {
                    let mut parts: Vec<(&str, bool)> = crit.iter().filter_map(|p| Some((p.get(0)?.as_str()?, p.get(1)?.as_bool()?))).collect();
                    parts.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
                    let left_n = parts.iter().filter(|p| !p.1).count();
                    yy += 2.4 * vh;
                    let head = format!("{} {}  ·  {} {}", tr("remain").to_uppercase(), left_n, tr("critdone").to_uppercase(), parts.len() - left_n);
                    let q0 = ease(clamp01((since - 0.25) / 0.5)) * k_in;
                    m.caps(c, &head, x, yy, cap, 0.22, 0.75, q0);
                    yy += cap * 1.3 + 1.4 * vh;
                    let csz = m.cl(8.0, 16.0, 1.05);
                    let row_h = csz * 1.95;
                    let col_w_ = (16.0 * vw).max(150.0 * ui).min(width);
                    let cols = ((width / col_w_) as usize).max(1);
                    let rows = (((h - 7.0 * vh) - yy) / row_h).floor().max(1.0) as usize;
                    let cap_n = cols * rows;
                    let show = parts.len().min(cap_n);
                    for (j, (name, done)) in parts.iter().take(show).enumerate() {
                        let (cx, cy) = (x + (j / rows) as f32 * col_w_, yy + (j % rows) as f32 * row_h);
                        let more = j + 1 == show && parts.len() > show;
                        let q = ease(clamp01((since - 0.3 - j as f32 * 0.012) / 0.45)) * k_in;
                        if q <= 0.01 {
                            continue;
                        }
                        let mk = csz * 0.32;
                        let my = cy + csz * 0.62;
                        if more {
                            let word = format!("+{} {}", parts.len() - show + 1, tr("more").to_uppercase());
                            m.caps(c, &word, cx, cy, csz, 0.12, 0.45, q);
                            continue;
                        }
                        if *done {
                            c.rect([cx, my - mk, cx + 2.0 * mk, my + mk], mk * 0.25, grey(1.0, 0.9 * q));
                        } else {
                            let pts = [[cx, my - mk], [cx + 2.0 * mk, my - mk], [cx + 2.0 * mk, my + mk], [cx, my + mk], [cx, my - mk]];
                            c.polyline(&pts, 1.0 * ui, grey(1.0, 0.45 * q));
                        }
                        let label = fonts().fit_ls(Face::Caps, csz, &name.to_uppercase(), col_w_ - 2.0 * mk - 2.4 * vh, 0.06);
                        m.caps(c, &label, cx + 2.0 * mk + 0.9 * vh, cy, csz, 0.06, if *done { 0.82 } else { 0.42 }, q);
                    }
                }
            }
        }
        if nodes.is_empty() {
            let t = tr("noadv").to_uppercase();
            let tw_ = tw(Face::Caps, &t, cap, 0.22);
            m.caps(c, &t, (sky[0] + sky[2] - tw_) / 2.0, (sky[1] + sky[3]) / 2.0, cap, 0.22, 0.45, clamp01(ease(m.vis(v, 2.0)) * 1.15));
        }

        // back, and what the hands can do
        let fsize = m.cl(18.0, 50.0, 3.2);
        let by = h - 5.0 * vh - 1.12 * fsize;
        let pb = m.vis(v, 6.0);
        let r = m.word(c, hid(k::BACK, 0, 0), &tr("back").to_uppercase(), left, by, WordStyle::new(Face::Thin, fsize), pb, live, false);
        m.caps(c, "ESC", r[2] + 1.6 * vh, by + 1.12 * fsize - cap * 1.6, cap, 0.22, 0.29, clamp01(ease(pb) * 1.15));
        for (word, k_) in [(tr("advhint"), 1.0 - self.focus_k), (tr("advback"), self.focus_k)] {
            if k_ > 0.01 {
                let hint = word.to_uppercase();
                let hw = tw(Face::Caps, &hint, cap, 0.22);
                m.caps(c, &hint, right - hw, h - 5.0 * vh - cap * 1.6, cap, 0.22, 0.3 * k_, clamp01(ease(pb) * 1.15) * ease(k_));
            }
        }
    }

    /// What a star is, from `yy` down: its kind (decoding), its name, what it asks (out of noise, `lines` at most), when
    /// it was made or how far it is. `wide` the name's room, `text_w` the description's.
    #[allow(clippy::too_many_arguments)]
    fn about(m: &mut Menus, c: &mut Canvas, nd: &Value, x: f32, yy: &mut f32, wide: f32, text_w: f32, since: f32, k_in: f32, lines: usize, big: bool) {
        let vh = m.vh();
        let vw = m.w / 100.0;
        let ui = m.uipx();
        let cap = m.cl(9.0, 18.0, 1.15);
        let kind = n(nd, "type");
        let (kind_word, kind_col) = match kind {
            CHALLENGE => (tr("challenge"), [FIRE[0], FIRE[1], FIRE[2], 1.0]),
            GOAL => (tr("goal"), grey(0.92, 1.0)),
            _ => (tr("task"), grey(0.55, 1.0)),
        };
        // all of it rises from when it is shown (a card coming after another waits for that one to sink)
        let kq = ease(k_in) * ease(clamp01(since / 0.3));
        let kw = decode(&kind_word.to_uppercase(), clamp01(since / 0.25), m.now, 31);
        m.rising_text(c, Face::Caps, &kw, x, *yy, cap, 1.4, 0.3, [kind_col[0], kind_col[1], kind_col[2], k_in], 0.0, kq);
        *yy += cap * 1.4 + 0.5 * vh;
        let tsz = if big { m.cl(22.0, 64.0, 4.4) } else { m.cl(18.0, 50.0, 3.3) };
        let title = s(nd, "title").to_uppercase();
        let title_lines = if big { fonts().wrap_ls(Face::Light, tsz, &title, wide, 0.02) } else { vec![fonts().fit_ls(Face::Light, tsz, &title, wide, 0.02)] };
        for (j, l) in title_lines.iter().take(2).enumerate() {
            let q = ease(clamp01((since - j as f32 * 0.08) / 0.45)) * ease(k_in);
            m.rising_text(c, Face::Light, l, x, *yy, tsz, 1.1, 0.02, grey(1.0, k_in), 0.2 * (1.0 - ease(clamp01(since / 0.5))), q);
            *yy += tsz * 1.1;
        }
        *yy += 0.4 * vh;
        let dsz = m.cl(9.0, 18.0, 1.3);
        let desc = decode(s(nd, "desc"), clamp01(since / (0.3 + s(nd, "desc").chars().count() as f32 * 0.004)), m.now, 57);
        for l in fonts().wrap_ls(Face::Caps, dsz, &desc, text_w, 0.04).iter().take(lines) {
            m.rising_text(c, Face::Caps, l, x, *yy, dsz, 1.5, 0.04, grey(0.62, k_in), 0.0, kq);
            *yy += dsz * 1.5;
        }
        *yy += 0.7 * vh;
        if b(nd, "done") {
            let line = format!("{}{}", tr("obtained").to_uppercase(), if s(nd, "date").is_empty() { String::new() } else { format!("  ·  {}", s(nd, "date")) });
            m.caps(c, &line, x, *yy, cap, 0.22, 0.85, kq);
        } else {
            // a hair that fills to how far, and its count
            let track = if big { (22.0 * vw).min(wide * 0.6) } else { 16.0 * vw };
            let fill = ease_out(0.0, 1.0, since / 0.7) * f(nd, "pct");
            let ly = *yy + cap * 0.7;
            c.line([x, ly], [x + track, ly], 1.0, grey(1.0, 0.15 * kq), 0.0);
            c.light(x, ly, x + track, 0.9 * kq, 0.0, fill, 0.6, ui);
            let word = if s(nd, "prog").is_empty() { tr("notyet").to_uppercase() } else { s(nd, "prog").to_string() };
            m.caps(c, &word, x + track + 1.4 * vh, *yy, cap, 0.22, 0.6, kq);
        }
        *yy += cap * 1.4;
    }

    pub fn event(&mut self, m: &mut Menus, v: &View, e: Event) {
        match e {
            // the stand's script (menus/dev.rs "key dev-focus <words of a title>"): fly to that star
            Event::Key(name, _) if name.starts_with("dev-focus ") => {
                let want = name["dev-focus ".len()..].to_lowercase();
                let nodes = v.data.get("nodes").and_then(Value::as_array);
                self.focus = nodes.and_then(|ns| ns.iter().position(|nd| s(nd, "title").to_lowercase().contains(&want)));
            }
            Event::Press(hit, button) if button & 15 == 0 => {
                let (kind, id, sub) = (hid_kind(hit), hid_n(hit), hid_sub(hit));
                match kind {
                    k::TAB if id == TABS => {
                        if sub as i64 != self.tab {
                            m.ask(format!("tab {sub}"));
                        }
                    }
                    k::BACK => m.ask("esc"),
                    k::PAGE | k::LIST if (kind == k::PAGE && id == SKY) || (kind == k::LIST && id == STAR) => {
                        self.drag = Some(m.mouse);
                        self.last_mouse = m.mouse;
                        self.vel = [0.0; 2];
                        self.press = Some((m.mouse, if kind == k::LIST { Some(sub as usize) } else { None }));
                    }
                    _ => {}
                }
            }
            // the right button: back from a star flown to
            Event::Press(_, button) if button & 15 == 1 => self.focus = None,
            Event::Release(_, _) => {
                if let Some((at, star)) = self.press.take() {
                    let d = ((m.mouse[0] - at[0]).powi(2) + (m.mouse[1] - at[1]).powi(2)).sqrt();
                    if d < 6.0 * m.uipx() {
                        // a click: on a star flies to it (again: back), on the sky back to the whole
                        self.focus = match star {
                            Some(i) if self.focus != Some(i) => Some(i),
                            _ => None,
                        };
                    }
                }
                self.drag = None;
            }
            Event::Scroll(dy) => {
                // closer round the pointer: what is under it stays under it
                self.zoom_to = (self.zoom_to * (1.0 + dy * 0.14)).clamp(0.5, 3.2);
                self.anchor = m.mouse;
            }
            Event::Key(name, _) => match name.as_str() {
                "Escape" if self.focus.is_some() => self.focus = None,
                "Escape" => m.ask("esc"),
                "Tab" => m.ask(format!("tab {}", self.tab + 1)),
                _ => {}
            },
            _ => {}
        }
    }
}
