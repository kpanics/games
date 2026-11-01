//! The player list (held Tab) over the game: a glass plate at the top, where one plays and how many small above, the
//! server's header, the players in columns of up to 20 (the face, one of ours' mark, the name, the score or the hearts,
//! the ping as four bars), the server's footer. The names, the header and the footer are the server's text: the game draws them with
//! its font at whole pixels where this says (extras kind 3); the faces too (kind 2).

use super::lang::trf;
use super::ui::{clamp01, ease, tw};
use super::{Menus, View};
use crate::gui::draw::{grey, rgba, Canvas};
use crate::gui::text::Face;
use serde_json::Value;

pub struct Tab;

/// A text the game draws: its number and its width in the font's pixels.
fn text(v: Option<&Value>) -> Option<(f32, f32)> {
    let a = v?.as_array()?;
    Some((a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32))
}

impl Tab {
    pub fn new() -> Tab {
        Tab
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let (w, h) = (m.w, m.h);
        let vh = m.vh();
        let d = v.data;
        let up = m.now - v.opened;
        let p = match v.leaving {
            Some(at) => 1.0 - clamp01((m.now - at) / 0.18),
            None => clamp01(up / 0.16),
        };
        let e = ease(p);
        if e <= 0.0 {
            return;
        }
        // a font pixel in window pixels: whole, so the game's text stays sharp
        let n = (h / 450.0).round().max(1.0);
        let line = 9.0 * n;
        let row_h = 10.0 * n;
        let face = 8.0 * n;
        let rows: Vec<Value> = d.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
        let header: Vec<Value> = d.get("header").and_then(Value::as_array).cloned().unwrap_or_default();
        let footer: Vec<Value> = d.get("footer").and_then(Value::as_array).cloned().unwrap_or_default();
        let count = rows.len();
        let cols = count.div_ceil(20).max(1);
        let per = count.div_ceil(cols).max(1);
        // a column: face, name, score, ping
        let name_w = rows.iter().filter_map(|r| text(r.get("name"))).map(|t| t.1).fold(0.0f32, f32::max) * n;
        let cap = m.cl(8.0, 15.0, 1.0);
        let score_w = rows
            .iter()
            .map(|r| if let Some(t) = text(r.get("score")) { t.1 * n } else if let Some(hv) = r.get("hearts").and_then(Value::as_i64) { tw(Face::Caps, &format!("{hv}"), cap, 0.1) + 1.4 * cap } else { 0.0 })
            .fold(0.0f32, f32::max);
        let ping_w = 7.0 * n;
        // one of ours (Recode's or Cursed's player, oneself too): their mark after the face (crate::ours)
        let marks: Vec<Option<crate::ours::Client>> = rows
            .iter()
            .map(|r| crate::ours::client_of(r.get("nick").and_then(Value::as_str).unwrap_or(""), r.get("me").and_then(Value::as_bool).unwrap_or(false)))
            .collect();
        let mark_w = if marks.iter().any(Option::is_some) { 9.0 * n } else { 0.0 };
        let col_w = face + 3.0 * n + mark_w + name_w + if score_w > 0.0 { 6.0 * n + score_w } else { 0.0 } + 6.0 * n + ping_w;
        let gap = 8.0 * n;
        let pad = 7.0 * n;
        let head_w = header.iter().chain(footer.iter()).filter_map(|l| text(Some(l))).map(|t| t.1 * n).fold(0.0f32, f32::max);
        let where_ = d.get("where").and_then(Value::as_str).unwrap_or("").to_uppercase();
        let total = d.get("count").and_then(Value::as_i64).unwrap_or(count as i64);
        let top_line = if where_.is_empty() { trf("online", &[&total.to_string()]) } else { format!("{where_}  ·  {}", trf("online", &[&total.to_string()])) }.to_uppercase();
        let top_w = tw(Face::Caps, &top_line, cap, 0.2);
        let inner = (cols as f32 * col_w + (cols - 1) as f32 * gap).max(head_w).max(top_w);
        let pw = (inner + 2.0 * pad).min(w - 4.0 * vh);
        let ph = pad + cap * 1.5 + 3.0 * n + header.len() as f32 * (line + n) + if header.is_empty() { 0.0 } else { 4.0 * n } + per as f32 * row_h
            + if footer.is_empty() { 0.0 } else { 4.0 * n + footer.len() as f32 * (line + n) } + pad;
        let x0 = ((w - pw) / 2.0).round();
        let y0 = (2.6 * vh - (1.0 - e) * 1.2 * vh).round();
        let panel = [x0, y0, x0 + pw, y0 + ph];
        c.glass(panel, 1.0 * vh, [0.02, 0.02, 0.025, 0.62 * e], 0.7 * e, 0.6 * e, 0.0);
        c.rect(panel, 1.0 * vh, grey(0.0, 0.18 * e));
        let mut y = y0 + pad;
        // where and how many
        m.rising_text(c, Face::Caps, &top_line, (w - top_w) / 2.0, y, cap, 1.5, 0.2, grey(0.55, e), 0.0, 1.0);
        y += cap * 1.5 + 3.0 * n;
        for l in &header {
            if let Some((id, tw_)) = text(Some(l)) {
                m.extra(3, id, [((w - tw_ * n) / 2.0).round(), y.round(), n], e);
            }
            y += line + n;
        }
        if !header.is_empty() {
            y += 4.0 * n;
        }
        // the players, column by column
        let cols_w = cols as f32 * col_w + (cols - 1) as f32 * gap;
        let cx0 = ((w - cols_w) / 2.0).round();
        for (i, r) in rows.iter().enumerate() {
            let (col, at) = (i / per, i % per);
            let x = cx0 + col as f32 * (col_w + gap);
            let ry = (y + at as f32 * row_h).round();
            let k = e * clamp01((up - 0.03 * at as f32) / 0.18).max(if v.leaving.is_some() { 1.0 } else { 0.0 });
            let spec = r.get("spectator").and_then(Value::as_bool).unwrap_or(false);
            let a = k * if spec { 0.55 } else { 1.0 };
            if r.get("me").and_then(Value::as_bool).unwrap_or(false) {
                c.rect([x - 2.0 * n, ry, x + col_w + 2.0 * n, ry + row_h], 1.5 * n, grey(1.0, 0.05 * k));
            }
            if let Some(f) = r.get("face").and_then(Value::as_f64) {
                m.extra(2, f as f32, [x, ry + n, face], a);
            }
            if let Some(client) = marks[i] {
                let mx = x + face + 3.0 * n;
                crate::ours::draw(c, [mx, ry + 1.5 * n, mx + 7.0 * n, ry + 8.5 * n], client, a);
            }
            if let Some((id, _)) = text(r.get("name")) {
                m.extra(3, id, [x + face + 3.0 * n + mark_w, ry + n, n], a);
            }
            let mut rx = x + col_w;
            // the ping: four bars, as many lit as it is good
            let ping = r.get("ping").and_then(Value::as_i64).unwrap_or(-1);
            let lit = if ping < 0 { 0 } else if ping < 150 { 4 } else if ping < 300 { 3 } else if ping < 600 { 2 } else if ping < 1000 { 1 } else { 0 };
            let bw = n.max(1.0);
            for b in 0..4 {
                let bh = (2.0 + 2.0 * b as f32) * n * 0.75;
                let bx = rx - ping_w + b as f32 * 2.0 * bw;
                let by = ry + n + 7.5 * n;
                let col = if ping < 0 { grey(1.0, 0.12 * a) } else if b < lit { if lit <= 1 { rgba(0.95, 0.35, 0.3, 0.95 * a) } else { grey(0.92, 0.95 * a) } } else { grey(1.0, 0.16 * a) };
                c.rect([bx, by - bh, bx + bw, by], 0.0, col);
            }
            rx -= ping_w + 6.0 * n;
            if let Some((id, sw)) = text(r.get("score")) {
                m.extra(3, id, [(rx - sw * n).round(), ry + n, n], a);
            } else if let Some(hv) = r.get("hearts").and_then(Value::as_i64) {
                let t = format!("{hv}");
                let tw_ = tw(Face::Caps, &t, cap, 0.1);
                m.rising_text(c, Face::Caps, &t, rx - tw_, ry + (row_h - cap * 1.3) / 2.0, cap, 1.3, 0.1, grey(0.92, a), 0.0, 1.0);
                let hx = rx - tw_ - 1.0 * cap;
                c.rect([hx - 0.3 * cap, ry + row_h / 2.0 - 0.3 * cap, hx + 0.3 * cap, ry + row_h / 2.0 + 0.3 * cap], 0.3 * cap, rgba(0.95, 0.3, 0.3, a));
            }
        }
        y += per as f32 * row_h;
        if !footer.is_empty() {
            y += 4.0 * n;
            for l in &footer {
                if let Some((id, tw_)) = text(Some(l)) {
                    m.extra(3, id, [((w - tw_ * n) / 2.0).round(), y.round(), n], e);
                }
                y += line + n;
            }
        }
    }
}
