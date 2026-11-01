//! Avatars: a member's own picture in place of their skin's head (the client sends the head of the player's skin; it is
//! the avatar until they choose their own, and again when they take theirs away; with neither, the head drawn from
//! their login). Whatever comes in is decoded and made anew — nothing of the sent bytes is ever served:
//! - a still picture (PNG, JPEG, WebP, or a GIF of one frame): cut square from its middle, 256×256, a PNG;
//! - a moving GIF: cut square, every frame 128×128, as a GIF again (for the site) and as a sheet of its frames with
//!   their times (for the client, which draws the frames itself). Frames closer than 40 ms are shown together (one
//!   every 40 ms keeps its motion), and a long one is kept to 240 frames (each shown longer, the same time in all).
//! - the skin's head: its face with the hat over it, 8×8, made 256×256 without smoothing.
//!
//! Limits: 6 MB sent, at least 32 px, at most 4096 px a side, at most 2000 frames sent, 30 changes a day.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::post;
use axum::Router;
use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, Delay, Frame, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits, RgbaImage};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::market::member;
use crate::{db, fail, now, Shared};

pub const MOST_BYTES: usize = 6 << 20;
const STILL: u32 = 256;
const FRAME: u32 = 128;
/// Frames sent, at most; frames kept (and the least time a kept frame is shown, ms).
const MOST_SENT_FRAMES: usize = 2000;
const MOST_FRAMES: usize = 240;
const LEAST_STEP: u32 = 40;
const MOST_SIDE: u32 = 4096;
const LEAST_SIDE: u32 = 32;
const COLS: usize = 8;
const PER_DAY: u32 = 30;

/// Each member's avatar as the site and the client get it (kept here, so every list of people has it without a join);
/// and the heads of their skins.
static KNOWN: OnceLock<Mutex<HashMap<i64, Value>>> = OnceLock::new();
static SKINS: OnceLock<Mutex<HashMap<i64, Value>>> = OnceLock::new();
static PACE: OnceLock<Mutex<HashMap<i64, (i64, u32)>>> = OnceLock::new();

fn known() -> std::sync::MutexGuard<'static, HashMap<i64, Value>> {
    KNOWN.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner())
}

fn skins() -> std::sync::MutexGuard<'static, HashMap<i64, Value>> {
    SKINS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner())
}

/// The column, and every avatar read into memory.
pub fn schema(c: &Connection) {
    let has: bool = c
        .prepare("SELECT 1 FROM pragma_table_info('users') WHERE name = 'avatar'")
        .and_then(|mut q| q.exists([]))
        .unwrap_or(false);
    if !has {
        c.execute("ALTER TABLE users ADD COLUMN avatar TEXT", []).expect("the avatar column");
    }
    let has_skin: bool = c
        .prepare("SELECT 1 FROM pragma_table_info('users') WHERE name = 'skin_avatar'")
        .and_then(|mut q| q.exists([]))
        .unwrap_or(false);
    if !has_skin {
        c.execute("ALTER TABLE users ADD COLUMN skin_avatar TEXT", []).expect("the skin avatar column");
    }
    for (column, mut map) in [("avatar", known()), ("skin_avatar", skins())] {
        map.clear();
        if let Ok(mut q) = c.prepare(&format!("SELECT uid, {column} FROM users WHERE {column} IS NOT NULL")) {
            if let Ok(rows) = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
                for (uid, text) in rows.flatten() {
                    if let Ok(v) = serde_json::from_str::<Value>(&text) {
                        map.insert(uid, public(&v));
                    }
                }
            }
        }
    }
}

/// Avatars whose files are not in the media folder any more (lost from it, 04.10: 61 skins' heads) are not handed out:
/// the head drawn from the login stands in, and the client's next word of its skin makes the file again (keep writes
/// it, and the head is not "the same again" while it is forgotten here). Read once at the start, the media known.
pub fn forget_missing(c: &Connection, media: &Path) -> usize {
    let mut gone = 0;
    for column in ["avatar", "skin_avatar"] {
        let Ok(mut q) = c.prepare(&format!("SELECT uid, {column} FROM users WHERE {column} IS NOT NULL")) else { continue };
        let Ok(rows) = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) else { continue };
        for (uid, text) in rows.flatten() {
            let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
            let lost = v["files"].as_array().is_some_and(|files| {
                files.iter().filter_map(Value::as_str).any(|f| f.len() > 2 && !media.join("pictures").join(&f[..2]).join(f).exists())
            });
            if lost {
                if column == "avatar" { known() } else { skins() }.remove(&uid);
                gone += 1;
            }
        }
    }
    gone
}

