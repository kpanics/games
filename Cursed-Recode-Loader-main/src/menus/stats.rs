//! Statistics: the game's three tabs as words.
//!
//! General: the four that matter most, large, each with what is behind it — the time played with a tick for each of
//! the world's days lived through, the way travelled with a bar of how it went (on foot, running, swimming, by boat…),
//! the mobs killed with the one killed most standing beside the number as itself, the deaths with the time since the
//! last. Under them the rest in groups (moving, fighting, time, the world's things, the rest), two columns laid as they
//! fit, every value with a hair of light for how much it is of its group's largest; what is at zero only counted.
//!
//! Items: what is counted chosen above (all, mined, crafted, used, broken, picked up, dropped); the first three stand
//! large, turning, then every other one a card — its model (it turns under the pointer), its name, its count, its bar.
//!
//! Mobs: the one killed most and the one that killed you most stand large as themselves (real models, breathing,
//! slowly turning), then every mob a card with its model, how many were killed and how many times it killed you.
//!
//! Numbers count up as they come; another tab's rows sink into their lines as the new ones rise after them.

use super::controls::{hid, hid_kind, hid_n, hid_sub, k};
use super::lang::{tr, trf};
use super::ui::{clamp01, ease, ease_out, tw, WordStyle, CAP, DIM};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, Canvas, Rgba};
use crate::gui::text::{fonts, Face};
use serde_json::Value;

const FIRE: Rgba = [1.0, 0.62, 0.32, 1.0];
const TABS: u32 = 2;
const METRIC: u32 = 81;
const CARD: u32 = 82;

pub struct Stats {
    tab: usize,
    /// What the items are counted by: 0 all, 1.. the game's headings.
    metric: usize,
    metric_at: f32,
    scroll: f32,
    scroll_to: f32,
    switched: f32,
    /// The tab before the last change, and how far down it was: its rows sink as the new ones rise.
    prev_tab: usize,
    prev_scroll: f32,
}

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map_or(&[], |a| a.as_slice())
}
fn txt(v: &Value, i: usize) -> String {
    v.get(i).and_then(Value::as_str).unwrap_or("").to_string()
}
fn num(v: &Value, i: usize) -> i64 {
    v.get(i).and_then(Value::as_i64).unwrap_or(0)
}
fn st<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn raw(v: &Value) -> f64 {
    v.get("raw").and_then(Value::as_f64).unwrap_or(0.0)
}

/// A count as the game would write it, with a thin space between thousands.
fn group(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// A number of its own for `i` and `salt` (0..1).
fn rnd(i: u32, salt: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h % 100_000) as f32 / 100_000.0
}

/// How a thing turns (as a fraction of a turn for extras 8 and 9): slowly round on its own, toward you under the
/// pointer (`face` 0..1).
fn turn(now: f32, i: usize, speed: f32, face: f32) -> f32 {
    let idle = (now * speed / 360.0 + rnd(i as u32, 71)).rem_euclid(1.0);
    // the nearest way round to facing you
    let to = if idle > 0.5 { 1.0 } else { 0.0 };
    (idle + (to - idle) * face).rem_euclid(1.0).min(0.9995)
}

