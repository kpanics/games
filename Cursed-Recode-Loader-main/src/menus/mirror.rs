//! Any of the game's screens with no page of its own, in the menus' way: its title rises (or flies in from the word
//! that opened it), its widgets as the game made them become words, rows with a caption and a control, fields and
//! lists in a column on the left; the buttons at its foot go to the bottom (Back, Cancel, Done on the left with Esc,
//! the rest on the right); a widget's tooltip is said at the bottom.

use super::controls::{hid, hid_kind, hid_n, hid_sub, k, Field};
use super::lang::tr;
use super::ui::{clamp01, ease, smooth, tw, WordStyle, CAP, DIM, SOFT};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::Face;
use serde_json::Value;

pub struct Mirror {
    scroll: f32,
    scroll_to: f32,
    content_h: f32,
    field: Option<Field>,
    drag: Option<(u32, [f32; 4], f32)>,
    tip: String,
    tip_k: f32,
    /// Where each list's rows were drawn (its number, the first row's top, a row's height).
    lists: Vec<(u32, f32, f32, usize)>,
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

impl Mirror {
    pub fn new() -> Mirror {
        Mirror { scroll: 0.0, scroll_to: 0.0, content_h: 0.0, field: None, drag: None, tip: String::new(), tip_k: 0.0, lists: Vec::new() }
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
        let empty = Vec::new();
        let items = v.data.get("items").and_then(Value::as_array).unwrap_or(&empty);
        // the dragged slider follows the pointer
        if let Some((id, r, last)) = self.drag {
            let val = clamp01((m.mouse[0] - r[0]) / (r[2] - r[0]).max(1.0));
            if (val - last).abs() > 0.0005 {
                m.ask(format!("slide {id} {val}"));
                self.drag = Some((id, r, val));
            }
        }

        // the title: risen out of its line, or flown in from the word that opened the screen
        let title = s(v.data, "title").to_uppercase();
        let tsize = m.cl(34.0, 120.0, 7.6);
        let ttop = 6.0 * vh;
        m.title(c, v, &title, left, ttop, tsize);

        // the column: the screen's widgets, foot buttons apart
        let col_top = 21.0 * vh;
        let col_bottom = h - 15.0 * vh;
        let col_w = (right - left).min(if narrow { right - left } else { w * 0.6 });
        let text_w = col_w.min(w * 0.5);
        m.scene_pose([8.0, 20.5, -0.07, 0.03, m.scene_to[4]], 0.16, 0.22);
        if !m.in_world {
            let shade = m.vis(v, 0.0);
            c.veil([0.0, 12.0 * vh, w * 0.75, h], crate::gui::draw::grey(0.0, 0.42 * shade), 14.0 * vh, 1.0);
        }
        c.push_clip([0.0, col_top - 2.0 * vh, w, col_bottom + 1.0 * vh]);
        self.scroll = m.scroll_ease(hid(k::PAGE, v.id, 1), self.scroll_to);
        let mut y = col_top - self.scroll;
        let mut order = 1.0f32;
        let row_h = (5.0 * vh).max(34.0 * ui);
        let mut i = 0;
        self.lists.clear();
        let mut hover_tip: Option<String> = None;
        let word_size = m.cl(18.0, 53.0, 3.4);
        while i < items.len() {
            let it = &items[i];
            let t = s(it, "t");
            if b(it, "foot") {
                i += 1;
                continue;
            }
            let p = m.vis(v, order);
            let wid = n(it, "id").max(0) as u32;
            let on = it.get("on").and_then(Value::as_bool).unwrap_or(true);
            match t {
                "header" => {
                    y += 2.0 * vh;
                    let size = m.cl(9.0, 18.0, 1.15);
                    m.caps(c, &s(it, "s").to_uppercase(), left, y, size, 0.22, CAP, clamp01(ease(p) * 1.15));
                    y += size * 1.3 + 0.8 * vh;
                }
                "text" => {
                    let size = m.cl(10.0, 17.0, 1.5);
                    y = m.paragraph(c, s(it, "s"), left, y, text_w, size, SOFT, clamp01(ease(p) * 1.15), 0.04) + 1.0 * vh;
                }
                "button" | "lock" => {
                    // the buttons of one line of the screen side by side, as words
                    let row_y = n(it, "y");
                    let mut x = left;
                    let mut j = i;
                    while j < items.len() && !b(&items[j], "foot") && matches!(s(&items[j], "t"), "button" | "lock") && n(&items[j], "y") == row_y {
                        let bt = &items[j];
                        let bid = n(bt, "id").max(0) as u32;
                        let id = hid(k::BUTTON, bid, 0);
                        let bon = bt.get("on").and_then(Value::as_bool).unwrap_or(true);
                        let label = if s(bt, "t") == "lock" { tr(if b(bt, "v") { "locked" } else { "lock" }).to_uppercase() } else { s(bt, "s").to_uppercase() };
                        let st = WordStyle::new(Face::Thin, word_size).enabled(bon);
                        let r = m.word(c, id, &label, x, y, st, p, live, false);
                        if m.over(r) && !s(bt, "tip").is_empty() {
                            hover_tip = Some(s(bt, "tip").to_string());
                        }
                        x = r[2] + 2.6 * vh;
                        j += 1;
                    }
                    y += 1.12 * word_size + 0.9 * vh;
                    i = j;
                    order += 1.0;
                    continue;
                }
                "cycle" | "check" | "slider" | "choose" => {
                    let rid = hid(k::ROW, wid, 0);
                    let label = if t == "check" { s(it, "label") } else { s(it, "label") };
                    let r = [left, y, left + col_w, y + row_h];
                    if live {
                        m.hit(r, rid);
                    }
                    if m.over(r) && !s(it, "tip").is_empty() {
                        hover_tip = Some(s(it, "tip").to_string());
                    }
                    m.row_caption(c, rid, label, left, y, row_h, col_w, p, on);
                    let ctl_right = left + col_w;
                    match t {
                        "choose" => {
                            let values: Vec<String> = it.get("values").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                            let index = n(it, "index");
                            m.choice(c, k::CHOICE, wid, &values, (index >= 0).then_some(index as usize), ctl_right, y, row_h, p, live, on);
                        }
                        "check" => {
                            let words = vec![tr("on").to_string(), tr("off").to_string()];
                            m.choice(c, k::CHOICE, wid, &words, Some(if b(it, "v") { 0 } else { 1 }), ctl_right, y, row_h, p, live, on);
                        }
                        "slider" => {
                            let held = self.drag.is_some_and(|d| d.0 == wid);
                            let val = if held { self.drag.unwrap().2 } else { f(it, "v") };
                            let width = (20.0 * w / 100.0).min(col_w * 0.38);
                            let tr_ = m.slider(c, wid, val, s(it, "value"), ctl_right, y, row_h, width, p, live, on);
                            if held {
                                self.drag = Some((wid, tr_, val));
                            }
                        }
                        _ => {
                            let values: Vec<String> = it.get("values").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                            let index = n(it, "index");
                            let total: f32 = values.iter().map(|x| tw(Face::Light, &x.to_uppercase(), m.cl(15.0, 39.0, 2.5), 0.02)).sum::<f32>() + 2.0 * vh * values.len() as f32;
                            if !values.is_empty() && values.len() <= 4 && total < col_w * 0.55 {
                                m.choice(c, k::CHOICE, wid, &values, (index >= 0).then_some(index as usize), ctl_right, y, row_h, p, live, on);
                            } else {
                                m.cycle(c, wid, s(it, "value"), index as i32, ctl_right, y, row_h, p, live, on, 10.0 * w / 100.0);
                            }
                        }
                    }
                    y += row_h;
                }
                "edit" => {
                    let id = hid(k::FIELD, wid, 0);
                    let fw = col_w.min(38.0 * w / 100.0);
                    // what it is for, small over it
                    let label = s(it, "label");
                    if !label.is_empty() {
                        let size = m.cl(9.0, 18.0, 1.15);
                        m.caps(c, &label.to_uppercase(), left, y, size, 0.22, CAP, clamp01(ease(p) * 1.15));
                        y += size * 1.5 + 0.4 * vh;
                    }
                    let r = m.field(c, id, s(it, "value"), s(it, "hint"), left, y, fw, p, live, self.field.as_ref());
                    if m.over(r) && !s(it, "tip").is_empty() {
                        hover_tip = Some(s(it, "tip").to_string());
                    }
                    y = r[3] + 1.6 * vh;
                }
                "list" => {
                    let rows: Vec<&str> = it.get("rows").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                    let sel = n(it, "sel");
                    let size = m.cl(16.0, 45.0, 2.9);
                    let rh = 1.12 * size + 0.4 * vh;
                    self.lists.push((wid, y, rh, rows.len()));
                    for (ri, row) in rows.iter().enumerate() {
                        let pr = m.vis(v, order + ri as f32 * 0.3);
                        if y > col_top - 2.0 * rh && y < col_bottom + rh {
                            let id = hid(k::LIST, wid, ri as u32);
                            let chosen = sel == ri as i64;
                            let st = WordStyle { glow: 0.0, ..WordStyle::new(Face::Light, size).rest(if chosen { 1.0 } else { DIM }) };
                            m.word(c, id, &row.to_uppercase(), left, y, st, pr, live, false);
                            let kk = m.an(id ^ 0x5e1, if chosen { 1.0 } else { 0.0 }, 6.0);
                            if kk > 0.01 {
                                let lw = tw(Face::Light, &row.to_uppercase(), size, 0.02);
                                let ly = y + 1.12 * size - 0.06 * size;
                                c.light(left, ly, left + lw, 0.85 * kk, 0.0, ease(kk), 0.6, ui);
                            }
                        }
                        y += rh;
                    }
                    y += 1.0 * vh;
                }
                "tabs" => {
                    let tabs: Vec<&str> = it.get("tabs").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                    let size = m.cl(18.0, 50.0, 3.2);
                    let mut x = left;
                    for (ti, tb) in tabs.iter().enumerate() {
                        let id = hid(k::TAB, wid, ti as u32);
                        let st = WordStyle::new(Face::Thin, size);
                        let r = m.word(c, id, &tb.to_uppercase(), x, y, st, p, live, false);
                        x = r[2] + 2.6 * vh;
                    }
                    y += 1.12 * size + 1.0 * vh;
                }
                _ => {}
            }
            i += 1;
            order += 1.0;
        }
        c.pop_clip();
        self.content_h = y + self.scroll - col_top;
        let room = col_bottom - col_top;
        self.scroll_to = self.scroll_to.clamp(0.0, (self.content_h - room).max(0.0));

        // the foot: Back / Cancel / Done on the left with Esc, the rest on the right
        let fsize = m.cl(18.0, 50.0, 3.2);
        let fy = h - 5.0 * vh - 1.12 * fsize;
        let mut lx = left;
        let foot: Vec<&Value> = items.iter().filter(|it| b(it, "foot") && s(it, "t") == "button").collect();
        let mut rights: Vec<&Value> = Vec::new();
        let pf = m.vis(v, order.min(8.0));
        for it in &foot {
            if b(it, "back") && lx == left {
                let id = hid(k::FOOT, n(it, "id").max(0) as u32, 0);
                let st = WordStyle::new(Face::Thin, fsize).enabled(it.get("on").and_then(Value::as_bool).unwrap_or(true));
                let r = m.word(c, id, &s(it, "s").to_uppercase(), lx, fy, st, pf, live, false);
                let cap = m.cl(9.0, 18.0, 1.15);
                m.caps(c, "ESC", r[2] + 1.6 * vh, fy + 1.12 * fsize - cap * 1.6, cap, 0.22, 0.29, clamp01(ease(pf) * 1.15));
                lx = r[2];
            } else {
                rights.push(it);
            }
        }
        let mut rx = right;
        for it in rights.iter().rev() {
            let label = s(it, "s").to_uppercase();
            let wdt = tw(Face::Thin, &label, fsize, 0.02);
            rx -= wdt;
            let id = hid(k::FOOT, n(it, "id").max(0) as u32, 0);
            let st = WordStyle::new(Face::Thin, fsize).enabled(it.get("on").and_then(Value::as_bool).unwrap_or(true));
            let r = m.word(c, id, &label, rx, fy, st, pf, live, false);
            if m.over(r) && !s(it, "tip").is_empty() {
                hover_tip = Some(s(it, "tip").to_string());
            }
            rx -= 3.4 * vh;
        }

        // what the widget under the pointer is for
        if let Some(t) = hover_tip {
            if t != self.tip {
                self.tip = t;
            }
        }
        let want = if m.prev_hits.iter().any(|(r, _)| m.over(*r)) && !self.tip.is_empty() { 1.0 } else { 0.0 };
        self.tip_k = super::ui::toward(self.tip_k, want, 6.0, m.dt);
        if self.tip_k > 0.01 && !self.tip.is_empty() {
            let size = m.cl(9.0, 20.0, 1.25);
            let tw_ = (40.0 * w / 100.0).max(300.0 * ui);
            let lines = crate::gui::text::fonts().wrap_ls(Face::Caps, size, &self.tip, tw_, 0.04);
            let mut ty = h - 5.4 * vh - lines.len() as f32 * size * 1.6;
            for l in &lines {
                let lw = tw(Face::Caps, l, size, 0.04);
                m.rising_text(c, Face::Caps, l, (w - lw) / 2.0, ty, size, 1.6, 0.04, grey(0.54, self.tip_k), 0.0, ease(self.tip_k));
                ty += size * 1.6;
            }
        }
        m.out[super::out::TYPING] = if self.field.is_some() { 1.0 } else { 0.0 };
        let _ = smooth(0.0, 1.0, 0.0);
    }

