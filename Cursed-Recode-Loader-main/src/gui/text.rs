//! The GUI's text: Geist and Geist Mono, and the menus' Sofia Sans Extra Condensed and Martian Mono, as distance fields
//! (tools/gui_atlas.py), laid out here into glyph quads.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

thread_local! {
    static SCALE: Cell<f32> = const { Cell::new(1.0) };
    /// a HUD element's own letters while it is drawn: its family (0 Geist, 1 Geist Mono, 2 the menus' condensed, 3
    /// Martian Mono), a step bolder or not, a shadow under them (its strength)
    static STYLE: Cell<(u8, bool, f32)> = const { Cell::new((0, false, 0.0)) };
}
/// Text is drawn and measured this much larger (the theme's text size), until set again.
pub fn set_scale(k: f32) {
    SCALE.with(|c| c.set(k));
}
pub fn scale() -> f32 {
    SCALE.with(|c| c.get())
}
/// The letters' family, weight and shadow until set again ((0, false, 0.0): as written).
pub fn set_style(family: u8, bolder: bool, shadow: f32) {
    STYLE.with(|c| c.set((family, bolder, shadow)));
}
pub fn shadow() -> f32 {
    STYLE.with(|c| c.get().2)
}
/// The letters' family and weight as set now (what a laid-out text depends on).
pub fn style() -> (u8, bool) {
    STYLE.with(|c| {
        let s = c.get();
        (s.0, s.1)
    })
}
/// The face and size really drawn for one asked (the family and weight set, the text size): the menus' own faces
/// (display, caps) stay as asked.
pub fn styled(face: Face, size: f32) -> (Face, f32) {
    let size = size * scale();
    let (family, bolder, _) = STYLE.with(|c| c.get());
    if family == 0 && !bolder || face.display() {
        return (face, size);
    }
    use Face::*;
    match family {
        1 => (Mono, size),
        // the condensed face is narrow and small for its size: a little larger
        2 => (match (face, bolder) {
            (Regular, false) | (Mono, false) => Book,
            (Medium, false) => Book,
            _ => Heavy,
        }, size * 1.28),
        3 => (Caps, size * 0.86),
        _ => (match face {
            Regular => Medium,
            Medium => Semi,
            Semi | Bold => Bold,
            f => f,
        }, size),
    }
}

/// Atlas px either side of the outline (tools/gui_atlas.py SPREAD).
pub const SPREAD: f32 = 12.0;
/// Atlas px to the em (tools/gui_atlas.py EM).
pub const EM: f32 = 48.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Regular = 0,
    Medium = 1,
    Semi = 2,
    Bold = 3,
    Mono = 4,
    /// The menus' words (the main menu's face): Sofia Sans Extra Condensed at 250, 300, 350, 450.
    Thin = 5,
    Light = 6,
    Book = 7,
    Heavy = 8,
    /// The menus' small spaced capitals: Martian Mono.
    Caps = 9,
}

impl Face {
    /// The menus' faces: no air of their own between letters (theirs is given, as the preview's CSS letter-spacing).
    pub fn display(self) -> bool {
        self as u8 >= 5
    }
}

/// `s` in face number `face` for the world's words (gui/mod.rs "glyphs"): "ascent descent width waiting;" then each
/// letter "left top width height ax ay aw ah page;" (em but the atlas's px; waiting 1: a letter is still being drawn
/// from the system's fonts); spaces only move the pen.
pub fn layout(face: u8, s: &str) -> String {
    let face = match face {
        1 => Face::Medium,
        2 => Face::Semi,
        3 => Face::Bold,
        4 => Face::Mono,
        _ => Face::Regular,
    };
    let f = fonts();
    let font = &f.faces[face as usize];
    let k = 1.0 / EM;
    let tr = tracking(face, EM) * k;
    let mut out = String::new();
    let mut pen = 0.0f32;
    let mut letters = String::new();
    let mut waiting = false;
    for c in s.chars() {
        let got = f.glyph(face, c);
        if !c.is_ascii() && super::dynfont::waiting(face, c) {
            waiting = true;
        }
        let Some(g) = got else { continue };
        if c != ' ' {
            letters.push_str(&format!("{} {} {} {} {} {} {} {} {};", pen + g.left * k, g.top * k, g.at[2] as f32 * k, g.at[3] as f32 * k, g.at[0], g.at[1], g.at[2], g.at[3], g.page));
        }
        pen += g.adv * k + tr;
    }
    out.push_str(&format!("{} {} {} {};", font.ascent * k, font.descent * k, (pen - tr).max(0.0), waiting as i32));
    out.push_str(&letters);
    out
}