/// What is shown of an avatar (the names of its files stay here).
fn public(v: &Value) -> Value {
    let mut p = v.clone();
    if let Some(o) = p.as_object_mut() {
        o.remove("files");
    }
    p
}

/// A member's avatar: their own, else their skin's head, else null (the head drawn from their login).
pub fn of(uid: i64) -> Value {
    if let Some(own) = known().get(&uid) {
        return own.clone();
    }
    skins().get(&uid).cloned().unwrap_or(Value::Null)
}

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/avatar", post(set).delete(clear).layer(DefaultBodyLimit::max(MOST_BYTES + 1024)))
        .route("/api/avatar/skin", post(skin).layer(DefaultBodyLimit::max(4096)))
}

fn paced(uid: i64) -> bool {
    let day = now() / 86_400;
    let mut p = PACE.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner());
    let e = p.entry(uid).or_insert((day, 0));
    if e.0 != day {
        *e = (day, 0);
    }
    if e.1 >= PER_DAY {
        return false;
    }
    e.1 += 1;
    true
}

async fn set(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Response {
    let w = match member(&app, &h, true).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    if body.is_empty() {
        return bad("send a picture");
    }
    if body.len() > MOST_BYTES {
        return bad("the picture is over 6 MB");
    }
    if !paced(w.uid) {
        return fail(StatusCode::TOO_MANY_REQUESTS, "slow");
    }
    let media = app.market.media.clone();
    let made = tokio::task::spawn_blocking(move || make(&body, &media)).await;
    let v = match made {
        Ok(Ok(v)) => v,
        Ok(Err(message)) => return bad(&message),
        Err(_) => return fail(StatusCode::INTERNAL_SERVER_ERROR, "busy"),
    };
    let uid = w.uid;
    let text = v.to_string();
    let saved = db(&app, move |c| c.execute("UPDATE users SET avatar = ?2 WHERE uid = ?1", params![uid, text])).await;
    if saved.is_err() {
        return fail(StatusCode::INTERNAL_SERVER_ERROR, "busy");
    }
    let shown = public(&v);
    known().insert(uid, shown.clone());
    Json(json!({ "avatar": shown })).into_response()
}

async fn clear(State(app): State<Shared>, h: HeaderMap) -> Response {
    let w = match member(&app, &h, true).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    let uid = w.uid;
    if db(&app, move |c| c.execute("UPDATE users SET avatar = NULL WHERE uid = ?1", params![uid])).await.is_err() {
        return fail(StatusCode::INTERNAL_SERVER_ERROR, "busy");
    }
    known().remove(&uid);
    Json(json!({ "avatar": of(uid) })).into_response()
}

/// The client's word of the player's skin: its head's 64 pixels (face with the hat over it), RGBA, as hex.
async fn skin(State(app): State<Shared>, h: HeaderMap, Json(body): Json<Value>) -> Response {
    let w = match member(&app, &h, true).await {
        Ok(w) => w,
        Err(r) => return r,
    };
    let Some(hex) = body["rgba"].as_str() else { return bad("send the head's pixels") };
    let v = match skin_head(hex, &app.market.media) {
        Ok(v) => v,
        Err(message) => return bad(&message),
    };
    let uid = w.uid;
    // The same head again: nothing to do (the client says it each time it starts).
    if skins().get(&uid).is_some_and(|known| known["url"] == v["url"]) {
        return Json(json!({ "avatar": of(uid) })).into_response();
    }
    if !paced(uid) {
        return fail(StatusCode::TOO_MANY_REQUESTS, "slow");
    }
    let text = v.to_string();
    let saved = db(&app, move |c| c.execute("UPDATE users SET skin_avatar = ?2 WHERE uid = ?1", params![uid, text])).await;
    if saved.is_err() {
        return fail(StatusCode::INTERNAL_SERVER_ERROR, "busy");
    }
    skins().insert(uid, public(&v));
    Json(json!({ "avatar": of(uid) })).into_response()
}

/// A skin's head from its 64 pixels (hex of RGBA): 256×256, each pixel a square of 32, a PNG.
pub fn skin_head(hex: &str, media: &Path) -> Result<Value, String> {
    if hex.len() != 8 * 8 * 4 * 2 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("the head is 8×8 RGBA, as hex".into());
    }
    let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0)).collect();
    if bytes.chunks(4).all(|p| p[3] == 0) {
        return Err("the head is empty".into());
    }
    let head = RgbaImage::from_raw(8, 8, bytes).ok_or("the head is 8×8")?;
    let big = image::imageops::resize(&head, STILL, STILL, FilterType::Nearest);
    let png_bytes = png(&big)?;
    let name = keep(media, &png_bytes, "png")?;
    Ok(json!({ "url": url(&name), "frames": 1, "size": STILL, "skin": true, "files": [name] }))
}

