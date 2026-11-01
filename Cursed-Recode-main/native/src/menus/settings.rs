//! The game's settings as one page (the approved preview's): the sections as words on the left under a search, the
//! section's rows on the right — a caption, and a choice of words, ‹ a value ›, a hair of light, a key — a round
//! arrow beside what is off its default. What a row is for opens under it when the pointer rests on it (or the keys
//! are on it). The keys go through it all: ↑↓ a row, ←→ its value, Enter, Ctrl+F the search. The search finds by
//! captions, what rows are for, headers and sections (in either keyboard layout), each section's finds under its name.
//! Changing a section only crosses its rows over (the rest stays). The valley behind shows what the video settings
//! change (Java's options feed its knobs).

use super::controls::{hid, hid_kind, hid_n, hid_sub, k, Field};
use super::lang::{ru, tr, trf};
use super::ui::{clamp01, ease, toward, tw, WordStyle, BONE, CAP, DIM, SOFT};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::{fonts, Face};
use serde_json::Value;

pub struct Settings {
    section: String,
    /// The section's rows as last drawn going away (their shapes), and since when: they cross over to the new.
    old: Option<(Vec<f32>, f32)>,
    switched: f32,
    scroll: f32,
    scroll_to: f32,
    content: f32,
    search: Option<Field>,
    query: String,
    drag: Option<(u32, [f32; 4], f32)>,
    tip: String,
    tip_k: f32,
    tip_was: String,
    /// The rows' place: where the list starts (its scroll moves it).
    rows_top: f32,
    /// The row the keys are on (its id), and the rows that can be on, in order: id, its own number, kind, top, height.
    focus: Option<u64>,
    focusable: Vec<(u64, u32, String, f32, f32)>,
    /// The row the pointer rests on and since when (what it is for opens under it after a moment).
    resting: Option<(u64, f32)>,
    pointer_was: [f32; 2],
    /// «Reset the section» pressed once: since when (a second press within a few seconds does it).
    reset_armed: f32,
    /// The scroll's line: how much it is seen (it shows while the list moves or the pointer is over it).
    bar_k: f32,
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

/// The sections' own names (the game's are long: "Video Settings…").
fn section_name(id: &str, game: &str) -> String {
    let key = format!("sec.{id}");
    let own = tr(&key);
    if own != key && (ru() || !game.is_empty()) {
        own.to_string()
    } else {
        game.to_string()
    }
}

impl Settings {
    pub fn new(data: &Value) -> Settings {
        Settings {
            section: s(data, "section").to_string(),
            old: None,
            switched: -10.0,
            scroll: 0.0,
            scroll_to: 0.0,
            content: 0.0,
            search: None,
            query: String::new(),
            drag: None,
            tip: String::new(),
            tip_k: 0.0,
            tip_was: String::new(),
            rows_top: 0.0,
            focus: None,
            focusable: Vec::new(),
            resting: None,
            pointer_was: [0.0; 2],
            reset_armed: -10.0,
            bar_k: 0.0,
        }
    }