impl Stats {
    pub fn new() -> Stats {
        Stats { tab: 0, metric: 0, metric_at: -10.0, scroll: 0.0, scroll_to: 0.0, switched: -10.0, prev_tab: 0, prev_scroll: 0.0 }
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        let vh = m.vh();
        let vw = w / 100.0;
        let narrow = w < 900.0 * ui;
        let left = if narrow { 20.0 * ui } else { 6.2 * vw };
        let right = w - if narrow { 20.0 * ui } else { 6.0 * vw };
        let live = v.leaving.is_none() && m.now - v.opened > 0.25;
        if !m.in_world {
            m.scene_pose([8.0, 20.5, -0.07, 0.03, m.scene_to[4]], 0.3, 0.0);
        }
        let shade = m.vis(v, 0.0);
        c.veil([0.0, 0.0, w, h], grey(0.0, 0.42 * shade), 1.0, 0.0);
        m.title(c, v, &tr("stats").to_uppercase(), left, 6.0 * vh, m.cl(34.0, 120.0, 7.6));
        let cap = m.cl(9.0, 18.0, 1.15);
        let d = v.data;
        // the tabs
        let tabs = arr(d, "tabs");
        let tsize = m.cl(16.0, 45.0, 2.9);
        let mut x = left;
        for (i, t) in tabs.iter().enumerate() {
            let id = hid(k::TAB, TABS, i as u32);
            let chosen = i == self.tab;
            let p = m.vis(v, 1.0 + i as f32 * 0.3);
            let label = t.as_str().unwrap_or("").to_uppercase();
            let stw = WordStyle { glow: 0.0, ..WordStyle::new(Face::Light, tsize).rest(if chosen { 1.0 } else { DIM }) };
            let r = m.word(c, id, &label, x, 15.4 * vh, stw, p, live, false);
            let kk = m.an(id ^ 0xc405e, if chosen { 1.0 } else { 0.0 }, 6.0);
            if kk > 0.01 {
                let lw = tw(Face::Light, &label, tsize, 0.02);
                c.light(x, r[3] - 0.06 * tsize, x + lw, 0.85 * kk * p, 0.0, ease(kk), 0.6, ui);
            }
            x = r[2] + 3.2 * vh;
        }
        if d.get("loading").and_then(Value::as_bool).unwrap_or(false) {
            let t = tr("asking").to_uppercase();
            let tw_ = tw(Face::Caps, &t, cap, 0.22);
            m.caps(c, &t, (w - tw_) / 2.0, h * 0.5, cap, 0.22, 0.5, clamp01(ease(m.vis(v, 2.0)) * 1.15));
        }
        let top = 23.0 * vh;
        let bottom = h - 12.0 * vh;
        self.scroll = m.scroll_ease(hid(k::PAGE, 801, self.tab as u32), self.scroll_to);
        // another tab: the old one's rows sink into their lines, top first, as the new one's rise after them
        let since_sw = m.now - self.switched;
        let mut passes: Vec<(usize, bool)> = Vec::new();
        if self.prev_tab != self.tab && since_sw < 0.5 {
            passes.push((self.prev_tab, true));
        }
        passes.push((self.tab, false));
        let mut height = 0.0;
        c.push_clip([left - 3.0 * vh, top - 1.0 * vh, right + 3.0 * vh, bottom]);
        for (tab, old) in passes {
            let switched = self.switched;
            let gate = move |order: f32| -> f32 {
                let rank = (order - 2.0).max(0.0);
                if old {
                    1.0 - clamp01((since_sw - rank * 0.05) / 0.22)
                } else if switched < 0.0 {
                    1.0
                } else {
                    clamp01((since_sw - 0.14 - rank * 0.1) / 0.35)
                }
            };
            let y0 = top - if old { self.prev_scroll } else { self.scroll };
            let counting = if old { 1.0 } else { ease_out(0.0, 1.0, (m.now - v.opened.max(self.switched) - 0.3) / 1.0) };
            let lay = Lay { left, right, top, bottom, y0, cap, vh, vw, ui, narrow, live: live && !old, counting };
            let hh = match tab {
                0 => self.general(m, c, v, &lay, &gate),
                1 => self.items(m, c, v, &lay, &gate),
                _ => self.mobs(m, c, v, &lay, &gate),
            };
            if !old {
                height = hh;
            }
        }
        c.pop_clip();
        self.scroll_to = self.scroll_to.clamp(0.0, (height - (bottom - top) + 2.0 * vh).max(0.0));
        // a hair of light at the edges when there is more above or below
        if self.scroll > 1.0 {
            c.veil([0.0, top - 1.0 * vh, w, top + 3.0 * vh], grey(0.0, 0.25 * shade), 3.0 * vh, 0.0);
        }
        let fsize = m.cl(18.0, 50.0, 3.2);
        let by = h - 5.0 * vh - 1.12 * fsize;
        let pb = m.vis(v, 6.0);
        let r = m.word(c, hid(k::BACK, 0, 0), &tr("back").to_uppercase(), left, by, WordStyle::new(Face::Thin, fsize), pb, live, false);
        m.caps(c, "ESC", r[2] + 1.6 * vh, by + 1.12 * fsize - cap * 1.6, cap, 0.22, 0.29, clamp01(ease(pb) * 1.15));
        let _ = CAP;
    }

