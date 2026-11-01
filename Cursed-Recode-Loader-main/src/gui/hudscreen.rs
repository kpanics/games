//! Custom Gui (Interface → Custom Gui): the container screens' panel. Java takes the game's picture of each container
//! without its grey and draws it, its items and words over a picture of ours made here: the glass panel the container
//! stands on (fading in as it opens; Java grows the whole screen in) and the hovered slot's light, which slides from
//! slot to slot. Java's HudScreen says where the picture is and which slot is under the pointer.

use super::*;
use crate::gui::data::Setting;

/// The open container as Java has it (HudScreen.append): the GUI's scale, its picture's place and size (GUI px),
/// the hovered slot's corner (None: no slot), how long it is open (s), its slots' corners.
#[derive(Clone, Default)]
pub struct ScreenF {
    pub gs: f32,
    pub rect: [f32; 4],
    pub hover: Option<[f32; 2]>,
    pub since: f32,
    pub slots: Vec<[f32; 2]>,
}

impl ScreenF {
    /// From Java's numbers at `at`: 0 (none), or a head of 10 then 2 a slot.
    pub fn parse(g: &dyn Fn(usize) -> f64, at: usize) -> (Option<ScreenF>, usize) {
        if g(at) < 0.5 {
            return (None, 1);
        }
        let n = (g(at + 9).max(0.0) as usize).min(128);
        let (hx, hy) = (g(at + 6) as f32, g(at + 7) as f32);
        let mut s = ScreenF {
            gs: (g(at + 1) as f32).max(1.0),
            rect: [g(at + 2) as f32, g(at + 3) as f32, g(at + 4) as f32, g(at + 5) as f32],
            hover: if hx > -900.0 { Some([hx, hy]) } else { None },
            since: g(at + 8) as f32,
            slots: Vec::with_capacity(n),
        };
        let mut i = at + 10;
        for _ in 0..n {
            s.slots.push([g(i) as f32, g(i + 1) as f32]);
            i += 2;
        }
        (Some(s), i - at)
    }
}

/// Custom Gui's animations.
const A_SC: u64 = 43;

