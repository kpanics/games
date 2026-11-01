//! What members upload for their profiles (and later their messages): banners (a picture or a moving GIF), the
//! screenshots of their gallery, and their music (MP3). Like the avatars, whatever comes in is decoded and made anew,
//! so nothing of the sent bytes is ever served: no camera data in a picture, no tags or cover in a song. Kept under
//! its hash beside the market's pictures (served from /media/), and recorded with its owner here: for the quota, for
//! reports, and for taking it down.
//!
//! - banner: cut to 3:1 from its middle; a picture 1500×500 (JPEG), a GIF 750×250 for the site and a sheet of at most
//!   48 frames 480×160 for the client (which draws the frames itself);
//! - shot: fitted within 1920×1080 (JPEG), with a 480×270 thumbnail;
//! - music: an MP3's frames only (layer III, as sent), up to 10 minutes; its title and artist read from its tags
//!   before they go, to fill the form.
//!
//! Limits: 12 MB a picture or GIF, 10 MB a song, 60 uploads an hour, 300 MB a member in all.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use image::codecs::gif::{GifEncoder, Repeat};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{Delay, Frame, ImageFormat, ImageReader, RgbImage, RgbaImage};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::avatar::{gif_frames_with, keep, limits, png, url};
use crate::{now, random_hex};

pub const MOST_PICTURE: usize = 12 << 20;
pub const MOST_MUSIC: usize = 10 << 20;
/// A voice message: three minutes of Opus (a browser records about 32 kbit/s; room for more).
pub const MOST_VOICE: usize = 3 << 20;
const MOST_VOICE_MS: i64 = 3 * 60 * 1000;
const MOST_MUSIC_MS: u64 = 10 * 60 * 1000;
const PER_HOUR: usize = 60;
const QUOTA: i64 = 300 << 20;
const BANNER: (u32, u32) = (1500, 500);
const BANNER_GIF: (u32, u32) = (750, 250);
const BANNER_SHEET: (u32, u32) = (480, 160);
const BANNER_GIF_FRAMES: usize = 120;
const BANNER_SHEET_FRAMES: usize = 48;
const SHOT: (u32, u32) = (1920, 1080);
const SHOT_THUMB: (u32, u32) = (480, 270);

pub fn schema(c: &Connection) {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS media (
             id TEXT PRIMARY KEY,
             owner INTEGER NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
             kind TEXT NOT NULL,                      -- banner | shot | music
             info TEXT NOT NULL,                      -- what the site and the client are told (urls, sizes, times)
             files TEXT NOT NULL,                     -- the names of its files (JSON)
             bytes INTEGER NOT NULL,
             created INTEGER NOT NULL,
             state TEXT NOT NULL DEFAULT 'ok',        -- ok | removed
             why TEXT);
         CREATE INDEX IF NOT EXISTS media_owner ON media(owner, kind);",
    )
    .expect("make the media table");
}

/// One more upload for this member within the hour, if they have one left.
pub fn paced(uid: i64) -> bool {
    static BOOK: OnceLock<Mutex<HashMap<i64, Vec<i64>>>> = OnceLock::new();
    if crate::is_owner(uid) {
        return true;
    }
    let at = now();
    let mut b = BOOK.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner());
    let list = b.entry(uid).or_default();
    list.retain(|t| at - t < 3600);
    if list.len() >= PER_HOUR {
        return false;
    }
    list.push(at);
    true
}

/// What a member's uploads take now (removed ones do not count).
pub fn used(c: &Connection, uid: i64) -> i64 {
    c.query_row("SELECT COALESCE(SUM(bytes), 0) FROM media WHERE owner = ?1 AND state = 'ok'", params![uid], |r| r.get(0)).unwrap_or(0)
}

pub fn quota_left(c: &Connection, uid: i64) -> i64 {
    if crate::is_owner(uid) {
        return i64::MAX;
    }
    QUOTA - used(c, uid)
}

