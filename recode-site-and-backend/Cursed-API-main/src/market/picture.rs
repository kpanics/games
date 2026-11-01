//! Pictures for pack pages: whatever image comes in, clean JPEGs go out (decoded and encoded anew: no metadata, no
//! tricks riding along), a big one and a small one for cards. And a pack's own icon (pack.png), kept pixel-sharp.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageEncoder, ImageFormat, ImageReader, Limits, RgbImage, RgbaImage};

#[derive(Debug)]
pub struct Made {
    pub big: Vec<u8>,
    pub small: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

const INK: [u8; 3] = [4, 3, 3]; // the site's black, under anything transparent

fn open(data: &[u8], most: u32) -> Result<DynamicImage, String> {
    let mut r = ImageReader::new(Cursor::new(data)).with_guessed_format().map_err(|_| "the picture can't be read".to_string())?;
    match r.format() {
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP) => {}
        _ => return Err("send a PNG, JPEG or WebP picture".into()),
    }
    let mut lim = Limits::default();
    lim.max_image_width = Some(most);
    lim.max_image_height = Some(most);
    lim.max_alloc = Some(512 << 20);
    r.limits(lim);
    r.decode().map_err(|e| format!("the picture doesn't decode ({e})"))
}

fn flatten(img: &DynamicImage) -> RgbImage {
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = p[3] as u32;
        for c in 0..3 {
            o[c] = ((p[c] as u32 * a + INK[c] as u32 * (255 - a) + 127) / 255) as u8;
        }
    }
    out
}

fn fit(img: &RgbImage, most_w: u32, most_h: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    if w <= most_w && h <= most_h {
        return img.clone();
    }
    let k = (most_w as f64 / w as f64).min(most_h as f64 / h as f64);
    let (nw, nh) = (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1));
    image::imageops::resize(img, nw, nh, FilterType::Lanczos3)
}

fn jpeg(img: &RgbImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(img).map_err(|e| e.to_string())?;
    Ok(out)
}

/// A screenshot for a pack page: at most 2560×1440 (and 640×360 for cards), at least 320 px on its short side.
pub fn make(data: &[u8]) -> Result<Made, String> {
    let img = open(data, 8192)?;
    let (w, h) = img.dimensions();
    if w.min(h) < 320 {
        return Err("the picture is too small (at least 320 px on its short side)".into());
    }
    if w > h * 4 || h > w * 4 {
        return Err("the picture is too narrow".into());
    }
    let rgb = flatten(&img);
    let big = fit(&rgb, 2560, 1440);
    let small = fit(&rgb, 640, 360);
    Ok(Made { big: jpeg(&big, 86)?, small: jpeg(&small, 80)?, w: big.width(), h: big.height() })
}

/// pack.png as a 128×128 PNG: pixel art scaled by whole steps without smoothing, bigger icons scaled down smoothly.
pub fn icon(data: &[u8]) -> Option<Vec<u8>> {
    let img = open(data, 2048).ok()?;
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let rgba: RgbaImage = img.to_rgba8();
    let filter = if w.max(h) <= 128 { FilterType::Nearest } else { FilterType::Lanczos3 };
    let sized = image::imageops::resize(&rgba, 128, 128, filter);
    let mut out = Vec::new();
    PngEncoder::new(&mut out).write_image(sized.as_raw(), 128, 128, image::ExtendedColorType::Rgba8).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 90, if x < 10 { 0 } else { 255 }]));
        let mut out = Vec::new();
        PngEncoder::new(&mut out).write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgba8).unwrap();
        out
    }

    #[test]
    fn a_screenshot_becomes_two_jpegs() {
        let m = make(&png_bytes(3000, 1500)).unwrap();
        assert_eq!((m.w, m.h), (2560, 1280));
        assert!(m.big.starts_with(&[0xFF, 0xD8]) && m.small.starts_with(&[0xFF, 0xD8]));
        assert!(m.small.len() < m.big.len());
    }

    #[test]
    fn junk_and_tiny_are_refused() {
        assert!(make(b"GIF89a....").is_err());
        assert!(make(&png_bytes(100, 100)).unwrap_err().contains("too small"));
        assert!(make(b"\x89PNG\r\n\x1a\nbroken").is_err());
    }

    #[test]
    fn an_icon_stays_sharp() {
        let i = icon(&png_bytes(16, 16)).unwrap();
        let back = image::load_from_memory(&i).unwrap();
        assert_eq!(back.dimensions(), (128, 128));
    }
}
