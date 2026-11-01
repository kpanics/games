//! Our materials over a CS2 finish (shaders/cosmetics/viewmodel.frag's look): Recode's own chams' looks on the item —
//! the Ink, the Polymorph's liquid metal, the Cursed's dead flesh and its vessels, the Shadow's smoke — or the item in a
//! colour of its own (her word, 25.10: "оставить свой цвет, добавить стили из Chams, остальные убрать"); each as much as
//! asked, in a colour chosen with the client's own picker (its hue running round, Chroma, when asked).

use std::sync::OnceLock;
use std::time::Instant;

/// The looks, in the page's order: their names (English: i18n), viewmodel.frag's number for each (kept as it is in a
/// loadout: never renumbered), their own colours (sRGB).
pub const KINDS: [(&str, u8, [u8; 3]); 5] = [
    ("Ink", 7, [52, 78, 196]),
    ("Polymorph", 2, [236, 238, 242]),
    ("Cursed", 8, [204, 198, 190]),
    ("Shadow", 1, [214, 226, 255]),
    ("Own colour", 6, [200, 40, 60]),
];

/// The colours offered (sRGB): the page's own, beside the picker.
pub const COLOURS: [[u8; 3]; 12] = [
    [255, 255, 255],
    [236, 238, 242],
    [24, 24, 28],
    [214, 226, 255],
    [120, 200, 255],
    [40, 110, 255],
    [140, 70, 255],
    [255, 90, 200],
    [200, 40, 60],
    [255, 112, 32],
    [255, 200, 60],
    [60, 220, 120],
];

/// Our material on an item: which (viewmodel.frag's number, as KINDS), how much (0..1), its colour (sRGB), how fast its
/// hue runs round (0 it stands).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Overlay {
    pub kind: u8,
    pub strength: f32,
    pub colour: [u8; 3],
    pub chroma: f32,
}

/// A colour (sRGB) with its hue gone round as far as `chroma` has taken it by now (0: as it is) — the client's Chroma,
/// for our materials and CS's crosshair alike.
pub fn running(colour: [u8; 3], chroma: f32) -> [u8; 3] {
    if chroma <= 0.0 {
        return colour;
    }
    static T0: OnceLock<Instant> = OnceLock::new();
    let t = T0.get_or_init(Instant::now).elapsed().as_secs_f32();
    let [r, g, b] = colour.map(|v| v as f32 / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let (s, v) = (if mx > 0.0 { (mx - mn) / mx } else { 0.0 }, mx);
    // a grey runs round as a full colour would
    let s = if s < 0.05 { 0.85 } else { s };
    let h = (t * 0.12 * chroma).fract();
    let k = |n: f32| {
        let k = (n + h * 6.0) % 6.0;
        v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
    };
    [k(5.0), k(3.0), k(1.0)].map(|c| (c * 255.0).round() as u8)
}

/// A look's place in KINDS by its number.
pub fn index_of(kind: u8) -> Option<usize> {
    KINDS.iter().position(|k| k.1 == kind)
}

impl Overlay {
    /// A look as it first comes (its number): all of it, in its own colour.
    pub fn of(kind: u8) -> Overlay {
        let colour = index_of(kind).map_or([255; 3], |i| KINDS[i].2);
        Overlay { kind, strength: 1.0, colour, chroma: 0.0 }
    }

    /// As the loadout keeps it: "kind,strength,rrggbb[,chroma]".
    pub fn dump(&self) -> String {
        let mut s = format!("{},{:.2},{:02x}{:02x}{:02x}", self.kind, self.strength, self.colour[0], self.colour[1], self.colour[2]);
        if self.chroma > 0.0 {
            s.push_str(&format!(",{:.2}", self.chroma));
        }
        s
    }

    pub fn parse(s: &str) -> Option<Overlay> {
        let f: Vec<&str> = s.split(',').collect();
        let kind: u8 = f.first()?.parse().ok()?;
        // (the looks before 25.10: the ash became the Cursed's flesh; the glow and the iridescence are gone)
        let kind = if kind == 3 { 8 } else { kind };
        index_of(kind)?;
        let strength = f.get(1).and_then(|v| v.parse().ok()).unwrap_or(1.0f32).clamp(0.0, 1.0);
        let hex = f.get(2).and_then(|v| u32::from_str_radix(v, 16).ok()).unwrap_or(0xffffff);
        let chroma = f.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.0f32).clamp(0.0, 3.0);
        Some(Overlay { kind, strength, colour: [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8], chroma })
    }

    /// Its colour now (its hue gone round as far as Chroma has taken it), sRGB.
    pub fn colour_now(&self) -> [u8; 3] {
        running(self.colour, self.chroma)
    }

    /// As the shader takes it: its number, how much, its colour in linear light.
    pub fn shader(&self) -> (u32, f32, [f32; 3]) {
        let lin = |v: u8| {
            let c = v as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        (self.kind as u32, self.strength, self.colour_now().map(lin))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_overlay_keeps_through_the_loadout() {
        let o = Overlay { kind: 8, strength: 0.75, colour: [255, 112, 32], chroma: 0.0 };
        assert_eq!(Overlay::parse(&o.dump()), Some(o));
        let r = Overlay { chroma: 1.5, ..Overlay::of(7) };
        assert_eq!(Overlay::parse(&r.dump()), Some(r));
        assert_eq!(Overlay::parse("0,1,ffffff"), None);
        assert_eq!(Overlay::parse("9,1,ffffff"), None);
        // the looks before: the ash is the Cursed now, the glow and the iridescence are none
        assert_eq!(Overlay::parse("3,1,ff7020").map(|o| o.kind), Some(8));
        assert_eq!(Overlay::parse("4,1,ffffff"), None);
        assert_eq!(Overlay::parse("5,1,ffffff"), None);
        assert_eq!(Overlay::of(2).colour, [236, 238, 242]);
        let (k, s, c) = Overlay::of(1).shader();
        assert!(k == 1 && s == 1.0 && c[2] > 0.99);
    }
}
