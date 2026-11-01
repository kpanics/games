//! Auto Swap's editor, in its panel (swap.rs keeps the swaps): the swaps under the module's settings; the one chosen
//! on the left, in the preview's place — its when, where to and what in order, the kinds, the inventory as Java sends
//! it ("swapinv": the things, what each swap would take now) with an arrow from the thing that would go to its place;
//! the templates. The things themselves Java draws where this says ("swapicons"), over the menu's picture: they shrink
//! into their middle as their part fades.

use super::binds::{self, Bind};
use super::cards;
use super::draw::{grey, mix, Canvas};
use super::i18n::{self, tr};
use super::swap::{Pick, Swap, FUNTIME, KINDS, PLACES, REALLYWORLD};
use super::swaptime::{self, Timing};
use super::text::{fonts, Face};
use super::theme::{self, pal};
use super::{ak, ease_out, smooth, switch, Gui, Hit, Listen, Owner, Phase};

const A_SWP: u64 = 60;
const A_CHIP: u64 = 61;
/// How many swaps there may be, and things in one's list.
pub const MAX_SWAPS: usize = 16;
pub const MAX_PICKS: usize = 8;
/// The places' words over their slots (swap.rs PLACES' order).
const PLACE_WORDS: [&str; 6] = ["Second hand", "Head slot", "Chest slot", "Legs slot", "Feet slot", "All armour"];
const TRIGGER_WORDS: [&str; 3] = ["Press", "Held", "Condition"];
const COND_WORDS: [&str; 5] = ["Health", "Durability", "Ran out", "Falling", "Danger"];
/// The number each condition compares: its least, most, step, unit, the one it starts at.
const COND_VALUES: [(f32, f32, f32, &str, f32); 5] = [
    (1.0, 10.0, 0.5, " hearts", 4.0),
    (1.0, 60.0, 1.0, "%", 15.0),
    (0.0, 0.0, 1.0, "", 0.0),
    (2.0, 30.0, 1.0, " blocks", 4.0),
    (2.0, 12.0, 0.5, " blocks", 6.0),
];
/// "All armour"'s number: a piece below it is changed (0: whenever a better one is there).
const ARMOUR_VALUE: (f32, f32, f32, &str, f32) = (0.0, 60.0, 1.0, "%", 15.0);
/// The templates: a name, what it does, the kinds its picture shows (two: one changed for the other).
const TEMPLATES: [(&str, &str, &[&str]); 8] = [
    ("Totem and shield", "A key changes the shield and the totem in the second hand", &["totem", "shield"]),
    ("Sphere to sphere", "A key goes round your spheres", &["sphere", "sphere"]),
    ("Chestplate and elytra", "A key puts on the elytra or the best chestplate", &["chestplate", "elytra"]),
    ("Elytra when falling", "After 4 blocks of falling by itself; the chestplate back on the ground", &["elytra"]),
    ("Worn armour", "A piece under 15% changed for a spare one", &["chestplate"]),
    ("Fireworks", "The second hand's ran out: the next ones from the inventory", &["firework"]),
    ("Shield near danger", "A crystal, a creeper or lit TNT near: the shield", &["shield"]),
    ("Low health", "At 4 hearts a golden apple into the second hand", &["enchanted_apple"]),
];

/// A thing of the player's as Java sent it.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Thing {
    pub id: String,
    pub name: String,
    pub ench: String,
    pub count: u32,
    /// how much of it is left (1 new; below 0 it does not wear)
    pub left: f32,
    /// the kinds it is (swap.rs KINDS' ids)
    pub kinds: Vec<String>,
    /// the places it goes to (a bit each, PLACES' order; the off hand takes anything)
    pub places: u8,
}

/// What a swap would do now, as Java sees it: the slot it would take (-1 none, -2 what it puts on is on already), the
/// slots with something of its list, "All armour"'s pieces (the place, the slot).
#[derive(Clone, PartialEq, Debug)]
pub struct Plan {
    pub from: i32,
    pub fits: Vec<usize>,
    pub pieces: Vec<(usize, usize)>,
}

impl Default for Plan {
    fn default() -> Plan {
        Plan { from: -1, fits: Vec::new(), pieces: Vec::new() }
    }
}

/// The inventory as Java sent it last: its 36 slots (0..9 the hotbar), what is worn (by place: the off hand, the head …
/// the feet), each swap's plan, whether only the hotbar is taken from.
#[derive(Clone, Default, Debug)]
pub struct Inv {
    pub slots: Vec<Option<Thing>>,
    pub worn: Vec<Option<Thing>>,
    pub plans: Vec<Plan>,
    /// where things are taken from (Java's "h"): 0 anywhere, 1 only the hotbar, 2 FunTime's (from above the hotbar only
    /// totems and spheres)
    pub reach: u8,
}

impl Inv {
    /// Whether the thing in slot `i` may be taken (the reach Java said): the hotbar's always; above it anything, nothing,
    /// or on FunTime a totem or a sphere.
    pub fn takes(&self, i: usize) -> bool {
        i < 9 || self.reach == 0 || self.reach == 2 && self.slot(i).is_some_and(|t| t.kinds.iter().any(|k| k == "totem" || k == "sphere"))
    }

    /// Java's lines, their fields by tabs: "h 1|0" only the hotbar; "s|w <slot|place> <count> <left> <kinds> <id>
    /// <name> <ench> <places>" a thing in a slot, a thing worn; "p <swap> <from> <fits> <pieces>" a swap's plan.
    pub fn read(text: &str) -> Inv {
        let mut inv = Inv { slots: vec![None; 36], worn: vec![None; 5], ..Inv::default() };
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match f[0] {
                "h" => inv.reach = f.get(1).and_then(|v| v.parse().ok()).unwrap_or(0),
                "s" | "w" if f.len() >= 8 => {
                    let Ok(i) = f[1].parse::<usize>() else { continue };
                    let t = Thing {
                        count: f[2].parse().unwrap_or(1),
                        left: f[3].parse().unwrap_or(-1.0),
                        kinds: f[4].split(',').filter(|k| !k.is_empty()).map(str::to_string).collect(),
                        id: f[5].to_string(),
                        name: f[6].to_string(),
                        ench: f[7].to_string(),
                        places: f.get(8).and_then(|p| p.parse().ok()).unwrap_or(1),
                    };
                    let list = if f[0] == "s" { &mut inv.slots } else { &mut inv.worn };
                    if let Some(x) = list.get_mut(i) {
                        *x = Some(t);
                    }
                }
                "p" if f.len() >= 5 => {
                    let Ok(i) = f[1].parse::<usize>() else { continue };
                    if i >= MAX_SWAPS * 4 {
                        continue;
                    }
                    if inv.plans.len() <= i {
                        inv.plans.resize(i + 1, Plan::default());
                    }
                    inv.plans[i] = Plan {
                        from: f[2].parse().unwrap_or(-1),
                        fits: f[3].split(',').filter_map(|s| s.parse().ok()).collect(),
                        pieces: f[4]
                            .split(',')
                            .filter_map(|s| {
                                let (a, b) = s.split_once(':')?;
                                Some((a.parse().ok()?, b.parse().ok()?))
                            })
                            .collect(),
                    };
                }
                _ => {}
            }
        }
        inv
    }

    fn slot(&self, i: usize) -> Option<&Thing> {
        self.slots.get(i).and_then(|t| t.as_ref())
    }

    fn worn(&self, p: usize) -> Option<&Thing> {
        self.worn.get(p).and_then(|t| t.as_ref())
    }

    fn plan(&self, i: usize) -> Plan {
        self.plans.get(i).cloned().unwrap_or_default()
    }
}

/// A thing Java draws over the menu: its square (layout px: x, y, side), how much it is seen (it shrinks as it fades),
/// how much it is darkened (a thing that is not taken here), the part of the window it may show in, which: "s3" a
/// slot's, "w0" what is worn (by place), "e1" a place's empty mark, "k:totem" a kind's, "p2:0" a swap's thing.
#[derive(Clone, Debug)]
pub struct Icon {
    pub at: [f32; 3],
    pub alpha: f32,
    pub dim: f32,
    pub clip: [f32; 4],
    pub what: String,
}

/// What the mouse drags in the editor: a thing of the list (which, where it was pressed, whether it moved yet), the
/// number of the condition (or of "All armour"), a delay of a swap's own timing (swaptime::SLIDERS').
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drag {
    Pick { n: usize, at: [f32; 2], moved: bool },
    Value,
    Timing(usize),
}

/// Where a thing's chip is drawn, kept by the thing itself (not its place in the list: reordered, removed or added,
/// each chip glides from where it was); `axis` 0 its left, 1 its top.
fn chip_key(p: &Pick, axis: usize) -> u64 {
    let s = match p {
        Pick::Item { id, name, ench } => format!("{id}|{name}|{ench}"),
        Pick::Kind(k) => k.clone(),
    };
    let mut h: u32 = 2_166_136_261;
    for b in s.bytes() {
        h = (h ^ b as u32).wrapping_mul(16_777_619);
    }
    ak(A_CHIP, ((h as usize) & 0x7fff_fffe) | axis)
}

fn kind_label(id: &str) -> &'static str {
    KINDS.iter().find(|k| k.0 == id).map_or("", |k| k.1)
}

fn pick_name(p: &Pick) -> String {
    match p {
        Pick::Item { name, .. } => name.clone(),
        Pick::Kind(k) => tr(kind_label(k)).to_string(),
    }
}

/// The kinds that go to a place (armour's only to its own; a sphere also on the head).
pub fn kinds_for(place: usize) -> &'static [&'static str] {
    match place {
        0 => &["totem", "talisman", "shield", "sphere", "firework", "golden_apple", "enchanted_apple", "food"],
        1 => &["sphere", "helmet", "same"],
        2 => &["elytra", "chestplate", "same"],
        3 => &["leggings", "same"],
        4 => &["boots", "same"],
        _ => &[],
    }
}

/// The number a swap's condition (or "All armour") compares: least, most, step, unit, the one it starts at.
fn range_of(s: &Swap) -> (f32, f32, f32, &'static str, f32) {
    if s.place == 5 {
        ARMOUR_VALUE
    } else {
        COND_VALUES[s.cond.min(4)]
    }
}

fn value_text(range: (f32, f32, f32, &str, f32), v: f32) -> String {
    let d = if range.2 < 1.0 && v.fract().abs() > 1e-3 { 1 } else { 0 };
    format!("{:.*}{}", d, v, i18n::unit(range.3))
}

fn first_upper(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().chain(ch).collect(),
        None => String::new(),
    }
}

fn first_lower(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_lowercase().chain(ch).collect(),
        None => String::new(),
    }
}

/// A bin: its lid and its body.
fn trash_icon(c: &mut Canvas, cc: [f32; 2], s: f32, col: super::draw::Rgba) {
    c.line([cc[0] - s, cc[1] - 0.62 * s], [cc[0] + s, cc[1] - 0.62 * s], 1.4, col, 0.0);
    c.line([cc[0] - 0.3 * s, cc[1] - 0.95 * s], [cc[0] + 0.3 * s, cc[1] - 0.95 * s], 1.4, col, 0.0);
    c.polyline(&[[cc[0] - 0.72 * s, cc[1] - 0.45 * s], [cc[0] - 0.55 * s, cc[1] + s], [cc[0] + 0.55 * s, cc[1] + s], [cc[0] + 0.72 * s, cc[1] - 0.45 * s]], 1.4, col);
}

