//! Packs (resource packs, a new world's data packs): the available ones on the left, the chosen on the right — each
//! its icon (the game draws it), its name, its description in small capitals, a mark when it does not fit this
//! version; a click moves it across, gliding from its place in one column to its place in the other; the chosen ones
//! have up and down; the search above, the folder and done at the foot.

use super::controls::{hid, hid_kind, hid_n, k, Field};
use super::lang::tr;
use super::ui::{clamp01, ease, tw, WordStyle, CAP};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, rgba, Canvas};
use crate::gui::text::{fonts, Face};
use serde_json::Value;

const ROW: u32 = 41;
const UP: u32 = 42;
const DOWN: u32 = 43;
const FOLDER: u32 = 44;

pub struct Packs {
    scroll: [f32; 2],
    scroll_to: [f32; 2],
    field: Option<Field>,
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn n(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(-1)
}

/// A pack's own number for its motion (by its id: the same pack in either column).
fn key(id: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in id.bytes() {
        h = (h ^ byte as u64).wrapping_mul(0x100_0000_01b3);
    }
    h & 0x0000_ffff_ffff_ffff
}

impl Packs {
    pub fn new() -> Packs {
        Packs { scroll: [0.0; 2], scroll_to: [0.0; 2], field: None }
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        let vh = m.vh();
        let narrow = w < 900.0 * ui;
        let left = if narrow { 20.0 * ui } else { w * 0.062 };
        let right = w - if narrow { 20.0 * ui } else { w * 0.06 };
        let live = v.leaving.is_none() && m.now - v.opened > 0.2;
        let d = v.data;
        if !m.in_world {
            m.scene_pose([8.0, 20.5, -0.07, 0.03, m.scene_to[4]], 0.3, 0.0);
        }
        let shade = m.vis(v, 0.0);
        c.veil([0.0, 0.0, w, h], grey(0.0, 0.32 * shade), 1.0, 0.0);
        m.title(c, v, &s(d, "title").to_uppercase(), left, 6.0 * vh, m.cl(34.0, 120.0, 7.6));
        let cap = m.cl(9.0, 18.0, 1.15);
        // what one can do: drop files here
        m.caps(c, &s(d, "drop").to_uppercase(), left, 15.6 * vh, cap, 0.18, CAP, clamp01(ease(m.vis(v, 1.0)) * 1.15));
        let mut top = 19.5 * vh;
        if n(d, "searchId") >= 0 {
            let sid = hid(k::FIELD, n(d, "searchId") as u32, 0);
            let fr = m.field(c, sid, s(d, "search"), s(d, "hint"), left, top, (right - left).min(34.0 * w / 100.0), m.vis(v, 1.5), live, self.field.as_ref());
            top = fr[3] + 2.4 * vh;
        }
        let bottom = h - 13.0 * vh;
        let gap = 4.0 * vw(w);
        let col_w = (right - left - gap) / 2.0;
        let tsize = m.cl(15.0, 40.0, 2.6);
        let dsize = cap * 0.95;
        let row = (tsize * 1.15 + dsize * 1.5 * 2.0 + 1.2 * vh).max(8.4 * vh);
        let icon = row - 1.6 * vh;
        for (side, (list_key, head_key)) in [("left", "available"), ("right", "selected")].iter().enumerate() {
            let x0 = left + side as f32 * (col_w + gap);
            let x1 = x0 + col_w;
            let ph = m.vis(v, 2.0 + side as f32 * 0.3);
            m.caps(c, &s(d, head_key).to_uppercase(), x0, top, cap, 0.22, 0.55, clamp01(ease(ph) * 1.15));
            m.reveal_line(c, x0, top + cap * 1.5 + 0.4 * vh, x1, ph);
            let ltop = top + cap * 1.5 + 1.6 * vh;
            let rows: Vec<Value> = d.get(*list_key).and_then(Value::as_array).cloned().unwrap_or_default();
            self.scroll[side] = m.scroll_ease(hid(k::PAGE, 830 + side as u32, 0), self.scroll_to[side]);
            c.push_clip([x0 - 1.0 * vh, ltop - 0.4 * vh, x1 + 1.0 * vh, bottom]);
            for (i, r) in rows.iter().enumerate() {
                let pid = key(s(r, "id"));
                // where it is: gliding there (across, when it moved to the other column)
                let tx = x0;
                let ty = ltop + i as f32 * row - self.scroll[side];
                let px = m.an(pid ^ 0x7000_0000_0000, tx, 9.0);
                let py = m.an(pid ^ 0x7100_0000_0000, ty, 9.0);
                if py < ltop - row || py > bottom {
                    continue;
                }
                let p = m.vis(v, 2.5 + (i as f32 * 0.12).min(5.0));
                let q = clamp01(ease(p) * 1.15);
                let num = n(r, "n").max(0) as u32;
                let rid = hid(ROW, num, 0);
                let rr = [px - 0.6 * vh, py, px + col_w + 0.6 * vh, py + row];
                let movable = b(r, "sel") || b(r, "unsel");
                if live && movable && py >= ltop - 1.0 && py + row <= bottom + 1.0 {
                    m.hit(rr, rid);
                }
                let hot = m.an(rid ^ 0x9ac4, if live && movable && m.over(rr) { 1.0 } else { 0.0 }, 8.0);
                if hot > 0.01 {
                    c.rect(rr, 0.6 * vh, grey(1.0, 0.045 * hot * q));
                }
                let ok = b(r, "ok");
                let iy = py + (row - icon) / 2.0;
                if iy >= ltop - 1.0 && iy + icon <= bottom + 1.0 {
                    m.extra(4, n(r, "icon") as f32, [px, iy, icon], q);
                }
                let tx2 = px + icon + 1.6 * vh;
                let tw_max = col_w - icon - 1.6 * vh - 4.0 * vh;
                let title = fonts().fit_ls(Face::Light, tsize, &s(r, "title").to_uppercase(), tw_max, 0.02);
                m.rising_text(c, Face::Light, &title, tx2, py + 0.6 * vh, tsize, 1.15, 0.02, grey(0.92, 1.0), 0.0, q);
                let desc = if ok { s(r, "desc").to_string() } else { s(r, "why").to_string() };
                let lines = fonts().wrap_ls(Face::Caps, dsize, &desc, tw_max, 0.06);
                let mut dy = py + 0.6 * vh + tsize * 1.15;
                for l in lines.iter().take(2) {
                    let col = if ok { grey(0.55, 1.0) } else { rgba(0.95, 0.45, 0.4, 1.0) };
                    m.rising_text(c, Face::Caps, l, tx2, dy, dsize, 1.5, 0.06, col, 0.0, q);
                    dy += dsize * 1.5;
                }
                // what a click does, said at the right while the pointer is on it
                if hot > 0.01 {
                    let word = tr(if b(r, "sel") { "add" } else { "remove1" }).to_uppercase();
                    let ww = tw(Face::Caps, &word, cap, 0.2);
                    let wx = px + col_w - ww - if side == 1 { 3.2 * vh } else { 0.0 };
                    m.rising_text(c, Face::Caps, &word, wx, py + 0.8 * vh, cap, 1.4, 0.2, grey(0.9, hot * q), 0.0, 1.0);
                }
                // up and down for the chosen
                if side == 1 {
                    let ax = px + col_w - 1.6 * vh;
                    for (j, (kind, can)) in [(UP, b(r, "up")), (DOWN, b(r, "down"))].iter().enumerate() {
                        let cy = py + row * (0.3 + 0.4 * j as f32);
                        let ar = [ax - 1.4 * vh, cy - 1.4 * vh, ax + 1.4 * vh, cy + 1.4 * vh];
                        let aid = hid(*kind, num, 0);
                        if live && *can {
                            m.hit(ar, aid);
                        }
                        let ah = m.an(aid ^ 0x55, if live && *can && m.over(ar) { 1.0 } else { 0.0 }, 8.0);
                        let a = if *can { (0.35 + 0.6 * ah) * q } else { 0.1 * q };
                        let sgn = if j == 0 { -1.0 } else { 1.0 };
                        let s_ = 0.55 * vh;
                        c.line([ax - s_, cy - sgn * s_ * 0.5], [ax, cy + sgn * s_ * 0.5], 1.4 * ui, grey(1.0, a), 0.0);
                        c.line([ax, cy + sgn * s_ * 0.5], [ax + s_, cy - sgn * s_ * 0.5], 1.4 * ui, grey(1.0, a), 0.0);
                    }
                }
                c.line([tx2, py + row - 0.5], [px + col_w, py + row - 0.5], 1.0, grey(1.0, 0.04 * q), 0.0);
            }
            c.pop_clip();
            let content = rows.len() as f32 * row;
            self.scroll_to[side] = self.scroll_to[side].clamp(0.0, (content - (bottom - ltop)).max(0.0));
            if rows.is_empty() {
                let t = tr("nothing").to_uppercase();
                m.caps(c, &t, x0, ltop + 2.0 * vh, cap, 0.22, 0.35, clamp01(ease(ph) * 1.15));
            }
        }

        // the foot: done (Esc) on the left, the folder on the right
        let fsize = m.cl(18.0, 50.0, 3.2);
        let by = h - 5.0 * vh - 1.12 * fsize;
        let pb = m.vis(v, 6.0);
        let r = m.word(c, hid(k::BACK, 0, 0), &tr("done").to_uppercase(), left, by, WordStyle::new(Face::Thin, fsize).enabled(b(d, "done")), pb, live, false);
        m.caps(c, "ESC", r[2] + 1.6 * vh, by + 1.12 * fsize - cap * 1.6, cap, 0.22, 0.29, clamp01(ease(pb) * 1.15));
        let folder = s(d, "folder").to_uppercase();
        let fw = tw(Face::Thin, &folder, fsize, 0.02);
        m.word(c, hid(FOLDER, 0, 0), &folder, right - fw, by, WordStyle::new(Face::Thin, fsize), pb, live, false);
        m.out[super::out::TYPING] = if self.field.is_some() { 1.0 } else { 0.0 };
    }

