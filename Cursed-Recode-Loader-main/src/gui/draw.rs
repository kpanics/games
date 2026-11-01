//! What the GUI draws in a frame: a list of shapes (shaders/gui/gui.comp draws them, in order, each pixel only the
//! ones over its tile). A shape is 24 numbers: its box, colour, parameters, kind, second colour, clip.

use super::text::{fonts, tracking, Face, EM};

pub const FLOATS: usize = 24;
pub const TILE: u32 = 16;

pub const RECT: f32 = 1.0;
pub const SHADOW: f32 = 2.0;
pub const LINE: f32 = 3.0;
pub const GLYPH: f32 = 4.0;
pub const IMAGE: f32 = 5.0;
pub const STATIC: f32 = 6.0;
pub const RING: f32 = 7.0;
pub const GLOW: f32 = 8.0;
pub const GLASS: f32 = 9.0;
pub const WHEEL: f32 = 10.0;
pub const FLAME: f32 = 11.0;
pub const LIGHT: f32 = 12.0;
pub const PIC: f32 = 13.0;
pub const ICON: f32 = 14.0;
pub const DISC: f32 = 15.0;
pub const VEIL: f32 = 16.0;

pub type Rgba = [f32; 4];

pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Rgba {
    [r, g, b, a]
}
pub const fn grey(v: f32, a: f32) -> Rgba {
    [v, v, v, a]
}
pub fn hex(c: u32, a: f32) -> Rgba {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0, a]
}
pub fn mix(a: Rgba, b: Rgba, k: f32) -> Rgba {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k)
}

pub struct Canvas {
    pub p: Vec<f32>,
    pub w: f32,
    pub h: f32,
    clip: [f32; 4],
    clips: Vec<[f32; 4]>,
    /// Everything drawn is this much seen (the window's rising and sinking).
    pub alpha: f32,
    /// Where glass was drawn (x0 y0 x1 y1): the world is blurred only there; and each glass's own (the HUD's plates
    /// are far apart: blurring their union would blur the whole screen).
    pub glass_area: Option<[f32; 4]>,
    pub glass_rects: Vec<[f32; 4]>,
    /// How grey the world is through its glass (0 as it is .. 1 grey): a screen's blocks over the greyed world.
    pub drain: f32,
}

impl Canvas {
    pub fn new(w: f32, h: f32) -> Canvas {
        Canvas { p: Vec::with_capacity(4096 * FLOATS), w, h, clip: [0.0, 0.0, w, h], clips: Vec::new(), alpha: 1.0, glass_area: None, glass_rects: Vec::new(), drain: 0.0 }
    }

    pub fn push_clip(&mut self, r: [f32; 4]) {
        self.clips.push(self.clip);
        self.clip = [r[0].max(self.clip[0]), r[1].max(self.clip[1]), r[2].min(self.clip[2]), r[3].min(self.clip[3])];
    }

    pub fn pop_clip(&mut self) {
        if let Some(c) = self.clips.pop() {
            self.clip = c;
        }
    }

    fn put(&mut self, r: [f32; 4], col: Rgba, par: [f32; 4], kind: f32, extra: [f32; 3], col2: Rgba) {
        let a = col[3] * self.alpha;
        // a shape fully transparent is left out (a gradient or checker that ends in a seen colour is not)
        if a <= 0.002 && kind != STATIC && col2[3] * self.alpha <= 0.002 {
            return;
        }
        let c = self.clip;
        if c[2] <= c[0] || c[3] <= c[1] {
            return;
        }
        self.p.extend_from_slice(&r);
        self.p.extend_from_slice(&[col[0], col[1], col[2], a]);
        self.p.extend_from_slice(&par);
        self.p.extend_from_slice(&[kind, extra[0], extra[1], extra[2]]);
        self.p.extend_from_slice(&[col2[0], col2[1], col2[2], col2[3] * self.alpha]);
        self.p.extend_from_slice(&c);
    }

    /// A rounded box.
    pub fn rect(&mut self, r: [f32; 4], radius: f32, col: Rgba) {
        self.put(r, col, [radius, 0.0, 0.0, 0.0], RECT, [0.0; 3], [0.0; 4]);
    }

