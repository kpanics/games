//! A name tag (CS2's: a plate on the weapon, the name written on it as its composite material writes it —
//! nametag_default.vcompmat: Arial Narrow, light grey (241, 241, 241), from the plate's top left): the plate's colour
//! texture with the name on it, made on a thread of its own (the system's font through GDI), kept by the name, read
//! by the renderer as any texture.

use super::store;
use super::texture::Image;
use std::collections::HashSet;
use std::sync::Mutex;
use windows_sys::Win32::Graphics::Gdi::*;

/// The plate's model, its material, its own colour texture (the plate unwritten).
pub const MODEL: &str = "nametag/module.rcm";
pub const MATERIAL: &str = "nametag_default";
const BASE: &str = "nametag/nametag_color_tga_c73f4653.rtx";
/// At most so many letters (CS2's).
pub const MAX_CHARS: usize = 20;
/// Where the name starts on the plate's texture (its share, the composite material's), its letters' height (px of
/// the 512 a side), its colour.
const AT: [f32; 2] = [0.04, 0.038];
const HEIGHT: i32 = 34;
const COLOUR: [u8; 3] = [241, 241, 241];

/// A name as a name tag takes it: no line breaks or the loadout's own separators, at most MAX_CHARS letters.
pub fn clean(name: &str) -> String {
    name.chars().filter(|c| !c.is_control() && !matches!(c, '|' | ';' | '@')).take(MAX_CHARS).collect()
}