fn bad(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": "picture", "message": message }))).into_response()
}

// ------------------------------------------------------------------------------------------------ the pictures

pub(crate) fn limits() -> Limits {
    let mut lim = Limits::default();
    lim.max_image_width = Some(MOST_SIDE);
    lim.max_image_height = Some(MOST_SIDE);
    lim.max_alloc = Some(256 << 20);
    lim
}

/// The middle square of a picture, `side` px: pixel art (a small picture) scaled up by whole steps without smoothing,
/// anything else smoothly.
fn square(img: &RgbaImage, side: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let s = w.min(h);
    let cut = image::imageops::crop_imm(img, (w - s) / 2, (h - s) / 2, s, s).to_image();
    if s == side {
        return cut;
    }
    let filter = if s < side && side % s == 0 { FilterType::Nearest } else if s < side { FilterType::CatmullRom } else { FilterType::Lanczos3 };
    image::imageops::resize(&cut, side, side, filter)
}

pub(crate) fn png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out).write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Kept under its hash among the market's pictures (served from /media/); its name.
pub(crate) fn keep(media: &Path, bytes: &[u8], ext: &str) -> Result<String, String> {
    let hash: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    let name = format!("{}.{ext}", &hash[..40]);
    let dir = media.join("pictures").join(&name[..2]);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&name);
    if !path.exists() {
        let part = dir.join(format!("{name}.part"));
        std::fs::write(&part, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&part, &path).map_err(|e| e.to_string())?;
    }
    Ok(name)
}

pub(crate) fn url(name: &str) -> String {
    format!("/media/{}/{}", &name[..2], name)
}

/// The picture sent, made anew: what is kept of the avatar (its files, and what the site and the client need).
pub fn make(data: &[u8], media: &Path) -> Result<Value, String> {
    let reader = ImageReader::new(Cursor::new(data)).with_guessed_format().map_err(|_| "the picture can't be read".to_string())?;
    match reader.format() {
        Some(ImageFormat::Gif) => {
            let frames = gif_frames(data)?;
            if frames.len() == 1 {
                return still(&frames[0].0, media);
            }
            moving(&frames, media)
        }
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP) => {
            let mut r = reader;
            r.limits(limits());
            let img = r.decode().map_err(|e| format!("the picture doesn't decode ({e})"))?;
            still(&img.to_rgba8(), media)
        }
        _ => Err("send a PNG, JPEG, WebP or GIF".into()),
    }
}

fn check_size(w: u32, h: u32) -> Result<(), String> {
    if w.min(h) < LEAST_SIDE {
        return Err(format!("the picture is too small (at least {LEAST_SIDE} px)"));
    }
    if w > h * 3 || h > w * 3 {
        return Err("the picture is too narrow for a square".into());
    }
    Ok(())
}

fn still(img: &RgbaImage, media: &Path) -> Result<Value, String> {
    check_size(img.width(), img.height())?;
    let out = square(img, STILL);
    let bytes = png(&out)?;
    let name = keep(media, &bytes, "png")?;
    Ok(json!({ "url": url(&name), "frames": 1, "size": STILL, "files": [name] }))
}

/// A frame's time as a browser shows it: less than 20 ms is shown for 100.
fn frame_ms(f: &Frame) -> u32 {
    let (n, d) = f.delay().numer_denom_ms();
    let ms = if d == 0 { 100 } else { n / d };
    if ms < 20 {
        100
    } else {
        ms.min(10_000)
    }
}

