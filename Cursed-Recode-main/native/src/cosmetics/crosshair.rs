//! CS's crosshair (shown while a CS item is in the hands, her word 24.10: its settings and a CS2 code pasted in): one
//! description of it — its style, colours, outline, gap, length, thickness, dot, T, the dynamic one's splits — and the
//! share codes CS makes of it read into it: the old CS:GO one ("CSGO-xxxxx-…", version 1), the two of September 2026
//! (versions 3 and 4, same container) and CS2's since 30.09.2026 ("CS" and 44 characters). The codes are a big number
//! in base 57 (least significant character first), a checksum byte first (the sum of the rest), then the version.
//! The layouts are akiver/csgo-sharecode's (MIT), tested against its samples. Sizes are pixels on a picture
//! `screen_height` high (the old code's units were a 480-high picture's).

/// The dictionary of the codes (no 0, 1, I, l, g).
const DICT: &[u8] = b"ABCDEFGHJKLMNOPQRSTUVWXYZabcdefhijkmnopqrstuvwxyz23456789";

/// CS2's styles (the old code's mapped onto them).
pub const STYLES: [&str; 10] = [
    "Dynamic Cross",
    "Dynamic Circle",
    "Dynamic Cross (Legacy)",
    "Static Circle",
    "Static Cross",
    "Static Cross (Shot Feedback)",
    "Dot Only",
    "Dynamic Quad",
    "Static Square",
    "Static Quad",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crosshair {
    pub style: u8,
    pub follow_recoil: bool,
    pub dot: bool,
    pub t_style: bool,
    /// 0 none, 1 all round, 2 half.
    pub outline_mode: u8,
    pub color: [u8; 4],
    pub outline: [u8; 4],
    /// The old code's outline thickness (pixels at its 480); the new ones' is a pixel.
    pub outline_thickness: f32,
    pub gap: f32,
    pub length: f32,
    pub thickness: f32,
    pub spread_limit: u8,
    pub split_distance: u8,
    pub inner_split_alpha: f32,
    pub outer_split_alpha: f32,
    pub split_ratio: f32,
    pub screen_height: u16,
    /// Read from the old CS:GO code: its sizes a 480-high picture's, its gap 4 + gap pixels, unscaled.
    pub legacy: bool,
}

impl Default for Crosshair {
    /// CS2's own (a static green cross, a dark outline).
    fn default() -> Crosshair {
        Crosshair {
            style: 4,
            follow_recoil: false,
            dot: false,
            t_style: false,
            outline_mode: 1,
            color: [50, 250, 50, 200],
            outline: [0, 0, 0, 255],
            outline_thickness: 1.0,
            gap: 2.0,
            length: 7.0,
            thickness: 2.0,
            spread_limit: 181,
            split_distance: 3,
            inner_split_alpha: 1.0,
            outer_split_alpha: 0.5,
            split_ratio: 0.3,
            screen_height: 1080,
            legacy: false,
        }
    }
}

/// The characters (no prefix, no dashes) as the code's bytes (`n` of them, big-endian).
fn chars_to_bytes(chars: &str, n: usize) -> Option<Vec<u8>> {
    // the big number, as base-256 digits (little-endian while it is built)
    let mut num: Vec<u32> = vec![0];
    for c in chars.bytes().rev() {
        let d = DICT.iter().position(|&x| x == c)? as u32;
        let mut carry = d;
        for v in num.iter_mut() {
            let x = *v * 57 + carry;
            *v = x & 0xff;
            carry = x >> 8;
        }
        while carry > 0 {
            num.push(carry & 0xff);
            carry >>= 8;
        }
    }
    if num.len() > n {
        if num[n..].iter().any(|&v| v != 0) {
            return None;
        }
        num.truncate(n);
    }
    num.resize(n, 0);
    Some(num.iter().rev().map(|&v| v as u8).collect())
}

/// The bytes as a code's characters (`count` of them).
fn bytes_to_chars(bytes: &[u8], count: usize) -> String {
    let mut num: Vec<u32> = bytes.iter().map(|&b| b as u32).collect();
    let mut out = String::with_capacity(count);
    for _ in 0..count {
        let mut rem = 0u32;
        for v in num.iter_mut() {
            let x = (rem << 8) | *v;
            *v = x / 57;
            rem = x % 57;
        }
        out.push(DICT[rem as usize] as char);
    }
    out
}

fn checksum_ok(b: &[u8]) -> bool {
    b[0] == (b[1..].iter().map(|&v| v as u32).sum::<u32>() % 256) as u8
}

fn i8v(b: u8) -> f32 {
    b as i8 as f32
}

/// A code as CS writes it, read; None when it is not one (or a version no CS reads).
pub fn decode(code: &str) -> Option<Crosshair> {
    let code = code.trim();
    if let Some(rest) = code.strip_prefix("CSGO") {
        let chars: String = rest.chars().filter(|&c| c != '-').collect();
        if chars.len() != 25 {
            return None;
        }
        let b = chars_to_bytes(&chars, 18)?;
        if !checksum_ok(&b) {
            return None;
        }
        return match b[1] {
            1 => Some(legacy_v1(&b)),
            3 | 4 => Some(pixel(&b, b[1])),
            _ => None,
        };
    }
    let chars = code.strip_prefix("CS")?;
    if chars.len() != 44 {
        return None;
    }
    let b = chars_to_bytes(chars, 32)?;
    if !checksum_ok(&b) || b[1] != 1 {
        return None;
    }
    let screen = b[2] as u16 | (b[3] as u16) << 8;
    if screen == 0 {
        return None;
    }
    let bits = b[18] as u32 | (b[19] as u32) << 8 | (b[20] as u32) << 16 | (b[21] as u32) << 24;
    Some(Crosshair {
        style: (b[4] & 0x1f).min(9),
        follow_recoil: b[4] & 0x20 != 0,
        dot: b[4] & 0x40 != 0,
        t_style: b[4] & 0x80 != 0,
        outline_mode: (b[13] >> 6).min(2),
        color: [b[5], b[6], b[7], b[8]],
        outline: [b[9], b[10], b[11], b[12]],
        outline_thickness: 1.0,
        gap: (i16::from_le_bytes([b[14], b[15]]) as f32).clamp(-3840.0, 3840.0),
        length: b[16] as f32,
        thickness: (b[13] & 0x3f).min(32) as f32,
        spread_limit: b[17],
        split_distance: (bits & 0x7f) as u8,
        inner_split_alpha: ((bits >> 7) & 0x7f).min(100) as f32 / 100.0,
        outer_split_alpha: (((bits >> 14) & 0x7f).min(70) + 30) as f32 / 100.0,
        split_ratio: ((bits >> 21) & 0x7f).min(100) as f32 / 100.0,
        screen_height: screen.max(240),
        legacy: false,
    })
}

/// The old CS:GO code (sizes in a 480-high picture's units; its styles onto CS2's).
fn legacy_v1(b: &[u8]) -> Crosshair {
    let outline_on = b[10] & 8 == 8;
    let alpha_on = (b[13] >> 4) & 4 == 4;
    let style = match (b[13] & 0xf) >> 1 {
        0 | 3 => 0,
        5 => 2,
        _ => 4,
    };
    Crosshair {
        style,
        follow_recoil: (b[8] >> 4) & 8 == 8,
        dot: (b[13] >> 4) & 1 == 1,
        t_style: (b[13] >> 4) & 8 == 8,
        outline_mode: outline_on as u8,
        color: [b[4], b[5], b[6], if alpha_on { b[7] } else { 255 }],
        outline: [0, 0, 0, 255],
        outline_thickness: b[3] as f32 / 2.0,
        gap: i8v(b[2]) / 10.0,
        length: b[14] as f32 / 10.0,
        thickness: b[12] as f32 / 10.0,
        spread_limit: 181,
        split_distance: b[8] & 7,
        inner_split_alpha: (b[10] >> 4) as f32 / 10.0,
        outer_split_alpha: (b[11] & 0xf) as f32 / 10.0,
        split_ratio: (b[11] >> 4) as f32 / 10.0,
        screen_height: 480,
        legacy: true,
    }
}

/// The codes of September 2026 (versions 3 and 4: pixels; the outline a switch, or by 4 a mode).
fn pixel(b: &[u8], version: u8) -> Crosshair {
    let bits = b[10] as u32 | (b[11] as u32) << 8 | (b[12] as u32) << 16 | (b[13] as u32) << 24;
    let outline_mode = if version == 4 { (b[13] >> 4) & 3 } else { (b[2] & 0x20 == 0x20) as u8 };
    Crosshair {
        style: (b[2] & 0xf).min(9),
        follow_recoil: b[2] & 0x10 != 0,
        dot: b[2] & 0x40 != 0,
        t_style: b[2] & 0x80 != 0,
        outline_mode: outline_mode.min(2),
        color: [b[3], b[4], b[5], b[6]],
        outline: [0, 0, 0, 255],
        outline_thickness: 1.0,
        gap: b[7] as f32,
        length: b[8] as f32,
        thickness: ((bits >> 23) & 0x1f) as f32,
        spread_limit: b[9],
        split_distance: (bits & 0x7f) as u8,
        inner_split_alpha: ((bits >> 7) & 0x1f) as f32 / 20.0,
        outer_split_alpha: (((bits >> 12) & 0xf) + 6) as f32 / 20.0,
        split_ratio: ((bits >> 16) & 0x7f) as f32 / 100.0,
        screen_height: (b[14] as u16 | (b[15] as u16) << 8).max(240),
        legacy: false,
    }
}

/// A rectangle to fill (picture px: x0 y0 x1 y1) and its colour.
pub type Bar = ([f32; 4], [u8; 4]);

/// Whether a style draws a cross (the circles, squares and quads are drawn as one too, for now), a dot alone, or opens
/// with movement.
fn dot_only(style: u8) -> bool {
    style == 6
}

pub fn dynamic(style: u8) -> bool {
    matches!(style, 0 | 1 | 2 | 7)
}

impl Crosshair {
    /// In pixels at `screen_height` (the old code's sizes made so: its gap 4 + gap, as a 1080 picture would have it).
    pub fn to_pixels(&self) -> Crosshair {
        if !self.legacy {
            return *self;
        }
        let k = 1080.0 / 480.0;
        Crosshair {
            length: (self.length * k).round(),
            thickness: (self.thickness * k).round().max(1.0),
            gap: (4.0 + self.gap).round(),
            outline_thickness: 1.0,
            screen_height: 1080,
            legacy: false,
            ..*self
        }
    }
}

/// Its bars on a picture `h` high, centred at `centre` (whole pixels: as CS draws it, crisp) — the outlines first
/// (under), then the colour; `spread` 0..1 how far a dynamic style opens (the player's movement).
pub fn bars(c: &Crosshair, centre: [f32; 2], h: f32, spread: f32) -> Vec<Bar> {
    let (size, thick, gap, outline) = if c.legacy {
        // VRF's CrosshairRenderer: the bars in a 480-high picture's units, the gap 4 + gap pixels, unscaled
        let k = h / 480.0;
        ((c.length * k).round(), (c.thickness * k).round().max(1.0), (4.0 + c.gap).round(), c.outline_thickness.round().max(1.0))
    } else {
        let k = h / c.screen_height.max(240) as f32;
        ((c.length * k).round(), (c.thickness * k).round().max(1.0), (c.gap * k).round(), 1.0)
    };
    let gap = gap + if dynamic(c.style) { (spread.clamp(0.0, 1.0) * h / 1080.0 * 14.0).round() } else { 0.0 };
    let (cx, cy) = (centre[0].floor(), centre[1].floor());
    let xlo = cx - (thick / 2.0).floor();
    let ylo = cy - (thick / 2.0).floor();
    let (xhi, yhi) = (xlo + thick, ylo + thick);
    let mut rects: Vec<[f32; 4]> = Vec::new();
    if !dot_only(c.style) && size > 0.0 {
        rects.push([xlo - gap - size, ylo, xlo - gap, yhi]);
        rects.push([xhi + gap, ylo, xhi + gap + size, yhi]);
        if !c.t_style {
            rects.push([xlo, ylo - gap - size, xhi, ylo - gap]);
        }
        rects.push([xlo, yhi + gap, xhi, yhi + gap + size]);
    }
    if c.dot || dot_only(c.style) {
        rects.push([xlo, ylo, xhi, yhi]);
    }
    let mut out: Vec<Bar> = Vec::new();
    if c.outline_mode > 0 {
        let o = outline;
        let col = if c.legacy { [0, 0, 0, c.color[3]] } else { c.outline };
        out.extend(rects.iter().map(|r| ([r[0] - o, r[1] - o, r[2] + o, r[3] + o], col)));
    }
    out.extend(rects.iter().map(|r| (*r, c.color)));
    out
}

/// This crosshair as CS2's own code ("CS…"), to copy and share.
pub fn encode(c: &Crosshair) -> String {
    let c = &c.to_pixels();
    let mut b = [0u8; 32];
    b[1] = 1;
    let sh = c.screen_height.max(240);
    b[2] = (sh & 0xff) as u8;
    b[3] = (sh >> 8) as u8;
    b[4] = c.style.min(9) | (c.follow_recoil as u8) << 5 | (c.dot as u8) << 6 | (c.t_style as u8) << 7;
    b[5..9].copy_from_slice(&c.color);
    b[9..13].copy_from_slice(&c.outline);
    b[13] = (c.thickness.round().clamp(0.0, 32.0) as u8) | (c.outline_mode.min(2)) << 6;
    let gap = (c.gap.round().clamp(-3840.0, 3840.0) as i16).to_le_bytes();
    b[14] = gap[0];
    b[15] = gap[1];
    b[16] = c.length.round().clamp(0.0, 255.0) as u8;
    b[17] = c.spread_limit;
    let inner = (c.inner_split_alpha.clamp(0.0, 1.0) * 100.0).round() as u32;
    let outer = ((c.outer_split_alpha.clamp(0.3, 1.0) * 100.0).round() as u32).saturating_sub(30);
    let ratio = (c.split_ratio.clamp(0.0, 1.0) * 100.0).round() as u32;
    let bits = (c.split_distance.min(127) as u32) | inner << 7 | outer << 14 | ratio << 21;
    b[18..22].copy_from_slice(&bits.to_le_bytes());
    b[22] = 90; // the scope dot's scale 1.0
    b[0] = (b[1..].iter().map(|&v| v as u32).sum::<u32>() & 0xff) as u8;
    format!("CS{}", bytes_to_chars(&b, 44))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_csgo_code() {
        let c = decode("CSGO-Cn37R-YE7vo-pLCAL-aURmZ-z6zkG").unwrap();
        assert_eq!((c.gap, c.length, c.thickness, c.outline_thickness), (-1.3, 4.6, 1.2, 2.0));
        assert_eq!(c.color, [175, 81, 213, 137]);
        assert!(c.dot && c.t_style && c.outline_mode == 1);
        assert_eq!((c.split_distance, c.inner_split_alpha, c.outer_split_alpha, c.split_ratio), (6, 0.6, 0.4, 0.5));
        let c = decode("CSGO-LibdP-VCVEd-ESayK-rSivi-2UBtG").unwrap();
        assert_eq!((c.gap, c.length, c.thickness), (1.0, 5.0, 0.5));
        assert_eq!(c.color, [50, 250, 50, 255]); // its alpha off
        assert!(!c.dot && c.outline_mode == 0);
    }

    #[test]
    fn the_september_codes() {
        let c = decode("CSGO-MnUCC-89iG7-2cVar-wy7Yn-amCpF").unwrap();
        assert_eq!((c.style, c.gap, c.length, c.thickness, c.spread_limit), (6, 4.0, 8.0, 3.0, 255));
        assert_eq!(c.color, [252, 15, 192, 255]);
        assert!(c.dot && c.outline_mode == 1 && !c.t_style);
        assert_eq!((c.split_distance, c.inner_split_alpha, c.outer_split_alpha, c.split_ratio, c.screen_height), (7, 1.0, 0.45, 0.3, 1080));
        let c = decode("CSGO-G8oAC-RyvWc-Hi3CZ-voSwn-QJbfE").unwrap();
        assert_eq!((c.style, c.outline_mode, c.gap, c.length, c.thickness, c.screen_height), (8, 1, 25.0, 7.0, 20.0, 768));
        assert_eq!(c.color, [124, 57, 57, 255]);
    }

    #[test]
    fn the_cs2_code() {
        let c = decode("CSvbPubOq37zTGqtsPTP5QTrp5CB4xFXiKRLfzJsm49ZRe").unwrap();
        assert_eq!((c.style, c.outline_mode, c.gap, c.length, c.thickness, c.spread_limit, c.screen_height), (2, 0, 0.0, 5.0, 1.0, 181, 768));
        assert_eq!((c.color, c.outline), ([255, 0, 0, 255], [0, 0, 0, 255]));
        assert!(c.dot && !c.t_style && !c.follow_recoil);
        assert_eq!((c.split_distance, c.inner_split_alpha, c.outer_split_alpha, c.split_ratio), (3, 1.0, 0.35, 0.0));
        // ours written out reads back the same
        let back = decode(&encode(&c)).unwrap();
        assert_eq!(back, c);
        let d = Crosshair::default();
        assert_eq!(decode(&encode(&d)).unwrap(), d);
    }

    #[test]
    fn not_a_code() {
        for bad in ["", "CSGO-12345-12345-12345-12345-1234", "CSGO-11111-22222-33333-44444-55555", "CS123", "hello"] {
            assert!(decode(bad).is_none(), "{bad}");
        }
        // one character changed: the checksum fails
        assert!(decode("CSvbPubOq37zTGqtsPTP5QTrp5CB4xFXiKRLfzJsm49ZRf").is_none());
    }
}
