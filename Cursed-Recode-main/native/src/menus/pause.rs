//! The pause menu (Esc in a world) as the main menu in the world: the world frosted behind it, the words on the left
//! rising from their lines with fire under the lit one, where one plays above them, the account top right, the game's
//! small buttons as small words, leaving the game last and apart.

use super::controls::{hid, hid_kind, hid_n, k};
use super::lang::tr;
use super::ui::{clamp01, ease_out, tw, WordStyle, BONE, DIM};
use super::{out, Event, Fly, Menus, View};
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::{fonts, Face};
use serde_json::Value;

pub struct Pause {
    /// The words drawn: their number, box, size, face, words, and whether they open a page (they fly to its title).
    boxes: Vec<(u64, [f32; 4], f32, Face, String, bool)>,
    /// Alt Manager's card under the nickname (alts.rs).
    alts: super::alts::AltCard,
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn clean(t: &str) -> String {
    t.replace("...", "").replace('…', "").trim().to_uppercase()
}

impl Pause {
    pub fn new() -> Pause {
        Pause { boxes: Vec::new(), alts: super::alts::AltCard::new() }
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        let vh = m.vh();
        let narrow = w < 760.0 * ui;
        let left = if narrow { 20.0 * ui } else { w * 0.062 };
        let live = v.leaving.is_none() && m.now - v.opened > 0.18;
        self.boxes.clear();
        // a shade on the left, under the words
        let shade = m.vis(v, 0.0);
        c.veil([0.0, 0.0, w * 0.55, h], grey(0.0, 0.3 * shade), 30.0 * vh, 1.0);
        let empty = Vec::new();
        let items = v.data.get("items").and_then(Value::as_array).unwrap_or(&empty);
        if !v.data.get("menu").and_then(Value::as_bool).unwrap_or(true) {
            // F3+Esc: paused, no menu
            let size = m.cl(34.0, 120.0, 7.6);
            m.title(c, v, &clean(s(v.data, "title")), left, 6.0 * vh, size);
            return;
        }
        let words: Vec<&Value> = items.iter().filter(|it| !matches!(s(it, "role"), "small" | "quit")).collect();
        let small: Vec<&Value> = items.iter().filter(|it| s(it, "role") == "small").collect();
        let quit = items.iter().find(|it| s(it, "role") == "quit");
        // the column: where one plays, the words, the small ones, leaving; 44% of it above the middle
        let cap = m.cl(9.0, 13.0, 1.15);
        let mono_line = (fonts().ascent(Face::Caps, cap) + fonts().descent(Face::Caps, cap)) * 1.15;
        let where_ = s(v.data, "where");
        let head_h = if where_.is_empty() { 0.0 } else { mono_line * 2.0 + 0.95 * vh + 1.4 * vh };
        let main_size = m.cl(36.0, 96.0, 8.4);
        let item_size = m.cl(26.0, 62.0, 5.4);
        let quit_size = m.cl(22.0, 44.0, 4.0);
        let small_size = cap;
        let mut col_h = head_h;
        for (i, _) in words.iter().enumerate() {
            col_h += 1.12 * if i == 0 { main_size } else { item_size };
        }
        if !small.is_empty() {
            col_h += 1.6 * vh + small_size * 1.6;
        }
        if quit.is_some() {
            col_h += 2.4 * vh + 1.12 * quit_size;
        }
        let mut y = h * 0.5 - 0.44 * col_h;
        let mut order = 0.0;
        if !where_.is_empty() {
            let p = m.vis(v, 0.0);
            let q = clamp01(ease_out(0.22, 1.0, p) * 1.15);
            let label = tr(if v.data.get("single").and_then(Value::as_bool).unwrap_or(false) { "pause.world" } else { "pause.server" }).to_uppercase();
            m.rising_text(c, Face::Caps, &label, left, y, cap, 1.15, 0.24, grey(0.373, 1.0), 0.0, q);
            let mut line2 = where_.to_string();
            if let Some(n) = v.data.get("players").and_then(Value::as_i64) {
                line2 = format!("{line2}  ·  {}", super::lang::trf("online", &[&n.to_string()]));
            }
            m.rising_text(c, Face::Caps, &line2, left, y + mono_line + 0.45 * vh, cap, 1.15, 0.08, grey(0.639, 1.0), 0.0, clamp01(q - 0.15));
            let lw = tw(Face::Caps, &label, cap, 0.24).max(tw(Face::Caps, &line2, cap, 0.08));
            m.reveal_line(c, left, y + mono_line * 2.0 + 0.95 * vh, left + lw, p);
            y += head_h;
            order += 1.0;
        }
        let focus = m.word_ids.contains(&m.lit) && self_has(items, m.lit);
        for (i, it) in words.iter().enumerate() {
            let main = i == 0;
            let size = if main { main_size } else { item_size };
            let wid = n(it) as u32;
            let id = hid(k::BUTTON, wid, 0);
            let st = WordStyle::new(if main { Face::Light } else { Face::Thin }, size).rest(if main { BONE } else { DIM }).enabled(it.get("on").and_then(Value::as_bool).unwrap_or(true));
            let p = m.vis(v, order);
            let label = clean(s(it, "s"));
            let r = m.word(c, id, &label, left, y, st, p, live, focus);
            let opens = matches!(s(it, "role"), "adv" | "stats" | "options" | "world");
            self.boxes.push((id, r, size, st.face, label, opens));
            y += 1.12 * size;
            order += 1.0;
        }
        if !small.is_empty() {
            y += 1.6 * vh;
            let mut x = left;
            let p = m.vis(v, order);
            for it in &small {
                let wid = n(it) as u32;
                let id = hid(k::BUTTON, wid, 0);
                let label = clean(s(it, "s"));
                let enabled = it.get("on").and_then(Value::as_bool).unwrap_or(true);
                let lw = tw(Face::Caps, &label, small_size, 0.22);
                let r = [x, y, x + lw, y + small_size * 1.6];
                if live && enabled {
                    m.hit(r, id);
                }
                let hot = m.an(id ^ 0x5a11, if live && enabled && m.over(r) { 1.0 } else { 0.0 }, 6.0);
                let q = clamp01(ease_out(0.22, 1.0, p) * 1.15);
                m.rising_text(c, Face::Caps, &label, x, y, small_size, 1.6, 0.22, grey(if enabled { 0.43 + 0.57 * hot } else { 0.25 }, 1.0), 0.0, q);
                if hot > 0.01 {
                    c.light(x, y + small_size * 1.5, x + lw, 0.8 * hot, 0.0, hot, 0.3, ui);
                }
                x += lw + 2.6 * vh;
            }
            y += small_size * 1.6;
            order += 1.0;
        }
        if let Some(it) = quit {
            y += 2.4 * vh;
            let wid = n(it) as u32;
            let id = hid(k::BUTTON, wid, 0);
            let st = WordStyle::new(Face::Thin, quit_size).enabled(it.get("on").and_then(Value::as_bool).unwrap_or(true));
            let p = m.vis(v, order);
            m.word(c, id, &clean(s(it, "s")), left, y, st, p, live, focus);
        }

        // the account, top right
        let who_size = m.cl(18.0, 34.0, 3.0);
        let head = m.cl(18.0, 30.0, 2.7);
        // (the account in use: Alt Manager changes it while the menu is up)
        let in_use = crate::alts::active();
        let nick = if in_use.is_empty() { s(v.data, "nick").to_uppercase() } else { in_use.to_uppercase() };
        let nick_w = tw(Face::Book, &nick, who_size, 0.05);
        let right = if narrow { w - 20.0 * ui } else { w - 4.4 * w / 100.0 };
        let who_top = 5.2 * vh;
        let who_line = who_size * 1.15;
        let who_left = right - nick_w - 1.1 * vh - head;
        let pw = m.vis(v, 1.0);
        let qw = clamp01(ease_out(0.22, 1.0, pw) * 1.15);
        m.rising_text(c, Face::Book, &nick, who_left, who_top, who_size, 1.15, 0.05, grey(0.902, 1.0), 0.0, qw);
        m.out[out::HEAD] = right - head;
        m.out[out::HEAD + 1] = who_top + (who_line - head) / 2.0 - 0.06 * who_size;
        m.out[out::HEAD + 2] = head;
        m.out[out::HEAD + 3] = pw;
        m.reveal_line(c, who_left, who_top + who_line + 0.25 * who_size, right, pw);
        let who = [who_left, who_top, right, who_top + who_line + 0.25 * who_size];
        self.alts.who(m, who, live);

        // the build line, bottom left
        let small = m.cl(8.0, 12.0, 1.05);
        let foot = h - 5.0 * vh - small * 1.3 - 0.4 * vh;
        let pb = m.vis(v, order + 1.0);
        m.rising_text(c, Face::Caps, "CURSED · ALPHA 0.1 · 26.3", left, foot, small, 1.3, 0.22, grey(0.427, 1.0), 0.0, clamp01(ease_out(0.0, 1.0, pb) * 1.15));
        // Alt Manager's card, over all of it (in a world a new account is the next join's)
        let in_world = m.in_world;
        self.alts.draw(m, c, who, live, in_world);
    }

    pub fn event(&mut self, m: &mut Menus, _v: &View, e: Event) {
        if self.alts.event(m, &e) {
            return;
        }
        match e {
            Event::Press(hit, b) if b & 15 == 0 && hid_kind(hit) == k::BUTTON => {
                // a word that opens a page flies up to be its title
                if let Some((_, r, size, face, label, true)) = self.boxes.iter().find(|bx| bx.0 == hit).cloned() {
                    let tw_ = tw(face, &label, size, 0.02);
                    m.fly = Some(Fly { text: label, face, size, at: [r[2] - tw_, r[1]], since: m.now, ls: 0.02 });
                }
                m.ask(format!("press {}", hid_n(hit)));
            }
            Event::Key(k_, _) if k_ == "Escape" => m.ask("esc"),
            _ => {}
        }
    }
}

fn n(v: &Value) -> i64 {
    v.get("id").and_then(Value::as_i64).unwrap_or(-1)
}

fn self_has(items: &[Value], lit: u64) -> bool {
    hid_kind(lit) == k::BUTTON && items.iter().any(|it| n(it) == hid_n(lit) as i64 && !matches!(s(it, "role"), "small"))
}
