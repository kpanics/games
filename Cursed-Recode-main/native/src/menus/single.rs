//! Singleplayer (the preview's): her worlds as the menu's words with what they are under each; the chosen one's
//! picture opens from a line of light and drifts slowly, its numbers under it counting up; playing, renaming in place,
//! a backup, the folder, deleting held (Undo for a while), a new world.

use super::controls::{hid, hid_kind, hid_n, k, Field};
use super::lang::{tr, trf};
use super::ui::{clamp01, ease, ease_io, ease_out, smooth, toward, tw, WordStyle, BONE, CAP, DIM};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::{fonts, Face};
use serde_json::Value;

const A_RENAME: u32 = 1;
const A_BACKUP: u32 = 2;
const A_FOLDER: u32 = 3;
const A_DELETE: u32 = 4;
const A_PLAY: u32 = 5;
const A_NEW: u32 = 6;
const A_UNDO: u32 = 7;

pub struct Single {
    sel: i64,
    /// When the picture of the chosen world began to show; the picture before it (its key) going.
    shown_at: f32,
    first: bool,
    old_pic: Option<(String, bool)>,
    scroll: f32,
    scroll_to: f32,
    rename: Option<Field>,
    hold_from: f32,
    said_at: f32,
    said_was: String,
    /// The numbers counting up to what they are (by the chosen world's folder).
    count_from: f32,
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn n(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(-1)
}
fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn size_text(bytes: f64) -> String {
    if bytes < 0.0 {
        "—".into()
    } else if bytes > 1e9 {
        format!("{:.2} GB", bytes / 1e9)
    } else if bytes > 1e6 {
        format!("{} MB", (bytes / 1e6).round())
    } else {
        format!("{} KB", (bytes / 1e3).round().max(1.0))
    }
}

pub fn played_text(ticks: f64) -> String {
    if ticks < 0.0 {
        return "—".into();
    }
    let s = ticks / 20.0;
    if s >= 3600.0 {
        trf("hours", &[&format!("{}", (s / 3600.0).round())])
    } else if s >= 60.0 {
        trf("minutes", &[&format!("{}", (s / 60.0).round())])
    } else {
        trf("seconds", &[&format!("{}", s.round())])
    }
}

/// When, as days and a time: today, yesterday, or the date.
pub fn ago(ms: i64) -> String {
    if ms <= 0 {
        return "—".into();
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    // local time: the offset of Windows' time zone
    let off = local_offset_ms();
    let day = |t: i64| (t + off).div_euclid(86_400_000);
    let diff = day(now) - day(ms);
    let mins = ((ms + off).rem_euclid(86_400_000)) / 60_000;
    let hm = format!("{:02}:{:02}", mins / 60, mins % 60);
    match diff {
        0 => format!("{} {hm}", tr("today")),
        1 => format!("{} {hm}", tr("yesterday")),
        _ => {
            let (y, m, d) = civil(day(ms));
            let _ = y;
            let months_en = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            let months_ru = ["янв", "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"];
            let mn = if super::lang::ru() { months_ru[(m - 1) as usize] } else { months_en[(m - 1) as usize] };
            format!("{d} {mn} {hm}")
        }
    }
}

fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn local_offset_ms() -> i64 {
    unsafe {
        let mut tz: windows_sys::Win32::System::Time::TIME_ZONE_INFORMATION = std::mem::zeroed();
        let r = windows_sys::Win32::System::Time::GetTimeZoneInformation(&mut tz);
        let bias = tz.Bias + if r == 2 { tz.DaylightBias } else { 0 };
        -(bias as i64) * 60_000
    }
}

impl Single {
    pub fn new(data: &Value) -> Single {
        Single { sel: n(data, "sel"), shown_at: -10.0, first: true, old_pic: None, scroll: 0.0, scroll_to: 0.0, rename: None, hold_from: -1.0, said_at: -10.0, said_was: String::new(), count_from: -10.0 }
    }

    pub fn typing(&self) -> bool {
        self.rename.is_some()
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        let vh = m.vh();
        let vw = w / 100.0;
        let narrow = w < 900.0 * ui;
        let left = if narrow { 20.0 * ui } else { 6.2 * vw };
        let right = w - if narrow { 20.0 * ui } else { 6.0 * vw };
        let live = v.leaving.is_none() && m.now - v.opened > 0.3;
        m.scene_pose([26.0, 19.0, -0.055, 0.05, m.scene_to[4]], 0.16, 0.35);
        let shade = m.vis(v, 0.0);
        c.veil([0.0, 12.0 * vh, 40.0 * vw, h], grey(0.0, 0.34 * shade), 14.0 * vh, 1.0);
        let empty = Vec::new();
        let worlds = v.data.get("worlds").and_then(Value::as_array).unwrap_or(&empty);
        // the game's choice is the page's (a click selects there first)
        let gsel = n(v.data, "sel");
        if gsel != self.sel && gsel >= 0 {
            self.sel = gsel;
            self.shown_at = m.now;
            self.count_from = m.now;
        }
        if self.sel < 0 && !worlds.is_empty() {
            self.sel = 0;
            m.ask("select 0");
        }
        if self.first && !worlds.is_empty() && v.leaving.is_none() {
            self.first = false;
            self.shown_at = m.now + 0.25;
            self.count_from = m.now + 0.3;
        }
        // the title, and what is there: how many worlds, how much on the disk
        let tsize = m.cl(34.0, 120.0, 7.6);
        m.title(c, v, &tr("single").to_uppercase(), left, 6.0 * vh, tsize);
        let cap = m.cl(9.0, 18.0, 1.15);
        let total: f64 = worlds.iter().map(|wd| wd.get("size").and_then(Value::as_f64).unwrap_or(0.0).max(0.0)).sum();
        let count = if worlds.len() == 1 { tr("world1").to_string() } else { trf("worlds", &[&worlds.len().to_string()]) };
        let p_sub = m.vis(v, 1.0);
        let q_sub = clamp01(ease(p_sub) * 1.15);
        let sub_top = 15.6 * vh;
        let x2 = m.caps(c, &count.to_uppercase(), left, sub_top, cap, 0.22, CAP, q_sub);
        m.caps(c, &trf("ondisk", &[&size_text(total)]).to_uppercase(), x2 + 2.4 * vh, sub_top, cap, 0.22, CAP, q_sub);
        if b(v.data, "loading") && worlds.is_empty() {
            m.caps(c, &tr("asking").to_uppercase(), left, 22.0 * vh, cap, 0.22, 0.5, q_sub);
        }

        // the list
        let list_top = 22.0 * vh;
        let list_bottom = h - 14.0 * vh;
        let list_w = if narrow { right - left } else { 32.0 * vw };
        let name_size = m.cl(26.0, 87.0, 5.4);
        let meta = cap;
        let entry_h = 1.12 * name_size + 0.7 * vh + meta * 1.3 + 2.6 * vh;
        self.scroll = m.scroll_ease(hid(k::PAGE, 501, 0), self.scroll_to);
        c.push_clip([0.0, list_top - 1.0 * vh, w, list_bottom]);
        let mut y = list_top - self.scroll;
        for (i, wd) in worlds.iter().enumerate() {
            let p = m.vis(v, 2.0 + i as f32);
            let chosen = i as i64 == self.sel;
            let id = hid(k::LIST, 1, i as u32);
            if y > list_top - entry_h && y < list_bottom {
                let name = s(wd, "name");
                let renaming = chosen && self.rename.is_some();
                if renaming {
                    self.rename_field(m, c, left, y, list_w, name_size);
                } else {
                    let shown = fonts().fit_ls(Face::Thin, name_size, &name.to_uppercase(), list_w, 0.02);
                    let st = WordStyle { glow: 0.6, ..WordStyle::new(Face::Thin, name_size).rest(if chosen { BONE } else { DIM }) };
                    if live {
                        m.hit([left, y, left + list_w, y + 1.12 * name_size + 0.7 * vh + meta * 1.3], id);
                    }
                    m.word(c, id, &shown, left, y, st, p, live, false);
                    let kk = m.an(id ^ 0x5e1, if chosen { 1.0 } else { 0.0 }, 6.0);
                    if kk > 0.01 {
                        let lw = tw(Face::Thin, &shown, name_size, 0.02);
                        let ly = y + 1.12 * name_size - 0.06 * name_size;
                        c.light(left, ly, left + lw, 0.85 * kk, 0.0, ease(kk), 0.6, ui);
                    }
                }
                // what it is: mode (hardcore brighter), difficulty, when; the folder when two share a name
                let mut x = left;
                let my = y + 1.12 * name_size + 0.7 * vh;
                let q = clamp01(ease(p) * 1.15);
                let hard = b(wd, "hardcore");
                x = m.caps(c, &s(wd, "mode").to_uppercase(), x, my, meta, 0.22, if hard { 0.79 } else { CAP }, q) + 1.4 * vh;
                if !hard && !s(wd, "difficulty").is_empty() {
                    x = m.caps(c, &s(wd, "difficulty").to_uppercase(), x, my, meta, 0.22, CAP, q) + 1.4 * vh;
                }
                x = m.caps(c, &ago(n(wd, "last")).to_uppercase(), x, my, meta, 0.22, 0.604, q) + 1.4 * vh;
                if worlds.iter().filter(|o| s(o, "name") == s(wd, "name")).count() > 1 {
                    m.caps(c, &s(wd, "folder").to_uppercase(), x, my, meta, 0.22, CAP, q);
                }
            }
            y += entry_h;
        }
        // a new world
        let nsize = m.cl(18.0, 53.0, 3.4);
        let pn = m.vis(v, 2.0 + worlds.len() as f32);
        let label = format!("+  {}", tr("newworld").to_uppercase());
        m.word(c, hid(k::BUTTON, A_NEW, 0), &label, left, y, WordStyle::new(Face::Thin, nsize).rest(0.353), pn, live, false);
        y += 1.12 * nsize;
        c.pop_clip();
        let content = y + self.scroll - list_top;
        self.scroll_to = self.scroll_to.clamp(0.0, (content - (list_bottom - list_top)).max(0.0));

        // the picture, its numbers
        let cur = worlds.get(self.sel.max(0) as usize).filter(|_| self.sel >= 0);
        if !narrow {
            self.picture(m, c, v, cur, vw, vh);
        }

        // what can be done with it, bottom right; play the largest
        let asize = m.cl(18.0, 50.0, 3.2);
        let big = m.cl(34.0, 118.0, 7.4);
        let pa = m.vis(v, 8.0);
        let ok = cur.is_some_and(|wd| b(wd, "ok") && !b(wd, "locked"));
        let fy = h - 5.0 * vh;
        let mut x = right - tw(Face::Light, &tr("play").to_uppercase(), big, 0.02);
        let play = m.word(c, hid(k::BUTTON, A_PLAY, 0), &tr("play").to_uppercase(), x, fy - 1.12 * big, WordStyle::new(Face::Light, big).rest(BONE).enabled(ok), pa, live, false);
        x = play[0] - 4.8 * vh;
        for (a, key) in [(A_DELETE, "delete"), (A_FOLDER, "folder"), (A_BACKUP, "backup"), (A_RENAME, "rename")] {
            let label = tr(key).to_uppercase();
            x -= tw(Face::Thin, &label, asize, 0.02);
            let y0 = fy - 1.12 * asize - 0.1 * big;
            let r = m.word(c, hid(k::BUTTON, a, 0), &label, x, y0, WordStyle::new(Face::Thin, asize).enabled(cur.is_some()), pa, live, false);
            if a == A_DELETE {
                // deleting is held: a brighter line fills under it, and over it what to do
                let held = self.hold_from >= 0.0 && m.lit == hid(k::BUTTON, A_DELETE, 0);
                if !held {
                    self.hold_from = -1.0;
                }
                let hint = m.an(hid(k::BUTTON, A_DELETE, 9), if m.lit == hid(k::BUTTON, A_DELETE, 0) { 1.0 } else { 0.0 }, 6.0);
                if hint > 0.01 {
                    m.caps(c, &tr("hold").to_uppercase(), r[0], y0 - cap * 1.6, cap, 0.2, 0.54 * hint, ease(hint));
                }
                if held {
                    let hq = ((m.now - self.hold_from) / 1.2).min(1.0);
                    let lw = tw(Face::Thin, &label, asize, 0.02);
                    c.light(r[0], r[3] - 0.06 * asize, r[0] + lw, 1.6, 0.0, hq, 1.2, ui);
                    if hq >= 1.0 {
                        self.hold_from = -1.0;
                        m.ask(format!("delete {}", self.sel));
                        m.tick_sound(1.0);
                    }
                }
            }
            x -= 3.4 * vh;
        }

        // a line for a moment at the bottom: backed up; deleted (Undo)
        let said = s(v.data, "said").to_string();
        if said != self.said_was {
            self.said_was = said.clone();
            self.said_at = m.now;
        }
        if !said.is_empty() {
            let age = m.now - self.said_at;
            if age > 7.0 {
                m.ask("said");
            }
            let k_in = smooth(0.0, 0.5, age).min(1.0 - smooth(6.4, 7.0, age));
            let text = match said.as_str() {
                "backup" => tr("backedup").to_uppercase(),
                "deleted" => trf("deleted", &[""]).trim().to_uppercase(),
                _ => String::new(),
            };
            let tsz = m.cl(16.0, 42.0, 2.6);
            let undo = b(v.data, "undo") && said == "deleted";
            let wd = tw(Face::Caps, &text, cap, 0.22) + if undo { 2.0 * vh + tw(Face::Thin, &tr("undo").to_uppercase(), tsz, 0.02) } else { 0.0 };
            let tx = (w - wd) / 2.0;
            let ty = h - 5.2 * vh - 1.12 * tsz;
            let end = m.caps(c, &text, tx, ty + 1.12 * tsz - cap * 1.5, cap, 0.22, 0.6, k_in);
            if undo {
                m.word(c, hid(k::BUTTON, A_UNDO, 0), &tr("undo").to_uppercase(), end + 2.0 * vh, ty, WordStyle::new(Face::Thin, tsz), k_in, live, false);
            }
        }

        // the way back
        let fsize = m.cl(18.0, 50.0, 3.2);
        let by = h - 5.0 * vh - 1.12 * fsize;
        let pb = m.vis(v, 9.0);
        let r = m.word(c, hid(k::BACK, 0, 0), &tr("back").to_uppercase(), left, by, WordStyle::new(Face::Thin, fsize), pb, live, false);
        m.caps(c, "ESC", r[2] + 1.6 * vh, by + 1.12 * fsize - cap * 1.6, cap, 0.22, 0.29, clamp01(ease(pb) * 1.15));
        m.out[super::out::TYPING] = if self.rename.is_some() { 1.0 } else { 0.0 };
        let _ = (ease_out(0.0, 1.0, 0.0), toward(0.0, 0.0, 0.0, 0.0));
    }

    /// The name as a line to type into, in the name's own face.
    fn rename_field(&mut self, m: &mut Menus, c: &mut Canvas, x: f32, y: f32, width: f32, size: f32) {
        let Some(f) = self.rename.as_ref() else { return };
        let ui = m.uipx();
        let line_top = y + 0.03 * size;
        let base = fonts().baseline(Face::Thin, line_top, size, 0.98);
        let text = f.text.to_uppercase();
        c.push_clip([x - 2.0, y - 0.1 * size, x + width, y + 1.12 * size]);
        c.text_ls(x, base, size, Face::Thin, &text, grey(1.0, 1.0), 0.02, 0.0);
        if ((m.now - f.moved) * 1.1).fract() < 0.6 {
            let cx = x + tw(Face::Thin, &text.chars().take(f.caret).collect::<String>(), size, 0.02);
            c.line([cx, line_top + 0.08 * size], [cx, line_top + 0.92 * size], 1.0, grey(1.0, 0.95), 0.0);
        }
        c.pop_clip();
        let ly = y + 1.12 * size - 0.06 * size;
        c.light(x, ly, x + width, 1.0, 0.0, 1.0, 0.8, ui);
    }

    /// The chosen world's picture (it opens from a line the first time, a line runs down it when another is chosen; it
    /// drifts slowly), its folder on it, its numbers under it.
    fn picture(&mut self, m: &mut Menus, c: &mut Canvas, v: &View, cur: Option<&Value>, vw: f32, vh: f32) {
        let ui = m.uipx();
        let (px, py, pw) = (41.0 * vw, 13.0 * vh, 53.0 * vw);
        let ph = pw * 9.0 / 16.0;
        let leave = match v.leaving {
            Some(at) => 1.0 - smooth(0.0, 1.0, (m.now - at) / 0.35),
            None => 1.0,
        };
        let since = m.now - self.shown_at;
        let Some(wd) = cur else { return };
        let key = s(wd, "pic").to_string();
        let pixel = b(wd, "pixel");
        // the picture's window opens from a line across its middle the first time; later the new one is drawn over the
        // old from the top, a line running down ahead of it
        let first = self.old_pic.is_none();
        let open = if first { ease_io((since) / 1.0) } else { 1.0 };
        let half = ph * 0.5 * open;
        let cy = py + ph / 2.0;
        let clip = [px, cy - half, px + pw, cy + half];
        let reveal = if first { 1.0 } else { ease_io(since / 0.75) };
        let drift = |t: f32, seed: f32| -> [f32; 4] {
            // slowly larger and aside, back and forth (16 s each way)
            let ph_ = (t / 16.0).fract();
            let k = 0.5 - 0.5 * (ph_ * std::f32::consts::TAU).cos();
            let s = 1.04 + 0.07 * k;
            let (dx, dy) = ((seed * 7.3).sin() * 0.015 * k, (seed * 3.1).cos() * 0.01 * k);
            [s, dx, dy, 0.0]
        };
        let draw = |m: &mut Menus, c: &mut Canvas, key: &str, pixel: bool, alpha: f32, clip: [f32; 4]| {
            if let Some((uv, size)) = crate::gui::pics::get(key) {
                let [s, dx, dy, _] = drift(m.now, key.len() as f32);
                // cover the window: the picture's middle, scaled
                let aspect = size[0] as f32 / size[1].max(1) as f32;
                let (uw, uh) = (uv[2] - uv[0], uv[3] - uv[1]);
                let (cw, ch) = if aspect > 16.0 / 9.0 { (uh * 16.0 / 9.0, uh) } else { (uw, uw * 9.0 / 16.0) };
                let (cw, ch) = (cw / s, ch / s);
                let cx = uv[0] + uw / 2.0 + dx * uw;
                let cyy = uv[1] + uh / 2.0 + dy * uh;
                let crop = [cx - cw / 2.0, cyy - ch / 2.0, cx + cw / 2.0, cyy + ch / 2.0];
                c.push_clip(clip);
                if pixel {
                    // only the world's icon (no picture of it yet): stretched over the window it was a few big blurred
                    // blocks; it lies there dark, and over it the icon itself, whole and sharp, as on a stand
                    c.pic([px, py, px + pw, py + ph], crop, alpha, true, 0.5, 0.16, 0.0);
                    let side = ph * 0.46;
                    let (ix, iy) = (px + (pw - side) / 2.0, py + (ph - side) / 2.0 - ph * 0.04);
                    c.glow([ix + side / 2.0, iy + side / 2.0], side * 0.9, grey(1.0, 0.07 * alpha));
                    c.pic([ix, iy, ix + side, iy + side], uv, alpha, true, 1.0, 1.0, 0.0);
                } else {
                    c.pic([px, py, px + pw, py + ph], crop, alpha, pixel, 0.92, 1.0, 0.0);
                }
                c.pop_clip();
                true
            } else {
                false
            }
        };
        c.push_clip(clip);
        // a dark ground under it, the valley seen dimly through
        c.rect([px, py, px + pw, py + ph], 0.0, grey(0.02, 0.65 * leave));
        c.pop_clip();
        if !first && reveal < 1.0 {
            if let Some((old, opx)) = self.old_pic.clone() {
                draw(m, c, &old, opx, leave, clip);
            }
        }
        let new_clip = if first { clip } else { [px, py, px + pw, py + ph * reveal] };
        let has = !key.is_empty() && draw(m, c, &key, pixel, leave, new_clip);
        // the picture shown is the one the next is drawn over: once it opened (the first) or was drawn in whole
        if !first && reveal >= 1.0 && since > 0.0 {
            self.old_pic = Some((key.clone(), pixel));
        }
        if self.old_pic.is_none() && open >= 1.0 {
            self.old_pic = Some((key.clone(), pixel));
        }
        if !has {
            let cap = m.cl(9.0, 18.0, 1.15);
            let t = tr("nopic").to_uppercase();
            let tw_ = tw(Face::Caps, &t, cap, 0.22);
            m.caps(c, &t, px + (pw - tw_) / 2.0, cy - cap * 0.65, cap, 0.22, 0.4, ease(open).min(leave));
        }
        // the shade at its foot, its folder on it
        c.push_clip(new_clip);
        c.veil([px, py + ph * 0.55, px + pw, py + ph], grey(0.0, 0.55 * leave), 1.0, 3.0);
        c.pop_clip();
        let cap = m.cl(9.0, 18.0, 1.15);
        // (it sinks as the page goes: dimmed to nothing it was a black word left over the menu after)
        m.caps(c, &s(wd, "folder").to_uppercase(), px + 1.8 * vh, py + ph - 1.6 * vh - cap * 1.3, cap, 0.22, 0.75, ease_out(0.0, 1.0, since / 0.6).min(leave));
        // the lines: two running apart from the middle the first time; one running down ahead of a new picture
        if first && open < 1.0 && open > 0.0 {
            for dir in [-1.0, 1.0] {
                let ly = cy + dir * half;
                c.light(px, ly, px + pw, 1.0 - smooth(0.85, 1.0, open), 0.0, 1.0, 1.0, ui);
            }
        } else if !first && reveal < 1.0 {
            c.light(px, py + ph * reveal, px + pw, 1.0 - smooth(0.9, 1.0, reveal), 0.0, 1.0, 1.0, ui);
        }
        // its numbers, counting up
        let sy = py + ph + 2.4 * vh;
        let k_count = ease_out(0.0, 1.0, (m.now - self.count_from) / 0.7);
        let played = n(wd, "played") as f64;
        let size = wd.get("size").and_then(Value::as_f64).unwrap_or(-1.0);
        let stats = [
            (tr("played"), if played < 0.0 { "—".to_string() } else { played_text(played * k_count as f64) }),
            (tr("size"), if size < 0.0 { "—".to_string() } else { size_text(size * k_count as f64) }),
            (tr("lastplayed"), ago(n(wd, "last"))),
            (tr("version"), s(wd, "version").to_string()),
            (tr("cheats"), tr(if b(wd, "cheats") { "on" } else { "off" }).to_string()),
        ];
        let vsize = m.cl(18.0, 56.0, 3.6);
        let gap = pw / stats.len() as f32;
        for (i, (label, value)) in stats.iter().enumerate() {
            let x = px + i as f32 * gap;
            let p = m.vis(v, 4.0 + i as f32 * 0.5).min(ease_out(0.0, 1.0, (m.now - self.count_from - i as f32 * 0.05) / 0.5).max(0.0));
            let q = clamp01(ease(p) * 1.15);
            m.caps(c, &label.to_uppercase(), x, sy, cap, 0.22, CAP, q);
            let shown = fonts().fit_ls(Face::Light, vsize, &value.to_uppercase(), gap - 1.0 * vh, 0.02);
            m.rising_text(c, Face::Light, &shown, x, sy + cap * 1.3 + 0.7 * vh, vsize, 1.0, 0.02, grey(BONE * leave, 1.0), 0.0, q);
        }
    }

    pub fn event(&mut self, m: &mut Menus, v: &View, e: Event) {
        let worlds = v.data.get("worlds").and_then(Value::as_array).cloned().unwrap_or_default();
        match e {
            Event::Press(hit, button) if button & 15 == 0 => {
                let (kind, wid, sub) = (hid_kind(hit), hid_n(hit), hid_sub(hit));
                if self.rename.is_some() && !(kind == k::LIST && sub as i64 == self.sel) {
                    self.finish_rename(m, true);
                }
                match kind {
                    k::LIST if wid == 1 => {
                        if sub as i64 != self.sel {
                            m.ask(format!("select {sub}"));
                            m.tick_sound(1.0);
                        } else if button & 16 != 0 {
                            m.ask(format!("play {sub}"));
                        }
                    }
                    k::BUTTON => match wid {
                        A_PLAY => m.ask(format!("play {}", self.sel)),
                        A_NEW => {
                            m.fly = None;
                            m.ask("create");
                        }
                        A_RENAME => {
                            if let Some(wd) = worlds.get(self.sel.max(0) as usize) {
                                self.rename = Some(Field::new(hid(k::FIELD, 1, 0), s(wd, "name"), 32, m.now));
                            }
                        }
                        A_BACKUP => m.ask(format!("backup {}", self.sel)),
                        A_FOLDER => m.ask(format!("folder {}", self.sel)),
                        A_DELETE => self.hold_from = m.now,
                        A_UNDO => m.ask("undo"),
                        _ => {}
                    },
                    k::BACK => m.ask("esc"),
                    _ => {}
                }
            }
            Event::Release(_, _) => self.hold_from = -1.0,
            Event::Scroll(dy) => self.scroll_to -= dy * 6.0 * m.vh(),
            Event::Char(ch) => {
                if let Some(f) = self.rename.as_mut() {
                    f.insert(ch, m.now);
                }
            }
            Event::Key(name, mods) => {
                if let Some(f) = self.rename.as_mut() {
                    match name.as_str() {
                        "Return" | "Keypad Enter" => self.finish_rename(m, true),
                        "Escape" => self.finish_rename(m, false),
                        _ => {
                            f.key(&name, mods, m.now);
                        }
                    }
                    return;
                }
                match name.as_str() {
                    "Escape" => m.ask("esc"),
                    "Return" | "Keypad Enter" => m.ask(format!("play {}", self.sel)),
                    "Up" if self.sel > 0 => m.ask(format!("select {}", self.sel - 1)),
                    "Down" if (self.sel + 1) < worlds.len() as i64 => m.ask(format!("select {}", self.sel + 1)),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn finish_rename(&mut self, m: &mut Menus, keep: bool) {
        if let Some(f) = self.rename.take() {
            if keep && !f.text.trim().is_empty() {
                m.ask(format!("rename {} {}", self.sel, f.text.trim()));
            }
        }
    }
}

fn hid_sub(id: u64) -> u32 {
    super::controls::hid_sub(id)
}
