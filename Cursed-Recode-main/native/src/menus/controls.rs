//! The menus' controls in the preview's way (style.css: .srow, .ch, .cy, .sl, .key, field): a row's caption on the
//! left in the condensed face, its control on the right — a choice of words with the chosen lit, ‹ a value ›, a hair
//! of light lit up to the value, an underlined field; each registers its boxes, the page hears the presses.

use super::ui::{clamp01, ease, toward, tw, WordStyle, BONE, DIM};
use super::Menus;
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::{fonts, Face};

/// A control's part's number: what it is, which widget, which part of it.
pub const fn hid(kind: u32, n: u32, sub: u32) -> u64 {
    ((kind as u64) << 48) | ((n as u64) << 16) | sub as u64
}
pub const fn hid_kind(id: u64) -> u32 {
    (id >> 48) as u32
}
pub const fn hid_n(id: u64) -> u32 {
    ((id >> 16) & 0xffff_ffff) as u32
}
pub const fn hid_sub(id: u64) -> u32 {
    (id & 0xffff) as u32
}

/// The parts' kinds (shared by the pages).
pub mod k {
    pub const ROW: u32 = 1;
    pub const CHOICE: u32 = 2;
    pub const PREV: u32 = 3;
    pub const NEXT: u32 = 4;
    pub const SLIDER: u32 = 5;
    pub const FIELD: u32 = 6;
    pub const BUTTON: u32 = 7;
    pub const LIST: u32 = 8;
    pub const TAB: u32 = 9;
    pub const RESET: u32 = 10;
    pub const BACK: u32 = 11;
    pub const FOOT: u32 = 12;
    pub const KEY: u32 = 13;
    pub const SECTION: u32 = 14;
    pub const SEARCH: u32 = 15;
    #[allow(dead_code)]
    pub const LOCK: u32 = 16;
    pub const PAGE: u32 = 17;
}

/// A text field being typed in: its owner's number, the text, the caret (in chars), when it was last moved.
#[derive(Clone)]
pub struct Field {
    pub id: u64,
    pub text: String,
    pub caret: usize,
    pub moved: f32,
    pub max: usize,
}

impl Field {
    pub fn new(id: u64, text: &str, max: usize, now: f32) -> Field {
        Field { id, text: text.to_string(), caret: text.chars().count(), moved: now, max: max.max(1) }
    }

    /// A key on the field: true when the text changed.
    pub fn key(&mut self, name: &str, mods: u32, now: f32) -> bool {
        let n = self.text.chars().count();
        self.moved = now;
        match name {
            "Backspace" if self.caret > 0 => {
                let at = if mods & 2 != 0 { self.word_left() } else { self.caret - 1 };
                self.remove(at, self.caret);
                self.caret = at;
                true
            }
            "Delete" if self.caret < n => {
                self.remove(self.caret, self.caret + 1);
                true
            }
            "Left" => {
                self.caret = if mods & 2 != 0 { self.word_left() } else { self.caret.saturating_sub(1) };
                false
            }
            "Right" => {
                self.caret = (self.caret + 1).min(n);
                false
            }
            "Home" => {
                self.caret = 0;
                false
            }
            "End" => {
                self.caret = n;
                false
            }
            _ => false,
        }
    }

    fn word_left(&self) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.caret;
        while i > 0 && chars[i - 1] == ' ' {
            i -= 1;
        }
        while i > 0 && chars[i - 1] != ' ' {
            i -= 1;
        }
        i
    }

    fn remove(&mut self, a: usize, b: usize) {
        let s: String = self.text.chars().enumerate().filter(|(i, _)| *i < a || *i >= b).map(|(_, c)| c).collect();
        self.text = s;
    }

    pub fn insert(&mut self, ch: char, now: f32) -> bool {
        if ch.is_control() || self.text.chars().count() >= self.max {
            return false;
        }
        let mut s: String = self.text.chars().take(self.caret).collect();
        s.push(ch);
        s.extend(self.text.chars().skip(self.caret));
        self.text = s;
        self.caret += 1;
        self.moved = now;
        true
    }

    pub fn paste(&mut self, t: &str, now: f32) -> bool {
        let mut any = false;
        for ch in t.chars() {
            any |= self.insert(ch, now);
        }
        any
    }
}

