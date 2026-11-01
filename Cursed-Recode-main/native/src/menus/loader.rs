//! The loader's page (loader/app.rs: Cursed Recode.exe), in the main menu's language — her word, 15.10: the loader
//! looks like the game's main menu, the valley behind it, big words. One page for all its screens: on the left a
//! column rises out of its lines — a caption in small capitals, a title (the sign-in's code, the folder, how much has
//! come), a few lines of text, the words to press (the main one large); the account top right, what plays on the
//! computer bottom right (not music of its own), the build and what's new bottom left (the valley's title over them).
//!
//! The data (the loader's): `cap`, `title`, `title_face` ("thin" | "book"), `text`, `words` ([action, label, kind]:
//! kind 1 the main one, 2 the way out, small), `nick`, `media` {title, artist, pos, len, playing}, `build`, `news`
//! [lines]. What is pressed is asked back as its action (`account`, `media prev|toggle|next` for the corners).

use super::lang::ru;
use super::ui::{clamp01, ease_out, id, smooth, toward, tw, WordStyle, BONE, DIM};
use super::{out, Event, Menus, View};
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::{fonts, Face};
use serde_json::Value;

const W_WORD: u32 = 1;
const W_NEWS: u32 = 2;
const W_WHO: u32 = 3;
const W_MEDIA: u32 = 4;

pub struct Loader {
    news_open: bool,
    news_at: f32,
    news_under: f32,
    who_hover: f32,
    hot: [f32; 3],
    disc_turn: f32,
    pulse: f32,
    /// The words' actions as last drawn, the main one's (Enter presses it), the way back's (Esc).
    actions: Vec<String>,
    main: Option<String>,
    back: Option<String>,
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

impl Loader {
    pub fn new() -> Loader {
        Loader { news_open: false, news_at: -10.0, news_under: 0.0, who_hover: 0.0, hot: [0.0; 3], disc_turn: 0.0, pulse: 0.0, actions: Vec::new(), main: None, back: None }
    }

    /// How far element `order` has risen from its line (the page's own clock).
    fn risen(m: &Menus, v: &View, order: f32) -> f32 {
        smooth(0.0, 1.0, (m.now - v.opened - 0.15 - order * 0.08) / 0.95)
    }

    /// How far it has sunk as the page goes (1 not at all).
    fn sunk(m: &Menus, v: &View, order: f32) -> f32 {
        match v.leaving {
            Some(at) => 1.0 - smooth(0.0, 1.0, (m.now - at - order * 0.03) / 0.36),
            None => 1.0,
        }
    }