    /// The four large and the groups under them; returns how tall it all is.
    fn general(&mut self, m: &mut Menus, c: &mut Canvas, v: &View, l: &Lay, gate: &dyn Fn(f32) -> f32) -> f32 {
        let d = v.data;
        let (vh, cap) = (l.vh, l.cap);
        let hero = d.get("hero").cloned().unwrap_or(Value::Null);
        let mobs = arr(d, "mobs");
        let mut y = l.y0;
        let big = m.cl(26.0, 84.0, 5.4);
        let cols = if l.narrow { 2 } else { 4 };
        let cw = (l.right - l.left) / cols as f32;
        let mut hero_h: f32 = 0.0;
        for (i, key) in ["time", "dist", "kills", "deaths"].iter().enumerate() {
            let e = hero.get(*key).cloned().unwrap_or(Value::Null);
            let (col, row) = (i % cols, i / cols);
            let kx = l.left + col as f32 * cw;
            let ky = y + row as f32 * 21.0 * vh;
            let order = 1.6 + i as f32 * 0.25;
            let p = m.vis(v, order).min(gate(order));
            let q = clamp01(ease(p) * 1.15);
            let name = if *key == "dist" { tr("travelled").to_string() } else { st(&e, "name").to_string() };
            m.caps(c, &name.to_uppercase(), kx, ky, cap, 0.18, 0.45, q);
            let shown = count_text(st(&e, "text"), l.counting);
            m.rising_text(c, Face::Thin, &shown, kx, ky + cap * 1.9, big, 1.1, 0.0, grey(0.96, 1.0), 0.0, q);
            if col > 0 {
                c.line([kx - 2.0 * vh, ky], [kx - 2.0 * vh, ky + 17.0 * vh], 1.0, grey(1.0, 0.06 * q), 0.0);
            }
            let vy = ky + cap * 1.9 + big * 1.12 + 1.2 * vh;
            let inner = cw - 4.0 * vh;
            let small = cap * 0.95;
            match *key {
                "time" => {
                    // a tick for each of the world's days lived through (twenty minutes each), lit as they count
                    let days = raw(&e) as f32 / 24000.0;
                    let n = (days.ceil() as usize).clamp(1, 40);
                    let gap = inner / 40.0;
                    let lit = days * l.counting;
                    for t in 0..n {
                        let tx = kx + t as f32 * gap;
                        let on = clamp01(lit - t as f32);
                        let th = 1.6 * vh * (0.45 + 0.55 * on);
                        c.line([tx, vy + 1.6 * vh - th], [tx, vy + 1.6 * vh], 1.2 * l.ui, grey(1.0, (0.12 + 0.7 * on) * q), if on > 0.5 { 2.0 * l.ui } else { 0.0 });
                    }
                    let word = trf("gamedays", &[&format!("{:.1}", days * l.counting)]).to_uppercase();
                    m.caps(c, &word, kx, vy + 2.4 * vh, small, 0.16, 0.42, q);
                }
                "dist" => {
                    // how the way went: a bar in parts, each the share of one way of going, drawn out as it counts
                    let modes = arr(&e, "modes");
                    let total: f64 = modes.iter().map(|md| num(md, 1) as f64).sum::<f64>().max(1.0);
                    let mut bx = kx;
                    let bw = inner * l.counting;
                    for (j, md) in modes.iter().enumerate() {
                        let part = (num(md, 1) as f64 / total) as f32;
                        let seg = bw * part;
                        if seg > 0.5 {
                            let shade_ = [0.95, 0.6, 0.8, 0.45, 0.7, 0.35][j % 6];
                            c.rect([bx, vy + 0.6 * vh, bx + (seg - 0.25 * vh).max(0.5), vy + 1.3 * vh], 0.0, grey(shade_, 0.85 * q));
                        }
                        bx += seg;
                    }
                    let legend: Vec<String> = modes.iter().take(3).map(|md| format!("{} {}%", txt(md, 0).to_uppercase(), ((num(md, 1) as f64 / total) * 100.0).round())).collect();
                    let line = fonts().fit_ls(Face::Caps, small, &legend.join("  ·  "), inner, 0.12);
                    m.caps(c, &line, kx, vy + 2.4 * vh, small, 0.12, 0.42, q);
                }
                "kills" => {
                    // the one killed most, standing beside the number as itself
                    if let Some(top) = mobs.first().filter(|r| num(r, 1) > 0) {
                        let ms = 11.0 * vh;
                        let nw = tw(Face::Thin, &shown, big, 0.0);
                        let mx = (kx + nw + 2.0 * vh).min(kx + cw - ms - 3.0 * vh);
                        let face = m.an(hid(k::LIST, CARD, 9000) ^ 0x7a1, 0.0, 4.0);
                        m.extra(9, num(top, 3) as f32 + turn(m.now, 9000, 22.0, face), [mx, ky - 0.5 * vh, ms], q);
                        let word = format!("{}  ·  {} ×{}", tr("mostkilled").to_uppercase(), txt(top, 0).to_uppercase(), group(num(top, 1)));
                        let word = fonts().fit_ls(Face::Caps, small, &word, inner, 0.12);
                        m.caps(c, &word, kx, vy + 2.4 * vh, small, 0.12, 0.42, q);
                    }
                }
                _ => {
                    let word = format!("{}: {}", st(&e, "sinceName").to_uppercase(), st(&e, "since").to_uppercase());
                    let word = fonts().fit_ls(Face::Caps, small, &word, inner, 0.12);
                    m.caps(c, &word, kx, vy + 2.4 * vh, small, 0.12, 0.42, q);
                }
            }
            hero_h = hero_h.max(ky - y + 19.0 * vh);
        }
        y += hero_h + 4.0 * vh;

        // the groups, two columns laid as they fit (the next group goes under the shorter column)
        let groups = d.get("groups").cloned().unwrap_or(Value::Null);
        let ncol = if l.narrow { 1 } else { 2 };
        let gap = 6.0 * l.vw;
        let col_w = (l.right - l.left - gap * (ncol - 1) as f32) / ncol as f32;
        let mut col_y = vec![y; ncol];
        let vsize = m.cl(13.0, 30.0, 1.9);
        let row = (vsize * 1.3).max(cap * 2.1) + 0.5 * vh;
        let mut order = 2.0;
        for gk in ["move", "fight", "time", "world", "other"] {
            let rows = arr(&groups, gk);
            let shown: Vec<&Value> = rows.iter().filter(|r| num(r, 2) > 0).collect();
            if shown.is_empty() {
                continue;
            }
            let zeros = rows.len() - shown.len();
            let ci = (0..ncol).min_by(|a, b| col_y[*a].total_cmp(&col_y[*b])).unwrap_or(0);
            let cx = l.left + ci as f32 * (col_w + gap);
            let mut gy = col_y[ci];
            let pg = m.vis(v, order).min(gate(order));
            let qg = clamp01(ease(pg) * 1.15);
            m.caps(c, &tr(&format!("g.{gk}")).to_uppercase(), cx, gy, cap, 0.3, 0.75, qg);
            c.light(cx, gy + cap * 1.6, cx + col_w, 0.35 * qg, 0.0, ease_out(0.0, 1.0, pg), 0.3, l.ui);
            gy += cap * 1.6 + 1.4 * vh;
            let max = shown.iter().map(|r| num(r, 2)).max().unwrap_or(1).max(1) as f32;
            for (j, r) in shown.iter().enumerate() {
                let ry = gy + j as f32 * row;
                if ry > l.bottom + row || ry + row < l.top - row {
                    continue;
                }
                let o = order + 0.15 + (j as f32 * 0.08).min(4.0);
                let p = m.vis(v, o).min(gate(o));
                let q = clamp01(ease(p) * 1.15);
                let label = fonts().fit_ls(Face::Caps, cap, &txt(r, 0).to_uppercase(), col_w * 0.62, 0.14);
                m.caps(c, &label, cx, ry + (row - cap * 1.3) / 2.0 - 0.3 * vh, cap, 0.14, 0.55, q);
                let value = count_text(&txt(r, 1), l.counting);
                let vw_ = tw(Face::Light, &value, vsize, 0.02);
                m.rising_text(c, Face::Light, &value, cx + col_w - vw_, ry + (row - vsize * 1.1) / 2.0 - 0.3 * vh, vsize, 1.1, 0.02, grey(0.92, 1.0), 0.0, q);
                // how much of the group's largest
                let part = num(r, 2) as f32 / max * l.counting;
                let ly = ry + row - 0.35 * vh;
                c.line([cx, ly], [cx + col_w, ly], 1.0, grey(1.0, 0.045 * q), 0.0);
                c.light(cx, ly, cx + col_w, 0.55 * q, 0.0, part.max(0.004), 0.25, l.ui);
            }
            gy += shown.len() as f32 * row;
            if zeros > 0 {
                let word = trf("zeros", &[&zeros.to_string()]).to_uppercase();
                m.caps(c, &word, cx, gy + 0.6 * vh, cap * 0.9, 0.16, 0.28, qg);
                gy += cap * 1.6 + 0.6 * vh;
            }
            col_y[ci] = gy + 4.0 * vh;
            order += 1.0;
        }
        col_y.iter().copied().fold(y, f32::max) - l.y0
    }

