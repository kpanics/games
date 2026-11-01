//! The credits (and the End's poem before them): the game's lines rising up the dark, set in the menus' faces — the
//! poem's two voices in their tints, the scrambled name changing its letters, headings and titles centred and larger,
//! the names in the book face. Where they are is the game's scroll (its keys speed it, turn it back, end it), laid out
//! as the game lays it (12 of its pixels a line) at its scale. Stars drift behind for the poem.

use super::lang::tr;
use super::ui::{clamp01, ease, tw};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, rgba, Canvas};
use crate::gui::text::Face;
use serde_json::Value;

pub struct Credits;

const SCRAMBLE: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ0123456789#%&";

fn hash(x: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

impl Credits {
    pub fn new() -> Credits {
        Credits
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let (w, h) = (m.w, m.h);
        let vh = m.vh();
        let d = v.data;
        let e = ease(m.vis(v, 0.0));
        let scroll = m.inp[0];
        let gs = m.inp[1].max(1.0);
        let poem = d.get("poem").and_then(Value::as_bool).unwrap_or(false);
        // the dark, and for the poem the stars drifting slowly up with it
        c.rect([0.0, 0.0, w, h], 0.0, grey(0.0, 0.92 * e));
        if poem {
            for i in 0..160u32 {
                let r = hash(i);
                let x = (r % 10_000) as f32 / 10_000.0 * w;
                let depth = 0.25 + ((r >> 8) % 100) as f32 / 100.0 * 0.75;
                let y0 = ((r >> 16) % 10_000) as f32 / 10_000.0;
                let y = (y0 * h * 1.2 - scroll * gs * 0.12 * depth).rem_euclid(h * 1.2) - h * 0.1;
                let s = (0.6 + 1.6 * depth) * m.uipx();
                let tw_ = 0.5 + 0.5 * ((m.now * (0.7 + depth) + i as f32).sin());
                c.rect([x - s, y - s, x + s, y + s], s, grey(1.0, (0.12 + 0.35 * depth * tw_) * e));
            }
        }
        let lines: Vec<Value> = d.get("lines").and_then(Value::as_array).cloned().unwrap_or_default();
        let unit = gs;
        let line_h = 12.0 * unit;
        let col_w = 256.0 * unit;
        let x0 = (w - col_w) / 2.0;
        let off = -scroll * unit;
        // the title first, where the game puts its logo
        let ty = h + 50.0 * unit + off;
        if ty > -40.0 * unit && ty < h {
            let t = "MINECRAFT";
            let size = 34.0 * unit;
            let tw_ = tw(Face::Thin, t, size, 0.12);
            m.rising_text(c, Face::Thin, t, (w - tw_) / 2.0, ty, size, 1.0, 0.12, grey(1.0, e), 0.2, 1.0);
            let sub = "JAVA EDITION";
            let ss = 6.0 * unit;
            let sw = tw(Face::Caps, sub, ss, 0.4);
            m.rising_text(c, Face::Caps, sub, (w - sw) / 2.0, ty + size * 1.05, ss, 1.3, 0.4, grey(0.6, e), 0.0, 1.0);
        }
        let size = 8.6 * unit;
        let big = 10.5 * unit;
        let mut y = h + 150.0 * unit + off;
        let n = lines.len();
        let tick = (m.now * 20.0) as u32;
        for (li, l) in lines.iter().enumerate() {
            // the last line stays in the middle once it gets there
            if li + 1 == n && y < h / 2.0 - 6.0 * unit {
                y = h / 2.0 - 6.0 * unit;
            }
            if y + line_h > 0.0 && y < h {
                let centred = l.get("c").and_then(Value::as_bool).unwrap_or(false);
                let runs: Vec<Value> = l.get("runs").and_then(Value::as_array).cloned().unwrap_or_default();
                let text: String = runs.iter().filter_map(|r| r.get(0).and_then(Value::as_str)).collect();
                let heading = text.trim().chars().all(|ch| ch == '=') && !text.trim().is_empty();
                // fade near the top and the bottom
                let a = e * clamp01(y / (14.0 * vh)) * clamp01((h - y) / (10.0 * vh));
                if heading {
                    let lw = col_w * 0.3;
                    c.light((w - lw) / 2.0, y + line_h * 0.5, (w + lw) / 2.0, 0.5 * a, 0.0, 1.0, 0.4, m.uipx());
                } else {
                    let face = if centred { Face::Light } else if poem { Face::Light } else { Face::Book };
                    let sz = if centred && !poem { big } else { size };
                    let full = text.trim_start().to_string();
                    let lead = (text.len() - full.len()) as f32;
                    let total = tw(face, &full, sz, 0.02);
                    let mut x = if centred { (w - total) / 2.0 } else { x0 + lead.min(12.0) * 2.0 * unit };
                    let mut first = true;
                    for r in &runs {
                        let mut s = r.get(0).and_then(Value::as_str).unwrap_or("").to_string();
                        if first {
                            s = s.trim_start().to_string();
                            first = false;
                        }
                        if s.is_empty() {
                            continue;
                        }
                        let col = r.get(1).and_then(Value::as_i64).unwrap_or(-1);
                        let obf = r.get(2).and_then(Value::as_bool).unwrap_or(false);
                        if obf {
                            s = s.chars().enumerate().map(|(k, ch)| if ch == ' ' { ' ' } else { SCRAMBLE[(hash(tick ^ (k as u32 * 7919) ^ li as u32) as usize) % SCRAMBLE.len()] as char }).collect();
                        }
                        // the game's colours, softened toward the menus' greys
                        let rgb = if col < 0 { [0.86, 0.86, 0.86] } else { [((col >> 16) & 255) as f32 / 255.0, ((col >> 8) & 255) as f32 / 255.0, (col & 255) as f32 / 255.0] };
                        let k = 0.55;
                        let tint = rgba(0.9 + (rgb[0] - 0.9) * k, 0.9 + (rgb[1] - 0.9) * k, 0.9 + (rgb[2] - 0.9) * k, 1.0);
                        let tint = if centred && !poem { grey(1.0, 1.0) } else { tint };
                        x = m.rising_text(c, face, &s, x, y + (line_h - sz * 1.1) / 2.0, sz, 1.1, 0.02, [tint[0], tint[1], tint[2], a], if obf { 0.4 } else { 0.0 }, 1.0);
                    }
                }
            }
            y += line_h;
        }
        // how to go faster
        let cap = m.cl(8.0, 14.0, 1.0);
        let hint = tr("creditshint").to_uppercase();
        let hw = tw(Face::Caps, &hint, cap, 0.2);
        m.caps(c, &hint, w - hw - 3.0 * vh, h - 3.0 * vh - cap * 1.3, cap, 0.2, 0.3, clamp01((m.now - v.opened - 2.0) / 1.0) * e);
    }

    pub fn event(&mut self, m: &mut Menus, _v: &View, e: Event) {
        if let Event::Key(name, _) = e {
            if name == "Escape" {
                m.ask("esc");
            }
        }
    }
}