    /// A rounded box's edge only, `width` wide (inside it).
    pub fn rect_line(&mut self, r: [f32; 4], radius: f32, width: f32, col: Rgba) {
        self.put(r, col, [radius, width, 4.0, 0.0], RECT, [0.0; 3], [0.0; 4]);
    }

    /// A rounded box lit: mode 1 left `col` to right `col2`, 2 a light from its right edge (`col2` added), 6 a light
    /// from the middle of its foot (`col2` added).
    pub fn rect_lit(&mut self, r: [f32; 4], radius: f32, col: Rgba, col2: Rgba, mode: f32) {
        self.put(r, col, [radius, 0.0, mode, 0.0], RECT, [0.0; 3], col2);
    }

    /// Glass: the world behind blurred (`blur` 0 sharp .. 1 the most), under its tint (`tint`'s alpha how much of it),
    /// a soft shadow round it (`shadow` 0..1), a sheen down from its top (`sheen` 0..1).
    pub fn glass(&mut self, r: [f32; 4], radius: f32, tint: Rgba, blur: f32, shadow: f32, sheen: f32) {
        let before = self.p.len();
        self.put(r, [tint[0], tint[1], tint[2], 1.0], [radius, blur, shadow, tint[3]], GLASS, [sheen, self.drain, 0.0], [0.0; 4]);
        if self.p.len() > before {
            let m = 24.0 + 40.0 * shadow;
            let a = [r[0] - m, r[1] - m, r[2] + m, r[3] + m];
            self.glass_rects.push(a);
            self.glass_area = Some(match self.glass_area {
                Some(g) => [g[0].min(a[0]), g[1].min(a[1]), g[2].max(a[2]), g[3].max(a[3])],
                None => a,
            });
        }
    }

