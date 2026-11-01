//! The game's screens that say something and wait (connecting, loading a world, saving, a message, a question, an
//! error) as one page: what it is about small above, the title large on the left, the message under it, a hair of
//! light for how far it has got (or a spark running along it while there is no measure), the chunks being made round
//! where one will stand as a field of points, the screen's buttons as words and its checkboxes as a row.

use super::controls::{hid, hid_kind, hid_n, k};
use super::lang::tr;
use super::ui::{clamp01, ease, ease_out, toward, tw, WordStyle, BONE, CAP, DIM, SOFT};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::{fonts, Face};
use serde_json::Value;

pub struct Notice {
    /// The shown share (eases to the game's), the main word's number (Enter presses it).
    shown: f32,
    main: Option<u32>,
    /// Each chunk's light (eases to its state).
    cells: Vec<f32>,
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

impl Notice {
    pub fn new() -> Notice {
        Notice { shown: 0.0, main: None, cells: Vec::new() }
    }

    /// Going into a world (v12, the launch's GN): the page is only the world's name over the horizon's line — the line,
    /// the loading along it and the world unloading are the launch's picture under it.
    fn draw_enter(&mut self, m: &mut Menus, c: &mut Canvas, v: &View, e: &Value) {
        use crate::launch::timeline::{gn, layout, smooth};
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        m.scene_pose([0.0, 22.0, -0.085, 0.0, m.scene_to[4]], 0.0, 0.0);
        let t = m.now - e.get("since").and_then(Value::as_f64).unwrap_or(m.now as f64) as f32;
        let lay = layout(w / ui, h / ui, 0.0, 0.0, 115.5);
        let (tip_x, tip_y) = (lay.tip[0] * ui, lay.tip[1] * ui);
        let away = match v.leaving {
            Some(at) => 1.0 - smooth(0.0, 0.45, m.now - at),
            None => 1.0,
        };
        let wp = smooth(0.0, 1.0, (t - gn::NAME_FROM) / (gn::NAME_TO - gn::NAME_FROM)).min(away);
        if wp <= 0.0 {
            return;
        }
        let name = s(e, "name").to_uppercase();
        let kind = if n(e, "kind") == 1 { tr("multi") } else { tr("single") }.to_uppercase();
        let s1 = (h * 0.054).clamp(26.0 * ui, 62.0 * ui);
        let s2 = (h * 0.011).clamp(8.0 * ui, 12.0 * ui);
        let gap = h * 0.011;
        let bottom = tip_y - (18.0 * ui).max(0.04 * lay.vh * ui);
        let top2 = bottom - s2 * 1.3;
        let top1 = top2 - gap - s1 * 1.06;
        let w1 = tw(Face::Light, &name, s1, 0.08);
        let w2 = tw(Face::Caps, &kind, s2, 0.3);
        let q = |i: f32| clamp01(ease_out(0.0, 1.0, wp * 1.15 - i * 0.15));
        m.rising_text(c, Face::Light, &name, tip_x - w1 / 2.0, top1, s1, 1.06, 0.08, grey(BONE, 1.0), 0.0, q(0.0));
        m.rising_text(c, Face::Caps, &kind, tip_x - w2 / 2.0, top2, s2, 1.3, 0.3, grey(0.54, 1.0), 0.0, q(1.0));
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        if let Some(e) = v.data.get("enter").filter(|e| e.is_object()) {
            let e = e.clone();
            self.draw_enter(m, c, v, &e);
            return;
        }
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        let vh = m.vh();
        let narrow = w < 900.0 * ui;
        let left = if narrow { 20.0 * ui } else { w * 0.062 };
        let right = w - if narrow { 20.0 * ui } else { w * 0.06 };
        let live = v.leaving.is_none() && m.now - v.opened > 0.2;
        let d = v.data;
        let grid = s(d, "grid");
        let has_grid = !grid.is_empty() && !narrow;
        let text_w = (right - left).min(if has_grid { w * 0.46 } else { w * 0.56 });
        m.scene_pose([8.0, 20.5, -0.07, 0.03, m.scene_to[4]], 0.16, 0.22);
        let shade = m.vis(v, 0.0);
        c.veil([0.0, 0.0, w * 0.66, h], grey(0.0, if m.in_world { 0.32 } else { 0.4 } * shade), 30.0 * vh, 1.0);

        // what goes in the column, measured first: it stands a little above the middle
        let about = s(d, "about").to_uppercase();
        let cap = m.cl(9.0, 18.0, 1.15);
        let mut tsize = m.cl(30.0, 104.0, 6.8);
        let title = s(d, "title").to_uppercase();
        let mut tlines = fonts().wrap_ls(Face::Light, tsize, &title, text_w * 1.25, 0.02);
        while tlines.len() > 2 && tsize > 24.0 * ui {
            tsize *= 0.86;
            tlines = fonts().wrap_ls(Face::Light, tsize, &title, text_w * 1.25, 0.02);
        }
        let msize = m.cl(10.0, 19.0, 1.6);
        let message: Vec<String> = d.get("message").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        let mlines: Vec<Vec<String>> = message.iter().map(|p| fonts().wrap_ls(Face::Caps, msize, p, text_w, 0.04)).collect();
        let stage = s(d, "stage");
        let link = s(d, "link");
        let progress = d.get("progress").and_then(Value::as_f64).unwrap_or(-1.0) as f32;
        let items: Vec<Value> = d.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let buttons: Vec<&Value> = items.iter().filter(|it| s(it, "t") == "button").collect();
        let checks: Vec<&Value> = items.iter().filter(|it| s(it, "t") == "check").collect();
        let wsize = m.cl(20.0, 58.0, 3.9);
        let row_h = (5.0 * vh).max(34.0 * ui);
        let mut col_h = tlines.len() as f32 * tsize * 1.04;
        if !about.is_empty() {
            col_h += cap * 1.4 + 1.6 * vh;
        }
        if !stage.is_empty() {
            col_h += cap * 1.6 + 1.0 * vh;
        }
        for l in &mlines {
            col_h += 1.6 * vh + l.len() as f32 * msize * 1.6;
        }
        if !link.is_empty() {
            col_h += 1.4 * vh + msize * 1.6;
        }
        let bar = progress >= 0.0 || (progress + 2.0).abs() < 0.01;
        if bar {
            col_h += 4.0 * vh;
        }
        col_h += checks.len() as f32 * row_h;
        if !buttons.is_empty() {
            col_h += 4.0 * vh + 1.12 * wsize;
        }
        let mut y = (h * 0.47 - col_h * 0.5).max(14.0 * vh);
        let mut order = 0.0f32;

        if !about.is_empty() {
            let p = m.vis(v, order);
            let q = clamp01(ease_out(0.22, 1.0, p) * 1.15);
            m.rising_text(c, Face::Caps, &about, left, y, cap, 1.4, 0.22, grey(CAP + 0.1, 1.0), 0.0, q);
            m.reveal_line(c, left, y + cap * 1.4 + 0.5 * vh, left + tw(Face::Caps, &about, cap, 0.22), p);
            y += cap * 1.4 + 1.6 * vh;
            order += 1.0;
        }
        // the title: one line flies in from the word that opened the screen (or rises); more rise one by one
        if tlines.len() == 1 {
            m.title(c, v, &tlines[0], left, y, tsize);
            y += tsize * 1.04;
        } else {
            for (i, l) in tlines.iter().enumerate() {
                let p = m.vis(v, order + i as f32 * 0.5);
                let q = clamp01(ease_out(0.22, 1.0, p) * 1.15);
                m.rising_text(c, Face::Light, l, left, y, tsize, 0.98, 0.02, grey(1.0, 1.0), 0.0, q);
                y += tsize * 1.04;
            }
        }
        order += 1.0;
        if !stage.is_empty() {
            y += 1.0 * vh;
            let p = m.vis(v, order);
            m.caps(c, &stage.to_uppercase(), left, y, cap, 0.18, SOFT, clamp01(ease(p) * 1.15));
            y += cap * 1.6;
            order += 1.0;
        }
        for lines in &mlines {
            y += 1.6 * vh;
            let p = m.vis(v, order);
            let q = clamp01(ease(p) * 1.15);
            for l in lines {
                m.rising_text(c, Face::Caps, l, left, y, msize, 1.6, 0.04, grey(SOFT + 0.08, 1.0), 0.0, q);
                y += msize * 1.6;
            }
            order += 1.0;
        }
        if !link.is_empty() {
            y += 1.4 * vh;
            let p = m.vis(v, order);
            let l = fonts().fit_ls(Face::Caps, msize, link, text_w, 0.04);
            m.rising_text(c, Face::Caps, &l, left, y, msize, 1.6, 0.04, grey(BONE, 1.0), 0.0, clamp01(ease(p) * 1.15));
            y += msize * 1.6;
            order += 1.0;
        }
        // how far: a hair of light filled to the share (its number at its end), or a spark running along it
        if bar {
            y += 2.4 * vh;
            let p = m.vis(v, order);
            let q = ease(p);
            let x1 = left + text_w;
            c.line([left, y], [left + (x1 - left) * q, y], 1.0 * ui, grey(1.0, 0.09 * q), 0.0);
            if progress >= 0.0 {
                self.shown = toward(self.shown, progress.clamp(0.0, 1.0), 5.0, m.dt);
                c.light(left, y, x1, 0.95 * q, 0.0, self.shown, 0.7, ui);
                let pct = format!("{}%", (self.shown * 100.0).round() as i32);
                let pw = tw(Face::Caps, &pct, cap, 0.18);
                m.caps(c, &pct, x1 - pw, y + 0.9 * vh, cap, 0.18, SOFT, clamp01(q * 1.15));
            } else {
                // no measure: a spark comes from the left, runs to the right and goes
                let t = (m.now - v.opened) / 1.7;
                let f = t - t.floor();
                let head = ease_out(0.0, 1.0, f) * 1.35;
                c.light(left, y, x1, 0.9 * q, (head - 0.35).max(0.0), head.min(1.0), 0.8, ui);
            }
            y += 1.6 * vh;
            order += 1.0;
        }
        // the checkboxes: a caption and its On / Off
        for it in &checks {
            let p = m.vis(v, order);
            let wid = n(it, "id").max(0) as u32;
            let rid = hid(k::ROW, wid, 0);
            let r = [left, y, left + text_w, y + row_h];
            if live {
                m.hit(r, rid);
            }
            let on = it.get("on").and_then(Value::as_bool).unwrap_or(true);
            m.row_caption(c, rid, s(it, "label"), left, y, row_h, text_w, p, on);
            let words = vec![tr("on").to_string(), tr("off").to_string()];
            m.choice(c, k::CHOICE, wid, &words, Some(if b(it, "v") { 0 } else { 1 }), left + text_w, y, row_h, p, live, on);
            y += row_h;
            order += 1.0;
        }
        // the buttons: the first that goes on (not back) is the main word, lit brighter; the rest as the others
        self.main = buttons.iter().find(|it| !b(it, "back")).or(buttons.first()).map(|it| n(it, "id").max(0) as u32);
        if !buttons.is_empty() {
            y += 4.0 * vh;
            let mut x = left;
            let p = m.vis(v, order + 0.5);
            let mut sorted = buttons.clone();
            sorted.sort_by_key(|it| (b(it, "back"), n(it, "x")));
            for it in sorted {
                let wid = n(it, "id").max(0) as u32;
                let main = Some(wid) == self.main;
                let st = WordStyle::new(if main { Face::Light } else { Face::Thin }, wsize).rest(if main { BONE } else { DIM }).enabled(it.get("on").and_then(Value::as_bool).unwrap_or(true));
                let r = m.word(c, hid(k::BUTTON, wid, 0), &s(it, "s").to_uppercase(), x, y, st, p, live, false);
                x = r[2] + 3.6 * vh;
                if b(it, "back") && b(d, "esc") {
                    m.caps(c, "ESC", r[2] + 1.4 * vh, y + 1.12 * wsize - cap * 1.6, cap, 0.22, 0.29, clamp01(ease(p) * 1.15));
                    x += 1.4 * vh + tw(Face::Caps, "ESC", cap, 0.22);
                }
            }
        }

        // the chunks being made round where one will stand: points brightening as each gets further
        if has_grid {
            let dd = n(d, "gridD").max(1) as usize;
            let cells: Vec<u8> = grid.bytes().collect();
            if self.cells.len() != cells.len() {
                self.cells = vec![0.0; cells.len()];
            }
            let span = (h * 0.42).min((right - left - text_w) * 0.7);
            let step = span / dd as f32;
            let cx = left + text_w + (right - left - text_w) * 0.55;
            let cy = h * 0.47;
            let x0 = cx - span / 2.0;
            let y0 = cy - span / 2.0;
            let p = m.vis(v, 2.0);
            let q = ease(p);
            for (i, &ch) in cells.iter().enumerate() {
                let target = if ch == b'.' { 0.0 } else { ((ch - b'a') as f32 + 1.0) / 11.0 };
                self.cells[i] = toward(self.cells[i], target.min(1.0), 6.0, m.dt);
                let lv = self.cells[i];
                let (gx, gy) = ((i % dd) as f32, (i / dd) as f32);
                // round: the corners of the square stay out
                let r = ((gx + 0.5 - dd as f32 / 2.0).powi(2) + (gy + 0.5 - dd as f32 / 2.0).powi(2)).sqrt() / (dd as f32 / 2.0);
                if r > 1.02 {
                    continue;
                }
                let sz = step * (0.18 + 0.34 * lv);
                let px = x0 + (gx + 0.5) * step;
                let py = y0 + (gy + 0.5) * step;
                let a = (0.10 + 0.85 * lv) * q * (1.0 - 0.35 * r);
                c.rect([px - sz / 2.0, py - sz / 2.0, px + sz / 2.0, py + sz / 2.0], sz * 0.5, grey(1.0, a));
            }
        }
    }

    pub fn event(&mut self, m: &mut Menus, v: &View, e: Event) {
        match e {
            Event::Press(hit, button) if button & 15 == 0 => match hid_kind(hit) {
                k::BUTTON => {
                    m.ask(format!("press {}", hid_n(hit)));
                    m.tick_sound(0.7);
                }
                k::CHOICE => {
                    let wid = hid_n(hit);
                    let want = super::controls::hid_sub(hit) == 0;
                    let cur = v.data.get("items").and_then(Value::as_array).and_then(|a| a.iter().find(|it| n(it, "id") == wid as i64)).map(|it| b(it, "v"));
                    if cur.is_some_and(|c| c != want) {
                        m.ask(format!("press {wid}"));
                    }
                }
                _ => {}
            },
            Event::Key(name, _) => match name.as_str() {
                "Escape" => m.ask("key Escape"),
                "Return" | "Keypad Enter" => {
                    if let Some(id) = self.main {
                        m.ask(format!("press {id}"));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}
