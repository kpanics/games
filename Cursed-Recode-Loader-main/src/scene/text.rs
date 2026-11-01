//! Text (name tags, holograms, the GUI's): the glyph quads of BakedSheetGlyph.renderChar — bit for bit
//! (CURSED_DEV_MODELCHECK). Java hands over a run of glyphs that share a pose and light; this writes their vertices in
//! DefaultVertexFormat.POSITION_TEX_LIGHTMAP_COLOR (position, UV0, UV2, color — 28 bytes; the world's) or
//! POSITION_TEX_COLOR (position, UV0, color — 24 bytes; the GUI's): the shadow first when the glyph has one, bold as a
//! second quad shifted by the bold offset, italic as a shear — as the game does.

pub const VERTEX: usize = 28;
pub const GUI_VERTEX: usize = 24;
/// i32 matrix properties, i32 light, i32 flat, i32 format (0 POSITION_TEX_LIGHTMAP_COLOR, 1 POSITION_TEX_COLOR),
/// f32 × 16 pose (column-major).
pub const HEADER: usize = 80;
/// f32 x y shadowOffset boldOffset left right up down u0 u1 v0 v1, i32 color, shadow color, flags (1 italic, 2 bold).
pub const GLYPH: usize = 60;

const PROPERTY_IDENTITY: i32 = 4;
const PROPERTY_TRANSLATION: i32 = 8;

#[inline]
fn to_abgr(color: i32) -> u32 {
    ((color & 0xFF00FF00u32 as i32) | ((color >> 16) & 0xFF) | ((color & 0xFF) << 16)) as u32
}

struct Out<'a> {
    m: &'a [f32; 16],
    props: i32,
    light: [u8; 4],
    gui: bool,
    dst: *mut u8,
    written: usize,
}

impl Out<'_> {
    #[inline]
    unsafe fn vertex(&mut self, x: f32, y: f32, z: f32, u: f32, v: f32, color: [u8; 4]) {
        let m = self.m;
        let p = if self.props & PROPERTY_IDENTITY != 0 {
            [x, y, z]
        } else if self.props & PROPERTY_TRANSLATION != 0 {
            [x + m[12], y + m[13], z + m[14]]
        } else {
            [
                m[0] * x + (m[4] * y + (m[8] * z + m[12])),
                m[1] * x + (m[5] * y + (m[9] * z + m[13])),
                m[2] * x + (m[6] * y + (m[10] * z + m[14])),
            ]
        };
        let mut out = [0u8; VERTEX];
        out[0..4].copy_from_slice(&p[0].to_le_bytes());
        out[4..8].copy_from_slice(&p[1].to_le_bytes());
        out[8..12].copy_from_slice(&p[2].to_le_bytes());
        out[12..16].copy_from_slice(&u.to_le_bytes());
        out[16..20].copy_from_slice(&v.to_le_bytes());
        if self.gui {
            out[20..24].copy_from_slice(&color);
            std::ptr::copy_nonoverlapping(out.as_ptr(), self.dst.add(self.written * GUI_VERTEX), GUI_VERTEX);
        } else {
            out[20..24].copy_from_slice(&self.light);
            out[24..28].copy_from_slice(&color);
            std::ptr::copy_nonoverlapping(out.as_ptr(), self.dst.add(self.written * VERTEX), VERTEX);
        }
        self.written += 1;
    }
}

/// BakedSheetGlyph.render: one quad of a glyph at (x, y, z).
#[allow(clippy::too_many_arguments)]
#[inline]
unsafe fn quad(o: &mut Out, g: &[f32], italic: bool, x: f32, y: f32, z: f32, color: [u8; 4], bold: bool) {
    let (left, right, up, down, u0, u1, v0, v1) = (g[4], g[5], g[6], g[7], g[8], g[9], g[10], g[11]);
    let f = x + left;
    let f1 = x + right;
    let f2 = y + up;
    let f3 = y + down;
    let f4 = if italic { 1.0 - 0.25 * up } else { 0.0 };
    let f5 = if italic { 1.0 - 0.25 * down } else { 0.0 };
    let f6 = if bold { 0.1 } else { 0.0 };
    o.vertex(f + f4 - f6, f2 - f6, z, u0, v0, color);
    o.vertex(f + f5 - f6, f3 + f6, z, u0, v1, color);
    o.vertex(f1 + f5 + f6, f3 + f6, z, u1, v1, color);
    o.vertex(f1 + f4 + f6, f2 - f6, z, u1, v0, color);
}

/// Vertices a glyph makes (Java reserves this many).
pub fn vertices(shadow: bool, bold: bool) -> usize {
    let quads = if bold { 2 } else { 1 };
    4 * quads * if shadow { 2 } else { 1 }
}

/// renderChar for `count` glyphs; returns the vertices written.
///
/// # Safety
/// `dst` must have room for the glyphs' vertices.
pub unsafe fn glyphs(header: &[u8], glyphs: &[u8], count: usize, dst: *mut u8) -> usize {
    let hi = |k: usize| i32::from_le_bytes(header[k * 4..k * 4 + 4].try_into().unwrap());
    let m: [f32; 16] = std::array::from_fn(|k| f32::from_le_bytes(header[16 + k * 4..20 + k * 4].try_into().unwrap()));
    let flat = hi(2) != 0;
    let mut o = Out { m: &m, props: hi(0), light: hi(1).to_le_bytes(), gui: hi(3) == 1, dst, written: 0 };
    for i in 0..count {
        let rec = &glyphs[i * GLYPH..(i + 1) * GLYPH];
        let g: [f32; 12] = std::array::from_fn(|k| f32::from_le_bytes(rec[k * 4..k * 4 + 4].try_into().unwrap()));
        let gi = |k: usize| i32::from_le_bytes(rec[k * 4..k * 4 + 4].try_into().unwrap());
        let (x, y, shadow_offset, bold_offset) = (g[0], g[1], g[2], g[3]);
        let color = to_abgr(gi(12)).to_le_bytes();
        let shadow_color = gi(13);
        let italic = gi(14) & 1 != 0;
        let bold = gi(14) & 2 != 0;
        let f3 = if flat { 0.0 } else { 0.001 };
        let f2 = if shadow_color != 0 {
            let sc = to_abgr(shadow_color).to_le_bytes();
            quad(&mut o, &g, italic, x + shadow_offset, y + shadow_offset, 0.0, sc, bold);
            if bold {
                quad(&mut o, &g, italic, x + bold_offset + shadow_offset, y + shadow_offset, f3, sc, true);
            }
            if flat {
                0.0
            } else {
                0.03
            }
        } else {
            0.0
        };
        quad(&mut o, &g, italic, x, y, f2, color, bold);
        if bold {
            quad(&mut o, &g, italic, x + bold_offset, y, f2 + f3, color, true);
        }
    }
    o.written
}