fn hash(s: &str) -> u64 {
    // FNV-1a
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// The key of the plate's texture with `name` written on it (asked for once: made on a thread, then to the GPU).
pub fn texture(name: &str) -> String {
    static ASKED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    let key = format!("nametag/written/{:016x}.rtx", hash(name));
    if ASKED.lock().unwrap().get_or_insert_with(HashSet::new).insert(key.clone()) {
        let (name, k) = (name.to_string(), key.clone());
        std::thread::Builder::new()
            .name("Cursed name tag".into())
            .spawn(move || {
                if let Some(img) = written(&name) {
                    store::ready_texture(&k, img);
                }
            })
            .ok();
    }
    key
}

/// The plate with the name written on it.
fn written(name: &str) -> Option<Image> {
    let base = Image::parse(&store::read_wait(BASE)?).ok()?;
    let (w, h) = (base.width as usize, base.height as usize);
    let mut px = base.mips.first()?.clone();
    if px.len() < w * h * 4 {
        return None;
    }
    let scale = w as f32 / 512.0;
    let text_h = (HEIGHT as f32 * scale).round() as i32;
    let (cov, cw, ch) = unsafe { coverage(name, text_h)? };
    let (x0, y0) = ((AT[0] * w as f32) as usize, (AT[1] * h as f32) as usize);
    // the letters over the plate, by their coverage (the plate's colours are sRGB: mixed in linear light)
    let lin = |v: u8| -> f32 {
        let c = v as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let srgb = |c: f32| -> u8 {
        let c = c.clamp(0.0, 1.0);
        let v = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
        (v * 255.0).round() as u8
    };
    for y in 0..ch.min(h.saturating_sub(y0)) {
        for x in 0..cw.min(w.saturating_sub(x0)) {
            let a = cov[y * cw + x] as f32 / 255.0;
            if a <= 0.0 {
                continue;
            }
            let o = ((y0 + y) * w + x0 + x) * 4;
            for k in 0..3 {
                px[o + k] = srgb(lin(px[o + k]) * (1.0 - a) + lin(COLOUR[k]) * a);
            }
        }
    }
    Some(Image { width: base.width, height: base.height, mips: mips(px, w, h), srgb: true, wide: false })
}

/// The mips of an sRGB picture (each half the last, its colours averaged in linear light).
fn mips(top: Vec<u8>, w: usize, h: usize) -> Vec<Vec<u8>> {
    let lin: Vec<f32> = (0..256)
        .map(|v| {
            let c = v as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
        .collect();
    let mut out = vec![top];
    let (mut w, mut h) = (w, h);
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let src = out.last().unwrap();
        let mut dst = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for k in 0..4 {
                    let mut sum = 0.0;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(w - 1);
                        let sy = (y * 2 + dy).min(h - 1);
                        let v = src[(sy * w + sx) * 4 + k];
                        sum += if k < 3 { lin[v as usize] } else { v as f32 / 255.0 };
                    }
                    let a = sum / 4.0;
                    dst[(y * nw + x) * 4 + k] = if k < 3 {
                        let v = if a <= 0.003_130_8 { a * 12.92 } else { 1.055 * a.powf(1.0 / 2.4) - 0.055 };
                        (v * 255.0).round() as u8
                    } else {
                        (a * 255.0).round() as u8
                    };
                }
            }
        }
        out.push(dst);
        w = nw;
        h = nh;
    }
    out
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The name drawn by the system (Arial Narrow; Arial pressed narrow where it is not there), `height` px tall: its
/// coverage (0..255, a row after another), its width and height.
unsafe fn coverage(name: &str, height: i32) -> Option<(Vec<u8>, usize, usize)> {
    let dc = CreateCompatibleDC(std::ptr::null_mut());
    if dc.is_null() {
        return None;
    }
    let make = |face: &str, width: i32| {
        let f = wide(face);
        CreateFontW(-height, width, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_TT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, ANTIALIASED_QUALITY as u32, 0, f.as_ptr())
    };
    let mut font = make("Arial Narrow", 0);
    let old = SelectObject(dc, font as HGDIOBJ);
    let mut got = [0u16; 64];
    let n = GetTextFaceW(dc, got.len() as i32, got.as_mut_ptr());
    let face = String::from_utf16_lossy(&got[..(n.max(1) - 1) as usize]);
    if !face.eq_ignore_ascii_case("Arial Narrow") {
        SelectObject(dc, old);
        DeleteObject(font as HGDIOBJ);
        // Arial Narrow's letters are some 0.82 of Arial's wide
        font = make("Arial", (height as f32 * 0.37) as i32);
        SelectObject(dc, font as HGDIOBJ);
    }
    let text: Vec<u16> = name.encode_utf16().collect();
    let mut size = windows_sys::Win32::Foundation::SIZE { cx: 0, cy: 0 };
    GetTextExtentPoint32W(dc, text.as_ptr(), text.len() as i32, &mut size);
    let (w, h) = ((size.cx.max(1) + 4) as usize, (size.cy.max(1) + 2) as usize);
    // a white-on-black picture of it (32-bit, top-down)
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = w as i32;
    info.bmiHeader.biHeight = -(h as i32);
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    let mut out = None;
    if !bmp.is_null() && !bits.is_null() {
        let old_bmp = SelectObject(dc, bmp as HGDIOBJ);
        std::ptr::write_bytes(bits as *mut u8, 0, w * h * 4);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, 0x00ff_ffff);
        TextOutW(dc, 1, 0, text.as_ptr(), text.len() as i32);
        GdiFlush();
        let src = std::slice::from_raw_parts(bits as *const u8, w * h * 4);
        out = Some(((0..w * h).map(|i| src[i * 4 + 1]).collect(), w, h));
        SelectObject(dc, old_bmp);
        DeleteObject(bmp as HGDIOBJ);
    }
    SelectObject(dc, old);
    DeleteObject(font as HGDIOBJ);
    DeleteDC(dc);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_cleaned_for_the_loadout() {
        assert_eq!(clean("Lunuca|best;@ever\nyes"), "Lunucabesteveryes");
        assert_eq!(clean(&"x".repeat(40)).chars().count(), MAX_CHARS);
    }

    #[test]
    fn the_names_letters_are_drawn() {
        let (cov, w, h) = unsafe { coverage("Lunuca", 34) }.expect("drawn");
        assert!(w > 40 && h >= 30, "{w}x{h}");
        assert!(cov.iter().filter(|&&v| v > 128).count() > 100);
    }

    #[test]
    #[ignore]
    fn probe_where_the_plate_goes() {
        use crate::cosmetics::model::Model;
        let load = |p: &str| Model::parse(&std::fs::read(format!("D:/CS2/work/out/{p}")).unwrap()).unwrap();
        let plate = load("nametag/module.rcm");
        println!("plate bounds {:?}", plate.bounds());
        for item in ["weapon_awp", "weapon_knife_karambit"] {
            let m = load(&format!("items/{item}/model.rcm"));
            let g = m.globals(&m.rest());
            let atts = crate::cosmetics::attach::parse(&std::fs::read_to_string(format!("D:/CS2/work/out/items/{item}/attachments.txt")).unwrap());
            let at = crate::cosmetics::attach::place(&atts["nametag"], &m, &g).unwrap();
            let h = crate::cosmetics::attach::holding(&at);
            println!("{item} bounds {:?} nametag at {:?} x {:?} y {:?} z {:?}", m.bounds(), [h[12], h[13], h[14]], [h[0], h[1], h[2]], [h[4], h[5], h[6]], [h[8], h[9], h[10]]);
            let on: Vec<u32> = (0..m.materials.len() as u32).collect();
            let out = [h[0], h[1], h[2]];
            let from: [f32; 3] = std::array::from_fn(|k| h[12 + k] + out[k] * 0.1);
            let hit = crate::cosmetics::sticker::raycast(&m, &on, from, out.map(|v| -v));
            println!("   the body's surface out of it there: {:?}", hit.map(|h| (0.1 - h.0, h.1)));
            for body in ["legacy.rcm"] {
                if let Ok(d) = std::fs::read(format!("D:/CS2/work/out/items/{item}/{body}")) {
                    let lm = Model::parse(&d).unwrap();
                    let lg = lm.globals(&lm.rest());
                    if let Some(a) = atts.get("nametag_legacy") {
                        let lat = crate::cosmetics::attach::holding(&crate::cosmetics::attach::place(a, &lm, &lg).unwrap());
                        let lon: Vec<u32> = (0..lm.materials.len() as u32).collect();
                        let lout = [lat[0], lat[1], lat[2]];
                        let lfrom: [f32; 3] = std::array::from_fn(|k| lat[12 + k] + lout[k] * 0.1);
                        let lhit = crate::cosmetics::sticker::raycast(&lm, &lon, lfrom, lout.map(|v| -v));
                        println!("   legacy at {:?}, surface out of it {:?}", [lat[12], lat[13], lat[14]], lhit.map(|h| (0.1 - h.0, h.1)));
                    }
                }
            }
        }
    }

    #[test]
    fn mips_halve_to_one() {
        let m = mips(vec![255; 8 * 4 * 4], 8, 4);
        assert_eq!(m.iter().map(|v| v.len() / 4).collect::<Vec<_>>(), vec![32, 8, 2, 1]);
    }
}