/// A made upload recorded under its owner: its id, and what is shown of it (with the id in it).
pub fn record(c: &Connection, owner: i64, kind: &str, made: &Made) -> rusqlite::Result<Value> {
    let id = random_hex(8);
    let mut info = made.info.clone();
    info["id"] = json!(id);
    c.execute(
        "INSERT INTO media (id, owner, kind, info, files, bytes, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, owner, kind, info.to_string(), json!(made.files).to_string(), made.bytes as i64, now()],
    )?;
    Ok(info)
}

/// An upload of its owner's, as recorded (None when it is not theirs, or gone).
pub fn info_of(c: &Connection, owner: i64, id: &str) -> Option<(String, Value)> {
    c.query_row("SELECT kind, info FROM media WHERE id = ?1 AND owner = ?2 AND state = 'ok'", params![id, owner], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .optional()
        .ok()
        .flatten()
        .and_then(|(k, i)| serde_json::from_str(&i).ok().map(|v| (k, v)))
}

/// Taken down (by its owner, or by staff with why): no longer counted, no longer shown. Its files stay on the disk
/// while anything else may share them (they are kept under their hash).
pub fn remove(c: &Connection, id: &str, why: Option<&str>) -> rusqlite::Result<usize> {
    c.execute("UPDATE media SET state = 'removed', why = ?2 WHERE id = ?1 AND state = 'ok'", params![id, why])
}

pub struct Made {
    pub info: Value,
    pub files: Vec<String>,
    pub bytes: usize,
}

// ------------------------------------------------------------------------------------------------ pictures

fn decode(data: &[u8]) -> Result<(ImageFormat, ImageReader<Cursor<&[u8]>>), String> {
    let reader = ImageReader::new(Cursor::new(data)).with_guessed_format().map_err(|_| "the picture can't be read".to_string())?;
    let format = reader.format().ok_or_else(|| "send a PNG, JPEG, WebP or GIF".to_string())?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Gif) {
        return Err("send a PNG, JPEG, WebP or GIF".into());
    }
    Ok((format, reader))
}

fn still_of(reader: ImageReader<Cursor<&[u8]>>) -> Result<RgbaImage, String> {
    let mut r = reader;
    r.limits(limits());
    Ok(r.decode().map_err(|e| format!("the picture doesn't decode ({e})"))?.to_rgba8())
}

/// The middle of a picture cut to `w`:`h`, then made `w`×`h`.
fn cut(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    let (cw, ch) = if iw as u64 * h as u64 > ih as u64 * w as u64 { ((ih as u64 * w as u64 / h as u64) as u32, ih) } else { (iw, (iw as u64 * h as u64 / w as u64) as u32) };
    let (cw, ch) = (cw.max(1), ch.max(1));
    let part = image::imageops::crop_imm(img, (iw - cw) / 2, (ih - ch) / 2, cw, ch).to_image();
    image::imageops::resize(&part, w, h, if cw < w { FilterType::CatmullRom } else { FilterType::Lanczos3 })
}

/// Fitted within `w`×`h` (never made bigger).
fn fit(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    if iw <= w && ih <= h {
        return img.clone();
    }
    let k = (w as f64 / iw as f64).min(h as f64 / ih as f64);
    image::imageops::resize(img, ((iw as f64 * k).round() as u32).max(1), ((ih as f64 * k).round() as u32).max(1), FilterType::Lanczos3)
}

/// On the site's dark (a picture's see-through parts would show the page through a JPEG as black anyway).
fn flat(img: &RgbaImage) -> RgbImage {
    RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let p = img.get_pixel(x, y).0;
        let a = p[3] as u32;
        let mix = |c: u8, bg: u32| ((c as u32 * a + bg * (255 - a)) / 255) as u8;
        image::Rgb([mix(p[0], 4), mix(p[1], 3), mix(p[2], 3)])
    })
}

fn jpeg(img: &RgbaImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(&flat(img)).map_err(|e| e.to_string())?;
    Ok(out)
}

fn banner_size(w: u32, h: u32) -> Result<(), String> {
    if w < 300 || h < 100 {
        return Err("the picture is too small for a banner (at least 300×100)".into());
    }
    Ok(())
}