impl Menus {
    /// A row's caption (style.css .srow .lab .t): condensed, light, grey; under the pointer white and stepped aside, a
    /// faint line drawn under the whole row. `p` how far the row has risen. Returns the caption's end.
    #[allow(clippy::too_many_arguments)]
    pub fn row_caption(&mut self, c: &mut Canvas, rid: u64, text: &str, x: f32, top: f32, h: f32, width: f32, p: f32, enabled: bool) -> f32 {
        let size = self.cl(16.0, 43.0, 2.75);
        let hot = self.an(rid ^ 0x5157, if self.over_row(rid) && enabled { 1.0 } else { 0.0 }, 5.0);
        let q = clamp01(ease(p) * 1.15);
        let shift = hot * 0.12 * size;
        let line = 1.12;
        let ty = top + (h - size * line) / 2.0;
        let col = if enabled { 0.663 + 0.337 * hot } else { 0.3 };
        let end = self.rising_text(c, Face::Light, &text.to_uppercase(), x + shift, ty, size, line, 0.03, grey(col, 1.0), 0.0, q);
        if hot > 0.01 {
            let ui = self.uipx();
            c.light(x, top + h, x + width, 0.22 * hot, 0.0, ease(hot), 0.0, ui);
        }
        end
    }

    /// Whether the pointer is on row `rid` (its box registered this frame or the last).
    pub fn over_row(&self, rid: u64) -> bool {
        self.prev_hits.iter().any(|(r, id)| *id == rid && self.over(*r))
    }

    /// A choice of words (style.css .ch): the chosen one lit with its line under it, the rest dim; drawn leftward from
    /// `right`. Returns the leftmost x.
    #[allow(clippy::too_many_arguments)]
    pub fn choice(&mut self, c: &mut Canvas, kind: u32, n: u32, words: &[String], sel: Option<usize>, right: f32, top: f32, h: f32, p: f32, live: bool, enabled: bool) -> f32 {
        let size = self.cl(15.0, 39.0, 2.5);
        let gap = 2.0 * self.vh();
        let widths: Vec<f32> = words.iter().map(|w| tw(Face::Light, &w.to_uppercase(), size, 0.02)).collect();
        let total: f32 = widths.iter().sum::<f32>() + gap * (words.len().saturating_sub(1)) as f32;
        let mut x = right - total;
        let left = x;
        let y = top + (h - 1.12 * size) / 2.0;
        for (i, w) in words.iter().enumerate() {
            let wid = super::controls::hid(kind, n, i as u32);
            let chosen = sel == Some(i);
            let st = WordStyle::new(Face::Light, size).rest(if chosen { 1.0 } else { DIM }).enabled(enabled);
            let st = WordStyle { glow: 0.0, ..st };
            self.word(c, wid, &w.to_uppercase(), x, y, st, p, live, false);
            // the chosen word's line stays under it
            let k = self.an(wid ^ 0xc405e, if chosen { 1.0 } else { 0.0 }, 6.0);
            if k > 0.01 {
                let ly = y + 1.12 * size - 0.06 * size;
                c.light(x, ly, x + widths[i], 0.85 * k * p, 0.0, ease(k), 0.6, self.uipx());
            }
            x += widths[i] + gap;
        }
        left
    }

