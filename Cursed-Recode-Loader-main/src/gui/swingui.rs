//! Swing Animations' editor (her word, 25.10: "как у рокстара тоже сделай менюшку где можно самому настроить как бить
//! будет"). The menu goes; a panel stands at the left over the world while the hand there swings as it is being made
//! (Java draws the hand at the moment the editor plays or is held at: hud_state 93..94). A swing is its moments
//! (viewmodel.rs Key): on a line of time, each a diamond — dragged along it, pressed to change it; where the item is,
//! how it is turned and how big at that moment, and how it eases to the next. A style to start from, a moment added
//! where the time stands, one removed, all of it mirrored; its code copied or pasted. Esc keeps it (the style is then
//! Custom).

use super::draw::{grey, mix, Canvas};
use super::i18n::{tr, trn};
use super::text::{fonts, Face};
use super::theme::{fade, pal};
use super::viewmodel::{at, keys_dump, keys_parse, Key, STYLES};
use super::{ak, Gui, A_HOV};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
enum Drag {
    #[default]
    None,
    Time,
    Moment(usize),
    Slider(usize),
}

/// The editor as it is: open, the moments being made, the one chosen, playing, where the time is (0..1 of the swing,
/// with a breath after it), what is dragged, its parts this frame (part, number, rect), a paste asked for.
#[derive(Default)]
pub(super) struct SwingEd {
    pub on: bool,
    pub keys: Vec<Key>,
    sel: usize,
    play: bool,
    cycle: f32,
    pub t: f32,
    drag: Drag,
    pub hits: Vec<(u8, u16, [f32; 4])>,
    pub paste: bool,
    opened: f32,
    /// the main hand holds nothing (the game draws a bare hand its own way: the swing is seen with an item)
    pub empty: bool,
    /// its screen was seen open
    seen: bool,
}

/// A moment's values as the sliders show them: label, low, high.
const SLIDERS: [(&str, f32, f32); 7] = [("Move X", -1.0, 1.0), ("Move Y", -1.0, 1.0), ("Move Z", -1.0, 1.0), ("Turn X", -360.0, 360.0), ("Turn Y", -360.0, 360.0), ("Turn Z", -360.0, 360.0), ("Size", 0.3, 2.0)];
const EASES: [&str; 4] = ["Straight", "Smooth", "Spring", "Sharp"];
/// The parts of the panel.
const P_TIME: u8 = 1;
const P_MOMENT: u8 = 2;
const P_BUTTON: u8 = 3;
const P_PRESET: u8 = 4;
const P_SLIDER: u8 = 5;
const P_EASE: u8 = 6;
const P_CODE: u8 = 7;
/// How long a swing plays in the editor, and the breath after it (s).
const PLAY: f32 = 0.6;
const BREATH: f32 = 0.45;

/// Words in boxes laid in rows `width` wide (each as wide as its word and `pad`, `bh` high, `gap` apart, a row `step`
/// under the last): each box from the rows' top left.
fn flow(words: &[&str], size: f32, pad: f32, bh: f32, gap: f32, step: f32, width: f32) -> Vec<[f32; 4]> {
    let mut out = Vec::new();
    let (mut x, mut y) = (0.0, 0.0);
    for w in words {
        let bw = fonts().width(Face::Medium, size, w) + pad;
        if x > 0.0 && x + bw > width {
            x = 0.0;
            y += step;
        }
        out.push([x, y, x + bw, y + bh]);
        x += bw + gap;
    }
    out
}

fn get(k: &Key, i: usize) -> f32 {
    match i {
        0..=2 => k.pos[i],
        3..=5 => k.rot[i - 3],
        _ => k.size,
    }
}

fn put(k: &mut Key, i: usize, v: f32) {
    match i {
        0..=2 => k.pos[i] = v,
        3..=5 => k.rot[i - 3] = v,
        _ => k.size = v,
    }
}

