//! Social interactions: the players one plays with. The game's three tabs as words, its search as a field, each player
//! a row — the face (the game draws it), the name, how it stands, hide / show and report as small words on the right;
//! what the game says when a tab is empty; the blocking help as a word at the foot.

use super::controls::{hid, hid_kind, hid_n, hid_sub, k, Field};
use super::lang::tr;
use super::ui::{clamp01, ease, tw, WordStyle, BONE, CAP, DIM};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::{fonts, Face};
use serde_json::Value;

pub struct Social {
    scroll: f32,
    scroll_to: f32,
    field: Option<Field>,
    /// The tab shown, when it changed; the rows of the tab before and how far down (they sink as the new rise).
    tab: i64,
    switched: f32,
    rows_now: Vec<Value>,
    old_rows: Vec<Value>,
    old_scroll: f32,
    /// What an empty tab said last.
    empty_said: String,
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

impl Social {
    pub fn new() -> Social {
        Social { scroll: 0.0, scroll_to: 0.0, field: None, tab: -1, switched: -10.0, rows_now: Vec::new(), old_rows: Vec::new(), old_scroll: 0.0, empty_said: String::new() }
    }

    pub fn typing(&self) -> bool {
        self.field.is_some()
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
        c.veil([0.0, 0.0, w, h], grey(0.0, 0.3 * shade), 1.0, 0.0);
        m.title(c, v, &s(d, "title").to_uppercase(), left, 6.0 * vh, m.cl(34.0, 120.0, 7.6));
        let cap = m.cl(9.0, 18.0, 1.15);

        // the tabs, the server's line on the right
        let tsize = m.cl(16.0, 45.0, 2.9);
        let chosen = n(d, "tab").max(0) as usize;
        let mut x = left;
        let tabs: Vec<Value> = d.get("tabs").and_then(Value::as_array).cloned().unwrap_or_default();
        let ty = 15.4 * vh;
        for (i, t) in tabs.iter().enumerate() {
            let label = t.get(0).and_then(Value::as_str).unwrap_or("").to_uppercase();
            let wid = t.get(1).and_then(Value::as_i64).unwrap_or(-1);
            let id = hid(k::BUTTON, wid.max(0) as u32, 0);
            let on = i == chosen;
            let p = m.vis(v, 1.0 + i as f32 * 0.3);
            let st = WordStyle { glow: 0.0, ..WordStyle::new(Face::Light, tsize).rest(if on { 1.0 } else { DIM }) };
            let r = m.word(c, id, &label, x, ty, st, p, live && wid >= 0, false);
            let kk = m.an(id ^ 0x50c1, if on { 1.0 } else { 0.0 }, 6.0);
            if kk > 0.01 {
                c.light(x, r[3] - 0.06 * tsize, x + tw(Face::Light, &label, tsize, 0.02), 0.85 * kk * p, 0.0, ease(kk), 0.6, ui);
            }
            x = r[2] + 3.2 * vh;
        }
        let server = s(d, "server").to_uppercase();
        if !server.is_empty() && !narrow {
            let sw = tw(Face::Caps, &server, cap, 0.18);
            m.caps(c, &server, right - sw, ty + tsize * 1.12 - cap * 1.6, cap, 0.18, CAP + 0.1, clamp01(ease(m.vis(v, 2.0)) * 1.15));
        }

        // the search
        let sid = hid(k::FIELD, n(d, "searchId").max(0) as u32, 0);
        let fw = (right - left).min(34.0 * w / 100.0);
        let fr = m.field(c, sid, s(d, "search"), s(d, "hint"), left, 22.0 * vh, fw, m.vis(v, 2.0), live, self.field.as_ref());

        // the players
        let top = fr[3] + 3.0 * vh;
        let bottom = h - 13.0 * vh;
        let rows: Vec<Value> = d.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
        // another tab: the rows before sink into their lines as the new ones rise after them
        if n(d, "tab") != self.tab {
            if self.tab >= 0 {
                self.old_rows = std::mem::take(&mut self.rows_now);
                self.old_scroll = self.scroll;
                self.switched = m.now;
                self.scroll_to = 0.0;
            }
            self.tab = n(d, "tab");
        }
        self.rows_now.clone_from(&rows);
        let since_sw = m.now - self.switched;
        if since_sw > 0.5 {
            self.old_rows.clear();
        }
        let nsize = m.cl(16.0, 44.0, 2.9);
        let row = (nsize * 1.25 + cap * 1.4).max(7.2 * vh);
        let face = row * 0.62;
        self.scroll = m.scroll_ease(hid(k::PAGE, 820, 0), self.scroll_to);
        c.push_clip([left - 2.0 * vh, top - 0.5 * vh, right + 1.0, bottom]);
        let small = cap * 1.05;
        let old_rows = self.old_rows.clone();
        for (list, old) in [(&old_rows, true), (&rows, false)] {
        let y = top - if old { self.old_scroll } else { self.scroll };
        for (i, r) in list.iter().enumerate() {
            let cy = y + i as f32 * row;
            if cy < top - row || cy > bottom {
                continue;
            }
            let gate = if old {
                1.0 - clamp01((since_sw - i as f32 * 0.03) / 0.22)
            } else if self.switched < 0.0 {
                1.0
            } else {
                clamp01((since_sw - 0.14 - i as f32 * 0.05) / 0.35)
            };
            let p = m.vis(v, 2.5 + (i as f32 * 0.12).min(5.0)).min(gate);
            let q = clamp01(ease(p) * 1.15);
            if q <= 0.0 {
                continue;
            }
            let gone = b(r, "gone");
            // only while the face is wholly inside the list (the game draws it over everything); the faces of the
            // tab before are the game's no longer
            let fy = cy + (row - face) / 2.0;
            if !old && fy >= top - 1.0 && fy + face <= bottom + 1.0 {
                m.extra(2, n(r, "face") as f32, [left, fy, face], q * if gone { 0.45 } else { 1.0 });
            }
            let tx = left + face + 2.0 * vh;
            let name_top = cy + (row - nsize * 1.1 - cap * 1.4) / 2.0;
            m.rising_text(c, Face::Light, &s(r, "name").to_uppercase(), tx, name_top, nsize, 1.1, 0.02, grey(if gone { 0.5 } else { 0.92 }, 1.0), 0.0, q);
            let status = s(r, "status").to_uppercase();
            if !status.is_empty() {
                m.caps(c, &status, tx, name_top + nsize * 1.1, cap, 0.16, CAP, q);
            }
            // hide / show, report: small words on the right
            let mut rx = right;
            let actions = [("report", s(r, "reportWord"), b(r, "reportOn")), ("show", s(r, "showWord"), true), ("hide", s(r, "hideWord"), true)];
            for (key, word, on) in actions {
                let wid = n(r, key);
                if wid < 0 || word.is_empty() {
                    continue;
                }
                let label = word.to_uppercase();
                let lw = tw(Face::Caps, &label, small, 0.2);
                rx -= lw;
                let id = hid(k::BUTTON, wid as u32, 0);
                let br = [rx - 0.6 * vh, cy, rx + lw + 0.6 * vh, cy + row];
                if live && on && !old {
                    m.hit(br, id);
                }
                let hot = if old { 0.0 } else { m.an(id ^ 0x50ca, if live && on && m.over(br) { 1.0 } else { 0.0 }, 6.0) };
                let ly = cy + (row - small * 1.6) / 2.0;
                m.rising_text(c, Face::Caps, &label, rx, ly, small, 1.6, 0.2, grey(if on { 0.45 + 0.55 * hot } else { 0.25 }, 1.0), 0.0, q);
                if hot > 0.01 {
                    c.light(rx, ly + small * 1.5, rx + lw, 0.8 * hot, 0.0, hot, 0.3, ui);
                }
                rx -= 3.0 * vh;
            }
            c.line([tx, cy + row - 0.5], [right, cy + row - 0.5], 1.0, grey(1.0, 0.045 * q), 0.0);
        }
        }
        let y = top - self.scroll + rows.len() as f32 * row;
        c.pop_clip();
        let content = y + self.scroll - top;
        self.scroll_to = self.scroll_to.clamp(0.0, (content - (bottom - top)).max(0.0));
        // what the game says of an empty tab: it rises after the rows went, and sinks when rows come
        let empty = s(d, "empty").to_uppercase();
        if rows.is_empty() {
            self.empty_said = if empty.is_empty() { tr("nothing").to_uppercase() } else { empty };
        }
        let gate = if self.switched < 0.0 { 1.0 } else { clamp01((since_sw - 0.14) / 0.35) };
        let empty_q = if rows.is_empty() { gate } else { 1.0 - clamp01(since_sw / 0.22) };
        if empty_q > 0.0 && !self.empty_said.is_empty() {
            let lines = fonts().wrap_ls(Face::Caps, cap, &self.empty_said, w * 0.5, 0.22);
            let mut ly = top + 6.0 * vh;
            for l in lines {
                let lw = tw(Face::Caps, &l, cap, 0.22);
                m.caps(c, &l, (w - lw) / 2.0, ly, cap, 0.22, 0.5, clamp01(ease(m.vis(v, 3.0).min(empty_q)) * 1.15));
                ly += cap * 1.7;
            }
        }

        // the foot: back (Esc) on the left, the blocking help on the right
        let fsize = m.cl(18.0, 50.0, 3.2);
        let by = h - 5.0 * vh - 1.12 * fsize;
        let pb = m.vis(v, 6.0);
        let r = m.word(c, hid(k::BACK, 0, 0), &tr("back").to_uppercase(), left, by, WordStyle::new(Face::Thin, fsize), pb, live, false);
        m.caps(c, "ESC", r[2] + 1.6 * vh, by + 1.12 * fsize - cap * 1.6, cap, 0.22, 0.29, clamp01(ease(pb) * 1.15));
        let hint_id = n(d, "hintId");
        if hint_id >= 0 && !narrow {
            let label = s(d, "hintWord").to_uppercase();
            let lw = tw(Face::Caps, &label, cap, 0.2);
            let id = hid(k::BUTTON, hint_id as u32, 0);
            let hy = by + 1.12 * fsize - cap * 1.6;
            let br = [right - lw, hy - 0.5 * vh, right, hy + cap * 1.6];
            if live {
                m.hit(br, id);
            }
            let hot = m.an(id ^ 0x1e1b, if live && m.over(br) { 1.0 } else { 0.0 }, 6.0);
            m.rising_text(c, Face::Caps, &label, right - lw, hy, cap, 1.6, 0.2, grey(0.4 + 0.6 * hot, 1.0), 0.0, clamp01(ease(pb) * 1.15));
            if hot > 0.01 {
                c.light(right - lw, hy + cap * 1.5, right, 0.8 * hot, 0.0, hot, 0.3, ui);
            }
        }
        let _ = BONE;
        m.out[super::out::TYPING] = if self.field.is_some() { 1.0 } else { 0.0 };
    }

    pub fn event(&mut self, m: &mut Menus, _v: &View, e: Event) {
        match e {
            Event::Press(hit, button) if button & 15 == 0 => {
                if self.field.as_ref().is_some_and(|f| f.id != hit) {
                    self.field = None;
                }
                match hid_kind(hit) {
                    k::BUTTON => {
                        m.ask(format!("press {}", hid_n(hit)));
                        m.tick_sound(0.7);
                    }
                    k::FIELD => {
                        if self.field.is_none() {
                            let text = _v.data.get("search").and_then(Value::as_str).unwrap_or("");
                            self.field = Some(Field::new(hit, text, 16, m.now));
                        }
                    }
                    k::BACK => m.ask("esc"),
                    _ => {}
                }
                let _ = hid_sub(hit);
            }
            Event::Scroll(dy) => self.scroll_to -= dy * 6.0 * m.vh(),
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