    /// ‹ a value › (style.css .cy): the arrows dim until the pointer is on them; the value slides in from the side it
    /// changed to. Returns the leftmost x.
    #[allow(clippy::too_many_arguments)]
    pub fn cycle(&mut self, c: &mut Canvas, n: u32, value: &str, index: i32, right: f32, top: f32, h: f32, p: f32, live: bool, enabled: bool, min_w: f32) -> f32 {
        let size = self.cl(15.0, 39.0, 2.5);
        let arrow = self.cl(11.0, 27.0, 1.7);
        let gap = 1.6 * self.vh();
        let vw = tw(Face::Light, &value.to_uppercase(), size, 0.02).max(min_w);
        let x1 = right - arrow;
        let vx1 = x1 - gap;
        let vx0 = vx1 - vw;
        let x0 = vx0 - gap - arrow;
        let cy = top + h / 2.0;
        let q = clamp01(ease(p) * 1.15);
        for (sub, ax, icon) in [(super::controls::k::PREV, x0, 7u32), (super::controls::k::NEXT, x1, 8u32)] {
            let r = [ax - 0.3 * arrow, top, ax + arrow * 1.3, top + h];
            let id = hid(sub, n, 0);
            if live && enabled {
                self.hit(r, id);
            }
            let hot = self.an(id ^ 0xa770, if live && enabled && self.over(r) { 1.0 } else { 0.0 }, 8.0);
            let s = arrow * 0.8;
            c.icon([ax, cy - s / 2.0 + (1.0 - q) * h, ax + s, cy + s / 2.0 + (1.0 - q) * h], icon, grey(if enabled { 0.333 + 0.667 * hot } else { 0.2 }, q));
        }
        // the value: slides in from where it went
        let key = hid(super::controls::k::NEXT, n, 0xffff);
        let last = self.anims.get(&key).copied().unwrap_or(index as f32);
        if (last - index as f32).abs() > 0.5 {
            // changed: the old leaves to the far side, the new comes from the near one
            let dir = if (index as f32) > last { 1.0 } else { -1.0 };
            self.anims.insert(key ^ 1, dir);
            self.anims.insert(key ^ 2, 0.0);
        }
        self.anims.insert(key, index as f32);
        let k = self.an(key ^ 2, 1.0, 6.5);
        let dir = self.anims.get(&(key ^ 1)).copied().unwrap_or(1.0);
        let off = (1.0 - ease(k)) * 0.9 * size * dir;
        c.push_clip([vx0, top, vx1, top + h]);
        let vtw = tw(Face::Light, &value.to_uppercase(), size, 0.02);
        let ty = top + (h - size * 1.15) / 2.0;
        self.rising_text(c, Face::Light, &value.to_uppercase(), vx0 + (vw - vtw) / 2.0 + off, ty, size, 1.15, 0.02, grey(if enabled { 1.0 } else { 0.35 }, ease(k).max(0.0)), 0.0, q);
        c.pop_clip();
        x0
    }

    /// A hair of light lit up to the value, a knob (style.css .sl), the value in mono beside it. Returns the track's
    /// box (the page drags by it).
    #[allow(clippy::too_many_arguments)]
    pub fn slider(&mut self, c: &mut Canvas, n: u32, v: f32, value: &str, right: f32, top: f32, h: f32, width: f32, p: f32, live: bool, enabled: bool) -> [f32; 4] {
        let num = self.cl(10.0, 22.0, 1.5);
        let num_w = tw(Face::Caps, "000000000", num, 0.06);
        // where the track begins stays; a longer value (a word: "Custom", "Пользовательский") shortens it from the right
        let x0 = right - num_w - 1.6 * self.vh() - width;
        let x1 = (right - num_w.max(tw(Face::Caps, value, num, 0.06)) - 1.6 * self.vh()).max(x0 + width * 0.35);
        let width = x1 - x0;
        let cy = top + h / 2.0;
        let id = hid(super::controls::k::SLIDER, n, 0);
        let r = [x0 - 6.0, top, x1 + 6.0, top + h];
        if live && enabled {
            self.hit(r, id);
        }
        let held = self.held == id;
        let hot = self.an(id ^ 0x51d, if live && enabled && (held || self.over(r)) { 1.0 } else { 0.0 }, 8.0);
        let draw = ease(p);
        let ui = self.uipx();
        let a = if enabled { 1.0 } else { 0.4 };
        // the track, thicker under the pointer
        c.line([x0, cy], [x0 + width * draw, cy], 1.0 + hot, grey(1.0, 0.14 * a), 0.0);
        let vx = x0 + width * clamp01(v);
        let shown = x0 + (vx - x0) * draw;
        c.rect_lit([x0, cy - 0.5 - hot * 0.5, shown, cy + 0.5 + hot * 0.5], 0.0, grey(1.0, 0.35 * a), grey(1.0, a), 1.0);
        c.glow([shown, cy], 6.0 * ui, grey(1.0, 0.25 * a * draw));
        let kh = (1.9 + 0.8 * hot) * self.vh() * 0.5;
        c.line([shown, cy - kh], [shown, cy + kh], 1.0, grey(1.0, a * draw), 5.0 * ui);
        // the value
        let q = clamp01(ease(p) * 1.15);
        let vw = tw(Face::Caps, value, num, 0.06);
        let ty = top + (h - num * 1.3) / 2.0;
        self.rising_text(c, Face::Caps, value, right - vw, ty, num, 1.3, 0.06, grey(BONE * a, 1.0), 0.0, q);
        [x0, top, x1, top + h]
    }

