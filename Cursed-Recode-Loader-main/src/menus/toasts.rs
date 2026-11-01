//! The game's toasts over everything, top right, in the game's order (it keeps their timing), in Geist as the HUD's
//! notifications (her pick, 19.10). Each as tall as its words — not the game's slots: its lines are its font's, wrapped
//! at 200 of its px, and a line of ours took two of them, half of the glass empty — and the column as wide as its
//! widest, easing to it as toasts come and go. Glass sliding in from the edge, the column below making room as it comes
//! and closing as it goes. On the left a dark socket with the thing itself in it — a real model (the stage over
//! everything), turning once round as it comes and then swaying; the tutorial's keys, mouse and people are the menu's
//! icons (gui.comp tutIcon), not the game's pixels — a ring drawing itself round the socket and a ring of light going out
//! of it as the toast arrives (a second for a challenge); a band of light passes over the glass once. What it is, small
//! and dim (coming out of noise; a challenge's white), what it says, an advancement's description under it; a hair at its
//! foot running out as its time does (the tutorial's: how far one has got). The music now playing is one of the column
//! (top left it was over the HUD's corner); the world saving bottom right. Above the toasts, in Recode's subscription's last half hour (or while the server is gone), its
//! notice: what happens and the time left before the game closes (license.rs).

use super::lang::{ru, tr};
use super::ui::{clamp01, ease, ease_io, toward, tw};
use super::{Menus, View};
use crate::gui::draw::{grey, Canvas, Rgba};
use crate::gui::text::{fonts, Face};
use serde_json::Value;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

/// How long the game shows an advancement's or a recipe's toast (s): the hair at the foot runs out over it.
const LIFE: f32 = 5.0;

pub struct Toasts {
    /// When each toast was first seen (by the game's toast).
    seen: HashMap<i64, f32>,
    /// When the license's notice was first seen (while it is).
    license: Option<f32>,
    /// How far down the column at the right starts (eased): under the main menu's account there.
    top: f32,
    /// How wide the column is (eased; below 0 while there is none): its widest toast's.
    wide: f32,
}

fn rnd(i: u32, salt: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h % 100_000) as f32 / 100_000.0
}

/// `text` coming out of noise: the first `k` of it as it is, the rest scrambled.
fn decode(text: &str, k: f32, now: f32, salt: u32) -> String {
    if k >= 1.0 {
        return text.to_string();
    }
    let noise: Vec<char> = if ru() { "абвгдежзиклмнопрстуфхцчшэя0123456789".chars().collect() } else { "abcdefghjkmnpqrstuvwxyz0123456789".chars().collect() };
    let count = text.chars().count();
    let shown = (k * count as f32) as usize;
    let tick = (now * 20.0) as u32;
    text.chars()
        .enumerate()
        .map(|(i, ch)| if i < shown || ch == ' ' { ch } else { noise[(rnd(i as u32 ^ tick, salt) * noise.len() as f32) as usize % noise.len()] })
        .collect()
}

/// A toast's words, measured before it is drawn: the column's width is its widest's.
struct Words {
    title: String,
    lines: Vec<String>,
    desc: String,
    /// As wide as its words want (its socket and pads with them).
    want: f32,
    height: f32,
}