#[derive(Clone, Copy)]
pub struct Glyph {
    pub adv: f32,
    pub left: f32,
    pub top: f32,
    pub at: [u16; 4],
    /// Which atlas: 0 the faces' own, 1 the letters drawn from the system's fonts (dynfont.rs).
    pub page: u8,
}

pub struct Font {
    pub ascent: f32,
    pub descent: f32,
    glyphs: HashMap<char, Glyph>,
    /// The ASCII ones at hand (most of what the menu writes), '?' where there is none.
    ascii: [Option<Glyph>; 128],
}

pub struct Fonts {
    pub faces: Vec<Font>,
}

pub fn fonts() -> &'static Fonts {
    static F: OnceLock<Fonts> = OnceLock::new();
    F.get_or_init(|| {
        let b = include_bytes!("../../assets/gui/text.bin");
        let mut at = 0usize;
        let u32_ = |at: &mut usize| {
            let v = u32::from_le_bytes(b[*at..*at + 4].try_into().unwrap());
            *at += 4;
            v
        };
        let f32_ = |at: &mut usize| {
            let v = f32::from_le_bytes(b[*at..*at + 4].try_into().unwrap());
            *at += 4;
            v
        };
        let n = u32_(&mut at);
        let mut faces = Vec::new();
        for _ in 0..n {
            let ascent = f32_(&mut at);
            let descent = f32_(&mut at);
            let count = u32_(&mut at);
            let mut glyphs = HashMap::new();
            for _ in 0..count {
                let c = char::from_u32(u32_(&mut at)).unwrap_or(' ');
                let adv = f32_(&mut at);
                let left = f32_(&mut at);
                let top = f32_(&mut at);
                let mut q = [0u16; 4];
                for v in q.iter_mut() {
                    *v = u16::from_le_bytes(b[at..at + 2].try_into().unwrap());
                    at += 2;
                }
                glyphs.insert(c, Glyph { adv, left, top, at: q, page: 0 });
            }
            let mut ascii = [None; 128];
            for (c, slot) in ascii.iter_mut().enumerate() {
                *slot = glyphs.get(&(c as u8 as char)).or_else(|| glyphs.get(&'?')).copied();
            }
            faces.push(Font { ascent, descent, glyphs, ascii });
        }
        Fonts { faces }
    })
}

impl Fonts {
    pub fn glyph(&self, face: Face, c: char) -> Option<Glyph> {
        let f = &self.faces[face as usize];
        if (c as u32) < 128 {
            return f.ascii[c as usize];
        }
        // a letter the face lacks: Geist's (it has the most), else the system's fonts (dynfont.rs), else '?'
        f.glyphs
            .get(&c)
            .or_else(|| if face as u8 != 0 { self.faces[0].glyphs.get(&c) } else { None })
            .copied()
            .or_else(|| if c.is_control() { None } else { super::dynfont::glyph(face, c) })
            .or_else(|| f.glyphs.get(&'?').copied())
    }

    fn adv(&self, face: Face, c: char) -> f32 {
        self.glyph(face, c).map_or(0.0, |g| g.adv)
    }

    /// The width of `s` at `size` px to the em.
    pub fn width(&self, face: Face, size: f32, s: &str) -> f32 {
        // (as drawn: what Streamer Mode hides put aside)
        let s = super::mask::masked(s);
        let (face, size) = styled(face, size);
        let k = size / EM;
        let (mut w, mut n) = (0.0, 0usize);
        for c in s.chars() {
            w += self.adv(face, c);
            n += 1;
        }
        w * k + tracking(face, size) * n.saturating_sub(1) as f32
    }