    pub fn typing(&self) -> bool {
        self.search.is_some()
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
        // the game moved to another section (its own screen for it opened): its rows cross over
        let sec = s(v.data, "section").to_string();
        if sec != self.section {
            // the first section is the page's own: its rows rise with the page, not as a switch over
            self.switched = if self.section.is_empty() { -10.0 } else { m.now };
            self.section = sec;
            self.scroll_to = 0.0;
            self.scroll = 0.0;
            m.anims.insert(hid(k::PAGE, 77, 1), 0.0);
        }
        // the slider being dragged follows the pointer
        if let Some((id, r, last)) = self.drag {
            let val = clamp01((m.mouse[0] - r[0]) / (r[2] - r[0]).max(1.0));
            if (val - last).abs() > 0.0005 {
                m.ask(format!("slide {id} {val}"));
                self.drag = Some((id, r, val));
            }
        }
        m.scene_pose([8.0, 20.5, -0.07, 0.03, m.scene_to[4]], 0.16, 0.22);
        // a shade under the words where the glow behind the ridges is bright: the sections, the rows
        let shade = m.vis(v, 0.0);
        c.veil([0.0, 12.0 * vh, 27.0 * vw, h], crate::gui::draw::grey(0.0, 0.32 * shade), 14.0 * vh, 1.0);
        c.veil([if narrow { 0.0 } else { 26.0 * vw }, 12.0 * vh, w, h - 9.0 * vh], crate::gui::draw::grey(0.0, 0.5 * shade), 12.0 * vh, 0.0);

        // the title flies in from the main menu's word (or rises)
        let tsize = m.cl(34.0, 120.0, 7.6);
        m.title(c, v, &tr("settings").to_uppercase(), left, 6.0 * vh, tsize);

        // the search, over the sections
        let cap = m.cl(9.0, 20.0, 1.25);
        let sp = m.vis(v, 1.0);
        let sid = hid(k::SEARCH, 0, 0);
        let sw = if narrow { right - left } else { 18.0 * vw };
        let st_top = 17.4 * vh - cap * 0.5;
        let r = [left, st_top, left + sw, st_top + cap * 1.9];
        if live {
            m.hit(r, sid);
        }
        let shown = self.search.as_ref().map_or(self.query.clone(), |f| f.text.clone());
        let q = clamp01(ease(sp) * 1.15);
        c.icon([left, st_top + cap * 0.45, left + cap * 1.1, st_top + cap * 1.55], 13, grey(0.35, q));
        let tx = left + cap * 1.7;
        if shown.is_empty() {
            m.rising_text(c, Face::Caps, &tr("search").to_uppercase(), tx, st_top, cap, 1.3, 0.2, grey(0.3, 1.0), 0.0, q);
        } else {
            m.rising_text(c, Face::Caps, &shown.to_uppercase(), tx, st_top, cap, 1.3, 0.2, grey(BONE, 1.0), 0.0, q);
        }
        if let Some(fd) = &self.search {
            if ((m.now - fd.moved) * 1.1).fract() < 0.6 {
                let cx = tx + tw(Face::Caps, &fd.text.chars().take(fd.caret).collect::<String>().to_uppercase(), cap, 0.2);
                c.line([cx, st_top + 0.15 * cap], [cx, st_top + 1.2 * cap], 1.0, grey(1.0, 0.9), 0.0);
            }
        }
        let lit = m.an(sid ^ 9, if self.search.is_some() { 1.0 } else { 0.0 }, 5.0);
        c.light(left, st_top + cap * 1.75, left + sw, 0.12 + 0.6 * lit, 0.0, ease(sp).max(0.0), 0.5 * lit, ui);

        // the sections
        let empty = Vec::new();
        let sections = v.data.get("sections").and_then(Value::as_array).unwrap_or(&empty);
        let ssize = m.cl(20.0, 62.0, 3.9);
        let mut y = 22.0 * vh;
        let searching = !s(v.data, "query").is_empty();
        let focus = sections.iter().enumerate().any(|(i, _)| m.lit == hid(k::SECTION, i as u32, 0));
        for (i, se) in sections.iter().enumerate() {
            let id = hid(k::SECTION, i as u32, 0);
            let chosen = !searching && s(se, "id") == self.section;
            let name = section_name(s(se, "id"), s(se, "name")).to_uppercase();
            let st = WordStyle::new(Face::Thin, ssize).rest(if chosen { BONE } else { DIM });
            let p = m.vis(v, 2.0 + i as f32);
            // the whole line of the column takes the click (short words are small to hit)
            if live {
                m.hit([left, y, left + if narrow { 40.0 * vw } else { 21.0 * vw }, y + 1.12 * ssize + 1.1 * vh], id);
            }
            let wr = m.word(c, id, &name, left, y, st, p, live, focus);
            let kk = m.an(id ^ 0xc405e, if chosen { 1.0 } else { 0.0 }, 6.0);
            if kk > 0.01 {
                let ly = y + 1.12 * ssize - 0.06 * ssize;
                c.light(left, ly, wr[2] - (wr[2] - wr[0]) * 0.0, 0.85 * kk * p, 0.0, ease(kk), 0.6, ui);
            }
            y += 1.12 * ssize + 1.1 * vh;
        }

        // the panel: the rows (crossing over from the section's before); the skin's rows leave room for the model
        let px = if narrow { left } else { 30.0 * vw };
        let skin = self.section == "skin" && !narrow && s(v.data, "query").is_empty();
        let full_right = right;
        let right = if skin { px + (right - px) * 0.56 } else { right };
        let pw = right - px;
        let ptop = 17.0 * vh;
        let pbottom = h - 13.0 * vh;
        let rows = v.data.get("rows").and_then(Value::as_array).unwrap_or(&empty);
        let from = c.p.len();
        c.push_clip([px - 2.0 * vh, ptop - 1.0, right + 2.0 * vh, pbottom]);
        self.scroll = m.scroll_ease(hid(k::PAGE, 77, 1), self.scroll_to);
        self.rows_top = ptop + 0.8 * vh - self.scroll;
        let mut y = self.rows_top;
        let row_h = (5.0 * vh).max(32.0 * ui);
        let sw_k = m.now - self.switched;
        let mut hover_tip: Option<String> = None;
        let mut order = 0.0f32;
        let ctl_right = right;
        let slider_w = (20.0 * vw).min(pw * 0.36);
        let tip_size = m.cl(9.0, 17.0, 1.12);
        // the pointer moved: the row under it is where the keys go on from
        let moved = (m.mouse[0] - self.pointer_was[0]).abs() + (m.mouse[1] - self.pointer_was[1]).abs() > 0.5;
        self.pointer_was = m.mouse;
        let mut focusable: Vec<(u64, u32, String, f32, f32)> = Vec::new();
        let mut rested = false;
        for row in rows.iter() {
            let t = s(row, "t");
            // the section's rows rise one after another (on a switch, quicker)
            let p = if sw_k < 1.2 { smooth01((sw_k - 0.12 - order.min(12.0) * 0.025) / 0.45).min(m.vis(v, 0.0)) } else { m.vis(v, 3.0 + order.min(14.0)) };
            let visible = y > ptop - 3.0 * row_h && y < pbottom + row_h;
            match t {
                "header" => {
                    y += if order > 0.0 { 2.4 * vh } else { 0.8 * vh };
                    if visible {
                        m.caps(c, &s(row, "s").to_uppercase(), px, y, cap, 0.22, CAP, clamp01(ease(p) * 1.15));
                    }
                    y += cap * 1.3 + 0.6 * vh;
                }
                "group" => {
                    // what the search found: each section's name over its rows (pressed, it opens the section)
                    let gsize = m.cl(14.0, 34.0, 2.2);
                    y += if order > 0.0 { 2.6 * vh } else { 0.4 * vh };
                    let gi = sections.iter().position(|se| s(se, "id") == s(row, "sec")).unwrap_or(0);
                    let gid = hid(k::BUTTON, 8000 + gi as u32, 0);
                    if visible {
                        let name = section_name(s(row, "sec"), s(row, "s")).to_uppercase();
                        let st = WordStyle::new(Face::Light, gsize).rest(BONE);
                        m.word(c, gid, &name, px, y, st, p, live, false);
                    }
                    y += 1.12 * gsize + 0.8 * vh;
                }
                "langs" => {
                    y = self.languages(m, c, row, px, y, pw, p, live, ptop, pbottom);
                }
                "link" => {
                    let id = hid(k::BUTTON, 9000 + order as u32, 0);
                    let size = m.cl(18.0, 53.0, 3.4);
                    let st = WordStyle::new(Face::Thin, size).enabled(row.get("on").and_then(Value::as_bool).unwrap_or(true));
                    if visible {
                        m.word(c, id, &s(row, "s").to_uppercase(), px, y, st, p, live, false);
                    }
                    m.link_ids.push((id, s(row, "id").to_string()));
                    y += 1.12 * size + 1.0 * vh;
                }
                "text" => {
                    if visible {
                        y = m.paragraph(c, s(row, "s"), px, y, pw * 0.8, m.cl(10.0, 17.0, 1.45), SOFT, clamp01(ease(p) * 1.15), 0.04) + 0.8 * vh;
                    } else {
                        y += 3.0 * vh;
                    }
                }
                "button" => {
                    let id = hid(k::BUTTON, n(row, "id").max(0) as u32, 0);
                    let size = m.cl(18.0, 46.0, 2.9);
                    let st = WordStyle::new(Face::Light, size).enabled(row.get("on").and_then(Value::as_bool).unwrap_or(true));
                    if visible {
                        let fk = m.an(id ^ 0xf0c5, if self.focus == Some(id) { 1.0 } else { 0.0 }, 9.0);
                        if fk > 0.01 {
                            c.rect([px - 1.2 * vh, y, right + 1.2 * vh, y + row_h], 0.6 * vh, grey(1.0, 0.04 * fk));
                        }
                        let _ = m.word(c, id, &s(row, "s").to_uppercase(), px, y + (row_h - 1.12 * size) / 2.0, st, p, live, false);
                    }
                    focusable.push((id, n(row, "id").max(0) as u32, "button".into(), y, row_h));
                    y += row_h;
                }
                _ => {
                    let wid = n(row, "id").max(0) as u32;
                    let on = row.get("on").and_then(Value::as_bool).unwrap_or(true);
                    let rid = hid(if t == "keybind" { k::KEY } else { k::ROW }, wid, 0);
                    // what it is for, opening under it while the pointer rests on it or the keys are on it
                    let tip = s(row, "tip");
                    let tip_text = if !tip.is_empty() { tip.to_string() } else if !s(row, "clash").is_empty() { trf("clash", &[s(row, "clash")]) } else { String::new() };
                    let tip_lines = if tip_text.is_empty() { Vec::new() } else { fonts().wrap_ls(Face::Caps, tip_size, &tip_text, pw * 0.66, 0.04) };
                    let tip_h = if tip_lines.is_empty() { 0.0 } else { tip_lines.len().min(4) as f32 * tip_size * 1.55 + 1.0 * vh };
                    let open_was = m.anims.get(&(rid ^ 0x7105)).copied().unwrap_or(0.0);
                    let rr = [px, y, right, y + row_h + open_was * tip_h];
                    let over = live && m.over(rr);
                    if over {
                        rested = true;
                        if self.resting.map(|r| r.0) != Some(rid) {
                            self.resting = Some((rid, m.now));
                        }
                        if moved {
                            self.focus = Some(rid);
                        }
                    }
                    let open = !tip_lines.is_empty() && (self.focus == Some(rid) && !over || self.resting.is_some_and(|r| r.0 == rid && m.now - r.1 > 0.35 && over));
                    let ok = m.an(rid ^ 0x7105, if open { 1.0 } else { 0.0 }, 7.0);
                    focusable.push((rid, wid, t.to_string(), y, row_h));
                    if visible {
                        if live {
                            m.hit(rr, rid);
                        }
                        // the row the keys or the pointer are on: a soft light behind it (no outline)
                        let fk = m.an(rid ^ 0xf0c5, if self.focus == Some(rid) { 1.0 } else { 0.0 }, 9.0);
                        if fk > 0.01 {
                            c.rect([px - 1.2 * vh, y, right + 1.2 * vh, y + row_h + ok * tip_h], 0.6 * vh, grey(1.0, 0.035 * fk));
                        }
                        if ok > 0.01 {
                            let mut ty = y + row_h - 0.5 * vh;
                            for l in tip_lines.iter().take(4) {
                                m.rising_text(c, Face::Caps, l, px, ty, tip_size, 1.55, 0.04, grey(0.56, ok), 0.0, ease(ok));
                                ty += tip_size * 1.55;
                            }
                        }
                        let label = s(row, "label");
                        let end = m.row_caption(c, rid, label, px, y, row_h, pw, p, on);
                        // the way back to the default, when it is off it
                        if b(row, "changed") {
                            let rs = m.cl(10.0, 21.0, 1.4);
                            let rid2 = hid(k::RESET, wid, if t == "keybind" { 1 } else { 0 });
                            let rx = end + 1.2 * vh;
                            let rrect = [rx - 4.0, y, rx + rs + 4.0, y + row_h];
                            if live {
                                m.hit(rrect, rid2);
                            }
                            let hot = m.an(rid2 ^ 3, if live && m.over(rrect) { 1.0 } else { 0.0 }, 8.0);
                            let k_in = m.an(rid2 ^ 5, 1.0, 4.0);
                            c.icon([rx, y + (row_h - rs) / 2.0, rx + rs, y + (row_h + rs) / 2.0], 9, grey(0.33 + 0.67 * hot, ease(k_in) * clamp01(ease(p) * 1.15)));
                        }
                        self.control(m, c, row, t, wid, ctl_right, y, row_h, slider_w, p, live, on);
                    }
                    y += row_h + ok * tip_h;
                }
            }
            order += 1.0;
        }
        if !rested {
            self.resting = None;
        }
        self.focusable = focusable;
        if self.focus.is_some_and(|f| !self.focusable.iter().any(|r| r.0 == f)) {
            self.focus = None;
        }
        // nothing found
        if searching && rows.is_empty() {
            let q = clamp01(ease(m.vis(v, 3.0)) * 1.15);
            m.rising_text(c, Face::Caps, &tr("nothingfound").to_uppercase(), px, ptop + 1.0 * vh, cap, 1.3, 0.22, grey(0.42, 1.0), 0.0, q);
        }
        c.pop_clip();
        // the list's place in it: a thin line at its right while it moves or the pointer is over it
        {
            let room = pbottom - ptop - 2.0 * vh;
            // (the rows' own height: rows_top is already where the scroll put them)
            let content = y - self.rows_top;
            if content > room + 1.0 {
                let moving = (self.scroll - self.scroll_to).abs() > 0.5;
                let near = m.mouse[0] > px && m.mouse[0] < full_right + 4.0 * vh && m.mouse[1] > ptop && m.mouse[1] < pbottom;
                self.bar_k = toward(self.bar_k, if moving { 1.0 } else if near { 0.55 } else { 0.18 }, 5.0, m.dt);
                let bx = right + 2.4 * vh;
                let span = pbottom - ptop;
                let th = (span * room / content).max(3.0 * vh);
                let ty = ptop + (span - th) * (self.scroll / (content - room)).clamp(0.0, 1.0);
                let a = self.bar_k * m.vis(v, 2.0);
                c.rect([bx - 0.5, ptop, bx + 0.5, pbottom], 0.5, grey(1.0, 0.05 * a));
                c.rect([bx - 1.0, ty, bx + 1.0, ty + th], 1.0, grey(1.0, 0.45 * a));
            }
        }
        // the skin: the player in it, turning slowly (the game draws it: extras kind 7, its box's left, top, height)
        if skin {
            let k = clamp01((m.now - self.switched - 0.15) / 0.5).min(m.vis(v, 2.0));
            let mh = (pbottom - ptop) * 0.86;
            let mw = mh * 0.5;
            let mx = right + (full_right - right) * 0.5 - mw * 0.5;
            let my = ptop + (pbottom - ptop - mh) * 0.5;
            if k > 0.01 {
                c.glow([mx + mw * 0.5, my + mh * 0.97], mw * 0.75, grey(1.0, 0.07 * k));
                c.light(mx - mw * 0.1, my + mh * 0.985, mx + mw * 1.1, 0.35 * k, 0.0, 1.0, 0.8, ui);
            }
            m.extra(7, 0.0, [mx, my, mh], ease(k));
        }
        self.content = y - self.rows_top;
        let room = pbottom - ptop - 2.0 * vh;
        self.scroll_to = self.scroll_to.clamp(0.0, (self.content - room).max(0.0));
        // the old section's rows going (up and out) while the new rise
        let fresh = c.p.split_off(from);
        if let Some((old, at)) = &self.old {
            let k_old = (m.now - at) / 0.22;
            if k_old < 1.0 {
                let start = c.p.len();
                c.p.extend_from_slice(old);
                c.fade_from(start, 1.0 - ease(k_old));
                c.shift(start, [0.0, -ease(k_old) * 2.0 * vh]);
            }
        }
        c.p.extend_from_slice(&fresh);
        if sw_k > 0.5 || self.old.is_none() {
            self.old = Some((fresh, -10.0));
        } else if let Some(o) = self.old.as_mut() {
            if o.1 < 0.0 {
                o.1 = self.switched;
            }
        }

        // a key being bound: said under everything
        if b(v.data, "listening") {
            hover_tip = Some(tr("press").to_string());
        }
        // the back way, bottom left (Esc)
        let fsize = m.cl(18.0, 50.0, 3.2);
        let fy = h - 5.0 * vh - 1.12 * fsize;
        let pf = m.vis(v, 6.0);
        let st = WordStyle::new(Face::Thin, fsize);
        let r = m.word(c, hid(k::BACK, 0, 0), &tr("back").to_uppercase(), left, fy, st, pf, live, false);
        m.caps(c, "ESC", r[2] + 1.6 * vh, fy + 1.12 * fsize - cap * 1.6, cap, 0.22, 0.29, clamp01(ease(pf) * 1.15));
        if self.section == "keys" && !searching {
            let label = tr("resetkeys").to_uppercase();
            let wd = tw(Face::Thin, &label, fsize, 0.02);
            m.word(c, hid(k::BUTTON, 9999, 0), &label, right - wd, fy, st, pf, live, false);
        } else if !searching && matches!(self.section.as_str(), "video" | "audio" | "controls" | "skin" | "chat" | "access" | "online") {
            // the section's every option back to its default: pressed twice (the first asks if it is meant)
            let armed = m.now - self.reset_armed < 3.0;
            let label = if armed { tr("sure") } else { tr("resetsection") }.to_uppercase();
            let wd = tw(Face::Thin, &label, fsize, 0.02);
            m.word(c, hid(k::BUTTON, 9998, 0), &label, full_right - wd, fy, st, pf, live, false);
        }

        // at the bottom: only that a key is being bound (what a row is for opens under it)
        if let Some(t) = hover_tip.filter(|t| !t.is_empty()) {
            self.tip = t;
        }
        let over_any = b(v.data, "listening");
        self.tip_k = toward(self.tip_k, if over_any && !self.tip.is_empty() { 1.0 } else { 0.0 }, 6.0, m.dt);
        if self.tip != self.tip_was {
            self.tip_was = self.tip.clone();
            m.anims.insert(hid(k::PAGE, 78, 0), 0.0);
        }
        let tin = m.an(hid(k::PAGE, 78, 0), 1.0, 7.0);
        if self.tip_k > 0.01 {
            let size = m.cl(9.0, 20.0, 1.25);
            let tx0 = if narrow { left } else { 30.0 * vw };
            let tx1 = if narrow { right } else { w - 30.0 * vw };
            let lines = fonts().wrap_ls(Face::Caps, size, &self.tip, tx1 - tx0, 0.04);
            let mut ty = h - 5.4 * vh - lines.len() as f32 * size * 1.6;
            for l in lines.iter().take(4) {
                m.rising_text(c, Face::Caps, l, tx0, ty, size, 1.6, 0.04, grey(0.54, self.tip_k), 0.0, ease(tin));
                ty += size * 1.6;
            }
        }
        m.out[super::out::TYPING] = if self.search.is_some() { 1.0 } else { 0.0 };
    }

