//! The main menu (as the approved launch preview has it): the menu on the left rising from lines of light, the account
//! top right, the music bottom right, the build line and what's new bottom left. Under the hovered word a line catches
//! fire; Quit is held. Its clocks come from Java's LaunchScreen (the card's second: the menu rises once a session).

use super::lang::tr;
use super::ui::{clamp01, ease_out, id, smooth, toward, tw, WordStyle, BONE, DIM, DIMMER};
use super::{out, Event, Fly, Menus, View};
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::{fonts, Face};
use crate::launch::timeline::tl;

/// The card's schedule (LaunchScreen, timeline.rs tl): the build line rises at BUILD, the account and the music at SIDE.
const BUILD: f32 = tl::BUILD;
const SIDE: f32 = tl::SIDE;

const W_ITEM: u32 = 1;
const W_NEWS: u32 = 2;
const W_MUSIC: u32 = 4;

/// The menu's words: their key, face, size (% of the height, at least and at most in ui px), whether it is the main one.
const ITEMS: [(&str, Face, f32, f32, f32, bool); 5] = [
    ("continue", Face::Light, 9.4, 40.0, 106.0, true),
    ("single", Face::Thin, 6.1, 30.0, 70.0, false),
    ("multi", Face::Thin, 6.1, 30.0, 70.0, false),
    ("settings", Face::Thin, 6.1, 30.0, 70.0, false),
    ("quit", Face::Thin, 4.0, 22.0, 44.0, false),
];
const QUIT: usize = 4;

pub struct Main {
    /// The card's second when this opened: the first time the menu rises with the card, later from its lines.
    card_at_open: f32,
    hold_from: f32,
    news_open: bool,
    news_at: f32,
    news_under: f32,
    who_hover: f32,
    boost: f32,
    pulse: f32,
    clicks: f32,
    disc_turn: f32,
    hot: [f32; 3],
    /// The word that flew off to be the next screen's title (not drawn here as the rest sink).
    flown: Option<usize>,
    /// Where each word was last drawn (its box, its size).
    boxes: [([f32; 4], f32); 5],
    /// What the main menu's own screen last said (the card's second, the way out, the music) and when: as it goes, the
    /// screen after it draws it and says none of that, so it goes on from these.
    kept: [f32; 5],
    kept_at: f32,
    /// Alt Manager's card under the nickname (alts.rs), and where the nickname was drawn.
    alts: super::alts::AltCard,
    who: [f32; 4],
}

impl Main {
    pub fn new() -> Main {
        Main {
            card_at_open: -1.0,
            hold_from: -1.0,
            news_open: false,
            news_at: -10.0,
            news_under: 0.0,
            who_hover: 0.0,
            boost: 0.0,
            pulse: 0.0,
            clicks: 0.0,
            disc_turn: 0.0,
            hot: [0.0; 3],
            flown: None,
            boxes: [([0.0; 4], 0.0); 5],
            kept: [0.0, -1.0, 0.0, 1.0, 0.0],
            kept_at: f32::NAN,
            alts: super::alts::AltCard::new(),
            who: [0.0; 4],
        }
    }