/// A banner from a picture or a GIF.
pub fn banner(data: &[u8], media: &Path) -> Result<Made, String> {
    let (format, reader) = decode(data)?;
    if format == ImageFormat::Gif {
        let frames = gif_frames_with(data, BANNER_GIF_FRAMES, banner_size)?;
        if frames.len() > 1 {
            return moving_banner(&frames, media);
        }
        return still_banner(&frames[0].0, media);
    }
    let img = still_of(reader)?;
    banner_size(img.width(), img.height())?;
    still_banner(&img, media)
}

fn still_banner(img: &RgbaImage, media: &Path) -> Result<Made, String> {
    let out = cut(img, BANNER.0, BANNER.1);
    let bytes = jpeg(&out, 86)?;
    let name = keep(media, &bytes, "jpg")?;
    Ok(Made { info: json!({ "url": url(&name), "w": BANNER.0, "h": BANNER.1, "frames": 1 }), files: vec![name], bytes: bytes.len() })
}

fn moving_banner(frames: &[(RgbaImage, u32)], media: &Path) -> Result<Made, String> {
    // The GIF again, for the site.
    let cuts: Vec<(RgbaImage, u32)> = frames.iter().map(|(img, ms)| (cut(img, BANNER_GIF.0, BANNER_GIF.1), *ms)).collect();
    let mut gif = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut gif, 10);
        enc.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;
        enc.encode_frames(cuts.iter().map(|(img, ms)| Frame::from_parts(img.clone(), 0, 0, Delay::from_numer_denom_ms(*ms, 1)))).map_err(|e| e.to_string())?;
    }
    // The sheet, for the client: fewer frames (each held for the time of those it stands for), smaller.
    let step = cuts.len().div_ceil(BANNER_SHEET_FRAMES).max(1);
    let mut sheet_frames: Vec<(RgbaImage, u32)> = Vec::new();
    for group in cuts.chunks(step) {
        let ms: u32 = group.iter().map(|(_, ms)| *ms).sum();
        sheet_frames.push((image::imageops::resize(&group[0].0, BANNER_SHEET.0, BANNER_SHEET.1, FilterType::Triangle), ms));
    }
    let n = sheet_frames.len();
    let cols = n.min(6);
    let rows = n.div_ceil(cols);
    let mut sheet = RgbaImage::new(cols as u32 * BANNER_SHEET.0, rows as u32 * BANNER_SHEET.1);
    for (i, (img, _)) in sheet_frames.iter().enumerate() {
        image::imageops::replace(&mut sheet, img, ((i % cols) as u32 * BANNER_SHEET.0) as i64, ((i / cols) as u32 * BANNER_SHEET.1) as i64);
    }
    let sheet_bytes = png(&sheet)?;
    // A still of its first frame too: what a list or a weak page shows.
    let first = jpeg(&cut(&frames[0].0, BANNER.0, BANNER.1), 84)?;
    let gif_name = keep(media, &gif, "gif")?;
    let sheet_name = keep(media, &sheet_bytes, "png")?;
    let first_name = keep(media, &first, "jpg")?;
    let delays: Vec<u32> = sheet_frames.iter().map(|(_, ms)| *ms).collect();
    Ok(Made {
        info: json!({
            "url": url(&gif_name), "still": url(&first_name), "w": BANNER_GIF.0, "h": BANNER_GIF.1, "frames": cuts.len(),
            "sheet": { "url": url(&sheet_name), "frames": n, "cols": cols, "rows": rows, "w": BANNER_SHEET.0, "h": BANNER_SHEET.1, "delays": delays },
        }),
        files: vec![gif_name, sheet_name, first_name],
        bytes: gif.len() + sheet_bytes.len() + first.len(),
    })
}