    fn live(m: &Menus, v: &View) -> bool {
        v.leaving.is_none() && m.now - v.opened > 0.45
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let (w, h) = (m.w, m.h);
        let dt = m.dt;
        let ui = m.uipx();
        let narrow = w < 760.0 * ui;
        let vh = h / 100.0;
        let left = if narrow { 20.0 * ui } else { w * 0.062 };
        let live = Self::live(m, v);
        let col_w = (w * 0.5).max(320.0 * ui);

        // the column: caption, title, text, the words; 44% of it above the middle
        let cap = m.cl(9.0, 13.0, 1.15);
        let cap_text = s(v.data, "cap").to_uppercase();
        let title = s(v.data, "title");
        let thin = s(v.data, "title_face") != "book";
        let title_face = if thin { Face::Thin } else { Face::Book };
        let title_size = if thin { m.cl(40.0, 132.0, 11.0) } else { m.cl(18.0, 40.0, 3.4) };
        let text_size = m.cl(13.0, 24.0, 2.0);
        let text = s(v.data, "text");
        let lines: Vec<String> = if text.is_empty() { Vec::new() } else { text.split('\n').flat_map(|p| fonts().wrap_ls(Face::Light, text_size, p, col_w, 0.02)).collect() };
        let words: Vec<(String, String, i64)> = v
            .data
            .get("words")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| Some((x.get(0)?.as_str()?.to_string(), x.get(1)?.as_str()?.to_string(), x.get(2).and_then(Value::as_i64).unwrap_or(0)))).collect())
            .unwrap_or_default();
        let sizes: Vec<f32> = words
            .iter()
            .map(|w| match w.2 {
                1 => m.cl(40.0, 106.0, 9.4),
                2 => m.cl(22.0, 44.0, 4.0),
                _ => m.cl(30.0, 70.0, 6.1),
            })
            .collect();
        let mono_line = (fonts().ascent(Face::Caps, cap) + fonts().descent(Face::Caps, cap)) * 1.15;
        let mut col_h = 0.0;
        if !cap_text.is_empty() {
            col_h += mono_line + 1.2 * vh;
        }
        if !title.is_empty() {
            col_h += title_size * 1.1 + 1.0 * vh;
        }
        col_h += lines.len() as f32 * text_size * 1.35 + if lines.is_empty() { 0.0 } else { 2.2 * vh };
        for (i, (_, _, kind)) in words.iter().enumerate() {
            col_h += 1.12 * sizes[i] + if *kind == 2 { 2.4 * vh } else { 0.0 };
        }
        let mut y = if narrow { h * 0.56 } else { h * 0.5 } - 0.44 * col_h;
        let mut order = 0.0;
        if !cap_text.is_empty() {
            let p = Self::risen(m, v, order).min(Self::sunk(m, v, order));
            let q = clamp01(ease_out(0.22, 1.0, p) * 1.15);
            m.rising_text(c, Face::Caps, &cap_text, left, y, cap, 1.15, 0.24, grey(0.5, 1.0), 0.0, q);
            m.reveal_line(c, left, y + mono_line + 0.3 * vh, left + tw(Face::Caps, &cap_text, cap, 0.24), p);
            y += mono_line + 1.2 * vh;
            order += 1.0;
        }
        if !title.is_empty() {
            let p = Self::risen(m, v, order).min(Self::sunk(m, v, order));
            let q = clamp01(ease_out(0.22, 1.0, p) * 1.15);
            let fitted = fonts().fit_ls(title_face, title_size, title, col_w.max(w - 2.0 * left), 0.02);
            m.rising_text(c, title_face, &fitted, left, y, title_size, 1.1, 0.02, grey(0.96, 1.0), 0.0, q);
            y += title_size * 1.1 + 1.0 * vh;
            order += 1.0;
        }
        for l in &lines {
            let p = Self::risen(m, v, order).min(Self::sunk(m, v, order));
            let q = clamp01(ease_out(0.22, 1.0, p) * 1.15);
            m.rising_text(c, Face::Light, l, left, y, text_size, 1.35, 0.02, grey(0.78, 1.0), 0.0, q);
            y += text_size * 1.35;
            order += 0.5;
        }
        if !lines.is_empty() {
            y += 2.2 * vh;
        }
        self.actions = words.iter().map(|w| w.0.clone()).collect();
        self.main = words.iter().find(|w| w.2 == 1).or(words.first()).map(|w| w.0.clone());
        self.back = words.iter().find(|w| w.0 == "back" || w.0 == "cancel").map(|w| w.0.clone());
        let focus = (0..words.len()).any(|i| m.lit == id(W_WORD, i as u32));
        for (i, (_, label, kind)) in words.iter().enumerate() {
            if *kind == 2 {
                y += 2.4 * vh;
            }
            let size = sizes[i];
            let p = Self::risen(m, v, order).min(Self::sunk(m, v, order));
            let face = if *kind == 1 { Face::Light } else { Face::Thin };
            let st = WordStyle::new(face, size).rest(if *kind == 1 { BONE } else { DIM });
            m.word(c, id(W_WORD, i as u32), &label.to_uppercase(), left, y, st, p, live, focus);
            y += 1.12 * size;
            order += 1.0;
        }
        self.pulse *= (-dt * 2.2).exp();
        m.out[out::BOOST] = if focus && live { 0.14 } else { 0.0 } + self.pulse;
        m.out[out::PULSE] = self.pulse;

        // the account, top right
        let nick = s(v.data, "nick").to_uppercase();
        if !nick.is_empty() {
            let who_size = m.cl(18.0, 34.0, 3.0);
            let nick_w = tw(Face::Book, &nick, who_size, 0.05);
            let right = if narrow { w - 20.0 * ui } else { w - 4.4 * w / 100.0 };
            let who_top = 5.2 * vh;
            let who_box = who_size * 1.15 + 0.25 * who_size;
            let who = [right - nick_w, who_top, right, who_top + who_box];
            self.who_hover = toward(self.who_hover, if live && m.over(who) { 1.0 } else { 0.0 }, 6.0, dt);
            if live {
                m.hit(who, id(W_WHO, 0));
            }
            let pw = Self::risen(m, v, 2.0).min(Self::sunk(m, v, 0.0));
            let qw = clamp01(ease_out(0.22, 1.0, pw) * 1.15);
            m.rising_text(c, Face::Book, &nick, who[0], who_top, who_size, 1.15, 0.05, grey(0.902 + 0.098 * self.who_hover, 1.0), 0.0, qw);
            m.reveal_line(c, who[0], who[3], right, pw);
            let sub = s(v.data, "sub").to_uppercase();
            if !sub.is_empty() {
                let small = m.cl(8.0, 12.0, 1.05);
                let sw = tw(Face::Caps, &sub, small, 0.2);
                m.rising_text(c, Face::Caps, &sub, right - sw, who[3] + 0.9 * vh, small, 1.3, 0.2, grey(0.5, 1.0), 0.0, qw);
            }
        }