    /// `s` cut to `max` px with an ellipsis.
    pub fn fit(&self, face: Face, size: f32, s: &str, max: f32) -> String {
        if self.width(face, size, s) <= max {
            return s.to_string();
        }
        let (face, size) = styled(face, size);
        let (k, tr) = (size / EM, tracking(face, size));
        let ell = self.adv(face, '…') * k;
        // what is kept, its letters and the air after each; the ellipsis after the last
        let (mut w, mut end) = (0.0, 0);
        for (i, c) in s.char_indices() {
            let a = self.adv(face, c) * k;
            if w + a + tr + ell > max {
                break;
            }
            w += a + tr;
            end = i + c.len_utf8();
        }
        s[..end].trim_end().to_string() + "…"
    }

    /// Words of `s` into lines of at most `max` px.
    pub fn wrap(&self, face: Face, size: f32, s: &str, max: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        let (sf, ss) = styled(face, size);
        let gap = self.adv(sf, ' ') * ss / EM + 2.0 * tracking(sf, ss);
        let mut lw = 0.0;
        for word in s.split(' ') {
            let ww = self.width(face, size, word);
            if line.is_empty() {
                line = word.to_string();
                lw = ww;
            } else if lw + gap + ww > max {
                lines.push(std::mem::replace(&mut line, word.to_string()));
                lw = ww;
            } else {
                line.push(' ');
                line.push_str(word);
                lw += gap + ww;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }
}

/// A little air between letters at small sizes (Geist is tight); none for the menus' faces (theirs is given).
pub fn tracking(face: Face, size: f32) -> f32 {
    match face {
        Face::Mono => 0.0,
        f if f.display() => 0.0,
        _ => (0.012 * size).min(0.25) - if size > 22.0 { 0.02 * size } else { 0.0 },
    }
}

impl Fonts {
    /// The width of `s` at `size` px to the em with `ls` em of air after each letter but the last (CSS's
    /// letter-spacing, as the menus give it); the theme's text size does not apply (the menus size their own).
    pub fn width_ls(&self, face: Face, size: f32, s: &str, ls: f32) -> f32 {
        let k = size / EM;
        let (mut w, mut n) = (0.0, 0usize);
        for c in s.chars() {
            w += self.adv(face, c);
            n += 1;
        }
        w * k + ls * size * n.saturating_sub(1) as f32
    }

    pub fn ascent(&self, face: Face, size: f32) -> f32 {
        self.faces[face as usize].ascent * size / EM
    }

    pub fn descent(&self, face: Face, size: f32) -> f32 {
        self.faces[face as usize].descent * size / EM
    }

    /// The baseline of a line box from `top`, `line` (of the size) tall: the font's own box centred in it (CSS's
    /// half-leading), as the menus' preview laid its words.
    pub fn baseline(&self, face: Face, top: f32, size: f32, line: f32) -> f32 {
        let (a, d) = (self.ascent(face, size), self.descent(face, size));
        top + (size * line - (a + d)) / 2.0 + a
    }

    /// `s` cut to `max` px (letter spacing `ls` em) with an ellipsis.
    pub fn fit_ls(&self, face: Face, size: f32, s: &str, max: f32, ls: f32) -> String {
        if self.width_ls(face, size, s, ls) <= max {
            return s.to_string();
        }
        let k = size / EM;
        let ell = self.adv(face, '…') * k;
        let (mut w, mut end) = (0.0, 0);
        for (i, c) in s.char_indices() {
            let a = self.adv(face, c) * k + ls * size;
            if w + a + ell > max {
                break;
            }
            w += a;
            end = i + c.len_utf8();
        }
        s[..end].trim_end().to_string() + "…"
    }

    /// Words of `s` into lines of at most `max` px (letter spacing `ls` em); a line break in it starts a line.
    pub fn wrap_ls(&self, face: Face, size: f32, s: &str, max: f32, ls: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for para in s.split('\n') {
            let mut line = String::new();
            for word in para.split(' ') {
                let cand = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if line.is_empty() || self.width_ls(face, size, &cand, ls) <= max {
                    line = cand;
                } else {
                    lines.push(std::mem::replace(&mut line, word.to_string()));
                }
            }
            lines.push(line);
        }
        lines
    }
}