/// A picture sent in a conversation: as a screenshot, and a moving GIF keeps moving — the GIF as it came (for the
/// site), and its frames on a sheet with their delays (for the game, which plays them), each frame at most 360 px.
pub fn chat_picture(data: &[u8], media: &Path) -> Result<Made, String> {
    let (format, _) = decode(data)?;
    let mut made = shot(data, media)?;
    if format != ImageFormat::Gif {
        return Ok(made);
    }
    let frames = gif_frames_with(data, 240, |w, h| if w.min(h) < 64 { Err("the picture is too small (at least 64 px)".into()) } else { Ok(()) })?;
    if frames.len() < 2 {
        return Ok(made);
    }
    // At most 40 on the sheet: each held for the time of those it stands for.
    let step = frames.len().div_ceil(40).max(1);
    let first = fit(&frames[0].0, 360, 360);
    let (fw, fh) = (first.width(), first.height());
    let mut sheet_frames: Vec<(RgbaImage, u32)> = Vec::new();
    for group in frames.chunks(step) {
        let ms: u32 = group.iter().map(|(_, ms)| *ms).sum();
        sheet_frames.push((image::imageops::resize(&group[0].0, fw, fh, FilterType::Triangle), ms.max(20)));
    }
    let n = sheet_frames.len();
    let cols = n.min(6);
    let rows = n.div_ceil(cols);
    let mut sheet = RgbaImage::new(cols as u32 * fw, rows as u32 * fh);
    for (i, (img, _)) in sheet_frames.iter().enumerate() {
        image::imageops::replace(&mut sheet, img, ((i % cols) as u32 * fw) as i64, ((i / cols) as u32 * fh) as i64);
    }
    let sheet_bytes = jpeg(&sheet, 84)?;
    let sheet_name = keep(media, &sheet_bytes, "jpg")?;
    let gif_name = keep(media, data, "gif")?;
    let delays: Vec<u32> = sheet_frames.iter().map(|(_, ms)| *ms).collect();
    if let Some(info) = made.info.as_object_mut() {
        info.insert("gif".into(), json!(url(&gif_name)));
        info.insert("frames".into(), json!(n));
        info.insert("sheet".into(), json!(url(&sheet_name)));
        info.insert("cols".into(), json!(cols));
        info.insert("rows".into(), json!(rows));
        info.insert("fw".into(), json!(fw));
        info.insert("fh".into(), json!(fh));
        info.insert("delays".into(), json!(delays));
    }
    made.bytes += sheet_bytes.len() + data.len();
    made.files.push(sheet_name);
    made.files.push(gif_name);
    Ok(made)
}

/// A screenshot for the gallery (a moving GIF is taken as its first frame).
pub fn shot(data: &[u8], media: &Path) -> Result<Made, String> {
    let (format, reader) = decode(data)?;
    let img = if format == ImageFormat::Gif {
        gif_frames_with(data, 1, |w, h| if w.min(h) < 64 { Err("the picture is too small (at least 64 px)".into()) } else { Ok(()) })?.remove(0).0
    } else {
        still_of(reader)?
    };
    if img.width().min(img.height()) < 64 {
        return Err("the picture is too small (at least 64 px)".into());
    }
    let big = fit(&img, SHOT.0, SHOT.1);
    let thumb = fit(&img, SHOT_THUMB.0, SHOT_THUMB.1);
    let big_bytes = jpeg(&big, 88)?;
    let thumb_bytes = jpeg(&thumb, 80)?;
    let big_name = keep(media, &big_bytes, "jpg")?;
    let thumb_name = keep(media, &thumb_bytes, "jpg")?;
    Ok(Made {
        info: json!({ "url": url(&big_name), "thumb": url(&thumb_name), "w": big.width(), "h": big.height() }),
        files: vec![big_name, thumb_name],
        bytes: big_bytes.len() + thumb_bytes.len(),
    })
}

// ------------------------------------------------------------------------------------------------ music

/// An MP3 made anew: its layer III frames only, as they were sent; how long it plays; the title and artist its tags
/// gave (for the form).
pub fn music(data: &[u8], media: &Path) -> Result<Made, String> {
    let (frames, ms, title, artist) = mp3(data)?;
    if ms > MOST_MUSIC_MS {
        return Err("the song is over 10 minutes".into());
    }
    let name = keep(media, &frames, "mp3")?;
    Ok(Made { info: json!({ "url": url(&name), "ms": ms, "tag_title": title, "tag_artist": artist }), files: vec![name], bytes: frames.len() })
}