        self.media(m, c, v, live, narrow);

        // the build line and what's new, bottom left (the valley's RECODE over them: the launch draws it at LEFT)
        let small = m.cl(8.0, 12.0, 1.05);
        let small_line = small * 1.3;
        let foot_bottom = h - 5.0 * vh;
        let foot_top = foot_bottom - small_line - 0.4 * vh;
        m.out[out::LEFT] = left;
        m.out[out::FOOT_TOP] = foot_top;
        let build = s(v.data, "build").to_uppercase();
        let build_w = tw(Face::Caps, &build, small, 0.22);
        let pf = Self::risen(m, v, 4.0).min(Self::sunk(m, v, 0.0));
        m.rising_text(c, Face::Caps, &build, left, foot_top, small, 1.3, 0.22, grey(0.427, 1.0), 0.0, pf);
        let news: Vec<String> = v.data.get("news").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        if !news.is_empty() {
            let label = (if ru() { "Что нового" } else { "What's new" }).to_uppercase();
            let x = left + build_w + if build.is_empty() { 0.0 } else { 3.0 * vh };
            let lw = tw(Face::Caps, &label, small, 0.22);
            let r = [x, foot_top, x + lw, foot_bottom];
            if live {
                m.hit(r, id(W_NEWS, 0));
            }
            let hot = live && m.over(r) || self.news_open;
            self.news_under = toward(self.news_under, if hot { 1.0 } else { 0.0 }, 5.0, dt);
            m.rising_text(c, Face::Caps, &label, x, foot_top, small, 1.3, 0.22, grey(0.627 + 0.373 * self.news_under, 1.0), 0.0, pf);
            if self.news_under > 0.01 {
                c.light(x, foot_bottom, x + lw, 0.9, 0.0, self.news_under, 0.0, ui);
            }
            self.news(m, c, &news, x, foot_top - 2.2 * vh);
        }
    }

    /// What plays on the computer, bottom right (Windows' own media controls; her word: no music of the loader's own).
    fn media(&mut self, m: &mut Menus, c: &mut Canvas, v: &View, live: bool, narrow: bool) {
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        let Some(md) = v.data.get("media").filter(|x| x.is_object()) else { return };
        if h < 520.0 * ui {
            return;
        }
        let dt = m.dt;
        let vh = h / 100.0;
        let width = m.cl(220.0, 320.0, 27.0);
        let right = if narrow { w - 20.0 * ui } else { w - 4.4 * w / 100.0 };
        let left = right - width;
        let bottom = h - if narrow { 12.0 } else { 6.0 } * vh;
        let disc = m.cl(32.0, 54.0, 5.0);
        let track = m.cl(15.0, 27.0, 2.4);
        let small = m.cl(8.0, 12.0, 1.1);
        let gap_x = 1.4 * vh;
        let gap_y = 0.45 * vh;
        let (track_h, artist_h, ctl_h) = (track * 1.2, small * 1.3, (14.0 * ui).max(1.9 * vh));
        let ctl_top = bottom - ctl_h;
        let bar_y = ctl_top - gap_y;
        let bar_top = bar_y - 1.0 * vh;
        let artist_top = bar_top - gap_y - artist_h;
        let track_top = artist_top - gap_y - track_h;
        let mp = (m.now - v.opened - 0.4) / 1.05;
        let sunk = Self::sunk(m, v, 0.0);
        let f = |k: &str| md.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let (pos, len, on) = (f("pos"), f("len"), md.get("playing").and_then(Value::as_bool).unwrap_or(false));
        let disc_y = track_top + (track_h + gap_y + artist_h - disc) / 2.0;
        let disc_a = smooth(0.0, 0.8, mp).min(sunk);
        if on {
            self.disc_turn += dt * 360.0 / 4.5;
        }
        c.disc([left, disc_y, left + disc, disc_y + disc], disc_a, (self.disc_turn % 360.0).to_radians());
        let text_x = left + disc + gap_x;
        let text_w = right - text_x;
        let p0 = ease_out(0.0, 1.0, mp).min(sunk);
        let title = fonts().fit_ls(Face::Heavy, track, &s(md, "title").to_uppercase(), text_w, 0.03);
        m.rising_text(c, Face::Heavy, &title, text_x, track_top, track, 1.2, 0.03, grey(0.902, 1.0), 0.0, p0);
        let p1 = ease_out(0.0, 1.0, mp - 0.1).min(sunk);
        let artist = fonts().fit_ls(Face::Caps, small, &s(md, "artist").to_uppercase(), text_w, 0.16);
        m.rising_text(c, Face::Caps, &artist, text_x, artist_top, small, 1.3, 0.16, grey(0.604, 1.0), 0.0, p1);
        let draw = ease_out(0.0, 1.0, mp - 0.2).min(sunk);
        if draw > 0.01 && len > 0.0 {
            c.light(left, bar_y, left + width, 0.14, 0.0, draw, 0.0, ui);
            c.light(left, bar_y, left + width * (pos / len).min(1.0), 0.8, 0.0, draw, 0.5, ui);
        }
        let pc = ease_out(0.0, 1.0, mp - 0.2).min(sunk);
        if len > 0.0 {
            let time = format!("{} / {}", fmt(pos), fmt(len));
            m.rising_text(c, Face::Caps, &time, left, ctl_top, small, ctl_h / small, 0.1, grey(0.604, 1.0), 0.0, pc);
        }
        let btn = (14.0 * ui).max(1.9 * vh);
        let gap = 1.6 * vh;
        let bx = right - btn * 3.0 - gap * 2.0;
        let off = (1.0 - pc) * 1.08 * ctl_h;
        let kinds = [0u32, if on { 1 } else { 2 }, 3];
        for i in 0..3 {
            let x = bx + (btn + gap) * i as f32;
            let r = [x, ctl_top, x + btn, ctl_top + btn];
            self.hot[i] = toward(self.hot[i], if live && m.over(r) { 1.0 } else { 0.0 }, 6.0, dt);
            if live {
                m.hit(r, id(W_MEDIA, i as u32));
            }
            c.push_clip([r[0] - 2.0, ctl_top - 2.0, r[2] + 2.0, ctl_top + ctl_h + 2.0]);
            c.icon([r[0], r[1] + off, r[2], r[3] + off], kinds[i], grey(0.604 + 0.396 * self.hot[i], pc));
            c.pop_clip();
        }
    }

    /// What's new: the build's notes rising in above the button (and sinking away when closed).
    fn news(&mut self, m: &mut Menus, c: &mut Canvas, rows: &[String], x: f32, bottom: f32) {
        let since = m.now - self.news_at;
        if !self.news_open && since > 0.4 {
            return;
        }
        let head = m.cl(14.0, 25.0, 2.2);
        let body = m.cl(9.0, 13.0, 1.15);
        let gap = 0.8 * m.h / 100.0;
        let heights: Vec<f32> = (0..rows.len()).map(|i| if i == 0 { head * 1.15 } else { body * 1.5 }).collect();
        let total: f32 = heights.iter().sum::<f32>() + gap * (rows.len().max(1) - 1) as f32;
        let mut y = bottom - total;
        for (i, row) in rows.iter().enumerate() {
            let k = if self.news_open { ease_out(0.0, 1.0, (since - i as f32 * 0.07) / 0.6) } else { 1.0 - clamp01(since / 0.35) };
            let alpha = if self.news_open { 1.0 } else { k };
            if i == 0 {
                m.rising_text(c, Face::Heavy, &row.to_uppercase(), x, y, head, 1.15, 0.06, grey(1.0, alpha), 0.0, k);
            } else {
                let px = m.rising_text(c, Face::Caps, "+", x, y, body, 1.5, 0.04, grey(0.427, alpha), 0.0, k);
                m.rising_text(c, Face::Caps, row, px + body * 0.6 * 1.2, y, body, 1.5, 0.04, grey(0.902, alpha), 0.0, k);
            }
            y += heights[i] + gap;
        }
    }

    pub fn event(&mut self, m: &mut Menus, v: &View, e: Event) {
        let live = Self::live(m, v);
        match e {
            Event::Press(hit, b) if live && b & 15 == 0 => {
                if let Some(i) = (0..self.actions.len()).find(|&i| hit == id(W_WORD, i as u32)) {
                    self.pulse = self.pulse.max(0.7);
                    m.ask(self.actions[i].clone());
                } else if hit == id(W_NEWS, 0) {
                    self.news_open = !self.news_open;
                    self.news_at = m.now;
                } else if hit == id(W_WHO, 0) {
                    m.ask("account");
                } else if let Some(i) = (0..3).find(|&i| hit == id(W_MEDIA, i)) {
                    m.ask(["media prev", "media toggle", "media next"][i as usize]);
                }
            }
            Event::Key(k, _) if live && (k == "Enter" || k == "Return") => {
                if let Some(a) = self.main.clone() {
                    m.ask(a);
                }
            }
            Event::Key(k, _) if k == "Escape" => {
                if self.news_open {
                    self.news_open = false;
                    self.news_at = m.now;
                } else if let Some(a) = self.back.clone().filter(|_| live) {
                    m.ask(a);
                }
            }
            _ => {}
        }
    }
}

fn fmt(s: f32) -> String {
    let (mm, r) = ((s / 60.0) as i32, (s % 60.0) as i32);
    format!("{mm}:{r:02}")
}