fn gif_decoder(data: &[u8]) -> Result<GifDecoder<Cursor<&[u8]>>, String> {
    let mut dec = GifDecoder::new(Cursor::new(data)).map_err(|e| format!("the GIF doesn't decode ({e})"))?;
    dec.set_limits(limits()).map_err(|_| "the GIF is too big (at most 4096 px a side)".to_string())?;
    Ok(dec)
}

/// A GIF's frames, whole (each drawn over the ones before, as a browser shows them), with their times (ms): frames
/// closer than the step shown together (the last of them for all their time), the step long enough for at most
/// MOST_FRAMES in the whole.
fn gif_frames(data: &[u8]) -> Result<Vec<(RgbaImage, u32)>, String> {
    gif_frames_with(data, MOST_FRAMES, check_size)
}

/// The same for any picture (media.rs: banners), with its own count of frames and its own check of the size.
pub(crate) fn gif_frames_with(data: &[u8], most_frames: usize, check: fn(u32, u32) -> Result<(), String>) -> Result<Vec<(RgbaImage, u32)>, String> {
    // First its times only (each frame let go as soon as it is read).
    let dec = gif_decoder(data)?;
    let (w, h) = dec.dimensions();
    check(w, h)?;
    let mut times = Vec::new();
    for f in dec.into_frames() {
        let f = f.map_err(|e| format!("the GIF doesn't decode ({e})"))?;
        if times.len() >= MOST_SENT_FRAMES {
            return Err(format!("the GIF has too many frames (at most {MOST_SENT_FRAMES})"));
        }
        times.push(frame_ms(&f));
    }
    if times.is_empty() {
        return Err("the GIF has no frames".into());
    }
    let total: u32 = times.iter().sum();
    let step = LEAST_STEP.max(total.div_ceil(most_frames as u32));
    // Then the frames kept.
    let mut out: Vec<(RgbaImage, u32)> = Vec::new();
    let mut held = 0;
    let last = times.len() - 1;
    for (i, f) in gif_decoder(data)?.into_frames().enumerate() {
        let f = f.map_err(|e| format!("the GIF doesn't decode ({e})"))?;
        held += times.get(i).copied().unwrap_or(100);
        if held >= step || i == last {
            out.push((f.into_buffer(), held));
            held = 0;
        }
    }
    if out.len() > most_frames {
        out.truncate(most_frames);
    }
    Ok(out)
}