    /// A row's control at the right, by what the row is.
    #[allow(clippy::too_many_arguments)]
    fn control(&mut self, m: &mut Menus, c: &mut Canvas, row: &Value, t: &str, wid: u32, right: f32, y: f32, row_h: f32, slider_w: f32, p: f32, live: bool, on: bool) {
        let vh = m.vh();
        match t {
            "check" => {
                let words = vec![tr("on").to_string(), tr("off").to_string()];
                m.choice(c, k::CHOICE, wid, &words, Some(if b(row, "v") { 0 } else { 1 }), right, y, row_h, p, live, on);
            }
            "slider" => {
                let held = self.drag.is_some_and(|d| d.0 == wid);
                let val = if held { self.drag.unwrap().2 } else { f(row, "v") };
                let r = m.slider(c, wid, val, s(row, "value"), right, y, row_h, slider_w, p, live, on);
                if held {
                    self.drag = Some((wid, r, val));
                }
            }
            "keybind" => {
                let size = m.cl(10.0, 22.0, 1.45);
                let listen = b(row, "listen");
                let text = if listen { tr("press").to_uppercase() } else { s(row, "value").to_uppercase() };
                let blink = !listen || ((m.now * 2.0).fract() < 0.5);
                let clash = !s(row, "clash").is_empty();
                let wd = tw(Face::Caps, &text, size, 0.12);
                let ty = y + (row_h - size * 1.3) / 2.0;
                let bright = if listen { if blink { 1.0 } else { 0.33 } } else if clash { 1.0 } else { BONE };
                m.rising_text(c, Face::Caps, &text, right - wd, ty, size, 1.3, 0.12, grey(bright, 1.0), 0.0, clamp01(ease(p) * 1.15));
                if clash {
                    c.light(right - wd, y + row_h * 0.5 + size * 0.9, right, 0.7, 0.0, 1.0, 0.4, m.uipx());
                }
            }
            _ => {
                let values: Vec<String> = row.get("values").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                let index = n(row, "index");
                let size = m.cl(15.0, 39.0, 2.5);
                let total: f32 = values.iter().map(|x| tw(Face::Light, &x.to_uppercase(), size, 0.02)).sum::<f32>() + 2.0 * vh * values.len() as f32;
                if !values.is_empty() && values.len() <= 4 && total < (right - m.w * 0.3) * 0.55 {
                    m.choice(c, k::CHOICE, wid, &values, (index >= 0).then_some(index as usize), right, y, row_h, p, live, on);
                } else {
                    m.cycle(c, wid, s(row, "value"), index as i32, right, y, row_h, p, live, on, 10.0 * m.w / 100.0);
                }
            }
        }
    }

