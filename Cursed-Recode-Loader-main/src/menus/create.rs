//! A new world (the preview's): its name typed large, the folder it will be saved in under it; game mode, difficulty
//! and world type as words with what the chosen one means under them; the seed; what the world starts with; game
//! rules, experiments, data packs. The valley behind becomes the terrain it will get (flat, large biomes, amplified;
//! the seed moves its hills), hardcore turns its glow red.

use super::controls::{hid, hid_kind, hid_n, hid_sub, k, Field};
use super::lang::{tr, trf};
use super::ui::{clamp01, ease, tw, WordStyle, CAP, DIM};
use super::{Event, Menus, View};
use crate::gui::draw::{grey, Canvas};
use crate::gui::text::{fonts, Face};
use serde_json::Value;

const R_MODE: u32 = 1;
const R_DIFF: u32 = 2;
const R_TYPE: u32 = 3;
const R_FLAG: u32 = 4;
const R_MORE: u32 = 5;
const A_CREATE: u32 = 10;
const A_CANCEL: u32 = 11;
const A_RANDOM: u32 = 12;
const F_NAME: u32 = 20;
const F_SEED: u32 = 21;

pub struct Create {
    field: Option<Field>,
    /// What the choices' notes said last (a new one rises).
    notes: [String; 3],
    notes_at: [f32; 3],
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
fn list(v: &Value, key: &str) -> Vec<String> {
    v.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}
fn word<'a>(v: &'a Value, key: &'a str) -> &'a str {
    v.get("words").and_then(|w| w.get(key)).and_then(Value::as_str).unwrap_or(key)
}

/// The terrain a world type gives (the valley's knobs: amplitude, flatness, its features' scale).
fn terrain(key: &str) -> [f32; 3] {
    match key {
        "flat" => [1.0, 1.0, 1.0],
        "large_biomes" => [1.0, 0.0, 0.45],
        "amplified" => [2.1, 0.0, 1.0],
        "single_biome_surface" => [0.8, 0.0, 0.7],
        "debug_all_block_states" => [0.4, 0.85, 1.0],
        _ => [1.0, 0.0, 1.0],
    }
}

/// Where a seed moves the valley's hills (the same seed, the same hills).
fn seed_shift(seed: &str) -> [f32; 2] {
    if seed.trim().is_empty() {
        return [0.0, 0.0];
    }
    let mut h: u64 = 1469598103934665603;
    for byte in seed.trim().bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(1099511628211);
    }
    [((h & 0xffff) as f32 / 65535.0 - 0.5) * 900.0, (((h >> 16) & 0xffff) as f32 / 65535.0 - 0.5) * 900.0]
}

impl Create {
    pub fn new() -> Create {
        Create { field: None, notes: Default::default(), notes_at: [-10.0; 3] }
    }

