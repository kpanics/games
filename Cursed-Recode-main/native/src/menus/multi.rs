//! Multiplayer (the preview's): her servers as rows — the icon, the name with its address under it, the description
//! in the server's own colours, how many play and how fast it answers, numbers and bars that live; a line marks where
//! she played last. Rows are dragged to reorder; double click or Enter joins; taking one off is held (Undo for a while);
//! games on the network are looked for under them. Each row's version is a chip after its address («Auto · 1.21.4» as
//! the server says, or the one chosen for it): pressed (or the row with the right button) it opens a list on glass.

use super::controls::{hid, hid_kind, hid_n, hid_sub, k};
use super::lang::{tr, trf};
use super::ui::{clamp01, ease, smooth, tw, WordStyle, BONE, CAP, DIM};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, rgba, Canvas, Rgba};
use crate::gui::text::{fonts, Face};
use serde_json::Value;

const A_JOIN: u32 = 1;
const A_DIRECT: u32 = 2;
const A_ADD: u32 = 3;
const A_EDIT: u32 = 4;
const A_REMOVE: u32 = 5;
const A_REFRESH: u32 = 6;
const A_UNDO: u32 = 7;

pub struct Multi {
    sel: i64,
    scroll: f32,
    scroll_to: f32,
    hold_from: f32,
    /// A row being dragged: which, where the pointer took it, where it is now (rows' order as shown).
    drag: Option<(usize, f32)>,
    press_row: Option<(usize, f32)>,
    removed_at: f32,
    refresh_at: f32,
    row_h: f32,
    /// The version list open by a row (which), the row it was last open by (it closes from there), the rows' chips'
    /// boxes (the list follows its chip), a version just chosen and not yet back in the data (row, protocol), the
    /// list's box as last drawn (the wheel over it leaves it open).
    via: Option<usize>,
    via_shown: usize,
    chips: Vec<[f32; 4]>,
    via_pending: Option<(usize, i64, f32)>,
    via_box: [f32; 4],
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn n(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(-1)
}
fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// A version of the list by its protocol: its name ("1.21.4", "1.20.3-1.20.4").
fn version_name(versions: &[Value], proto: i64) -> Option<String> {
    versions.iter().find(|e| e.get(0).and_then(Value::as_i64) == Some(proto)).and_then(|e| e.get(1)).and_then(Value::as_str).map(str::to_string)
}

/// A server's version as its chip says it: the one chosen for it, else «Auto · what it says» (its own words when the list
/// has no such version: the last of them that starts with a digit).
fn via_label(sv: &Value, versions: &[Value], chosen: i64) -> String {
    if chosen > 0 {
        return version_name(versions, chosen).unwrap_or_else(|| chosen.to_string());
    }
    let auto = tr("vauto").to_uppercase();
    let p = n(sv, "proto");
    if p <= 0 {
        return auto;
    }
    match version_name(versions, p) {
        Some(name) => format!("{auto} · {name}"),
        None => match s(sv, "version").split_whitespace().rev().find(|w| w.starts_with(|ch: char| ch.is_ascii_digit())) {
            Some(w) => format!("{auto} · {}", w.chars().take(14).collect::<String>()),
            None => auto,
        },
    }
}

/// A server's description in its own colours: runs of (text, rgb or -1, flags 1 bold 2 italic 4 underline 8 struck
/// 16 obfuscated), two lines at most, cut to `width`; `sat` how much of its colour (0 grey), `alpha` how seen.
#[allow(clippy::too_many_arguments)]
pub fn motd(c: &mut Canvas, runs: &[Value], x: f32, top: f32, width: f32, size: f32, sat: f32, alpha: f32, now: f32, q: f32) {
    let lh = size * 1.55;
    let (mut line, mut pen) = (0usize, x);
    let f = fonts();
    let k = size / crate::gui::text::EM;
    let ls = 0.02 * size;
    let base0 = f.baseline(Face::Caps, top, size, 1.55) + (1.0 - q) * lh;
    c.push_clip([x - 2.0, top - 2.0, x + width + 2.0, top + lh * 2.0 + 2.0]);
    'runs: for r in runs {
        let Some(arr) = r.as_array() else { continue };
        let text = arr.first().and_then(Value::as_str).unwrap_or("");
        let rgb = arr.get(1).and_then(Value::as_i64).unwrap_or(-1);
        let flags = arr.get(2).and_then(Value::as_i64).unwrap_or(0);
        let col: Rgba = if rgb < 0 { grey(0.67, 1.0) } else { rgba(((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0, 1.0) };
        let lum = col[0] * 0.2126 + col[1] * 0.7152 + col[2] * 0.0722;
        let col = [lum + (col[0] - lum) * sat, lum + (col[1] - lum) * sat, lum + (col[2] - lum) * sat, alpha];
        for (ci, ch) in text.chars().enumerate() {
            if ch == '\n' {
                line += 1;
                pen = x;
                if line >= 2 {
                    break 'runs;
                }
                continue;
            }
            // obfuscated: a letter that changes, 20 times a second
            let ch = if flags & 16 != 0 && ch != ' ' {
                let h = ((now * 20.0) as u32).wrapping_mul(2654435761).wrapping_add(ci as u32 * 40503) >> 7;
                (b'A' + (h % 26) as u8) as char
            } else {
                ch
            };
            let adv = f.glyph(Face::Caps, ch).map_or(0.0, |g| g.adv) * k + ls;
            if pen + adv > x + width {
                line += 1;
                pen = x;
                if line >= 2 {
                    break 'runs;
                }
            }
            let base = base0 + line as f32 * lh;
            let s: String = ch.to_string();
            c.text_ls(pen, base, size, Face::Caps, &s, col, 0.02, 0.0);
            if flags & 1 != 0 {
                c.text_ls(pen + 0.06 * size, base, size, Face::Caps, &s, col, 0.02, 0.0);
            }
            if flags & 4 != 0 {
                c.line([pen, base + 0.18 * size], [pen + adv, base + 0.18 * size], 1.0, col, 0.0);
            }
            if flags & 8 != 0 {
                c.line([pen, base - 0.32 * size], [pen + adv, base - 0.32 * size], 1.0, col, 0.0);
            }
            pen += adv;
        }
    }
    c.pop_clip();
}

impl Multi {
    pub fn new(data: &Value) -> Multi {
        Multi {
            sel: n(data, "sel"),
            scroll: 0.0,
            scroll_to: 0.0,
            hold_from: -1.0,
            drag: None,
            press_row: None,
            removed_at: -10.0,
            refresh_at: -10.0,
            row_h: 1.0,
            via: None,
            via_shown: 0,
            chips: Vec::new(),
            via_pending: None,
            via_box: [0.0; 4],
        }
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        self.moved(m);
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        let vh = m.vh();
        let vw = w / 100.0;
        let narrow = w < 900.0 * ui;
        let left = if narrow { 20.0 * ui } else { 6.2 * vw };
        let right = w - if narrow { 20.0 * ui } else { 6.0 * vw };
        let live = v.leaving.is_none() && m.now - v.opened > 0.3;
        m.scene_pose([14.0, 27.0, -0.12, -0.05, m.scene_to[4]], 0.3, 0.25);
        let shade = m.vis(v, 0.0);
        c.veil([0.0, 14.0 * vh, w, h - 10.0 * vh], grey(0.0, 0.42 * shade), 18.0 * vh, 0.0);
        let empty = Vec::new();
        let servers = v.data.get("servers").and_then(Value::as_array).unwrap_or(&empty);
        let lan = v.data.get("lan").and_then(Value::as_array).unwrap_or(&empty);
        let versions = v.data.get("versions").and_then(Value::as_array).unwrap_or(&empty);
        self.chips.resize(servers.len(), [0.0; 4]);
        if self.via.is_some_and(|i| i >= servers.len()) {
            self.via = None;
        }
        // a version chosen: back in the data (or not after a second: the data says), nothing more to hold
        if let Some((pi, pp, at)) = self.via_pending {
            if m.now - at > 1.0 || servers.get(pi).is_none_or(|sv| n(sv, "via").max(0) == pp) {
                self.via_pending = None;
            }
        }
        let gsel = n(v.data, "sel");
        if gsel >= 0 {
            self.sel = gsel;
        }
        m.title(c, v, &tr("multi").to_uppercase(), left, 6.0 * vh, m.cl(34.0, 120.0, 7.6));
        let cap = m.cl(9.0, 18.0, 1.15);
        let online: i64 = servers.iter().map(|s_| n(s_, "online").max(0)).sum();
        let q_sub = clamp01(ease(m.vis(v, 1.0)) * 1.15);
        let count = if servers.len() == 1 { tr("server1").to_string() } else { trf("servers", &[&servers.len().to_string()]) };
        let x2 = m.caps(c, &count.to_uppercase(), left, 15.6 * vh, cap, 0.22, CAP, q_sub);
        m.caps(c, &trf("online", &[&online.to_string()]).to_uppercase(), x2 + 2.4 * vh, 15.6 * vh, cap, 0.22, CAP, q_sub);

        // the rows
        let top = 21.0 * vh;
        let bottom = h - 17.0 * vh;
        let row_h = 10.4 * vh;
        self.row_h = row_h;
        let icon = 6.6 * vh;
        let name_w = if narrow { (right - left) * 0.45 } else { 26.0 * vw };
        let right_w = if narrow { 0.0 } else { 15.0 * vw };
        let gap = 2.6 * vh;
        let motd_x = left + icon + gap + name_w + gap;
        let motd_w = (right - right_w - gap - motd_x).max(0.0);
        self.scroll = m.scroll_ease(hid(k::PAGE, 601, 0), self.scroll_to);
        c.push_clip([left - 3.0 * vh, top - 1.0, right + 1.0, bottom]);
        // the order as shown: the dragged row where the pointer holds it
        let mut order: Vec<usize> = (0..servers.len()).collect();
        let drag_y = self.drag.map(|(i, grab)| (i, m.mouse[1] - grab));
        if let Some((di, dy)) = drag_y {
            let slot = (((dy - top + self.scroll) / row_h).round() as i64).clamp(0, servers.len() as i64 - 1) as usize;
            order.retain(|&i| i != di);
            order.insert(slot.min(order.len()), di);
        }
        let refresh = m.now - self.refresh_at;
        for (slot, &i) in order.iter().enumerate() {
            let sv = &servers[i];
            let dragged = drag_y.is_some_and(|(di, _)| di == i);
            let target = top + slot as f32 * row_h - self.scroll;
            let key = hid(k::LIST, 2, i as u32);
            let y = if dragged {
                drag_y.unwrap().1
            } else {
                // rows glide to their place
                let cur = m.anims.get(&(key ^ 0x7)).copied().unwrap_or(target);
                let ny = super::ui::toward(cur, target, 14.0, m.dt);
                m.anims.insert(key ^ 0x7, ny);
                ny
            };
            if y < top - row_h || y > bottom {
                continue;
            }
            let p = m.vis(v, 2.0 + slot as f32 * 0.6);
            let q = clamp01(ease(p) * 1.15);
            let chosen = i as i64 == self.sel;
            let r = [left, y, right, y + row_h];
            if live && self.drag.is_none() {
                m.hit(r, key);
            }
            let hot = m.an(key ^ 0x40, if live && (m.over(r) || dragged) { 1.0 } else { 0.0 }, 6.0);
            let lit = hot.max(if chosen { 0.6 } else { 0.0 });
            let state = n(sv, "state");
            let down = state == 2;
            // the wave of asking again runs down the rows
            let wave = (1.0 - ((refresh - slot as f32 * 0.06) / 0.5 - 0.5).abs() * 2.0).max(0.0);
            // the icon
            let iy = y + (row_h - icon) / 2.0;
            let grow = 1.0 + 0.04 * hot;
            let ic = [left - icon * (grow - 1.0) / 2.0, iy - icon * (grow - 1.0) / 2.0, left + icon * (1.0 + grow) / 2.0 - icon / 2.0 * 0.0 + icon * (grow - 1.0) / 2.0, iy + icon * (grow + 1.0) / 2.0 - icon / 2.0 * 0.0];
            let ic = [ic[0], ic[1], ic[0] + icon * grow, ic[1] + icon * grow];
            let ic_alpha = q;
            let icon_key = s(sv, "icon");
            if let Some((uv, _)) = (!icon_key.is_empty()).then(|| crate::gui::pics::get(icon_key)).flatten() {
                let sat = if down { 0.0 } else { 0.75 + 0.25 * lit };
                let br = if down { 0.5 } else { 0.86 + 0.14 * lit };
                c.pic(ic, uv, ic_alpha, true, sat, br, 0.0);
            } else {
                c.rect(ic, 0.0, grey(1.0, 0.05 * ic_alpha));
                c.icon([ic[0] + icon * 0.3, ic[1] + icon * 0.3, ic[2] - icon * 0.3, ic[3] - icon * 0.3], 13, grey(0.3, ic_alpha));
            }
            // the name and its address
            let nsize = m.cl(22.0, 70.0, 4.4);
            let nx = left + icon + gap;
            let ny = y + (row_h - (1.12 * nsize + 0.5 * vh + cap * 1.3)) / 2.0;
            let name = fonts().fit_ls(Face::Thin, nsize, &s(sv, "name").to_uppercase(), name_w, 0.02);
            let st = WordStyle { glow: 0.5, ..WordStyle::new(Face::Thin, nsize).rest(if chosen { BONE } else { DIM }) };
            m.word(c, key, &name, nx, ny, st, p, live && self.drag.is_none(), false);
            // its version: a chip after the address (the address cut to leave it room)
            let chosen_v = match self.via_pending {
                Some((pi, pp, _)) if pi == i => pp,
                _ => n(sv, "via").max(0),
            };
            let chip_text = via_label(sv, versions, chosen_v);
            let csize = cap * 0.92;
            let ch_h = csize * 2.1;
            let ch_w = tw(Face::Caps, &chip_text, csize, 0.1) + csize * 3.3;
            let addr = fonts().fit_ls(Face::Caps, cap, s(sv, "ip"), (name_w - ch_w - 1.0 * vh).max(4.0 * cap), 0.1);
            let addr_y = ny + 1.12 * nsize + 0.5 * vh;
            let addr_end = m.rising_text(c, Face::Caps, &addr, nx, addr_y, cap, 1.3, 0.1, grey(0.353 + 0.25 * lit, 1.0), 0.0, q);
            let cmid = addr_y + 0.65 * cap;
            let chip = [addr_end + 0.9 * vh, cmid - ch_h / 2.0, addr_end + 0.9 * vh + ch_w, cmid + ch_h / 2.0];
            self.chips[i] = chip;
            let cid = hid(k::LIST, 4, i as u32);
            let open = self.via == Some(i);
            let ch_hot = m.an(hid(k::LIST, 8, i as u32), if live && self.drag.is_none() && (m.over(chip) || open) { 1.0 } else { 0.0 }, 8.0);
            let manual = chosen_v > 0;
            // the chip's rim: a slightly larger rounded plate under it, so the chip reads as a button
            // (it opens the version list — Via)
            let rim = 0.32 * vh;
            c.rect([chip[0] - rim, chip[1] - rim, chip[2] + rim, chip[3] + rim], ch_h / 2.0 + rim, grey(1.0, (0.13 + 0.11 * ch_hot) * q));
            c.rect(chip, ch_h / 2.0, grey(1.0, (0.06 + 0.10 * ch_hot) * q));
            if manual {
                // chosen by hand: a lamp under it
                c.light(chip[0] + 0.5 * ch_h, chip[3] - 1.0, chip[2] - 0.5 * ch_h, 0.6 * q, 0.0, 1.0, 0.7, ui);
            }
            let tb = (if manual { 0.86 } else { 0.70 }) + (if manual { 0.14 } else { 0.22 }) * ch_hot;
            m.rising_text(c, Face::Caps, &chip_text, chip[0] + 1.05 * csize, cmid - 0.65 * csize, csize, 1.3, 0.1, grey(tb, 1.0), 0.0, q);
            // its arrow: down, up while the list is open
            let turn = m.an(hid(k::LIST, 9, i as u32), if open { 1.0 } else { 0.0 }, 10.0);
            let (ax, asz) = (chip[2] - 1.15 * csize, 0.3 * csize);
            let d = asz * 0.55 * (1.0 - 2.0 * ease(turn));
            let acol = grey(tb, q);
            c.line([ax - asz, cmid - d], [ax, cmid + d], 1.2 * ui, acol, 0.0);
            c.line([ax, cmid + d], [ax + asz, cmid - d], 1.2 * ui, acol, 0.0);
            if live && self.drag.is_none() {
                m.hit(chip, cid);
            }
            // the description
            if motd_w > 40.0 {
                let msize = m.cl(9.0, 21.0, 1.35);
                let runs = sv.get("motd").and_then(Value::as_array).cloned().unwrap_or_default();
                let my = y + (row_h - msize * 1.55 * 2.0) / 2.0;
                if state == 1 {
                    let dots = ".".repeat(((m.now * 3.0) as usize % 3) + 1);
                    m.rising_text(c, Face::Caps, &format!("{}{dots}", tr("asking").trim_end_matches('…')).to_uppercase(), motd_x, my, msize, 1.55, 0.06, grey(0.4, 1.0), 0.0, q);
                } else {
                    motd(c, &runs, motd_x, my, motd_w, msize, 0.55 + 0.45 * lit, (0.62 + 0.38 * lit) * q.min(1.0), m.now, q);
                }
            }
            // how many play, how fast it answers
            if right_w > 0.0 {
                let psize = m.cl(18.0, 53.0, 3.4);
                let on = n(sv, "online");
                let max = n(sv, "max");
                let rx = right;
                let py = y + (row_h - (1.0 * psize + 0.6 * vh + cap * 1.3)) / 2.0;
                if on >= 0 && !down {
                    let mx = format!("/{max}");
                    let mw = tw(Face::Light, &mx, psize, 0.02);
                    m.rising_text(c, Face::Light, &mx, rx - mw, py, psize, 1.0, 0.02, grey(0.333, 1.0), 0.0, q);
                    let ot = format!("{on}");
                    let ow = tw(Face::Light, &ot, psize, 0.02);
                    m.rising_text(c, Face::Light, &ot, rx - mw - ow, py, psize, 1.0, 0.02, grey(0.74 + 0.26 * lit, 1.0), 0.0, q);
                } else if down {
                    let t = tr("noanswer").to_uppercase();
                    let tw_ = tw(Face::Caps, &t, cap, 0.22);
                    m.caps(c, &t, rx - tw_, py + psize * 0.3, cap, 0.22, 0.4, q);
                }
                // the bars and the time
                let ping = n(sv, "ping");
                let bars = if down || ping < 0 { 0 } else if ping < 150 { 5 } else if ping < 300 { 4 } else if ping < 600 { 3 } else if ping < 1000 { 2 } else { 1 };
                let by = py + psize + 0.6 * vh;
                let bh = 1.3 * vh;
                let mut bx = rx - 5.0 * 4.0;
                if state == 4 || state == 3 {
                    let t = trf("ms", &[&ping.max(0).to_string()]).to_uppercase();
                    let tw_ = tw(Face::Caps, &t, cap, 0.1);
                    bx = rx - tw_ - 1.2 * vh - 5.0 * 4.0;
                    m.caps(c, &t, rx - tw_, by + bh - cap * 1.15, cap, 0.1, 0.5 + 0.3 * lit, q);
                }
                for bi in 0..5 {
                    let hgt = bh * (0.3 + 0.14 * bi as f32);
                    let lit_bar = (bi as i64) < bars || (state == 1 && ((m.now * 6.0) as i64 % 5) == bi as i64);
                    let x0 = bx + bi as f32 * 4.0;
                    c.rect([x0, by + bh - hgt, x0 + 2.0, by + bh], 0.0, if lit_bar { grey(0.85, q) } else { grey(0.23, q) });
                }
            }
            // where she played last: a line of light at its left
            if b(sv, "last") {
                let lx = left - 2.4 * vh;
                c.line([lx, y + row_h * 0.3], [lx, y + row_h * 0.7], 1.0, grey(1.0, 0.85 * q), 4.0 * ui);
            }
            // the hairline under it, and the wave
            c.line([left + icon + gap, y + row_h - 0.5], [right, y + row_h - 0.5], 1.0, grey(1.0, 0.05 * q), 0.0);
            if wave > 0.0 {
                c.light(left, y + row_h * 0.5, right, wave * 0.8, 0.0, 1.0, 1.0, ui);
            }
        }
        // games on the network, under the servers
        let mut ly = top + servers.len() as f32 * row_h - self.scroll + 1.2 * vh;
        for (li, l) in lan.iter().enumerate() {
            let id = hid(k::LIST, 3, li as u32);
            let i = n(l, "i");
            let chosen = i == self.sel;
            let p = m.vis(v, 3.0);
            let size = m.cl(18.0, 46.0, 3.0);
            let st = WordStyle::new(Face::Light, size).rest(if chosen { BONE } else { DIM });
            m.word(c, id, &s(l, "motd").to_uppercase(), left + icon + gap, ly, st, p, live, false);
            m.caps(c, s(l, "ip"), left + icon + gap + name_w + gap, ly + 1.12 * size - cap * 1.4, cap, 0.1, 0.45, clamp01(ease(p) * 1.15));
            ly += 1.12 * size + 1.0 * vh;
        }
        c.pop_clip();
        let content = servers.len() as f32 * row_h + lan.len() as f32 * 6.0 * vh;
        self.scroll_to = self.scroll_to.clamp(0.0, (content - (bottom - top)).max(0.0));

        // looking for games on the network: a light sweeping a line
        let pl = m.vis(v, 6.0);
        let lan_y = h - 12.6 * vh;
        let t = tr("lan").to_uppercase();
        let lw = 12.0 * vw;
        let sweep = ((m.now / 2.6).fract() * 1.3) - 0.3;
        c.light(left, lan_y + cap * 0.7, left + lw, 0.08 * pl, 0.0, 1.0, 0.0, ui);
        c.light(left + lw * sweep.max(0.0), lan_y + cap * 0.7, left + lw * (sweep + 0.3).min(1.0), 0.9 * pl, 0.0, 1.0, 0.8, ui);
        m.caps(c, &t, left + lw + 1.6 * vh, lan_y, cap, 0.22, 0.4, clamp01(ease(pl) * 1.15));

        // what can be done: join the largest
        let asize = m.cl(18.0, 50.0, 3.2);
        let big = m.cl(34.0, 118.0, 7.4);
        let pa = m.vis(v, 8.0);
        let fy = h - 5.0 * vh;
        let any = self.sel >= 0;
        let join = tr("join").to_uppercase();
        let jx = right - tw(Face::Light, &join, big, 0.02);
        let jr = m.word(c, hid(k::BUTTON, A_JOIN, 0), &join, jx, fy - 1.12 * big, WordStyle::new(Face::Light, big).rest(BONE).enabled(any), pa, live, false);
        let mut x = jr[0] - 4.8 * vh;
        let on_server = self.sel >= 0 && (self.sel as usize) < servers.len();
        for (a, key, enabled) in [(A_REFRESH, "refresh", true), (A_REMOVE, "remove1", on_server), (A_EDIT, "edit", on_server), (A_ADD, "add", true), (A_DIRECT, "direct", true)] {
            let label = tr(key).to_uppercase();
            x -= tw(Face::Thin, &label, asize, 0.02);
            let y0 = fy - 1.12 * asize - 0.1 * big;
            let r = m.word(c, hid(k::BUTTON, a, 0), &label, x, y0, WordStyle::new(Face::Thin, asize).enabled(enabled), pa, live, false);
            if a == A_REMOVE {
                let held = self.hold_from >= 0.0 && m.lit == hid(k::BUTTON, A_REMOVE, 0);
                if !held {
                    self.hold_from = -1.0;
                }
                let hint = m.an(hid(k::BUTTON, A_REMOVE, 9), if m.lit == hid(k::BUTTON, A_REMOVE, 0) { 1.0 } else { 0.0 }, 6.0);
                if hint > 0.01 {
                    m.caps(c, &tr("hold").to_uppercase(), r[0], y0 - cap * 1.6, cap, 0.2, 0.54 * hint, ease(hint));
                }
                if held {
                    let hq = ((m.now - self.hold_from) / 1.0).min(1.0);
                    let lw = tw(Face::Thin, &label, asize, 0.02);
                    c.light(r[0], r[3] - 0.06 * asize, r[0] + lw, 1.6, 0.0, hq, 1.2, ui);
                    if hq >= 1.0 {
                        self.hold_from = -1.0;
                        self.removed_at = m.now;
                        m.ask(format!("remove {}", self.sel));
                        m.tick_sound(1.0);
                    }
                }
            }
            x -= 3.4 * vh;
        }
        // taken off: Undo for a while
        if b(v.data, "undo") {
            let age = m.now - self.removed_at;
            if age > 7.0 {
                m.ask("forget");
            }
            let k_in = smooth(0.0, 0.5, age).min(1.0 - smooth(6.4, 7.0, age));
            let text = trf("deleted", &[s(v.data, "removed")]).to_uppercase();
            let tsz = m.cl(16.0, 42.0, 2.6);
            let wd = tw(Face::Caps, &text, cap, 0.22) + 2.0 * vh + tw(Face::Thin, &tr("undo").to_uppercase(), tsz, 0.02);
            let tx = (w - wd) / 2.0;
            let ty = h - 5.2 * vh - 1.12 * tsz;
            let end = m.caps(c, &text, tx, ty + 1.12 * tsz - cap * 1.5, cap, 0.22, 0.6, k_in);
            m.word(c, hid(k::BUTTON, A_UNDO, 0), &tr("undo").to_uppercase(), end + 2.0 * vh, ty, WordStyle::new(Face::Thin, tsz), k_in, live, false);
        }
        // back
        let fsize = m.cl(18.0, 50.0, 3.2);
        let by = h - 5.0 * vh - 1.12 * fsize;
        let pb = m.vis(v, 9.0);
        let r = m.word(c, hid(k::BACK, 0, 0), &tr("back").to_uppercase(), left, by, WordStyle::new(Face::Thin, fsize), pb, live, false);
        m.caps(c, "ESC", r[2] + 1.6 * vh, by + 1.12 * fsize - cap * 1.6, cap, 0.22, 0.29, clamp01(ease(pb) * 1.15));
        self.versions_list(m, c, servers, versions, live);
    }

    /// The version list of a row, on glass under its chip (over it near the screen's foot): «Auto · what it says», a
    /// hair, then each version in two columns, newest first; the chosen one lit with a lamp under it. Versions that
    /// cannot be joined yet are dim, said under them.
    fn versions_list(&mut self, m: &mut Menus, c: &mut Canvas, servers: &[Value], versions: &[Value], live: bool) {
        if let Some(i) = self.via {
            self.via_shown = i;
        }
        let p = m.an(hid(k::LIST, 7, 0), if self.via.is_some() { 1.0 } else { 0.0 }, 9.0);
        if p < 0.01 {
            return;
        }
        let i = self.via_shown;
        let (Some(sv), Some(chip)) = (servers.get(i), self.chips.get(i).copied()) else {
            self.via = None;
            return;
        };
        let e = ease(p);
        let (ui, vh) = (m.uipx(), m.vh());
        let size = m.cl(9.0, 18.0, 1.15);
        let row = 2.45 * size;
        let pad = 1.1 * size;
        let chosen = match self.via_pending {
            Some((pi, pp, _)) if pi == i => pp,
            _ => n(sv, "via").max(0),
        };
        let auto = via_label(sv, versions, 0);
        let half = versions.len().div_ceil(2).max(1);
        let name_w = versions.iter().filter_map(|v| v.get(1).and_then(Value::as_str)).map(|t| tw(Face::Caps, t, size, 0.1)).fold(0.0f32, f32::max);
        let col_w = name_w + 3.4 * size;
        let w = (2.0 * col_w).max(tw(Face::Caps, &auto, size, 0.1) + 3.4 * size) + pad;
        let soon = versions.iter().any(|v| !v.get(2).and_then(Value::as_bool).unwrap_or(true));
        let foot = if soon { 2.3 * size } else { 0.0 };
        let hgt = pad + row + 0.9 * size + half as f32 * row + foot + 0.7 * pad;
        let x = chip[0].min(m.w - w - 2.0 * vh).max(2.0 * vh);
        let below = chip[3] + 0.7 * vh;
        let down = below + hgt < m.h - 2.0 * vh;
        let y = if down { below } else { (chip[1] - 0.7 * vh - hgt).max(2.0 * vh) };
        let y = y + (1.0 - e) * 1.0 * vh * if down { -1.0 } else { 1.0 };
        let r = [x, y, x + w, y + hgt];
        self.via_box = r;
        let open = self.via.is_some() && live;
        if open {
            // a press past it closes it (and is nothing else's)
            m.hit([0.0, 0.0, m.w, m.h], hid(k::LIST, 5, 0));
            m.hit(r, hid(k::LIST, 5, 1));
        }
        // the glass covers what is under it as much as the list is there (it shows the world behind it, not the page:
        // whole while it went, it stood black over the page until the last frame)
        let a0 = c.alpha;
        c.alpha = a0 * e;
        c.glass(r, 0.9 * vh, [0.02, 0.02, 0.025, 0.78], 0.75, 0.6, 0.0);
        c.alpha = a0;
        // what it can be set to: its own way first, then each version (two columns, newest at the top left)
        let mut items: Vec<(i64, String, bool, [f32; 4])> = vec![(0, auto, true, [r[0] + 0.5 * pad, r[1] + pad - 0.3 * size, r[2] - 0.5 * pad, r[1] + pad - 0.3 * size + row])];
        let top = r[1] + pad + row + 0.9 * size - 0.3 * size;
        c.line([r[0] + pad, top - 0.45 * size], [r[2] - pad, top - 0.45 * size], 1.0, grey(1.0, 0.08 * e), 0.0);
        for (k, v) in versions.iter().enumerate() {
            let proto = v.get(0).and_then(Value::as_i64).unwrap_or(0);
            let name = v.get(1).and_then(Value::as_str).unwrap_or("").to_string();
            let ok = v.get(2).and_then(Value::as_bool).unwrap_or(true);
            let (cx, ry) = (k / half, k % half);
            let x0 = r[0] + 0.5 * pad + cx as f32 * col_w;
            let y0 = top + ry as f32 * row;
            items.push((proto, name, ok, [x0, y0, x0 + col_w - 0.3 * size, y0 + row]));
        }
        for (idx, (proto, text, ok, ir)) in items.iter().enumerate() {
            let id = hid(k::LIST, 6, idx as u32);
            let hot = m.an(id ^ 0x40, if open && *ok && m.over(*ir) { 1.0 } else { 0.0 }, 10.0);
            let sel = *proto == chosen;
            // they come up one after another as it opens
            let q = clamp01((p - idx as f32 * 0.012) * 1.25);
            c.rect(*ir, 0.5 * size, grey(1.0, (0.07 * hot + if sel { 0.05 } else { 0.0 }) * e));
            let bright = if !ok { 0.24 } else if sel { 1.0 } else { 0.56 + 0.34 * hot };
            let mid = (ir[1] + ir[3]) / 2.0;
            let tx = ir[0] + 1.1 * size;
            let end = m.rising_text(c, Face::Caps, text, tx, mid - 0.65 * size, size, 1.3, 0.1, grey(bright, 1.0), 0.0, q);
            if sel {
                c.light(tx, ir[3] - 0.42 * size, end - 0.1 * size, 0.85 * e, 0.0, 1.0, 0.8, ui);
            }
            if open && *ok {
                m.hit(*ir, id);
            }
        }
        if soon {
            m.caps(c, &tr("vsoon").to_uppercase(), r[0] + pad, r[3] - 0.7 * pad - 1.5 * size, 0.85 * size, 0.1, 0.34, e);
        }
    }

    /// The version list opened by a row, or closed when it was open by it.
    fn toggle_versions(&mut self, m: &Menus, row: usize) {
        self.via = if self.via == Some(row) { None } else { Some(row) };
        m.tick_sound(1.0);
    }

    pub fn event(&mut self, m: &mut Menus, v: &View, e: Event) {
        let servers = v.data.get("servers").and_then(Value::as_array).map_or(0, |a| a.len());
        match e {
            // the right button on a row (or its chip): its version list
            Event::Press(hit, button) if button & 15 == 1 => {
                if hid_kind(hit) == k::LIST && matches!(hid_n(hit), 2 | 4) {
                    self.toggle_versions(m, hid_sub(hit) as usize);
                } else if hid_kind(hit) == k::LIST && hid_n(hit) == 5 && hid_sub(hit) == 0 {
                    self.via = None;
                }
            }
            Event::Press(hit, button) if button & 15 == 0 => {
                let (kind, id, sub) = (hid_kind(hit), hid_n(hit), hid_sub(hit));
                match (kind, id) {
                    (k::LIST, 4) => self.toggle_versions(m, sub as usize),
                    (k::LIST, 5) => {
                        if sub == 0 {
                            self.via = None;
                        }
                    }
                    (k::LIST, 6) => {
                        if let Some(i) = self.via {
                            let versions = v.data.get("versions").and_then(Value::as_array).cloned().unwrap_or_default();
                            let proto = if sub == 0 { 0 } else { versions.get(sub as usize - 1).and_then(|e| e.get(0)).and_then(Value::as_i64).unwrap_or(0) };
                            m.ask(format!("via {i} {proto}"));
                            self.via_pending = Some((i, proto, m.now));
                            self.via = None;
                            m.tick_sound(1.0);
                        }
                    }
                    (k::LIST, 2) => {
                        if button & 16 != 0 {
                            m.ask(format!("join {sub}"));
                        } else {
                            if sub as i64 != self.sel {
                                m.ask(format!("select {sub}"));
                                self.sel = sub as i64;
                                m.tick_sound(1.0);
                            }
                            // held and moved: dragged
                            let row_top = m.anims.get(&(hit ^ 0x7)).copied().unwrap_or(0.0);
                            self.press_row = Some((sub as usize, m.mouse[1] - row_top));
                        }
                    }
                    (k::LIST, 3) => {
                        let lan = v.data.get("lan").and_then(Value::as_array).cloned().unwrap_or_default();
                        if let Some(l) = lan.get(sub as usize) {
                            let i = n(l, "i");
                            m.ask(if button & 16 != 0 { format!("join {i}") } else { format!("select {i}") });
                        }
                    }
                    (k::BUTTON, A_JOIN) => m.ask(format!("join {}", self.sel)),
                    (k::BUTTON, A_DIRECT) => m.ask("direct"),
                    (k::BUTTON, A_ADD) => m.ask("add"),
                    (k::BUTTON, A_EDIT) => m.ask(format!("edit {}", self.sel)),
                    (k::BUTTON, A_REMOVE) => self.hold_from = m.now,
                    (k::BUTTON, A_REFRESH) => {
                        self.refresh_at = m.now;
                        m.ask("refresh");
                    }
                    (k::BUTTON, A_UNDO) => m.ask("undo"),
                    (k::BACK, _) => m.ask("esc"),
                    _ => {}
                }
            }
            Event::Release(_, _) => {
                self.hold_from = -1.0;
                if let Some((i, grab)) = self.drag.take() {
                    let top = 21.0 * m.vh();
                    let slot = (((m.mouse[1] - grab - top + self.scroll) / self.row_h).round() as i64).clamp(0, servers as i64 - 1);
                    if slot as usize != i {
                        m.ask(format!("move {i} {slot}"));
                    }
                }
                self.press_row = None;
            }
            // the wheel over the open version list leaves it as it is; elsewhere it closes it and the rows go
            Event::Scroll(_) if self.via.is_some() && m.over(self.via_box) => {}
            Event::Scroll(dy) => {
                self.via = None;
                self.scroll_to -= dy * 6.0 * m.vh();
            }
            Event::Key(name, _) if self.via.is_some() && name == "Escape" => self.via = None,
            Event::Key(name, _) => match name.as_str() {
                "Escape" => m.ask("esc"),
                "Return" | "Keypad Enter" if self.sel >= 0 => m.ask(format!("join {}", self.sel)),
                "Up" if self.sel > 0 => m.ask(format!("select {}", self.sel - 1)),
                "Down" if (self.sel + 1) < servers as i64 => m.ask(format!("select {}", self.sel + 1)),
                _ => {}
            },
            _ => {}
        }
    }

    /// The pointer moved with a row held: past a few px it is dragged.
    pub fn moved(&mut self, m: &Menus) {
        if let (Some((i, grab)), None) = (self.press_row, self.drag) {
            if m.held != 0 && (m.mouse[1] - m.held_pos[1]).abs() > 8.0 {
                self.drag = Some((i, grab));
            }
        }
    }
}
