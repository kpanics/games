//! Server Adaptation (her word, 25.10): helpers for the servers' own things, as ReallyWorld, HolyWorld and FunTime have
//! them (and their copies, where the same things are): while one is held, where it will reach — a trap's box, a plast's
//! wall, a ring of who it touches — lit when someone is inside; how long one used still holds, at the place; keys that
//! use them from the hotbar (on ReallyWorld itself only from there: its rules) or the inventory; on ReallyWorld the
//! chat's words it mutes for held back, and /fix all after a fight. The mode is the player's to choose (or the
//! server's, when asked); what a server forbids is gone by its own address, whatever the mode (swap.rs hidden_here).
//!
//! Java tells the tick's facts with the acting modules' (play.rs: what the hands hold, the cooldowns started, the other
//! players near) and, each frame, the camera ("play\tview"); what is drawn is drawn on the HUD, projected here.

use super::draw::{grey, Canvas, Rgba};
use super::i18n::{tr, trn};
use super::text::{fonts, Face};
use super::theme::{self, fade};
use super::Gui;
use std::collections::HashMap;
use std::time::Instant;

pub(super) const MODULE: &str = "server-adaptation";
pub(super) const MODES: [&str; 3] = ["ReallyWorld", "HolyWorld", "FunTime"];
const EXPERIENCE_ITEM: &str = "minecraft:flower_banner_pattern";
const EXPERIENCE_NAME: &str = "Свиток опыта";

