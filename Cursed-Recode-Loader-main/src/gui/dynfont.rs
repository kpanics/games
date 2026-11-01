//! Letters the GUI's faces lack (Greek, Chinese, Japanese, Korean, Arabic, Hebrew, Thai, the small capitals and marks
//! servers write their names in…): drawn at run time from the system's fonts (GDI), turned into the same distance
//! fields as the atlas's (tools/gui_atlas.py: rendered at 4x, the distance kept at EM px to the em, SPREAD either
//! side), packed into a second atlas render.rs uploads as they come. They are made on a thread of their own, at low
//! priority: a letter asked for the first time is not there for a frame or a few (a list of every language's name
//! would otherwise stop the game for seconds).

use super::text::{Face, Glyph, EM, SPREAD};
use std::collections::HashMap;
use std::sync::Mutex;
use windows_sys::Win32::Graphics::Gdi::*;

/// The second atlas's side (R8: 4 MB).
pub const SIZE: u32 = 2048;
const UP: i32 = 4;

/// The system's fonts tried in turn for a letter (the first that has it draws it).
const FALLBACK: &[&str] = &[
    "Segoe UI", "Segoe UI Symbol", "Segoe UI Historic", "Microsoft YaHei UI", "Microsoft JhengHei UI", "Yu Gothic UI", "Malgun Gothic", "Nirmala UI",
    "Leelawadee UI", "Ebrima", "Gadugi", "Javanese Text", "Myanmar Text", "Mongolian Baiti", "Microsoft Himalaya", "Segoe UI Emoji", "Arial Unicode MS",
];

#[derive(Default)]
struct Store {
    /// A letter in a weight: its glyph in the second atlas (None: no font has it).
    map: HashMap<(char, u8), Option<Glyph>>,
    /// Asked for and being made.
    asked: std::collections::HashSet<(char, u8)>,
    x: u32,
    y: u32,
    shelf: u32,
    pending: Vec<([u32; 4], Vec<u8>)>,
    full: bool,
}

static STORE: Mutex<Option<Store>> = Mutex::new(None);

/// The weight a face asks for (GDI's).
fn weight(face: Face) -> (u8, i32) {
    match face {
        Face::Thin => (0, 250),
        Face::Light => (1, 300),
        Face::Book | Face::Caps | Face::Regular | Face::Mono => (2, 400),
        Face::Medium | Face::Semi => (3, 600),
        Face::Bold | Face::Heavy => (4, 700),
    }
}

/// `c` in `face`'s weight, from the system's fonts (made once; None: none of them has it, or not made yet).
pub fn glyph(face: Face, c: char) -> Option<Glyph> {
    let (w, gdi_weight) = weight(face);
    let mut s = STORE.lock().unwrap();
    let st = s.get_or_insert_with(Store::default);
    if let Some(g) = st.map.get(&(c, w)) {
        return *g;
    }
    if !st.full && st.asked.insert((c, w)) {
        let _ = maker().send((c, w, gdi_weight));
    }
    None
}

/// Whether `c` in `face`'s weight was asked for and is not made yet (it may still come).
pub fn waiting(face: Face, c: char) -> bool {
    let (w, _) = weight(face);
    let s = STORE.lock().unwrap();
    s.as_ref().is_some_and(|st| !st.full && st.asked.contains(&(c, w)) && !st.map.contains_key(&(c, w)))
}

/// The thread that makes the letters (low priority), and how it is asked.
fn maker() -> &'static std::sync::mpsc::Sender<(char, u8, i32)> {
    static SENDER: std::sync::OnceLock<std::sync::mpsc::Sender<(char, u8, i32)>> = std::sync::OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(char, u8, i32)>();
        std::thread::Builder::new()
            .name("cursed-letters".into())
            .spawn(move || {
                unsafe {
                    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
                    SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
                }
                let mut fonts = Fonts::default();
                while let Ok((c, w, gdi_weight)) = rx.recv() {
                    let made = unsafe { draw(&mut fonts, c, gdi_weight) };
                    place(c, w, made);
                }
            })
            .expect("the letters' thread");
        tx
    })
}

