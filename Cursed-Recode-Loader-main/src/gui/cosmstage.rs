//! The Cosmetics page's look's showcase: a dark room the item stands in, one lamp over it lighting the wall behind and
//! a pool on the dark glossy floor that mirrors it (the engine draws the reflection: cosmetics/preview.rs ground), the
//! lamp turned by the right button held on the item. Its light is kept in the loadout's file ("light="), and its
//! quality ("quality=": how much larger it is drawn, then brought down) chosen by the chip at its top left.

use super::draw::{grey, mix, Canvas, Rgba};
use super::i18n::tr;
use super::text::{fonts, Face};
use super::theme::pal;
use super::{ak, Gui, Hit, A_HOV};
use crate::cosmetics::{loadout, preview::Preview};

/// What is held (cosm.drag): the light, turned.
pub(super) const DRAG_LIGHT: u8 = 9;
/// The part (Hit::Cosm): the quality's chip (each press the next).
pub(super) const QUALITY: u8 = 44;
const QUALITIES: [&str; 3] = ["Standard", "High", "Ultra"];

/// A soft filled ellipse (`m` its middle, `half` its half width and height): thin rows, a few ellipses inside each other
/// for its soft edge.
fn ellipse(c: &mut Canvas, m: [f32; 2], half: [f32; 2], col: Rgba) {
    const LAYERS: usize = 5;
    for layer in 0..LAYERS {
        let k = 1.0 - layer as f32 / LAYERS as f32 * 0.85;
        let (hx, hy) = (half[0] * k, (half[1] * k).max(1.0));
        let rows = (hy * 2.0).clamp(3.0, 160.0) as usize;
        let dy = 2.0 * hy / rows as f32;
        for i in 0..rows {
            let y = -hy + (i as f32 + 0.5) * dy;
            let w = hx * (1.0 - (y / hy).powi(2)).max(0.0).sqrt();
            c.rect([m[0] - w, m[1] + y - dy * 0.5, m[0] + w, m[1] + y + dy * 0.5 + 0.5], 0.0, [col[0], col[1], col[2], col[3] / LAYERS as f32]);
        }
    }
}

impl Gui {
    /// The room behind the item (in its place `view`, `k` how far it has come in; `shown` the look as this frame shows
    /// it): dark, the lamp's light on the wall behind, the floor from right under the item — glossy, darker away, the
    /// lamp's pool on it and its sheen.
    pub(super) fn showcase(&mut self, c: &mut Canvas, view: [f32; 4], k: f32, shown: Option<&Preview>) {
        let r = self.r;
        let (w, h) = (view[2] - view[0], view[3] - view[1]);
        let (yaw, pitch) = loadout::current().0.light;
        // where the floor meets the item (the engine's floor, in the page's own measure), eased as it moves
        let place = self.stage_place(view);
        let fit = shown.filter(|p| !p.in_hand).and_then(|p| {
            let (at, width) = crate::cosmetics::preview::under(p)?;
            let (pw, ph) = ((p.rect[2] - p.rect[0]).max(1.0), (p.rect[3] - p.rect[1]).max(1.0));
            let sx = (place[2] - place[0]) / pw;
            Some([place[0] + (at[0] - p.rect[0]) * sx, place[1] + (at[1] - p.rect[1]) * (place[3] - place[1]) / ph, width * sx])
        });
        let [fx, fy, fw] = fit.unwrap_or([(view[0] + view[2]) / 2.0, place[3] - 0.04 * h, 0.55 * w]);
        let fy = self.an(ak(A_HOV, 36_400), fy.clamp(view[1] + 0.3 * h, view[3] - 0.04 * h), 10.0, fy);
        let fx = self.an(ak(A_HOV, 36_401), fx, 10.0, fx);
        let fw = self.an(ak(A_HOV, 36_402), fw, 10.0, fw);
        // the lamp: over the item, aside as its light is turned
        let lx = fx + yaw.sin() * pitch.cos() * 0.22 * w;
        // the room, near black
        c.rect(view, r, grey(0.012, 0.93 * k));
        c.push_clip(view);
        // its light on the wall behind, falling off
        c.glow([lx, view[1] + 0.3 * h], 0.62 * w, grey(1.0, 0.07 * k));
        c.glow([lx, view[1] + 0.34 * h], 0.3 * w, grey(1.0, 0.045 * k));
        // the floor: a touch lighter than the wall where it begins, darker away; the lamp's pool under the item, a soft
        // sheen along it (the wall's light in the gloss) — none with the item in the hands (the view is the player's)
        let floor = [view[0], fy, view[2], view[3]];
        let k_floor = k * self.an(ak(A_HOV, 36_403), if self.cosm.in_hand { 0.0 } else { 1.0 }, 10.0, 1.0);
        c.rect(floor, 0.0, grey(1.0, 0.022 * k_floor));
        c.veil(floor, grey(0.0, 0.55 * k_floor), 1.0, 3.0);
        ellipse(c, [lx * 0.35 + fx * 0.65, fy + 0.012 * h], [0.62 * fw.max(0.25 * w), 0.035 * h], grey(1.0, 0.11 * k_floor));
        ellipse(c, [lx, fy + 0.004 * h], [0.9 * w, 0.006 * h], grey(1.0, 0.035 * k_floor));
        // the corners deeper
        c.veil([view[0], view[1], view[0] + 0.18 * w, view[3]], grey(0.0, 0.35 * k), 1.0, 1.0);
        c.veil([view[2] - 0.18 * w, view[1], view[2], view[3]], grey(0.0, 0.35 * k), 1.0, 2.0);
        c.pop_clip();
    }

