//! A server's words in the HUD's own letters (Java's HudText): the sidebar's title and lines, the boss bars' names, the
//! title, the action bar. Its runs keep the server's colours (a dark one made lighter, to read on the dark glass) and
//! styles; a pack's glyphs are Java's, drawn in their place (icon 25: a slot's run).

use super::*;

const BOLD: u32 = 1;
const UNDER: u32 = 4;
const STRIKE: u32 = 8;
const OBF: u32 = 16;
const GLYPH: u32 = 32;
const NO_COLOUR: u32 = 256;

/// The icon Java draws a run of a pack's glyphs by.
const IC_TEXT_GLYPH: f32 = 25.0;

#[derive(Clone, Default)]
pub struct Run {
    pub text: String,
    /// 0xRRGGBB, or none given (the element's text colour)
    pub col: Option<u32>,
    pub fl: u32,
    /// a pack's glyphs: their width in the game's units (its line is 9)
    pub gw: f32,
}

/// One of the server's texts: its slot (Java's, for its glyphs) and runs.
#[derive(Clone, Default)]
pub struct RText {
    pub slot: u32,
    pub runs: Vec<Run>,
}

impl RText {
    /// From Java's runs: fields by U+0003 (words, RRGGBB, flags, the glyphs' width), runs by U+0002.
    pub fn parse(slot: u32, s: &str) -> RText {
        let runs = s
            .split('\u{2}')
            .filter(|r| !r.is_empty())
            .map(|r| {
                let mut f = r.split('\u{3}');
                let text = f.next().unwrap_or("").to_string();
                let col = u32::from_str_radix(f.next().unwrap_or("FFFFFF"), 16).unwrap_or(0xffffff);
                let fl: u32 = f.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                let gw: f32 = f.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                Run { text, col: (fl & NO_COLOUR == 0).then_some(col), fl, gw }
            })
            .collect();
        RText { slot, runs }
    }

    /// Plain words (the editor's example, a sample line).
    pub fn words(s: &str) -> RText {
        RText { slot: u32::MAX, runs: vec![Run { text: s.to_string(), ..Default::default() }] }
    }

    /// Its words only: a pack's glyphs left out (a server's menu draws its own picture by them in its title: the
    /// client's menu is ours, her word 15.10), the spaces round them trimmed.
    pub fn words_only(&self) -> RText {
        let mut runs: Vec<Run> = self.runs.iter().filter(|r| r.fl & GLYPH == 0).cloned().collect();
        if let Some(first) = runs.first_mut() {
            first.text = first.text.trim_start().to_string();
        }
        if let Some(last) = runs.last_mut() {
            last.text = last.text.trim_end().to_string();
        }
        runs.retain(|r| !r.text.is_empty());
        RText { slot: self.slot, runs }
    }

    /// Nothing to show (no words, no glyphs).
    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|r| r.fl & GLYPH == 0 && r.text.trim().is_empty())
    }

    pub fn plain(&self) -> String {
        self.runs.iter().filter(|r| r.fl & GLYPH == 0).map(|r| r.text.as_str()).collect()
    }

    /// Its width at `fs` in `face` (a glyph's unit is a tenth of the size, as the chat's).
    pub fn width(&self, fs: f32, face: Face) -> f32 {
        self.runs
            .iter()
            .map(|r| if r.fl & GLYPH != 0 { r.gw * fs * super::text::scale() / 10.0 } else { fonts().width(bolder(face, r.fl), fs, &r.text) })
            .sum()
    }
}

/// A run's face: a step bolder when the server made it bold.
fn bolder(face: Face, fl: u32) -> Face {
    if fl & BOLD == 0 {
        return face;
    }
    match face {
        Face::Regular | Face::Medium => Face::Semi,
        Face::Semi => Face::Bold,
        f => f,
    }
}

/// A server's colour readable on the dark glass: a dark one lifted (as the chat's), none given: `base`.
pub fn readable(c: Option<u32>, base: Rgba) -> Rgba {
    let Some(c) = c else { return base };
    let r = crate::gui::draw::hex(c & 0xffffff, 1.0);
    let l = 0.3 * r[0] + 0.59 * r[1] + 0.11 * r[2];
    if l < 0.3 {
        let k = (0.3 - l) / 0.3 * 0.75;
        [r[0] + (0.55 - r[0]) * k, r[1] + (0.55 - r[1]) * k, r[2] + (0.6 - r[2]) * k, base[3]]
    } else {
        [r[0], r[1], r[2], base[3]]
    }
}

impl Gui {
    /// `t` written from (x, base) at `fs`: its runs in their colours (`col` where none is given), a pack's glyphs Java's
    /// (when `icons`: in the game); its width.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn rtext(&mut self, c: &mut Canvas, t: &RText, x: f32, base: f32, fs: f32, face: Face, col: Rgba, icons: bool) -> f32 {
        let mut px = x;
        for (i, r) in t.runs.iter().enumerate() {
            if r.fl & GLYPH != 0 {
                let gs = fs * super::text::scale();
                let w = r.gw * gs / 10.0;
                if icons && t.slot != u32::MAX {
                    self.hud.icons.push([IC_TEXT_GLYPH, (t.slot * 256 + i.min(255) as u32) as f32, px, base - gs * 0.78, w, gs * 0.9, c.alpha]);
                }
                px += w;
                continue;
            }
            let colour = readable(r.col, col);
            let f = bolder(face, r.fl);
            let words: std::borrow::Cow<str> = if r.fl & OBF != 0 {
                // the server's "obfuscated": letters going round
                let n = (self.now * 20.0) as u32;
                std::borrow::Cow::Owned(r.text.chars().enumerate().map(|(i, ch)| if ch == ' ' { ' ' } else { ['#', '%', '&', '?', '@', '$', '§'][(i as u32 + n) as usize % 7] }).collect())
            } else {
                std::borrow::Cow::Borrowed(r.text.as_str())
            };
            let w = c.text(px, base, fs, f, &words, colour);
            if r.fl & UNDER != 0 {
                c.rect([px, base + fs * 0.14, px + w, base + fs * 0.14 + (fs * 0.07).max(1.0)], 0.5, fade(colour, 0.8));
            }
            if r.fl & STRIKE != 0 {
                c.rect([px, base - fs * 0.3, px + w, base - fs * 0.3 + (fs * 0.07).max(1.0)], 0.5, fade(colour, 0.85));
            }
            px += w;
        }
        px - x
    }
}