impl Gui {
    /// Custom Gui's preview in its panel: a chest and an inventory on the module's glass as its settings make them now,
    /// the light going from slot to slot, the words light (the items marks in their colours).
    pub(crate) fn custom_gui_preview(&mut self, c: &mut Canvas, st: [f32; 4], reveal: f32, i: usize) {
        let r = self.r;
        let t = self.now;
        let set: Vec<Setting> = self.mods[i].set.clone();
        let open = ease_out(((reveal - 0.12) / 0.7).clamp(0.0, 1.0));
        let cy0 = (st[1] + st[3]) / 2.0;
        let hh = (st[3] - st[1]) / 2.0 * open;
        c.push_clip([st[0], cy0 - hh, st[2], cy0 + hh]);
        c.rect(st, 0.75 * r, grey(0.0, 0.28));
        // the world behind, dimmed as the setting says
        c.interference(st, 0.75 * r, t, 3.0, 0.35);
        c.rect(st, 0.75 * r, grey(0.0, theme::num(&set, "Dim") / 100.0 * 0.9));
        // the container: 9 by 3 and the inventory under it, the game's measures (GUI px) at the preview's size, in the
        // stage's top (it is as tall as the settings beside it)
        let (iw, ih) = (176.0, 166.0);
        let sh_h = (st[3] - st[1]).min(15.0 * r);
        let u = ((st[2] - st[0]) * 0.8 / iw).min(sh_h * 0.86 / ih);
        let (x0, y0) = ((st[0] + st[2]) / 2.0 - iw * u / 2.0, st[1] + sh_h / 2.0 - ih * u / 2.0);
        let m = theme::num(&set, "Panel margin") * u;
        let pr = [x0 - m, y0 - m, x0 + iw * u + m, y0 + ih * u + m];
        let rad = 4.0 * u * theme::num(&set, "Corners") / 100.0;
        let look = theme::HudLook::of(&self.hud_set(), t);
        let glass = theme::or(theme::col(&set, "Glass"), look.glass, t);
        let sh = if theme::on(&set, "Shadow") { 0.6 } else { 0.0 };
        match theme::pick(&set, "Style") {
            1 => c.glass(pr, rad, [glass[0], glass[1], glass[2], glass[3] * 0.3], look.blur.max(0.6), sh, 0.0),
            2 => {
                c.glass(pr, rad, [glass[0], glass[1], glass[2], glass[3].max(0.6)], look.blur, sh, 0.0);
                c.interference(pr, rad, t, 3.0, 0.5);
            }
            3 => c.glass(pr, rad, [glass[0], glass[1], glass[2], glass[3].max(0.94)], 0.0, sh, 0.0),
            _ => c.glass(pr, rad, glass, look.blur, sh, 0.0),
        }
        if theme::on(&set, "Line of light") {
            c.rect([pr[0] + rad * 0.6, pr[1] + 0.6, pr[2] - rad * 0.6, pr[1] + 1.6], 0.5, grey(1.0, 0.18));
        }
        let wc = theme::or(theme::col(&set, "Slot colour"), [1.0; 4], t);
        let wa = theme::num(&set, "Slot opacity") / 100.0;
        let lc = theme::or(theme::col(&set, "Labels colour"), [0.925, 0.925, 0.925, 1.0], t);
        let fs = 8.0 * u;
        c.text(x0 + 8.0 * u, y0 + 13.0 * u, fs, Face::Semi, tr("Chest"), lc);
        c.text(x0 + 8.0 * u, y0 + 79.0 * u, fs, Face::Semi, tr("Inventory"), lc);
        let mut slots: Vec<[f32; 2]> = Vec::new();
        for row in 0..3 {
            for col in 0..9 {
                slots.push([8.0 + col as f32 * 18.0, 18.0 + row as f32 * 18.0]);
            }
        }
        for row in 0..3 {
            for col in 0..9 {
                slots.push([8.0 + col as f32 * 18.0, 84.0 + row as f32 * 18.0]);
            }
        }
        for col in 0..9 {
            slots.push([8.0 + col as f32 * 18.0, 142.0]);
        }
        // a few items: marks in their colours
        const ITEMS: [(usize, u32); 9] = [(2, 0x63c7d6), (5, 0xe0b04a), (12, 0x9b6b3a), (20, 0x7bd05a), (31, 0xc04040), (40, 0x8e8e9a), (54, 0x63c7d6), (56, 0xe0b04a), (60, 0x9b6b3a)];
        for (k, s) in slots.iter().enumerate() {
            // (a well is the slot's 16 px: the game's frame around it gone, a gap between)
            let rr = [x0 + s[0] * u, y0 + s[1] * u, x0 + (s[0] + 16.0) * u, y0 + (s[1] + 16.0) * u];
            c.rect(rr, 0.0, fade(wc, wa));
            if let Some(&(_, col)) = ITEMS.iter().find(|it| it.0 == k) {
                let q = [rr[0] + 3.0 * u, rr[1] + 3.0 * u, rr[2] - 3.0 * u, rr[3] - 3.0 * u];
                c.rect(q, 2.0 * u, crate::gui::draw::hex(col, 0.9));
            }
        }
        // the light walks over the slots
        let style = theme::pick(&set, "Hover");
        if style != 3 {
            let hc = theme::or(theme::col(&set, "Hover colour"), [0.93, 0.93, 0.93, 1.0], t);
            let path = [0usize, 1, 2, 11, 20, 29, 38, 47, 56, 57, 58, 49, 40, 31, 22, 13, 4];
            let step = (t * 1.6 * (theme::num(&set, "Hover speed") / 100.0).max(0.25)) as usize;
            let s = slots[path[step % path.len()]];
            let to = [x0 + (s[0] - 1.0) * u, y0 + (s[1] - 1.0) * u];
            let sp = (theme::num(&set, "Hover speed") / 100.0).max(0.25) * 18.0;
            let hx = self.an(ak(A_SC, 20), to[0], sp, to[0]);
            let hy = self.an(ak(A_SC, 21), to[1], sp, to[1]);
            let hr = [hx, hy, hx + 18.0 * u, hy + 18.0 * u];
            match style {
                1 => c.rect(hr, 2.0 * u, fade(hc, 0.22)),
                2 => {
                    c.rect([hr[0] + u, hr[1] + 2.0 * u, hr[2] + u, hr[3] + 2.0 * u], 2.0 * u, grey(0.0, 0.3));
                    c.rect(hr, 2.0 * u, fade(hc, 0.16));
                }
                _ => {
                    c.glow([(hr[0] + hr[2]) / 2.0, (hr[1] + hr[3]) / 2.0], 18.0 * u * 0.95, fade(hc, 0.2));
                    c.rect(hr, 2.0 * u, fade(hc, 0.12));
                }
            }
        }
        c.pop_clip();
        let _ = r;
    }
}