    /// The quality's chip at the top left of the item's place: how much larger the item is drawn, then brought down (1,
    /// 2 or 3 times across: its edges smooth, its finish's fine lines whole); each press the next, its value coming up
    /// into place.
    pub(super) fn quality_chip(&mut self, c: &mut Canvas, view: [f32; 4]) {
        let r = self.r;
        let q = loadout::current().0.quality.clamp(1, 3) as usize;
        let fs = 0.62 * r;
        let label = tr("Quality");
        let lw = fonts().width(Face::Medium, fs, label);
        // (as wide as its widest value: it keeps its width as the value changes)
        let vw = QUALITIES.iter().map(|v| fonts().width(Face::Medium, fs, tr(v))).fold(0.0, f32::max);
        let b = [view[0] + 0.6 * r, view[1] + 0.6 * r, view[0] + 0.6 * r + lw + vw + 1.6 * r, view[1] + 2.0 * r];
        let hit = Hit::Cosm(QUALITY, 0);
        let hov = self.an(ak(A_HOV, 36_310), if self.hover == hit { 1.0 } else { 0.0 }, 14.0, 0.0);
        c.rect(b, 0.7 * r, mix(grey(0.0, 0.42), grey(0.12, 0.6), hov));
        let base = b[3] - 0.45 * r;
        c.text(b[0] + 0.55 * r, base, fs, Face::Medium, label, mix(pal().t4, pal().t3, hov));
        let k = if pal().anim { super::ease_out((self.now - self.cosm.quality_at) / 0.25) } else { 1.0 };
        let col = mix(pal().t2, pal().t1, hov);
        c.text(b[0] + 1.05 * r + lw, base + (1.0 - k) * 0.35 * r, fs, Face::Medium, tr(QUALITIES[q - 1]), [col[0], col[1], col[2], col[3] * k]);
        self.hits.push((b, hit));
    }

    /// The quality's chip pressed: the next one, kept.
    pub(super) fn quality_press(&mut self) {
        loadout::change(|l| l.quality = l.quality.clamp(1, 3) % 3 + 1);
        self.cosm.quality_at = self.now;
        self.sound_tick(0.5);
    }

    /// Where the item stands in its place: over the floor (the place's foot left to its reflection).
    pub(super) fn stage_place(&self, view: [f32; 4]) -> [f32; 4] {
        [view[0], view[1], view[2], view[3] - (view[3] - view[1]) * 0.26]
    }

    /// The light turned by the pointer held with the right button (from `last`).
    pub(super) fn light_move(&mut self, x: f32, y: f32, last: [f32; 2]) {
        let (dx, dy) = (x - last[0], y - last[1]);
        loadout::change(|l| {
            l.light.0 = (l.light.0 + dx * 0.01).rem_euclid(std::f32::consts::TAU);
            l.light.1 = (l.light.1 + dy * 0.01).clamp(-1.3, 1.4);
        });
        self.cosm.drag = Some((DRAG_LIGHT, [x, y]));
    }
}
