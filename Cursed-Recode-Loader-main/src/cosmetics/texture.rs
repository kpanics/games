//! A cosmetic's texture (.rtx, tools/cs2pack/rtx.py makes it): RGBA8 with its whole mip chain (the largest first),
//! compressed with zlib; flagged sRGB when it is a colour (the GPU then reads it in linear light), or 16 bits a
//! channel (flag bit 1: a weapon's positions, for its finish's pattern).

#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub srgb: bool,
    /// 16 bits a channel (RGBA16 unorm, little endian) instead of 8.
    pub wide: bool,
    /// Each mip's texels, the largest first.
    pub mips: Vec<Vec<u8>>,
}

/// The largest side a texture may have (a broken file is refused before it asks for gigabytes).
const MOST: u32 = 8192;

impl Image {
    pub fn parse(d: &[u8]) -> Result<Image, String> {
        if d.len() < 20 || &d[..4] != b"RTX1" {
            return Err("not a cosmetic texture".into());
        }
        let u = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let (width, height, mips, flags) = (u(4), u(8), u(12), u(16));
        if width == 0 || height == 0 || width > MOST || height > MOST || mips == 0 || mips > 14 {
            return Err("bad texture sizes".into());
        }
        let wide = flags & 2 != 0;
        let texel = if wide { 8 } else { 4 };
        let mut sizes = Vec::with_capacity(mips as usize);
        let (mut w, mut h) = (width, height);
        for _ in 0..mips {
            sizes.push((w * h * texel) as usize);
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        let total: usize = sizes.iter().sum();
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&d[20..], total).map_err(|e| format!("the texture does not inflate: {e:?}"))?;
        if raw.len() != total {
            return Err(format!("the texture holds {} bytes of {total}", raw.len()));
        }
        let mut out = Vec::with_capacity(sizes.len());
        let mut at = 0;
        for s in sizes {
            out.push(raw[at..at + s].to_vec());
            at += s;
        }
        Ok(Image { width, height, srgb: flags & 1 != 0, wide, mips: out })
    }

    /// One texel of one colour (what a material without that texture gets).
    pub fn solid(rgba: [u8; 4], srgb: bool) -> Image {
        Image { width: 1, height: 1, srgb, wide: false, mips: vec![rgba.to_vec()] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(w: u32, h: u32, srgb: bool) -> Vec<u8> {
        let mut raw = Vec::new();
        let (mut x, mut y) = (w, h);
        let mut n = 0;
        loop {
            raw.extend(std::iter::repeat_n(n as u8, (x * y * 4) as usize));
            n += 1;
            if x == 1 && y == 1 {
                break;
            }
            x = (x / 2).max(1);
            y = (y / 2).max(1);
        }
        let mut d = b"RTX1".to_vec();
        for v in [w, h, n, srgb as u32] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend(miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
        d
    }

    #[test]
    fn a_texture_is_read_with_its_mips() {
        let t = Image::parse(&make(8, 4, true)).unwrap();
        assert_eq!((t.width, t.height, t.mips.len(), t.srgb), (8, 4, 4, true));
        assert_eq!(t.mips[0].len(), 8 * 4 * 4);
        assert_eq!(t.mips[3], vec![3; 4]);
        let d = make(8, 4, false);
        assert!(Image::parse(&d[..d.len() - 2]).is_err());
        assert!(Image::parse(b"RTX0").is_err());
    }

    #[test]
    fn a_packed_texture_reads() {
        let Ok(d) = std::fs::read("D:/CS2/work/dota/tinker_color.rtx") else { return };
        let t = Image::parse(&d).unwrap();
        assert!(t.srgb && t.mips.len() > 1);
    }
}