impl Gui {
    /// The picture under the open container (window px), or None (no container, Custom Gui off).
    pub(crate) fn screen_frame(&mut self, w: f32, h: f32) -> Option<Canvas> {
        let sf = self.hud.facts.container.clone()?;
        let i = self.mods.iter().position(|m| m.id == "custom-gui")?;
        if !self.mods[i].on {
            return None;
        }
        let set: Vec<Setting> = self.mods[i].set.clone();
        let t = self.now;
        let look = theme::HudLook::of(&self.hud_set(), t);
        let gs = sf.gs;
        let mut c = Canvas::new(w, h);
        // it fades in as it opens (Java grows it)
        let k = ease_out((sf.since / 0.22).clamp(0.0, 1.0));
        c.alpha = k;
        let m = theme::num(&set, "Panel margin") * gs;
        let [l, tp, iw, ih] = sf.rect;
        let r = [l * gs - m, tp * gs - m, (l + iw) * gs + m, (tp + ih) * gs + m];
        let rad = 4.0 * gs * theme::num(&set, "Corners") / 100.0;
        let glass = theme::or(theme::col(&set, "Glass"), look.glass, t);
        let sh = if theme::on(&set, "Shadow") { 0.6 } else { 0.0 };
        match theme::pick(&set, "Style") {
            1 => c.glass(r, rad, [glass[0], glass[1], glass[2], glass[3] * 0.3], look.blur.max(0.6), sh, 0.0),
            2 => {
                c.glass(r, rad, [glass[0], glass[1], glass[2], glass[3].max(0.6)], look.blur, sh, 0.0);
                c.interference(r, rad, t, 3.0, 0.5);
            }
            3 => c.glass(r, rad, [glass[0], glass[1], glass[2], glass[3].max(0.94)], 0.0, sh, 0.0),
            _ => c.glass(r, rad, glass, look.blur, sh, 0.0),
        }
        if theme::on(&set, "Line of light") {
            c.rect([r[0] + rad * 0.6, r[1] + 0.6, r[2] - rad * 0.6, r[1] + 1.6], 0.5, grey(1.0, 0.18));
        }
        // the hovered slot's light: it slides to the slot under the pointer, fades where there is none
        let style = theme::pick(&set, "Hover");
        let hc = theme::or(theme::col(&set, "Hover colour"), [0.93, 0.93, 0.93, 1.0], t);
        let speed = (theme::num(&set, "Hover speed") / 100.0).max(0.25) * 18.0;
        let on = sf.hover.is_some();
        let a = self.an(ak(A_SC, 0), if on { 1.0 } else { 0.0 }, 14.0, 0.0);
        if let Some(hv) = sf.hover {
            let to = [(l + hv[0] - 1.0) * gs, (tp + hv[1] - 1.0) * gs];
            // (just appeared: it starts where it is)
            let fresh = a < 0.02;
            let x = if fresh { self.an_set(ak(A_SC, 1), to[0]) } else { self.an(ak(A_SC, 1), to[0], speed, to[0]) };
            let y = if fresh { self.an_set(ak(A_SC, 2), to[1]) } else { self.an(ak(A_SC, 2), to[1], speed, to[1]) };
            self.hud.screen_hover = [x, y];
        }
        if a > 0.01 && style != 3 {
            let [x, y] = self.hud.screen_hover;
            let s = 18.0 * gs;
            let hr = [x, y, x + s, y + s];
            let rr = 2.0 * gs;
            c.alpha = k * a;
            match style {
                1 => c.rect(hr, rr, fade(hc, 0.22)),
                2 => {
                    c.rect([hr[0] + 1.0 * gs, hr[1] + 2.0 * gs, hr[2] + 1.0 * gs, hr[3] + 2.0 * gs], rr, grey(0.0, 0.3));
                    c.rect(hr, rr, fade(hc, 0.16));
                }
                _ => {
                    c.glow([(hr[0] + hr[2]) / 2.0, (hr[1] + hr[3]) / 2.0], s * 0.95, fade(hc, 0.2));
                    c.rect(hr, rr, fade(hc, 0.12));
                }
            }
        }
        // the player's own elements shown over the inventories, where they were put on the HUD (their pictures and
        // GIFs Java draws: their icons apart from the HUD's)
        c.alpha = k;
        self.hud.screen_icons.clear();
        let mine: Vec<usize> = (0..self.mods.len())
            .filter(|&j| self.mods[j].cat == Cat::HudEl && self.mods[j].on && theme::pick(&self.mods[j].set, "Show on") > 0)
            .collect();
        if !mine.is_empty() {
            let mut hl = theme::HudLook::of(&self.hud_set(), t);
            hl.show = true;
            let icons0 = self.hud.icons.len();
            self.hud.on_screen = true;
            for j in mine {
                self.hud_element(&mut c, j, &hl, t);
            }
            self.hud.on_screen = false;
            let moved: Vec<[f32; 7]> = self.hud.icons.drain(icons0..).collect();
            self.hud.screen_icons = moved.into_iter().map(|mut ic| {
                ic[6] *= k;
                ic
            }).collect();
        }
        c.alpha = 1.0;
        Some(c)
    }
}