    /// What is counted chosen above, the first three large, then a card for each; returns how tall it is.
    fn items(&mut self, m: &mut Menus, c: &mut Canvas, v: &View, l: &Lay, gate: &dyn Fn(f32) -> f32) -> f32 {
        let d = v.data;
        let (vh, cap) = (l.vh, l.cap);
        let heads = arr(d, "itemHeads");
        let mut items: Vec<&Value> = arr(d, "items").iter().collect();
        let metric = self.metric.min(heads.len());
        let value = |r: &Value| -> i64 {
            if metric == 0 {
                (2..8).map(|i| num(r, i)).sum()
            } else {
                num(r, 1 + metric)
            }
        };
        items.retain(|r| value(r) > 0);
        items.sort_by_key(|r| -value(r));
        let mut y = l.y0;
        // what is counted: words, the chosen one lit
        let msize = cap * 1.05;
        let mut x = l.left;
        let names: Vec<String> = std::iter::once(tr("all").to_string()).chain(heads.iter().map(|h_| h_.as_str().unwrap_or("").to_string())).collect();
        let pm = m.vis(v, 1.8).min(gate(1.8));
        for (i, name) in names.iter().enumerate() {
            let label = name.to_uppercase();
            let lw = tw(Face::Caps, &label, msize, 0.16);
            let id = hid(k::BUTTON, METRIC, i as u32);
            let r = [x - 0.6 * vh, y - 0.4 * vh, x + lw + 0.6 * vh, y + msize * 1.9];
            if l.live {
                m.hit(r, id);
            }
            let on = i == metric;
            let hot = m.an(id ^ 0x3e7, if on || (l.live && m.over(r)) { 1.0 } else { 0.0 }, 7.0);
            m.caps(c, &label, x, y, msize, 0.16, 0.36 + 0.6 * hot, clamp01(ease(pm) * 1.15));
            let kk = m.an(id ^ 0x3e8, if on { 1.0 } else { 0.0 }, 6.0);
            if kk > 0.01 {
                c.light(x, y + msize * 1.6, x + lw, 0.8 * kk, 0.0, ease(kk), 0.4, l.ui);
            }
            x += lw + 2.6 * vh;
        }
        y += msize * 1.9 + 3.0 * vh;
        if items.is_empty() {
            let t = tr("nothing").to_uppercase();
            let tw_ = tw(Face::Caps, &t, cap, 0.22);
            m.caps(c, &t, (l.left + l.right - tw_) / 2.0, y + 8.0 * vh, cap, 0.22, 0.5, clamp01(ease(pm) * 1.15));
            return y - l.y0 + 12.0 * vh;
        }
        // a new choice: the cards rise anew
        let since_metric = m.now - self.metric_at;
        let mgate = |o: f32| if since_metric < 1.2 { clamp01((since_metric - 0.05 - (o - 2.0).max(0.0) * 0.05) / 0.35) } else { 1.0 };
        let max = value(items[0]).max(1) as f32;
        let fade_edges = |yy: f32, size: f32| clamp01((yy - l.top) / (2.0 * vh)) * clamp01((l.bottom - (yy + size)) / (2.0 * vh));
        // the first three, large and turning
        let podium = items.len().min(3);
        let pw = (l.right - l.left) / 3.0;
        let ph = 18.0 * vh;
        for (i, r) in items.iter().take(podium).enumerate() {
            let px = l.left + i as f32 * pw;
            let o = 2.0 + i as f32 * 0.3;
            let p = m.vis(v, o).min(gate(o)).min(mgate(o));
            let q = clamp01(ease(p) * 1.15);
            let is_ = 13.0 * vh;
            let id = hid(k::LIST, CARD, 5000 + i as u32);
            let box_ = [px, y, px + pw - 2.0 * vh, y + ph];
            if l.live {
                m.hit(box_, id);
            }
            let hot = m.an(id ^ 0x77, if l.live && m.over(box_) { 1.0 } else { 0.0 }, 6.0);
            let iy = y + (ph - is_) / 2.0 - 0.5 * vh * hot;
            c.glow([px + is_ / 2.0, iy + is_ / 2.0], is_ * 0.8, grey(1.0, (0.05 + 0.06 * hot) * q));
            m.extra(8, num(r, 1) as f32 + turn(m.now, i, 30.0, hot), [px, iy, is_], q * fade_edges(iy, is_));
            let tx = px + is_ + 1.6 * vh;
            let rank = format!("{:02}", i + 1);
            m.rising_text(c, Face::Thin, &rank, tx, y + 1.0 * vh, cap * 1.6, 1.2, 0.1, grey(0.4, 1.0), 0.0, q);
            let nsz = m.cl(13.0, 32.0, 2.1);
            let name = fonts().fit_ls(Face::Light, nsz, &txt(r, 0).to_uppercase(), pw - is_ - 4.0 * vh, 0.02);
            m.rising_text(c, Face::Light, &name, tx, y + 4.4 * vh, nsz, 1.1, 0.02, grey(0.9, 1.0), 0.0, q);
            let big = m.cl(22.0, 70.0, 4.6);
            let vs = group((value(r) as f32 * l.counting).round() as i64);
            m.rising_text(c, Face::Thin, &vs, tx, y + 4.4 * vh + nsz * 1.2, big, 1.1, 0.0, grey(0.97, 1.0), 0.0, q);
        }
        y += ph + 3.5 * vh;
        // every other one: a card
        let rest: Vec<&Value> = items.iter().skip(podium).copied().collect();
        let cols = (((l.right - l.left) / (24.0 * vh)) as usize).max(3);
        let cwid = (l.right - l.left) / cols as f32;
        let chh = 11.5 * vh;
        for (j, r) in rest.iter().enumerate() {
            let (ci, ri) = (j % cols, j / cols);
            let cx = l.left + ci as f32 * cwid;
            let cy = y + ri as f32 * chh;
            if cy > l.bottom + chh || cy + chh < l.top - chh {
                continue;
            }
            let o = 2.6 + (ri as f32 * 0.25 + ci as f32 * 0.05).min(5.0);
            let p = m.vis(v, o).min(gate(o)).min(mgate(o));
            let q = clamp01(ease(p) * 1.15);
            let id = hid(k::LIST, CARD, j as u32);
            let box_ = [cx, cy, cx + cwid - 1.6 * vh, cy + chh - 1.6 * vh];
            if l.live {
                m.hit(box_, id);
            }
            let hot = m.an(id ^ 0x77, if l.live && m.over(box_) { 1.0 } else { 0.0 }, 8.0);
            if hot > 0.01 {
                c.rect(box_, 0.6 * vh, grey(1.0, 0.04 * hot * q));
            }
            let is_ = 5.2 * vh * (1.0 + 0.18 * hot);
            let iy = cy + 1.2 * vh - 0.4 * vh * hot;
            let sp = spin(m, id ^ 0x5b1, hot);
            m.extra(8, num(r, 1) as f32 + sp, [cx + 1.0 * vh, iy, is_], q * fade_edges(iy, is_));
            let tx = cx + 1.0 * vh + 5.2 * vh + 1.4 * vh;
            let tw_max = cwid - (tx - cx) - 2.4 * vh;
            let name = fonts().fit_ls(Face::Caps, cap * 0.92, &txt(r, 0).to_uppercase(), tw_max, 0.1);
            m.caps(c, &name, tx, cy + 1.6 * vh, cap * 0.92, 0.1, 0.5, q);
            let vsz = m.cl(15.0, 40.0, 2.6);
            let vs = group((value(r) as f32 * l.counting).round() as i64);
            m.rising_text(c, Face::Light, &vs, tx, cy + 1.6 * vh + cap * 1.5, vsz, 1.1, 0.02, grey(0.95, 1.0), 0.0, q);
            let ly = cy + chh - 3.0 * vh;
            let part = value(r) as f32 / max * l.counting;
            c.line([tx, ly], [cx + cwid - 2.4 * vh, ly], 1.0, grey(1.0, 0.05 * q), 0.0);
            c.light(tx, ly, cx + cwid - 2.4 * vh, 0.5 * q, 0.0, part.max(0.01), 0.25, l.ui);
        }
        y += rest.len().div_ceil(cols) as f32 * chh;
        y - l.y0
    }