/// A letter made (or not: no font has it) into the second atlas.
fn place(c: char, w: u8, made: Option<(Vec<u8>, u32, u32, f32, f32, f32)>) {
    let mut s = STORE.lock().unwrap();
    let st = s.get_or_insert_with(Store::default);
    st.asked.remove(&(c, w));
    let g = made.and_then(|(field, fw, fh, adv, left, top)| {
        // a place in the second atlas (shelves)
        if st.x + fw + 1 > SIZE {
            st.x = 0;
            st.y += st.shelf + 1;
            st.shelf = 0;
        }
        if st.y + fh > SIZE {
            st.full = true;
            return None;
        }
        let at = [st.x, st.y, fw, fh];
        st.x += fw + 1;
        st.shelf = st.shelf.max(fh);
        st.pending.push((at, field));
        Some(Glyph { adv, left, top, at: [at[0] as u16, at[1] as u16, fw as u16, fh as u16], page: 1 })
    });
    st.map.insert((c, w), g);
}

/// The system's fonts made once for the letters' thread (by their place in FALLBACK and the weight).
#[derive(Default)]
struct Fonts {
    made: HashMap<(usize, i32), HFONT>,
}

impl Fonts {
    unsafe fn get(&mut self, i: usize, gdi_weight: i32) -> HFONT {
        *self.made.entry((i, gdi_weight)).or_insert_with(|| {
            let face = wide(FALLBACK[i]);
            CreateFontW(-(EM as i32 * UP), 0, 0, 0, gdi_weight, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_TT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, ANTIALIASED_QUALITY as u32, 0, face.as_ptr())
        })
    }
}

/// The renderer's frame: the letters to copy into the second atlas now.
pub fn drain() -> Vec<([u32; 4], Vec<u8>)> {
    let mut s = STORE.lock().unwrap();
    s.as_mut().map_or_else(Vec::new, |st| std::mem::take(&mut st.pending))
}