/// Two sheets, one over the other.
fn copy_icon(c: &mut Canvas, cc: [f32; 2], s: f32, col: super::draw::Rgba) {
    c.polyline(&[[cc[0] - s, cc[1] + 0.35 * s], [cc[0] - s, cc[1] - s], [cc[0] + 0.35 * s, cc[1] - s]], 1.4, col);
    c.polyline(&[[cc[0] - 0.55 * s, cc[1] - 0.55 * s], [cc[0] + s, cc[1] - 0.55 * s], [cc[0] + s, cc[1] + s], [cc[0] - 0.55 * s, cc[1] + s], [cc[0] - 0.55 * s, cc[1] - 0.55 * s]], 1.4, col);
}

/// A cross (a thing out of the list, the templates closed).
fn cross(c: &mut Canvas, cc: [f32; 2], s: f32, col: super::draw::Rgba) {
    c.line([cc[0] - s, cc[1] - s], [cc[0] + s, cc[1] + s], 1.4, col, 0.0);
    c.line([cc[0] + s, cc[1] - s], [cc[0] - s, cc[1] + s], 1.4, col, 0.0);
}

/// Two arrows round each other: "the same as worn", a thing changed for another.
fn round_arrows(c: &mut Canvas, cc: [f32; 2], s: f32, col: super::draw::Rgba) {
    c.arrow([cc[0] - s, cc[1] - 0.35 * s], [cc[0] + s, cc[1] - 0.35 * s], 1.4, 0.4 * s, col);
    c.arrow([cc[0] + s, cc[1] + 0.35 * s], [cc[0] - s, cc[1] + 0.35 * s], 1.4, 0.4 * s, col);
}

fn caption(c: &mut Canvas, x: f32, y: f32, r: f32, s: &str) {
    c.text(x, y + 0.7 * r, 0.66 * r, Face::Semi, s, pal().t4);
}

/// A point of the curve from `a` to `b` that leaves `a` and comes into `b` upright.
fn curve(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    let k = ((a[1] - b[1]).abs() * 0.55).max(12.0);
    let c1 = [a[0], a[1] - k];
    let c2 = [b[0], b[1] + k];
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    [w[0] * a[0] + w[1] * c1[0] + w[2] * c2[0] + w[3] * b[0], w[0] * a[1] + w[1] * c1[1] + w[2] * c2[1] + w[3] * b[1]]
}

impl Gui {
    /// A thing for Java to draw in `r`'s middle (a square), as seen as what is drawn now.
    fn swap_icon(&mut self, c: &Canvas, r: [f32; 4], what: String) {
        self.swap_icon_dim(c.alpha, c.clip_now(), r, what, 0.0);
    }

    fn swap_icon_dim(&mut self, alpha: f32, clip: [f32; 4], r: [f32; 4], what: String, dim: f32) {
        if alpha <= 0.02 {
            return;
        }
        let s = (r[2] - r[0]).min(r[3] - r[1]);
        let at = [(r[0] + r[2] - s) / 2.0, (r[1] + r[3] - s) / 2.0, s];
        self.swap_icons.push(Icon { at, alpha, dim, clip, what });
    }

    /// The things to draw now, a line each for Java: x, y, side, alpha, how dark, the part that may show (x0 y0 x1
    /// y1), which (window px: as the window is drawn now; nothing until it is open, nothing under a card, the word
    /// over the menu, or the chip being dragged).
    pub(super) fn swap_icons_text(&self) -> String {
        if self.phase == Phase::Closed || self.look.open <= 0.97 {
            return String::new();
        }
        let [mx, my, sc, drop, wa] = self.xf;
        let t = |x: f32, y: f32| (mx + (x - mx) * sc, my + (y - my) * sc + drop);
        let over = |ic: &Icon, q: [f32; 4]| ic.at[0] < q[2] && ic.at[0] + ic.at[2] > q[0] && ic.at[1] < q[3] && ic.at[1] + ic.at[2] > q[1];
        let card = self.card.as_ref().map(|_| self.card_rect);
        let toast = self.toast_rect;
        let mut out = String::new();
        for (k, ic) in self.swap_icons.iter().enumerate() {
            let a = ic.alpha * wa;
            if a < 0.02 || card.is_some_and(|cr| over(ic, cr)) || (toast[2] > toast[0] && over(ic, toast)) {
                continue;
            }
            if self.swap_lift.is_some_and(|(q, li)| k < li && over(ic, q)) {
                continue;
            }
            let (x, y) = t(ic.at[0], ic.at[1]);
            let (c0x, c0y) = t(ic.clip[0], ic.clip[1]);
            let (c1x, c1y) = t(ic.clip[2], ic.clip[3]);
            out.push_str(&format!("{x:.1} {y:.1} {:.1} {a:.3} {:.2} {c0x:.1} {c0y:.1} {c1x:.1} {c1y:.1} {}\n", ic.at[2] * sc, ic.dim, ic.what));
        }
        out
    }

    fn conds_here(&self) -> bool {
        self.server != FUNTIME && self.server != REALLYWORLD
    }

    /// When a swap goes, as said in its row and its sentence.
    fn swap_when_text(&self, i: usize, s: &Swap) -> String {
        let key = || {
            if self.listen == Some(Listen::Swap(i)) {
                self.rec_label()
            } else {
                s.bind.as_ref().map_or_else(|| tr("no key").to_string(), |b| b.label())
            }
        };
        match s.trigger {
            0 => key(),
            1 => i18n::trn("holding {key}", &[("key", key())]),
            // "All armour" at 0: whenever a better piece is there
            _ if s.place == 5 && s.value <= 0.0 => tr("a better one is there").to_string(),
            _ => {
                let cw = tr(COND_WORDS[s.cond.min(4)]);
                let v = value_text(range_of(s), s.value);
                match s.cond {
                    0 => format!("{cw} ≤ {v}"),
                    1 => format!("{cw} < {v}"),
                    3 => format!("{cw} ≥ {v}"),
                    4 => format!("{cw} ≤ {v}"),
                    _ => cw.to_string(),
                }
            }
        }
    }

    /// What a swap puts on, as said: its first things' names.
    fn swap_what_text(s: &Swap) -> String {
        if s.place == 5 {
            return tr("the best spare pieces").to_string();
        }
        if s.picks.is_empty() {
            return tr("nothing chosen yet").to_string();
        }
        let names: Vec<String> = s.picks.iter().take(2).map(pick_name).collect();
        let more = s.picks.len().saturating_sub(2);
        if more > 0 {
            format!("{} +{}", names.join(", "), more)
        } else {
            names.join(", ")
        }
    }

    fn swap_sentence(&self, i: usize, s: &Swap) -> String {
        first_upper(&format!("{} → {} → {}", self.swap_when_text(i, s), Self::swap_what_text(s), first_lower(tr(PLACES[s.place.min(5)]))))
    }

    // ------------------------------------------------------------------ the list, under the module's settings
    pub(super) fn swap_list_height(&self) -> f32 {
        let r = self.r;
        1.6 * r + 2.25 * r + self.swaps.len().max(1) as f32 * 3.15 * r + 0.4 * r
    }