    /// The languages: two columns of names, each with its region; the game's chosen lit.
    #[allow(clippy::too_many_arguments)]
    fn languages(&mut self, m: &mut Menus, c: &mut Canvas, row: &Value, x: f32, top: f32, width: f32, p: f32, live: bool, clip_top: f32, clip_bottom: f32) -> f32 {
        let vh = m.vh();
        let size = m.cl(16.0, 45.0, 2.9);
        let cap = m.cl(9.0, 18.0, 1.15);
        let sel = s(row, "sel");
        let empty = Vec::new();
        let list = row.get("list").and_then(Value::as_array).unwrap_or(&empty);
        let col_w = (width - 4.0 * vh) / 2.0;
        let rh = 1.12 * size + 1.6 * vh;
        let half = list.len().div_ceil(2);
        for (i, l) in list.iter().enumerate() {
            let (col, at) = (i / half.max(1), i % half.max(1));
            let lx = x + col as f32 * (col_w + 4.0 * vh);
            let ly = top + at as f32 * rh;
            if ly < clip_top - rh || ly > clip_bottom + rh {
                continue;
            }
            let id = hid(k::LIST, 7, i as u32);
            let chosen = s(l, "code") == sel;
            let st = WordStyle { glow: 0.0, ..WordStyle::new(Face::Light, size).rest(if chosen { 1.0 } else { 0.541 }) };
            let pr = p;
            m.word(c, id, &s(l, "name").to_uppercase(), lx, ly, st, pr, live, false);
            m.link_ids.push((id, s(l, "code").to_string()));
            let region = s(l, "region").to_uppercase();
            let rw = tw(Face::Caps, &region, cap, 0.22);
            let hot = m.an(id ^ 0x1e9, if chosen || m.lit == id { 1.0 } else { 0.0 }, 6.0);
            m.caps(c, &region, lx + col_w - rw, ly + 1.12 * size - cap * 1.5, cap, 0.22, 0.3 + 0.3 * hot, clamp01(ease(pr) * 1.15));
            let kk = m.an(id ^ 0x5e1, if chosen { 1.0 } else { 0.0 }, 6.0);
            if kk > 0.01 {
                let lw = tw(Face::Light, &s(l, "name").to_uppercase(), size, 0.02);
                let lyy = ly + 1.12 * size - 0.06 * size;
                c.light(lx, lyy, lx + lw, 0.85 * kk, 0.0, ease(kk), 0.6, m.uipx());
            }
        }
        top + half as f32 * rh + 2.0 * vh
    }