impl Toasts {
    pub fn new() -> Toasts {
        Toasts { seen: HashMap::new(), license: None, top: -1.0, wide: -1.0 }
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        let vh = m.vh();
        let now = m.now;
        let fade = match v.leaving {
            Some(at) => 1.0 - clamp01((now - at) / 0.3),
            None => 1.0,
        };
        let mut list: Vec<Value> = v.data.get("toasts").and_then(Value::as_array).cloned().unwrap_or_default();
        // the game's order down the column (a toast takes the first slots free: a new one may come above the others);
        // the music, slotless, last
        list.sort_by_key(|t| t.get("slot").and_then(Value::as_i64).unwrap_or(i64::MAX));
        // the toasts gone are forgotten
        let keys: Vec<i64> = list.iter().filter_map(|t| t.get("k").and_then(Value::as_i64)).collect();
        self.seen.retain(|k_, _| keys.contains(k_));
        let gap = 0.9 * vh;
        let margin = 1.6 * vh;
        let pad = 0.9 * vh;
        let radius = 0.72 * vh;
        // what it is, what it says, an advancement's description
        let cap = m.cl(9.0, 17.0, 1.0);
        let tsize = m.cl(10.0, 20.0, 1.25);
        let dsize = cap * 0.9;
        let sock = 3.8 * vh;
        // in the main menu its account is top right (main_menu.rs: its nick and head, the line under them): under it
        let goal = if v.data.get("menu").and_then(Value::as_bool).unwrap_or(false) { 5.2 * vh + m.cl(18.0, 34.0, 3.0) * 1.4 + 1.6 * vh } else { margin };
        self.top = if self.top < 0.0 { goal } else { toward(self.top, goal, 6.0, m.dt) };
        let top = self.top;
        // the toasts' words first: the column's width
        let words: Vec<Words> = list
            .iter()
            .map(|t| {
                let has_icon = t.get("icon").is_some() || t.get("glyph").is_some();
                let iw = if has_icon { sock + pad * 1.2 } else { 0.0 };
                // the music now playing: what it is, its name
                let music = t.get("music").and_then(Value::as_str);
                let title = if music.is_some() { tr("nowplaying") } else { t.get("title").and_then(Value::as_str).unwrap_or("") };
                let text = music.or_else(|| t.get("text").and_then(Value::as_str)).unwrap_or("");
                let desc = t.get("desc").and_then(Value::as_str).unwrap_or("");
                let own = tw(Face::Medium, title, cap, 0.0).max(tw(Face::Regular, text, tsize, 0.0)).max(tw(Face::Regular, desc, dsize, 0.0));
                let want = (own + iw + 2.0 * pad).clamp(18.0 * vh, 34.0 * vh);
                let room = want - 2.0 * pad - iw;
                let mut lines = fonts().wrap_ls(Face::Regular, tsize, text, room, 0.0);
                if lines.len() > 3 {
                    lines.truncate(3);
                    lines[2] = fonts().fit_ls(Face::Regular, tsize, &format!("{}…", lines[2]), room, 0.0);
                }
                let desc_h = if desc.is_empty() { 0.0 } else { dsize * 1.35 + 0.1 * vh };
                let body = cap * 1.4 + lines.len() as f32 * tsize * 1.3 + desc_h;
                let height = (body + 1.6 * pad).max(if has_icon { sock + 2.0 * pad } else { 0.0 });
                Words {
                    title: fonts().fit_ls(Face::Medium, cap, title, room, 0.0),
                    lines,
                    desc: fonts().fit_ls(Face::Regular, dsize, desc, room, 0.0),
                    want,
                    height,
                }
            })
            .collect();
        match words.iter().map(|x| x.want).reduce(f32::max) {
            Some(widest) => self.wide = if self.wide < 0.0 { widest } else { toward(self.wide, widest, 9.0, m.dt) },
            None => self.wide = -1.0,
        }
        // the license's notice first, the toasts under it as wide as it is
        let (shift, notice_w) = match crate::license::now() {
            crate::license::Say::Closing { end, why } => {
                let left = end.saturating_duration_since(std::time::Instant::now()).as_secs_f32();
                let since = now - *self.license.get_or_insert(now);
                let (nh, nw) = license_notice(m, c, left, why, since, fade, self.wide, margin, top, pad, cap, tsize);
                (nh + gap, nw)
            }
            _ => {
                self.license = None;
                (0.0, 0.0)
            }
        };
        // how far down the column the next toast is: each takes its height as it slides in and gives it back as it goes
        let mut down = top + shift;
        for (t, said) in list.iter().zip(&words) {
            let came = ease_io(t.get("p").and_then(Value::as_f64).unwrap_or(1.0) as f32);
            let p = came * fade;
            if p <= 0.002 {
                continue;
            }
            let key = t.get("k").and_then(Value::as_i64).unwrap_or(0);
            let since = now - *self.seen.entry(key).or_insert(now);
            let hot = t.get("hot").and_then(Value::as_bool).unwrap_or(false);
            let adv = t.get("adv").and_then(Value::as_i64);
            let has_icon = t.get("icon").is_some() || t.get("glyph").is_some();
            let iw = if has_icon { sock + pad * 1.2 } else { 0.0 };
            // the column's width (a new wider one comes at its own, the others widening to it)
            let width = self.wide.max(said.want).max(notice_w);
            let th = said.height;
            let x = w - margin - width + (1.0 - p) * (width + margin);
            let y = down;
            down += (th + gap) * came;
            let r = [x, y, x + width, y + th];
            // a challenge and a goal are bright; the rest are quiet
            let accent: Rgba = if hot || adv == Some(2) { grey(1.0, 1.0) } else { grey(0.85, 1.0) };
            let tint = |a: f32| [accent[0], accent[1], accent[2], a];
            c.glass(r, radius, [0.02, 0.02, 0.025, 0.66 * p], 0.65 * p, 0.55 * p, 0.0);
            // a band of light passes over the glass once it has come
            let sweep = (since - 0.3) / 0.9;
            if (0.0..1.0).contains(&sweep) {
                c.push_clip(r);
                let sx = x - width * 0.2 + sweep * width * 1.4;
                c.glow([sx, y + th / 2.0], th * 1.1, tint(0.09 * p * (PI * sweep).sin()));
                c.pop_clip();
            }
            // the socket: dark, a ring drawing itself round it, a ring of light going out as it arrives (a challenge's
            // a second after it)
            let center = [x + pad + sock / 2.0, y + th / 2.0];
            if has_icon {
                let arrive = clamp01((since - 0.15) / 0.8);
                c.rect([center[0] - sock / 2.0, center[1] - sock / 2.0, center[0] + sock / 2.0, center[1] + sock / 2.0], sock / 2.0, grey(0.0, 0.38 * p));
                c.glow(center, sock * 0.85, tint((0.10 + 0.12 * (1.0 - arrive)) * p));
                c.arc(center, sock / 2.0, 1.2 * ui, ease(arrive), tint(0.75 * p));
                for (n, from) in [0.2, 0.42].into_iter().enumerate() {
                    let kb = clamp01((since - from) / 0.75);
                    if (n == 0 || hot) && kb > 0.0 && kb < 1.0 {
                        c.ring(center, sock / 2.0 + 2.4 * vh * ease(kb), 1.2 * ui, tint(0.6 * (1.0 - kb) * p));
                    }
                }
                let isz = sock * 0.74;
                let at = [center[0] - isz / 2.0, center[1] - isz / 2.0, isz];
                if let Some(i) = t.get("icon").and_then(Value::as_f64) {
                    // once round as it comes, then a slow sway
                    let turn = ease_io(clamp01((since - 0.1) / 1.3)) + 0.035 * (now * 1.3 + key as f32 * 0.01).sin();
                    m.extra(5, i as f32 + turn.rem_euclid(1.0).min(0.9995), at, p);
                } else if let Some(g) = t.get("glyph").and_then(Value::as_f64) {
                    // the tutorial's: keys lighting in turn, a mouse looking round, a button pressed, people
                    let gs = sock * 0.7;
                    let q = ease(clamp01((since - 0.15) / 0.5));
                    c.icon_t([center[0] - gs / 2.0, center[1] - gs / 2.0, center[0] + gs / 2.0, center[1] + gs / 2.0], g as u32, since, grey(0.92, p * q));
                }
                // more than one recipe: how many
                let count = t.get("count").and_then(Value::as_i64).unwrap_or(1);
                if count > 1 {
                    let word = format!("×{count}");
                    let bs = cap * 0.85;
                    let bw = tw(Face::Mono, &word, bs, 0.0);
                    m.rising_text(c, Face::Mono, &word, center[0] + sock / 2.0 - bw, center[1] + sock / 2.0 - bs * 1.1, bs, 1.3, 0.0, grey(0.85, p), 0.0, 1.0);
                }
            }
            // its words in the middle of its height (a short one's beside a socket taller than they are)
            let body = cap * 1.4 + said.lines.len() as f32 * tsize * 1.3 + if said.desc.is_empty() { 0.0 } else { dsize * 1.35 + 0.1 * vh };
            let tx = x + pad + iw;
            let ty = y + (th - body) / 2.0;
            let title = decode(&said.title, clamp01((since - 0.1) / 0.5), now, key as u32);
            m.rising_text(c, Face::Medium, &title, tx, ty, cap, 1.4, 0.0, grey(if hot { 0.95 } else { 0.55 }, p), 0.0, 1.0);
            let mut ly = ty + cap * 1.4;
            for (i, l) in said.lines.iter().enumerate() {
                let q = ease(clamp01((since - 0.2 - i as f32 * 0.06) / 0.4));
                m.rising_text(c, Face::Regular, l, tx, ly, tsize, 1.3, 0.0, grey(0.96, p), 0.0, q);
                ly += tsize * 1.3;
            }
            if !said.desc.is_empty() {
                m.rising_text(c, Face::Regular, &said.desc, tx, ly + 0.1 * vh, dsize, 1.35, 0.0, grey(0.5, p), 0.0, ease(clamp01((since - 0.4) / 0.4)));
            }
            // the foot: its time running out (the tutorial's: how far one has got)
            let (fx0, fx1, fy) = (x + radius, x + width - radius, y + th - 2.0 * ui);
            if let Some(pr) = t.get("progress").and_then(Value::as_f64) {
                c.light(fx0, fy, fx1, 0.9 * p, 0.0, clamp01(pr as f32), 0.6, ui);
            } else if adv.is_some() || t.get("icons").is_some() {
                let left_ = 1.0 - clamp01((since - 0.3) / (LIFE - 0.3));
                c.line([fx0, fy], [fx0 + (fx1 - fx0) * left_, fy], 1.0 * ui, tint(0.45 * p), 0.0);
            }
        }
        // saving the world: a small turning arc and the words, bottom right
        let save = v.data.get("saving").and_then(Value::as_f64).unwrap_or(0.0) as f32 * fade;
        if save > 0.01 {
            let label = tr("saving");
            let lw = tw(Face::Medium, label, cap, 0.0);
            let x1 = w - margin;
            let y = h - margin - cap * 1.4;
            m.rising_text(c, Face::Medium, label, x1 - lw, y, cap, 1.4, 0.0, grey(0.75, save), 0.0, 1.0);
            let rr = cap * 0.55;
            let cx = x1 - lw - 1.0 * vh - rr;
            let cy = y + cap * 0.7;
            c.ring([cx, cy], rr, 1.4 * ui, grey(1.0, 0.15 * save));
            let turn = now * 1.4;
            let pts: Vec<[f32; 2]> = (0..=12)
                .map(|i| {
                    let a = turn * TAU + i as f32 / 12.0 * 1.6;
                    [cx + a.cos() * rr, cy + a.sin() * rr]
                })
                .collect();
            c.polyline(&pts, 1.6 * ui, grey(1.0, 0.85 * save));
        }
    }
}