pub fn any() -> bool {
    let s = STORE.lock().unwrap();
    s.as_ref().is_some_and(|st| !st.map.is_empty())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The letter drawn by the first system font that has it, at 4x, as a distance field at EM (its cells R8, its size,
/// its advance, left and top in atlas px of EM).
unsafe fn draw(fonts: &mut Fonts, c: char, gdi_weight: i32) -> Option<(Vec<u8>, u32, u32, f32, f32, f32)> {
    let dc = CreateCompatibleDC(std::ptr::null_mut());
    if dc.is_null() {
        return None;
    }
    let mut utf16 = [0u16; 2];
    let units = c.encode_utf16(&mut utf16).len();
    let mut out = None;
    for i in 0..FALLBACK.len() {
        let font = fonts.get(i, gdi_weight);
        if font.is_null() {
            continue;
        }
        let old = SelectObject(dc, font as HGDIOBJ);
        // whether this font has it (surrogate pairs: no index check, try to draw it)
        let mut idx = [0u16; 2];
        let has = if units == 1 { GetGlyphIndicesW(dc, utf16.as_ptr(), 1, idx.as_mut_ptr(), GGI_MARK_NONEXISTING_GLYPHS) != GDI_ERROR as u32 && idx[0] != 0xffff } else { true };
        if has {
            out = outline(dc, c);
        }
        SelectObject(dc, old);
        if out.is_some() {
            break;
        }
    }
    DeleteDC(dc);
    out
}

unsafe fn outline(dc: HDC, c: char) -> Option<(Vec<u8>, u32, u32, f32, f32, f32)> {
    let ident = MAT2 { eM11: FIXED { fract: 0, value: 1 }, eM12: FIXED { fract: 0, value: 0 }, eM21: FIXED { fract: 0, value: 0 }, eM22: FIXED { fract: 0, value: 1 } };
    let mut gm: GLYPHMETRICS = std::mem::zeroed();
    let size = GetGlyphOutlineW(dc, c as u32, GGO_GRAY8_BITMAP, &mut gm, 0, std::ptr::null_mut(), &ident);
    if size == GDI_ERROR as u32 {
        return None;
    }
    let (bw, bh) = (gm.gmBlackBoxX as i32, gm.gmBlackBoxY as i32);
    let adv = gm.gmCellIncX as f32 / UP as f32;
    if size == 0 || bw <= 0 || bh <= 0 {
        // a space: only its advance
        return Some((vec![0u8; 1], 1, 1, adv, 0.0, 0.0));
    }
    let mut bits = vec![0u8; size as usize];
    if GetGlyphOutlineW(dc, c as u32, GGO_GRAY8_BITMAP, &mut gm, size, bits.as_mut_ptr() as *mut _, &ident) == GDI_ERROR as u32 {
        return None;
    }
    let pitch = ((bw + 3) & !3) as usize;
    // the coverage into a padded canvas (the field reaches SPREAD out), a multiple of UP each way
    let pad = SPREAD as i32 * UP;
    let mut w = bw + 2 * pad;
    let mut h = bh + 2 * pad;
    w += (UP - w % UP) % UP;
    h += (UP - h % UP) % UP;
    let mut inside = vec![false; (w * h) as usize];
    for y in 0..bh {
        for x in 0..bw {
            // GGO_GRAY8: 0..64
            if bits[y as usize * pitch + x as usize] >= 32 {
                inside[((y + pad) * w + x + pad) as usize] = true;
            }
        }
    }
    let d_in = edt(&inside, w as usize, h as usize, true);
    let d_out = edt(&inside, w as usize, h as usize, false);
    let (fw, fh) = ((w / UP) as u32, (h / UP) as u32);
    let mut field = vec![0u8; (fw * fh) as usize];
    for y in 0..fh {
        for x in 0..fw {
            let (sx, sy) = ((x as i32 * UP + UP / 2) as usize, (y as i32 * UP + UP / 2) as usize);
            let i = sy * w as usize + sx;
            let d = (d_in[i].sqrt() - d_out[i].sqrt()) / UP as f32;
            field[(y * fw + x) as usize] = (128.0 + d * 127.0 / SPREAD).clamp(0.0, 255.0) as u8;
        }
    }
    // the box's corner from the pen (y down from the baseline), in atlas px
    let left = (gm.gmptGlyphOrigin.x - pad) as f32 / UP as f32;
    let top = (-gm.gmptGlyphOrigin.y - pad) as f32 / UP as f32;
    Some((field, fw, fh, adv, left, top))
}

/// The squared distance of each cell to the nearest cell of the other kind (`of_inside`: inside cells measured to
/// the outside), Felzenszwalb & Huttenlocher's way: rows, then columns.
fn edt(inside: &[bool], w: usize, h: usize, of_inside: bool) -> Vec<f32> {
    const INF: f32 = 1e20;
    let mut g: Vec<f32> = (0..w * h).map(|i| if inside[i] == of_inside { INF } else { 0.0 }).collect();
    let n_max = w.max(h);
    let (mut f, mut d, mut v, mut z) = (vec![0f32; n_max], vec![0f32; n_max], vec![0usize; n_max], vec![0f32; n_max + 1]);
    let mut pass = |g: &mut Vec<f32>, n: usize, at: &dyn Fn(usize) -> usize| {
        for q in 0..n {
            f[q] = g[at(q)];
        }
        // the lower envelope of the parabolas from each cell
        let mut k = 0usize;
        v[0] = 0;
        z[0] = -INF;
        z[1] = INF;
        let inter = |f: &[f32], q: usize, p: usize| ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
        for q in 1..n {
            if f[q] >= INF {
                continue;
            }
            let mut s = inter(&f, q, v[k]);
            while s <= z[k] {
                if k == 0 {
                    break;
                }
                k -= 1;
                s = inter(&f, q, v[k]);
            }
            if f[v[0]] >= INF {
                // nothing yet but far cells: this one starts the envelope
                v[0] = q;
                z[0] = -INF;
                z[1] = INF;
                k = 0;
                continue;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = INF;
        }
        k = 0;
        for q in 0..n {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let p = v[k];
            d[q] = if f[p] >= INF { INF } else { (q as f32 - p as f32).powi(2) + f[p] };
        }
        for q in 0..n {
            g[at(q)] = d[q];
        }
    };
    for y in 0..h {
        pass(&mut g, w, &|x| y * w + x);
    }
    for x in 0..w {
        pass(&mut g, h, &|y| y * w + x);
    }
    g.into_iter().map(|x| x.min(1e12)).collect()
}