const BITRATES_V1: [u32; 16] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0];
const BITRATES_V2: [u32; 16] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0];

/// A layer III frame's header at `b`: (its length, its samples, its sample rate).
fn frame_at(b: &[u8]) -> Option<(usize, u32, u32)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] & 0xE0 != 0xE0 {
        return None;
    }
    let version = (b[1] >> 3) & 3; // 0: MPEG 2.5, 2: MPEG 2, 3: MPEG 1
    let layer = (b[1] >> 1) & 3; // 1: layer III
    if version == 1 || layer != 1 {
        return None;
    }
    let bitrate_index = (b[2] >> 4) as usize;
    let rate_index = ((b[2] >> 2) & 3) as usize;
    if bitrate_index == 0 || bitrate_index == 15 || rate_index == 3 {
        return None;
    }
    let padding = ((b[2] >> 1) & 1) as usize;
    let base = [44_100, 48_000, 32_000][rate_index];
    let (rate, kbps, samples) = match version {
        3 => (base, BITRATES_V1[bitrate_index], 1152),
        2 => (base / 2, BITRATES_V2[bitrate_index], 576),
        _ => (base / 4, BITRATES_V2[bitrate_index], 576),
    };
    let len = (samples as usize / 8) * kbps as usize * 1000 / rate as usize + padding;
    (len >= 4).then_some((len, samples, rate))
}

fn syncsafe(b: &[u8]) -> usize {
    b.iter().take(4).fold(0usize, |n, x| (n << 7) | (*x as usize & 0x7F))
}

/// The text of an ID3v2 text frame (its first byte says how it is written).
fn id3_text(b: &[u8]) -> String {
    let Some((&enc, rest)) = b.split_first() else { return String::new() };
    let text = match enc {
        1 | 2 => {
            let (rest, big) = match rest {
                [0xFE, 0xFF, r @ ..] => (r, true),
                [0xFF, 0xFE, r @ ..] => (r, false),
                r => (r, enc == 2),
            };
            let units: Vec<u16> = rest.chunks_exact(2).map(|p| if big { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) }).collect();
            String::from_utf16_lossy(&units)
        }
        3 => String::from_utf8_lossy(rest).into_owned(),
        _ => rest.iter().map(|&c| c as char).collect(),
    };
    crate::market::clean_line(&text.replace('\0', " "), 120)
}

/// The title and artist of an ID3v2 tag (versions 3 and 4).
fn id3_names(tag: &[u8], version: u8) -> (String, String) {
    let (mut title, mut artist) = (String::new(), String::new());
    let mut at = 0;
    while at + 10 <= tag.len() {
        let id = &tag[at..at + 4];
        if id[0] == 0 {
            break;
        }
        let size = if version >= 4 { syncsafe(&tag[at + 4..at + 8]) } else { u32::from_be_bytes([tag[at + 4], tag[at + 5], tag[at + 6], tag[at + 7]]) as usize };
        let body = at + 10;
        if size == 0 || body + size > tag.len() {
            break;
        }
        match id {
            b"TIT2" => title = id3_text(&tag[body..body + size]),
            b"TPE1" => artist = id3_text(&tag[body..body + size]),
            _ => {}
        }
        at = body + size;
    }
    (title, artist)
}