/// Recode's subscription's last half hour, or the server silent for half an hour (license.rs): a notice that is not
/// missed (her word, 15.10) — larger than a toast, staying, the time left before the game closes counting down in it.
/// It comes in as a toast does, a ring of light going out of its dial; its dial is the half hour running out; a band of
/// light passes over it again each minute, and its last minute beats each second. At least as wide as the toasts'
/// column (`column`, below 0 none) under it. Its height and width, for the toasts under it.
#[allow(clippy::too_many_arguments)]
fn license_notice(m: &mut Menus, c: &mut Canvas, left: f32, why: crate::license::Why, since: f32, fade: f32, column: f32, margin: f32, top: f32, pad: f32, cap: f32, tsize: f32) -> (f32, f32) {
    let (w, ui, vh, now) = (m.w, m.uipx(), m.vh(), m.now);
    let ending = why == crate::license::Why::Ending;
    let p = ease_io(clamp01(since / 0.55)) * fade;
    let big = tsize * 1.9;
    let hsize = cap * 0.95;
    let lh = pad * 0.9 + cap * 1.45 + big * 1.15 + hsize * 1.5 + pad * 0.8;
    let sock = lh - 2.0 * pad;
    // what happened, the time left, what to do
    let title = tr(if ending { "lic.ending" } else { "lic.offline" });
    let lead = tr("lic.closesin");
    let hint = tr(if ending { "lic.extend" } else { "lic.backon" });
    let lead_w = tw(Face::Regular, lead, tsize, 0.0);
    let gap = 0.8 * vh;
    let words = tw(Face::Medium, title, cap, 0.0).max(lead_w + gap + tw(Face::Mono, "00:00", big, 0.0)).max(tw(Face::Regular, hint, hsize, 0.0));
    let width = (words + sock + 3.2 * pad).clamp(26.0 * vh, 44.0 * vh).max(column);
    let x = w - margin - width + (1.0 - p) * (width + margin);
    let y = top;
    let r = [x, y, x + width, y + lh];
    if p <= 0.002 {
        return (lh, width);
    }
    let total = 30.0 * 60.0;
    let secs = left.max(0.0).ceil() as u32;
    let last = left < 60.0;
    // the last minute beats each second
    let beat = if last { (1.0 - (left.fract())).powf(3.0) } else { 0.0 };
    c.glass(r, 0.72 * vh, [0.02, 0.02, 0.025, 0.74 * p], 0.7 * p, 0.6 * p, 0.0);
    // a band of light over the glass as it comes and again each minute
    let since_minute = (60.0 - left.rem_euclid(60.0)) % 60.0;
    let sweep = if since < 1.6 { (since - 0.3) / 1.0 } else { since_minute / 1.0 };
    if (0.0..1.0).contains(&sweep) {
        c.push_clip(r);
        let sx = x - width * 0.2 + sweep * width * 1.4;
        c.glow([sx, y + lh / 2.0], lh * 1.1, grey(1.0, 0.1 * p * (PI * sweep).sin()));
        c.pop_clip();
    }
    // the dial: the half hour running out, a ring of light going out of it as it comes
    let center = [x + pad + sock / 2.0, y + lh / 2.0];
    c.rect([center[0] - sock / 2.0, center[1] - sock / 2.0, center[0] + sock / 2.0, center[1] + sock / 2.0], sock / 2.0, grey(0.0, 0.4 * p));
    c.glow(center, sock * 0.85, grey(1.0, (0.08 + 0.14 * clamp01(1.0 - since / 1.2) + 0.12 * beat) * p));
    c.ring(center, sock * 0.36, 1.0 * ui, grey(1.0, 0.14 * p));
    let part = clamp01(left / total) * ease(clamp01((since - 0.15) / 0.9));
    c.arc(center, sock * 0.36, 2.0 * ui, part, grey(1.0, 0.9 * p));
    let kb = clamp01((since - 0.2) / 0.9);
    if kb > 0.0 && kb < 1.0 {
        c.ring(center, sock / 2.0 + 3.0 * vh * ease(kb), 1.2 * ui, grey(1.0, 0.6 * (1.0 - kb) * p));
    }
    if beat > 0.01 {
        c.ring(center, sock * 0.36 + sock * 0.2 * (1.0 - beat), 1.0 * ui, grey(1.0, 0.5 * beat * p));
    }
    let tx = x + pad + sock + pad * 1.2;
    let room = x + width - pad - tx;
    let title = fonts().fit_ls(Face::Medium, cap, title, room, 0.0);
    let title = decode(&title, clamp01((since - 0.1) / 0.6), now, 0x11c);
    let ty = y + pad * 0.9;
    m.rising_text(c, Face::Medium, &title, tx, ty, cap, 1.4, 0.0, grey(0.75, p), 0.0, 1.0);
    // the time left in Geist Mono (its figures keep their places as they change), the words before it on its line
    let cy = ty + cap * 1.45;
    let base = fonts().baseline(Face::Mono, cy, big, 1.15);
    let lead_top = base - fonts().baseline(Face::Regular, 0.0, tsize, 1.3);
    let q = ease(clamp01((since - 0.25) / 0.45));
    m.rising_text(c, Face::Regular, lead, tx, lead_top, tsize, 1.3, 0.0, grey(0.8, p), 0.0, q);
    let clock = format!("{}:{:02}", secs / 60, secs % 60);
    let bright = 0.96 + 0.04 * beat;
    m.rising_text(c, Face::Mono, &clock, tx + lead_w + gap, cy, big, 1.15, 0.0, grey(bright, p), 0.25 * beat, ease(clamp01((since - 0.35) / 0.45)));
    let hint = fonts().fit_ls(Face::Regular, hsize, hint, room, 0.0);
    m.rising_text(c, Face::Regular, &hint, tx, cy + big * 1.15, hsize, 1.5, 0.0, grey(0.5, p), 0.0, ease(clamp01((since - 0.5) / 0.4)));
    // the foot: the half hour running out
    let (fx0, fx1, fy) = (x + 0.72 * vh, x + width - 0.72 * vh, y + lh - 2.0 * ui);
    c.line([fx0, fy], [fx0 + (fx1 - fx0) * clamp01(left / total), fy], 1.0 * ui, grey(1.0, 0.5 * p), 0.0);
    (lh, width)
}