    pub fn event(&mut self, m: &mut Menus, v: &View, e: Event) {
        let rows = v.data.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
        let by_id = |id: u32, t: &str| rows.iter().find(|r| n(r, "id") == id as i64 && (t.is_empty() || s(r, "t") == t)).cloned();
        match e {
            Event::Press(hit, button) if button & 15 == 0 => {
                let (kind, wid, sub) = (hid_kind(hit), hid_n(hit), hid_sub(hit));
                if kind != k::SEARCH && self.search.is_some() {
                    self.search = None;
                }
                match kind {
                    k::SECTION => {
                        let secs = v.data.get("sections").and_then(Value::as_array).cloned().unwrap_or_default();
                        if let Some(se) = secs.get(wid as usize) {
                            let id = s(se, "id").to_string();
                            if id != self.section || !self.query.is_empty() {
                                self.query.clear();
                                m.ask(format!("section {id}"));
                            }
                        }
                    }
                    k::SEARCH => {
                        if self.search.is_none() {
                            self.search = Some(Field::new(hit, &self.query, 40, m.now));
                        }
                    }
                    k::BACK => m.ask("close"),
                    k::BUTTON => {
                        if wid == 9999 {
                            m.ask("keysreset");
                        } else if wid == 9998 {
                            if m.now - self.reset_armed < 3.0 {
                                self.reset_armed = -10.0;
                                m.ask("sectionreset");
                            } else {
                                self.reset_armed = m.now;
                            }
                        } else if (8000..8100).contains(&wid) {
                            let secs = v.data.get("sections").and_then(Value::as_array).cloned().unwrap_or_default();
                            if let Some(se) = secs.get((wid - 8000) as usize) {
                                self.query.clear();
                                self.search = None;
                                m.ask(format!("section {}", s(se, "id")));
                            }
                        } else if let Some((_, link)) = m.link_ids.iter().find(|(id, _)| *id == hit).cloned() {
                            m.ask(format!("link {link}"));
                        } else {
                            m.ask(format!("press {wid}"));
                        }
                        m.tick_sound(0.7);
                    }
                    k::LIST => {
                        if let Some((_, code)) = m.link_ids.iter().find(|(id, _)| *id == hit).cloned() {
                            m.ask(format!("lang {code}"));
                        }
                    }
                    k::CHOICE => {
                        if let Some(r) = by_id(wid, "") {
                            if s(&r, "t") == "check" {
                                if b(&r, "v") != (sub == 0) {
                                    m.ask(format!("press {wid}"));
                                }
                            } else {
                                m.ask(format!("cycle {wid} {sub}"));
                            }
                        }
                    }
                    k::PREV | k::NEXT => {
                        if let Some(r) = by_id(wid, "cycle") {
                            let count = r.get("values").and_then(Value::as_array).map_or(0, |a| a.len()) as i64;
                            if count > 0 {
                                let at = n(&r, "index").max(0);
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
                    k::KEY => {
                        m.ask(format!("listen {wid}"));
                        m.anims.insert(hid(k::KEY, wid, 7), m.now);
                    }
                    k::RESET => {
                        if sub == 1 {
                            m.ask(format!("keyreset {wid}"));
                        } else {
                            m.ask(format!("reset {wid}"));
                        }
                        m.tick_sound(0.6);
                    }
                    k::ROW => {
                        // the caption of a row: a toggle flips, a cycle steps on
                        if let Some(r) = by_id(wid, "") {
                            match s(&r, "t") {
                                "check" => m.ask(format!("press {wid}")),
                                "cycle" => {
                                    let count = r.get("values").and_then(Value::as_array).map_or(0, |a| a.len()) as i64;
                                    if count > 0 {
                                        m.ask(format!("cycle {wid} {}", (n(&r, "index").max(0) + 1) % count));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Release(_, _) => {
                if let Some((id, _, _)) = self.drag.take() {
                    m.ask(format!("release {id}"));
                }
            }
            Event::Scroll(dy) => self.scroll_to -= dy * 6.0 * m.vh(),
            Event::Char(ch) => {
                if let Some(fd) = self.search.as_mut() {
                    if fd.insert(ch, m.now) {
                        self.query = fd.text.clone();
                        m.ask(format!("search {}", self.query));
                    }
                }
            }
            Event::Key(ref name, mods) if self.search.is_none() && !b(v.data, "listening") && self.nav(m, &rows, name, mods) => {}
            Event::Key(name, mods) => {
                if let Some(fd) = self.search.as_mut() {
                    match name.as_str() {
                        "Escape" | "Return" | "Keypad Enter" => self.search = None,
                        "Down" => {
                            // from the search down into what it found
                            self.search = None;
                            self.focus = self.focusable.first().map(|r| r.0);
                        }
                        _ => {
                            if fd.key(&name, mods, m.now) {
                                self.query = fd.text.clone();
                                m.ask(format!("search {}", self.query));
                            }
                        }
                    }
                } else if name == "Escape" {
                    if !self.query.is_empty() {
                        self.query.clear();
                        m.ask("search ");
                    } else {
                        m.ask("close");
                    }
                } else if name == "F" && mods & 2 != 0 {
                    self.search = Some(Field::new(hid(k::SEARCH, 0, 0), &self.query, 40, m.now));
                }
            }
            _ => {}
        }
    }
}

impl Settings {
    /// The keys through the rows: ↑↓ a row (the list follows), ←→ its value, Enter / Space presses it. True when taken.
    fn nav(&mut self, m: &mut Menus, rows: &[Value], name: &str, mods: u32) -> bool {
        let at = self.focus.and_then(|f| self.focusable.iter().position(|r| r.0 == f));
        match name {
            "Tab" => {
                // the next section (Shift: the one before), as the column says
                let order = ["video", "audio", "controls", "keys", "skin", "chat", "packs", "access", "online"];
                let i = order.iter().position(|s| *s == self.section).unwrap_or(0);
                let back = mods & 1 != 0;
                let to = if back { (i + order.len() - 1) % order.len() } else { (i + 1) % order.len() };
                self.focus = None;
                m.ask(format!("section {}", order[to]));
                m.tick_sound(0.4);
                true
            }
            "Down" | "Up" => {
                if self.focusable.is_empty() {
                    return false;
                }
                let next = match (at, name) {
                    (None, _) => 0,
                    (Some(i), "Down") => (i + 1).min(self.focusable.len() - 1),
                    (Some(i), _) => i.saturating_sub(1),
                };
                let (id, _, _, top, h) = self.focusable[next].clone();
                self.focus = Some(id);
                // the list follows: the row seen whole, with a little room round it
                let vh = m.vh();
                let (ptop, pbottom) = (17.0 * vh, m.h - 13.0 * vh);
                let pad = 3.0 * vh;
                if top < ptop + pad {
                    self.scroll_to -= ptop + pad - top;
                } else if top + h > pbottom - pad {
                    self.scroll_to += top + h - (pbottom - pad);
                }
                self.scroll_to = self.scroll_to.max(0.0);
                m.tick_sound(0.35);
                true
            }
            "Left" | "Right" | "Return" | "Keypad Enter" | "Space" => {
                let Some(i) = at else { return false };
                let (_, wid, t, _, _) = self.focusable[i].clone();
                let row = rows.iter().find(|r| n(r, "id") == wid as i64 && s(r, "t") == t).cloned();
                let enter = !matches!(name, "Left" | "Right");
                let dir: i64 = if name == "Left" { -1 } else { 1 };
                match t.as_str() {
                    "check" => m.ask(format!("press {wid}")),
                    "slider" => {
                        if enter {
                            return false;
                        }
                        let v = row.as_ref().map_or(0.0, |r| f(r, "v"));
                        let to = (v + 0.05 * dir as f32).clamp(0.0, 1.0);
                        m.ask(format!("slide {wid} {to}"));
                        m.ask(format!("release {wid}"));
                    }
                    "keybind" => {
                        if !enter {
                            return false;
                        }
                        m.ask(format!("listen {wid}"));
                    }
                    "button" => {
                        if !enter {
                            return false;
                        }
                        m.ask(format!("press {wid}"));
                    }
                    _ => {
                        let Some(r) = row else { return false };
                        let count = r.get("values").and_then(Value::as_array).map_or(0, |a| a.len()) as i64;
                        if count == 0 {
                            return false;
                        }
                        let at = n(&r, "index").max(0);
                        m.ask(format!("cycle {wid} {}", (at + dir + count) % count));
                    }
                }
                m.tick_sound(0.5);
                true
            }
            _ => false,
        }
    }
}

fn smooth01(x: f32) -> f32 {
    let k = x.clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}