/// The frames of an MP3, how long they play (ms), and the names its tags gave.
fn mp3(data: &[u8]) -> Result<(Vec<u8>, u64, String, String), String> {
    let mut at = 0;
    let (mut title, mut artist) = (String::new(), String::new());
    // Tags at its start (there may be more than one).
    while data.len() >= at + 10 && &data[at..at + 3] == b"ID3" {
        let version = data[at + 3];
        let size = syncsafe(&data[at + 6..at + 10]);
        let footer = if data[at + 5] & 0x10 != 0 { 10 } else { 0 };
        let end = at + 10 + size + footer;
        if end > data.len() {
            return Err("the song's tag is cut short".into());
        }
        if title.is_empty() && (3..=4).contains(&version) {
            (title, artist) = id3_names(&data[at + 10..at + 10 + size], version);
        }
        at = end;
    }
    let mut out = Vec::with_capacity(data.len() - at);
    let mut samples: u64 = 0;
    let mut rate = 0u32;
    let mut frames = 0usize;
    // Whatever stands before the first frame (a few stray bytes) is passed over.
    let start = (at..data.len().min(at + 4096)).find(|&i| frame_at(&data[i..]).is_some_and(|(len, _, _)| frame_at(&data[(i + len).min(data.len())..]).is_some()));
    let Some(mut i) = start else { return Err("this is not an MP3".into()) };
    while i < data.len() {
        match frame_at(&data[i..]) {
            Some((len, n, r)) if i + len <= data.len() => {
                out.extend_from_slice(&data[i..i + len]);
                samples += n as u64;
                rate = r;
                frames += 1;
                i += len;
            }
            _ => break,
        }
    }
    // Only tags may follow the frames (ID3v1, APE, Lyrics3): anything else is not an MP3 sent whole.
    let rest = data.len() - i;
    if frames < 10 || (rest > 128 + 4096 && rest * 50 > data.len()) {
        return Err("this is not an MP3".into());
    }
    let ms = if rate == 0 { 0 } else { samples * 1000 / rate as u64 };
    Ok((out, ms, title, artist))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cursed-media-test-{}", std::process::id()));
        std::fs::create_dir_all(d.join("pictures")).unwrap();
        d
    }

    fn a_png(w: u32, h: u32) -> Vec<u8> {
        png(&RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 90, 255]))).unwrap()
    }

    /// A silent MPEG-1 layer III stream: 128 kbps, 44.1 kHz frames (417 bytes, no padding), with tags around it.
    fn an_mp3(frames: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let title = b"\x03Dark Song";
        let artist = b"\x03Lunuca";
        let mut tag = Vec::new();
        for (id, body) in [(b"TIT2", &title[..]), (b"TPE1", &artist[..])] {
            tag.extend_from_slice(id);
            tag.extend_from_slice(&(body.len() as u32).to_be_bytes());
            tag.extend_from_slice(&[0, 0]);
            tag.extend_from_slice(body);
        }
        out.extend_from_slice(b"ID3\x03\x00\x00");
        let n = tag.len();
        out.extend_from_slice(&[((n >> 21) & 0x7F) as u8, ((n >> 14) & 0x7F) as u8, ((n >> 7) & 0x7F) as u8, (n & 0x7F) as u8]);
        out.extend_from_slice(&tag);
        for _ in 0..frames {
            let mut f = vec![0u8; 417];
            f[0] = 0xFF;
            f[1] = 0xFB;
            f[2] = 0x90;
            f[3] = 0x64;
            out.extend_from_slice(&f);
        }
        out.extend_from_slice(b"TAG");
        out.extend_from_slice(&[0u8; 125]);
        out
    }

    #[test]
    fn an_mp3_keeps_its_frames_only_and_says_its_names() {
        let (frames, ms, title, artist) = mp3(&an_mp3(100)).unwrap();
        assert_eq!(frames.len(), 100 * 417);
        assert_eq!(&frames[..2], &[0xFF, 0xFB]);
        assert_eq!(ms, 100 * 1152 * 1000 / 44_100);
        assert_eq!((title.as_str(), artist.as_str()), ("Dark Song", "Lunuca"));
    }

    #[test]
    fn what_is_not_an_mp3_is_refused() {
        assert!(mp3(b"hello, this is not music at all").is_err());
        assert!(mp3(&a_png(64, 64)).is_err());
        let mut half = an_mp3(100);
        half.extend(std::iter::repeat(7u8).take(40_000));
        assert!(mp3(&half).is_err());
    }

    #[test]
    fn a_banner_is_three_to_one_and_a_shot_keeps_its_shape() {
        let d = dir();
        let b = banner(&a_png(1200, 900), &d).unwrap();
        assert_eq!((b.info["w"].as_u64(), b.info["h"].as_u64()), (Some(1500), Some(500)));
        assert!(banner(&a_png(200, 80), &d).is_err());
        let s = shot(&a_png(2560, 1440), &d).unwrap();
        assert_eq!((s.info["w"].as_u64(), s.info["h"].as_u64()), (Some(1920), Some(1080)));
    }

    #[test]
    fn a_moving_banner_has_a_gif_a_sheet_and_a_still() {
        let mut gif = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut gif);
            enc.set_repeat(Repeat::Infinite).unwrap();
            let frames = (0..60).map(|i| Frame::from_parts(RgbaImage::from_pixel(600, 200, image::Rgba([(i * 4) as u8, 20, 200, 255])), 0, 0, Delay::from_numer_denom_ms(50, 1)));
            enc.encode_frames(frames).unwrap();
        }
        let b = banner(&gif, &dir()).unwrap();
        assert!(b.info["url"].as_str().unwrap().ends_with(".gif"));
        assert!(b.info["sheet"]["frames"].as_u64().unwrap() <= BANNER_SHEET_FRAMES as u64);
        assert_eq!(b.files.len(), 3);
    }
}