    /// An underlined field (the seed's, style.css .seed input): the text in mono, the caret, a line under it that lights
    /// while it has the keys. `field`: the one typed in now (if it is this). Returns its box.
    #[allow(clippy::too_many_arguments)]
    pub fn field(&mut self, c: &mut Canvas, id: u64, text: &str, hint: &str, x: f32, top: f32, width: f32, p: f32, live: bool, field: Option<&Field>) -> [f32; 4] {
        let size = self.cl(12.0, 29.0, 1.9);
        let h = size * 1.9;
        let r = [x, top, x + width, top + h];
        if live {
            self.hit(r, id);
        }
        let focus = field.is_some_and(|f| f.id == id);
        let shown = field.filter(|f| f.id == id).map_or(text, |f| f.text.as_str());
        let lit = self.an(id ^ 0xf1e1d, if focus { 1.0 } else { 0.0 }, 5.0);
        let q = clamp01(ease(p) * 1.15);
        let ty = top + (h - size * 1.3) / 2.0 - 0.15 * size;
        c.push_clip([x, top - 0.2 * size, x + width, top + h]);
        // the text scrolls so the caret stays in the field
        let caret_x = field.filter(|f| f.id == id).map_or(0.0, |f| tw(Face::Caps, &shown.chars().take(f.caret).collect::<String>(), size, 0.08));
        let scroll = (caret_x - width * 0.92).max(0.0);
        if shown.is_empty() && !hint.is_empty() {
            self.rising_text(c, Face::Caps, &hint.to_uppercase(), x, ty, size, 1.3, 0.08, grey(0.29, 1.0), 0.0, q);
        } else {
            self.rising_text(c, Face::Caps, shown, x - scroll, ty, size, 1.3, 0.08, grey(BONE, 1.0), 0.0, q);
        }
        if focus {
            let f = field.unwrap();
            let blink = ((self.now - f.moved) * 1.1).fract() < 0.6;
            if blink {
                let cx = x + caret_x - scroll;
                c.line([cx, ty + 0.15 * size], [cx, ty + 1.15 * size], 1.0, grey(1.0, 0.9), 0.0);
            }
        }
        c.pop_clip();
        let ui = self.uipx();
        let ly = top + h - 1.0;
        c.light(x, ly, x + width, 0.18 * ease(p), 0.0, 1.0, 0.0, ui);
        if lit > 0.01 {
            c.light(x, ly, x + width, lit, 0.0, ease(lit), 0.8, ui);
        }
        r
    }

    /// Long text in mono, wrapped to `width`, risen `q`: the lines' tops; returns the bottom.
    #[allow(clippy::too_many_arguments)]
    pub fn paragraph(&self, c: &mut Canvas, text: &str, x: f32, top: f32, width: f32, size: f32, bright: f32, q: f32, ls: f32) -> f32 {
        let lines = fonts().wrap_ls(Face::Caps, size, text, width, ls);
        let lh = size * 1.6;
        let mut y = top;
        for l in lines {
            self.rising_text(c, Face::Caps, &l, x, y, size, 1.6, ls, grey(bright, 1.0), 0.0, q);
            y += lh;
        }
        y
    }

    /// Scrolls `cur` toward `to`, smooth.
    pub fn scroll_ease(&mut self, key: u64, to: f32) -> f32 {
        let dt = self.dt;
        let v = self.anims.entry(key).or_insert(to);
        *v = toward(*v, to, 14.0, dt);
        *v
    }
}