    /// Draws the main menu. While it goes (another screen is up and draws it) the card's clock and the music go on from
    /// what its own screen said last: the menu sinks as it is, and its world stays as it was.
    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let said: [f32; 5] = m.inp[..5].try_into().unwrap();
        if v.leaving.is_none() || self.kept_at.is_nan() {
            self.kept = said;
            self.kept_at = m.now;
        } else {
            let mut k = self.kept;
            k[0] += m.now - self.kept_at;
            m.inp[..5].copy_from_slice(&k);
        }
        self.draw_page(m, c, v);
        m.inp[..5].copy_from_slice(&said);
    }

    fn card(m: &Menus) -> f32 {
        m.inp[0]
    }

    fn exit_e(m: &Menus) -> f32 {
        m.inp[1]
    }

    /// How far the menu has sunk on the way out (1 not at all), or into its lines as another screen comes
    /// (`order` goes a little after the one before).
    fn sunk(&self, m: &Menus, v: &View, order: f32) -> f32 {
        let e = Self::exit_e(m);
        let quit = if e >= 0.0 { 1.0 - smooth(0.0, 1.0, (e - order * 0.05) / 0.6) } else { 1.0 };
        let away = match v.leaving {
            Some(at) => 1.0 - smooth(0.0, 1.0, (m.now - at - order * 0.03) / 0.36),
            None => 1.0,
        };
        quit.min(away)
    }

    /// How far element `order` has risen: with the card the first time, from its line when the menu comes back.
    fn risen(&self, m: &Menus, v: &View, from: f32, order: f32, step: f32) -> f32 {
        if self.card_at_open < tl::LIVE {
            smooth(0.0, 1.0, (Self::card(m) - from - order * step) / 1.05)
        } else {
            smooth(0.0, 1.0, (m.now - v.opened - order * 0.07) / 0.95)
        }
    }

    fn live(&self, m: &Menus, v: &View) -> bool {
        Self::card(m) >= tl::LIVE && Self::exit_e(m) < 0.0 && v.leaving.is_none()
    }

    fn draw_page(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        if self.card_at_open < 0.0 {
            self.card_at_open = Self::card(m);
        }
        let (w, h) = (m.w, m.h);
        let t = Self::card(m);
        let dt = m.dt;
        let ui = m.uipx();
        let narrow = w < 760.0 * ui;
        let vh = h / 100.0;
        let left = if narrow { 20.0 * ui } else { w * 0.062 };
        let live = self.live(m, v);
        let leaving = v.leaving.is_some();

        // the menu's column: last server, the words; 44% of it above the middle
        let cap = m.cl(9.0, 13.0, 1.15);
        let last = v.data.get("last").filter(|l| l.is_object());
        let mono_line = (fonts().ascent(Face::Caps, cap) + fonts().descent(Face::Caps, cap)) * 1.15;
        let last_h = if last.is_some() { mono_line * 2.0 + 0.45 * vh + 0.5 * vh } else { 0.0 };
        let mut col_h = if last.is_some() { last_h + 1.4 * vh } else { 0.0 };
        let sizes: Vec<f32> = ITEMS.iter().map(|it| m.cl(it.3, it.4, it.2)).collect();
        for (i, s) in sizes.iter().enumerate() {
            col_h += 1.12 * s + if i == QUIT { 2.4 * vh } else { 0.0 };
        }
        let mut y = if narrow { h * 0.56 } else { h * 0.5 } - 0.44 * col_h;
        let mut order = 0.0;
        if let Some(l) = last {
            let p = self.risen(m, v, tl::MENU_FROM, 0.0, 0.13).min(self.sunk(m, v, 0.0));
            let rp = ease_out(0.22, 1.0, p);
            let cap_text = tr("last").to_uppercase();
            let addr = l.get("address").and_then(|a| a.as_str()).unwrap_or("");
            let cap_w = tw(Face::Caps, &cap_text, cap, 0.24);
            let srv_w = tw(Face::Caps, addr, cap, 0.08);
            for i in 0..2 {
                let q = clamp01(rp * 1.15 - i as f32 * 0.15);
                let top = y + i as f32 * (mono_line + 0.45 * vh);
                if i == 0 {
                    m.rising_text(c, Face::Caps, &cap_text, left, top, cap, 1.15, 0.24, grey(0.373, 1.0), 0.0, q);
                } else {
                    m.rising_text(c, Face::Caps, addr, left, top, cap, 1.15, 0.08, grey(0.639, 1.0), 0.0, q);
                }
            }
            m.reveal_line(c, left, y + last_h, left + cap_w.max(srv_w), p);
            y += last_h + 1.4 * vh;
            order += 1.0;
        }
        let focus = ITEMS.iter().enumerate().any(|(i, _)| m.lit == id(W_ITEM, i as u32));
        for (i, it) in ITEMS.iter().enumerate() {
            if i == QUIT {
                y += 2.4 * vh;
            }
            let size = sizes[i];
            let label = tr(it.0).to_uppercase();
            let p = self.risen(m, v, tl::MENU_FROM, order, 0.13).min(self.sunk(m, v, order));
            let st = WordStyle::new(it.1, size).rest(if it.5 { BONE } else { DIM });
            if self.flown != Some(i) {
                let r = m.word(c, id(W_ITEM, i as u32), &label, left, y, st, p, live, focus);
                self.boxes[i] = (r, size);
            }
            // Quit held: a brighter line runs under it; once full, the way out
            if i == QUIT && self.hold_from >= 0.0 {
                let hq = ((m.now - self.hold_from) / 1.0).min(1.0);
                let lw = tw(it.1, &label, size, 0.02);
                let line_y = y + 1.12 * size - 0.06 * size;
                c.light(left, line_y, left + lw, 1.6, 0.0, hq, 1.2, ui);
                if hq >= 1.0 && Self::exit_e(m) < 0.0 {
                    self.hold_from = -1.0;
                    m.ask("quit");
                }
                if m.lit != id(W_ITEM, QUIT as u32) {
                    self.hold_from = -1.0;
                }
            }
            y += 1.12 * size;
            order += 1.0;
        }
        let hovered = focus && live;
        self.boost = toward(self.boost, if hovered { 0.14 } else { 0.0 }, 2.2, dt);
        self.pulse *= (-dt * 2.2).exp();

        // the account, top right
        let who_size = m.cl(18.0, 34.0, 3.0);
        let head = m.cl(18.0, 30.0, 2.7);
        // (the account in use: Alt Manager changes it while the menu is up)
        let in_use = crate::alts::active();
        let nick = if in_use.is_empty() { v.str("nick").to_uppercase() } else { in_use.to_uppercase() };
        let nick_w = tw(Face::Book, &nick, who_size, 0.05);
        let gap = 1.1 * vh;
        let right = if narrow { w - 20.0 * ui } else { w - 4.4 * w / 100.0 };
        let who_top = 5.2 * vh;
        let who_line = who_size * 1.15;
        let who_box = who_line + 0.25 * who_size;
        let who_left = right - nick_w - gap - head;
        let who = [who_left, who_top, right, who_top + who_box];
        self.who_hover = toward(self.who_hover, if live && m.over(who) || self.alts.is_open() { 1.0 } else { 0.0 }, 6.0, dt);
        self.alts.who(m, who, live);
        self.who = who;
        let pw = self.risen(m, v, SIDE, 0.0, 0.0).min(self.sunk(m, v, 0.0));
        let qw = clamp01(ease_out(0.22, 1.0, pw) * 1.15);
        m.rising_text(c, Face::Book, &nick, who_left, who_top, who_size, 1.15, 0.05, grey(0.902 + 0.098 * self.who_hover, 1.0), 0.0, qw);
        let head_y = who_top + (who_line - head) / 2.0 - 0.06 * who_size;
        m.out[out::HEAD] = right - head;
        m.out[out::HEAD + 1] = head_y;
        m.out[out::HEAD + 2] = head;
        m.out[out::HEAD + 3] = (smooth(0.0, 1.0, (t - SIDE - 0.2) / 0.8)).min(self.sunk(m, v, 0.0)).min(if self.card_at_open < tl::LIVE { 1.0 } else { pw });
        m.reveal_line(c, who_left, who_top + who_box, right, pw);

        self.music(m, c, v, live, narrow);

        // the build line and what's new, bottom left
        let small = m.cl(8.0, 12.0, 1.05);
        let small_line = small * 1.3;
        let foot_bottom = h - 5.0 * vh;
        let foot_top = foot_bottom - small_line - 0.4 * vh;
        m.out[out::LEFT] = left;
        m.out[out::FOOT_TOP] = foot_top;
        let build = "CURSED · ALPHA 0.1 · 26.3";
        let build_w = tw(Face::Caps, build, small, 0.22);
        for i in 0..2 {
            let p = if self.card_at_open < tl::LIVE { ease_out(0.0, 1.0, (t - BUILD) / 0.9 - i as f32 * 0.2) } else { self.risen(m, v, 0.0, 4.0 + i as f32, 0.0) };
            let p = p.min(self.sunk(m, v, 0.0));
            let x = if i == 0 { left } else { left + build_w + 3.0 * vh };
            if i == 0 {
                m.rising_text(c, Face::Caps, build, x, foot_top, small, 1.3, 0.22, grey(0.427, 1.0), 0.0, p);
            } else {
                let label = tr("news").to_uppercase();
                let lw = tw(Face::Caps, &label, small, 0.22);
                let r = [x, foot_top, x + lw, foot_bottom];
                if live {
                    m.hit(r, id(W_NEWS, 0));
                }
                let hot = live && m.over(r) || self.news_open;
                self.news_under = toward(self.news_under, if hot { 1.0 } else { 0.0 }, 5.0, dt);
                m.rising_text(c, Face::Caps, &label, x, foot_top, small, 1.3, 0.22, grey(0.627 + 0.373 * self.news_under, 1.0), 0.0, p);
                if self.news_under > 0.01 {
                    c.light(x, foot_bottom, x + lw, 0.9, 0.0, self.news_under, 0.0, ui);
                }
                self.news(m, c, x, foot_top - 2.2 * vh);
            }
        }

        // the scene: the glow of a hover, presses (no fire under the lit word: everything is monochrome, 14.10)
        for at in [out::FLAME0, out::FLAME1] {
            m.out[at + 4] = 0.0;
        }
        let _ = h;
        m.out[out::BOOST] = self.boost + self.pulse;
        m.out[out::PULSE] = self.pulse;
        m.out[out::CLICKS] = self.clicks;
        let _ = leaving;
        // Alt Manager's card, over all of it
        let who = self.who;
        self.alts.draw(m, c, who, live, false);
    }

    /// The music, bottom right: the record turning while it plays, the track and its artist, how far it is, the time,
    /// back to the start / pause / on.
    fn music(&mut self, m: &mut Menus, c: &mut Canvas, v: &View, live: bool, narrow: bool) {
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        if h < 520.0 * ui {
            return;
        }
        let dt = m.dt;
        let t = Self::card(m);
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
        let mp = if self.card_at_open < tl::LIVE { (t - SIDE - 0.15) / 1.05 } else { (m.now - v.opened - 0.1) / 1.05 };
        let sunk = self.sunk(m, v, 0.0);
        let (pos, len, on) = (m.inp[2], m.inp[3].max(1.0), m.inp[4] > 0.5);
        let disc_y = track_top + (track_h + gap_y + artist_h - disc) / 2.0;
        let disc_a = smooth(0.0, 0.8, mp).min(sunk);
        if on {
            self.disc_turn += dt * 360.0 / 4.5;
        }
        c.disc([left, disc_y, left + disc, disc_y + disc], disc_a, (self.disc_turn % 360.0).to_radians());
        let text_x = left + disc + gap_x;
        let p0 = ease_out(0.0, 1.0, mp).min(sunk);
        m.rising_text(c, Face::Heavy, "MONOCHROME LCD", text_x, track_top, track, 1.2, 0.03, grey(0.902, 1.0), 0.0, p0);
        let p1 = ease_out(0.0, 1.0, mp - 0.1).min(sunk);
        m.rising_text(c, Face::Caps, "MIKE KLUBNIKA", text_x, artist_top, small, 1.3, 0.16, grey(0.604, 1.0), 0.0, p1);
        // the bar: a faint line, the part played bright
        let draw = ease_out(0.0, 1.0, mp - 0.2).min(sunk);
        if draw > 0.01 {
            c.light(left, bar_y, left + width, 0.14, 0.0, draw, 0.0, ui);
            c.light(left, bar_y, left + width * (pos / len).min(1.0), 0.8, 0.0, draw, 0.5, ui);
        }
        // the controls: the time on the left, three buttons on the right
        let pc = ease_out(0.0, 1.0, mp - 0.2).min(sunk);
        let time = format!("{} / {}", fmt(pos), fmt(len));
        m.rising_text(c, Face::Caps, &time, left, ctl_top, small, ctl_h / small, 0.1, grey(0.604, 1.0), 0.0, pc);
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
                m.hit(r, id(W_MUSIC, i as u32));
            }
            c.push_clip([r[0] - 2.0, ctl_top - 2.0, r[2] + 2.0, ctl_top + ctl_h + 2.0]);
            c.icon([r[0], r[1] + off, r[2], r[3] + off], kinds[i], grey(0.604 + 0.396 * self.hot[i], pc));
            c.pop_clip();
        }
    }

    /// What's new: lines rising in above the button (and sinking away when closed).
    fn news(&mut self, m: &mut Menus, c: &mut Canvas, x: f32, bottom: f32) {
        let since = m.now - self.news_at;
        if !self.news_open && since > 0.4 {
            return;
        }
        let head = m.cl(14.0, 25.0, 2.2);
        let body = m.cl(9.0, 13.0, 1.15);
        let rows = [tr("news.0"), tr("news.1"), tr("news.2"), tr("news.3")];
        let gap = 0.8 * m.h / 100.0;
        let heights: Vec<f32> = (0..rows.len()).map(|i| if i == 0 { head * 1.15 } else { body * 1.5 }).collect();
        let total: f32 = heights.iter().sum::<f32>() + gap * (rows.len() - 1) as f32;
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
        let live = self.live(m, v);
        if live && self.alts.event(m, &e) {
            return;
        }
        match e {
            // (before the menu is up nothing skips the launch: her word, 15.10 — it plays to its end)
            Event::Press(_, _) | Event::Key(..) if !live && Self::card(m) < tl::SETTLE => {}
            Event::Press(hit, b) if live && b & 15 == 0 => {
                if hit == id(W_ITEM, QUIT as u32) {
                    self.hold_from = m.now;
                } else if let Some(i) = (0..QUIT).find(|&i| hit == id(W_ITEM, i as u32)) {
                    self.pulse = self.pulse.max(0.7);
                    self.clicks += 1.0;
                    if i == 0 {
                        m.ask("continue");
                    } else {
                        // the word flies up to be the next screen's title
                        let (r, size) = self.boxes[i];
                        let st = ITEMS[i];
                        m.fly = Some(Fly { text: tr(st.0).to_uppercase(), face: st.1, size, at: [r[0] + (r[2] - r[0]) - tw(st.1, &tr(st.0).to_uppercase(), size, 0.02), r[1]], since: m.now, ls: 0.02 });
                        self.flown = Some(i);
                        m.ask(["", "open single", "open multi", "open settings"][i]);
                    }
                } else if hit == id(W_NEWS, 0) {
                    self.news_open = !self.news_open;
                    self.news_at = m.now;
                } else if hit == id(W_MUSIC, 1) {
                    m.ask("music 0");
                } else if hit == id(W_MUSIC, 0) || hit == id(W_MUSIC, 2) {
                    m.ask("music 1");
                }
            }
            Event::Release(_, _) => self.hold_from = -1.0,
            Event::Key(k, _) if k == "Escape" => {
                if self.news_open {
                    self.news_open = false;
                    self.news_at = m.now;
                }
            }
            _ => {}
        }
        let _ = DIMMER;
    }
}

fn fmt(s: f32) -> String {
    let (mm, r) = ((s / 60.0) as i32, (s % 60.0) as i32);
    format!("{mm}:{r:02}")
}
