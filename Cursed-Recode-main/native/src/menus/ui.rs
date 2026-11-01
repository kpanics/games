//! The menus' words and lines (the main menu's language, as the approved preview has it): words in Sofia Sans Extra
//! Condensed that light up under the pointer with a line of fire under them; small spaced capitals in Martian Mono for
//! what is said about them; everything rises out of a line of light and sinks back into it.

use super::Menus;
use crate::gui::draw::{grey, Canvas, Rgba};
use crate::gui::text::{fonts, Face};

/// The preview's greys (style.css): the word at rest, a word dimmed while another is lit, a caption, the bone of what
/// is chosen, the soft grey of what is said beside.
pub const DIM: f32 = 0.478;
pub const DIMMER: f32 = 0.267;
pub const CAP: f32 = 0.373;
pub const BONE: f32 = 0.902;
pub const SOFT: f32 = 0.604;

pub fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}
pub fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let k = clamp01((x - a) / (b - a));
    k * k * (3.0 - 2.0 * k)
}
pub fn ease_out(a: f32, b: f32, x: f32) -> f32 {
    let k = clamp01((x - a) / (b - a));
    1.0 - (1.0 - k) * (1.0 - k) * (1.0 - k)
}
/// The preview's --ease (cubic-bezier(.22, .9, .24, 1)), near enough: a quick start, a long settle.
pub fn ease(k: f32) -> f32 {
    let k = clamp01(k);
    1.0 - (1.0 - k).powf(3.4)
}
pub fn ease_io(k: f32) -> f32 {
    let k = clamp01(k);
    if k < 0.5 {
        4.0 * k * k * k
    } else {
        1.0 - (-2.0 * k + 2.0).powi(3) / 2.0
    }
}
pub fn toward(v: f32, target: f32, rate: f32, dt: f32) -> f32 {
    v + (target - v) * (dt * rate).min(1.0)
}

/// An element's number: what it is, which one.
pub const fn id(kind: u32, n: u32) -> u64 {
    ((kind as u64) << 32) | n as u64
}

/// A word's light: how bright, how far it has stepped aside, its line under it (drawn from, to).
#[derive(Clone, Copy, Default)]
pub struct WordFx {
    pub color: f32,
    pub shift: f32,
    pub hl_from: f32,
    pub hl_to: f32,
    pub on: bool,
    pub gone: bool,
    pub seen: bool,
}

/// The fire under a lit word: its word, how far it has run, how much is left as it dies.
#[derive(Clone, Copy)]
pub struct Flame {
    pub word: u64,
    pub p: f32,
    pub a: f32,
    pub dying: bool,
}

/// How a word is set: its face, size (picture px), letter spacing (em), its grey at rest, whether it is the main one
/// (bone at rest), whether it can be pressed.
#[derive(Clone, Copy)]
pub struct WordStyle {
    pub face: Face,
    pub size: f32,
    pub ls: f32,
    pub rest: f32,
    pub enabled: bool,
    /// The word glows as it lights (the main menu's): 0 none.
    pub glow: f32,
}

impl WordStyle {
    pub fn new(face: Face, size: f32) -> WordStyle {
        WordStyle { face, size, ls: 0.02, rest: DIM, enabled: true, glow: 1.0 }
    }
    pub fn ls(mut self, ls: f32) -> WordStyle {
        self.ls = ls;
        self
    }
    pub fn rest(mut self, rest: f32) -> WordStyle {
        self.rest = rest;
        self
    }
    pub fn enabled(mut self, on: bool) -> WordStyle {
        self.enabled = on;
        self
    }
}

/// The width text takes as the preview's CSS measures it: every letter followed by its spacing.
pub fn tw(face: Face, s: &str, size: f32, ls: f32) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    fonts().width_ls(face, size, s, ls) + ls * size
}

impl Menus {
    /// Picture px per ui px: the preview's css px, a little larger on tall screens.
    pub fn uipx(&self) -> f32 {
        (self.h / 1300.0).max(1.0)
    }

    /// The preview's clamp(min px, pct vh, max px), its px as ui px.
    pub fn cl(&self, min: f32, max: f32, pct: f32) -> f32 {
        let ui = self.uipx();
        (self.h * pct / 100.0).clamp(min * ui, max * ui)
    }