fn moving(frames: &[(RgbaImage, u32)], media: &Path) -> Result<Value, String> {
    let squares: Vec<(RgbaImage, u32)> = frames.iter().map(|(img, ms)| (square(img, FRAME), *ms)).collect();
    // The GIF again, for the site.
    let mut gif = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut gif, 10);
        enc.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;
        enc.encode_frames(squares.iter().map(|(img, ms)| Frame::from_parts(img.clone(), 0, 0, Delay::from_numer_denom_ms(*ms, 1))))
            .map_err(|e| e.to_string())?;
    }
    // The sheet, for the client: frames in rows of eight.
    let n = squares.len();
    let cols = n.min(COLS);
    let rows = n.div_ceil(cols);
    let mut sheet = RgbaImage::new(cols as u32 * FRAME, rows as u32 * FRAME);
    for (i, (img, _)) in squares.iter().enumerate() {
        image::imageops::replace(&mut sheet, img, ((i % cols) as u32 * FRAME) as i64, ((i / cols) as u32 * FRAME) as i64);
    }
    let sheet_bytes = png(&sheet)?;
    let gif_name = keep(media, &gif, "gif")?;
    let sheet_name = keep(media, &sheet_bytes, "png")?;
    let delays: Vec<u32> = squares.iter().map(|(_, ms)| *ms).collect();
    Ok(json!({
        "url": url(&gif_name), "sheet": url(&sheet_name), "frames": n, "cols": cols, "rows": rows, "size": FRAME,
        "delays": delays, "files": [gif_name, sheet_name],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cursed-avatar-test-{}", std::process::id()));
        std::fs::create_dir_all(d.join("pictures")).unwrap();
        d
    }

    fn a_png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 90, 255]));
        png(&img).unwrap()
    }

    fn a_gif(n: usize, side: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut out);
            enc.set_repeat(Repeat::Infinite).unwrap();
            let frames = (0..n).map(|i| Frame::from_parts(RgbaImage::from_pixel(side, side, image::Rgba([(i * 40) as u8, 20, 200, 255])), 0, 0, Delay::from_numer_denom_ms(if i == 0 { 10 } else { 70 }, 1)));
            enc.encode_frames(frames).unwrap();
        }
        out
    }

    #[test]
    fn a_still_picture_becomes_a_square_png() {
        let v = make(&a_png(300, 200), &dir()).unwrap();
        assert_eq!(v["frames"], 1);
        assert_eq!(v["size"], STILL);
        assert!(v["url"].as_str().unwrap().starts_with("/media/"));
    }

    #[test]
    fn a_moving_gif_keeps_moving() {
        let v = make(&a_gif(10, 64), &dir()).unwrap();
        assert_eq!(v["frames"], 10);
        assert_eq!(v["cols"], 8);
        assert_eq!(v["rows"], 2);
        let delays: Vec<u64> = v["delays"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap()).collect();
        assert_eq!(delays[0], 100, "a frame under 20 ms is shown for 100, as browsers do");
        assert_eq!(delays[1], 70);
        assert!(v["url"].as_str().unwrap().ends_with(".gif") && v["sheet"].as_str().unwrap().ends_with(".png"));
    }

    #[test]
    fn what_is_not_allowed() {
        assert!(make(b"not a picture at all", &dir()).is_err());
        assert!(make(&a_png(20, 20), &dir()).unwrap_err().contains("too small"));
        assert!(make(&a_png(400, 100), &dir()).unwrap_err().contains("narrow"));
    }

    #[test]
    fn a_long_quick_gif_is_kept_with_its_time() {
        // 372 frames of 20 ms (hers): shown two at a time, 40 ms each, the same seven and a half seconds.
        let mut out = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut out);
            enc.set_repeat(Repeat::Infinite).unwrap();
            let frames = (0..372).map(|i| Frame::from_parts(RgbaImage::from_pixel(40, 40, image::Rgba([(i % 255) as u8, 20, 200, 255])), 0, 0, Delay::from_numer_denom_ms(20, 1)));
            enc.encode_frames(frames).unwrap();
        }
        let v = make(&out, &dir()).unwrap();
        assert_eq!(v["frames"], 186);
        let delays: Vec<u64> = v["delays"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap()).collect();
        assert!(delays.iter().all(|d| *d == 40));
        assert_eq!(delays.iter().sum::<u64>(), 372 * 20);
    }

    #[test]
    fn a_very_long_gif_is_kept_to_its_most_frames() {
        let v = make(&a_gif(1000, 40), &dir()).unwrap();
        assert!(v["frames"].as_u64().unwrap() <= MOST_FRAMES as u64);
    }

    /// A real picture through it (CURSED_TEST_PICTURE=<file>): what comes out.
    #[test]
    #[ignore]
    fn a_picture_from_the_disk() {
        let path = std::env::var("CURSED_TEST_PICTURE").expect("CURSED_TEST_PICTURE");
        let data = std::fs::read(&path).unwrap();
        let v = make(&data, &dir()).unwrap();
        let files: Vec<String> = v["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect();
        for f in &files {
            let len = std::fs::metadata(dir().join("pictures").join(&f[..2]).join(f)).unwrap().len();
            println!("{f}: {len} bytes");
        }
        println!("frames {} cols {} rows {} delays {:?}", v["frames"], v["cols"], v["rows"], v["delays"].as_array().map(|d| d.len()));
    }

    #[test]
    fn a_skins_head_is_made_big_and_sharp() {
        let hex: String = (0..64).map(|i| format!("{:02x}{:02x}{:02x}ff", i * 3, 100, 200 - i)).collect();
        let v = skin_head(&hex, &dir()).unwrap();
        assert_eq!(v["skin"], true);
        assert_eq!(v["size"], STILL);
        assert!(skin_head("00", &dir()).is_err());
        assert!(skin_head(&"00".repeat(256), &dir()).unwrap_err().contains("empty"));
    }
}