impl Gui {
    /// "Edit the swing": the editor opens over the world with the swing as it is (the player's own, or the style chosen
    /// as its start), the menu goes.
    pub(super) fn swing_open(&mut self) {
        let Some(m) = self.mods.iter().find(|m| m.id == "swing-animations") else { return };
        let style = match super::theme::find(&m.set, "Style") {
            Some(super::data::Kind::Choice { v, opts }) => opts.get(*v).copied().unwrap_or("Recode"),
            _ => "Recode",
        };
        let own = keys_parse(super::theme::text(&m.set, "Keyframes"));
        let keys = if style == "Custom" && own.len() >= 2 { own } else { Gui::swing_preset(if style == "Vanilla" || style == "Custom" { "Recode" } else { style }) };
        self.swing_ed = SwingEd { on: true, sel: 1.min(keys.len() - 1), keys, play: true, opened: self.now, ..Default::default() };
        self.want_editor = true;
        self.close(false);
    }

    /// The editor closed (Esc, or its screen gone): the swing kept as the player's own, Custom chosen, the module on.
    pub(super) fn swing_finish(&mut self) {
        if !self.swing_ed.on {
            return;
        }
        self.swing_ed.on = false;
        let text = keys_dump(&self.swing_ed.keys);
        if let Some(m) = self.mods.iter_mut().find(|m| m.id == "swing-animations") {
            m.on = true;
            for s in m.set.iter_mut() {
                match s.label {
                    "Keyframes" => s.kind = super::data::Kind::Text(text.clone()),
                    "Animation source" => if let super::data::Kind::Choice { v, .. } = &mut s.kind { *v = 0; },
                    "Style" => {
                        if let super::data::Kind::Choice { v, opts } = &mut s.kind {
                            *v = opts.iter().position(|o| *o == "Custom").unwrap_or(*v);
                        }
                    }
                    _ => {}
                }
            }
        }
        self.save();
    }

    /// Each frame: the time going on while it plays.
    fn swing_tick(&mut self) {
        let e = &mut self.swing_ed;
        if e.play && e.drag == Drag::None {
            e.cycle = (e.cycle + self.dt) % (PLAY + BREATH);
            e.t = (e.cycle / PLAY).min(1.0);
        }
    }