    pub fn typing(&self) -> bool {
        self.field.is_some()
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
        let d = v.data;
        // the valley: the camera rises to look over the land the world will have
        m.scene_pose([-34.0, 44.0, -0.27, 0.02, m.scene_to[4]], 0.05, 0.0);
        let t = terrain(s(d, "typeKey"));
        let seed = self.field.as_ref().filter(|f| f.id == hid(k::FIELD, F_SEED, 0)).map_or(s(d, "seed").to_string(), |f| f.text.clone());
        let sh = seed_shift(&seed);
        m.knob(0, t[0]);
        m.knob(1, t[1]);
        m.knob(2, t[2]);
        m.knob(3, 41.0 + (t[0] - 1.0).max(0.0) * 34.0);
        m.knob(4, sh[0]);
        m.knob(5, sh[1]);
        m.knob(6, 6.0 + 4.0 * t[1]);
        m.knob(7, 41.0 + (t[0] - 1.0).max(0.0) * 34.0);
        m.knob(13, (1.0 - t[1]) * (1.0 + 0.8 * (t[0] - 1.0).max(0.0)));
        m.knob(16, if b(d, "hardcore") { 1.0 } else { 0.0 });
        let shade = m.vis(v, 0.0);
        c.veil([0.0, 12.0 * vh, 56.0 * vw, h], grey(0.0, 0.38 * shade), 16.0 * vh, 1.0);

        m.title(c, v, &tr("newworld").to_uppercase(), left, 6.0 * vh, m.cl(34.0, 120.0, 7.6));
        let col_w = if narrow { right - left } else { 46.0 * vw };
        let cap = m.cl(9.0, 18.0, 1.15);
        let mut y = 21.0 * vh;
        let mut order = 1.0;

        // the name, large; the folder it goes in
        let nsize = m.cl(30.0, 101.0, 6.4);
        let nid = hid(k::FIELD, F_NAME, 0);
        let name = self.field.as_ref().filter(|f| f.id == nid).map_or(s(d, "name").to_string(), |f| f.text.clone());
        let p = m.vis(v, order);
        let q = clamp01(ease(p) * 1.15);
        let r = [left, y, left + col_w, y + nsize * 1.05];
        if live {
            m.hit(r, nid);
        }
        let focus = self.field.as_ref().is_some_and(|f| f.id == nid);
        let shown = if name.is_empty() && !focus { word(d, "selectWorld.enterName").to_uppercase() } else { name.to_uppercase() };
        m.rising_text(c, Face::Light, &fonts().fit_ls(Face::Light, nsize, &shown, col_w, 0.02), left, y, nsize, 1.0, 0.02, grey(if name.is_empty() && !focus { 0.27 } else { 1.0 }, 1.0), 0.0, q);
        if let Some(f) = self.field.as_ref().filter(|f| f.id == nid) {
            if ((m.now - f.moved) * 1.1).fract() < 0.6 {
                let cx = left + tw(Face::Light, &f.text.chars().take(f.caret).collect::<String>().to_uppercase(), nsize, 0.02);
                c.line([cx, y + 0.1 * nsize], [cx, y + 0.95 * nsize], 1.0, grey(1.0, 0.95), 0.0);
            }
        }
        let under = y + nsize * 1.05 + 0.4 * vh;
        c.light(left, under, left + col_w, 0.18 * ease(p), 0.0, 1.0, 0.0, ui);
        let lit = m.an(nid ^ 7, if focus { 1.0 } else { 0.0 }, 5.0);
        if lit > 0.01 {
            c.light(left, under, left + col_w, lit, 0.0, ease(lit), 0.8, ui);
        }
        m.caps(c, &trf("savedas", &[s(d, "folder")]).to_uppercase(), left, under + 0.8 * vh, cap, 0.22, CAP, q);
        y = under + 0.8 * vh + cap * 1.3 + 2.5 * vh;
        order += 1.0;

        // the choices: game mode, difficulty, world type
        let wsize = m.cl(18.0, 53.0, 3.4);
        let rows: [(u32, &str, Vec<String>, i64, Vec<String>, bool); 3] = [
            (R_MODE, word(d, "selectWorld.gameMode"), list(d, "modes"), n(d, "mode"), list(d, "modeInfo"), true),
            (R_DIFF, word(d, "options.difficulty"), list(d, "diffs"), n(d, "diff"), list(d, "diffInfo"), !b(d, "hardcore")),
            (R_TYPE, word(d, "selectWorld.mapType"), list(d, "types"), n(d, "type"), Vec::new(), true),
        ];
        for (ri, (rid, label, words, sel, infos, enabled)) in rows.iter().enumerate() {
            let p = m.vis(v, order);
            let q = clamp01(ease(p) * 1.15);
            m.caps(c, &label.to_uppercase(), left, y, cap, 0.22, CAP, q);
            y += cap * 1.3 + 0.5 * vh;
            let mut x = left;
            let focus = words.iter().enumerate().any(|(i, _)| m.lit == hid(k::CHOICE, *rid, i as u32));
            for (i, wd) in words.iter().enumerate() {
                let id = hid(k::CHOICE, *rid, i as u32);
                let chosen = i as i64 == *sel;
                let label = wd.to_uppercase();
                let lw = tw(Face::Thin, &label, wsize, 0.02);
                if x + lw > left + col_w + 20.0 * vw && x > left {
                    break;
                }
                let st = WordStyle::new(Face::Thin, wsize).rest(if chosen { 1.0 } else { DIM }).enabled(*enabled || chosen);
                let r = m.word(c, id, &label, x, y, st, p, live && *enabled, focus);
                let kk = m.an(id ^ 0xc405e, if chosen { 1.0 } else { 0.0 }, 6.0);
                if kk > 0.01 {
                    c.light(x, r[3] - 0.06 * wsize, x + lw, 0.85 * kk * p, 0.0, ease(kk), 0.6, ui);
                }
                x = r[2] + 2.6 * vh;
            }
            if *rid == R_TYPE && b(d, "customize") {
                let label = word(d, "selectWorld.customizeType").to_uppercase();
                m.word(c, hid(k::BUTTON, R_TYPE, 9), &label, x + 1.0 * vh, y, WordStyle::new(Face::Thin, wsize * 0.7).rest(0.4), p, live, false);
            }
            y += 1.12 * wsize + 0.3 * vh;
            // what the chosen one means (a new note rises)
            let note = match *rid {
                R_TYPE if b(d, "amplified") => word(d, "generator.minecraft.amplified.info").to_string(),
                R_TYPE if s(d, "typeKey") == "flat" => tr("flatnote").to_string(),
                _ => infos.get((*sel).max(0) as usize).cloned().unwrap_or_default(),
            };
            if note != self.notes[ri] {
                self.notes[ri] = note.clone();
                self.notes_at[ri] = m.now;
            }
            let nk = clamp01(ease((m.now - self.notes_at[ri]) / 0.45)).min(q);
            let nsz = m.cl(9.0, 18.0, 1.15);
            let lines = fonts().wrap_ls(Face::Caps, nsz, &note, col_w, 0.06);
            let mut ny = y;
            for l in lines.iter().take(2) {
                m.rising_text(c, Face::Caps, l, left, ny, nsz, 1.5, 0.06, grey(0.42, 1.0), 0.0, nk);
                ny += nsz * 1.5;
            }
            y += nsz * 1.5 * lines.len().clamp(1, 2) as f32 + 2.2 * vh;
            order += 1.0;
        }

        // the seed: typed, or one at random
        let p = m.vis(v, order);
        let q = clamp01(ease(p) * 1.15);
        m.caps(c, &word(d, "selectWorld.enterSeed").to_uppercase(), left, y, cap, 0.22, CAP, q);
        y += cap * 1.3 + 0.5 * vh;
        let sid = hid(k::FIELD, F_SEED, 0);
        let fw = col_w.min(26.0 * vw);
        let fr = m.field(c, sid, s(d, "seed"), s(d, "seedHint"), left, y, fw, p, live, self.field.as_ref());
        let rsz = m.cl(15.0, 39.0, 2.5);
        m.word(c, hid(k::BUTTON, A_RANDOM, 0), &tr("random").to_uppercase(), fr[2] + 2.0 * vh, y + (fr[3] - fr[1] - 1.12 * rsz) / 2.0, WordStyle::new(Face::Light, rsz), p, live, false);
        y = fr[3] + 2.2 * vh;
        order += 1.0;

        // what it starts with: words lit when on
        let p = m.vis(v, order);
        let fsz = m.cl(15.0, 39.0, 2.5);
        let mut x = left;
        let flags = [("structures", "selectWorld.mapFeatures", true), ("bonus", "selectWorld.bonusItems", true), ("commands", "selectWorld.allowCommands", !b(d, "hardcore"))];
        for (i, (key, label, enabled)) in flags.iter().enumerate() {
            let on = b(d, key);
            let id = hid(k::CHOICE, R_FLAG, i as u32);
            let text = word(d, label).to_uppercase();
            let st = WordStyle { glow: 0.0, ..WordStyle::new(Face::Light, fsz).rest(if on { 0.9 } else { 0.35 }).enabled(*enabled) };
            let r = m.word(c, id, &text, x, y, st, p, live && *enabled, false);
            let kk = m.an(id ^ 0x0f0f, if on { 1.0 } else { 0.0 }, 6.0);
            if kk > 0.01 {
                let lw = tw(Face::Light, &text, fsz, 0.02);
                c.light(x, r[3] - 0.06 * fsz, x + lw, 0.8 * kk * p, 0.0, ease(kk), 0.5, ui);
            }
            x = r[2] + 2.8 * vh;
        }
        y += 1.12 * fsz + 2.0 * vh;
        order += 1.0;
        // more: game rules, experiments, data packs (the game's screens)
        let p = m.vis(v, order);
        let mut x = left;
        for (i, key) in ["selectWorld.gameRules", "selectWorld.experiments", "selectWorld.dataPacks"].iter().enumerate() {
            let label = word(d, key).to_uppercase();
            let id = hid(k::BUTTON, R_MORE, i as u32);
            let lw = tw(Face::Caps, &label, cap, 0.22);
            let r = [x, y, x + lw, y + cap * 1.6];
            if live {
                m.hit(r, id);
            }
            let hot = m.an(id ^ 0x3a, if live && m.over(r) { 1.0 } else { 0.0 }, 6.0);
            m.caps(c, &label, x, y, cap, 0.22, 0.43 + 0.57 * hot, clamp01(ease(p) * 1.15));
            if hot > 0.01 {
                c.light(x, y + cap * 1.5, x + lw, 0.8 * hot, 0.0, hot, 0.3, ui);
            }
            x += lw + 2.6 * vh;
        }

        // the terrain it will get, said by the valley; cancel and create, bottom right
        if !narrow {
            let pt = m.vis(v, 3.0);
            let t1 = tr("terrain").to_uppercase();
            let t2 = s(d, "typeName").to_uppercase();
            let tw1 = tw(Face::Caps, &t1, cap, 0.22);
            let tw2 = tw(Face::Caps, &t2, cap, 0.22);
            let ty = h - 17.0 * vh - cap * 3.0;
            m.caps(c, &t1, right - tw1, ty, cap, 0.22, CAP, clamp01(ease(pt) * 1.15));
            m.caps(c, &t2, right - tw2, ty + cap * 1.6, cap, 0.22, 0.75, clamp01(ease(pt) * 1.15));
        }
        let big = m.cl(34.0, 118.0, 7.4);
        let asz = m.cl(18.0, 50.0, 3.2);
        let pa = m.vis(v, 9.0);
        let fy = h - 5.0 * vh;
        let create = tr("create").to_uppercase();
        let cx = right - tw(Face::Light, &create, big, 0.02);
        let r = m.word(c, hid(k::BUTTON, A_CREATE, 0), &create, cx, fy - 1.12 * big, WordStyle::new(Face::Light, big).rest(0.902), pa, live, false);
        let cancel = word(d, "gui.cancel").to_uppercase();
        let kx = r[0] - 4.8 * vh - tw(Face::Thin, &cancel, asz, 0.02);
        m.word(c, hid(k::BUTTON, A_CANCEL, 0), &cancel, kx, fy - 1.12 * asz - 0.1 * big, WordStyle::new(Face::Thin, asz), pa, live, false);
        m.out[super::out::TYPING] = if self.field.is_some() { 1.0 } else { 0.0 };
    }

