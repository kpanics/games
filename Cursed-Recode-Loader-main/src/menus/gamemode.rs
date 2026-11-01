//! The game mode switcher (F3+F4) over the game: the four modes as the game's icons in a row in the middle, the chosen
//! one lit from below with its name large above; F4 (the game's) goes on to the next, letting go of F3 switches; the
//! pointer on a mode chooses it, a click switches at once.

use super::controls::{hid, hid_kind, hid_n};
use super::ui::{clamp01, ease, tw};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::Face;
use serde_json::Value;

const SLOT: u32 = 40;

pub struct GameMode {
    hovered: i64,
}

impl GameMode {
    pub fn new() -> GameMode {
        GameMode { hovered: -1 }
    }

    pub fn draw(&mut self, m: &mut Menus, c: &mut Canvas, v: &View) {
        let (w, h) = (m.w, m.h);
        let vh = m.vh();
        let ui = m.uipx();
        let d = v.data;
        let p = m.vis(v, 0.0);
        let e = ease(p);
        let live = v.leaving.is_none();
        let modes: Vec<String> = d.get("modes").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        let chosen = d.get("chosen").and_then(Value::as_i64).unwrap_or(0);
        let now = d.get("now").and_then(Value::as_i64).unwrap_or(-1);
        c.veil([w * 0.2, h * 0.24, w * 0.8, h * 0.76], grey(0.0, 0.5 * e), 20.0 * vh, 0.0);
        let icon = m.cl(34.0, 92.0, 6.2);
        let slot = icon * 2.0;
        let x0 = (w - slot * modes.len() as f32) / 2.0;
        let cy = h * 0.5;
        // the chosen one's name, large above the row (each name fades as it is chosen and left)
        let nsize = m.cl(28.0, 84.0, 6.0);
        for (i, name) in modes.iter().enumerate() {
            let k = m.an(hid(SLOT, i as u32, 9), if i as i64 == chosen { 1.0 } else { 0.0 }, 9.0);
            if k > 0.01 {
                let t = name.to_uppercase();
                let tw_ = tw(Face::Light, &t, nsize, 0.04);
                m.rising_text(c, Face::Light, &t, (w - tw_) / 2.0, cy - icon * 0.5 - 2.2 * vh - nsize * 1.05, nsize, 1.05, 0.04, grey(1.0, 1.0), 0.0, ease(k) * e);
            }
        }
        let mut over = -1;
        for i in 0..modes.len() {
            let sx = x0 + slot * i as f32;
            let r = [sx, cy - slot * 0.5, sx + slot, cy + slot * 0.5];
            let id = hid(SLOT, i as u32, 0);
            if live {
                m.hit(r, id);
            }
            if m.over(r) {
                over = i as i64;
            }
            let k = m.an(id ^ 0x6a3e, if i as i64 == chosen { 1.0 } else { 0.0 }, 9.0);
            let ix = sx + (slot - icon) / 2.0;
            let iy = cy - icon / 2.0 - 0.4 * vh * k;
            // the chosen one stands in a soft light, a line lit under it
            if k > 0.01 {
                c.glow([sx + slot / 2.0, cy], slot * 0.62, grey(1.0, 0.10 * k * e));
                c.light(ix - 0.6 * vh, cy + icon * 0.5 + 1.4 * vh, ix + icon + 0.6 * vh, 0.95 * k * e, 0.0, ease(k), 0.8, ui);
            }
            m.extra(1, i as f32, [ix, iy, icon], e * (0.55 + 0.45 * k));
            // where one is now: a small mark under its place
            if i as i64 == now {
                let mx = sx + slot / 2.0;
                let my = cy + icon * 0.5 + 3.0 * vh;
                c.rect([mx - 0.25 * vh, my - 0.25 * vh, mx + 0.25 * vh, my + 0.25 * vh], 0.25 * vh, grey(1.0, 0.55 * e));
            }
        }
        // the pointer on a mode chooses it (once per move onto it)
        if live && over >= 0 && over != self.hovered && over != chosen {
            m.ask(format!("hover {over}"));
        }
        self.hovered = over;
        let cap = m.cl(9.0, 18.0, 1.15);
        let hint = d.get("hint").and_then(Value::as_str).unwrap_or("").to_uppercase();
        let hw = tw(Face::Caps, &hint, cap, 0.2);
        m.caps(c, &hint, (w - hw) / 2.0, cy + slot * 0.5 + 3.5 * vh, cap, 0.2, 0.45, clamp01(e * 1.15));
    }

    pub fn event(&mut self, m: &mut Menus, _v: &View, e: Event) {
        match e {
            Event::Press(hit, b) if b & 15 == 0 && hid_kind(hit) == SLOT => m.ask(format!("pick {}", hid_n(hit))),
            Event::Key(name, _) if name == "Escape" => m.ask("esc"),
            _ => {}
        }
    }
}