    /// The panel at the left (the HUD's frame, while it is open): drawn, its parts kept for the pointer.
    pub(super) fn swing_draw(&mut self, c: &mut Canvas) {
        self.swing_tick();
        let r = self.r;
        let h = self.hud.h;
        let k = super::smooth(0.0, 1.0, ((self.now - self.swing_ed.opened) / 0.35).clamp(0.0, 1.0));
        let pw = 21.0 * r;
        let inner = pw - 2.4 * r;
        // (its height from what is in it: the buttons' and the styles' rows are as many as their words need)
        let labels = [if self.swing_ed.play { tr("Pause") } else { tr("Play") }, tr("Add here"), tr("Remove"), tr("Mirror")];
        let btns = flow(&labels, 0.72 * r, 1.5 * r, 1.9 * r, 0.4 * r, 2.3 * r, inner);
        let btns_h = btns.last().map_or(0.0, |q| q[3]);
        let styles: Vec<usize> = (0..STYLES.len()).filter(|&n| !matches!(STYLES[n], "Vanilla" | "Custom")).collect();
        let names: Vec<&str> = styles.iter().map(|&n| tr(STYLES[n])).collect();
        let chips = flow(&names, 0.68 * r, 1.3 * r, 1.75 * r, 0.35 * r, 2.1 * r, inner);
        let chips_h = chips.last().map_or(0.0, |q| q[3]);
        let ph = 6.9 * r + btns_h + 2.0 * r + chips_h + 2.1 * r + 7.0 * 2.2 * r + 2.05 * r + 2.9 * r + if self.swing_ed.empty { 2.0 * r } else { 1.2 * r };
        let x0 = 1.6 * r - (1.0 - k) * 2.0 * r;
        let y0 = ((h - ph) / 2.0).max(0.6 * r);
        let rr = [x0, y0, x0 + pw, y0 + ph];
        let base = c.alpha;
        c.alpha = base * k;
        let look = super::theme::HudLook::of(&self.hud_set(), self.now);
        let set = &self.mods[self.mod_index("hb-hotbar")].set;
        let el = super::theme::ElemLook::of(set, &look, self.now);
        el.draw_plate(c, rr, &look, r / 12.0, self.now);
        let mut hits: Vec<(u8, u16, [f32; 4])> = Vec::new();
        let (l, rt) = (rr[0] + 1.2 * r, rr[2] - 1.2 * r);
        let mouse = self.mouse;
        let over = |q: [f32; 4]| mouse[0] >= q[0] && mouse[0] <= q[2] && mouse[1] >= q[1] && mouse[1] <= q[3];
        c.text(l, rr[1] + 1.9 * r, 1.05 * r, Face::Bold, tr("Your swing"), pal().t1);
        c.text(l, rr[1] + 3.0 * r, 0.68 * r, Face::Regular, tr("The hand swings as you make it · Esc keeps it"), pal().t3);
        // ---- the line of time: the moments on it, where the time is (how far it has played, lit)
        let bar = [l, rr[1] + 3.9 * r, rt, rr[1] + 6.3 * r];
        let mid = (bar[1] + bar[3]) / 2.0;
        let px = l + self.swing_ed.t * (rt - l);
        c.rect([l, mid - 1.0, rt, mid + 1.0], 1.0, grey(1.0, 0.16));
        c.rect([l, mid - 1.0, px, mid + 1.0], 1.0, fade(pal().accent, 0.55));
        c.rect([px - 1.0, bar[1] + 0.3 * r, px + 1.0, bar[3] - 0.3 * r], 1.0, pal().accent);
        hits.push((P_TIME, 0, bar));
        let keys = self.swing_ed.keys.clone();
        for (n, key) in keys.iter().enumerate() {
            let kx = l + key.t * (rt - l);
            let sel = n == self.swing_ed.sel;
            let zone = [kx - 0.6 * r, bar[1], kx + 0.6 * r, bar[3]];
            let hv = self.an(ak(A_HOV, 41_000 + n), if sel { 1.0 } else if over(zone) { 0.5 } else { 0.0 }, 14.0, 0.0);
            let d = (0.3 + 0.12 * hv) * r;
            c.rect([kx - d, mid - d, kx + d, mid + d], d, mix(grey(1.0, 0.45), pal().t1, hv));
            if sel {
                c.ring([kx, mid], d + 0.22 * r, 1.5, grey(1.0, 0.5 * hv));
            }
            hits.push((P_MOMENT, n as u16, zone));
        }
        // ---- play, add, remove, mirror
        let by = rr[1] + 6.9 * r;
        for (n, (label, q)) in labels.iter().zip(btns).enumerate() {
            let b = [l + q[0], by + q[1], l + q[2], by + q[3]];
            let hv = self.an(ak(A_HOV, 41_100 + n), if over(b) { 1.0 } else { 0.0 }, 14.0, 0.0);
            // (the ends of a swing are not removed)
            let dim = n == 2 && (self.swing_ed.sel == 0 || self.swing_ed.sel + 1 >= keys.len());
            let dk = self.an(ak(A_HOV, 41_110 + n), if dim { 0.4 } else { 1.0 }, 12.0, 1.0);
            c.alpha = base * k * dk;
            c.rect(b, 0.95 * r, mix(pal().f1, pal().f3, hv));
            c.text(b[0] + 0.75 * r, b[1] + 1.28 * r, 0.72 * r, Face::Medium, label, mix(pal().t2, pal().t1, hv));
            c.alpha = base * k;
            hits.push((P_BUTTON, n as u16, b));
        }
        // ---- a style to start from
        c.text(l, by + btns_h + 1.5 * r, 0.7 * r, Face::Medium, tr("Start from"), pal().t3);
        let top = by + btns_h + 2.0 * r;
        for ((&n, name), q) in styles.iter().zip(names).zip(chips) {
            let b = [l + q[0], top + q[1], l + q[2], top + q[3]];
            let hv = self.an(ak(A_HOV, 41_200 + n), if over(b) { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.875 * r, mix(pal().f1, pal().f3, hv));
            c.text(b[0] + 0.65 * r, b[1] + 1.2 * r, 0.68 * r, Face::Medium, name, mix(pal().t3, pal().t1, hv));
            hits.push((P_PRESET, n as u16, b));
        }
        // ---- the moment chosen: where, turned, how big, its ease
        let sel = self.swing_ed.sel.min(keys.len().saturating_sub(1));
        let Some(key) = keys.get(sel).copied() else {
            c.alpha = base;
            self.swing_ed.hits = hits;
            return;
        };
        let my = top + chips_h + 1.6 * r;
        let head = trn("Moment {n} of {m} · {pct}%", &[("n", (sel + 1).to_string()), ("m", keys.len().to_string()), ("pct", format!("{:.0}", key.t * 100.0))]);
        c.text(l, my, 0.8 * r, Face::Medium, &head, pal().t1);
        let mut y = my + 0.5 * r;
        for (n, (label, lo, hi)) in SLIDERS.iter().enumerate() {
            // (+ 0: a turn mirrored from nothing is 0°, not -0°)
            let v = get(&key, n) + 0.0;
            let shown = if (3..6).contains(&n) { format!("{v:.0}°") } else if n == 6 { format!("{v:.2}x") } else { format!("{v:+.2}") };
            c.text(l, y + 0.95 * r, 0.7 * r, Face::Regular, tr(label), pal().t2);
            c.text_r(rt, y + 0.95 * r, 0.7 * r, Face::Mono, &shown, pal().t1);
            let cy = y + 1.65 * r;
            let f = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
            c.rect([l, cy - 1.5, rt, cy + 1.5], 1.5, grey(1.0, 0.12));
            // (from the middle for those that go both ways)
            let zero = if n == 6 { l } else { l + (rt - l) * ((0.0 - lo) / (hi - lo)) };
            let at_x = l + (rt - l) * f;
            c.rect([zero.min(at_x), cy - 1.5, zero.max(at_x), cy + 1.5], 1.5, pal().accent);
            let zone = [l, cy - 0.7 * r, rt, cy + 0.7 * r];
            let hv = self.an(ak(A_HOV, 41_300 + n), if over(zone) || self.swing_ed.drag == Drag::Slider(n) { 1.0 } else { 0.0 }, 14.0, 0.0);
            let kr = (0.38 + 0.12 * hv) * r;
            c.rect([at_x - kr, cy - kr, at_x + kr, cy + kr], kr, pal().t1);
            hits.push((P_SLIDER, n as u16, zone));
            y += 2.2 * r;
        }
        // its ease to the next
        let ey = y + 0.3 * r;
        let mut ex = l;
        for (n, name) in EASES.iter().enumerate() {
            let label = tr(name);
            let ew = fonts().width(Face::Medium, 0.68 * r, label) + 1.3 * r;
            let b = [ex, ey, ex + ew, ey + 1.75 * r];
            let on = key.ease as usize == n;
            let hv = self.an(ak(A_HOV, 41_400 + n), if over(b) { 1.0 } else { 0.0 }, 14.0, 0.0);
            let lit = self.an(ak(A_HOV, 41_410 + n), if on { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.875 * r, mix(mix(pal().f1, pal().f3, hv), grey(1.0, 0.2), lit));
            c.text(ex + 0.65 * r, ey + 1.2 * r, 0.68 * r, Face::Medium, label, mix(pal().t3, pal().t1, hv.max(lit)));
            hits.push((P_EASE, n as u16, b));
            ex = b[2] + 0.35 * r;
        }
        // ---- its code
        let qy = ey + 2.75 * r;
        let mut qx = l;
        for (n, label) in [tr("Copy the code"), tr("Paste a code")].iter().enumerate() {
            let qw = fonts().width(Face::Medium, 0.7 * r, label) + 1.5 * r;
            let b = [qx, qy, qx + qw, qy + 1.9 * r];
            let hv = self.an(ak(A_HOV, 41_500 + n), if over(b) { 1.0 } else { 0.0 }, 14.0, 0.0);
            c.rect(b, 0.95 * r, mix(pal().f1, pal().f3, hv));
            c.text(qx + 0.75 * r, qy + 1.28 * r, 0.7 * r, Face::Medium, label, mix(pal().t2, pal().t1, hv));
            hits.push((P_CODE, n as u16, b));
            qx = b[2] + 0.4 * r;
        }
        // nothing in the hand: where the swing is seen
        if self.swing_ed.empty {
            c.text(l, qy + 3.0 * r, 0.66 * r, Face::Regular, tr("Take an item in your hand to see the swing"), pal().t3);
        }
        c.alpha = base;
        self.swing_ed.hits = hits;
    }

    /// Each HUD frame: its screen gone (Esc, or another screen over it) keeps the swing; a screen that never came
    /// leaves it as it was.
    pub(super) fn swing_follow(&mut self) {
        if !self.swing_ed.on {
            return;
        }
        if self.hud.facts.editor {
            self.swing_ed.seen = true;
        } else if self.swing_ed.seen {
            self.swing_finish();
        } else if self.now - self.swing_ed.opened > 3.0 {
            self.swing_ed.on = false;
        }
    }

    fn swing_hit(&self, x: f32, y: f32) -> Option<(u8, u16, [f32; 4])> {
        self.swing_ed.hits.iter().rev().find(|(_, _, q)| x >= q[0] && x <= q[2] && y >= q[1] && y <= q[3]).copied()
    }

    /// A press over the world while it is open: its parts, or nothing (true either way: the world is not clicked).
    pub(super) fn swing_press(&mut self, x: f32, y: f32, button: i32) -> bool {
        if button != 0 {
            return true;
        }
        let Some((part, n, q)) = self.swing_hit(x, y) else { return true };
        let n = n as usize;
        let len = self.swing_ed.keys.len();
        match part {
            P_TIME => {
                self.swing_ed.play = false;
                self.swing_ed.drag = Drag::Time;
                self.swing_ed.t = ((x - q[0]) / (q[2] - q[0]).max(1.0)).clamp(0.0, 1.0);
            }
            P_MOMENT => {
                self.swing_ed.sel = n;
                self.swing_ed.play = false;
                self.swing_ed.t = self.swing_ed.keys[n].t;
                // the ends stay where they are (a swing starts and ends at rest)
                if n > 0 && n + 1 < len {
                    self.swing_ed.drag = Drag::Moment(n);
                }
                self.sound_tick(0.4);
            }
            P_BUTTON => match n {
                0 => {
                    self.swing_ed.play = !self.swing_ed.play;
                    self.swing_ed.cycle = self.swing_ed.t * PLAY;
                }
                1 => {
                    // a moment where the time stands, as the swing is there now
                    let t = self.swing_ed.t.clamp(0.01, 0.99);
                    // (a moment already there is chosen, not doubled)
                    if let Some(i) = self.swing_ed.keys.iter().position(|q| (q.t - t).abs() < 0.015) {
                        self.swing_ed.sel = i;
                        self.sound_tick(0.4);
                        return true;
                    }
                    let (pos, rot, size) = at(&self.swing_ed.keys, t);
                    let ease = self.swing_ed.keys.iter().rev().find(|k| k.t <= t).map_or(1, |k| k.ease);
                    let k = Key { t, pos, rot, size, ease };
                    let i = self.swing_ed.keys.iter().position(|q| q.t > t).unwrap_or(len);
                    self.swing_ed.keys.insert(i, k);
                    self.swing_ed.sel = i;
                    self.switch_sound(true, 0.4);
                }
                2 => {
                    let s = self.swing_ed.sel;
                    if s > 0 && s + 1 < len {
                        self.swing_ed.keys.remove(s);
                        self.swing_ed.sel = s - 1;
                        self.switch_sound(false, 0.4);
                    }
                }
                _ => {
                    // to the other side: across, and the turns about up and forward
                    for k in self.swing_ed.keys.iter_mut() {
                        k.pos[0] = -k.pos[0];
                        k.rot[1] = -k.rot[1];
                        k.rot[2] = -k.rot[2];
                    }
                    self.sound_tick(0.5);
                }
            },
            P_PRESET => {
                if let Some(style) = STYLES.get(n) {
                    self.swing_ed.keys = Gui::swing_preset(style);
                    self.swing_ed.sel = 1.min(self.swing_ed.keys.len() - 1);
                    self.swing_ed.play = true;
                    self.switch_sound(true, 0.4);
                }
            }
            P_SLIDER => {
                self.swing_ed.drag = Drag::Slider(n);
                self.swing_slide(n, x, q);
            }
            P_EASE => {
                let s = self.swing_ed.sel;
                if let Some(k) = self.swing_ed.keys.get_mut(s) {
                    k.ease = n as u8;
                    self.sound_tick(0.4);
                }
            }
            P_CODE => {
                if n == 0 {
                    let code = format!("swing:{}", keys_dump(&self.swing_ed.keys));
                    self.copy(&code);
                    self.hud.note(tr("Swing Animations").to_string(), tr("The swing's code is copied").to_string(), self.now);
                } else {
                    self.swing_ed.paste = true;
                    self.outbox.push("paste".into());
                }
                self.sound_tick(0.5);
            }
            _ => {}
        }
        true
    }

    fn swing_slide(&mut self, n: usize, x: f32, q: [f32; 4]) {
        let (_, lo, hi) = SLIDERS[n];
        let f = ((x - q[0]) / (q[2] - q[0]).max(1.0)).clamp(0.0, 1.0);
        let mut v = lo + (hi - lo) * f;
        // (a snap to rest: 0 for the moves and turns, 1 for the size)
        let rest = if n == 6 { 1.0 } else { 0.0 };
        if (v - rest).abs() < (hi - lo) * 0.015 {
            v = rest;
        }
        let s = self.swing_ed.sel;
        if let Some(k) = self.swing_ed.keys.get_mut(s) {
            put(k, n, v);
        }
    }

    /// The pointer moved (a part held follows it).
    pub(super) fn swing_move(&mut self, x: f32, _y: f32) {
        match self.swing_ed.drag {
            Drag::Time => {
                if let Some((_, _, q)) = self.swing_ed.hits.iter().find(|h| h.0 == P_TIME).copied() {
                    self.swing_ed.t = ((x - q[0]) / (q[2] - q[0]).max(1.0)).clamp(0.0, 1.0);
                }
            }
            Drag::Moment(i) => {
                if let Some((_, _, q)) = self.swing_ed.hits.iter().find(|h| h.0 == P_TIME).copied() {
                    let t = ((x - q[0]) / (q[2] - q[0]).max(1.0)).clamp(0.0, 1.0);
                    // between its neighbours
                    let lo = self.swing_ed.keys.get(i.wrapping_sub(1)).map_or(0.0, |k| k.t) + 0.02;
                    let hi = self.swing_ed.keys.get(i + 1).map_or(1.0, |k| k.t) - 0.02;
                    if let Some(k) = self.swing_ed.keys.get_mut(i) {
                        k.t = t.clamp(lo, hi.max(lo));
                        self.swing_ed.t = k.t;
                    }
                }
            }
            Drag::Slider(n) => {
                if let Some((_, _, q)) = self.swing_ed.hits.iter().find(|h| h.0 == P_SLIDER && h.1 as usize == n).copied() {
                    self.swing_slide(n, x, q);
                }
            }
            Drag::None => {}
        }
    }

    pub(super) fn swing_release(&mut self) {
        self.swing_ed.drag = Drag::None;
    }

    /// The clipboard came for a swing's code.
    pub(super) fn swing_paste(&mut self, text: &str) {
        let body = text.trim().strip_prefix("swing:").unwrap_or(text.trim());
        let keys = keys_parse(body);
        if keys.len() >= 2 {
            self.swing_ed.keys = keys;
            self.swing_ed.sel = 1.min(self.swing_ed.keys.len() - 1);
            self.swing_ed.play = true;
            self.switch_sound(true, 0.5);
        } else {
            self.hud.note(tr("Swing Animations").to_string(), tr("That is no swing's code").to_string(), self.now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::data::Kind;
    use super::super::theme;
    use super::*;

    #[test]
    fn the_swing_made_is_kept_as_ones_own() {
        let mut g = Gui::new();
        g.swing_open();
        assert!(g.swing_ed.on && g.swing_ed.keys.len() >= 2);
        // the hand held where the time stands, with no swing of the game's
        g.swing_ed.t = 0.28;
        let said = g.hand_call("main\tWeapons\t0\t0");
        assert!(said.starts_with("edit "), "{said}");
        assert!(!g.hand_call("off\tWeapons\t0\t0").contains("edit "));
        // a code pasted (from the clipboard asked for), a broken one left
        g.swing_ed.paste = true;
        g.paste("swing:0 0 0 0 0 0 0 1 1;0.5 0.1 0 0 -40 0 0 1.2 3;1 0 0 0 0 0 0 1 1");
        assert_eq!(g.swing_ed.keys.len(), 3);
        assert!(!g.swing_ed.paste);
        g.swing_paste("swing:nothing");
        assert_eq!(g.swing_ed.keys.len(), 3);
        g.swing_finish();
        assert!(!g.swing_ed.on);
        let i = g.mod_index("swing-animations");
        let set = g.mods[i].set.clone();
        assert!(g.mods[i].on);
        assert!(matches!(theme::find(&set, "Style"), Some(Kind::Choice { v, opts }) if opts[*v] == "Custom"));
        assert_eq!(keys_parse(theme::text(&set, "Keyframes")).len(), 3);
        // and swung so after (at the moment made: where it was put)
        assert_eq!(g.hand_call("main\tWeapons\t0.5\t0"), "swing 0.1 0 0 -40 0 0 1.2");
    }

    #[test]
    fn a_moment_is_added_where_none_is_and_chosen_where_one_is() {
        let mut g = Gui::new();
        g.swing_open();
        g.swing_ed.keys = Gui::swing_preset("Smooth");
        g.swing_ed.play = false;
        // "Add here" where the swing's middle is: that one chosen, none doubled
        g.swing_ed.hits = vec![(P_BUTTON, 1, [0.0, 0.0, 10.0, 10.0]), (P_BUTTON, 3, [20.0, 0.0, 30.0, 10.0])];
        g.swing_ed.t = 0.5;
        g.swing_press(5.0, 5.0, 0);
        assert_eq!((g.swing_ed.keys.len(), g.swing_ed.sel), (3, 1));
        // a little before it: a new one, as the swing is there
        g.swing_ed.t = 0.3;
        g.swing_press(5.0, 5.0, 0);
        assert_eq!((g.swing_ed.keys.len(), g.swing_ed.sel), (4, 1));
        assert!((g.swing_ed.keys[1].t - 0.3).abs() < 1e-6);
        // mirrored: across, and back again as it was
        let before = g.swing_ed.keys.clone();
        g.swing_press(25.0, 5.0, 0);
        assert!((g.swing_ed.keys[2].pos[0] + before[2].pos[0]).abs() < 1e-6);
        g.swing_press(25.0, 5.0, 0);
        assert_eq!(g.swing_ed.keys, before);
    }
}