pub(super) fn item_name_matches(expected: &str, actual: &str) -> bool {
    fn plain(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if matches!(ch, '§' | '&') && chars.peek().is_some_and(|next| next.is_ascii_hexdigit() || "klmnorxKLMNORX".contains(*next)) {
                chars.next();
            } else {
                out.push(ch);
            }
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
    }
    let expected = plain(expected);
    !expected.is_empty() && expected == plain(actual)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Zone {
    /// a box round the player's block: so many blocks out to each side (and the block's own), so many up
    Cube {
        side: i32,
        up: i32,
    },
    /// a box centred on the player: so far out each way, so far up
    Centred {
        half: f32,
        up: f32,
    },
    /// a ring on the ground this far out
    Ring(f32),
    /// FunTime's plast: a wall ahead, a floor under, a roof over, as the look goes
    Plast,
    None,
}

/// A server's own thing: its mode (MODES), its name (its key's label), the game's item it is, where it reaches, used
/// sneaking (ReallyWorld's firework star), the setting of how long it holds (none: it has no time).
struct Thing {
    mode: usize,
    name: &'static str,
    item: &'static str,
    zone: Zone,
    sneak: bool,
    time: Option<&'static str>,
}

const fn thing(mode: usize, name: &'static str, item: &'static str, zone: Zone, sneak: bool, time: Option<&'static str>) -> Thing {
    Thing {
        mode,
        name,
        item,
        zone,
        sneak,
        time,
    }
}

/// The things, as the clients that know them have them (their items and their reach).
const THINGS: [Thing; 14] = [
    thing(0, "Anti Fly", "minecraft:firework_star", Zone::Ring(16.0), true, None),
    thing(
        0,
        "Ender trap",
        "minecraft:heart_of_the_sea",
        Zone::Centred { half: 1.5, up: 3.0 },
        false,
        Some("Trap time"),
    ),
    thing(
        1,
        "Stun trap",
        "minecraft:prismarine_shard",
        Zone::Cube { side: 3, up: 6 },
        false,
        Some("Trap time"),
    ),
    thing(
        1,
        "Chorus trap",
        "minecraft:popped_chorus_fruit",
        Zone::Cube { side: 3, up: 6 },
        false,
        Some("Trap time"),
    ),
    thing(1, "Stun", "minecraft:nether_star", Zone::Ring(30.0), false, None),
    thing(1, "Jack o'Lantern", "minecraft:jack_o_lantern", Zone::Ring(10.0), false, None),
    thing(1, "Snowball", "minecraft:snowball", Zone::None, false, None),
    thing(2, "Trap", "minecraft:netherite_scrap", Zone::Cube { side: 2, up: 4 }, false, Some("Trap time")),
    thing(2, "Plast", "minecraft:dried_kelp", Zone::Plast, false, Some("Plast time")),
    thing(2, "Disorientation", "minecraft:ender_eye", Zone::Ring(10.0), false, None),
    thing(2, "Sheer dust", "minecraft:sugar", Zone::Ring(10.0), false, None),
    thing(2, "Fire tornado", "minecraft:fire_charge", Zone::Ring(10.0), false, None),
    thing(2, "God aura", "minecraft:phantom_membrane", Zone::Ring(3.0), false, None),
    thing(2, "Freeze snowball", "minecraft:snowball", Zone::None, false, None),
];

/// A time running at a place: what (its name), where (the top of its zone), since, how long (s).
#[derive(Clone, Debug)]
struct Timer {
    name: &'static str,
    at: [f64; 3],
    since: Instant,
    secs: f32,
}

#[derive(Default)]
pub(super) struct Adapt {
    /// the camera this frame: where it is, its turn (a quaternion: x, y, z, w), the projection (by columns); seen yet
    cam: [f64; 3],
    rot: [f32; 4],
    proj: [f32; 16],
    seen: bool,
    /// the player's feet (between ticks, as drawn) and look (yaw, pitch)
    feet: [f64; 3],
    look: [f32; 2],
    /// what the hands hold (the game's item ids: main, off)
    held: [String; 2],
    /// the other players near, but friends: their feet
    others: Vec<[f64; 3]>,
    timers: Vec<Timer>,
    /// the uses the keys asked for (its item, sneaking)
    asks: Vec<(&'static str, bool, &'static str)>,
    /// in a fight at the last tick; when /fix all is due
    fight: bool,
    fix_at: Option<Instant>,
    /// the chat's line held back last, and when (the same again soon: it goes)
    held_back: Option<(String, Instant)>,
}

/// A vector turned by a quaternion (x, y, z, w).
fn turn(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let u = [q[0], q[1], q[2]];
    let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let c1 = cross(u, v);
    let t = [c1[0] + q[3] * v[0], c1[1] + q[3] * v[1], c1[2] + q[3] * v[2]];
    let c2 = cross(u, t);
    [v[0] + 2.0 * c2[0], v[1] + 2.0 * c2[1], v[2] + 2.0 * c2[2]]
}

/// A box's twelve edges.
fn edges(lo: [f64; 3], hi: [f64; 3]) -> Vec<([f64; 3], [f64; 3])> {
    let p = |x: bool, y: bool, z: bool| [if x { hi[0] } else { lo[0] }, if y { hi[1] } else { lo[1] }, if z { hi[2] } else { lo[2] }];
    let mut e = Vec::with_capacity(12);
    for a in [false, true] {
        for b in [false, true] {
            e.push((p(false, a, b), p(true, a, b)));
            e.push((p(a, false, b), p(a, true, b)));
            e.push((p(a, b, false), p(a, b, true)));
        }
    }
    e
}

/// Where a thing reaches now: its boxes (lowest, highest corners) or its ring (centre, radius).
fn reach(z: Zone, feet: [f64; 3], look: [f32; 2], big: bool) -> (Vec<([f64; 3], [f64; 3])>, Option<([f64; 3], f64)>) {
    let b = [feet[0].floor(), feet[1].floor(), feet[2].floor()];
    match z {
        Zone::Cube { side, up } => {
            let (s, u) = if big {
                (side as f64 + 1.0, up as f64 + 2.0)
            } else {
                (side as f64, up as f64)
            };
            (vec![([b[0] - s, b[1], b[2] - s], [b[0] + s + 1.0, b[1] + u, b[2] + s + 1.0])], None)
        }
        Zone::Centred { half, up } => {
            let h = half as f64;
            (
                vec![([feet[0] - h, feet[1], feet[2] - h], [feet[0] + h, feet[1] + up as f64, feet[2] + h])],
                None,
            )
        }
        Zone::Ring(r) => (Vec::new(), Some((feet, r as f64))),
        Zone::Plast => {
            let (s, t) = if big { (3.0, 4.0) } else { (2.0, 2.0) };
            if look[1] < -45.0 {
                // a roof over
                (vec![([b[0] - s, b[1] + 3.0, b[2] - s], [b[0] + s + 1.0, b[1] + 3.0 + t, b[2] + s + 1.0])], None)
            } else if look[1] > 45.0 {
                // a floor under
                (vec![([b[0] - s, b[1] - 3.0, b[2] - s], [b[0] + s + 1.0, b[1] - 1.0, b[2] + s + 1.0])], None)
            } else {
                // a wall three blocks ahead, across the look (its nearest side of the world)
                let f = (look[0] % 360.0 + 360.0) % 360.0;
                let hgt = if big { 7.0 } else { 5.0 };
                let (c, across_x) = if !(45.0..315.0).contains(&f) {
                    ([b[0], b[1] - 1.0, b[2] + 3.0], true)
                } else if f < 135.0 {
                    ([b[0] - 3.0, b[1] - 1.0, b[2]], false)
                } else if f < 225.0 {
                    ([b[0], b[1] - 1.0, b[2] - 3.0], true)
                } else {
                    ([b[0] + 3.0, b[1] - 1.0, b[2]], false)
                };
                let bx = if across_x {
                    ([c[0] - s, c[1], c[2] - 1.0], [c[0] + s + 1.0, c[1] + hgt, c[2] + 2.0])
                } else {
                    ([c[0] - 1.0, c[1], c[2] - s], [c[0] + 2.0, c[1] + hgt, c[2] + s + 1.0])
                };
                (vec![bx], None)
            }
        }
        Zone::None => (Vec::new(), None),
    }
}

impl Gui {
    fn adapt_set(&self) -> Option<&[super::data::Setting]> {
        self.mods.iter().find(|m| m.id == MODULE && m.on).map(|m| m.set.as_slice())
    }

    /// The mode the helpers work by (MODES' number): the chosen one, or the server's when asked and it is one of them;
    /// none while the module is off.
    pub(super) fn adapt_mode(&self) -> Option<usize> {
        let set = self.adapt_set()?;
        if theme::on(set, "By the server") {
            if let Some(i) = MODES.iter().position(|s| *s == self.server) {
                return Some(i);
            }
        }
        Some(theme::pick(set, "Mode").min(MODES.len() - 1))
    }

    pub(super) fn adapt_ready(&self, item: &str) -> bool {
        self.adapt_mode()
            .is_some_and(|mode| item == EXPERIENCE_ITEM || THINGS.iter().any(|t| t.mode == mode && t.item == item))
    }

    /// A key down in the world (play_key): a thing's key of the mode asks for it to be used.
    pub(super) fn adapt_key(&mut self, name: &str) {
        let Some(mode) = self.adapt_mode() else { return };
        let set = self.adapt_set().unwrap_or(&[]);
        let mut asked = Vec::new();
        for t in THINGS.iter().filter(|t| t.mode == mode) {
            if let Some(super::data::Kind::Key(k)) = theme::find(set, t.name) {
                if !k.is_empty() && k == name {
                    asked.push((t.item, t.sneak, ""));
                }
            }
        }
        if let Some(super::data::Kind::Key(key)) = theme::find(set, "Experience scroll") {
            if !key.is_empty() && key == name {
                asked.push((EXPERIENCE_ITEM, false, EXPERIENCE_NAME));
            }
        }
        self.adapt.asks.extend(asked);
    }

    /// The camera this frame ("x y z | qx qy qz qw | 16 by columns | feet x y z | yaw pitch", spaces).
    pub(super) fn adapt_view(&mut self, s: &str) {
        let v: Vec<f64> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        if v.len() < 28 {
            return;
        }
        let a = &mut self.adapt;
        a.cam = [v[0], v[1], v[2]];
        a.rot = [v[3] as f32, v[4] as f32, v[5] as f32, v[6] as f32];
        for i in 0..16 {
            a.proj[i] = v[7 + i] as f32;
        }
        a.feet = [v[23], v[24], v[25]];
        a.look = [v[26] as f32, v[27] as f32];
        a.seen = true;
    }

    /// Each tick (play_tick's facts): the hands, the players near, the cooldowns started (a timer for a thing used),
    /// the keys' uses asked, /fix all after a fight.
    pub(super) fn adapt_tick(&mut self, f: &HashMap<&str, &str>, out: &mut Vec<String>) {
        let Some(mode) = self.adapt_mode() else {
            self.adapt.timers.clear();
            self.adapt.asks.clear();
            self.adapt.fix_at = None;
            return;
        };
        let set: Vec<super::data::Setting> = self.adapt_set().map(|s| s.to_vec()).unwrap_or_default();
        let get = |k: &str| f.get(k).copied().unwrap_or("");
        self.adapt.held = [get("held").to_string(), get("off").to_string()];
        // the others near, but friends ("name x y z" by ;)
        let friends = self.play.friends.clone();
        self.adapt.others = get("others")
            .split(';')
            .filter_map(|p| {
                let mut w = p.split_whitespace();
                let name = w.next()?;
                if friends.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                    return None;
                }
                Some([w.next()?.parse().ok()?, w.next()?.parse().ok()?, w.next()?.parse().ok()?])
            })
            .collect();
        let pos: Vec<f64> = get("pos").split_whitespace().filter_map(|v| v.parse().ok()).collect();
        // a cooldown started for a thing (its own group, or the hand's item as it went): its time at the place
        for g in get("started").split(',').filter(|g| !g.is_empty()) {
            let held = self.adapt.held.clone();
            let found = THINGS
                .iter()
                .find(|t| t.mode == mode && t.time.is_some() && (t.item == g || held.iter().any(|h| h == t.item)));
            if let (Some(t), true) = (found, pos.len() == 3) {
                let big = mode == 2 && theme::on(&set, "Dragon trap") && t.name == "Trap";
                let secs = if big {
                    theme::num(&set, "Dragon trap time")
                } else {
                    theme::num(&set, t.time.unwrap_or("Trap time"))
                };
                let feet = [pos[0], pos[1], pos[2]];
                let (boxes, _) = reach(t.zone, feet, self.adapt.look, big);
                let top = boxes.first().map_or([feet[0], feet[1] + 3.0, feet[2]], |b| {
                    [(b.0[0] + b.1[0]) / 2.0, b.1[1] + 0.4, (b.0[2] + b.1[2]) / 2.0]
                });
                self.adapt.timers.retain(|q| q.name != t.name);
                self.adapt.timers.push(Timer {
                    name: t.name,
                    at: top,
                    since: Instant::now(),
                    secs,
                });
            }
        }
        self.adapt.timers.retain(|t| t.since.elapsed().as_secs_f32() < t.secs + 0.6);
        // the keys' uses: from the hotbar only on ReallyWorld itself (its rules), from anywhere elsewhere
        let hotbar_only = self.server == super::swap::REALLYWORLD;
        for (item, sneak, name) in std::mem::take(&mut self.adapt.asks) {
            let mut request = format!("use {item} {} {}", hotbar_only as i32, sneak as i32);
            if !name.is_empty() {
                request.push('\u{1f}');
                request.push_str(name);
            }
            out.push(request);
        }
        // ReallyWorld: /fix all a moment after a fight ends
        let fight = self.hud.facts.combat.is_some();
        if mode == 0 && self.adapt.fight && !fight && theme::on(&set, "/fix all after a fight") {
            self.adapt.fix_at = Some(Instant::now() + std::time::Duration::from_millis(1200));
        }
        self.adapt.fight = fight;
        if self.adapt.fix_at.is_some_and(|t| Instant::now() >= t) {
            self.adapt.fix_at = None;
            out.push("cmd fix all".into());
        }
    }

    /// A line about to be sent to the chat: held back (true) when ReallyWorld mutes for a word in it — sent again
    /// within a few seconds, it goes.
    pub(super) fn adapt_chat(&mut self, line: &str) -> bool {
        if self.adapt_mode() != Some(0) {
            return false;
        }
        let set: Vec<super::data::Setting> = self.adapt_set().map(|s| s.to_vec()).unwrap_or_default();
        if !theme::on(&set, "Chat filter") {
            return false;
        }
        let l = line.trim().to_lowercase().replace('ё', "е");
        // commands: only those that say something to someone
        if let Some(cmd) = l.strip_prefix('/') {
            let first = cmd.split_whitespace().next().unwrap_or("");
            if !["msg", "m", "r", "w", "tell", "reply", "pm", "cc"].contains(&first) {
                return false;
            }
        }
        let words = theme::text(&set, "Words").to_lowercase().replace('ё', "е");
        let Some(word) = words.split(',').map(str::trim).find(|w| !w.is_empty() && l.contains(*w)) else {
            return false;
        };
        if let Some((was, at)) = &self.adapt.held_back {
            if *was == l && at.elapsed().as_secs_f32() < 6.0 {
                self.adapt.held_back = None;
                return false;
            }
        }
        self.adapt.held_back = Some((l, Instant::now()));
        let t = self.clock.elapsed().as_secs_f32();
        self.hud.note(
            tr("Server Adaptation").to_string(),
            trn(
                "“{word}” — ReallyWorld mutes for it. Enter again to send it anyway",
                &[("word", word.to_string())],
            ),
            t,
        );
        true
    }

    // ------------------------------------------------------------------ drawn (the HUD's frame)
    /// Where the camera is this frame.
    pub(super) fn adapt_cam(&self) -> [f64; 3] {
        self.adapt.cam
    }

    pub(super) fn adapt_clip(&self, p: [f64; 3]) -> [f32; 4] {
        let a = &self.adapt;
        let v = [(p[0] - a.cam[0]) as f32, (p[1] - a.cam[1]) as f32, (p[2] - a.cam[2]) as f32];
        let v = turn([-a.rot[0], -a.rot[1], -a.rot[2], a.rot[3]], v);
        let m = &a.proj;
        let row = |r: usize| m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2] + m[12 + r];
        [row(0), row(1), row(2), row(3)]
    }

    fn adapt_screen(q: [f32; 4], w: f32, h: f32) -> [f32; 2] {
        [(q[0] / q[3] * 0.5 + 0.5) * w, (1.0 - (q[1] / q[3] * 0.5 + 0.5)) * h]
    }

    /// A segment of the world drawn (cut where it goes behind the eye).
    pub(super) fn adapt_seg(&self, c: &mut Canvas, a: [f64; 3], b: [f64; 3], w: f32, h: f32, width: f32, col: Rgba) {
        const NEAR: f32 = 0.05;
        let (mut p, mut q) = (self.adapt_clip(a), self.adapt_clip(b));
        if p[3] < NEAR && q[3] < NEAR {
            return;
        }
        let cut = |x: [f32; 4], y: [f32; 4]| {
            let t = (NEAR - x[3]) / (y[3] - x[3]);
            std::array::from_fn(|i| x[i] + (y[i] - x[i]) * t)
        };
        if p[3] < NEAR {
            p = cut(p, q);
        } else if q[3] < NEAR {
            q = cut(q, p);
        }
        c.line(Gui::adapt_screen(p, w, h), Gui::adapt_screen(q, w, h), width, col, width * 1.5);
    }

    /// The zone of the thing held (lit when someone else is in it), and the times running at their places.
    pub(super) fn adapt_draw(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(mode) = self.adapt_mode() else { return };
        if !self.adapt.seen {
            return;
        }
        let set: Vec<super::data::Setting> = self.adapt_set().map(|s| s.to_vec()).unwrap_or_default();
        let width = theme::num(&set, "Zone thickness").max(0.5);
        let base = theme::or(theme::col(&set, "Zone colour"), super::theme::pal().accent, self.now);
        let lit = theme::or(theme::col(&set, "Someone inside"), [0.42, 0.83, 0.55, 1.0], self.now);
        let held = self.adapt.held.clone();
        let thing = THINGS
            .iter()
            .find(|t| t.mode == mode && t.zone != Zone::None && held.iter().any(|x| x == t.item));
        let shown = self.an(
            super::ak(super::A_HOV, 42_000),
            if thing.is_some() && theme::on(&set, "Zones") { 1.0 } else { 0.0 },
            10.0,
            0.0,
        );
        if let (Some(t), true) = (thing, shown > 0.01) {
            let big = mode == 2 && theme::on(&set, "Dragon trap");
            let (boxes, ring) = reach(t.zone, self.adapt.feet, self.adapt.look, big);
            let others = self.adapt.others.clone();
            let inside = others.iter().any(|o| {
                boxes.iter().any(|b| {
                    o[0] >= b.0[0] - 0.5 && o[0] <= b.1[0] + 0.5 && o[1] >= b.0[1] - 1.8 && o[1] <= b.1[1] + 0.5 && o[2] >= b.0[2] - 0.5 && o[2] <= b.1[2] + 0.5
                }) || ring.is_some_and(|(cn, r)| ((o[0] - cn[0]).powi(2) + (o[2] - cn[2]).powi(2)).sqrt() <= r && (o[1] - cn[1]).abs() < 6.0)
            });
            let k = self.an(super::ak(super::A_HOV, 42_001), if inside { 1.0 } else { 0.0 }, 10.0, 0.0);
            let col = super::draw::mix(base, lit, k);
            let col = fade(col, shown);
            for b in &boxes {
                for (p, q) in edges(b.0, b.1) {
                    self.adapt_seg(c, p, q, w, h, width, col);
                }
            }
            if let Some((cn, r)) = ring {
                let n = 96;
                let y = cn[1] + 0.05;
                for i in 0..n {
                    let a0 = std::f64::consts::TAU * i as f64 / n as f64;
                    let a1 = std::f64::consts::TAU * (i + 1) as f64 / n as f64;
                    self.adapt_seg(
                        c,
                        [cn[0] + r * a0.cos(), y, cn[2] + r * a0.sin()],
                        [cn[0] + r * a1.cos(), y, cn[2] + r * a1.sin()],
                        w,
                        h,
                        width,
                        col,
                    );
                }
            }
        }
        // the times running, over their places
        if !theme::on(&set, "Timers") {
            return;
        }
        let r = self.r;
        let accent = super::theme::pal().accent;
        for t in self.adapt.timers.clone() {
            let left = (t.secs - t.since.elapsed().as_secs_f32()).max(0.0);
            let q = self.adapt_clip(t.at);
            if q[3] < 0.05 {
                continue;
            }
            let at = Gui::adapt_screen(q, w, h);
            // (nearer, bigger; a moment to come in and to go)
            let dist = ((t.at[0] - self.adapt.cam[0]).powi(2) + (t.at[1] - self.adapt.cam[1]).powi(2) + (t.at[2] - self.adapt.cam[2]).powi(2)).sqrt() as f32;
            let k = (1.15 - dist / 40.0).clamp(0.55, 1.0);
            let a = (t.since.elapsed().as_secs_f32() / 0.25).min(1.0) * (left / 0.5).min(1.0);
            let rad = 1.1 * r * k;
            c.alpha = a;
            c.rect(
                [at[0] - rad - 2.0, at[1] - rad - 2.0, at[0] + rad + 2.0, at[1] + rad + 2.0],
                rad + 2.0,
                grey(0.0, 0.45),
            );
            c.ring(at, rad, 2.0, grey(1.0, 0.18));
            c.arc(at, rad, 2.0, (left / t.secs.max(0.1)).clamp(0.0, 1.0), accent);
            let secs = if left < 10.0 {
                format!("{left:.1}")
            } else {
                format!("{}", left.ceil() as i32)
            };
            c.text_c(at[0], at[1] + 0.3 * r * k, 0.75 * r * k, Face::Semi, &secs, grey(1.0, 0.95));
            let name = tr(t.name);
            let nw = fonts().width(Face::Medium, 0.62 * r * k, name);
            c.rect(
                [
                    at[0] - nw / 2.0 - 0.4 * r,
                    at[1] + rad + 0.35 * r,
                    at[0] + nw / 2.0 + 0.4 * r,
                    at[1] + rad + 1.35 * r,
                ],
                0.5 * r,
                grey(0.0, 0.45),
            );
            c.text_c(at[0], at[1] + rad + 1.05 * r, 0.62 * r * k, Face::Medium, name, grey(1.0, 0.9));
            c.alpha = 1.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experience_scroll_uses_its_material_and_exact_name_in_every_mode() {
        assert!(item_name_matches("Свиток опыта", "&eСвиток опыта"));
        assert!(item_name_matches("Свиток опыта", "§e§l СВИТОК\u{a0}опыта  "));
        assert!(!item_name_matches("Свиток опыта", "Цветок"));
        assert!(!item_name_matches("Свиток опыта", "Большой свиток опыта"));
        assert!(!item_name_matches("&e", ""));
        let mut g = Gui::new();
        let i = g.mod_index(MODULE);
        g.mods[i].on = true;
        for s in &mut g.mods[i].set {
            if s.label == "Experience scroll" {
                s.kind = super::super::data::Kind::Key("G".into());
            }
        }
        assert_eq!(g.play_call("item-name", "Свиток опыта\u{1f}&eСвиток опыта"), "1");
        assert_eq!(g.play_call("item-name", "Свиток опыта\u{1f}Цветок"), "0");
        for mode in 0..MODES.len() {
            if let super::super::data::Kind::Choice { v, .. } = &mut g.mods[i].set.iter_mut().find(|s| s.label == "Mode").unwrap().kind {
                *v = mode;
            }
            assert!(g.adapt_ready(EXPERIENCE_ITEM));
            assert_eq!(g.adapt_ready("minecraft:firework_star"), mode == 0);
            g.adapt_key("G");
            let mut out = Vec::new();
            g.adapt_tick(&HashMap::new(), &mut out);
            assert_eq!(out, ["use minecraft:flower_banner_pattern 0 0\u{1f}Свиток опыта"]);
        }
        g.adapt_key("G");
        g.mods[i].on = false;
        assert!(!g.adapt_ready(EXPERIENCE_ITEM));
        let mut out = Vec::new();
        g.adapt_tick(&HashMap::new(), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn queued_uses_are_cancelled_when_the_world_or_server_changes() {
        let mut g = Gui::new();
        let i = g.mod_index(MODULE);
        g.mods[i].on = true;
        for s in &mut g.mods[i].set {
            if s.label == "Experience scroll" {
                s.kind = super::super::data::Kind::Key("G".into());
            }
        }
        g.play_call("tick", "world=1\u{1f}server=first.test");
        g.adapt_key("G");
        assert!(g.play_call("tick", "world=0").is_empty());
        assert!(g.play_call("tick", "world=1\u{1f}server=first.test").is_empty());
        g.adapt_key("G");
        assert!(g.play_call("tick", "world=1\u{1f}server=second.test").is_empty());
        g.adapt_key("G");
        assert_eq!(
            g.play_call("tick", "world=1\u{1f}server=second.test"),
            "use minecraft:flower_banner_pattern 0 0\u{1f}Свиток опыта"
        );
    }

    #[test]
    fn zones_reach_as_the_servers_have_them() {
        let feet = [10.4, 64.0, -3.6];
        // FunTime's trap: 5 by 4 by 5 round the block; the dragon's 7 by 6 by 7
        let (b, _) = reach(Zone::Cube { side: 2, up: 4 }, feet, [0.0, 0.0], false);
        assert_eq!(b[0], ([8.0, 64.0, -6.0], [13.0, 68.0, -1.0]));
        let (b, _) = reach(Zone::Cube { side: 2, up: 4 }, feet, [0.0, 0.0], true);
        assert_eq!((b[0].1[0] - b[0].0[0], b[0].1[1] - b[0].0[1]), (7.0, 6.0));
        // the plast: looking south a wall three ahead, looking up a roof
        let (b, _) = reach(Zone::Plast, feet, [0.0, 0.0], false);
        assert_eq!(b[0], ([8.0, 63.0, -2.0], [13.0, 68.0, 1.0]));
        let (b, _) = reach(Zone::Plast, feet, [0.0, -60.0], false);
        assert_eq!(b[0].0[1], 67.0);
        // a box has twelve edges; a quaternion of nothing turns nothing
        assert_eq!(edges([0.0; 3], [1.0; 3]).len(), 12);
        assert_eq!(turn([0.0, 0.0, 0.0, 1.0], [1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn keys_use_things_and_the_chat_filter_holds_back_once() {
        let mut g = Gui::new();
        let i = g.mods.iter().position(|m| m.id == MODULE).unwrap();
        g.mods[i].on = true;
        for s in g.mods[i].set.iter_mut() {
            if s.label == "Anti Fly" {
                s.kind = super::super::data::Kind::Key("G".into());
            }
        }
        // ReallyWorld (the default): its key uses the firework star sneaking; anywhere (not ReallyWorld itself)
        g.adapt_key("G");
        let mut out = Vec::new();
        g.adapt_tick(&HashMap::new(), &mut out);
        assert_eq!(out, vec!["use minecraft:firework_star 0 1".to_string()]);
        // on ReallyWorld itself: from the hotbar only
        g.server = super::super::swap::REALLYWORLD.to_string();
        g.adapt_key("G");
        let mut out = Vec::new();
        g.adapt_tick(&HashMap::new(), &mut out);
        assert_eq!(out[0], "use minecraft:firework_star 1 1");
        // a word it mutes for: held back once, sent the second time
        assert!(!g.adapt_chat("привет всем"));
        assert!(g.adapt_chat("иди на funtime"));
        assert!(!g.adapt_chat("иди на funtime"));
        // another mode: no filter
        for s in g.mods[i].set.iter_mut() {
            if let super::super::data::Kind::Choice { v, opts } = &mut s.kind {
                if s.label == "Mode" {
                    *v = opts.iter().position(|o| *o == "FunTime").unwrap();
                }
            }
        }
        assert!(!g.adapt_chat("иди на funtime"));
    }
}