    pub fn vh(&self) -> f32 {
        self.h / 100.0
    }

    /// A value that eases toward `target` (keyed): rate per second.
    pub fn an(&mut self, key: u64, target: f32, rate: f32) -> f32 {
        let dt = self.dt;
        let v = self.anims.entry(key).or_insert(target);
        *v = toward(*v, target, rate, dt);
        *v
    }

    pub fn an_set(&mut self, key: u64, v: f32) {
        self.anims.insert(key, v);
    }

    /// A box the pointer can press this frame.
    pub fn hit(&mut self, r: [f32; 4], id: u64) {
        self.hits.push((r, id));
    }

    pub fn over(&self, r: [f32; 4]) -> bool {
        let [x, y] = self.mouse;
        x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3]
    }

    /// Text set in a line box `line` em tall from `top`, risen `q` of the way out of it (0 still under its line), seen
    /// only inside its box (a little room above for the glow). Returns the pen's end.
    #[allow(clippy::too_many_arguments)]
    pub fn rising_text(&self, c: &mut Canvas, face: Face, s: &str, x: f32, top: f32, size: f32, line: f32, ls: f32, col: Rgba, glow: f32, q: f32) -> f32 {
        // the box: its line, or down to the descenders when a tight line would cut them (the tail of a р, a у)
        let baseline = fonts().baseline(face, top, size, line);
        let lh = (line * size).max(baseline + 1.1 * fonts().descent(face, size) - top);
        let base = baseline + (1.0 - q) * 1.08 * lh;
        c.push_clip([-1e6, top - 0.1 * size, 1e6, top + lh]);
        let wd = c.text_ls(x, base, size, face, s, col, ls, glow);
        c.pop_clip();
        x + wd + if s.is_empty() { 0.0 } else { ls * size }
    }

    /// The line an element rises out of: drawn left to right (0..0.45 of the reveal), fading at its end.
    pub fn reveal_line(&self, c: &mut Canvas, x0: f32, y: f32, x1: f32, p: f32) {
        let lp = ease_out(0.0, 0.45, p);
        let fade = 1.0 - smooth(0.72, 1.0, p);
        if p > 0.0 && fade > 0.0 {
            c.light(x0, y, x1, fade, 0.0, lp, 1.0, self.uipx());
        }
    }

    /// A word that lights under the pointer: it brightens, steps aside .14 em, a line is drawn under it left to right
    /// and fire runs along it (gone to the right when it is left). `p` how far it has risen (0..1, its reveal line with
    /// it), `alpha` how much of it is seen. `dim` another word of its group is lit. Returns its box.
    #[allow(clippy::too_many_arguments)]
    pub fn word(&mut self, c: &mut Canvas, wid: u64, text: &str, x: f32, y: f32, st: WordStyle, p: f32, live: bool, dim: bool) -> [f32; 4] {
        let size = st.size;
        let box_h = 1.12 * size;
        let text_w = tw(st.face, text, size, st.ls);
        let mut fx = self.words.get(&wid).copied().unwrap_or_default();
        let shift_px = fx.shift * 0.14 * size;
        let r = [x, y, x + text_w + shift_px, y + box_h];
        if live && st.enabled {
            self.hit(r, wid);
        }
        // a word of a screen going away keeps the look it had as it sinks (the pressed one lit, the rest as dim as
        // they were): no flash of the others coming back up first
        let keep = !live && fx.seen;
        let on = if keep { fx.on } else { self.lit == wid && live && st.enabled };
        if on && !fx.on {
            // lit: a line that had gone off to the right starts again from the left
            if fx.gone && fx.hl_from > 0.98 {
                fx.hl_from = 0.0;
                fx.hl_to = 0.0;
            }
            fx.gone = false;
        } else if !on && fx.on {
            fx.gone = true;
        }
        fx.on = on;
        let target = if !st.enabled { 0.34 } else if on { 1.0 } else if dim { DIMMER } else { st.rest };
        let dt = self.dt;
        fx.color = if !fx.seen {
            target
        } else if keep {
            fx.color
        } else {
            super::ui::toward(fx.color, target, 6.0, dt)
        };
        fx.seen = true;
        fx.shift = toward(fx.shift, if on { 1.0 } else { 0.0 }, 5.0, dt);
        fx.hl_to = toward(fx.hl_to, if fx.on || fx.gone { 1.0 } else { 0.0 }, 4.5, dt);
        fx.hl_from = if fx.gone { toward(fx.hl_from, 1.0, 4.5, dt) } else { 0.0 };
        self.words.insert(wid, fx);
        let q = clamp01(ease_out(0.22, 1.0, p) * 1.15);
        let line_top = y + 0.03 * size;
        let line_y = y + box_h - 0.06 * size;
        let ui = self.uipx();
        // the title of the screen just left flying down into this word (its way back)
        if let Some(bf) = self.back_fly.clone().filter(|f| f.text == text && self.now - f.since < 0.9) {
            let k = ease_io((self.now - bf.since) / 0.85);
            let sz = bf.size + (size - bf.size) * k;
            let tx = bf.at[0] + (x - bf.at[0]) * k;
            let top = bf.at[1] + (line_top - bf.at[1]) * k;
            let base = fonts().baseline(st.face, top, sz, 0.98);
            let col = 1.0 + (fx.color - 1.0) * k;
            c.text_ls(tx, base, sz, st.face, text, grey(col, 1.0), st.ls, 1.0 - k);
            self.rects.insert(wid, [x, line_y, x + text_w, line_y]);
            self.word_ids_now.insert(wid);
            return r;
        }
        // under the word: its reveal line, the line drawn under it while lit (no fire: everything is monochrome, 14.10)
        self.reveal_line(c, x, line_y, x + text_w, p);
        if fx.hl_to > 0.01 && fx.hl_from < 0.99 {
            c.light(x, line_y, x + text_w, 1.0, fx.hl_from, fx.hl_to, 1.0, ui);
        }
        self.rects.insert(wid, [x, line_y, x + text_w, line_y]);
        self.word_ids_now.insert(wid);
        let base_c = fx.color;
        c.push_clip([-1e6, line_top - 0.1 * size, 1e6, y + box_h]);
        let base = fonts().baseline(st.face, line_top, size, 0.98) + (1.0 - q) * 1.08 * 0.98 * size;
        c.text_ls(x + shift_px, base, size, st.face, text, grey(base_c, 1.0), st.ls, fx.shift * st.glow);
        c.pop_clip();
        r
    }

    /// Small spaced capitals (Martian Mono), risen `q` out of their line; returns the pen's end.
    pub fn caps(&self, c: &mut Canvas, s: &str, x: f32, top: f32, size: f32, ls: f32, bright: f32, q: f32) -> f32 {
        self.rising_text(c, Face::Caps, s, x, top, size, 1.3, ls, grey(bright, 1.0), 0.0, q)
    }

    /// The pointer moved onto `now` (0 none): the word it is on lights, the one before goes; a click is heard.
    pub fn light_word(&mut self, now: u64) {
        if now == self.lit {
            return;
        }
        for f in self.flames.iter_mut() {
            f.dying = true;
        }
        self.lit = now;
        if now != 0 && self.word_ids.contains(&now) {
            self.flames.push(Flame { word: now, p: 0.0, a: 1.0, dying: false });
            self.tick_sound(0.9);
        }
    }

    /// The flames run along their lines and the old ones die.
    pub fn tick_flames(&mut self) {
        let dt = self.dt;
        for f in self.flames.iter_mut() {
            if !f.dying {
                f.p = toward(f.p, 1.0, 3.6, dt);
            } else {
                f.a -= dt / 0.5;
            }
        }
        let n = self.flames.len();
        let mut i = 0;
        self.flames.retain(|f| {
            i += 1;
            f.a > 0.0 && n - (i - 1) <= 2
        });
    }

    /// The click under the pointer, as the main menu's (quiet; silent on the stand).
    pub fn tick_sound(&self, k: f32) {
        crate::launch::sound::play(crate::launch::sound::Sfx::Tick, 3.2 * k, 0.0);
    }
}