    /// The one killed most and the one that killed you most, large, as themselves; then a card for each mob.
    fn mobs(&mut self, m: &mut Menus, c: &mut Canvas, v: &View, l: &Lay, gate: &dyn Fn(f32) -> f32) -> f32 {
        let d = v.data;
        let (vh, cap) = (l.vh, l.cap);
        let mobs: Vec<&Value> = arr(d, "mobs").iter().collect();
        let mut y = l.y0;
        if mobs.is_empty() {
            let t = tr("nothing").to_uppercase();
            let tw_ = tw(Face::Caps, &t, cap, 0.22);
            m.caps(c, &t, (l.left + l.right - tw_) / 2.0, y + 10.0 * vh, cap, 0.22, 0.5, clamp01(ease(m.vis(v, 2.0).min(gate(2.0))) * 1.15));
            return 14.0 * vh;
        }
        let fade_edges = |yy: f32, size: f32| clamp01((yy - l.top) / (2.0 * vh)) * clamp01((l.bottom - (yy + size)) / (2.0 * vh));
        // the two that stand large
        let most = mobs.iter().copied().filter(|r| num(r, 1) > 0).max_by_key(|r| num(r, 1));
        let nemesis = mobs.iter().copied().filter(|r| num(r, 2) > 0).max_by_key(|r| num(r, 2));
        let hh = 30.0 * vh;
        let halves: Vec<(&Value, bool)> = most.map(|r| (r, false)).into_iter().chain(nemesis.map(|r| (r, true))).collect();
        let hw = (l.right - l.left) / 2.0;
        for (i, (r, bad)) in halves.iter().enumerate() {
            let hx = l.left + i as f32 * hw;
            let o = 1.8 + i as f32 * 0.4;
            let p = m.vis(v, o).min(gate(o));
            let q = clamp01(ease(p) * 1.15);
            let ms = 28.0 * vh;
            let id = hid(k::LIST, CARD, 6000 + i as u32);
            let box_ = [hx, y, hx + hw - 2.0 * vh, y + hh];
            if l.live {
                m.hit(box_, id);
            }
            let hot = m.an(id ^ 0x77, if l.live && m.over(box_) { 1.0 } else { 0.0 }, 4.0);
            let col = if *bad { FIRE } else { grey(1.0, 1.0) };
            // the ground it stands on: a soft pool of light
            c.glow([hx + ms * 0.5, y + hh * 0.86], ms * 0.55, [col[0], col[1], col[2], 0.07 * q]);
            m.extra(9, num(r, 3) as f32 + turn(m.now, 6000 + i, 14.0, hot), [hx, y + (hh - ms) * 0.5, ms], q * fade_edges(y + (hh - ms) * 0.5, ms));
            let tx = hx + ms + 1.0 * vh;
            let word = if *bad { tr("nemesis") } else { tr("mostkilled") }.to_uppercase();
            m.rising_text(c, Face::Caps, &word, tx, y + 7.0 * vh, cap, 1.4, 0.3, [col[0], col[1], col[2], if *bad { 1.0 } else { 0.5 }], 0.0, q);
            let nsz = m.cl(20.0, 60.0, 4.0);
            let name = fonts().fit_ls(Face::Light, nsz, &txt(r, 0).to_uppercase(), hw - ms - 3.0 * vh, 0.02);
            m.rising_text(c, Face::Light, &name, tx, y + 7.0 * vh + cap * 1.6, nsz, 1.1, 0.02, grey(1.0, 1.0), 0.0, q);
            let big = m.cl(26.0, 84.0, 5.6);
            let n = if *bad { num(r, 2) } else { num(r, 1) };
            let vs = format!("×{}", group((n as f32 * l.counting).round() as i64));
            m.rising_text(c, Face::Thin, &vs, tx, y + 7.0 * vh + cap * 1.6 + nsz * 1.15, big, 1.1, 0.0, [col[0], col[1], col[2], 0.97], 0.0, q);
        }
        if !halves.is_empty() {
            y += hh + 3.0 * vh;
        }
        // every mob a card: itself, its name, how many killed, how many times it killed you
        let cols = (((l.right - l.left) / (22.0 * vh)) as usize).max(3);
        let cwid = (l.right - l.left) / cols as f32;
        let chh = 25.0 * vh;
        for (j, r) in mobs.iter().enumerate() {
            let (ci, ri) = (j % cols, j / cols);
            let cx = l.left + ci as f32 * cwid;
            let cy = y + ri as f32 * chh;
            if cy > l.bottom + chh || cy + chh < l.top - chh {
                continue;
            }
            let o = 2.6 + (ri as f32 * 0.3 + ci as f32 * 0.06).min(5.0);
            let p = m.vis(v, o).min(gate(o));
            let q = clamp01(ease(p) * 1.15);
            let id = hid(k::LIST, CARD, 3000 + j as u32);
            let box_ = [cx, cy, cx + cwid - 1.6 * vh, cy + chh - 1.6 * vh];
            if l.live {
                m.hit(box_, id);
            }
            let hot = m.an(id ^ 0x77, if l.live && m.over(box_) { 1.0 } else { 0.0 }, 6.0);
            if hot > 0.01 {
                c.rect(box_, 0.8 * vh, grey(1.0, 0.035 * hot * q));
            }
            let ms = 15.0 * vh * (1.0 + 0.1 * hot);
            let mx = cx + (cwid - 1.6 * vh - ms) / 2.0;
            let my = cy + 0.5 * vh - 0.6 * vh * hot;
            c.glow([mx + ms / 2.0, my + ms * 0.92], ms * 0.42, grey(1.0, 0.05 * q));
            m.extra(9, num(r, 3) as f32 + turn(m.now, j, 18.0, hot), [mx, my, ms], q * fade_edges(my, ms));
            let name = fonts().fit_ls(Face::Light, cap * 1.5, &txt(r, 0).to_uppercase(), cwid - 3.0 * vh, 0.02);
            let nw = tw(Face::Light, &name, cap * 1.5, 0.02);
            let ny = cy + 16.5 * vh;
            m.rising_text(c, Face::Light, &name, cx + (cwid - 1.6 * vh - nw) / 2.0, ny, cap * 1.5, 1.1, 0.02, grey(0.92, 1.0), 0.0, q);
            let k1 = format!("{} {}", tr("killed").to_uppercase(), group((num(r, 1) as f32 * l.counting).round() as i64));
            let k2 = format!("{} {}", tr("killedyou").to_uppercase(), group(num(r, 2)));
            let small = cap * 0.9;
            let w1 = tw(Face::Caps, &k1, small, 0.12);
            let w2 = tw(Face::Caps, &k2, small, 0.12);
            let line_y = ny + cap * 1.8;
            let gap_ = 1.6 * vh;
            let sx = cx + (cwid - 1.6 * vh - w1 - gap_ - w2) / 2.0;
            m.caps(c, &k1, sx, line_y, small, 0.12, 0.7, q);
            let by = num(r, 2) > 0;
            m.rising_text(c, Face::Caps, &k2, sx + w1 + gap_, line_y, small, 1.3, 0.12, if by { [FIRE[0], FIRE[1], FIRE[2], 0.9] } else { grey(0.3, 1.0) }, 0.0, q);
        }
        y += mobs.len().div_ceil(cols) as f32 * chh;
        y - l.y0
    }