// ------------------------------------------------------------------------------------------------ voice

/// A voice message as it was recorded: Opus in Ogg (the client, Firefox) or in WebM (Chrome and its kin), kept as it
/// came. An Ogg says how long it plays; a WebM of a recorder does not, so the length its sender measured is taken.
pub fn voice(data: &[u8], said_ms: i64, media: &Path) -> Result<Made, String> {
    let (ext, ms) = if data.starts_with(b"OggS") {
        ("ogg", ogg_opus_ms(data)?)
    } else if data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        let head = &data[..data.len().min(4096)];
        if !contains(head, b"webm") || !contains(&data[..data.len().min(64 * 1024)], b"A_OPUS") {
            return Err("a voice message is Opus in Ogg or WebM".into());
        }
        if said_ms <= 0 {
            return Err("the voice message's length is missing".into());
        }
        ("webm", said_ms)
    } else {
        return Err("a voice message is Opus in Ogg or WebM".into());
    };
    if ms < 300 {
        return Err("the voice message is too short".into());
    }
    if ms > MOST_VOICE_MS {
        return Err("a voice message is three minutes at most".into());
    }
    let name = keep(media, data, ext)?;
    Ok(Made { info: json!({ "url": url(&name), "ms": ms, "format": ext }), files: vec![name], bytes: data.len() })
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// How long an Ogg Opus plays: its last page's granule position (48 kHz) less the pre-skip of its head.
fn ogg_opus_ms(data: &[u8]) -> Result<i64, String> {
    let bad = || "a voice message is Opus in Ogg or WebM".to_string();
    let mut at = 0usize;
    let mut pre_skip: Option<u64> = None;
    let mut last_granule: u64 = 0;
    while at + 27 <= data.len() {
        if &data[at..at + 4] != b"OggS" {
            return Err(bad());
        }
        let granule = u64::from_le_bytes(data[at + 6..at + 14].try_into().map_err(|_| bad())?);
        let segments = data[at + 26] as usize;
        let lacing_end = at + 27 + segments;
        if lacing_end > data.len() {
            return Err(bad());
        }
        let body: usize = data[at + 27..lacing_end].iter().map(|&b| b as usize).sum();
        let end = lacing_end + body;
        if end > data.len() {
            return Err(bad());
        }
        if pre_skip.is_none() {
            let packet = &data[lacing_end..end];
            if packet.len() < 19 || &packet[..8] != b"OpusHead" {
                return Err(bad());
            }
            pre_skip = Some(u16::from_le_bytes([packet[10], packet[11]]) as u64);
        }
        if granule != u64::MAX {
            last_granule = granule;
        }
        at = end;
    }
    let pre = pre_skip.ok_or_else(bad)?;
    Ok((last_granule.saturating_sub(pre) * 1000 / 48_000) as i64)
}