    /// Every swap a row (its thing, what it puts on, when, where; its switch), and the buttons for a new one; `y` its
    /// top. A row chosen is the one the editor shows.
    pub(super) fn swap_list(&mut self, c: &mut Canvas, x0: f32, x1: f32, y: f32, rise: impl Fn(f32) -> (f32, f32)) {
        let r = self.r;
        let base = c.alpha;
        let (a, dy) = rise(0.3);
        c.alpha = base * a;
        let yy = y + 0.6 * r + dy;
        caption(c, x0, yy, r, &i18n::trn("Swaps · {n}", &[("n", self.swaps.len().to_string())]));
        // a new one, the templates
        let bh = 1.75 * r;
        let by = yy + 1.25 * r;
        let lw = fonts().width(Face::Medium, 0.72 * r, tr("New swap")) + 2.4 * r;
        let tw = fonts().width(Face::Medium, 0.72 * r, tr("Templates")) + 1.6 * r;
        let nb = [x0, by, x0 + lw, by + bh];
        let tb = [nb[2] + 0.4 * r, by, nb[2] + 0.4 * r + tw, by + bh];
        let full = self.swaps.len() >= MAX_SWAPS;
        for (b, hit, word, k) in [(nb, Hit::Swap(2, 0), "New swap", 0usize), (tb, Hit::Swap(3, 0), "Templates", 1)] {
            let hv = self.an(ak(A_SWP, 20 + k), if self.hover == hit || (k == 1 && self.swap_tpl) { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.alpha = base * a * if full && k == 0 { 0.4 } else { 1.0 };
            c.rect(b, 0.55 * r, mix(pal().f2, pal().f4, hv));
            let mid = (b[1] + b[3]) / 2.0;
            if k == 0 {
                let pc = [b[0] + 0.95 * r, mid];
                let col = mix(pal().t2, pal().t1, hv);
                c.line([pc[0] - 0.32 * r, pc[1]], [pc[0] + 0.32 * r, pc[1]], 1.5, col, 0.0);
                c.line([pc[0], pc[1] - 0.32 * r], [pc[0], pc[1] + 0.32 * r], 1.5, col, 0.0);
                c.text(b[0] + 1.65 * r, mid + 0.26 * r, 0.72 * r, Face::Medium, tr(word), col);
            } else {
                c.text_c((b[0] + b[2]) / 2.0, mid + 0.26 * r, 0.72 * r, Face::Medium, tr(word), mix(pal().t2, pal().t1, hv));
            }
            self.hits.push((b, hit));
        }
        c.alpha = base * a;
        let mut ry = by + bh + 0.5 * r;
        if self.swaps.is_empty() {
            let mut ly = ry + 1.2 * r;
            for line in fonts().wrap(Face::Regular, 0.72 * r, tr("None yet: a template on the left is the quickest start"), x1 - x0) {
                c.text(x0, ly, 0.72 * r, Face::Regular, &line, pal().t4);
                ly += 1.1 * r;
            }
        }
        let sel = self.swap_sel.min(self.swaps.len().saturating_sub(1));
        let list = self.swaps.clone();
        for (i, s) in list.iter().enumerate() {
            let (a, dy) = rise(0.33 + (i as f32 * 0.03).min(0.3));
            c.alpha = base * a;
            let rr = [x0 - 0.35 * r, ry + dy, x1 + 0.35 * r, ry + dy + 2.95 * r];
            let mid = (rr[1] + rr[3]) / 2.0;
            let hit = Hit::Swap(0, i as u16);
            let chosen = i == sel && !self.swap_tpl;
            let hv = self.an(ak(A_SWP, 100 + i), if self.hover == hit || self.hover == Hit::Swap(1, i as u16) { 1.0 } else { 0.0 }, 12.0, 0.0);
            let cv = self.an(ak(A_SWP, 130 + i), if chosen { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect(rr, 0.6 * r, grey(1.0, 0.035 * hv));
            if cv > 0.01 {
                let ba = c.alpha;
                c.alpha = ba * cv;
                c.rect_lit(rr, 0.6 * r, grey(1.0, 0.11), grey(1.0, 0.035), 1.0);
                c.line([rr[0] + 0.33 * r, rr[1] + 0.75 * r], [rr[0] + 0.33 * r, rr[3] - 0.75 * r], 2.0, pal().accent, 5.0);
                c.alpha = ba;
            }
            self.hits.push((rr, hit));
            let ba = c.alpha;
            if !s.on {
                c.alpha = ba * 0.5;
            }
            // its first thing (a place's mark when it has none)
            let ib = [x0 + 0.45 * r, mid - 0.85 * r, x0 + 2.15 * r, mid + 0.85 * r];
            let what = if s.place == 5 {
                if self.swap_inv.worn(2).is_some() { "w2".to_string() } else { "e2".to_string() }
            } else if s.picks.is_empty() {
                format!("e{}", s.place)
            } else {
                format!("p{i}:0")
            };
            // (an off one's thing darkened: its alpha would shrink it)
            self.swap_icon_dim(ba, c.clip_now(), ib, what, if s.on { 0.0 } else { 0.5 });
            let tx = x0 + 2.75 * r;
            let room = x1 - 2.6 * r - tx;
            let l1 = fonts().fit(Face::Medium, 0.78 * r, &first_upper(&Self::swap_what_text(s)), room);
            c.text(tx, mid - 0.12 * r, 0.78 * r, Face::Medium, &l1, if chosen { pal().t1 } else { mix(pal().t2, pal().t1, hv) });
            let l2 = first_upper(&format!("{} · {}", self.swap_when_text(i, s), first_lower(tr(PLACES[s.place.min(5)]))));
            let l2 = fonts().fit(Face::Regular, 0.64 * r, &l2, room);
            c.text(tx, mid + 0.92 * r, 0.64 * r, Face::Regular, &l2, pal().t3);
            c.alpha = ba;
            let sw = [x1 - 2.02 * r, mid - 0.6 * r, x1, mid + 0.6 * r];
            let onv = self.an(ak(A_SWP, 160 + i), if s.on { 1.0 } else { 0.0 }, 10.0, if s.on { 1.0 } else { 0.0 });
            let shv = self.an(ak(A_SWP, 190 + i), if self.hover == Hit::Swap(1, i as u16) { 1.0 } else { 0.0 }, 12.0, 0.0);
            switch(c, sw, onv, shv, true);
            self.hits.push(([sw[0] - 0.3 * r, rr[1], rr[2], rr[3]], Hit::Swap(1, i as u16)));
            ry += 3.15 * r;
        }
        c.alpha = base;
    }

    // ------------------------------------------------------------------ the editor, in the preview's place
    /// The chosen swap (or the templates, when asked for or there is none) in `st`; how tall it needs to be is kept for
    /// the panel's height.
    pub(super) fn swap_editor(&mut self, c: &mut Canvas, st: [f32; 4]) {
        let r = self.r;
        let opened = self.now - self.open_at;
        // the panel opening, and another swap chosen (the templates shown, one added or deleted): each part rises in
        // its turn
        let since = self.now - self.swap_shown_at;
        let rise = move |d: f32| -> (f32, f32) {
            let k = ((opened - d) / 0.4).clamp(0.0, 1.0);
            let q = ((since - d * 0.5) / 0.3).clamp(0.0, 1.0);
            (smooth(0.0, 1.0, k) * smooth(0.0, 1.0, q), (1.0 - ease_out(k)) * 0.35 * r + (1.0 - ease_out(q)) * 0.3 * r)
        };
        let base = c.alpha;
        c.rect(st, 0.75 * r, grey(0.0, 0.16));
        self.hits.push((st, Hit::Swap(18, 0)));
        c.push_clip(st);
        let (x0, x1) = (st[0] + 1.2 * r, st[2] - 1.2 * r);
        let top = st[1] + 1.1 * r;
        let end = if self.swaps.is_empty() || self.swap_tpl {
            self.swap_templates(c, x0, x1, top, rise)
        } else {
            self.swap_sel = self.swap_sel.min(self.swaps.len() - 1);
            self.swap_one(c, x0, x1, top, rise)
        };
        c.pop_clip();
        c.alpha = base;
        self.swap_need = end - st[1] + 2.0 * r;
    }

    /// Words side by side, one of them chosen (its plate glides to it); `off` those that do nothing here (dim). Its
    /// right edge.
    #[allow(clippy::too_many_arguments)]
    fn swap_seg(&mut self, c: &mut Canvas, x: f32, y: f32, opts: &[&str], cur: usize, part: u8, key: usize, off: &[bool]) -> f32 {
        let r = self.r;
        let size = 0.74 * r;
        let pad = 0.66 * r;
        let sh = 1.9 * r;
        let ws: Vec<f32> = opts.iter().map(|o| fonts().width(Face::Medium, size, tr(o)) + 2.0 * pad).collect();
        let total = ws.iter().sum::<f32>() + 0.36 * r;
        c.rect([x, y, x + total, y + sh], 0.6 * r, pal().f1);
        let cur = cur.min(opts.len() - 1);
        let cx = x + 0.18 * r + ws[..cur].iter().sum::<f32>();
        let tx = self.an(ak(A_SWP, key), cx, 16.0, cx);
        let tw = self.an(ak(A_SWP, key + 1), ws[cur], 16.0, ws[cur]);
        c.rect([tx, y + 0.18 * r, tx + tw, y + sh - 0.18 * r], 0.44 * r, pal().f4);
        let base = c.alpha;
        let mut ox = x + 0.18 * r;
        for (i, o) in opts.iter().enumerate() {
            let hit = Hit::Swap(part, i as u16);
            let hv = self.an(ak(A_SWP, key + 2 + i), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.alpha = base * if off.get(i).copied().unwrap_or(false) { 0.4 } else { 1.0 };
            let col = if i == cur { pal().t1 } else { mix(pal().t3, pal().t2, hv) };
            c.text_c(ox + ws[i] / 2.0, y + sh / 2.0 + 0.27 * r, size, Face::Medium, tr(o), col);
            self.hits.push(([ox, y, ox + ws[i], y + sh], hit));
            ox += ws[i];
        }
        c.alpha = base;
        x + total
    }

    /// A track from `x` to `x2` at `mid`, filled to `k` (0..1); dragged, it sets the swap's number.
    fn swap_slider(&mut self, c: &mut Canvas, x: f32, x2: f32, mid: f32, k: f32) {
        let r = self.r;
        let dragged = self.swap_drag == Some(Drag::Value);
        let f = self.an(ak(A_SWP, 60), k, if dragged { 40.0 } else { 9.0 }, k);
        let t = [x, mid - 0.15 * r, x2, mid + 0.15 * r];
        c.rect(t, r, grey(1.0, 0.1));
        c.rect_lit([t[0], t[1], t[0] + (t[2] - t[0]) * f, t[3]], r, theme::fade(pal().accent, 0.45), pal().accent, 1.0);
        let hit = [x - 0.5 * r, mid - 0.9 * r, x2 + 0.5 * r, mid + 0.9 * r];
        let big = if dragged || self.hover == Hit::Swap(11, 0) { 1.22 } else { 1.0 };
        let th = [t[0] + (t[2] - t[0]) * f, mid];
        let q = 0.46 * r * big;
        c.shadow([th[0] - q, th[1] - q, th[0] + q, th[1] + q], q, 0.4 * r, grey(0.0, 0.5));
        c.rect([th[0] - q, th[1] - q, th[0] + q, th[1] + q], q, pal().accent);
        self.hits.push((hit, Hit::Swap(11, 0)));
        self.swap_track = [x, x2];
    }

    /// The swap chosen: its name and sentence, when, where to (the places as slots), the inventory (the arrow from the
    /// thing that would go), what in order, the kinds. Where it ends.
    fn swap_one(&mut self, c: &mut Canvas, x0: f32, x1: f32, mut y: f32, rise: impl Fn(f32) -> (f32, f32)) -> f32 {
        let r = self.r;
        let base = c.alpha;
        let si = self.swap_sel;
        let s = self.swaps[si].clone();
        let plan = self.swap_inv.plan(si);
        let here = self.conds_here();

        // ---- its name and sentence; on, a copy, out
        let (a, dy) = rise(0.1);
        c.alpha = base * a;
        let yy = y + dy;
        let bw = 1.9 * r;
        let armed = self.swap_arm.is_some_and(|t| self.now - t < 3.0);
        let dhv = self.an(ak(A_SWP, 10), if self.hover == Hit::Swap(6, 0) || armed { 1.0 } else { 0.0 }, 12.0, 0.0);
        let label = tr("Delete?");
        let lw = fonts().width(Face::Medium, 0.7 * r, label) + 2.5 * r;
        let w = self.an(ak(A_SWP, 11), if armed { lw } else { bw }, 14.0, bw);
        let del = [x1 - w, yy, x1, yy + bw];
        c.rect(del, 0.6 * r, mix(pal().f1, pal().f3, dhv));
        trash_icon(c, [del[0] + bw / 2.0, yy + bw / 2.0], 0.34 * r, mix(pal().t3, pal().t1, dhv));
        if w > bw + 0.3 * r {
            c.push_clip(del);
            c.text(del[0] + 1.75 * r, yy + 1.22 * r, 0.7 * r, Face::Medium, label, pal().t1);
            c.pop_clip();
        }
        self.hits.push((del, Hit::Swap(6, 0)));
        let cb = [del[0] - 0.4 * r - bw, yy, del[0] - 0.4 * r, yy + bw];
        let chv = self.an(ak(A_SWP, 12), if self.hover == Hit::Swap(7, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
        let full = self.swaps.len() >= MAX_SWAPS;
        let ca = c.alpha;
        if full {
            c.alpha = ca * 0.4;
        }
        c.rect(cb, 0.6 * r, mix(pal().f1, pal().f3, chv));
        copy_icon(c, [(cb[0] + cb[2]) / 2.0, (cb[1] + cb[3]) / 2.0], 0.36 * r, mix(pal().t3, pal().t1, chv));
        c.alpha = ca;
        self.hits.push((cb, Hit::Swap(7, 0)));
        let sw = [cb[0] - 0.65 * r - 2.3 * r, yy + 0.3 * r, cb[0] - 0.65 * r, yy + 1.66 * r];
        let onv = self.an(ak(A_SWP, 13), if s.on { 1.0 } else { 0.0 }, 10.0, if s.on { 1.0 } else { 0.0 });
        let shv = self.an(ak(A_SWP, 14), if self.hover == Hit::Swap(5, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
        switch(c, sw, onv, shv, false);
        self.hits.push((sw, Hit::Swap(5, 0)));
        let title = i18n::trn("Swap {n}", &[("n", (si + 1).to_string())]);
        c.text(x0, yy + 1.4 * r, 1.12 * r, Face::Bold, &fonts().fit(Face::Bold, 1.12 * r, &title, sw[0] - x0 - 0.6 * r), pal().t1);
        let sentence = fonts().fit(Face::Regular, 0.74 * r, &self.swap_sentence(si, &s), x1 - x0);
        c.text(x0, yy + 2.75 * r, 0.74 * r, Face::Regular, &sentence, pal().t3);
        y += 3.6 * r;

        // ---- when: a key pressed, held, or a condition
        let (a, dy) = rise(0.14);
        c.alpha = base * a;
        let yy = y + dy;
        caption(c, x0, yy, r, tr("When"));
        let sy = yy + 1.15 * r;
        let right = self.swap_seg(c, x0, sy, &TRIGGER_WORDS, s.trigger, 8, 30, &[false, false, !here]);
        if s.trigger < 2 {
            let wait = self.listen == Some(Listen::Swap(si));
            let label = if wait { self.rec_label() } else { s.bind.as_ref().map_or_else(|| tr("Set a key").to_string(), |b| b.label()) };
            let face = if s.bind.is_some() || wait { Face::Mono } else { Face::Medium };
            let kw = fonts().width(face, 0.7 * r, &label) + 2.6 * r;
            let kb = [right + 0.6 * r, sy, (right + 0.6 * r + kw).min(x1), sy + 1.9 * r];
            let khv = self.an(ak(A_SWP, 15), if self.hover == Hit::Swap(9, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
            let typing = self.since(ak(A_SWP, 16), self.hover == Hit::Swap(9, 0));
            let pulse = if wait { 0.5 + 0.5 * (self.now * 7.0).sin() } else { 0.0 };
            let unset = s.bind.is_none() && !wait;
            c.rect(kb, 0.6 * r, mix(mix(if unset { pal().f2 } else { pal().f1 }, pal().f3, khv), grey(1.0, 0.22), pulse));
            let mid = (kb[1] + kb[3]) / 2.0;
            let col = if unset { mix(pal().t2, pal().t1, khv) } else { pal().t1 };
            cards::key_icon(c, [kb[0] + 1.0 * r, mid], 0.95 * r, typing, col);
            c.push_clip(kb);
            c.text(kb[0] + 1.8 * r, mid + 0.25 * r, 0.7 * r, face, &label, col);
            c.pop_clip();
            self.hits.push((kb, Hit::Swap(9, 0)));
        }
        y += 3.25 * r;
        if s.trigger == 2 {
            let (a, dy) = rise(0.17);
            c.alpha = base * a;
            let yy = y + dy;
            // "All armour" goes by its wear only
            let off: Vec<bool> = (0..5).map(|k| s.place == 5 && k != 1).collect();
            self.swap_seg(c, x0, yy, &COND_WORDS, s.cond, 10, 40, &off);
            y += 2.45 * r;
            if s.cond != 2 && s.place != 5 {
                let yy = y + dy;
                let mid = yy + 0.95 * r;
                let range = range_of(&s);
                let vt = value_text(range, s.value);
                let sign = match s.cond {
                    1 => "<",
                    3 => "≥",
                    _ => "≤",
                };
                let vw = fonts().width(Face::Mono, 0.74 * r, &format!("{sign} {vt}")).max(5.6 * r);
                c.text(x0, mid + 0.27 * r, 0.74 * r, Face::Mono, &format!("{sign} {vt}"), pal().t1);
                let backable = matches!(s.cond, 0 | 3 | 4);
                let bl = tr("Back when it passes");
                let bwid = if backable { fonts().width(Face::Regular, 0.74 * r, bl) + 3.2 * r } else { 0.0 };
                let t0 = x0 + vw + 0.9 * r;
                let t1 = (x1 - bwid - 1.2 * r).max(t0 + 4.0 * r);
                let k = ((s.value - range.0) / (range.1 - range.0)).clamp(0.0, 1.0);
                self.swap_slider(c, t0, t1, mid, k);
                if backable {
                    let hit = Hit::Swap(12, 0);
                    let bhv = self.an(ak(A_SWP, 17), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
                    let bon = self.an(ak(A_SWP, 18), if s.back { 1.0 } else { 0.0 }, 10.0, if s.back { 1.0 } else { 0.0 });
                    c.text(x1 - bwid + 0.2 * r, mid + 0.27 * r, 0.74 * r, Face::Regular, bl, mix(pal().t2, pal().t1, bhv));
                    switch(c, [x1 - 2.02 * r, mid - 0.6 * r, x1, mid + 0.6 * r], bon, bhv, true);
                    self.hits.push(([x1 - bwid, mid - 0.95 * r, x1, mid + 0.95 * r], hit));
                }
                y += 2.1 * r;
            }
        }
        // what it does, and the server's rules
        let hint = match (s.trigger, s.cond) {
            (0, _) => "Each press puts on the next of the list, round",
            (1, _) => "On while the key is held; let go, what was on comes back",
            (_, 0) => "At this health or lower; a heart more and it goes back",
            (_, 1) => "What is worn below this is changed for a less worn one",
            (_, 2) => "The place emptied (the last one used up): the next from the inventory",
            (_, 3) => "After this many blocks of falling; on the ground or in water it goes back",
            _ => "An end crystal, a creeper or lit TNT this near",
        };
        let (a, dy) = rise(0.19);
        c.alpha = base * a;
        for line in fonts().wrap(Face::Regular, 0.68 * r, tr(hint), x1 - x0) {
            c.text(x0, y + dy + 0.5 * r, 0.68 * r, Face::Regular, &line, pal().t4);
            y += 1.05 * r;
        }
        let mut notes: Vec<String> = Vec::new();
        if self.server == FUNTIME {
            let pause = self.slider_value(Owner::Module(self.mod_index(super::swap::MODULE)), swaptime::L_FT);
            notes.push(if pause > 0.0 {
                i18n::trn("FunTime: only on a key; from the hotbar at once, from the inventory only totems and spheres, {n} ms after the key", &[("n", format!("{pause:.0}"))])
            } else {
                tr("FunTime: only on a key; from the inventory only totems and spheres").to_string()
            });
        } else if self.server == REALLYWORLD {
            notes.push(tr("ReallyWorld: only on a key and only from the hotbar (their rule)").to_string());
        }
        if s.trigger == 2 && !super::swap::may_go_by_itself(&s, "") {
            notes.push(tr("A totem never goes by itself: only on a key").to_string());
        }
        for n in notes {
            let ny = y + dy + 0.35 * r;
            c.rect([x0, ny - 0.05 * r, x0 + 0.22 * r, ny + 0.75 * r], 0.1 * r, pal().accent);
            for line in fonts().wrap(Face::Medium, 0.68 * r, &n, x1 - x0 - 0.7 * r) {
                c.text(x0 + 0.6 * r, y + dy + 0.95 * r, 0.68 * r, Face::Medium, &line, pal().t2);
                y += 1.05 * r;
            }
            y += 0.2 * r;
        }
        y += 0.5 * r;

        // ---- where to: the places as slots, what is on each in it
        let (a, dy) = rise(0.22);
        c.alpha = base * a;
        let yy = y + dy;
        caption(c, x0, yy, r, tr("Where to"));
        let gap = 0.2 * r;
        let slot = ((x1 - x0 - 8.0 * gap) / 9.0).min(2.5 * r);
        let gw = 9.0 * slot + 8.0 * gap;
        let gx = x0 + (x1 - x0 - gw) / 2.0;
        // the places a little wider apart than the grid's columns: their words fit
        let cell = ((x1 - x0) / 6.0).min(5.2 * r).max(gw / 6.0);
        let tx0 = (x0 + x1) / 2.0 - 3.0 * cell;
        let ty = yy + 2.25 * r;
        let mut targets = [[0.0f32; 4]; 6];
        for (p, target) in targets.iter_mut().enumerate() {
            let cx = tx0 + cell * (p as f32 + 0.5);
            let t = [cx - slot / 2.0, ty, cx + slot / 2.0, ty + slot];
            *target = t;
            let hit = Hit::Swap(13, p as u16);
            let chosen = s.place == p;
            let hv = self.an(ak(A_SWP, 240 + p), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
            let sv = self.an(ak(A_SWP, 250 + p), if chosen { 1.0 } else { 0.0 }, 12.0, 0.0);
            let word = fonts().fit(Face::Medium, 0.62 * r, tr(PLACE_WORDS[p]), cell - 0.15 * r);
            c.text_c(cx, ty - 0.5 * r, 0.62 * r, Face::Medium, &word, mix(mix(pal().t4, pal().t2, hv), pal().t1, sv));
            if sv > 0.01 {
                c.glow([cx, ty + slot / 2.0], slot * 0.95, theme::fade(pal().accent, 0.16 * sv));
            }
            c.rect(t, 0.5 * r, mix(mix(pal().f1, pal().f2, hv), mix(pal().f3, pal().f4, hv), sv));
            // the chosen one's mark under it
            let mw = 0.7 * r * sv;
            if mw > 0.05 {
                c.rect([cx - mw, t[3] + 0.3 * r, cx + mw, t[3] + 0.45 * r], r, pal().accent);
            }
            self.hits.push(([t[0] - 0.5 * r, ty - 1.2 * r, t[2] + 0.5 * r, t[3] + 0.6 * r], hit));
            if p < 5 {
                let inner = slot * 0.68;
                let ib = [cx - inner / 2.0, ty + (slot - inner) / 2.0, cx + inner / 2.0, ty + (slot + inner) / 2.0];
                let what = if self.swap_inv.worn(p).is_some() { format!("w{p}") } else { format!("e{p}") };
                self.swap_icon(c, ib, what);
            } else {
                // every piece, small
                let q = slot * 0.36;
                for k in 1..=4usize {
                    let (ox, oy) = (((k - 1) % 2) as f32, ((k - 1) / 2) as f32);
                    let qx = cx - q - 0.04 * r + ox * (q + 0.08 * r);
                    let qy = ty + slot / 2.0 - q - 0.04 * r + oy * (q + 0.08 * r);
                    let what = if self.swap_inv.worn(k).is_some() { format!("w{k}") } else { format!("e{k}") };
                    self.swap_icon(c, [qx, qy, qx + q, qy + q], what);
                }
            }
            // what it puts on is on already
            if chosen && plan.from == -2 {
                let dc = [t[2] - 0.15 * r, t[1] + 0.15 * r];
                c.rect([dc[0] - 0.42 * r, dc[1] - 0.42 * r, dc[0] + 0.42 * r, dc[1] + 0.42 * r], 0.42 * r, pal().accent);
                c.polyline(&[[dc[0] - 0.2 * r, dc[1]], [dc[0] - 0.04 * r, dc[1] + 0.16 * r], [dc[0] + 0.22 * r, dc[1] - 0.16 * r]], 1.4, grey(0.05, 1.0));
            }
        }
        y = ty + slot + 0.9 * r;

        // ---- the inventory: a press takes the very thing, the right button any of its kind (no words over it: the
        // arrows come up through there)
        let (a, dy) = rise(0.26);
        c.alpha = base * a;
        let gy = y + dy + 0.35 * r;
        let row_y = |row: usize| gy + row as f32 * (slot + gap) + if row == 3 { 0.45 * r } else { 0.0 };
        let mut rects = [[0.0f32; 4]; 36];
        for (i, rect) in rects.iter_mut().enumerate() {
            let (row, col) = if i < 9 { (3, i) } else { ((i - 9) / 9, (i - 9) % 9) };
            let sx = gx + col as f32 * (slot + gap);
            let sy = row_y(row);
            *rect = [sx, sy, sx + slot, sy + slot];
        }
        let item_rank = |t: &Thing| {
            s.picks.iter().position(|p| matches!(p, Pick::Item { id, name, ench } if *id == t.id && *name == t.name && *ench == t.ench))
        };
        let slot_a = c.alpha;
        for (i, rr) in rects.iter().enumerate() {
            let thing = self.swap_inv.slot(i).cloned();
            let hit = Hit::Swap(14, i as u16);
            let hv = self.an(ak(A_SWP, 300 + i), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
            let from = plan.from == i as i32 || plan.pieces.iter().any(|q| q.1 == i);
            let fv = self.an(ak(A_SWP, 340 + i), if from { 1.0 } else { 0.0 }, 10.0, 0.0);
            let fits = plan.fits.contains(&i);
            // not taken from here (only the hotbar is, or on FunTime a totem or a sphere): dim, the thing darkened (not
            // shrunk: that is its fading)
            let dim = !self.swap_inv.takes(i);
            c.alpha = slot_a * if dim { 0.4 } else { 1.0 };
            let mid = [(rr[0] + rr[2]) / 2.0, (rr[1] + rr[3]) / 2.0];
            if fv > 0.01 {
                c.glow(mid, slot * 0.9, theme::fade(pal().accent, 0.2 * fv));
            }
            let lit = fv.max(if fits { 0.3 } else { 0.0 });
            c.rect(*rr, 0.42 * r, mix(mix(pal().f1, pal().f2, hv), mix(pal().f3, pal().f4, hv), lit));
            self.hits.push((*rr, hit));
            let Some(t) = thing else { continue };
            let inner = slot * 0.68;
            let ir = [mid[0] - inner / 2.0, mid[1] - inner / 2.0, mid[0] + inner / 2.0, mid[1] + inner / 2.0];
            self.swap_icon_dim(slot_a, c.clip_now(), ir, format!("s{i}"), if dim { 0.6 } else { 0.0 });
            if t.count > 1 {
                c.text_r(rr[2] - 0.14 * r, rr[3] - 0.14 * r, 0.54 * r, Face::Semi, &t.count.to_string(), pal().t1);
            }
            if t.left >= 0.0 && t.left < 0.999 {
                let bx = [rr[0] + 0.3 * r, rr[3] - 0.3 * r, rr[2] - 0.3 * r, rr[3] - 0.18 * r];
                c.rect(bx, r, grey(1.0, 0.12));
                c.rect([bx[0], bx[1], bx[0] + (bx[2] - bx[0]) * t.left.clamp(0.0, 1.0), bx[3]], r, grey(1.0, 0.4 + 0.5 * t.left));
            }
            // its place in the list, when it is there itself
            if let Some(n) = item_rank(&t) {
                c.rect([rr[0] + 0.12 * r, rr[1] + 0.12 * r, rr[0] + 0.82 * r, rr[1] + 0.82 * r], 0.35 * r, pal().accent);
                c.text_c(rr[0] + 0.47 * r, rr[1] + 0.68 * r, 0.5 * r, Face::Bold, &(n + 1).to_string(), grey(0.05, 1.0));
            } else if fits && !from {
                c.rect([rr[2] - 0.5 * r, rr[1] + 0.26 * r, rr[2] - 0.26 * r, rr[1] + 0.5 * r], r, grey(1.0, 0.55));
            }
        }
        c.alpha = slot_a;
        // the arrows: from what would go to its place (each piece of "All armour" to its own)
        let arrows: Vec<(usize, usize)> = if s.place == 5 {
            plan.pieces.clone()
        } else if plan.from >= 0 {
            vec![(s.place, plan.from as usize)]
        } else {
            Vec::new()
        };
        let av = self.an(ak(A_SWP, 61), if arrows.is_empty() { 0.0 } else { 1.0 }, 8.0, 0.0);
        for (p, i) in arrows {
            if i >= 36 || p > 4 {
                continue;
            }
            let from = [(rects[i][0] + rects[i][2]) / 2.0, rects[i][1] + 0.1 * r];
            let to = [(targets[p][0] + targets[p][2]) / 2.0, targets[p][3] + 0.15 * r];
            let pts: Vec<[f32; 2]> = (0..=28).map(|k| curve(from, to, k as f32 / 28.0)).collect();
            let ba = c.alpha;
            c.alpha = ba * av;
            for w in pts.windows(2) {
                c.line(w[0], w[1], 3.0, theme::fade(pal().accent, 0.12), 6.0);
            }
            c.polyline(&pts, 1.6, theme::fade(pal().accent, 0.9));
            let n = pts.len();
            c.arrow(pts[n - 4], pts[n - 1], 1.6, 0.42 * r, pal().accent);
            // light running along it
            for j in 0..3 {
                let t = (self.now * 0.55 + j as f32 / 3.0).fract();
                let q = curve(from, to, t);
                let fade = smooth(0.0, 0.12, t) * (1.0 - smooth(0.85, 1.0, t));
                c.glow(q, 0.5 * r, theme::fade(pal().accent, 0.5 * fade));
            }
            c.alpha = ba;
        }
        y = row_y(3) + slot + 0.35 * r;
        // the line under it: the thing pointed at, or what there is to know
        let info: Option<(String, bool)> = match self.hover {
            Hit::Swap(14, i) => self.swap_inv.slot(i as usize).map(|t| {
                let kind = t.kinds.iter().find(|k| kinds_for(s.place).contains(&k.as_str()));
                let more = match kind {
                    Some(k) => i18n::trn("click: this one · right click: {kind}", &[("kind", first_lower(tr(kind_label(k))))]),
                    None => tr("click: this one").to_string(),
                };
                (format!("{} — {}", t.name, more), true)
            }),
            _ if self.swap_inv.reach == 1 => Some((tr("Only from the hotbar: the rest is not taken").to_string(), false)),
            _ if self.swap_inv.reach == 2 => Some((tr("From the inventory only totems and spheres: the rest from the hotbar").to_string(), false)),
            _ if s.place != 5 && plan.from == -1 && !s.picks.is_empty() => Some((tr("Nothing of the list in the inventory now").to_string(), false)),
            _ if s.place != 5 && plan.from == -2 => Some((tr("The first of the list is on already").to_string(), false)),
            _ if s.place != 5 => Some((tr("click: this very thing · right click: any of its kind").to_string(), false)),
            _ => None,
        };
        if let Some((text, hot)) = info {
            let text = fonts().fit(Face::Regular, 0.68 * r, &text, gw);
            c.text(gx, y + 0.95 * r, 0.68 * r, Face::Regular, &text, if hot { pal().t2 } else { pal().t4 });
        }
        y += 1.6 * r;

        if s.place == 5 {
            // ---- "All armour": its number
            let (a, dy) = rise(0.3);
            c.alpha = base * a;
            let yy = y + dy;
            caption(c, x0, yy, r, tr("All armour"));
            let mut ly = yy + 1.2 * r;
            for line in fonts().wrap(Face::Regular, 0.72 * r, tr("Each worn piece below the number is changed for the best spare one: another of the same thing first, else the best armour for its place. At 0, whenever a better one is there."), x1 - x0) {
                c.text(x0, ly + 0.6 * r, 0.72 * r, Face::Regular, &line, pal().t3);
                ly += 1.1 * r;
            }
            let mid = ly + 1.2 * r;
            let vt = format!("< {}", value_text(ARMOUR_VALUE, s.value));
            let vw = fonts().width(Face::Mono, 0.74 * r, &vt).max(4.4 * r);
            c.text(x0, mid + 0.27 * r, 0.74 * r, Face::Mono, &vt, pal().t1);
            let k = ((s.value - ARMOUR_VALUE.0) / (ARMOUR_VALUE.1 - ARMOUR_VALUE.0)).clamp(0.0, 1.0);
            self.swap_slider(c, x0 + vw + 0.9 * r, x1 - 0.5 * r, mid, k);
            return mid + 1.6 * r;
        }

        // ---- what, in order: dragged to change the order
        let (a, dy) = rise(0.3);
        c.alpha = base * a;
        let yy = y + dy;
        caption(c, x0, yy, r, tr("What, in order"));
        if s.picks.len() > 1 {
            c.text_r(x1, yy + 0.7 * r, 0.62 * r, Face::Regular, tr("drag to change the order"), pal().t4);
        }
        let ch = 2.1 * r;
        let cy0 = yy + 1.15 * r;
        let widths: Vec<f32> = s.picks.iter().map(|p| (fonts().width(Face::Medium, 0.72 * r, &pick_name(p)) + 5.4 * r).min(x1 - x0)).collect();
        let dragged = match self.swap_drag {
            Some(Drag::Pick { n, moved: true, .. }) if n < s.picks.len() => Some(n),
            _ => None,
        };
        // the order shown: the dragged one where it would go (its room kept there)
        let order: Vec<usize> = (0..s.picks.len()).filter(|&n| Some(n) != dragged).collect();
        let lay = |order: &[usize], gapw: Option<(usize, f32)>| -> Vec<(usize, [f32; 4])> {
            let mut out = Vec::new();
            let (mut x, mut y) = (x0, cy0);
            let mut put = |n: usize, w: f32, out: &mut Vec<(usize, [f32; 4])>| {
                if x + w > x1 + 0.01 && x > x0 {
                    x = x0;
                    y += ch + 0.4 * r;
                }
                out.push((n, [x, y, x + w, y + ch]));
                x += w + 0.4 * r;
            };
            for (k, &n) in order.iter().enumerate() {
                if let Some((at, w)) = gapw {
                    if at == k {
                        put(usize::MAX, w, &mut out);
                    }
                }
                put(n, widths[n], &mut out);
            }
            if let Some((at, w)) = gapw {
                if at >= order.len() {
                    put(usize::MAX, w, &mut out);
                }
            }
            out
        };
        let placed = if let Some(dn) = dragged {
            let plain = lay(&order, None);
            let m = self.mouse;
            let at = plain.iter().filter(|(_, q)| q[3] + 0.2 * r < m[1] || (m[1] <= q[3] + 0.2 * r && m[1] >= q[1] - 0.2 * r && (q[0] + q[2]) / 2.0 < m[0])).count();
            self.swap_drop_at = at;
            lay(&order, Some((at, widths[dn])))
        } else {
            lay(&order, None)
        };
        let mut bottom = cy0;
        let mut chips = vec![[0.0f32; 4]; s.picks.len()];
        for &(n, q) in &placed {
            bottom = bottom.max(q[3]);
            if n == usize::MAX {
                c.rect(q, 0.55 * r, grey(1.0, 0.03));
                continue;
            }
            chips[n] = q;
            let x = self.an(chip_key(&s.picks[n], 0), q[0], 16.0, q[0]);
            let yv = self.an(chip_key(&s.picks[n], 1), q[1], 16.0, q[1]);
            let q = [x, yv, x + (q[2] - q[0]), yv + ch];
            self.swap_chip(c, si, n, &s.picks[n], q, false);
        }
        self.swap_chips = chips;
        if s.picks.is_empty() {
            for line in fonts().wrap(Face::Regular, 0.72 * r, tr("Nothing yet: a thing from the inventory above, or a kind below"), x1 - x0) {
                c.text(x0, bottom + 0.9 * r, 0.72 * r, Face::Regular, &line, pal().t4);
                bottom += 1.1 * r;
            }
            bottom += 0.6 * r;
        }
        y = bottom + 0.8 * r;

        // ---- the kinds that go there
        let kinds = kinds_for(s.place);
        if !kinds.is_empty() {
            let (a, dy) = rise(0.34);
            c.alpha = base * a;
            let yy = y + dy;
            caption(c, x0, yy, r, tr("Or any of a kind"));
            let (mut x, mut ky) = (x0, yy + 1.15 * r);
            let kh = 2.0 * r;
            for &id in kinds {
                let k = KINDS.iter().position(|q| q.0 == id).unwrap_or(0);
                let word = tr(kind_label(id));
                let w = fonts().width(Face::Medium, 0.7 * r, word) + 3.3 * r;
                if x + w > x1 + 0.01 && x > x0 {
                    x = x0;
                    ky += kh + 0.4 * r;
                }
                let b = [x, ky, x + w, ky + kh];
                let hit = Hit::Swap(17, k as u16);
                let inl = s.picks.iter().any(|p| matches!(p, Pick::Kind(q) if q == id));
                let hv = self.an(ak(A_SWP, 500 + k), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
                let iv = self.an(ak(A_SWP, 520 + k), if inl { 1.0 } else { 0.0 }, 12.0, 0.0);
                c.rect(b, 0.55 * r, mix(mix(pal().f1, pal().f2, hv), mix(pal().f3, pal().f4, hv), iv));
                let mid = (b[1] + b[3]) / 2.0;
                let ib = [b[0] + 0.35 * r, mid - 0.68 * r, b[0] + 1.71 * r, mid + 0.68 * r];
                if id == "same" {
                    if self.swap_inv.worn(s.place).is_some() {
                        self.swap_icon(c, ib, format!("w{}", s.place));
                    } else {
                        round_arrows(c, [(ib[0] + ib[2]) / 2.0, mid], 0.45 * r, pal().t2);
                    }
                } else {
                    self.swap_icon(c, ib, format!("k:{id}"));
                }
                c.text(b[0] + 2.1 * r, mid + 0.25 * r, 0.7 * r, Face::Medium, word, if inl { pal().t1 } else { mix(pal().t3, pal().t1, hv) });
                if iv > 0.01 {
                    c.rect([b[2] - 0.62 * r, mid - 0.12 * r * iv, b[2] - 0.38 * r, mid + 0.12 * r * iv], r, pal().accent);
                }
                self.hits.push((b, hit));
                x += w + 0.4 * r;
            }
            y = ky + kh + 0.6 * r;
        }
        y = self.swap_timing(c, x0, x1, y, &s, &rise);
        // the chip being dragged, over the rest
        if let Some(dn) = dragged {
            let (m, grab) = (self.mouse, self.swap_grab);
            let q = [m[0] - grab[0], m[1] - grab[1], m[0] - grab[0] + widths[dn], m[1] - grab[1] + ch];
            c.alpha = base;
            c.shadow(q, 0.55 * r, 1.2 * r, grey(0.0, 0.5));
            // the things drawn before it are not drawn over it
            self.swap_lift = Some((q, self.swap_icons.len()));
            self.swap_chip(c, si, dn, &s.picks[dn], q, true);
            // let go, it glides from here
            self.anim.insert(chip_key(&s.picks[dn], 0), q[0]);
            self.anim.insert(chip_key(&s.picks[dn], 1), q[1]);
        }
        c.alpha = base;
        y
    }

    /// The swap's timing: the module's (its preset said), or its own — how it clicks, how the player stands, its
    /// delays (swaptime.rs). Where it ends.
    fn swap_timing(&mut self, c: &mut Canvas, x0: f32, x1: f32, mut y: f32, s: &Swap, rise: &impl Fn(f32) -> (f32, f32)) -> f32 {
        let r = self.r;
        let base = c.alpha;
        let (a, dy) = rise(0.38);
        c.alpha = base * a;
        let yy = y + dy;
        caption(c, x0, yy, r, tr("Timing"));
        let sy = yy + 1.15 * r;
        let own = s.timing.is_some();
        let right = self.swap_seg(c, x0, sy, &["Shared", "Its own"], own as usize, 19, 600, &[]);
        if !own {
            let mi = self.mod_index(super::swap::MODULE);
            let preset = self.mods[mi].set.iter().find(|q| q.label == swaptime::L_PRESET).and_then(|q| match &q.kind {
                super::data::Kind::Choice { v, opts } => opts.get(*v).copied(),
                _ => None,
            });
            let said = i18n::trn("As in the module's settings: {p}", &[("p", tr(preset.unwrap_or("Standard")).to_string())]);
            let said = fonts().fit(Face::Regular, 0.7 * r, &said, x1 - right - 0.8 * r);
            c.text(right + 0.8 * r, sy + 1.22 * r, 0.7 * r, Face::Regular, &said, pal().t4);
        }
        y += 3.25 * r;
        let Some(t) = s.timing else {
            c.alpha = base;
            return y;
        };
        let (a, dy) = rise(0.41);
        c.alpha = base * a;
        // the words' column as wide as its widest (within reason: the tracks keep most of the row)
        let widest = [swaptime::L_WAY, swaptime::L_STOP].iter().chain(swaptime::SLIDERS.iter()).map(|l| fonts().width(Face::Regular, 0.74 * r, tr(l))).fold(0.0f32, f32::max);
        let label_w = (widest + 1.0 * r).clamp(8.4 * r, (x1 - x0) * 0.45);
        let rows: [(&str, u8, &[&str], usize, usize); 2] = [(swaptime::L_WAY, 20, &swaptime::WAYS, t.way, 610), (swaptime::L_STOP, 21, &swaptime::STOPS, t.stop, 620)];
        for (label, part, opts, cur, key) in rows {
            let yy = y + dy;
            c.text(x0, yy + 1.22 * r, 0.74 * r, Face::Regular, &fonts().fit(Face::Regular, 0.74 * r, tr(label), label_w - 0.5 * r), pal().t2);
            self.swap_seg(c, x0 + label_w, yy, opts, cur, part, key, &[]);
            y += 2.4 * r;
        }
        let unit = i18n::unit(" ms");
        let vw = fonts().width(Face::Mono, 0.74 * r, &format!("± 2000{unit}"));
        for (k, label) in swaptime::SLIDERS.iter().enumerate() {
            // the stand's length only when it stands before
            if k == 3 && t.stop != 0 {
                continue;
            }
            let yy = y + dy;
            let mid = yy + 0.95 * r;
            let v = t.ms(k);
            let (min, max) = swaptime::RANGES[k];
            let vt = if k == 4 { format!("± {v:.0}{unit}") } else { format!("{v:.0}{unit}") };
            c.text(x0, mid + 0.27 * r, 0.74 * r, Face::Regular, &fonts().fit(Face::Regular, 0.74 * r, tr(label), label_w - 0.5 * r), pal().t2);
            c.text(x1 - fonts().width(Face::Mono, 0.74 * r, &vt), mid + 0.27 * r, 0.74 * r, Face::Mono, &vt, pal().t1);
            self.swap_tslider(c, k, x0 + label_w + 0.2 * r, x1 - vw - 1.0 * r, mid, ((v - min) / (max - min)).clamp(0.0, 1.0));
            y += 2.1 * r;
        }
        c.alpha = base;
        y + 0.4 * r
    }

    /// A delay's track of a swap's own timing (`k`: swaptime::SLIDERS'), from `x` to `x2` at `mid`, filled to `f`.
    fn swap_tslider(&mut self, c: &mut Canvas, k: usize, x: f32, x2: f32, mid: f32, f: f32) {
        let r = self.r;
        let hit = Hit::Swap(22, k as u16);
        let dragged = self.swap_drag == Some(Drag::Timing(k));
        let f = self.an(ak(A_SWP, 630 + k), f, if dragged { 40.0 } else { 9.0 }, f);
        let t = [x, mid - 0.15 * r, x2, mid + 0.15 * r];
        c.rect(t, r, grey(1.0, 0.1));
        c.rect_lit([t[0], t[1], t[0] + (t[2] - t[0]) * f, t[3]], r, theme::fade(pal().accent, 0.45), pal().accent, 1.0);
        let big = if dragged || self.hover == hit { 1.22 } else { 1.0 };
        let th = [t[0] + (t[2] - t[0]) * f, mid];
        let q = 0.46 * r * big;
        c.shadow([th[0] - q, th[1] - q, th[0] + q, th[1] + q], q, 0.4 * r, grey(0.0, 0.5));
        c.rect([th[0] - q, th[1] - q, th[0] + q, th[1] + q], q, pal().accent);
        self.hits.push(([x - 0.5 * r, mid - 0.9 * r, x2 + 0.5 * r, mid + 0.9 * r], hit));
        self.swap_ttrack[k] = [x, x2];
    }

    /// A delay of the chosen swap's own timing dragged to `x` on its track.
    fn swap_timing_at(&mut self, k: usize, x: f32) {
        let k = k.min(swaptime::SLIDERS.len() - 1);
        let [t0, t1] = self.swap_ttrack[k];
        let Some(mut t) = self.swaps.get(self.swap_sel).and_then(|s| s.timing) else { return };
        let (min, max) = swaptime::RANGES[k];
        let f = ((x - t0) / (t1 - t0).max(1.0)).clamp(0.0, 1.0);
        let was = t.ms(k);
        t.set_ms(k, min + (max - min) * f);
        if (t.ms(k) - was).abs() > 1e-3 {
            self.swaps[self.swap_sel].timing = Some(t);
            self.sound_tick(0.2);
        }
    }

    /// A thing of the list: its number, its picture, its name, its cross.
    fn swap_chip(&mut self, c: &mut Canvas, si: usize, n: usize, p: &Pick, q: [f32; 4], lifted: bool) {
        let r = self.r;
        let hit = Hit::Swap(15, n as u16);
        let xh = Hit::Swap(16, n as u16);
        let hv = self.an(ak(A_SWP, 440 + n), if self.hover == hit || self.hover == xh || lifted { 1.0 } else { 0.0 }, 12.0, 0.0);
        c.rect(q, 0.55 * r, mix(pal().f2, pal().f4, hv));
        let mid = (q[1] + q[3]) / 2.0;
        c.text_c(q[0] + 0.75 * r, mid + 0.24 * r, 0.62 * r, Face::Mono, &(n + 1).to_string(), pal().t3);
        let ib = [q[0] + 1.35 * r, mid - 0.7 * r, q[0] + 2.75 * r, mid + 0.7 * r];
        self.swap_icon(c, ib, format!("p{si}:{n}"));
        let name = fonts().fit(Face::Medium, 0.72 * r, &pick_name(p), q[2] - q[0] - 4.9 * r);
        c.text(q[0] + 3.1 * r, mid + 0.26 * r, 0.72 * r, Face::Medium, &name, pal().t1);
        let xc = [q[2] - 0.95 * r, mid];
        let xv = self.an(ak(A_SWP, 460 + n), if self.hover == xh { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.rect([xc[0] - 0.6 * r, xc[1] - 0.6 * r, xc[0] + 0.6 * r, xc[1] + 0.6 * r], 0.6 * r, grey(1.0, 0.06 * xv));
        cross(c, xc, 0.24 * r, mix(pal().t4, pal().t1, xv));
        if !lifted {
            self.hits.push((q, hit));
            self.hits.push(([xc[0] - 0.75 * r, q[1], q[2], q[3]], xh));
        }
    }

    /// The templates (and an empty swap), two to a row; where they end.
    fn swap_templates(&mut self, c: &mut Canvas, x0: f32, x1: f32, y: f32, rise: impl Fn(f32) -> (f32, f32)) -> f32 {
        let r = self.r;
        let base = c.alpha;
        let (a, dy) = rise(0.1);
        c.alpha = base * a;
        let yy = y + dy;
        let none = self.swaps.is_empty();
        c.text(x0, yy + 1.4 * r, 1.12 * r, Face::Bold, tr(if none { "No swaps yet" } else { "Templates" }), pal().t1);
        c.text(x0, yy + 2.75 * r, 0.74 * r, Face::Regular, tr("Start from a template or from an empty swap; anything in it can be changed after"), pal().t3);
        if !none {
            let bw = 1.9 * r;
            let xb = [x1 - bw, yy, x1, yy + bw];
            let hv = self.an(ak(A_SWP, 19), if self.hover == Hit::Swap(3, 0) { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect(xb, 0.6 * r, mix(pal().f1, pal().f3, hv));
            cross(c, [xb[0] + bw / 2.0, xb[1] + bw / 2.0], 0.3 * r, mix(pal().t3, pal().t1, hv));
            self.hits.push((xb, Hit::Swap(3, 0)));
        }
        let here = self.conds_here();
        let gap = 0.6 * r;
        let cw = (x1 - x0 - gap) / 2.0;
        let chh = 4.5 * r;
        let top = yy + 4.0 * r;
        let full = self.swaps.len() >= MAX_SWAPS;
        let mut end = top;
        for t in 0..=TEMPLATES.len() {
            let (a, dy2) = rise(0.14 + t as f32 * 0.03);
            c.alpha = base * a * if full { 0.4 } else { 1.0 };
            // its pictures darkened when the list is full (their alpha would shrink them)
            let (pa, pd) = (base * a, if full { 0.6 } else { 0.0 });
            let (col, row) = (t % 2, t / 2);
            let q = [x0 + col as f32 * (cw + gap), top + row as f32 * (chh + gap) + dy2 - dy, x0 + col as f32 * (cw + gap) + cw, top + row as f32 * (chh + gap) + chh + dy2 - dy];
            let hit = Hit::Swap(4, t as u16);
            let hv = self.an(ak(A_SWP, 800 + t), if self.hover == hit { 1.0 } else { 0.0 }, 12.0, 0.0);
            c.rect(q, 0.7 * r, mix(pal().f1, pal().f3, hv));
            self.hits.push((q, hit));
            let (name, line, pics): (&str, &str, &[&str]) = if t == 0 { ("Empty swap", "Everything in it chosen by you", &[]) } else { TEMPLATES[t - 1] };
            // its picture: one thing, or two round each other
            let pc = [q[0] + 1.0 * r, q[1] + 1.0 * r];
            match pics.len() {
                0 => {
                    let cc = [pc[0] + 1.2 * r, pc[1] + 1.25 * r];
                    let col = mix(pal().t3, pal().t1, hv);
                    c.line([cc[0] - 0.5 * r, cc[1]], [cc[0] + 0.5 * r, cc[1]], 1.6, col, 0.0);
                    c.line([cc[0], cc[1] - 0.5 * r], [cc[0], cc[1] + 0.5 * r], 1.6, col, 0.0);
                }
                1 => self.swap_icon_dim(pa, c.clip_now(), [pc[0], pc[1], pc[0] + 2.4 * r, pc[1] + 2.4 * r], format!("k:{}", pics[0]), pd),
                _ => {
                    self.swap_icon_dim(pa, c.clip_now(), [pc[0] - 0.1 * r, pc[1] - 0.1 * r, pc[0] + 1.6 * r, pc[1] + 1.6 * r], format!("k:{}", pics[0]), pd);
                    self.swap_icon_dim(pa, c.clip_now(), [pc[0] + 0.9 * r, pc[1] + 0.9 * r, pc[0] + 2.6 * r, pc[1] + 2.6 * r], format!("k:{}", pics[1]), pd);
                }
            }
            let tx = q[0] + 4.0 * r;
            let tw = q[2] - tx - 0.8 * r;
            c.text(tx, q[1] + 1.6 * r, 0.8 * r, Face::Semi, &fonts().fit(Face::Semi, 0.8 * r, tr(name), tw), pal().t1);
            // a condition's template: it will not go by itself here
            let auto = (4..=8).contains(&t);
            // two lines at most: the second ends in "…" when there is more
            let mut lines = fonts().wrap(Face::Regular, 0.64 * r, tr(line), tw);
            if lines.len() > 2 {
                let rest = lines[1..].join(" ");
                lines.truncate(1);
                lines.push(fonts().fit(Face::Regular, 0.64 * r, &rest, tw));
            }
            let mut ly = q[1] + 2.65 * r;
            for l in lines {
                c.text(tx, ly, 0.64 * r, Face::Regular, &l, if auto && !here { pal().t5 } else { pal().t3 });
                ly += 0.95 * r;
            }
            end = q[3];
        }
        c.alpha = base;
        end
    }

    // ------------------------------------------------------------------ what the editor's presses do
    fn swap_add(&mut self, s: Swap, key: bool) {
        if self.swaps.len() >= MAX_SWAPS {
            self.toast(i18n::trn("No more than {n} swaps", &[("n", MAX_SWAPS.to_string())]));
            return;
        }
        let wants_key = key && s.trigger < 2;
        self.swaps.push(s);
        self.swap_sel = self.swaps.len() - 1;
        self.swap_tpl = false;
        self.swap_shown_at = self.now;
        self.swap_held.clear();
        if wants_key {
            self.listen = Some(Listen::Swap(self.swap_sel));
            self.rec.clear();
        }
        self.switch_sound(true, 0.6);
        self.save();
    }

    /// A template's swap (the spheres: the ones in the inventory, each itself, when there are two or more).
    fn swap_template(&self, t: usize) -> Swap {
        let mut s = Swap::new();
        let k = |id: &str| Pick::Kind(id.to_string());
        match t {
            0 => s.picks = vec![k("totem"), k("shield")],
            1 => {
                let mut seen: Vec<Pick> = Vec::new();
                for th in self.swap_inv.slots.iter().flatten() {
                    let p = Pick::Item { id: th.id.clone(), name: th.name.clone(), ench: th.ench.clone() };
                    if th.id == "minecraft:player_head" && !seen.contains(&p) && seen.len() < MAX_PICKS {
                        seen.push(p);
                    }
                }
                s.picks = if seen.len() >= 2 { seen } else { vec![k("sphere")] };
            }
            2 => {
                s.place = 2;
                s.picks = vec![k("elytra"), k("chestplate")];
            }
            3 => {
                s.place = 2;
                s.trigger = 2;
                s.cond = 3;
                s.value = 4.0;
                s.picks = vec![k("elytra")];
            }
            4 => {
                s.place = 5;
                s.trigger = 2;
                s.cond = 1;
                s.value = 15.0;
            }
            5 => {
                s.trigger = 2;
                s.cond = 2;
                s.back = false;
                s.picks = vec![k("firework")];
            }
            6 => {
                s.trigger = 2;
                s.cond = 4;
                s.value = 6.0;
                s.picks = vec![k("shield")];
            }
            _ => {
                s.trigger = 2;
                s.cond = 0;
                s.value = 4.0;
                s.picks = vec![k("enchanted_apple"), k("golden_apple")];
            }
        }
        s
    }

    /// The chosen swap changed by `f`, kept.
    fn swap_edit(&mut self, f: impl FnOnce(&mut Swap)) {
        let i = self.swap_sel;
        if let Some(s) = self.swaps.get_mut(i) {
            f(s);
            self.save();
        }
    }

    /// The key a swap waited for is in (None: it has none now).
    pub(super) fn swap_key_set(&mut self, i: usize, keys: Option<Vec<String>>) {
        self.listen = None;
        self.rec.clear();
        if let Some(s) = self.swaps.get_mut(i) {
            s.bind = keys.map(|k| Bind { keys: binds::canonical(k), hold: false, value: None }).filter(|b| !b.keys.is_empty());
        }
        self.swap_held.clear();
        self.sound_tick(0.8);
        self.save();
    }

    /// The number dragged to `x` on its track.
    fn swap_value_at(&mut self, x: f32) {
        let [t0, t1] = self.swap_track;
        let Some(s) = self.swaps.get(self.swap_sel) else { return };
        let (min, max, step, _, _) = range_of(s);
        let k = ((x - t0) / (t1 - t0).max(1.0)).clamp(0.0, 1.0);
        let v = (((min + (max - min) * k) / step).round() * step).clamp(min, max);
        if (v - s.value).abs() > 1e-4 {
            self.swaps[self.swap_sel].value = v;
            self.sound_tick(0.2);
        }
    }

    /// A press in the editor or the list (`button` 0 the left, 1 the right).
    pub(super) fn swap_press(&mut self, part: u8, i: usize, button: i32) {
        let n = self.swaps.len();
        let sel = self.swap_sel.min(n.saturating_sub(1));
        self.swap_sel = sel;
        if part != 6 {
            self.swap_arm = None;
        }
        match part {
            0 if i < n => {
                if self.swap_sel != i || self.swap_tpl {
                    self.swap_sel = i;
                    self.swap_tpl = false;
                    self.swap_shown_at = self.now;
                    self.sound_tick(0.5);
                }
            }
            1 if i < n => {
                self.swaps[i].on = !self.swaps[i].on;
                let on = self.swaps[i].on;
                self.swap_held.retain(|&h| h != i);
                self.switch_sound(on, 0.6);
                self.save();
            }
            2 => self.swap_add(Swap::new(), true),
            3 => {
                self.swap_tpl = !self.swap_tpl;
                self.swap_shown_at = self.now;
                self.sound_tick(0.5);
            }
            4 => {
                let s = if i == 0 { Swap::new() } else { self.swap_template(i - 1) };
                self.swap_add(s, true);
            }
            _ if n == 0 => {}
            5 => {
                self.swap_edit(|s| s.on = !s.on);
                let on = self.swaps[sel].on;
                self.swap_held.retain(|&h| h != sel);
                self.switch_sound(on, 0.6);
            }
            6 => {
                if self.swap_arm.is_some_and(|t| self.now - t < 3.0) {
                    self.swap_arm = None;
                    self.swaps.remove(sel);
                    self.swap_sel = sel.saturating_sub(1);
                    self.swap_shown_at = self.now;
                    self.swap_held.clear();
                    if self.listen.is_some_and(|l| matches!(l, Listen::Swap(_))) {
                        self.listen = None;
                    }
                    self.switch_sound(false, 0.6);
                    self.save();
                } else {
                    self.swap_arm = Some(self.now);
                    self.sound_tick(0.5);
                }
            }
            7 => {
                if n >= MAX_SWAPS {
                    self.toast(i18n::trn("No more than {n} swaps", &[("n", MAX_SWAPS.to_string())]));
                } else {
                    let mut s = self.swaps[sel].clone();
                    // two swaps on one key would both go: the copy waits for its own
                    s.bind = None;
                    self.swap_add(s, false);
                }
            }
            8 if i < 3 => {
                let here = self.conds_here();
                self.swap_edit(|s| {
                    if s.trigger != i && i == 2 {
                        if s.place == 5 {
                            s.cond = 1;
                        }
                        let (min, max, _, _, start) = range_of(s);
                        if s.value < min || s.value > max {
                            s.value = start;
                        }
                    }
                    s.trigger = i;
                });
                if i == 2 && !here {
                    self.toast(tr("Here it will not go by itself: the server allows keys only").to_string());
                }
                if i < 2 && self.swaps[sel].bind.is_none() {
                    self.listen = Some(Listen::Swap(sel));
                    self.rec.clear();
                }
                self.swap_held.clear();
                self.sound_tick(0.5);
            }
            9 => {
                if button == 1 {
                    self.swap_key_set(sel, None);
                } else {
                    self.listen = Some(Listen::Swap(sel));
                    self.rec.clear();
                    self.sound_tick(0.6);
                }
            }
            10 if i < 5 => {
                if self.swaps[sel].place == 5 && i != 1 {
                    return;
                }
                self.swap_edit(|s| {
                    if s.cond != i {
                        s.cond = i;
                        s.value = COND_VALUES[i].4;
                        s.back = i != 2;
                    }
                });
                self.sound_tick(0.5);
            }
            11 if button == 0 => {
                self.swap_drag = Some(Drag::Value);
                let x = self.mouse[0];
                self.swap_value_at(x);
            }
            12 => {
                self.swap_edit(|s| s.back = !s.back);
                let on = self.swaps[sel].back;
                self.switch_sound(on, 0.5);
            }
            13 if i < 6 => {
                self.swap_edit(|s| {
                    if s.place == i {
                        return;
                    }
                    let was_all = s.place == 5;
                    s.place = i;
                    // the kinds that do not go there are left out ("All armour" keeps the list: it is not used there)
                    s.picks.retain(|p| match p {
                        Pick::Kind(k) => i == 5 || kinds_for(i).contains(&k.as_str()),
                        Pick::Item { .. } => true,
                    });
                    if i == 5 && s.trigger == 2 {
                        s.cond = 1;
                    }
                    // the number is another thing there ("All armour": the wear; elsewhere the condition's): anew
                    if was_all != (i == 5) {
                        s.value = range_of(s).4;
                    }
                });
                self.swap_held.clear();
                self.sound_tick(0.5);
            }
            14 if i < 36 => {
                let Some(t) = self.swap_inv.slot(i).cloned() else { return };
                let place = self.swaps[sel].place;
                if place == 5 {
                    self.toast(tr("All armour takes the best spare pieces by itself").to_string());
                    return;
                }
                let pick = if button == 1 {
                    match t.kinds.iter().find(|k| kinds_for(place).contains(&k.as_str())) {
                        Some(k) => Pick::Kind(k.clone()),
                        None => {
                            self.toast(tr("No kind of it goes there").to_string());
                            return;
                        }
                    }
                } else {
                    // (Java says where it may be put on: the off hand takes anything)
                    if place > 0 && t.places & (1 << place) == 0 {
                        self.toast(tr("It does not go there").to_string());
                        return;
                    }
                    Pick::Item { id: t.id, name: t.name, ench: t.ench }
                };
                let mut full = false;
                self.swap_edit(|s| {
                    if let Some(k) = s.picks.iter().position(|p| *p == pick) {
                        s.picks.remove(k);
                    } else if s.picks.len() >= MAX_PICKS {
                        full = true;
                    } else {
                        s.picks.push(pick);
                    }
                });
                if full {
                    self.toast(i18n::trn("No more than {n} things in a list", &[("n", MAX_PICKS.to_string())]));
                } else {
                    self.sound_tick(0.6);
                }
            }
            15 if button == 0 => {
                self.swap_drag = Some(Drag::Pick { n: i, at: self.mouse, moved: false });
                // not known till a frame draws it lifted
                self.swap_drop_at = usize::MAX;
                let q = self.hits.iter().rev().find(|h| h.1 == Hit::Swap(15, i as u16)).map_or([0.0; 4], |h| h.0);
                self.swap_grab = [self.mouse[0] - q[0], self.mouse[1] - q[1]];
            }
            15 | 16 => {
                self.swap_edit(|s| {
                    if i < s.picks.len() {
                        s.picks.remove(i);
                    }
                });
                self.sound_tick(0.5);
            }
            // its timing: the module's or its own (from the module's as it is now); how it clicks, how it stands; a delay
            19 if i < 2 => {
                let mi = self.mod_index(super::swap::MODULE);
                let module = swaptime::module_timing(&self.mods[mi].set).0;
                self.swap_edit(|s| s.timing = if i == 1 { Some(s.timing.unwrap_or(module)) } else { None });
                self.sound_tick(0.5);
            }
            20 if i < swaptime::WAYS.len() => {
                self.swap_edit(|s| s.timing = s.timing.map(|t| Timing { way: i, ..t }));
                self.sound_tick(0.5);
            }
            21 if i < swaptime::STOPS.len() => {
                self.swap_edit(|s| s.timing = s.timing.map(|t| Timing { stop: i, ..t }));
                self.sound_tick(0.5);
            }
            22 if button == 0 && i < swaptime::SLIDERS.len() => {
                self.swap_drag = Some(Drag::Timing(i));
                let x = self.mouse[0];
                self.swap_timing_at(i, x);
            }
            17 if i < KINDS.len() => {
                let id = KINDS[i].0;
                if !kinds_for(self.swaps[sel].place).contains(&id) {
                    return;
                }
                let mut full = false;
                self.swap_edit(|s| {
                    if let Some(k) = s.picks.iter().position(|p| matches!(p, Pick::Kind(q) if q == id)) {
                        s.picks.remove(k);
                    } else if s.picks.len() >= MAX_PICKS {
                        full = true;
                    } else {
                        s.picks.push(Pick::Kind(id.to_string()));
                    }
                });
                if full {
                    self.toast(i18n::trn("No more than {n} things in a list", &[("n", MAX_PICKS.to_string())]));
                } else {
                    self.sound_tick(0.6);
                }
            }
            _ => {}
        }
    }

    /// The mouse moved while the editor drags.
    pub(super) fn swap_move(&mut self, x: f32, y: f32) {
        match self.swap_drag {
            Some(Drag::Value) => self.swap_value_at(x),
            Some(Drag::Timing(k)) => self.swap_timing_at(k, x),
            Some(Drag::Pick { n, at, moved: false }) if (x - at[0]).abs() + (y - at[1]).abs() > 5.0 => {
                self.swap_drag = Some(Drag::Pick { n, at, moved: true });
            }
            _ => {}
        }
    }

    /// Let go: a thing dragged goes where it was let go, the number is kept.
    pub(super) fn swap_release(&mut self) {
        match self.swap_drag.take() {
            Some(Drag::Value | Drag::Timing(_)) => self.save(),
            Some(Drag::Pick { n, moved: true, .. }) => {
                let len = self.swaps.get(self.swap_sel).map_or(0, |s| s.picks.len());
                if n >= len {
                    return;
                }
                let mut at = self.swap_drop_at;
                if at == usize::MAX {
                    // let go before a frame drew it lifted: where it goes by the chips as they were
                    let (m, r) = (self.mouse, self.r);
                    at = self.swap_chips.iter().enumerate().filter(|(k, q)| *k != n && (q[3] + 0.2 * r < m[1] || (m[1] <= q[3] + 0.2 * r && m[1] >= q[1] - 0.2 * r && (q[0] + q[2]) / 2.0 < m[0]))).count();
                }
                self.swap_edit(|s| {
                    let p = s.picks.remove(n);
                    s.picks.insert(at.min(s.picks.len()), p);
                });
                // (each chip glides from where it is drawn to its new place by itself: their places are kept by thing)
                self.sound_tick(0.6);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inventory_is_read() {
        let inv = Inv::read("h\t1\ns\t3\t2\t-1\ttotem\tminecraft:totem_of_undying\tТотем\t\nw\t2\t1\t0.250\tchestplate\tminecraft:iron_chestplate\tНагрудник\tminecraft:protection:2\np\t0\t3\t3,7\t\np\t1\t-1\t\t2:12,4:30\nnonsense\n");
        assert_eq!(inv.reach, 1);
        let t = inv.slot(3).unwrap();
        assert_eq!((t.count, t.left, t.kinds.clone(), t.name.as_str()), (2, -1.0, vec!["totem".to_string()], "Тотем"));
        assert_eq!(inv.worn(2).unwrap().ench, "minecraft:protection:2");
        assert!((inv.worn(2).unwrap().left - 0.25).abs() < 1e-6);
        assert_eq!(inv.plan(0), Plan { from: 3, fits: vec![3, 7], pieces: vec![] });
        assert_eq!(inv.plan(1).pieces, vec![(2, 12), (4, 30)]);
        assert_eq!(inv.plan(5), Plan::default());
        assert!(inv.slot(0).is_none() && inv.slot(99).is_none());
        // FunTime: the hotbar's anything, above it only a totem (a talisman is one) or a sphere
        let ft = Inv::read("h\t2\ns\t3\t1\t-1\tshield\tminecraft:shield\tShield\t\ns\t12\t1\t-1\ttotem,talisman\tminecraft:totem_of_undying\tT\t\ns\t14\t1\t-1\tsphere\tminecraft:player_head\tS\t\ns\t20\t1\t-1\tshield\tminecraft:shield\tShield\t\n");
        assert_eq!(ft.reach, 2);
        assert!(ft.takes(3) && ft.takes(5) && ft.takes(12) && ft.takes(14));
        assert!(!ft.takes(20) && !ft.takes(25));
        assert!(inv.takes(3) && !inv.takes(12));
        // where it goes: the off hand when Java did not say
        assert_eq!(t.places, 1);
        assert_eq!(Inv::read("s\t0\t1\t1.000\thelmet\tminecraft:iron_helmet\tШлем\t\t3\n").slot(0).unwrap().places, 3);
    }

    #[test]
    fn a_thing_goes_only_where_it_may() {
        let mut g = Gui::new();
        let mut s = Swap::new();
        s.place = 2;
        g.swaps = vec![s];
        // a sword (the off hand only), a chestplate (the off hand and the chest)
        g.swap_inv = Inv::read("s\t0\t1\t1.000\t\tminecraft:diamond_sword\tМеч\t\t1\ns\t1\t1\t1.000\tchestplate\tminecraft:iron_chestplate\tНагрудник\t\t5\n");
        g.swap_press(14, 0, 0);
        assert!(g.swaps[0].picks.is_empty());
        g.swap_press(14, 1, 0);
        assert_eq!(g.swaps[0].picks.len(), 1);
        // a drag that was going on does not outlive the menu
        g.swap_drag = Some(Drag::Value);
        g.close(true);
        assert!(g.swap_drag.is_none());
    }

    #[test]
    fn kinds_go_where_they_fit() {
        assert!(kinds_for(0).contains(&"totem") && !kinds_for(0).contains(&"helmet"));
        assert!(kinds_for(1).contains(&"sphere") && kinds_for(1).contains(&"same"));
        assert!(kinds_for(2).contains(&"elytra"));
        assert!(kinds_for(5).is_empty());
        // every kind named there is a kind
        for p in 0..6 {
            for k in kinds_for(p) {
                assert!(KINDS.iter().any(|q| q.0 == *k), "{k}");
            }
        }
    }

    #[test]
    fn a_place_changed_drops_the_kinds_that_do_not_go() {
        let mut g = Gui::new();
        let mut s = Swap::new();
        s.picks = vec![Pick::Kind("totem".into()), Pick::Item { id: "minecraft:elytra".into(), name: "E".into(), ench: String::new() }, Pick::Kind("elytra".into())];
        g.swaps = vec![s];
        g.swap_press(13, 2, 0);
        assert_eq!(g.swaps[0].place, 2);
        assert_eq!(g.swaps[0].picks.len(), 2);
        // "All armour" with a condition goes by its wear
        g.swap_press(8, 2, 0);
        g.swap_press(10, 0, 0);
        assert_eq!(g.swaps[0].cond, 0);
        g.swap_press(13, 5, 0);
        assert_eq!((g.swaps[0].place, g.swaps[0].cond), (5, 1));
        g.swap_press(10, 3, 0);
        assert_eq!(g.swaps[0].cond, 1);
        // its number starts anew there and back (the wear's 15%, elsewhere the condition's own)
        assert_eq!(g.swaps[0].value, ARMOUR_VALUE.4);
        g.swap_press(13, 1, 0);
        assert_eq!(g.swaps[0].value, COND_VALUES[1].4);
    }

    #[test]
    fn a_key_is_recorded_for_a_swap() {
        let mut g = Gui::new();
        g.swap_press(4, 1, 0);
        assert_eq!(g.swaps.len(), 1);
        assert!(g.listen == Some(Listen::Swap(0)));
        g.bind_key_down("Left Alt");
        assert!(g.bind_key(false, false, "Left Alt"));
        g.bind_key_down("Z");
        assert!(g.bind_key(false, false, "Z"));
        assert!(g.listen.is_none());
        assert_eq!(g.swaps[0].bind.as_ref().unwrap().keys, vec!["Left Alt".to_string(), "Z".to_string()]);
        // Backspace while waiting takes it off
        g.swap_press(9, 0, 0);
        assert!(g.bind_key(false, true, "Backspace"));
        assert!(g.swaps[0].bind.is_none());
    }

    #[test]
    fn a_dragged_thing_goes_where_it_is_let_go() {
        let mut g = Gui::new();
        let mut s = Swap::new();
        s.picks = ["totem", "shield", "sphere"].iter().map(|k| Pick::Kind(k.to_string())).collect();
        g.swaps = vec![s];
        g.swap_drag = Some(Drag::Pick { n: 0, at: [0.0, 0.0], moved: true });
        g.swap_drop_at = 2;
        g.swap_release();
        let ids: Vec<String> = g.swaps[0].picks.iter().map(|p| match p { Pick::Kind(k) => k.clone(), _ => String::new() }).collect();
        assert_eq!(ids, vec!["shield", "sphere", "totem"]);
    }

    #[test]
    fn templates_make_swaps() {
        let mut g = Gui::new();
        g.swap_inv = Inv::read("s\t1\t1\t-1\tsphere\tminecraft:player_head\tСфера A\t\ns\t2\t1\t-1\tsphere\tminecraft:player_head\tСфера B\t\n");
        for t in 0..=TEMPLATES.len() {
            g.swap_press(4, t, 0);
        }
        assert_eq!(g.swaps.len(), TEMPLATES.len() + 1);
        // the spheres of the inventory, each itself
        assert_eq!(g.swaps[2].picks.len(), 2);
        assert!(matches!(&g.swaps[2].picks[0], Pick::Item { name, .. } if name == "Сфера A"));
        // "Worn armour" is "All armour" on its wear
        assert_eq!((g.swaps[5].place, g.swaps[5].trigger, g.swaps[5].cond), (5, 2, 1));
    }
}