    pub fn event(&mut self, m: &mut Menus, _v: &View, e: Event) {
        match e {
            Event::Press(hit, button) if button & 15 == 0 => match (hid_kind(hit), hid_n(hit)) {
                (k::TAB, TABS) => {
                    let t = hid_sub(hit) as usize;
                    if t != self.tab {
                        self.prev_tab = self.tab;
                        self.prev_scroll = self.scroll;
                        self.tab = t;
                        self.switched = m.now;
                        self.scroll_to = 0.0;
                        m.tick_sound(0.9);
                    }
                }
                (k::BUTTON, METRIC) => {
                    let i = hid_sub(hit) as usize;
                    if i != self.metric {
                        self.metric = i;
                        self.metric_at = m.now;
                        self.scroll_to = 0.0;
                        m.tick_sound(0.6);
                    }
                }
                (k::BACK, _) => m.ask("esc"),
                _ => {}
            },
            Event::Scroll(dy) => self.scroll_to -= dy * 7.0 * m.vh(),
            Event::Key(name, _) => match name.as_str() {
                "Escape" => m.ask("esc"),
                "Tab" => {
                    self.prev_tab = self.tab;
                    self.prev_scroll = self.scroll;
                    self.tab = (self.tab + 1) % 3;
                    self.switched = m.now;
                    self.scroll_to = 0.0;
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// Where a tab's content goes this frame.
struct Lay {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    /// Where it begins (its top, scrolled).
    y0: f32,
    cap: f32,
    vh: f32,
    vw: f32,
    ui: f32,
    narrow: bool,
    live: bool,
    /// How far the numbers have counted up (0..1).
    counting: f32,
}

/// A value the game worded ("1.2 h", "35", "3.4 km") counting up: its number part scaled by `k`.
fn count_text(value: &str, k: f32) -> String {
    if k >= 1.0 {
        return value.to_uppercase();
    }
    let start = value.find(|c: char| c.is_ascii_digit());
    let Some(start) = start else { return value.to_uppercase() };
    let end = value[start..].find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',')).map_or(value.len(), |e| start + e);
    let num_s = value[start..end].replace(',', "");
    let Ok(v) = num_s.parse::<f64>() else { return value.to_uppercase() };
    let decimals = num_s.split('.').nth(1).map_or(0, |d| d.len());
    let shown = format!("{:.*}", decimals, v * k as f64);
    format!("{}{}{}", &value[..start], shown, &value[end..]).to_uppercase()
}

/// An item that faces you and turns round under the pointer (`hot`), back to facing you after (a fraction of a turn).
fn spin(m: &mut Menus, key: u64, hot: f32) -> f32 {
    let dt = m.dt;
    let cur = *m.anims.get(&key).unwrap_or(&0.0);
    let next = if hot > 0.05 { cur + dt * 0.45 * hot } else { cur + (cur.round() - cur) * (1.0 - (-dt * 6.0).exp()) };
    m.anims.insert(key, next);
    next.rem_euclid(1.0).min(0.9995)
}