    pub fn event(&mut self, m: &mut Menus, v: &View, e: Event) {
        let d = v.data;
        match e {
            Event::Press(hit, button) if button & 15 == 0 => {
                let (kind, id, sub) = (hid_kind(hit), hid_n(hit), hid_sub(hit));
                if kind != k::FIELD {
                    self.field = None;
                }
                match (kind, id) {
                    (k::FIELD, F_NAME) => {
                        if self.field.as_ref().map_or(true, |f| f.id != hit) {
                            self.field = Some(Field::new(hit, s(d, "name"), 32, m.now));
                        }
                    }
                    (k::FIELD, F_SEED) => {
                        if self.field.as_ref().map_or(true, |f| f.id != hit) {
                            self.field = Some(Field::new(hit, s(d, "seed"), 32, m.now));
                        }
                    }
                    (k::CHOICE, R_MODE) => m.ask(format!("mode {sub}")),
                    (k::CHOICE, R_DIFF) => m.ask(format!("diff {sub}")),
                    (k::CHOICE, R_TYPE) => m.ask(format!("type {sub}")),
                    (k::CHOICE, R_FLAG) => m.ask(["structures", "bonus", "commands"][sub.min(2) as usize]),
                    (k::BUTTON, R_TYPE) => m.ask("customize"),
                    (k::BUTTON, R_MORE) => m.ask(["rules", "experiments", "packs"][sub.min(2) as usize]),
                    (k::BUTTON, A_RANDOM) => m.ask("random"),
                    (k::BUTTON, A_CREATE) => m.ask("create"),
                    (k::BUTTON, A_CANCEL) => m.ask("cancel"),
                    _ => {}
                }
            }
            Event::Char(ch) => {
                if let Some(f) = self.field.as_mut() {
                    if f.insert(ch, m.now) {
                        let verb = if hid_n(f.id) == F_NAME { "name" } else { "seed" };
                        m.ask(format!("{verb} {}", f.text));
                    }
                }
            }
            Event::Key(name, mods) => {
                if let Some(f) = self.field.as_mut() {
                    match name.as_str() {
                        "Escape" | "Return" | "Keypad Enter" | "Tab" => self.field = None,
                        _ => {
                            if f.key(&name, mods, m.now) {
                                let verb = if hid_n(f.id) == F_NAME { "name" } else { "seed" };
                                m.ask(format!("{verb} {}", f.text));
                            }
                        }
                    }
                    return;
                }
                match name.as_str() {
                    "Escape" => m.ask("cancel"),
                    "Return" | "Keypad Enter" => m.ask("create"),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