    /// The glasses' places, those that touch merged (fewer, larger areas to blur).
    pub fn glass_merged(&self) -> Vec<[f32; 4]> {
        let mut v: Vec<[f32; 4]> = self.glass_rects.iter().map(|r| [r[0].max(0.0), r[1].max(0.0), r[2].min(self.w), r[3].min(self.h)]).filter(|r| r[2] > r[0] && r[3] > r[1]).collect();
        loop {
            let mut merged = false;
            'outer: for i in 0..v.len() {
                for j in i + 1..v.len() {
                    let (a, b) = (v[i], v[j]);
                    if a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3] {
                        v[i] = [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])];
                        v.swap_remove(j);
                        merged = true;
                        break 'outer;
                    }
                }
            }
            if !merged {
                return v;
            }
        }
    }

    /// A colour wheel: hue round it (red to the right, going round as on a clock's other way), saturation out from the
    /// middle, at `value` brightness.
    pub fn wheel(&mut self, c: [f32; 2], radius: f32, value: f32) {
        self.put([c[0] - radius, c[1] - radius, c[0] + radius, c[1] + radius], grey(1.0, 1.0), [radius, value, 0.0, 0.0], WHEEL, [0.0; 3], [0.0; 4]);
    }

    /// A box from `col` on the left to `col2` on the right over a checkerboard (how transparent a colour is shows).
    pub fn rect_checker(&mut self, r: [f32; 4], radius: f32, col: Rgba, col2: Rgba) {
        self.put(r, col, [radius, 0.0, 3.0, 0.0], RECT, [0.0; 3], col2);
    }

    /// A soft shadow of a rounded box, `blur` px out.
    pub fn shadow(&mut self, r: [f32; 4], radius: f32, blur: f32, col: Rgba) {
        self.put(r, col, [radius, blur, 0.0, 0.0], SHADOW, [0.0; 3], [0.0; 4]);
    }

    /// A segment `width` px wide with round ends; `glow` px of soft light round it.
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, col: Rgba, glow: f32) {
        self.put([a[0], a[1], b[0], b[1]], col, [width, glow, 0.0, 0.0], LINE, [0.0; 3], [0.0; 4]);
    }

    /// A line through `pts` (its segments one after another in one colour: the shader lays them as one stroke, its
    /// joints not doubled).
    pub fn polyline(&mut self, pts: &[[f32; 2]], width: f32, col: Rgba) {
        for w in pts.windows(2) {
            self.line(w[0], w[1], width, col, 0.0);
        }
    }

    /// An arrow from `a` to its point `b`, its head `head` px back along it (one stroke: its point not doubled).
    pub fn arrow(&mut self, a: [f32; 2], b: [f32; 2], width: f32, head: f32, col: Rgba) {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (ux, uy) = (dx / l, dy / l);
        let back = [b[0] - ux * head, b[1] - uy * head];
        let side = [-uy * head * 0.8, ux * head * 0.8];
        self.line(a, b, width, col, 0.0);
        self.line([back[0] + side[0], back[1] + side[1]], b, width, col, 0.0);
        self.line([back[0] - side[0], back[1] - side[1]], b, width, col, 0.0);
    }

    /// A circle's outline.
    pub fn ring(&mut self, c: [f32; 2], radius: f32, width: f32, col: Rgba) {
        self.put([c[0] - radius, c[1] - radius, c[0] + radius, c[1] + radius], col, [radius, width, 0.0, 0.0], RING, [0.0; 3], [0.0; 4]);
    }

    /// An arc of a circle's outline: `part` of it (0..1) from the top, clockwise.
    pub fn arc(&mut self, c: [f32; 2], radius: f32, width: f32, part: f32, col: Rgba) {
        if part <= 0.001 {
            return;
        }
        self.put([c[0] - radius, c[1] - radius, c[0] + radius, c[1] + radius], col, [radius, width, part.min(1.0), 0.0], RING, [0.0; 3], [0.0; 4]);
    }

    /// The clip shapes drawn now are cut to.
    pub fn clip_now(&self) -> [f32; 4] {
        self.clip
    }

    /// Moves by `d` the clips of what was drawn from shape `from` on under a clip of its own (not `outer`, the one the
    /// place was drawn under): an element's own cut (a list's) goes with it where it belongs.
    pub fn shift_own_clips(&mut self, from: usize, outer: [f32; 4], d: [f32; 2]) {
        let n = self.p.len() / FLOATS;
        for i in from..n {
            let q = &mut self.p[i * FLOATS..(i + 1) * FLOATS];
            if q[20..24] != outer[..] {
                q[20] += d[0];
                q[22] += d[0];
                q[21] += d[1];
                q[23] += d[1];
            }
        }
    }

    /// Moves what was drawn from shape `from` on by `d` (its box; its clip stays, the clip it was drawn under being
    /// the place's): the HUD draws an element at a place of its own, then puts it where it belongs.
    pub fn shift(&mut self, from: usize, d: [f32; 2]) {
        let n = self.p.len() / FLOATS;
        for i in from..n {
            let q = &mut self.p[i * FLOATS..(i + 1) * FLOATS];
            for k in [0, 2] {
                q[k] += d[0];
            }
            for k in [1, 3] {
                q[k] += d[1];
            }
            if q[12] == FLAME || q[12] == LIGHT {
                q[8] += d[0];
                q[9] += d[0];
                q[13] += d[1];
            }
        }
    }

    /// What was drawn from shape `from` on turned left to right about x = `sum` / 2 (its icons too; words would read
    /// backwards: they are drawn after).
    pub fn mirror_x(&mut self, from: usize, sum: f32) {
        let n = self.p.len() / FLOATS;
        for i in from..n {
            let q = &mut self.p[i * FLOATS..(i + 1) * FLOATS];
            let (a, b) = (q[0], q[2]);
            q[0] = sum - b;
            q[2] = sum - a;
            if q[12] == ICON {
                q[8] = if q[8] > 99.5 { q[8] - 100.0 } else { q[8] + 100.0 };
            }
        }
    }

    /// What was drawn from shape `from` on, `k` as seen (both its colours): an element fading in or out.
    pub fn fade_from(&mut self, from: usize, k: f32) {
        let n = self.p.len() / FLOATS;
        for i in from..n {
            let q = &mut self.p[i * FLOATS..(i + 1) * FLOATS];
            q[7] *= k;
            q[19] *= k;
        }
    }

    /// What was drawn from shape `from` on, scaled by `k` round `c` and moved by `d`: boxes, radii, widths, blurs (a
    /// clip narrower than the canvas with them).
    pub fn scale_about(&mut self, from: usize, c: [f32; 2], k: f32, d: [f32; 2]) {
        let n = self.p.len() / FLOATS;
        let (w, h) = (self.w, self.h);
        let sx = |v: f32| c[0] + (v - c[0]) * k + d[0];
        let sy = |v: f32| c[1] + (v - c[1]) * k + d[1];
        for i in from..n {
            let q = &mut self.p[i * FLOATS..(i + 1) * FLOATS];
            q[0] = sx(q[0]);
            q[2] = sx(q[2]);
            q[1] = sy(q[1]);
            q[3] = sy(q[3]);
            let kind = q[12];
            if kind == RECT || kind == GLOW || kind == GLASS || kind == WHEEL || kind == STATIC {
                q[8] *= k;
            } else if kind == SHADOW || kind == LINE || kind == RING {
                q[8] *= k;
                q[9] *= k;
            } else if kind == FLAME {
                q[8] = sx(q[8]);
                q[9] = sx(q[9]);
                q[11] *= k;
                q[13] = sy(q[13]);
            } else if kind == LIGHT {
                q[8] = sx(q[8]);
                q[9] = sx(q[9]);
                q[13] = sy(q[13]);
                q[15] *= k;
            }
            if q[20] > 0.0 || q[21] > 0.0 || q[22] < w || q[23] < h {
                q[20] = sx(q[20]);
                q[22] = sx(q[22]);
                q[21] = sy(q[21]);
                q[23] = sy(q[23]);
            }
        }
    }

    /// Soft light round a point.
    pub fn glow(&mut self, c: [f32; 2], radius: f32, col: Rgba) {
        self.put([c[0] - radius, c[1] - radius, c[0] + radius, c[1] + radius], col, [radius, 0.0, 0.0, 0.0], GLOW, [0.0; 3], [0.0; 4]);
    }

    /// Her logo (white, its heat as the alpha) in a box.
    pub fn logo(&mut self, r: [f32; 4], col: Rgba) {
        self.put(r, col, [0.0, 0.0, 1.0, 1.0], IMAGE, [0.0; 3], [0.0; 4]);
    }

    /// Interference: the preview's place until previews are made (grain, rolling bands, tearing, a line of light).
    pub fn interference(&mut self, r: [f32; 4], radius: f32, time: f32, seed: f32, strength: f32) {
        self.put(r, grey(1.0, 1.0), [radius, time, seed, strength], STATIC, [0.0; 3], [0.0; 4]);
    }

    /// Text from its baseline at `x`; returns its width.
    pub fn text(&mut self, x: f32, base: f32, size: f32, face: Face, s: &str, col: Rgba) -> f32 {
        // (what Streamer Mode hides, put aside)
        let s = super::mask::masked(s);
        let s = s.as_ref();
        let f = fonts();
        let (face, size) = super::text::styled(face, size);
        let k = size / EM;
        let tr = tracking(face, size);
        // a shadow under the letters (a HUD element's): soft, a little down and right, all of it before the letters
        let sh = super::text::shadow();
        if sh > 0.001 && col[3] > 0.01 {
            let d = (size * 0.07).max(1.0);
            let sc = [0.0, 0.0, 0.0, col[3] * sh];
            let mut pen = x;
            for ch in s.chars() {
                let Some(g) = f.glyph(face, ch) else { continue };
                if ch != ' ' {
                    let x0 = pen + g.left * k + d;
                    let y0 = base + g.top * k + d;
                    let [ax, ay, aw, ah] = g.at.map(|v| v as f32);
                    self.put([x0, y0, x0 + aw * k, y0 + ah * k], sc, [ax, ay, ax + aw, ay + ah], GLYPH, [0.55, 0.0, g.page as f32], [0.0; 4]);
                }
                pen += g.adv * k + tr;
            }
        }
        let mut pen = x;
        for ch in s.chars() {
            let Some(g) = f.glyph(face, ch) else { continue };
            if ch != ' ' {
                let x0 = pen + g.left * k;
                let y0 = base + g.top * k;
                let [ax, ay, aw, ah] = g.at.map(|v| v as f32);
                self.put([x0, y0, x0 + aw * k, y0 + ah * k], col, [ax, ay, ax + aw, ay + ah], GLYPH, [0.0, 0.0, g.page as f32], [0.0; 4]);
            }
            pen += g.adv * k + tr;
        }
        pen - x - if s.is_empty() { 0.0 } else { tr }
    }

    /// The menus' text: from its baseline at `x`, `ls` em of air after each letter (CSS's letter-spacing), a soft
    /// glow round the letters (`glow` 0..1, as the main menu's words); the theme's text size does not apply. Returns
    /// where the pen ended (the width, its last air not counted).
    pub fn text_ls(&mut self, x: f32, base: f32, size: f32, face: Face, s: &str, col: Rgba, ls: f32, glow: f32) -> f32 {
        let s = super::mask::masked(s);
        let s = s.as_ref();
        let f = fonts();
        let k = size / EM;
        let mut pen = x;
        let gap = ls * size;
        let exact = if face.display() { 1.0 } else { 0.0 };
        for ch in s.chars() {
            let Some(g) = f.glyph(face, ch) else { continue };
            if ch != ' ' && ch != '\u{a0}' {
                let x0 = pen + g.left * k;
                let y0 = base + g.top * k;
                let [ax, ay, aw, ah] = g.at.map(|v| v as f32);
                self.put([x0, y0, x0 + aw * k, y0 + ah * k], col, [ax, ay, ax + aw, ay + ah], GLYPH, [glow, exact, g.page as f32], [0.0; 4]);
            }
            pen += g.adv * k + gap;
        }
        pen - x - if s.is_empty() { 0.0 } else { gap }
    }

    /// Fire burning along a line under a word (the main menu's): from `x0` to `x1` at `y` (picture px, y down), drawn
    /// left to right to `progress`, `alpha` seen; `ui` picture px per ui px (its tongues are sized in ui px).
    pub fn flame(&mut self, x0: f32, y: f32, x1: f32, progress: f32, alpha: f32, ui: f32) {
        if alpha <= 0.002 {
            return;
        }
        let w = (x1 - x0).max(1.0);
        self.put([x0 - 0.06 * w, y - 112.0 * ui, x1 + 0.06 * w, y + 5.0 * ui], grey(1.0, alpha), [x0, x1, progress, ui], FLAME, [y, 0.0, 0.0], [0.0; 4]);
    }

    /// A line of light (the menus' hairline under what rises): `x0`..`x1` at `y`, seen from `from` to `to` of its length,
    /// white fading over its last third, a soft glow round it (`glow` 0..1); `ui` picture px per ui px.
    pub fn light(&mut self, x0: f32, y: f32, x1: f32, strength: f32, from: f32, to: f32, glow: f32, ui: f32) {
        if strength <= 0.002 || to <= from {
            return;
        }
        let w = (x1 - x0).max(1.0);
        self.put([x0 - 0.1 * w, y - 14.0 * ui, x1 + 0.1 * w, y + 14.0 * ui], grey(1.0, strength), [x0, x1, from, to], LIGHT, [y, glow, ui], [0.0; 4]);
    }

    /// A picture from the menus' picture atlas (render.rs Pics): `uv` its place there (atlas px), `pixel` drawn without
    /// smoothing (the game's own pixel art), `sat` its colour (0 grey .. 1 as it is), `bright`; `radius` its corners.
    pub fn pic(&mut self, r: [f32; 4], uv: [f32; 4], alpha: f32, pixel: bool, sat: f32, bright: f32, radius: f32) {
        self.put(r, [bright, bright, bright, alpha], uv, PIC, [if pixel { 1.0 } else { 0.0 }, sat, radius], [0.0; 4]);
    }

    /// A small icon of the menus (shaders/gui/gui.comp icon()): 0 back to start, 1 pause, 2 play, 3 next, 4 plus,
    /// 5 cross, 6 check, 7 left, 8 right, 9 again (a round arrow), 10 up, 11 down, 12 a lock, 13 a search glass.
    pub fn icon(&mut self, r: [f32; 4], kind: u32, col: Rgba) {
        self.put(r, col, [kind as f32, 0.0, 0.0, 0.0], ICON, [0.0; 3], [0.0; 4]);
    }

    /// An icon with a clock of its own (`t` seconds, -1 none): 45 the binds' keyboard, its keys lighting in turn; the
    /// tutorial's (gui.comp tutIcon): 46 W over A S D pressed in turn, 47 a mouse looking round, 48 its right button.
    pub fn icon_t(&mut self, r: [f32; 4], kind: u32, t: f32, col: Rgba) {
        self.put(r, col, [kind as f32, t, 0.0, 0.0], ICON, [0.0; 3], [0.0; 4]);
    }

    /// The music's record (the main menu's widget): black vinyl, its grooves and sheen, a pale label with a mark that
    /// turns (`turn` rad).
    pub fn disc(&mut self, r: [f32; 4], alpha: f32, turn: f32) {
        self.put(r, grey(1.0, alpha), [turn, 0.0, 0.0, 0.0], DISC, [0.0; 3], [0.0; 4]);
    }

    /// A shade over a box, its edges soft (`soft` px), deeper toward `side` (0 even, 1 the left, 2 the right, 3 the
    /// bottom): what keeps words readable over a bright world.
    pub fn veil(&mut self, r: [f32; 4], col: Rgba, soft: f32, side: f32) {
        self.put(r, col, [soft, side, 0.0, 0.0], VEIL, [0.0; 3], [0.0; 4]);
    }

    /// Text centred on `cx`.
    pub fn text_c(&mut self, cx: f32, base: f32, size: f32, face: Face, s: &str, col: Rgba) -> f32 {
        let w = fonts().width(face, size, s);
        self.text(cx - w / 2.0, base, size, face, s, col)
    }

    /// Text ending at `rx`.
    pub fn text_r(&mut self, rx: f32, base: f32, size: f32, face: Face, s: &str, col: Rgba) -> f32 {
        let w = fonts().width(face, size, s);
        self.text(rx - w, base, size, face, s, col)
    }

    /// The shapes by tile: for each tile of `TILE` px its run in the index list. Returns (runs: start, count a tile,
    /// indices), the tiles across.
    pub fn bins(&self) -> (Vec<u32>, Vec<u32>, u32) {
        let tx = (self.w as u32).div_ceil(TILE).max(1);
        let ty = (self.h as u32).div_ceil(TILE).max(1);
        let n = self.p.len() / FLOATS;
        let span = |i: usize| -> Option<[u32; 4]> {
            let q = &self.p[i * FLOATS..(i + 1) * FLOATS];
            let kind = q[12];
            let (mut x0, mut y0, mut x1, mut y1) = (q[0], q[1], q[2], q[3]);
            let m = if kind == LINE {
                let (a, b) = ((x0.min(x1), y0.min(y1)), (x0.max(x1), y0.max(y1)));
                x0 = a.0;
                y0 = a.1;
                x1 = b.0;
                y1 = b.1;
                q[8] * 0.5 + q[9] * 2.0 + 1.0
            } else if kind == SHADOW {
                q[9] * 1.5 + 1.0
            } else if kind == GLASS {
                24.0 + 40.0 * q[10]
            } else {
                1.0
            };
            let c = &q[20..24];
            let (x0, y0, x1, y1) = ((x0 - m).max(c[0]).max(0.0), (y0 - m).max(c[1]).max(0.0), (x1 + m).min(c[2]).min(self.w), (y1 + m).min(c[3]).min(self.h));
            if x1 <= x0 || y1 <= y0 {
                return None;
            }
            Some([(x0 as u32) / TILE, (y0 as u32) / TILE, ((x1 as u32) / TILE).min(tx - 1), ((y1 as u32) / TILE).min(ty - 1)])
        };
        let spans: Vec<Option<[u32; 4]>> = (0..n).map(span).collect();
        let mut count = vec![0u32; (tx * ty) as usize];
        for s in spans.iter().flatten() {
            for y in s[1]..=s[3] {
                for x in s[0]..=s[2] {
                    count[(y * tx + x) as usize] += 1;
                }
            }
        }
        let mut runs = vec![0u32; (tx * ty * 2) as usize];
        let mut at = 0u32;
        for (i, c) in count.iter().enumerate() {
            runs[i * 2] = at;
            at += c;
        }
        let mut fill = vec![0u32; (tx * ty) as usize];
        let mut idx = vec![0u32; at as usize];
        for (i, s) in spans.iter().enumerate() {
            let Some(s) = s else { continue };
            for y in s[1]..=s[3] {
                for x in s[0]..=s[2] {
                    let t = (y * tx + x) as usize;
                    idx[(runs[t * 2] + fill[t]) as usize] = i as u32;
                    fill[t] += 1;
                }
            }
        }
        for (i, f) in fill.iter().enumerate() {
            runs[i * 2 + 1] = *f;
        }
        (runs, idx, tx)
    }
}