    pub fn event(&mut self, m: &mut Menus, v: &View, e: Event) {
        match e {
            Event::Press(hit, button) if button & 15 == 0 => {
                if self.field.as_ref().is_some_and(|f| f.id != hit) {
                    self.field = None;
                }
                match hid_kind(hit) {
                    ROW => {
                        m.ask(format!("move {}", hid_n(hit)));
                        m.tick_sound(0.7);
                    }
                    UP => m.ask(format!("up {}", hid_n(hit))),
                    DOWN => m.ask(format!("down {}", hid_n(hit))),
                    FOLDER => m.ask("folder"),
                    k::BACK => m.ask("esc"),
                    k::FIELD => {
                        if self.field.is_none() {
                            self.field = Some(Field::new(hit, s(v.data, "search"), 50, m.now));
                        }
                    }
                    _ => {}
                }
            }
            Event::Scroll(dy) => {
                // the column under the pointer
                let side = if m.mouse[0] > m.w / 2.0 { 1 } else { 0 };
                self.scroll_to[side] -= dy * 6.0 * m.vh();
            }
            Event::Char(ch) => {
                if let Some(fd) = self.field.as_mut() {
                    if fd.insert(ch, m.now) {
                        m.ask(format!("text {} {}", hid_n(fd.id), fd.text));
                    }
                }
            }
            Event::Key(name, mods) => {
                if let Some(fd) = self.field.as_mut() {
                    match name.as_str() {
                        "Escape" | "Return" | "Keypad Enter" => self.field = None,
                        "V" if mods & 2 != 0 => m.ask(format!("paste {}", hid_n(fd.id))),
                        _ => {
                            if fd.key(&name, mods, m.now) {
                                m.ask(format!("text {} {}", hid_n(fd.id), fd.text));
                            }
                        }
                    }
                } else if name == "Escape" {
                    m.ask("esc");
                }
            }
            _ => {}
        }
    }
}

fn vw(w: f32) -> f32 {
    w / 100.0
}
