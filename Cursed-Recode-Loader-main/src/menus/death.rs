//! Death: the world sinks under a dark red, the game's words for it come in wide and draw together in the middle, what
//! killed under them, the score, then the two ways on as words (respawn the main one) once the game lets them be
//! pressed.

use super::controls::{hid, hid_kind, hid_n, k};
use super::ui::{clamp01, ease, ease_out, smooth, tw, WordStyle, BONE, CAP, DIM};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, rgba, Canvas};
use crate::gui::text::{fonts, Face};
use serde_json::Value;

pub struct Death {
    main: Option<(u32, bool)>,
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

impl Death {
    pub fn new() -> Death {
        Death { main: None }
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let (w, h) = (m.w, m.h);
        let ui = m.uipx();
        let vh = m.vh();
        let d = v.data;
        let t = m.now - v.opened;
        let live = v.leaving.is_none() && t > 0.6;
        let going = v.leaving.map_or(0.0, |at| smooth(0.0, 1.0, (m.now - at) / 0.5));
        let hard = b(d, "hardcore");
        // the world under a dark red, deeper below
        let k_ = smooth(0.0, 1.0, t / 1.2) * (1.0 - going);
        c.veil([0.0, 0.0, w, h], rgba(0.09, 0.0, 0.005, 0.5 * k_), 0.0, 0.0);
        c.veil([0.0, h * 0.35, w, h], rgba(if hard { 0.32 } else { 0.24 }, 0.01, 0.015, 0.55 * k_), 45.0 * vh, 3.0);
        c.veil([0.0, 0.0, w, h * 0.3], rgba(0.0, 0.0, 0.0, 0.35 * k_), 25.0 * vh, 0.0);

        // the one who killed: standing over it all at the right, as itself, rising out of the dark in a red light,
        // turning a little as if it looked about
        if b(d, "killer") {
            let ks = 60.0 * vh;
            let kin = smooth(0.0, 1.0, (t - 0.8) / 1.6) * (1.0 - going);
            let kx = w * 0.79 - ks / 2.0;
            let ky = h - ks * 0.96 + (1.0 - kin) * 6.0 * vh;
            c.glow([kx + ks / 2.0, h - 4.0 * vh], ks * 0.55, rgba(0.55, 0.05, 0.04, 0.35 * kin));
            let look = (m.now * 0.35).sin() * 0.06;
            m.extra(11, look.rem_euclid(1.0).min(0.9995), [kx, ky, ks], kin);
        }

        // the title: wide and faint, drawing together and brightening
        let title = s(d, "title").to_uppercase();
        let tsize = m.cl(44.0, 176.0, 12.0);
        let e = ease_out(0.0, 1.0, t / 2.4);
        let ls = 0.03 + 0.42 * (1.0 - e);
        let tw_ = tw(Face::Thin, &title, tsize, ls);
        let ty = h * 0.27;
        let alpha = clamp01(t / 0.9) * (1.0 - going);
        let base = fonts().baseline(Face::Thin, ty, tsize, 0.98);
        c.text_ls((w - tw_) / 2.0 + ls * tsize * 0.5, base, tsize, Face::Thin, &title, grey(0.97, alpha), ls, 0.35 * (1.0 - e) + 0.1);
        let mut y = ty + tsize * 1.1;

        // what killed: centred lines
        let cause = s(d, "cause");
        let csize = m.cl(16.0, 42.0, 2.7);
        let pc = clamp01(ease_out(0.0, 1.0, (t - 0.7) / 0.8)) * (1.0 - going);
        if !cause.is_empty() {
            y += 1.0 * vh;
            for l in fonts().wrap_ls(Face::Light, csize, cause, w * 0.62, 0.02) {
                let lw = tw(Face::Light, &l, csize, 0.02);
                m.rising_text(c, Face::Light, &l, (w - lw) / 2.0, y, csize, 1.15, 0.02, grey(0.78, 1.0), 0.0, pc);
                y += csize * 1.15;
            }
        }
        // the score: its caption small, its number
        let cap = m.cl(9.0, 18.0, 1.15);
        let score = s(d, "score");
        if !score.is_empty() {
            y += 2.2 * vh;
            let ps = clamp01(ease_out(0.0, 1.0, (t - 1.0) / 0.8)) * (1.0 - going);
            let label = s(d, "scoreLabel").to_uppercase();
            let ssize = m.cl(18.0, 46.0, 3.0);
            let lw = tw(Face::Caps, &label, cap, 0.24);
            let vw = tw(Face::Book, score, ssize, 0.02);
            let gap = 1.4 * vh;
            let x0 = (w - lw - gap - vw) / 2.0;
            let line_top = y;
            m.rising_text(c, Face::Caps, &label, x0, line_top + (ssize * 1.1 - cap * 1.3) * 0.62, cap, 1.3, 0.24, grey(CAP + 0.08, 1.0), 0.0, ps);
            m.rising_text(c, Face::Book, score, x0 + lw + gap, line_top, ssize, 1.1, 0.02, grey(0.96, 1.0), 0.0, ps);
            y += ssize * 1.1;
        }

        // the ways on, side by side in the middle; the main one first and brighter
        let items: Vec<Value> = d.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut words: Vec<&Value> = items.iter().filter(|it| s(it, "t") == "button").collect();
        words.sort_by_key(|it| !b(it, "main"));
        self.main = words.iter().find(|it| b(it, "main")).map(|it| (n(it, "id").max(0) as u32, it.get("on").and_then(Value::as_bool).unwrap_or(true)));
        let wsize = m.cl(22.0, 66.0, 4.6);
        let gap = 5.0 * vh;
        let labels: Vec<String> = words.iter().map(|it| s(it, "s").to_uppercase()).collect();
        let total: f32 = labels.iter().map(|l| tw(Face::Light, l, wsize, 0.02)).sum::<f32>() + gap * (labels.len().max(1) - 1) as f32;
        let mut x = (w - total) / 2.0;
        let by = (y + 7.0 * vh).max(h * 0.62);
        let pb = clamp01((t - 1.25) / 0.7) * (1.0 - going);
        for (it, label) in words.iter().zip(labels) {
            let wid = n(it, "id").max(0) as u32;
            let main = b(it, "main");
            let st = WordStyle::new(if main { Face::Light } else { Face::Thin }, wsize).rest(if main { BONE } else { DIM }).enabled(it.get("on").and_then(Value::as_bool).unwrap_or(true));
            let r = m.word(c, hid(k::BUTTON, wid, 0), &label, x, by, st, pb, live, false);
            x = r[2] + gap;
        }
        let _ = (ease(0.0), ui);
    }

    pub fn event(&mut self, m: &mut Menus, _v: &View, e: Event) {
        match e {
            Event::Press(hit, button) if button & 15 == 0 && hid_kind(hit) == k::BUTTON => {
                m.ask(format!("press {}", hid_n(hit)));
                m.tick_sound(0.7);
            }
            Event::Key(name, _) if name == "Return" || name == "Keypad Enter" || name == "Space" => {
                if let Some((id, true)) = self.main {
                    m.ask(format!("press {id}"));
                }
            }
            _ => {}
        }
    }
}
