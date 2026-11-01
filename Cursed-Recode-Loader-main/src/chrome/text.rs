//! The bar's text, set natively: the launch menu's faces and their distance fields (tools/text_atlas.py), the metrics
//! from assets/launch/text_metrics.bin (tools/text_metrics.py; the window before the game has no Java to lay it out).

use std::collections::HashMap;
use std::sync::OnceLock;

/// The faces as text_atlas.py packs them.
#[derive(Clone, Copy)]
pub enum Face {
    /// Sofia Sans Extra Condensed 350.
    Nick = 2,
    /// Martian Mono 400 (the menu's small capitals: «CURSED · ALPHA 0.1 · 26.3»).
    Mono = 4,
}

struct Glyph {
    adv: f32,
    left: f32,
    top: f32,
    at: [f32; 4],
}

struct FaceMetrics {
    ascent: f32,
    glyphs: HashMap<u32, Glyph>,
}

pub struct Metrics {
    pub em: f32,
    pub spread: f32,
    pub atlas: [u32; 2],
    faces: Vec<FaceMetrics>,
}

pub fn metrics() -> &'static Metrics {
    static M: OnceLock<Metrics> = OnceLock::new();
    M.get_or_init(|| parse(include_bytes!("../../assets/launch/text_metrics.bin")))
}

fn parse(b: &[u8]) -> Metrics {
    assert_eq!(&b[0..4], b"CTM1", "The text metrics are broken");
    let mut at = 4usize;
    let u32_ = |at: &mut usize| {
        let v = u32::from_le_bytes([b[*at], b[*at + 1], b[*at + 2], b[*at + 3]]);
        *at += 4;
        v
    };
    let em = u32_(&mut at) as f32;
    let spread = u32_(&mut at) as f32;
    let atlas = [u32_(&mut at), u32_(&mut at)];
    let faces_n = u32_(&mut at);
    let f32_ = |at: &mut usize| {
        let v = f32::from_le_bytes([b[*at], b[*at + 1], b[*at + 2], b[*at + 3]]);
        *at += 4;
        v
    };
    let u16_ = |at: &mut usize| {
        let v = u16::from_le_bytes([b[*at], b[*at + 1]]);
        *at += 2;
        v as f32
    };
    let mut faces = Vec::new();
    for _ in 0..faces_n {
        let len = b[at] as usize;
        at += 1 + len;
        let ascent = f32_(&mut at);
        let _descent = f32_(&mut at);
        let n = u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        at += 4;
        let mut glyphs = HashMap::new();
        for _ in 0..n {
            let cp = u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
            at += 4;
            let adv = f32_(&mut at);
            let left = f32_(&mut at);
            let top = f32_(&mut at);
            let at4 = [u16_(&mut at), u16_(&mut at), u16_(&mut at), u16_(&mut at)];
            glyphs.insert(cp, Glyph { adv, left, top, at: at4 });
        }
        faces.push(FaceMetrics { ascent, glyphs });
    }
    Metrics { em, spread, atlas, faces }
}

impl Metrics {
    fn glyph(&self, face: Face, c: char) -> Option<&Glyph> {
        let f = &self.faces[face as usize];
        f.glyphs.get(&(c as u32)).or_else(|| f.glyphs.get(&('?' as u32)))
    }

    /// The width `text` takes at `size` px to the em, `spacing` em after each letter (as CSS letter-spacing).
    pub fn width(&self, face: Face, text: &str, size: f32, spacing: f32) -> f32 {
        let k = size / self.em;
        text.chars().map(|c| self.glyph(face, c).map_or(0.0, |g| g.adv * k) + spacing * size).sum()
    }

    /// The face's height above its baseline at `size`.
    pub fn ascent(&self, face: Face, size: f32) -> f32 {
        self.faces[face as usize].ascent * size / self.em
    }

    /// `text` from its pen at `x` on `baseline` into `out`, 12 numbers a glyph (shaders/chrome/chrome.comp): its box,
    /// its field in the atlas, its colour and alpha. Returns the pen's end.
    #[allow(clippy::too_many_arguments)]
    pub fn emit(&self, out: &mut Vec<f32>, face: Face, text: &str, x: f32, baseline: f32, size: f32, spacing: f32, rgba: [f32; 4]) -> f32 {
        let k = size / self.em;
        let mut x = x;
        for c in text.chars() {
            let Some(g) = self.glyph(face, c) else { continue };
            if c != ' ' {
                let (x0, y0) = (x + g.left * k, baseline + g.top * k);
                out.extend_from_slice(&[x0, y0, x0 + g.at[2] * k, y0 + g.at[3] * k, g.at[0], g.at[1], g.at[0] + g.at[2], g.at[1] + g.at[3]]);
                out.extend_from_slice(&rgba);
            }
            x += g.adv * k + spacing * size;
        }
        x
    }
}