    pub fn event(&mut self, m: &mut Menus, v: &View, e: Event) {
        let items = v.data.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let by_id = |id: u32| items.iter().find(|it| n(it, "id") == id as i64).cloned();
        match e {
            Event::Press(hit, button) if button & 15 == 0 => {
                let (kind, wid, sub) = (hid_kind(hit), hid_n(hit), hid_sub(hit));
                // a click elsewhere lets the field go
                if self.field.as_ref().is_some_and(|f| f.id != hit) {
                    self.field = None;
                }
                match kind {
                    k::BUTTON | k::FOOT => {
                        m.ask(format!("press {wid}"));
                        m.tick_sound(0.7);
                    }
                    k::CHOICE => {
                        if let Some(it) = by_id(wid) {
                            if s(&it, "t") == "choose" {
                                if let Some(id) = it.get("ids").and_then(Value::as_array).and_then(|a| a.get(sub as usize)).and_then(Value::as_i64) {
                                    m.ask(format!("press {id}"));
                                    m.tick_sound(0.6);
                                }
                            } else if s(&it, "t") == "check" {
                                let want = sub == 0;
                                if b(&it, "v") != want {
                                    m.ask(format!("press {wid}"));
                                }
                            } else {
                                m.ask(format!("cycle {wid} {sub}"));
                            }
                        }
                    }
                    k::PREV | k::NEXT => {
                        if let Some(it) = by_id(wid) {
                            let count = it.get("values").and_then(Value::as_array).map_or(0, |a| a.len()) as i64;
                            if count > 0 {
                                let at = n(&it, "index").max(0);
                                let to = if kind == k::NEXT { (at + 1) % count } else { (at + count - 1) % count };
                                m.ask(format!("cycle {wid} {to}"));
                            }
                        }
                    }
                    k::SLIDER => {
                        let r = m.prev_hits.iter().find(|(_, id)| *id == hit).map(|h| h.0).unwrap_or([0.0; 4]);
                        let r = [r[0] + 6.0, r[1], r[2] - 6.0, r[3]];
                        let val = clamp01((m.mouse[0] - r[0]) / (r[2] - r[0]).max(1.0));
                        self.drag = Some((wid, r, val));
                        m.ask(format!("slide {wid} {val}"));
                    }
                    k::FIELD => {
                        if self.field.as_ref().map_or(true, |f| f.id != hit) {
                            let it = by_id(wid).unwrap_or(Value::Null);
                            self.field = Some(Field::new(hit, s(&it, "value"), n(&it, "max").max(1) as usize, m.now));
                        }
                    }
                    k::LIST => {
                        let dbl = button & 16 != 0;
                        m.ask(format!("{} {wid} {sub}", if dbl { "open" } else { "select" }));
                    }
                    k::TAB => m.ask(format!("tab {wid} {sub}")),
                    _ => {}
                }
            }
            Event::Release(_, _) => {
                if let Some((id, _, _)) = self.drag.take() {
                    m.ask(format!("release {id}"));
                }
            }
            Event::Scroll(dy) => {
                self.scroll_to -= dy * 6.0 * m.vh();
            }
            Event::Char(ch) => {
                if let Some(fd) = self.field.as_mut() {
                    if fd.insert(ch, m.now) {
                        let id = hid_n(fd.id);
                        m.ask(format!("text {id} {}", fd.text));
                    }
                }
            }
            Event::Key(name, mods) => {
                if let Some(fd) = self.field.as_mut() {
                    match name.as_str() {
                        "Escape" | "Return" | "Keypad Enter" => {
                            self.field = None;
                            if name != "Escape" {
                                m.ask("key Return");
                            }
                        }
                        "V" if mods & 2 != 0 => m.ask(format!("paste {}", hid_n(fd.id))),
                        _ => {
                            if fd.key(&name, mods, m.now) {
                                let id = hid_n(fd.id);
                                m.ask(format!("text {id} {}", fd.text));
                            }
                        }
                    }
                } else if name == "Escape" {
                    m.ask("esc");
                } else if name == "Return" || name == "Keypad Enter" {
                    m.ask("key Return");
                }
            }
            _ => {}
        }
    }

    /// The game's text for the field typed in changed under it (a paste): taken in.
    pub fn sync_field(&mut self, v: &View) {
        if let Some(fd) = self.field.as_mut() {
            let id = hid_n(fd.id) as i64;
            if let Some(it) = v.data.get("items").and_then(Value::as_array).and_then(|a| a.iter().find(|it| n(it, "id") == id)) {
                let t = s(it, "value");
                if t != fd.text && it.get("pasted").is_some() {
                    fd.text = t.to_string();
                    fd.caret = fd.text.chars().count();
                }
            }
        }
    }
}
